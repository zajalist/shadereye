//! Tool implementations: thin adapters over the shadereye libraries.
//! Each returns (text_json, optional_png) so the MCP layer can format content.

use base64::Engine;

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn validate(source: &str, lang: Option<&str>) -> serde_json::Value {
    let l = lang.and_then(parse_lang);
    let r = shadereye_compile::validate(source, l);
    serde_json::to_value(r).unwrap()
}

pub fn parse_lang(s: &str) -> Option<shadereye_compile::ShaderLang> {
    use shadereye_compile::ShaderLang::*;
    match s.to_lowercase().as_str() {
        "glsl" => Some(Glsl),
        "wgsl" => Some(Wgsl),
        "hlsl" => Some(Hlsl),
        "spirv" | "spv" => Some(SpirV),
        _ => None,
    }
}

/// Returns (json_summary, png_base64).
pub fn render(
    source: &str,
    lang: Option<&str>,
    w: u32,
    h: u32,
    time: f32,
) -> (serde_json::Value, String) {
    let p = shadereye_render::RenderParams {
        source: source.into(),
        lang: lang.and_then(parse_lang),
        width: w,
        height: h,
        time,
        mouse: [0.0; 4],
    };
    match shadereye_render::render(&p) {
        Ok(o) => (
            serde_json::json!({"ok":true,"backend":o.backend,"width":o.width,"height":o.height}),
            b64(&o.png),
        ),
        Err(e) => (
            serde_json::json!({"ok":false,"error":e.to_string()}),
            String::new(),
        ),
    }
}

pub fn render_animation(
    source: &str,
    lang: Option<&str>,
    w: u32,
    h: u32,
    t0: f32,
    t1: f32,
    frames: u32,
) -> (serde_json::Value, String) {
    let base = shadereye_render::RenderParams {
        source: source.into(),
        lang: lang.and_then(parse_lang),
        width: w,
        height: h,
        time: t0,
        mouse: [0.0; 4],
    };
    match shadereye_render::render_animation(&base, t0, t1, frames) {
        Ok(o) => (
            serde_json::json!({"ok":true,"frames":frames,"backend":o.backend}),
            b64(&o.png),
        ),
        Err(e) => (
            serde_json::json!({"ok":false,"error":e.to_string()}),
            String::new(),
        ),
    }
}

pub fn visualize(
    source: &str,
    expr: &str,
    mode: &str,
    w: u32,
    h: u32,
) -> (serde_json::Value, String) {
    use shadereye_render::VisualizeMode::*;
    let m = match mode {
        "rg" => RgVec2,
        "rgb" => RgbVec3,
        "normalized" => Normalized,
        "heatmap" => Heatmap,
        _ => GrayscaleFloat,
    };
    match shadereye_render::visualize_expression(source, expr, m, w, h) {
        Ok(o) => (serde_json::json!({"ok":true,"mode":mode}), b64(&o.png)),
        Err(e) => (
            serde_json::json!({"ok":false,"error":e.to_string()}),
            String::new(),
        ),
    }
}

pub fn probe(
    source: &str,
    lang: Option<&str>,
    w: u32,
    h: u32,
    coords: &[(u32, u32)],
) -> (serde_json::Value, String) {
    let p = shadereye_render::RenderParams {
        source: source.into(),
        lang: lang.and_then(parse_lang),
        width: w,
        height: h,
        time: 0.0,
        mouse: [0.0; 4],
    };
    match shadereye_render::probe_pixels(&p, coords) {
        Ok(r) => (serde_json::to_value(&r).unwrap(), b64(&r.crop_png)),
        Err(e) => (
            serde_json::json!({"ok":false,"error":e.to_string()}),
            String::new(),
        ),
    }
}

