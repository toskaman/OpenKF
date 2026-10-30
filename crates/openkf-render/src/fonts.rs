//! Modern hardware-accelerated Font Engine & Canvas Text Rasterizer (`Pillar 08`).
//!
//! Provides text measurement, font atlas glyph lookup, kerning pair evaluation,
//! and dynamic quad batch generation matching Unreal Engine 2.5 `UCanvas::DrawText`
//! and `UCanvas::TextSize`.

use std::collections::HashMap;
use crate::error::RenderError;
use crate::hal::TextureHandle;
use crate::vertex::UIVertex;

/// Single glyph metrics within a texture atlas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphMetrics {
    /// Unicode codepoint.
    pub codepoint: char,
    /// Minimum UV coordinates in atlas `[u_min, v_min]`.
    pub uv_min: [f32; 2],
    /// Maximum UV coordinates in atlas `[u_max, v_max]`.
    pub uv_max: [f32; 2],
    /// Quad width in design pixels.
    pub width: f32,
    /// Quad height in design pixels.
    pub height: f32,
    /// Horizontal bearing offset from cursor.
    pub bearing_x: f32,
    /// Vertical bearing offset from line baseline/top.
    pub bearing_y: f32,
    /// Horizontal advance step after placing glyph.
    pub advance_x: f32,
    /// Texture page index for multi-page font packages.
    pub page_index: u32,
}

impl GlyphMetrics {
    #[inline]
    pub fn new(
        codepoint: char,
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        width: f32,
        height: f32,
        bearing_x: f32,
        bearing_y: f32,
        advance_x: f32,
        page_index: u32,
    ) -> Self {
        Self {
            codepoint,
            uv_min,
            uv_max,
            width,
            height,
            bearing_x,
            bearing_y,
            advance_x,
            page_index,
        }
    }
}

/// Binary character layout matching Unreal Engine 2.5 `FFontCharacter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct UE2FontCharacter {
    /// Horizontal pixel offset in texture.
    pub start_u: i32,
    /// Vertical pixel offset in texture.
    pub start_v: i32,
    /// Horizontal pixel span.
    pub u_size: i32,
    /// Vertical pixel span.
    pub v_size: i32,
    /// Texture page index in font's texture array.
    pub texture_index: u32,
}

impl UE2FontCharacter {
    pub const SIZE: usize = 20;

    #[inline]
    pub fn new(start_u: i32, start_v: i32, u_size: i32, v_size: i32, texture_index: u32) -> Self {
        Self {
            start_u,
            start_v,
            u_size,
            v_size,
            texture_index,
        }
    }

    /// Converts this UE2 character into normalized `GlyphMetrics`.
    pub fn to_glyph_metrics(
        &self,
        codepoint: char,
        tex_width: u32,
        tex_height: u32,
    ) -> GlyphMetrics {
        let tw = tex_width.max(1) as f32;
        let th = tex_height.max(1) as f32;

        let u_min = self.start_u as f32 / tw;
        let v_min = self.start_v as f32 / th;
        let u_max = (self.start_u + self.u_size) as f32 / tw;
        let v_max = (self.start_v + self.v_size) as f32 / th;

        let w = self.u_size as f32;
        let h = self.v_size as f32;

        GlyphMetrics {
            codepoint,
            uv_min: [u_min, v_min],
            uv_max: [u_max, v_max],
            width: w,
            height: h,
            bearing_x: 0.0,
            bearing_y: 0.0,
            advance_x: w,
            page_index: self.texture_index,
        }
    }
}

/// Kerning pair adjustment table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KerningTable {
    pairs: HashMap<(char, char), f32>,
}

impl KerningTable {
    pub fn new() -> Self {
        Self {
            pairs: HashMap::new(),
        }
    }

    /// Adds a kerning adjustment between two characters in design pixels.
    pub fn add_pair(&mut self, first: char, second: char, amount: f32) {
        self.pairs.insert((first, second), amount);
    }

    /// Returns the kerning advance adjustment between two characters.
    #[inline]
    pub fn get_kerning(&self, first: char, second: char) -> f32 {
        self.pairs.get(&(first, second)).copied().unwrap_or(0.0)
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    pub fn clear(&mut self) {
        self.pairs.clear();
    }
}

/// Font Atlas holding rasterized glyphs, kerning, and texture metadata.
#[derive(Clone, Debug)]
pub struct FontAtlas {
    pub name: String,
    pub texture_width: u32,
    pub texture_height: u32,
    pub line_height: f32,
    pub base_size: f32,
    pub glyphs: HashMap<char, GlyphMetrics>,
    pub kerning: KerningTable,
    pub page_textures: Vec<Option<TextureHandle>>,
    pub page_count: u32,
}

impl FontAtlas {
    /// Creates a new font atlas.
    pub fn new(
        name: impl Into<String>,
        texture_width: u32,
        texture_height: u32,
        line_height: f32,
        base_size: f32,
    ) -> Self {
        Self {
            name: name.into(),
            texture_width,
            texture_height,
            line_height: line_height.max(1.0),
            base_size: base_size.max(1.0),
            glyphs: HashMap::new(),
            kerning: KerningTable::new(),
            page_textures: vec![None],
            page_count: 1,
        }
    }

