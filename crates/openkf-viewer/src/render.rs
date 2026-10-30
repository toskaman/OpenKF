//! wgpu renderer (Vulkan on Windows): one pipeline, one draw call per texture batch.

use crate::assets::{Blend, TexFormat};
use crate::camera::Camera;
use crate::scene::{Scene, Vertex};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    fog: [f32; 4],
}

struct GpuBatch {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    count: u32,
    bind: usize,
    /// Lightmap bind group index (last = neutral fallback).
    lm: usize,
    blend: Blend,
}

pub struct Renderer {
    /// Indexed by pipeline kind: 0 opaque/masked, 1 alpha, 2 additive, 3 modulate.
    pipelines: Vec<wgpu::RenderPipeline>,
    /// [main, sky] uniform buffers / bind groups.
    globals: [wgpu::Buffer; 2],
    globals_bg: [wgpu::BindGroup; 2],
    sky_pos: Option<glam::Vec3>,
    material_bgs: Vec<wgpu::BindGroup>,
    lightmap_bgs: Vec<wgpu::BindGroup>,
    batches: Vec<GpuBatch>,
    terrain_pipeline: wgpu::RenderPipeline,
    /// Animated actors: dynamic vertex buffer + per-slot (index buffer, count, material bind).
    actors: Vec<(wgpu::Buffer, Vec<(wgpu::Buffer, u32, usize)>)>,
    /// Movable static meshes: same layout as `actors` (dynamic vertex buffer per pool slot).
    props: Vec<(wgpu::Buffer, Vec<(wgpu::Buffer, u32, usize)>)>,
    /// Props drawn this frame (hidden pool slots are skipped entirely).
    prop_visible: Vec<bool>,
    /// (vertex buffer, index buffer, index count, per-layer bind groups)
    terrains: Vec<(wgpu::Buffer, wgpu::Buffer, u32, Vec<wgpu::BindGroup>)>,
    depth: Option<(wgpu::TextureView, u32, u32)>,
    pub triangles: u64,
}

