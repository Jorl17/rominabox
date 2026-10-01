//! Downloading a core from the download list.
//!
//! On the buildbot each core is in a directory named `latest`, which changes
//! in place, so we do not check the bytes against a hash. We accept a download
//! as the core when it succeeds and the archive contains the filename in the
//! list. The licence is the text on the current branch of the repository.
//! In `fetched.json` we record the files of each download, when we fetched
//! them, and the server's `ETag`, `Last-Modified` and `Content-Length`. On
//! export we compare those with the server's current answer (see [`is_newer`]).

use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::target::Target;

/// How long we wait on export for the server's answer about a core.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// We set this in `scripts/test.py` for every test run. We then stop the
/// process on any download, so no test can use the network. A check fails
/// as for an unreachable server, so we can still test an export from a cache.
/// We stop a metadata download the same way (`crate::metadata`).
pub const OFFLINE_VARIABLE: &str = "ROMINABOX_OFFLINE";

pub struct CoreDownload {
    pub filename: String,
    pub mirrors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreState {
    Present,
    Installed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreFailure {
    Unreachable,
    NotRecorded,
}

/// The values in the server's answer that identify its file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_length: Option<u64>,
}

impl Version {
    fn is_empty(&self) -> bool {
        self.etag.is_none() && self.last_modified.is_none() && self.content_length.is_none()
    }
}

pub struct Response {
    pub body: Vec<u8>,
    pub version: Version,
}

pub trait Transport {
    fn get(&self, url: &str) -> Result<Response, ()>;
    /// The headers of `url`, without its body.
    fn head(&self, url: &str) -> Result<Version, ()>;
}

pub struct UreqTransport;

pub(crate) fn offline() -> bool {
    std::env::var_os(OFFLINE_VARIABLE).is_some()
}

fn version_of(response: &ureq::Response) -> Version {
    Version {
        etag: response.header("etag").map(str::to_string),
        last_modified: response.header("last-modified").map(str::to_string),
        content_length: response
            .header("content-length")
            .and_then(|value| value.trim().parse().ok()),
    }
}

impl Transport for UreqTransport {
    fn get(&self, url: &str) -> Result<Response, ()> {
        if offline() {
            panic!("{OFFLINE_VARIABLE} is set and a core was asked of the network");
        }
        let response = ureq::get(url)
            .timeout(DOWNLOAD_TIMEOUT)
            .call()
            .map_err(|_| ())?;
        let version = version_of(&response);
        let mut body = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut body)
            .map_err(|_| ())?;
        Ok(Response { body, version })
    }

    fn head(&self, url: &str) -> Result<Version, ()> {
        if offline() {
            return Err(());
        }
        let response = ureq::head(url)
            .timeout(CHECK_TIMEOUT)
            .call()
            .map_err(|_| ())?;
        Ok(version_of(&response))
    }
}

/// Whether the server's file is a different nightly from the cached one.
///
/// `latest` changes in place and only ever moves forward, so a file that is
/// not the one we downloaded is the newer one. We compare the first validator
/// present on both sides, in this order: `ETag`, `Last-Modified`,
/// `Content-Length`. We replace a cached core with nothing recorded, or with
/// no validator in common with the answer, whenever the answer has any
/// validator. When the answer has none, we keep the cached core.
pub fn is_newer(recorded: Option<&Version>, server: &Version) -> bool {
    if server.is_empty() {
        return false;
    }
    let Some(recorded) = recorded else {
        return true;
    };
    fn differs<T: PartialEq>(a: &Option<T>, b: &Option<T>) -> Option<bool> {
        Some(a.as_ref()? != b.as_ref()?)
    }
    differs(&recorded.etag, &server.etag)
        .or_else(|| differs(&recorded.last_modified, &server.last_modified))
        .or_else(|| differs(&recorded.content_length, &server.content_length))
        .unwrap_or(true)
}

/// Place the named core into `directory`, or return an error.
///
/// We reuse a file that is already there without checking its bytes, because
/// the nightlies on the buildbot change in place. A `.partial` file is not the
/// core. It remains when a download stops halfway, and on the next attempt we
/// delete it and start again.
pub fn ensure_core(
    directory: &Path,
    core: &CoreDownload,
    transport: &dyn Transport,
) -> Result<CoreState, CoreFailure> {
    Ok(place(
        directory,
        &core.filename,
        &core.mirrors,
        Body::Archive,
        false,
        transport,
    )?
    .0)
}

