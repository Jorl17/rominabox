//! Obtaining a core from the download list.
//!
//! On the buildbot each core is under a directory named `latest`, which is
//! replaced in place, so a recorded hash cannot identify it. We accept a
//! download as the core when it succeeds and the archive contains the file
//! name in the list. The licence is the text on the repository's current
//! branch. We do not fetch a file already in the cache again. In
//! `fetched.json` we record what we downloaded and when, for a later update.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

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

pub trait Transport {
    fn get(&self, url: &str) -> Result<Vec<u8>, ()>;
}

pub struct UreqTransport;

impl Transport for UreqTransport {
    fn get(&self, url: &str) -> Result<Vec<u8>, ()> {
        let response = ureq::get(url)
            .timeout(Duration::from_secs(120))
            .call()
            .map_err(|_| ())?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut bytes)
            .map_err(|_| ())?;
        Ok(bytes)
    }
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
    Ok(place_archive(directory, &core.filename, &core.mirrors, transport)?.0)
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
    target: &str,
    transport: &dyn Transport,
) -> Vec<CoreInstall> {
    install_pins(directory, target, &pins(), transport)
}

fn install_pins(
    directory: &Path,
    target: &str,
    pins: &PinSet,
    transport: &dyn Transport,
) -> Vec<CoreInstall> {
    pins.cores
        .iter()
        .filter_map(|core| install_one(directory, target, pins, core, transport))
        .collect()
}

/// Fetch one recorded component into `directory`, or `None` when it is not pinned.
pub fn install_component(
    directory: &Path,
    target: &str,
    component: &str,
    transport: &dyn Transport,
) -> Option<CoreInstall> {
    let pins = pins();
    let core = pins.cores.iter().find(|core| core.component == component)?;
    install_one(directory, target, &pins, core, transport)
}

fn install_one(
    directory: &Path,
    target: &str,
    pins: &PinSet,
    core: &PinnedCore,
    transport: &dyn Transport,
) -> Option<CoreInstall> {
    let folder = pins.targets.get(target)?;
    let artifact = core.artifacts.get(target)?;
    let mirrors = pins
        .core_mirrors
        .iter()
        .map(|base| format!("{base}/{folder}/latest/{}.zip", artifact.filename))
        .collect::<Vec<_>>();
    let core_state = place_recorded(
        directory,
        "cores",
        &core.component,
        &artifact.filename,
        &mirrors,
        true,
        transport,
    );
    let license_mirrors = pins
        .license_mirrors
        .iter()
        .map(|pattern| {
            pattern
                .replace("{repository}", &core.repository)
                .replace("{ref}", &core.license_ref)
                .replace("{path}", &core.license_path)
        })
        .collect::<Vec<_>>();
    let license_state = place_recorded(
        directory,
        "licenses",
        &core.component,
        &core.license_file,
        &license_mirrors,
        false,
        transport,
    );
    Some(CoreInstall {
        component: core.component.clone(),
        core: core_state.into(),
        license: license_state.into(),
    })
}

/// `(state, source)`. `source` is the URL of the installed file. A file
/// already on disk has no source, because we did not download it in this call.
fn place_archive(
    directory: &Path,
    filename: &str,
    mirrors: &[String],
    transport: &dyn Transport,
) -> Result<(CoreState, Option<String>), CoreFailure> {
    let path = directory.join(filename);
    discard_partial(directory, filename);
    if path.is_file() {
        return Ok((CoreState::Present, None));
    }
    let (binary, source) = fetch_named(mirrors, filename, transport)?;
    install_bytes(directory, filename, &binary)?;
    Ok((CoreState::Installed, Some(source)))
}

fn place_text(
    directory: &Path,
    filename: &str,
    mirrors: &[String],
    transport: &dyn Transport,
) -> Result<(CoreState, Option<String>), CoreFailure> {
    let path = directory.join(filename);
    discard_partial(directory, filename);
    if path.is_file() {
        return Ok((CoreState::Present, None));
    }
    let (bytes, source) = fetch_body(mirrors, transport)?;
    install_bytes(directory, filename, &bytes)?;
    Ok((CoreState::Installed, Some(source)))
}

