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
}
