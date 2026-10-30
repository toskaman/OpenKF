//! UE2 `UModel` (BSP) decoding, package v128 / Killing Floor v1065.
//!
//! Layout verified byte-for-byte on KF-WestLondon.rom (Model304):
//! props, `FBox`, `FSphere`, `Vectors: TArray<FVector>`, `Points: TArray<FVector>`,
//! `Nodes: TArray<FBspNode>`, `Surfs: TArray<FBspSurf>`, `Verts: TArray<FVert>`,
//! `i32 NumSharedSides`, `i32 NumZones`, ...
//!
//! FBspNode: `FPlane`, `u64 ZoneMask`, `u8 NodeFlags`, compact `iVertPool, iSurf,
//! iBack, iFront, iPlane, iCollisionBound, iRenderBound`, `FSphere ExclusiveSphereBound`,
//! `u8 iZone[2]`, `u8 NumVertices`, `i32 iLeaf[2]`, `i32 iSection, iFirstVertex, iLightMap`.
//!
//! FBspSurf: compact `Material`, `u32 PolyFlags`, compact `pBase, vNormal, vTextureU,
//! vTextureV, iBrushPoly, Actor`, `FPlane`, `f32 LightMapScale`.
//!
//! FVert: compact `pVertex, iSide`.

use crate::compact_index::read_compact_index;
use crate::error::PackageError;
use crate::properties::read_properties;
use crate::reader::UnrealPackage;

/// Poly flag: surface is not rendered.
pub const PF_INVISIBLE: u32 = 0x0000_0001;
pub const PF_MASKED: u32 = 0x0000_0002;
pub const PF_TRANSLUCENT: u32 = 0x0000_0004;
pub const PF_TWO_SIDED: u32 = 0x0000_0100;
pub const PF_MODULATED: u32 = 0x0000_0040;
/// Sky portal: the surface shows the SkyZone instead of its texture.
pub const PF_FAKE_BACKDROP: u32 = 0x0000_0080;
pub const PF_UNLIT: u32 = 0x0040_0000;
pub const PF_PORTAL: u32 = 0x0400_0000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BspNode {
    pub plane: [f32; 4],
    pub zone_mask: u64,
    pub node_flags: u8,
    pub i_vert_pool: i32,
    pub i_surf: i32,
    pub i_back: i32,
    pub i_front: i32,
    pub i_plane: i32,
    pub i_collision_bound: i32,
    pub i_render_bound: i32,
    pub exclusive_sphere: [f32; 4],
    pub i_zone: [u8; 2],
    pub num_vertices: u8,
    pub i_leaf: [i32; 2],
    pub i_section: i32,
    pub i_first_vertex: i32,
    pub i_light_map: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BspSurf {
    /// Package object index of the material (0 none, <0 import, >0 export).
    pub material: i32,
    pub poly_flags: u32,
    pub p_base: i32,
    pub v_normal: i32,
    pub v_texture_u: i32,
    pub v_texture_v: i32,
    pub i_brush_poly: i32,
    pub actor: i32,
    pub plane: [f32; 4],
    pub light_map_scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BspVert {
    pub p_vertex: i32,
    pub i_side: i32,
}

/// Render vertex of a BSP section (40 bytes on disk).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BspSectionVertex {
    pub position: [f32; 3],
    /// Diffuse texture coordinates as stored by the engine.
    pub uv: [f32; 2],
    /// Lightmap atlas coordinates (0..1).
    pub lightmap_uv: [f32; 2],
    pub normal: [f32; 3],
}

/// Pre-built render section: nodes reference it via `iSection` / `iFirstVertex`.
#[derive(Debug, Clone)]
pub struct BspSection {
    pub vertices: Vec<BspSectionVertex>,
    pub material: i32,
    pub num_nodes: i32,
    pub poly_flags: u32,
    /// Index into `Model::lightmaps` (-1 = none).
    pub light_map_texture: i32,
}

/// Baked BSP lightmap atlas: square, level 0 first.
#[derive(Debug, Clone)]
pub struct LightMapTexture {
    pub width: u32,
    /// `true` = DXT5 (1 byte/pixel), `false` = DXT1 (0.5 byte/pixel).
    pub dxt5: bool,
    pub mips: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    pub vectors: Vec<[f32; 3]>,
    pub points: Vec<[f32; 3]>,
    pub nodes: Vec<BspNode>,
    pub surfs: Vec<BspSurf>,
    pub verts: Vec<BspVert>,
    pub num_shared_sides: i32,
    pub num_zones: i32,
    /// Render sections (empty if the tail of the model could not be parsed).
    pub sections: Vec<BspSection>,
    pub lightmaps: Vec<LightMapTexture>,
}

/// One renderable BSP polygon (convex, world space).
#[derive(Debug, Clone)]
pub struct BspPolygon {
    pub surf: usize,
    pub positions: Vec<[f32; 3]>,
    pub normal: [f32; 3],
}

struct Cur<'a> {
    d: &'a [u8],
    o: usize,
}

