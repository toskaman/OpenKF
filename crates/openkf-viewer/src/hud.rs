//! Minimal 2D HUD: CPU-rasterised 5x7 bitmap text in a small RGBA texture,
//! drawn as screen-space quads (text block + crosshair) over the 3D frame.

const FONT: &[(char, [u8; 7])] = &[
    ('%', [0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011]),
    ('(', [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010]),
    (')', [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000]),
    ('-', [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000]),
    ('.', [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100]),
    ('/', [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000]),
    ('0', [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110]),
    ('1', [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('2', [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111]),
    ('3', [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110]),
    ('4', [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010]),
    ('5', [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110]),
    ('6', [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110]),
    ('7', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000]),
    ('8', [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]),
    ('9', [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100]),
    (':', [0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b01100, 0b00000]),
    ('?', [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100]),
    ('A', [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ('B', [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110]),
    ('C', [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110]),
    ('D', [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110]),
    ('E', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111]),
    ('F', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000]),
    ('G', [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111]),
    ('H', [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ('I', [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('J', [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100]),
    ('K', [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001]),
    ('L', [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111]),
    ('M', [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001]),
    ('N', [0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001]),
    ('O', [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('P', [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000]),
    ('Q', [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101]),
    ('R', [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001]),
    ('S', [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110]),
    ('T', [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('U', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('V', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100]),
    ('W', [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010]),
    ('X', [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001]),
    ('Y', [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('Z', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111]),
    ('|', [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
];

const SHADER: &str = r#"
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@vertex fn vs(@location(0) p: vec2<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var o: VOut; o.pos = vec4<f32>(p, 0.0, 1.0); o.uv = uv; return o;
}
@fragment fn fs(i: VOut) -> @location(0) vec4<f32> { return textureSample(tex, samp, i.uv); }
"#;

pub const TEX_W: u32 = 512;
pub const TEX_H: u32 = 64;
const SCALE: f32 = 3.0;

pub struct Hud {
    pipeline: wgpu::RenderPipeline,
    text_tex: wgpu::Texture,
    text_bg: wgpu::BindGroup,
    cross_bg: wgpu::BindGroup,
    vb: wgpu::Buffer,
    last_text: Vec<String>,
}

fn glyph(c: char) -> Option<[u8; 7]> {
    FONT.iter().find(|g| g.0 == c).map(|g| g.1)
}

/// Rasterises up to 6 lines of text (white with a dark outline) into TEX_W x TEX_H RGBA.
pub fn rasterise(lines: &[String]) -> Vec<u8> {
    let mut px = vec![0u8; (TEX_W * TEX_H * 4) as usize];
    let put = |x: i32, y: i32, rgba: [u8; 4], px: &mut Vec<u8>| {
        if x < 0 || y < 0 || x >= TEX_W as i32 || y >= TEX_H as i32 {
            return;
        }
        let i = ((y as u32 * TEX_W + x as u32) * 4) as usize;
        if rgba[3] >= px[i + 3] {
            px[i..i + 4].copy_from_slice(&rgba);
        }
    };
    for (li, line) in lines.iter().take(6).enumerate() {
        let y0 = 2 + li as i32 * 10;
        for (ci, c) in line.to_uppercase().chars().enumerate() {
            let x0 = 2 + ci as i32 * 6;
            let Some(g) = glyph(c) else { continue };
            for (r, bits) in g.iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        let (x, y) = (x0 + col, y0 + r as i32);
                        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (1, 1)] {
                            put(x + dx, y + dy, [0, 0, 0, 200], &mut px);
                        }
                        put(x, y, [240, 230, 200, 255], &mut px);
                    }
                }
            }
        }
    }
    px
}

fn crosshair() -> Vec<u8> {
    let n = 16u32;
    let mut px = vec![0u8; (n * n * 4) as usize];
    for i in 0..n {
        for (x, y) in [(i, n / 2), (n / 2, i)] {
            let d = (i as i32 - n as i32 / 2).abs();
            if d < 2 { continue; }
            let k = ((y * n + x) * 4) as usize;
            px[k..k + 4].copy_from_slice(&[255, 255, 255, 220]);
        }
    }
    px
}

impl Hud {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("hud"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hud"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&bgl], push_constant_ranges: &[] });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hud"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: "vs",
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout { array_stride: 16, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2] }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: "fs",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Nearest, min_filter: wgpu::FilterMode::Nearest, ..Default::default() });
        let mk = |w: u32, h: u32, data: &[u8]| {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hud tex"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(t.as_image_copy(), data, wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) }, t.size());
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&t.create_view(&Default::default())) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                ],
            });
            (t, bg)
        };
        let (text_tex, text_bg) = mk(TEX_W, TEX_H, &rasterise(&[]));
        let (_cross_tex, cross_bg) = mk(16, 16, &crosshair());
        let vb = device.create_buffer(&wgpu::BufferDescriptor { label: Some("hud vb"), size: 2 * 6 * 16, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        Self { pipeline, text_tex, text_bg, cross_bg, vb, last_text: Vec::new() }
    }

    /// Draws `lines` bottom-left and (optionally) a crosshair over `target`.
    pub fn draw(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, target: &wgpu::TextureView, w: u32, h: u32, lines: &[String], cross: bool) {
        if lines != self.last_text.as_slice() {
            queue.write_texture(self.text_tex.as_image_copy(), &rasterise(lines), wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(TEX_W * 4), rows_per_image: Some(TEX_H) }, self.text_tex.size());
            self.last_text = lines.to_vec();
        }
        let (fw, fh) = (w.max(1) as f32, h.max(1) as f32);
        let quad = |x: f32, y: f32, qw: f32, qh: f32, vmax: f32| -> [[f32; 4]; 6] {
            // pixels (origin top-left) -> NDC; v cropped to [0, vmax]
            let (l, r) = (x / fw * 2.0 - 1.0, (x + qw) / fw * 2.0 - 1.0);
            let (t, b) = (1.0 - y / fh * 2.0, 1.0 - (y + qh) / fh * 2.0);
            [[l, t, 0.0, 0.0], [r, t, 1.0, 0.0], [r, b, 1.0, vmax], [l, t, 0.0, 0.0], [r, b, 1.0, vmax], [l, b, 0.0, vmax]]
        };
        // Only the rows actually used by the text (10 px per line + margin).
        let used = ((lines.len().min(6) * 10 + 4) as u32).min(TEX_H) as f32;
        let (tw, th) = (TEX_W as f32 * SCALE, used * SCALE);
        let text = quad(16.0, fh - th - 12.0, tw, th, used / TEX_H as f32);
        let c = 16.0 * 2.0;
        let cr = quad(fw / 2.0 - c / 2.0, fh / 2.0 - c / 2.0, c, c, 1.0);
        let mut data: Vec<f32> = Vec::with_capacity(48);
        for v in text.iter().chain(cr.iter()) {
            data.extend_from_slice(v);
        }
        queue.write_buffer(&self.vb, 0, bytemuck::cast_slice(&data));
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hud"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vb.slice(..));
            pass.set_bind_group(0, &self.text_bg, &[]);
            pass.draw(0..6, 0..1);
            if cross {
                pass.set_bind_group(0, &self.cross_bg, &[]);
                pass.draw(6..12, 0..1);
            }
        }
        queue.submit([enc.finish()]);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn text_has_pixels() {
        let px = super::rasterise(&["PV 100 | KILLS 3".into()]);
        assert!(px.chunks(4).filter(|p| p[3] == 255).count() > 50);
    }
}
