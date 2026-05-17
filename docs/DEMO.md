# Recording a shadereye demo

This is a self-filming kit for producing a short shadereye demo video. It
covers the MCP config, a vetted demo prompt, a 60-second shot list, and
recording instructions for asciinema, vhs, and OBS.

## 1. Claude Code MCP config

Build/install the binary first:

```sh
cargo install --git https://github.com/zajalist/shadereye shadereye-mcp
```

Then add this to your Claude Code MCP server config
(`~/.claude.json` or the project `.mcp.json`):

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

`SHADERTOY_API_KEY` is optional — only `shadertoy_get` / `shadertoy_search`
need it. Every other tool (validate, render, visualize, probe, diff,
translate, run_in_browser, lookup_reference) works without any key.

Confirm the server is connected: in Claude Code run `/mcp` and you should see
`shadereye` with 11 tools.

## 2. Vetted demo prompt

Run this from the repo root (so `examples/raymarch_broken.glsl` resolves). It
reliably exercises **render → lookup_reference → fix → render**, and you can
prepend a validate step:

> Render `examples/raymarch_broken.glsl` with the shadereye `render_shader`
> tool and tell me why it looks wrong. Then call `lookup_reference` to confirm
> the GLSL rule involved, edit the file to fix it, and `render_shader` again to
> prove the blue channel is restored. Keep narration short.

Expected tool sequence: `render_shader` (flat red↔green, no blue) →
`lookup_reference("integer division")` (returns the **integer-division**
gotcha: *"In GLSL, 1/2 == 0 (integer division). Use 1.0/2.0 for float
results."*) → edit `float t = 1 / 2;` → `float t = 1.0 / 2.0;` →
`render_shader` (now shows blue). Total: ~4 tool calls, ~45s.

Optional browser variant (shows the WebGL2 ground-truth path): ask Claude to
also `run_in_browser` on a shader missing `precision highp float;` to surface
the real `getShaderInfoLog` and the **webgl-precision** gotcha.

## 3. 60-second shot list / script

| Time | Shot | Narration / on-screen |
|---|---|---|
| 0:00–0:06 | Title card / README top | "shadereye gives a coding LLM eyes for shaders." |
| 0:06–0:14 | Claude Code with `/mcp` showing 11 shadereye tools | "11 MCP tools: render, probe, diff, browser, reference." |
| 0:14–0:24 | Paste the demo prompt, first `render_shader` returns the broken image inline | "It renders the shader and *sees* a flat red-green gradient — no blue." |
| 0:24–0:36 | `lookup_reference("integer division")` returns the bundled gotcha | "It looks up the GLSL gotcha: 1/2 is integer division → 0." |
| 0:36–0:46 | Claude edits the line, calls `render_shader` again, blue appears | "One-line fix, re-render, blue channel restored." |
| 0:46–0:56 | (optional) `run_in_browser` precision example | "The browser backend surfaces the real WebGL compile log." |
| 0:56–1:00 | Cut to `media/plasma.gif` / `media/webgl_demo.gif` | "Same engine, animated, native and in a real browser GPU." |

## 4. Recording instructions

### asciinema (terminal session)

```sh
asciinema rec shadereye-demo.cast --cols 120 --rows 32
# run the demo prompt in Claude Code, then Ctrl-D to stop
# convert to GIF with agg (https://github.com/asciinema/agg):
agg --font-size 18 shadereye-demo.cast shadereye-demo.gif
```

### vhs (deterministic, scripted)

`media/loop.tape` reproduces the animated terminal transcript. If you install
[vhs](https://github.com/charmbracelet/vhs):

```sh
vhs media/loop.tape      # writes media/loop.gif
```

Edit the `Type`/`Sleep` lines in the tape to retime or change the narrative.

### OBS (screen capture of the real Claude Code session)

1. Scene → Display/Window Capture of the Claude Code terminal.
2. Output: 1280×720, 30 fps, MP4 (or record then `ffmpeg` to GIF).
3. Start recording, run the demo prompt, stop after the second render.
4. Trim and (optionally) convert to GIF:

```sh
ffmpeg -i rec.mp4 -vf "fps=12,scale=960:-1:flags=lanczos,palettegen" pal.png
ffmpeg -i rec.mp4 -i pal.png -lavfi "fps=12,scale=960:-1:flags=lanczos[x];[x][1:v]paletteuse" demo.gif
```

## 5. Tips

- **Font size ≥ 16** so code is legible after compression.
- **1280×720** is the safe target; crop tighter for GIFs to cut size.
- **Hide secrets**: clear `SHADERTOY_API_KEY` from visible shell history /
  env panes; the key is not needed for the core demo.
- Run from the repo root so `examples/raymarch_broken.glsl` resolves.
- Keep narration short — the inline rendered images carry the story.
- Prefer MP4 for anything over ~5 MB; ship the GIF only for embeds.
- Do a dry run first: model phrasing varies, but the tool sequence
  (render → lookup_reference → edit → render) is stable with the prompt above.
