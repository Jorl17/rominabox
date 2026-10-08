//! The light and contrast of the picture, which the player sets on VIDEO in
//! the game's Options.
//!
//! We write a pass in the game's shader language, with one shader parameter
//! for each setting, as we name them in `settings.inc`
//! (`RIB_SETTING_PARAMETER`), and list its preset in `shaders.cfg`. In the
//! player we leave the pass out while both are at 1.0, and otherwise apply
//! it after the shader the player chose (`menu/drivers/rmlui/video.c`).

use crate::player_settings::Key;
use crate::shader_format::Language;
use crate::shader_source::Parameter;
use std::path::PathBuf;

/// Where we write the pass among the game's menu assets.
const FOLDER: &str = "shaders/video";

/// The pass. We multiply the light of each colour by the brightness, in
/// linear light, so black stays black and white clips, and then move each
/// colour away from mid-grey by the contrast. `BRIGHTNESS` and `CONTRAST`
/// stand for the parameters' names.
const BODY: &str = "vec3 colour = COMPAT_TEXTURE(Texture, TEX0.xy).rgb;
    vec3 light = mix(pow((colour + 0.055) / 1.055, vec3(2.4)), colour / 12.92, vec3(lessThanEqual(colour, vec3(0.04045))));
    light = min(light * BRIGHTNESS, vec3(1.0));
    colour = mix(1.055 * pow(light, vec3(1.0 / 2.4)) - 0.055, light * 12.92, vec3(lessThanEqual(light, vec3(0.0031308))));
    FragColor = vec4(clamp((colour - 0.5) * CONTRAST + 0.5, 0.0, 1.0), 1.0);";

/// The pass and its preset in `language`, as (path in the menu assets, text),
/// and the path of the preset, which we name in `shaders.cfg`.
pub fn files(language: Language) -> (Vec<(PathBuf, String)>, String) {
    let parameter = |key: Key, label| {
        let values = crate::player_settings::positions(key);
        Parameter {
            id: crate::player_settings::parameter(key),
            label,
            initial: 1.0,
            minimum: values.iter().copied().fold(f32::INFINITY, f32::min),
            maximum: values.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            step: 0.01,
        }
    };
    let parameters = [
        parameter(Key::VideoBrightness, "Brightness"),
        parameter(Key::VideoContrast, "Contrast"),
    ];
    let body = BODY
        .replace("BRIGHTNESS", parameters[0].id)
        .replace("CONTRAST", parameters[1].id);
    let pass = format!("video.{}", language.pass_extension());
    let preset = format!("video.{}", language.preset_extension());
    let folder = PathBuf::from(FOLDER);
    (
        vec![
            (folder.join(&pass), crate::shader_source::pass_with(language, &body, &parameters)),
            (folder.join(&preset), crate::shader_source::preset(&pass)),
        ],
        format!("{FOLDER}/{preset}"),
    )
}