#[derive(Debug, Deserialize)]
struct PinSet {
    #[serde(rename = "coreMirrors")]
    core_mirrors: Vec<String>,
    targets: std::collections::BTreeMap<String, String>,
    #[serde(rename = "licenseMirrors")]
    license_mirrors: Vec<String>,
    cores: Vec<PinnedCore>,
}

#[derive(Debug, Deserialize)]
struct PinnedCore {
    component: String,
    repository: String,
    #[serde(rename = "licenseFile")]
    license_file: String,
    #[serde(rename = "licensePath")]
    license_path: String,
    /// The branch whose tip we read the licence from. We use a branch and not
    /// a commit, because the nightly can be newer than any recorded commit.
    #[serde(rename = "licenseRef")]
    license_ref: String,
    artifacts: std::collections::BTreeMap<String, PinnedArtifact>,
}

#[derive(Debug, Deserialize)]
struct PinnedArtifact {
    filename: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreInstall {
    pub component: String,
    pub core: InstallOutcome,
    pub license: InstallOutcome,
}

impl CoreInstall {
    /// The core is in the cache now. We fetch its licence text with it when
    /// we can, and we never stop a game for a missing licence.
    pub fn usable(&self) -> bool {
        matches!(self.core, InstallOutcome::Present | InstallOutcome::Installed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallOutcome {
    Present,
    Installed,
    Unreachable,
    NotRecorded,
}

impl From<Result<CoreState, CoreFailure>> for InstallOutcome {
    fn from(result: Result<CoreState, CoreFailure>) -> Self {
        match result {
            Ok(CoreState::Present) => Self::Present,
            Ok(CoreState::Installed) => Self::Installed,
            Err(CoreFailure::Unreachable) => Self::Unreachable,
            Err(CoreFailure::NotRecorded) => Self::NotRecorded,
        }
    }
}

/// The download list that we ship with the app.
///
/// It is the file next to the crate, so when a host moves we change that
/// file and need no new build.
fn pins() -> PinSet {
    serde_json::from_str(include_str!("../../core-pins.json")).expect("core pins parse")
}

/// Fetch every pinned core for `target` into `directory`, which has the same
/// `cores/` and `licenses/` layout as a runtime kit.
///
/// When one core fails, we continue with the next. On a later call, we skip
/// the cores that already match.
pub fn install_target(
    directory: &Path,
    target: Target,
    transport: &dyn Transport,
) -> Vec<CoreInstall> {
    install_pins(directory, target, &pins(), transport)
}

fn install_pins(
    directory: &Path,
    target: Target,
    pins: &PinSet,
    transport: &dyn Transport,
) -> Vec<CoreInstall> {
    pins.cores
        .iter()
        .filter_map(|core| Located::new(pins, core, target))
        .map(|located| located.install(directory, false, transport))
        .collect()
}

/// Fetch one recorded component into `directory`, or `None` when it is not pinned.
pub fn install_component(
    directory: &Path,
    target: Target,
    component: &str,
    transport: &dyn Transport,
) -> Option<CoreInstall> {
    let pins = pins();
    Some(Located::find(&pins, target, component)?.install(directory, false, transport))
}

/// What we must do on export before we can take one core from the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// The core is not in the cache.
    Download,
    /// The server has a newer nightly than the cached one.
    Update,
    /// The cached core is current, or we could not reach the server.
    UseCache,
}

/// Ask the server whether the cached core is still current, or `None` when
/// the component has no download for `target`.
///
/// We download nothing here. When we cannot reach the server, we use the
/// cached core and say nothing.
pub fn assess(
    cache: &Path,
    target: Target,
    component: &str,
    transport: &dyn Transport,
) -> Option<Need> {
    let pins = pins();
    Some(Located::find(&pins, target, component)?.assess(cache, transport))
}

/// Replace a cached core with the server's newer one, and its licence with
/// today's text. Return `false` when we could not fetch the core, and use the
/// cached one. Keep the cached licence text when we cannot fetch the new one.
pub fn update(cache: &Path, target: Target, component: &str, transport: &dyn Transport) -> bool {
    let pins = pins();
    let Some(located) = Located::find(&pins, target, component) else {
        return false;
    };
    let install = located.install(cache, true, transport);
    install.core == InstallOutcome::Installed
}

/// One pinned core's files and where they come from, for one target.
struct Located<'a> {
    component: &'a str,
    core_file: &'a str,
    core_urls: Vec<String>,
    licence_file: &'a str,
    licence_urls: Vec<String>,
}

impl<'a> Located<'a> {
    fn find(pins: &'a PinSet, target: Target, component: &str) -> Option<Self> {
        let core = pins.cores.iter().find(|core| core.component == component)?;
        Self::new(pins, core, target)
    }

