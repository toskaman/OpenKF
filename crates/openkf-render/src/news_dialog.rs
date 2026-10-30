//! In-Game Modal NEWS and Patchlog Viewer for OpenKF / GUI2K4
//!
//! Provides a modern, resolution-independent modal dialog implementing [`crate::gui::GUIComponent`]
//! for displaying release notes, patchlogs, and system updates:
//! - Section parser for standard retail / OpenKF `NEWS.txt` formatted documents.
//! - Categorized feature cards with metallic Unreal Engine 2.5 GUI2K4 styling.
//! - High-visibility version badges (e.g. Engine Version, x86_64, Vulkan/D3D12, VAC-Safe).
//! - Executive summary comparison table rendering (Vanilla 2009 vs OpenKF Client).
//! - Smooth scrollable content viewport with interactive scrollbar and mouse-wheel support.
//! - Interactive close buttons (Header X and bottom action bar) with state tracking.

use std::any::Any;
use std::fmt::Debug;
use crate::gui::{
    Canvas, ComponentState, GUIComponent, LayoutBox, MouseButton, MouseEvent, Rect,
    TextAlignment,
};

/// High-level categorization for patchlog feature cards.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FeatureCategory {
    Graphics,
    CanvasUI,
    Steamworks,
    RawInput,
    Enhancements,
    Audio,
    Networking,
    General,
}

impl FeatureCategory {
    /// Returns the human-readable display string for this category.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Graphics => "RENDER HAL / 3D",
            Self::CanvasUI => "GUI2K4 / UI",
            Self::Steamworks => "STEAMWORKS 64",
            Self::RawInput => "RAW INPUT / AIM",
            Self::Enhancements => "ENHANCEMENTS",
            Self::Audio => "AUDIO / SFX",
            Self::Networking => "NETWORKING",
            Self::General => "ENGINE / CORE",
        }
    }

    /// Color tint for category badge: [R, G, B, A].
    pub fn badge_color(&self) -> [f32; 4] {
        match self {
            Self::Graphics => [0.15, 0.40, 0.65, 0.95],      // Sapphire blue
            Self::CanvasUI => [0.55, 0.20, 0.55, 0.95],      // Purple
            Self::Steamworks => [0.10, 0.50, 0.35, 0.95],    // Steam emerald green
            Self::RawInput => [0.70, 0.35, 0.10, 0.95],      // Amber orange
            Self::Enhancements => [0.65, 0.15, 0.20, 0.95],  // KF Crimson
            Self::Audio => [0.20, 0.55, 0.55, 0.95],         // Teal
            Self::Networking => [0.25, 0.35, 0.70, 0.95],    // Royal blue
            Self::General => [0.35, 0.35, 0.38, 0.95],       // Steel gray
        }
    }

    /// Infers category from a section title string.
    pub fn infer(title: &str) -> Self {
        let lower = title.to_ascii_lowercase();
        if lower.contains("render") || lower.contains("viewport") || lower.contains("vulkan") || lower.contains("d3d") || lower.contains("bsp") || lower.contains("mesh") {
            Self::Graphics
        } else if lower.contains("canvas") || lower.contains("gui") || lower.contains("hud") || lower.contains("browser") || lower.contains("menu") {
            Self::CanvasUI
        } else if lower.contains("steam") || lower.contains("achievement") || lower.contains("friend") || lower.contains("cloud") {
            Self::Steamworks
        } else if lower.contains("mouse") || lower.contains("input") || lower.contains("aim") || lower.contains("sensor") || lower.contains("raw") {
            Self::RawInput
        } else if lower.contains("anisotropic") || lower.contains("enhancement") || lower.contains("graphic") || lower.contains("visual") {
            Self::Enhancements
        } else if lower.contains("audio") || lower.contains("sound") || lower.contains("voice") {
            Self::Audio
        } else if lower.contains("network") || lower.contains("server") || lower.contains("packet") || lower.contains("netcode") {
            Self::Networking
        } else {
            Self::General
        }
    }
}

/// A parsed section from `NEWS.txt` displayed as an interactive feature card.
#[derive(Clone, Debug, PartialEq)]
pub struct NewsSection {
    /// Unique 0-based index of this section.
    pub id: usize,
    /// Section heading (e.g. "1. VIEWPORT 3D & MODERN RENDER HAL (Vulkan 1.3 / D3D12)").
    pub title: String,
    /// Inferred category.
    pub category: FeatureCategory,
    /// Optional prominent badge string.
    pub badge: Option<String>,
    /// Summary or introductory text lines.
    pub paragraphs: Vec<String>,
    /// Parsed bullet points detailing specific features.
    pub bullet_points: Vec<String>,
    /// Raw unparsed lines for fallback rendering.
    pub raw_lines: Vec<String>,
}

impl NewsSection {
    pub fn new(id: usize, title: impl Into<String>) -> Self {
        let t = title.into();
        let cat = FeatureCategory::infer(&t);
        Self {
            id,
            title: t,
            category: cat,
            badge: None,
            paragraphs: Vec::new(),
            bullet_points: Vec::new(),
            raw_lines: Vec::new(),
        }
    }
}

/// A single row in the Vanilla vs OpenKF Executive Summary table.
#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonRow {
    pub feature: String,
    pub vanilla: String,
    pub openkf: String,
}

/// Top-level header metadata parsed from `NEWS.txt`.
#[derive(Clone, Debug, PartialEq)]
pub struct NewsHeader {
    pub title: String,
    pub subtitle: String,
    pub project: String,
    pub version: String,
    pub edition: String,
    pub compatibility: String,
    pub badges: Vec<String>,
}