    /// Adds or updates a glyph metric.
    pub fn add_glyph(&mut self, glyph: GlyphMetrics) {
        if glyph.page_index >= self.page_count {
            self.page_count = glyph.page_index + 1;
            while self.page_textures.len() < self.page_count as usize {
                self.page_textures.push(None);
            }
        }
        self.glyphs.insert(glyph.codepoint, glyph);
    }

    /// Adds a kerning pair.
    pub fn add_kerning(&mut self, first: char, second: char, amount: f32) {
        self.kerning.add_pair(first, second, amount);
    }

    /// Retrieves glyph metrics for a character.
    #[inline]
    pub fn get_glyph(&self, ch: char) -> Option<&GlyphMetrics> {
        self.glyphs.get(&ch)
    }

    /// Retrieves glyph metrics with fallback (checks '?', then ' ', then any glyph).
    pub fn get_glyph_or_fallback(&self, ch: char) -> Option<&GlyphMetrics> {
        self.glyphs.get(&ch)
            .or_else(|| self.glyphs.get(&'?'))
            .or_else(|| self.glyphs.get(&' '))
            .or_else(|| self.glyphs.values().next())
    }

    /// Creates a `FontAtlas` from a slice of UE2 character structures.
    pub fn from_ue2_characters(
        name: impl Into<String>,
        characters: &[(char, UE2FontCharacter)],
        tex_width: u32,
        tex_height: u32,
        line_height: f32,
        base_size: f32,
    ) -> Self {
        let mut atlas = Self::new(name, tex_width, tex_height, line_height, base_size);
        for &(ch, ref char_def) in characters {
            let glyph = char_def.to_glyph_metrics(ch, tex_width, tex_height);
            atlas.add_glyph(glyph);
        }
        atlas
    }

    /// Builds a built-in default engine font atlas (ASCII 32..=126) for out-of-the-box UI rendering.
    pub fn default_engine_font() -> Self {
        let tex_size = 256u32;
        let cell_w = 16.0f32;
        let cell_h = 16.0f32;
        let line_height = 16.0f32;
        let base_size = 16.0f32;

        let mut atlas = Self::new("DefaultFont", tex_size, tex_size, line_height, base_size);

        // Populate ASCII 32 (' ') through 126 ('~') in a 16x16 grid
        for code in 32u8..=126u8 {
            let ch = code as char;
            let index = (code - 32) as u32;
            let col = index % 16;
            let row = index / 16;

            let u0 = (col as f32 * cell_w) / tex_size as f32;
            let v0 = (row as f32 * cell_h) / tex_size as f32;
            let u1 = ((col as f32 + 1.0) * cell_w) / tex_size as f32;
            let v1 = ((row as f32 + 1.0) * cell_h) / tex_size as f32;

            // Proportional advance adjustments for clean appearance
            let adv = match ch {
                '.' | ':' | '!' | '|' | '\'' | '`' | ';' => 6.0,
                'i' | 'l' | 'j' | '[' | ']' | '(' | ')' => 8.0,
                'f' | 'r' | 't' => 10.0,
                ' ' => 8.0,
                'm' | 'w' | 'M' | 'W' | '@' => 16.0,
                _ => 12.0,
            };

            let glyph = GlyphMetrics::new(
                ch,
                [u0, v0],
                [u1, v1],
                cell_w,
                cell_h,
                0.0,
                0.0,
                adv,
                0,
            );
            atlas.add_glyph(glyph);
        }

        // Add standard kerning pairs
        atlas.add_kerning('A', 'V', -2.0);
        atlas.add_kerning('V', 'A', -2.0);
        atlas.add_kerning('T', 'o', -1.5);
        atlas.add_kerning('T', 'e', -1.5);
        atlas.add_kerning('T', 'a', -1.5);
        atlas.add_kerning('W', 'a', -1.0);
        atlas.add_kerning('P', 'a', -1.0);
        atlas.add_kerning('L', 'T', -1.5);

        atlas
    }

