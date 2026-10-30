//! UE2 `USkeletalMesh` decoding (Killing Floor v1065 `.ukx`).
//!
//! Header parsed field by field (verified on KF_Freaks_Trip.CLOT_Freak):
//! props, FBox, FSphere, `i32 Version`, `i32 VertexCount`, `TArray<FMeshVert> Verts`,
//! `TArray<UMaterial*> Textures`, MeshScale, MeshOrigin, RotOrigin, `TArray<u16> FaceLevel`,
//! `TArray<FMeshFace> Faces`, `TArray<u16> CollapseWedgeThus`, `TArray<FMeshWedge> Wedges`,
//! `TArray<FMeshMaterial> Materials`, 6 LOD floats/ints, impostor block (61 bytes, v>=3),
//! `TArray<FVector> Points2`, `TArray<FMeshBone> RefSkeleton`, `UMeshAnimation* Animation`.
//!
//! The full-resolution geometry is stored at the end as lazy arrays
//! (`i32 EndPos, compact N, data`): Points (12), Wedges (10), Triangles (12),
//! VertInfluences (8), CollapseWedge (2), f1C8 (2).

use crate::compact_index::read_compact_index;
use crate::error::PackageError;
use crate::properties::read_properties;
use crate::reader::UnrealPackage;

#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    pub flags: u32,
    /// Rotation relative to parent (x, y, z, w).
    pub orientation: [f32; 4],
    /// Position relative to parent.
    pub position: [f32; 3],
    pub num_children: i32,
    pub parent: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct Wedge {
    pub point: u16,
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, Copy)]
pub struct Triangle {
    pub wedges: [u16; 3],
    pub material: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct Influence {
    pub weight: f32,
    pub point: u16,
    pub bone: u16,
}

#[derive(Debug, Clone)]
pub struct SkeletalMesh {
    pub name: String,
    pub version: i32,
    /// Material object indices (ULodMesh `Textures`).
    pub textures: Vec<i32>,
    /// FMeshMaterial: (PolyFlags, TextureIndex into `textures`).
    pub materials: Vec<(u32, i32)>,
    pub mesh_scale: [f32; 3],
    pub mesh_origin: [f32; 3],
    /// Pitch, Yaw, Roll.
    pub rot_origin: [i32; 3],
    pub bones: Vec<Bone>,
    /// Default animation object index.
    pub animation: i32,
    pub points: Vec<[f32; 3]>,
    pub wedges: Vec<Wedge>,
    pub triangles: Vec<Triangle>,
    pub influences: Vec<Influence>,
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
    fn u32(&mut self) -> Result<u32, PackageError> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.d[self.o..self.o + 4].try_into().unwrap());
        self.o += 4;
        Ok(v)
    }
    fn i32(&mut self) -> Result<i32, PackageError> {
        Ok(self.u32()? as i32)
    }
    fn f32(&mut self) -> Result<f32, PackageError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn ci(&mut self) -> Result<i32, PackageError> {
        read_compact_index(self.d, &mut self.o)
    }
    #[allow(dead_code)]
    fn count(&mut self, elem: usize) -> Result<usize, PackageError> {
        self.count_capped(elem, 1_000_000)
    }
    fn count_capped(&mut self, elem: usize, max: usize) -> Result<usize, PackageError> {
        let n = self.ci()?;
        if n < 0 {
            return Err(PackageError::UnexpectedEof { offset: self.o, requested: 0, total: self.d.len() });
        }
        let n = n as usize;
        if n > max {
            return Err(PackageError::UnexpectedEof { offset: self.o, requested: n * elem, total: self.d.len() });
        }
        // Hardened FArray 0*2 GPF: elem==0 bypasses need() — use 1 byte min check
        let check_elem = if elem == 0 { 1 } else { elem };
        if elem == 0 && n > 8192 {
            return Err(PackageError::UnexpectedEof { offset: self.o, requested: n, total: 8192 });
        }
        let need = n.checked_mul(check_elem).ok_or(PackageError::UnexpectedEof { offset: self.o, requested: usize::MAX, total: self.d.len() })?;
        self.need(need)?;
        Ok(n)
    }
    fn skip(&mut self, n: usize) -> Result<(), PackageError> {
        self.need(n)?;
        self.o += n;
        Ok(())
    }
}

/// Finds lazy arrays (`i32 AbsEnd, compact N, N*elem bytes`) from `start`.
fn lazy_arrays(d: &[u8], base: usize, start: usize) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    let mut p = start;
    while p + 8 < d.len() {
        let skip = i32::from_le_bytes(d[p..p + 4].try_into().unwrap());
        if skip > 0 && (skip as usize) > base + p && (skip as usize) <= base + d.len() {
            let mut q = p + 4;
            if let Ok(n) = read_compact_index(d, &mut q) {
                if n <= 0 || n as usize > 500_000 { p += 1; continue; }
                let end = skip as usize - base;
                if end > q && (end - q).is_multiple_of(n as usize) && matches!((end - q) / n as usize, 2 | 4 | 8 | 10 | 12 | 16) {
                    out.push((q, n as usize, (end - q) / n as usize));
                    p = end;
                    continue;
                }
            }
        }
        p += 1;
    }
    out
}

