//! UE2 `UPackageMap`: network object indices.
//!
//! The server lists its packages with `USES` (control channel), in order. Each
//! package contributes `ObjectCount` = export count of the generation `GEN`
//! named in its `USES` line. An object travels as
//! `SerializeInt(Index, MaxObjectIndex)` where `Index = ObjectBase + ExportIndex`
//! and `MaxObjectIndex` = sum of all ObjectCounts. Index 0 is reserved for None?
//! (see [`PackageMap::resolve`]; validated against captures).

#[derive(Debug, Clone)]
pub struct UsesLine {
    pub guid: String,
    pub pkg: String,
    pub flags: u32,
    pub size: u64,
    pub gen: u32,
    pub fname: String,
}

impl UsesLine {
    pub fn parse(msg: &str) -> Option<Self> {
        let rest = msg.strip_prefix("USES ")?;
        let get = |k: &str| {
            rest.split(' ')
                .find_map(|kv| kv.strip_prefix(k).and_then(|v| v.strip_prefix('=')))
                .map(str::to_string)
        };
        Some(Self {
            guid: get("GUID")?,
            pkg: get("PKG")?,
            flags: get("FLAGS")?.parse().ok()?,
            size: get("SIZE")?.parse().ok()?,
            gen: get("GEN")?.parse().ok()?,
            fname: get("FNAME").unwrap_or_default(),
        })
    }
}

/// Local description of one package's exports (filled from the game files).
#[derive(Debug, Clone, Default)]
pub struct PackageExports {
    /// (object name, class name) for each export, in export-table order.
    pub exports: Vec<(String, String)>,
    /// export count per generation (from the package summary).
    pub generation_exports: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct PackageEntry {
    pub uses: UsesLine,
    pub object_base: u32,
    pub object_count: u32,
    pub local: Option<PackageExports>,
}

#[derive(Debug, Clone, Default)]
pub struct PackageMap {
    pub packages: Vec<PackageEntry>,
    pub max_object_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetObject<'a> {
    pub package: &'a str,
    pub export_index: u32,
    pub name: Option<&'a str>,
    pub class: Option<&'a str>,
}

impl PackageMap {
    pub fn build(uses: Vec<UsesLine>, mut lookup: impl FnMut(&UsesLine) -> Option<PackageExports>) -> Self {
        let mut base = 0u32;
        let mut packages = Vec::with_capacity(uses.len());
        for u in uses {
            let local = lookup(&u);
            let count = local
                .as_ref()
                .map(|l| {
                    l.generation_exports
                        .get(u.gen.saturating_sub(1) as usize)
                        .copied()
                        .unwrap_or(l.exports.len() as u32)
                })
                .unwrap_or(0);
            packages.push(PackageEntry { uses: u, object_base: base, object_count: count, local });
            base += count;
        }
        Self { packages, max_object_index: base }
    }

    pub fn resolve(&self, index: u32) -> Option<NetObject<'_>> {
        let p = self
            .packages
            .iter()
            .find(|p| index >= p.object_base && index < p.object_base + p.object_count)?;
        let ei = index - p.object_base;
        let e = p.local.as_ref().and_then(|l| l.exports.get(ei as usize));
        Some(NetObject {
            package: &p.uses.pkg,
            export_index: ei,
            name: e.map(|e| e.0.as_str()),
            class: e.map(|e| e.1.as_str()),
        })
    }
}
