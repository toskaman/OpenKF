use crate::vertex::UIVertex;
use std::f32::consts::PI;

/// Perk / specialization type matching Killing Floor retail perks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PerkType {
    Berserker,
    Commando,
    SupportSpecialist,
    FieldMedic,
    Demolitions,
    Firebug,
    Sharpshooter,
}

impl PerkType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Berserker => "BERSERKER",
            Self::Commando => "COMMANDO",
            Self::SupportSpecialist => "SUPPORT",
            Self::FieldMedic => "MEDIC",
            Self::Demolitions => "DEMO",
            Self::Firebug => "FIREBUG",
            Self::Sharpshooter => "SHARPSHOOTER",
        }
    }
}

/// Weapon firing mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FireMode {
    #[default]
    Single,
    Burst,
    FullAuto,
    Melee,
}

impl FireMode {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Single => "SEMI",
            Self::Burst => "BURST",
            Self::FullAuto => "AUTO",
            Self::Melee => "MELEE",
        }
    }
}

/// Player health, armor, cash, and perk status for HUD display.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerState {
    pub health: f32,
    pub max_health: f32,
    pub armor: f32,
    pub max_armor: f32,
    pub cash: u32,
    pub perk: Option<PerkType>,
    pub perk_level: u8,
    pub is_alive: bool,
    pub flashlight_on: bool,
    pub weight: f32,
    pub max_weight: f32,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            health: 100.0,
            max_health: 100.0,
            armor: 100.0,
            max_armor: 100.0,
            cash: 1000,
            perk: Some(PerkType::Commando),
            perk_level: 5,
            is_alive: true,
            flashlight_on: false,
            weight: 8.0,
            max_weight: 15.0,
        }
    }
}

impl PlayerState {
    #[inline]
    pub fn health_ratio(&self) -> f32 {
        if self.max_health <= 0.0 {
            0.0
        } else {
            (self.health / self.max_health).clamp(0.0, 1.0)
        }
    }

    #[inline]
    pub fn armor_ratio(&self) -> f32 {
        if self.max_armor <= 0.0 {
            0.0
        } else {
            (self.armor / self.max_armor).clamp(0.0, 1.0)
        }
    }
}

/// Weapon ammunition, recoil bloom, and reloading state.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponState {
    pub weapon_name: String,
    pub current_ammo: u32,
    pub max_clip: u32,
    pub reserve_ammo: u32,
    pub fire_mode: FireMode,
    pub recoil_bloom: f32,
    pub is_aiming_down_sights: bool,
    pub is_reloading: bool,
    pub reload_progress: f32,
    pub secondary_ammo: Option<u32>,
}

impl Default for WeaponState {
    fn default() -> Self {
        Self {
            weapon_name: "Bullpup".to_string(),
            current_ammo: 40,
            max_clip: 40,
            reserve_ammo: 320,
            fire_mode: FireMode::FullAuto,
            recoil_bloom: 0.0,
            is_aiming_down_sights: false,
            is_reloading: false,
            reload_progress: 0.0,
            secondary_ammo: None,
        }
    }
}

/// Killing Floor wave progression and trader status.
#[derive(Clone, Debug, PartialEq)]
pub struct WaveState {
    pub current_wave: u32,
    pub max_waves: u32,
    pub zeds_remaining: u32,
    pub total_zeds_in_wave: u32,
    pub wave_active: bool,
    pub trader_time_remaining: f32,
    pub trader_distance: Option<f32>,
    pub trader_direction_angle: Option<f32>,
}

impl Default for WaveState {
    fn default() -> Self {
        Self {
            current_wave: 1,
            max_waves: 10,
            zeds_remaining: 35,
            total_zeds_in_wave: 35,
            wave_active: true,
            trader_time_remaining: 0.0,
            trader_distance: None,
            trader_direction_angle: None,
        }
    }
}

/// Hitmarker tracking state for combat feedback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitmarkerState {
    pub timer: f32,
    pub duration: f32,
    pub is_headshot: bool,
}

impl Default for HitmarkerState {
    fn default() -> Self {
        Self {
            timer: 0.0,
            duration: 0.25,
            is_headshot: false,
        }
    }
}

impl HitmarkerState {
    pub fn trigger(&mut self, is_headshot: bool) {
        self.timer = self.duration;
        self.is_headshot = is_headshot;
    }

    pub fn update(&mut self, dt: f32) {
        if self.timer > 0.0 {
            self.timer = (self.timer - dt).max(0.0);
        }
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.timer > 0.0
    }

    #[inline]
    pub fn alpha(&self) -> f32 {
        if self.duration <= 0.0 {
            0.0
        } else {
            (self.timer / self.duration).clamp(0.0, 1.0)
        }
    }
}

/// Draw batch recorded on the 2D HUD canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudDrawBatch {
    pub texture_id: u32,
    pub index_offset: u32,
    pub index_count: u32,
}

/// 2D Canvas for accumulating UI quads and geometry.
pub struct HudCanvas {
    pub width: f32,
    pub height: f32,
    pub vertices: Vec<UIVertex>,
    pub indices: Vec<u32>,
    pub batches: Vec<HudDrawBatch>,
}