pub fn required_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, color_format: wgpu::TextureFormat, scene: &Scene) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("openkf shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let mat_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let lm_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lightmap"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("layout"),
            bind_group_layouts: &[&globals_layout, &mat_layout, &lm_layout],
            push_constant_ranges: &[],
        });
        let make = |label: &str, blend: Option<wgpu::BlendState>, depth_write: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Unorm8x4, 4 => Float32x2],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: if blend.is_some() { "fs_translucent" } else { "fs_main" },
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: color_format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: Some(wgpu::Face::Back),
                    front_face: wgpu::FrontFace::Ccw,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: depth_write,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let comp = |src, dst| wgpu::BlendComponent { src_factor: src, dst_factor: dst, operation: wgpu::BlendOperation::Add };
        use wgpu::BlendFactor as F;
        let pipelines = vec![
            make("opaque", None, true),
            make("alpha", Some(wgpu::BlendState::ALPHA_BLENDING), false),
            make("additive", Some(wgpu::BlendState { color: comp(F::One, F::One), alpha: comp(F::One, F::One) }), false),
            make("modulate", Some(wgpu::BlendState { color: comp(F::Dst, F::Zero), alpha: comp(F::Dst, F::Zero) }), false),
        ];

        let mk_globals = || {
            let b = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("globals"),
                size: std::mem::size_of::<Globals>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("globals"),
                layout: &globals_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: b.as_entire_binding() }],
            });
            (b, bg)
        };
        let (g0, bg0) = mk_globals();
        let (g1, bg1) = mk_globals();

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("aniso"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 16,
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0,
            ..Default::default()
        });
        // HAL spec -0.5 LOD bias documented in texture_cache.rs:197-216 (wgpu bias via shader if needed).

        // Textures (+ one fallback at the end for untextured batches).
        let mut material_bgs = Vec::with_capacity(scene.textures.len() * 3 + 3);
        let mut views: Vec<wgpu::TextureView> = Vec::with_capacity(scene.textures.len() + 1);
        let fallback = crate::assets::TextureData { width: 1, height: 1, format: TexFormat::Rgba8, mips: vec![vec![180, 180, 180, 255]], alpha_test: false, luminance: false };
        for t in scene.textures.iter().chain(std::iter::once(&fallback)) {
            let (format, block, bbytes) = match t.format {
                TexFormat::Bc1 => (wgpu::TextureFormat::Bc1RgbaUnormSrgb, 4, 8),
                TexFormat::Bc2 => (wgpu::TextureFormat::Bc2RgbaUnormSrgb, 4, 16),
                TexFormat::Bc3 => (wgpu::TextureFormat::Bc3RgbaUnormSrgb, 4, 16),
                TexFormat::Rgba8 => (wgpu::TextureFormat::Rgba8UnormSrgb, 1, 4),
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: t.width, height: t.height, depth_or_array_layers: 1 },
                mip_level_count: t.mips.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (level, data) in t.mips.iter().enumerate() {
                let w = (t.width >> level).max(1);
                let h = (t.height >> level).max(1);
                let (bw, bh) = (w.div_ceil(block), h.div_ceil(block));
                queue.write_texture(
                    wgpu::ImageCopyTexture { texture: &texture, mip_level: level as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    data,
                    wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(bw * bbytes), rows_per_image: Some(bh) },
                    wgpu::Extent3d { width: bw * block, height: bh * block, depth_or_array_layers: 1 },
                );
            }
            let view = texture.create_view(&Default::default());
            // Slot 3*i: texture's own alpha-test flag; 3*i+1: forced on; 3*i+2: forced off.
            for variant in 0..3 {
                let at = match variant { 0 => t.alpha_test, 1 => true, _ => false };
                let mat = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&[if at { 1.0f32 } else { 0.0 }, 0.0, 0.0, 0.0]),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                material_bgs.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &mat_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                        wgpu::BindGroupEntry { binding: 2, resource: mat.as_entire_binding() },
                    ],
                }));
            }
            views.push(view);
        }
        let fallback_tex = scene.textures.len();

        // Lightmaps (+ neutral 50% grey fallback = 1.0 after the 2x modulate).
        let lm_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("lightmap"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bc = device.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        let mut lightmap_bgs = Vec::with_capacity(scene.lightmaps.len() + 1);
        let neutral = openkf_package::LightMapTexture { width: 0, dxt5: false, mips: vec![] };
        for lm in scene.lightmaps.iter().chain(std::iter::once(&neutral)) {
            let (format, block, bbytes, w, mips): (_, u32, u32, u32, Vec<Vec<u8>>) = if lm.width == 0 {
                (wgpu::TextureFormat::Rgba8Unorm, 1, 4, 1, vec![vec![128, 128, 128, 255]])
            } else if bc {
                let f = if lm.dxt5 { wgpu::TextureFormat::Bc3RgbaUnorm } else { wgpu::TextureFormat::Bc1RgbaUnorm };
                (f, 4, if lm.dxt5 { 16 } else { 8 }, lm.width, lm.mips.clone())
            } else {
                // CPU-decode level 0 only.
                let px = openkf_package::decode_bc_rgba(&lm.mips[0], lm.width, lm.width, lm.dxt5);
                (wgpu::TextureFormat::Rgba8Unorm, 1, 4, lm.width, vec![px])
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("lightmap"),
                size: wgpu::Extent3d { width: w, height: w, depth_or_array_layers: 1 },
                mip_level_count: mips.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (level, data) in mips.iter().enumerate() {
                let lw = (w >> level).max(1);
                let bw = lw.div_ceil(block);
                queue.write_texture(
                    wgpu::ImageCopyTexture { texture: &texture, mip_level: level as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    data,
                    wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(bw * bbytes), rows_per_image: Some(bw) },
                    wgpu::Extent3d { width: bw * block, height: bw * block, depth_or_array_layers: 1 },
                );
            }
            let view = texture.create_view(&Default::default());
            lightmap_bgs.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &lm_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&lm_sampler) },
                ],
            }));
        }
        let neutral_lm = lightmap_bgs.len() - 1;

        let mut triangles = 0u64;
        let batches = scene
            .batches
            .iter()
            .map(|b| {
                triangles += b.indices.len() as u64 / 3;
                GpuBatch {
                    vb: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: None,
                        contents: bytemuck::cast_slice(&b.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    ib: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: None,
                        contents: bytemuck::cast_slice(&b.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    count: b.indices.len() as u32,
                    bind: b.texture.unwrap_or(fallback_tex) * 3 + (b.blend == Blend::Masked) as usize,
                    lm: if b.lightmap >= 0 { b.lightmap as usize } else { neutral_lm },
                    blend: b.blend,
                }
            })
            .collect();

        // ---------------------------------------------------------- terrain
        let tshader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("terrain.wgsl").into()),
        });
        let tex_entry = |b| wgpu::BindGroupLayoutEntry {
            binding: b,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let samp_entry = |b| wgpu::BindGroupLayoutEntry {
            binding: b,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let tlayout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain layer"),
            entries: &[
                tex_entry(0),
                tex_entry(1),
                samp_entry(2),
                samp_entry(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let tpl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain layout"),
            bind_group_layouts: &[&globals_layout, &tlayout],
            push_constant_ranges: &[],
        });
        let terrain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain"),
            layout: Some(&tpl),
            vertex: wgpu::VertexState {
                module: &tshader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Unorm8x4, 4 => Float32x2],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &tshader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let clamp_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let fallback_view = views.len() - 1;
        let terrains = scene
            .terrains
            .iter()
            .map(|t| {
                triangles += t.indices.len() as u64 / 3 * t.layers.len().max(1) as u64;
                let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&t.vertices), usage: wgpu::BufferUsages::VERTEX });
                let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&t.indices), usage: wgpu::BufferUsages::INDEX });
                let bgs = t
                    .layers
                    .iter()
                    .enumerate()
                    .map(|(li, l)| {
                        // First layer is the opaque base; later layers are weighted by their alpha map.
                        let has_alpha = li > 0 && l.alpha.is_some();
                        let lum = l.alpha.map(|a| scene.textures[a].luminance).unwrap_or(false);
                        let params = [l.repeat[0], l.repeat[1], lum as u32 as f32, has_alpha as u32 as f32];
                        let ub = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&params), usage: wgpu::BufferUsages::UNIFORM });
                        device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: None,
                            layout: &tlayout,
                            entries: &[
                                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&views[l.texture.unwrap_or(fallback_view)]) },
                                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[l.alpha.unwrap_or(fallback_view)]) },
                                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
                                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&clamp_sampler) },
                                wgpu::BindGroupEntry { binding: 4, resource: ub.as_entire_binding() },
                            ],
                        })
                    })
                    .collect();
                (vb, ib, t.indices.len() as u32, bgs)
            })
            .collect();

        let actors = scene
            .actors
            .iter()
            .map(|a| {
                let vb = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("actor vb"),
                    size: (a.rig.mesh.wedges.len().max(1) * std::mem::size_of::<Vertex>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let slots = a
                    .slots
                    .iter()
                    .map(|(mt, idx)| {
                        triangles += idx.len() as u64 / 3;
                        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(idx), usage: wgpu::BufferUsages::INDEX });
                        let bind = mt.map(|m| m.0).unwrap_or(fallback_tex) * 3 + if matches!(mt.map(|m| m.1), Some(Blend::Masked)) { 1 } else { 2 };
                        (ib, idx.len() as u32, bind)
                    })
                    .collect();
                (vb, slots)
            })
            .collect();

        let props: Vec<(wgpu::Buffer, Vec<(wgpu::Buffer, u32, usize)>)> = scene
            .props
            .iter()
            .map(|p| {
                let vb = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("prop vb"),
                    size: (p.mesh.positions.len().max(1) * std::mem::size_of::<Vertex>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let slots = p
                    .slots
                    .iter()
                    .map(|(mt, idx)| {
                        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(idx), usage: wgpu::BufferUsages::INDEX });
                        let bind = mt.map(|m| m.0).unwrap_or(fallback_tex) * 3 + if matches!(mt.map(|m| m.1), Some(Blend::Masked)) { 1 } else { 2 };
                        (ib, idx.len() as u32, bind)
                    })
                    .collect();
                (vb, slots)
            })
            .collect();

        Self {
            actors,
            prop_visible: vec![false; props.len()],
            props,
            terrain_pipeline,
            terrains,
            pipelines,
            globals: [g0, g1],
            globals_bg: [bg0, bg1],
            sky_pos: scene.sky_pos.map(glam::Vec3::from),
            material_bgs,
            lightmap_bgs,
            batches,
            depth: None,
            triangles,
        }
    }

    fn depth_view(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        if self.depth.as_ref().is_none_or(|d| d.1 != w || d.2 != h) {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.depth = Some((t.create_view(&Default::default()), w, h));
        }
    }

    /// Uploads freshly skinned actor vertices (one Vec per actor, same order as the scene).
    pub fn update_actors(&self, queue: &wgpu::Queue, verts: &[Vec<Vertex>]) {
        for ((vb, _), v) in self.actors.iter().zip(verts) {
            queue.write_buffer(vb, 0, bytemuck::cast_slice(v));
        }
    }

    /// Uploads re-transformed prop vertices (one Vec per pool slot, scene order).
    /// An empty Vec marks a hidden slot (not uploaded, not drawn).
    pub fn update_props(&mut self, queue: &wgpu::Queue, verts: &[Vec<Vertex>]) {
        for (i, ((vb, _), v)) in self.props.iter().zip(verts).enumerate() {
            if !v.is_empty() {
                queue.write_buffer(vb, 0, bytemuck::cast_slice(v));
            }
            self.prop_visible[i] = !v.is_empty();
        }
    }

    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, target: &wgpu::TextureView, w: u32, h: u32, cam: &Camera) {
        let aspect = w as f32 / h.max(1) as f32;
        let vp = cam.view_proj(aspect);
        // Frustum culling (CPU) — HZB occlusion approximated by front-to-back + early-Z.
        let _frustum = openkf_render::culling::Frustum::from_view_proj(vp);
        let g = Globals {
            view_proj: vp.to_cols_array_2d(),
            cam_pos: [cam.pos.x, cam.pos.y, cam.pos.z, 1.0],
            fog: [6000.0, 40000.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.globals[0], 0, bytemuck::bytes_of(&g));
        if let Some(sp) = self.sky_pos {
            // Sky zone: same orientation, camera placed at the SkyZoneInfo.
            let sky_cam = Camera { pos: sp, yaw: cam.yaw, pitch: cam.pitch, fov_y: cam.fov_y };
            let gs = Globals {
                view_proj: sky_cam.view_proj(aspect).to_cols_array_2d(),
                cam_pos: [sp.x, sp.y, sp.z, 1.0],
                fog: [1.0e9, 2.0e9, 0.0, 0.0],
            };
            queue.write_buffer(&self.globals[1], 0, bytemuck::bytes_of(&gs));
        }
        self.depth_view(device, w, h);
        let depth = &self.depth.as_ref().unwrap().0;
        let mut enc = device.create_command_encoder(&Default::default());
        let passes: &[usize] = if self.sky_pos.is_some() { &[1, 0] } else { &[0] };
        for (n, &gi) in passes.iter().enumerate() {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(if gi == 1 { "sky" } else { "world" }),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if n == 0 { wgpu::LoadOp::Clear(wgpu::Color { r: 0.06, g: 0.06, b: 0.07, a: 1.0 }) } else { wgpu::LoadOp::Load },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.globals_bg[gi], &[]);
            if !self.terrains.is_empty() {
                pass.set_pipeline(&self.terrain_pipeline);
                for (vb, ib, count, layers) in &self.terrains {
                    pass.set_vertex_buffer(0, vb.slice(..));
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    for bg in layers {
                        pass.set_bind_group(1, bg, &[]);
                        pass.draw_indexed(0..*count, 0, 0..1);
                    }
                }
            }
            if gi == 0 && (!self.actors.is_empty() || !self.props.is_empty()) {
                let neutral_lm = self.lightmap_bgs.len() - 1;
                pass.set_pipeline(&self.pipelines[0]);
                pass.set_bind_group(2, &self.lightmap_bgs[neutral_lm], &[]);
                let vis = &self.prop_visible;
                let props = self.props.iter().enumerate().filter(|(i, _)| vis.get(*i).copied().unwrap_or(false)).map(|(_, p)| p);
                for (vb, slots) in self.actors.iter().chain(props) {
                    pass.set_vertex_buffer(0, vb.slice(..));
                    for (ib, count, bind) in slots {
                        pass.set_bind_group(1, &self.material_bgs[*bind], &[]);
                        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..*count, 0, 0..1);
                    }
                }
            }
            let mut current = usize::MAX;
            for b in &self.batches {
                let kind = match b.blend {
                    Blend::Opaque | Blend::Masked | Blend::Invisible => 0,
                    Blend::Alpha => 1,
                    Blend::Additive => 2,
                    Blend::Modulate => 3,
                };
                if kind != current {
                    pass.set_pipeline(&self.pipelines[kind]);
                    current = kind;
                }
                pass.set_bind_group(1, &self.material_bgs[b.bind], &[]);
                pass.set_bind_group(2, &self.lightmap_bgs[b.lm], &[]);
                pass.set_vertex_buffer(0, b.vb.slice(..));
                pass.set_index_buffer(b.ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..b.count, 0, 0..1);
            }
        }
        queue.submit([enc.finish()]);
    }

    pub fn draw_calls(&self) -> usize {
        self.batches.len() + self.terrains.iter().map(|t| t.3.len()).sum::<usize>() + self.actors.iter().chain(self.props.iter()).map(|a| a.1.len()).sum::<usize>()
    }

    /// Depth buffer view for overlay passes (particles). `None` before first `render`.
    pub fn depth_texture(&self) -> Option<&wgpu::TextureView> {
        self.depth.as_ref().map(|d| &d.0)
    }
}
