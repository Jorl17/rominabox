use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct Catalog {
    version: u32,
    systems: Vec<System>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct System {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub extensions: Vec<String>,
    /// Sibling files that must be collected with the content, by extension.
    #[serde(default)]
    pub support_files: Vec<String>,
    /// Extensions by which we recognise this console but cannot export it yet.
    /// We declare them per console and not in the exporter code.
    #[serde(default)]
    pub recognize_only: Vec<String>,
    pub catalog: Option<String>,
    pub cores: Vec<Core>,
    #[serde(default)]
    pub firmware: Vec<FirmwareRequirement>,
    pub controller_profile: String,
    pub category: String,
    /// The position of the ASCII title in the cartridge header of this console,
    /// when it has one. We declare it in the console package, not in code here.
    #[serde(default)]
    pub header_title: Option<HeaderTitle>,
}

/// A bounded ASCII field inside a cartridge header.
#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct HeaderTitle {
    pub offset: u64,
    pub length: u64,
}

/// The target of this build, in the triple format of the packages.
///
/// Callers pass a target and do not assume one, so to support a platform we
/// prepare its kit and declare its binaries.
pub fn current_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x86_64",
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-arm64",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-arm64",
        _ => "unsupported",
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Core {
    /// Target triple -> artifact filename. A core is not one file. The same
    /// component is a .dylib on macOS and a .dll on Windows, and the kit for
    /// each target has a separate one.
    pub artifacts: std::collections::BTreeMap<String, String>,
    pub component: String,
    pub license: String,
    pub license_file: String,
    /// What this BUILD of the core can do. When an upstream project supports a
    /// format, that does not mean the artifact we download includes it.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// The libretro library name of this build.
    ///
    /// We apply the emulated controller with a remap file in a folder named
    /// after this string, so without it there is no place to write one.
    #[serde(default, rename = "libraryName")]
    pub library_name: Option<String>,
}

impl Core {
    /// The artifact filename for a target, if this component declares one.
    pub fn artifact_for(&self, target: &str) -> Option<&str> {
        self.artifacts.get(target).map(String::as_str)
    }

    /// The artifact for the target we are running on.
    pub fn artifact(&self) -> Option<&str> {
        self.artifact_for(current_target())
    }

    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|value| value == capability)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareRequirement {
    pub id: String,
    pub accepted_names: Vec<String>,
    pub minimum: usize,
    pub help: String,
}

impl System {
    /// Whether we can recognise a dropped file but not export it.
    pub fn is_recognize_only(&self, extension: &str) -> bool {
        self.recognize_only
            .iter()
            .any(|value| value.eq_ignore_ascii_case(extension))
    }

    pub fn preferred_core(&self) -> Option<&Core> {
        self.cores.first()
    }
}

/// Returns the declared system metadata. A core entry lists a core we may
/// package and its licence. It does not mean that a runtime kit contains the
/// core or that we have tested gameplay on a target platform.
pub fn registry() -> &'static [System] {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    let catalog = CATALOG.get_or_init(|| {
        let parsed: Catalog = serde_json::from_str(include_str!("../../systems.json"))
            .expect("desktop/systems.json must be valid");
        assert_eq!(parsed.version, 1, "unsupported systems catalog version");
        parsed
    });
    &catalog.systems
}

pub fn find(value: &str) -> Option<&'static System> {
    let value = value.trim();
    registry().iter().find(|system| {
        system.id.eq_ignore_ascii_case(value)
            || system
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(value))
    })
}

pub fn candidates_for_extension(extension: &str) -> Vec<&'static System> {
    let extension = extension.trim_start_matches('.');
    registry()
        .iter()
        .filter(|system| {
            system
                .extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_and_shared_extensions_come_from_one_registry() {
        assert_eq!(
            find("Genesis").map(|system| system.id.as_str()),
            Some("megadrive")
        );
        assert_eq!(find("Mega Drive").unwrap().controller_profile, "megadrive");
        assert_eq!(
            find("Game Boy Color").unwrap().controller_profile,
            "gameboy"
        );
        let cue: Vec<_> = candidates_for_extension(".cue")
            .into_iter()
            .map(|system| system.id.as_str())
            .collect();
        assert_eq!(cue, ["segacd", "ps1", "pcecd"]);
        assert_eq!(
            find("nes").unwrap().preferred_core().unwrap().artifact().unwrap(),
            "nestopia_libretro.dylib"
        );
        assert_eq!(find("sg1000").unwrap().controller_profile, "mastersystem");
        let sega_cd = find("Sega CD").unwrap();
        assert_eq!(sega_cd.firmware[0].minimum, 1);
        assert!(sega_cd.firmware[0]
            .accepted_names
            .iter()
            .any(|name| name == "bios_CD_U.bin"));
        assert_eq!(
            find("pcecd").unwrap().firmware[0].accepted_names,
            ["syscard3.pce"]
        );
    }
}