/// Install, then record what we downloaded. We would skip a core without a
/// record on the next export and have nothing to compare on an update, so
/// when the record fails we remove the file and try again next time.
fn place_recorded(
    cache: &Path,
    section: &str,
    component: &str,
    filename: &str,
    mirrors: &[String],
    archive: bool,
    transport: &dyn Transport,
) -> Result<CoreState, CoreFailure> {
    let directory = cache.join(section);
    let placed = if archive {
        place_archive(&directory, filename, mirrors, transport)
    } else {
        place_text(&directory, filename, mirrors, transport)
    };
    let (state, source) = placed?;
    let Some(source) = source else {
        return Ok(state);
    };
    if let Err(error) = record_download(cache, section, component, filename, &source) {
        let _ = fs::remove_file(directory.join(filename));
        return Err(error);
    }
    Ok(state)
}

fn fetch_named(
    mirrors: &[String],
    filename: &str,
    transport: &dyn Transport,
) -> Result<(Vec<u8>, String), CoreFailure> {
    let mut saw_bytes = false;
    for url in mirrors {
        let Ok(archive) = transport.get(url) else {
            continue;
        };
        saw_bytes = true;
        if let Some(binary) = extract_named(&archive, filename) {
            return Ok((binary, url.clone()));
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
) -> Result<(Vec<u8>, String), CoreFailure> {
    let mut saw_bytes = false;
    for url in mirrors {
        let Ok(bytes) = transport.get(url) else {
            continue;
        };
        if bytes.is_empty() {
            saw_bytes = true;
            continue;
        }
        return Ok((bytes, url.clone()));
    }
    Err(if saw_bytes {
        CoreFailure::NotRecorded
    } else {
        CoreFailure::Unreachable
    })
}

fn record_download(
    cache: &Path,
    section: &str,
    component: &str,
    filename: &str,
    source: &str,
) -> Result<(), CoreFailure> {
    let path = cache.join("fetched.json");
    let mut doc: serde_json::Value = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .filter(|value: &serde_json::Value| value.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    let entry = serde_json::json!({
        "filename": filename,
        "source": source,
        "downloadedAt": utc_now(),
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

    struct Scripted {
        files: HashMap<String, Result<Vec<u8>, ()>>,
        calls: Cell<usize>,
    }

    impl Transport for Scripted {
        fn get(&self, url: &str) -> Result<Vec<u8>, ()> {
            self.calls.set(self.calls.get() + 1);
            match self.files.get(url) {
                Some(Ok(bytes)) => Ok(bytes.clone()),
                _ => Err(()),
            }
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
            calls: Cell::new(0),
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
            calls: Cell::new(0),
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
            calls: Cell::new(0),
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
            calls: Cell::new(0),
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
            calls: Cell::new(0),
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
            calls: Cell::new(0),
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
            calls: Cell::new(0),
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
            calls: Cell::new(0),
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
            calls: Cell::new(0),
        };
        let dir = temp();
        let report = install_pins(&dir, "macos-arm64", &pins, &transport);
        assert_eq!(report[0].component, "a");
        assert_eq!(report[0].core, InstallOutcome::Unreachable);
        assert_eq!(report[1].core, InstallOutcome::Installed);
        assert_eq!(report[1].license, InstallOutcome::Installed);
        assert_eq!(fs::read(dir.join("cores/b.dylib")).unwrap(), b"bbb");
        assert!(!dir.join("cores/a.dylib").exists());
        let again = install_pins(&dir, "macos-arm64", &pins, &transport);
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
            calls: Cell::new(0),
        };
        let dir = temp();
        let report = install_pins(&dir, "macos-arm64", &pins, &transport);
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
        let again = install_pins(&dir, "macos-arm64", &pins, &transport);
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

    #[test]
    fn a_download_is_stamped_in_utc() {
        assert_eq!(format_unix_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix_utc(86_400), "1970-01-02T00:00:00Z");
    }
}
