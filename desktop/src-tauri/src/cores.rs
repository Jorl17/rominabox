//! Obtaining a core, and rejecting any file that is not the recorded core.
//!
//! On the buildbot each core is under a directory named `latest`, which is
//! replaced in place. With a recorded hash this is safe, because we install
//! neither a different file nor a short one, and do not fetch again a file we
//! already checked. The same rule applies to the licence text.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub struct CoreDownload {
    pub filename: String,
    pub mirrors: Vec<String>,
    pub archive_sha256: String,
    pub binary_sha256: String,
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

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Place the recorded core into `directory`, or refuse.
///
/// We keep a file already there only when its hash matches. A `.partial` file
/// is never the core. It remains when a download stops halfway, and on the
/// next attempt we delete it and start again.
pub fn ensure_core(
    directory: &Path,
    core: &CoreDownload,
    transport: &dyn Transport,
) -> Result<CoreState, CoreFailure> {
    let path = directory.join(&core.filename);
    discard_partial(directory, &core.filename);
    if file_matches(&path, &core.binary_sha256) {
        return Ok(CoreState::Present);
    }
    let archive = match fetch_recorded(&core.mirrors, &core.archive_sha256, transport) {
        Ok(bytes) => bytes,
        Err(error) => {
            discard_unless_recorded(&path, &core.binary_sha256);
            return Err(error);
        }
    };
    let binary = match extract_named(&archive, &core.filename) {
        Some(bytes) if sha256_hex(&bytes) == core.binary_sha256 => bytes,
        _ => {
            discard_unless_recorded(&path, &core.binary_sha256);
            return Err(CoreFailure::NotRecorded);
        }
    };
    install_bytes(directory, &core.filename, &binary)?;
    Ok(CoreState::Installed)
}

/// Place a recorded file, such as a licence, that is not inside a zip.
pub fn ensure_file(
    directory: &Path,
    filename: &str,
    mirrors: &[String],
    sha256: &str,
    transport: &dyn Transport,
) -> Result<CoreState, CoreFailure> {
    let path = directory.join(filename);
    discard_partial(directory, filename);
    if file_matches(&path, sha256) {
        return Ok(CoreState::Present);
    }
    let bytes = match fetch_recorded(mirrors, sha256, transport) {
        Ok(bytes) => bytes,
        Err(error) => {
            discard_unless_recorded(&path, sha256);
            return Err(error);
        }
    };
    install_bytes(directory, filename, &bytes)?;
    Ok(CoreState::Installed)
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
    revision: String,
    #[serde(rename = "licenseFile")]
    license_file: String,
    #[serde(rename = "licensePath")]
    license_path: String,
    #[serde(rename = "licenseSha256")]
    license_sha256: String,
    artifacts: std::collections::BTreeMap<String, PinnedArtifact>,
}

#[derive(Debug, Deserialize)]
struct PinnedArtifact {
    filename: String,
    #[serde(rename = "archiveSha256")]
    archive_sha256: String,
    #[serde(rename = "binarySha256")]
    binary_sha256: String,
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

/// The pinned hashes for the nightly cores. Generated from the console packages.
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
        .collect();
    let core_state = ensure_core(
        &directory.join("cores"),
        &CoreDownload {
            filename: artifact.filename.clone(),
            mirrors,
            archive_sha256: artifact.archive_sha256.clone(),
            binary_sha256: artifact.binary_sha256.clone(),
        },
        transport,
    );
    let license_mirrors = pins
        .license_mirrors
        .iter()
        .map(|pattern| {
            pattern
                .replace("{repository}", &core.repository)
                .replace("{revision}", &core.revision)
                .replace("{path}", &core.license_path)
        })
        .collect::<Vec<_>>();
    let license_state = ensure_file(
        &directory.join("licenses"),
        &core.license_file,
        &license_mirrors,
        &core.license_sha256,
        transport,
    );
    Some(CoreInstall {
        component: core.component.clone(),
        core: core_state.into(),
        license: license_state.into(),
    })
}

fn fetch_recorded(
    mirrors: &[String],
    expected: &str,
    transport: &dyn Transport,
) -> Result<Vec<u8>, CoreFailure> {
    let mut saw_bytes = false;
    for url in mirrors {
        match transport.get(url) {
            Ok(bytes) => {
                saw_bytes = true;
                if sha256_hex(&bytes) == expected {
                    return Ok(bytes);
                }
            }
            Err(()) => {}
        }
    }
    if saw_bytes {
        Err(CoreFailure::NotRecorded)
    } else {
        Err(CoreFailure::Unreachable)
    }
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

fn file_matches(path: &Path, expected: &str) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    sha256_hex(&bytes) == expected
}

fn discard_partial(directory: &Path, filename: &str) {
    let partial = directory.join(format!("{filename}.partial"));
    if partial.is_file() {
        let _ = fs::remove_file(partial);
    }
}

fn discard_unless_recorded(path: &Path, expected: &str) {
    if path.is_file() && !file_matches(path, expected) {
        let _ = fs::remove_file(path);
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
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

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
            archive_sha256: sha256_hex(&archive),
            binary_sha256: sha256_hex(body),
        };
        (core, archive)
    }

    fn temp() -> PathBuf {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rominabox-core-fetch-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
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
    fn a_corrupt_file_is_not_trusted_and_a_wrong_replacement_is_not_kept() {
        let (core, _) = download("handy_libretro.dylib", b"the-core", &["https://mirror/a"]);
        let dir = temp();
        fs::write(dir.join(&core.filename), b"garbage").unwrap();
        let transport = Scripted {
            files: HashMap::from([("https://mirror/a".into(), Ok(b"not-the-zip".to_vec()))]),
            calls: Cell::new(0),
        };
        let error = ensure_core(&dir, &core, &transport).unwrap_err();
        assert_eq!(error, CoreFailure::NotRecorded);
        assert!(!dir.join(&core.filename).exists());
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
            license_mirrors: vec!["https://licence/{repository}/{revision}/{path}".into()],
            cores: vec![
                PinnedCore {
                    component: "a".into(),
                    repository: "libretro/a".into(),
                    revision: "abc".into(),
                    license_file: "a.txt".into(),
                    license_path: "COPYING".into(),
                    license_sha256: sha256_hex(b"licence-a"),
                    artifacts: std::collections::BTreeMap::from([(
                        "macos-arm64".into(),
                        PinnedArtifact {
                            filename: first.filename,
                            archive_sha256: first.archive_sha256,
                            binary_sha256: first.binary_sha256,
                        },
                    )]),
                },
                PinnedCore {
                    component: "b".into(),
                    repository: "libretro/b".into(),
                    revision: "def".into(),
                    license_file: "b.txt".into(),
                    license_path: "COPYING".into(),
                    license_sha256: sha256_hex(b"licence-b"),
                    artifacts: std::collections::BTreeMap::from([(
                        "macos-arm64".into(),
                        PinnedArtifact {
                            filename: second.filename,
                            archive_sha256: second.archive_sha256,
                            binary_sha256: second.binary_sha256,
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
                    "https://licence/libretro/b/def/COPYING".into(),
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
}
