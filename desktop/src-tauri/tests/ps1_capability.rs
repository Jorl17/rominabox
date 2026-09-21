//! What PlayStation support requires, and what these tests show.
//!
//! PS1 tests more of the console catalog than Master System, with a core
//! that we do not ship, firmware that the author supplies, disc formats with
//! different support and a second controller mode. We declare each of these
//! in the package instead of adding a branch to the exporter.
//!
//! We do not show here that PlayStation games run. We use no PCSX ReARMed
//! artifact, and where a fixture stands in for one, it is an empty file.
//! Copying an empty file tests staging and nothing about emulation.

use rominabox_desktop::{
    content,
    controls::{self, Controls},
    packaging::{self, SystemAvailability, Unavailable},
    systems,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rominabox-ps1-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn ps1() -> &'static systems::System {
    systems::find("ps1").expect("PlayStation is declared")
}

/// A kit with a stand-in for a core that does not exist yet.
///
/// The bytes are empty on purpose. We test resolution and staging with it,
/// and it is not a core that runs or evidence that one works.
fn kit_with_stub_core(root: &Path) -> PathBuf {
    let kit = root.join("runtime-kit");
    fs::create_dir_all(kit.join("cores")).unwrap();
    fs::create_dir_all(kit.join("licenses")).unwrap();
    let core = ps1().cores.first().expect("a declared core");
    fs::write(
        kit.join("cores")
            .join(core.artifact().expect("an artifact for this target")),
        [],
    )
    .unwrap();
    fs::write(kit.join("licenses").join(&core.license_file), []).unwrap();
    kit
}

fn availability(kit: &Path) -> SystemAvailability {
    packaging::system_availability(kit)
        .into_iter()
        .find(|entry| entry.id == "ps1")
        .expect("every declared console is reported")
}

#[test]
fn playstation_is_declared_but_not_claimed_as_shipped_support() {
    // We keep intent apart from availability so that we can describe and
    // recognise PS1 without a build that claims it can export one.
    let empty = scratch().join("empty-kit");
    fs::create_dir_all(empty.join("cores")).unwrap();
    fs::create_dir_all(empty.join("licenses")).unwrap();

    let reported = availability(&empty);
    assert_eq!(reported.component, None);
    match reported.unavailable {
        Some(Unavailable::NoPreparedCore { ref tried }) => {
            assert!(
                tried.iter().any(|reason| reason.contains("pcsx_rearmed")),
                "the reason must name the core that is missing: {tried:?}"
            );
        }
        other => panic!("expected a missing core, got {other:?}"),
    }
    assert!(!packaging::available_systems(&empty).contains(&"ps1".to_string()));
}

#[test]
fn a_prepared_artifact_is_all_that_stands_between_declared_and_available() {
    // Only the artifact is missing for PS1 to become available. There is no
    // exporter branch, staging list or registration for it.
    let root = scratch();
    let kit = kit_with_stub_core(&root);
    let reported = availability(&kit);
    assert_eq!(
        reported.component.as_deref(),
        Some("pcsx_rearmed"),
        "resolution should select the declared component once its files exist"
    );
    assert_eq!(reported.unavailable, None);
    assert!(packaging::available_systems(&kit).contains(&"ps1".to_string()));
}

#[test]
fn a_playstation_bios_is_offered_but_never_demanded() {
    // PCSX ReARMed has a high-level BIOS, so a PlayStation game runs without
    // one. If we required a file that the author does not need, they would
    // look for a BIOS to fix a problem they do not have.
    let requirement = ps1()
        .firmware
        .first()
        .expect("a BIOS is still worth offering for compatibility");
    assert_eq!(requirement.id, "ps1-bios");
    assert_eq!(
        requirement.minimum, 0,
        "a PlayStation export must not be blocked on a BIOS"
    );
    assert!(
        requirement
            .accepted_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case("scph5501.bin")),
        "the common region BIOS names should be accepted: {:?}",
        requirement.accepted_names
    );
    // We never ship or search for proprietary firmware.
    assert!(
        requirement.help.to_lowercase().contains("entitled"),
        "the help text must not imply we provide a BIOS: {}",
        requirement.help
    );
}

