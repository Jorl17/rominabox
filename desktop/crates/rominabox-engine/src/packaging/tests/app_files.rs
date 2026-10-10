use super::*;
use crate::packaging::app_files::{
    stage_controller_remap, stage_legal_materials, stage_pixel_options,
};
use crate::packaging::legal;

/// A unique empty directory, matching the pattern the other tests use.
fn scratch_dir() -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir("rominabox-remap")
}

/// We write the emulated controller to the remap file.
///
/// In RetroArch, the device has an effect only in a remap file, not in the
/// controls config, so with the device there, every console that declares a
/// `coreDevice` would stay on the default pad of the core.
#[test]
fn the_emulated_device_is_written_as_a_remap_not_a_config_line() {
    let root = scratch_dir();
    let profile = controls::ControlProfile {
        id: "ps1".into(),
        name: "PlayStation".into(),
        systems: vec!["ps1".into()],
        image: String::new(),
        core_device: Some(517),
        controls: Vec::new(),
        groups: Default::default(),
    };
    let core = crate::systems::Core {
        frames: crate::systems::Frames::default(),
        artifacts: Default::default(),
        component: "pcsx_rearmed".into(),
        license: "GPL-2.0".into(),
        license_file: "pcsx_rearmed.txt".into(),
        capabilities: Vec::new(),
        library_name: Some("PCSX-ReARMed".into()),
        pixels: Vec::new(),
    };
    let remaps = root.join("remaps");
    stage_controller_remap(&profile, &[], &[], &[], false, &core, &remaps).expect("a remap is written");

    // The directory name is the library name of the core, not its component
    // id, because in config_load_remap the path comes from the artifact.
    let written = remaps.join("PCSX-ReARMed/PCSX-ReARMed.rmp");
    let text = fs::read_to_string(&written).expect("remap exists at the path RetroArch reads");
    assert!(
        text.contains("input_libretro_device_p1 = \"517\""),
        "the remap must name the declared device: {text}"
    );
}

/// A pad that is the core's default device, with each pad a separate
/// player, gets no remap at all.
#[test]
fn a_profile_with_no_declared_device_writes_nothing() {
    let root = scratch_dir();
    let profile = controls::ControlProfile {
        id: "nes".into(),
        name: "NES".into(),
        systems: vec!["nes".into()],
        image: String::new(),
        core_device: None,
        controls: Vec::new(),
        groups: Default::default(),
    };
    let core = crate::systems::Core {
        frames: crate::systems::Frames::default(),
        artifacts: Default::default(),
        component: "nestopia".into(),
        license: "GPL-2.0".into(),
        license_file: "nestopia.txt".into(),
        capabilities: Vec::new(),
        library_name: None,
        pixels: Vec::new(),
    };
    let remaps = root.join("remaps");
    stage_controller_remap(&profile, &[], &[], &[], false, &core, &remaps)
        .expect("nothing to do is not an error");
    assert!(!remaps.exists(), "no remap directory should be created");
}

/// A control the author moved on the pad is a remap even on a pad that is
/// the core's default device.
#[test]
fn a_moved_control_is_written_into_the_remap() {
    let root = scratch_dir();
    let profile = controls::profile_for_system("megadrive").unwrap();
    assert_eq!(
        profile.core_device.map(|_| ()),
        Some(()),
        "the Mega Drive pad names its device"
    );
    let default_device = controls::ControlProfile {
        core_device: None,
        ..profile
    };
    let core = crate::systems::Core {
        frames: crate::systems::Frames::default(),
        artifacts: Default::default(),
        component: "genesis_plus_gx".into(),
        license: String::new(),
        license_file: String::new(),
        capabilities: Vec::new(),
        library_name: Some("Genesis Plus GX".into()),
        pixels: Vec::new(),
    };
    let remaps = root.join("remaps");
    stage_controller_remap(&default_device, &swapped_c_and_b(), &[], &[], false, &core, &remaps)
        .expect("a remap is written");
    let text = fs::read_to_string(remaps.join("Genesis Plus GX/Genesis Plus GX.rmp")).unwrap();
    assert_eq!(text, "input_player1_btn_a = \"0\"\ninput_player1_btn_b = \"8\"\n");
}

