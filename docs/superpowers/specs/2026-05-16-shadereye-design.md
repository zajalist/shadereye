# shadereye — Design Spec

**Date:** 2026-05-16
**Status:** Approved (brainstorming) — pending spec review
**Repo:** `github.com/zajalist/shadereye` (public) · local `D:\Projects\shadereye`

## Problem

Coding LLMs (Claude Code and peers) write shaders effectively blind. They cannot
see rendered output, cannot inspect intermediate values, and have no tight
compile/perceive/correct loop. The result is slow, error-prone shader work that
relies on the human to be the eyes. There is no lightweight, LLM-shaped tool that
closes this perception gap and connects the model to existing shader knowledge
(Shadertoy, language references).

## Goal

A small, single-binary **MCP server** that gives an LLM:

1. **Perception** — render shaders to images the model can actually view.
2. **Debugging aids** — animation montages, expression visualization, pixel
   probing, image diffing.
3. **Correctness loop** — structured compiler errors and golden-image testing.
4. **Knowledge connection** — Shadertoy import/search and a bundled+online
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
| Render | `wgpu` headless, **software fallback** | Cross-platform offscreen render; works in CI / no GPU |
| Images | `image` | PNG encode, montage, diff |
| Network | `reqwest` | Shadertoy API + optional live docs |
| Reference data | embedded static assets | Offline-first bundled reference |

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

The renderer prefers a real GPU adapter and falls back to a software adapter
(e.g. Vulkan software / DX WARP), always reporting which backend was used.

## MCP Tools (surface)

| Tool | Input → Output |
|---|---|
| `validate_shader` | source, lang? → structured errors/warnings (line:col), entry points, detected language |
| `render_shader` | source, lang?, w, h, time, mouse, channels → **PNG image content block** + warnings + backend used |
| `render_animation` | source, time range, frame count → **contact-sheet montage PNG** (+ optional per-frame) |
| `visualize_expression` | source, expression, mapping (grayscale/RG/RGB/normalized/heatmap) → rendered PNG of that expression |
| `probe_pixels` | source, list of coords or a region → exact RGBA floats + zoomed crop PNG |
| `diff_shaders` / `golden_test` | A vs B (or vs stored reference PNG), tolerance → metrics (max/mean abs diff, % pixels over tol) + highlighted diff PNG + pass/fail |
| `translate_shader` | source, from, to → translated source + notes |
| `shadertoy_get` | id or URL → code + render passes + metadata, harness-adapted (`SHADERTOY_API_KEY`; graceful if absent) |
| `shadertoy_search` | query → result list (id, title, author, thumbnail meta) |
| `lookup_reference` | query, online? → curated bundled reference entry; optional live docs fallback |

### MCP Resources

`shadereye://reference/...` — exposes the bundled curated reference (GLSL/WGSL
builtins, Shadertoy uniform conventions, common gotchas/patterns) so the model
can browse it directly.

## Crate Layout

Cargo workspace, each lib pure (input → output, no MCP coupling) and
independently unit-testable:

- `shadereye-mcp` — binary; `rmcp` wiring and tool/resource registration only.
- `shadereye-compile` — naga validate/translate, language detection,
  Shadertoy→harness source adaptation.
- `shadereye-render` — wgpu headless device, fragment harness, montage builder,
  pixel probe, image diff. Pure: (source, params) → image/data.
- `shadereye-shadertoy` — Shadertoy API client + harness adaptation of fetched
  passes.
- `shadereye-reference` — embedded reference data, lookup, optional best-effort
  web fetch.

Boundary test: each lib can be exercised and golden-tested without starting an
MCP server.

## Error Handling

- Shader compile/parse errors are **returned as structured tool results**, never
  panics — the model reads them and self-corrects.
- Renderer: GPU adapter → software fallback; the chosen backend is reported in
  every render result.
- Network tools (Shadertoy, live docs) degrade gracefully with actionable
  messages: missing `SHADERTOY_API_KEY`, offline, rate-limited, not found.
- Reference lookup is offline-first; online fetch is a best-effort, flagged
  fallback that never blocks the bundled answer.

## Testing

- Unit tests per lib crate.
- Golden-image tests for `shadereye-render` using small fixed example shaders;
  the `golden_test` tool dogfoods the same diff engine.
- GitHub Actions CI builds and tests on the **software render backend** so it
  runs without a GPU.

## Showcase Deliverables

This is a public portfolio project — completeness matters:

- **README**: the "LLMs are blind to shaders" hook; screenshots/GIF of the
  compile→see→fix loop; install (`cargo install` and prebuilt release binaries);
  Claude Code MCP config snippet; tool reference table; an example transcript
  ("Claude fixes a broken raymarcher").
- `examples/` — GLSL + WGSL example shaders, reused by tests and the README.
- `docs/gallery.md` — rendered outputs.
- GitHub Actions: CI workflow + release workflow building cross-platform
  binaries.
- `LICENSE` (MIT), short `CONTRIBUTING.md`.
- Public repo `github.com/zajalist/shadereye`.

## Scope Cuts (YAGNI for "lightweight")

- v1 = single fragment pass. Multipass, buffer ping-pong, audio/video iChannels,
  cubemaps → roadmap, not v1.
- Live docs fetch is thin and behind a flag; bundled reference is primary.
- No in-shader step debugger / printf; `probe_pixels` + `visualize_expression`
  cover that need pragmatically.

## Roadmap (post-v1)

- Shadertoy multipass / buffer passes.
- Texture/audio/cubemap channels.
- Optional MSL/HLSL target packaging.
- Watch mode / incremental re-render hints.