impl HudCanvas {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            vertices: Vec::with_capacity(1024),
            indices: Vec::with_capacity(1536),
            batches: Vec::with_capacity(64),
        }
    }

    pub fn reset(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        self.vertices.clear();
        self.indices.clear();
        self.batches.clear();
    }

    /// Appends a textured or colored 2D quad (2 triangles, 4 vertices, 6 indices).
    pub fn draw_quad(
        &mut self,
        p0: [f32; 2],
        p1: [f32; 2],
        p2: [f32; 2],
        p3: [f32; 2],
        uv0: [f32; 2],
        uv1: [f32; 2],
        uv2: [f32; 2],
        uv3: [f32; 2],
        color: [f32; 4],
        texture_id: u32,
    ) {
        let base_vertex = self.vertices.len() as u32;

        self.vertices.push(UIVertex::new(p0, uv0, color));
        self.vertices.push(UIVertex::new(p1, uv1, color));
        self.vertices.push(UIVertex::new(p2, uv2, color));
        self.vertices.push(UIVertex::new(p3, uv3, color));

        let index_start = self.indices.len() as u32;
        self.indices.push(base_vertex);
        self.indices.push(base_vertex + 1);
        self.indices.push(base_vertex + 2);
        self.indices.push(base_vertex);
        self.indices.push(base_vertex + 2);
        self.indices.push(base_vertex + 3);

        // Merge batch with previous if matching texture_id
        if let Some(last_batch) = self.batches.last_mut() {
            if last_batch.texture_id == texture_id {
                last_batch.index_count += 6;
                return;
            }
        }

        self.batches.push(HudDrawBatch {
            texture_id,
            index_offset: index_start,
            index_count: 6,
        });
    }

    /// Draws an axis-aligned solid rectangle.
    pub fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.draw_quad(
            [x, y],
            [x + w, y],
            [x + w, y + h],
            [x, y + h],
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            color,
            0,
        );
    }

    /// Draws an axis-aligned rectangle outline with a specified thickness.
    pub fn draw_rect_outline(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        thickness: f32,
        color: [f32; 4],
    ) {
        // Top
        self.draw_rect(x, y, w, thickness, color);
        // Bottom
        self.draw_rect(x, y + h - thickness, w, thickness, color);
        // Left
        self.draw_rect(x, y + thickness, thickness, h - 2.0 * thickness, color);
        // Right
        self.draw_rect(x + w - thickness, y + thickness, thickness, h - 2.0 * thickness, color);
    }

    /// Draws a progress bar with background plate, filled bar, and border outline.
    pub fn draw_progress_bar(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        ratio: f32,
        bg_color: [f32; 4],
        fill_color: [f32; 4],
        border_color: [f32; 4],
    ) {
        // Background
        self.draw_rect(x, y, w, h, bg_color);

        // Fill bar
        let clamped = ratio.clamp(0.0, 1.0);
        let pad = 2.0;
        let fill_w = (w - pad * 2.0) * clamped;
        let fill_h = h - pad * 2.0;
        if fill_w > 0.0 {
            self.draw_rect(x + pad, y + pad, fill_w, fill_h, fill_color);
        }

        // Border outline
        self.draw_rect_outline(x, y, w, h, 1.5, border_color);
    }

    /// Draws a 2D line segment with thickness.
    pub fn draw_line(&mut self, p1: [f32; 2], p2: [f32; 2], thickness: f32, color: [f32; 4]) {
        let dx = p2[0] - p1[0];
        let dy = p2[1] - p1[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.0001 {
            return;
        }

        let half_t = thickness * 0.5;
        let nx = -dy / len * half_t;
        let ny = dx / len * half_t;

        self.draw_quad(
            [p1[0] + nx, p1[1] + ny],
            [p2[0] + nx, p2[1] + ny],
            [p2[0] - nx, p2[1] - ny],
            [p1[0] - nx, p1[1] - ny],
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            color,
            0,
        );
    }

    /// Draws a 2D triangle.
    pub fn draw_triangle(
        &mut self,
        p0: [f32; 2],
        p1: [f32; 2],
        p2: [f32; 2],
        color: [f32; 4],
        texture_id: u32,
    ) {
        let base_vertex = self.vertices.len() as u32;

        self.vertices.push(UIVertex::new(p0, [0.0, 0.0], color));
        self.vertices.push(UIVertex::new(p1, [1.0, 0.0], color));
        self.vertices.push(UIVertex::new(p2, [0.5, 1.0], color));

        let index_start = self.indices.len() as u32;
        self.indices.push(base_vertex);
        self.indices.push(base_vertex + 1);
        self.indices.push(base_vertex + 2);

        if let Some(last_batch) = self.batches.last_mut() {
            if last_batch.texture_id == texture_id {
                last_batch.index_count += 3;
                return;
            }
        }

        self.batches.push(HudDrawBatch {
            texture_id,
            index_offset: index_start,
            index_count: 3,
        });
    }

    /// Renders text using an embedded 5x7 bitmap matrix font.
    /// Supports ASCII uppercase letters, digits, and common punctuation.
    pub fn draw_text(&mut self, text: &str, mut x: f32, y: f32, scale: f32, color: [f32; 4]) {
        let pixel_size = scale.max(1.0);
        let char_spacing = 6.0 * pixel_size;

        for ch in text.chars() {
            if ch == ' ' {
                x += char_spacing;
                continue;
            }

            let bitmap = get_glyph_bitmap(ch);
            for row in 0..7 {
                let row_bits = (bitmap >> (row * 5)) & 0x1F;
                for col in 0..5 {
                    if (row_bits & (1 << (4 - col))) != 0 {
                        let px = x + col as f32 * pixel_size;
                        let py = y + row as f32 * pixel_size;
                        self.draw_rect(px, py, pixel_size, pixel_size, color);
                    }
                }
            }
            x += char_spacing;
        }
    }
}

