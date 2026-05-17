# shadereye - Design Spec

**Date:** 2026-05-16
**Status:** Implemented - v1 shipped
**Repo:** `github.com/zajalist/shadereye` (public) · local `D:\Projects\shadereye`

> This is the original design document. It describes the full intended surface
> including post-v1 roadmap items. For the actually-shipped v1 tool surface, see
> the [README](../../../README.md). Items marked "Roadmap (post-v1)" / "Scope
> Cuts" below are intentionally not in v1.

## Problem

Coding LLMs (Claude Code and peers) write shaders effectively blind. They cannot
see rendered output, cannot inspect intermediate values, and have no tight
compile/perceive/correct loop. The result is slow, error-prone shader work that
relies on the human to be the eyes. There is no lightweight, LLM-shaped tool that
closes this perception gap and connects the model to existing shader knowledge
(Shadertoy, language references).

## Goal

A small, single-binary **MCP server** that gives an LLM:

1. **Perception** - render shaders to images the model can actually view.
2. **Debugging aids** - animation montages, expression visualization, pixel
   probing, image diffing.
3. **Correctness loop** - structured compiler errors and golden-image testing.
4. **Runtime ground truth** - execute the shader in a real browser GPU pipeline
   (WebGL2/WebGPU) and capture *all* console output, JS exceptions, and
   driver-level shader/program logs.
5. **Knowledge connection** - Shadertoy import/search and a bundled+online
   reference.

Multi-language: GLSL, WGSL, HLSL (and SPIR-V) via `naga`, with cross-translation.

Non-goal (v1): being a full shader IDE, a step debugger, or an engine-grade
material system. Stay lightweight.

## Stack

| Concern | Choice | Why |
|---|---|---|
| Language | Rust, single binary | No runtime deps; matches naga/wgpu |
| MCP | `rmcp` (official Rust SDK), stdio transport | Standard for Claude Code local servers |
| Parse/validate/translate | `naga` | One lib covers GLSL/WGSL/HLSL/SPIR-V + cross-compile + validation |
| Render (native) | `wgpu` headless, **software fallback** | Fast default; cross-platform offscreen render; works in CI / no GPU |
| Render (browser) | headless system Chromium via CDP (`chromiumoxide`) | Runtime-accurate WebGL2/WebGPU + real console/driver errors; Shadertoy-faithful. (Playwright is a documented alternative; CDP keeps a single binary.) |
| Images | `image` | PNG encode, montage, diff |
| Network | `reqwest` | Shadertoy API + optional live docs |
| Reference data | embedded static assets | Offline-first bundled reference |

The two render backends are complementary: **native** is the fast, deterministic
default for tight loops and CI; **browser** is ground truth for runtime errors
and for shaders authored against the browser driver (especially Shadertoy
imports). naga's static validation and the browser's runtime errors catch
different classes of problems.

## Render Model

Canonical render path is a **Shadertoy-style fullscreen-quad fragment pass**:

- The harness owns the vertex stage and a fullscreen triangle.
- Standard uniforms supplied: `iResolution`, `iTime`, `iTimeDelta`, `iFrame`,
  `iMouse`, `iChannel0..3`.
- v1 channels: solid color + a small set of bundled textures/noise. **Multipass
  / buffer ping-pong, audio/video/cubemap channels are deferred** (documented
  roadmap) to keep v1 lightweight.
- Also accepts a full custom vertex+fragment pair when the user supplies both.
- Language auto-detected (overridable). Non-GLSL fragment entry points are
  adapted to the harness uniform set.

The native renderer prefers a real GPU adapter and falls back to a software
adapter (e.g. Vulkan software / DX WARP), always reporting which backend was
used.

### Browser execution harness

A second execution path runs the shader in a **real browser GPU pipeline** by
driving a headless system Chromium over CDP. The tool generates an HTML+JS
harness page:

- **WebGL2 / GLSL ES 3.00** canvas for Shadertoy-convention GLSL (matches
  Shadertoy's actual runtime exactly).
- **WebGPU / WGSL** canvas where the browser supports it.
- The harness runs a loop over N frames (so animation/runtime errors surface,
  not just compile errors), then yields.

It captures, into one structured transcript:

- every `console.log/info/warn/error` line and uncaught JS exception/stack;
- WebGL `getShaderInfoLog`, `getProgramInfoLog`, and `gl.getError()` codes
  (vertex + fragment, per compile/link stage);
- WebGPU `device.pushErrorScope`/`uncapturederror` validation messages;
- per-frame timing and the final canvas as a PNG screenshot.

This is the "keep executing on the GPU and see all the console errors" path:
ground-truth runtime behaviour the static naga check cannot produce.

## MCP Tools (surface)

| Tool | Input → Output |
|---|---|
| `validate_shader` | source, lang? → structured errors/warnings (line:col), entry points, detected language |
| `render_shader` | source, lang?, w, h, time, mouse, channels, **backend (`native`\|`browser`, default `native`)** → **PNG image content block** + warnings + backend used |
| `render_animation` | source, time range, frame count, backend → **contact-sheet montage PNG** (+ optional per-frame) |
| `run_in_browser` | source, lang?, w, h, frame count → canvas screenshot PNG + **full transcript: console log/warn/error, JS exceptions, WebGL `getShaderInfoLog`/`getProgramInfoLog`/`gl.getError`, WebGPU validation errors**, per-frame timing |
| `visualize_expression` | source, expression, mapping (grayscale/RG/RGB/normalized/heatmap) → rendered PNG of that expression |
| `probe_pixels` | source, list of coords or a region → exact RGBA floats + zoomed crop PNG |
| `diff_shaders` / `golden_test` | A vs B (or vs stored reference PNG), tolerance → metrics (max/mean abs diff, % pixels over tol) + highlighted diff PNG + pass/fail |
| `translate_shader` | source, from, to → translated source + notes |
| `shadertoy_get` | id or URL → code + render passes + metadata, harness-adapted (`SHADERTOY_API_KEY`; graceful if absent) |
| `shadertoy_search` | query → result list (id, title, author, thumbnail meta) |
| `lookup_reference` | query, online? → curated bundled reference entry; optional live docs fallback |

### MCP Resources

`shadereye://reference/...` - exposes the bundled curated reference (GLSL/WGSL
builtins, Shadertoy uniform conventions, common gotchas/patterns) so the model
can browse it directly.

## Crate Layout

Cargo workspace, each lib pure (input → output, no MCP coupling) and
independently unit-testable:

- `shadereye-mcp` - binary; `rmcp` wiring and tool/resource registration only.
- `shadereye-compile` - naga validate/translate, language detection,
  Shadertoy→harness source adaptation.
- `shadereye-render` - wgpu headless device, fragment harness, montage builder,
  pixel probe, image diff. Pure: (source, params) → image/data.
- `shadereye-browser` - generates the WebGL2/WebGPU HTML+JS harness, manages a
  headless system Chromium over CDP (`chromiumoxide`), collects console/JS/GL/GPU
  error transcript + canvas screenshot. Pure: (source, params) → (image,
  transcript).
- `shadereye-shadertoy` - Shadertoy API client + harness adaptation of fetched
  passes.
- `shadereye-reference` - embedded reference data, lookup, optional best-effort
  web fetch.

Boundary test: each lib can be exercised and golden-tested without starting an
MCP server.

## Error Handling

- Shader compile/parse errors are **returned as structured tool results**, never
  panics - the model reads them and self-corrects.
- Native renderer: GPU adapter → software fallback; the chosen backend is
  reported in every render result.
- Browser backend: if no system Chromium is found it returns an actionable
  message (how to install / point at a binary) and the caller can fall back to
  the native backend; a documented opt-in can auto-download a browser. Browser
  shader-compile failures are surfaced as the captured driver log, not a panic.
- Network tools (Shadertoy, live docs) degrade gracefully with actionable
  messages: missing `SHADERTOY_API_KEY`, offline, rate-limited, not found.
- Reference lookup is offline-first; online fetch is a best-effort, flagged
  fallback that never blocks the bundled answer.

## Testing

- Unit tests per lib crate.
- Golden-image tests for `shadereye-render` using small fixed example shaders;
  the `golden_test` tool dogfoods the same diff engine.
- GitHub Actions CI builds and tests on the **software render backend** so it
  runs without a GPU. Browser-backend tests run against the CI-provided
  Chromium; they are gated/skippable so a missing browser never red-fails the
  core suite.

## Showcase Deliverables

This is a public portfolio project - completeness matters:

- **README**: the "LLMs are blind to shaders" hook; screenshots/GIF of the
  compile→see→fix loop; install (`cargo install` and prebuilt release binaries);
  Claude Code MCP config snippet; tool reference table; two example transcripts -
  "Claude fixes a broken raymarcher" (native) and "Claude pastes a Shadertoy
  shader, runs it in-browser, reads the real WebGL compile log + screenshot, and
  fixes the precision error" (browser backend).
- `examples/` - GLSL + WGSL example shaders, reused by tests and the README.
- `docs/gallery.md` - rendered outputs.
- GitHub Actions: CI workflow + release workflow building cross-platform
  binaries.
- `LICENSE` (MIT), short `CONTRIBUTING.md`.
- Public repo `github.com/zajalist/shadereye`.

## Scope Cuts (YAGNI for "lightweight")

- v1 = single fragment pass. Multipass, buffer ping-pong, audio/video iChannels,
  cubemaps → roadmap, not v1.
- Browser backend uses a system Chromium via CDP; no bundled Node/Playwright and
  no browser auto-download by default (opt-in only). This keeps the single-binary
  promise intact.
- Live docs fetch is thin and behind a flag; bundled reference is primary.
- No in-shader step debugger / printf; `probe_pixels` + `visualize_expression`
  cover that need pragmatically.

## Roadmap (post-v1)

- Shadertoy multipass / buffer passes.
- Texture/audio/cubemap channels.
- Optional MSL/HLSL target packaging.
- Browser backend: WebGPU/WGSL as canvas support matures; optional Playwright
  driver; interactive watch session that streams console errors live.
- Watch mode / incremental re-render hints.
