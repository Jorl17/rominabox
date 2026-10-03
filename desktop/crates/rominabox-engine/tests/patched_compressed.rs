//! A patch for a compressed disc (CHD). We cannot write a CHD in the builder,
//! so we export a patched one decompressed, as a cue sheet and its tracks,
//! and only when the author chose to include the patch. Until then we ask in
//! the details step, with the size either way, and refuse to export, with the
//! reason. Without the patch, we export the disc as it is.
#![cfg(target_os = "macos")]

mod chd_writer;
mod export_fixture;
mod patch_writers;

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use chd_writer::{write_cd, CdTrack};
use export_fixture::{export_request, workspace};
use patch_writers::bps;
use rominabox_engine::content::GameFiles;
use rominabox_engine::packaging::{export_game, ErrorStage, ExportRequest};

/// A PlayStation disc of a data track and an audio track as a CHD in `root`,
/// with a patch for the data track beside it, and an export request for it:
/// the data track, what the patch makes of it, and the audio track.
fn compressed_disc_with_patch(root: &Path) -> (ExportRequest, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut request = export_request(root);
    let core = &rominabox_engine::systems::find("ps1").unwrap().cores[0];
    fs::write(request.runtime_kit.join("cores").join(core.artifact().unwrap()), b"core").unwrap();
    fs::write(request.runtime_kit.join("licenses").join(&core.license_file), b"licence").unwrap();
    let data: Vec<u8> = (0..4 * 2352).map(|at| (at % 253) as u8).collect();
    let mut patched = data.clone();
    patched[3000..3008].copy_from_slice(b"PATCHED!");
    let audio: Vec<u8> = (0..2 * 2352).map(|at| (at % 241) as u8).collect();
    let chd = root.join("Tiny Disc (Europe).chd");
    write_cd(
        &chd,
        &[CdTrack { kind: "MODE2_RAW", bytes: &data, pregap: 0 }, CdTrack { kind: "AUDIO", bytes: &audio, pregap: 0 }],
    );
    fs::write(root.join("Director's Cut.bps"), bps(&data, &patched)).unwrap();
    request.game.rom = chd;
    request.game.system = "ps1".into();
    (request, data, patched, audio)
}

fn content_of(app: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(app.join("Contents/Resources/content"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn the_details_step_asks_before_a_compressed_disc_is_decompressed_and_export_waits_for_the_answer() {
    let root = workspace();
    let (request, data, _, audio) = compressed_disc_with_patch(&root);

    let traveling = rominabox_engine::traveling::files_with(&request.game.rom, Some("ps1"), &request.game.files).unwrap();
    let compressed = traveling.compressed.expect("the author is asked");
    assert_eq!(compressed.game, "Tiny Disc (Europe).chd");
    assert_eq!(compressed.patches, ["Director's Cut.bps"]);
    assert_eq!(compressed.without_bytes, fs::metadata(&request.game.rom).unwrap().len());
    let sheet_bytes = "FILE \"Tiny Disc (Europe) (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n\
                       FILE \"Tiny Disc (Europe) (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n"
        .len();
    assert_eq!(compressed.with_bytes, (data.len() + audio.len() + sheet_bytes) as u64);
    assert!(!compressed.included);
    assert_eq!(traveling.patches, ["Director's Cut.bps"], "the Files list names the patch while it waits");

    let refused = export_game(&request, &AtomicBool::new(false), |_| {}).unwrap_err();
    assert_eq!(refused.stage, ErrorStage::Refused);
    assert!(
        refused.message.contains("\"Director's Cut.bps\" changes \"Tiny Disc (Europe).chd\", which is compressed"),
        "{}",
        refused.message
    );
}

#[test]
fn an_included_patch_exports_the_disc_decompressed_with_the_patch_on_its_track() {
    let root = workspace();
    let (mut request, _, patched, audio) = compressed_disc_with_patch(&root);
    request.game.files = GameFiles { decompress: true, ..GameFiles::default() };

    let traveling = rominabox_engine::traveling::files_with(&request.game.rom, Some("ps1"), &request.game.files).unwrap();
    assert!(traveling.compressed.unwrap().included);
    let app = export_game(&request, &AtomicBool::new(false), |_| {}).unwrap().app_path;

    assert_eq!(
        content_of(&app),
        ["Tiny Disc (Europe) (Track 1).bin", "Tiny Disc (Europe) (Track 2).bin", "Tiny Disc (Europe).cue"],
        "the CHD itself does not travel, nor anything written on the way"
    );
    let content = app.join("Contents/Resources/content");
    assert_eq!(fs::read(content.join("Tiny Disc (Europe) (Track 1).bin")).unwrap(), patched);
    assert_eq!(fs::read(content.join("Tiny Disc (Europe) (Track 2).bin")).unwrap(), audio);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(app.join("Contents/Resources/game.json")).unwrap()).unwrap();
    assert_eq!(manifest["rom"], "content/Tiny Disc (Europe).cue", "the game starts from the sheet");
}

#[test]
fn a_patch_left_out_leaves_the_compressed_disc_as_it_is() {
    let root = workspace();
    let (mut request, _, _, _) = compressed_disc_with_patch(&root);
    request.game.files = GameFiles { left_out: vec!["Director's Cut.bps".into()], ..GameFiles::default() };

    let traveling = rominabox_engine::traveling::files_with(&request.game.rom, Some("ps1"), &request.game.files).unwrap();
    assert!(traveling.compressed.is_none());
    assert!(traveling.patches.is_empty());
    let app = export_game(&request, &AtomicBool::new(false), |_| {}).unwrap().app_path;
    assert_eq!(content_of(&app), ["Tiny Disc (Europe).chd"]);
    assert_eq!(
        fs::read(app.join("Contents/Resources/content/Tiny Disc (Europe).chd")).unwrap(),
        fs::read(&request.game.rom).unwrap()
    );
}
