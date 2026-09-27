//! The kind of shader file that an author added, read from the file itself.
//!
//! RetroArch has three shader languages, each for some of its video drivers,
//! GLSL (the OpenGL driver), slang (Vulkan, Metal, Direct3D 11 and 12, and
//! the core-profile OpenGL driver) and the retired Cg. A preset lists passes
//! in one of them. The name of a file can be wrong, so we judge it by its
//! content. Nothing is compiled. We recognise a pass by the typical lines of
//! each language, and a preset by its number of passes or by its base preset.

use std::fs;
use std::path::Path;

/// A language RetroArch runs shaders in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// In the OpenGL driver it is compiled twice, with `VERTEX` and then
    /// `FRAGMENT` defined.
    Glsl,
    /// `#version 450`, and one file holding both stages, each after
    /// `#pragma stage`, or an `#include` of them.
    Slang,
    /// Entry points `main_vertex` and `main_fragment`.
    Cg,
}

/// What a file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A list of passes: `shaders = N`, then `shader0 = …` and so on, or
    /// `#reference` to a preset it builds on.
    Preset,
    /// One pass's source.
    Pass,
}

/// The text of a shader file. Not necessarily UTF-8: libretro's own passes
/// have Latin-1 in their comments, which are only bytes to RetroArch.
pub fn text(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", name(path)))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// A preset says how many passes it has, or which preset it builds on; a
/// pass does neither.
pub fn kind(text: &str) -> Kind {
    let is_preset = text.lines().map(str::trim).any(|line| {
        line.starts_with("#reference ")
            // As with C's strtoul in RetroArch, the count `1_` is 1.
            || line.split_once('=').is_some_and(|(key, value)| {
                key.trim() == "shaders"
                    && value.trim().trim_matches('"').starts_with(|c: char| c.is_ascii_digit())
            })
    });
    if is_preset {
        Kind::Preset
    } else {
        Kind::Pass
    }
}

/// The language a pass is written in, when it is one.
pub fn language(text: &str) -> Option<Language> {
    let directives: Vec<&str> = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix('#'))
        .map(str::trim)
        .collect();
    let branches_on = |stage: &str| {
        directives.iter().any(|directive| {
            (directive.starts_with("if") || directive.starts_with("elif"))
                && directive
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .any(|word| word == stage)
        })
    };
    // Check this first, because a GLSL pass ported from slang can still have
    // its `#pragma stage` lines, and the OpenGL driver ignores them.
    if branches_on("VERTEX") && branches_on("FRAGMENT") {
        return Some(Language::Glsl);
    }
    if directives.iter().any(|directive| {
        directive.starts_with("pragma stage") || directive.starts_with("version 450")
    }) {
        return Some(Language::Slang);
    }
    if text.contains("main_fragment") || text.contains("main_vertex") {
        return Some(Language::Cg);
    }
    None
}

/// A pass for an exported game, in GLSL because its video driver is OpenGL.
pub fn require_runnable_pass(path: &Path) -> Result<(), String> {
    let source = text(path)?;
    if kind(&source) == Kind::Preset {
        return Err(format!(
            "{} is a preset where a shader pass belongs",
            name(path)
        ));
    }
    match language(&source) {
        Some(Language::Glsl) => Ok(()),
        Some(Language::Slang) => Err(format!(
            "{} is a slang shader. Exported games run GLSL shaders (.glsl or .glslp).",
            name(path)
        )),
        Some(Language::Cg) => Err(format!(
            "{} is a Cg shader. Exported games run GLSL shaders (.glsl or .glslp).",
            name(path)
        )),
        None => Err(not_a_shader(path)),
    }
}

fn not_a_shader(path: &Path) -> String {
    format!(
        "{} is not a shader RetroArch can run. Add a .glsl or .glslp shader.",
        name(path)
    )
}

fn name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const GLSL: &str =
        "#if defined(VERTEX)\nvoid main() {}\n#elif defined(FRAGMENT)\nvoid main() {}\n#endif\n";
    const SLANG: &str = "#version 450\n#pragma stage vertex\nvoid main() {}\n#pragma stage fragment\nvoid main() {}\n";
    const CG: &str = "struct out_vertex { float4 position : POSITION; };\nfloat4 main_fragment() : COLOR { return 0; }\n";

    #[test]
    fn each_language_is_read_from_its_passes() {
        assert_eq!(language(GLSL), Some(Language::Glsl));
        assert_eq!(
            language("#ifdef VERTEX\n#endif\n#ifdef FRAGMENT\n#endif\n"),
            Some(Language::Glsl)
        );
        assert_eq!(language(SLANG), Some(Language::Slang));
        // libretro's imgborder.slang, whose stages are in an included file.
        assert_eq!(
            language("#version 450\n#include \"imgborder.inc\"\n"),
            Some(Language::Slang)
        );
        assert_eq!(language(CG), Some(Language::Cg));
        assert_eq!(language("// the pass\n"), None);
        assert_eq!(
            language("#if defined(VERTEXES)\n#elif defined(FRAGMENTS)\n#endif\n"),
            None
        );
        // libretro's crt-blurPi.glsl, ported from slang.
        assert_eq!(
            language(&format!(
                "#pragma stage vertex\n{GLSL}#pragma stage fragment\n"
            )),
            Some(Language::Glsl)
        );
    }

    #[test]
    fn a_preset_is_known_by_its_passes_or_what_it_builds_on() {
        assert_eq!(kind("shaders = 2\nshader0 = a.glsl\n"), Kind::Preset);
        assert_eq!(kind("shaders = \"1\"\n"), Kind::Preset);
        assert_eq!(kind("#reference \"base/crt.glslp\"\n"), Kind::Preset);
        // libretro's simple_dither.glslp.
        assert_eq!(
            kind("shaders = 1_\nshader0 = shaders/simple_dither.glsl\n"),
            Kind::Preset
        );
        assert_eq!(kind(GLSL), Kind::Pass);
    }

    /// The name can be wrong. A slang pass saved as `.glsl` is slang, and a
    /// picture renamed to `.glsl` is not a shader at all.
    #[test]
    fn what_a_file_holds_decides_not_its_name() {
        let folder = rominabox_scratch::Scratch::dir("rominabox-shader-format");
        let write = |file: &str, bytes: &[u8]| {
            let path = folder.join(file);
            fs::write(&path, bytes).unwrap();
            path
        };
        assert_eq!(
            require_runnable_pass(&write("crt.glsl", GLSL.as_bytes())),
            Ok(())
        );
        // libretro's quilez.glsl credits \"I\xf1igo Qu\xedlez\" in Latin-1.
        let latin = [b"/* I\xf1igo */\n".as_slice(), GLSL.as_bytes()].concat();
        assert_eq!(require_runnable_pass(&write("quilez.glsl", &latin)), Ok(()));
        let slang = require_runnable_pass(&write("crt-slang.glsl", SLANG.as_bytes())).unwrap_err();
        assert!(
            slang.contains("crt-slang.glsl is a slang shader"),
            "{slang}"
        );
        let cg = require_runnable_pass(&write("old.glsl", CG.as_bytes())).unwrap_err();
        assert!(cg.contains("is a Cg shader"), "{cg}");
        let picture =
            require_runnable_pass(&write("icon.glsl", b"\x89PNG\r\n\x1a\n\xff\xfe")).unwrap_err();
        assert!(picture.contains("icon.glsl is not a shader"), "{picture}");
        let preset = require_runnable_pass(&write("nested.glsl", b"shaders = 1\n")).unwrap_err();
        assert!(preset.contains("is a preset"), "{preset}");
        let _ = fs::remove_dir_all(&folder);
    }
}
