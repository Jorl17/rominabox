//! The source of a catalog preset, in the shader language of its game.
//!
//! A catalog preset is one fragment body (`integrations/shaders/catalog.json`)
//! written with the names in RetroArch's GLSL passes: `Texture`, `TEX0`,
//! `TextureSize`, `InputSize`, `OutputSize`, `FrameCount`, `FrameDirection`,
//! `COMPAT_TEXTURE` and `FragColor`. For each language, we define those names
//! in a pass around the same body, so we write each filter once, and a game
//! with slang shaders gets the same filter as a game with GLSL shaders.

use crate::shader_format::Language;

/// A value of a pass that the player can change while the game runs, as we
/// declare it in a `#pragma parameter` line. In the fragment body we read it
/// by its id.
pub struct Parameter<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub initial: f32,
    pub minimum: f32,
    pub maximum: f32,
    pub step: f32,
}

/// A pass with `fragment_body` in `language`.
pub fn pass(language: Language, fragment_body: &str) -> String {
    pass_with(language, fragment_body, &[])
}

/// A pass with `fragment_body` in `language`, with `parameters`.
pub fn pass_with(language: Language, fragment_body: &str, parameters: &[Parameter]) -> String {
    let pragmas: String = parameters
        .iter()
        .map(|p| {
            format!(
                "#pragma parameter {} \"{}\" {} {} {} {}\n",
                p.id, p.label, p.initial, p.minimum, p.maximum, p.step
            )
        })
        .collect();
    match language {
        Language::Glsl => {
            let uniforms = if parameters.is_empty() {
                String::new()
            } else {
                let declared: String = parameters
                    .iter()
                    .map(|p| format!("uniform COMPAT_PRECISION float {};\n", p.id))
                    .collect();
                let fixed: String = parameters
                    .iter()
                    .map(|p| format!("#define {} {:?}\n", p.id, p.initial))
                    .collect();
                format!("#ifdef PARAMETER_UNIFORM\n{declared}#else\n{fixed}#endif\n")
            };
            format!("{pragmas}{}", glsl(fragment_body, &uniforms))
        }
        Language::Slang => {
            let members: String = parameters.iter().map(|p| format!("    float {};\n", p.id)).collect();
            let locals: String = parameters
                .iter()
                .map(|p| format!("    float {id} = params.{id};\n", id = p.id))
                .collect();
            slang(fragment_body, &pragmas, &members, &locals)
        }
    }
}

/// A preset of the one pass `pass_file`, beside it.
pub fn preset(pass_file: &str) -> String {
    format!("shaders = 1\nshader0 = {pass_file}\nfilter_linear0 = false\n")
}

fn glsl(fragment_body: &str, uniforms: &str) -> String {
    format!(
        r#"/* Original ROM-in-a-Box preset. RetroArch's OpenGL driver compiles this
 * twice, once with VERTEX defined and once with FRAGMENT defined. */
#if defined(VERTEX)
#if __VERSION__ >= 130
#define COMPAT_VARYING out
#define COMPAT_ATTRIBUTE in
#define COMPAT_TEXTURE texture
#else
#define COMPAT_VARYING varying
#define COMPAT_ATTRIBUTE attribute
#define COMPAT_TEXTURE texture2D
#endif
#ifdef GL_ES
#define COMPAT_PRECISION mediump
#else
#define COMPAT_PRECISION
#endif
COMPAT_ATTRIBUTE vec4 VertexCoord;
COMPAT_ATTRIBUTE vec4 COLOR;
COMPAT_ATTRIBUTE vec4 TexCoord;
COMPAT_VARYING vec4 COL0;
COMPAT_VARYING vec4 TEX0;
uniform mat4 MVPMatrix;
uniform COMPAT_PRECISION int FrameDirection;
uniform COMPAT_PRECISION int FrameCount;
uniform COMPAT_PRECISION vec2 OutputSize;
uniform COMPAT_PRECISION vec2 TextureSize;
uniform COMPAT_PRECISION vec2 InputSize;
void main()
{{
    gl_Position = MVPMatrix * VertexCoord;
    COL0 = COLOR;
    TEX0.xy = TexCoord.xy;
}}
#elif defined(FRAGMENT)
#if __VERSION__ >= 130
#define COMPAT_VARYING in
#define COMPAT_TEXTURE texture
out vec4 FragColor;
#else
#define COMPAT_VARYING varying
#define FragColor gl_FragColor
#define COMPAT_TEXTURE texture2D
#endif
#ifdef GL_ES
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
#define COMPAT_PRECISION mediump
#else
#define COMPAT_PRECISION
#endif
uniform COMPAT_PRECISION int FrameDirection;
uniform COMPAT_PRECISION int FrameCount;
uniform COMPAT_PRECISION vec2 OutputSize;
uniform COMPAT_PRECISION vec2 TextureSize;
uniform COMPAT_PRECISION vec2 InputSize;
{uniforms}uniform sampler2D Texture;
COMPAT_VARYING vec4 TEX0;
void main()
{{
    {fragment_body}
}}
#endif
"#
    )
}

/// In slang the picture is `Source`, its coordinates cover only the frame,
/// and `SourceSize` is the frame's size. In GLSL, `TextureSize` is the size
/// of the texture, which can be larger, with `TEX0` scaled to match. A
/// coordinate times the size is the same pixel in both.
fn slang(fragment_body: &str, pragmas: &str, members: &str, locals: &str) -> String {
    format!(
        r#"#version 450
{pragmas}/* Original ROM-in-a-Box preset. RetroArch compiles slang through SPIR-V, and
 * this is the fragment body of the GLSL form, with its names defined below. */
layout(push_constant) uniform Push
{{
    vec4 SourceSize;
    vec4 OutputSize;
    uint FrameCount;
    int FrameDirection;
{members}}} params;

layout(std140, set = 0, binding = 0) uniform UBO
{{
    mat4 MVP;
}} global;

#pragma stage vertex
layout(location = 0) in vec4 Position;
layout(location = 1) in vec2 TexCoord;
layout(location = 0) out vec2 vTexCoord;

void main()
{{
    gl_Position = global.MVP * Position;
    vTexCoord = TexCoord;
}}

#pragma stage fragment
layout(location = 0) in vec2 vTexCoord;
layout(location = 0) out vec4 FragColor;
layout(set = 0, binding = 2) uniform sampler2D Source;

#define Texture Source
#define COMPAT_TEXTURE(sampler, coordinate) texture(sampler, coordinate)

void main()
{{
    vec4 TEX0 = vec4(vTexCoord, 0.0, 0.0);
    vec2 TextureSize = params.SourceSize.xy;
    vec2 InputSize = params.SourceSize.xy;
    vec2 OutputSize = params.OutputSize.xy;
    int FrameCount = int(params.FrameCount);
    int FrameDirection = params.FrameDirection;
{locals}    {fragment_body}
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader_format::{kind, language, Kind};

    const BODY: &str = "FragColor = COMPAT_TEXTURE(Texture, TEX0.xy);";

    /// We read each form back as the language we wrote it in, with the body
    /// inside it.
    #[test]
    fn each_form_is_a_pass_in_its_language() {
        for wanted in [Language::Glsl, Language::Slang] {
            let source = pass(wanted, BODY);
            assert_eq!(language(&source), Ok(wanted));
            assert_eq!(kind(&source), Kind::Pass);
            assert!(source.contains(BODY));
        }
        assert_eq!(kind(&preset("scanlines.slang")), Kind::Preset);
    }
}
