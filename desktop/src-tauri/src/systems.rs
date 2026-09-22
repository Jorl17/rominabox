use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// What to tell an author about the BIOS files they have and the ones they still need.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FirmwareNoticeKind {
    /// A required file is still missing. The author cannot continue.
    Required,
    /// The author supplied the same filename twice. We would refuse it at export.
    Duplicate,
    /// A supplied file is not one we accept for this console, so it does not count.
    Unmatched,
    /// Games for this console can start without a BIOS. We only inform the author.
    Optional,
    /// Every required file is present.
    Ready,
}

impl FirmwareNoticeKind {
    /// We stop the author for a missing required file or a duplicate name, and
    /// not for an optional BIOS, a file that did not match, or a confirmation.
    pub fn blocks_progress(self) -> bool {
        matches!(self, Self::Required | Self::Duplicate)
    }

    fn order(self) -> u8 {
        match self {
            Self::Required => 0,
            Self::Duplicate => 1,
            Self::Unmatched => 2,
            Self::Optional => 3,
            Self::Ready => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareNotice {
    pub kind: FirmwareNoticeKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareFileStatus {
    pub name: String,
    pub counted: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareAssessment {
    pub can_continue: bool,
    pub notices: Vec<FirmwareNotice>,
    pub files: Vec<FirmwareFileStatus>,
}

impl FirmwareAssessment {
    /// The sentences we show when we refuse an export, in the builder and at export.
    pub fn refusal(&self) -> String {
        self.notices
            .iter()
            .filter(|notice| {
                notice.kind.blocks_progress() || notice.kind == FirmwareNoticeKind::Unmatched
            })
            .map(|notice| notice.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

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

/// Whether these files meet the BIOS requirements of this console.
///
/// We use this answer both to refuse an export and in the builder, so the two
/// cannot disagree about what "required" means.
pub fn assess_firmware(system: &System, files: &[PathBuf]) -> FirmwareAssessment {
    let mut notices = Vec::new();
    let mut reported = Vec::new();
    let mut seen = HashSet::new();
    let mut counts = vec![0usize; system.firmware.len()];

    for path in files {
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            let label = path.display().to_string();
            let text = format!("{label} has no usable name, so it cannot be a BIOS.");
            reported.push(FirmwareFileStatus {
                name: label,
                counted: false,
                reason: Some(text.clone()),
            });
            notices.push(FirmwareNotice {
                kind: FirmwareNoticeKind::Unmatched,
                text,
            });
            continue;
        };
        let key = name.to_ascii_lowercase();
        if !seen.insert(key) {
            let text = format!("{name} is listed more than once. Remove the extra copy.");
            reported.push(FirmwareFileStatus {
                name: name.to_string(),
                counted: false,
                reason: Some(text.clone()),
            });
            notices.push(FirmwareNotice {
                kind: FirmwareNoticeKind::Duplicate,
                text,
            });
            continue;
        }
        let mut matched = false;
        for (index, group) in system.firmware.iter().enumerate() {
            if group
                .accepted_names
                .iter()
                .any(|accepted| accepted.eq_ignore_ascii_case(name))
            {
                counts[index] += 1;
                matched = true;
            }
        }
        if matched {
            reported.push(FirmwareFileStatus {
                name: name.to_string(),
                counted: true,
                reason: None,
            });
            continue;
        }
        let text = unmatched_reason(system, name);
        reported.push(FirmwareFileStatus {
            name: name.to_string(),
            counted: false,
            reason: Some(text.clone()),
        });
        notices.push(FirmwareNotice {
            kind: FirmwareNoticeKind::Unmatched,
            text,
        });
    }

    let mut short_of_required = false;
    let mut required_groups = 0usize;
    for (group, have) in system.firmware.iter().zip(&counts) {
        if group.minimum == 0 {
            if *have == 0 {
                notices.push(FirmwareNotice {
                    kind: FirmwareNoticeKind::Optional,
                    text: group.help.clone(),
                });
            }
            continue;
        }
        required_groups += 1;
        if *have < group.minimum {
            short_of_required = true;
            notices.push(FirmwareNotice {
                kind: FirmwareNoticeKind::Required,
                text: required_explanation(system, group, *have),
            });
        }
    }

    let blocked = notices.iter().any(|notice| notice.kind.blocks_progress());
    if required_groups > 0 && !short_of_required && !blocked {
        notices.push(FirmwareNotice {
            kind: FirmwareNoticeKind::Ready,
            text: format!("{} has the BIOS it needs.", system.name),
        });
    }
    notices.sort_by_key(|notice| notice.kind.order());

    FirmwareAssessment {
        can_continue: !blocked,
        notices,
        files: reported,
    }
}

fn required_explanation(system: &System, group: &FirmwareRequirement, have: usize) -> String {
    let counted = if have > 0 {
        format!(" {have} of them already counted.")
    } else {
        String::new()
    };
    format!(
        "{name} cannot start without {needed}.{counted} {help} This console does not include a BIOS that can start the game.",
        name = system.name,
        needed = needed_phrase(group),
        help = group.help,
    )
}

fn needed_phrase(group: &FirmwareRequirement) -> String {
    let names = &group.accepted_names;
    if names.is_empty() {
        return format!("{} BIOS file", group.minimum.max(1));
    }
    if group.minimum == 1 && names.len() == 1 {
        return format!("the BIOS file {}", names[0]);
    }
    if group.minimum == 1 {
        return format!("one of these BIOS files: {}", join_choice(names));
    }
    format!(
        "{} of these BIOS files: {}",
        group.minimum,
        join_choice(names)
    )
}

fn unmatched_reason(system: &System, name: &str) -> String {
    let mut accepted = Vec::new();
    let mut seen = HashSet::new();
    for group in &system.firmware {
        for candidate in &group.accepted_names {
            if seen.insert(candidate.to_ascii_lowercase()) {
                accepted.push(candidate.clone());
            }
        }
    }
    if accepted.is_empty() {
        return format!(
            "{name} is not a BIOS for {console}, so it does not count. This console does not use a separate BIOS file.",
            console = system.name,
        );
    }
    format!(
        "{name} is not a BIOS for {console}, so it does not count. This console accepts {accepted}.",
        console = system.name,
        accepted = join_choice(&accepted),
    )
}

fn join_choice(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} or {second}"),
        many => {
            let (last, rest) = many.split_last().expect("at least three names");
            format!("{} or {last}", rest.join(", "))
        }
    }
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
    use std::path::PathBuf;

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
            find("nes")
                .unwrap()
                .preferred_core()
                .unwrap()
                .artifact()
                .unwrap(),
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

    #[test]
    fn a_missing_required_bios_stops_progress_and_names_the_file() {
        let assessment = assess_firmware(find("pcecd").unwrap(), &[]);
        assert!(!assessment.can_continue);
        assert_eq!(assessment.notices[0].kind, FirmwareNoticeKind::Required);
        assert_eq!(
            assessment.notices[0].text,
            "PC Engine CD / TurboGrafx-CD cannot start without the BIOS file syscard3.pce. Select a Super CD-ROM2 System Card 3 BIOS that you are entitled to use. This console does not include a BIOS that can start the game."
        );
    }

    #[test]
    fn an_optional_bios_does_not_stop_progress() {
        let assessment = assess_firmware(find("ps1").unwrap(), &[]);
        assert!(assessment.can_continue);
        assert!(assessment
            .notices
            .iter()
            .all(|notice| notice.kind == FirmwareNoticeKind::Optional));
        assert!(assessment.notices[0].text.contains("built-in"));
        assert!(!assessment
            .notices
            .iter()
            .any(|notice| notice.kind == FirmwareNoticeKind::Required));
    }

    #[test]
    fn one_accepted_region_file_is_enough_when_the_minimum_is_one() {
        let sega = find("segacd").unwrap();
        assert!(!assess_firmware(sega, &[]).can_continue);
        let assessment = assess_firmware(sega, &[PathBuf::from("bios_CD_J.bin")]);
        assert!(assessment.can_continue);
        assert!(assessment.files[0].counted);
        assert_eq!(assessment.notices[0].kind, FirmwareNoticeKind::Ready);
    }

    #[test]
    fn a_matching_name_is_accepted_without_regard_to_case() {
        let assessment = assess_firmware(find("pcecd").unwrap(), &[PathBuf::from("SysCard3.PCE")]);
        assert!(assessment.can_continue);
        assert!(assessment.files[0].counted);
        assert!(assessment.files[0].reason.is_none());
    }

    #[test]
    fn an_unmatched_file_says_why_it_did_not_count() {
        let assessment =
            assess_firmware(find("pcecd").unwrap(), &[PathBuf::from("/tmp/notes.txt")]);
        assert!(!assessment.can_continue);
        assert_eq!(assessment.files[0].name, "notes.txt");
        assert!(!assessment.files[0].counted);
        assert_eq!(
            assessment.files[0].reason.as_deref(),
            Some("notes.txt is not a BIOS for PC Engine CD / TurboGrafx-CD, so it does not count. This console accepts syscard3.pce.")
        );
    }

    #[test]
    fn an_optional_console_explains_an_unmatched_file_and_still_continues() {
        let assessment = assess_firmware(find("ps1").unwrap(), &[PathBuf::from("notes.txt")]);
        assert!(assessment.can_continue);
        assert!(!assessment.files[0].counted);
        let reason = assessment.files[0].reason.as_deref().unwrap();
        assert!(reason.contains("notes.txt"));
        assert!(reason.contains("PlayStation"));
        assert!(reason.contains("scph5501.bin"));
        assert!(assessment
            .notices
            .iter()
            .any(|notice| notice.kind == FirmwareNoticeKind::Optional));
        assert!(!assessment
            .notices
            .iter()
            .any(|notice| notice.kind.blocks_progress()));
    }

    #[test]
    fn a_console_with_no_bios_requirement_is_not_blocked() {
        let assessment = assess_firmware(find("megadrive").unwrap(), &[]);
        assert!(assessment.can_continue);
        assert!(assessment.notices.is_empty());
    }

    #[test]
    fn a_repeated_filename_stops_progress_even_when_the_name_is_accepted() {
        let assessment = assess_firmware(
            find("pcecd").unwrap(),
            &[
                PathBuf::from("syscard3.pce"),
                PathBuf::from("elsewhere/SYSCARD3.PCE"),
            ],
        );
        assert!(!assessment.can_continue);
        assert!(assessment.files[0].counted);
        assert!(!assessment.files[1].counted);
        assert!(assessment
            .notices
            .iter()
            .any(|notice| notice.kind == FirmwareNoticeKind::Duplicate));
    }

    #[test]
    fn the_minimum_is_a_count_not_a_single_file() {
        let system = System {
            id: "example".into(),
            name: "Example Console".into(),
            aliases: Vec::new(),
            extensions: Vec::new(),
            support_files: Vec::new(),
            recognize_only: Vec::new(),
            catalog: None,
            cores: Vec::new(),
            firmware: vec![FirmwareRequirement {
                id: "example-bios".into(),
                accepted_names: vec!["a.bin".into(), "b.bin".into(), "c.bin".into()],
                minimum: 2,
                help: "Two region files are required.".into(),
            }],
            controller_profile: "retropad".into(),
            category: "cartridge".into(),
            header_title: None,
        };
        let one = assess_firmware(&system, &[PathBuf::from("a.bin")]);
        assert!(!one.can_continue);
        assert_eq!(
            one.notices[0].text,
            "Example Console cannot start without 2 of these BIOS files: a.bin, b.bin or c.bin. 1 of them already counted. Two region files are required. This console does not include a BIOS that can start the game."
        );
        let two = assess_firmware(&system, &[PathBuf::from("a.bin"), PathBuf::from("C.BIN")]);
        assert!(two.can_continue);
        assert!(two.files.iter().all(|file| file.counted));
    }
}
