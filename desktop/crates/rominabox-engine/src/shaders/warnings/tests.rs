use super::super::tests::{custom_preset, on, PASS, SLANG};
use super::*;
use crate::shaders::CustomShader;
use std::fs;

/// A GLSL pass in version 130, as in libretro's crt-royale.
const NEW_PASS: &str = "#version 130\n#if defined(VERTEX)\n#elif defined(FRAGMENT)\n#endif\n";

/// A selection with one shader of the author's per (name, file, text).
fn added(folder: &Path, shaders: &[(&str, &str, &str)]) -> ShaderSelection {
    let mut selection = ShaderSelection::default();
    for (name, file, text) in shaders {
        let path = folder.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        selection.custom.push(CustomShader {
            name: Some(name.to_string()),
            path,
        });
    }
    selection
}

/// Each warning as one paragraph: what will happen, then why.
fn said(warnings: &[Warning]) -> Vec<String> {
    warnings.iter().map(|warning| format!("{} {}", warning.text, warning.detail)).collect()
}

/// The library in the repository, as in every kit.
fn library() -> PathBuf {
    on(ExportTarget::Macos).library
}

/// `deep_file`, as we write it in a warning, with the folders in the middle
/// left out.
const SHORT: &str = "a-folder-of-twenty-c/\u{2026}/a-folder-of-twenty-c/pass.glsl";

/// A file deep enough in its folders that its path in an unpacked Windows
/// game may be longer than Windows allows.
fn deep_file() -> String {
    format!("{}/pass.glsl", ["a-folder-of-twenty-c"; 8].join("/"))
}

const MAC_ONE: &str = "The shader \u{201c}Retro Glow\u{201d} won\u{2019}t load on a Mac. The game will run fine, \
     but without the filter. This is because Macs do not support this GLSL shader. If you have a slang \
     version of the shader, add that instead.";

/// An author's GLSL shader above version 120 has no other version, so we warn
/// that it will not load on a Mac, on the Menu step and on Create app for a
/// Mac game. We say nothing for a Windows game.
#[test]
fn a_glsl_shader_of_the_authors_too_new_for_a_mac_is_a_warning() {
    let root = rominabox_scratch::Scratch::dir("rominabox-warn-mac-one");
    let selection = added(&root, &[("Retro Glow", "glow.glsl", NEW_PASS)]);
    let step = shader_warnings(&selection, &library()).unwrap();
    assert_eq!(step.warned, [root.join("glow.glsl")]);
    assert_eq!(said(&step.warnings), [MAC_ONE]);
    // We show what will happen, and the reason in the tooltip of the help button.
    assert_eq!(
        step.warnings[0].text,
        "The shader \u{201c}Retro Glow\u{201d} won\u{2019}t load on a Mac. The game will run fine, but without the filter."
    );
    let mac = notice(&selection, &[ExportTarget::Macos], &library()).unwrap();
    assert_eq!(mac.len(), 1);
    assert_eq!(mac[0].heading, None);
    assert_eq!(said(&mac[0].warnings), [MAC_ONE]);
    assert_eq!(notice(&selection, &[ExportTarget::Windows], &library()).unwrap(), []);
    let _ = fs::remove_dir_all(&root);
}

/// With several shaders, we name them all in one sentence in the plural, on
/// Create app and on the Menu step, where we mark each of them.
#[test]
fn several_shaders_too_new_for_a_mac_are_one_sentence() {
    let root = rominabox_scratch::Scratch::dir("rominabox-warn-mac-many");
    let selection = added(
        &root,
        &[
            ("Retro Glow", "glow.glsl", NEW_PASS),
            ("Pixel Frame", "frame.glsl", NEW_PASS),
            ("Soft Edge", "edge.glsl", NEW_PASS),
        ],
    );
    let many = "The shaders \u{201c}Retro Glow\u{201d}, \u{201c}Pixel Frame\u{201d} and \u{201c}Soft Edge\u{201d} \
         won\u{2019}t load on a Mac. The game will run fine, but without these filters. This is because \
         Macs do not support these GLSL shaders. If you have slang versions of the shaders, add those instead.";
    assert_eq!(said(&notice(&selection, &[ExportTarget::Macos], &library()).unwrap()[0].warnings), [many]);
    let step = shader_warnings(&selection, &library()).unwrap();
    assert_eq!(said(&step.warnings), [many]);
    assert_eq!(step.warned, ["glow.glsl", "frame.glsl", "edge.glsl"].map(|file| root.join(file)));
    let _ = fs::remove_dir_all(&root);
}

