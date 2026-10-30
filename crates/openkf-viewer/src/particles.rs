//! CPU particle pool: combat effects (muzzle flash, blood, explosions),
//! ambient emitter smoke, and sticky decals (blood/impact marks).
//!
//! The live thread spawns in UE coordinates; the render thread ticks physics,
//! expands camera-facing quads and uploads them each frame.

/// One particle in UE world units.
#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    /// Seconds left / total.
    pub life: f32,
    pub max_life: f32,
    /// World-unit quad size.
    pub size: f32,
    /// Linear RGB + base alpha.
    pub color: [f32; 4],
    /// Gravity multiplier (0 = floats, 1 = falls like debris).
    pub gravity: f32,
    /// Air drag per second.
    pub drag: f32,
}

/// Surface-aligned mark (blood pool, impact scorch): projected onto the
/// surface plane instead of facing the camera. UE coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Decal {
    pub pos: [f32; 3],
    /// Surface normal (unit, UE).
    pub normal: [f32; 3],
    /// World-unit quad size.
    pub size: f32,
    /// Linear RGB + base alpha.
    pub color: [f32; 4],
    /// Seconds left / total.
    pub life: f32,
    pub max_life: f32,
    /// In-plane rotation seed (blood pools don't all align).
    pub seed: u64,
}

/// Fixed-budget pools (oldest recycled first).
#[derive(Debug, Clone)]
pub struct ParticleSystem {
    pub parts: Vec<Particle>,
    pub budget: usize,
    pub decals: Vec<Decal>,
    pub decal_budget: usize,
    next_seed: u64,
}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self::new(2048)
    }
}

impl ParticleSystem {
    pub fn new(budget: usize) -> Self {
        let budget = budget.max(1);
        Self { parts: Vec::new(), budget, decals: Vec::new(), decal_budget: (budget / 8).max(8), next_seed: 1 }
    }

    pub fn spawn(&mut self, p: Particle) {
        if self.parts.len() >= self.budget {
            self.parts.remove(0);
        }
        self.parts.push(p);
    }

    /// Radial burst (blood, explosion chunks, muzzle sparks).
    pub fn burst(&mut self, pos: [f32; 3], count: usize, color: [f32; 4], speed: f32, life: f32, size: f32, gravity: f32) {
        for i in 0..count {
            // Deterministic spread (no RNG dependency): golden-angle spiral + up bias.
            let a = i as f32 * 2.39996;
            let up = ((i * 37) % 100) as f32 / 100.0;
            let v = [a.cos() * speed * (0.4 + 0.6 * up), a.sin() * speed * (0.4 + 0.6 * up), speed * (0.3 + up)];
            self.spawn(Particle {
                pos,
                vel: v,
                life: life * (0.6 + 0.4 * up),
                max_life: life,
                size: size * (0.7 + 0.6 * up),
                color,
                gravity,
                drag: 1.5,
            });
        }
    }

    /// Sticky mark (blood pool / impact scorch) on the plane of `normal`
    /// (unit, UE): no motion, long fade.
    pub fn decal(&mut self, pos: [f32; 3], normal: [f32; 3], color: [f32; 4], size: f32, life: f32) {
        if self.decals.len() >= self.decal_budget {
            self.decals.remove(0);
        }
        let seed = self.next_seed;
        self.next_seed += 1;
        self.decals.push(Decal { pos, normal, size, color, life, max_life: life, seed });
    }

    /// Rising smoke puff (emitters, explosions).
    pub fn smoke(&mut self, pos: [f32; 3], color: [f32; 4], size: f32, life: f32) {
        self.spawn(Particle { pos, vel: [0.0, 0.0, 30.0], life, max_life: life, size, color, gravity: -0.05, drag: 0.8 });
    }

    pub fn update(&mut self, dt: f32) {
        let dt = dt.min(0.1);
        self.parts.retain_mut(|p| {
            p.life -= dt;
            if p.life <= 0.0 {
                return false;
            }
            p.vel[2] -= 950.0 * p.gravity * dt;
            let d = (1.0 - p.drag * dt).max(0.0);
            p.vel[0] *= d;
            p.vel[1] *= d;
            p.vel[2] *= d;
            p.pos[0] += p.vel[0] * dt;
            p.pos[1] += p.vel[1] * dt;
            p.pos[2] += p.vel[2] * dt;
            true
        });
        self.decals.retain_mut(|d| {
            d.life -= dt;
            d.life > 0.0
        });
    }

    pub fn len(&self) -> usize {
        self.parts.len() + self.decals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty() && self.decals.is_empty()
    }
}

/// Orthonormal in-plane axes for a decal normal (render space), rotated by
/// a deterministic angle from `seed`.
fn decal_basis(n: glam::Vec3, seed: u64) -> (glam::Vec3, glam::Vec3) {
    let n = n.normalize_or_zero();
    let helper = if n.y.abs() > 0.9 { glam::Vec3::X } else { glam::Vec3::Y };
    let t = n.cross(helper).normalize_or_zero();
    let b = n.cross(t);
    let a = (seed % 628) as f32 / 100.0;
    let (s, c) = a.sin_cos();
    (t * c + b * s, b * c - t * s)
}

/// UE -> render space (same as the map scene: swap Y and Z).
#[inline]
fn conv(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[2], p[1]]
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PVertex {
    pos: [f32; 3],
    color: [f32; 4],
    uv: [f32; 2],
}

/// Standalone additive/alpha particle pass (own pipeline, drawn after the world).
pub struct ParticleRenderer {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    max: usize,
}

