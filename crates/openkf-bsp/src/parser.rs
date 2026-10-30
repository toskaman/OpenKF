//! Binary parser for Unreal Engine 2 / Killing Floor BSP tree and geometry data.

use crate::types::{BspZone, FBox, FBspNode, FBspSurf, FPlane, FSphere, FVector, FVert};
use crate::model::UModel;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum BspError {
    #[error("Buffer underflow: {0}")]
    BufferUnderflow(&'static str),
    #[error("Invalid BSP data format: {0}")]
    InvalidFormat(String),
    #[error("Array index out of bounds: index {index}, max {len}")]
    IndexOutOfBounds { index: usize, len: usize },
    #[error("IO error: {0}")]
    Io(String),
}

/// Decodes an Unreal Engine FCompactIndex from bytes at the given offset.
/// Returns decoded value and advances the offset.
pub fn decode_compact_index(data: &[u8], offset: &mut usize) -> Result<i32, BspError> {
    if *offset >= data.len() {
        return Err(BspError::BufferUnderflow("compact index byte 0"));
    }
    let b0 = data[*offset];
    *offset += 1;

    let sign = if (b0 & 0x80) != 0 { -1 } else { 1 };
    let mut v: i32 = 0;

    if (b0 & 0x40) != 0 {
        if *offset >= data.len() {
            return Err(BspError::BufferUnderflow("compact index byte 1"));
        }
        let b1 = data[*offset];
        *offset += 1;

        if (b1 & 0x80) != 0 {
            if *offset >= data.len() {
                return Err(BspError::BufferUnderflow("compact index byte 2"));
            }
            let b2 = data[*offset];
            *offset += 1;

            if (b2 & 0x80) != 0 {
                if *offset >= data.len() {
                    return Err(BspError::BufferUnderflow("compact index byte 3"));
                }
                let b3 = data[*offset];
                *offset += 1;

                if (b3 & 0x80) != 0 {
                    if *offset >= data.len() {
                        return Err(BspError::BufferUnderflow("compact index byte 4"));
                    }
                    let b4 = data[*offset];
                    *offset += 1;
                    v = b4 as i32;
                }
                v = (v << 7) + ((b3 & 0x7F) as i32);
            }
            v = (v << 7) + ((b2 & 0x7F) as i32);
        }
        v = (v << 7) + ((b1 & 0x7F) as i32);
    }
    v = (v << 6) + ((b0 & 0x3F) as i32);
    Ok(sign * v)
}

/// Encodes a 32-bit integer as an Unreal Engine FCompactIndex.
pub fn encode_compact_index(mut value: i32, out: &mut Vec<u8>) {
    let sign_bit = if value < 0 {
        value = -value;
        0x80u8
    } else {
        0x00u8
    };

    let mut b0 = sign_bit | ((value & 0x3F) as u8);
    value >>= 6;
    if value > 0 {
        b0 |= 0x40;
        out.push(b0);
        while value > 0 {
            let mut b = (value & 0x7F) as u8;
            value >>= 7;
            if value > 0 {
                b |= 0x80;
            }
            out.push(b);
        }
    } else {
        out.push(b0);
    }
}

/// Reads a standard 3D FVector (3 x f32, 12 bytes).
pub fn read_fvector(data: &[u8], offset: &mut usize) -> Result<FVector, BspError> {
    if *offset + 12 > data.len() {
        return Err(BspError::BufferUnderflow("FVector"));
    }
    let mut cursor = Cursor::new(&data[*offset..*offset + 12]);
    *offset += 12;
    let x = cursor.read_f32::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;
    let y = cursor.read_f32::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;
    let z = cursor.read_f32::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;
    Ok(FVector::new(x, y, z))
}

/// Reads an FPlane (4 x f32, 16 bytes).
pub fn read_fplane(data: &[u8], offset: &mut usize) -> Result<FPlane, BspError> {
    if *offset + 16 > data.len() {
        return Err(BspError::BufferUnderflow("FPlane"));
    }
    let mut cursor = Cursor::new(&data[*offset..*offset + 16]);
    *offset += 16;
    let x = cursor.read_f32::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;
    let y = cursor.read_f32::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;
    let z = cursor.read_f32::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;
    let w = cursor.read_f32::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;
    Ok(FPlane::new(x, y, z, w))
}

/// Reads an FBox (25 bytes: min FVector, max FVector, is_valid u8).
pub fn read_fbox(data: &[u8], offset: &mut usize) -> Result<FBox, BspError> {
    if *offset + 25 > data.len() {
        return Err(BspError::BufferUnderflow("FBox"));
    }
    let min = read_fvector(data, offset)?;
    let max = read_fvector(data, offset)?;
    let is_valid = data[*offset];
    *offset += 1;
    Ok(FBox { min, max, is_valid })
}

/// Reads an FSphere (16 bytes: center FVector, radius f32).
pub fn read_fsphere(data: &[u8], offset: &mut usize) -> Result<FSphere, BspError> {
    if *offset + 16 > data.len() {
        return Err(BspError::BufferUnderflow("FSphere"));
    }
    let center = read_fvector(data, offset)?;
    let mut cursor = Cursor::new(&data[*offset..*offset + 4]);
    *offset += 4;
    let radius = cursor.read_f32::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;
    Ok(FSphere { center, radius })
}

/// Reads an aligned or unaligned 64-byte `FBspNode` safely from slice.
pub fn read_fbsp_node_raw(data: &[u8], offset: &mut usize) -> Result<FBspNode, BspError> {
    if *offset + 64 > data.len() {
        return Err(BspError::BufferUnderflow("FBspNode"));
    }
    let slice = &data[*offset..*offset + 64];
    *offset += 64;
    let mut node = FBspNode::default();
    unsafe {
        std::ptr::copy_nonoverlapping(slice.as_ptr(), &mut node as *mut FBspNode as *mut u8, 64);
    }
    Ok(node)
}

/// Reads an aligned or unaligned 60-byte `FBspSurf` safely from slice.
pub fn read_fbsp_surf_raw(data: &[u8], offset: &mut usize) -> Result<FBspSurf, BspError> {
    if *offset + 60 > data.len() {
        return Err(BspError::BufferUnderflow("FBspSurf"));
    }
    let slice = &data[*offset..*offset + 60];
    *offset += 60;
    let mut surf = FBspSurf::default();
    unsafe {
        std::ptr::copy_nonoverlapping(slice.as_ptr(), &mut surf as *mut FBspSurf as *mut u8, 60);
    }
    Ok(surf)
}

/// Reads an aligned or unaligned 8-byte `FVert` safely from slice.
pub fn read_fvert_raw(data: &[u8], offset: &mut usize) -> Result<FVert, BspError> {
    if *offset + 8 > data.len() {
        return Err(BspError::BufferUnderflow("FVert"));
    }
    let slice = &data[*offset..*offset + 8];
    *offset += 8;
    let mut vert = FVert::default();
    unsafe {
        std::ptr::copy_nonoverlapping(slice.as_ptr(), &mut vert as *mut FVert as *mut u8, 8);
    }
    Ok(vert)
}

/// Serializer / Deserializer for UModel binary data.
pub struct UModelParser;

impl UModelParser {
    /// Parse a UModel from a raw binary dump (e.g. fixed struct layout).
    pub fn parse_raw(data: &[u8]) -> Result<UModel, BspError> {
        let mut offset = 0;
        if data.is_empty() {
            return Err(BspError::BufferUnderflow("empty data"));
        }

        // Optional leading byte 0x01 if exported with None property
        if data[0] == 0x01 && data.len() > 25 {
            offset = 1;
        }

        let bounding_box = read_fbox(data, &mut offset)?;
        let bounding_sphere = read_fsphere(data, &mut offset)?;

        // Vectors
        let num_vectors = decode_compact_index(data, &mut offset)?;
        if num_vectors < 0 {
            return Err(BspError::InvalidFormat("negative vector count".into()));
        }
        if num_vectors > 200_000 {
            return Err(BspError::InvalidFormat("vector count too large".into()));
        }
        let mut vectors = Vec::with_capacity(num_vectors as usize);
        for _ in 0..num_vectors {
            vectors.push(read_fvector(data, &mut offset)?);
        }

        // Points
        let num_points = decode_compact_index(data, &mut offset)?;
        if num_points < 0 {
            return Err(BspError::InvalidFormat("negative point count".into()));
        }
        if num_points > 200_000 {
            return Err(BspError::InvalidFormat("point count too large".into()));
        }
        let mut points = Vec::with_capacity(num_points as usize);
        for _ in 0..num_points {
            points.push(read_fvector(data, &mut offset)?);
        }

        // BspNodes (raw 64 bytes each)
        let num_nodes = decode_compact_index(data, &mut offset)?;
        if num_nodes < 0 {
            return Err(BspError::InvalidFormat("negative node count".into()));
        }
        if num_nodes > 200_000 {
            return Err(BspError::InvalidFormat("node count too large".into()));
        }
        let mut bsp_nodes = Vec::with_capacity(num_nodes as usize);
        for _ in 0..num_nodes {
            bsp_nodes.push(read_fbsp_node_raw(data, &mut offset)?);
        }

        // BspSurfs (raw 60 bytes each)
        let num_surfs = decode_compact_index(data, &mut offset)?;
        if num_surfs < 0 {
            return Err(BspError::InvalidFormat("negative surf count".into()));
        }
        if num_surfs > 200_000 {
            return Err(BspError::InvalidFormat("surf count too large".into()));
        }
        let mut bsp_surfs = Vec::with_capacity(num_surfs as usize);
        for _ in 0..num_surfs {
            bsp_surfs.push(read_fbsp_surf_raw(data, &mut offset)?);
        }

        // Verts (raw 8 bytes each)
        let num_verts = decode_compact_index(data, &mut offset)?;
        if num_verts < 0 {
            return Err(BspError::InvalidFormat("negative vert count".into()));
        }
        if num_verts > 200_000 {
            return Err(BspError::InvalidFormat("vert count too large".into()));
        }
        let mut verts = Vec::with_capacity(num_verts as usize);
        for _ in 0..num_verts {
            verts.push(read_fvert_raw(data, &mut offset)?);
        }

        // Shared sides & zones
        let mut num_shared_sides = 0;
        let mut num_zones = 0;
        let mut zones = Vec::new();
        if offset + 8 <= data.len() {
            let mut cur = Cursor::new(&data[offset..offset + 8]);
            offset += 8;
            num_shared_sides = cur.read_i32::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;
            num_zones = cur.read_i32::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;

            for _ in 0..num_zones.max(0) {
                if offset >= data.len() { break; }
                let _act = decode_compact_index(data, &mut offset)?;
                if offset + 20 <= data.len() {
                    let mut z_cur = Cursor::new(&data[offset..offset + 20]);
                    offset += 20;
                    let connectivity = z_cur.read_u64::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;
                    let visibility = z_cur.read_u64::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;
                    let last_render_time = z_cur.read_f32::<LittleEndian>().map_err(|e| BspError::Io(e.to_string()))?;
                    zones.push(BspZone {
                        connectivity,
                        visibility,
                        last_render_time,
                    });
                }
            }
        }

        Ok(UModel {
            bounding_box,
            bounding_sphere,
            vectors,
            points,
            bsp_nodes,
            bsp_surfs,
            verts,
            num_shared_sides,
            num_zones,
            zones,
            leaves: Vec::new(),
            root_outside: 0,
            linked: 1,
        })
    }

    /// Serializes a UModel into raw binary bytes following the UPrimitive::Serialize specification.
    pub fn serialize_raw(model: &UModel) -> Vec<u8> {
        let mut buf = Vec::new();

        // 1. FBox (25 bytes)
        buf.extend_from_slice(bytemuck::bytes_of(&model.bounding_box));

        // 2. FSphere (16 bytes)
        buf.extend_from_slice(bytemuck::bytes_of(&model.bounding_sphere));

        // 3. Vectors
        encode_compact_index(model.vectors.len() as i32, &mut buf);
        for v in &model.vectors {
            buf.extend_from_slice(bytemuck::bytes_of(v));
        }

        // 4. Points
        encode_compact_index(model.points.len() as i32, &mut buf);
        for p in &model.points {
            buf.extend_from_slice(bytemuck::bytes_of(p));
        }

        // 5. BspNodes
        encode_compact_index(model.bsp_nodes.len() as i32, &mut buf);
        for n in &model.bsp_nodes {
            buf.extend_from_slice(bytemuck::bytes_of(n));
        }

        // 6. BspSurfs
        encode_compact_index(model.bsp_surfs.len() as i32, &mut buf);
        for s in &model.bsp_surfs {
            buf.extend_from_slice(bytemuck::bytes_of(s));
        }

        // 7. Verts
        encode_compact_index(model.verts.len() as i32, &mut buf);
        for vert in &model.verts {
            buf.extend_from_slice(bytemuck::bytes_of(vert));
        }

        // 8. NumSharedSides & NumZones
        buf.extend_from_slice(&model.num_shared_sides.to_le_bytes());
        buf.extend_from_slice(&model.num_zones.to_le_bytes());

        // 9. Zones
        for z in &model.zones {
            encode_compact_index(0, &mut buf); // actor ref
            buf.extend_from_slice(&z.connectivity.to_le_bytes());
            buf.extend_from_slice(&z.visibility.to_le_bytes());
            buf.extend_from_slice(&z.last_render_time.to_le_bytes());
        }

        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_index_roundtrip() {
        let test_cases = vec![0, 1, -1, 42, -42, 63, 64, -64, 127, 128, 255, 1024, -1024, 65535, -65535, 1_000_000];
        for val in test_cases {
            let mut encoded = Vec::new();
            encode_compact_index(val, &mut encoded);
            let mut off = 0;
            let decoded = decode_compact_index(&encoded, &mut off).expect("decode failed");
            assert_eq!(val, decoded, "Mismatch for compact index value {}", val);
            assert_eq!(off, encoded.len(), "Did not consume full encoded bytes");
        }
    }

    #[test]
    fn test_umodel_raw_roundtrip() {
        let mut model = UModel::new();
        model.bounding_box = FBox {
            min: FVector::new(-512.0, -512.0, 0.0),
            max: FVector::new(512.0, 512.0, 512.0),
            is_valid: 1,
        };
        model.bounding_sphere = FSphere {
            center: FVector::new(0.0, 0.0, 256.0),
            radius: 600.0,
        };
        model.vectors = vec![
            FVector::new(0.0, 0.0, 1.0),
            FVector::new(1.0, 0.0, 0.0),
            FVector::new(0.0, 1.0, 0.0),
        ];
        model.points = vec![
            FVector::new(-100.0, -100.0, 0.0),
            FVector::new(100.0, -100.0, 0.0),
            FVector::new(100.0, 100.0, 0.0),
            FVector::new(-100.0, 100.0, 0.0),
        ];
        let mut node = FBspNode::default();
        node.plane = FPlane::new(0.0, 0.0, 1.0, 0.0);
        node.num_vertices = 4;
        node.i_vert_pool = 0;
        node.i_surf = 0;
        model.bsp_nodes.push(node);

        let mut surf = FBspSurf::default();
        surf.material = 10;
        surf.p_base = 0;
        surf.v_normal = 0;
        surf.v_texture_u = 1;
        surf.v_texture_v = 2;
        model.bsp_surfs.push(surf);

        model.verts = vec![
            FVert { p_vertex: 0, i_side: 0 },
            FVert { p_vertex: 1, i_side: 1 },
            FVert { p_vertex: 2, i_side: 2 },
            FVert { p_vertex: 3, i_side: 3 },
        ];

        let bytes = UModelParser::serialize_raw(&model);
        let parsed = UModelParser::parse_raw(&bytes).expect("parse_raw should succeed");

        assert_eq!(parsed.vectors.len(), 3);
        assert_eq!(parsed.points.len(), 4);
        assert_eq!(parsed.bsp_nodes.len(), 1);
        assert_eq!(parsed.bsp_surfs.len(), 1);
        assert_eq!(parsed.verts.len(), 4);
        assert_eq!(parsed.bsp_nodes[0].num_vertices, 4);
        assert_eq!(parsed.bsp_surfs[0].material, 10);
        assert_eq!(parsed.bsp_surfs[0].p_base, 0);
    }
}