pub fn read_skeletal_mesh(pkg: &UnrealPackage<'_>, export_index: usize) -> Result<SkeletalMesh, PackageError> {
    let e = pkg.exports.get(export_index).ok_or(PackageError::ExportIndexOutOfBounds(export_index, pkg.exports.len()))?;
    let base = e.serial_offset as usize;
    let d = pkg.get_export_slice(e)?;
    let props = read_properties(pkg, d, 0)?;
    let mut c = Cur { d, o: props.end_offset };
    c.skip(25 + 16)?;
    let version = c.i32()?;
    let _vertex_count = c.i32()?;
    let n = c.count_capped(4, 200_000)?;
    c.skip(n * 4)?; // Verts (vertex-animated meshes only)
    let n = c.count_capped(1, 64)?;
    let mut textures = Vec::with_capacity(n);
    for _ in 0..n {
        textures.push(c.ci()?);
    }
    let mesh_scale = [c.f32()?, c.f32()?, c.f32()?];
    let mesh_origin = [c.f32()?, c.f32()?, c.f32()?];
    let rot_origin = [c.i32()?, c.i32()?, c.i32()?];
    let n = c.count_capped(2, 200_000)?;
    c.skip(n * 2)?; // FaceLevel
    let n = c.count_capped(8, 200_000)?;
    c.skip(n * 8)?; // Faces
    let n = c.count_capped(2, 200_000)?;
    c.skip(n * 2)?; // CollapseWedgeThus
    let n = c.count_capped(10, 200_000)?;
    c.skip(n * 10)?; // Wedges (LOD mesh)
    let n = c.count_capped(8, 64)?;
    let mut materials = Vec::with_capacity(n);
    for _ in 0..n {
        materials.push((c.u32()?, c.i32()?));
    }
    c.skip(24)?; // MeshScaleMax, LODHysteresis, LODStrength, LODMinVerts, LODMorph, LODZDisplace
    if version >= 3 {
        c.skip(4)?; // HasImpostor
        c.ci()?; // SpriteMaterial
        c.skip(12 * 3 + 4 + 12)?; // ImpScale, ImpRotation, ImpLocation, ImpColor, 3 modes
    }
    if version >= 4 {
        c.skip(4)?; // SkinTesselationFactor
    }
    let n = c.count_capped(12, 200_000)?;
    c.skip(n * 12)?; // Points2
    let nb = c.count_capped(57, 512)?;
    let mut bones = Vec::with_capacity(nb);
    for _ in 0..nb {
        let name_idx = c.ci()?;
        let flags = c.u32()?;
        let orientation = [c.f32()?, c.f32()?, c.f32()?, c.f32()?];
        let position = [c.f32()?, c.f32()?, c.f32()?];
        c.skip(16)?; // Length, XSize, YSize, ZSize
        let num_children = c.i32()?;
        let parent = c.i32()?;
        bones.push(Bone { name: pkg.resolve_name(name_idx).unwrap_or("?").to_string(), flags, orientation, position, num_children, parent });
    }
    let animation = c.ci()?;

    // Full-resolution geometry: last run of lazy arrays with element sizes 12,10,12,8,2,2.
    let arrays = lazy_arrays(d, base, c.o);
    let pattern = [12usize, 10, 12, 8, 2, 2];
    let start = (0..arrays.len().saturating_sub(5))
        .rev()
        .find(|&i| (0..6).all(|k| arrays[i + k].2 == pattern[k]))
        .ok_or(PackageError::UnexpectedEof { offset: c.o, requested: 0, total: d.len() })?;
    let a = |k: usize| arrays[start + k];
    let rd_f = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let rd_u16 = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
    let (po, pn, _) = a(0);
    let points = (0..pn).map(|i| [rd_f(po + i * 12), rd_f(po + i * 12 + 4), rd_f(po + i * 12 + 8)]).collect();
    let (wo, wn, _) = a(1);
    let wedges = (0..wn).map(|i| Wedge { point: rd_u16(wo + i * 10), uv: [rd_f(wo + i * 10 + 2), rd_f(wo + i * 10 + 6)] }).collect();
    let (to, tn, _) = a(2);
    let triangles = (0..tn)
        .map(|i| {
            let b = to + i * 12;
            Triangle { wedges: [rd_u16(b), rd_u16(b + 2), rd_u16(b + 4)], material: d[b + 6] }
        })
        .collect();
    let (io, inn, _) = a(3);
    let influences = (0..inn)
        .map(|i| {
            let b = io + i * 8;
            Influence { weight: rd_f(b), point: rd_u16(b + 4), bone: rd_u16(b + 6) }
        })
        .collect();

    Ok(SkeletalMesh {
        name: pkg.format_full_name(export_index as i32 + 1),
        version,
        textures,
        materials,
        mesh_scale,
        mesh_origin,
        rot_origin,
        bones,
        animation,
        points,
        wedges,
        triangles,
        influences,
    })
}

impl SkeletalMesh {
    /// Material object index for a triangle (via FMeshMaterial.TextureIndex).
    pub fn triangle_material(&self, t: &Triangle) -> i32 {
        let ti = self.materials.get(t.material as usize).map(|m| m.1).unwrap_or(t.material as i32);
        self.textures.get(ti.max(0) as usize).copied().unwrap_or(0)
    }
}
