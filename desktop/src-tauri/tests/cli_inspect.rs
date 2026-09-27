//! Identifying a cartridge from a cached catalogue with the CLI.
//!
//! We compute the checksums for the catalogue match on the main thread of the
//! CLI, which has 1 MB of stack on Windows and 8 MB on macOS, and check that
//! identification finishes there without overflowing the stack. We download
//! nothing and write the catalogue into the cache as a lookup leaves it.

use rominabox_scratch::Scratch;
use sha1::{Digest, Sha1};
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

const CATALOG: &str = "Sega - Mega Drive - Genesis";
const NAME: &str = "ROM-in-a-Box Test Cartridge (World)";

#[test]
fn a_cartridge_in_a_cached_catalogue_is_identified_by_its_checksums() {
    let scratch = Scratch::dir("rominabox-cli-inspect");
    // A generated cartridge: the Mega Drive header's console name, then a
    // pattern no real dump has.
    let mut cartridge: Vec<u8> = (0..65536u32).map(|i| (i * 7 % 251) as u8).collect();
    cartridge[0x100..0x110].copy_from_slice(b"SEGA MEGA DRIVE ");
    let rom = scratch.path().join("cartridge.md");
    fs::write(&rom, &cartridge).unwrap();

    let cache = scratch.path().join("cache");
    fs::create_dir_all(cache.join("catalogs")).unwrap();
    let sha1: String = Sha1::digest(&cartridge)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect();
    fs::write(
        cache.join("catalogs").join(format!("{CATALOG}.dat")),
        format!(
            "clrmamepro (\n\tname \"{CATALOG}\"\n)\n\ngame (\n\tname \"{NAME}\"\n\trom ( name \"{NAME}.md\" size {} crc {:08X} sha1 {sha1} )\n)\n",
            cartridge.len(),
            crc32fast::hash(&cartridge),
        ),
    )
    .unwrap();

    let request = serde_json::json!({ "rom": rom, "cache": cache, "online": false });
    let mut child = Command::new(env!("CARGO_BIN_EXE_rominabox-cli"))
        .arg("inspect")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start rominabox-cli");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(request.to_string().as_bytes())
        .expect("send the request");
    let output = child.wait_with_output().expect("wait for rominabox-cli");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "inspect failed with {}: {stdout}{stderr}",
        output.status
    );
    let result: serde_json::Value = stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("a JSON line"))
        .find(|event| event["type"] == "result")
        .unwrap_or_else(|| panic!("inspect printed no result: {stdout}{stderr}"))["result"]
        .clone();
    assert_eq!(result["system"], "megadrive", "{result}");
    assert_eq!(result["matched"], true, "{result}");
    assert_eq!(result["catalogName"], NAME, "{result}");
}