    /// Generates a procedural 8-bit alpha texture bitmap (256x256) for the default font atlas.
    pub fn generate_default_alpha_bitmap(&self) -> Vec<u8> {
        let width = self.texture_width as usize;
        let height = self.texture_height as usize;
        let mut pixels = vec![0u8; width * height];

        for (&ch, glyph) in &self.glyphs {
            let px0 = (glyph.uv_min[0] * self.texture_width as f32) as usize;
            let py0 = (glyph.uv_min[1] * self.texture_height as f32) as usize;
            let gw = (glyph.width as usize).min(16);
            let gh = (glyph.height as usize).min(16);

            // Rasterize simple legible 8x12 glyph stroke patterns
            let pattern = get_glyph_pattern(ch);
            for y in 0..gh.min(12) {
                let row_bits = if y < pattern.len() { pattern[y] } else { 0 };
                for x in 0..gw.min(8) {
                    if (row_bits & (1 << (7 - x))) != 0 {
                        let tx = px0 + x + 2;
                        let ty = py0 + y + 2;
                        if tx < width && ty < height {
                            pixels[ty * width + tx] = 255;
                        }
                    }
                }
            }
        }

        pixels
    }
}

/// Helper 8-row bit patterns for ASCII glyph rasterization.
fn get_glyph_pattern(ch: char) -> &'static [u8] {
    match ch {
        'A' => &[0x18, 0x24, 0x42, 0x7E, 0x42, 0x42, 0x42, 0x00],
        'B' => &[0x7C, 0x42, 0x42, 0x7C, 0x42, 0x42, 0x7C, 0x00],
        'C' => &[0x3C, 0x42, 0x40, 0x40, 0x40, 0x42, 0x3C, 0x00],
        'D' => &[0x78, 0x44, 0x42, 0x42, 0x42, 0x44, 0x78, 0x00],
        'E' => &[0x7E, 0x40, 0x40, 0x78, 0x40, 0x40, 0x7E, 0x00],
        'F' => &[0x7E, 0x40, 0x40, 0x78, 0x40, 0x40, 0x40, 0x00],
        'G' => &[0x3C, 0x42, 0x40, 0x4E, 0x42, 0x42, 0x3C, 0x00],
        'H' => &[0x42, 0x42, 0x42, 0x7E, 0x42, 0x42, 0x42, 0x00],
        'I' => &[0x38, 0x10, 0x10, 0x10, 0x10, 0x10, 0x38, 0x00],
        'K' => &[0x44, 0x48, 0x50, 0x60, 0x50, 0x48, 0x44, 0x00],
        'L' => &[0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x7E, 0x00],
        'M' => &[0x82, 0xC6, 0xAA, 0x92, 0x82, 0x82, 0x82, 0x00],
        'N' => &[0x42, 0x62, 0x52, 0x4A, 0x46, 0x42, 0x42, 0x00],
        'O' => &[0x3C, 0x42, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00],
        'P' => &[0x7C, 0x42, 0x42, 0x7C, 0x40, 0x40, 0x40, 0x00],
        'R' => &[0x7C, 0x42, 0x42, 0x7C, 0x50, 0x48, 0x44, 0x00],
        'S' => &[0x3C, 0x42, 0x40, 0x3C, 0x02, 0x42, 0x3C, 0x00],
        'T' => &[0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        'U' => &[0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00],
        'V' => &[0x42, 0x42, 0x42, 0x42, 0x24, 0x24, 0x18, 0x00],
        '0' => &[0x3C, 0x46, 0x4A, 0x52, 0x62, 0x42, 0x3C, 0x00],
        '1' => &[0x18, 0x28, 0x08, 0x08, 0x08, 0x08, 0x3E, 0x00],
        '2' => &[0x3C, 0x42, 0x02, 0x0C, 0x30, 0x40, 0x7E, 0x00],
        '3' => &[0x3C, 0x42, 0x02, 0x1C, 0x02, 0x42, 0x3C, 0x00],
        '!' => &[0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00],
        '?' => &[0x3C, 0x42, 0x02, 0x0C, 0x10, 0x00, 0x10, 0x00],
        ':' => &[0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
        '.' => &[0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        '-' => &[0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        '+' => &[0x00, 0x18, 0x18, 0x7E, 0x18, 0x18, 0x00, 0x00],
        _ => &[0x7E, 0x42, 0x42, 0x42, 0x42, 0x42, 0x7E, 0x00], // Hollow box default
    }
}

/// A 2D Quad representing one rendered character glyph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextQuad {
    /// 4 corner vertices: [Top-Left, Top-Right, Bottom-Right, Bottom-Left].
    pub vertices: [UIVertex; 4],
    /// Texture page index (for multi-page font atlases).
    pub texture_page: u32,
    /// Rendered character.
    pub character: char,
}

impl TextQuad {
    #[inline]
    pub fn new(vertices: [UIVertex; 4], texture_page: u32, character: char) -> Self {
        Self {
            vertices,
            texture_page,
            character,
        }
    }

    /// Converts this quad into two triangles (6 vertices) ready for rasterization.
    /// Vertex layout: [0, 1, 2] and [0, 2, 3].
    #[inline]
    pub fn to_triangles(&self) -> [UIVertex; 6] {
        [
            self.vertices[0],
            self.vertices[1],
            self.vertices[2],
            self.vertices[0],
            self.vertices[2],
            self.vertices[3],
        ]
    }

    /// Returns the bounding box `(min_x, min_y, max_x, max_y)` of this quad.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let x0 = self.vertices[0].position[0];
        let y0 = self.vertices[0].position[1];
        let x1 = self.vertices[2].position[0];
        let y1 = self.vertices[2].position[1];
        (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1))
    }
}

/// Batch of text quads sharing a single texture page.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBatch {
    pub texture_page: u32,
    pub quads: Vec<TextQuad>,
    pub vertices: Vec<UIVertex>,
    pub indices: Vec<u16>,
}

