use super::*;
use std::fs;

/// The menu we compose at export with this shader selection.
fn composed(selection: ShaderSelection) -> crate::menu::Composition {
    let entries = if selection.bundled.is_empty() && selection.custom.is_empty() {
        vec!["controls".to_string()]
    } else {
        vec!["controls".to_string(), "shaders".to_string()]
    };
    crate::menu::compose_menu(&crate::menu::MenuRequest {
        shaders: selection,
        menu_entries: Some(entries),
        ..crate::menu::MenuRequest::new(
            crate::repo::at("integrations/designs/native"),
            crate::repo::at("desktop/assets/controllers"),
        )
    })
    .unwrap()
}

#[test]
fn an_empty_selection_is_no_shaders() {
    let resolved = resolve(&ShaderSelection::default()).unwrap();
    assert!(resolved.is_empty());
}

/// A GLSL pass for the OpenGL driver of RetroArch, as far as we check it.
const PASS: &str = "#if defined(VERTEX)\n#elif defined(FRAGMENT)\n#endif\n";
/// A slang pass, as far as we check it.
const SLANG: &str = "#version 450\n#pragma stage vertex\n#pragma stage fragment\n";

/// We recognise a slang preset by the contents of its pass, whatever its
/// name, stage it under the file name for slang in RetroArch, and run
/// the game on glcore. A game without one stays on gl.
#[test]
fn a_slang_preset_is_staged_as_slang_and_runs_on_glcore() {
    let root = rominabox_scratch::Scratch::dir("rominabox-slang");
    let selection = custom_preset(
        &root,
        "crt.glslp",
        &[("crt.glslp", "shaders = 1\nshader0 = crt.slang\n"), ("crt.slang", SLANG)],
    );
    let resolved = resolve(&selection).unwrap();
    assert_eq!(resolved[1].relative_preset, "shaders/pal/crt.slangp");
    assert_eq!(video_driver(&selection).unwrap(), VideoDriver::Glcore);
    assert_eq!(video_driver(&ShaderSelection::default()).unwrap(), VideoDriver::Gl);
    let glsl = custom_preset(
        &root,
        "pal.glslp",
        &[("pal.glslp", "shaders = 1\nshader0 = pass.glsl\n"), ("pass.glsl", PASS)],
    );
    assert_eq!(video_driver(&glsl).unwrap(), VideoDriver::Gl);
    let _ = fs::remove_dir_all(&root);
}

/// Each game has one video driver, which takes one language. We refuse a
/// slang shader beside a GLSL one, in a game or in one preset, and we
/// refuse Cg.
#[test]
fn shaders_in_two_languages_and_cg_are_refused() {
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-languages");
    let file = |name: &str, text: &str| {
        fs::write(root.join(name), text).unwrap();
        CustomShader { name: name.split('.').next().unwrap().into(), path: root.join(name) }
    };
    let selection = ShaderSelection {
        custom: vec![file("crt.slang", SLANG), file("pal.glsl", PASS)],
        ..ShaderSelection::default()
    };
    let error = resolve(&selection).unwrap_err();
    assert!(error.contains("crt is slang and pal is GLSL"), "{error}");
    assert!(error.contains("one language"), "{error}");
    let mixed = custom_preset(
        &root,
        "mixed.slangp",
        &[("mixed.slangp", "shaders = 2\nshader0 = crt.slang\nshader1 = pal.glsl\n")],
    );
    let error = resolve(&mixed).unwrap_err();
    assert!(error.contains("crt.slang is slang and pal.glsl is GLSL"), "{error}");
    let cg = ShaderSelection {
        custom: vec![file("old.cg", "float4 main_fragment() : COLOR { return 0; }\n")],
        ..ShaderSelection::default()
    };
    assert!(resolve(&cg).unwrap_err().contains("old.cg is a Cg shader"));
    let _ = fs::remove_dir_all(&root);
}

