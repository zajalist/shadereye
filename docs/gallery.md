# Gallery

Rendered outputs from the bundled `examples/` shaders. The images below are
generated post-build by running the example shaders through the `shadereye-mcp`
`render_shader` tool; they are not committed as part of the source tree.

## Plasma

`examples/plasma.glsl` — an animated Shadertoy-style plasma using stacked sines
of the UV coordinates modulated by `iTime`, mapped through a cosine palette.

## Mandelbrot

A classic escape-time Mandelbrot set, colored by iteration count — a good
stress test for the `visualize_expression` and `probe_pixels` tools.

## Raymarch

`examples/raymarch_broken.glsl` — the intentionally broken raymarcher from the
README transcript (integer-division gotcha), shown before and after the fix to
demonstrate the compile→see→fix loop.