impl Default for NewsHeader {
    fn default() -> Self {
        Self {
            title: "OPENKF CLIENT - NEWS & COMPREHENSIVE CHANGELOG".to_string(),
            subtitle: "Modern 64-Bit Engine Client & Graphical Enhancements for Killing Floor".to_string(),
            project: "OpenKF (Killing Floor v1065 / Unreal Engine 2.5)".to_string(),
            version: "v1.2.0".to_string(),
            edition: "October 2026 Edition".to_string(),
            compatibility: "100% Steam Retail Multiplayer & VAC Safe".to_string(),
            badges: vec![
                "v1.2.0".to_string(),
                "64-Bit x86_64".to_string(),
                "Vulkan 1.3 / D3D12".to_string(),
                "VAC Safe".to_string(),
                "8000 Hz Input".to_string(),
            ],
        }
    }
}

/// Structured document parsed from `NEWS.txt`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewsDocument {
    pub header: NewsHeader,
    pub comparison_rows: Vec<ComparisonRow>,
    pub sections: Vec<NewsSection>,
}

impl NewsDocument {
    /// Parses the raw text of `NEWS.txt` into structured sections and metadata.
    pub fn parse(text: &str) -> Self {
        let mut doc = NewsDocument::default();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        let mut current_section: Option<NewsSection> = None;
        let mut in_table = false;
        let mut section_counter = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Detect separator lines
            if line.starts_with("====") || line.starts_with("----") {
                i += 1;
                continue;
            }

            // Parse header key-values
            if line.starts_with("Project:") {
                doc.header.project = line.trim_start_matches("Project:").trim().to_string();
                i += 1;
                continue;
            }
            if line.starts_with("Current Version:") {
                let v_str = line.trim_start_matches("Current Version:").trim();
                // Extract version tag e.g. "v1.2.0" and edition e.g. "(October 2026 Edition)"
                if let Some(paren_idx) = v_str.find('(') {
                    doc.header.version = v_str[..paren_idx].trim().to_string();
                    let edition_part = v_str[paren_idx..].trim_matches(|c| c == '(' || c == ')');
                    doc.header.edition = edition_part.trim().to_string();
                } else {
                    doc.header.version = v_str.to_string();
                }
                if !doc.header.version.is_empty() && !doc.header.badges.contains(&doc.header.version) {
                    doc.header.badges.insert(0, doc.header.version.clone());
                }
                i += 1;
                continue;
            }
            if line.starts_with("Compatibility:") {
                doc.header.compatibility = line.trim_start_matches("Compatibility:").trim().to_string();
                i += 1;
                continue;
            }

            // Detect Executive Summary comparison table
            if line.contains("EXECUTIVE SUMMARY:") || (line.contains("FEATURE") && line.contains("VANILLA") && line.contains("OPENKF")) {
                in_table = true;
                i += 1;
                continue;
            }

            if in_table {
                if line.starts_with("CORE TECHNICAL DELIVERABLES") || (line.len() > 3 && line.chars().next().is_some_and(|c| c.is_ascii_digit()) && line.contains('.')) {
                    in_table = false;
                    // Fallthrough to section parsing
                } else if line.contains('|') {
                    let parts: Vec<&str> = line.split('|').collect();
                    if parts.len() >= 3 {
                        let col0 = parts[0].trim();
                        let col1 = parts[1].trim();
                        let col2 = parts[2].trim();
                        if !col0.is_empty() && !col0.starts_with("FEATURE") && !col0.starts_with("---") {
                            doc.comparison_rows.push(ComparisonRow {
                                feature: col0.to_string(),
                                vanilla: col1.to_string(),
                                openkf: col2.to_string(),
                            });
                        }
                    }
                    i += 1;
                    continue;
                } else if line.is_empty() || line.starts_with("---") {
                    i += 1;
                    continue;
                }
            }

            // Detect numbered section start (e.g. "1. VIEWPORT 3D...", "2. CANVAS 2D...")
            let is_numbered_section = if let Some(first_char) = line.chars().next() {
                first_char.is_ascii_digit() && line.contains(". ")
            } else {
                false
            };

            if is_numbered_section {
                if let Some(prev) = current_section.take() {
                    doc.sections.push(prev);
                }
                section_counter += 1;
                let mut sec = NewsSection::new(section_counter, line);
                if let Some(start_paren) = line.find('(') {
                    if let Some(end_paren) = line.rfind(')') {
                        if end_paren > start_paren {
                            sec.badge = Some(line[start_paren + 1..end_paren].trim().to_string());
                        }
                    }
                }
                current_section = Some(sec);
                i += 1;
                continue;
            }

            // Accumulate content into current active section
            if let Some(ref mut sec) = current_section {
                if !line.is_empty() && !line.starts_with("---") {
                    sec.raw_lines.push(line.to_string());
                    if line.starts_with('*') || line.starts_with('-') || line.starts_with('•') {
                        let clean = line.trim_start_matches(['*', '-', '•']).trim();
                        sec.bullet_points.push(clean.to_string());
                    } else if !line.starts_with("---") && !line.starts_with("===") {
                        sec.paragraphs.push(line.to_string());
                    }
                }
            }

            i += 1;
        }

        if let Some(last) = current_section {
            doc.sections.push(last);
        }

        // If no sections were parsed (e.g. arbitrary plain text), create a fallback general section
        if doc.sections.is_empty() && !text.trim().is_empty() {
            let mut fallback = NewsSection::new(1, "General Release Notes");
            for l in text.lines() {
                let trimmed = l.trim();
                if !trimmed.is_empty() {
                    fallback.raw_lines.push(trimmed.to_string());
                    if trimmed.starts_with('*') || trimmed.starts_with('-') {
                        fallback.bullet_points.push(trimmed.trim_start_matches(['*', '-']).trim().to_string());
                    } else {
                        fallback.paragraphs.push(trimmed.to_string());
                    }
                }
            }
            doc.sections.push(fallback);
        }

        doc
    }
}

/// In-Game Modal NEWS and Patchlog Viewer Dialog Page.
///
/// Implements [`GUIComponent`] for seamless integration into [`crate::gui::GuiController`].
pub struct NewsDialogPage {
    pub id: String,
    pub bounds: Rect,
    pub layout_box: LayoutBox,
    pub state: ComponentState,
    pub visible: bool,
    pub enabled: bool,
    pub is_open: bool,
    pub document: NewsDocument,
    pub scroll_offset_y: f32,
    pub max_scroll_y: f32,
    pub content_height: f32,
    pub selected_section: Option<usize>,
    pub close_button_hovered: bool,
    pub footer_close_hovered: bool,
    pub scrollbar_hovered: bool,
    pub scrollbar_dragging: bool,
    pub drag_start_y: f32,
    pub drag_start_scroll: f32,
}