/// Each catalog preset is one fragment, which we write in the language of
/// the author's shaders, so in a slang game we bundle them as slang.
#[test]
fn catalog_presets_are_written_in_a_slang_game_s_language() {
    let root = rominabox_scratch::Scratch::dir("rominabox-slang-catalog");
    fs::write(root.join("crt.slang"), SLANG).unwrap();
    let selection = ShaderSelection {
        bundled: vec!["scanlines".into()],
        custom: vec![CustomShader { name: "CRT".into(), path: root.join("crt.slang") }],
        initial: Some("scanlines".into()),
    };
    let resolved = resolve(&selection).unwrap();
    assert_eq!(resolved[1].relative_preset, "shaders/scanlines/scanlines.slangp");
    assert_eq!(resolved[2].relative_preset, "shaders/crt/crt.slangp");
    let staged = rominabox_scratch::Scratch::dir("rominabox-slang-catalog-staged");
    composed(selection.clone()).write(&staged).unwrap();
    let pass = fs::read_to_string(staged.join("shaders/scanlines/scanlines.slang")).unwrap();
    assert_eq!(crate::shader_format::language(&pass), Ok(Language::Slang));
    assert!(pass.contains("float line = mod(floor(TEX0.y * TextureSize.y), 2.0);"), "{pass}");
    let preset = fs::read_to_string(staged.join("shaders/scanlines/scanlines.slangp")).unwrap();
    assert!(preset.contains("shader0 = scanlines.slang"), "{preset}");
    assert!(staged.join("shaders/crt/crt.slang").is_file());
    assert_eq!(
        launch_preset(&selection).unwrap().as_deref(),
        Some("shaders/scanlines/scanlines.slangp")
    );
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&staged);
}

