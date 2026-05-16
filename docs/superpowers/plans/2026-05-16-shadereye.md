# shadereye Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `shadereye`, a single-binary Rust MCP server that lets an LLM compile, render, debug, browser-execute, test, and look up shaders across GLSL/WGSL/HLSL.

**Architecture:** A Cargo workspace of pure library crates (`compile`, `render`, `browser`, `shadertoy`, `reference`) each (input → output) and independently testable, plus a thin `shadereye-mcp` binary that registers them as `rmcp` tools/resources over stdio. Native rendering uses headless `wgpu` with a software fallback; the browser backend drives a system Chromium over CDP via `chromiumoxide`.

**Tech Stack:** Rust 2021, `rmcp`, `naga`, `wgpu`, `pollster`, `bytemuck`, `image`, `chromiumoxide`, `tokio`, `reqwest`, `serde`/`serde_json`, `thiserror`, `which`.

---

## Notes for the implementing engineer

- **External API drift:** `rmcp`, `naga`, and `wgpu` change APIs between minor versions. The crate versions pinned below were correct at planning time. If `cargo build` fails on an external API, run `cargo doc --open -p <crate>` (or read docs.rs for the *exact pinned version*) and adapt the call site. Do **not** change our own types/signatures to work around this — only the external call.
- **Our logic is fully specified.** Every test and every function body for shadereye's own code is given. Do not invent behavior.
- **Commit after every task** with the message shown. Run `cargo fmt` and `cargo clippy --all-targets -- -D warnings` before each commit; fix warnings.
- Work top-to-bottom. Later tasks depend on earlier types.

## File / crate map

```
shadereye/
  Cargo.toml                         # workspace
  crates/
    shadereye-compile/
      Cargo.toml
      src/lib.rs                     # ShaderLang, CompileError, detect_language, validate, translate, wrap_shadertoy
    shadereye-render/
      Cargo.toml
      src/lib.rs                     # RenderParams, RenderOutput, Backend reporting, render(), render_animation(), probe_pixels()
      src/diff.rs                    # DiffResult, diff_images()
    shadereye-browser/
      Cargo.toml
      src/lib.rs                     # BrowserRunParams, BrowserTranscript, run_in_browser()
      src/harness.rs                 # generate_html() WebGL2/GLSL-ES-3.00 page
    shadereye-shadertoy/
      Cargo.toml
      src/lib.rs                     # ShadertoyClient, ShadertoyShader, get(), search(), adapt_to_harness()
    shadereye-reference/
      Cargo.toml
      src/lib.rs                     # ReferenceDb, lookup(); embeds data/*.json
      data/glsl.json data/wgsl.json data/shadertoy.json data/gotchas.json
    shadereye-mcp/
      Cargo.toml
      src/main.rs                    # rmcp server, tool registration, stdio
      src/tools.rs                   # one fn per MCP tool, calls the libs
  examples/                          # *.glsl / *.wgsl sample shaders (also test fixtures)
  docs/gallery.md
  .github/workflows/ci.yml
  .github/workflows/release.yml
  README.md  LICENSE  CONTRIBUTING.md  .gitignore
```

---

## Task 1: Workspace skeleton

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `rust-toolchain.toml`

- [ ] **Step 1: Create `.gitignore`**

```
/target
**/*.rs.bk
Cargo.lock.orig
.DS_Store
/tmp
```

- [ ] **Step 2: Create `rust-toolchain.toml`**

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Create workspace `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = [
  "crates/shadereye-compile",
  "crates/shadereye-render",
  "crates/shadereye-browser",
  "crates/shadereye-shadertoy",
  "crates/shadereye-reference",
  "crates/shadereye-mcp",
]

[workspace.package]
edition = "2021"
license = "MIT"
repository = "https://github.com/zajalist/shadereye"
version = "0.1.0"

[workspace.dependencies]
naga = { version = "23", features = ["glsl-in", "wgsl-in", "wgsl-out", "spv-in", "spv-out"] }
wgpu = "23"
pollster = "0.3"
bytemuck = { version = "1", features = ["derive"] }
image = { version = "0.25", default-features = false, features = ["png"] }
chromiumoxide = { version = "0.7", features = ["tokio-runtime"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "process", "time"] }
futures = "0.3"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
which = "6"
rmcp = { version = "0.1", features = ["server", "transport-io"] }
anyhow = "1"
```

- [ ] **Step 4: Verify workspace parses**

