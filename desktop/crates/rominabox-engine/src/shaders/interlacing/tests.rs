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

/// Flycast sends every Dreamcast picture as a whole frame, 640 by 480. CRT
/// Royale takes a picture of that height as two interlaced fields and shows
/// one field a frame, with the other field's lines made from their
/// neighbours. So in a Dreamcast game we switch that off.
#[test]
fn crt_royale_takes_a_dreamcast_picture_as_a_whole_frame() {
    let (root, preset) = staged("dreamcast", &["crt-royale", SLANG], "crt-royale");
    assert_eq!(value_in(&root.join(&preset), "interlace_detect_toggle").as_deref().map(str::parse::<f32>), Some(Ok(0.0)));
}

/// Genesis Plus GX sends a Mega Drive game's interlaced mode as two fields
/// woven into one picture, so CRT Royale keeps showing one field a frame.
#[test]
fn crt_royale_still_takes_a_mega_drive_picture_as_interlaced() {
    let (root, preset) = staged("megadrive", &["crt-royale", SLANG], "crt-royale");
    assert_eq!(value_in(&root.join(&preset), "interlace_detect_toggle"), None);
}

/// CRT Geom's switch has a parameter in both languages, so a Dreamcast game
/// in GLSL switches it off too.
#[test]
fn crt_geom_takes_a_dreamcast_picture_as_a_whole_frame_in_glsl() {
    let (root, preset) = staged("dreamcast", &["crt-geom"], "crt-geom");
    assert!(preset.ends_with(".glslp"), "{preset}");
    assert_eq!(value_in(&root.join(&preset), "interlace_detect").as_deref().map(str::parse::<f32>), Some(Ok(0.0)));
}

/// CRT Guest takes a picture over 375 lines high as interlaced, unless its
/// interlace mode is off.
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
