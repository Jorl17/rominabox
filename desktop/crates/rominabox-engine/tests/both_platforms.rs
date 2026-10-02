//! One download for both platforms. We zip the game made for Mac and for
//! Windows as `<title>.zip`, with `Mac/<title>.app` and
//! `Windows/<title>.exe`. Unpacked on a Mac, the Mac game is signed and its
//! programs are executable, and the Windows game is the one program we make
//! in a Windows builder.
#![cfg(target_os = "macos")]

mod export_fixture;

use export_fixture::{export_request_from, fixture_kit, unpack, windows_kit_here, workspace};
use rominabox_engine::packaging::{export_for_both, ErrorStage, ExportTarget};
use std::os::unix::fs::PermissionsExt;
use std::{fs, path::PathBuf, process::Command, sync::atomic::AtomicBool};

#[test]
fn both_platforms_are_one_zip_holding_each_game() {
    let root = workspace();
    let mac_kit = fixture_kit(&root.join("mac"));
    let windows_kit = windows_kit_here(&root.join("windows"));
    let request = export_request_from(&root, PathBuf::new());
    let kit_for = |platform: &ExportTarget| -> Result<PathBuf, String> {
        Ok(match platform {
            ExportTarget::Macos => mac_kit.clone(),
            ExportTarget::Windows => windows_kit.clone(),
        })
    };
    let cancelled = AtomicBool::new(false);
    let mut fractions = Vec::new();
    let result = export_for_both(&request, &kit_for, &|_| None, &cancelled, |event| {
        fractions.push(event.fraction)
    })
    .unwrap();

    let zip = request.output_dir.join("Hotkey Isolation.zip");
    assert_eq!(result.app_path, zip);
    let written: Vec<_> = fs::read_dir(&request.output_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(written, ["Hotkey Isolation.zip"], "only the zip is left");
    assert!(fractions.windows(2).all(|pair| pair[0] <= pair[1]), "{fractions:?}");
    assert_eq!(fractions.last(), Some(&1.0));

    let unzipped = root.join("unzipped");
    let status = Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(&zip)
        .arg(&unzipped)
        .status()
        .unwrap();
    assert!(status.success());
    let app = unzipped.join("Mac/Hotkey Isolation.app");
    let player = app.join("Contents/MacOS/retroarch");
    assert_eq!(fs::metadata(&player).unwrap().permissions().mode() & 0o777, 0o755);
    let verified = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let program = unzipped.join("Windows/Hotkey Isolation.exe");
    let windows = root.join("windows-unpacked");
    unpack(&program, &windows);
    assert!(windows.join("Hotkey Isolation.exe").is_file());
    assert!(windows.join("Runtime/retroarch.exe").is_file());
    assert!(windows.join("Resources/game-core.dll").is_file());

    // A second export does not replace the first unless set in the request.
    let refused = export_for_both(&request, &kit_for, &|_| None, &cancelled, |_| {}).unwrap_err();
    assert_eq!(refused.stage, ErrorStage::Exists);
    let mut replacing = request.clone();
    replacing.replace = true;
    export_for_both(&replacing, &kit_for, &|_| None, &cancelled, |_| {}).unwrap();
}