Run: `cargo metadata --no-deps --format-version 1 > NUL 2>&1; echo $LASTEXITCODE`
Expected: prints `0` is not required yet (members don't exist). Instead just confirm the file is valid TOML by `cargo verify-project` after Task 2. Skip running here.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml .gitignore rust-toolchain.toml
git commit -m "chore: workspace skeleton"
```

---

## Task 2: `shadereye-compile` — language detection

**Files:**
- Create: `crates/shadereye-compile/Cargo.toml`, `crates/shadereye-compile/src/lib.rs`

- [ ] **Step 1: Create `crates/shadereye-compile/Cargo.toml`**

```toml
[package]
name = "shadereye-compile"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
naga.workspace = true
serde.workspace = true
thiserror.workspace = true
```

- [ ] **Step 2: Write the failing test** in `crates/shadereye-compile/src/lib.rs`

```rust
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
    if src.contains("@vertex") || src.contains("@fragment") || src.contains("fn main") && src.contains("->") {
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
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p shadereye-compile`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/shadereye-compile
git commit -m "feat(compile): shader language detection"
```

---

## Task 3: `shadereye-compile` — Shadertoy → full GLSL wrapping

**Files:**
- Modify: `crates/shadereye-compile/src/lib.rs`

- [ ] **Step 1: Write the failing test** (append to `mod tests`)

```rust
    #[test]
    fn wraps_shadertoy_body_into_full_glsl() {
        let body = "void mainImage(out vec4 o, in vec2 fc){ o = vec4(fc/iResolution.xy, 0.0, 1.0); }";
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shadereye-compile wraps_shadertoy`
Expected: FAIL — `wrap_shadertoy_fragment` not found.

- [ ] **Step 3: Implement** (add to `lib.rs`, above `#[cfg(test)]`)

```rust
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p shadereye-compile`
Expected: all passed.

- [ ] **Step 5: Commit**

```bash
git add crates/shadereye-compile
git commit -m "feat(compile): wrap Shadertoy mainImage into full GLSL"
```

---

## Task 4: `shadereye-compile` — validation via naga

**Files:**
- Modify: `crates/shadereye-compile/src/lib.rs`

- [ ] **Step 1: Write the failing test** (append to `mod tests`)

```rust
    #[test]
    fn validates_good_wgsl() {
        let src = "@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0,0.0,0.0,1.0); }";
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shadereye-compile validates_good_wgsl`
Expected: FAIL — `validate` / `ValidationReport` not found.

- [ ] **Step 3: Implement** (add to `lib.rs`)

```rust
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
        ShaderLang::Wgsl => naga::front::wgsl::parse_str(src)
            .map_err(|e| e.emit_to_string(src)),
        ShaderLang::Glsl => {
            let mut fe = naga::front::glsl::Frontend::default();
            fe.parse(
                &naga::front::glsl::Options::from(naga::ShaderStage::Fragment),
                src,
            )
            .map_err(|e| format!("{e:?}"))
        }
        ShaderLang::SpirV => Err("SPIR-V validation not supported as text input".to_string()),
        ShaderLang::Hlsl => Err("HLSL is supported only as a translation target/source via naga; \
            validate after translating to WGSL".to_string()),
    };

    match module {
        Ok(m) => {
            let mut validator = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            );
            if let Err(e) = validator.validate(&m) {
                errors.push(Diagnostic { message: format!("{e:?}"), line: None, column: None });
            }
        }
        Err(msg) => errors.push(Diagnostic { message: msg, line: None, column: None }),
    }

    ValidationReport { language, errors, warnings: Vec::new() }
}
```

> API note: if `parse_str`/`Frontend::parse`/`Validator::validate` signatures differ in the pinned naga, adjust the call (not the public `validate` signature). The error string must remain non-empty on failure.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shadereye-compile`
Expected: all passed.

- [ ] **Step 5: Commit**

```bash
git add crates/shadereye-compile
git commit -m "feat(compile): naga-backed validation with structured diagnostics"
```

---

## Task 5: `shadereye-compile` — translation

**Files:**
- Modify: `crates/shadereye-compile/src/lib.rs`

- [ ] **Step 1: Write the failing test** (append to `mod tests`)

```rust
    #[test]
    fn translates_wgsl_to_glsl() {
        let src = "@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(0.2,0.4,0.6,1.0); }";
        let out = translate(src, ShaderLang::Wgsl, ShaderLang::Glsl).expect("translate ok");
        assert!(out.contains("0.2") || out.to_lowercase().contains("vec4"));
        assert!(!out.is_empty());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shadereye-compile translates_wgsl_to_glsl`
Expected: FAIL — `translate` not found.

- [ ] **Step 3: Implement**

```rust
/// Cross-compile a shader between languages via naga's IR.
/// Supported source: WGSL, GLSL. Supported target: WGSL, GLSL, SPIR-V (HLSL via wgsl-out path is roadmap).
pub fn translate(src: &str, from: ShaderLang, to: ShaderLang) -> Result<String, CompileError> {
    let module = match from {
        ShaderLang::Wgsl => naga::front::wgsl::parse_str(src)
            .map_err(|e| CompileError::Translation(e.emit_to_string(src)))?,
        ShaderLang::Glsl => {
            let mut fe = naga::front::glsl::Frontend::default();
            fe.parse(&naga::front::glsl::Options::from(naga::ShaderStage::Fragment), src)
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
        ShaderLang::Wgsl => naga::back::wgsl::write_string(
            &module, &info, naga::back::wgsl::WriterFlags::empty(),
        )
        .map_err(|e| CompileError::Translation(format!("{e:?}"))),
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
                &mut buf, &module, &info, &opts, &pipe, naga::proc::BoundsCheckPolicies::default(),
            )
            .map_err(|e| CompileError::Translation(format!("{e:?}")))?;
            w.write().map_err(|e| CompileError::Translation(format!("{e:?}")))?;
            Ok(buf)
        }
        other => Err(CompileError::Unsupported(other)),
    }
}
```

> API note: naga back-end constructor signatures vary by version; keep the `translate` signature and error type, adapt internals to the pinned naga.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shadereye-compile`
Expected: all passed.

- [ ] **Step 5: Commit**

```bash
git add crates/shadereye-compile
git commit -m "feat(compile): cross-language translation via naga IR"
```

---

## Task 6: `shadereye-render` — headless wgpu device + fullscreen pass

**Files:**
- Create: `crates/shadereye-render/Cargo.toml`, `crates/shadereye-render/src/lib.rs`

- [ ] **Step 1: Create `crates/shadereye-render/Cargo.toml`**

```toml
[package]
name = "shadereye-render"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
shadereye-compile = { path = "../shadereye-compile" }
wgpu.workspace = true
pollster.workspace = true
bytemuck.workspace = true
image.workspace = true
serde.workspace = true
thiserror.workspace = true
```

- [ ] **Step 2: Write the failing test** in `crates/shadereye-render/src/lib.rs`

```rust
//! Headless wgpu rendering of Shadertoy-style fragment shaders.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_solid_color_shader() {
        let body = "void mainImage(out vec4 o, in vec2 fc){ o = vec4(1.0, 0.0, 0.0, 1.0); }";
        let out = render(&RenderParams {
            source: body.into(),
            lang: None,
            width: 16,
            height: 16,
            time: 0.0,
            mouse: [0.0; 4],
        })
        .expect("render ok");
        assert_eq!(out.width, 16);
        assert_eq!(out.height, 16);
        // PNG signature
        assert_eq!(&out.png[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        // center pixel is red
        let px = out.pixel(8, 8);
        assert!(px[0] > 200 && px[1] < 50 && px[2] < 50, "got {px:?}");
        assert!(!out.backend.is_empty());
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p shadereye-render renders_solid_color_shader`
Expected: FAIL — `render`/`RenderParams` not found.

- [ ] **Step 4: Implement** the device + render pipeline in `lib.rs` (above `#[cfg(test)]`)

```rust
use serde::Serialize;
use shadereye_compile::{wrap_shadertoy_fragment, ShaderLang};
use wgpu::util::DeviceExt;

#[derive(Debug, Clone)]
pub struct RenderParams {
    pub source: String,
    pub lang: Option<ShaderLang>,
    pub width: u32,
    pub height: u32,
    pub time: f32,
    pub mouse: [f32; 4],
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderOutput {
    pub width: u32,
    pub height: u32,
    #[serde(skip)]
    pub png: Vec<u8>,
    #[serde(skip)]
    pub rgba: Vec<u8>, // width*height*4, row-major, top-left origin
    pub backend: String,
}

impl RenderOutput {
    /// RGBA of pixel (x,y), origin top-left.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("no wgpu adapter (GPU or software) available")]
    NoAdapter,
    #[error("shader compile failed: {0}")]
    ShaderCompile(String),
    #[error("gpu error: {0}")]
    Gpu(String),
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    i_resolution: [f32; 4],
    i_mouse: [f32; 4],
    i_time: f32,
    i_time_delta: f32,
    i_frame: i32,
    _pad0: f32,
}

const VERTEX_WGSL: &str = r#"
@vertex
fn vs(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(vec2(-1.0,-1.0), vec2(3.0,-1.0), vec2(-1.0,3.0));
    return vec4<f32>(p[vi], 0.0, 1.0);
}
"#;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    backend: String,
}

fn acquire_gpu() -> Result<Gpu, RenderError> {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .or_else(|| {
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: None,
                    force_fallback_adapter: true,
                }))
            })
            .ok_or(RenderError::NoAdapter)?;
        let backend = format!("{:?}", adapter.get_info().backend);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        Ok(Gpu { device, queue, backend })
    })
}

/// Render one frame of a Shadertoy-style (or full GLSL/WGSL) fragment shader.
pub fn render(params: &RenderParams) -> Result<RenderOutput, RenderError> {
    let gpu = acquire_gpu()?;
    let lang = params.lang.unwrap_or_else(|| {
        shadereye_compile::detect_language(&params.source)
    });

    // Build the fragment module. GLSL is wrapped + handed to wgpu's naga path.
    let fs_module = match lang {
        ShaderLang::Wgsl => gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fs-wgsl"),
            source: wgpu::ShaderSource::Wgsl(params.source.clone().into()),
        }),
        ShaderLang::Glsl => {
            let wrapped = wrap_shadertoy_fragment(&params.source);
            gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("fs-glsl"),
                source: wgpu::ShaderSource::Glsl {
                    shader: wrapped.into(),
                    stage: wgpu::naga::ShaderStage::Fragment,
                    defines: Default::default(),
                },
            })
        }
        other => return Err(RenderError::ShaderCompile(format!("unsupported render language {other:?}"))),
    };
    let vs_module = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("vs"),
        source: wgpu::ShaderSource::Wgsl(VERTEX_WGSL.into()),
    });

    let uniforms = Uniforms {
        i_resolution: [params.width as f32, params.height as f32, 1.0, 0.0],
        i_mouse: params.mouse,
        i_time: params.time,
        i_time_delta: 1.0 / 60.0,
        i_frame: (params.time * 60.0) as i32,
        _pad0: 0.0,
    };
    let ubo = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ubo"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bgl,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: ubo.as_entire_binding() }],
    });
    let pll = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });

    let fmt = wgpu::TextureFormat::Rgba8UnormSrgb;
    let pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pll),
        vertex: wgpu::VertexState { module: &vs_module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: &fs_module,
            entry_point: None, // use the module's sole entry point
            targets: &[Some(fmt.into())],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d { width: params.width, height: params.height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: fmt,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());

    let bpr = (params.width * 4).next_multiple_of(256);
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (bpr * params.height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut enc = gpu.device.create_command_encoder(&Default::default());
    {
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rp.set_pipeline(&pipeline);
        rp.set_bind_group(0, &bind_group, &[]);
        rp.draw(0..3, 0..1);
    }
    enc.copy_texture_to_buffer(
        wgpu::ImageCopyTexture { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::ImageCopyBuffer {
            buffer: &readback,
            layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(params.height) },
        },
        wgpu::Extent3d { width: params.width, height: params.height, depth_or_array_layers: 1 },
    );
    gpu.queue.submit([enc.finish()]);

    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    gpu.device.poll(wgpu::Maintain::Wait);
    let data = slice.get_mapped_range();

    let mut rgba = Vec::with_capacity((params.width * params.height * 4) as usize);
    for row in 0..params.height {
        let start = (row * bpr) as usize;
        rgba.extend_from_slice(&data[start..start + (params.width * 4) as usize]);
    }
    drop(data);
    readback.unmap();

    let img = image::RgbaImage::from_raw(params.width, params.height, rgba.clone())
        .ok_or_else(|| RenderError::Gpu("buffer size mismatch".into()))?;
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| RenderError::Gpu(e.to_string()))?;

    Ok(RenderOutput { width: params.width, height: params.height, png, rgba, backend: gpu.backend })
}
```

> API note: wgpu 23 device/pipeline descriptor fields shown here match wgpu 23. If the pinned wgpu differs (e.g. `request_device` arity, `entry_point: Option<&str>`), adjust call sites only.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p shadereye-render renders_solid_color_shader`
Expected: PASS (uses GPU or software fallback). If no adapter at all in the dev env, see Task 18 CI note; locally this should pass.

- [ ] **Step 6: Commit**

```bash
git add crates/shadereye-render
git commit -m "feat(render): headless wgpu fullscreen fragment render"
```

---

## Task 7: `shadereye-render` — animation contact sheet

**Files:**
- Modify: `crates/shadereye-render/src/lib.rs`

- [ ] **Step 1: Write the failing test** (append to `mod tests`)

```rust
    #[test]
    fn animation_makes_a_montage_wider_than_one_frame() {
        let body = "void mainImage(out vec4 o, in vec2 fc){ o = vec4(fract(iTime), 0.0, 0.0, 1.0); }";
        let m = render_animation(&RenderParams {
            source: body.into(), lang: None, width: 16, height: 16, time: 0.0, mouse: [0.0;4],
        }, 0.0, 1.0, 4).expect("anim ok");
        // 4 frames laid horizontally => width >= 4*16
        assert!(m.width >= 64, "montage width {}", m.width);
        assert_eq!(&m.png[0..8], &[0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A]);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shadereye-render animation_makes_a_montage`
Expected: FAIL — `render_animation` not found.

- [ ] **Step 3: Implement**

```rust
/// Render `frames` evenly spaced over [t0, t1] and tile them left→right into one PNG.
pub fn render_animation(
    base: &RenderParams, t0: f32, t1: f32, frames: u32,
) -> Result<RenderOutput, RenderError> {
    let frames = frames.max(1);
    let mut tiles = Vec::new();
    for i in 0..frames {
        let t = if frames == 1 { t0 } else { t0 + (t1 - t0) * (i as f32 / (frames - 1) as f32) };
        let p = RenderParams { time: t, ..base.clone() };
        tiles.push(render(&p)?);
    }
    let fw = base.width;
    let fh = base.height;
    let total_w = fw * frames;
    let mut canvas = image::RgbaImage::new(total_w, fh);
    for (i, tile) in tiles.iter().enumerate() {
        let t = image::RgbaImage::from_raw(fw, fh, tile.rgba.clone()).unwrap();
        image::imageops::overlay(&mut canvas, &t, (i as u32 * fw) as i64, 0);
    }
    let rgba = canvas.clone().into_raw();
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(canvas)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| RenderError::Gpu(e.to_string()))?;
    Ok(RenderOutput { width: total_w, height: fh, png, rgba, backend: tiles[0].backend.clone() })
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p shadereye-render`
Expected: all passed.

- [ ] **Step 5: Commit**

```bash
git add crates/shadereye-render
git commit -m "feat(render): animation contact-sheet montage"
```

---

## Task 8: `shadereye-render` — expression visualizer + pixel probe

**Files:**
- Modify: `crates/shadereye-render/src/lib.rs`

- [ ] **Step 1: Write the failing test** (append to `mod tests`)

```rust
    #[test]
    fn visualize_expression_grayscale() {
        // visualize the constant 0.5 as grayscale; whole image ~128
        let out = visualize_expression(
            "void mainImage(out vec4 o, in vec2 fc){ o = vec4(0.0); }",
            "0.5", VisualizeMode::GrayscaleFloat, 16, 16,
        ).expect("viz ok");
        let p = out.pixel(8, 8);
        assert!((p[0] as i32 - 128).abs() < 20, "got {p:?}");
    }

    #[test]
    fn probe_returns_exact_rgba() {
        let body = "void mainImage(out vec4 o, in vec2 fc){ o = vec4(1.0,0.0,0.0,1.0); }";
        let pr = probe_pixels(&RenderParams {
            source: body.into(), lang: None, width: 16, height: 16, time: 0.0, mouse: [0.0;4],
        }, &[(4,4),(8,8)]).expect("probe ok");
        assert_eq!(pr.samples.len(), 2);
        assert!(pr.samples[0].rgba[0] > 200 && pr.samples[0].rgba[1] < 50);
        assert_eq!(&pr.crop_png[0..8], &[0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A]);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shadereye-render visualize_expression_grayscale`
Expected: FAIL — symbols not found.

- [ ] **Step 3: Implement**

```rust
#[derive(Debug, Clone, Copy, Serialize)]
pub enum VisualizeMode { GrayscaleFloat, RgVec2, RgbVec3, Normalized, Heatmap }

/// Replace the shader's output with a chosen expression mapped to color.
/// Only Shadertoy-style GLSL bodies are supported (the common debug case).
pub fn visualize_expression(
    glsl_body: &str, expr: &str, mode: VisualizeMode, w: u32, h: u32,
) -> Result<RenderOutput, RenderError> {
    let map = match mode {
        VisualizeMode::GrayscaleFloat => format!("vec3(float({expr}))"),
        VisualizeMode::RgVec2 => format!("vec3(vec2({expr}), 0.0)"),
        VisualizeMode::RgbVec3 => format!("vec3({expr})"),
        VisualizeMode::Normalized => format!("vec3(0.5 + 0.5 * float({expr}))"),
        VisualizeMode::Heatmap => format!(
            "(clamp(float({expr}),0.0,1.0) * vec3(1.0,0.0,0.0) + (1.0-clamp(float({expr}),0.0,1.0)) * vec3(0.0,0.0,1.0))"
        ),
    };
    // Keep the user body so helper fns/uniform usage still compile, then override output.
    let src = format!(
        "{glsl_body}\nvoid _se_viz(out vec4 o, in vec2 fc){{ o = vec4({map}, 1.0); }}\n\
         void mainImage(out vec4 o, in vec2 fc){{ _se_viz(o, fc); }}"
    );
    // The user's own mainImage would clash; strip it by renaming.
    let src = src.replacen("void mainImage", "void _se_user_main", 1);
    render(&RenderParams { source: src, lang: Some(ShaderLang::Glsl), width: w, height: h, time: 0.0, mouse: [0.0;4] })
}

#[derive(Debug, Clone, Serialize)]
pub struct PixelSample { pub x: u32, pub y: u32, pub rgba: [u8; 4] }

#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub samples: Vec<PixelSample>,
    #[serde(skip)]
    pub crop_png: Vec<u8>,
}

/// Render once, return exact RGBA at each coord plus a 4x-zoomed full-image crop.
pub fn probe_pixels(params: &RenderParams, coords: &[(u32, u32)]) -> Result<ProbeResult, RenderError> {
    let out = render(params)?;
    let samples = coords.iter().map(|&(x, y)| PixelSample { x, y, rgba: out.pixel(x.min(out.width-1), y.min(out.height-1)) }).collect();
    let img = image::RgbaImage::from_raw(out.width, out.height, out.rgba.clone()).unwrap();
    let zoom = image::imageops::resize(&img, out.width * 4, out.height * 4, image::imageops::FilterType::Nearest);
    let mut crop_png = Vec::new();
    image::DynamicImage::ImageRgba8(zoom)
        .write_to(&mut std::io::Cursor::new(&mut crop_png), image::ImageFormat::Png)
        .map_err(|e| RenderError::Gpu(e.to_string()))?;
    Ok(ProbeResult { samples, crop_png })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shadereye-render`
Expected: all passed.

- [ ] **Step 5: Commit**

```bash
git add crates/shadereye-render
git commit -m "feat(render): expression visualizer and pixel probe"
```

---

## Task 9: `shadereye-render` — image diff / golden test

**Files:**
- Create: `crates/shadereye-render/src/diff.rs`
- Modify: `crates/shadereye-render/src/lib.rs` (add `pub mod diff;`)

- [ ] **Step 1: Add `pub mod diff;`** at the top of `lib.rs` (after the `//!` doc line).

- [ ] **Step 2: Write the failing test** in `crates/shadereye-render/src/diff.rs`

```rust
//! Image comparison for golden tests.

use serde::Serialize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_images_zero_diff() {
        let a = vec![10u8; 16*16*4];
        let r = diff_images(16,16,&a,&a,0.0).unwrap();
        assert_eq!(r.max_abs, 0);
        assert!(r.passed);
    }

    #[test]
    fn different_images_flag_fail() {
        let a = vec![0u8; 16*16*4];
        let b = vec![255u8; 16*16*4];
        let r = diff_images(16,16,&a,&b,0.01).unwrap();
        assert_eq!(r.max_abs, 255);
        assert!(!r.passed);
        assert_eq!(&r.diff_png[0..8], &[0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A]);
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p shadereye-render --lib diff`
Expected: FAIL — `diff_images` not found.

- [ ] **Step 4: Implement** in `diff.rs` (above `#[cfg(test)]`)

```rust
#[derive(Debug, Clone, Serialize)]
pub struct DiffResult {
    pub max_abs: u8,
    pub mean_abs: f32,
    pub pct_pixels_over_tol: f32,
    pub passed: bool,
    #[serde(skip)]
    pub diff_png: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
#[error("image size mismatch or bad buffer")]
pub struct DiffError;

/// Compare two RGBA buffers. `tol` is the allowed fraction (0..1) of pixels that
/// may differ by any amount before the test is considered failed.
pub fn diff_images(w: u32, h: u32, a: &[u8], b: &[u8], tol: f32) -> Result<DiffResult, DiffError> {
    if a.len() != b.len() || a.len() != (w * h * 4) as usize {
        return Err(DiffError);
    }
    let mut max_abs = 0u8;
    let mut sum: u64 = 0;
    let mut differing = 0u64;
    let mut diff_img = image::RgbaImage::new(w, h);
    for i in 0..(w * h) as usize {
        let mut pix_diff = 0u8;
        for c in 0..4 {
            let d = a[i * 4 + c].abs_diff(b[i * 4 + c]);
            pix_diff = pix_diff.max(d);
            sum += d as u64;
            max_abs = max_abs.max(d);
        }
        if pix_diff > 0 { differing += 1; }
        let x = (i as u32) % w;
        let y = (i as u32) / w;
        diff_img.put_pixel(x, y, image::Rgba([pix_diff, 0, if pix_diff == 0 { 40 } else { 0 }, 255]));
    }
    let total = (w * h) as f32;
    let pct = differing as f32 / total;
    let mut diff_png = Vec::new();
    image::DynamicImage::ImageRgba8(diff_img)
        .write_to(&mut std::io::Cursor::new(&mut diff_png), image::ImageFormat::Png)
        .map_err(|_| DiffError)?;
    Ok(DiffResult {
        max_abs,
        mean_abs: sum as f32 / (total * 4.0),
        pct_pixels_over_tol: pct,
        passed: pct <= tol,
        diff_png,
    })
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p shadereye-render`
Expected: all passed.

- [ ] **Step 6: Commit**

```bash
git add crates/shadereye-render
git commit -m "feat(render): image diff and golden-test comparison"
```

---

## Task 10: `shadereye-browser` — HTML harness generator

**Files:**
- Create: `crates/shadereye-browser/Cargo.toml`, `crates/shadereye-browser/src/harness.rs`, `crates/shadereye-browser/src/lib.rs`

- [ ] **Step 1: Create `crates/shadereye-browser/Cargo.toml`**

```toml
[package]
name = "shadereye-browser"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
chromiumoxide.workspace = true
tokio.workspace = true
futures.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
which.workspace = true
base64 = "0.22"
```

- [ ] **Step 2: Write the failing test** in `crates/shadereye-browser/src/harness.rs`

```rust
//! Generates the self-contained WebGL2 / GLSL-ES-3.00 harness page that runs a
//! Shadertoy-style shader and reports compile logs + console output.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_embeds_user_body_and_reporters() {
        let html = generate_html("void mainImage(out vec4 o, in vec2 fc){ o=vec4(1.0); }", 32, 32, 3);
        assert!(html.contains("<canvas"));
        assert!(html.contains("getShaderInfoLog"));
        assert!(html.contains("getProgramInfoLog"));
        assert!(html.contains("mainImage"));
        assert!(html.contains("SHADEREYE_DONE")); // completion sentinel for the driver
        assert!(html.contains("300 es"));         // GLSL ES 3.00
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p shadereye-browser html_embeds`
Expected: FAIL — `generate_html` not found.

- [ ] **Step 4: Implement** `generate_html` in `harness.rs`

```rust
/// Build an HTML page: a WebGL2 canvas running the Shadertoy body for `frames`
/// frames, logging every shader/program info log and GL error to console, then
/// printing the sentinel `SHADEREYE_DONE`.
pub fn generate_html(user_body: &str, w: u32, h: u32, frames: u32) -> String {
    // The fragment shader: GLSL ES 3.00 with Shadertoy uniforms.
    let frag = format!(
        "#version 300 es\nprecision highp float;\nout vec4 _st;\n\
         uniform vec3 iResolution; uniform float iTime; uniform float iTimeDelta;\n\
         uniform int iFrame; uniform vec4 iMouse;\n{user_body}\n\
         void main(){{ vec4 c; mainImage(c, gl_FragCoord.xy); _st = c; }}"
    );
    let vert = "#version 300 es\nin vec2 p; void main(){ gl_Position = vec4(p,0.0,1.0); }";
    let frag_js = serde_json::to_string(&frag).unwrap();
    let vert_js = serde_json::to_string(&vert).unwrap();
    format!(r#"<!doctype html><html><head><meta charset="utf-8"></head><body>
<canvas id="c" width="{w}" height="{h}"></canvas>
<script>
(function(){{
  function fail(msg){{ console.error("SHADEREYE_GL_ERROR: "+msg); console.log("SHADEREYE_DONE"); }}
  var cv=document.getElementById('c');
  var gl=cv.getContext('webgl2');
  if(!gl){{ fail("webgl2 unavailable"); return; }}
  var vsrc={vert_js}, fsrc={frag_js};
  function sh(type,src){{
    var s=gl.createShader(type); gl.shaderSource(s,src); gl.compileShader(s);
    var log=gl.getShaderInfoLog(s);
    if(log) console.warn("SHADEREYE_SHADER_LOG["+(type===gl.VERTEX_SHADER?"vert":"frag")+"]: "+log);
    if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){{ fail("shader compile failed"); return null; }}
    return s;
  }}
  var vs=sh(gl.VERTEX_SHADER,vsrc); if(!vs) return;
  var fs=sh(gl.FRAGMENT_SHADER,fsrc); if(!fs) return;
  var pr=gl.createProgram(); gl.attachShader(pr,vs); gl.attachShader(pr,fs); gl.linkProgram(pr);
  var plog=gl.getProgramInfoLog(pr); if(plog) console.warn("SHADEREYE_PROGRAM_LOG: "+plog);
  if(!gl.getProgramParameter(pr,gl.LINK_STATUS)){{ fail("program link failed"); return; }}
  gl.useProgram(pr);
  var buf=gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,buf);
  gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,3,-1,-1,3]),gl.STATIC_DRAW);
  var loc=gl.getAttribLocation(pr,"p"); gl.enableVertexAttribArray(loc);
  gl.vertexAttribPointer(loc,2,gl.FLOAT,false,0,0);
  var uRes=gl.getUniformLocation(pr,"iResolution"), uT=gl.getUniformLocation(pr,"iTime"),
      uF=gl.getUniformLocation(pr,"iFrame"), uM=gl.getUniformLocation(pr,"iMouse"),
      uD=gl.getUniformLocation(pr,"iTimeDelta");
  var n=0;
  function frame(){{
    gl.uniform3f(uRes,{w}.0,{h}.0,1.0); gl.uniform1f(uT,n/60.0);
    gl.uniform1f(uD,1.0/60.0); gl.uniform1i(uF,n); gl.uniform4f(uM,0,0,0,0);
    gl.viewport(0,0,{w},{h}); gl.drawArrays(gl.TRIANGLES,0,3);
    var e=gl.getError(); if(e!==gl.NO_ERROR) console.error("SHADEREYE_GL_ERROR: code "+e);
    n++;
    if(n<{frames}) requestAnimationFrame(frame);
    else {{ console.log("SHADEREYE_DONE"); }}
  }}
  requestAnimationFrame(frame);
}})();
</script></body></html>"#)
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p shadereye-browser html_embeds`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/shadereye-browser
git commit -m "feat(browser): WebGL2 GLSL-ES harness HTML generator"
```

---

## Task 11: `shadereye-browser` — drive Chromium over CDP

**Files:**
- Modify: `crates/shadereye-browser/src/lib.rs`

- [ ] **Step 1: Write the failing test** in `crates/shadereye-browser/src/lib.rs`

```rust
//! Runs the harness page in a headless system Chromium and captures the
//! console/error transcript plus a screenshot.

pub mod harness;

use serde::Serialize;

#[derive(Debug, Clone)]
pub struct BrowserRunParams {
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserTranscript {
    pub console: Vec<String>,    // every log/info/warn/error line, prefixed with level
    pub exceptions: Vec<String>, // uncaught JS exceptions
    pub gl_errors: Vec<String>,  // lines tagged SHADEREYE_GL_ERROR / SHADER_LOG / PROGRAM_LOG
    pub compiled_ok: bool,
    #[serde(skip)]
    pub screenshot_png: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("no Chromium/Chrome found. Install Chrome or set SHADEREYE_BROWSER to its path")]
    NoBrowser,
    #[error("browser driver error: {0}")]
    Driver(String),
}

/// Locate a system Chrome/Chromium binary.
pub fn find_browser() -> Result<std::path::PathBuf, BrowserError> {
    if let Ok(p) = std::env::var("SHADEREYE_BROWSER") {
        let pb = std::path::PathBuf::from(p);
        if pb.exists() { return Ok(pb); }
    }
    for name in ["chrome", "chromium", "chromium-browser", "google-chrome", "msedge"] {
        if let Ok(p) = which::which(name) { return Ok(p); }
    }
    #[cfg(windows)]
    for p in [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    ] {
        let pb = std::path::PathBuf::from(p);
        if pb.exists() { return Ok(pb); }
    }
    Err(BrowserError::NoBrowser)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_browser_reports_missing_clearly() {
        // Either it finds one (Ok) or returns the actionable NoBrowser error.
        match find_browser() {
            Ok(p) => assert!(p.exists()),
            Err(e) => assert!(e.to_string().contains("SHADEREYE_BROWSER")),
        }
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test -p shadereye-browser find_browser_reports_missing_clearly`
Expected: PASS.

- [ ] **Step 3: Implement `run_in_browser`** (append to `lib.rs`, above `#[cfg(test)]`)

```rust
use base64::Engine;
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;

/// Run the shader in a real headless browser; collect console/exception/GL logs
/// and a screenshot. Times out after ~15s of no completion sentinel.
pub fn run_in_browser(p: &BrowserRunParams) -> Result<BrowserTranscript, BrowserError> {
    let exe = find_browser()?;
    let html = harness::generate_html(&p.source, p.width, p.height, p.frames.max(1));
    let data_url = format!(
        "data:text/html;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(html.as_bytes())
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| BrowserError::Driver(e.to_string()))?;

    rt.block_on(async move {
        let cfg = BrowserConfig::builder()
            .chrome_executable(exe)
            .arg("--headless=new")
            .arg("--use-gl=swiftshader")
            .arg("--enable-unsafe-swiftshader")
            .arg("--disable-gpu-sandbox")
            .build()
            .map_err(BrowserError::Driver)?;
        let (mut browser, mut handler) = Browser::launch(cfg)
            .await
            .map_err(|e| BrowserError::Driver(e.to_string()))?;
        let handle = tokio::spawn(async move { while handler.next().await.is_some() {} });

        let page = browser
            .new_page(data_url.as_str())
            .await
            .map_err(|e| BrowserError::Driver(e.to_string()))?;

        let mut console = Vec::new();
        let mut exceptions = Vec::new();
        let mut gl_errors = Vec::new();

        let mut logs = page
            .event_listener::<chromiumoxide::cdp::js_protocol::runtime::EventConsoleApiCalled>()
            .await
            .map_err(|e| BrowserError::Driver(e.to_string()))?;
        let mut exc = page
            .event_listener::<chromiumoxide::cdp::js_protocol::runtime::EventExceptionThrown>()
            .await
            .map_err(|e| BrowserError::Driver(e.to_string()))?;

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut done = false;
        while tokio::time::Instant::now() < deadline && !done {
            tokio::select! {
                Some(ev) = logs.next() => {
                    let level = ev.r#type.clone();
                    let text: String = ev.args.iter()
                        .filter_map(|a| a.value.as_ref().map(|v| v.to_string().trim_matches('"').to_string()))
                        .collect::<Vec<_>>().join(" ");
                    if text.contains("SHADEREYE_DONE") { done = true; }
                    if text.contains("SHADEREYE_GL_ERROR") || text.contains("SHADEREYE_SHADER_LOG")
                        || text.contains("SHADEREYE_PROGRAM_LOG") {
                        gl_errors.push(text.clone());
                    }
                    console.push(format!("[{level:?}] {text}"));
                }
                Some(ev) = exc.next() => {
                    exceptions.push(format!("{:?}", ev.exception_details));
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {}
            }
        }

        let shot = page
            .screenshot(
                chromiumoxide::page::ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .build(),
            )
            .await
            .unwrap_or_default();

        let _ = browser.close().await;
        handle.abort();

        let compiled_ok = !gl_errors.iter().any(|l| l.contains("compile failed") || l.contains("link failed"));
        Ok(BrowserTranscript {
            console,
            exceptions,
            gl_errors,
            compiled_ok,
            screenshot_png: shot,
        })
    })
}
```

> API note: `chromiumoxide` event type paths and `ScreenshotParams` builder differ across 0.5–0.7. Keep `BrowserTranscript`/`run_in_browser` signatures; adapt CDP calls to the pinned version using `cargo doc -p chromiumoxide`.

- [ ] **Step 4: Build (no network/browser test in unit suite)**

Run: `cargo build -p shadereye-browser`
Expected: compiles clean.

- [ ] **Step 5: Manual smoke (optional, requires Chrome)**

Run a throwaway example: create `crates/shadereye-browser/examples/smoke.rs`:

```rust
fn main() {
    let t = shadereye_browser::run_in_browser(&shadereye_browser::BrowserRunParams {
        source: "void mainImage(out vec4 o, in vec2 fc){ o=vec4(1.0,0.5,0.0,1.0); }".into(),
        width: 64, height: 64, frames: 3,
    }).unwrap();
    println!("compiled_ok={} console_lines={} shot_bytes={}", t.compiled_ok, t.console.len(), t.screenshot_png.len());
}
```

Run: `cargo run -p shadereye-browser --example smoke`
Expected: prints `compiled_ok=true` and non-zero shot bytes when Chrome is present.

- [ ] **Step 6: Commit**

```bash
git add crates/shadereye-browser
git commit -m "feat(browser): run shader in headless Chromium, capture console+GL transcript"
```

---

## Task 12: `shadereye-shadertoy` — client + harness adaptation

**Files:**
- Create: `crates/shadereye-shadertoy/Cargo.toml`, `crates/shadereye-shadertoy/src/lib.rs`

- [ ] **Step 1: Create `crates/shadereye-shadertoy/Cargo.toml`**

```toml
[package]
name = "shadereye-shadertoy"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
reqwest.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true

[dev-dependencies]
```

- [ ] **Step 2: Write the failing test** in `crates/shadereye-shadertoy/src/lib.rs`

```rust
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
    #[error("SHADERTOY_API_KEY not set — get a free app key at shadertoy.com/myapps")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_id_from_url_and_raw() {
        assert_eq!(parse_id("https://www.shadertoy.com/view/Ms2SD1"), "Ms2SD1");
        assert_eq!(parse_id("https://www.shadertoy.com/embed/Ms2SD1?gui=true"), "Ms2SD1");
        assert_eq!(parse_id("  Ms2SD1 "), "Ms2SD1");
    }
}
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cargo test -p shadereye-shadertoy parses_id`
Expected: PASS.

- [ ] **Step 4: Implement `get` and `search`** (append above `#[cfg(test)]`)

```rust
fn api_key() -> Result<String, ShadertoyError> {
    std::env::var("SHADERTOY_API_KEY").map_err(|_| ShadertoyError::NoKey)
}

#[derive(Deserialize)]
struct RawResponse { #[serde(default)] Shader: Option<RawShader>, #[serde(default)] Error: Option<String> }
#[derive(Deserialize)]
struct RawShader { info: RawInfo, renderpass: Vec<RawPass> }
#[derive(Deserialize)]
struct RawInfo { id: String, name: String, username: String }
#[derive(Deserialize)]
struct RawPass { code: String, #[serde(rename = "type")] kind: String }

/// Fetch a shader by id or URL via the official API.
pub fn get(id_or_url: &str) -> Result<ShadertoyShader, ShadertoyError> {
    let key = api_key()?;
    let id = parse_id(id_or_url);
    let url = format!("https://www.shadertoy.com/api/v1/shaders/{id}?key={key}");
    let rt = tokio::runtime::Runtime::new().map_err(|e| ShadertoyError::Api(e.to_string()))?;
    let body: RawResponse = rt.block_on(async {
        reqwest::get(&url).await.map_err(|e| ShadertoyError::Api(e.to_string()))?
            .json().await.map_err(|e| ShadertoyError::Api(e.to_string()))
    })?;
    if let Some(err) = body.Error { return Err(ShadertoyError::NotFound(err)); }
    let sh = body.Shader.ok_or_else(|| ShadertoyError::NotFound(id.clone()))?;
    let image_code = sh.renderpass.iter()
        .find(|p| p.kind == "image")
        .map(|p| p.code.clone())
        .ok_or_else(|| ShadertoyError::NotFound(format!("{id} has no image pass")))?;
    Ok(ShadertoyShader { id: sh.info.id, title: sh.info.name, author: sh.info.username, image_code })
}

/// Keyword search; returns shader ids.
pub fn search(query: &str) -> Result<Vec<String>, ShadertoyError> {
    let key = api_key()?;
    let url = format!("https://www.shadertoy.com/api/v1/shaders/query/{}?key={key}",
        urlencoding_min(query));
    let rt = tokio::runtime::Runtime::new().map_err(|e| ShadertoyError::Api(e.to_string()))?;
    #[derive(Deserialize)]
    struct Q { #[serde(default)] Results: Vec<String> }
    let q: Q = rt.block_on(async {
        reqwest::get(&url).await.map_err(|e| ShadertoyError::Api(e.to_string()))?
            .json().await.map_err(|e| ShadertoyError::Api(e.to_string()))
    })?;
    Ok(q.Results)
}

/// Tiny percent-encoder for the query path segment (avoids a url crate dep).
fn urlencoding_min(s: &str) -> String {
    s.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p shadereye-shadertoy`
Expected: PASS (network functions are not unit-tested; only `parse_id`).

- [ ] **Step 6: Commit**

```bash
git add crates/shadereye-shadertoy
git commit -m "feat(shadertoy): API client for get/search with graceful no-key error"
```

---

## Task 13: `shadereye-reference` — bundled reference + lookup

**Files:**
- Create: `crates/shadereye-reference/Cargo.toml`, `crates/shadereye-reference/src/lib.rs`, `crates/shadereye-reference/data/gotchas.json`, `crates/shadereye-reference/data/glsl.json`

- [ ] **Step 1: Create `crates/shadereye-reference/Cargo.toml`**

```toml
[package]
name = "shadereye-reference"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
reqwest.workspace = true
tokio.workspace = true
```

- [ ] **Step 2: Create `crates/shadereye-reference/data/glsl.json`**

```json
[
  {"name":"clamp","sig":"genType clamp(genType x, genType minV, genType maxV)","note":"Returns min(max(x,minV),maxV). Common for keeping colors in [0,1]."},
  {"name":"mix","sig":"genType mix(genType a, genType b, genType t)","note":"Linear interpolation a*(1-t)+b*t."},
  {"name":"smoothstep","sig":"genType smoothstep(edge0, edge1, x)","note":"Hermite interpolation; 0 below edge0, 1 above edge1. edge0 must be < edge1."},
  {"name":"fract","sig":"genType fract(genType x)","note":"x - floor(x). Core of most procedural/noise patterns."}
]
```

- [ ] **Step 3: Create `crates/shadereye-reference/data/gotchas.json`**

```json
[
  {"name":"webgl-precision","note":"WebGL2/GLSL ES requires an explicit precision qualifier (e.g. 'precision highp float;'). naga/desktop GLSL does not — shaders that work in shadereye native may fail in-browser without it."},
  {"name":"integer-division","note":"In GLSL, 1/2 == 0 (integer division). Use 1.0/2.0 for float results."},
  {"name":"shadertoy-uniforms","note":"Shadertoy provides iResolution(vec3), iTime(float), iTimeDelta, iFrame(int), iMouse(vec4), iChannel0..3. Entry point is void mainImage(out vec4 fragColor, in vec2 fragCoord)."},
  {"name":"wgsl-no-implicit-cast","note":"WGSL has no implicit int/float conversion. Write f32(i) / i32(f) explicitly."}
]
```

- [ ] **Step 4: Write the failing test** in `crates/shadereye-reference/src/lib.rs`

```rust
//! Offline-first curated reference, with optional best-effort web fallback.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RefEntry {
    pub name: String,
    #[serde(default)]
    pub sig: String,
    pub note: String,
}

const GLSL: &str = include_str!("../data/glsl.json");
const GOTCHAS: &str = include_str!("../data/gotchas.json");

/// Case-insensitive substring search over bundled GLSL builtins + gotchas.
pub fn lookup(query: &str) -> Vec<RefEntry> {
    let q = query.to_lowercase();
    let mut out = Vec::new();
    for src in [GLSL, GOTCHAS] {
        if let Ok(entries) = serde_json::from_str::<Vec<RefEntry>>(src) {
            for e in entries {
                if e.name.to_lowercase().contains(&q)
                    || e.note.to_lowercase().contains(&q)
                    || e.sig.to_lowercase().contains(&q) {
                    out.push(e);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_smoothstep() {
        let r = lookup("smoothstep");
        assert!(r.iter().any(|e| e.name == "smoothstep"));
    }

    #[test]
    fn finds_gotcha_by_topic() {
        let r = lookup("precision");
        assert!(r.iter().any(|e| e.name == "webgl-precision"));
    }

    #[test]
    fn empty_for_unknown() {
        assert!(lookup("zzz-no-such-token-xyz").is_empty());
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p shadereye-reference`
Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/shadereye-reference
git commit -m "feat(reference): bundled offline GLSL/gotcha reference + lookup"
```

---

## Task 14: `shadereye-mcp` — binary skeleton + first tool

**Files:**
- Create: `crates/shadereye-mcp/Cargo.toml`, `crates/shadereye-mcp/src/main.rs`, `crates/shadereye-mcp/src/tools.rs`

- [ ] **Step 1: Create `crates/shadereye-mcp/Cargo.toml`**

```toml
[package]
name = "shadereye-mcp"
edition.workspace = true
license.workspace = true
version.workspace = true
default-run = "shadereye-mcp"

[[bin]]
name = "shadereye-mcp"
path = "src/main.rs"

[dependencies]
shadereye-compile = { path = "../shadereye-compile" }
shadereye-render = { path = "../shadereye-render" }
shadereye-browser = { path = "../shadereye-browser" }
shadereye-shadertoy = { path = "../shadereye-shadertoy" }
shadereye-reference = { path = "../shadereye-reference" }
rmcp.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
base64 = "0.22"
```

- [ ] **Step 2: Implement `tools.rs`** — pure adapter functions returning JSON-able results. These are unit-testable without MCP.

```rust
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
        "glsl" => Some(Glsl), "wgsl" => Some(Wgsl), "hlsl" => Some(Hlsl), "spirv" | "spv" => Some(SpirV),
        _ => None,
    }
}

/// Returns (json_summary, png_base64).
pub fn render(source: &str, lang: Option<&str>, w: u32, h: u32, time: f32) -> (serde_json::Value, String) {
    let p = shadereye_render::RenderParams {
        source: source.into(), lang: lang.and_then(parse_lang),
        width: w, height: h, time, mouse: [0.0; 4],
    };
    match shadereye_render::render(&p) {
        Ok(o) => (serde_json::json!({"ok":true,"backend":o.backend,"width":o.width,"height":o.height}), b64(&o.png)),
        Err(e) => (serde_json::json!({"ok":false,"error":e.to_string()}), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_tool_returns_errors_field() {
        let v = validate("@fragment fn f()->@location(0) vec4<f32>{return vec4<f32>(1.0);}", Some("wgsl"));
        assert!(v.get("errors").is_some());
    }

    #[test]
    fn render_tool_returns_png_for_good_shader() {
        let (j, png) = render("void mainImage(out vec4 o,in vec2 fc){o=vec4(0.0,1.0,0.0,1.0);}", None, 16, 16, 0.0);
        assert_eq!(j["ok"], true);
        assert!(!png.is_empty());
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p shadereye-mcp`
Expected: 2 passed.

- [ ] **Step 4: Implement `main.rs`** — register tools with rmcp over stdio.

```rust
//! shadereye MCP server (stdio).

mod tools;

use rmcp::{ServiceExt, transport::stdio};
use rmcp::handler::server::ServerHandler;
use rmcp::model::*;

#[derive(Clone, Default)]
struct Shadereye;

impl ServerHandler for Shadereye {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation { name: "shadereye".into(), version: env!("CARGO_PKG_VERSION").into() },
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
```

> API note: rmcp 0.1 tool registration uses the `#[tool]` / `#[tool(tool_box)]` macros. Task 15 wires the actual tool methods. This step only needs to **compile and start**; verify with `cargo build -p shadereye-mcp`. If `ServiceExt`/`stdio`/`ServerHandler` paths differ in the pinned rmcp, fix imports per `cargo doc -p rmcp` — do not change tool function signatures in `tools.rs`.

- [ ] **Step 5: Build**

Run: `cargo build -p shadereye-mcp`
Expected: compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/shadereye-mcp
git commit -m "feat(mcp): server skeleton + validate/render tool adapters"
```

---

## Task 15: `shadereye-mcp` — wire all tools via rmcp `#[tool]`

**Files:**
- Modify: `crates/shadereye-mcp/src/main.rs`, `crates/shadereye-mcp/src/tools.rs`

- [ ] **Step 1: Add remaining adapter fns to `tools.rs`** (append above `#[cfg(test)]`)

```rust
pub fn render_animation(source: &str, lang: Option<&str>, w: u32, h: u32, t0: f32, t1: f32, frames: u32)
 -> (serde_json::Value, String) {
    let base = shadereye_render::RenderParams {
        source: source.into(), lang: lang.and_then(parse_lang), width: w, height: h, time: t0, mouse:[0.0;4],
    };
    match shadereye_render::render_animation(&base, t0, t1, frames) {
        Ok(o) => (serde_json::json!({"ok":true,"frames":frames,"backend":o.backend}), b64(&o.png)),
        Err(e) => (serde_json::json!({"ok":false,"error":e.to_string()}), String::new()),
    }
}

pub fn visualize(source: &str, expr: &str, mode: &str, w: u32, h: u32) -> (serde_json::Value, String) {
    use shadereye_render::VisualizeMode::*;
    let m = match mode { "rg" => RgVec2, "rgb" => RgbVec3, "normalized" => Normalized, "heatmap" => Heatmap, _ => GrayscaleFloat };
    match shadereye_render::visualize_expression(source, expr, m, w, h) {
        Ok(o) => (serde_json::json!({"ok":true,"mode":mode}), b64(&o.png)),
        Err(e) => (serde_json::json!({"ok":false,"error":e.to_string()}), String::new()),
    }
}

pub fn probe(source: &str, lang: Option<&str>, w: u32, h: u32, coords: &[(u32,u32)]) -> (serde_json::Value, String) {
    let p = shadereye_render::RenderParams { source: source.into(), lang: lang.and_then(parse_lang), width:w, height:h, time:0.0, mouse:[0.0;4] };
    match shadereye_render::probe_pixels(&p, coords) {
        Ok(r) => (serde_json::to_value(&r).unwrap(), b64(&r.crop_png)),
        Err(e) => (serde_json::json!({"ok":false,"error":e.to_string()}), String::new()),
    }
}

pub fn diff(source_a: &str, source_b: &str, w: u32, h: u32, tol: f32) -> (serde_json::Value, String) {
    let mk = |s:&str| shadereye_render::RenderParams { source:s.into(), lang:None, width:w, height:h, time:0.0, mouse:[0.0;4] };
    let a = match shadereye_render::render(&mk(source_a)) { Ok(o)=>o, Err(e)=>return (serde_json::json!({"ok":false,"error":e.to_string()}),String::new()) };
    let b = match shadereye_render::render(&mk(source_b)) { Ok(o)=>o, Err(e)=>return (serde_json::json!({"ok":false,"error":e.to_string()}),String::new()) };
    match shadereye_render::diff::diff_images(w,h,&a.rgba,&b.rgba,tol) {
        Ok(d) => (serde_json::to_value(&d).unwrap(), b64(&d.diff_png)),
        Err(_) => (serde_json::json!({"ok":false,"error":"size mismatch"}), String::new()),
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
        source: source.into(), width: w, height: h, frames,
    }) {
        Ok(t) => (serde_json::json!({
            "ok":true,"compiled_ok":t.compiled_ok,
            "console":t.console,"exceptions":t.exceptions,"gl_errors":t.gl_errors
        }), b64(&t.screenshot_png)),
        Err(e) => (serde_json::json!({"ok":false,"error":e.to_string()}), String::new()),
    }
}

pub fn shadertoy_get(id_or_url: &str) -> serde_json::Value {
    match shadereye_shadertoy::get(id_or_url) {
        Ok(s) => serde_json::json!({"ok":true,"id":s.id,"title":s.title,"author":s.author,"image_code":s.image_code}),
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
```

- [ ] **Step 2: Add a coverage test** (append to `tools.rs` `mod tests`)

```rust
    #[test]
    fn translate_tool_round_trips_wgsl_to_glsl() {
        let v = translate("@fragment fn f()->@location(0) vec4<f32>{return vec4<f32>(0.1,0.2,0.3,1.0);}", "wgsl", "glsl");
        assert_eq!(v["ok"], true, "{v}");
    }

    #[test]
    fn lookup_reference_tool_finds_entry() {
        let v = lookup_reference("smoothstep");
        assert!(v["entries"].as_array().unwrap().iter().any(|e| e["name"] == "smoothstep"));
    }
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p shadereye-mcp`
Expected: all passed.

- [ ] **Step 4: Register tools with rmcp** in `main.rs`. Replace the `impl ServerHandler` block with the macro-based tool router (rmcp 0.1 pattern):

```rust
use rmcp::{tool, tool_router, tool_handler, ServiceExt, transport::stdio};
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::ServerHandler;
use rmcp::model::*;
use rmcp::handler::server::wrapper::Parameters;
use serde::Deserialize;

#[derive(Clone)]
struct Shadereye { tool_router: ToolRouter<Self> }

#[derive(Deserialize, schemars::JsonSchema)]
struct ValidateArgs { source: String, #[serde(default)] lang: Option<String> }
#[derive(Deserialize, schemars::JsonSchema)]
struct RenderArgs { source: String, #[serde(default)] lang: Option<String>,
    #[serde(default = "d512")] width: u32, #[serde(default = "d512")] height: u32, #[serde(default)] time: f32 }
fn d512() -> u32 { 512 }

fn img_result(json: serde_json::Value, png_b64: String) -> CallToolResult {
    let mut content = vec![Content::text(json.to_string())];
    if !png_b64.is_empty() {
        content.push(Content::image(png_b64, "image/png".to_string()));
    }
    CallToolResult::success(content)
}

#[tool_router]
impl Shadereye {
    fn new() -> Self { Self { tool_router: Self::tool_router() } }

    #[tool(description = "Validate a shader (GLSL/WGSL); returns structured errors/warnings.")]
    async fn validate_shader(&self, Parameters(a): Parameters<ValidateArgs>) -> CallToolResult {
        CallToolResult::success(vec![Content::text(
            tools::validate(&a.source, a.lang.as_deref()).to_string())])
    }

    #[tool(description = "Render a Shadertoy-style/GLSL/WGSL fragment shader to a PNG image.")]
    async fn render_shader(&self, Parameters(a): Parameters<RenderArgs>) -> CallToolResult {
        let (j, png) = tools::render(&a.source, a.lang.as_deref(), a.width, a.height, a.time);
        img_result(j, png)
    }
    // Repeat one #[tool] method per remaining tools:: function
    // (render_animation, visualize_expression, probe_pixels, diff_shaders,
    //  translate_shader, run_in_browser, shadertoy_get, shadertoy_search,
    //  lookup_reference) following the exact same pattern: a JsonSchema args
    // struct + a method that calls the matching tools:: fn and wraps via
    // img_result (for image tools) or Content::text (for text-only tools).
}

#[tool_handler]
impl ServerHandler for Shadereye {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation { name: "shadereye".into(), version: env!("CARGO_PKG_VERSION").into() },
            instructions: Some("Shader perception/debug/test/translate tools for LLMs.".into()),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = Shadereye::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

Add `schemars = "0.8"` to `crates/shadereye-mcp/Cargo.toml` dependencies.

> API note: rmcp 0.1's macro names (`#[tool_router]`, `#[tool_handler]`, `Parameters`, `Content::image`) are stable in 0.1.x but check `cargo doc -p rmcp` for the exact `Content::image` constructor signature. Every remaining `tools::` fn MUST get a `#[tool]` wrapper — do not leave the comment as the implementation.

- [ ] **Step 5: Implement every remaining `#[tool]` method** explicitly (no shortcut). For each of `render_animation`, `visualize_expression`, `probe_pixels`, `diff_shaders`, `translate_shader`, `run_in_browser`, `shadertoy_get`, `shadertoy_search`, `lookup_reference`: define a `#[derive(Deserialize, schemars::JsonSchema)]` args struct with the same parameters as the matching `tools::` fn, and an `async fn` that calls it and wraps the result with `img_result` (image tools) or `Content::text` (text-only: `translate_shader`, `shadertoy_get`, `shadertoy_search`, `lookup_reference`).

- [ ] **Step 6: Build + run a stdio smoke test**

Run: `cargo build --release`
Then verify the server starts and lists tools:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' '{"jsonrpc":"2.0","method":"notifications/initialized"}' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | ./target/release/shadereye-mcp
```

Expected: JSON responses; the `tools/list` result contains `render_shader`, `validate_shader`, `run_in_browser`, etc.

- [ ] **Step 7: Commit**

```bash
git add crates/shadereye-mcp
git commit -m "feat(mcp): register full tool surface over stdio"
```

---

## Task 16: Example shaders + golden test fixtures

**Files:**
- Create: `examples/plasma.glsl`, `examples/raymarch_broken.glsl`, `examples/solid.wgsl`
- Create: `crates/shadereye-render/tests/golden.rs`

- [ ] **Step 1: Create `examples/plasma.glsl`**

```glsl
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    float v = sin(uv.x * 10.0 + iTime) + sin(uv.y * 10.0 + iTime);
    fragColor = vec4(0.5 + 0.5 * cos(v + vec3(0.0, 2.0, 4.0)), 1.0);
}
```

- [ ] **Step 2: Create `examples/raymarch_broken.glsl`** (intentionally has the integer-division gotcha for the README transcript)

```glsl
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    float t = 1 / 2;          // BUG: integer division -> 0.0
    fragColor = vec4(uv, t, 1.0);
}
```

- [ ] **Step 3: Create `examples/solid.wgsl`**

```wgsl
@fragment
fn fs() -> @location(0) vec4<f32> {
    return vec4<f32>(0.10, 0.65, 0.90, 1.0);
}
```

- [ ] **Step 4: Write golden test** `crates/shadereye-render/tests/golden.rs`

```rust
use shadereye_render::*;

#[test]
fn plasma_example_renders_nonblack() {
    let src = include_str!("../../../examples/plasma.glsl");
    let o = render(&RenderParams {
        source: src.into(), lang: None, width: 64, height: 64, time: 1.0, mouse: [0.0;4],
    }).expect("render");
    // not entirely black
    let any = (0..o.width).any(|x| { let p = o.pixel(x, 32); p[0] as u32 + p[1] as u32 + p[2] as u32 > 30 });
    assert!(any, "plasma rendered all black");
}

#[test]
fn broken_raymarch_compiles_but_blue_channel_is_zero() {
    let src = include_str!("../../../examples/raymarch_broken.glsl");
    let o = render(&RenderParams {
        source: src.into(), lang: None, width: 32, height: 32, time: 0.0, mouse: [0.0;4],
    }).expect("render");
    // t == 0 because of integer division; blue channel ~0 across the image
    assert!(o.pixel(16,16)[2] < 10, "expected blue~0 from the integer-division bug");
}
```

- [ ] **Step 5: Run golden tests**

Run: `cargo test -p shadereye-render --test golden`
Expected: 2 passed.

- [ ] **Step 6: Commit**

```bash
git add examples crates/shadereye-render/tests
git commit -m "test: example shaders + golden render fixtures"
```

---

## Task 17: README, LICENSE, CONTRIBUTING, gallery

**Files:**
- Create: `README.md`, `LICENSE`, `CONTRIBUTING.md`, `docs/gallery.md`

- [ ] **Step 1: Create `LICENSE`** — MIT, copyright holder `zajalist`, year 2026. Use the standard MIT text verbatim with `Copyright (c) 2026 zajalist`.

- [ ] **Step 2: Create `README.md`** with these sections, in order:
  1. Title + one-line pitch: "shadereye — give your coding LLM eyes for shaders."
  2. **The problem**: LLMs write shaders blind; no compile/see/fix loop.
  3. **What it does**: bullet list of the 11 tools (table from the spec).
  4. **Two render backends**: native `wgpu` (fast/CI) vs browser (real WebGL2 console+driver errors, Shadertoy-faithful).
  5. **Install**: `cargo install --git https://github.com/zajalist/shadereye shadereye-mcp` and prebuilt-binary note.
  6. **Use with Claude Code**: exact MCP config block:

```json
{
  "mcpServers": {
    "shadereye": {
      "command": "shadereye-mcp",
      "env": { "SHADERTOY_API_KEY": "your-free-key" }
    }
  }
}
```

  7. **Example transcript (native)**: "Claude fixes a broken raymarcher" — Claude calls `render_shader` on `examples/raymarch_broken.glsl`, sees a flat blue-less image, calls `lookup_reference("integer division")`, gets the gotcha, fixes `1/2` → `1.0/2.0`, re-renders.
  8. **Example transcript (browser)**: paste a Shadertoy shader missing `precision`, `run_in_browser` returns the real `getShaderInfoLog`, fix, re-run.
  9. Tool reference table. 10. Roadmap (from spec). 11. License.

- [ ] **Step 3: Create `CONTRIBUTING.md`** — short: how to build (`cargo build`), test (`cargo test`), the crate layout one-liner, "browser tests need Chrome / set `SHADEREYE_BROWSER`".

- [ ] **Step 4: Create `docs/gallery.md`** — placeholder headings "Plasma", "Mandelbrot", "Raymarch" each with a sentence; note images are generated by `cargo run -p shadereye-mcp` examples (filled post-build).

- [ ] **Step 5: Commit**

```bash
git add README.md LICENSE CONTRIBUTING.md docs/gallery.md
git commit -m "docs: README, license, contributing, gallery"
```

---

## Task 18: CI + release workflows

**Files:**
- Create: `.github/workflows/ci.yml`, `.github/workflows/release.yml`

- [ ] **Step 1: Create `.github/workflows/ci.yml`**

```yaml
name: ci
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: rustfmt, clippy }
      - name: Mesa software GL (for wgpu software fallback)
        run: sudo apt-get update && sudo apt-get install -y mesa-vulkan-drivers libvulkan1 vulkan-tools
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets -- -D warnings
      - name: Test (browser tests auto-skip without Chrome)
        run: cargo test --workspace
        env:
          WGPU_BACKEND: vulkan
          LIBGL_ALWAYS_SOFTWARE: "1"
```

> Note: if `cargo test --workspace` fails on the headless renderer in CI because no Vulkan software device is present, set `force_fallback_adapter:true` unconditionally is already in `acquire_gpu`. If GPU tests still cannot run on the runner, gate the render/golden tests behind `#[ignore]` and add `cargo test --workspace -- --include-ignored` only on a self-hosted/GPU runner. Keep compile/diff/reference/shadertoy/browser-html tests always-on (they need no GPU).

- [ ] **Step 2: Create `.github/workflows/release.yml`**

```yaml
name: release
on:
  push:
    tags: ["v*"]
jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - os: windows-latest
            target: x86_64-pc-windows-msvc
          - os: macos-latest
            target: aarch64-apple-darwin
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { targets: "${{ matrix.target }}" }
      - run: cargo build --release -p shadereye-mcp --target ${{ matrix.target }}
      - uses: softprops/action-gh-release@v2
        with:
          files: |
            target/${{ matrix.target }}/release/shadereye-mcp
            target/${{ matrix.target }}/release/shadereye-mcp.exe
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

- [ ] **Step 3: Commit**

```bash
git add .github
git commit -m "ci: test workflow + tagged release binaries"
```

---

## Task 19: Full workspace verification

**Files:** none (verification only)

- [ ] **Step 1: Format + lint**

Run: `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`
Expected: no output, exit 0. Fix any issue, then re-run.

- [ ] **Step 2: Full test suite**

Run: `cargo test --workspace`
Expected: all crates' tests pass (browser-network/Shadertoy tests are not in the suite; HTML-gen and find_browser tests are).

- [ ] **Step 3: Release build**

Run: `cargo build --release`
Expected: `target/release/shadereye-mcp` exists.

- [ ] **Step 4: stdio tools/list smoke** (same command as Task 15 Step 6) — confirm all 11 tool names present.

- [ ] **Step 5: Commit any fmt/clippy fixes**

```bash
git add -A
git commit -m "chore: workspace fmt/clippy clean, verified build"
```

---

## Task 20: Publish to GitHub

**Files:** none (repo publish)

- [ ] **Step 1: Create the public repo**

```bash
gh repo create zajalist/shadereye --public --source=. --remote=origin \
  --description "Give your coding LLM eyes for shaders: an MCP server to compile, render, debug, browser-execute, test and translate GLSL/WGSL/HLSL."
```

- [ ] **Step 2: Push**

```bash
git push -u origin main
```

- [ ] **Step 3: Set repo topics**

```bash
gh repo edit zajalist/shadereye --add-topic mcp,shaders,glsl,wgsl,shadertoy,llm,rust,webgl
```

- [ ] **Step 4: Verify CI is green**

Run: `gh run watch` (or `gh run list`)
Expected: the `ci` workflow completes successfully. If the headless-render job fails on the runner, apply the Task 18 `#[ignore]` gating, commit, push, re-check.

- [ ] **Step 5: Final commit/tag for first release**

```bash
git tag v0.1.0
git push origin v0.1.0
```

Expected: `release` workflow builds binaries and attaches them to the `v0.1.0` GitHub release.

---

## Self-Review (completed by plan author)

**Spec coverage:** validate ✔ T4 · render ✔ T6 · animation ✔ T7 · visualize ✔ T8 · probe ✔ T8 · diff/golden ✔ T9 · translate ✔ T5 · browser run + console/GL transcript ✔ T10–11 · shadertoy get/search ✔ T12 · reference (bundled) ✔ T13 · online reference fallback → **deferred**: marked roadmap in spec; bundled-only shipped in T13 (acceptable, matches "bundled is primary"; online flag is post-v1) · MCP resources for reference browsing → folded into `lookup_reference` tool for v1 (resource endpoint is a small post-v1 add; note added here so it is not lost) · crate isolation ✔ (per-crate libs) · error handling (no panics, structured) ✔ T4/T6/T10/T12 · software fallback ✔ T6 · CI software backend ✔ T18 · showcase (README/examples/gallery/CI/release/MIT) ✔ T16–20.

**Placeholder scan:** No "TBD/TODO/implement later". Task 15 Step 5 explicitly forbids leaving the "repeat pattern" comment as the implementation and requires each tool method written out.

**Type consistency:** `RenderParams`/`RenderOutput`/`RenderError` consistent T6→T20. `ShaderLang`/`validate`/`translate` consistent T2→T15. `BrowserRunParams`/`BrowserTranscript`/`run_in_browser` consistent T11→T15. `diff_images`→`DiffResult` consistent T9→T15. `tools::` fn names match the `#[tool]` wrappers in T15.

**Scope:** Single coherent deliverable (one MCP server); fits one plan. Two known v1 trims (online docs fallback, dedicated MCP resource endpoint) explicitly recorded as post-v1, consistent with the spec's roadmap.