impl ParticleRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, max: usize) -> Self {
        let max = max.max(64);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particles"),
            source: wgpu::ShaderSource::Wgsl(include_str!("particles.wgsl").into()),
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&globals_layout], push_constant_ranges: &[] });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particles"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 36,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x2],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::render::DEPTH,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor { label: Some("particle globals"), size: 112, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        let vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle vb"),
            size: (max * 4 * 36) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ib = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle ib"),
            size: (max * 6 * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline, globals, globals_bg, vb, ib, max }
    }

    /// Expands billboards for `parts` and surface quads for `decals`
    /// (UE coords) and draws them over `target`.
    pub fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        cam: &crate::camera::Camera,
        aspect: f32,
        parts: &[Particle],
        decals: &[Decal],
    ) {
        if parts.is_empty() && decals.is_empty() {
            return;
        }
        let right = cam.right();
        let up = right.cross(cam.forward()).normalize();
        let mut verts: Vec<PVertex> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let mut nquads = 0usize;
        let mut quad = |cx: glam::Vec3, ax: glam::Vec3, bx: glam::Vec3, col: [f32; 4], n: &mut usize| {
            let q = [cx - ax - bx, cx + ax - bx, cx + ax + bx, cx - ax + bx];
            let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
            for k in 0..4 {
                verts.push(PVertex { pos: q[k].to_array(), color: col, uv: uvs[k] });
            }
            let b = *n as u32 * 4;
            *n += 1;
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        };
        // Surface decals first (depth-tested against the world).
        for d in decals.iter().take(self.max) {
            if nquads >= self.max {
                break;
            }
            let n = glam::Vec3::from(conv(d.normal));
            let (tx, bx) = decal_basis(n, d.seed);
            let fade = (d.life / d.max_life).clamp(0.0, 1.0);
            let hs = d.size * 0.5;
            let col = [d.color[0], d.color[1], d.color[2], d.color[3] * fade];
            let cx = glam::Vec3::from(conv(d.pos)) + n * 0.6;
            quad(cx, tx * hs, bx * hs, col, &mut nquads);
        }
        for p in parts.iter() {
            if nquads >= self.max {
                break;
            }
            let c = conv(p.pos);
            let fade = (p.life / p.max_life).clamp(0.0, 1.0);
            let hs = p.size * 0.5 * (0.6 + 0.4 * fade);
            let col = [p.color[0], p.color[1], p.color[2], p.color[3] * fade];
            let cx = glam::Vec3::from(c);
            quad(cx, right * hs, up * hs, col, &mut nquads);
        }
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct G {
            view_proj: [[f32; 4]; 4],
            cam_pos: [f32; 4],
            fog: [f32; 4],
        }
        let g = G { view_proj: cam.view_proj(aspect).to_cols_array_2d(), cam_pos: [cam.pos.x, cam.pos.y, cam.pos.z, 1.0], fog: [6000.0, 40000.0, 0.0, 0.0] };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&g));
        queue.write_buffer(&self.vb, 0, bytemuck::cast_slice(&verts));
        queue.write_buffer(&self.ib, 0, bytemuck::cast_slice(&idx));
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("particles"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_vertex_buffer(0, self.vb.slice(..));
            pass.set_index_buffer(self.ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..idx.len() as u32, 0, 0..1);
        }
        queue.submit([enc.finish()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_and_expire() {
        let mut s = ParticleSystem::new(64);
        s.burst([0.0; 3], 10, [1.0, 0.0, 0.0, 1.0], 200.0, 1.0, 8.0, 1.0);
        assert_eq!(s.len(), 10);
        // update() caps dt at 0.1 s per call (frame stability): step it.
        for _ in 0..5 {
            s.update(0.1);
        }
        assert_eq!(s.len(), 10); // min life 0.6 s > 0.5 s elapsed
        for _ in 0..10 {
            s.update(0.1);
        }
        assert!(s.is_empty());
    }

    #[test]
    fn budget_recycles_oldest() {
        let mut s = ParticleSystem::new(64);
        for i in 0..10 {
            s.decal([i as f32, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0], 8.0, 5.0);
        }
        assert_eq!(s.decals.len(), 8); // decal budget = max(64/8, 8)
        assert_eq!(s.decals[0].pos[0], 2.0);
    }

    #[test]
    fn decal_basis_is_orthonormal() {
        for n in [glam::Vec3::Y, glam::Vec3::X, glam::Vec3::Z, glam::Vec3::new(1.0, 2.0, 3.0).normalize()] {
            for seed in [1u64, 42, 999] {
                let (t, b) = super::decal_basis(n, seed);
                assert!((t.length() - 1.0).abs() < 1e-5, "tangent unit: {t}");
                assert!((b.length() - 1.0).abs() < 1e-5, "binormal unit: {b}");
                assert!(t.dot(b).abs() < 1e-5, "orthogonal: {t} {b}");
                assert!(t.dot(n).abs() < 1e-5 && b.dot(n).abs() < 1e-5, "in plane of {n}");
            }
        }
    }

    #[test]
    fn decal_seeds_rotate_differently() {
        let n = glam::Vec3::Y;
        let (t1, _) = super::decal_basis(n, 1);
        let (t2, _) = super::decal_basis(n, 200);
        assert!(t1.dot(t2).abs() < 0.99, "seeds should rotate: {t1} {t2}");
    }

    #[test]
    fn decals_expire() {
        let mut s = ParticleSystem::new(64);
        s.decal([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], 8.0, 0.15);
        assert_eq!(s.len(), 1);
        s.update(0.1);
        s.update(0.1);
        assert!(s.is_empty());
    }
}
