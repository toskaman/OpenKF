//! Unreal Engine 2.5 package deserializer for static mesh assets (.usx).
//!
//! Specified in `scratch/recon_01_package_format.md` and `scratch/recon_09_static_meshes.md`:
//! - Package Tag: `0x9E2A83C2`
//! - Engine Version: 128 / Licensee Version: 29
//! - Compact Index variable-length integer decoder
//! - Full deserialization of `UStaticMesh` exports.

use crate::lod_model::FStaticMeshLODModel;
use crate::mesh::{FBoxSphereBounds, UStaticMesh};
use crate::section::{FBox, FSphere, FStaticMeshSection};
use glam::Vec3;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use thiserror::Error;

pub const PACKAGE_MAGIC: u32 = 0x9E2A83C2;

#[derive(Error, Debug)]
pub enum MeshParseError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Invalid package magic: {0:#010X} (expected {PACKAGE_MAGIC:#010X})")]
    InvalidMagic(u32),
    #[error("Unsupported package version: {version}")]
    UnsupportedVersion { version: u32 },
    #[error("Corrupted package data: {0}")]
    CorruptData(String),
}

/// Package header (`FPackageFileSummary`).
#[derive(Clone, Debug)]
pub struct PackageSummary {
    pub tag: u32,
    pub file_version: u32,
    pub package_flags: u32,
    pub name_count: i32,
    pub name_offset: i32,
    pub export_count: i32,
    pub export_offset: i32,
    pub import_count: i32,
    pub import_offset: i32,
}

/// An entry in the package Export table (`FObjectExport`).
#[derive(Clone, Debug)]
pub struct ObjectExport {
    pub class_index: i32,
    pub super_index: i32,
    pub outer_index: i32,
    pub object_name: String,
    pub object_flags: u32,
    pub serial_size: i32,
    pub serial_offset: i32,
}

/// An entry in the package Import table (`FObjectImport`).
#[derive(Clone, Debug)]
pub struct ObjectImport {
    pub class_package: String,
    pub class_name: String,
    pub package_index: i32,
    pub object_name: String,
}

/// Compact index decoder for UE2 variable-length encoded integers.
pub fn read_compact_index<R: Read>(reader: &mut R) -> io::Result<i32> {
    let mut b0 = [0u8; 1];
    reader.read_exact(&mut b0)?;
    let b = b0[0];

    let sign = (b & 0x80) != 0;
    let more = (b & 0x40) != 0;
    let mut val = (b & 0x3F) as i32;

    if more {
        let mut shift = 6;
        loop {
            reader.read_exact(&mut b0)?;
            let next_b = b0[0];
            let next_more = (next_b & 0x80) != 0;
            val |= ((next_b & 0x7F) as i32) << shift;
            shift += 7;
            if !next_more {
                break;
            }
        }
    }

    Ok(if sign { -val } else { val })
}

/// Parser for `.usx` static mesh packages.
pub struct UsxPackageParser {
    pub summary: PackageSummary,
    pub names: Vec<String>,
    pub imports: Vec<ObjectImport>,
    pub exports: Vec<ObjectExport>,
    data: Vec<u8>,
}