impl Cur<'_> {
    fn need(&self, n: usize) -> Result<(), PackageError> {
        if self.o + n > self.d.len() {
            Err(PackageError::UnexpectedEof { offset: self.o, requested: n, total: self.d.len() })
        } else {
            Ok(())
        }
    }
    fn u8(&mut self) -> Result<u8, PackageError> {
        self.need(1)?;
        self.o += 1;
        Ok(self.d[self.o - 1])
    }
    fn u32(&mut self) -> Result<u32, PackageError> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.d[self.o..self.o + 4].try_into().unwrap());
        self.o += 4;
        Ok(v)
    }
    fn i32(&mut self) -> Result<i32, PackageError> {
        Ok(self.u32()? as i32)
    }
    fn u64(&mut self) -> Result<u64, PackageError> {
        Ok(self.u32()? as u64 | (self.u32()? as u64) << 32)
    }
    fn f32(&mut self) -> Result<f32, PackageError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn f4(&mut self) -> Result<[f32; 4], PackageError> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }
    fn ci(&mut self) -> Result<i32, PackageError> {
        read_compact_index(self.d, &mut self.o)
    }
    fn count(&mut self, min_elem: usize) -> Result<usize, PackageError> {
        let n = self.ci()?;
        if n < 0 {
            return Err(PackageError::UnexpectedEof { offset: self.o, requested: 0, total: self.d.len() });
        }
        // FArray 0*2 GPF hardening: treat 0 stride as 1 for bounds, cap huge n
        let check_elem = if min_elem == 0 { 1 } else { min_elem };
        if min_elem == 0 && n as usize > 8192 {
            return Err(PackageError::UnexpectedEof { offset: self.o, requested: n as usize, total: 8192 });
        }
        let need = (n as usize).checked_mul(check_elem).ok_or(PackageError::UnexpectedEof { offset: self.o, requested: usize::MAX, total: self.d.len() })?;
        self.need(need)?;
        Ok(n as usize)
    }
    fn count_capped(&mut self, min_elem: usize, max: usize) -> Result<usize, PackageError> {
        let n = self.count(min_elem)?;
        if n > max {
            return Err(PackageError::UnexpectedEof { offset: self.o, requested: n, total: max });
        }
        Ok(n)
    }
    fn vec3s(&mut self) -> Result<Vec<[f32; 3]>, PackageError> {
        let n = self.count_capped(12, 200_000)?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push([self.f32()?, self.f32()?, self.f32()?]);
        }
        Ok(v)
    }
}

