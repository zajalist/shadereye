//! Render N frames of a shader file across a time range to zero-padded PNGs.
//!
//! Usage:
//!   cargo run -p shadereye-render --example dump_frames -- \
//!       <shader_path> <out_dir> <width> <height> <frames> <t0> <t1>
//!
//! Writes `<out_dir>/frame_0000.png` .. `frame_{N-1:04}.png`. Frame times are
//! evenly spaced over the inclusive range [t0, t1] (a single frame uses t0).

use std::process::ExitCode;

use shadereye_render::{render, RenderParams};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 8 {
        eprintln!(
            "usage: {} <shader_path> <out_dir> <width> <height> <frames> <t0> <t1>",
            args.first().map(String::as_str).unwrap_or("dump_frames")
        );
        return ExitCode::FAILURE;
    }

    let shader_path = &args[1];
    let out_dir = &args[2];
    let width: u32 = args[3].parse().expect("width must be a u32");
    let height: u32 = args[4].parse().expect("height must be a u32");
    let frames: u32 = args[5].parse().expect("frames must be a u32");
    let t0: f32 = args[6].parse().expect("t0 must be an f32");
    let t1: f32 = args[7].parse().expect("t1 must be an f32");

    let source = match std::fs::read_to_string(shader_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to read {shader_path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = std::fs::create_dir_all(out_dir) {
        eprintln!("failed to create {out_dir}: {e}");
        return ExitCode::FAILURE;
    }

    let frames = frames.max(1);
    for i in 0..frames {
        let t = if frames == 1 {
            t0
        } else {
            t0 + (t1 - t0) * (i as f32 / (frames - 1) as f32)
        };
        let params = RenderParams {
            source: source.clone(),
            lang: None,
            width,
            height,
            time: t,
            mouse: [0.0; 4],
        };
        let out = match render(&params) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("render failed at frame {i} (t={t}): {e}");
                return ExitCode::FAILURE;
            }
        };
        let path = format!("{out_dir}/frame_{i:04}.png");
        if let Err(e) = std::fs::write(&path, &out.png) {
            eprintln!("failed to write {path}: {e}");
            return ExitCode::FAILURE;
        }
        println!(
            "frame {:>4}/{} t={:.4} -> {} ({}x{}, {})",
            i + 1,
            frames,
            t,
            path,
            out.width,
            out.height,
            out.backend
        );
    }

    println!("done: {frames} frame(s) in {out_dir}");
    ExitCode::SUCCESS
}
