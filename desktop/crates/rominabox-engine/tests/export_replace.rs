//! Exporting onto an app that is already there.
//!
//! We ask no question at export. Unless the request has the replace option, we
//! report `exists` and do nothing. A replacement goes beside the old app, and
//! we swap it in only when it is complete, so after an export failure the old
//! app is as it was.
#![cfg(target_os = "macos")]

mod export_fixture;

use export_fixture::{export_request, workspace};
use rominabox_engine::packaging::{export_game, ExportRequest, ExportStage};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
};

/// The contents of the output folder, by name.
fn names(folder: &Path) -> Vec<String> {
    let mut names = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Export once and mark the app, so a test can tell the old one from a new one.
fn existing_app(request: &ExportRequest) -> std::path::PathBuf {
    let app = export_game(request, &AtomicBool::new(false), |_| {})
        .unwrap()
        .app_path;
    fs::write(app.join("Contents/old-app"), b"the app that was here").unwrap();
    app
}

#[test]
fn an_existing_app_is_reported_and_nothing_is_done() {
    let root = workspace();
    let request = export_request(&root);
    let app = existing_app(&request);

    let mut events = 0;
    let error = export_game(&request, &AtomicBool::new(false), |_| events += 1).unwrap_err();

    assert_eq!(
        serde_json::to_value(error.stage).unwrap(),
        "exists",
        "{error}"
    );
    assert_eq!(
        error.sentence(),
        "An app with this name already exists in out."
    );
    assert_eq!(events, 0, "the export began before asking");
    assert!(app.join("Contents/old-app").is_file());
    assert_eq!(names(&request.output_dir), ["Hotkey Isolation.app"]);
}

#[test]
fn replace_puts_the_new_app_where_the_old_one_was() {
    let root = workspace();
    let mut request = export_request(&root);
    let app = existing_app(&request);

    request.replace = true;
    let result = export_game(&request, &AtomicBool::new(false), |_| {}).unwrap();

    assert_eq!(result.app_path, app);
    assert!(app.join("Contents/Resources/game.json").is_file());
    assert!(
        !app.join("Contents/old-app").exists(),
        "the old app is still there"
    );
    assert_eq!(names(&request.output_dir), ["Hotkey Isolation.app"]);
}

#[test]
fn a_replacement_that_fails_leaves_the_old_app_as_it_was() {
    let root = workspace();
    let mut request = export_request(&root);
    let app = existing_app(&request);
    let runtime = request.runtime_kit.join("bin/retroarch");

    request.replace = true;
    export_game(&request, &AtomicBool::new(false), |progress| {
        if matches!(progress.stage, ExportStage::Stage) {
            fs::remove_file(&runtime).unwrap();
        }
    })
    .unwrap_err();

    assert_eq!(
        fs::read(app.join("Contents/old-app")).unwrap(),
        b"the app that was here"
    );
    assert!(app.join("Contents/Resources/game.json").is_file());
    assert_eq!(names(&request.output_dir), ["Hotkey Isolation.app"]);
}

/// In the CLI we answer with a line a script can act on, not a failure message.
#[test]
fn the_cli_reports_an_existing_app_as_exists() {
    let root = workspace();
    let request = export_request(&root);
    let app = existing_app(&request);

    let output = Command::new(env!("CARGO_BIN_EXE_rominabox-cli"))
        .arg("export")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(&serde_json::to_vec(&request).unwrap())?;
            child.wait_with_output()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let last: serde_json::Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    assert_eq!(last["type"], "exists", "{stdout}");
    assert_eq!(last["appPath"], app.to_string_lossy().as_ref());
    assert!(!output.status.success());
    assert!(app.join("Contents/old-app").is_file());
}
