//! Interactive 3D Main Menu and Navigation Architecture for OpenKF
//!
//! Inspired by the retail Killing Floor v1065 / Unreal Engine 2.5 `GUI2K4` architecture (`KFMainMenu`).
//! Connects the 2D immediate GUI framework with a real-time 3D viewport background scene (`Entry.rom`).
//!
//! Features:
//! - [`MainMenuPage`]: Root page implementing [`crate::gui::GUIComponent`].
//! - Primary navigation button stack:
//!   - Solo (`btn_solo`): Instant action and offline wave survival practice
//!   - Multiplayer Server Browser (`btn_multiplayer`): Browse Internet, LAN, and Favorites servers
//!   - Perks (`btn_perks`): View perk progression, bonuses, and achievements (Berserker, Commando, Support, etc.)
//!   - Settings (`btn_settings`): Audio volume, FOV, sensitivity, and video configuration
//!   - Exit (`btn_exit`): Quit confirmation dialog
//! - Interactive sound cues: Emits [`MenuSoundCue::Hover`] and [`MenuSoundCue::Click`] events
//! - Real-time 3D background viewport scene ([`MainMenu3DBackground`]): Cinematic camera orbit,
//!   lighting flicker, and atmospheric fog simulation behind the 2D UI canvas.

use std::any::Any;
use std::fmt::Debug;
use glam::Vec3;

use crate::camera::Camera;
use crate::error::RenderError;
use crate::gui::{
    Canvas, ComponentState, GUIComponent, LayoutBox,
    MouseButton, MouseEvent, Rect, TextAlignment,
};
use crate::hal::{PrimitiveType, RenderInterface, TransformType, Viewport};
use crate::hud::PerkType;
use crate::scene::SceneUniforms;
use crate::server_browser::ServerDifficulty;
use crate::vertex::UIVertex;
use crate::viewport::{Scene, SceneMeshItem};

/// Well-known button IDs used across the Killing Floor main menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MenuButtonId {
    Solo,
    Multiplayer,
    Perks,
    Settings,
    Exit,
    SoloStart,
    SoloBack,
    SoloMapNext,
    SoloMapPrev,
    SoloDiffNext,
    SoloDiffPrev,
    SoloWavesCycle,
    PerksBack,
    SettingsBack,
    SettingsFpsNext,
    SettingsFpsPrev,
    SettingsFpsCycle,
    SettingsAnisoNext,
    SettingsAnisoPrev,
    SettingsAnisoCycle,
    ExitConfirm,
    ExitCancel,
}

impl MenuButtonId {
    pub const BTN_SOLO: &'static str = "btn_solo";
    pub const BTN_MULTIPLAYER: &'static str = "btn_multiplayer";
    pub const BTN_PERKS: &'static str = "btn_perks";
    pub const BTN_SETTINGS: &'static str = "btn_settings";
    pub const BTN_EXIT: &'static str = "btn_exit";
    pub const BTN_SOLO_START: &'static str = "btn_solo_start";
    pub const BTN_SOLO_BACK: &'static str = "btn_solo_back";
    pub const BTN_SOLO_MAP_NEXT: &'static str = "btn_solo_map_next";
    pub const BTN_SOLO_MAP_PREV: &'static str = "btn_solo_map_prev";
    pub const BTN_SOLO_DIFF_NEXT: &'static str = "btn_solo_diff_next";
    pub const BTN_SOLO_DIFF_PREV: &'static str = "btn_solo_diff_prev";
    pub const BTN_SOLO_WAVES_CYCLE: &'static str = "btn_solo_waves_cycle";
    pub const BTN_PERKS_BACK: &'static str = "btn_perks_back";
    pub const BTN_SETTINGS_BACK: &'static str = "btn_settings_back";
    pub const BTN_SETTINGS_FPS_NEXT: &'static str = "btn_settings_fps_next";
    pub const BTN_SETTINGS_FPS_PREV: &'static str = "btn_settings_fps_prev";
    pub const BTN_SETTINGS_FPS_CYCLE: &'static str = "btn_settings_fps_cycle";
    pub const BTN_SETTINGS_ANISO_NEXT: &'static str = "btn_settings_aniso_next";
    pub const BTN_SETTINGS_ANISO_PREV: &'static str = "btn_settings_aniso_prev";
    pub const BTN_SETTINGS_ANISO_CYCLE: &'static str = "btn_settings_aniso_cycle";
    pub const BTN_EXIT_CONFIRM: &'static str = "btn_exit_confirm";
    pub const BTN_EXIT_CANCEL: &'static str = "btn_exit_cancel";

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Solo => Self::BTN_SOLO,
            Self::Multiplayer => Self::BTN_MULTIPLAYER,
            Self::Perks => Self::BTN_PERKS,
            Self::Settings => Self::BTN_SETTINGS,
            Self::Exit => Self::BTN_EXIT,
            Self::SoloStart => Self::BTN_SOLO_START,
            Self::SoloBack => Self::BTN_SOLO_BACK,
            Self::SoloMapNext => Self::BTN_SOLO_MAP_NEXT,
            Self::SoloMapPrev => Self::BTN_SOLO_MAP_PREV,
            Self::SoloDiffNext => Self::BTN_SOLO_DIFF_NEXT,
            Self::SoloDiffPrev => Self::BTN_SOLO_DIFF_PREV,
            Self::SoloWavesCycle => Self::BTN_SOLO_WAVES_CYCLE,
            Self::PerksBack => Self::BTN_PERKS_BACK,
            Self::SettingsBack => Self::BTN_SETTINGS_BACK,
            Self::SettingsFpsNext => Self::BTN_SETTINGS_FPS_NEXT,
            Self::SettingsFpsPrev => Self::BTN_SETTINGS_FPS_PREV,
            Self::SettingsFpsCycle => Self::BTN_SETTINGS_FPS_CYCLE,
            Self::SettingsAnisoNext => Self::BTN_SETTINGS_ANISO_NEXT,
            Self::SettingsAnisoPrev => Self::BTN_SETTINGS_ANISO_PREV,
            Self::SettingsAnisoCycle => Self::BTN_SETTINGS_ANISO_CYCLE,
            Self::ExitConfirm => Self::BTN_EXIT_CONFIRM,
            Self::ExitCancel => Self::BTN_EXIT_CANCEL,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            Self::BTN_SOLO => Some(Self::Solo),
            Self::BTN_MULTIPLAYER | "btn_server_browser" => Some(Self::Multiplayer),
            Self::BTN_PERKS => Some(Self::Perks),
            Self::BTN_SETTINGS => Some(Self::Settings),
            Self::BTN_EXIT => Some(Self::Exit),
            Self::BTN_SOLO_START => Some(Self::SoloStart),
            Self::BTN_SOLO_BACK => Some(Self::SoloBack),
            Self::BTN_SOLO_MAP_NEXT => Some(Self::SoloMapNext),
            Self::BTN_SOLO_MAP_PREV => Some(Self::SoloMapPrev),
            Self::BTN_SOLO_DIFF_NEXT => Some(Self::SoloDiffNext),
            Self::BTN_SOLO_DIFF_PREV => Some(Self::SoloDiffPrev),
            Self::BTN_SOLO_WAVES_CYCLE => Some(Self::SoloWavesCycle),
            Self::BTN_PERKS_BACK => Some(Self::PerksBack),
            Self::BTN_SETTINGS_BACK => Some(Self::SettingsBack),
            Self::BTN_SETTINGS_FPS_NEXT => Some(Self::SettingsFpsNext),
            Self::BTN_SETTINGS_FPS_PREV => Some(Self::SettingsFpsPrev),
            Self::BTN_SETTINGS_FPS_CYCLE => Some(Self::SettingsFpsCycle),
            Self::BTN_SETTINGS_ANISO_NEXT => Some(Self::SettingsAnisoNext),
            Self::BTN_SETTINGS_ANISO_PREV => Some(Self::SettingsAnisoPrev),
            Self::BTN_SETTINGS_ANISO_CYCLE => Some(Self::SettingsAnisoCycle),
            Self::BTN_EXIT_CONFIRM => Some(Self::ExitConfirm),
            Self::BTN_EXIT_CANCEL => Some(Self::ExitCancel),
            _ => None,
        }
    }
}

/// Sound cue events triggered by menu interactions (matching retail `KF_MenuSnd`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MenuSoundCue {
    Hover,
    Click,
    Back,
    Confirm,
    Select,
    Slide,
}

impl MenuSoundCue {
    pub fn sound_cue_name(&self) -> &'static str {
        match self {
            Self::Hover => "KF_MenuSnd.Hover",
            Self::Click => "KF_MenuSnd.Click",
            Self::Back => "KF_MenuSnd.Back",
            Self::Confirm => "KF_MenuSnd.Confirm",
            Self::Select => "KF_MenuSnd.Select",
            Self::Slide => "KF_MenuSnd.Slide",
        }
    }
}

/// Active screen or modal state in the main menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MainMenuState {
    #[default]
    Main,
    SoloLobby,
    ServerBrowser,
    Perks,
    Settings,
    ConfirmExit,
}

/// Actions emitted when an interactive button is activated.
#[derive(Clone, Debug, PartialEq)]
pub enum MainMenuAction {
    None,
    StartSolo {
        map: String,
        difficulty: ServerDifficulty,
        waves: u32,
    },
    OpenServerBrowser,
    OpenPerks,
    OpenSettings,
    ExitGame,
    StateChanged(MainMenuState),
    FpsCapChanged(u32),
    AnisotropyChanged(u32),
}

/// Individual menu push button with styling, hover animations, and sound cues.
#[derive(Clone, Debug)]
pub struct MainMenuButton {
    pub id: String,
    pub label: String,
    pub subtext: String,
    pub layout_box: LayoutBox,
    pub bounds: Rect,
    pub state: ComponentState,
    pub enabled: bool,
    pub visible: bool,
    pub hover_anim: f32, // 0.0 (normal) .. 1.0 (fully hovered glow)
}

impl MainMenuButton {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        subtext: impl Into<String>,
        layout_box: LayoutBox,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            subtext: subtext.into(),
            layout_box,
            bounds: Rect::default(),
            state: ComponentState::Normal,
            enabled: true,
            visible: true,
            hover_anim: 0.0,
        }
    }

    pub fn update(&mut self, delta_time: f32) {
        let target = if self.state == ComponentState::Hovered || self.state == ComponentState::Pressed {
            1.0
        } else {
            0.0
        };
        let speed = if target > self.hover_anim { 9.0 } else { 7.0 };
        if self.hover_anim < target {
            self.hover_anim = (self.hover_anim + delta_time * speed).min(target);
        } else if self.hover_anim > target {
            self.hover_anim = (self.hover_anim - delta_time * speed).max(target);
        }
    }
}

