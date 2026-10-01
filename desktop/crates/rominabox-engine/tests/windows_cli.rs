//! The command line as a person sees it on Windows. In Explorer, in the
//! properties of a program and in Task Manager, it has the builder's icon,
//! name and version, as declared in the builder's configuration.
#![cfg(windows)]

use editpe::constants::RT_ICON;
use editpe::{Image, ResourceEntryName};
use rominabox_engine::repo;
use serde_json::Value;
use std::fs;

#[test]
fn the_command_line_has_the_builders_icon_name_and_version() {
    let config: Value =
        serde_json::from_str(&fs::read_to_string(repo::at("desktop/src-tauri/tauri.conf.json")).unwrap()).unwrap();
    let program = Image::parse_file(env!("CARGO_BIN_EXE_rominabox-cli")).unwrap();
    let resources = program.resource_directory().expect("the command line has resources");

    // The builder's Windows icon, which the configuration lists beside it.
    let named = config["bundle"]["icon"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|path| path.ends_with(".ico"))
        .unwrap();
    let icon = fs::read(repo::at("desktop/src-tauri").join(named)).unwrap();
    let sizes = u16::from_le_bytes([icon[4], icon[5]]) as usize;
    let held = resources
        .root()
        .get(ResourceEntryName::ID(RT_ICON as u32))
        .and_then(|entry| entry.as_table())
        .map_or(0, |table| table.entries().len());
    assert_eq!(held, sizes, "every size of the builder's icon");
    // The first image of the icon, in the order of the directory in the .ico file.
    let at = |offset: usize| u32::from_le_bytes(icon[offset..offset + 4].try_into().unwrap()) as usize;
    let (length, start) = (at(6 + 8), at(6 + 12));
    let first = resources.get_main_icon().unwrap().expect("the command line has an icon");
    assert!(first == &icon[start..start + length], "the command line's icon is the builder's");

    let version = resources.get_version_info().unwrap().expect("the command line says what it is");
    let strings = &version.strings[0].strings;
    for (field, declared) in [
        ("ProductName", "productName"),
        ("FileDescription", "productName"),
        ("ProductVersion", "version"),
        ("FileVersion", "version"),
    ] {
        assert_eq!(strings.get(field).map(String::as_str), config[declared].as_str(), "{field}");
    }
}
