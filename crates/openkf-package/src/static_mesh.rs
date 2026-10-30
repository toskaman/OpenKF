//! UE2 `UStaticMesh` decoding (package v128, Killing Floor v1065).
//!
//! Native layout after tagged properties (verified on retail .usx):
//! `FBox BoundingBox`, `FSphere BoundingSphere` (UPrimitive),
//! `TArray<FStaticMeshSection> Sections`, `FBox BoundingBox`,
//! `VertexStream { TArray<{Position, Normal}>, i32 Revision }`,
//! `ColorStream { TArray<FColor>, i32 }`, `AlphaStream { TArray<FColor>, i32 }`,
//! `TArray<UVStream { TArray<{U,V}>, i32 CoordIndex, i32 Revision }>`,
//! `IndexBuffer { TArray<u16>, i32 }`, `WireframeIndexBuffer { TArray<u16>, i32 }`, ...

use crate::compact_index::read_compact_index;
use crate::error::PackageError;
use crate::properties::{read_properties, PropValue, PropertyList};
use crate::reader::{ObjectRef, UnrealPackage};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BoundingBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub valid: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StaticMeshSection {
    pub is_strip: bool,
    pub first_index: u16,
    pub min_vertex_index: u16,
    pub max_vertex_index: u16,
    pub num_triangles: u16,
    pub num_primitives: u16,
}

#[derive(Debug, Clone)]
pub struct StaticMeshMaterial {
    /// Package object index of the material (0 = none).
    pub material: i32,
    /// Fully qualified material name, e.g. `PatchTex.Common.WHouseDoorSkin`.
    pub material_name: String,
    pub enable_collision: bool,
}

#[derive(Debug, Clone)]
pub struct StaticMesh<'a> {
    pub name: String,
    pub bounding_box: BoundingBox,
    /// Center + radius.
    pub bounding_sphere: [f32; 4],
    pub sections: Vec<StaticMeshSection>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// RGBA vertex colors (empty if none).
    pub colors: Vec<[u8; 4]>,
    /// One entry per UV channel.
    pub uv_channels: Vec<Vec<[f32; 2]>>,
    pub indices: Vec<u16>,
    pub materials: Vec<StaticMeshMaterial>,
    pub props: PropertyList<'a>,
}

struct Cur<'a> {
    d: &'a [u8],
    o: usize,
}

impl<'a> Cur<'a> {
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
    fn u16(&mut self) -> Result<u16, PackageError> {
        self.need(2)?;
        let v = u16::from_le_bytes([self.d[self.o], self.d[self.o + 1]]);
        self.o += 2;
        Ok(v)
    }
    fn i32(&mut self) -> Result<i32, PackageError> {
        self.need(4)?;
        let v = i32::from_le_bytes(self.d[self.o..self.o + 4].try_into().unwrap());
        self.o += 4;
        Ok(v)
    }
    fn f32(&mut self) -> Result<f32, PackageError> {
        Ok(f32::from_bits(self.i32()? as u32))
    }
    fn vec3(&mut self) -> Result<[f32; 3], PackageError> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
    fn count(&mut self, elem_size: usize) -> Result<usize, PackageError> {
        let n = read_compact_index(self.d, &mut self.o)?;
        if n < 0 {
            return Err(PackageError::UnexpectedEof { offset: self.o, requested: 0, total: self.d.len() });
        }
        // Hardened against retail FArray 0*2 GPF: elem_size==0 would bypass need() check.
        // Use 1 byte minimum for bounds validation when elem_size is 0 (e.g., UV channel count).
        let check_elem = if elem_size == 0 { 1 } else { elem_size };
        let need = (n as usize).checked_mul(check_elem).ok_or(PackageError::UnexpectedEof { offset: self.o, requested: usize::MAX, total: self.d.len() })?;
        if elem_size == 0 && n as usize > 8192 {
            return Err(PackageError::UnexpectedEof { offset: self.o, requested: n as usize, total: 8192 });
        }
        self.need(need)?;
        Ok(n as usize)
    }
    fn count_capped(&mut self, elem_size: usize, max: usize) -> Result<usize, PackageError> {
        let n = self.count(elem_size)?;
        if n > max {
            return Err(PackageError::UnexpectedEof { offset: self.o, requested: n, total: max });
        }
        Ok(n)
    }
    fn bbox(&mut self) -> Result<BoundingBox, PackageError> {
        Ok(BoundingBox { min: self.vec3()?, max: self.vec3()?, valid: self.u8()? != 0 })
    }
    fn colors(&mut self) -> Result<Vec<[u8; 4]>, PackageError> {
        // FColor is serialized R, G, B, A (same as UPalette).
        let n = self.count(4)?;
        let v = (0..n)
            .map(|i| {
                let c = &self.d[self.o + i * 4..];
                [c[0], c[1], c[2], c[3]]
            })
            .collect();
        self.o += n * 4;
        self.i32()?; // Revision
        Ok(v)
    }
    fn indices(&mut self) -> Result<Vec<u16>, PackageError> {
        let n = self.count(2)?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(self.u16()?);
        }
        self.i32()?; // Revision
        Ok(v)
    }
}