pub fn diff(
    source_a: &str,
    source_b: &str,
    w: u32,
    h: u32,
    tol: f32,
) -> (serde_json::Value, String) {
    let mk = |s: &str| shadereye_render::RenderParams {
        source: s.into(),
        lang: None,
        width: w,
        height: h,
        time: 0.0,
        mouse: [0.0; 4],
    };
    let a = match shadereye_render::render(&mk(source_a)) {
        Ok(o) => o,
        Err(e) => {
            return (
                serde_json::json!({"ok":false,"error":e.to_string()}),
                String::new(),
            )
        }
    };
    let b = match shadereye_render::render(&mk(source_b)) {
        Ok(o) => o,
        Err(e) => {
            return (
                serde_json::json!({"ok":false,"error":e.to_string()}),
                String::new(),
            )
        }
    };
    match shadereye_render::diff::diff_images(w, h, &a.rgba, &b.rgba, tol) {
        Ok(d) => (serde_json::to_value(&d).unwrap(), b64(&d.diff_png)),
        Err(_) => (
            serde_json::json!({"ok":false,"error":"size mismatch"}),
            String::new(),
        ),
    }
}

pub fn translate(source: &str, from: &str, to: &str) -> serde_json::Value {
    match (parse_lang(from), parse_lang(to)) {
        (Some(f), Some(t)) => match shadereye_compile::translate(source, f, t) {
            Ok(s) => serde_json::json!({"ok":true,"source":s}),
            Err(e) => serde_json::json!({"ok":false,"error":e.to_string()}),
        },
        _ => serde_json::json!({"ok":false,"error":"unknown language"}),
    }
}

pub fn run_browser(source: &str, w: u32, h: u32, frames: u32) -> (serde_json::Value, String) {
    match shadereye_browser::run_in_browser(&shadereye_browser::BrowserRunParams {
        source: source.into(),
        width: w,
        height: h,
        frames,
    }) {
        Ok(t) => (
            serde_json::json!({
                "ok":true,"compiled_ok":t.compiled_ok,
                "console":t.console,"exceptions":t.exceptions,"gl_errors":t.gl_errors
            }),
            b64(&t.screenshot_png),
        ),
        Err(e) => (
            serde_json::json!({"ok":false,"error":e.to_string()}),
            String::new(),
        ),
    }
}

pub fn shadertoy_get(id_or_url: &str) -> serde_json::Value {
    match shadereye_shadertoy::get(id_or_url) {
        Ok(s) => {
            serde_json::json!({"ok":true,"id":s.id,"title":s.title,"author":s.author,"image_code":s.image_code})
        }
        Err(e) => serde_json::json!({"ok":false,"error":e.to_string()}),
    }
}

pub fn shadertoy_search(q: &str) -> serde_json::Value {
    match shadereye_shadertoy::search(q) {
        Ok(ids) => serde_json::json!({"ok":true,"ids":ids}),
        Err(e) => serde_json::json!({"ok":false,"error":e.to_string()}),
    }
}

pub fn lookup_reference(q: &str) -> serde_json::Value {
    serde_json::json!({"ok":true,"entries": shadereye_reference::lookup(q)})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_tool_returns_errors_field() {
        let v = validate(
            "@fragment fn f()->@location(0) vec4<f32>{return vec4<f32>(1.0);}",
            Some("wgsl"),
        );
        assert!(v.get("errors").is_some());
    }

    #[test]
    fn render_tool_returns_png_for_good_shader() {
        let (j, png) = render(
            "void mainImage(out vec4 o,in vec2 fc){o=vec4(0.0,1.0,0.0,1.0);}",
            None,
            16,
            16,
            0.0,
        );
        assert_eq!(j["ok"], true);
        assert!(!png.is_empty());
    }

    #[test]
    fn translate_tool_round_trips_wgsl_to_glsl() {
        let v = translate(
            "@fragment fn f()->@location(0) vec4<f32>{return vec4<f32>(0.1,0.2,0.3,1.0);}",
            "wgsl",
            "glsl",
        );
        assert_eq!(v["ok"], true, "{v}");
    }

    #[test]
    fn lookup_reference_tool_finds_entry() {
        let v = lookup_reference("smoothstep");
        assert!(v["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "smoothstep"));
    }
}
