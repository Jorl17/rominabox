use super::*;
use std::cell::Cell;
use std::collections::HashMap;
use std::io::Write;

/// Answers from a table, in place of the server. `served` is what we report
/// for every download, and `heads` is the answer to every check. `calls` is
/// the number of downloads and `checks` the number of checks.
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

/// The file under `latest` changes in place. We still accept a changed file
/// as the core when the archive contains the name in the list.
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

/// We use a file already in the cache as the core, whatever its bytes,
/// because the nightlies on the buildbot change in place.
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
    // The mirror base in the test pins is the full prefix to which we append
    // `/{folder}/latest/{file}.zip`. Point the scripted files at those URLs,
    // and fail only the first core.
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

/// The licence text on the branch may differ from a previously pinned one.
/// We store the current text when downloading.
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

/// A cache with the nightly that had the `etag` when we fetched it.
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
    // With the same ETag and another date, we compare the ETag.
    let mut later = tagged("\"1\"");
    later.last_modified = Some("Fri, 25 Sep 2026 19:01:39 GMT".into());
    assert!(!is_newer(Some(&recorded), &later));
    // With no ETag in the answer, we compare Last-Modified, then the length.
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
    // With nothing recorded, or nothing in common, we take the server's file.
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
    // When the answer has no validator, we keep the cached core.
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

/// The licence text does not count. We use and update a cached core without
/// its licence text as any other, and do not download it again.
#[test]
fn a_cached_core_without_its_licence_text_is_still_cached() {
    let dir = cached_flycast("\"1\"");
    let pins = flycast_pins();
    let located = Located::find(&pins, Target::MacosArm64, "flycast").unwrap();
    fs::remove_file(dir.join("licenses").join(located.licence_file)).unwrap();
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
