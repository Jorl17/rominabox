use super::*;
use std::fs;

/// The menu we compose at export with this shader selection.
fn composed(selection: ShaderSelection) -> crate::menu::Composition {
    let entries = if selection.bundled.is_empty() && selection.custom.is_empty() {
        vec!["controls".to_string()]
    } else {
        vec!["controls".to_string(), "video".to_string(), "shaders".to_string()]
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
/// A lookup texture, as far as we check it, which is a PNG header.
const PICTURE: &[u8] = b"\x89PNG\r\n\x1a\n";

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
        CustomShader { name: Some(name.split('.').next().unwrap().into()), path: root.join(name) }
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
        custom: vec![CustomShader { name: Some("CRT".into()), path: root.join("crt.slang") }],
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
    assert!(declarations.contains("screens = \"pause options controls video shaders\""));
    // Pressing BACK on the shader screen leads to VIDEO, which contains its
    // button, and not to the pause row.
    assert!(
        declarations.contains("screen_button_video = \"video shaders-back\""),
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
    fs::write(source.join("resources/lut.png"), PICTURE).unwrap();
    let composed = composed(ShaderSelection {
        custom: vec![CustomShader {
            name: Some("PAL".into()),
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
            name: Some("PAL".into()),
            path: folder.join(name),
        }],
        initial: Some("pal".into()),
        bundled: Vec::new(),
    }
}

/// Every file below `folder`, by its path from it with `/` between parts.
fn files_below(folder: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut reading = vec![folder.to_path_buf()];
    while let Some(directory) = reading.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                reading.push(path);
                continue;
            }
            let relative = path.strip_prefix(folder).unwrap();
            let parts: Vec<_> = relative.iter().map(|part| part.to_string_lossy()).collect();
            found.push(parts.join("/"));
        }
    }
    found.sort();
    found
}

/// Many libretro GLSL presets name files in a neighbouring folder, for
/// example `crt/crt-royale-pal-r57shell.glslp` has its pass in
/// `../pal/shaders/`. We bundle such a preset with the files it lists,
/// and only those, each where the `../` in the preset leads.
#[test]
fn a_preset_takes_the_files_it_names_from_a_neighbouring_folder() {
    let source = rominabox_scratch::Scratch::dir("rominabox-shader-neighbour");
    let selection = custom_preset(
        &source,
        "crt/royale-pal.glslp",
        &[
            (
                "crt/royale-pal.glslp",
                "shaders = 1\nshader0 = ../pal/shaders/pal.glsl\ntextures = \"lut\"\nlut = \"../pal/resources/lut.png\"\n",
            ),
            ("pal/shaders/pal.glsl", PASS),
            ("pal/shaders/unnamed.glsl", PASS),
            ("pal/pal.glslp", "shaders = 1\nshader0 = shaders/pal.glsl\n"),
        ],
    );
    fs::create_dir_all(source.join("pal/resources")).unwrap();
    fs::write(source.join("pal/resources/lut.png"), PICTURE).unwrap();
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-neighbour-staged");
    composed(selection.clone()).write(&root).unwrap();
    assert_eq!(
        files_below(&root.join("shaders/pal")),
        ["crt/royale-pal.glslp", "icon.png", "pal/resources/lut.png", "pal/shaders/pal.glsl"]
    );
    assert_eq!(
        launch_preset(&selection).unwrap().as_deref(),
        Some("shaders/pal/crt/royale-pal.glslp")
    );
}

/// Presets written on Windows have backslashes in their file paths
/// (`bezel/scanline-classic/`: `..\..\..\shaders\menus\menu-hdr.slang`).
/// For RetroArch, a backslash in the paths of a preset is the platform's
/// separator, so we bundle such a preset as if it had slashes.
#[test]
fn a_preset_with_backslashes_in_its_paths_is_bundled_whole() {
    let source = rominabox_scratch::Scratch::dir("rominabox-shader-backslash");
    let selection = custom_preset(
        &source,
        "crt/royale-pal.glslp",
        &[
            (
                "crt/royale-pal.glslp",
                "shaders = 1\nshader0 = ..\\pal\\shaders\\pal.glsl\ntextures = \"lut\"\nlut = \"..\\pal\\resources\\lut.png\"\n",
            ),
            ("pal/shaders/pal.glsl", PASS),
        ],
    );
    fs::create_dir_all(source.join("pal/resources")).unwrap();
    fs::write(source.join("pal/resources/lut.png"), PICTURE).unwrap();
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-backslash-staged");
    composed(selection).write(&root).unwrap();
    assert_eq!(
        files_below(&root.join("shaders/pal")),
        ["crt/royale-pal.glslp", "icon.png", "pal/resources/lut.png", "pal/shaders/pal.glsl"]
    );
}

