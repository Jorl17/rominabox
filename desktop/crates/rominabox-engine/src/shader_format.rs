//! The kind of shader file an author added, which we read from the file
//! itself, and the video driver for a game's shaders.
//!
//! RetroArch has three shader languages, each for its own video drivers: GLSL
//! (its OpenGL driver), slang (Vulkan, Metal, Direct3D 11 and 12, and its
//! core-profile OpenGL driver) and the retired Cg. A preset lists passes in
//! one of them. The player has both OpenGL drivers, so the shaders of an
//! exported game can be GLSL or slang, and we choose the driver from their
//! language. We refuse Cg. A file's name is only a claim, and we decide by
//! its contents. We compile nothing. We recognise a pass by the lines typical
//! of each language, and a preset by its count of passes or by the preset it
//! builds on.

use crate::packaging::ExportTarget;
use std::fs;
use std::path::Path;

/// A shader language for an exported game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// In the OpenGL driver it is compiled twice, with `VERTEX` and then
    /// `FRAGMENT` defined.
    Glsl,
    /// `#version 450`, and one file holding both stages, each after
    /// `#pragma stage`, or an `#include` of them.
    Slang,
}

/// Why a file's passes are not in a shader language for an exported game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unrunnable {
    /// Retired Cg, with entry points `main_vertex` and `main_fragment`. None
    /// of the player's drivers can use it.
    Cg,
    /// Not a shader pass in any RetroArch language.
    NotAShader,
}

/// The video driver we set for the player of an exported game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoDriver {
    /// RetroArch's OpenGL driver (`gl2.c`).
    Gl,
    /// Its core-profile OpenGL driver (`gl3.c`), with which slang is compiled
    /// through SPIR-V.
    Glcore,
}

impl VideoDriver {
    /// Its name in RetroArch's `video_driver` setting.
    pub fn name(self) -> &'static str {
        match self {
            VideoDriver::Gl => "gl",
            VideoDriver::Glcore => "glcore",
        }
    }
}

impl Language {
    /// Every shader language for an exported game.
    pub const ALL: [Language; 2] = [Language::Glsl, Language::Slang];

    /// The driver for a game whose shaders are in this language. For a game
    /// with no shaders we use the GLSL driver.
    pub fn video_driver(self) -> VideoDriver {
        match self {
            Language::Glsl => VideoDriver::Gl,
            Language::Slang => VideoDriver::Glcore,
        }
    }

    /// The name of a pass in this language. In RetroArch, a preset's language
    /// comes from its name, and a pass's language from its preset's.
    pub fn pass_extension(self) -> &'static str {
        match self {
            Language::Glsl => "glsl",
            Language::Slang => "slang",
        }
    }

    /// The name of a preset in this language.
    pub fn preset_extension(self) -> &'static str {
        match self {
            Language::Glsl => "glslp",
            Language::Slang => "slangp",
        }
    }

    /// Its name in a sentence.
    pub fn name(self) -> &'static str {
        match self {
            Language::Glsl => "GLSL",
            Language::Slang => "slang",
        }
    }
}

/// The GLSL version of a pass: the number on its `#version` line, or 110,
/// the version of a GLSL pass without that line.
pub fn glsl_version(text: &str) -> u32 {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix('#'))
        .find_map(|directive| directive.trim_start().strip_prefix("version"))
        .and_then(|rest| rest.split_whitespace().next()?.parse().ok())
        .unwrap_or(110)
}

/// The newest GLSL version that compiles in a game on `platform`, or none
/// when every version in libretro's passes compiles there. On a Mac,
/// RetroArch's OpenGL driver has an OpenGL 2.1 context, in which GLSL above
/// version 120 does not compile.
pub fn newest_glsl(platform: ExportTarget) -> Option<u32> {
    match platform {
        ExportTarget::Macos => Some(120),
        ExportTarget::Windows => None,
    }
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

/// The language a pass is written in.
pub fn language(text: &str) -> Result<Language, Unrunnable> {
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
        return Ok(Language::Glsl);
    }
    if directives.iter().any(|directive| {
        directive.starts_with("pragma stage") || directive.starts_with("version 450")
    }) {
        return Ok(Language::Slang);
    }
    if text.contains("main_fragment") || text.contains("main_vertex") {
        return Err(Unrunnable::Cg);
    }
    Err(Unrunnable::NotAShader)
}