impl TextBatch {
    pub fn from_quads(texture_page: u32, quads: Vec<TextQuad>) -> Self {
        let mut vertices = Vec::with_capacity(quads.len() * 4);
        let mut indices = Vec::with_capacity(quads.len() * 6);

        for (i, quad) in quads.iter().enumerate() {
            let base_idx = (i * 4) as u16;
            vertices.extend_from_slice(&quad.vertices);
            indices.push(base_idx);
            indices.push(base_idx + 1);
            indices.push(base_idx + 2);
            indices.push(base_idx);
            indices.push(base_idx + 2);
            indices.push(base_idx + 3);
        }

        Self {
            texture_page,
            quads,
            vertices,
            indices,
        }
    }

    #[inline]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    #[inline]
    pub fn index_count(&self) -> usize {
        self.indices.len()
    }
}

/// Canvas text rendering configuration matching Unreal Engine 2.5 `UCanvas`.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasTextSettings {
    /// Extra horizontal space added between characters (`UCanvas::SpaceX`).
    pub space_x: f32,
    /// Extra vertical line space added between lines (`UCanvas::SpaceY`).
    pub space_y: f32,
    /// Tab stop size measured in space character widths.
    pub tab_size: u32,
    /// Optional right-margin wrap width.
    pub wrap_width: Option<f32>,
    /// Optional scissor clipping rectangle `[min_x, min_y, max_x, max_y]`.
    pub clip_rect: Option<[f32; 4]>,
    /// Whether kerning table adjustments are applied.
    pub enable_kerning: bool,
}

impl Default for CanvasTextSettings {
    fn default() -> Self {
        Self {
            space_x: 0.0,
            space_y: 0.0,
            tab_size: 4,
            wrap_width: None,
            clip_rect: None,
            enable_kerning: true,
        }
    }
}

/// Modern font engine managing font atlases, text measurement, and dynamic quad generation.
#[derive(Clone, Debug)]
pub struct FontEngine {
    fonts: HashMap<String, FontAtlas>,
    active_font_name: String,
    pub settings: CanvasTextSettings,
}

impl Default for FontEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl FontEngine {
    /// Creates a new `FontEngine` preloaded with the default engine bitmap font.
    pub fn new() -> Self {
        let default_font = FontAtlas::default_engine_font();
        let default_name = default_font.name.clone();

        let mut fonts = HashMap::new();
        fonts.insert(default_name.clone(), default_font);

        Self {
            fonts,
            active_font_name: default_name,
            settings: CanvasTextSettings::default(),
        }
    }

    /// Registers a new font atlas.
    pub fn register_font(&mut self, font: FontAtlas) {
        self.fonts.insert(font.name.clone(), font);
    }

    /// Sets the active font by name.
    pub fn set_active_font(&mut self, name: &str) -> Result<(), RenderError> {
        if self.fonts.contains_key(name) {
            self.active_font_name = name.to_string();
            Ok(())
        } else {
            Err(RenderError::ResourceNotFound(format!("Font '{}' not registered", name)))
        }
    }

    /// Returns a reference to the active font atlas.
    #[inline]
    pub fn active_font(&self) -> &FontAtlas {
        self.fonts
            .get(&self.active_font_name)
            .expect("Active font must always exist in registered fonts")
    }

    /// Returns a mutable reference to the active font atlas.
    #[inline]
    pub fn active_font_mut(&mut self) -> &mut FontAtlas {
        self.fonts
            .get_mut(&self.active_font_name)
            .expect("Active font must always exist in registered fonts")
    }

