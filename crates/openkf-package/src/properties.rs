//! UE2 tagged property stream (`UObject::SerializeTaggedProperties`, package v128).
//!
//! Every serialized UObject starts with a list of tagged properties terminated by
//! the name `None`. This module parses that list without needing class metadata,
//! which is enough to read assets (textures, meshes, sounds) and map actors.

use crate::compact_index::read_compact_index;
use crate::error::PackageError;
use crate::reader::UnrealPackage;

/// Property type id stored in the low 4 bits of the tag info byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropType {
    Byte,
    Int,
    Bool,
    Float,
    Object,
    Name,
    String,
    Class,
    Array,
    Struct,
    Vector,
    Rotator,
    Str,
    Map,
    FixedArray,
    Unknown(u8),
}

impl PropType {
    fn from_id(id: u8) -> Self {
        match id {
            1 => Self::Byte,
            2 => Self::Int,
            3 => Self::Bool,
            4 => Self::Float,
            5 => Self::Object,
            6 => Self::Name,
            7 => Self::String,
            8 => Self::Class,
            9 => Self::Array,
            10 => Self::Struct,
            11 => Self::Vector,
            12 => Self::Rotator,
            13 => Self::Str,
            14 => Self::Map,
            15 => Self::FixedArray,
            other => Self::Unknown(other),
        }
    }
}

/// Decoded value of a property. Complex values keep their raw bytes.
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue<'a> {
    Byte(u8),
    Int(i32),
    Bool(bool),
    Float(f32),
    /// Object reference (package object index: 0 none, <0 import, >0 export).
    Object(i32),
    Name(String),
    Str(String),
    Vector([f32; 3]),
    /// Pitch, Yaw, Roll in Unreal rotation units (65536 = 360 degrees).
    Rotator([i32; 3]),
    /// Struct name + raw payload.
    Struct(String, &'a [u8]),
    /// Any other type (arrays, maps...) kept as raw bytes.
    Raw(&'a [u8]),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property<'a> {
    pub name: String,
    pub ty: PropType,
    pub array_index: u32,
    pub value: PropValue<'a>,
}

/// Parsed property list plus the offset where the native (non-property) payload begins.
#[derive(Debug, Clone, Default)]
pub struct PropertyList<'a> {
    pub props: Vec<Property<'a>>,
    pub end_offset: usize,
}

impl<'a> PropertyList<'a> {
    pub fn get(&self, name: &str) -> Option<&PropValue<'a>> {
        self.props
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name) && p.array_index == 0)
            .map(|p| &p.value)
    }
    pub fn get_int(&self, name: &str) -> Option<i32> {
        match self.get(name)? {
            PropValue::Int(v) => Some(*v),
            PropValue::Byte(v) => Some(*v as i32),
            _ => None,
        }
    }
    pub fn get_byte(&self, name: &str) -> Option<u8> {
        match self.get(name)? {
            PropValue::Byte(v) => Some(*v),
            PropValue::Int(v) => Some(*v as u8),
            _ => None,
        }
    }
    pub fn get_bool(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            PropValue::Bool(v) => Some(*v),
            _ => None,
        }
    }
    pub fn get_object(&self, name: &str) -> Option<i32> {
        match self.get(name)? {
            PropValue::Object(v) => Some(*v),
            _ => None,
        }
    }
}

fn eof(offset: usize, requested: usize, total: usize) -> PackageError {
    PackageError::UnexpectedEof { offset, requested, total }
}

fn take<'a>(data: &'a [u8], off: &mut usize, n: usize) -> Result<&'a [u8], PackageError> {
    if *off + n > data.len() {
        return Err(eof(*off, n, data.len()));
    }
    let s = &data[*off..*off + n];
    *off += n;
    Ok(s)
}

fn u8_at(data: &[u8], off: &mut usize) -> Result<u8, PackageError> {
    Ok(take(data, off, 1)?[0])
}

fn i32_at(data: &[u8], off: &mut usize) -> Result<i32, PackageError> {
    Ok(i32::from_le_bytes(take(data, off, 4)?.try_into().unwrap()))
}

fn f32_at(data: &[u8], off: &mut usize) -> Result<f32, PackageError> {
    Ok(f32::from_le_bytes(take(data, off, 4)?.try_into().unwrap()))
}

/// Reads an UE2 `FString` (compact length; negative = UTF-16).
pub fn read_fstring(data: &[u8], off: &mut usize) -> Result<String, PackageError> {
    let len = read_compact_index(data, off)?;
    if len == 0 {
        return Ok(String::new());
    }
    if len > 0 {
        let bytes = take(data, off, len as usize)?;
        let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        Ok(bytes.iter().map(|&b| b as char).collect())
    } else {
        let n = (-len) as usize;
        let bytes = take(data, off, n * 2)?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0)
            .collect();
        Ok(String::from_utf16_lossy(&units))
    }
}

