use super::*;

#[test]
fn export_refuses_a_missing_required_bios_with_the_builders_explanation() {
    let system = crate::systems::find("pcecd").unwrap();
    let mut settings = request(false);
    settings.game.system = "pcecd".into();
    let error = validate_firmware(&settings, system).unwrap_err();
    let assessment = crate::systems::assess_firmware(system, &[]);
    assert!(!assessment.can_continue);
    assert_eq!(error.message, assessment.refusal());
}

#[test]
fn export_allows_a_console_whose_bios_is_optional() {
    let system = crate::systems::find("ps1").unwrap();
    let mut settings = request(false);
    settings.game.system = "ps1".into();
    assert!(validate_firmware(&settings, system).is_ok());
}

// We reject a disc container by the core that would have to read it.
//
// CHD support depends on the prepared artifact of a core, not on the
// console. We declare CHD for the PC Engine CD core, and do not restrict
// any cartridge core.

fn request_for(system: &str, rom: &str) -> ExportRequest {
    let mut value = request(false);
    value.game.system = system.to_string();
    value.game.rom = PathBuf::from(rom);
    value
}

fn refusal(system: &str, rom: &str) -> Option<String> {
    let value = request_for(system, rom);
    let definition = crate::systems::find(system).expect("known system");
    let core = definition.cores.first()?;
    let extension = value
        .game.rom
        .extension()
        .and_then(OsStr::to_str)?
        .to_ascii_lowercase();
    (!core.capabilities.is_empty()
        && CONTAINER_FORMATS.contains(&extension.as_str())
        && !core.supports(&extension))
    .then(|| core.component.clone())
}

/// In the CHD check, we read the capability from the built core, not from the
/// console. The Genesis Plus GX core is compiled with CHD, so the author can
/// export a Sega CD game in CHD.
///
/// The capability belongs to each artifact. Another core, or a different build
/// for a target, can lack a format, and we check the binary.
#[test]
fn sega_cd_chd_works_now_that_the_core_is_built_with_chd() {
    assert_eq!(
        refusal("segacd", "game.chd"),
        None,
        "HAVE_CHD=1 is declared by the component, so this must not be refused"
    );
}

#[test]
fn every_disc_console_accepts_what_its_core_can_actually_decode() {
    for (system, rom) in [
        ("segacd", "game.cue"),
        ("segacd", "game.chd"),
        ("segacd", "game.iso"),
        ("pcecd", "game.chd"),
        ("ps1", "game.chd"),
        ("ps1", "game.pbp"),
    ] {
        assert_eq!(refusal(system, rom), None, "{system} should accept {rom}");
    }
}

/// We reject an export when the core lacks the format.
#[test]
fn a_format_the_selected_core_cannot_decode_is_still_refused() {
    assert_eq!(
        refusal("segacd", "game.rvz").as_deref(),
        Some("genesis_plus_gx"),
        "a format outside the core's decoded set must still be refused"
    );
}

#[test]
fn a_cartridge_core_is_never_constrained_by_this_check() {
    for (system, rom) in [("megadrive", "game.md"), ("nes", "game.nes")] {
        assert_eq!(refusal(system, rom), None, "{system} must be unaffected");
    }
}

/// A Mac game made on a Windows or Linux builder runs on Apple silicon, and
/// on Intel Macs too when the author selects that. The player in the Mac kit
/// contains both. On a Mac builder, we make a game for the same kind of Mac.
#[test]
fn a_mac_game_made_on_another_system_is_for_apple_silicon_and_intel_when_asked() {
    use Target::{MacosArm64, MacosX86_64, WindowsX86_64};
    for host in [Some(WindowsX86_64), None] {
        assert_eq!(ExportTarget::Macos.targets_on(host, false), vec![MacosArm64], "{host:?}");
        assert_eq!(
            ExportTarget::Macos.targets_on(host, true),
            vec![MacosArm64, MacosX86_64],
            "{host:?}"
        );
        assert_eq!(ExportTarget::Windows.targets_on(host, true), vec![WindowsX86_64], "{host:?}");
    }
    assert_eq!(ExportTarget::Macos.targets_on(Some(MacosArm64), false), vec![MacosArm64]);
    assert_eq!(
        ExportTarget::Macos.targets_on(Some(MacosArm64), true),
        vec![MacosArm64, MacosX86_64]
    );
    assert_eq!(ExportTarget::Macos.targets_on(Some(MacosX86_64), true), vec![MacosX86_64]);
}