/// Every relative path in a preset we stage, `../` included, leads to a
/// staged file in the game's folder for that preset. When we read it again
/// with the same resolver, it lists the same files, and all of them exist.
#[test]
fn a_staged_preset_resolves_inside_its_own_folder() {
    let source = rominabox_scratch::Scratch::dir("rominabox-shader-walk");
    let selection = custom_preset(
        &source,
        "presets/plus/royale-pal.glslp",
        &[
            ("presets/plus/royale-pal.glslp", "#reference \"../../crt/royale.glslp\"\n"),
            (
                "crt/royale.glslp",
                "shaders = 2\nshader0 = ../pal/shaders/pal.glsl\nshader1 = shaders/royale.glsl\ntextures = \"mask\"\nmask = \"../resources/mask.png\"\n",
            ),
            ("crt/shaders/royale.glsl", PASS),
            ("pal/shaders/pal.glsl", PASS),
        ],
    );
    fs::create_dir_all(source.join("resources")).unwrap();
    fs::write(source.join("resources/mask.png"), PICTURE).unwrap();
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-walk-staged");
    composed(selection.clone()).write(&root).unwrap();
    let staged = root.join("shaders/pal");
    let preset = launch_preset(&selection).unwrap().unwrap();
    let again = resolve(&ShaderSelection {
        custom: vec![CustomShader { name: Some("PAL".into()), path: root.join(&preset) }],
        ..selection.clone()
    })
    .unwrap();
    let inside = fs::canonicalize(&staged).unwrap();
    for (file, _) in &again[1].files {
        let resolved = fs::canonicalize(file).unwrap();
        assert!(resolved.starts_with(&inside), "{} is outside {}", file.display(), staged.display());
    }
    let names = |shader: &ResolvedShader| {
        let mut names: Vec<String> = shader.files.iter().map(|(_, name)| name.clone()).collect();
        names.sort();
        names
    };
    assert_eq!(names(&again[1]), names(&resolve(&selection).unwrap()[1]));
    let mut expected = names(&again[1]);
    expected.push("icon.png".into());
    expected.sort();
    assert_eq!(files_below(&staged), expected, "the staged files are those the preset names");
    for file in files_below(&root) {
        let named = ["royale", "pal.glsl", "mask.png"].iter().any(|part| file.contains(part));
        assert!(!named || file.starts_with("shaders/pal/"), "{file} was written outside the game's shader folder");
    }
}

