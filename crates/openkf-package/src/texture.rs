//! UE2 `UTexture` / `UPalette` decoding (package v128, Killing Floor v1065).
//!
//! Native layout after the tagged properties:
//! `compact MipCount` then per mip:
//! `i32 SkipPos (absolute)`, `compact DataLen`, `DataLen bytes`,
//! `i32 USize`, `i32 VSize`, `u8 UBits`, `u8 VBits`.

use crate::compact_index::read_compact_index;
use crate::error::PackageError;
use crate::properties::{read_properties, PropertyList};
use crate::reader::{ObjectRef, UnrealPackage};

/// `ETextureFormat` from Engine/Classes/BitmapMaterial.uc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    P8,
    Rgba7,
    Rgb16,
    Dxt1,
    Rgb8,
    Rgba8,
    NoData,
    Dxt3,
    Dxt5,
    L8,
    G16,
    RrrGggBbb,
    Unknown(u8),
}

impl TextureFormat {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::P8,
            1 => Self::Rgba7,
            2 => Self::Rgb16,
            3 => Self::Dxt1,
            4 => Self::Rgb8,
            5 => Self::Rgba8,
            6 => Self::NoData,
            7 => Self::Dxt3,
            8 => Self::Dxt5,
            9 => Self::L8,
            10 => Self::G16,
            11 => Self::RrrGggBbb,
            o => Self::Unknown(o),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Mip<'a> {
    pub width: u32,
    pub height: u32,
    /// Raw (still compressed / paletted) data, borrowed from the package.
    pub data: &'a [u8],
}

#[derive(Debug, Clone)]
pub struct Texture<'a> {
    pub name: String,
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
    pub mips: Vec<Mip<'a>>,
    /// RGBA palette for P8 textures.
    pub palette: Option<Vec<[u8; 4]>>,
    pub masked: bool,
    pub alpha_texture: bool,
    pub props: PropertyList<'a>,
}

fn rd_i32(d: &[u8], o: &mut usize) -> Result<i32, PackageError> {
    if *o + 4 > d.len() {
        return Err(PackageError::UnexpectedEof { offset: *o, requested: 4, total: d.len() });
    }
    let v = i32::from_le_bytes(d[*o..*o + 4].try_into().unwrap());
    *o += 4;
    Ok(v)
}

/// Reads a `UPalette` export: properties then `TArray<FColor>` (R,G,B,A).
pub fn read_palette(pkg: &UnrealPackage<'_>, export_index: usize) -> Result<Vec<[u8; 4]>, PackageError> {
    let d = pkg.get_export_data(export_index)?;
    let pl = read_properties(pkg, d, 0)?;
    let mut o = pl.end_offset;
    let n = read_compact_index(d, &mut o)? as usize;
    if o + n * 4 > d.len() {
        return Err(PackageError::UnexpectedEof { offset: o, requested: n * 4, total: d.len() });
    }
    Ok(d[o..o + n * 4].chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect())
}