/// InGame HUD Configuration settings.
#[derive(Clone, Debug, PartialEq)]
pub struct HudConfig {
    pub scale: f32,
    pub show_crosshair: bool,
    pub crosshair_color: [f32; 4],
    pub crosshair_base_gap: f32,
    pub crosshair_length: f32,
    pub crosshair_thickness: f32,
    pub crosshair_bloom_mult: f32,
    pub show_center_dot: bool,
    pub hide_crosshair_on_ads: bool,
    pub show_hitmarker: bool,
    pub hitmarker_color: [f32; 4],
    pub hitmarker_headshot_color: [f32; 4],
    pub hitmarker_size: f32,
    pub hitmarker_thickness: f32,
    pub show_health_bar: bool,
    pub show_armor_bar: bool,
    pub show_ammo: bool,
    pub show_wave_status: bool,
    pub show_trader_compass: bool,
    pub show_openkf_watermark: bool,
}

impl Default for HudConfig {
    fn default() -> Self {
        Self {
            scale: 1.0,
            show_crosshair: true,
            crosshair_color: [1.0, 1.0, 1.0, 0.85],
            crosshair_base_gap: 6.0,
            crosshair_length: 8.0,
            crosshair_thickness: 2.0,
            crosshair_bloom_mult: 28.0,
            show_center_dot: true,
            hide_crosshair_on_ads: true,
            show_hitmarker: true,
            hitmarker_color: [1.0, 1.0, 1.0, 0.95],
            hitmarker_headshot_color: [1.0, 0.15, 0.15, 1.0],
            hitmarker_size: 10.0,
            hitmarker_thickness: 2.0,
            show_health_bar: true,
            show_armor_bar: true,
            show_ammo: true,
            show_wave_status: true,
            show_trader_compass: true,
            show_openkf_watermark: true,
        }
    }
}

/// Modern In-Game HUD replicating Killing Floor retail visual presentation and mechanics.
pub struct InGameHud {
    pub config: HudConfig,
    pub hitmarker: HitmarkerState,
    pub pulse_timer: f32,
}

impl Default for InGameHud {
    fn default() -> Self {
        Self::new(HudConfig::default())
    }
}

impl InGameHud {
    pub fn new(config: HudConfig) -> Self {
        Self {
            config,
            hitmarker: HitmarkerState::default(),
            pulse_timer: 0.0,
        }
    }

    /// Updates dynamic HUD timers (hitmarker fade, pulse animations).
    pub fn update(&mut self, dt: f32) {
        self.hitmarker.update(dt);
        self.pulse_timer = (self.pulse_timer + dt) % (2.0 * PI);
    }

    /// Triggers a combat hitmarker.
    pub fn trigger_hitmarker(&mut self, is_headshot: bool) {
        self.hitmarker.trigger(is_headshot);
    }

    /// Renders the complete Killing Floor HUD matching retail layout.
    pub fn render(
        &self,
        canvas: &mut HudCanvas,
        player_state: &PlayerState,
        weapon_state: &WeaponState,
        wave_state: &WaveState,
    ) {
        if !player_state.is_alive {
            self.render_death_overlay(canvas);
            return;
        }

        // 1. Dynamic Center Crosshair & Hitmarker
        if self.config.show_crosshair {
            self.render_crosshair(canvas, weapon_state);
        }
        if self.config.show_hitmarker && self.hitmarker.is_active() {
            self.render_hitmarker(canvas);
        }

        // 2. Bottom-Left: Health, Armor, Cash, and Perk Status
        if self.config.show_health_bar || self.config.show_armor_bar {
            self.render_health_and_armor(canvas, player_state);
        }

        // 3. Bottom-Right: Weapon Ammunition, Reserve, and Reload Status
        if self.config.show_ammo {
            self.render_weapon_ammo(canvas, weapon_state);
        }

        // 4. Top-Center: Wave Number, Zed Counter, and Trader Compass
        if self.config.show_wave_status {
            self.render_wave_status(canvas, wave_state);
        }

        // 5. Top-Right: OpenKF Watermark Banner & Live FPS
        if self.config.show_openkf_watermark {
            self.render_watermark(canvas);
        }
    }

    /// Renders the HUD directly onto an UnrealScript-style `Canvas2D` instance.
    pub fn render_to_canvas2d(
        &self,
        canvas: &mut crate::canvas::Canvas2D,
        player_state: &PlayerState,
        weapon_state: &WeaponState,
        wave_state: &WaveState,
    ) {
        let mut hud_canvas = HudCanvas::new(canvas.size_x, canvas.size_y);
        self.render(&mut hud_canvas, player_state, weapon_state, wave_state);

        // Blit hud_canvas vertices and indices into Canvas2D
        for batch in &hud_canvas.batches {
            let start = batch.index_offset as usize;
            let end = start + batch.index_count as usize;
            for i in (start..end).step_by(3) {
                let idx0 = hud_canvas.indices[i] as usize;
                let idx1 = hud_canvas.indices[i + 1] as usize;
                let idx2 = hud_canvas.indices[i + 2] as usize;

                let v0 = &hud_canvas.vertices[idx0];
                let v1 = &hud_canvas.vertices[idx1];
                let v2 = &hud_canvas.vertices[idx2];

                // Approximate triangle draw via lines on Canvas2D
                canvas.draw_line(v0.position[0], v0.position[1], v1.position[0], v1.position[1], v0.color);
                canvas.draw_line(v1.position[0], v1.position[1], v2.position[0], v2.position[1], v1.color);
                canvas.draw_line(v2.position[0], v2.position[1], v0.position[0], v0.position[1], v2.color);
            }
        }
    }