/// Reads a `UModel` export (0-based export index).
pub fn read_model(pkg: &UnrealPackage<'_>, export_index: usize) -> Result<Model, PackageError> {
    let d = pkg.get_export_data(export_index)?;
    let props = read_properties(pkg, d, 0)?;
    let mut c = Cur { d, o: props.end_offset };
    c.need(25 + 16)?;
    c.o += 25 + 16; // FBox + FSphere (UPrimitive)
    let vectors = c.vec3s()?;
    let points = c.vec3s()?;

    let nn = c.count_capped(16 + 8 + 1 + 7 + 16 + 3 + 8 + 12, 200_000)?;
    let mut nodes = Vec::with_capacity(nn);
    for _ in 0..nn {
        nodes.push(BspNode {
            plane: c.f4()?,
            zone_mask: c.u64()?,
            node_flags: c.u8()?,
            i_vert_pool: c.ci()?,
            i_surf: c.ci()?,
            i_back: c.ci()?,
            i_front: c.ci()?,
            i_plane: c.ci()?,
            i_collision_bound: c.ci()?,
            i_render_bound: c.ci()?,
            exclusive_sphere: c.f4()?,
            i_zone: [c.u8()?, c.u8()?],
            num_vertices: c.u8()?,
            i_leaf: [c.i32()?, c.i32()?],
            i_section: c.i32()?,
            i_first_vertex: c.i32()?,
            i_light_map: c.i32()?,
        });
    }

    let ns = c.count_capped(1 + 4 + 6 + 16 + 4, 200_000)?;
    let mut surfs = Vec::with_capacity(ns);
    for _ in 0..ns {
        surfs.push(BspSurf {
            material: c.ci()?,
            poly_flags: c.u32()?,
            p_base: c.ci()?,
            v_normal: c.ci()?,
            v_texture_u: c.ci()?,
            v_texture_v: c.ci()?,
            i_brush_poly: c.ci()?,
            actor: c.ci()?,
            plane: c.f4()?,
            light_map_scale: c.f32()?,
        });
    }

    let nv = c.count_capped(2, 200_000)?;
    let mut verts = Vec::with_capacity(nv);
    for _ in 0..nv {
        verts.push(BspVert { p_vertex: c.ci()?, i_side: c.ci()? });
    }
    let num_shared_sides = c.i32()?;
    let num_zones = c.i32()?;
    let base = pkg.exports.get(export_index).map(|e| e.serial_offset as usize).unwrap_or(0);
    let (sections, lightmaps) = read_model_tail(&mut c, num_zones, base).unwrap_or_default();

    Ok(Model {
        name: pkg.format_full_name(export_index as i32 + 1),
        vectors,
        points,
        nodes,
        surfs,
        verts,
        num_shared_sides,
        num_zones,
        sections,
        lightmaps,
    })
}

