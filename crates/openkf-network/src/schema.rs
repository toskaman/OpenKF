//! Network schema built from the game's packages: package map (objects and
//! names) and per-class `FClassNetCache` (RepIndex -> property / RPC).
//!
//! Order rule (UE2 `UClass::Link` + `GetClassNetCache`): superclass fields first,
//! then the class's own net fields sorted by export index, keeping properties with `CPF_Net`
//! and functions with `FUNC_Net`. RepIndex = position in that list.

use crate::package_map::{PackageExports, PackageMap, UsesLine};
use openkf_package::class_fields::{read_class_info, read_enum_count, FieldInfo};
use openkf_package::reader::ObjectRef;
use openkf_package::MmapPackage;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const CPF_OPTIONAL_PARM: u32 = 0x10;
pub const CPF_PARM: u32 = 0x80;
pub const CPF_OUT_PARM: u32 = 0x100;
pub const CPF_RETURN_PARM: u32 = 0x400;

#[derive(Debug, Clone)]
pub enum NetType {
    Byte { bits: u32 },
    Int,
    Float,
    Bool,
    Name,
    /// Object reference. `actor`: referenced class derives from Actor.
    Object { actor: bool },
    Str,
    Vector,
    Rotator,
    /// Location (compressed vector), Pitch byte, Yaw byte, Velocity (compressed vector).
    CompressedPosition,
    /// 4 x int16.
    Plane,
    Struct { name: String, members: Vec<(String, u32, NetType)> },
    Unknown(String),
}

#[derive(Debug, Clone)]
pub struct NetField {
    pub name: String,
    pub owner: String,
    pub array_dim: u32,
    /// None for functions.
    pub ty: Option<NetType>,
    /// RPC parameters (name, flags, type) for functions.
    pub params: Vec<(String, u32, NetType)>,
}

#[derive(Debug, Clone)]
pub struct ClassNet {
    pub name: String,
    pub chain: Vec<String>,
    pub fields: Vec<NetField>,
}

impl ClassNet {
    pub fn is_a(&self, class_name: &str) -> bool {
        self.chain.iter().any(|c| c == class_name)
    }
}


pub struct GameSchema {
    pub map: PackageMap,
    pkgs: Vec<Option<MmapPackage>>,
    by_name: HashMap<String, usize>,
    /// Concatenated name tables (UPackageMap name indices).
    pub names: Vec<String>,
    class_cache: HashMap<(usize, usize), Arc<ClassNet>>,
}

