use super::super::*;
use std::fs;

/// The shader library in the repository, which we put in the kit.
fn library() -> PathBuf {
    crate::repo::at("integrations/shaders/library")
}

/// The game folder of a Windows game for `system` with these catalog presets,
/// and the preset of the row `id` in its `shaders.cfg`, from the folder.
fn staged(system: &str, bundled: &[&str], id: &str) -> (rominabox_scratch::Scratch, String) {
    let composed = crate::menu::compose_menu(&crate::menu::MenuRequest {
        shaders: ShaderSelection {
            bundled: bundled.iter().map(|id| id.to_string()).collect(),
            ..Default::default()
        },
        shader_library: library(),
        target: ExportTarget::Windows,
        system: system.into(),
        menu_entries: Some(vec!["controls".into(), "video".into(), "shaders".into()]),
        ..crate::menu::MenuRequest::new(
            crate::repo::at("integrations/designs/native"),
            crate::repo::at("desktop/assets/controllers"),
        )
    })
    .unwrap();
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-interlacing");
    composed.write(&root).unwrap();
    let key = format!("shader_preset_{id} = ");
    let config = composed.text("shaders.cfg").unwrap();
    let preset = config
        .lines()
        .find_map(|line| line.strip_prefix(&key))
        .unwrap_or_else(|| panic!("no row for {id} in {config}"))
        .trim_matches('"')
        .to_string();
    (root, preset)
}

/// The value that `preset` gives `parameter`, as RetroArch reads it: its own
/// line, or else the value in the presets it references, the first first.
fn value_in(preset: &Path, parameter: &str) -> Option<String> {
    let text = fs::read_to_string(preset).unwrap_or_else(|error| panic!("{}: {error}", preset.display()));
    let own = text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == parameter).then(|| value.trim().trim_matches('"').to_string())
    });
    own.or_else(|| {
        text.lines()
            .filter_map(|line| line.trim().strip_prefix("#reference"))
            .find_map(|named| value_in(&preset.parent().unwrap().join(named.trim().trim_matches('"')), parameter))
    })
}

/// A game in slang, as a game with a preset only in slang is.
const SLANG: &str = "gba-lcd";

/// Every Dreamcast picture from Flycast is a whole frame, 640 by 480. In CRT
/// Royale a picture of that height counts as two interlaced fields, and each
/// frame shows one field, with the other field's lines made from their
/// neighbours. So in a Dreamcast game we switch that off.
#[test]
fn crt_royale_takes_a_dreamcast_picture_as_a_whole_frame() {
    let (root, preset) = staged("dreamcast", &["crt-royale", SLANG], "crt-royale");
    assert_eq!(value_in(&root.join(&preset), "interlace_detect_toggle").as_deref().map(str::parse::<f32>), Some(Ok(0.0)));
}

/// In a Mega Drive game's interlaced mode, each picture from Genesis Plus GX
/// is two fields woven together, so CRT Royale keeps one field a frame.
#[test]
fn crt_royale_still_takes_a_mega_drive_picture_as_interlaced() {
    let (root, preset) = staged("megadrive", &["crt-royale", SLANG], "crt-royale");
    assert_eq!(value_in(&root.join(&preset), "interlace_detect_toggle"), None);
}

/// CRT Geom's switch is a parameter in both languages, so we switch it off
/// in a Dreamcast game in GLSL too.
#[test]
fn crt_geom_takes_a_dreamcast_picture_as_a_whole_frame_in_glsl() {
    let (root, preset) = staged("dreamcast", &["crt-geom"], "crt-geom");
    assert!(preset.ends_with(".glslp"), "{preset}");
    assert_eq!(value_in(&root.join(&preset), "interlace_detect").as_deref().map(str::parse::<f32>), Some(Ok(0.0)));
}

/// In CRT Guest a picture over 375 lines high counts as interlaced, unless
/// its interlace mode is off.
#[test]
fn crt_guest_takes_a_dreamcast_picture_as_a_whole_frame() {
    let (root, preset) = staged("dreamcast", &["crt-guest-advanced"], "crt-guest-advanced");
    assert_eq!(value_in(&root.join(&preset), "interm").as_deref().map(str::parse::<f32>), Some(Ok(0.0)));
}

/// The preset of the row is the one we stage whole: every file the preset
/// and the presets it references name is in the game's folder.
#[test]
fn the_row_of_a_dreamcast_game_names_a_preset_whose_files_are_all_there() {
    let (root, preset) = staged("dreamcast", &["crt-royale", SLANG], "crt-royale");
    let walked = resolve(&ShaderSelection {
        custom: vec![CustomShader { name: Some("Royale".into()), path: root.join(&preset) }],
        ..Default::default()
    })
    .unwrap();
    let names = |shader: &ResolvedShader| shader.files.len();
    let library_preset = resolve(&ShaderSelection {
        custom: vec![CustomShader {
            name: Some("Royale".into()),
            path: library().join("slang/crt/crt-royale.slangp"),
        }],
        ..Default::default()
    })
    .unwrap();
    assert!(names(&walked[1]) > names(&library_preset[1]), "the row's preset reaches every file of CRT Royale");
}

/// Each language's files of a preset declare the parameter that the catalog
/// names as its switch, so that the value we give it reaches the shader.
#[test]
fn each_switch_in_the_catalog_is_a_parameter_of_its_preset() {
    for preset in catalog_file().unwrap().presets {
        let (Some(interlacing), Made::Files(files)) = (&preset.interlacing, &preset.made) else {
            continue;
        };
        for language in Language::ALL {
            let (Some(switch), Some(path)) = (interlacing.switch(language), files.in_language(language)) else {
                continue;
            };
            let folder = library_folder(language);
            let listed = library_files(&library().join(folder), path).unwrap();
            assert!(
                super::super::brightness::declares(&listed, switch.parameter()),
                "{folder}/{path} declares no {}",
                switch.parameter()
            );
        }
    }
}

/// What a Dreamcast game's row of CRT Royale names: our preset, which
/// references the library preset and switches the guess off.
#[test]
fn the_preset_of_a_dreamcast_game_references_the_library_preset() {
    let (root, preset) = staged("dreamcast", &["crt-royale", SLANG], "crt-royale");
    assert_eq!(preset, "shaders/crt-royale/whole-frames.slangp");
    assert_eq!(
        fs::read_to_string(root.join(&preset)).unwrap(),
        "#reference \"../slang/crt/crt-royale.slangp\"\ninterlace_detect_toggle = \"0.000000\"\n"
    );
}