    /// Looks up a registered font by name.
    #[inline]
    pub fn get_font(&self, name: &str) -> Option<&FontAtlas> {
        self.fonts.get(name)
    }

    /// Returns the list of registered font names.
    pub fn font_names(&self) -> Vec<&str> {
        self.fonts.keys().map(|s| s.as_str()).collect()
    }

    /// Measures the bounding width and height of a string using the active font.
    ///
    /// Matches Unreal Engine 2.5 `UCanvas::TextSize`.
    pub fn measure_text(&self, text: &str, font_size: f32) -> (f32, f32) {
        self.measure_text_ext(None, text, font_size, &self.settings)
    }

    /// Alias for `measure_text` matching UE2 naming convention (`UCanvas::GetTextSize`).
    #[inline]
    pub fn get_text_size(&self, text: &str, font_size: f32) -> (f32, f32) {
        self.measure_text(text, font_size)
    }

    /// Extended measurement supporting custom font selection and canvas settings.
    pub fn measure_text_ext(
        &self,
        font_name: Option<&str>,
        text: &str,
        font_size: f32,
        settings: &CanvasTextSettings,
    ) -> (f32, f32) {
        if text.is_empty() {
            return (0.0, 0.0);
        }

        let font = font_name
            .and_then(|n| self.get_font(n))
            .unwrap_or_else(|| self.active_font());

        let scale = if font.base_size > 0.0 {
            font_size / font.base_size
        } else {
            1.0
        };

        let space_advance = font
            .get_glyph(' ')
            .map(|g| g.advance_x)
            .unwrap_or(font.base_size * 0.5)
            * scale;
        let tab_step = space_advance * settings.tab_size as f32;

        let mut max_width: f32 = 0.0;
        let mut cur_line_width: f32 = 0.0;
        let mut line_count: usize = 1;

        let mut prev_char: Option<char> = None;

        for ch in text.chars() {
            match ch {
                '\r' => continue,
                '\n' => {
                    max_width = max_width.max(cur_line_width);
                    cur_line_width = 0.0;
                    line_count += 1;
                    prev_char = None;
                }
                '\t' => {
                    let next_tab = ((cur_line_width / tab_step.max(1.0)).floor() + 1.0) * tab_step;
                    cur_line_width = next_tab;
                    prev_char = None;
                }
                _ => {
                    if let Some(glyph) = font.get_glyph_or_fallback(ch) {
                        let mut adv = glyph.advance_x * scale + settings.space_x;

                        if settings.enable_kerning {
                            if let Some(prev) = prev_char {
                                adv += font.kerning.get_kerning(prev, ch) * scale;
                            }
                        }

                        // Check word/character wrap
                        if let Some(wrap_w) = settings.wrap_width {
                            if cur_line_width + adv > wrap_w && cur_line_width > 0.0 {
                                max_width = max_width.max(cur_line_width);
                                cur_line_width = 0.0;
                                line_count += 1;
                            }
                        }

                        cur_line_width += adv;
                        prev_char = Some(ch);
                    }
                }
            }
        }

        max_width = max_width.max(cur_line_width);
        let total_height = line_count as f32 * font.line_height * scale
            + ((line_count - 1) as f32 * settings.space_y).max(0.0);

        (max_width, total_height)
    }

    /// Generates dynamic UI quads for a string matching `UCanvas::DrawText`.
    ///
    /// Emits 4 vertices per glyph with normalized atlas UV coordinates and the specified color.
    pub fn generate_text_quads(
        &self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: [f32; 4],
    ) -> Vec<TextQuad> {
        self.generate_text_quads_ext(None, text, x, y, font_size, color, &self.settings)
    }