/// The Mega Drive's C moved onto the bottom button, and B onto C's.
fn swapped_c_and_b() -> Vec<crate::pad_positions::Placed> {
    let chosen: controls::Controls = serde_json::from_value(serde_json::json!({
        "bindings": { "a": { "pad": "b" }, "b": { "pad": "a" } }
    }))
    .unwrap();
    controls::placement("megadrive", &chosen).unwrap()
}

/// Every pad as player 1 is a port map in RetroArch, which has an effect only
/// in a remap. So we write a remap even with nothing moved on the default pad,
/// with pads 2 to 8 as player 1, and we move a control on every pad, because
/// in RetroArch each pad has separate lines for its controls.
#[test]
fn every_pad_is_player_one_in_the_remap_and_moves_its_controls_on_each() {
    let root = scratch_dir();
    let profile = controls::ControlProfile {
        core_device: None,
        ..controls::profile_for_system("megadrive").unwrap()
    };
    let core = crate::systems::Core {
        frames: crate::systems::Frames::default(),
        artifacts: Default::default(),
        component: "genesis_plus_gx".into(),
        license: String::new(),
        license_file: String::new(),
        capabilities: Vec::new(),
        library_name: Some("Genesis Plus GX".into()),
        pixels: Vec::new(),
    };
    let ports: String = (2..=8)
        .map(|pad| format!("input_remap_port_p{pad} = \"0\"\n"))
        .collect();
    let remaps = root.join("nothing-moved");
    stage_controller_remap(&profile, &[], &[], &[], true, &core, &remaps).expect("a remap is written");
    let text = fs::read_to_string(remaps.join("Genesis Plus GX/Genesis Plus GX.rmp")).unwrap();
    assert_eq!(text, ports);

    let remaps = root.join("moved");
    stage_controller_remap(&profile, &swapped_c_and_b(), &[], &[], true, &core, &remaps)
        .expect("a remap is written");
    let text = fs::read_to_string(remaps.join("Genesis Plus GX/Genesis Plus GX.rmp")).unwrap();
    let moved: String = (1..=8)
        .map(|pad| format!("input_player{pad}_btn_a = \"0\"\ninput_player{pad}_btn_b = \"8\"\n"))
        .collect();
    assert_eq!(text, ports + &moved);
}

/// We ship a remap while every pad is player 1, which is the default unless
/// the author turns it off. A remap has an effect in RetroArch only in the
/// folder named after the library name of the core, so we declare for every
/// core the name in its artifact, or nobody could export its console's games.
#[test]
fn every_core_names_the_folder_its_remap_goes_in() {
    let nameless: Vec<String> = crate::systems::registry()
        .iter()
        .flat_map(|system| system.cores.iter().map(move |core| (system, core)))
        .filter(|(_, core)| core.library_name.is_none())
        .map(|(system, core)| format!("{} ({})", core.component, system.id))
        .collect();
    assert!(nameless.is_empty(), "no libraryName: {nameless:?}");
}

