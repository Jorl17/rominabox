//! The brightness parameter of a shader. In the game, for a brightness above
//! 100 %, we raise this parameter first and add the rest of the light with our
//! pass (`crate::video`).

use crate::shader_format::Language;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The brightness parameter of a shader, with the light at each value in
/// `table`, as a multiple of the light at the value in the shader, from 1.0
/// up. For a bundled preset we measured the table once with
/// scripts/measure_shader_brightness.py and keep it in the catalogue. For an
/// author's shader we make it from the declaration of the parameter
/// ([`declared`]).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct BrightnessControl {
    pub parameter: String,
    #[serde(default)]
    pub table: Vec<(f32, f32)>,
}

impl BrightnessControl {
    /// The text we write in `shaders.cfg`, which is the parameter, then each
    /// light and its value as `light:value`.
    pub(super) fn text(&self) -> String {
        std::iter::once(self.parameter.clone())
            .chain(self.table.iter().map(|(light, value)| format!("{light}:{value}")))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The brightness parameter of a bundled preset, as the catalogue declares
/// it: the table we measured from the slang version where there is one, and,
/// for a preset with other light in its GLSL version, the table we measured
/// from that version.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CatalogBrightness {
    pub(super) parameter: String,
    #[serde(default)]
    table: Vec<(f32, f32)>,
    #[serde(default)]
    glsl_table: Option<Vec<(f32, f32)>>,
}

impl CatalogBrightness {
    /// The control for a game in `language`, or None when the preset has no
    /// table.
    pub(super) fn control(&self, language: Language) -> Option<BrightnessControl> {
        let table = match (language, &self.glsl_table) {
            (Language::Glsl, Some(glsl)) => glsl,
            _ => &self.table,
        };
        (!table.is_empty()).then(|| BrightnessControl {
            parameter: self.parameter.clone(),
            table: table.clone(),
        })
    }
}

/// Words in the name or the description of a brightness parameter, in lower
/// case with letters and digits only, as we compare them ([`compared`]).
const LIGHT_WORDS: [&str; 4] = ["brightness", "brightboost", "luminance", "gain"];

/// The extensions of a preset and of a file of parameter values, which are
/// the files with the values of a preset's parameters. In the game, we give
/// the author's preset the extension of its language.
const PRESET_EXTENSIONS: [&str; 3] = ["glslp", "slangp", "params"];

/// A parameter, from its `#pragma parameter` line in a pass.
struct Declared {
    id: String,
    description: String,
    initial: f32,
    maximum: f32,
}

/// The brightness parameter of an author's shader, from the declarations in
/// its files. We only read the files and run nothing.
///
/// We look for a parameter whose name or description contains one of
/// [`LIGHT_WORDS`], with a value in the shader above 0 and below its maximum.
/// The value in the shader is the one in the author's preset, or else the
/// parameter's default. A parameter named or described as plain "brightness"
/// comes first, then the first such parameter in the passes. We take the
/// light to rise in proportion to the value, so twice the value is twice the
/// light, up to the maximum. `files` are the shader's files, each as (source,
/// path in the game).
pub(super) fn declared(files: &[(PathBuf, String)]) -> Option<BrightnessControl> {
    let mut sources = Vec::new();
    let mut presets = Vec::new();
    for (source, name) in files {
        let Ok(text) = std::fs::read_to_string(source) else {
            continue;
        };
        if is_preset(Path::new(name)) {
            presets.push(text);
        } else {
            sources.push(text);
        }
    }
    let mut parameters: Vec<Declared> = Vec::new();
    for parameter in sources.iter().flat_map(|text| text.lines()).filter_map(parameter) {
        if !parameters.iter().any(|known| known.id == parameter.id) {
            parameters.push(parameter);
        }
    }
    let candidates: Vec<(&Declared, f32)> = parameters
        .iter()
        .map(|parameter| (parameter, preset_value(&presets, &parameter.id).unwrap_or(parameter.initial)))
        .filter(|(parameter, own)| *own > 0.0 && parameter.maximum > *own && adds_light(parameter))
        .collect();
    let plain = |parameter: &Declared| {
        compared(&parameter.id) == "brightness" || compared(&parameter.description) == "brightness"
    };
    let (chosen, own) = candidates
        .iter()
        .find(|(parameter, _)| plain(parameter))
        .or_else(|| candidates.first())?;
    let light = (chosen.maximum / own * 1000.0).round() / 1000.0;
    Some(BrightnessControl {
        parameter: chosen.id.clone(),
        table: vec![(1.0, *own), (light, chosen.maximum)],
    })
}

/// Whether one of `files` contains a `#pragma parameter` line for parameter
/// `id`.
#[cfg(test)]
pub(super) fn declares(files: &[(PathBuf, String)], id: &str) -> bool {
    files
        .iter()
        .filter_map(|(source, _)| std::fs::read_to_string(source).ok())
        .any(|text| text.lines().filter_map(parameter).any(|parameter| parameter.id == id))
}

fn is_preset(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| PRESET_EXTENSIONS.iter().any(|name| name.eq_ignore_ascii_case(extension)))
}

/// The parameter in a line `#pragma parameter ID "Description" initial
/// minimum maximum step`.
fn parameter(line: &str) -> Option<Declared> {
    let rest = line.trim().strip_prefix("#pragma")?.trim_start().strip_prefix("parameter")?;
    let (id, rest) = rest.trim_start().split_once(char::is_whitespace)?;
    let (description, rest) = rest.trim_start().strip_prefix('"')?.split_once('"')?;
    let numbers: Vec<f32> = rest.split_whitespace().map_while(|number| number.parse().ok()).collect();
    let &[initial, _, maximum, ..] = numbers.as_slice() else {
        return None;
    };
    Some(Declared {
        id: id.to_string(),
        description: description.to_string(),
        initial,
        maximum,
    })
}

/// The value of parameter `id` in the first of `presets` with a line
/// `id = "value"`. The author's preset comes first.
fn preset_value(presets: &[String], id: &str) -> Option<f32> {
    presets.iter().flat_map(|text| text.lines()).find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == id).then(|| value.trim().trim_matches('"').parse().ok())?
    })
}

fn adds_light(parameter: &Declared) -> bool {
    let (id, description) = (compared(&parameter.id), compared(&parameter.description));
    LIGHT_WORDS.iter().any(|word| id.contains(word) || description.contains(word))
}

/// `text` in lower case with only its letters and digits, so `BRIGHT_BOOST`,
/// `brightBoost` and "Bright Boost" are all `brightboost`.
fn compared(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}