/// Configuration settings for a Solo offline practice game.
#[derive(Clone, Debug, PartialEq)]
pub struct SoloMatchConfig {
    pub selected_map_index: usize,
    pub available_maps: Vec<String>,
    pub difficulty: ServerDifficulty,
    pub wave_count: u32,
}

impl Default for SoloMatchConfig {
    fn default() -> Self {
        Self {
            selected_map_index: 0,
            available_maps: vec![
                "KF-BioticsLab".to_string(),
                "KF-WestLondon".to_string(),
                "KF-Farm".to_string(),
                "KF-Manor".to_string(),
                "KF-Offices".to_string(),
            ],
            difficulty: ServerDifficulty::Normal,
            wave_count: 7, // Medium (7 waves + Patriarch)
        }
    }
}

impl SoloMatchConfig {
    pub fn current_map(&self) -> &str {
        if self.available_maps.is_empty() {
            "KF-BioticsLab"
        } else {
            &self.available_maps[self.selected_map_index % self.available_maps.len()]
        }
    }

    pub fn next_map(&mut self) {
        if !self.available_maps.is_empty() {
            self.selected_map_index = (self.selected_map_index + 1) % self.available_maps.len();
        }
    }

    pub fn prev_map(&mut self) {
        if !self.available_maps.is_empty() {
            self.selected_map_index = if self.selected_map_index == 0 {
                self.available_maps.len() - 1
            } else {
                self.selected_map_index - 1
            };
        }
    }

    pub fn next_difficulty(&mut self) {
        self.difficulty = match self.difficulty {
            ServerDifficulty::Beginner => ServerDifficulty::Normal,
            ServerDifficulty::Normal => ServerDifficulty::Hard,
            ServerDifficulty::Hard => ServerDifficulty::Suicidal,
            ServerDifficulty::Suicidal => ServerDifficulty::HellOnEarth,
            ServerDifficulty::HellOnEarth => ServerDifficulty::Beginner,
        };
    }

    pub fn prev_difficulty(&mut self) {
        self.difficulty = match self.difficulty {
            ServerDifficulty::Beginner => ServerDifficulty::HellOnEarth,
            ServerDifficulty::Normal => ServerDifficulty::Beginner,
            ServerDifficulty::Hard => ServerDifficulty::Normal,
            ServerDifficulty::Suicidal => ServerDifficulty::Hard,
            ServerDifficulty::HellOnEarth => ServerDifficulty::Suicidal,
        };
    }

    pub fn cycle_waves(&mut self) {
        self.wave_count = match self.wave_count {
            4 => 7,
            7 => 10,
            _ => 4,
        };
    }

    pub fn wave_length_name(&self) -> &'static str {
        match self.wave_count {
            4 => "Short (4 Waves)",
            7 => "Medium (7 Waves)",
            10 => "Long (10 Waves)",
            _ => "Custom",
        }
    }
}

/// Progression and bonus statistics for Killing Floor perks.
#[derive(Clone, Debug, PartialEq)]
pub struct PerkProgressInfo {
    pub perk: PerkType,
    pub level: u8,
    pub current_xp: u32,
    pub next_level_xp: u32,
    pub description: String,
}

impl PerkProgressInfo {
    pub fn new(perk: PerkType, level: u8, current_xp: u32, next_level_xp: u32, description: impl Into<String>) -> Self {
        Self {
            perk,
            level: level.min(6),
            current_xp,
            next_level_xp,
            description: description.into(),
        }
    }

    pub fn progress_ratio(&self) -> f32 {
        if self.next_level_xp == 0 {
            1.0
        } else {
            (self.current_xp as f32 / self.next_level_xp as f32).clamp(0.0, 1.0)
        }
    }
}

/// Configurable user preferences.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuSettings {
    pub master_volume: f32,
    pub sfx_volume: f32,
    pub music_volume: f32,
    pub fov_degrees: f32,
    pub mouse_sensitivity: f32,
    pub fullscreen: bool,
    pub vsync: bool,
    pub fps_cap: u32,
    pub anisotropy_level: u32,
}

impl Default for MenuSettings {
    fn default() -> Self {
        Self {
            master_volume: 0.85,
            sfx_volume: 0.90,
            music_volume: 0.70,
            fov_degrees: 90.0,
            mouse_sensitivity: 2.5,
            fullscreen: false,
            vsync: true,
            fps_cap: 165,
            anisotropy_level: 16,
        }
    }
}

impl MenuSettings {
    /// Cycles to the next FPS cap preset in [60, 120, 140, 165, 180, 220, 240, 0 (Uncapped)].
    pub fn cycle_fps_next(&mut self) -> u32 {
        self.fps_cap = match self.fps_cap {
            60 => 120,
            120 => 140,
            140 => 165,
            165 => 180,
            180 => 220,
            220 => 240,
            240 => 0,
            _ => 60,
        };
        self.fps_cap
    }

    /// Cycles to the previous FPS cap preset.
    pub fn cycle_fps_prev(&mut self) -> u32 {
        self.fps_cap = match self.fps_cap {
            0 => 240,
            240 => 220,
            220 => 180,
            180 => 165,
            165 => 140,
            140 => 120,
            120 => 60,
            _ => 0,
        };
        self.fps_cap
    }

    /// Human-friendly display string for GUI rendering.
    pub fn fps_display_string(&self) -> String {
        if self.fps_cap == 0 {
            "Uncapped (Max)".to_string()
        } else {
            format!("{} FPS", self.fps_cap)
        }
    }

    /// Cycles to the next Anisotropic Filtering preset in [0 (Sans), 2x, 4x, 6x, 8x, 10x, 12x, 14x, 16x (Max)].
    pub fn cycle_anisotropy_next(&mut self) -> u32 {
        self.anisotropy_level = match self.anisotropy_level {
            0 => 2,
            2 => 4,
            4 => 6,
            6 => 8,
            8 => 10,
            10 => 12,
            12 => 14,
            14 => 16,
            _ => 0,
        };
        self.anisotropy_level
    }

    /// Cycles to the previous Anisotropic Filtering preset.
    pub fn cycle_anisotropy_prev(&mut self) -> u32 {
        self.anisotropy_level = match self.anisotropy_level {
            16 => 14,
            14 => 12,
            12 => 10,
            10 => 8,
            8 => 6,
            6 => 4,
            4 => 2,
            2 => 0,
            _ => 16,
        };
        self.anisotropy_level
    }

    /// Human-friendly display string for Anisotropic Filtering level.
    pub fn anisotropy_display_string(&self) -> String {
        match self.anisotropy_level {
            0 => "Off".to_string(),
            16 => "16x (Max / Ultra)".to_string(),
            val => format!("{}x Anisotropic", val),
        }
    }
}

/// Real-time 3D Viewport Scene running behind the GUI2K4 2D Canvas.
///
/// Simulates the iconic retail `Entry.rom` background level with:
/// - Dynamic 3D camera orbit and subtle breathing sway
/// - Dark dystopian Horzine laboratory / London street mood
/// - Fluorescent light pulse / flicker simulation
/// - Scene uniforms generation for hardware pipelines
#[derive(Clone, Debug)]
pub struct MainMenu3DBackground {
    pub scene: Scene,
    pub camera: Camera,
    pub camera_target: Vec3,
    pub camera_orbit_radius: f32,
    pub camera_orbit_speed: f32,
    pub camera_base_pitch: f32,
    pub fog_color: [f32; 4],
    pub time_seconds: f32,
}

impl Default for MainMenu3DBackground {
    fn default() -> Self {
        Self::new()
    }
}

impl MainMenu3DBackground {
    pub fn new() -> Self {
        let mut camera = Camera::default();
        camera.fov_degrees = 85.0;
        camera.near_clip = 1.0;
        camera.far_clip = 50000.0;

        let target = Vec3::new(0.0, 0.0, 40.0);
        let mut bg = Self {
            scene: Scene::new(),
            camera,
            camera_target: target,
            camera_orbit_radius: 450.0,
            camera_orbit_speed: 0.08,
            camera_base_pitch: -8.0,
            fog_color: [0.08, 0.02, 0.02, 1.0], // Dark dystopian London bloody tint
            time_seconds: 0.0,
        };
        bg.update(0.0);
        bg
    }

    /// Advances the 3D scene time, updates orbital camera position and orientation,
    /// and updates ambient lighting flicker.
    pub fn update(&mut self, delta_time: f32) {
        self.time_seconds += delta_time;
        self.scene.time_seconds = self.time_seconds;

        // Orbit camera around focal point with subtle cinematic breathing sway
        let yaw_rad = self.time_seconds * self.camera_orbit_speed;
        let pitch_deg = self.camera_base_pitch + (self.time_seconds * 0.7).sin() * 1.5;
        let roll_deg = (self.time_seconds * 0.4).cos() * 0.5;

        let x = self.camera_target.x + self.camera_orbit_radius * yaw_rad.cos();
        let y = self.camera_target.y + self.camera_orbit_radius * yaw_rad.sin();
        let z = self.camera_target.z + (self.time_seconds * 0.5).sin() * 15.0;

        let eye = Vec3::new(x, y, z);
        self.camera.position = eye;

        // Direct camera forward towards target
        let to_target = (self.camera_target - eye).normalize_or_zero();
        let yaw_deg = to_target.y.atan2(to_target.x).to_degrees();
        self.camera.update_deg(eye, pitch_deg, yaw_deg, roll_deg);

        // Ambient lighting pulsing (simulating flickering fluorescent laboratory light)
        let flicker = 0.035 + ((self.time_seconds * 4.3).sin() * (self.time_seconds * 11.7).cos()).abs() * 0.025;
        self.scene.ambient_light = [flicker * 1.4, flicker * 0.5, flicker * 0.5, 1.0];
    }

    /// Builds the standard uniform buffer structure for shaders.
    pub fn scene_uniforms(&self, screen_size: [f32; 2]) -> SceneUniforms {
        self.scene.uniforms(&self.camera, screen_size)
    }

