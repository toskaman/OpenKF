//! Builds a render scene from a .rom map: world BSP + every placed static mesh,
//! baked to world space in parallel and batched per texture.
//!
//! Coordinates are converted from UE2 (X fwd, Y right, Z up, left-handed) to
//! a right-handed Y-up space by swapping Y and Z: (x, y, z) -> (x, z, y).

use crate::assets::{decode_texture, texture_size, AssetLibrary, Blend, Pkg, TexRef, TextureData};
use bytemuck::{Pod, Zeroable};
use openkf_package::*;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub uv: [f32; 2],
    /// Baked light color (RGB, 2x modulate). Alpha: 255 = vertex color,
    /// 64 = sample the batch lightmap at `lm_uv`, 0 = fallback shading.
    pub light: [u8; 4],
    pub lm_uv: [f32; 2],
}

pub struct Batch {
    /// Index into `Scene::textures` (`None` = untextured).
    pub texture: Option<usize>,
    pub blend: Blend,
    /// Index into `Scene::lightmaps` (-1 = none).
    pub lightmap: i32,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

pub struct TerrainLayerDraw {
    pub texture: Option<usize>,
    pub alpha: Option<usize>,
    /// World units per texture repeat (converted x, z).
    pub repeat: [f32; 2],
}

pub struct TerrainMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub layers: Vec<TerrainLayerDraw>,
}

pub struct Scene {
    pub actors: Vec<crate::zeds::SkelActor>,
    /// Movable static meshes (pickups, projectiles) — live mode only.
    pub props: Vec<crate::props::PropActor>,
    pub terrains: Vec<TerrainMesh>,
    pub lightmaps: Vec<LightMapTexture>,
    pub name: String,
    pub batches: Vec<Batch>,
    pub textures: Vec<TextureData>,
    pub spawn_pos: [f32; 3],
    /// SkyZoneInfo location (converted space), if the map has a sky zone.
    pub sky_pos: Option<[f32; 3]>,
    /// Yaw in radians (converted space, around +Y).
    pub spawn_yaw: f32,
    pub stats: String,
}

#[inline]
fn conv(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[2], p[1]]
}

#[derive(Default)]
struct TexTable {
    index: HashMap<TexRef, usize>,
    refs: Vec<TexRef>,
    sizes: Vec<(u32, u32)>,
    mats: HashMap<(usize, i32), Option<(usize, Blend)>>,
}

impl TexTable {
    /// Texture slot for a material reference of `pkg` (deduplicated).
    fn get(&mut self, lib: &mut AssetLibrary, pkg: &'static Pkg, mat: i32) -> Option<(usize, Blend)> {
        let key = (pkg as *const Pkg as usize, mat);
        if let Some(v) = self.mats.get(&key) {
            return *v;
        }
        let r = lib.material_texture(pkg, mat).map(|m| {
            let t = m.tex;
            let i = match self.index.get(&t) {
                Some(&i) => i,
                None => {
                    self.refs.push(t);
                    self.sizes.push(texture_size(t));
                    self.index.insert(t, self.refs.len() - 1);
                    self.refs.len() - 1
                }
            };
            (i, m.blend)
        });
        self.mats.insert(key, r);
        r
    }
}

struct Partial {
    by_tex: HashMap<(Option<usize>, Blend, i32), (Vec<Vertex>, Vec<u32>)>,
}

impl Partial {
    fn new() -> Self {
        Self { by_tex: HashMap::new() }
    }
    fn merge(mut self, other: Partial) -> Partial {
        for (k, (v, i)) in other.by_tex {
            let e = self.by_tex.entry(k).or_default();
            let base = e.0.len() as u32;
            e.0.extend(v);
            e.1.extend(i.into_iter().map(|x| x + base));
        }
        self
    }
}