    /// Renders dynamic crosshair with recoil bloom expansion and optional center dot.
    pub fn render_crosshair(&self, canvas: &mut HudCanvas, weapon_state: &WeaponState) {
        if self.config.hide_crosshair_on_ads && weapon_state.is_aiming_down_sights {
            return;
        }

        let cx = canvas.width * 0.5;
        let cy = canvas.height * 0.5;
        let scale = self.config.scale;

        let bloom = weapon_state.recoil_bloom.clamp(0.0, 5.0);
        let gap = (self.config.crosshair_base_gap + bloom * self.config.crosshair_bloom_mult) * scale;
        let length = self.config.crosshair_length * scale;
        let thickness = self.config.crosshair_thickness * scale;
        let color = self.config.crosshair_color;
        let shadow_color = [0.0, 0.0, 0.0, color[3] * 0.6];

        // 1. Center dot
        if self.config.show_center_dot {
            let dot_size = 2.0 * scale;
            let d_offset = dot_size * 0.5;
            canvas.draw_rect(
                cx - d_offset - 1.0,
                cy - d_offset - 1.0,
                dot_size + 2.0,
                dot_size + 2.0,
                shadow_color,
            );
            canvas.draw_rect(cx - d_offset, cy - d_offset, dot_size, dot_size, color);
        }

        let half_t = thickness * 0.5;

        // Shadow / Outline Pass for contrast
        let s_offset = 1.0;
        let draw_tick = |cv: &mut HudCanvas, x: f32, y: f32, w: f32, h: f32, col: [f32; 4]| {
            cv.draw_rect(x, y, w, h, col);
        };

        // Top Tick
        draw_tick(
            canvas,
            cx - half_t - s_offset,
            cy - gap - length - s_offset,
            thickness + 2.0 * s_offset,
            length + 2.0 * s_offset,
            shadow_color,
        );
        draw_tick(canvas, cx - half_t, cy - gap - length, thickness, length, color);

        // Bottom Tick
        draw_tick(
            canvas,
            cx - half_t - s_offset,
            cy + gap - s_offset,
            thickness + 2.0 * s_offset,
            length + 2.0 * s_offset,
            shadow_color,
        );
        draw_tick(canvas, cx - half_t, cy + gap, thickness, length, color);

        // Left Tick
        draw_tick(
            canvas,
            cx - gap - length - s_offset,
            cy - half_t - s_offset,
            length + 2.0 * s_offset,
            thickness + 2.0 * s_offset,
            shadow_color,
        );
        draw_tick(canvas, cx - gap - length, cy - half_t, length, thickness, color);

        // Right Tick
        draw_tick(
            canvas,
            cx + gap - s_offset,
            cy - half_t - s_offset,
            length + 2.0 * s_offset,
            thickness + 2.0 * s_offset,
            shadow_color,
        );
        draw_tick(canvas, cx + gap, cy - half_t, length, thickness, color);
    }

    /// Renders combat hitmarker 'X' cross with headshot coloring and fadeout.
    pub fn render_hitmarker(&self, canvas: &mut HudCanvas) {
        let alpha = self.hitmarker.alpha();
        if alpha <= 0.0 {
            return;
        }

        let cx = canvas.width * 0.5;
        let cy = canvas.height * 0.5;
        let scale = self.config.scale;
        let size = self.config.hitmarker_size * scale;
        let thickness = self.config.hitmarker_thickness * scale;
        let inner_gap = 4.0 * scale;

        let base_color = if self.hitmarker.is_headshot {
            self.config.hitmarker_headshot_color
        } else {
            self.config.hitmarker_color
        };
        let color = [
            base_color[0],
            base_color[1],
            base_color[2],
            base_color[3] * alpha,
        ];

        // 4 diagonal tick lines: Top-Left, Top-Right, Bottom-Left, Bottom-Right
        canvas.draw_line(
            [cx - inner_gap, cy - inner_gap],
            [cx - size, cy - size],
            thickness,
            color,
        );
        canvas.draw_line(
            [cx + inner_gap, cy - inner_gap],
            [cx + size, cy - size],
            thickness,
            color,
        );
        canvas.draw_line(
            [cx - inner_gap, cy + inner_gap],
            [cx - size, cy + size],
            thickness,
            color,
        );
        canvas.draw_line(
            [cx + inner_gap, cy + inner_gap],
            [cx + size, cy + size],
            thickness,
            color,
        );
    }

