use shadereye_render::*;

#[test]
fn plasma_example_renders_nonblack() {
    let src = include_str!("../../../examples/plasma.glsl");
    let o = render(&RenderParams {
        source: src.into(),
        lang: None,
        width: 64,
        height: 64,
        time: 1.0,
        mouse: [0.0; 4],
    })
    .expect("render");
    // not entirely black
    let any = (0..o.width).any(|x| {
        let p = o.pixel(x, 32);
        p[0] as u32 + p[1] as u32 + p[2] as u32 > 30
    });
    assert!(any, "plasma rendered all black");
}

#[test]
fn broken_raymarch_compiles_but_blue_channel_is_zero() {
    let src = include_str!("../../../examples/raymarch_broken.glsl");
    let o = render(&RenderParams {
        source: src.into(),
        lang: None,
        width: 32,
        height: 32,
        time: 0.0,
        mouse: [0.0; 4],
    })
    .expect("render");
    // t == 0 because of integer division; blue channel ~0 across the image
    assert!(
        o.pixel(16, 16)[2] < 10,
        "expected blue~0 from the integer-division bug"
    );
}