/// We write picture options to the per-core options file of RetroArch.
///
/// The directory is the library name of the core, as for the remap. For a
/// core with nothing declared we write no file, because its defaults already
/// leave the pixels unchanged.
#[test]
fn picture_options_are_written_where_retroarch_reads_them() {
    let root = scratch_dir();
    let core = crate::systems::Core {
        frames: crate::systems::Frames::default(),
        artifacts: Default::default(),
        component: "nestopia".into(),
        license: "GPL-2.0".into(),
        license_file: "nestopia.txt".into(),
        capabilities: Vec::new(),
        library_name: Some("Nestopia".into()),
        pixels: vec![crate::systems::PixelOption {
            key: "nestopia_blargg_ntsc_filter".into(),
            value: "disabled".into(),
        }],
    };
    let destination = root.join("core-options");
    stage_pixel_options(&core, &destination).expect("options are written");
    let text = fs::read_to_string(destination.join("Nestopia/Nestopia.opt"))
        .expect("options exist at the path RetroArch reads");
    assert_eq!(text, "nestopia_blargg_ntsc_filter = \"disabled\"\n");

    let untouched = crate::systems::Core {
        frames: crate::systems::Frames::default(),
        pixels: Vec::new(),
        library_name: None,
        ..core
    };
    let empty = root.join("empty");
    stage_pixel_options(&untouched, &empty).expect("nothing to write is not an error");
    assert!(!empty.exists(), "no options directory should be created");

    let nameless = crate::systems::Core {
        frames: crate::systems::Frames::default(),
        library_name: None,
        pixels: vec![crate::systems::PixelOption {
            key: "nestopia_blargg_ntsc_filter".into(),
            value: "disabled".into(),
        }],
        ..untouched
    };
    let error = stage_pixel_options(&nameless, &root.join("missing"))
        .expect_err("options with no library name have nowhere to go");
    assert!(error.message.contains("libraryName"), "{}", error.message);

    let launcher = write_test_launcher(request(false));
    assert!(
        launcher.contains("managed\tconfig\n"),
        "the launcher has to be told about the directory core options are copied into"
    );
}

/// When we need a device but have nowhere to write it, we fail with an error.
#[test]
fn a_declared_device_with_no_library_name_is_refused() {
    let root = scratch_dir();
    let profile = controls::ControlProfile {
        id: "megadrive6".into(),
        name: "Mega Drive six-button".into(),
        systems: vec!["megadrive".into()],
        image: String::new(),
        core_device: Some(513),
        controls: Vec::new(),
        groups: Default::default(),
    };
    let core = crate::systems::Core {
        frames: crate::systems::Frames::default(),
        artifacts: Default::default(),
        component: "genesis_plus_gx".into(),
        license: "MAME".into(),
        license_file: "genesis_plus_gx.txt".into(),
        capabilities: Vec::new(),
        library_name: None,
        pixels: Vec::new(),
    };
    let error = stage_controller_remap(&profile, &[], &[], &[], false, &core, &root.join("remaps"))
        .expect_err("silently shipping the wrong pad is the defect being prevented");
    let message = error.to_string();
    assert!(
        message.contains("libraryName"),
        "the refusal must say what is missing: {message}"
    );
}