    /// Renders Health bar, Armor bar, Cash, and Perk information (Bottom-Left).
    pub fn render_health_and_armor(&self, canvas: &mut HudCanvas, player: &PlayerState) {
        let scale = self.config.scale;
        let x = 30.0 * scale;
        let bar_w = 200.0 * scale;
        let bar_h = 16.0 * scale;
        let text_scale = 1.8 * scale;
        let small_scale = 1.3 * scale;

        let base_y = canvas.height - 110.0 * scale;

        // Container panel background
        canvas.draw_rect(
            x - 10.0 * scale,
            base_y - 20.0 * scale,
            bar_w + 100.0 * scale,
            115.0 * scale,
            [0.05, 0.05, 0.08, 0.75],
        );
        canvas.draw_rect_outline(
            x - 10.0 * scale,
            base_y - 20.0 * scale,
            bar_w + 100.0 * scale,
            115.0 * scale,
            1.5 * scale,
            [0.3, 0.3, 0.35, 0.8],
        );

        // --- 1. HEALTH BAR ---
        let health_ratio = player.health_ratio();
        let is_low_health = player.health <= 25.0;
        let pulse = if is_low_health {
            (self.pulse_timer * 4.0).sin().abs() * 0.4 + 0.6
        } else {
            1.0
        };

        let health_fill = if is_low_health {
            [0.9 * pulse, 0.1 * pulse, 0.1 * pulse, 1.0]
        } else if health_ratio < 0.5 {
            [0.9, 0.6, 0.1, 1.0]
        } else {
            [0.85, 0.15, 0.15, 1.0] // KF retail red health
        };

        let health_y = base_y;
        canvas.draw_text("HP", x, health_y + 1.0 * scale, text_scale, [1.0, 0.3, 0.3, 1.0]);
        canvas.draw_progress_bar(
            x + 35.0 * scale,
            health_y,
            bar_w,
            bar_h,
            health_ratio,
            [0.2, 0.05, 0.05, 0.8],
            health_fill,
            [0.5, 0.2, 0.2, 1.0],
        );
        let hp_str = format!("{}", player.health.max(0.0).round() as i32);
        canvas.draw_text(
            &hp_str,
            x + bar_w + 45.0 * scale,
            health_y + 1.0 * scale,
            text_scale,
            [1.0, 1.0, 1.0, 1.0],
        );

        // --- 2. ARMOR / SHIELD BAR ---
        let armor_y = base_y + 24.0 * scale;
        let armor_ratio = player.armor_ratio();
        let armor_fill = [0.15, 0.55, 0.95, 1.0]; // Kevlar Blue

        canvas.draw_text("AP", x, armor_y + 1.0 * scale, text_scale, [0.4, 0.7, 1.0, 1.0]);
        canvas.draw_progress_bar(
            x + 35.0 * scale,
            armor_y,
            bar_w,
            bar_h,
            armor_ratio,
            [0.05, 0.1, 0.2, 0.8],
            armor_fill,
            [0.2, 0.4, 0.7, 1.0],
        );
        let ap_str = format!("{}", player.armor.max(0.0).round() as i32);
        canvas.draw_text(
            &ap_str,
            x + bar_w + 45.0 * scale,
            armor_y + 1.0 * scale,
            text_scale,
            [0.8, 0.9, 1.0, 1.0],
        );

        // --- 3. CASH / DOSH & WEIGHT ---
        let info_y = armor_y + 28.0 * scale;
        let cash_str = format!("DOSH: £{}", player.cash);
        canvas.draw_text(&cash_str, x, info_y, small_scale, [1.0, 0.85, 0.2, 1.0]);

        let weight_str = format!("WT: {:.0}/{:.0}KG", player.weight, player.max_weight);
        canvas.draw_text(
            &weight_str,
            x + 130.0 * scale,
            info_y,
            small_scale,
            [0.75, 0.75, 0.75, 1.0],
        );

        // --- 4. PERK & LEVEL ---
        if let Some(perk) = player.perk {
            let perk_str = format!("LVL {} {}", player.perk_level, perk.display_name());
            canvas.draw_text(
                &perk_str,
                x,
                base_y - 14.0 * scale,
                small_scale,
                [0.9, 0.6, 0.2, 1.0],
            );
        }
    }