/// The distinction makes sense only when some console requires a BIOS.
#[test]
fn a_console_that_cannot_start_without_a_bios_demands_one() {
    for id in ["segacd", "pcecd"] {
        let system = systems::find(id).expect("a declared console");
        let requirement = system
            .firmware
            .first()
            .unwrap_or_else(|| panic!("{id} cannot start without a BIOS"));
        assert!(
            requirement.minimum >= 1,
            "{id} must demand the BIOS it cannot start without"
        );
    }
    assert!(
        ps1().firmware.iter().all(|group| group.minimum == 0),
        "PlayStation must stay on the optional side of that distinction"
    );
}

/// We do NOT offer the analogue pad, because it is not analogue.
///
/// `ps1-analog` declares sixteen controls: the fourteen digital buttons plus
/// L3 and R3, which are the stick *clicks*. It declares no axes at all, no
/// left stick and no right stick. There is nothing to bind, so the in-game
/// controls menu would show nothing, and an author who chose it would get a
/// pad that looks analogue and behaves digitally.
///
/// For PlayStation we offer only the digital pad until the sticks are
/// declared. This test fails if we offer the analogue pad without them.
#[test]
fn the_analogue_pad_is_not_offered_while_it_declares_no_sticks() {
    let pad = controls::profile_for_system("ps1").expect("a default pad");
    assert_eq!(pad.id, "ps1");
    assert_eq!(pad.image, "controller-ps1.png");

    let pick = |id: &str| -> Controls {
        let mut options = Controls::default();
        options.profile = Some(id.to_string());
        options
    };
    assert!(
        controls::validate_for_system("ps1", &pick("ps1-analog")).is_err(),
        "the analogue pad must not be selectable while it declares no axes"
    );
    assert!(
        controls::validate_for_system("nes", &pick("ps1-analog")).is_err(),
        "a PlayStation pad must not be selectable for another console"
    );
}

/// What it must declare before we can offer it.
///
/// In RetroArch, analogue directions are separate bind entries, `l_x_plus`,
/// `l_x_minus`, `l_y_plus`, `l_y_minus` and the `r_` equivalents, and in
/// `controls.rs` we write `input_player1_<id>_axis` from the id of a control.
/// So we can express the sticks, and only their declaration is missing.
#[test]
fn the_analogue_profile_still_declares_only_the_stick_clicks() {
    // We cannot reach the profile through the console, so we read its
    // declaration from the generated registry, which the player reads too.
    let registry: serde_json::Value =
        serde_json::from_str(include_str!("../../controls.json")).expect("controls registry");
    let profile = registry["profiles"]
        .as_array()
        .expect("profiles")
        .iter()
        .find(|p| p["id"] == "ps1-analog")
        .expect("the profile still exists");
    let ids: Vec<&str> = profile["controls"]
        .as_array()
        .expect("controls")
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    for click in ["l3", "r3"] {
        assert!(
            ids.contains(&click),
            "the stick clicks are declared: {ids:?}"
        );
    }
    for stick in ["l_x_plus", "l_x_minus", "r_x_plus", "r_x_minus"] {
        assert!(
            !ids.contains(&stick),
            "{stick} is now declared — offer the pad again and delete this test"
        );
    }
}