/// Reads a `UTexture` export (0-based export index).
pub fn read_texture<'a>(pkg: &UnrealPackage<'a>, export_index: usize) -> Result<Texture<'a>, PackageError> {
    let export = pkg
        .exports
        .get(export_index)
        .ok_or(PackageError::ExportIndexOutOfBounds(export_index, pkg.exports.len()))?;
    let base = export.serial_offset as usize;
    let d: &'a [u8] = pkg.get_export_slice(export)?;
    let props = read_properties(pkg, d, 0)?;
    let format = TextureFormat::from_byte(props.get_byte("Format").unwrap_or(0));

    let mut o = props.end_offset;
    let raw_mip = read_compact_index(d, &mut o)?.max(0) as usize;
    // Retail max is ~13 (4096), cap to 32 to prevent OOM on malicious packages
    let mip_count = raw_mip.min(32);
    let mut mips = Vec::with_capacity(mip_count);
    for _ in 0..mip_count {
        let _skip_pos = rd_i32(d, &mut o)?;
        let len = read_compact_index(d, &mut o)?.max(0) as usize;
        if o + len > d.len() {
            return Err(PackageError::PayloadOutOfBounds { offset: base + o, size: len, total: base + d.len() });
        }
        let data = &d[o..o + len];
        o += len;
        let w = rd_i32(d, &mut o)?.max(0) as u32;
        let h = rd_i32(d, &mut o)?.max(0) as u32;
        o += 2; // UBits, VBits
        mips.push(Mip { width: w, height: h, data });
    }

    let palette = match props.get_object("Palette") {
        Some(idx) if idx > 0 => read_palette(pkg, (idx - 1) as usize).ok(),
        _ => None,
    };
    let name = pkg.format_full_name(export_index as i32 + 1);
    let width = props.get_int("USize").map(|v| v as u32).or(mips.first().map(|m| m.width)).unwrap_or(0);
    let height = props.get_int("VSize").map(|v| v as u32).or(mips.first().map(|m| m.height)).unwrap_or(0);
    Ok(Texture {
        name,
        format,
        width,
        height,
        mips,
        palette,
        masked: props.get_bool("bMasked").unwrap_or(false),
        alpha_texture: props.get_bool("bAlphaTexture").unwrap_or(false),
        props,
    })
}

/// Lists 0-based export indices whose class is `Texture` (Engine.Texture).
pub fn texture_exports(pkg: &UnrealPackage<'_>) -> Vec<usize> {
    pkg.exports
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| match pkg.resolve_object_ref(e.class_index) {
            Ok(ObjectRef::Import(imp)) => imp.resolve_object_name(&pkg.names) == Some("Texture"),
            _ => false,
        })
        .map(|(i, _)| i)
        .collect()
}

// ---------------------------------------------------------------- decoding

fn rgb565(c: u16) -> [u8; 4] {
    let r = ((c >> 11) & 31) as u32;
    let g = ((c >> 5) & 63) as u32;
    let b = (c & 31) as u32;
    [((r * 527 + 23) >> 6) as u8, ((g * 259 + 33) >> 6) as u8, ((b * 527 + 23) >> 6) as u8, 255]
}