    /// Extended quad generation supporting font selection, wrapping, clipping, and canvas settings.
    pub fn generate_text_quads_ext(
        &self,
        font_name: Option<&str>,
        text: &str,
        start_x: f32,
        start_y: f32,
        font_size: f32,
        color: [f32; 4],
        settings: &CanvasTextSettings,
    ) -> Vec<TextQuad> {
        if text.is_empty() {
            return Vec::new();
        }

        let font = font_name
            .and_then(|n| self.get_font(n))
            .unwrap_or_else(|| self.active_font());

        let scale = if font.base_size > 0.0 {
            font_size / font.base_size
        } else {
            1.0
        };

        let line_step = font.line_height * scale + settings.space_y;
        let space_advance = font
            .get_glyph(' ')
            .map(|g| g.advance_x)
            .unwrap_or(font.base_size * 0.5)
            * scale;
        let tab_step = space_advance * settings.tab_size as f32;

        let mut quads = Vec::with_capacity(text.len());
        let mut cursor_x = start_x;
        let mut cursor_y = start_y;
        let mut prev_char: Option<char> = None;

        for ch in text.chars() {
            match ch {
                '\r' => continue,
                '\n' => {
                    cursor_x = start_x;
                    cursor_y += line_step;
                    prev_char = None;
                }
                '\t' => {
                    let rel_x = cursor_x - start_x;
                    let next_tab = ((rel_x / tab_step.max(1.0)).floor() + 1.0) * tab_step;
                    cursor_x = start_x + next_tab;
                    prev_char = None;
                }
                _ => {
                    if let Some(glyph) = font.get_glyph_or_fallback(ch) {
                        let mut kern = 0.0;
                        if settings.enable_kerning {
                            if let Some(prev) = prev_char {
                                kern = font.kerning.get_kerning(prev, ch) * scale;
                            }
                        }

                        let glyph_adv = glyph.advance_x * scale + settings.space_x + kern;

                        // Check wrapping threshold
                        if let Some(wrap_w) = settings.wrap_width {
                            if (cursor_x - start_x) + glyph_adv > wrap_w && cursor_x > start_x {
                                cursor_x = start_x;
                                cursor_y += line_step;
                                kern = 0.0;
                            }
                        }

                        let gx = cursor_x + kern + glyph.bearing_x * scale;
                        let gy = cursor_y + glyph.bearing_y * scale;
                        let gw = glyph.width * scale;
                        let gh = glyph.height * scale;

                        let x0 = gx;
                        let y0 = gy;
                        let x1 = gx + gw;
                        let y1 = gy + gh;

                        let u0 = glyph.uv_min[0];
                        let v0 = glyph.uv_min[1];
                        let u1 = glyph.uv_max[0];
                        let v1 = glyph.uv_max[1];

                        // Scissor clip test
                        let visible = if let Some(clip) = settings.clip_rect {
                            !(x1 < clip[0] || x0 > clip[2] || y1 < clip[1] || y0 > clip[3])
                        } else {
                            true
                        };

                        if visible && ch != ' ' {
                            let v_top_left = UIVertex::new([x0, y0], [u0, v0], color);
                            let v_top_right = UIVertex::new([x1, y0], [u1, v0], color);
                            let v_bot_right = UIVertex::new([x1, y1], [u1, v1], color);
                            let v_bot_left = UIVertex::new([x0, y1], [u0, v1], color);

                            quads.push(TextQuad::new(
                                [v_top_left, v_top_right, v_bot_right, v_bot_left],
                                glyph.page_index,
                                ch,
                            ));
                        }

                        cursor_x += glyph.advance_x * scale + settings.space_x + kern;
                        prev_char = Some(ch);
                    }
                }
            }
        }

        quads
    }

    /// Emits raw indexed UI vertices ready for immediate rasterizer draw calls.
    pub fn generate_text_vertices(
        &self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: [f32; 4],
    ) -> Vec<UIVertex> {
        let quads = self.generate_text_quads(text, x, y, font_size, color);
        let mut vertices = Vec::with_capacity(quads.len() * 6);
        for quad in quads {
            vertices.extend_from_slice(&quad.to_triangles());
        }
        vertices
    }

    /// Generates batched text grouped by texture atlas page for multi-page font rendering.
    pub fn generate_text_batches(
        &self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: [f32; 4],
    ) -> Vec<TextBatch> {
        let quads = self.generate_text_quads(text, x, y, font_size, color);
        let mut page_map: HashMap<u32, Vec<TextQuad>> = HashMap::new();

        for quad in quads {
            page_map.entry(quad.texture_page).or_default().push(quad);
        }

        let mut batches = Vec::new();
        for (page, page_quads) in page_map {
            batches.push(TextBatch::from_quads(page, page_quads));
        }

        // Sort by texture page index for deterministic draw ordering
        batches.sort_by_key(|b| b.texture_page);
        batches
    }