impl GameSchema {
    pub fn load(kf_dir: &Path, uses: Vec<UsesLine>) -> Self {
        let mut pkgs = Vec::new();
        let mut infos = Vec::new();
        for u in &uses {
            let path: Option<PathBuf> = crate::download::find_package_by_guid(kf_dir, &u.fname, &u.guid);
            let m = path.and_then(|p| MmapPackage::open(p).ok());
            infos.push(m.as_ref().map(|m| {
                let p = m.as_package();
                let exports = p
                    .exports
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        let n = p.resolve_name(e.object_name).unwrap_or("?").to_string();
                        let c = if e.class_index == 0 { "Class".into() } else { openkf_package::export_class_name(&p, i).to_string() };
                        (n, c)
                    })
                    .collect();
                PackageExports { exports, generation_exports: p.summary.generations.iter().map(|g| g.export_count as u32).collect() }
            }));
            pkgs.push(m);
        }
        let mut it = infos.into_iter();
        let map = PackageMap::build(uses, |_| it.next().flatten());
        let mut names = Vec::new();
        for (i, p) in map.packages.iter().enumerate() {
            if let Some(m) = &pkgs[i] {
                let pk = m.as_package();
                let count = pk
                    .summary
                    .generations
                    .get(p.uses.gen.saturating_sub(1) as usize)
                    .map(|g| g.name_count as usize)
                    .unwrap_or(pk.names.len());
                for k in 0..count {
                    names.push(pk.names.get(k).unwrap_or("?").to_string());
                }
            }
        }
        let by_name = map.packages.iter().enumerate().map(|(i, p)| (p.uses.pkg.to_ascii_lowercase(), i)).collect();
        Self { map, pkgs, by_name, names, class_cache: HashMap::new() }
    }

    /// Global object index -> (package slot, export index).
    pub fn locate(&self, index: u32) -> Option<(usize, usize)> {
        let i = self.map.packages.iter().position(|p| index >= p.object_base && index < p.object_base + p.object_count)?;
        Some((i, (index - self.map.packages[i].object_base) as usize))
    }

    pub fn object_name(&self, index: u32) -> String {
        match self.map.resolve(index) {
            Some(o) => format!("{}.{}", o.package, o.name.unwrap_or("?")),
            None => format!("#{index}"),
        }
    }

    fn find_export(&self, pkg: &str, name: &str, outer: Option<&str>, class: &str) -> Option<(usize, usize)> {
        let slot = *self.by_name.get(&pkg.to_ascii_lowercase())?;
        let m = self.pkgs[slot].as_ref()?;
        let p = m.as_package();
        let idx = p.exports.entries.iter().enumerate().position(|(i, e)| {
            let cls = if e.class_index == 0 { "Class" } else { openkf_package::export_class_name(&p, i) };
            cls.eq_ignore_ascii_case(class)
                && p.resolve_name(e.object_name).ok().is_some_and(|n| n.eq_ignore_ascii_case(name))
                && match outer {
                    None => true,
                    Some(o) => e.package > 0
                        && p.exports.get((e.package - 1) as usize).and_then(|x| p.resolve_name(x.object_name).ok()).is_some_and(|n| n.eq_ignore_ascii_case(o)),
                }
        })?;
        Some((slot, idx))
    }

    /// Resolve a package-local object reference to (slot, export).
    fn resolve_ref(&self, slot: usize, r: i32) -> Option<(usize, usize)> {
        let m = self.pkgs[slot].as_ref()?;
        let p = m.as_package();
        match p.resolve_object_ref(r).ok()? {
            ObjectRef::Null => None,
            ObjectRef::Export(_) => Some((slot, (r - 1) as usize)),
            ObjectRef::Import(imp) => {
                let name = imp.resolve_object_name(&p.names)?;
                // Outer chain: collect names up to the root package.
                let mut chain = Vec::new();
                let mut o = imp.package;
                while o < 0 {
                    let x = p.imports.get((-o - 1) as usize)?;
                    chain.push(x.resolve_object_name(&p.names)?.to_string());
                    o = x.package;
                }
                let root = chain.last()?.clone();
                let outer = if chain.len() >= 2 { Some(chain[0].as_str()) } else { None };
                let class = imp.resolve_class_name(&p.names).unwrap_or("Class");
                self.find_export(&root, name, outer, class)
            }
        }
    }

    fn info(&self, slot: usize, export: usize) -> Option<openkf_package::class_fields::ClassInfo> {
        let m = self.pkgs[slot].as_ref()?;
        read_class_info(&m.as_package(), export).ok()
    }

    fn super_of(&self, slot: usize, export: usize) -> Option<(usize, usize)> {
        let m = self.pkgs[slot].as_ref()?;
        let p = m.as_package();
        let e = p.exports.get(export)?;
        let d = p.get_export_slice(e).ok()?;
        // Re-read the Super reference (first compact index after header).
        let mut o = 0usize;
        if e.object_flags & openkf_package::RF_HAS_STACK != 0 {
            let node = openkf_package::read_compact_index(d, &mut o).ok()?;
            openkf_package::read_compact_index(d, &mut o).ok()?;
            o += 12;
            if node != 0 {
                openkf_package::read_compact_index(d, &mut o).ok()?;
            }
        }
        let s = openkf_package::read_compact_index(d, &mut o).ok()?;
        self.resolve_ref(slot, s)
    }

    fn class_chain(&self, slot: usize, export: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = Some((slot, export));
        let mut guard = 0;
        while let Some((s, e)) = cur {
            guard += 1;
            if guard > 64 {
                break;
            }
            let p = self.pkgs[s].as_ref().unwrap().as_package();
            out.push(p.exports.get(e).and_then(|x| p.resolve_name(x.object_name).ok()).unwrap_or("?").to_string());
            cur = self.super_of(s, e);
        }
        out
    }

    fn net_type(&mut self, slot: usize, f: &FieldInfo, depth: u32) -> NetType {
        match f.kind.as_str() {
            "IntProperty" => NetType::Int,
            "FloatProperty" => NetType::Float,
            "BoolProperty" => NetType::Bool,
            "NameProperty" => NetType::Name,
            "StrProperty" => NetType::Str,
            "ClassProperty" => NetType::Object { actor: false },
            "ObjectProperty" => {
                let actor = self.resolve_ref(slot, f.inner_ref).map(|(s, e)| self.class_chain(s, e).iter().any(|c| c == "Actor")).unwrap_or(false);
                NetType::Object { actor }
            }
            "ByteProperty" => {
                let bits = self
                    .resolve_ref(slot, f.inner_ref)
                    .and_then(|(s, e)| read_enum_count(&self.pkgs[s].as_ref()?.as_package(), e).ok())
                    .map(|n| if n <= 1 { 0 } else { 32 - (n - 1).leading_zeros() })
                    .unwrap_or(8);
                NetType::Byte { bits }
            }
            "StructProperty" => {
                let sname = f.inner.rsplit('.').next().unwrap_or("").to_string();
                match sname.as_str() {
                    "Vector" => NetType::Vector,
                    "Rotator" => NetType::Rotator,
                    "CompressedPosition" => NetType::CompressedPosition,
                    "Plane" => NetType::Plane,
                    _ if depth < 4 => {
                        let Some((s, e)) = self.resolve_ref(slot, f.inner_ref) else { return NetType::Unknown(f.inner.clone()) };
                        let mut members = Vec::new();
                        for (ms, me) in self.struct_chain(s, e) {
                            let Some(si) = self.info(ms, me) else { continue };
                            for m in si.fields.iter().filter(|m| m.is_property()) {
                                let t = self.net_type(ms, m, depth + 1);
                                members.push((m.name.clone(), m.array_dim, t));
                            }
                        }
                        NetType::Struct { name: sname, members }
                    }
                    _ => NetType::Unknown(f.inner.clone()),
                }
            }
            k => NetType::Unknown(k.to_string()),
        }
    }

    /// Struct + its super structs, derived first (TFieldIterator order).
    fn struct_chain(&self, slot: usize, export: usize) -> Vec<(usize, usize)> {
        let mut v = vec![(slot, export)];
        let mut cur = self.super_of(slot, export);
        while let Some(x) = cur {
            if v.len() > 16 {
                break;
            }
            v.push(x);
            cur = self.super_of(x.0, x.1);
        }
        v
    }

    pub fn class_net(&mut self, slot: usize, export: usize) -> Option<Arc<ClassNet>> {
        if let Some(c) = self.class_cache.get(&(slot, export)) {
            return Some(c.clone());
        }
        let info = self.info(slot, export)?;
        let mut fields = match self.super_of(slot, export) {
            Some((s, e)) => self.class_net(s, e).map(|c| c.fields.clone()).unwrap_or_default(),
            None => Vec::new(),
        };
        // UClass::Link (Core.dll 0x10123440): CPF_Net properties and FUNC_Net
        // functions without super function, then sorted by LinkerIndex
        // (export index in the package; comparator at 0x10124280).
        let mut own: Vec<&FieldInfo> = info.fields.iter().filter(|f| f.is_net()).collect();
        own.sort_by_key(|f| f.export_index);
        for f in own {
            if f.is_property() {
                let ty = self.net_type(slot, f, 0);
                fields.push(NetField { name: f.name.clone(), owner: info.name.clone(), array_dim: f.array_dim, ty: Some(ty), params: vec![] });
            } else {
                // Skip overrides of an already-registered net function (GetSuperFunction).
                if fields.iter().any(|x| x.ty.is_none() && x.name == f.name) {
                    continue;
                }
                let mut params = Vec::new();
                if let Some(fi) = self.info(slot, f.export_index) {
                    for p in fi.fields.iter().filter(|p| p.is_property() && p.property_flags & CPF_PARM != 0 && p.property_flags & CPF_RETURN_PARM == 0) {
                        let t = self.net_type(slot, p, 0);
                        params.push((p.name.clone(), p.property_flags, t));
                    }
                }
                fields.push(NetField { name: f.name.clone(), owner: info.name.clone(), array_dim: 1, ty: None, params });
            }
        }
        let chain = self.class_chain(slot, export);
        let c = Arc::new(ClassNet { name: info.name, chain, fields });
        self.class_cache.insert((slot, export), c.clone());
        Some(c)
    }
}