/// A game contains the licence of every component in the kit's index for it:
/// the player's libraries, the controller data and the fonts of its design,
/// then its core. We list them in the game's index for the About views, and
/// show them in the About panel of a Mac game.
#[test]
fn a_game_carries_every_licence_the_kit_holds_for_its_player() {
    let root = scratch_dir();
    let kit = root.join("kit");
    for folder in ["native", "data", "fonts"] {
        fs::create_dir_all(kit.join("licenses").join(folder)).unwrap();
    }
    fs::create_dir_all(kit.join("provenance/native-rmlui")).unwrap();
    fs::write(kit.join("licenses/NATIVE-DEPENDENCIES.txt"), "list").unwrap();
    let shipped = ["native/retroarch.txt", "native/rmlui.txt", "data/retroarch-joypad-autoconfig.txt", "fonts/silkscreen.txt"];
    for name in shipped.iter().chain(&["fonts/science-gothic.txt"]) {
        fs::write(kit.join("licenses").join(name), name).unwrap();
    }
    let row = |group: legal::Group, title: &str, file: &str| legal::Row {
        group,
        title: title.into(),
        version: "1".into(),
        licence: "MIT & <Zlib>".into(),
        file: file.into(),
        copyright: String::new(),
    };
    let kit_rows = vec![
        row(legal::Group::Native, "RetroArch", "native/retroarch.txt"),
        row(legal::Group::Native, "RmlUi", "native/rmlui.txt"),
        row(legal::Group::Native, "glslang", "native/glslang.txt"),
        row(legal::Group::Data, "Joypad profiles", "data/retroarch-joypad-autoconfig.txt"),
        row(legal::Group::Fonts, "Silkscreen", "fonts/silkscreen.txt"),
        row(legal::Group::Fonts, "Science Gothic", "fonts/science-gothic.txt"),
    ];
    fs::write(kit.join("licenses/index.json"), serde_json::to_vec(&kit_rows).unwrap()).unwrap();
    fs::write(kit.join("licenses/genesis_plus_gx.txt"), "core").unwrap();
    fs::write(
        kit.join("manifest.json"),
        r#"{"components":[{"name":"RetroArch"},{"name":"Mbed TLS"}]}"#,
    )
    .unwrap();
    let core = crate::systems::Core {
        frames: crate::systems::Frames::default(),
        artifacts: Default::default(),
        component: "genesis_plus_gx".into(),
        license: "GPL-3.0".into(),
        license_file: "genesis_plus_gx.txt".into(),
        capabilities: Vec::new(),
        library_name: Some("Genesis Plus GX".into()),
        pixels: Vec::new(),
    };
    let legal = root.join("Legal");
    let rows = legal::game_rows(&kit, &core, &["Silkscreen".into()]).unwrap();
    stage_legal_materials(
        &kit,
        None,
        &legal,
        &core,
        Path::new("licenses/genesis_plus_gx.txt"),
        &rows,
    )
    .expect("the legal materials are staged");
    let licenses = legal.join("Licenses");
    for name in shipped {
        assert_eq!(fs::read_to_string(licenses.join(name)).unwrap(), name);
    }
    assert!(!licenses.join("fonts/science-gothic.txt").exists(), "a font the design does not use was shipped");
    assert_eq!(fs::read_to_string(licenses.join("genesis_plus_gx.txt")).unwrap(), "core");

    let index: Vec<legal::Row> = serde_json::from_slice(&fs::read(licenses.join("index.json")).unwrap()).unwrap();
    let listed: Vec<(&str, &str)> = index.iter().map(|row| (row.title.as_str(), row.file.as_str())).collect();
    assert_eq!(
        listed,
        [
            ("Genesis Plus GX", "genesis_plus_gx.txt"),
            ("RetroArch", "native/retroarch.txt"),
            ("RmlUi", "native/rmlui.txt"),
            // The kit has the row but no text, so we keep the row with no file.
            ("glslang", ""),
            ("Joypad profiles", "data/retroarch-joypad-autoconfig.txt"),
            ("Silkscreen", "fonts/silkscreen.txt"),
        ]
    );
    let readme = fs::read_to_string(licenses.join("README.txt")).unwrap();
    assert!(readme.contains("index.json") && readme.contains(crate::WEBSITE), "{readme}");

    let credits = legal::credits_html(&licenses).unwrap();
    assert!(credits.contains("<td>Genesis Plus GX</td><td>GPL-3.0</td>"), "{credits}");
    assert!(credits.contains("MIT &amp; &lt;Zlib&gt;"), "a licence name is escaped: {credits}");
    assert!(credits.contains(&format!("href=\"{}\"", crate::WEBSITE)), "{credits}");
    assert!(credits.contains(">native/rmlui.txt</pre>"), "the texts follow the table: {credits}");
}

/// We refuse an old kit without an index, and tell the author what to do.
#[test]
fn a_kit_with_no_licence_index_is_refused() {
    let root = scratch_dir();
    let core = crate::systems::Core {
        frames: crate::systems::Frames::default(),
        artifacts: Default::default(),
        component: "genesis_plus_gx".into(),
        license: "GPL-3.0".into(),
        license_file: "genesis_plus_gx.txt".into(),
        capabilities: Vec::new(),
        library_name: None,
        pixels: Vec::new(),
    };
    let message = legal::game_rows(&root, &core, &[]).unwrap_err().to_string();
    assert!(message.contains("index.json") && message.contains("build_kit.py"), "{message}");
}
