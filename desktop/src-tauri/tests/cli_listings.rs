//! What an author can name in an export, with the names in the builder. With
//! `designs` and `defaults` we print the designs, palettes, sound packs and
//! settings we use in the builder, so someone with only the command line (and
//! its skill) never has to guess them. Neither command takes a request.

use std::process::{Command, Stdio};

fn result(command: &str) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_rominabox-cli"))
        .arg(command)
        .stdin(Stdio::null())
        .output()
        .expect("run rominabox-cli");
    assert!(output.status.success(), "{command}: {}", String::from_utf8_lossy(&output.stderr));
    let printed: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("{command} printed no JSON: {e}"));
    assert_eq!(printed["type"], "result", "{command}: {printed}");
    printed["result"].clone()
}

fn builders(file: &str) -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(file);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

#[test]
fn designs_are_the_builders() {
    assert_eq!(result("designs"), builders("designs.json"));
}

#[test]
fn defaults_are_the_builders() {
    assert_eq!(result("defaults"), builders("defaults.json"));
}