    fn new(pins: &'a PinSet, core: &'a PinnedCore, target: Target) -> Option<Self> {
        let folder = pins.targets.get(target.key())?;
        let artifact = core.artifacts.get(target.key())?;
        Some(Self {
            component: &core.component,
            core_file: &artifact.filename,
            core_urls: pins
                .core_mirrors
                .iter()
                .map(|base| format!("{base}/{folder}/latest/{}.zip", artifact.filename))
                .collect(),
            licence_file: &core.license_file,
            licence_urls: pins
                .license_mirrors
                .iter()
                .map(|pattern| {
                    pattern
                        .replace("{repository}", &core.repository)
                        .replace("{ref}", &core.license_ref)
                        .replace("{path}", &core.license_path)
                })
                .collect(),
        })
    }

    fn assess(&self, cache: &Path, transport: &dyn Transport) -> Need {
        // Only the core counts, whether or not its licence text is cached.
        if !cache.join("cores").join(self.core_file).is_file() {
            return Need::Download;
        }
        let Some(server) = self
            .core_urls
            .iter()
            .find_map(|url| transport.head(url).ok())
        else {
            return Need::UseCache;
        };
        let recorded = recorded_version(cache, "cores", self.component);
        if is_newer(recorded.as_ref(), &server) {
            Need::Update
        } else {
            Need::UseCache
        }
    }

