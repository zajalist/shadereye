# Gallery

Rendered outputs from the bundled `examples/` shaders. The images below are
generated post-build by running the example shaders through the `shadereye-mcp`
`render_shader` tool; they are not committed as part of the source tree.

## Plasma

`examples/plasma.glsl` — an animated Shadertoy-style plasma using stacked sines
of the UV coordinates modulated by `iTime`, mapped through a cosine palette.

## Solid (WGSL)

`examples/solid.wgsl` — a minimal WGSL fragment shader, used to demonstrate the
multi-language path and `translate_shader` (WGSL ↔ GLSL).

## Raymarch

`examples/raymarch_broken.glsl` — the intentionally broken raymarcher from the
README transcript (integer-division gotcha), shown before and after the fix to
demonstrate the compile→see→fix loop.