    /// Adds a 3D prop or static mesh item to the background scene.
    pub fn add_mesh(&mut self, item: SceneMeshItem) {
        self.scene.add_mesh(item);
    }
}

/// Root interactive Killing Floor Main Menu page.
///
/// Implements [`GUIComponent`] and manages the main button stack, sub-views (Solo,
/// Perks, Settings, Exit Confirmation), sound cues, and 3D background viewport scene.
pub struct MainMenuPage {
    pub id: String,
    pub bounds: Rect,
    pub layout_box: LayoutBox,
    pub state: ComponentState,
    pub visible: bool,
    pub enabled: bool,

    // Active menu screen state
    pub menu_state: MainMenuState,
    pub previous_state: MainMenuState,

    // Primary navigation buttons
    pub buttons: Vec<MainMenuButton>,
    pub hovered_button_id: Option<String>,
    pub pressed_button_id: Option<String>,

    // Sub-view configurations
    pub solo_config: SoloMatchConfig,
    pub perks_info: Vec<PerkProgressInfo>,
    pub selected_perk_index: usize,
    pub settings: MenuSettings,

    // 3D background viewport scene
    pub background_3d: MainMenu3DBackground,

    // Audio cue queue
    pub sound_events: Vec<MenuSoundCue>,

    // Animation & UI transitions
    pub menu_time: f32,
    pub state_transition_alpha: f32,

    // Sub-screen interactive control bounds (recomputed during layout)
    sub_button_bounds: Vec<(String, Rect)>,
}

impl Debug for MainMenuPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MainMenuPage")
            .field("id", &self.id)
            .field("bounds", &self.bounds)
            .field("menu_state", &self.menu_state)
            .field("buttons_count", &self.buttons.len())
            .field("sound_events_count", &self.sound_events.len())
            .finish()
    }
}

impl Default for MainMenuPage {
    fn default() -> Self {
        Self::new()
    }
}

impl MainMenuPage {
    /// Creates a new main menu page with default buttons, perks info, settings, and 3D background.
    pub fn new() -> Self {
        let mut page = Self {
            id: "kf_main_menu".to_string(),
            bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
            layout_box: LayoutBox::relative(0.0, 0.0, 1.0, 1.0),
            state: ComponentState::Normal,
            visible: true,
            enabled: true,
            menu_state: MainMenuState::Main,
            previous_state: MainMenuState::Main,
            buttons: Vec::new(),
            hovered_button_id: None,
            pressed_button_id: None,
            solo_config: SoloMatchConfig::default(),
            perks_info: Self::default_perks_info(),
            selected_perk_index: 0,
            settings: MenuSettings::default(),
            background_3d: MainMenu3DBackground::new(),
            sound_events: Vec::new(),
            menu_time: 0.0,
            state_transition_alpha: 1.0,
            sub_button_bounds: Vec::new(),
        };

        page.init_buttons();
        page.layout_elements();
        page
    }

    /// Initializes default perks data matching Killing Floor retail perk roster.
    fn default_perks_info() -> Vec<PerkProgressInfo> {
        vec![
            PerkProgressInfo::new(
                PerkType::Berserker,
                5,
                42000,
                50000,
                "Melee combat specialist. High damage resistance, faster melee swing, and bloat bile immunity.",
            ),
            PerkProgressInfo::new(
                PerkType::Commando,
                6,
                100000,
                100000,
                "Assault rifle master. Extends Zed-Time, reveals cloaked Stalkers, and displays Zed health bars.",
            ),
            PerkProgressInfo::new(
                PerkType::SupportSpecialist,
                4,
                24500,
                35000,
                "Shotgun lethality and defensive barricades. Increased carry capacity and rapid door welding.",
            ),
            PerkProgressInfo::new(
                PerkType::FieldMedic,
                5,
                78000,
                90000,
                "Combat physician. Rapid healing dart recharge, increased movement speed, and robust body armor.",
            ),
            PerkProgressInfo::new(
                PerkType::Demolitions,
                3,
                12000,
                20000,
                "Explosive ordnance expert. Massive pipe bomb, M79, and LAW damage with explosive resistance.",
            ),
            PerkProgressInfo::new(
                PerkType::Firebug,
                4,
                28000,
                35000,
                "Incendiary weapon connoisseur. Flame thrower range boost, husk fire resistance, and incendiary grenades.",
            ),
            PerkProgressInfo::new(
                PerkType::Sharpshooter,
                6,
                8500,
                8500,
                "Long-range marksman. Devastating headshot multipliers with pistols, lever-action rifles, and crossbows.",
            ),
        ]
    }

    /// Sets up the 5 primary main menu push buttons.
    fn init_buttons(&mut self) {
        self.buttons = vec![
            MainMenuButton::new(
                MenuButtonId::BTN_SOLO,
                "SOLO",
                "Instant action offline survival practice",
                LayoutBox::relative(0.05, 0.28, 0.24, 0.065),
            ),
            MainMenuButton::new(
                MenuButtonId::BTN_MULTIPLAYER,
                "MULTIPLAYER",
                "Browse dedicated & listen game servers",
                LayoutBox::relative(0.05, 0.36, 0.24, 0.065),
            ),
            MainMenuButton::new(
                MenuButtonId::BTN_PERKS,
                "PERKS",
                "Specialization progress, bonuses & achievements",
                LayoutBox::relative(0.05, 0.44, 0.24, 0.065),
            ),
            MainMenuButton::new(
                MenuButtonId::BTN_SETTINGS,
                "SETTINGS",
                "Configure display, audio & mouse input",
                LayoutBox::relative(0.05, 0.52, 0.24, 0.065),
            ),
            MainMenuButton::new(
                MenuButtonId::BTN_EXIT,
                "EXIT",
                "Quit Killing Floor and return to desktop",
                LayoutBox::relative(0.05, 0.60, 0.24, 0.065),
            ),
        ];
    }

    /// Updates internal animation timers, hover fades, and the 3D background viewport scene.
    pub fn update(&mut self, delta_time: f32) {
        self.menu_time += delta_time;

        // Advance 3D background scene & camera orbit
        self.background_3d.update(delta_time);

        // Update button hover animations
        for btn in &mut self.buttons {
            btn.update(delta_time);
        }

        // Smooth state transition alpha
        if self.state_transition_alpha < 1.0 {
            self.state_transition_alpha = (self.state_transition_alpha + delta_time * 5.0).min(1.0);
        }
    }

    /// Transitions the menu to a new state and resets transition animation.
    pub fn set_menu_state(&mut self, new_state: MainMenuState) {
        if self.menu_state != new_state {
            self.previous_state = self.menu_state;
            self.menu_state = new_state;
            self.state_transition_alpha = 0.0;
            self.layout_elements();
        }
    }

    /// Access the underlying 3D background scene.
    pub fn scene(&self) -> &Scene {
        &self.background_3d.scene
    }

    /// Access mutable reference to the underlying 3D background scene.
    pub fn scene_mut(&mut self) -> &mut Scene {
        &mut self.background_3d.scene
    }

    /// Access the 3D viewport camera.
    pub fn camera(&self) -> &Camera {
        &self.background_3d.camera
    }