    /// Renders weapon clip count, reserve ammo, reload progress, and fire mode (Bottom-Right).
    pub fn render_weapon_ammo(&self, canvas: &mut HudCanvas, weapon: &WeaponState) {
        let scale = self.config.scale;
        let box_w = 220.0 * scale;
        let box_h = 85.0 * scale;
        let x = canvas.width - box_w - 30.0 * scale;
        let y = canvas.height - box_h - 25.0 * scale;

        // Container panel
        canvas.draw_rect(x, y, box_w, box_h, [0.05, 0.05, 0.08, 0.75]);
        canvas.draw_rect_outline(x, y, box_w, box_h, 1.5 * scale, [0.3, 0.3, 0.35, 0.8]);

        // Weapon Name
        let name_scale = 1.6 * scale;
        canvas.draw_text(
            &weapon.weapon_name.to_uppercase(),
            x + 12.0 * scale,
            y + 10.0 * scale,
            name_scale,
            [0.9, 0.9, 0.9, 1.0],
        );

        // Fire Mode Tag
        let mode_str = weapon.fire_mode.display_name();
        canvas.draw_text(
            mode_str,
            x + box_w - 55.0 * scale,
            y + 12.0 * scale,
            1.2 * scale,
            [0.6, 0.8, 1.0, 1.0],
        );

        // Reloading Indicator
        if weapon.is_reloading {
            let reload_text = "RELOADING...";
            canvas.draw_text(
                reload_text,
                x + 12.0 * scale,
                y + 35.0 * scale,
                1.5 * scale,
                [1.0, 0.8, 0.1, 1.0],
            );
            canvas.draw_progress_bar(
                x + 12.0 * scale,
                y + 55.0 * scale,
                box_w - 24.0 * scale,
                10.0 * scale,
                weapon.reload_progress,
                [0.2, 0.2, 0.2, 0.8],
                [1.0, 0.7, 0.0, 1.0],
                [0.5, 0.4, 0.1, 1.0],
            );
            return;
        }

        // Ammunition Counts (Clip / Reserve)
        let ammo_scale = 2.4 * scale;
        let is_low_ammo = weapon.max_clip > 0 && weapon.current_ammo <= (weapon.max_clip / 4);
        let ammo_color = if weapon.current_ammo == 0 {
            [1.0, 0.1, 0.1, 1.0]
        } else if is_low_ammo {
            [1.0, 0.7, 0.1, 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };

        let clip_str = format!("{:02}", weapon.current_ammo);
        canvas.draw_text(&clip_str, x + 12.0 * scale, y + 36.0 * scale, ammo_scale, ammo_color);

        let slash_x = x + 62.0 * scale;
        canvas.draw_text("/", slash_x, y + 38.0 * scale, 2.0 * scale, [0.5, 0.5, 0.5, 1.0]);

        let reserve_str = format!("{}", weapon.reserve_ammo);
        canvas.draw_text(
            &reserve_str,
            slash_x + 18.0 * scale,
            y + 40.0 * scale,
            1.8 * scale,
            [0.75, 0.75, 0.75, 1.0],
        );

        // Secondary Ammo (e.g. M203 grenade rounds)
        if let Some(sec_ammo) = weapon.secondary_ammo {
            let sec_str = format!("SEC: {}", sec_ammo);
            canvas.draw_text(
                &sec_str,
                x + box_w - 65.0 * scale,
                y + 42.0 * scale,
                1.3 * scale,
                [1.0, 0.5, 0.2, 1.0],
            );
        }
    }

    /// Renders Wave status, specimens remaining, and trader countdown / compass (Top-Center).
    pub fn render_wave_status(&self, canvas: &mut HudCanvas, wave: &WaveState) {
        let scale = self.config.scale;
        let banner_w = 260.0 * scale;
        let banner_h = 55.0 * scale;
        let cx = canvas.width * 0.5;
        let x = cx - banner_w * 0.5;
        let y = 15.0 * scale;

        // Banner backdrop
        canvas.draw_rect(x, y, banner_w, banner_h, [0.05, 0.05, 0.08, 0.75]);
        canvas.draw_rect_outline(x, y, banner_w, banner_h, 1.5 * scale, [0.3, 0.3, 0.35, 0.8]);

        let text_scale = 1.6 * scale;

        // Wave counter header
        let wave_str = if wave.current_wave >= wave.max_waves {
            "FINAL WAVE".to_string()
        } else {
            format!("WAVE {} / {}", wave.current_wave, wave.max_waves)
        };
        canvas.draw_text(&wave_str, x + 16.0 * scale, y + 8.0 * scale, text_scale, [1.0, 0.85, 0.2, 1.0]);

        if wave.wave_active {
            // Zeds Remaining
            let zed_str = format!("ZEDS: {}", wave.zeds_remaining);
            let zed_color = if wave.zeds_remaining <= 5 {
                [1.0, 0.3, 0.3, 1.0] // Pulse or highlight last few Zeds
            } else {
                [0.9, 0.9, 0.9, 1.0]
            };
            canvas.draw_text(&zed_str, x + 16.0 * scale, y + 30.0 * scale, text_scale, zed_color);
        } else {
            // Trader Open Phase
            let time_secs = wave.trader_time_remaining.max(0.0) as u32;
            let trader_str = format!("TRADER: {:02}:{:02}", time_secs / 60, time_secs % 60);
            canvas.draw_text(&trader_str, x + 16.0 * scale, y + 30.0 * scale, text_scale, [0.2, 0.9, 0.4, 1.0]);

            // Trader distance & compass direction indicator
            if self.config.show_trader_compass {
                if let Some(dist) = wave.trader_distance {
                    let dist_str = format!("{:.0}M", dist);
                    canvas.draw_text(
                        &dist_str,
                        x + banner_w - 60.0 * scale,
                        y + 30.0 * scale,
                        1.4 * scale,
                        [0.4, 1.0, 0.6, 1.0],
                    );
                }

                if let Some(angle) = wave.trader_direction_angle {
                    self.render_trader_arrow(canvas, cx, y + banner_h + 20.0 * scale, angle);
                }
            }
        }
    }

    /// Renders directional trader compass arrow pointing towards the trader pod.
    pub fn render_trader_arrow(&self, canvas: &mut HudCanvas, cx: f32, cy: f32, angle: f32) {
        let scale = self.config.scale;
        let radius = 16.0 * scale;
        let color = [0.2, 0.95, 0.4, 0.9];

        // Arrow tip
        let tip_x = cx + angle.sin() * radius;
        let tip_y = cy - angle.cos() * radius;

        // Base wings
        let wing_angle = 2.4; // ~137 degrees from tip
        let left_x = cx + (angle + wing_angle).sin() * (radius * 0.7);
        let left_y = cy - (angle + wing_angle).cos() * (radius * 0.7);

        let right_x = cx + (angle - wing_angle).sin() * (radius * 0.7);
        let right_y = cy - (angle - wing_angle).cos() * (radius * 0.7);

        canvas.draw_triangle([tip_x, tip_y], [left_x, left_y], [right_x, right_y], color, 0);
    }

    /// Renders red death screen overlay.
    pub fn render_death_overlay(&self, canvas: &mut HudCanvas) {
        let scale = self.config.scale;
        // Blood red wash
        canvas.draw_rect(
            0.0,
            0.0,
            canvas.width,
            canvas.height,
            [0.35, 0.02, 0.02, 0.65],
        );

        let cx = canvas.width * 0.5;
        let cy = canvas.height * 0.5;
        let text = "YOU ARE DEAD";
        let text_scale = 3.5 * scale;
        let text_w = text.len() as f32 * 6.0 * text_scale;
        canvas.draw_text(
            text,
            cx - text_w * 0.5,
            cy - 20.0 * scale,
            text_scale,
            [1.0, 0.1, 0.1, 1.0],
        );
    }

    /// Renders top-right OpenKF watermark and live FPS overlay.
    pub fn render_watermark(&self, canvas: &mut HudCanvas) {
        let scale = self.config.scale;
        let pad_x = 20.0 * scale;
        let pad_y = 16.0 * scale;

        let title = "OpenKF v0.2 - By MSDOS10";
        let title_scale = 1.35 * scale;
        let char_w = 6.0 * title_scale;
        let title_w = title.len() as f32 * char_w;

        let x1 = canvas.width - pad_x - title_w;
        let y1 = pad_y;

        // Shadow + Foreground (Cyan)
        canvas.draw_text(title, x1 + 1.0, y1 + 1.0, title_scale, [0.0, 0.0, 0.0, 0.85]);
        canvas.draw_text(title, x1, y1, title_scale, [0.0, 0.9, 1.0, 1.0]);

        // FPS line (underneath, right-aligned)
        let fps_text = "60 FPS";
        let fps_scale = 1.25 * scale;
        let fps_char_w = 6.0 * fps_scale;
        let fps_w = fps_text.len() as f32 * fps_char_w;
        let x2 = canvas.width - pad_x - fps_w;
        let y2 = y1 + 16.0 * scale;

        // Shadow + Foreground (Green)
        canvas.draw_text(fps_text, x2 + 1.0, y2 + 1.0, fps_scale, [0.0, 0.0, 0.0, 0.85]);
        canvas.draw_text(fps_text, x2, y2, fps_scale, [0.35, 1.0, 0.35, 1.0]);
    }
}

/// Compact 5x7 matrix bitmap font dictionary for HUD characters.
/// Encoded as 35 bits (stored in a u64) row by row from top to bottom.
fn get_glyph_bitmap(ch: char) -> u64 {
    match ch.to_ascii_uppercase() {
        '0' => 0b01110_10001_10011_10101_11001_10001_01110,
        '1' => 0b00100_01100_00100_00100_00100_00100_01110,
        '2' => 0b01110_10001_00001_00010_00100_01000_11111,
        '3' => 0b11110_00001_00001_01110_00001_00001_11110,
        '4' => 0b00010_00110_01010_10010_11111_00010_00010,
        '5' => 0b11111_10000_11110_00001_00001_10001_01110,
        '6' => 0b00110_01000_10000_11110_10001_10001_01110,
        '7' => 0b11111_00001_00010_00100_01000_01000_01000,
        '8' => 0b01110_10001_10001_01110_10001_10001_01110,
        '9' => 0b01110_10001_10001_01111_00001_00010_01100,
        'A' => 0b01110_10001_10001_11111_10001_10001_10001,
        'B' => 0b11110_10001_10001_11110_10001_10001_11110,
        'C' => 0b01110_10001_10000_10000_10000_10001_01110,
        'D' => 0b11100_10010_10001_10001_10001_10010_11100,
        'E' => 0b11111_10000_10000_11110_10000_10000_11111,
        'F' => 0b11111_10000_10000_11110_10000_10000_10000,
        'G' => 0b01110_10001_10000_10111_10001_10001_01110,
        'H' => 0b10001_10001_10001_11111_10001_10001_10001,
        'I' => 0b01110_00100_00100_00100_00100_00100_01110,
        'J' => 0b00001_00001_00001_00001_10001_10001_01110,
        'K' => 0b10001_10010_10100_11000_10100_10010_10001,
        'L' => 0b10000_10000_10000_10000_10000_10000_11111,
        'M' => 0b10001_11011_10101_10101_10001_10001_10001,
        'N' => 0b10001_11001_10101_10011_10001_10001_10001,
        'O' => 0b01110_10001_10001_10001_10001_10001_01110,
        'P' => 0b11110_10001_10001_11110_10000_10000_10000,
        'Q' => 0b01110_10001_10001_10001_10101_10011_01101,
        'R' => 0b11110_10001_10001_11110_10100_10010_10001,
        'S' => 0b01110_10001_10000_01110_00001_10001_01110,
        'T' => 0b11111_00100_00100_00100_00100_00100_00100,
        'U' => 0b10001_10001_10001_10001_10001_10001_01110,
        'V' => 0b10001_10001_10001_10001_10001_01010_00100,
        'W' => 0b10001_10001_10001_10101_10101_11011_10001,
        'X' => 0b10001_10001_01010_00100_01010_10001_10001,
        'Y' => 0b10001_10001_01010_00100_00100_00100_00100,
        'Z' => 0b11111_00001_00010_00100_01000_10000_11111,
        '/' => 0b00001_00010_00010_00100_01000_01000_10000,
        ':' => 0b00000_01100_01100_00000_01100_01100_00000,
        '-' => 0b00000_00000_00000_11111_00000_00000_00000,
        '+' => 0b00000_00100_00100_11111_00100_00100_00000,
        '.' => 0b00000_00000_00000_00000_00000_01100_01100,
        '£' => 0b01110_10001_10000_11100_10000_10000_11111,
        '%' => 0b11001_11010_00100_01000_01011_10011_00000,
        '[' => 0b01110_01000_01000_01000_01000_01000_01110,
        ']' => 0b01110_00010_00010_00010_00010_00010_01110,
        _ => 0b11111_10001_10001_10001_10001_10001_11111, // Square box fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_quad_generation() {
        let mut canvas = HudCanvas::new(1920.0, 1080.0);
        assert_eq!(canvas.vertices.len(), 0);
        assert_eq!(canvas.indices.len(), 0);

        canvas.draw_rect(10.0, 20.0, 100.0, 50.0, [1.0, 0.0, 0.0, 1.0]);

        // 1 rect = 4 vertices, 6 indices, 1 batch
        assert_eq!(canvas.vertices.len(), 4);
        assert_eq!(canvas.indices.len(), 6);
        assert_eq!(canvas.batches.len(), 1);
        assert_eq!(canvas.batches[0].index_count, 6);

        // Verify UIVertex bounds
        assert_eq!(canvas.vertices[0].position, [10.0, 20.0]);
        assert_eq!(canvas.vertices[2].position, [110.0, 70.0]);
    }

    #[test]
    fn test_canvas_line_generation() {
        let mut canvas = HudCanvas::new(1920.0, 1080.0);
        canvas.draw_line([0.0, 0.0], [100.0, 0.0], 2.0, [1.0, 1.0, 1.0, 1.0]);

        assert_eq!(canvas.vertices.len(), 4);
        assert_eq!(canvas.indices.len(), 6);
    }

    #[test]
    fn test_canvas_text_generation() {
        let mut canvas = HudCanvas::new(1920.0, 1080.0);
        canvas.draw_text("HP 100", 0.0, 0.0, 1.0, [1.0, 1.0, 1.0, 1.0]);

        // "HP 100" consists of 6 chars (including space)
        // Space generates no vertices, glyphs generate pixel quads
        assert!(canvas.vertices.len() > 0);
        assert!(canvas.indices.len() > 0);
    }

    #[test]
    fn test_player_state_ratios() {
        let player = PlayerState {
            health: 75.0,
            max_health: 100.0,
            armor: 50.0,
            max_armor: 100.0,
            ..Default::default()
        };

        assert!((player.health_ratio() - 0.75).abs() < 0.001);
        assert!((player.armor_ratio() - 0.50).abs() < 0.001);

        // Clamping check
        let dead_player = PlayerState {
            health: -10.0,
            max_health: 100.0,
            armor: 0.0,
            max_armor: 100.0,
            ..Default::default()
        };
        assert_eq!(dead_player.health_ratio(), 0.0);
    }

    #[test]
    fn test_hitmarker_lifecycle() {
        let mut hud = InGameHud::default();
        assert!(!hud.hitmarker.is_active());
        assert_eq!(hud.hitmarker.alpha(), 0.0);

        hud.trigger_hitmarker(true);
        assert!(hud.hitmarker.is_active());
        assert!(hud.hitmarker.is_headshot);
        assert_eq!(hud.hitmarker.alpha(), 1.0);

        hud.update(0.1);
        assert!(hud.hitmarker.is_active());
        assert!(hud.hitmarker.alpha() < 1.0 && hud.hitmarker.alpha() > 0.0);

        hud.update(0.2); // Total 0.3s > 0.25s duration
        assert!(!hud.hitmarker.is_active());
        assert_eq!(hud.hitmarker.alpha(), 0.0);
    }

    #[test]
    fn test_recoil_bloom_crosshair() {
        let hud = InGameHud::default();
        let mut canvas_rest = HudCanvas::new(1920.0, 1080.0);
        let mut canvas_bloom = HudCanvas::new(1920.0, 1080.0);

        let weap_rest = WeaponState {
            recoil_bloom: 0.0,
            ..Default::default()
        };
        let weap_bloom = WeaponState {
            recoil_bloom: 1.0,
            ..Default::default()
        };

        hud.render_crosshair(&mut canvas_rest, &weap_rest);
        hud.render_crosshair(&mut canvas_bloom, &weap_bloom);

        // Check that blooming pushes tick vertices further away from center
        // Center is 960, 540
        let rest_top_y = canvas_rest.vertices.iter().map(|v| v.position[1]).fold(f32::INFINITY, f32::min);
        let bloom_top_y = canvas_bloom.vertices.iter().map(|v| v.position[1]).fold(f32::INFINITY, f32::min);

        assert!(bloom_top_y < rest_top_y, "Bloom should push top tick higher up (smaller y)");
    }

    #[test]
    fn test_hud_complete_render_pass() {
        let hud = InGameHud::default();
        let mut canvas = HudCanvas::new(1920.0, 1080.0);

        let player = PlayerState::default();
        let weapon = WeaponState::default();
        let wave = WaveState::default();

        hud.render(&mut canvas, &player, &weapon, &wave);

        // Complete HUD must output substantial geometry for bars, text, crosshair
        assert!(canvas.vertices.len() >= 100);
        assert!(canvas.indices.len() >= 150);
        assert!(canvas.batches.len() >= 1);
    }

    #[test]
    fn test_hud_death_overlay() {
        let hud = InGameHud::default();
        let mut canvas = HudCanvas::new(1920.0, 1080.0);

        let dead_player = PlayerState {
            health: 0.0,
            is_alive: false,
            ..Default::default()
        };
        let weapon = WeaponState::default();
        let wave = WaveState::default();

        hud.render(&mut canvas, &dead_player, &weapon, &wave);

        // Must draw death overlay
        assert!(canvas.vertices.len() > 0);
    }

    #[test]
    fn test_trader_direction_arrow() {
        let mut canvas = HudCanvas::new(1920.0, 1080.0);
        let hud = InGameHud::default();

        hud.render_trader_arrow(&mut canvas, 960.0, 100.0, 0.0);

        // Arrow generates 1 triangle = 3 vertices, 3 indices
        assert_eq!(canvas.vertices.len(), 3);
        assert_eq!(canvas.indices.len(), 3);
    }

    #[test]
    fn test_render_to_canvas2d() {
        let mut canvas2d = crate::canvas::Canvas2D::new(1920.0, 1080.0);
        let hud = InGameHud::default();
        let player = PlayerState::default();
        let weapon = WeaponState::default();
        let wave = WaveState::default();

        hud.render_to_canvas2d(&mut canvas2d, &player, &weapon, &wave);

        // Flush canvas to verify command recording
        let flush = canvas2d.flush_to_gpu();
        assert!(flush.line_count > 0);
        assert!(flush.commands.len() > 0);
    }
}
