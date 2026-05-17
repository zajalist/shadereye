//! shadereye MCP server (stdio).
//!
//! API note: the pinned `rmcp` resolves to 0.1.5, whose macro surface is
//! `#[tool(tool_box)]` (on both the inherent impl and the `ServerHandler`
//! impl) with per-tool aggregate arg structs via `#[tool(aggr)]`. The plan's
//! `#[tool_router]` / `#[tool_handler]` / `Parameters<T>` / `ToolRouter<Self>`
//! names do not exist in 0.1.5; the equivalent below registers all eleven
//! tools over stdio with the same names, descriptions and image/text content
//! behavior. `tools.rs` is unchanged from the plan.

mod tools;

use rmcp::model::*;
use rmcp::schemars::{self, JsonSchema};
use rmcp::{tool, transport::stdio, ServerHandler, ServiceExt};
use serde::Deserialize;

#[derive(Clone, Default)]
struct Shadereye;

fn d512() -> u32 {
    512
}
fn d16() -> u32 {
    16
}
fn d8() -> u32 {
    8
}

#[derive(Deserialize, JsonSchema)]
struct ValidateArgs {
    source: String,
    #[serde(default)]
    lang: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct RenderArgs {
    source: String,
    #[serde(default)]
    lang: Option<String>,
    #[serde(default = "d512")]
    width: u32,
    #[serde(default = "d512")]
    height: u32,
    #[serde(default)]
    time: f32,
}

#[derive(Deserialize, JsonSchema)]
struct RenderAnimationArgs {
    source: String,
    #[serde(default)]
    lang: Option<String>,
    #[serde(default = "d512")]
    width: u32,
    #[serde(default = "d512")]
    height: u32,
    #[serde(default)]
    t0: f32,
    #[serde(default)]
    t1: f32,
    #[serde(default = "d8")]
    frames: u32,
}

#[derive(Deserialize, JsonSchema)]
struct VisualizeArgs {
    source: String,
    expr: String,
    #[serde(default)]
    mode: String,
    #[serde(default = "d512")]
    width: u32,
    #[serde(default = "d512")]
    height: u32,
}

#[derive(Deserialize, JsonSchema)]
struct ProbeArgs {
    source: String,
    #[serde(default)]
    lang: Option<String>,
    #[serde(default = "d512")]
    width: u32,
    #[serde(default = "d512")]
    height: u32,
    #[serde(default)]
    coords: Vec<(u32, u32)>,
}

#[derive(Deserialize, JsonSchema)]
struct DiffArgs {
    source_a: String,
    source_b: String,
    #[serde(default = "d512")]
    width: u32,
    #[serde(default = "d512")]
    height: u32,
    #[serde(default)]
    tol: f32,
}

#[derive(Deserialize, JsonSchema)]
struct TranslateArgs {
    source: String,
    from: String,
    to: String,
}

#[derive(Deserialize, JsonSchema)]
struct RunBrowserArgs {
    source: String,
    #[serde(default = "d512")]
    width: u32,
    #[serde(default = "d512")]
    height: u32,
    #[serde(default = "d16")]
    frames: u32,
}

#[derive(Deserialize, JsonSchema)]
struct ShadertoyGetArgs {
    id_or_url: String,
}

#[derive(Deserialize, JsonSchema)]
struct ShadertoySearchArgs {
    query: String,
}

#[derive(Deserialize, JsonSchema)]
struct LookupReferenceArgs {
    query: String,
}

fn img_result(json: serde_json::Value, png_b64: String) -> CallToolResult {
    let mut content = vec![Content::text(json.to_string())];
    if !png_b64.is_empty() {
        content.push(Content::image(png_b64, "image/png".to_string()));
    }
    CallToolResult::success(content)
}

fn text_result(json: serde_json::Value) -> CallToolResult {
    CallToolResult::success(vec![Content::text(json.to_string())])
}

// rmcp 0.1.5 implements `IntoCallToolResult` for `Result<CallToolResult,
// rmcp::Error>` (and for `T: IntoContents`) but NOT for a bare
// `CallToolResult`, so every tool method returns the `Result` form.
type ToolResult = Result<CallToolResult, rmcp::Error>;

#[tool(tool_box)]
impl Shadereye {
    #[tool(description = "Validate a shader (GLSL/WGSL); returns structured errors/warnings.")]
    async fn validate_shader(&self, #[tool(aggr)] a: ValidateArgs) -> ToolResult {
        Ok(text_result(tools::validate(&a.source, a.lang.as_deref())))
    }