impl Debug for NewsDialogPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewsDialogPage")
            .field("id", &self.id)
            .field("bounds", &self.bounds)
            .field("is_open", &self.is_open)
            .field("visible", &self.visible)
            .field("sections_count", &self.document.sections.len())
            .field("scroll_offset_y", &self.scroll_offset_y)
            .field("max_scroll_y", &self.max_scroll_y)
            .finish()
    }
}

impl Default for NewsDialogPage {
    fn default() -> Self {
        Self::with_bounds(Rect::new(0.0, 0.0, 1024.0, 768.0))
    }
}

impl NewsDialogPage {
    /// Creates a new news dialog with default dimensions.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), ..Default::default() }
    }

    /// Creates a news dialog initialized with specific screen/viewport bounds.
    pub fn with_bounds(bounds: Rect) -> Self {
        let mut page = Self {
            id: "NewsDialogPage".to_string(),
            bounds,
            layout_box: LayoutBox::relative(0.05, 0.05, 0.90, 0.90),
            state: ComponentState::Normal,
            visible: true,
            enabled: true,
            is_open: true,
            document: NewsDocument::default(),
            scroll_offset_y: 0.0,
            max_scroll_y: 0.0,
            content_height: 0.0,
            selected_section: None,
            close_button_hovered: false,
            footer_close_hovered: false,
            scrollbar_hovered: false,
            scrollbar_dragging: false,
            drag_start_y: 0.0,
            drag_start_scroll: 0.0,
        };
        page.recalculate_layout();
        page
    }

    /// Builder method to load news text immediately upon creation.
    pub fn with_news_text(mut self, text: &str) -> Self {
        self.load_news_text(text);
        self
    }

    /// Parses and loads a new NEWS/patchlog text document.
    pub fn load_news_text(&mut self, text: &str) {
        self.document = NewsDocument::parse(text);
        self.scroll_offset_y = 0.0;
        self.recalculate_layout();
    }

    /// Scrolls the viewport by `delta` units.
    ///
    /// Positive delta scrolls upward (towards top of document).
    /// Negative delta scrolls downward (towards bottom of document).
    pub fn scroll(&mut self, delta: f32) {
        let scroll_step = 40.0;
        let new_offset = self.scroll_offset_y - delta * scroll_step;
        self.scroll_offset_y = new_offset.clamp(0.0, self.max_scroll_y);
    }

    /// Sets explicit scroll offset in pixels, clamped to [0.0, max_scroll_y].
    pub fn set_scroll_offset(&mut self, offset: f32) {
        self.scroll_offset_y = offset.clamp(0.0, self.max_scroll_y);
    }

    /// Current scroll offset in pixels.
    pub fn scroll_offset(&self) -> f32 {
        self.scroll_offset_y
    }

    /// Maximum available scroll distance in pixels.
    pub fn max_scroll(&self) -> f32 {
        self.max_scroll_y
    }

    /// Scrolls to the very top.
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset_y = 0.0;
    }

    /// Scrolls to the bottom.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset_y = self.max_scroll_y;
    }

    /// Opens the dialog and resets scroll offset to top.
    pub fn open(&mut self) {
        self.is_open = true;
        self.visible = true;
        self.scroll_offset_y = 0.0;
    }

    /// Closes the modal dialog.
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Checks if the modal dialog is currently open.
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Sets modal open/closed state.
    pub fn set_open(&mut self, open: bool) {
        self.is_open = open;
    }

    /// Returns the parsed sections.
    pub fn sections(&self) -> &[NewsSection] {
        &self.document.sections
    }

    /// Returns the active engine version tag.
    pub fn version(&self) -> &str {
        &self.document.header.version
    }

    /// Returns the parsed version badges.
    pub fn badges(&self) -> &[String] {
        &self.document.header.badges
    }

    /// Returns reference to parsed header metadata.
    pub fn header(&self) -> &NewsHeader {
        &self.document.header
    }

    /// Returns the Executive Summary comparison rows.
    pub fn comparison_rows(&self) -> &[ComparisonRow] {
        &self.document.comparison_rows
    }

    // --- Layout Geometry Calculations ---

    /// Rectangle of the main modal dialog window (centered with margins).
    pub fn dialog_rect(&self) -> Rect {
        let margin_x = (self.bounds.width * 0.06).max(24.0);
        let margin_y = (self.bounds.height * 0.05).max(20.0);
        Rect::new(
            self.bounds.x + margin_x,
            self.bounds.y + margin_y,
            self.bounds.width - 2.0 * margin_x,
            self.bounds.height - 2.0 * margin_y,
        )
    }

    /// Top title header rectangle.
    pub fn header_rect(&self) -> Rect {
        let dlg = self.dialog_rect();
        Rect::new(dlg.x, dlg.y, dlg.width, 42.0)
    }

    /// Top-right close [X] button rectangle.
    pub fn close_button_rect(&self) -> Rect {
        let hdr = self.header_rect();
        let btn_size = 30.0;
        Rect::new(hdr.x + hdr.width - btn_size - 6.0, hdr.y + 6.0, btn_size, btn_size)
    }

    /// Version badges bar rectangle below title.
    pub fn badges_rect(&self) -> Rect {
        let dlg = self.dialog_rect();
        Rect::new(dlg.x + 12.0, dlg.y + 46.0, dlg.width - 24.0, 32.0)
    }

    /// Bottom action/footer toolbar rectangle.
    pub fn footer_rect(&self) -> Rect {
        let dlg = self.dialog_rect();
        let footer_height = 44.0;
        Rect::new(dlg.x, dlg.y + dlg.height - footer_height, dlg.width, footer_height)
    }

    /// Bottom-right action button rectangle ("Close").
    pub fn footer_close_button_rect(&self) -> Rect {
        let ftr = self.footer_rect();
        let btn_w = 110.0;
        let btn_h = 30.0;
        Rect::new(ftr.x + ftr.width - btn_w - 12.0, ftr.y + 7.0, btn_w, btn_h)
    }

    /// Scrollable content viewport area.
    pub fn content_rect(&self) -> Rect {
        let dlg = self.dialog_rect();
        let top = dlg.y + 84.0;
        let bottom = dlg.y + dlg.height - 48.0;
        let scrollbar_width = 16.0;
        Rect::new(dlg.x + 12.0, top, dlg.width - 24.0 - scrollbar_width, (bottom - top).max(10.0))
    }

    /// Vertical scrollbar track rectangle.
    pub fn scrollbar_rect(&self) -> Rect {
        let content = self.content_rect();
        let scrollbar_width = 12.0;
        Rect::new(content.x + content.width + 4.0, content.y, scrollbar_width, content.height)
    }

    /// Vertical scrollbar thumb/handle rectangle.
    pub fn scrollbar_thumb_rect(&self) -> Rect {
        let track = self.scrollbar_rect();
        if self.max_scroll_y <= 0.0 || self.content_height <= 0.0 {
            return Rect::new(track.x, track.y, track.width, track.height);
        }

        let visible_ratio = (track.height / self.content_height).clamp(0.08, 1.0);
        let thumb_height = (track.height * visible_ratio).max(24.0);
        let available_track = (track.height - thumb_height).max(0.0);
        let scroll_fraction = (self.scroll_offset_y / self.max_scroll_y).clamp(0.0, 1.0);
        let thumb_y = track.y + scroll_fraction * available_track;

        Rect::new(track.x, thumb_y, track.width, thumb_height)
    }

    /// Recalculates total content virtual height and maximum scroll limit.
    pub fn recalculate_layout(&mut self) {
        let mut total_h = 20.0; // Top padding

        // Title and Project description banner
        total_h += 50.0;

        // Executive Summary table if present
        if !self.document.comparison_rows.is_empty() {
            total_h += 40.0; // Table header
            total_h += (self.document.comparison_rows.len() as f32) * 22.0; // Rows
            total_h += 30.0; // Margin
        }

        // Feature cards
        for sec in &self.document.sections {
            total_h += 36.0; // Card header bar
            // Paragraph lines
            total_h += (sec.paragraphs.len() as f32) * 18.0;
            // Bullet points
            total_h += (sec.bullet_points.len() as f32) * 22.0;
            // Raw lines fallback if neither paragraphs nor bullets exist
            if sec.paragraphs.is_empty() && sec.bullet_points.is_empty() {
                total_h += (sec.raw_lines.len() as f32) * 18.0;
            }
            total_h += 24.0; // Card spacing
        }

        total_h += 40.0; // Bottom padding

        self.content_height = total_h;
        let view_h = self.content_rect().height;
        self.max_scroll_y = (total_h - view_h).max(0.0);
        self.scroll_offset_y = self.scroll_offset_y.clamp(0.0, self.max_scroll_y);
    }

    // --- Input & Event Handling ---

    /// Dispatches mouse clicks.
    pub fn handle_click(&mut self, x: f32, y: f32) -> bool {
        if !self.is_open || !self.visible {
            return false;
        }

        // 1. Top-Right Close Button
        if self.close_button_rect().contains(x, y) {
            self.close();
            return true;
        }

        // 2. Footer Close Button
        if self.footer_close_button_rect().contains(x, y) {
            self.close();
            return true;
        }

        // 3. Scrollbar track click
        let track = self.scrollbar_rect();
        if track.contains(x, y) {
            let thumb = self.scrollbar_thumb_rect();
            if thumb.contains(x, y) {
                self.scrollbar_dragging = true;
                self.drag_start_y = y;
                self.drag_start_scroll = self.scroll_offset_y;
            } else if y < thumb.y {
                self.scroll(3.0); // Page up
            } else {
                self.scroll(-3.0); // Page down
            }
            return true;
        }

        // 4. Modal Dialog consumes any clicks inside its bounds
        let dlg = self.dialog_rect();
        if dlg.contains(x, y) {
            // Check if user clicked a feature card header to toggle/select
            let content = self.content_rect();
            if content.contains(x, y) {
                // Future interactive sub-elements
            }
            return true;
        }

        // 5. Clicking outside dialog on modal backdrop closes or blocks click
        // For retail UE2/GUI2K4 modal dialogs, clicks outside consume and dismiss dialog
        if self.bounds.contains(x, y) {
            self.close();
            return true;
        }

        false
    }

    // --- Rendering Pipeline ---

    /// Renders modal background, dialog frame, version badges, feature cards, and scrollbar.
    pub fn render_canvas(&self, canvas: &mut Canvas) {
        if !self.is_open || !self.visible {
            return;
        }

        // 1. Fullscreen Dark Semi-Transparent Backdrop (Modal Dimmer)
        canvas.draw_filled_rect(self.bounds, [0.02, 0.03, 0.05, 0.82]);

        // 2. Dialog Main Window (KF Metallic Charcoal / Dark Slate)
        let dlg = self.dialog_rect();
        canvas.draw_filled_rect(dlg, [0.08, 0.09, 0.11, 0.98]);
        canvas.draw_border(dlg, 2.0, [0.55, 0.14, 0.14, 1.0]); // Crimson border

        // 3. Dialog Title Bar Header (Deep KF Crimson)
        let hdr = self.header_rect();
        canvas.draw_filled_rect(hdr, [0.24, 0.05, 0.05, 1.0]);
        canvas.draw_border(hdr, 1.0, [0.65, 0.20, 0.20, 0.8]);

        // Header Title Text
        canvas.draw_text(
            &self.document.header.title,
            [hdr.x + 16.0, hdr.y + hdr.height * 0.5],
            15.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Left,
        );

        // Header Close [X] Button
        let close_btn = self.close_button_rect();
        let close_bg = if self.close_button_hovered {
            [0.85, 0.15, 0.15, 1.0] // Bright red hovered
        } else {
            [0.45, 0.10, 0.10, 0.9] // Dark red normal
        };
        canvas.draw_filled_rect(close_btn, close_bg);
        canvas.draw_border(close_btn, 1.0, [0.9, 0.4, 0.4, 1.0]);
        canvas.draw_text(
            "X",
            [close_btn.x + close_btn.width * 0.5, close_btn.y + close_btn.height * 0.5],
            13.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Center,
        );

        // 4. Version Badges Row
        let badges_box = self.badges_rect();
        canvas.draw_filled_rect(badges_box, [0.12, 0.13, 0.16, 0.9]);
        canvas.draw_border(badges_box, 1.0, [0.25, 0.28, 0.35, 0.8]);

        let mut badge_x = badges_box.x + 8.0;
        for badge_text in &self.document.header.badges {
            let badge_w = (badge_text.len() as f32 * 7.5 + 16.0).max(65.0);
            let b_rect = Rect::new(badge_x, badges_box.y + 4.0, badge_w, badges_box.height - 8.0);
            
            // Badge background pill
            let is_version = badge_text.starts_with('v') || badge_text.contains("1.2");
            let is_vac = badge_text.contains("VAC");
            let bg = if is_version {
                [0.18, 0.45, 0.25, 0.95] // Emerald green for version
            } else if is_vac {
                [0.15, 0.35, 0.65, 0.95] // Valve blue for VAC Safe
            } else {
                [0.25, 0.28, 0.35, 0.95] // Steel blue
            };

            canvas.draw_filled_rect(b_rect, bg);
            canvas.draw_border(b_rect, 1.0, [0.6, 0.7, 0.8, 0.8]);
            canvas.draw_text(
                badge_text,
                [b_rect.x + b_rect.width * 0.5, b_rect.y + b_rect.height * 0.5],
                11.0,
                [1.0, 1.0, 1.0, 1.0],
                TextAlignment::Center,
            );

            badge_x += badge_w + 8.0;
        }

        // 5. Scrollable Content Area Rendering
        let content = self.content_rect();
        canvas.draw_filled_rect(content, [0.06, 0.07, 0.09, 0.95]);
        canvas.draw_border(content, 1.0, [0.20, 0.22, 0.26, 1.0]);

        // Virtual rendering with scroll clipping bounds
        let mut cur_y = content.y + 12.0 - self.scroll_offset_y;
        let clip_top = content.y;
        let clip_bottom = content.y + content.height;

        // Subtitle & Project Banner
        if cur_y + 40.0 >= clip_top && cur_y <= clip_bottom {
            canvas.draw_text(
                format!("Project: {} | {}", self.document.header.project, self.document.header.edition),
                [content.x + 12.0, cur_y],
                13.0,
                [0.85, 0.85, 0.90, 1.0],
                TextAlignment::Left,
            );
            canvas.draw_text(
                format!("Compatibility: {}", self.document.header.compatibility),
                [content.x + 12.0, cur_y + 18.0],
                12.0,
                [0.35, 0.85, 0.45, 1.0], // Soft green for compatibility
                TextAlignment::Left,
            );
        }
        cur_y += 50.0;

        // Executive Summary Comparison Table
        if !self.document.comparison_rows.is_empty() {
            let table_header_y = cur_y;
            if table_header_y + 24.0 >= clip_top && table_header_y <= clip_bottom {
                let th_rect = Rect::new(content.x + 8.0, table_header_y, content.width - 16.0, 22.0);
                canvas.draw_filled_rect(th_rect, [0.18, 0.20, 0.25, 0.95]);
                canvas.draw_border(th_rect, 1.0, [0.35, 0.40, 0.50, 0.8]);

                let col_w1 = (th_rect.width * 0.32).max(120.0);
                let col_w2 = (th_rect.width * 0.34).max(120.0);
                canvas.draw_text("FEATURE / SUBSYSTEM", [th_rect.x + 8.0, th_rect.y + 11.0], 11.0, [1.0, 1.0, 0.7, 1.0], TextAlignment::Left);
                canvas.draw_text("VANILLA CLIENT (2009)", [th_rect.x + col_w1 + 8.0, th_rect.y + 11.0], 11.0, [0.75, 0.75, 0.75, 1.0], TextAlignment::Left);
                canvas.draw_text("OPENKF CLIENT v1.2.0", [th_rect.x + col_w1 + col_w2 + 8.0, th_rect.y + 11.0], 11.0, [0.4, 0.9, 0.5, 1.0], TextAlignment::Left);
            }
            cur_y += 24.0;

            for (idx, row) in self.document.comparison_rows.iter().enumerate() {
                if cur_y + 20.0 >= clip_top && cur_y <= clip_bottom {
                    let row_rect = Rect::new(content.x + 8.0, cur_y, content.width - 16.0, 20.0);
                    let row_color = if idx % 2 == 0 {
                        [0.10, 0.11, 0.14, 0.85]
                    } else {
                        [0.08, 0.09, 0.11, 0.85]
                    };
                    canvas.draw_filled_rect(row_rect, row_color);

                    let col_w1 = (row_rect.width * 0.32).max(120.0);
                    let col_w2 = (row_rect.width * 0.34).max(120.0);
                    canvas.draw_text(&row.feature, [row_rect.x + 8.0, row_rect.y + 10.0], 10.5, [0.9, 0.9, 0.9, 1.0], TextAlignment::Left);
                    canvas.draw_text(&row.vanilla, [row_rect.x + col_w1 + 8.0, row_rect.y + 10.0], 10.5, [0.7, 0.7, 0.7, 1.0], TextAlignment::Left);
                    canvas.draw_text(&row.openkf, [row_rect.x + col_w1 + col_w2 + 8.0, row_rect.y + 10.0], 10.5, [0.4, 0.9, 0.5, 1.0], TextAlignment::Left);
                }
                cur_y += 20.0;
            }
            cur_y += 24.0;
        }

        // Render Feature Cards
        for sec in &self.document.sections {
            let card_header_y = cur_y;
            let card_width = content.width - 16.0;

            // Compute total height of this card
            let card_h = 32.0 
                + (sec.paragraphs.len() as f32 * 18.0) 
                + (sec.bullet_points.len() as f32 * 22.0)
                + (if sec.paragraphs.is_empty() && sec.bullet_points.is_empty() { sec.raw_lines.len() as f32 * 18.0 } else { 0.0 })
                + 12.0;

            let card_rect = Rect::new(content.x + 8.0, card_header_y, card_width, card_h);

            // Culling against viewport
            if card_rect.y + card_rect.height >= clip_top && card_rect.y <= clip_bottom {
                // Card background
                canvas.draw_filled_rect(card_rect, [0.11, 0.12, 0.15, 0.95]);
                canvas.draw_border(card_rect, 1.0, [0.26, 0.30, 0.38, 0.9]);

                // Card header bar
                let ch_rect = Rect::new(card_rect.x, card_rect.y, card_rect.width, 28.0);
                canvas.draw_filled_rect(ch_rect, [0.16, 0.18, 0.22, 1.0]);
                canvas.draw_border(ch_rect, 1.0, [0.35, 0.40, 0.50, 0.6]);

                // Category pill on the right of header bar
                let cat_badge = sec.category.as_str();
                let cat_w = (cat_badge.len() as f32 * 7.0 + 12.0).max(60.0);
                let cat_rect = Rect::new(ch_rect.x + ch_rect.width - cat_w - 6.0, ch_rect.y + 4.0, cat_w, 20.0);
                canvas.draw_filled_rect(cat_rect, sec.category.badge_color());
                canvas.draw_border(cat_rect, 1.0, [0.8, 0.8, 0.9, 0.8]);
                canvas.draw_text(
                    cat_badge,
                    [cat_rect.x + cat_rect.width * 0.5, cat_rect.y + cat_rect.height * 0.5],
                    10.0,
                    [1.0, 1.0, 1.0, 1.0],
                    TextAlignment::Center,
                );

                // Section Title text
                canvas.draw_text(
                    &sec.title,
                    [ch_rect.x + 10.0, ch_rect.y + ch_rect.height * 0.5],
                    13.0,
                    [0.95, 0.95, 1.0, 1.0],
                    TextAlignment::Left,
                );

                // Card body contents
                let mut line_y = ch_rect.y + 36.0;

                // Paragraphs
                for para in &sec.paragraphs {
                    if line_y >= clip_top && line_y <= clip_bottom {
                        canvas.draw_text(
                            para,
                            [card_rect.x + 14.0, line_y],
                            11.5,
                            [0.85, 0.85, 0.88, 1.0],
                            TextAlignment::Left,
                        );
                    }
                    line_y += 18.0;
                }

                // Bullet points with icon
                for bullet in &sec.bullet_points {
                    if line_y >= clip_top && line_y <= clip_bottom {
                        // Diamond / bullet symbol
                        canvas.draw_text(
                            "•",
                            [card_rect.x + 14.0, line_y],
                            12.0,
                            [0.85, 0.35, 0.35, 1.0], // Crimson bullet
                            TextAlignment::Left,
                        );
                        canvas.draw_text(
                            bullet,
                            [card_rect.x + 28.0, line_y],
                            11.5,
                            [0.92, 0.92, 0.95, 1.0],
                            TextAlignment::Left,
                        );
                    }
                    line_y += 22.0;
                }

                // Fallback raw lines
                if sec.paragraphs.is_empty() && sec.bullet_points.is_empty() {
                    for raw in &sec.raw_lines {
                        if line_y >= clip_top && line_y <= clip_bottom {
                            canvas.draw_text(
                                raw,
                                [card_rect.x + 14.0, line_y],
                                11.0,
                                [0.80, 0.80, 0.80, 1.0],
                                TextAlignment::Left,
                            );
                        }
                        line_y += 18.0;
                    }
                }
            }

            cur_y += card_h + 16.0;
        }

        // 6. Vertical Scrollbar
        let track = self.scrollbar_rect();
        canvas.draw_filled_rect(track, [0.09, 0.10, 0.12, 0.9]);
        canvas.draw_border(track, 1.0, [0.22, 0.24, 0.28, 0.8]);

        let thumb = self.scrollbar_thumb_rect();
        let thumb_color = if self.scrollbar_dragging {
            [0.65, 0.25, 0.25, 1.0] // Crimson active
        } else if self.scrollbar_hovered {
            [0.45, 0.48, 0.55, 1.0] // Lighter slate hovered
        } else {
            [0.30, 0.32, 0.38, 0.95] // Slate grey normal
        };
        canvas.draw_filled_rect(thumb, thumb_color);
        canvas.draw_border(thumb, 1.0, [0.55, 0.60, 0.70, 0.9]);

        // 7. Bottom Toolbar / Footer
        let footer = self.footer_rect();
        canvas.draw_filled_rect(footer, [0.12, 0.13, 0.16, 1.0]);
        canvas.draw_border(footer, 1.0, [0.28, 0.32, 0.40, 0.8]);

        // Footer info label
        let footer_info = format!("{} sections loaded | Use Mouse Wheel or Drag Scrollbar to Navigate", self.document.sections.len());
        canvas.draw_text(
            &footer_info,
            [footer.x + 16.0, footer.y + footer.height * 0.5],
            11.5,
            [0.70, 0.72, 0.76, 1.0],
            TextAlignment::Left,
        );

        // Footer "Close" button
        let footer_btn = self.footer_close_button_rect();
        let f_btn_bg = if self.footer_close_hovered {
            [0.45, 0.12, 0.12, 1.0] // Crimson hovered
        } else {
            [0.24, 0.26, 0.32, 1.0] // Slate dark
        };
        canvas.draw_filled_rect(footer_btn, f_btn_bg);
        canvas.draw_border(footer_btn, 1.0, [0.55, 0.20, 0.20, 1.0]);
        canvas.draw_text(
            "CLOSE",
            [footer_btn.x + footer_btn.width * 0.5, footer_btn.y + footer_btn.height * 0.5],
            12.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Center,
        );
    }
}