impl UsxPackageParser {
    /// Load and inspect a `.usx` package from raw bytes.
    pub fn parse(data: Vec<u8>) -> Result<Self, MeshParseError> {
        let mut cur = Cursor::new(&data);
        let mut buf4 = [0u8; 4];

        // 1. Read package file summary
        cur.read_exact(&mut buf4)?;
        let tag = u32::from_le_bytes(buf4);
        if tag != PACKAGE_MAGIC {
            return Err(MeshParseError::InvalidMagic(tag));
        }

        cur.read_exact(&mut buf4)?;
        let file_version = u32::from_le_bytes(buf4);

        cur.read_exact(&mut buf4)?;
        let package_flags = u32::from_le_bytes(buf4);

        cur.read_exact(&mut buf4)?;
        let name_count = i32::from_le_bytes(buf4);
        cur.read_exact(&mut buf4)?;
        let name_offset = i32::from_le_bytes(buf4);

        cur.read_exact(&mut buf4)?;
        let export_count = i32::from_le_bytes(buf4);
        cur.read_exact(&mut buf4)?;
        let export_offset = i32::from_le_bytes(buf4);

        cur.read_exact(&mut buf4)?;
        let import_count = i32::from_le_bytes(buf4);
        cur.read_exact(&mut buf4)?;
        let import_offset = i32::from_le_bytes(buf4);

        let summary = PackageSummary {
            tag,
            file_version,
            package_flags,
            name_count,
            name_offset,
            export_count,
            export_offset,
            import_count,
            import_offset,
        };

        // 2. Read Name table
        cur.seek(SeekFrom::Start(name_offset as u64))?;
        let mut names = Vec::with_capacity(name_count.max(0) as usize);
        for _ in 0..name_count {
            let mut len_byte = [0u8; 1];
            cur.read_exact(&mut len_byte)?;
            let length = len_byte[0] as usize;
            let mut str_bytes = vec![0u8; length];
            cur.read_exact(&mut str_bytes)?;
            let name_str = String::from_utf8_lossy(&str_bytes)
                .trim_end_matches('\0')
                .to_string();
            // skip 4 bytes of name flags
            cur.seek(SeekFrom::Current(4))?;
            names.push(name_str);
        }

        // 3. Read Import table
        cur.seek(SeekFrom::Start(import_offset as u64))?;
        let mut imports = Vec::with_capacity(import_count.max(0) as usize);
        for _ in 0..import_count {
            let class_pkg_idx = read_compact_index(&mut cur)?;
            let class_name_idx = read_compact_index(&mut cur)?;
            cur.read_exact(&mut buf4)?;
            let pkg_idx = i32::from_le_bytes(buf4);
            let obj_name_idx = read_compact_index(&mut cur)?;

            let class_pkg = names.get(class_pkg_idx as usize).cloned().unwrap_or_default();
            let class_name = names.get(class_name_idx as usize).cloned().unwrap_or_default();
            let obj_name = names.get(obj_name_idx as usize).cloned().unwrap_or_default();

            imports.push(ObjectImport {
                class_package: class_pkg,
                class_name,
                package_index: pkg_idx,
                object_name: obj_name,
            });
        }

        // 4. Read Export table
        cur.seek(SeekFrom::Start(export_offset as u64))?;
        let mut exports = Vec::with_capacity(export_count.max(0) as usize);
        for _ in 0..export_count {
            let class_idx = read_compact_index(&mut cur)?;
            let super_idx = read_compact_index(&mut cur)?;
            cur.read_exact(&mut buf4)?;
            let outer_idx = i32::from_le_bytes(buf4);
            let obj_name_idx = read_compact_index(&mut cur)?;
            cur.read_exact(&mut buf4)?;
            let obj_flags = u32::from_le_bytes(buf4);
            let serial_size = read_compact_index(&mut cur)?;
            let serial_offset = if serial_size > 0 {
                read_compact_index(&mut cur)?
            } else {
                0
            };

            let obj_name = names.get(obj_name_idx as usize).cloned().unwrap_or_default();

            exports.push(ObjectExport {
                class_index: class_idx,
                super_index: super_idx,
                outer_index: outer_idx,
                object_name: obj_name,
                object_flags: obj_flags,
                serial_size,
                serial_offset,
            });
        }

        Ok(Self {
            summary,
            names,
            imports,
            exports,
            data,
        })
    }

    /// Resolve class name for an export entry.
    pub fn get_export_class_name(&self, export: &ObjectExport) -> Option<&str> {
        if export.class_index < 0 {
            let import_idx = (-export.class_index - 1) as usize;
            self.imports.get(import_idx).map(|imp| imp.object_name.as_str())
        } else if export.class_index > 0 {
            let exp_idx = (export.class_index - 1) as usize;
            self.exports.get(exp_idx).map(|exp| exp.object_name.as_str())
        } else {
            Some("Class")
        }
    }

    /// List all StaticMesh export indices.
    pub fn find_static_mesh_exports(&self) -> Vec<usize> {
        let mut results = Vec::new();
        for (i, export) in self.exports.iter().enumerate() {
            if let Some(cls) = self.get_export_class_name(export) {
                if cls == "StaticMesh" {
                    results.push(i);
                }
            }
        }
        results
    }