/// With `../` a preset can reach any file on the computer, so we take a
/// file it lists only if it is the kind of file in its line. A lookup
/// texture must be a picture, a `#reference` a preset, and a slang
/// `#include` shader source. We refuse a private key named as any of them,
/// and name the line in the refusal.
#[test]
fn a_named_file_must_be_the_kind_its_line_names() {
    let source = rominabox_scratch::Scratch::dir("rominabox-shader-kinds");
    let secrets = [
        ("home/.ssh/id_rsa", "-----BEGIN OPENSSH PRIVATE KEY-----\n"),
        ("home/.aws/credentials", "[default]\naws_secret_access_key = x\n"),
        ("home/.env", "TOKEN=x\n"),
    ];
    for (name, text, refusal) in [
        (
            "texture/pal.glslp",
            "shaders = 1\nshader0 = ../pass.glsl\ntextures = \"lut\"\nlut = \"../home/.ssh/id_rsa\"\n",
            "shader preset line 4 names ../home/.ssh/id_rsa, which is not a PNG, JPEG, BMP or TGA picture",
        ),
        (
            "reference/pal.glslp",
            "#reference \"../home/.aws/credentials\"\n",
            "shader preset line 1 names ../home/.aws/credentials, which is not a shader preset",
        ),
        (
            "include/crt.slang",
            "#version 450\n#include \"../home/.env\"\n",
            "crt.slang names ../home/.env, which is not shader source",
        ),
    ] {
        let mut files = vec![(name, text), ("pass.glsl", PASS)];
        files.extend(secrets);
        let error = resolve(&custom_preset(&source, name, &files))
            .expect_err(&format!("{name} took a private file into the game"));
        assert!(error.contains(refusal), "{name}: {error}");
    }
    let _ = fs::remove_dir_all(&source);
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

/// A preset must name its files by paths relative to itself, `../` included,
/// and each file must exist. An absolute path leads nowhere once someone
/// copies the game to another computer, and with a missing file the game runs
/// unfiltered with no message. We do not follow `#include` here, so we refuse
/// it and do not copy it with a path nobody checked.
#[test]
fn a_preset_names_files_that_are_there_by_paths_from_itself() {
    let source = rominabox_scratch::Scratch::dir("rominabox-shader-directives");
    let pass = source.join("pass.glsl").display().to_string();
    let absolute_reference = format!("#reference \"{}\"\n", source.join("base.glslp").display());
    let absolute_pass = format!("shaders = 1\nshader0 = \"{pass}\"\n");
    for (name, text, refusal) in [
        ("absolute.glslp", absolute_reference.as_str(), "absolute path"),
        ("absolute-pass.glslp", absolute_pass.as_str(), "absolute path"),
        ("missing.glslp", "#reference \"../missing.glslp\"\n", "missing file: ../missing.glslp"),
        ("included.glslp", "#include \"more.cfg\"\nshaders = 1\nshader0 = pass.glsl\n", "#include"),
    ] {
        let selection = custom_preset(
            &source,
            name,
            &[
                (name, text),
                ("pass.glsl", PASS),
                ("base.glslp", "shaders = 1\nshader0 = pass.glsl\n"),
                ("more.cfg", "\n"),
            ],
        );
        let error = resolve(&selection).unwrap_err();
        assert!(error.contains(refusal), "{name}: {error}");
    }
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
        ],
    );
    fs::write(source.join("icon.png"), PICTURE).unwrap();
    let error = resolve(&selection).expect_err("a preset's icon.png would be overwritten");
    assert!(error.contains("icon.png"), "{error}");
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

    // We keep a single pass that includes files from a neighbouring folder
    // at its place among those files, with its one-pass preset beside it.
    let away = [("away/crt.slang", "#version 450\n#include \"../include/common.inc\"\n")];
    let away = resolve(&custom_preset(&source, "away/crt.slang", &away)).unwrap();
    assert_eq!(away[1].relative_preset, "shaders/pal/away/pal.slangp");
    let names: Vec<&str> = away[1].files.iter().map(|(_, name)| name.as_str()).collect();
    assert_eq!(names, ["away/pal.slang", "include/common.inc", "include/nested.inc"]);

    let included = source.join("include/common.inc").display().to_string();
    for (text, refusal) in [
        (format!("#version 450\n#include \"{included}\"\n"), "absolute path"),
        ("#version 450\n#include \"../nowhere.inc\"\n".to_string(), "crt.slang names a missing file: ../nowhere.inc"),
    ] {
        let refused = [("refused/crt.slang", text.as_str())];
        let error = resolve(&custom_preset(&source, "refused/crt.slang", &refused)).unwrap_err();
        assert!(error.contains(refusal), "{error}");
    }
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

/// The shader library in the repository, which we put in the kit.
fn library() -> std::path::PathBuf {
    crate::repo::at("integrations/shaders/library")
}

/// The menu we compose at export with these catalog presets, from the library.
fn composed_from_library(bundled: &[&str]) -> crate::menu::Composition {
    crate::menu::compose_menu(&crate::menu::MenuRequest {
        shaders: ShaderSelection {
            bundled: bundled.iter().map(|id| id.to_string()).collect(),
            ..Default::default()
        },
        shader_library: library(),
        menu_entries: Some(vec!["controls".into(), "video".into(), "shaders".into()]),
        ..crate::menu::MenuRequest::new(
            crate::repo::at("integrations/designs/native"),
            crate::repo::at("desktop/assets/controllers"),
        )
    })
    .unwrap()
}