/// Parses the `Materials` array property (array of `StaticMeshMaterial` structs).
fn parse_materials(pkg: &UnrealPackage<'_>, raw: &[u8]) -> Vec<StaticMeshMaterial> {
    let mut out = Vec::new();
    let mut o = 0usize;
    let Ok(n) = read_compact_index(raw, &mut o) else { return out };
    for _ in 0..n.max(0) {
        let Ok(pl) = read_properties(pkg, raw, o) else { break };
        o = pl.end_offset;
        let material = pl.get_object("Material").unwrap_or(0);
        out.push(StaticMeshMaterial {
            material,
            material_name: if material != 0 { pkg.format_full_name(material) } else { String::new() },
            enable_collision: pl.get_bool("EnableCollision").unwrap_or(false),
        });
    }
    out
}

/// Reads a `UStaticMesh` export (0-based export index).
pub fn read_static_mesh<'a>(pkg: &UnrealPackage<'a>, export_index: usize) -> Result<StaticMesh<'a>, PackageError> {
    let d: &'a [u8] = pkg.get_export_data(export_index)?;
    let props = read_properties(pkg, d, 0)?;
    let materials = match props.get("Materials") {
        Some(PropValue::Raw(raw)) => parse_materials(pkg, raw),
        _ => Vec::new(),
    };

    let mut c = Cur { d, o: props.end_offset };
    let _primitive_box = c.bbox()?;
    let bounding_sphere = [c.f32()?, c.f32()?, c.f32()?, c.f32()?];

    let ns = c.count_capped(14, 64)?;
    let mut sections = Vec::with_capacity(ns);
    for _ in 0..ns {
        sections.push(StaticMeshSection {
            is_strip: c.i32()? != 0,
            first_index: c.u16()?,
            min_vertex_index: c.u16()?,
            max_vertex_index: c.u16()?,
            num_triangles: c.u16()?,
            num_primitives: c.u16()?,
        });
    }
    let bounding_box = c.bbox()?;

    let nv = c.count_capped(24, 200_000)?;
    let mut positions = Vec::with_capacity(nv);
    let mut normals = Vec::with_capacity(nv);
    for _ in 0..nv {
        positions.push(c.vec3()?);
        normals.push(c.vec3()?);
    }
    c.i32()?; // Revision

    let colors = c.colors()?;
    let _alpha = c.colors()?;

    let nuv = c.count_capped(0, 8)?;
    let mut uv_channels = Vec::with_capacity(nuv);
    for _ in 0..nuv {
        let n = c.count_capped(8, 200_000)?;
        let mut uv = Vec::with_capacity(n);
        for _ in 0..n {
            uv.push([c.f32()?, c.f32()?]);
        }
        c.i32()?; // CoordinateIndex
        c.i32()?; // Revision
        uv_channels.push(uv);
    }

    let indices = c.indices()?;

    Ok(StaticMesh {
        name: pkg.format_full_name(export_index as i32 + 1),
        bounding_box,
        bounding_sphere,
        sections,
        positions,
        normals,
        colors,
        uv_channels,
        indices,
        materials,
        props,
    })
}

/// Lists 0-based export indices whose class is `StaticMesh`.
pub fn static_mesh_exports(pkg: &UnrealPackage<'_>) -> Vec<usize> {
    pkg.exports
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| match pkg.resolve_object_ref(e.class_index) {
            Ok(ObjectRef::Import(imp)) => imp.resolve_object_name(&pkg.names) == Some("StaticMesh"),
            _ => false,
        })
        .map(|(i, _)| i)
        .collect()
}

impl StaticMesh<'_> {
    /// Triangle list (indices into `positions`) for one section, expanding strips.
    pub fn section_triangles(&self, s: &StaticMeshSection) -> Vec<[u16; 3]> {
        let start = s.first_index as usize;
        let mut tris = Vec::with_capacity(s.num_triangles as usize);
        if s.is_strip {
            let n = s.num_primitives as usize + 2;
            let idx = &self.indices[start.min(self.indices.len())..(start + n).min(self.indices.len())];
            for i in 2..idx.len() {
                let (a, b, c) = (idx[i - 2], idx[i - 1], idx[i]);
                if a == b || b == c || a == c {
                    continue;
                }
                tris.push(if i % 2 == 0 { [a, b, c] } else { [b, a, c] });
            }
        } else {
            let n = s.num_primitives as usize * 3;
            let idx = &self.indices[start.min(self.indices.len())..(start + n).min(self.indices.len())];
            for t in idx.chunks_exact(3) {
                tris.push([t[0], t[1], t[2]]);
            }
        }
        tris
    }
}
