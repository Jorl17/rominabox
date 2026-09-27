//! The icon and the name of a game's programs on Windows.
//!
//! A Windows game is two programs, the launcher a person opens and the player
//! whose window they see. In Explorer a program has the icon of its first
//! icon group, and the player's window has group 1 (`IDI_ICON`, 1). In Task
//! Manager a process has the name in its version information. So we give
//! both programs the game's icon as their one icon group, 1, and its title.

use std::path::Path;

use editpe::constants::{RT_GROUP_ICON, RT_ICON};
use editpe::types::{FixedFileInfo, VersionU16};
use editpe::{Image, ResourceEntryName, VersionInfo, VersionStringTable};

use crate::export_error::{ErrorStage, ExportError};

/// The icon group for the window and the file icon of every program.
const ICON_GROUP: u32 = 1;
/// The version we give the game's programs, as we do in
/// CFBundleShortVersionString in its macOS bundle.
const VERSION: &str = "1.0";

/// Writes `title`, and `icon` (an `.ico`) when there is one, into the program
/// at `path`, in place.
pub fn describe(path: &Path, icon: Option<&[u8]>, title: &str) -> Result<(), ExportError> {
    let failed = |what: &str, error: &dyn std::fmt::Display| {
        ExportError::new(ErrorStage::Icon, format!("{}: could not {what}: {error}", path.display()))
            .about(path)
    };
    let mut image = Image::parse_file(path).map_err(|error| failed("read the program", &error))?;
    let mut resources = image.resource_directory().cloned().unwrap_or_default();

    if let Some(icon) = icon {
        // We keep only the game's icon. Otherwise the RetroArch icon would stay
        // in the player, and the file icon would be the group that sorts first.
        resources.root_mut().remove(ResourceEntryName::ID(RT_ICON as u32));
        resources.root_mut().remove(ResourceEntryName::ID(RT_GROUP_ICON as u32));
        resources.set_main_icon(icon).map_err(|error| failed("set its icon", &error))?;
        let groups = resources
            .root_mut()
            .get_mut(ResourceEntryName::ID(RT_GROUP_ICON as u32))
            .and_then(|entry| entry.as_table_mut())
            .ok_or_else(|| failed("find its icon group", &"none was written"))?;
        let group = groups
            .remove(ResourceEntryName::from_string("MAINICON"))
            .ok_or_else(|| failed("find its icon group", &"MAINICON was not written"))?;
        groups.insert(ResourceEntryName::ID(ICON_GROUP), group);
    }

    let mut strings = VersionStringTable {
        key: "040904B0".into(),
        ..Default::default()
    };
    for (key, value) in [
        ("FileDescription", title),
        ("ProductName", title),
        ("FileVersion", VERSION),
        ("ProductVersion", VERSION),
    ] {
        strings.strings.insert(key.into(), value.into());
    }
    let version = VersionInfo {
        info: FixedFileInfo::default(),
        strings: vec![strings],
        // The key of the string table, for English (United States), Unicode.
        vars: vec![VersionU16 {
            major: 0x0409,
            minor: 0x04B0,
        }],
    };
    resources
        .set_version_info(&version)
        .map_err(|error| failed("set its name", &error))?;

    image
        .set_resource_directory(resources)
        .map_err(|error| failed("write its resources", &error))?;
    image.write_file(path).map_err(|error| failed("write the program", &error))
}