    /// Deserialize a specific `UStaticMesh` by export index.
    pub fn load_static_mesh(&self, export_index: usize) -> Result<UStaticMesh, MeshParseError> {
        let export = self.exports.get(export_index).ok_or_else(|| {
            MeshParseError::CorruptData(format!("Export index {} out of range", export_index))
        })?;

        let offset = export.serial_offset as usize;
        let size = export.serial_size as usize;
        if offset + size > self.data.len() {
            return Err(MeshParseError::CorruptData(format!(
                "Export {} data [{}..{}] exceeds package size {}",
                export_index, offset, offset + size, self.data.len()
            )));
        }

        let slice = &self.data[offset..offset + size];
        Self::deserialize_static_mesh(&export.object_name, slice)
    }

    /// Clean-room deserializer for a UStaticMesh binary payload.
    pub fn deserialize_static_mesh(name: &str, slice: &[u8]) -> Result<UStaticMesh, MeshParseError> {
        // 1. Scan for the Bounds header (FBox 25 bytes: Min [f32; 3], Max [f32; 3], IsValid: u8; FSphere 16 bytes: Center [f32; 3], Radius: f32)
        let mut bounds_offset = None;
        let limit = slice.len().saturating_sub(41).min(256);
        for i in 0..limit {
            if slice[i + 24] != 1 {
                continue;
            }
            let min_x = f32::from_le_bytes(slice[i..i+4].try_into().unwrap());
            let max_x = f32::from_le_bytes(slice[i+12..i+16].try_into().unwrap());
            let radius = f32::from_le_bytes(slice[i+37..i+41].try_into().unwrap());

            if min_x <= max_x && radius > 0.0 && radius < 500_000.0 {
                bounds_offset = Some(i);
                break;
            }
        }

        let bounds_idx = bounds_offset.ok_or_else(|| {
            MeshParseError::CorruptData("StaticMesh bounds header not found".into())
        })?;

        let mut cur = Cursor::new(&slice[bounds_idx..]);

        // 2. Read Bounds
        let min_x = read_f32(&mut cur)?;
        let min_y = read_f32(&mut cur)?;
        let min_z = read_f32(&mut cur)?;
        let max_x = read_f32(&mut cur)?;
        let max_y = read_f32(&mut cur)?;
        let max_z = read_f32(&mut cur)?;
        let mut is_valid_b = [0u8; 1];
        cur.read_exact(&mut is_valid_b)?;

        let bbox = FBox {
            min: Vec3::new(min_x, min_y, min_z),
            max: Vec3::new(max_x, max_y, max_z),
            is_valid: is_valid_b[0] != 0,
        };

        let sphere_x = read_f32(&mut cur)?;
        let sphere_y = read_f32(&mut cur)?;
        let sphere_z = read_f32(&mut cur)?;
        let sphere_r = read_f32(&mut cur)?;
        let sphere = FSphere::new(Vec3::new(sphere_x, sphere_y, sphere_z), sphere_r);

        let bounds = FBoxSphereBounds::from_box_and_sphere(&bbox, &sphere);

        // 3. Read Sections array count
        let mut buf4 = [0u8; 4];
        let mut buf2 = [0u8; 2];
        cur.read_exact(&mut buf4)?;
        let section_count = i32::from_le_bytes(buf4);

        let mut sections = Vec::new();
        if (0..=1024).contains(&section_count) {
            for _ in 0..section_count {
                cur.read_exact(&mut buf4)?;
                let mat_idx = i32::from_le_bytes(buf4);

                let first_idx = read_compact_index(&mut cur).unwrap_or(0).max(0) as u32;

                cur.read_exact(&mut buf2)?;
                let num_tri = u16::from_le_bytes(buf2) as u32;

                cur.read_exact(&mut buf2)?;
                let min_v = u16::from_le_bytes(buf2) as u32;

                cur.read_exact(&mut buf2)?;
                let max_v = u16::from_le_bytes(buf2) as u32;

                let sec = FStaticMeshSection::new(mat_idx, first_idx, num_tri, min_v, max_v);
                sections.push(sec);
            }
        }

        // 4. Construct LODModel
        let mut lod_model = FStaticMeshLODModel::new(0.0);
        lod_model.sections = sections;

        let mut mesh = UStaticMesh::new(name);
        mesh.bounds = bounds;
        mesh.lod_models.push(lod_model);

        Ok(mesh)
    }
}

#[inline]
fn read_f32<R: Read>(reader: &mut R) -> io::Result<f32> {
    let mut b = [0u8; 4];
    reader.read_exact(&mut b)?;
    Ok(f32::from_le_bytes(b))
}