/// We recognise a preset by its count of passes, and stage it under the
/// file name for a GLSL preset in RetroArch.
#[test]
fn a_preset_is_known_by_what_it_holds() {
    let root = rominabox_scratch::Scratch::dir("rominabox-preset-named-otherwise");
    let selection = custom_preset(
        &root,
        "pal.txt",
        &[
            ("pal.txt", "shaders = 1\nshader0 = pass.glsl\n"),
            ("pass.glsl", PASS),
        ],
    );
    let resolved = resolve(&selection).unwrap();
    assert_eq!(resolved[1].relative_preset, "shaders/pal/pal.glslp");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn bundling_a_preset_uses_the_row_and_writes_glsl() {
    let composed = composed(ShaderSelection {
        bundled: vec!["scanlines".into(), "phosphor".into()],
        initial: Some("phosphor".into()),
        custom: Vec::new(),
    });
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-stage");
    composed.write(&root).unwrap();
    let document = composed.text("menu.rml").unwrap();
    let list_at = document.find("id=\"shaders-list\"").expect("the shader list");
    let list = &document[list_at..document[list_at..].find("id=\"shaders-back\"").unwrap() + list_at];
    assert_eq!(list.matches("class=\"list-row ").count(), 3, "{list}");
    assert!(document.contains("id=\"phosphor\""));
    // There is no button on the pause row. The shader screen is an entry in
    // Options, the one screen with the controls, shaders and sound.
    assert!(
        !document.contains("class=\"menu-action screen-link\" id=\"shaders\""),
        "the shader screen must not add a button to the pause row"
    );
    assert!(document.contains(">SHADERS<"), "the Options entry");
    assert!(!document.contains(crate::lists::LINKS_SLOT));
    let config = composed.text("shaders.cfg").unwrap();
    assert!(config.contains("shader_preset_scanlines = \"shaders/scanlines/scanlines.glslp\""));
    assert_eq!(
        launch_preset(&ShaderSelection {
            bundled: vec!["scanlines".into(), "phosphor".into()],
            initial: Some("phosphor".into()),
            custom: Vec::new(),
        })
        .unwrap()
        .as_deref(),
        Some("shaders/phosphor/phosphor.glslp")
    );
    let preset = fs::read_to_string(root.join("shaders/scanlines/scanlines.glslp")).unwrap();
    assert!(preset.contains("shader0 = scanlines.glsl"));
    let source = fs::read_to_string(root.join("shaders/phosphor/phosphor.glsl")).unwrap();
    assert!(source.contains("#if defined(VERTEX)"));
    assert!(source.contains("#elif defined(FRAGMENT)"));
    assert!(root.join("shaders/phosphor/icon.png").is_file());
    let declarations = composed.text("design.cfg").unwrap();
    assert!(declarations.contains("screens = \"pause options controls shaders\""));
    // Pressing BACK on the shader screen leads to Options, which contains
    // it, and not to the pause row.
    assert!(
        declarations.contains("screen_button_options = \"options shaders-back\""),
        "{declarations}"
    );
    assert!(declarations.contains("screen_button_pause = \"options-back\""));
}

/// libretro presets have their passes in `shaders/` and their lookup
/// textures in `resources/`. If we copied them flat, a preset such as
/// `pal-r57shell.glslp` would name files that are not there, and the game
/// would run with no shader and no message to the player.
#[test]
fn a_preset_keeps_its_folders_so_every_file_it_names_is_there() {
    let source = rominabox_scratch::Scratch::dir("rominabox-shader-folders");
    fs::create_dir_all(source.join("shaders")).unwrap();
    fs::create_dir_all(source.join("resources")).unwrap();
    let preset = source.join("pal.glslp");
    fs::write(
        &preset,
        "shaders = 1\nshader0 = shaders/pass.glsl\ntextures = \"lut\"\nlut = \"resources/lut.png\"\n",
    )
    .unwrap();
    fs::write(source.join("shaders/pass.glsl"), PASS).unwrap();
    fs::write(source.join("resources/lut.png"), b"lut").unwrap();
    let composed = composed(ShaderSelection {
        custom: vec![CustomShader {
            name: "PAL".into(),
            path: preset,
        }],
        initial: Some("pal".into()),
        bundled: Vec::new(),
    });
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-folders-staged");
    composed.write(&root).unwrap();
    let staged = root.join("shaders/pal");
    assert!(staged.join("pal.glslp").is_file());
    for named in ["shaders/pass.glsl", "resources/lut.png"] {
        assert!(
            staged.join(named).is_file(),
            "the preset names {named}, which is not beside it"
        );
    }
    let _ = fs::remove_dir_all(&source);
    let _ = fs::remove_dir_all(&root);
}

/// A preset from the author at `folder/name`, with the other files listed,
/// as the only custom shader of a selection.
fn custom_preset(folder: &Path, name: &str, files: &[(&str, &str)]) -> ShaderSelection {
    for (path, text) in files {
        let path = folder.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    ShaderSelection {
        custom: vec![CustomShader {
            name: "PAL".into(),
            path: folder.join(name),
        }],
        initial: Some("pal".into()),
        bundled: Vec::new(),
    }
}

/// A preset can name another with `#reference`, whose files are beside it.
/// The libretro `presets/` folders contain such presets.
#[test]
fn a_referenced_preset_and_its_files_are_copied_too() {
    let source = rominabox_scratch::Scratch::dir("rominabox-shader-reference");
    let selection = custom_preset(
        &source,
        "pal.glslp",
        &[
            ("pal.glslp", "#reference \"base/base.glslp\"\n"),
            ("base/base.glslp", "shaders = 1\nshader0 = shaders/pass.glsl\n"),
            ("base/shaders/pass.glsl", PASS),
        ],
    );
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-reference-staged");
    composed(selection).write(&root).unwrap();
    for named in ["pal.glslp", "base/base.glslp", "base/shaders/pass.glsl"] {
        assert!(root.join("shaders/pal").join(named).is_file(), "{named} was not copied");
    }
    let _ = fs::remove_dir_all(&source);
    let _ = fs::remove_dir_all(&root);
}

/// We apply the rule for passes to the directives used by RetroArch, so
/// their files must be next to the preset. We do not follow `#include`
/// here, so we reject it and do not copy it with an unchecked path.
#[test]
fn a_preset_cannot_reach_outside_its_folder_through_a_directive() {
    let source = rominabox_scratch::Scratch::dir("rominabox-shader-directives");
    for (name, text) in [
        ("outside.glslp", "#reference \"../elsewhere.glslp\"\nshaders = 1\nshader0 = pass.glsl\n"),
        ("included.glslp", "#include \"more.cfg\"\nshaders = 1\nshader0 = pass.glsl\n"),
    ] {
        let selection = custom_preset(
            &source,
            name,
            &[(name, text), ("pass.glsl", PASS), ("more.cfg", "\n")],
        );
        assert!(resolve(&selection).is_err(), "{name} was accepted");
    }
    let _ = fs::remove_dir_all(&source);
}

/// The row's picture is `icon.png` in the preset's folder, and we write it
/// last, so it would silently replace a preset file with that name.
#[test]
fn a_preset_file_the_row_picture_would_replace_is_refused() {
    let source = rominabox_scratch::Scratch::dir("rominabox-shader-icon");
    let selection = custom_preset(
        &source,
        "pal.glslp",
        &[
            (
                "pal.glslp",
                "shaders = 1\nshader0 = pass.glsl\ntextures = \"icon\"\nicon = \"icon.png\"\n",
            ),
            ("pass.glsl", PASS),
            ("icon.png", "lookup"),
        ],
    );
    assert!(resolve(&selection).is_err(), "a preset's icon.png would be overwritten");
    let _ = fs::remove_dir_all(&source);
}

/// We copy the files a slang pass `#include`s, at their paths from the
/// including file. If we leave one behind, the game runs unfiltered with
/// no message. libretro passes include files from a folder beside the
/// preset's (`../include/`), which still belongs with the preset.
#[test]
fn a_slang_pass_takes_the_files_it_includes() {
    let source = rominabox_scratch::Scratch::dir("rominabox-slang-includes");
    let pass = "#version 450\n#include \"../include/common.inc\"\n";
    let selection = custom_preset(
        &source,
        "crt.slangp",
        &[
            ("crt.slangp", "shaders = 1\nshader0 = shaders/crt.slang\n"),
            ("shaders/crt.slang", pass),
            ("include/common.inc", "#pragma stage vertex\n#include \"nested.inc\"\n"),
            ("include/nested.inc", "#pragma stage fragment\n"),
        ],
    );
    let root = rominabox_scratch::Scratch::dir("rominabox-slang-includes-staged");
    composed(selection).write(&root).unwrap();
    let staged = ["crt.slangp", "shaders/crt.slang", "include/common.inc", "include/nested.inc"];
    for named in staged {
        let path = root.join("shaders/pal").join(named);
        assert!(path.is_file(), "{named} was not copied");
    }

    let lone_files = [("lone/crt.slang", "#version 450\n#include \"stages.inc\"\n"), ("lone/stages.inc", SLANG)];
    let lone = resolve(&custom_preset(&source, "lone/crt.slang", &lone_files)).unwrap();
    let names: Vec<&str> = lone[1].files.iter().map(|(_, name)| name.as_str()).collect();
    assert!(names.contains(&"stages.inc"), "a lone pass's include was not copied: {names:?}");

    let away = [("away/crt.slang", "#version 450\n#include \"../include/common.inc\"\n")];
    let error = resolve(&custom_preset(&source, "away/crt.slang", &away)).unwrap_err();
    assert!(error.contains("beside it"), "{error}");
    let _ = fs::remove_dir_all(&source);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_ordinary_menu_gains_no_shader_screen() {
    let composed = composed(ShaderSelection::default());
    let document = composed.text("menu.rml").unwrap();
    assert!(!document.contains("id=\"shaders\""));
    assert!(!document.contains("id=\"shaders-panel\""));
    assert!(composed.text("shaders.cfg").is_none());
    assert!(!composed
        .names()
        .iter()
        .any(|name| name.starts_with("shaders")));
    assert!(!document.contains("video_shader"));
}