/// Parses zones, polys, bounds, leaves, lights and render sections, then locates
/// the lightmap atlases (lazy arrays: `i32 AbsoluteEnd, compact Len, bytes`).
fn read_model_tail(c: &mut Cur<'_>, num_zones: i32, base: usize) -> Result<(Vec<BspSection>, Vec<LightMapTexture>), PackageError> {
    if num_zones > 1024 {
        return Err(PackageError::UnexpectedEof { offset: c.o, requested: num_zones as usize, total: 1024 });
    }
    for _ in 0..num_zones.max(0) {
        c.ci()?; // ZoneActor
        c.need(20)?;
        c.o += 20; // Connectivity, Visibility, LastRenderTime
    }
    c.ci()?; // Polys
    let nb = c.count(25)?;
    c.o += nb * 25; // Bounds
    let nh = c.count(4)?;
    c.o += nh * 4; // LeafHulls
    let nl = c.count(11)?;
    for _ in 0..nl {
        c.ci()?;
        c.ci()?;
        c.ci()?;
        c.need(8)?;
        c.o += 8;
    }
    let nlt = c.count(1)?;
    for _ in 0..nlt {
        c.ci()?;
    }
    c.i32()?; // RootOutside
    c.i32()?; // Linked
    let nsec = c.count_capped(1, 16_384)?;
    let mut sections = Vec::with_capacity(nsec);
    for _ in 0..nsec {
        let nv = c.count_capped(40, 65_536)?;
        let mut vertices = Vec::with_capacity(nv);
        for _ in 0..nv {
            vertices.push(BspSectionVertex {
                position: [c.f32()?, c.f32()?, c.f32()?],
                uv: [c.f32()?, c.f32()?],
                lightmap_uv: [c.f32()?, c.f32()?],
                normal: [c.f32()?, c.f32()?, c.f32()?],
            });
        }
        c.i32()?; // Revision
        let material = c.ci()?;
        let num_nodes = c.i32()?;
        let poly_flags = c.u32()?;
        let light_map_texture = c.i32()?;
        sections.push(BspSection { vertices, material, num_nodes, poly_flags, light_map_texture });
    }

    // Lightmap atlases: scan the remaining bytes for lazy arrays. Each atlas
    // stores two levels (WxW then W/2xW/2) as DXT5.
    let mut blobs: Vec<&[u8]> = Vec::new();
    let d = c.d;
    let mut p = c.o;
    while p + 8 < d.len() {
        let skip = i32::from_le_bytes(d[p..p + 4].try_into().unwrap());
        if skip > 0 && (skip as usize) > p && (skip as usize) <= base + d.len() {
            let mut q = p + 4;
            if let Ok(n) = read_compact_index(d, &mut q) {
                // End position is absolute in the file (normal) or relative to the
                // object start (seen in some re-saved maps).
                let end = q + n.max(0) as usize;
                if n >= 16 && end <= d.len() && (base + end == skip as usize || end == skip as usize) {
                    blobs.push(&d[q..q + n as usize]);
                    p = q + n as usize;
                    continue;
                }
            }
        }
        p += 1;
    }
    // Fallback for re-saved maps whose lazy-array end positions are stale: find
    // `i32, compact N, N bytes, i32, compact N/4, N/4 bytes` with N an atlas size.
    if blobs.is_empty() {
        let sizes = [32768usize, 65536, 131072, 262144, 524288, 1048576];
        let mut p = c.o;
        while p + 8 < d.len() {
            let mut q = p + 4;
            let found = match read_compact_index(d, &mut q) {
                Ok(n) if n > 0 && sizes.contains(&(n as usize)) && q + n as usize + 8 < d.len() => {
                    let n = n as usize;
                    let mut r = q + n + 4;
                    match read_compact_index(d, &mut r) {
                        Ok(m) if m as usize * 4 == n && r + n / 4 <= d.len() => {
                            blobs.push(&d[q..q + n]);
                            blobs.push(&d[r..r + n / 4]);
                            Some(r + n / 4)
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            p = found.unwrap_or(p + 1);
        }
    }

    // Each atlas stores two levels (W then W/2), as DXT5 (W*W bytes) or DXT1 (W*W/2).
    let square = |n: usize| {
        let w = (n as f64).sqrt().round() as usize;
        (w * w == n && w.is_multiple_of(4)).then_some(w)
    };
    let mut lightmaps = Vec::new();
    let mut i = 0;
    while i + 1 < blobs.len() {
        let (a, b) = (blobs[i].len(), blobs[i + 1].len());
        if b * 4 == a {
            let fmt = square(a).map(|w| (w, true)).or_else(|| square(a * 2).map(|w| (w, false)));
            if let Some((w, dxt5)) = fmt {
                lightmaps.push(LightMapTexture { width: w as u32, dxt5, mips: vec![blobs[i].to_vec(), blobs[i + 1].to_vec()] });
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    if lightmaps.is_empty() && sections.iter().any(|s| s.light_map_texture >= 0) && std::env::var_os("OPENKF_DEBUG").is_some() {
        eprintln!("[lightmaps] none matched; lazy blobs: {:?}; tail {} bytes after sections; first: {:02x?}", blobs.iter().map(|b| b.len()).collect::<Vec<_>>(), c.d.len() - c.o, &c.d[c.o..(c.o + 64).min(c.d.len())]);
    }
    Ok((sections, lightmaps))
}

impl Model {
    /// All visible polygons (skips `PF_INVISIBLE` surfaces and degenerate nodes).
    pub fn polygons(&self) -> Vec<BspPolygon> {
        let mut out = Vec::new();
        for n in &self.nodes {
            if n.num_vertices < 3 || n.i_surf < 0 {
                continue;
            }
            let Some(s) = self.surfs.get(n.i_surf as usize) else { continue };
            if s.poly_flags & PF_INVISIBLE != 0 {
                continue;
            }
            let start = n.i_vert_pool.max(0) as usize;
            let end = start + n.num_vertices as usize;
            let Some(vs) = self.verts.get(start..end) else { continue };
            let positions: Option<Vec<[f32; 3]>> =
                vs.iter().map(|v| self.points.get(v.p_vertex as usize).copied()).collect();
            let Some(positions) = positions else { continue };
            out.push(BspPolygon {
                surf: n.i_surf as usize,
                positions,
                normal: [n.plane[0], n.plane[1], n.plane[2]],
            });
        }
        out
    }

    /// Texture UV (in texels) of a world point on a surface: (P - Base)·U, (P - Base)·V.
    /// Divide by the texture's USize/VSize for normalized coordinates.
    pub fn texel_uv(&self, surf: &BspSurf, p: [f32; 3]) -> [f32; 2] {
        let b = self.points.get(surf.p_base as usize).copied().unwrap_or([0.0; 3]);
        let u = self.vectors.get(surf.v_texture_u as usize).copied().unwrap_or([0.0; 3]);
        let v = self.vectors.get(surf.v_texture_v as usize).copied().unwrap_or([0.0; 3]);
        let d = [p[0] - b[0], p[1] - b[1], p[2] - b[2]];
        [
            d[0] * u[0] + d[1] * u[1] + d[2] * u[2],
            d[0] * v[0] + d[1] * v[1] + d[2] * v[2],
        ]
    }
}