    /// With `replace` we fetch both files even when they are cached. When the
    /// licence download fails then, we keep the cached text and go on.
    fn install(&self, cache: &Path, replace: bool, transport: &dyn Transport) -> CoreInstall {
        let core = place_recorded(
            cache,
            "cores",
            self.component,
            self.core_file,
            &self.core_urls,
            Body::Archive,
            replace,
            transport,
        );
        let mut license = place_recorded(
            cache,
            "licenses",
            self.component,
            self.licence_file,
            &self.licence_urls,
            Body::Text,
            replace,
            transport,
        );
        if replace && license.is_err() && cache.join("licenses").join(self.licence_file).is_file() {
            license = Ok(CoreState::Present);
        }
        CoreInstall {
            component: self.component.to_string(),
            core: core.into(),
            license: license.into(),
        }
    }
}

#[derive(Clone, Copy)]
enum Body {
    /// A zip that must contain exactly one file with the expected name.
    Archive,
    /// The file itself, which must not be empty.
    Text,
}

/// `(state, source, version)`. `source` is the URL of the download that we
/// installed. A file already on disk has no source, because we did not
/// download it in this call. With `replace` we download it anyway, and write
/// over the file only once the new one is complete.
fn place(
    directory: &Path,
    filename: &str,
    mirrors: &[String],
    body: Body,
    replace: bool,
    transport: &dyn Transport,
) -> Result<(CoreState, Option<(String, Version)>), CoreFailure> {
    let path = directory.join(filename);
    discard_partial(directory, filename);
    if !replace && path.is_file() {
        return Ok((CoreState::Present, None));
    }
    let (bytes, source, version) = match body {
        Body::Archive => fetch_named(mirrors, filename, transport)?,
        Body::Text => fetch_body(mirrors, transport)?,
    };
    install_bytes(directory, filename, &bytes)?;
    Ok((CoreState::Installed, Some((source, version))))
}

/// Install, then record what we downloaded. We would skip a core without a
/// record on the next export and have nothing to compare on an update, so
/// when the record fails we remove the file and try again next time.
#[allow(clippy::too_many_arguments)]
fn place_recorded(
    cache: &Path,
    section: &str,
    component: &str,
    filename: &str,
    mirrors: &[String],
    body: Body,
    replace: bool,
    transport: &dyn Transport,
) -> Result<CoreState, CoreFailure> {
    let directory = cache.join(section);
    let (state, arrived) = place(&directory, filename, mirrors, body, replace, transport)?;
    let Some((source, version)) = arrived else {
        return Ok(state);
    };
    if let Err(error) = record_download(cache, section, component, filename, &source, &version) {
        let _ = fs::remove_file(directory.join(filename));
        return Err(error);
    }
    Ok(state)
}

fn fetch_named(
    mirrors: &[String],
    filename: &str,
    transport: &dyn Transport,
) -> Result<(Vec<u8>, String, Version), CoreFailure> {
    let mut saw_bytes = false;
    for url in mirrors {
        let Ok(response) = transport.get(url) else {
            continue;
        };
        saw_bytes = true;
        if let Some(binary) = extract_named(&response.body, filename) {
            return Ok((binary, url.clone(), response.version));
        }
    }
    Err(if saw_bytes {
        CoreFailure::NotRecorded
    } else {
        CoreFailure::Unreachable
    })
}

/// An empty body is not a licence. If we saved it, on the next export we
/// would treat the blank file as present and never ask again.
fn fetch_body(
    mirrors: &[String],
    transport: &dyn Transport,
) -> Result<(Vec<u8>, String, Version), CoreFailure> {
    let mut saw_bytes = false;
    for url in mirrors {
        let Ok(response) = transport.get(url) else {
            continue;
        };
        if response.body.is_empty() {
            saw_bytes = true;
            continue;
        }
        return Ok((response.body, url.clone(), response.version));
    }
    Err(if saw_bytes {
        CoreFailure::NotRecorded
    } else {
        CoreFailure::Unreachable
    })
}

fn read_record(cache: &Path) -> serde_json::Value {
    fs::read(cache.join("fetched.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .filter(|value: &serde_json::Value| value.is_object())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// What the server reported about the file when we downloaded it, or `None`
/// when we recorded nothing.
fn recorded_version(cache: &Path, section: &str, component: &str) -> Option<Version> {
    let entry = read_record(cache)
        .get(section)?
        .get(component)?
        .get("version")?
        .clone();
    serde_json::from_value::<Version>(entry)
        .ok()
        .filter(|version| !version.is_empty())
}

fn record_download(
    cache: &Path,
    section: &str,
    component: &str,
    filename: &str,
    source: &str,
    version: &Version,
) -> Result<(), CoreFailure> {
    let mut doc = read_record(cache);
    let entry = serde_json::json!({
        "filename": filename,
        "source": source,
        "downloadedAt": utc_now(),
        "version": version,
    });
    let object = doc.as_object_mut().ok_or(CoreFailure::Unreachable)?;
    let slot = object
        .entry(section)
        .or_insert_with(|| serde_json::json!({}));
    if !slot.is_object() {
        *slot = serde_json::json!({});
    }
    slot.as_object_mut()
        .ok_or(CoreFailure::Unreachable)?
        .insert(component.to_string(), entry);
    let text = serde_json::to_vec_pretty(&doc).map_err(|_| CoreFailure::Unreachable)?;
    install_bytes(cache, "fetched.json", &text)
}

fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format_unix_utc(secs)
}

fn format_unix_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let hour = tod / 3_600;
    let minute = (tod % 3_600) / 60;
    let second = tod % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Days since 1970-01-01 to a civil date. Howard Hinnant's `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month as u32, day as u32)
}

fn extract_named(archive: &[u8], filename: &str) -> Option<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).ok()?;
    let mut found = None;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).ok()?;
        if entry.is_dir() {
            continue;
        }
        let name = Path::new(entry.name())
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if name != filename {
            continue;
        }
        if found.is_some() {
            return None;
        }
        let mut binary = Vec::new();
        entry.read_to_end(&mut binary).ok()?;
        found = Some(binary);
    }
    found
}

fn discard_partial(directory: &Path, filename: &str) {
    let partial = directory.join(format!("{filename}.partial"));
    if partial.is_file() {
        let _ = fs::remove_file(partial);
    }
}

fn install_bytes(directory: &Path, filename: &str, bytes: &[u8]) -> Result<(), CoreFailure> {
    fs::create_dir_all(directory).map_err(|_| CoreFailure::Unreachable)?;
    let partial = directory.join(format!("{filename}.partial"));
    fs::write(&partial, bytes).map_err(|_| CoreFailure::Unreachable)?;
    fs::rename(&partial, directory.join(filename)).map_err(|_| CoreFailure::Unreachable)?;
    Ok(())
}

#[cfg(test)]
mod tests;