// --- GUIComponent Implementation ---

impl GUIComponent for NewsDialogPage {
    fn id(&self) -> &str {
        &self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.recalculate_layout();
    }

    fn layout_box(&self) -> LayoutBox {
        self.layout_box
    }

    fn layout(&mut self, parent_bounds: Rect) {
        self.bounds = self.layout_box.resolve(parent_bounds);
        self.recalculate_layout();
    }

    fn state(&self) -> ComponentState {
        self.state
    }

    fn set_state(&mut self, state: ComponentState) {
        self.state = state;
    }

    fn is_visible(&self) -> bool {
        self.visible && self.is_open
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if visible {
            self.is_open = true;
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        if !self.visible || !self.is_open || !self.enabled {
            return false;
        }

        match *event {
            MouseEvent::Down { button: MouseButton::Left, x, y } => {
                self.handle_click(x, y)
            }
            MouseEvent::Up { button: MouseButton::Left, .. } => {
                if self.scrollbar_dragging {
                    self.scrollbar_dragging = false;
                    true
                } else {
                    false
                }
            }
            MouseEvent::Scroll { delta, x, y } => {
                let dlg = self.dialog_rect();
                if dlg.contains(x, y) {
                    self.scroll(delta);
                    true
                } else {
                    false
                }
            }
            MouseEvent::Move { x, y } => {
                // Update button hover states
                self.close_button_hovered = self.close_button_rect().contains(x, y);
                self.footer_close_hovered = self.footer_close_button_rect().contains(x, y);
                self.scrollbar_hovered = self.scrollbar_rect().contains(x, y);

                // Handle active scrollbar thumb drag
                if self.scrollbar_dragging {
                    let track = self.scrollbar_rect();
                    let thumb = self.scrollbar_thumb_rect();
                    let available_track = (track.height - thumb.height).max(1.0);
                    let delta_y = y - self.drag_start_y;
                    let scroll_delta = (delta_y / available_track) * self.max_scroll_y;
                    self.scroll_offset_y = (self.drag_start_scroll + scroll_delta).clamp(0.0, self.max_scroll_y);
                    return true;
                }

                if self.bounds.contains(x, y) {
                    self.state = ComponentState::Hovered;
                    true
                } else {
                    if self.state == ComponentState::Hovered {
                        self.state = ComponentState::Normal;
                    }
                    false
                }
            }
            _ => false,
        }
    }

    fn render(&self, canvas: &mut Canvas) {
        self.render_canvas(canvas);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_NEWS: &str = r#"
================================================================================
  OPENKF CLIENT - NEWS & COMPREHENSIVE CHANGELOG
  Modern 64-Bit Engine Client & Graphical Enhancements for Killing Floor
================================================================================

Project: OpenKF (Killing Floor v1065 / Unreal Engine 2.5)
Current Version: v1.2.0 (October 2026 Edition)
Compatibility: 100% Steam Retail Multiplayer & VAC Safe

--------------------------------------------------------------------------------
  EXECUTIVE SUMMARY: VANILLA (2009) vs. OPENKF CLIENT v1.2.0
--------------------------------------------------------------------------------

FEATURE / SUBSYSTEM     | VANILLA CLIENT (2009)        | OPENKF CLIENT v1.2.0 (OCTOBER 2026)
------------------------+------------------------------+------------------------------------------
Process Architecture    | 32-bit (x86) legacy only     | Native 64-bit (x86_64) Client + Supervisor
Virtual Memory Address  | Restricted to 2 GB max       | Full 64-bit Virtual Space (0 OOM crashes)
Texture Cache Pool      | Hardcoded 64 MB (thrashing)  | 1024 MB High-Performance Memory Cache
Graphics API / Pipeline | Direct3D 9 (2004 legacy)     | Vulkan 1.3 & D3D12 Modern Render HAL

--------------------------------------------------------------------------------
  CORE TECHNICAL DELIVERABLES (TASKS 1 TO 18)
--------------------------------------------------------------------------------

1. VIEWPORT 3D & MODERN RENDER HAL (Vulkan 1.3 / D3D12)
   -----------------------------------------------------------------------------
   * Hardware Abstraction Layer (Render HAL):
     Replaces legacy Direct3D 9 immediate state machine with modern PSOs.
   * 3D Camera & Aim Controller:
     - Gimbal Lock Prevention: Strict pitch clamping to [-89.0°, +89.0°].
   * BSP Level Geometry Extractor:
     - Interleaved GPU Vertex Buffer: 36-byte BspVertex stride.

2. CANVAS 2D & GUI2K4 FRAMEWORK (Modern Menu, Server Browser, HUD)
   -----------------------------------------------------------------------------
   * Hierarchical UI Component Framework:
     Clean-room reimplementation inspired by Unreal Engine 2.5 GUI2K4.u.
   * 2D Immediate-Mode Canvas Batcher:
     32-byte GPU Vertex Layout.

3. NATIVE 64-BIT STEAMWORKS API INTEGRATION (steam_api64.dll)
   -----------------------------------------------------------------------------
   * Dynamic 64-Bit Runtime Loader:
     Win32 dynamic loader utilizing LoadLibraryW and GetProcAddress.
   * Steam AppID 1250 Compliance.

4. RAW INPUT MOUSE SUBSYSTEM (WM_INPUT, Zero-Latency 1:1 Aim)
   -----------------------------------------------------------------------------
   * Low-Level Hardware Input Polling:
     Tested and verified up to 8000 Hz.
"#;

    #[test]
    fn test_parse_news_document_header_and_badges() {
        let doc = NewsDocument::parse(SAMPLE_NEWS);
        assert!(doc.header.version.starts_with("v1."));
        assert_eq!(doc.header.edition, "October 2026 Edition");
        assert!(doc.header.project.contains("Killing Floor v1065"));
        assert!(doc.header.compatibility.contains("VAC Safe"));
        assert!(doc.header.badges.contains(&"v1.2.0".to_string()));
        assert!(doc.header.badges.contains(&"64-Bit x86_64".to_string()));
    }

    #[test]
    fn test_parse_comparison_table() {
        let doc = NewsDocument::parse(SAMPLE_NEWS);
        assert_eq!(doc.comparison_rows.len(), 4);
        assert_eq!(doc.comparison_rows[0].feature, "Process Architecture");
        assert_eq!(doc.comparison_rows[0].vanilla, "32-bit (x86) legacy only");
        assert_eq!(doc.comparison_rows[0].openkf, "Native 64-bit (x86_64) Client + Supervisor");
        assert_eq!(doc.comparison_rows[3].feature, "Graphics API / Pipeline");
    }

    #[test]
    fn test_parse_sections_and_categories() {
        let doc = NewsDocument::parse(SAMPLE_NEWS);
        assert_eq!(doc.sections.len(), 4);

        // Section 1
        assert!(doc.sections[0].title.contains("VIEWPORT 3D"));
        assert_eq!(doc.sections[0].category, FeatureCategory::Graphics);
        assert_eq!(doc.sections[0].badge.as_deref(), Some("Vulkan 1.3 / D3D12"));
        assert!(doc.sections[0].bullet_points.len() >= 2);

        // Section 2
        assert!(doc.sections[1].title.contains("CANVAS 2D"));
        assert_eq!(doc.sections[1].category, FeatureCategory::CanvasUI);

        // Section 3
        assert!(doc.sections[2].title.contains("STEAMWORKS"));
        assert_eq!(doc.sections[2].category, FeatureCategory::Steamworks);

        // Section 4
        assert!(doc.sections[3].title.contains("RAW INPUT"));
        assert_eq!(doc.sections[3].category, FeatureCategory::RawInput);
    }

    #[test]
    fn test_news_dialog_creation_and_bounds() {
        let page = NewsDialogPage::with_bounds(Rect::new(0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(page.id(), "NewsDialogPage");
        assert!(page.is_open());
        assert!(page.is_visible());
        assert_eq!(page.bounds().width, 1920.0);
        assert_eq!(page.bounds().height, 1080.0);

        let dlg = page.dialog_rect();
        assert!(dlg.width > 1000.0);
        assert!(dlg.height > 800.0);
    }

    #[test]
    fn test_load_news_text_and_scroll() {
        let mut page = NewsDialogPage::new("TestNewsDialog");
        page.load_news_text(SAMPLE_NEWS);

        assert_eq!(page.sections().len(), 4);
        assert_eq!(page.version(), "v1.2.0");
        assert_eq!(page.scroll_offset(), 0.0);
        assert!(page.max_scroll() > 0.0);

        // Test scrolling down (delta < 0)
        page.scroll(-2.0);
        assert!(page.scroll_offset() > 0.0);
        let scrolled = page.scroll_offset();

        // Test scrolling up (delta > 0)
        page.scroll(1.0);
        assert!(page.scroll_offset() < scrolled);

        // Test scroll to top
        page.scroll_to_top();
        assert_eq!(page.scroll_offset(), 0.0);

        // Test scroll to bottom
        page.scroll_to_bottom();
        assert_eq!(page.scroll_offset(), page.max_scroll());
    }

    #[test]
    fn test_close_button_clicks() {
        let mut page = NewsDialogPage::new("NewsCloseTest");
        page.load_news_text(SAMPLE_NEWS);
        assert!(page.is_open());

        // Click top-right close button
        let close_btn = page.close_button_rect();
        let consumed = page.handle_mouse_event(&MouseEvent::Down {
            button: MouseButton::Left,
            x: close_btn.x + 2.0,
            y: close_btn.y + 2.0,
        });
        assert!(consumed);
        assert!(!page.is_open());
        assert!(!page.is_visible());

        // Re-open and click footer close button
        page.open();
        assert!(page.is_open());
        let footer_btn = page.footer_close_button_rect();
        let consumed_footer = page.handle_mouse_event(&MouseEvent::Down {
            button: MouseButton::Left,
            x: footer_btn.x + 5.0,
            y: footer_btn.y + 5.0,
        });
        assert!(consumed_footer);
        assert!(!page.is_open());
    }

    #[test]
    fn test_mouse_wheel_event_scrolling() {
        let mut page = NewsDialogPage::new("NewsScrollWheelTest");
        page.load_news_text(SAMPLE_NEWS);

        let dlg = page.dialog_rect();
        let scroll_event = MouseEvent::Scroll {
            delta: -1.0,
            x: dlg.x + 50.0,
            y: dlg.y + 50.0,
        };
        let consumed = page.handle_mouse_event(&scroll_event);
        assert!(consumed);
        assert!(page.scroll_offset() > 0.0);
    }

    #[test]
    fn test_render_to_canvas_and_quad_batches() {
        let mut page = NewsDialogPage::new("NewsRenderTest");
        page.load_news_text(SAMPLE_NEWS);

        let mut canvas = Canvas::new(1024.0, 768.0);
        page.render(&mut canvas);

        assert!(!canvas.commands.is_empty());
        assert!(!canvas.vertices.is_empty());
        assert!(!canvas.indices.is_empty());

        // Verify UIVertex geometry was generated
        assert_eq!(canvas.vertices.len() % 4, 0);
        assert_eq!(canvas.indices.len() % 6, 0);
    }

    #[test]
    fn test_retail_news_file_parsing() {
        // Parse actual repo NEWS.txt
        let full_news = include_str!("../../../NEWS.txt");
        let doc = NewsDocument::parse(full_news);

        assert!(doc.sections.len() >= 5);
        assert!(doc.header.version.starts_with("v1."));
        assert!(doc.comparison_rows.len() >= 10);
    }
}
