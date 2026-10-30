//! Asset library: opens retail packages on demand, resolves UE2 materials to
//! textures and decodes them into GPU-ready mip chains.

use openkf_package::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A parsed package whose bytes live for the whole process (viewer lifetime).
pub type Pkg = UnrealPackage<'static>;

pub struct AssetLibrary {
    pub kf_dir: PathBuf,
    packages: HashMap<String, Option<&'static Pkg>>,
}

/// GPU texture payload.
#[derive(Clone)]
pub enum TexFormat {
    Bc1,
    Bc2,
    Bc3,
    Rgba8,
}

#[derive(Clone)]
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub format: TexFormat,
    /// Mip chain, level 0 first. BC data is in 4x4 blocks.
    pub mips: Vec<Vec<u8>>,
    /// Alpha-test this material (masked / alpha textures).
    pub alpha_test: bool,
    /// Source was single-channel (L8/G8): use red as the weight for terrain alpha maps.
    pub luminance: bool,
}

/// How a material is composited (from FinalBlend / Shader / texture flags / BSP flags).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Blend {
    Opaque,
    Masked,
    Alpha,
    Additive,
    Modulate,
    Invisible,
}

#[derive(Clone, Copy, Debug)]
pub struct MatInfo {
    pub tex: TexRef,
    pub blend: Blend,
}

/// Where a texture lives: package + export index.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TexRef {
    pub pkg: *const Pkg,
    pub export: usize,
}
unsafe impl Send for TexRef {}
unsafe impl Sync for TexRef {}

impl TexRef {
    pub fn pkg(&self) -> &'static Pkg {
        unsafe { &*self.pkg }
    }
}

const SEARCH: &[(&str, &str)] = &[
    ("Textures", "utx"),
    ("StaticMeshes", "usx"),
    ("Maps", "rom"),
    ("System", "u"),
    ("Animations", "ukx"),
];

impl AssetLibrary {
    pub fn new(kf_dir: impl Into<PathBuf>) -> Self {
        Self { kf_dir: kf_dir.into(), packages: HashMap::new() }
    }

    /// Registers an already-loaded package (e.g. the map itself) under a name.
    pub fn insert_bytes(&mut self, name: &str, bytes: Vec<u8>) -> Option<&'static Pkg> {
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let pkg = UnrealPackage::from_bytes(bytes).ok().map(|p| &*Box::leak(Box::new(p)));
        self.packages.insert(name.to_ascii_lowercase(), pkg);
        pkg
    }

    pub fn load_file(&mut self, name: &str, path: &Path) -> Option<&'static Pkg> {
        let bytes = std::fs::read(path).ok()?;
        self.insert_bytes(name, bytes)
    }

    /// Finds and opens a package by name in the standard KF folders.
    pub fn package(&mut self, name: &str) -> Option<&'static Pkg> {
        let key = name.to_ascii_lowercase();
        if let Some(p) = self.packages.get(&key) {
            return *p;
        }
        for (dir, ext) in SEARCH {
            let p = self.kf_dir.join(dir).join(format!("{name}.{ext}"));
            if p.exists() {
                return self.load_file(name, &p);
            }
        }
        // Retail download cache (Cache.ini), then packages downloaded by OpenKF.
        for ext in ["utx", "usx", "rom", "u", "ukx", "uax"] {
            if let Some(p) = openkf_network::download::retail_cache(&self.kf_dir).get(&format!("{name}.{ext}").to_ascii_lowercase()).cloned() {
                return self.load_file(name, &p);
            }
        }
        // Packages downloaded from servers (FastDL), any extension.
        let dl = openkf_network::download::download_dir(&self.kf_dir);
        for ext in ["utx", "usx", "rom", "u", "ukx", "uax"] {
            let p = dl.join(format!("{name}.{ext}"));
            if p.is_file() {
                return self.load_file(name, &p);
            }
        }
        self.packages.insert(key, None);
        None
    }

    /// Resolves an object reference of `pkg` to (package, export index).
    pub fn resolve(&mut self, pkg: &'static Pkg, obj: i32) -> Option<(&'static Pkg, usize)> {
        if obj > 0 {
            return Some((pkg, (obj - 1) as usize));
        }
        if obj == 0 {
            return None;
        }
        let full = pkg.format_full_name(obj);
        let (top, rest) = full.split_once('.')?;
        let target = self.package(top)?;
        let idx = target.exports.entries.iter().enumerate().find_map(|(i, _)| {
            target.format_full_name(i as i32 + 1).eq_ignore_ascii_case(rest).then_some(i)
        })?;
        Some((target, idx))
    }

    /// Follows UE2 material wrappers (Shader, FinalBlend, Combiner, TexModifiers...)
    /// down to a `Texture` export.
    pub fn material_texture(&mut self, pkg: &'static Pkg, obj: i32) -> Option<MatInfo> {
        let r = self.material_texture_inner(pkg, obj);
        if r.is_none() && obj != 0 && std::env::var_os("OPENKF_DEBUG").is_some() {
            let cls = self.resolve(pkg, obj).map(|(p, e)| export_class_name(p, e).to_string()).unwrap_or_else(|| "<unresolved>".into());
            eprintln!("[material] no texture for {} ({cls})", pkg.format_full_name(obj));
        }
        r
    }

    fn material_texture_inner(&mut self, pkg: &'static Pkg, obj: i32) -> Option<MatInfo> {
        let (mut p, mut e) = self.resolve(pkg, obj)?;
        let mut blend = Blend::Opaque;
        for _ in 0..8 {
            let class = export_class_name(p, e).to_string();
            if class == "Texture" {
                if blend == Blend::Opaque {
                    if let Ok(tp) = read_object_properties(p, e) {
                        if tp.get_bool("bMasked").unwrap_or(false) {
                            blend = Blend::Masked;
                        } else if tp.get_bool("bAlphaTexture").unwrap_or(false) {
                            blend = Blend::Alpha;
                        }
                    }
                }
                return Some(MatInfo { tex: TexRef { pkg: p as *const Pkg, export: e }, blend });
            }
            let props = read_object_properties(p, e).ok()?;
            // Outermost non-default blend wins.
            if blend == Blend::Opaque {
                if class == "FinalBlend" {
                    blend = match props.get_byte("FrameBufferBlending").unwrap_or(0) {
                        1 | 3 => Blend::Modulate,
                        2 => Blend::Alpha,
                        4 | 6 => Blend::Additive,
                        5 => Blend::Modulate,
                        7 => Blend::Invisible,
                        _ => if props.get_bool("AlphaTest").unwrap_or(false) { Blend::Masked } else { Blend::Opaque },
                    };
                } else if class == "Shader" {
                    blend = match props.get_byte("OutputBlending").unwrap_or(0) {
                        1 => Blend::Masked,
                        2 | 6 => Blend::Modulate,
                        3 | 5 => Blend::Additive,
                        4 => Blend::Invisible,
                        _ => if props.get_object("Opacity").unwrap_or(0) != 0 { Blend::Alpha } else { Blend::Opaque },
                    };
                }
            }
            let next = ["Diffuse", "Material", "Material1", "FallbackMaterial", "SelfIllumination", "Material2"]
                .iter()
                .find_map(|k| props.get_object(k).filter(|&o| o != 0))?;
            let (np, ne) = self.resolve(p, next)?;
            p = np;
            e = ne;
        }
        None
    }
}