/// We put a libretro preset in the game with every file it lists, each at
/// its path in its pack, inside the game's folder for its language, so the
/// paths in the preset and its passes lead to them. Its credits and picture
/// go in the preset's folder.
#[test]
fn a_libretro_preset_is_staged_whole_at_its_paths_in_its_pack() {
    let composed = composed_from_library(&["crt-royale"]);
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-royale");
    composed.write(&root).unwrap();
    let game = root.join("shaders/glsl");
    for file in [
        "crt/crt-royale.glslp",
        "crt/shaders/crt-royale/src/crt-royale-first-pass-linearize-crt-gamma-bob-fields.glsl",
        "crt/shaders/crt-royale/TileableLinearApertureGrille15Wide8And5d5Spacing.png",
        "blurs/shaders/royale/blur9fast-vertical.glsl",
    ] {
        assert!(game.join(file).is_file(), "{file} is not in the game");
    }
    let preset = fs::read_to_string(game.join("crt/crt-royale.glslp")).unwrap();
    for (key, value) in preset.lines().filter_map(|line| line.split_once('=')) {
        let key = key.trim();
        if !(key.len() > 6 && key.starts_with("shader") && key[6..].chars().all(|c| c.is_ascii_digit())) {
            continue;
        }
        let named = value.trim().trim_matches('"');
        assert!(game.join("crt").join(named).is_file(), "{named} is not where the preset looks");
    }
    let credits = fs::read_to_string(root.join("shaders/crt-royale/CREDITS.txt")).unwrap();
    assert!(credits.contains("TroggleMonkey") && credits.contains("libretro/glsl-shaders"), "{credits}");
    let config = composed.text("shaders.cfg").unwrap();
    assert!(config.contains("shader_preset_crt-royale = \"shaders/glsl/crt/crt-royale.glslp\""), "{config}");
    let selection = ShaderSelection { bundled: vec!["crt-royale".into()], ..Default::default() };
    assert_eq!(video_driver(&selection).unwrap(), VideoDriver::Gl);
}

/// When a libretro preset exists only in slang, the game uses slang, and we
/// take the other catalog presets in slang too.
#[test]
fn a_preset_only_in_slang_makes_the_game_slang() {
    let selection = ShaderSelection {
        bundled: vec!["crt-easymode".into(), "crt-guest-advanced".into(), "scanlines".into()],
        ..Default::default()
    };
    assert_eq!(video_driver(&selection).unwrap(), VideoDriver::Glcore);
    let presets: Vec<String> = resolve(&selection).unwrap().into_iter().map(|item| item.relative_preset).collect();
    assert_eq!(
        presets,
        [
            "",
            "shaders/slang/crt/crt-easymode.slangp",
            "shaders/slang/crt/crt-guest-advanced.slangp",
            "shaders/scanlines/scanlines.slangp",
        ]
    );
    let composed = composed_from_library(&["crt-guest-advanced"]);
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-guest");
    composed.write(&root).unwrap();
    assert!(root.join("shaders/slang/crt/crt-guest-advanced.slangp").is_file());
}

/// When the author adds a shader, its language is the game's. We refuse a
/// catalog preset that libretro lacks in that language, and name both.
#[test]
fn a_catalog_preset_not_in_the_authors_language_is_refused() {
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-mixed");
    let mut selection = custom_preset(
        &root,
        "mine.glslp",
        &[("mine.glslp", "shaders = 1\nshader0 = pass.glsl\n"), ("pass.glsl", PASS)],
    );
    selection.bundled = vec!["crt-guest-advanced".into()];
    let error = resolve(&selection).unwrap_err();
    assert!(error.contains("CRT Guest has no GLSL version"), "{error}");
    let _ = fs::remove_dir_all(&root);
}

