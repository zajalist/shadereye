//! Minimal Shadertoy API client. Requires SHADERTOY_API_KEY for live calls.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ShadertoyShader {
    pub id: String,
    pub title: String,
    pub author: String,
    /// Concatenated image-pass code, ready to feed the renderer.
    pub image_code: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ShadertoyError {
    #[error("SHADERTOY_API_KEY not set - get a free app key at shadertoy.com/myapps")]
    NoKey,
    #[error("shader not found: {0}")]
    NotFound(String),
    #[error("network/API error: {0}")]
    Api(String),
}

/// Extract a shader id from a raw id or any shadertoy.com/view/<id> URL.
pub fn parse_id(input: &str) -> String {
    if let Some(idx) = input.rfind("/view/") {
        return input[idx + 6..].trim_end_matches('/').to_string();
    }
    if let Some(idx) = input.rfind("/embed/") {
        return input[idx + 7..].split('?').next().unwrap_or("").to_string();
    }
    input.trim().to_string()
}

fn api_key() -> Result<String, ShadertoyError> {
    std::env::var("SHADERTOY_API_KEY").map_err(|_| ShadertoyError::NoKey)
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct RawResponse {
    #[serde(default)]
    Shader: Option<RawShader>,
    #[serde(default)]
    Error: Option<String>,
}
#[derive(Deserialize)]
struct RawShader {
    info: RawInfo,
    renderpass: Vec<RawPass>,
}
#[derive(Deserialize)]
struct RawInfo {
    id: String,
    name: String,
    username: String,
}
#[derive(Deserialize)]
struct RawPass {
    code: String,
    #[serde(rename = "type")]
    kind: String,
}

/// Fetch a shader by id or URL via the official API.
pub fn get(id_or_url: &str) -> Result<ShadertoyShader, ShadertoyError> {
    let key = api_key()?;
    let id = parse_id(id_or_url);
    let url = format!("https://www.shadertoy.com/api/v1/shaders/{id}?key={key}");
    let rt = tokio::runtime::Runtime::new().map_err(|e| ShadertoyError::Api(e.to_string()))?;
    let body: RawResponse = rt.block_on(async {
        reqwest::get(&url)
            .await
            .map_err(|e| ShadertoyError::Api(e.to_string()))?
            .json()
            .await
            .map_err(|e| ShadertoyError::Api(e.to_string()))
    })?;
    if let Some(err) = body.Error {
        return Err(ShadertoyError::NotFound(err));
    }
    let sh = body
        .Shader
        .ok_or_else(|| ShadertoyError::NotFound(id.clone()))?;
    let image_code = sh
        .renderpass
        .iter()
        .find(|p| p.kind == "image")
        .map(|p| p.code.clone())
        .ok_or_else(|| ShadertoyError::NotFound(format!("{id} has no image pass")))?;
    Ok(ShadertoyShader {
        id: sh.info.id,
        title: sh.info.name,
        author: sh.info.username,
        image_code,
    })
}

/// Keyword search; returns shader ids.
pub fn search(query: &str) -> Result<Vec<String>, ShadertoyError> {
    let key = api_key()?;
    let url = format!(
        "https://www.shadertoy.com/api/v1/shaders/query/{}?key={key}",
        urlencoding_min(query)
    );
    let rt = tokio::runtime::Runtime::new().map_err(|e| ShadertoyError::Api(e.to_string()))?;
    #[derive(Deserialize)]
    #[allow(non_snake_case)]
    struct Q {
        #[serde(default)]
        Results: Vec<String>,
    }
    let q: Q = rt.block_on(async {
        reqwest::get(&url)
            .await
            .map_err(|e| ShadertoyError::Api(e.to_string()))?
            .json()
            .await
            .map_err(|e| ShadertoyError::Api(e.to_string()))
    })?;
    Ok(q.Results)
}

/// Tiny percent-encoder for the query path segment (avoids a url crate dep).
fn urlencoding_min(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_id_from_url_and_raw() {
        assert_eq!(parse_id("https://www.shadertoy.com/view/Ms2SD1"), "Ms2SD1");
        assert_eq!(
            parse_id("https://www.shadertoy.com/embed/Ms2SD1?gui=true"),
            "Ms2SD1"
        );
        assert_eq!(parse_id("  Ms2SD1 "), "Ms2SD1");
    }
}