#[test]
fn a_disc_format_we_cannot_collect_is_refused_with_a_reason() {
    let root = scratch();
    let playlist = root.join("game.m3u");
    fs::write(&playlist, b"disc1.cue\n").unwrap();
    let refusal = content::collect(&playlist).expect_err("an m3u points at other files");
    assert!(
        refusal.contains("recognised but not exported"),
        "the gap should be explained, not merely refused: {refusal}"
    );

    // We support a cue sheet, so that rule must not reject it.
    let cue = root.join("game.cue");
    let track = root.join("game.bin");
    fs::write(&track, b"data").unwrap();
    fs::write(&cue, b"FILE \"game.bin\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();
    content::collect(&cue).expect("a cue sheet and its tracks can be collected");
}

#[test]
fn an_exported_playstation_game_keeps_its_memory_cards_to_itself() {
    // Memory cards go into the save directory in PCSX ReARMed, so when we
    // isolate saves we isolate memory cards. With a path outside the per-game
    // data root, one exported game could read or overwrite the cards of
    // another. We check the config keys in the isolation unit test in
    // packaging.rs. Here we check that the cards go into a directory that we
    // create in the launcher under the per-game data root.
    for directory in ["saves", "states", "system"] {
        assert!(
            packaging::MANAGED_DATA_DIRECTORIES.contains(&directory),
            "{directory} must be created under the per-game data root"
        );
    }
    assert!(
        !packaging::MANAGED_DATA_DIRECTORIES
            .iter()
            .any(|entry| entry.starts_with('/') || entry.contains("..")),
        "no managed directory may escape the per-game data root"
    );
}

/// LibCrypt protection data is the case where this is needed.
///
/// For example, Ape Escape (Europe) is protected, and its subchannel data is
/// in an .sbi file beside the image that nothing in the image refers to. If
/// we export the image alone, the game starts and then fails where it checks
/// the protection. So leaving the file out is worse than refusing to export.
#[test]
fn a_libcrypt_subchannel_file_travels_with_the_disc_image() {
    let root = scratch();
    let image = root.join("Ape Escape (Europe).chd");
    let subchannel = root.join("Ape Escape (Europe).sbi");
    fs::write(&image, b"not a real disc image").unwrap();
    fs::write(&subchannel, b"not real subchannel data").unwrap();

    let collected = content::collect(&image).expect("a chd is a supported form");
    let names: Vec<String> = collected
        .files
        .iter()
        .map(|file| file.relative.to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().any(|name| name == "Ape Escape (Europe).sbi"),
        "the subchannel file must be exported alongside the image: {names:?}"
    );
    assert!(names.iter().any(|name| name == "Ape Escape (Europe).chd"));
}

#[test]
fn an_image_with_no_subchannel_file_is_unaffected() {
    let root = scratch();
    let image = root.join("Unprotected Game.chd");
    fs::write(&image, b"not a real disc image").unwrap();

    let collected = content::collect(&image).expect("a chd is a supported form");
    assert_eq!(
        collected.files.len(),
        1,
        "nothing should be invented when there is no sibling to collect"
    );
}

/// The PlayStation pad we ship is a complete DualShock.
///
/// We declare the device and all of its sticks, so the core gets stick values
/// from the DualShock that we report as connected. The eight direction ids
/// are spelled exactly as in RetroArch `configuration.c:333-340`, because
/// `input_player1_<id>_axis` comes from the id, and a rename would unbind the
/// stick with no error.
#[test]
fn the_playstation_pad_declares_both_sticks_and_the_dualshock_device() {
    let pad = controls::profile_for_system("ps1").expect("a default pad");
    assert_eq!(
        pad.core_device,
        Some(517),
        "517 is what this core's own controller table calls \"dualshock\"; \
         261 is \"analog\", a different peripheral"
    );
    let ids: Vec<&str> = pad.controls.iter().map(|c| c.id.as_str()).collect();
    for direction in [
        "l_x_plus",
        "l_x_minus",
        "l_y_plus",
        "l_y_minus",
        "r_x_plus",
        "r_x_minus",
        "r_y_plus",
        "r_y_minus",
    ] {
        assert!(
            ids.contains(&direction),
            "{direction} must be bindable: {ids:?}"
        );
    }

    // Each stick is one object on the illustration, so its directions and its
    // click are in one group. Otherwise the scene would need twenty-four
    // callouts, and both gutters are already full with seven 54 dp callouts.
    for (click, group) in [("l3", "l_stick"), ("r3", "r_stick")] {
        let members: Vec<&str> = pad
            .controls
            .iter()
            .filter(|c| c.group.as_deref() == Some(group))
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(
            members.len(),
            5,
            "{group} is four directions plus its click: {members:?}"
        );
        assert!(members.contains(&click), "{click} belongs to {group}");
    }
}

/// The existing callouts keep their positions.
///
/// The sticks have no callout position, so adding them leaves every button at
/// its anchor. If this count changes, someone has laid out the gutters again,
/// and we must check the artwork of every console again.
#[test]
fn adding_sticks_did_not_disturb_the_button_callouts() {
    let pad = controls::profile_for_system("ps1").expect("a default pad");
    let with_callouts = pad
        .controls
        .iter()
        .filter(|c| (c.callout_x, c.callout_y) != (0, 0))
        .count();
    assert_eq!(
        with_callouts, 14,
        "the fourteen button callouts are unchanged; sticks are drawn in-scene"
    );
}