impl GameSchema {
    /// For a global object index: Some(class) if the object *is* a class,
    /// else the class of the object (static level actor).
    pub fn class_of_object(&self, index: u32) -> Option<((usize, usize), bool)> {
        let (slot, e) = self.locate(index)?;
        let p = self.pkgs[slot].as_ref()?.as_package();
        let ex = p.exports.get(e)?;
        if ex.class_index == 0 {
            Some(((slot, e), true))
        } else {
            Some((self.resolve_ref(slot, ex.class_index)?, false))
        }
    }
}

impl GameSchema {
    /// Global network index of `Package.Object` (e.g. "KFMod.TrenchGun"), if the
    /// package is in the server's package map.
    pub fn object_index(&self, full: &str) -> Option<u32> {
        let (pkg, name) = full.split_once('.')?;
        let slot = *self.by_name.get(&pkg.to_ascii_lowercase())?;
        let p = self.pkgs[slot].as_ref()?.as_package();
        let e = p.exports.entries.iter().position(|e| e.class_index == 0 && p.resolve_name(e.object_name).ok().is_some_and(|n| n.eq_ignore_ascii_case(name)))?;
        Some(self.map.packages[slot].object_base + e as u32)
    }
}

impl GameSchema {
    /// Serialized bytes of the export behind a network object index.
    pub fn export_data(&self, index: u32) -> Option<Vec<u8>> {
        let (slot, e) = self.locate(index)?;
        let p = self.pkgs[slot].as_ref()?.as_package();
        p.get_export_data(e).ok().map(|d| d.to_vec())
    }
}