/// Every libretro preset in the catalog is in the library with every file it
/// lists, and nothing else is there, because we include it in the builder.
#[test]
fn the_library_holds_each_catalog_preset_whole_and_nothing_else() {
    let mut named = std::collections::BTreeSet::new();
    for preset in library_presets().unwrap() {
        let (folder, path) = preset.split_once('/').unwrap();
        for (_, name) in library_files(&library().join(folder), path).unwrap() {
            named.insert(format!("{folder}/{name}"));
        }
    }
    let mut held = std::collections::BTreeSet::new();
    let mut folders = vec![library()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                let relative = path.strip_prefix(library()).unwrap();
                held.insert(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    assert_eq!(held, named);
}

/// Every catalogue preset has its picture, so no row in a game, and no card
/// in the builder, is blank.
#[test]
fn every_catalog_preset_has_its_preview() {
    for entry in catalog().unwrap() {
        assert!(PREVIEWS.iter().any(|(id, _)| *id == entry.id), "{} has no preview", entry.id);
    }
}

/// We keep a shader from the author with the name of a language out of the
/// library folder for that language, where we put the libretro presets.
#[test]
fn an_authors_shader_never_takes_a_library_folder() {
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-named-glsl");
    let mut selection = custom_preset(
        &root,
        "mine.glslp",
        &[("mine.glslp", "shaders = 1\nshader0 = pass.glsl\n"), ("pass.glsl", PASS)],
    );
    selection.custom[0].name = Some("GLSL".into());
    selection.initial = None;
    selection.bundled = vec!["crt-lottes".into()];
    let ids: Vec<String> = resolve(&selection).unwrap().into_iter().map(|item| item.id).collect();
    assert_eq!(ids, ["none", "crt-lottes", "glsl-2"]);
    let _ = fs::remove_dir_all(&root);
}

/// A filter the author added has no picture of what it does, so in its row
/// we show a pixel S, for shader, at the size of the other filter pictures.
#[test]
fn an_added_filter_shows_a_pixel_s() {
    let picture = image::load_from_memory(&icon_png("mine").unwrap()).unwrap().to_rgba8();
    assert_eq!(picture.dimensions(), (256, 256));
    // The middle of the S's top bar, and the open space right of its middle.
    assert_eq!(picture.get_pixel(128, 44).0, [232, 232, 232, 255]);
    assert_eq!(picture.get_pixel(128, 100).0, [16, 18, 24, 255]);
}

/// When a file of the author's filter would be at a path longer than Windows
/// can open in an unpacked game, we warn and name the file. We give no warning
/// for a filter whose files all fit, and we make the game in either case.
#[test]
fn an_authors_filter_too_deep_for_windows_is_a_warning() {
    let deep_source = rominabox_scratch::Scratch::dir("rominabox-shader-deep");
    let deep = format!("{}/pass.glsl", ["a-folder-of-twenty-c"; 8].join("/"));
    let preset = format!("shaders = 1\nshader0 = {deep}\n");
    let selection = custom_preset(&deep_source, "pal.glslp", &[("pal.glslp", &preset), (&deep, PASS)]);
    assert_eq!(
        windows_warnings(&selection).unwrap(),
        [ShaderWarning {
            path: deep_source.join("pal.glslp"),
            sentence: format!(
                "On Windows this filter may not load, and the game would run without it: \
                 \u{201c}{deep}\u{201d} sits too deep among its folders."
            ),
        }]
    );
    assert!(resolve(&selection).is_ok(), "a deep filter must still be accepted");

    let shallow_source = rominabox_scratch::Scratch::dir("rominabox-shader-shallow");
    let shallow = custom_preset(
        &shallow_source,
        "pal.glslp",
        &[("pal.glslp", "shaders = 1\nshader0 = pass.glsl\n"), ("pass.glsl", PASS)],
    );
    assert_eq!(windows_warnings(&shallow).unwrap(), []);
    let _ = fs::remove_dir_all(&deep_source);
    let _ = fs::remove_dir_all(&shallow_source);
}

/// We call a file nobody named by its file name without a shader extension,
/// in any case, and keep any other extension.
#[test]
fn an_unnamed_file_is_named_after_it() {
    for (file, name) in [
        ("crt.glsl", "crt"),
        ("pal-r57shell.SLANGP", "pal-r57shell"),
        ("CRT Royale.glslp", "CRT Royale"),
        ("my.crt.slang", "my.crt"),
        ("notes.txt", "notes.txt"),
        (".slang", ".slang"),
    ] {
        assert_eq!(named_after_file(Path::new(file)), name, "{file}");
    }
    let given = |name: Option<&str>| CustomShader { name: name.map(String::from), path: "crt.glsl".into() };
    assert_eq!(given(Some(" Royale ")).name(), "Royale");
    assert_eq!(given(Some("  ")).name(), "crt");
    assert_eq!(given(None).name(), "crt");
}

/// We keep the name of a shader nobody named in the project, because we pack
/// its file under its id in the game, which differs from that name.
#[test]
fn a_packed_project_keeps_the_name_its_file_gave() {
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-packed-name");
    fs::write(root.join("CRT Royale.glsl"), PASS).unwrap();
    let selection = ShaderSelection {
        custom: vec![CustomShader { name: None, path: root.join("CRT Royale.glsl") }],
        ..ShaderSelection::default()
    };
    let (stored, files) = pack_selection(&selection).unwrap();
    assert_eq!(stored.custom[0].name.as_deref(), Some("CRT Royale"));
    assert_ne!(named_after_file(&stored.custom[0].path), "CRT Royale", "{files:?}");
}

/// For a bundled preset with a brightness parameter, we give the game that
/// parameter, the light we measured at each of its values and our pass for
/// brightness and contrast. For a preset without one, we give no parameter.
#[test]
fn a_preset_that_adds_light_tells_the_game_how_much() {
    let composed = composed_from_library(&["crt-lottes", "sharp-bilinear-simple"]);
    let config = composed.text("shaders.cfg").unwrap();
    let line = config
        .lines()
        .find(|line| line.starts_with("shader_brightness_crt-lottes = \"brightBoost 1:1 "))
        .unwrap_or_else(|| panic!("{config}"));
    assert!(line.split(' ').count() >= 4, "{line}");
    assert!(!config.contains("shader_brightness_sharp-bilinear-simple"), "{config}");
    assert!(config.contains("video_pass = \"shaders/video/video.glslp\""), "{config}");
}

/// A preset with other light in its GLSL version has a table for each
/// language in the catalogue. We give a GLSL game the GLSL table, and a slang
/// game, here one with a preset that exists only in slang, the other.
#[test]
fn a_game_gets_the_brightness_table_of_its_language() {
    let catalog: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(crate::repo::at("integrations/shaders/catalog.json")).unwrap())
            .unwrap();
    let ntsc = catalog["presets"].as_array().unwrap().iter().find(|preset| preset["id"] == "ntsc-adaptive").unwrap();
    let line = |key: &str| {
        let pairs: Vec<String> = ntsc["brightness"][key]
            .as_array()
            .unwrap_or_else(|| panic!("ntsc-adaptive has no {key}"))
            .iter()
            .map(|pair| format!("{}:{}", pair[0].as_f64().unwrap() as f32, pair[1].as_f64().unwrap() as f32))
            .collect();
        format!("shader_brightness_ntsc-adaptive = \"ntsc_bright {}\"", pairs.join(" "))
    };
    let glsl_game = composed_from_library(&["ntsc-adaptive"]);
    let glsl = glsl_game.text("shaders.cfg").unwrap();
    assert!(glsl.lines().any(|found| found == line("glslTable")), "{glsl}");
    let slang_game = composed_from_library(&["ntsc-adaptive", "crt-guest-advanced"]);
    let slang = slang_game.text("shaders.cfg").unwrap();
    assert!(slang.lines().any(|found| found == line("table")), "{slang}");
}

/// For an author's shader, we give the game the brightness parameter from its
/// passes, a plain "brightness" first, at its value in the author's preset,
/// with the light rising in proportion to the value up to the maximum.
#[test]
fn an_authors_shader_brightens_through_the_parameter_it_declares() {
    let source = rominabox_scratch::Scratch::dir("rominabox-shader-light");
    let glow = format!("#pragma parameter GLOW_GAIN \"Glow gain\" 1.0 0.0 3.0 0.1\n{PASS}");
    let light = format!("#pragma parameter BRIGHTNESS \"Brightness\" 1.0 0.0 2.5 0.05\n{PASS}");
    let selection = custom_preset(
        &source,
        "pal.glslp",
        &[
            ("pal.glslp", "shaders = 2\nshader0 = a.glsl\nshader1 = b.glsl\nBRIGHTNESS = \"1.25\"\n"),
            ("a.glsl", &glow),
            ("b.glsl", &light),
        ],
    );
    let composed = composed(selection);
    let config = composed.text("shaders.cfg").unwrap();
    assert!(config.contains("shader_brightness_pal = \"BRIGHTNESS 1:1.25 2:2.5\"\n"), "{config}");
    let _ = fs::remove_dir_all(&source);
}

/// Without a plain "brightness", we take the first parameter named or
/// described for light that can rise above its value in the shader. We pass
/// over a parameter at 0, one at its maximum and one named for something
/// else.
#[test]
fn the_first_parameter_for_light_that_can_rise_is_the_one() {
    let source = rominabox_scratch::Scratch::dir("rominabox-shader-light-first");
    let pass = source.join("crt.glsl");
    let declared = |lines: &str| {
        fs::write(&pass, format!("{lines}{PASS}")).unwrap();
        super::brightness::declared(&[(pass.clone(), "crt.glsl".to_string())])
    };
    let control = declared(concat!(
        "#pragma parameter lum \"Luminance\" 0.0 0.0 1.0 0.01\n",
        "#pragma parameter post_br \"Post-Brightness\" 1.0 0.25 1.0 0.01\n",
        "#pragma parameter SCANLINE \"Scanline weight\" 0.5 0.0 1.0 0.1\n",
        "#pragma parameter BRIGHT_BOOST \"Bright boost\" 1.2 1.0 2.0 0.05\n",
        "#pragma parameter gain \"Gain\" 1.0 0.0 2.0 0.1\n",
    ));
    assert_eq!(
        control,
        Some(BrightnessControl {
            parameter: "BRIGHT_BOOST".into(),
            table: vec![(1.0, 1.2), (1.667, 2.0)],
        })
    );
    assert_eq!(declared("#pragma parameter SCANLINE \"Scanline weight\" 0.5 0.0 1.0 0.1\n"), None);
    let _ = fs::remove_dir_all(&source);
}

/// We stage a preset that we arranged from the pack's files with the files
/// from the pack that we name in it, say in its credits that we arranged it,
/// and give the game the light at each value of its brightness parameter. In
/// the credits of a preset from the pack, we name its path there.
#[test]
fn an_arranged_preset_is_staged_with_the_packs_files_and_its_credits() {
    let composed = composed_from_library(&["gba-lcd-original-colours", "crt-royale"]);
    let root = rominabox_scratch::Scratch::dir("rominabox-shader-arranged");
    composed.write(&root).unwrap();
    let game = root.join("shaders/slang");
    for file in [
        "rominabox/gba-lcd-original-colours.slangp",
        "rominabox/shaders/authentic_gba_fast.slang",
        "handheld/shaders/authentic_gbc/to_lin_fast.slang",
        "handheld/shaders/authentic_gbc/parameters.inc",
        "handheld/shaders/authentic_gbc/shared.inc",
        "handheld/shaders/color/gba-color.slang",
    ] {
        assert!(game.join(file).is_file(), "{file} is not in the game");
    }
    let arranged = fs::read_to_string(root.join("shaders/gba-lcd-original-colours/CREDITS.txt")).unwrap();
    assert!(
        arranged.contains("fishku")
            && arranged.contains("Arranged for ROM-in-a-Box from files in https://github.com/libretro/slang-shaders"),
        "{arranged}"
    );
    let royale = fs::read_to_string(root.join("shaders/crt-royale/CREDITS.txt")).unwrap();
    assert!(royale.contains("libretro/slang-shaders at commit") && royale.contains(": crt/crt-royale.slangp"), "{royale}");
    let config = composed.text("shaders.cfg").unwrap();
    assert!(
        config.contains("shader_brightness_gba-lcd-original-colours = \"AUTH_GBC_BRIG 1:0.08 "),
        "{config}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Each language version of a preset contains a `#pragma parameter` line for
/// the brightness parameter that we give the preset in the catalogue, so that
/// we can raise it in a game in either language.
#[test]
fn each_language_of_a_preset_declares_its_parameter_that_adds_light() {
    for preset in catalog_file().unwrap().presets {
        let (Some(control), Made::Files(files)) = (&preset.brightness, &preset.made) else {
            continue;
        };
        for language in Language::ALL {
            let Some(path) = files.in_language(language) else {
                continue;
            };
            let folder = library_folder(language);
            let listed = library_files(&library().join(folder), path).unwrap();
            assert!(
                super::brightness::declares(&listed, &control.parameter),
                "{folder}/{path} declares no {}",
                control.parameter
            );
        }
    }
}