    /// Simulates `UCanvas::DrawText`, returning generated quads and final cursor position `(cur_x, cur_y)`.
    pub fn draw_text(
        &self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: [f32; 4],
    ) -> (Vec<TextQuad>, (f32, f32)) {
        let quads = self.generate_text_quads(text, x, y, font_size, color);
        let (w, h) = self.measure_text(text, font_size);
        (quads, (x + w, y + h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_engine_font_creation() {
        let engine = FontEngine::new();
        let font = engine.active_font();
        assert_eq!(font.name, "DefaultFont");
        assert_eq!(font.texture_width, 256);
        assert_eq!(font.texture_height, 256);
        assert_eq!(font.line_height, 16.0);
        assert_eq!(font.base_size, 16.0);

        // Verify ASCII printable glyphs exist
        assert!(font.get_glyph('A').is_some());
        assert!(font.get_glyph('z').is_some());
        assert!(font.get_glyph('0').is_some());
        assert!(font.get_glyph('!').is_some());
        assert!(font.get_glyph(' ').is_some());

        let glyph_a = font.get_glyph('A').unwrap();
        assert_eq!(glyph_a.codepoint, 'A');
        assert!(glyph_a.uv_max[0] > glyph_a.uv_min[0]);
        assert!(glyph_a.uv_max[1] > glyph_a.uv_min[1]);
        assert_eq!(glyph_a.page_index, 0);
    }

    #[test]
    fn test_measure_text_ascii_and_utf8() {
        let engine = FontEngine::new();

        // Empty string
        let (w_empty, h_empty) = engine.measure_text("", 16.0);
        assert_eq!(w_empty, 0.0);
        assert_eq!(h_empty, 0.0);

        // Single character 'A' (adv = 12.0)
        let (w_a, h_a) = engine.measure_text("A", 16.0);
        assert_eq!(w_a, 12.0);
        assert_eq!(h_a, 16.0);

        // Scaled to 32.0 (2x scale)
        let (w_a32, h_a32) = engine.measure_text("A", 32.0);
        assert_eq!(w_a32, 24.0);
        assert_eq!(h_a32, 32.0);

        // Multiline measurement
        let text = "Line1\nLine2\nLongerLine3";
        let (w_multi, h_multi) = engine.measure_text(text, 16.0);
        assert_eq!(h_multi, 16.0 * 3.0); // 3 lines
        let (w_line3, _) = engine.measure_text("LongerLine3", 16.0);
        assert_eq!(w_multi, w_line3);

        // Tab spacing
        let (w_tab, _) = engine.measure_text("\t", 16.0);
        assert!(w_tab > 0.0);
    }

    #[test]
    fn test_get_text_size_alias() {
        let engine = FontEngine::new();
        let s = "Killing Floor 2009 Retail";
        assert_eq!(engine.measure_text(s, 24.0), engine.get_text_size(s, 24.0));
    }

    #[test]
    fn test_kerning_pairs() {
        let mut engine = FontEngine::new();
        engine.settings.enable_kerning = false;
        let (w_no_kern, _) = engine.measure_text("AV", 16.0);

        engine.settings.enable_kerning = true;
        let (w_with_kern, _) = engine.measure_text("AV", 16.0);

        // 'A' and 'V' have kerning of -2.0
        assert_eq!(w_no_kern - w_with_kern, 2.0);
    }

    #[test]
    fn test_generate_text_quads() {
        let engine = FontEngine::new();
        let color = [1.0, 0.5, 0.0, 1.0];
        let quads = engine.generate_text_quads("KF", 100.0, 50.0, 16.0, color);

        assert_eq!(quads.len(), 2);
        let q0 = &quads[0];
        assert_eq!(q0.character, 'K');
        assert_eq!(q0.texture_page, 0);

        // Top-left vertex position
        assert_eq!(q0.vertices[0].position, [100.0, 50.0]);
        assert_eq!(q0.vertices[0].color, color);

        // Top-right vertex position
        assert_eq!(q0.vertices[1].position[0], 100.0 + 16.0);
        assert_eq!(q0.vertices[1].position[1], 50.0);

        // Bottom-right vertex position
        assert_eq!(q0.vertices[2].position, [100.0 + 16.0, 50.0 + 16.0]);

        // Bottom-left vertex position
        assert_eq!(q0.vertices[3].position, [100.0, 50.0 + 16.0]);

        // Check triangles conversion
        let tris = q0.to_triangles();
        assert_eq!(tris.len(), 6);
        assert_eq!(tris[0].position, q0.vertices[0].position);
        assert_eq!(tris[1].position, q0.vertices[1].position);
        assert_eq!(tris[2].position, q0.vertices[2].position);
        assert_eq!(tris[3].position, q0.vertices[0].position);
        assert_eq!(tris[4].position, q0.vertices[2].position);
        assert_eq!(tris[5].position, q0.vertices[3].position);
    }

    #[test]
    fn test_generate_text_vertices() {
        let engine = FontEngine::new();
        let color = [1.0, 1.0, 1.0, 1.0];
        let verts = engine.generate_text_vertices("Hello", 0.0, 0.0, 16.0, color);
        // 5 characters * 6 vertices per quad = 30 vertices
        assert_eq!(verts.len(), 30);
    }

    #[test]
    fn test_ue2_canvas_space_x_and_space_y() {
        let mut engine = FontEngine::new();
        let (w_base, h_base) = engine.measure_text("AB\nCD", 16.0);

        engine.settings.space_x = 4.0;
        engine.settings.space_y = 8.0;

        let (w_spaced, h_spaced) = engine.measure_text("AB\nCD", 16.0);
        // Each line has 2 characters -> 2 * 4.0 = 8.0 extra width
        assert_eq!(w_spaced - w_base, 8.0);
        // 2 lines -> 1 extra space_y between lines = 8.0
        assert_eq!(h_spaced - h_base, 8.0);
    }

    #[test]
    fn test_ue2_font_character_conversion() {
        let ue2_char = UE2FontCharacter::new(32, 64, 16, 24, 2);
        let glyph = ue2_char.to_glyph_metrics('X', 512, 512);

        assert_eq!(glyph.codepoint, 'X');
        assert_eq!(glyph.width, 16.0);
        assert_eq!(glyph.height, 24.0);
        assert_eq!(glyph.page_index, 2);
        assert_eq!(glyph.uv_min, [32.0 / 512.0, 64.0 / 512.0]);
        assert_eq!(glyph.uv_max, [48.0 / 512.0, 88.0 / 512.0]);
    }

    #[test]
    fn test_text_batches_multipage() {
        let mut atlas = FontAtlas::new("MultiPageFont", 256, 256, 16.0, 16.0);

        let g1 = GlyphMetrics::new('1', [0.0, 0.0], [0.1, 0.1], 10.0, 16.0, 0.0, 0.0, 10.0, 0);
        let g2 = GlyphMetrics::new('2', [0.0, 0.0], [0.1, 0.1], 10.0, 16.0, 0.0, 0.0, 10.0, 1);
        let g3 = GlyphMetrics::new('3', [0.0, 0.0], [0.1, 0.1], 10.0, 16.0, 0.0, 0.0, 10.0, 0);

        atlas.add_glyph(g1);
        atlas.add_glyph(g2);
        atlas.add_glyph(g3);

        let mut engine = FontEngine::new();
        engine.register_font(atlas);
        engine.set_active_font("MultiPageFont").unwrap();

        let batches = engine.generate_text_batches("123", 0.0, 0.0, 16.0, [1.0; 4]);
        assert_eq!(batches.len(), 2);

        // Page 0 has '1' and '3' (2 quads, 8 verts, 12 indices)
        assert_eq!(batches[0].texture_page, 0);
        assert_eq!(batches[0].quads.len(), 2);
        assert_eq!(batches[0].vertex_count(), 8);
        assert_eq!(batches[0].index_count(), 12);

        // Page 1 has '2' (1 quad, 4 verts, 6 indices)
        assert_eq!(batches[1].texture_page, 1);
        assert_eq!(batches[1].quads.len(), 1);
        assert_eq!(batches[1].vertex_count(), 4);
        assert_eq!(batches[1].index_count(), 6);
    }

    #[test]
    fn test_text_wrapping() {
        let mut engine = FontEngine::new();
        // Each 'A' has advance of 12.0
        // "AAAA" is 48.0 wide. If wrap_width is 30.0, it should wrap.
        engine.settings.wrap_width = Some(30.0);
        let (_, h) = engine.measure_text("AAAA", 16.0);
        assert!(h > 16.0); // Should be wrapped onto multiple lines
    }

    #[test]
    fn test_scissor_clipping() {
        let mut engine = FontEngine::new();
        // Clip bounds [0.0, 0.0, 50.0, 50.0]
        engine.settings.clip_rect = Some([0.0, 0.0, 50.0, 50.0]);

        // Place text outside at (200.0, 200.0)
        let quads = engine.generate_text_quads("Invisible", 200.0, 200.0, 16.0, [1.0; 4]);
        assert_eq!(quads.len(), 0);

        // Place text inside at (10.0, 10.0)
        let inside = engine.generate_text_quads("A", 10.0, 10.0, 16.0, [1.0; 4]);
        assert_eq!(inside.len(), 1);
    }

    #[test]
    fn test_procedural_bitmap_generation() {
        let atlas = FontAtlas::default_engine_font();
        let bitmap = atlas.generate_default_alpha_bitmap();
        assert_eq!(bitmap.len(), 256 * 256);
        // Ensure there are non-zero pixel values from rasterized glyphs
        let non_zero_count = bitmap.iter().filter(|&&b| b > 0).count();
        assert!(non_zero_count > 100);
    }

    #[test]
    fn test_draw_text_simulation() {
        let engine = FontEngine::new();
        let (quads, cursor) = engine.draw_text("Test", 10.0, 20.0, 16.0, [1.0; 4]);
        assert_eq!(quads.len(), 4);
        assert!(cursor.0 > 10.0);
        assert_eq!(cursor.1, 20.0 + 16.0);
    }
}
