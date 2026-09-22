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

fn scratch() -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir("rominabox-ps1")
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
    let root = scratch();
    let empty = root.join("empty-kit");
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

/// We offer both PlayStation pads, and both are complete.
///
/// `ps1-analog` declares the same twenty-four controls as the DualShock,
/// because the Dual Analog is the same physical pad without vibration. A pad
/// with only the digital buttons and L3 and R3, which are the stick clicks,
/// would look analogue and work digitally. The two differ only in the
/// emulated device, 261 for the analogue pad and 517 for the DualShock, and a
/// game such as Ape Escape does not start without 517.
#[test]
fn both_playstation_pads_are_offered_and_both_declare_their_sticks() {
    let pad = controls::profile_for_system("ps1").expect("a default pad");
    assert_eq!(pad.id, "ps1", "the DualShock stays the default");

    let offered = controls::variants_for_system("ps1").expect("PlayStation variants");
    let ids: Vec<&str> = offered.iter().map(|p| p.id.as_str()).collect();
    assert!(
        ids.contains(&"ps1") && ids.contains(&"ps1-analog"),
        "PlayStation offers both pads: {ids:?}"
    );

    // Check both axes of both sticks. With only some of the eight directions,
    // a pad without any vertical axis would pass.
    let dualshock = offered
        .iter()
        .find(|p| p.id == "ps1")
        .expect("the DualShock is offered");
    let reference: Vec<&str> = dualshock.controls.iter().map(|c| c.id.as_str()).collect();
    for profile in &offered {
        let declared: Vec<&str> = profile.controls.iter().map(|c| c.id.as_str()).collect();
        for stick in [
            "l_x_plus", "l_x_minus", "l_y_plus", "l_y_minus",
            "r_x_plus", "r_x_minus", "r_y_plus", "r_y_minus",
            "l3", "r3",
        ] {
            assert!(
                declared.contains(&stick),
                "{} declares {stick}; a pad offered as analogue must have both \
                 axes of both sticks, not only the stick clicks",
                profile.id
            );
        }
        // The Dual Analog is the same physical pad as the DualShock without
        // vibration, so it must have every binding of the DualShock.
        for id in &reference {
            assert!(
                declared.contains(id),
                "{} is missing {id}, which the DualShock declares",
                profile.id
            );
        }
    }

    // The two pads differ only in the emulated device, and nothing on screen
    // shows it, because they have the same drawing, controls and overlay
    // digest. A game such as Ape Escape does not start on 261.
    assert_eq!(
        dualshock.core_device,
        Some(517),
        "the DualShock is device 517"
    );
    let analogue = offered
        .iter()
        .find(|p| p.id == "ps1-analog")
        .expect("the analogue pad is offered");
    assert_eq!(
        analogue.core_device,
        Some(261),
        "the analogue pad is device 261; offering two pads that reach the core \
         as the same device is offering one pad under two names"
    );

    let pick = |id: &str| -> Controls {
        let mut options = Controls::default();
        options.profile = Some(id.to_string());
        options
    };
    controls::validate_for_system("ps1", &pick("ps1-analog"))
        .expect("the analogue pad is selectable now that it is a whole pad");
    assert!(
        controls::validate_for_system("nes", &pick("ps1-analog")).is_err(),
        "a PlayStation pad must not be selectable for another console"
    );
}


#[test]
fn a_playlist_collects_each_disc_and_names_one_that_is_missing() {
    let root = scratch();
    let playlist = root.join("game.m3u");
    fs::write(&playlist, b"disc1.cue\n").unwrap();
    let refusal = content::collect(&playlist).expect_err("the cue is not beside the playlist");
    assert!(
        refusal.contains("disc1.cue"),
        "the missing disc should be named: {refusal}"
    );

    let cue = root.join("game.cue");
    let track = root.join("game.bin");
    fs::write(&track, b"data").unwrap();
    fs::write(&cue, b"FILE \"game.bin\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();
    fs::write(&playlist, b"game.cue\n").unwrap();
    let collected = content::collect(&playlist).expect("a playlist and its cue can be collected");
    let names: Vec<String> = collected
        .files
        .iter()
        .map(|file| file.relative.to_string_lossy().into_owned())
        .collect();
    for name in ["game.m3u", "game.cue", "game.bin"] {
        assert!(names.iter().any(|found| found == name), "{names:?}");
    }
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

/// We draw each stick once, not eight times at the origin.
///
/// We declare the eight analogue directions with a group and no callout
/// position, and group them in `themes.rs` as in the offscreen renderer. With
/// a hit marker and a callout for every control, eight of them would pile up
/// in the top-left corner of the scene in an export, and we never open the
/// Controls panel in the interaction baseline, so nothing would show it.
#[test]
fn the_playstation_scene_draws_each_stick_once() {
    let root = scratch();
    let assets = rominabox_desktop::repo::at("integrations/designs/native");
    // A staged asset directory in the layout used in prepare_controls_assets:
    // the menu template and the artwork that we copy by name. The image is an
    // empty stand-in, because we test the markup, not the picture.
    let source = root.join("staged");
    let destination = root.join("menu-assets");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::copy(assets.join("menu.rml"), source.join("menu.rml")).unwrap();
    let profile = controls::profile_for_system("ps1").expect("a default pad");
    fs::write(source.join(&profile.image), []).unwrap();
    fs::write(source.join("CONTROLLERS.txt"), []).unwrap();

    let options = Controls::default();
    crate_themes_prepare(&source, &destination, "ps1", &options);

    let markup = fs::read_to_string(destination.join("menu.rml")).expect("scene markup");
    let stick_callouts = markup.matches("control-callout").count();
    let stick_groups = markup.matches("class=\"control-group\"").count();

    assert_eq!(
        stick_groups, 2,
        "each stick is one element: left and right, drawn beneath the pad"
    );
    // Fourteen buttons, and not one callout per analogue direction.
    assert_eq!(
        stick_callouts, 14,
        "only the buttons take gutter callouts; {stick_groups} groups are drawn separately"
    );
    for direction in ["l_x_plus", "r_y_minus"] {
        assert!(
            !markup.contains(&format!("control-hit-{direction}")),
            "{direction} must not get its own marker; its stick carries one"
        );
    }
    for click in ["l3", "r3"] {
        assert!(
            markup.contains(&format!("control-hit-{click}")),
            "the stick's own anchor is the marker that is drawn"
        );
    }
}

fn crate_themes_prepare(
    source: &Path,
    destination: &Path,
    system: &str,
    options: &Controls,
) {
    rominabox_desktop::themes::prepare_controls_assets(source, source, destination, system, options, None)
        .expect("the scene markup is generated");
}