/// Texture size from properties (cheap, no decode).
pub fn texture_size(t: TexRef) -> (u32, u32) {
    match read_object_properties(t.pkg(), t.export) {
        Ok(p) => (
            p.get_int("USize").unwrap_or(256).max(1) as u32,
            p.get_int("VSize").unwrap_or(256).max(1) as u32,
        ),
        Err(_) => (256, 256),
    }
}

/// Decodes a texture into GPU form. BC data is passed through when `bc_supported`.
pub fn decode_texture(t: TexRef, bc_supported: bool) -> Option<TextureData> {
    let tex = read_texture(t.pkg(), t.export).ok()?;
    let m0 = tex.mips.first()?;
    let (w, h) = (m0.width.max(1), m0.height.max(1));
    let alpha_test = tex.masked || tex.alpha_texture
        || matches!(tex.format, TextureFormat::Dxt3 | TextureFormat::Dxt5) && tex.alpha_texture;
    let bc = match tex.format {
        TextureFormat::Dxt1 => Some((TexFormat::Bc1, 8usize)),
        TextureFormat::Dxt3 => Some((TexFormat::Bc2, 16)),
        TextureFormat::Dxt5 => Some((TexFormat::Bc3, 16)),
        _ => None,
    };
    if let (true, Some((fmt, bs))) = (bc_supported && w % 4 == 0 && h % 4 == 0, bc) {
        // Pass-through compressed mips while they match the expected chain.
        let mut mips = Vec::new();
        for (i, m) in tex.mips.iter().enumerate() {
            let (ew, eh) = ((w >> i).max(1), (h >> i).max(1));
            let need = (ew.div_ceil(4) * eh.div_ceil(4)) as usize * bs;
            if m.width != ew || m.height != eh || m.data.len() < need {
                break;
            }
            mips.push(m.data[..need].to_vec());
        }
        if !mips.is_empty() {
            // DXT1 punch-through alpha only matters for masked textures.
            let alpha_test = match fmt { TexFormat::Bc1 => tex.masked, _ => alpha_test };
            return Some(TextureData { width: w, height: h, format: fmt, mips, alpha_test, luminance: false });
        }
    }
    // CPU decode path (P8, RGBA8, L8, or BC without support).
    let mut mips = Vec::new();
    for i in 0..tex.mips.len() {
        let (ew, eh) = ((w >> i).max(1), (h >> i).max(1));
        match tex.decode_rgba(i) {
            Some((mw, mh, rgba)) if mw == ew && mh == eh => mips.push(rgba),
            _ => break,
        }
    }
    if mips.is_empty() {
        return None;
    }
    let luminance = matches!(tex.format, TextureFormat::L8 | TextureFormat::G16);
    Some(TextureData { width: w, height: h, format: TexFormat::Rgba8, mips, alpha_test, luminance })
}
