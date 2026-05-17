//! Headless wgpu rendering of Shadertoy-style fragment shaders.

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
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
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
        Ok(Gpu {
            device,
            queue,
            backend,
        })
    })
}

/// Render one frame of a Shadertoy-style (or full GLSL/WGSL) fragment shader.
pub fn render(params: &RenderParams) -> Result<RenderOutput, RenderError> {
    let gpu = acquire_gpu()?;
    let lang = params
        .lang
        .unwrap_or_else(|| shadereye_compile::detect_language(&params.source));

    // Build the fragment module. GLSL is wrapped + handed to wgpu's naga path.
    let fs_module = match lang {
        ShaderLang::Wgsl => gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("fs-wgsl"),
                source: wgpu::ShaderSource::Wgsl(params.source.clone().into()),
            }),
        ShaderLang::Glsl => {
            let wrapped = wrap_shadertoy_fragment(&params.source);
            gpu.device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("fs-glsl"),
                    source: wgpu::ShaderSource::Glsl {
                        shader: wrapped.into(),
                        stage: wgpu::naga::ShaderStage::Fragment,
                        defines: Default::default(),
                    },
                })
        }
        other => {
            return Err(RenderError::ShaderCompile(format!(
                "unsupported render language {other:?}"
            )))
        }
    };
    let vs_module = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
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
    let ubo = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ubo"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
    let bgl = gpu
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: ubo.as_entire_binding(),
        }],
    });
    let pll = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

    let fmt = wgpu::TextureFormat::Rgba8UnormSrgb;
    let pipeline = gpu
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pll),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
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
        size: wgpu::Extent3d {
            width: params.width,
            height: params.height,
            depth_or_array_layers: 1,
        },
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
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
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
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &readback,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bpr),
                rows_per_image: Some(params.height),
            },
        },
        wgpu::Extent3d {
            width: params.width,
            height: params.height,
            depth_or_array_layers: 1,
        },
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

    Ok(RenderOutput {
        width: params.width,
        height: params.height,
        png,
        rgba,
        backend: gpu.backend,
    })
}

/// Render `frames` evenly spaced over [t0, t1] and tile them left→right into one PNG.
pub fn render_animation(
    base: &RenderParams,
    t0: f32,
    t1: f32,
    frames: u32,
) -> Result<RenderOutput, RenderError> {
    let frames = frames.max(1);
    let mut tiles = Vec::new();
    for i in 0..frames {
        let t = if frames == 1 {
            t0
        } else {
            t0 + (t1 - t0) * (i as f32 / (frames - 1) as f32)
        };
        let p = RenderParams {
            time: t,
            ..base.clone()
        };
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
    Ok(RenderOutput {
        width: total_w,
        height: fh,
        png,
        rgba,
        backend: tiles[0].backend.clone(),
    })
}

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
        assert_eq!(
            &out.png[0..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
        // center pixel is red
        let px = out.pixel(8, 8);
        assert!(px[0] > 200 && px[1] < 50 && px[2] < 50, "got {px:?}");
        assert!(!out.backend.is_empty());
    }

    #[test]
    fn animation_makes_a_montage_wider_than_one_frame() {
        let body =
            "void mainImage(out vec4 o, in vec2 fc){ o = vec4(fract(iTime), 0.0, 0.0, 1.0); }";
        let m = render_animation(
            &RenderParams {
                source: body.into(),
                lang: None,
                width: 16,
                height: 16,
                time: 0.0,
                mouse: [0.0; 4],
            },
            0.0,
            1.0,
            4,
        )
        .expect("anim ok");
        // 4 frames laid horizontally => width >= 4*16
        assert!(m.width >= 64, "montage width {}", m.width);
        assert_eq!(
            &m.png[0..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
        );
    }
}