impl GameSchema {
    /// Members of a `SoundGroup` export (UE2 KF: tagged props "None", then
    /// compact count + compact object refs), as global network indices.
    /// Nested groups are flattened one level deep.
    pub fn sound_group_members(&self, index: u32) -> Vec<u32> {
        self.sound_group_members_depth(index, 0)
    }

    fn sound_group_members_depth(&self, index: u32, depth: u32) -> Vec<u32> {
        let Some((slot, e)) = self.locate(index) else { return Vec::new() };
        let Some(m) = self.pkgs[slot].as_ref() else { return Vec::new() };
        let p = m.as_package();
        // Only `SoundGroup` exports have the count + refs layout; anything else
        // (already a `Sound`, unknown class) yields no members.
        if openkf_package::export_class_name(&p, e) != "SoundGroup" {
            return Vec::new();
        }
        let Ok(d) = p.get_export_data(e) else { return Vec::new() };
        let mut o = 0usize;
        let mut out = Vec::new();
        let Ok(_none) = openkf_package::read_compact_index(d, &mut o) else { return out };
        let Ok(count) = openkf_package::read_compact_index(d, &mut o) else { return out };
        for _ in 0..count.clamp(0, 64) {
            let Ok(r) = openkf_package::read_compact_index(d, &mut o) else { break };
            let Some((s2, e2)) = self.resolve_ref(slot, r) else { continue };
            let idx = self.map.packages[s2].object_base + e2 as u32;
            if depth == 0 {
                let nested = self.pkgs[s2].as_ref().is_some_and(|m2| openkf_package::export_class_name(&m2.as_package(), e2) == "SoundGroup");
                if nested {
                    out.extend(self.sound_group_members_depth(idx, 1));
                    continue;
                }
            }
            out.push(idx);
        }
        out
    }
}

impl GameSchema {
    /// Global index of any export `Package.Name` (sounds, meshes...), first match.
    pub fn object_index_any(&self, full: &str) -> Option<u32> {
        let (pkg, name) = full.split_once('.')?;
        let name = name.rsplit('.').next().unwrap_or(name);
        let slot = *self.by_name.get(&pkg.to_ascii_lowercase())?;
        let p = self.pkgs[slot].as_ref()?.as_package();
        let e = p.exports.entries.iter().position(|e| p.resolve_name(e.object_name).ok().is_some_and(|n| n.eq_ignore_ascii_case(name)))?;
        Some(self.map.packages[slot].object_base + e as u32)
    }
}

impl GameSchema {
    /// Runs `f` on the level package (first package of the map, the `.rom`).
    pub fn with_level<R>(&self, f: impl FnOnce(&openkf_package::UnrealPackage<'_>) -> R) -> Option<R> {
        let m = self.pkgs.first()?.as_ref()?;
        Some(f(&m.as_package()))
    }
}
