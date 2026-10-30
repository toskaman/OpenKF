//! Reads the field list of a compiled UnrealScript class (UE2, package v128):
//! the order of `Children`/`Next` and the net flags needed to rebuild the
//! network `FClassNetCache`.

use crate::compact_index::read_compact_index;
use crate::error::PackageError;
use crate::level::{export_class_name, RF_HAS_STACK};
use crate::reader::UnrealPackage;

pub const CPF_NET: u32 = 0x20;
pub const FUNC_NET: u32 = 0x40;

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    /// Export class name: IntProperty, StructProperty, Function, Const, Enum, State...
    pub kind: String,
    pub array_dim: u32,
    pub property_flags: u32,
    pub function_flags: u32,
    /// For object-ish properties: referenced struct/class (full name), else empty.
    pub inner: String,
    /// Raw object reference of `inner` (package-local index).
    pub inner_ref: i32,
    pub export_index: usize,
}

impl FieldInfo {
    pub fn is_property(&self) -> bool {
        self.kind.ends_with("Property")
    }
    pub fn is_net(&self) -> bool {
        (self.is_property() && self.property_flags & CPF_NET != 0)
            || (self.kind == "Function" && self.function_flags & FUNC_NET != 0)
    }
}

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    /// Full name of the superclass (e.g. `Engine.Actor`), empty for Object.
    pub super_full: String,
    pub fields: Vec<FieldInfo>,
}

fn skip_header(d: &[u8], flags: u32, o: &mut usize) -> Result<(), PackageError> {
    if flags & RF_HAS_STACK != 0 {
        let node = read_compact_index(d, o)?;
        let _ = read_compact_index(d, o)?;
        *o += 12;
        if node != 0 {
            let _ = read_compact_index(d, o)?;
        }
    }
    Ok(())
}

fn i32_at(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Fields of any UStruct export (Class, ScriptStruct, Function, State).
pub fn read_class_info(pkg: &UnrealPackage<'_>, class_export: usize) -> Result<ClassInfo, PackageError> {
    let e = pkg.exports.get(class_export).ok_or(PackageError::ExportIndexOutOfBounds(class_export, pkg.exports.len()))?;
    let d = pkg.get_export_slice(e)?;
    let mut o = 0;
    skip_header(d, e.object_flags, &mut o)?;
    // UClass: no tagged properties here (defaults are at the end). Others: "None".
    if e.class_index != 0 {
        let n = read_compact_index(d, &mut o)?;
        if pkg.resolve_name(n).unwrap_or("") != "None" {
            return Err(PackageError::InvalidData("struct tagged props not None".into()));
        }
    }
    let sup = read_compact_index(d, &mut o)?; // UField::Super
    let _next = read_compact_index(d, &mut o)?; // UField::Next
    let _text = read_compact_index(d, &mut o)?; // UStruct::ScriptText
    let mut child = read_compact_index(d, &mut o)?; // UStruct::Children
    let mut fields = Vec::new();
    let mut guard = 0;
    while child > 0 && guard < 10_000 {
        guard += 1;
        let ci = (child - 1) as usize;
        let fe = pkg.exports.get(ci).ok_or(PackageError::ExportIndexOutOfBounds(ci, pkg.exports.len()))?;
        let fd = pkg.get_export_slice(fe)?;
        let kind = export_class_name(pkg, ci).to_string();
        let name = pkg.resolve_name(fe.object_name).unwrap_or("?").to_string();
        let mut p = 0;
        skip_header(fd, fe.object_flags, &mut p)?;
        if kind != "Function" && kind != "State" {
            // Tagged properties of the field object itself: expect a single "None".
            let n = read_compact_index(fd, &mut p)?;
            if pkg.resolve_name(n).unwrap_or("") != "None" {
                return Err(PackageError::InvalidData(format!("{name}: tagged props not None")));
            }
        } else {
            let n = read_compact_index(fd, &mut p)?;
            if pkg.resolve_name(n).unwrap_or("") != "None" {
                return Err(PackageError::InvalidData(format!("{name}: fn tagged props not None")));
            }
        }
        let _fsup = read_compact_index(fd, &mut p)?;
        let next = read_compact_index(fd, &mut p)?;
        let (mut array_dim, mut property_flags, mut function_flags, mut inner, mut inner_ref) = (1, 0, 0, String::new(), 0);
        if kind.ends_with("Property") {
            array_dim = i32_at(fd, p).unwrap_or(1);
            property_flags = i32_at(fd, p + 4).unwrap_or(0);
            p += 8;
            let _cat = read_compact_index(fd, &mut p)?;
            if property_flags & CPF_NET != 0 {
                p += 2;
            }
            if matches!(kind.as_str(), "ObjectProperty" | "ClassProperty" | "StructProperty" | "ByteProperty" | "ArrayProperty") && p < fd.len() {
                let r = read_compact_index(fd, &mut p)?;
                inner = pkg.format_full_name(r);
                inner_ref = r;
            }
        } else if kind == "Function" {
            // Tail: iNative u16, OperPrecedence u8, FunctionFlags u32, [RepOffset u16 if FUNC_NET].
            let n = fd.len();
            let a = n.checked_sub(4).and_then(|x| i32_at(fd, x)).unwrap_or(0);
            let b = n.checked_sub(6).and_then(|x| i32_at(fd, x)).unwrap_or(0);
            function_flags = if a & FUNC_NET == 0 && b & FUNC_NET == 0 { a } else if b & FUNC_NET != 0 { b } else { a };
        }
        fields.push(FieldInfo { name, kind, array_dim, property_flags, function_flags, inner, inner_ref, export_index: ci });
        child = next;
    }
    Ok(ClassInfo {
        name: pkg.resolve_name(e.object_name).unwrap_or("?").to_string(),
        super_full: if sup == 0 { String::new() } else { pkg.format_full_name(sup) },
        fields,
    })
}

/// Number of names in a UEnum export.
pub fn read_enum_count(pkg: &UnrealPackage<'_>, enum_export: usize) -> Result<u32, PackageError> {
    let e = pkg.exports.get(enum_export).ok_or(PackageError::ExportIndexOutOfBounds(enum_export, pkg.exports.len()))?;
    let d = pkg.get_export_slice(e)?;
    let mut o = 0;
    skip_header(d, e.object_flags, &mut o)?;
    let _none = read_compact_index(d, &mut o)?;
    let _sup = read_compact_index(d, &mut o)?;
    let _next = read_compact_index(d, &mut o)?;
    Ok(read_compact_index(d, &mut o)? as u32)
}