/// When the author adds a GLSL shader, we make the game in GLSL, so on a Mac
/// the presets whose GLSL does not compile there will not load. We name those presets,
/// and the author's shaders as the reason.
#[test]
fn presets_kept_in_glsl_by_the_authors_shaders_are_a_warning_on_a_mac() {
    let root = rominabox_scratch::Scratch::dir("rominabox-warn-mac-kept");
    let mut selection = added(&root, &[("Retro Glow", "glow.glsl", PASS)]);
    selection.bundled = vec!["crt-royale".into(), "crt-lottes".into(), "ntsc-adaptive".into()];
    let kept = "The shaders \u{201c}CRT Royale\u{201d} and \u{201c}NTSC\u{201d} won\u{2019}t load on a Mac. \
         The game will run fine, but without these filters. This is because \u{201c}Retro Glow\u{201d} is a \
         GLSL shader, so the game uses the GLSL version of every shader, and Macs do not support the GLSL \
         versions of CRT Royale and NTSC. If you have a slang version of \u{201c}Retro Glow\u{201d}, add that \
         instead.";
    assert_eq!(said(&notice(&selection, &[ExportTarget::Macos], &library()).unwrap()[0].warnings), [kept]);
    let step = shader_warnings(&selection, &library()).unwrap();
    assert_eq!(step.warned, [root.join("glow.glsl")]);
    assert_eq!(said(&step.warnings), [kept]);

    let mut two = added(&root, &[("Retro Glow", "glow.glsl", PASS), ("Pixel Frame", "frame.glsl", PASS)]);
    two.bundled = vec!["crt-royale".into()];
    assert_eq!(
        said(&notice(&two, &[ExportTarget::Macos], &library()).unwrap()[0].warnings),
        ["The shader \u{201c}CRT Royale\u{201d} won\u{2019}t load on a Mac. The game will run fine, but without \
          the filter. This is because \u{201c}Retro Glow\u{201d} and \u{201c}Pixel Frame\u{201d} are GLSL shaders, \
          so the game uses the GLSL version of every shader, and Macs do not support the GLSL version of CRT \
          Royale. If you have slang versions of \u{201c}Retro Glow\u{201d} and \u{201c}Pixel Frame\u{201d}, add \
          those instead."]
    );

    // An author's shader too new for a Mac and a preset we make in GLSL
    // because of the author's shaders will not load for the same reason, so
    // we name both in one sentence.
    let mut both = added(&root, &[("Retro Glow", "glow.glsl", NEW_PASS), ("Pixel Frame", "frame.glsl", PASS)]);
    both.bundled = vec!["crt-royale".into()];
    assert_eq!(
        said(&notice(&both, &[ExportTarget::Macos], &library()).unwrap()[0].warnings),
        ["The shaders \u{201c}Retro Glow\u{201d} and \u{201c}CRT Royale\u{201d} won\u{2019}t load on a Mac. The game \
          will run fine, but without these filters. This is because \u{201c}Retro Glow\u{201d} and \u{201c}Pixel \
          Frame\u{201d} are GLSL shaders, so the game uses the GLSL version of every shader, and Macs do not support \
          the GLSL versions of Retro Glow and CRT Royale. If you have slang versions of \u{201c}Retro Glow\u{201d} \
          and \u{201c}Pixel Frame\u{201d}, add those instead."]
    );
    let step = shader_warnings(&both, &library()).unwrap();
    assert_eq!(step.warned, [root.join("glow.glsl"), root.join("frame.glsl")]);
    let _ = fs::remove_dir_all(&root);
}

/// We warn only when we know a shader will not load. A slang shader of the
/// author's loads on a Mac, and our presets alone always have a version that
/// loads, so we say nothing on either platform.
#[test]
fn shaders_with_a_version_that_loads_are_no_warning() {
    let root = rominabox_scratch::Scratch::dir("rominabox-warn-none");
    let mut selection = added(&root, &[("Retro Glow", "glow.slang", SLANG)]);
    selection.bundled = vec!["crt-royale".into()];
    assert_eq!(shader_warnings(&selection, &library()).unwrap(), ShaderWarnings::default());
    assert_eq!(notice(&selection, &ExportTarget::ALL, &library()).unwrap(), []);
    let presets = ShaderSelection {
        bundled: vec!["crt-royale".into(), "ntsc-adaptive".into(), "lcd-grid-v2".into()],
        ..Default::default()
    };
    assert_eq!(notice(&presets, &ExportTarget::ALL, &library()).unwrap(), []);
    let _ = fs::remove_dir_all(&root);
}

