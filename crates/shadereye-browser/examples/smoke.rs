fn main() {
    let t = shadereye_browser::run_in_browser(&shadereye_browser::BrowserRunParams {
        source: "void mainImage(out vec4 o, in vec2 fc){ o=vec4(1.0,0.5,0.0,1.0); }".into(),
        width: 64,
        height: 64,
        frames: 3,
    })
    .unwrap();
    println!(
        "compiled_ok={} console_lines={} shot_bytes={}",
        t.compiled_ok,
        t.console.len(),
        t.screenshot_png.len()
    );
}