/// A pass we can use in an exported game, and its language.
pub fn require_runnable_pass(path: &Path) -> Result<Language, String> {
    let source = text(path)?;
    if kind(&source) == Kind::Preset {
        return Err(format!(
            "{} is a preset where a shader pass belongs",
            name(path)
        ));
    }
    language(&source).map_err(|unrunnable| match unrunnable {
        Unrunnable::Cg => format!(
            "{} is a Cg shader, which exported games cannot run. Add a GLSL or slang shader.",
            name(path)
        ),
        Unrunnable::NotAShader => format!(
            "{} is not a shader RetroArch can run. Add a GLSL or slang shader.",
            name(path)
        ),
    })
}

/// The one language of every named shader, or none when there are none. In
/// RetroArch a game's shaders use one driver, and each driver one language.
pub fn one_language<'a>(
    named: impl IntoIterator<Item = (&'a str, Language)>,
) -> Result<Option<Language>, String> {
    let mut named = named.into_iter();
    let Some((first, language)) = named.next() else {
        return Ok(None);
    };
    match named.find(|(_, other)| *other != language) {
        Some((second, other)) => Err(format!(
            "{first} is {} and {second} is {}. A game's shaders must all be in one language.",
            language.name(),
            other.name()
        )),
        None => Ok(Some(language)),
    }
}

/// The file's name, for a sentence about it.
pub fn name(path: &Path) -> String {
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
        assert_eq!(language(GLSL), Ok(Language::Glsl));
        assert_eq!(
            language("#ifdef VERTEX\n#endif\n#ifdef FRAGMENT\n#endif\n"),
            Ok(Language::Glsl)
        );
        assert_eq!(language(SLANG), Ok(Language::Slang));
        // libretro's imgborder.slang, whose stages are in an included file.
        assert_eq!(
            language("#version 450\n#include \"imgborder.inc\"\n"),
            Ok(Language::Slang)
        );
        assert_eq!(language(CG), Err(Unrunnable::Cg));
        assert_eq!(language("// the pass\n"), Err(Unrunnable::NotAShader));
        assert_eq!(
            language("#if defined(VERTEXES)\n#elif defined(FRAGMENTS)\n#endif\n"),
            Err(Unrunnable::NotAShader)
        );
        // libretro's crt-blurPi.glsl, ported from slang.
        assert_eq!(
            language(&format!(
                "#pragma stage vertex\n{GLSL}#pragma stage fragment\n"
            )),
            Ok(Language::Glsl)
        );
    }

    /// We choose the driver from a game's shader language: the OpenGL driver
    /// for GLSL, and the core-profile one for slang.
    #[test]
    fn each_language_runs_on_its_own_driver() {
        assert_eq!(Language::Glsl.video_driver().name(), "gl");
        assert_eq!(Language::Slang.video_driver().name(), "glcore");
    }

    /// libretro's crt-royale passes start with `#version 130`, and a GLSL
    /// pass without a `#version` line is version 110.
    #[test]
    fn the_glsl_version_of_a_pass_is_on_its_version_line() {
        assert_eq!(glsl_version("#version 130\n\n// crt-royale\n"), 130);
        assert_eq!(glsl_version("// notice\n  # version 120\nvoid main() {}\n"), 120);
        assert_eq!(glsl_version("#version 300 es\n"), 300);
        assert_eq!(glsl_version(GLSL), 110);
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

    /// The name is only a claim. A slang pass saved as `.glsl` is slang, we
    /// refuse a Cg pass whatever its name, and a picture renamed `.glsl` is
    /// not a shader at all.
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
            Ok(Language::Glsl)
        );
        // libretro's quilez.glsl credits \"I\xf1igo Qu\xedlez\" in Latin-1.
        let latin = [b"/* I\xf1igo */\n".as_slice(), GLSL.as_bytes()].concat();
        assert_eq!(
            require_runnable_pass(&write("quilez.glsl", &latin)),
            Ok(Language::Glsl)
        );
        assert_eq!(
            require_runnable_pass(&write("crt-slang.glsl", SLANG.as_bytes())),
            Ok(Language::Slang)
        );
        let cg = require_runnable_pass(&write("old.slang", CG.as_bytes())).unwrap_err();
        assert!(cg.contains("old.slang is a Cg shader"), "{cg}");
        let picture =
            require_runnable_pass(&write("icon.glsl", b"\x89PNG\r\n\x1a\n\xff\xfe")).unwrap_err();
        assert!(picture.contains("icon.glsl is not a shader"), "{picture}");
        let preset = require_runnable_pass(&write("nested.glsl", b"shaders = 1\n")).unwrap_err();
        assert!(preset.contains("is a preset"), "{preset}");
        let _ = fs::remove_dir_all(&folder);
    }
}
