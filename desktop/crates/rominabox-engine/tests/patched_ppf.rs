//! A PPF patch for a PlayStation disc takes effect in the emulator, PCSX
//! ReARMed, while the game runs, so we export the disc as it is, and a CHD
//! stays compressed. We ship the patch named after the disc's serial, which
//! is the name expected in the folder the player starts in, and copy it there
//! in the launcher (see the shipped tests). Only one PPF takes effect.
#![cfg(target_os = "macos")]

mod export_fixture;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use export_fixture::{export_request, workspace};
use rominabox_engine::content::{self, GameFiles};
use rominabox_engine::packaging::{export_game, ErrorStage, ExportRequest};

/// A PPF 3.0 patch writing `bytes` at `offset`.
fn ppf(offset: u64, bytes: &[u8]) -> Vec<u8> {
    let mut patch = b"PPF30\x02".to_vec();
    patch.extend_from_slice(&[b' '; 50]);
    patch.extend_from_slice(&[0, 0, 0, 0]);
    patch.extend_from_slice(&offset.to_le_bytes());
    patch.push(bytes.len() as u8);
    patch.extend_from_slice(bytes);
    patch
}

/// A PlayStation disc in `root` whose serial is SCES-01564, and an export
/// request for it.
fn playstation_disc(root: &Path) -> (ExportRequest, Vec<u8>) {
    let mut request = export_request(root);
    let core = &rominabox_engine::systems::find("ps1").unwrap().cores[0];
    fs::write(request.runtime_kit.join("cores").join(core.artifact().unwrap()), b"core").unwrap();
    fs::write(request.runtime_kit.join("licenses").join(&core.license_file), b"licence").unwrap();
    let mut data = vec![0u8; 2352 * 4];
    let boot = b"BOOT = cdrom:\\SCES_015.64;1";
    data[0x40..0x40 + boot.len()].copy_from_slice(boot);
    fs::write(root.join("Tiny Disc (Europe).bin"), &data).unwrap();
    let cue = root.join("Tiny Disc (Europe).cue");
    fs::write(&cue, "FILE \"Tiny Disc (Europe).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n").unwrap();
    request.game.rom = cue;
    request.game.system = "ps1".into();
    (request, data)
}

fn names(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|path| path.file_name().unwrap().to_string_lossy().into_owned()).collect()
}

#[test]
fn a_ppf_beside_a_playstation_disc_is_shipped_under_its_serial_and_the_disc_travels_as_it_is() {
    let root = workspace();
    let (mut request, data) = playstation_disc(&root);
    let patch = ppf(100, b"PATCHED");
    fs::write(root.join("Tiny Disc (Europe).ppf"), &patch).unwrap();

    let set = content::collect_for(&request.game.rom, Some("ps1")).unwrap();
    assert_eq!(names(&set.played_patches), ["Tiny Disc (Europe).ppf"]);
    assert_eq!(names(&set.patches()), ["Tiny Disc (Europe).ppf"], "it is the game's patch, on the Files list");

    request.output_dir = root.join("patched");
    let app = export_game(&request, &AtomicBool::new(false), |_| {}).unwrap().app_path;
    let resources = app.join("Contents/Resources");
    assert_eq!(fs::read(resources.join("patches/SCES_015.64")).unwrap(), patch);
    assert_eq!(fs::read(resources.join("content/Tiny Disc (Europe).bin")).unwrap(), data, "the disc is not changed");
    assert!(!resources.join("content/Tiny Disc (Europe).ppf").exists());
}

#[test]
fn a_ppf_beside_the_disc_under_another_name_stays_behind_and_one_the_author_chose_travels() {
    let root = workspace();
    let (request, _) = playstation_disc(&root);
    fs::write(root.join("Translation.ppf"), ppf(100, b"PATCHED")).unwrap();
    let set = content::collect_for(&request.game.rom, Some("ps1")).unwrap();
    assert!(set.played_patches.is_empty(), "a PPF states no game, so beside it one needs the game's name");

    let chosen = GameFiles { added: vec![root.join("Translation.ppf")], ..GameFiles::default() };
    let set = content::collect_with(&request.game.rom, Some("ps1"), &chosen).unwrap();
    assert_eq!(names(&set.played_patches), ["Translation.ppf"]);
}

#[test]
fn the_emulator_reads_one_ppf_so_a_second_the_author_chose_is_refused() {
    let root = workspace();
    let (request, _) = playstation_disc(&root);
    fs::write(root.join("A.ppf"), ppf(100, b"FIRST")).unwrap();
    fs::write(root.join("B.ppf"), ppf(200, b"SECOND")).unwrap();
    let chosen = GameFiles { added: vec![root.join("A.ppf"), root.join("B.ppf")], ..GameFiles::default() };
    let set = content::collect_with(&request.game.rom, Some("ps1"), &chosen).unwrap();
    assert_eq!(names(&set.played_patches), ["A.ppf"]);
    assert_eq!(names(&set.refused_patches), ["B.ppf"]);
}

#[test]
fn a_ppf_for_a_disc_whose_serial_cannot_be_read_refuses_the_export() {
    let root = workspace();
    let (request, _) = playstation_disc(&root);
    fs::write(root.join("Tiny Disc (Europe).bin"), vec![0u8; 2352 * 4]).unwrap();
    fs::write(root.join("Tiny Disc (Europe).ppf"), ppf(100, b"PATCHED")).unwrap();
    let refused = export_game(&request, &AtomicBool::new(false), |_| {}).unwrap_err();
    assert_eq!(refused.stage, ErrorStage::Refused);
    assert!(refused.message.contains("the serial of \"Tiny Disc (Europe).cue\" could not be read"), "{}", refused.message);
}

#[test]
fn a_ppf_the_author_chose_for_another_console_is_refused() {
    let root = workspace();
    let request = export_request(&root);
    fs::write(root.join("Fix.ppf"), ppf(1, b"X")).unwrap();
    let chosen = GameFiles { added: vec![root.join("Fix.ppf")], ..GameFiles::default() };
    let set = content::collect_with(&request.game.rom, Some("megadrive"), &chosen).unwrap();
    assert!(set.played_patches.is_empty());
    assert_eq!(names(&set.refused_patches), ["Fix.ppf"]);
}