    /// Access mutable reference to the 3D viewport camera.
    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.background_3d.camera
    }

    /// Builds scene uniforms for 3D shaders.
    pub fn scene_uniforms(&self, screen_size: [f32; 2]) -> SceneUniforms {
        self.background_3d.scene_uniforms(screen_size)
    }

    /// Drains and returns all queued sound cue events.
    pub fn take_sound_events(&mut self) -> Vec<MenuSoundCue> {
        std::mem::take(&mut self.sound_events)
    }

    /// Returns the most recently queued sound cue event.
    pub fn last_sound_event(&self) -> Option<MenuSoundCue> {
        self.sound_events.last().copied()
    }

    /// Checks if a particular sound cue was emitted and is in the queue.
    pub fn has_sound_event(&self, cue: MenuSoundCue) -> bool {
        self.sound_events.contains(&cue)
    }

    /// Activates a button by ID, executes state transitions, emits sound cues,
    /// and returns the resulting [`MainMenuAction`].
    pub fn handle_click(&mut self, button_id: &str) -> MainMenuAction {
        match button_id {
            MenuButtonId::BTN_SOLO => {
                self.sound_events.push(MenuSoundCue::Click);
                self.set_menu_state(MainMenuState::SoloLobby);
                MainMenuAction::StateChanged(MainMenuState::SoloLobby)
            }
            MenuButtonId::BTN_MULTIPLAYER | "btn_server_browser" => {
                self.sound_events.push(MenuSoundCue::Click);
                self.set_menu_state(MainMenuState::ServerBrowser);
                MainMenuAction::OpenServerBrowser
            }
            MenuButtonId::BTN_PERKS => {
                self.sound_events.push(MenuSoundCue::Click);
                self.set_menu_state(MainMenuState::Perks);
                MainMenuAction::OpenPerks
            }
            MenuButtonId::BTN_SETTINGS => {
                self.sound_events.push(MenuSoundCue::Click);
                self.set_menu_state(MainMenuState::Settings);
                MainMenuAction::OpenSettings
            }
            MenuButtonId::BTN_EXIT => {
                self.sound_events.push(MenuSoundCue::Click);
                self.set_menu_state(MainMenuState::ConfirmExit);
                MainMenuAction::StateChanged(MainMenuState::ConfirmExit)
            }
            MenuButtonId::BTN_SOLO_START => {
                self.sound_events.push(MenuSoundCue::Confirm);
                MainMenuAction::StartSolo {
                    map: self.solo_config.current_map().to_string(),
                    difficulty: self.solo_config.difficulty,
                    waves: self.solo_config.wave_count,
                }
            }
            MenuButtonId::BTN_SOLO_BACK => {
                self.sound_events.push(MenuSoundCue::Back);
                self.set_menu_state(MainMenuState::Main);
                MainMenuAction::StateChanged(MainMenuState::Main)
            }
            MenuButtonId::BTN_SOLO_MAP_NEXT => {
                self.solo_config.next_map();
                self.sound_events.push(MenuSoundCue::Select);
                MainMenuAction::None
            }
            MenuButtonId::BTN_SOLO_MAP_PREV => {
                self.solo_config.prev_map();
                self.sound_events.push(MenuSoundCue::Select);
                MainMenuAction::None
            }
            MenuButtonId::BTN_SOLO_DIFF_NEXT => {
                self.solo_config.next_difficulty();
                self.sound_events.push(MenuSoundCue::Select);
                MainMenuAction::None
            }
            MenuButtonId::BTN_SOLO_DIFF_PREV => {
                self.solo_config.prev_difficulty();
                self.sound_events.push(MenuSoundCue::Select);
                MainMenuAction::None
            }
            MenuButtonId::BTN_SOLO_WAVES_CYCLE => {
                self.solo_config.cycle_waves();
                self.sound_events.push(MenuSoundCue::Select);
                MainMenuAction::None
            }
            MenuButtonId::BTN_PERKS_BACK => {
                self.sound_events.push(MenuSoundCue::Back);
                self.set_menu_state(MainMenuState::Main);
                MainMenuAction::StateChanged(MainMenuState::Main)
            }
            MenuButtonId::BTN_SETTINGS_BACK => {
                self.sound_events.push(MenuSoundCue::Back);
                self.set_menu_state(MainMenuState::Main);
                MainMenuAction::StateChanged(MainMenuState::Main)
            }
            MenuButtonId::BTN_SETTINGS_FPS_NEXT | MenuButtonId::BTN_SETTINGS_FPS_CYCLE => {
                let new_fps = self.settings.cycle_fps_next();
                self.sound_events.push(MenuSoundCue::Select);
                MainMenuAction::FpsCapChanged(new_fps)
            }
            MenuButtonId::BTN_SETTINGS_FPS_PREV => {
                let new_fps = self.settings.cycle_fps_prev();
                self.sound_events.push(MenuSoundCue::Select);
                MainMenuAction::FpsCapChanged(new_fps)
            }
            MenuButtonId::BTN_SETTINGS_ANISO_NEXT | MenuButtonId::BTN_SETTINGS_ANISO_CYCLE => {
                let new_aniso = self.settings.cycle_anisotropy_next();
                self.sound_events.push(MenuSoundCue::Select);
                MainMenuAction::AnisotropyChanged(new_aniso)
            }
            MenuButtonId::BTN_SETTINGS_ANISO_PREV => {
                let new_aniso = self.settings.cycle_anisotropy_prev();
                self.sound_events.push(MenuSoundCue::Select);
                MainMenuAction::AnisotropyChanged(new_aniso)
            }
            MenuButtonId::BTN_EXIT_CONFIRM => {
                self.sound_events.push(MenuSoundCue::Confirm);
                MainMenuAction::ExitGame
            }
            MenuButtonId::BTN_EXIT_CANCEL => {
                self.sound_events.push(MenuSoundCue::Back);
                let target = self.previous_state;
                self.set_menu_state(target);
                MainMenuAction::StateChanged(target)
            }
            // Perk row selection clicks: "btn_perk_0", "btn_perk_1", etc.
            perk_id if perk_id.starts_with("btn_perk_") => {
                if let Ok(idx) = perk_id["btn_perk_".len()..].parse::<usize>() {
                    if idx < self.perks_info.len() {
                        self.selected_perk_index = idx;
                        self.sound_events.push(MenuSoundCue::Select);
                    }
                }
                MainMenuAction::None
            }
            _ => MainMenuAction::None,
        }
    }

    /// Recalculates bounding rectangles for all buttons and sub-panels.
    fn layout_elements(&mut self) {
        // Layout primary navigation buttons
        for btn in &mut self.buttons {
            btn.bounds = btn.layout_box.resolve(self.bounds);
        }

        self.sub_button_bounds.clear();

        // Layout sub-screen interactive controls
        match self.menu_state {
            MainMenuState::SoloLobby => {
                let panel = self.sub_panel_rect();
                // Map controls
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SOLO_MAP_PREV.to_string(),
                    Rect::new(panel.x + panel.width - 240.0, panel.y + 70.0, 36.0, 32.0),
                ));
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SOLO_MAP_NEXT.to_string(),
                    Rect::new(panel.x + panel.width - 60.0, panel.y + 70.0, 36.0, 32.0),
                ));
                // Difficulty controls
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SOLO_DIFF_PREV.to_string(),
                    Rect::new(panel.x + panel.width - 240.0, panel.y + 120.0, 36.0, 32.0),
                ));
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SOLO_DIFF_NEXT.to_string(),
                    Rect::new(panel.x + panel.width - 60.0, panel.y + 120.0, 36.0, 32.0),
                ));
                // Wave length cycle button
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SOLO_WAVES_CYCLE.to_string(),
                    Rect::new(panel.x + panel.width - 240.0, panel.y + 170.0, 216.0, 32.0),
                ));
                // Action buttons: Start & Back
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SOLO_START.to_string(),
                    Rect::new(panel.x + panel.width - 160.0, panel.y + panel.height - 56.0, 140.0, 40.0),
                ));
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SOLO_BACK.to_string(),
                    Rect::new(panel.x + 20.0, panel.y + panel.height - 56.0, 120.0, 40.0),
                ));
            }
            MainMenuState::Perks => {
                let panel = self.sub_panel_rect();
                // Perk list rows
                for i in 0..self.perks_info.len() {
                    let row_y = panel.y + 60.0 + (i as f32) * 52.0;
                    self.sub_button_bounds.push((
                        format!("btn_perk_{}", i),
                        Rect::new(panel.x + 20.0, row_y, panel.width * 0.45, 46.0),
                    ));
                }
                // Back button
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_PERKS_BACK.to_string(),
                    Rect::new(panel.x + 20.0, panel.y + panel.height - 56.0, 120.0, 40.0),
                ));
            }
            MainMenuState::Settings => {
                let panel = self.sub_panel_rect();
                // Back button
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SETTINGS_BACK.to_string(),
                    Rect::new(panel.x + 20.0, panel.y + panel.height - 56.0, 120.0, 40.0),
                ));

                // Target Framerate (FPS Cap) row is row index 7
                let fps_row_y = panel.y + 70.0 + 7.0 * 44.0;
                let val_x = panel.x + panel.width - 240.0;
                let val_w = 216.0;
                let val_h = 28.0;

                // Left Arrow (<)
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SETTINGS_FPS_PREV.to_string(),
                    Rect::new(val_x, fps_row_y, 32.0, val_h),
                ));
                // Center Box (Cycle)
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SETTINGS_FPS_CYCLE.to_string(),
                    Rect::new(val_x + 36.0, fps_row_y, val_w - 72.0, val_h),
                ));
                // Right Arrow (>)
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SETTINGS_FPS_NEXT.to_string(),
                    Rect::new(val_x + val_w - 32.0, fps_row_y, 32.0, val_h),
                ));

                // Anisotropic Filtering (AF) row is row index 8
                let aniso_row_y = panel.y + 70.0 + 8.0 * 44.0;

                // Left Arrow (<)
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SETTINGS_ANISO_PREV.to_string(),
                    Rect::new(val_x, aniso_row_y, 32.0, val_h),
                ));
                // Center Box (Cycle)
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SETTINGS_ANISO_CYCLE.to_string(),
                    Rect::new(val_x + 36.0, aniso_row_y, val_w - 72.0, val_h),
                ));
                // Right Arrow (>)
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_SETTINGS_ANISO_NEXT.to_string(),
                    Rect::new(val_x + val_w - 32.0, aniso_row_y, 32.0, val_h),
                ));
            }
            MainMenuState::ConfirmExit => {
                let modal = self.confirm_exit_modal_rect();
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_EXIT_CONFIRM.to_string(),
                    Rect::new(modal.x + modal.width * 0.5 - 130.0, modal.y + modal.height - 54.0, 110.0, 36.0),
                ));
                self.sub_button_bounds.push((
                    MenuButtonId::BTN_EXIT_CANCEL.to_string(),
                    Rect::new(modal.x + modal.width * 0.5 + 20.0, modal.y + modal.height - 54.0, 110.0, 36.0),
                ));
            }
            _ => {}
        }
    }

    /// Sub-panel area for Solo, Perks, and Settings.
    pub fn sub_panel_rect(&self) -> Rect {
        Rect::new(
            self.bounds.x + self.bounds.width * 0.35,
            self.bounds.y + self.bounds.height * 0.18,
            self.bounds.width * 0.60,
            self.bounds.height * 0.72,
        )
    }

    /// Centered rectangle for Exit Confirmation dialog.
    pub fn confirm_exit_modal_rect(&self) -> Rect {
        let width = 480.0;
        let height = 210.0;
        Rect::new(
            self.bounds.x + (self.bounds.width - width) * 0.5,
            self.bounds.y + (self.bounds.height - height) * 0.5,
            width,
            height,
        )
    }

    /// Finds interactive button ID at the given mouse position (x, y).
    fn hit_test_button(&self, x: f32, y: f32) -> Option<String> {
        // If modal dialog is active, it takes absolute input priority
        if self.menu_state == MainMenuState::ConfirmExit {
            for (id, r) in &self.sub_button_bounds {
                if r.contains(x, y) {
                    return Some(id.clone());
                }
            }
            return None;
        }

        // Check sub-buttons if in a sub-view
        for (id, r) in &self.sub_button_bounds {
            if r.contains(x, y) {
                return Some(id.clone());
            }
        }

        // Check main button stack
        for btn in &self.buttons {
            if btn.visible && btn.enabled && btn.bounds.contains(x, y) {
                return Some(btn.id.clone());
            }
        }

        None
    }

    /// Generates modern 2D [`UIVertex`] vertex and index streams for rendering.
    pub fn generate_ui_vertices(&self) -> (Vec<UIVertex>, Vec<u32>) {
        let mut canvas = Canvas::new(self.bounds.width, self.bounds.height);
        self.render(&mut canvas);
        (canvas.vertices, canvas.indices)
    }

    /// Direct render of the 3D background viewport scene to modern hardware [`RenderInterface`].
    pub fn render_3d_background(&self, interface: &mut dyn RenderInterface) -> Result<(), RenderError> {
        if !self.visible {
            return Ok(());
        }

        // Viewport matching full window
        interface.set_viewport(Viewport {
            x: self.bounds.x,
            y: self.bounds.y,
            width: self.bounds.width,
            height: self.bounds.height,
            min_depth: 0.0,
            max_depth: 1.0,
        });

        // Set View and Projection matrices from camera
        interface.set_transform(TransformType::View, self.background_3d.camera.view_matrix());
        interface.set_transform(TransformType::Projection, self.background_3d.camera.projection_matrix());

        // Render each 3D mesh section in the background scene
        for mesh in &self.background_3d.scene.meshes {
            interface.set_transform(TransformType::World, mesh.world_transform);
            interface.set_vertex_buffer(0, mesh.vertex_buffer, 0);
            interface.set_index_buffer(mesh.index_buffer, 0, mesh.index_format);
            interface.draw_indexed_primitive(
                PrimitiveType::TriangleList,
                mesh.section.first_index,
                mesh.section.num_triangles(),
                mesh.section.min_vertex_index,
                mesh.section.max_vertex_index - mesh.section.min_vertex_index + 1,
            );
        }

        Ok(())
    }
}

