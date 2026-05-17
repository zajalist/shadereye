//! shadereye MCP server (stdio).

mod tools;

use rmcp::handler::server::ServerHandler;
use rmcp::model::*;
use rmcp::{transport::stdio, ServiceExt};

#[derive(Clone, Default)]
struct Shadereye;

impl ServerHandler for Shadereye {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "shadereye".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "Shader tools for LLMs: validate_shader, render_shader, render_animation, \
                 visualize_expression, probe_pixels, diff_shaders, translate_shader, \
                 run_in_browser, shadertoy_get, shadertoy_search, lookup_reference."
                    .into(),
            ),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = Shadereye.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