/// When a file of the author's shader would be at a path longer than Windows
/// allows in an unpacked game, we warn and name the file. With several such
/// shaders we name the shaders and not the files. We make the game either way.
#[test]
fn a_shader_with_a_path_too_long_for_windows_is_a_warning() {
    let root = rominabox_scratch::Scratch::dir("rominabox-warn-windows");
    let deep = deep_file();
    let preset = format!("shaders = 1\nshader0 = {deep}\n");
    let selection = custom_preset(&root, "pal.glslp", &[("pal.glslp", &preset), (&deep, PASS)]);
    let step = shader_warnings(&selection, &library()).unwrap();
    assert_eq!(step.warned, [root.join("pal.glslp")]);
    assert_eq!(
        said(&step.warnings),
        [format!(
                "The shader \u{201c}PAL\u{201d} may not load on Windows. The game will run fine, but without the \
                 filter. This is because the full path of one of its files, \u{201c}{SHORT}\u{201d}, may be longer \
                 than Windows allows. If you can, move the shader\u{2019}s files into fewer folders."
        )]
    );
    assert!(resolve_ok(&selection), "a deep shader must still be accepted");

    let mut two = selection.clone();
    let other = root.join("other");
    let more = custom_preset(&other, "pal.glslp", &[("pal.glslp", &preset), (&deep, PASS)]);
    two.custom.push(CustomShader { name: Some("Second".into()), ..more.custom[0].clone() });
    assert_eq!(
        said(&notice(&two, &[ExportTarget::Windows], &library()).unwrap()[0].warnings),
        ["The shaders \u{201c}PAL\u{201d} and \u{201c}Second\u{201d} may not load on Windows. The game will run \
          fine, but without these filters. This is because the full path of a file in each of them may be longer \
          than Windows allows. If you can, move the shaders\u{2019} files into fewer folders."]
    );

    let shallow = custom_preset(&root.join("shallow"), "pal.glslp", &[
        ("pal.glslp", "shaders = 1\nshader0 = pass.glsl\n"),
        ("pass.glsl", PASS),
    ]);
    assert_eq!(shader_warnings(&shallow, &library()).unwrap(), ShaderWarnings::default());
    let _ = fs::remove_dir_all(&root);
}

/// In a warning we leave out the folders in the middle of a long path, and
/// write a short path whole.
#[test]
fn a_long_path_is_shortened_in_the_middle() {
    assert_eq!(shortened(&deep_file()), SHORT);
    assert_eq!(shortened("shaders/blur/pass.glsl"), "shaders/blur/pass.glsl");
    assert_eq!(shortened("pass.glsl"), "pass.glsl");
}

fn resolve_ok(selection: &ShaderSelection) -> bool {
    crate::shaders::resolve(selection).is_ok()
}

/// With problems on both platforms of the export, we put each platform's
/// sentences under its heading, and leave the platform out of the sentences.
/// With problems on one platform, there is no heading and the sentence names
/// the platform.
#[test]
fn the_notice_has_headings_only_when_both_platforms_have_problems() {
    let root = rominabox_scratch::Scratch::dir("rominabox-warn-both");
    let deep = deep_file();
    let preset = format!("shaders = 1\nshader0 = {deep}\n");
    let mut selection = custom_preset(&root, "pal.glslp", &[("pal.glslp", &preset), (&deep, NEW_PASS)]);
    selection.initial = None;
    let both = notice(&selection, &ExportTarget::ALL, &library()).unwrap();
    let headings: Vec<Option<&str>> = both.iter().map(|section| section.heading.as_deref()).collect();
    assert_eq!(headings, [Some("On a Mac"), Some("On Windows")]);
    assert_eq!(
        said(&both[0].warnings),
        ["The shader \u{201c}PAL\u{201d} won\u{2019}t load. The game will run fine, but without the filter. This is \
          because Macs do not support this GLSL shader. If you have a slang version of the shader, add that instead."]
    );
    assert_eq!(
        said(&both[1].warnings),
        [format!(
            "The shader \u{201c}PAL\u{201d} may not load. The game will run fine, but without the filter. This is \
             because the full path of one of its files, \u{201c}{SHORT}\u{201d}, may be longer than Windows allows. \
             If you can, move the shader\u{2019}s files into fewer folders."
        )]
    );
    let mac = notice(&selection, &[ExportTarget::Macos], &library()).unwrap();
    assert_eq!(mac.len(), 1);
    assert_eq!(mac[0].heading, None);
    assert!(mac[0].warnings[0].text.contains("won\u{2019}t load on a Mac."), "{:?}", mac[0]);
    let _ = fs::remove_dir_all(&root);
}