impl GUIComponent for MainMenuPage {
    fn id(&self) -> &str {
        &self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.layout_box = LayoutBox::absolute(bounds.x, bounds.y, bounds.width, bounds.height);
        self.layout_elements();
    }

    fn layout_box(&self) -> LayoutBox {
        self.layout_box
    }

    fn layout(&mut self, parent_bounds: Rect) {
        self.bounds = self.layout_box.resolve(parent_bounds);
        self.layout_elements();
    }

    fn state(&self) -> ComponentState {
        self.state
    }

    fn set_state(&mut self, state: ComponentState) {
        self.state = state;
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        if !self.visible || !self.enabled {
            return false;
        }

        match *event {
            MouseEvent::Move { x, y } => {
                let hit_btn = self.hit_test_button(x, y);

                // Check for hover transition and trigger hover sound cue
                if hit_btn != self.hovered_button_id {
                    if hit_btn.is_some() {
                        self.sound_events.push(MenuSoundCue::Hover);
                    }
                    self.hovered_button_id = hit_btn.clone();
                }

                // Update hover states on main buttons
                for btn in &mut self.buttons {
                    if Some(&btn.id) == hit_btn.as_ref() {
                        btn.state = ComponentState::Hovered;
                    } else if btn.state == ComponentState::Hovered {
                        btn.state = ComponentState::Normal;
                    }
                }

                self.bounds.contains(x, y)
            }
            MouseEvent::Down { button: MouseButton::Left, x, y } => {
                if let Some(hit_btn) = self.hit_test_button(x, y) {
                    self.pressed_button_id = Some(hit_btn.clone());
                    for btn in &mut self.buttons {
                        if btn.id == hit_btn {
                            btn.state = ComponentState::Pressed;
                        }
                    }
                    true
                } else {
                    false
                }
            }
            MouseEvent::Up { button: MouseButton::Left, x, y } => {
                if let Some(pressed_btn) = self.pressed_button_id.take() {
                    for btn in &mut self.buttons {
                        if btn.state == ComponentState::Pressed {
                            btn.state = if btn.bounds.contains(x, y) {
                                ComponentState::Hovered
                            } else {
                                ComponentState::Normal
                            };
                        }
                    }

                    if let Some(hit_btn) = self.hit_test_button(x, y) {
                        if hit_btn == pressed_btn {
                            self.handle_click(&hit_btn);
                            return true;
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn render(&self, canvas: &mut Canvas) {
        if !self.visible {
            return;
        }

        let b = self.bounds;

        // 1. Dystopian dark backdrop overlay (simulates blood-stained CRT / industrial Horzine terminal)
        let bg_color = [0.03, 0.03, 0.04, 0.82];
        canvas.draw_filled_rect(b, bg_color);

        // 2. Left side banner gradient / dark shadow panel behind button stack
        let left_panel = Rect::new(b.x, b.y, b.width * 0.32, b.height);
        canvas.draw_filled_rect(left_panel, [0.02, 0.02, 0.03, 0.88]);
        canvas.draw_filled_rect(
            Rect::new(left_panel.x + left_panel.width - 2.0, left_panel.y, 2.0, left_panel.height),
            [0.45, 0.12, 0.12, 0.70], // Vertical crimson accent divider
        );

        // 3. Top Header: KILLING FLOOR Logo Banner
        let header = Rect::new(b.x + 40.0, b.y + 40.0, 520.0, 110.0);
        // Crimson blood hazard stripes
        canvas.draw_filled_rect(
            Rect::new(header.x, header.y, 6.0, 80.0),
            [0.85, 0.15, 0.15, 1.0],
        );
        canvas.draw_text(
            "KILLING FLOOR",
            [header.x + 18.0, header.y + 24.0],
            36.0,
            [0.95, 0.20, 0.20, 1.0], // Blood red title
            TextAlignment::Left,
        );
        canvas.draw_text(
            "SURGICAL STRIKE // HORZINE BIOTECH DISINFESTATION",
            [header.x + 20.0, header.y + 64.0],
            11.5,
            [0.75, 0.75, 0.80, 0.85],
            TextAlignment::Left,
        );

        // 4. Render Primary Navigation Buttons
        for btn in &self.buttons {
            if !btn.visible {
                continue;
            }

            let btn_r = btn.bounds;
            let h = btn.hover_anim;

            // Interpolate background color: Normal gunmetal -> Hovered crimson highlight
            let bg_normal = [0.10, 0.12, 0.15, 0.92];
            let bg_hover = [0.35, 0.08, 0.08, 0.98];
            let bg = [
                bg_normal[0] + (bg_hover[0] - bg_normal[0]) * h,
                bg_normal[1] + (bg_hover[1] - bg_normal[1]) * h,
                bg_normal[2] + (bg_hover[2] - bg_normal[2]) * h,
                1.0,
            ];

            canvas.draw_filled_rect(btn_r, bg);

            // Left vertical active indicator strip
            let indicator_w = 4.0 + 4.0 * h;
            let indicator_color = if h > 0.01 {
                [0.95, 0.25, 0.25, 1.0]
            } else {
                [0.35, 0.40, 0.45, 0.8]
            };
            canvas.draw_filled_rect(
                Rect::new(btn_r.x, btn_r.y, indicator_w, btn_r.height),
                indicator_color,
            );

            // Border styling
            let border_color = [
                0.35 + 0.50 * h,
                0.40 - 0.20 * h,
                0.45 - 0.25 * h,
                1.0,
            ];
            canvas.draw_border(btn_r, 1.5, border_color);

            // Button label
            let text_color = if h > 0.5 {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.85, 0.88, 0.92, 1.0]
            };
            canvas.draw_text(
                &btn.label,
                [btn_r.x + 22.0, btn_r.y + 16.0],
                18.0,
                text_color,
                TextAlignment::Left,
            );

            // Subtext
            canvas.draw_text(
                &btn.subtext,
                [btn_r.x + 22.0, btn_r.y + 36.0],
                10.5,
                [0.60, 0.65, 0.70, 0.80],
                TextAlignment::Left,
            );
        }

        // 5. Bottom Status Bar & Build Information
        let footer_y = b.y + b.height - 35.0;
        canvas.draw_text(
            "OpenKF Engine v0.1.0 // Unreal Engine 2.5 Compatible // Vulkan 1.3",
            [b.x + 20.0, footer_y],
            11.0,
            [0.50, 0.55, 0.60, 0.80],
            TextAlignment::Left,
        );
        canvas.draw_text(
            "STATUS: ONLINE | STEAM CONNECTED",
            [b.x + b.width - 20.0, footer_y],
            11.0,
            [0.25, 0.85, 0.35, 0.90], // Green online status
            TextAlignment::Right,
        );

        // 6. Sub-screens rendering
        match self.menu_state {
            MainMenuState::SoloLobby => self.render_solo_lobby(canvas),
            MainMenuState::Perks => self.render_perks_view(canvas),
            MainMenuState::Settings => self.render_settings_view(canvas),
            MainMenuState::ConfirmExit => self.render_confirm_exit_dialog(canvas),
            _ => {}
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl MainMenuPage {
    /// Renders the Solo offline practice lobby setup screen.
    fn render_solo_lobby(&self, canvas: &mut Canvas) {
        let panel = self.sub_panel_rect();

        // Panel background
        canvas.draw_filled_rect(panel, [0.08, 0.10, 0.12, 0.95]);
        canvas.draw_border(panel, 1.5, [0.65, 0.18, 0.18, 1.0]);

        // Panel Title
        let header = Rect::new(panel.x, panel.y, panel.width, 44.0);
        canvas.draw_filled_rect(header, [0.25, 0.05, 0.05, 1.0]);
        canvas.draw_text(
            "SOLO MATCH SETUP // WAVE SURVIVAL PRACTICE",
            [header.x + 20.0, header.y + 22.0],
            15.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Left,
        );

        // Map Selection Row
        let map_y = panel.y + 70.0;
        canvas.draw_text("Map:", [panel.x + 30.0, map_y + 16.0], 14.0, [0.85, 0.85, 0.85, 1.0], TextAlignment::Left);
        let map_val_box = Rect::new(panel.x + panel.width - 240.0, map_y, 216.0, 32.0);
        canvas.draw_filled_rect(map_val_box, [0.04, 0.05, 0.06, 1.0]);
        canvas.draw_border(map_val_box, 1.0, [0.35, 0.40, 0.45, 1.0]);
        canvas.draw_text(
            self.solo_config.current_map(),
            [map_val_box.x + map_val_box.width * 0.5, map_val_box.y + 16.0],
            13.0,
            [0.95, 0.95, 0.95, 1.0],
            TextAlignment::Center,
        );
        // Map arrow buttons (< and >)
        canvas.draw_text("<", [panel.x + panel.width - 222.0, map_y + 16.0], 14.0, [0.9, 0.3, 0.3, 1.0], TextAlignment::Center);
        canvas.draw_text(">", [panel.x + panel.width - 42.0, map_y + 16.0], 14.0, [0.9, 0.3, 0.3, 1.0], TextAlignment::Center);

        // Difficulty Selection Row
        let diff_y = panel.y + 120.0;
        canvas.draw_text("Difficulty:", [panel.x + 30.0, diff_y + 16.0], 14.0, [0.85, 0.85, 0.85, 1.0], TextAlignment::Left);
        let diff_val_box = Rect::new(panel.x + panel.width - 240.0, diff_y, 216.0, 32.0);
        canvas.draw_filled_rect(diff_val_box, [0.04, 0.05, 0.06, 1.0]);
        canvas.draw_border(diff_val_box, 1.0, [0.35, 0.40, 0.45, 1.0]);
        canvas.draw_text(
            self.solo_config.difficulty.as_str(),
            [diff_val_box.x + diff_val_box.width * 0.5, diff_val_box.y + 16.0],
            13.0,
            [0.95, 0.85, 0.35, 1.0], // Gold difficulty text
            TextAlignment::Center,
        );
        canvas.draw_text("<", [panel.x + panel.width - 222.0, diff_y + 16.0], 14.0, [0.9, 0.3, 0.3, 1.0], TextAlignment::Center);
        canvas.draw_text(">", [panel.x + panel.width - 42.0, diff_y + 16.0], 14.0, [0.9, 0.3, 0.3, 1.0], TextAlignment::Center);

        // Game Length Row
        let len_y = panel.y + 170.0;
        canvas.draw_text("Game Length:", [panel.x + 30.0, len_y + 16.0], 14.0, [0.85, 0.85, 0.85, 1.0], TextAlignment::Left);
        let len_val_box = Rect::new(panel.x + panel.width - 240.0, len_y, 216.0, 32.0);
        canvas.draw_filled_rect(len_val_box, [0.04, 0.05, 0.06, 1.0]);
        canvas.draw_border(len_val_box, 1.0, [0.35, 0.40, 0.45, 1.0]);
        canvas.draw_text(
            self.solo_config.wave_length_name(),
            [len_val_box.x + len_val_box.width * 0.5, len_val_box.y + 16.0],
            13.0,
            [0.95, 0.95, 0.95, 1.0],
            TextAlignment::Center,
        );

        // Action Buttons: Start Match & Back
        let btn_start = Rect::new(panel.x + panel.width - 160.0, panel.y + panel.height - 56.0, 140.0, 40.0);
        canvas.draw_filled_rect(btn_start, [0.55, 0.12, 0.12, 1.0]);
        canvas.draw_border(btn_start, 1.5, [0.85, 0.25, 0.25, 1.0]);
        canvas.draw_text(
            "START MATCH",
            [btn_start.x + btn_start.width * 0.5, btn_start.y + 20.0],
            14.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Center,
        );

        let btn_back = Rect::new(panel.x + 20.0, panel.y + panel.height - 56.0, 120.0, 40.0);
        canvas.draw_filled_rect(btn_back, [0.18, 0.20, 0.22, 1.0]);
        canvas.draw_border(btn_back, 1.0, [0.45, 0.48, 0.52, 1.0]);
        canvas.draw_text(
            "BACK",
            [btn_back.x + btn_back.width * 0.5, btn_back.y + 20.0],
            13.0,
            [0.90, 0.90, 0.90, 1.0],
            TextAlignment::Center,
        );
    }

    /// Renders the Perks roster, progress, and bonus statistics screen.
    fn render_perks_view(&self, canvas: &mut Canvas) {
        let panel = self.sub_panel_rect();

        canvas.draw_filled_rect(panel, [0.08, 0.10, 0.12, 0.95]);
        canvas.draw_border(panel, 1.5, [0.65, 0.18, 0.18, 1.0]);

        // Header
        let header = Rect::new(panel.x, panel.y, panel.width, 44.0);
        canvas.draw_filled_rect(header, [0.25, 0.05, 0.05, 1.0]);
        canvas.draw_text(
            "PERK PROGRESSION & SPECIALIZATIONS",
            [header.x + 20.0, header.y + 22.0],
            15.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Left,
        );

        // Perks List (Left Column)
        let list_w = panel.width * 0.45;
        for (i, p) in self.perks_info.iter().enumerate() {
            let row_y = panel.y + 60.0 + (i as f32) * 52.0;
            let row_rect = Rect::new(panel.x + 20.0, row_y, list_w, 46.0);

            let is_selected = i == self.selected_perk_index;
            let row_bg = if is_selected {
                [0.40, 0.12, 0.12, 1.0] // Selected crimson
            } else {
                [0.12, 0.14, 0.16, 0.9]
            };
            canvas.draw_filled_rect(row_rect, row_bg);
            canvas.draw_border(
                row_rect,
                1.0,
                if is_selected { [0.85, 0.30, 0.30, 1.0] } else { [0.30, 0.35, 0.40, 1.0] },
            );

            // Perk display name & Level
            let perk_label = format!("{} (LEVEL {})", p.perk.display_name(), p.level);
            canvas.draw_text(
                &perk_label,
                [row_rect.x + 12.0, row_rect.y + 16.0],
                13.0,
                [1.0, 1.0, 1.0, 1.0],
                TextAlignment::Left,
            );

            // XP progress bar
            let bar_rect = Rect::new(row_rect.x + 12.0, row_rect.y + 28.0, row_rect.width - 24.0, 8.0);
            canvas.draw_filled_rect(bar_rect, [0.04, 0.04, 0.05, 1.0]);
            let filled_w = (bar_rect.width * p.progress_ratio()).max(0.0);
            canvas.draw_filled_rect(
                Rect::new(bar_rect.x, bar_rect.y, filled_w, bar_rect.height),
                [0.85, 0.20, 0.20, 1.0],
            );
        }

        // Perk Details Panel (Right Column)
        if let Some(selected_perk) = self.perks_info.get(self.selected_perk_index) {
            let details_x = panel.x + list_w + 35.0;
            let details_w = panel.width - list_w - 55.0;
            let details_rect = Rect::new(details_x, panel.y + 60.0, details_w, panel.height - 130.0);

            canvas.draw_filled_rect(details_rect, [0.05, 0.06, 0.07, 0.95]);
            canvas.draw_border(details_rect, 1.0, [0.35, 0.40, 0.45, 1.0]);

            canvas.draw_text(
                selected_perk.perk.display_name(),
                [details_x + 16.0, details_rect.y + 24.0],
                18.0,
                [0.95, 0.25, 0.25, 1.0],
                TextAlignment::Left,
            );

            let rank_text = format!("Rank: Level {} / 6", selected_perk.level);
            canvas.draw_text(
                &rank_text,
                [details_x + 16.0, details_rect.y + 54.0],
                13.0,
                [0.90, 0.90, 0.90, 1.0],
                TextAlignment::Left,
            );

            let xp_text = format!("XP: {} / {}", selected_perk.current_xp, selected_perk.next_level_xp);
            canvas.draw_text(
                &xp_text,
                [details_x + 16.0, details_rect.y + 78.0],
                12.0,
                [0.75, 0.80, 0.85, 1.0],
                TextAlignment::Left,
            );

            // Description
            canvas.draw_text(
                &selected_perk.description,
                [details_x + 16.0, details_rect.y + 120.0],
                12.0,
                [0.80, 0.85, 0.90, 0.95],
                TextAlignment::Left,
            );
        }

        // Back button
        let btn_back = Rect::new(panel.x + 20.0, panel.y + panel.height - 56.0, 120.0, 40.0);
        canvas.draw_filled_rect(btn_back, [0.18, 0.20, 0.22, 1.0]);
        canvas.draw_border(btn_back, 1.0, [0.45, 0.48, 0.52, 1.0]);
        canvas.draw_text(
            "BACK",
            [btn_back.x + btn_back.width * 0.5, btn_back.y + 20.0],
            13.0,
            [0.90, 0.90, 0.90, 1.0],
            TextAlignment::Center,
        );
    }

    /// Renders settings (audio, video, controls) screen.
    fn render_settings_view(&self, canvas: &mut Canvas) {
        let panel = self.sub_panel_rect();

        canvas.draw_filled_rect(panel, [0.08, 0.10, 0.12, 0.95]);
        canvas.draw_border(panel, 1.5, [0.65, 0.18, 0.18, 1.0]);

        // Header
        let header = Rect::new(panel.x, panel.y, panel.width, 44.0);
        canvas.draw_filled_rect(header, [0.25, 0.05, 0.05, 1.0]);
        canvas.draw_text(
            "SETTINGS // DISPLAY, AUDIO & CONTROLS",
            [header.x + 20.0, header.y + 22.0],
            15.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Left,
        );

        // Setting entries
        let rows = [
            ("Master Volume", format!("{}%", (self.settings.master_volume * 100.0) as u32)),
            ("SFX Volume", format!("{}%", (self.settings.sfx_volume * 100.0) as u32)),
            ("Music Volume", format!("{}%", (self.settings.music_volume * 100.0) as u32)),
            ("Field of View (FOV)", format!("{:.0}°", self.settings.fov_degrees)),
            ("Mouse Sensitivity", format!("{:.1}", self.settings.mouse_sensitivity)),
            ("Fullscreen Mode", if self.settings.fullscreen { "Enabled" } else { "Windowed" }.to_string()),
            ("Vertical Sync (VSync)", if self.settings.vsync { "Enabled" } else { "Disabled" }.to_string()),
            ("Target Framerate (FPS Cap)", self.settings.fps_display_string()),
            ("Anisotropic Filtering (AF)", self.settings.anisotropy_display_string()),
        ];

        for (i, (label, val)) in rows.iter().enumerate() {
            let row_y = panel.y + 70.0 + (i as f32) * 44.0;
            canvas.draw_text(*label, [panel.x + 30.0, row_y + 14.0], 13.5, [0.85, 0.85, 0.85, 1.0], TextAlignment::Left);

            let val_x = panel.x + panel.width - 240.0;
            let val_w = 216.0;

            if i == 7 {
                // FPS Cap with interactive [<] [  165 FPS  ] [>] buttons
                let left_arrow = Rect::new(val_x, row_y, 32.0, 28.0);
                canvas.draw_filled_rect(left_arrow, [0.15, 0.17, 0.20, 1.0]);
                canvas.draw_border(left_arrow, 1.0, [0.45, 0.48, 0.52, 1.0]);
                canvas.draw_text("<", [left_arrow.x + 16.0, left_arrow.y + 14.0], 13.0, [1.0, 1.0, 1.0, 1.0], TextAlignment::Center);

                let center_box = Rect::new(val_x + 36.0, row_y, val_w - 72.0, 28.0);
                canvas.draw_filled_rect(center_box, [0.04, 0.05, 0.06, 1.0]);
                canvas.draw_border(center_box, 1.0, [0.65, 0.25, 0.25, 1.0]);
                canvas.draw_text(
                    val,
                    [center_box.x + center_box.width * 0.5, center_box.y + 14.0],
                    12.5,
                    [0.35, 0.95, 0.55, 1.0], // Neon green for active FPS cap
                    TextAlignment::Center,
                );

                let right_arrow = Rect::new(val_x + val_w - 32.0, row_y, 32.0, 28.0);
                canvas.draw_filled_rect(right_arrow, [0.15, 0.17, 0.20, 1.0]);
                canvas.draw_border(right_arrow, 1.0, [0.45, 0.48, 0.52, 1.0]);
                canvas.draw_text(">", [right_arrow.x + 16.0, right_arrow.y + 14.0], 13.0, [1.0, 1.0, 1.0, 1.0], TextAlignment::Center);
            } else if i == 8 {
                // Anisotropic Filtering with interactive [<] [  16x (Max)  ] [>] buttons
                let left_arrow = Rect::new(val_x, row_y, 32.0, 28.0);
                canvas.draw_filled_rect(left_arrow, [0.15, 0.17, 0.20, 1.0]);
                canvas.draw_border(left_arrow, 1.0, [0.45, 0.48, 0.52, 1.0]);
                canvas.draw_text("<", [left_arrow.x + 16.0, left_arrow.y + 14.0], 13.0, [1.0, 1.0, 1.0, 1.0], TextAlignment::Center);

                let center_box = Rect::new(val_x + 36.0, row_y, val_w - 72.0, 28.0);
                canvas.draw_filled_rect(center_box, [0.04, 0.05, 0.06, 1.0]);
                canvas.draw_border(center_box, 1.0, [0.25, 0.45, 0.65, 1.0]);
                canvas.draw_text(
                    val,
                    [center_box.x + center_box.width * 0.5, center_box.y + 14.0],
                    12.5,
                    [0.35, 0.80, 0.95, 1.0], // Cyan for anisotropic filtering
                    TextAlignment::Center,
                );

                let right_arrow = Rect::new(val_x + val_w - 32.0, row_y, 32.0, 28.0);
                canvas.draw_filled_rect(right_arrow, [0.15, 0.17, 0.20, 1.0]);
                canvas.draw_border(right_arrow, 1.0, [0.45, 0.48, 0.52, 1.0]);
                canvas.draw_text(">", [right_arrow.x + 16.0, right_arrow.y + 14.0], 13.0, [1.0, 1.0, 1.0, 1.0], TextAlignment::Center);
            } else {
                let val_box = Rect::new(val_x, row_y, val_w, 28.0);
                canvas.draw_filled_rect(val_box, [0.04, 0.05, 0.06, 1.0]);
                canvas.draw_border(val_box, 1.0, [0.35, 0.40, 0.45, 1.0]);
                canvas.draw_text(
                    val,
                    [val_box.x + val_box.width * 0.5, val_box.y + 14.0],
                    12.5,
                    [0.95, 0.85, 0.35, 1.0],
                    TextAlignment::Center,
                );
            }
        }

        // Back button
        let btn_back = Rect::new(panel.x + 20.0, panel.y + panel.height - 56.0, 120.0, 40.0);
        canvas.draw_filled_rect(btn_back, [0.18, 0.20, 0.22, 1.0]);
        canvas.draw_border(btn_back, 1.0, [0.45, 0.48, 0.52, 1.0]);
        canvas.draw_text(
            "BACK",
            [btn_back.x + btn_back.width * 0.5, btn_back.y + 20.0],
            13.0,
            [0.90, 0.90, 0.90, 1.0],
            TextAlignment::Center,
        );
    }

    /// Renders modal confirmation dialog for quitting Killing Floor.
    fn render_confirm_exit_dialog(&self, canvas: &mut Canvas) {
        // Modal dimmer backdrop
        canvas.draw_filled_rect(self.bounds, [0.0, 0.0, 0.0, 0.70]);

        let modal = self.confirm_exit_modal_rect();
        canvas.draw_filled_rect(modal, [0.10, 0.11, 0.13, 0.98]);
        canvas.draw_border(modal, 2.0, [0.75, 0.20, 0.20, 1.0]);

        // Title bar
        let title_bar = Rect::new(modal.x, modal.y, modal.width, 36.0);
        canvas.draw_filled_rect(title_bar, [0.35, 0.08, 0.08, 1.0]);
        canvas.draw_text(
            "QUIT KILLING FLOOR",
            [title_bar.x + title_bar.width * 0.5, title_bar.y + 18.0],
            14.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Center,
        );

        // Body message
        canvas.draw_text(
            "Are you sure you want to quit to desktop?",
            [modal.x + modal.width * 0.5, modal.y + 90.0],
            14.0,
            [0.95, 0.95, 0.95, 1.0],
            TextAlignment::Center,
        );

        // Confirm (Yes) & Cancel (No) Buttons
        let btn_yes = Rect::new(modal.x + modal.width * 0.5 - 130.0, modal.y + modal.height - 54.0, 110.0, 36.0);
        canvas.draw_filled_rect(btn_yes, [0.55, 0.12, 0.12, 1.0]);
        canvas.draw_border(btn_yes, 1.0, [0.85, 0.25, 0.25, 1.0]);
        canvas.draw_text(
            "YES",
            [btn_yes.x + btn_yes.width * 0.5, btn_yes.y + 18.0],
            13.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Center,
        );

        let btn_no = Rect::new(modal.x + modal.width * 0.5 + 20.0, modal.y + modal.height - 54.0, 110.0, 36.0);
        canvas.draw_filled_rect(btn_no, [0.20, 0.22, 0.25, 1.0]);
        canvas.draw_border(btn_no, 1.0, [0.45, 0.50, 0.55, 1.0]);
        canvas.draw_text(
            "NO",
            [btn_no.x + btn_no.width * 0.5, btn_no.y + 18.0],
            13.0,
            [0.90, 0.90, 0.90, 1.0],
            TextAlignment::Center,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_menu_creation_and_defaults() {
        let page = MainMenuPage::new();
        assert_eq!(page.id(), "kf_main_menu");
        assert_eq!(page.menu_state, MainMenuState::Main);
        assert_eq!(page.buttons.len(), 5);
        assert!(page.is_visible());
        assert!(page.is_enabled());

        // Verify primary 5 buttons exist with expected IDs
        let button_ids: Vec<&str> = page.buttons.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(
            button_ids,
            vec![
                MenuButtonId::BTN_SOLO,
                MenuButtonId::BTN_MULTIPLAYER,
                MenuButtonId::BTN_PERKS,
                MenuButtonId::BTN_SETTINGS,
                MenuButtonId::BTN_EXIT,
            ]
        );
    }

    #[test]
    fn test_gui_component_contract_layout_and_bounds() {
        let mut page = MainMenuPage::new();
        let new_bounds = Rect::new(100.0, 50.0, 1280.0, 720.0);
        page.layout(new_bounds);

        assert_eq!(page.bounds(), new_bounds);
        // Verify button bounds were updated relative to new parent
        assert!(page.buttons[0].bounds.width > 0.0);
        assert!(page.buttons[0].bounds.height > 0.0);
        assert!(page.buttons[0].bounds.x >= new_bounds.x);
    }

    #[test]
    fn test_button_hover_sound_cues() {
        let mut page = MainMenuPage::new();
        page.layout(Rect::new(0.0, 0.0, 1920.0, 1080.0));

        let solo_btn = &page.buttons[0];
        let hover_x = solo_btn.bounds.x + 10.0;
        let hover_y = solo_btn.bounds.y + 10.0;

        // Mouse move onto Solo button
        let event = MouseEvent::Move { x: hover_x, y: hover_y };
        let handled = page.handle_mouse_event(&event);
        assert!(handled);
        assert_eq!(page.hovered_button_id, Some(MenuButtonId::BTN_SOLO.to_string()));
        assert_eq!(page.buttons[0].state, ComponentState::Hovered);

        // Sound event should have been emitted
        assert!(page.has_sound_event(MenuSoundCue::Hover));
        let events = page.take_sound_events();
        assert_eq!(events, vec![MenuSoundCue::Hover]);

        // Moving within the same button does NOT trigger duplicate hover sound
        let move_inside = MouseEvent::Move { x: hover_x + 5.0, y: hover_y + 5.0 };
        page.handle_mouse_event(&move_inside);
        assert!(page.take_sound_events().is_empty());
    }

    #[test]
    fn test_handle_click_actions_and_state_transitions() {
        let mut page = MainMenuPage::new();

        // 1. Click Solo -> StateChanged(SoloLobby)
        let action = page.handle_click(MenuButtonId::BTN_SOLO);
        assert_eq!(action, MainMenuAction::StateChanged(MainMenuState::SoloLobby));
        assert_eq!(page.menu_state, MainMenuState::SoloLobby);
        assert_eq!(page.last_sound_event(), Some(MenuSoundCue::Click));

        // 2. Click Start Solo in lobby -> StartSolo action
        let action_start = page.handle_click(MenuButtonId::BTN_SOLO_START);
        assert_eq!(
            action_start,
            MainMenuAction::StartSolo {
                map: "KF-BioticsLab".to_string(),
                difficulty: ServerDifficulty::Normal,
                waves: 7,
            }
        );
        assert_eq!(page.last_sound_event(), Some(MenuSoundCue::Confirm));

        // 3. Click Back in solo lobby -> returns to Main
        let action_back = page.handle_click(MenuButtonId::BTN_SOLO_BACK);
        assert_eq!(action_back, MainMenuAction::StateChanged(MainMenuState::Main));
        assert_eq!(page.menu_state, MainMenuState::Main);

        // 4. Click Multiplayer -> OpenServerBrowser
        let action_mp = page.handle_click(MenuButtonId::BTN_MULTIPLAYER);
        assert_eq!(action_mp, MainMenuAction::OpenServerBrowser);
        assert_eq!(page.menu_state, MainMenuState::ServerBrowser);

        // 5. Click Perks -> OpenPerks
        let action_perks = page.handle_click(MenuButtonId::BTN_PERKS);
        assert_eq!(action_perks, MainMenuAction::OpenPerks);
        assert_eq!(page.menu_state, MainMenuState::Perks);

        // 6. Click Settings -> OpenSettings
        let action_settings = page.handle_click(MenuButtonId::BTN_SETTINGS);
        assert_eq!(action_settings, MainMenuAction::OpenSettings);
        assert_eq!(page.menu_state, MainMenuState::Settings);

        // 7. Click Exit -> ConfirmExit modal
        let action_exit = page.handle_click(MenuButtonId::BTN_EXIT);
        assert_eq!(action_exit, MainMenuAction::StateChanged(MainMenuState::ConfirmExit));
        assert_eq!(page.menu_state, MainMenuState::ConfirmExit);

        // 8. Confirm Exit -> ExitGame
        let action_confirm = page.handle_click(MenuButtonId::BTN_EXIT_CONFIRM);
        assert_eq!(action_confirm, MainMenuAction::ExitGame);
    }

    #[test]
    fn test_solo_config_cycling() {
        let mut page = MainMenuPage::new();
        assert_eq!(page.solo_config.current_map(), "KF-BioticsLab");
        assert_eq!(page.solo_config.difficulty, ServerDifficulty::Normal);
        assert_eq!(page.solo_config.wave_count, 7);

        page.handle_click(MenuButtonId::BTN_SOLO_MAP_NEXT);
        assert_eq!(page.solo_config.current_map(), "KF-WestLondon");

        page.handle_click(MenuButtonId::BTN_SOLO_DIFF_NEXT);
        assert_eq!(page.solo_config.difficulty, ServerDifficulty::Hard);

        page.handle_click(MenuButtonId::BTN_SOLO_WAVES_CYCLE);
        assert_eq!(page.solo_config.wave_count, 10);
    }

    #[test]
    fn test_exit_modal_cancel() {
        let mut page = MainMenuPage::new();
        page.handle_click(MenuButtonId::BTN_EXIT);
        assert_eq!(page.menu_state, MainMenuState::ConfirmExit);

        let action_cancel = page.handle_click(MenuButtonId::BTN_EXIT_CANCEL);
        assert_eq!(action_cancel, MainMenuAction::StateChanged(MainMenuState::Main));
        assert_eq!(page.menu_state, MainMenuState::Main);
    }

    #[test]
    fn test_mouse_click_down_up_workflow() {
        let mut page = MainMenuPage::new();
        page.layout(Rect::new(0.0, 0.0, 1920.0, 1080.0));

        let btn = &page.buttons[0]; // Solo
        let cx = btn.bounds.x + btn.bounds.width * 0.5;
        let cy = btn.bounds.y + btn.bounds.height * 0.5;

        // Down
        let down = MouseEvent::Down { button: MouseButton::Left, x: cx, y: cy };
        assert!(page.handle_mouse_event(&down));
        assert_eq!(page.buttons[0].state, ComponentState::Pressed);

        // Up
        let up = MouseEvent::Up { button: MouseButton::Left, x: cx, y: cy };
        assert!(page.handle_mouse_event(&up));
        assert_eq!(page.menu_state, MainMenuState::SoloLobby);
        assert!(page.has_sound_event(MenuSoundCue::Click));
    }

    #[test]
    fn test_3d_background_viewport_scene_update() {
        let mut page = MainMenuPage::new();
        let initial_cam_pos = page.camera().position;

        // Advance time by 1.5 seconds
        page.update(1.5);

        assert!(page.background_3d.time_seconds > 1.0);
        assert_ne!(page.camera().position, initial_cam_pos);

        // Check scene uniforms generation
        let uniforms = page.scene_uniforms([1920.0, 1080.0]);
        assert_eq!(uniforms.screen_size, [1920.0, 1080.0]);
        assert!(uniforms.time_seconds > 1.0);
    }

    #[test]
    fn test_render_and_ui_vertex_generation() {
        let mut page = MainMenuPage::new();
        page.layout(Rect::new(0.0, 0.0, 1920.0, 1080.0));

        let (verts, indices) = page.generate_ui_vertices();
        assert!(!verts.is_empty());
        assert!(!indices.is_empty());
        // Indices count must be multiple of 3 (triangles)
        assert_eq!(indices.len() % 3, 0);
    }

    #[test]
    fn test_perks_selection_and_progression() {
        let mut page = MainMenuPage::new();
        page.handle_click(MenuButtonId::BTN_PERKS);
        assert_eq!(page.menu_state, MainMenuState::Perks);
        assert_eq!(page.selected_perk_index, 0);

        // Click 2nd perk (Commando)
        page.handle_click("btn_perk_1");
        assert_eq!(page.selected_perk_index, 1);
        assert_eq!(page.last_sound_event(), Some(MenuSoundCue::Select));

        // Click back button
        let back_action = page.handle_click(MenuButtonId::BTN_PERKS_BACK);
        assert_eq!(back_action, MainMenuAction::StateChanged(MainMenuState::Main));
        assert_eq!(page.menu_state, MainMenuState::Main);
    }

    #[test]
    fn test_settings_sub_screen_back() {
        let mut page = MainMenuPage::new();
        page.handle_click(MenuButtonId::BTN_SETTINGS);
        assert_eq!(page.menu_state, MainMenuState::Settings);

        let back_action = page.handle_click(MenuButtonId::BTN_SETTINGS_BACK);
        assert_eq!(back_action, MainMenuAction::StateChanged(MainMenuState::Main));
        assert_eq!(page.menu_state, MainMenuState::Main);
    }

    #[test]
    fn test_settings_fps_cap_cycling() {
        let mut page = MainMenuPage::new();
        page.handle_click(MenuButtonId::BTN_SETTINGS);
        assert_eq!(page.menu_state, MainMenuState::Settings);
        assert_eq!(page.settings.fps_cap, 165);

        // Next: 165 -> 180 -> 220 -> 240 -> 0 (Uncapped) -> 60 -> 120 -> 140 -> 165
        let act1 = page.handle_click(MenuButtonId::BTN_SETTINGS_FPS_NEXT);
        assert_eq!(act1, MainMenuAction::FpsCapChanged(180));
        assert_eq!(page.settings.fps_cap, 180);

        let act2 = page.handle_click(MenuButtonId::BTN_SETTINGS_FPS_CYCLE);
        assert_eq!(act2, MainMenuAction::FpsCapChanged(220));
        assert_eq!(page.settings.fps_cap, 220);

        // Prev: 220 -> 180
        let act3 = page.handle_click(MenuButtonId::BTN_SETTINGS_FPS_PREV);
        assert_eq!(act3, MainMenuAction::FpsCapChanged(180));
        assert_eq!(page.settings.fps_cap, 180);
    }

    #[test]
    fn test_settings_anisotropy_cycling() {
        let mut page = MainMenuPage::new();
        page.handle_click(MenuButtonId::BTN_SETTINGS);
        assert_eq!(page.menu_state, MainMenuState::Settings);
        assert_eq!(page.settings.anisotropy_level, 16);

        // Next: 16 -> 0 (Sans) -> 2 -> 4 -> 6 -> 8 -> 10 -> 12 -> 14 -> 16
        let act1 = page.handle_click(MenuButtonId::BTN_SETTINGS_ANISO_NEXT);
        assert_eq!(act1, MainMenuAction::AnisotropyChanged(0));
        assert_eq!(page.settings.anisotropy_level, 0);

        let act2 = page.handle_click(MenuButtonId::BTN_SETTINGS_ANISO_CYCLE);
        assert_eq!(act2, MainMenuAction::AnisotropyChanged(2));
        assert_eq!(page.settings.anisotropy_level, 2);

        // Prev: 2 -> 0 -> 16
        let act3 = page.handle_click(MenuButtonId::BTN_SETTINGS_ANISO_PREV);
        assert_eq!(act3, MainMenuAction::AnisotropyChanged(0));
        assert_eq!(page.settings.anisotropy_level, 0);

        let act4 = page.handle_click(MenuButtonId::BTN_SETTINGS_ANISO_PREV);
        assert_eq!(act4, MainMenuAction::AnisotropyChanged(16));
        assert_eq!(page.settings.anisotropy_level, 16);
    }

    #[test]
    fn test_modal_precedence_blocks_background_clicks() {
        let mut page = MainMenuPage::new();
        page.layout(Rect::new(0.0, 0.0, 1920.0, 1080.0));

        // Open Exit Modal
        page.handle_click(MenuButtonId::BTN_EXIT);
        assert_eq!(page.menu_state, MainMenuState::ConfirmExit);

        // Click outside the modal on the Solo button location
        let solo_btn = &page.buttons[0];
        let click_x = solo_btn.bounds.x + 10.0;
        let click_y = solo_btn.bounds.y + 10.0;

        let down = MouseEvent::Down { button: MouseButton::Left, x: click_x, y: click_y };
        let handled = page.handle_mouse_event(&down);
        // Modal consumes/blocks clicks to background buttons
        assert!(!handled);
        assert_eq!(page.menu_state, MainMenuState::ConfirmExit);
    }

    #[test]
    fn test_menu_button_id_conversions() {
        let buttons = [
            (MenuButtonId::Solo, MenuButtonId::BTN_SOLO),
            (MenuButtonId::Multiplayer, MenuButtonId::BTN_MULTIPLAYER),
            (MenuButtonId::Perks, MenuButtonId::BTN_PERKS),
            (MenuButtonId::Settings, MenuButtonId::BTN_SETTINGS),
            (MenuButtonId::Exit, MenuButtonId::BTN_EXIT),
            (MenuButtonId::SoloStart, MenuButtonId::BTN_SOLO_START),
            (MenuButtonId::SoloBack, MenuButtonId::BTN_SOLO_BACK),
            (MenuButtonId::ExitConfirm, MenuButtonId::BTN_EXIT_CONFIRM),
            (MenuButtonId::ExitCancel, MenuButtonId::BTN_EXIT_CANCEL),
        ];

        for (id, s) in buttons {
            assert_eq!(id.as_str(), s);
            assert_eq!(MenuButtonId::from_str(s), Some(id));
        }
        assert_eq!(MenuButtonId::from_str("btn_server_browser"), Some(MenuButtonId::Multiplayer));
        assert_eq!(MenuButtonId::from_str("unknown_button"), None);
    }

    #[test]
    fn test_sound_cue_names() {
        assert_eq!(MenuSoundCue::Hover.sound_cue_name(), "KF_MenuSnd.Hover");
        assert_eq!(MenuSoundCue::Click.sound_cue_name(), "KF_MenuSnd.Click");
        assert_eq!(MenuSoundCue::Back.sound_cue_name(), "KF_MenuSnd.Back");
        assert_eq!(MenuSoundCue::Confirm.sound_cue_name(), "KF_MenuSnd.Confirm");
        assert_eq!(MenuSoundCue::Select.sound_cue_name(), "KF_MenuSnd.Select");
        assert_eq!(MenuSoundCue::Slide.sound_cue_name(), "KF_MenuSnd.Slide");
    }
}
