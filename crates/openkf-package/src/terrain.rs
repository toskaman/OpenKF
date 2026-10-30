//! UE2 `TerrainInfo` decoding: G16 heightmap + texture layers -> world-space grid.
//!
//! World position of heightmap sample (x, y) with height h (0..65535):
//! `Location + TerrainScale * (x - W/2, y - H/2, (h - 32768) / 256)`.

use crate::error::PackageError;
use crate::level::{export_class_name, read_object_properties};
use crate::properties::{read_properties, PropValue};
use crate::reader::UnrealPackage;
use crate::texture::read_texture;

#[derive(Debug, Clone)]
pub struct TerrainLayer {
    /// Material object index (in the map package).
    pub texture: i32,
    /// Alpha (weight) map texture object index.
    pub alpha_map: i32,
    pub u_scale: f32,
    pub v_scale: f32,
}

#[derive(Debug, Clone)]
pub struct Terrain {
    pub actor: String,
    pub location: [f32; 3],
    pub scale: [f32; 3],
    pub width: usize,
    pub height: usize,
    /// Raw 16-bit heights, row-major (y * width + x).
    pub heights: Vec<u16>,
    pub layers: Vec<TerrainLayer>,
}

impl Terrain {
    /// UE world position of grid sample (x, y).
    pub fn world_pos(&self, x: usize, y: usize) -> [f32; 3] {
        let h = self.heights[y * self.width + x] as f32;
        [
            self.location[0] + self.scale[0] * (x as f32 - self.width as f32 / 2.0),
            self.location[1] + self.scale[1] * (y as f32 - self.height as f32 / 2.0),
            self.location[2] + self.scale[2] * (h - 32768.0) / 256.0,
        ]
    }

    /// Upward-ish normal from central differences (UE space).
    pub fn normal(&self, x: usize, y: usize) -> [f32; 3] {
        let x0 = x.saturating_sub(1);
        let x1 = (x + 1).min(self.width - 1);
        let y0 = y.saturating_sub(1);
        let y1 = (y + 1).min(self.height - 1);
        let (a, b) = (self.world_pos(x0, y), self.world_pos(x1, y));
        let (c, d) = (self.world_pos(x, y0), self.world_pos(x, y1));
        let dx = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let dy = [d[0] - c[0], d[1] - c[1], d[2] - c[2]];
        let n = [dx[1] * dy[2] - dx[2] * dy[1], dx[2] * dy[0] - dx[0] * dy[2], dx[0] * dy[1] - dx[1] * dy[0]];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
        let n = [n[0] / l, n[1] / l, n[2] / l];
        if n[2] < 0.0 { [-n[0], -n[1], -n[2]] } else { n }
    }
}

/// Reads every `TerrainInfo` actor of a map.
pub fn read_terrains(pkg: &UnrealPackage<'_>) -> Vec<Terrain> {
    let mut out = Vec::new();
    for i in 0..pkg.exports.len() {
        if export_class_name(pkg, i) != "TerrainInfo" {
            continue;
        }
        if let Ok(t) = read_terrain(pkg, i) {
            out.push(t);
        }
    }
    out
}

pub fn read_terrain(pkg: &UnrealPackage<'_>, export_index: usize) -> Result<Terrain, PackageError> {
    let props = read_object_properties(pkg, export_index)?;
    let vec = |name: &str, d: [f32; 3]| match props.get(name) {
        Some(PropValue::Vector(v)) => *v,
        _ => d,
    };
    let location = vec("Location", [0.0; 3]);
    let scale = vec("TerrainScale", [64.0, 64.0, 64.0]);
    let map = props.get_object("TerrainMap").unwrap_or(0);
    if map <= 0 {
        return Err(PackageError::ExportIndexOutOfBounds(0, 0));
    }
    let tex = read_texture(pkg, (map - 1) as usize)?;
    let m0 = tex.mips.first().ok_or(PackageError::ExportIndexOutOfBounds(0, 0))?;
    let (w, h) = (m0.width as usize, m0.height as usize);
    if m0.data.len() < w * h * 2 {
        return Err(PackageError::UnexpectedEof { offset: 0, requested: w * h * 2, total: m0.data.len() });
    }
    let heights = m0.data[..w * h * 2].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();

    let mut layers = Vec::new();
    for p in props.props.iter().filter(|p| p.name.eq_ignore_ascii_case("Layers")) {
        if let PropValue::Struct(_, raw) = &p.value {
            if let Ok(lp) = read_properties(pkg, raw, 0) {
                let f = |n: &str, d: f32| match lp.get(n) {
                    Some(PropValue::Float(v)) => *v,
                    _ => d,
                };
                let texture = lp.get_object("Texture").unwrap_or(0);
                if texture != 0 {
                    layers.push(TerrainLayer {
                        texture,
                        alpha_map: lp.get_object("AlphaMap").unwrap_or(0),
                        u_scale: f("UScale", 1.0),
                        v_scale: f("VScale", 1.0),
                    });
                }
            }
        }
    }
    Ok(Terrain {
        actor: pkg.format_full_name(export_index as i32 + 1),
        location,
        scale,
        width: w,
        height: h,
        heights,
        layers,
    })
}
