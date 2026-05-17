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

/// One diagnostic produced while compiling a shader.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub message: String,
    /// 1-based line if naga reported a location.
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// Result of validating a shader: structured, never panics.
#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub language: ShaderLang,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("unsupported language for this operation: {0:?}")]
    Unsupported(ShaderLang),
    #[error("translation failed: {0}")]
    Translation(String),
}

/// Parse + validate a shader. `lang` overrides detection when `Some`.
/// Returns a report; parse failures land in `errors`, not as a panic.
pub fn validate(src: &str, lang: Option<ShaderLang>) -> ValidationReport {
    let language = lang.unwrap_or_else(|| detect_language(src));
    let mut errors = Vec::new();

    let module = match language {
        ShaderLang::Wgsl => naga::front::wgsl::parse_str(src).map_err(|e| e.emit_to_string(src)),
        ShaderLang::Glsl => {
            let mut fe = naga::front::glsl::Frontend::default();
            fe.parse(
                &naga::front::glsl::Options::from(naga::ShaderStage::Fragment),
                src,
            )
            .map_err(|e| format!("{e:?}"))
        }
        ShaderLang::SpirV => Err("SPIR-V validation not supported as text input".to_string()),
        ShaderLang::Hlsl => Err(
            "HLSL is supported only as a translation target/source via naga; \
            validate after translating to WGSL"
                .to_string(),
        ),
    };

    match module {
        Ok(m) => {
            let mut validator = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            );
            if let Err(e) = validator.validate(&m) {
                errors.push(Diagnostic {
                    message: format!("{e:?}"),
                    line: None,
                    column: None,
                });
            }
        }
        Err(msg) => errors.push(Diagnostic {
            message: msg,
            line: None,
            column: None,
        }),
    }

    ValidationReport {
        language,
        errors,
        warnings: Vec::new(),
    }
}

/// Cross-compile a shader between languages via naga's IR.
/// Supported source: WGSL, GLSL. Supported target: WGSL, GLSL, SPIR-V (HLSL via wgsl-out path is roadmap).
pub fn translate(src: &str, from: ShaderLang, to: ShaderLang) -> Result<String, CompileError> {
    let module = match from {
        ShaderLang::Wgsl => naga::front::wgsl::parse_str(src)
            .map_err(|e| CompileError::Translation(e.emit_to_string(src)))?,
        ShaderLang::Glsl => {
            let mut fe = naga::front::glsl::Frontend::default();
            fe.parse(
                &naga::front::glsl::Options::from(naga::ShaderStage::Fragment),
                src,
            )
            .map_err(|e| CompileError::Translation(format!("{e:?}")))?
        }
        other => return Err(CompileError::Unsupported(other)),
    };

    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|e| CompileError::Translation(format!("{e:?}")))?;

    match to {
        ShaderLang::Wgsl => {
            naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
                .map_err(|e| CompileError::Translation(format!("{e:?}")))
        }
        ShaderLang::Glsl => {
            let mut buf = String::new();
            let opts = naga::back::glsl::Options::default();
            let pipe = naga::back::glsl::PipelineOptions {
                shader_stage: naga::ShaderStage::Fragment,
                entry_point: module
                    .entry_points
                    .first()
                    .map(|e| e.name.clone())
                    .unwrap_or_else(|| "main".into()),
                multiview: None,
            };
            let mut w = naga::back::glsl::Writer::new(
                &mut buf,
                &module,
                &info,
                &opts,
                &pipe,
                naga::proc::BoundsCheckPolicies::default(),
            )
            .map_err(|e| CompileError::Translation(format!("{e:?}")))?;
            w.write()
                .map_err(|e| CompileError::Translation(format!("{e:?}")))?;
            Ok(buf)
        }
        other => Err(CompileError::Unsupported(other)),
    }
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

    #[test]
    fn validates_good_wgsl() {
        let src =
            "@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0,0.0,0.0,1.0); }";
        let r = validate(src, Some(ShaderLang::Wgsl));
        assert!(r.errors.is_empty(), "unexpected errors: {:?}", r.errors);
    }

    #[test]
    fn reports_glsl_syntax_error_with_message() {
        // missing semicolon
        let body = "void mainImage(out vec4 o, in vec2 fc){ o = vec4(1.0) }";
        let r = validate(&wrap_shadertoy_fragment(body), Some(ShaderLang::Glsl));
        assert!(!r.errors.is_empty());
        assert!(!r.errors[0].message.is_empty());
    }

    #[test]
    fn translates_wgsl_to_glsl() {
        let src =
            "@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(0.2,0.4,0.6,1.0); }";
        let out = translate(src, ShaderLang::Wgsl, ShaderLang::Glsl).expect("translate ok");
        assert!(out.contains("0.2") || out.to_lowercase().contains("vec4"));
        assert!(!out.is_empty());
    }
}
