//! Shader language detection, validation, translation, and Shadertoy wrapping.

use serde::Serialize;

/// A shader source language understood by shadereye.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ShaderLang {
    Glsl,
    Wgsl,
    Hlsl,
    SpirV,
}

/// Heuristically detect the shader language from source text.
/// Used when the caller does not pass an explicit language.
pub fn detect_language(src: &str) -> ShaderLang {
    if src.contains("@vertex")
        || src.contains("@fragment")
        || src.contains("fn main") && src.contains("->")
    {
        return ShaderLang::Wgsl;
    }
    if src.contains("SV_TARGET") || src.contains("cbuffer") || src.contains("Texture2D") {
        return ShaderLang::Hlsl;
    }
    ShaderLang::Glsl
}

/// The uniform/IO preamble injected before a Shadertoy `mainImage` body so the
/// result is a complete GLSL 460 fragment shader the wgpu/naga pipeline accepts.
const SHADERTOY_PREAMBLE: &str = r#"#version 460
layout(location = 0) out vec4 _st_fragColor;
layout(set = 0, binding = 0) uniform ShadereyeUniforms {
    vec4 iResolution;   // xy = pixels, z = 1.0
    vec4 iMouse;        // xy = current, zw = click
    float iTime;
    float iTimeDelta;
    int   iFrame;
    float _pad0;
} U;
#define iResolution (U.iResolution.xyz)
#define iMouse (U.iMouse)
#define iTime (U.iTime)
#define iTimeDelta (U.iTimeDelta)
#define iFrame (U.iFrame)
"#;

const SHADERTOY_MAIN: &str = r#"
void main() {
    vec4 _c = vec4(0.0);
    mainImage(_c, gl_FragCoord.xy);
    _st_fragColor = _c;
}
"#;

/// Wrap a Shadertoy-style fragment body (containing `mainImage`) into a complete
/// GLSL 460 fragment shader. If the source already declares `#version`, it is a
/// complete shader and returned unchanged.
pub fn wrap_shadertoy_fragment(src: &str) -> String {
    if src.trim_start().starts_with("#version") {
        return src.to_string();
    }
    format!("{SHADERTOY_PREAMBLE}\n{src}\n{SHADERTOY_MAIN}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_wgsl_by_attributes() {
        let src = "@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }";
        assert_eq!(detect_language(src), ShaderLang::Wgsl);
    }

    #[test]
    fn detects_hlsl_by_semantics() {
        let src = "float4 main() : SV_TARGET { return float4(1,1,1,1); }";
        assert_eq!(detect_language(src), ShaderLang::Hlsl);
    }

    #[test]
    fn defaults_to_glsl() {
        let src = "void mainImage(out vec4 c, in vec2 p){ c = vec4(1.0); }";
        assert_eq!(detect_language(src), ShaderLang::Glsl);
    }

    #[test]
    fn wraps_shadertoy_body_into_full_glsl() {
        let body =
            "void mainImage(out vec4 o, in vec2 fc){ o = vec4(fc/iResolution.xy, 0.0, 1.0); }";
        let full = wrap_shadertoy_fragment(body);
        assert!(full.starts_with("#version 460"));
        assert!(full.contains("uniform")); // uniform block present
        assert!(full.contains("iResolution"));
        assert!(full.contains("iTime"));
        assert!(full.contains("void main()"));
        assert!(full.contains("mainImage("));
        // user body must be included verbatim
        assert!(full.contains("o = vec4(fc/iResolution.xy, 0.0, 1.0);"));
    }

    #[test]
    fn does_not_double_wrap_complete_glsl() {
        let already = "#version 460\nout vec4 c;\nvoid main(){ c = vec4(1.0); }";
        assert_eq!(wrap_shadertoy_fragment(already), already);
    }
}