    #[tool(description = "Render a Shadertoy-style/GLSL/WGSL fragment shader to a PNG image.")]
    async fn render_shader(&self, #[tool(aggr)] a: RenderArgs) -> ToolResult {
        let (j, png) = tools::render(&a.source, a.lang.as_deref(), a.width, a.height, a.time);
        Ok(img_result(j, png))
    }

    #[tool(
        description = "Render an animated shader as a horizontal filmstrip PNG across a time range."
    )]
    async fn render_animation(&self, #[tool(aggr)] a: RenderAnimationArgs) -> ToolResult {
        let (j, png) = tools::render_animation(
            &a.source,
            a.lang.as_deref(),
            a.width,
            a.height,
            a.t0,
            a.t1,
            a.frames,
        );
        Ok(img_result(j, png))
    }

    #[tool(
        description = "Visualize a GLSL sub-expression as color (grayscale/rg/rgb/normalized/heatmap)."
    )]
    async fn visualize_expression(&self, #[tool(aggr)] a: VisualizeArgs) -> ToolResult {
        let (j, png) = tools::visualize(&a.source, &a.expr, &a.mode, a.width, a.height);
        Ok(img_result(j, png))
    }

    #[tool(description = "Probe exact RGBA values at pixel coordinates plus a zoomed crop image.")]
    async fn probe_pixels(&self, #[tool(aggr)] a: ProbeArgs) -> ToolResult {
        let (j, png) = tools::probe(&a.source, a.lang.as_deref(), a.width, a.height, &a.coords);
        Ok(img_result(j, png))
    }

    #[tool(description = "Render two shaders and diff them; returns metrics and a diff PNG.")]
    async fn diff_shaders(&self, #[tool(aggr)] a: DiffArgs) -> ToolResult {
        let (j, png) = tools::diff(&a.source_a, &a.source_b, a.width, a.height, a.tol);
        Ok(img_result(j, png))
    }

    #[tool(description = "Translate a shader between languages (glsl/wgsl/hlsl/spirv).")]
    async fn translate_shader(&self, #[tool(aggr)] a: TranslateArgs) -> ToolResult {
        Ok(text_result(tools::translate(&a.source, &a.from, &a.to)))
    }

    #[tool(
        description = "Run a shader in headless Chromium (WebGL2); capture console/GL transcript + screenshot."
    )]
    async fn run_in_browser(&self, #[tool(aggr)] a: RunBrowserArgs) -> ToolResult {
        let (j, png) = tools::run_browser(&a.source, a.width, a.height, a.frames);
        Ok(img_result(j, png))
    }

    #[tool(description = "Fetch a Shadertoy shader by id or URL.")]
    async fn shadertoy_get(&self, #[tool(aggr)] a: ShadertoyGetArgs) -> ToolResult {
        Ok(text_result(tools::shadertoy_get(&a.id_or_url)))
    }

    #[tool(description = "Search Shadertoy for shader ids matching a query.")]
    async fn shadertoy_search(&self, #[tool(aggr)] a: ShadertoySearchArgs) -> ToolResult {
        Ok(text_result(tools::shadertoy_search(&a.query)))
    }

    #[tool(description = "Look up offline GLSL/WGSL/Shadertoy reference + gotcha entries.")]
    async fn lookup_reference(&self, #[tool(aggr)] a: LookupReferenceArgs) -> ToolResult {
        Ok(text_result(tools::lookup_reference(&a.query)))
    }
}

#[tool(tool_box)]
impl ServerHandler for Shadereye {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "shadereye".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some("Shader perception/debug/test/translate tools for LLMs.".into()),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = Shadereye.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