fn color_block(b: &[u8], out: &mut [[u8; 4]; 16], dxt1: bool) {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let p0 = rgb565(c0);
    let p1 = rgb565(c1);
    let mix = |a: u8, b: u8, wa: u32, wb: u32| ((a as u32 * wa + b as u32 * wb) / (wa + wb)) as u8;
    let mut pal = [p0, p1, [0; 4], [0; 4]];
    if c0 > c1 || !dxt1 {
        for k in 0..3 {
            pal[2][k] = mix(p0[k], p1[k], 2, 1);
            pal[3][k] = mix(p0[k], p1[k], 1, 2);
        }
        pal[2][3] = 255;
        pal[3][3] = 255;
    } else {
        for k in 0..3 {
            pal[2][k] = mix(p0[k], p1[k], 1, 1);
        }
        pal[2][3] = 255;
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    for i in 0..16 {
        out[i] = pal[((bits >> (2 * i)) & 3) as usize];
    }
}

fn decode_bc(data: &[u8], w: u32, h: u32, fmt: TextureFormat) -> Option<Vec<u8>> {
    let bs = if fmt == TextureFormat::Dxt1 { 8 } else { 16 };
    let bw = w.div_ceil(4) as usize;
    let bh = h.div_ceil(4) as usize;
    if data.len() < bw * bh * bs {
        return None;
    }
    let mut out = vec![0u8; (w * h * 4) as usize];
    let mut px = [[0u8; 4]; 16];
    for by in 0..bh {
        for bx in 0..bw {
            let blk = &data[(by * bw + bx) * bs..][..bs];
            let (alpha, col) = if bs == 16 { (&blk[..8], &blk[8..]) } else { (&blk[..0], blk) };
            color_block(col, &mut px, fmt == TextureFormat::Dxt1);
            if fmt == TextureFormat::Dxt3 {
                for i in 0..16 {
                    let nib = (alpha[i / 2] >> ((i & 1) * 4)) & 15;
                    px[i][3] = nib * 17;
                }
            } else if fmt == TextureFormat::Dxt5 {
                let a0 = alpha[0] as u32;
                let a1 = alpha[1] as u32;
                let mut ap = [a0, a1, 0, 0, 0, 0, 0, 0];
                if a0 > a1 {
                    for k in 1..7 {
                        ap[k + 1] = ((7 - k as u32) * a0 + k as u32 * a1) / 7;
                    }
                } else {
                    for k in 1..5 {
                        ap[k + 1] = ((5 - k as u32) * a0 + k as u32 * a1) / 5;
                    }
                    ap[6] = 0;
                    ap[7] = 255;
                }
                let mut bits = 0u64;
                for k in 0..6 {
                    bits |= (alpha[2 + k] as u64) << (8 * k);
                }
                for i in 0..16 {
                    px[i][3] = ap[((bits >> (3 * i)) & 7) as usize] as u8;
                }
            }
            for i in 0..16 {
                let x = bx * 4 + (i % 4);
                let y = by * 4 + (i / 4);
                if x < w as usize && y < h as usize {
                    out[(y * w as usize + x) * 4..][..4].copy_from_slice(&px[i]);
                }
            }
        }
    }
    Some(out)
}

/// Decodes a raw DXT1 (`dxt5 = false`) or DXT5 surface to RGBA8.
pub fn decode_bc_rgba(data: &[u8], w: u32, h: u32, dxt5: bool) -> Vec<u8> {
    let fmt = if dxt5 { TextureFormat::Dxt5 } else { TextureFormat::Dxt1 };
    decode_bc(data, w, h, fmt).unwrap_or_else(|| {
        let capped = (w.min(4096) as usize).checked_mul(h.min(4096) as usize).unwrap_or(0).checked_mul(4).unwrap_or(0).min(64 * 1024 * 1024);
        vec![128; capped]
    })
}

impl Texture<'_> {
    /// Decodes a mip level to tightly packed RGBA8. Returns `None` for unsupported formats.
    pub fn decode_rgba(&self, mip: usize) -> Option<(u32, u32, Vec<u8>)> {
        let m = self.mips.get(mip)?;
        let (w, h) = (m.width, m.height);
        let n = (w * h) as usize;
        let rgba = match self.format {
            TextureFormat::Dxt1 => {
                let mut px = decode_bc(m.data, w, h, self.format)?;
                // UE2 only alpha-tests DXT1 when the texture is masked/alpha;
                // otherwise the 1-bit "transparent" pixels render opaque.
                if !self.masked && !self.alpha_texture {
                    px.chunks_exact_mut(4).for_each(|c| c[3] = 255);
                }
                px
            }
            TextureFormat::Dxt3 | TextureFormat::Dxt5 => decode_bc(m.data, w, h, self.format)?,
            TextureFormat::P8 => {
                let pal = self.palette.as_ref()?;
                if m.data.len() < n {
                    return None;
                }
                let mut out = Vec::with_capacity(n * 4);
                for &i in &m.data[..n] {
                    let mut c = pal.get(i as usize).copied().unwrap_or([0, 0, 0, 255]);
                    if !self.alpha_texture {
                        c[3] = if self.masked && i == 0 { 0 } else { 255 };
                    }
                    out.extend_from_slice(&c);
                }
                out
            }
            TextureFormat::Rgba8 => {
                // Stored as BGRA.
                if m.data.len() < n * 4 {
                    return None;
                }
                m.data[..n * 4].chunks_exact(4).flat_map(|c| [c[2], c[1], c[0], c[3]]).collect()
            }
            TextureFormat::L8 => {
                if m.data.len() < n {
                    return None;
                }
                m.data[..n].iter().flat_map(|&l| [l, l, l, 255]).collect()
            }
            _ => return None,
        };
        Some((w, h, rgba))
    }
}