/// Parses the tagged property list starting at `offset` within `data`.
pub fn read_properties<'a>(
    pkg: &UnrealPackage<'_>,
    data: &'a [u8],
    offset: usize,
) -> Result<PropertyList<'a>, PackageError> {
    let mut off = offset;
    let mut props = Vec::new();
    loop {
        let name_idx = read_compact_index(data, &mut off)?;
        let name = pkg.resolve_name(name_idx)?.to_string();
        if name.eq_ignore_ascii_case("None") {
            break;
        }
        let info = u8_at(data, &mut off)?;
        let ty = PropType::from_id(info & 0x0F);
        let size_code = (info >> 4) & 0x07;
        let array_flag = info & 0x80 != 0;

        let struct_name = if ty == PropType::Struct {
            let idx = read_compact_index(data, &mut off)?;
            Some(pkg.resolve_name(idx)?.to_string())
        } else {
            None
        };

        let size = match size_code {
            0 => 1,
            1 => 2,
            2 => 4,
            3 => 12,
            4 => 16,
            5 => u8_at(data, &mut off)? as usize,
            6 => u16::from_le_bytes(take(data, &mut off, 2)?.try_into().unwrap()) as usize,
            _ => i32_at(data, &mut off)? as usize,
        };

        let mut array_index = 0u32;
        if array_flag && ty != PropType::Bool {
            let b = u8_at(data, &mut off)? as u32;
            array_index = if b & 0x80 == 0 {
                b
            } else if b & 0xC0 == 0x80 {
                ((b & 0x7F) << 8) | u8_at(data, &mut off)? as u32
            } else {
                let rest = take(data, &mut off, 3)?;
                ((b & 0x3F) << 24) | (rest[0] as u32) << 16 | (rest[1] as u32) << 8 | rest[2] as u32
            };
        }

        // UE2 quirk seen in map packages: the declared size of an array of tagged
        // structs is too small by 1..=Count bytes (element `None` terminators not
        // counted; e.g. StaticMesh `Materials`: declared 12, actual 13). Accept the
        // structural length only within that bound and if a valid tag follows it.
        let size = if ty == PropType::Array {
            match struct_array_len(pkg, data, off) {
                Some((n, count)) if n > size && n - size <= count && valid_tag_at(pkg, data, off + n) => n,
                _ => size,
            }
        } else if ty == PropType::Struct
            && !matches!(struct_name.as_deref(), Some("Vector" | "Rotator" | "Color" | "Plane" | "Sphere" | "Box" | "Coords" | "Quat" | "Range" | "Scale" | "Matrix"))
        {
            // Same quirk for tagged structs (e.g. TerrainInfo `Layers`): the final
            // `None` of the struct is not counted in the declared size.
            match read_properties(pkg, data, off) {
                Ok(pl) if pl.end_offset == off + size + 1 && !pl.props.is_empty() && valid_tag_at(pkg, data, off + size + 1) => size + 1,
                _ => size,
            }
        } else {
            size
        };

        let value = if ty == PropType::Bool {
            PropValue::Bool(array_flag)
        } else {
            let payload = take(data, &mut off, size)?;
            let mut p = 0usize;
            match ty {
                PropType::Byte => PropValue::Byte(payload.first().copied().unwrap_or(0)),
                PropType::Int => PropValue::Int(i32_at(payload, &mut p)?),
                PropType::Float => PropValue::Float(f32_at(payload, &mut p)?),
                PropType::Object | PropType::Class => {
                    PropValue::Object(read_compact_index(payload, &mut p)?)
                }
                PropType::Name => {
                    let idx = read_compact_index(payload, &mut p)?;
                    PropValue::Name(pkg.resolve_name(idx)?.to_string())
                }
                PropType::Str | PropType::String => PropValue::Str(read_fstring(payload, &mut p)?),
                PropType::Struct => {
                    let sname = struct_name.unwrap_or_default();
                    match (sname.as_str(), payload.len()) {
                        ("Vector", 12) => PropValue::Vector([
                            f32_at(payload, &mut p)?,
                            f32_at(payload, &mut p)?,
                            f32_at(payload, &mut p)?,
                        ]),
                        ("Rotator", 12) => PropValue::Rotator([
                            i32_at(payload, &mut p)?,
                            i32_at(payload, &mut p)?,
                            i32_at(payload, &mut p)?,
                        ]),
                        _ => PropValue::Struct(sname, payload),
                    }
                }
                _ => PropValue::Raw(payload),
            }
        };

        props.push(Property { name, ty, array_index, value });
    }
    Ok(PropertyList { props, end_offset: off })
}

/// If `data[off..]` is `compact Count` followed by `Count` tagged-property structs
/// (each ending with `None`), returns (total byte length, Count); otherwise `None`.
fn struct_array_len(pkg: &UnrealPackage<'_>, data: &[u8], off: usize) -> Option<(usize, usize)> {
    let mut o = off;
    let n = read_compact_index(data, &mut o).ok()?;
    if !(1..=4096).contains(&n) {
        return None;
    }
    for _ in 0..n {
        // Every element must start with a resolvable, non-None name.
        let mut probe = o;
        let first = read_compact_index(data, &mut probe).ok()?;
        let first_name = pkg.resolve_name(first).ok()?;
        if first_name.eq_ignore_ascii_case("None") {
            return None;
        }
        let pl = read_properties(pkg, data, o).ok()?;
        if pl.props.is_empty() {
            return None;
        }
        o = pl.end_offset;
    }
    Some((o - off, n as usize))
}

/// True if a plausible property tag (or the terminating `None`) starts at `pos`.
fn valid_tag_at(pkg: &UnrealPackage<'_>, data: &[u8], pos: usize) -> bool {
    let mut o = pos;
    let Ok(idx) = read_compact_index(data, &mut o) else { return false };
    let Ok(name) = pkg.resolve_name(idx) else { return false };
    if name.eq_ignore_ascii_case("None") {
        return true;
    }
    match data.get(o) {
        Some(&info) => (1..=15).contains(&(info & 0x0F)),
        None => false,
    }
}