pub fn load_scene(kf_dir: &Path, rom: &Path, bc_supported: bool, zeds: bool, pool: Option<&[(String, String, usize)]>) -> Result<Scene, String> {
    let t0 = std::time::Instant::now();
    let mut lib = AssetLibrary::new(kf_dir);
    let map_name = rom.file_stem().unwrap().to_string_lossy().to_string();
    let map: &'static Pkg = lib.load_file(&map_name, rom).ok_or("cannot open map")?;

    // ---------------------------------------------------------- textures
    let mut tt = TexTable::default();

    // ---------------------------------------------------------- BSP
    let model = read_level_model(map).map_err(|e| format!("BSP: {e:?}"))?;
    let polys = model.polygons();
    let surf_tex: Vec<Option<(usize, Blend)>> = model.surfs.iter().map(|s| tt.get(&mut lib, map, s.material)).collect();
    let mut bsp = Partial::new();
    let use_sections = !model.sections.is_empty();
    for n in &model.nodes {
        if n.num_vertices < 3 || n.i_surf < 0 {
            continue;
        }
        let Some(s) = model.surfs.get(n.i_surf as usize) else { continue };
        if s.poly_flags & (PF_INVISIBLE | PF_FAKE_BACKDROP) != 0 {
            // Sky portals stay empty so the sky pass shows through.
            continue;
        }
        let mt = surf_tex[n.i_surf as usize];
        let tex = mt.map(|m| m.0);
        let mut blend = mt.map(|m| m.1).unwrap_or(Blend::Opaque);
        if blend == Blend::Opaque {
            if s.poly_flags & PF_TRANSLUCENT != 0 {
                blend = Blend::Additive;
            } else if s.poly_flags & PF_MODULATED != 0 {
                blend = Blend::Modulate;
            } else if s.poly_flags & PF_MASKED != 0 {
                blend = Blend::Masked;
            }
        }
        if blend == Blend::Invisible {
            continue;
        }
        let (tw, th) = tex.map(|t| tt.sizes[t]).unwrap_or((256, 256));
        // Positions + lightmap UVs from the render section when available.
        let mut lm_index = -1;
        let verts: Vec<([f32; 3], [f32; 2])> = match model.sections.get(n.i_section.max(0) as usize).filter(|_| use_sections && n.i_section >= 0) {
            Some(sec) if (n.i_first_vertex.max(0) as usize + n.num_vertices as usize) <= sec.vertices.len() => {
                if (sec.light_map_texture as usize) < model.lightmaps.len() && sec.light_map_texture >= 0 {
                    lm_index = sec.light_map_texture;
                }
                sec.vertices[n.i_first_vertex as usize..n.i_first_vertex as usize + n.num_vertices as usize]
                    .iter()
                    .map(|v| (v.position, v.lightmap_uv))
                    .collect()
            }
            _ => {
                let st = n.i_vert_pool.max(0) as usize;
                match model.verts.get(st..st + n.num_vertices as usize) {
                    Some(vs) => vs.iter().filter_map(|v| model.points.get(v.p_vertex as usize).map(|&p| (p, [0.0; 2]))).collect(),
                    None => continue,
                }
            }
        };
        if verts.len() < 3 {
            continue;
        }
        let light = if s.poly_flags & PF_UNLIT != 0 {
            [128, 128, 128, 255]
        } else if lm_index >= 0 {
            [255, 255, 255, 64]
        } else {
            [0; 4]
        };
        let normal = conv([n.plane[0], n.plane[1], n.plane[2]]);
        let e = bsp.by_tex.entry((tex, blend, if light[3] == 64 { lm_index } else { -1 })).or_default();
        let base = e.0.len() as u32;
        for &(p, lm) in &verts {
            let uv = model.texel_uv(s, p);
            e.0.push(Vertex { pos: conv(p), nrm: normal, uv: [uv[0] / tw as f32, uv[1] / th as f32], light, lm_uv: lm });
        }
        for i in 1..verts.len() as u32 - 1 {
            e.1.extend_from_slice(&[base, base + i, base + i + 1]);
        }
    }

    // ---------------------------------------------------------- static meshes
    let placements: Vec<MeshPlacement> = static_mesh_placements(map).into_iter().filter(|p| !p.hidden).collect();
    // Resolve each unique mesh once: (package, export) -> mesh + per-section texture.
    struct MeshEntry {
        mesh: StaticMesh<'static>,
        section_tex: Vec<Option<(usize, Blend)>>,
    }
    let mut mesh_index: HashMap<i32, Option<usize>> = HashMap::new();
    let mut meshes: Vec<MeshEntry> = Vec::new();
    // (mesh index, placement, per-section texture override from actor Skins)
    let mut work: Vec<(usize, &MeshPlacement, Option<Vec<Option<(usize, Blend)>>>)> = Vec::new();
    let mut unresolved = 0usize;
    for pl in &placements {
        let mi = *mesh_index.entry(pl.mesh_ref).or_insert_with(|| {
            let (p, e) = lib.resolve(map, pl.mesh_ref)?;
            let mesh = read_static_mesh(p, e).ok()?;
            let section_tex = (0..mesh.sections.len())
                .map(|si| mesh.materials.get(si).and_then(|m| tt.get(&mut lib, p, m.material)))
                .collect();
            meshes.push(MeshEntry { mesh, section_tex });
            Some(meshes.len() - 1)
        });
        match mi {
            Some(i) => {
                let overrides = (!pl.skins.is_empty()).then(|| {
                    (0..meshes[i].section_tex.len())
                        .map(|si| match pl.skins.get(si) {
                            Some(&skin) if skin != 0 => tt.get(&mut lib, map, skin).or(meshes[i].section_tex[si]),
                            _ => meshes[i].section_tex[si],
                        })
                        .collect()
                });
                work.push((i, pl, overrides))
            }
            None => unresolved += 1,
        }
    }
    // ---------------------------------------------------------- terrain
    let mut terrains = Vec::new();
    for t in read_terrains(map) {
        let (w, h) = (t.width, t.height);
        let mut vertices = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                vertices.push(Vertex {
                    pos: conv(t.world_pos(x, y)),
                    nrm: conv(t.normal(x, y)),
                    // Alpha-map coordinates (0..1 over the heightmap).
                    uv: [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32],
                    light: [0; 4],
                    lm_uv: [0.0; 2],
                });
            }
        }
        let mut indices = Vec::with_capacity((w - 1) * (h - 1) * 6);
        for y in 0..h - 1 {
            for x in 0..w - 1 {
                let i = (y * w + x) as u32;
                let (r, d) = (i + 1, i + w as u32);
                indices.extend_from_slice(&[i, d, r, r, d, d + 1]);
            }
        }
        let layers = t
            .layers
            .iter()
            .enumerate()
            // Overlay layers need an alpha map and a real scale; the base layer always draws.
            .filter(|(li, l)| *li == 0 || (l.alpha_map != 0 && l.u_scale > 0.0 && l.v_scale > 0.0))
            .map(|(_, l)| {
                let tex = tt.get(&mut lib, map, l.texture).map(|m| m.0);
                let alpha = if l.alpha_map != 0 { tt.get(&mut lib, map, l.alpha_map).map(|m| m.0) } else { None };
                let (tw, th) = tex.map(|i| tt.sizes[i]).unwrap_or((256, 256));
                // UE2: one texture repeat every UScale * USize world units.
                TerrainLayerDraw {
                    texture: tex,
                    alpha,
                    repeat: [l.u_scale.max(0.01) * tw as f32, l.v_scale.max(0.01) * th as f32],
                }
            })
            .collect();
        terrains.push(TerrainMesh { vertices, indices, layers });
    }

    // ---------------------------------------------------------- specimens (demo line-up)
    let mut actors = Vec::new();
    if let Some(pool) = pool {
        actors = crate::zeds::spawn_pool(&mut lib, pool, |lib, pkg, mat| tt.get(lib, pkg, mat));
    } else if zeds {
        let mut ps = ([0.0f32; 3], 0i32);
        for i in 0..map.exports.len() {
            if export_class_name(map, i).ends_with("PlayerStart") {
                if let Ok(p) = read_object_properties(map, i) {
                    if let Some(PropValue::Vector(v)) = p.get("Location") { ps.0 = *v; }
                    if let Some(PropValue::Rotator(r)) = p.get("Rotation") { ps.1 = r[1]; }
                    break;
                }
            }
        }
        actors = crate::zeds::spawn_line(&mut lib, "KF_Freaks_Trip", ps.0, ps.1, |lib, pkg, mat| tt.get(lib, pkg, mat));
    }

    let mut props = Vec::new();
    if pool.is_some() && !crate::live_view::prop_pool().is_empty() {
        props = crate::props::spawn_pool(&mut lib, crate::live_view::prop_pool(), |lib, pkg, mat| tt.get(lib, pkg, mat));
    }

    let t_resolve = t0.elapsed();
    if std::env::var_os("OPENKF_DEBUG").is_some() {
        let bsp_none = polys.iter().filter(|p| surf_tex[p.surf].is_none()).count();
        let mut blends: HashMap<String, usize> = HashMap::new();
        for t in surf_tex.iter().flatten() { *blends.entry(format!("{:?}", t.1)).or_default() += 1; }
        eprintln!("[debug] BSP surf blends: {blends:?}");
        eprintln!("[debug] BSP polys without texture: {bsp_none}/{}", polys.len());
        let mut none_flags: HashMap<u32, usize> = HashMap::new();
        for p in &polys { if surf_tex[p.surf].is_none() { *none_flags.entry(model.surfs[p.surf].poly_flags).or_default() += 1; } }
        eprintln!("[debug] their poly flags: {none_flags:x?}");
        let mut flags: HashMap<u32, (usize, String)> = HashMap::new();
        for p in &polys {
            let su = &model.surfs[p.surf];
            let e = flags.entry(su.poly_flags).or_insert((0, map.format_full_name(su.material)));
            e.0 += 1;
        }
        let mut fl: Vec<_> = flags.into_iter().collect();
        fl.sort_by_key(|a| std::cmp::Reverse(a.1 .0));
        for (f, (n, m)) in fl.iter().take(25) { eprintln!("[debug] flags {f:08x}: {n} polys e.g. {m}"); }
        for me in &meshes {
            for (si, t) in me.section_tex.iter().enumerate() {
                if let Some((ti, b)) = t {
                    let r = tt.refs[*ti];
                    let tn = r.pkg().format_full_name(r.export as i32 + 1);
                    let mat = me.mesh.materials.get(si).map(|m| m.material_name.clone()).unwrap_or_default();
                    if tn.to_ascii_lowercase().contains("fire") || tn.to_ascii_lowercase().contains("glow") || tn.to_ascii_lowercase().contains("smoke") || tn.to_ascii_lowercase().contains("fx") {
                        eprintln!("[debug] mesh {} sec {si}: {mat} -> {tn} {b:?}", me.mesh.name);
                    }
                }
            }
        }
        for me in &meshes {
            let n = me.section_tex.iter().filter(|t| t.is_none()).count();
            if n > 0 { eprintln!("[debug] mesh {} : {n}/{} sections untextured, {} materials", me.mesh.name, me.section_tex.len(), me.mesh.materials.len()); }
        }
    }

    // Bake placements in parallel.
    let baked = work
        .par_iter()
        .fold(Partial::new, |mut acc, (mi, pl, overrides)| {
            let (mi, pl) = (*mi, *pl);
            let me = &meshes[mi];
            let m = &me.mesh;
            let uv0 = m.uv_channels.first();
            let wp: Vec<[f32; 3]> = m.positions.iter().map(|&p| conv(pl.transform_point(p))).collect();
            let wn: Vec<[f32; 3]> = m.normals.iter().map(|&n| conv(pl.transform_normal(n))).collect();
            for (si, s) in m.sections.iter().enumerate() {
                let mt = overrides.as_ref().map_or(me.section_tex[si], |o| o[si]);
                let key = (mt.map(|m| m.0), mt.map(|m| m.1).unwrap_or(Blend::Opaque), -1);
                if key.1 == Blend::Invisible {
                    continue;
                }
                let e = acc.by_tex.entry(key).or_default();
                let mut remap: HashMap<u16, u32> = HashMap::new();
                for t in m.section_triangles(s) {
                    for &i in &t {
                        let idx = *remap.entry(i).or_insert_with(|| {
                            let k = i as usize;
                            e.0.push(Vertex {
                                pos: wp[k],
                                nrm: wn.get(k).copied().unwrap_or([0.0, 1.0, 0.0]),
                                uv: uv0.and_then(|u| u.get(k)).copied().unwrap_or([0.0; 2]),
                                light: match pl.vertex_colors.get(k) {
                                    _ if pl.unlit => [128, 128, 128, 255], // 2x128 = full bright
                                    Some(c) if pl.vertex_colors.len() == m.positions.len() => [c[0], c[1], c[2], 255],
                                    _ => [0; 4],
                                },
                                lm_uv: [0.0; 2],
                            });
                            e.0.len() as u32 - 1
                        });
                        e.1.push(idx);
                    }
                }
            }
            acc
        })
        .reduce(Partial::new, Partial::merge);
    let t_bake = t0.elapsed();

    // Decode textures in parallel.
    let textures: Vec<TextureData> = tt.refs
        .par_iter()
        .map(|&t| {
            decode_texture(t, bc_supported).unwrap_or(TextureData {
                width: 1,
                height: 1,
                format: crate::assets::TexFormat::Rgba8,
                mips: vec![vec![200, 0, 200, 255]],
                alpha_test: false,
                luminance: false,
            })
        })
        .collect();
    let t_tex = t0.elapsed();

    let merged = bsp.merge(baked);
    let mut batches: Vec<Batch> = merged
        .by_tex
        .into_iter()
        .filter(|(_, (_, i))| !i.is_empty())
        .map(|((texture, blend, lightmap), (vertices, indices))| Batch { texture, blend, lightmap, vertices, indices })
        .collect();
    // Opaque, then masked, then translucent kinds (drawn without depth writes).
    batches.sort_by_key(|b| b.blend);

    // ---------------------------------------------------------- spawn
    let (mut spawn_pos, mut spawn_yaw) = ([0.0f32; 3], 0.0f32);
    for i in 0..map.exports.len() {
        if export_class_name(map, i).ends_with("PlayerStart") {
            if let Ok(p) = read_object_properties(map, i) {
                if let Some(PropValue::Vector(v)) = p.get("Location") {
                    spawn_pos = conv([v[0], v[1], v[2] + 50.0]);
                }
                if let Some(PropValue::Rotator(r)) = p.get("Rotation") {
                    spawn_yaw = r[1] as f32 * std::f32::consts::TAU / 65536.0;
                }
                break;
            }
        }
    }

    let mut sky_pos = None;
    for i in 0..map.exports.len() {
        if export_class_name(map, i).contains("SkyZoneInfo") {
            if let Ok(p) = read_object_properties(map, i) {
                if let Some(PropValue::Vector(v)) = p.get("Location") {
                    sky_pos = Some(conv(*v));
                    break;
                }
            }
        }
    }

    let tris: usize = batches.iter().map(|b| b.indices.len() / 3).sum::<usize>()
        + terrains.iter().map(|t| t.indices.len() / 3 * t.layers.len().max(1)).sum::<usize>();
    eprintln!("[spawn] UE location {:?}, yaw {:.0} deg", [spawn_pos[0], spawn_pos[2], spawn_pos[1]], spawn_yaw.to_degrees());
    let stats = format!(
        "{map_name}: {} animated actors, {} props, {} lightmaps, {} terrain(s), {} BSP polys, {} placements ({} unique meshes, {unresolved} unresolved), {} textures, {} batches, {tris} triangles | resolve {:.0} ms, bake {:.0} ms, textures {:.0} ms",
        actors.len(),
        props.len(),
        model.lightmaps.len(),
        terrains.len(),
        polys.len(),
        work.len(),
        meshes.len(),
        textures.len(),
        batches.len(),
        t_resolve.as_secs_f64() * 1e3,
        (t_bake - t_resolve).as_secs_f64() * 1e3,
        (t_tex - t_bake).as_secs_f64() * 1e3,
    );
    Ok(Scene { actors, props, lightmaps: model.lightmaps.clone(), terrains, name: map_name, batches, textures, spawn_pos, sky_pos, spawn_yaw, stats })
}
