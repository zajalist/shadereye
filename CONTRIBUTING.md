# Contributing

Thanks for your interest in `shadereye`.

## Build

```sh
cargo build
```

## Test

```sh
cargo test
```

Run `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings` before
opening a PR.

## Crate layout

A Cargo workspace of independently-testable library crates -
`shadereye-compile` (validate/translate), `shadereye-render` (headless `wgpu` +
diff), `shadereye-browser` (CDP harness), `shadereye-shadertoy`,
`shadereye-reference` - plus the thin `shadereye-mcp` binary that registers them
as MCP tools over stdio.

## Browser tests

The browser backend needs a system Chrome/Chromium. shadereye auto-detects it;
if it can't be found, set the `SHADEREYE_BROWSER` environment variable to the
browser executable path. Browser tests are skipped when no browser is available
so a missing browser never fails the core suite.
