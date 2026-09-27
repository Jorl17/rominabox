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
    /// Both files are in the cache now.
    pub fn usable(&self) -> bool {
        let ok = |outcome: &InstallOutcome| {
            matches!(outcome, InstallOutcome::Present | InstallOutcome::Installed)
        };
        ok(&self.core) && ok(&self.license)
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
    /// The core or its licence is not in the cache.
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
        let cached = cache.join("cores").join(self.core_file).is_file()
            && cache.join("licenses").join(self.licence_file).is_file();
        if !cached {
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
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::io::Write;

    /// Replies from a table. `served` is the reply to every download, and
    /// `heads` the reply to a check. `calls` counts downloads and `checks`
    /// counts checks.
    #[derive(Default)]
    struct Scripted {
        files: HashMap<String, Result<Vec<u8>, ()>>,
        heads: HashMap<String, Version>,
        served: Version,
        calls: Cell<usize>,
        checks: Cell<usize>,
    }

    impl Transport for Scripted {
        fn get(&self, url: &str) -> Result<Response, ()> {
            self.calls.set(self.calls.get() + 1);
            match self.files.get(url) {
                Some(Ok(bytes)) => Ok(Response {
                    body: bytes.clone(),
                    version: self.served.clone(),
                }),
                _ => Err(()),
            }
        }

        fn head(&self, url: &str) -> Result<Version, ()> {
            self.checks.set(self.checks.get() + 1);
            self.heads.get(url).cloned().ok_or(())
        }
    }

    fn zip_of(name: &str, bytes: &[u8]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            writer
                .start_file(
                    name,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn download(filename: &str, body: &[u8], mirrors: &[&str]) -> (CoreDownload, Vec<u8>) {
        let archive = zip_of(filename, body);
        let core = CoreDownload {
            filename: filename.to_string(),
            mirrors: mirrors.iter().map(|url| (*url).to_string()).collect(),
        };
        (core, archive)
    }

    fn temp() -> rominabox_scratch::Scratch {
        rominabox_scratch::Scratch::dir("rominabox-core-fetch")
    }

    /// Libretro replaces `latest` in place, so a downloaded file can differ
    /// from an earlier one. We accept the download as the core when the
    /// archive contains the file name in the list.
    #[test]
    fn a_file_that_was_never_recorded_is_installed_when_the_archive_names_it() {
        let body = b"replaced-after-the-pin";
        let (core, archive) = download("handy_libretro.dylib", body, &["https://mirror/a"]);
        let dir = temp();
        let transport = Scripted {
            files: HashMap::from([("https://mirror/a".into(), Ok(archive))]),
            ..Default::default()
        };
        let state = ensure_core(&dir, &core, &transport).unwrap();
        assert_eq!(state, CoreState::Installed);
        assert_eq!(fs::read(dir.join(&core.filename)).unwrap(), body);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_archive_without_the_named_file_is_refused() {
        let archive = zip_of("someone_else.dylib", b"not-the-core");
        let (core, _) = download("handy_libretro.dylib", b"the-core", &["https://mirror/a"]);
        let dir = temp();
        let transport = Scripted {
            files: HashMap::from([("https://mirror/a".into(), Ok(archive))]),
            ..Default::default()
        };
        let error = ensure_core(&dir, &core, &transport).unwrap_err();
        assert_eq!(error, CoreFailure::NotRecorded);
        assert!(!dir.join(&core.filename).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// We accept a file already in the cache as the fetched core, whatever
    /// bytes it contains. We do not check it against a recorded hash, because
    /// nightly cores are replaced in place on the buildbot.
    #[test]
    fn a_core_already_there_is_not_fetched_even_when_its_bytes_differ() {
        let (core, _) = download("handy_libretro.dylib", b"the-core", &["https://mirror/a"]);
        let dir = temp();
        fs::write(dir.join(&core.filename), b"not-the-bytes-we-recorded").unwrap();
        let transport = Scripted {
            files: HashMap::new(),
            ..Default::default()
        };
        let state = ensure_core(&dir, &core, &transport).unwrap();
        assert_eq!(state, CoreState::Present);
        assert_eq!(transport.calls.get(), 0);
        assert_eq!(
            fs::read(dir.join(&core.filename)).unwrap(),
            b"not-the-bytes-we-recorded"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_launch_does_not_download_what_it_already_has() {
        let (core, _) = download("handy_libretro.dylib", b"the-core", &["https://mirror/a"]);
        let dir = temp();
        fs::write(dir.join(&core.filename), b"the-core").unwrap();
        let transport = Scripted {
            files: HashMap::new(),
            ..Default::default()
        };
        let state = ensure_core(&dir, &core, &transport).unwrap();
        assert_eq!(state, CoreState::Present);
        assert_eq!(transport.calls.get(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_truncated_download_is_not_kept() {
        let (core, _) = download("handy_libretro.dylib", b"the-core", &["https://mirror/a"]);
        let dir = temp();
        let transport = Scripted {
            files: HashMap::from([("https://mirror/a".into(), Ok(b"PK\x03\x04cut".to_vec()))]),
            ..Default::default()
        };
        let error = ensure_core(&dir, &core, &transport).unwrap_err();
        assert_eq!(error, CoreFailure::NotRecorded);
        assert!(!dir.join(&core.filename).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_network_leaves_the_directory_without_a_core() {
        let (core, _) = download("handy_libretro.dylib", b"the-core", &["https://mirror/a"]);
        let dir = temp();
        let transport = Scripted {
            files: HashMap::new(),
            ..Default::default()
        };
        let error = ensure_core(&dir, &core, &transport).unwrap_err();
        assert_eq!(error, CoreFailure::Unreachable);
        assert!(!dir.join(&core.filename).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_dead_first_mirror_is_not_the_end() {
        let (core, archive) = download(
            "handy_libretro.dylib",
            b"the-core",
            &["https://down/a", "https://up/b"],
        );
        let dir = temp();
        let transport = Scripted {
            files: HashMap::from([("https://up/b".into(), Ok(archive))]),
            ..Default::default()
        };
        let state = ensure_core(&dir, &core, &transport).unwrap();
        assert_eq!(state, CoreState::Installed);
        assert_eq!(fs::read(dir.join(&core.filename)).unwrap(), b"the-core");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_partial_file_is_not_a_core() {
        let (core, _) = download("handy_libretro.dylib", b"the-core", &["https://mirror/a"]);
        let dir = temp();
        let partial = dir.join(format!("{}.partial", core.filename));
        fs::write(&partial, b"the-core").unwrap();
        let transport = Scripted {
            files: HashMap::new(),
            ..Default::default()
        };
        let error = ensure_core(&dir, &core, &transport).unwrap_err();
        assert_eq!(error, CoreFailure::Unreachable);
        assert!(!dir.join(&core.filename).exists());
        assert!(!partial.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_core_failing_does_not_stop_the_next() {
        let (first, _) = download("a.dylib", b"aaa", &["https://cores/a"]);
        let (second, second_zip) = download("b.dylib", b"bbb", &["https://cores/b"]);
        let pins = PinSet {
            core_mirrors: vec!["https://cores".into()],
            targets: std::collections::BTreeMap::from([(
                "macos-arm64".into(),
                "apple/osx/arm64".into(),
            )]),
            license_mirrors: vec!["https://licence/{repository}/{ref}/{path}".into()],
            cores: vec![
                PinnedCore {
                    component: "a".into(),
                    repository: "libretro/a".into(),
                    license_file: "a.txt".into(),
                    license_path: "COPYING".into(),
                    license_ref: "master".into(),
                    artifacts: std::collections::BTreeMap::from([(
                        "macos-arm64".into(),
                        PinnedArtifact {
                            filename: first.filename,
                        },
                    )]),
                },
                PinnedCore {
                    component: "b".into(),
                    repository: "libretro/b".into(),
                    license_file: "b.txt".into(),
                    license_path: "COPYING".into(),
                    license_ref: "develop".into(),
                    artifacts: std::collections::BTreeMap::from([(
                        "macos-arm64".into(),
                        PinnedArtifact {
                            filename: second.filename,
                        },
                    )]),
                },
            ],
        };
        // The mirror base in the test pins is the full prefix to which we
        // append `/{folder}/latest/{file}.zip`. Point the scripted files at
        // those computed URLs, and fail only the first core.
        let transport = Scripted {
            files: HashMap::from([
                (
                    "https://cores/apple/osx/arm64/latest/b.dylib.zip".into(),
                    Ok(second_zip),
                ),
                (
                    "https://licence/libretro/b/develop/COPYING".into(),
                    Ok(b"licence-b".to_vec()),
                ),
            ]),
            ..Default::default()
        };
        let dir = temp();
        let report = install_pins(&dir, Target::MacosArm64, &pins, &transport);
        assert_eq!(report[0].component, "a");
        assert_eq!(report[0].core, InstallOutcome::Unreachable);
        assert_eq!(report[1].core, InstallOutcome::Installed);
        assert_eq!(report[1].license, InstallOutcome::Installed);
        assert_eq!(fs::read(dir.join("cores/b.dylib")).unwrap(), b"bbb");
        assert!(!dir.join("cores/a.dylib").exists());
        let again = install_pins(&dir, Target::MacosArm64, &pins, &transport);
        assert_eq!(again[1].core, InstallOutcome::Present);
        let _ = fs::remove_dir_all(&dir);
    }

    /// The licence is the text on the repository's current branch, and we
    /// store that text with the download.
    #[test]
    fn a_licence_that_was_never_recorded_is_kept() {
        let pins = PinSet {
            core_mirrors: vec!["https://cores".into()],
            targets: std::collections::BTreeMap::from([(
                "macos-arm64".into(),
                "apple/osx/arm64".into(),
            )]),
            license_mirrors: vec!["https://licence/{repository}/{ref}/{path}".into()],
            cores: vec![PinnedCore {
                component: "flycast".into(),
                repository: "flyinghead/flycast".into(),
                license_file: "flycast.txt".into(),
                license_path: "LICENSE".into(),
                license_ref: "master".into(),
                artifacts: std::collections::BTreeMap::from([(
                    "macos-arm64".into(),
                    PinnedArtifact {
                        filename: "flycast_libretro.dylib".into(),
                    },
                )]),
            }],
        };
        let archive = zip_of("flycast_libretro.dylib", b"the-core");
        let today = b"the licence as it stands now";
        let transport = Scripted {
            files: HashMap::from([
                (
                    "https://cores/apple/osx/arm64/latest/flycast_libretro.dylib.zip".into(),
                    Ok(archive),
                ),
                (
                    "https://licence/flyinghead/flycast/master/LICENSE".into(),
                    Ok(today.to_vec()),
                ),
            ]),
            ..Default::default()
        };
        let dir = temp();
        let report = install_pins(&dir, Target::MacosArm64, &pins, &transport);
        assert_eq!(report[0].core, InstallOutcome::Installed);
        assert_eq!(report[0].license, InstallOutcome::Installed);
        assert_eq!(fs::read(dir.join("licenses/flycast.txt")).unwrap(), today);
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("fetched.json")).unwrap()).unwrap();
        assert_eq!(
            record["cores"]["flycast"]["filename"],
            "flycast_libretro.dylib"
        );
        assert_eq!(
            record["cores"]["flycast"]["source"],
            "https://cores/apple/osx/arm64/latest/flycast_libretro.dylib.zip"
        );
        assert_eq!(
            record["licenses"]["flycast"]["source"],
            "https://licence/flyinghead/flycast/master/LICENSE"
        );
        let when = record["cores"]["flycast"]["downloadedAt"].as_str().unwrap();
        assert!(
            when.len() == 20 && when.ends_with('Z') && when.contains('T'),
            "{when}"
        );
        let calls = transport.calls.get();
        let again = install_pins(&dir, Target::MacosArm64, &pins, &transport);
        assert_eq!(again[0].core, InstallOutcome::Present);
        assert_eq!(again[0].license, InstallOutcome::Present);
        assert_eq!(
            transport.calls.get(),
            calls,
            "a second fetch asked the network"
        );
        let again_record: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("fetched.json")).unwrap()).unwrap();
        assert_eq!(again_record["cores"]["flycast"]["downloadedAt"], when);
        let _ = fs::remove_dir_all(&dir);
    }

    fn flycast_pins() -> PinSet {
        PinSet {
            core_mirrors: vec!["https://cores".into()],
            targets: std::collections::BTreeMap::from([(
                "macos-arm64".into(),
                "apple/osx/arm64".into(),
            )]),
            license_mirrors: vec!["https://licence/{repository}/{ref}/{path}".into()],
            cores: vec![PinnedCore {
                component: "flycast".into(),
                repository: "flyinghead/flycast".into(),
                license_file: "flycast.txt".into(),
                license_path: "LICENSE".into(),
                license_ref: "master".into(),
                artifacts: std::collections::BTreeMap::from([(
                    "macos-arm64".into(),
                    PinnedArtifact {
                        filename: "flycast_libretro.dylib".into(),
                    },
                )]),
            }],
        }
    }

    const FLYCAST_ZIP: &str = "https://cores/apple/osx/arm64/latest/flycast_libretro.dylib.zip";
    const FLYCAST_LICENCE: &str = "https://licence/flyinghead/flycast/master/LICENSE";

    fn tagged(etag: &str) -> Version {
        Version {
            etag: Some(etag.into()),
            last_modified: Some("Thu, 24 Sep 2026 19:01:39 GMT".into()),
            content_length: Some(868_642),
        }
    }

    /// A cache with the nightly whose ETag was `etag` when we fetched it.
    fn cached_flycast(etag: &str) -> rominabox_scratch::Scratch {
        let dir = temp();
        let pins = flycast_pins();
        let located = Located::find(&pins, Target::MacosArm64, "flycast").unwrap();
        let transport = Scripted {
            files: HashMap::from([
                (
                    FLYCAST_ZIP.into(),
                    Ok(zip_of("flycast_libretro.dylib", b"old-core")),
                ),
                (FLYCAST_LICENCE.into(), Ok(b"old licence".to_vec())),
            ]),
            served: tagged(etag),
            ..Default::default()
        };
        assert!(located.install(&dir, false, &transport).usable());
        dir
    }

    #[test]
    fn the_first_validator_both_sides_have_decides() {
        let recorded = tagged("\"1\"");
        assert!(!is_newer(Some(&recorded), &tagged("\"1\"")));
        assert!(is_newer(Some(&recorded), &tagged("\"2\"")));
        // Same ETag, different date, so we compare by the ETag.
        let mut later = tagged("\"1\"");
        later.last_modified = Some("Fri, 25 Sep 2026 19:01:39 GMT".into());
        assert!(!is_newer(Some(&recorded), &later));
        // No ETag in the reply, so we compare Last-Modified, then the length.
        later.etag = None;
        assert!(is_newer(Some(&recorded), &later));
        let length_only = Version {
            content_length: Some(868_642),
            ..Version::default()
        };
        assert!(!is_newer(Some(&recorded), &length_only));
        let other_length = Version {
            content_length: Some(1),
            ..Version::default()
        };
        assert!(is_newer(Some(&recorded), &other_length));
        // Nothing recorded, or nothing in common, so we use the server's values.
        assert!(is_newer(None, &tagged("\"1\"")));
        let dated = Version {
            etag: Some("\"1\"".into()),
            ..Version::default()
        };
        let undated = Version {
            last_modified: Some("Thu, 24 Sep 2026 19:01:39 GMT".into()),
            ..Version::default()
        };
        assert!(is_newer(Some(&dated), &undated));
        // When the server reports nothing, we change nothing.
        assert!(!is_newer(None, &Version::default()));
    }

    #[test]
    fn a_download_records_what_the_server_called_it() {
        let dir = cached_flycast("\"1\"");
        assert_eq!(
            recorded_version(&dir, "cores", "flycast"),
            Some(tagged("\"1\""))
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_core_is_a_download_without_asking_the_server() {
        let dir = temp();
        let pins = flycast_pins();
        let located = Located::find(&pins, Target::MacosArm64, "flycast").unwrap();
        let transport = Scripted::default();
        assert_eq!(located.assess(&dir, &transport), Need::Download);
        assert_eq!(transport.checks.get() + transport.calls.get(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_changed_nightly_is_an_update_and_an_unchanged_one_is_not() {
        let dir = cached_flycast("\"1\"");
        let pins = flycast_pins();
        let located = Located::find(&pins, Target::MacosArm64, "flycast").unwrap();
        let same = Scripted {
            heads: HashMap::from([(FLYCAST_ZIP.into(), tagged("\"1\""))]),
            ..Default::default()
        };
        assert_eq!(located.assess(&dir, &same), Need::UseCache);
        let changed = Scripted {
            heads: HashMap::from([(FLYCAST_ZIP.into(), tagged("\"2\""))]),
            ..Default::default()
        };
        assert_eq!(located.assess(&dir, &changed), Need::Update);
        assert_eq!(changed.calls.get(), 0, "a check downloaded the core");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_check_that_cannot_reach_the_server_uses_the_cache() {
        let dir = cached_flycast("\"1\"");
        let pins = flycast_pins();
        let located = Located::find(&pins, Target::MacosArm64, "flycast").unwrap();
        let offline = Scripted::default();
        assert_eq!(located.assess(&dir, &offline), Need::UseCache);
        assert_eq!(offline.checks.get(), 1);
        assert_eq!(offline.calls.get(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_update_replaces_the_core_its_licence_and_the_record() {
        let dir = cached_flycast("\"1\"");
        let pins = flycast_pins();
        let located = Located::find(&pins, Target::MacosArm64, "flycast").unwrap();
        let transport = Scripted {
            files: HashMap::from([
                (
                    FLYCAST_ZIP.into(),
                    Ok(zip_of("flycast_libretro.dylib", b"new-core")),
                ),
                (FLYCAST_LICENCE.into(), Ok(b"new licence".to_vec())),
            ]),
            served: tagged("\"2\""),
            ..Default::default()
        };
        let install = located.install(&dir, true, &transport);
        assert_eq!(install.core, InstallOutcome::Installed);
        assert_eq!(
            fs::read(dir.join("cores/flycast_libretro.dylib")).unwrap(),
            b"new-core"
        );
        assert_eq!(
            fs::read(dir.join("licenses/flycast.txt")).unwrap(),
            b"new licence"
        );
        assert_eq!(
            recorded_version(&dir, "cores", "flycast"),
            Some(tagged("\"2\""))
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_update_that_fails_keeps_the_cached_files() {
        let dir = cached_flycast("\"1\"");
        let pins = flycast_pins();
        let located = Located::find(&pins, Target::MacosArm64, "flycast").unwrap();
        let install = located.install(&dir, true, &Scripted::default());
        assert_eq!(install.core, InstallOutcome::Unreachable);
        assert_eq!(install.license, InstallOutcome::Present);
        assert_eq!(
            fs::read(dir.join("cores/flycast_libretro.dylib")).unwrap(),
            b"old-core"
        );
        assert_eq!(
            fs::read(dir.join("licenses/flycast.txt")).unwrap(),
            b"old licence"
        );
        assert_eq!(
            recorded_version(&dir, "cores", "flycast"),
            Some(tagged("\"1\""))
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_download_is_stamped_in_utc() {
        assert_eq!(format_unix_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix_utc(86_400), "1970-01-02T00:00:00Z");
    }
}
