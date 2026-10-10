//! How we switch off the guess of interlacing from the height of the picture
//! in a shader, for a console whose core's pictures are whole frames.
//!
//! In several CRT shaders, a picture of a television's interlaced height
//! counts as two fields woven together, and each frame shows one field with
//! the other field's lines made from their neighbours. Flycast's pictures are
//! whole frames at that height, so half their lines are lost. Each such
//! preset in the catalog names its switch for the guess, in each language
//! with one. For a console whose core's pictures are whole frames
//! (`systems::Frames`), the row of the preset names a preset of ours beside
//! it, which references the library preset and switches the guess off.

use super::{Language, ResolvedShader};
use crate::systems::Frames;
use serde::Deserialize;

/// A catalog preset's switch for its guess, in each language that has one.
#[derive(Clone, Debug, Deserialize)]
pub(super) struct CatalogInterlacing {
    glsl: Option<Switch>,
    slang: Option<Switch>,
}

impl CatalogInterlacing {
    pub(super) fn switch(&self, language: Language) -> Option<Switch> {
        match language {
            Language::Glsl => self.glsl.clone(),
            Language::Slang => self.slang.clone(),
        }
    }
}

/// The parameter of a preset for its guess, and its value for off.
#[derive(Clone, Debug, Deserialize)]
pub(super) struct Switch {
    parameter: String,
    off: f32,
}

#[cfg(test)]
impl Switch {
    fn parameter(&self) -> &str {
        &self.parameter
    }
}

/// The name of our preset in the folder of a shader's row, before the
/// extension of the game's language.
const WHOLE_FRAMES: &str = "whole-frames";

/// The shaders of a game in `language`, for a console whose core's pictures
/// are `frames`. For whole frames, we write our preset beside each preset
/// with a switch for its guess, and name ours in its row.
pub(super) fn for_frames(resolved: &mut [ResolvedShader], frames: Frames, language: Language) {
    if frames != Frames::Whole {
        return;
    }
    for shader in resolved {
        let Some(switch) = shader.interlacing.take() else {
            continue;
        };
        let from_shaders = shader
            .relative_preset
            .strip_prefix("shaders/")
            .expect("a library preset is in the game's shaders folder");
        let file = format!("{WHOLE_FRAMES}.{}", language.preset_extension());
        let text = format!(
            "#reference \"../{from_shaders}\"\n{} = \"{:.6}\"\n",
            switch.parameter, switch.off
        );
        shader.relative_preset = format!("{}/{file}", shader.folder().to_string_lossy().replace('\\', "/"));
        shader.written.push((file, text));
    }
}

#[cfg(test)]
mod tests;
