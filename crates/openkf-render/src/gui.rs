//! Modern 2D UI component tree inspired by Unreal Engine 2.5 GUI2K4.
//!
//! Provides a hierarchical, resolution-independent UI system with:
//! - [`GUIComponent`]: Abstract trait for widgets and containers.
//! - [`GUIPage`]: Root window container managed in a page stack.
//! - [`GUIButton`]: Clickable push-button with hover, active, pressed states and event callbacks.
//! - [`GUIListBox`]: Scrollable selection list with item picking and mouse wheel scrolling.
//! - [`GUILabel`]: Text label with alignment and font styling.
//! - [`GuiController`]: Central UI manager coordinating page stacks, input routing, and rendering.
//! - [`Canvas`]: 2D immediate-mode canvas emitting [`UIVertex`] geometry and draw primitives.

use std::any::Any;
use std::fmt::Debug;
use crate::hal::TextureHandle;
use crate::vertex::UIVertex;

/// 2D Rectangle defining widget bounds and screen regions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for Rect {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }
}

impl Rect {
    /// Creates a new rectangle with the given origin and dimensions.
    #[inline]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Checks if a 2D point (x, y) is inside this rectangle.
    #[inline]
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= (self.x + self.width) && py >= self.y && py <= (self.y + self.height)
    }

    #[inline]
    pub fn min_x(&self) -> f32 {
        self.x
    }

    #[inline]
    pub fn max_x(&self) -> f32 {
        self.x + self.width
    }

    #[inline]
    pub fn min_y(&self) -> f32 {
        self.y
    }

    #[inline]
    pub fn max_y(&self) -> f32 {
        self.y + self.height
    }

    #[inline]
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width * 0.5, self.y + self.height * 0.5)
    }
}

/// Anchor points for screen anchoring and alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorPoint {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// Layout positioning mode for GUI components.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayoutMode {
    /// Proportional coordinates (0.0 .. 1.0) relative to parent bounds (UE2 WinLeft/WinTop style).
    Relative,
    /// Absolute pixel dimensions and coordinates.
    Absolute,
    /// Centered within parent with fixed pixel dimensions.
    Centered { width: f32, height: f32 },
    /// Anchored to a parent edge/corner with pixel offsets and sizes.
    Anchored {
        anchor: AnchorPoint,
        offset_x: f32,
        offset_y: f32,
        width: f32,
        height: f32,
    },
}

/// Layout specification inspired by UE2 GUIComponent WinLeft, WinTop, WinWidth, WinHeight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutBox {
    pub win_left: f32,
    pub win_top: f32,
    pub win_width: f32,
    pub win_height: f32,
    pub mode: LayoutMode,
}

impl Default for LayoutBox {
    fn default() -> Self {
        Self {
            win_left: 0.0,
            win_top: 0.0,
            win_width: 1.0,
            win_height: 1.0,
            mode: LayoutMode::Relative,
        }
    }
}

impl LayoutBox {
    /// Creates a relative layout box where values represent fractions of parent dimensions (0.0 to 1.0).
    #[inline]
    pub const fn relative(win_left: f32, win_top: f32, win_width: f32, win_height: f32) -> Self {
        Self {
            win_left,
            win_top,
            win_width,
            win_height,
            mode: LayoutMode::Relative,
        }
    }

    /// Creates an absolute layout box in pixel coordinates.
    #[inline]
    pub const fn absolute(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            win_left: x,
            win_top: y,
            win_width: width,
            win_height: height,
            mode: LayoutMode::Absolute,
        }
    }

    /// Creates a centered layout box with explicit pixel width and height.
    #[inline]
    pub const fn centered(width: f32, height: f32) -> Self {
        Self {
            win_left: 0.0,
            win_top: 0.0,
            win_width: width,
            win_height: height,
            mode: LayoutMode::Centered { width, height },
        }
    }

    /// Creates an anchored layout box relative to an anchor point.
    #[inline]
    pub const fn anchored(
        anchor: AnchorPoint,
        offset_x: f32,
        offset_y: f32,
        width: f32,
        height: f32,
    ) -> Self {
        Self {
            win_left: offset_x,
            win_top: offset_y,
            win_width: width,
            win_height: height,
            mode: LayoutMode::Anchored {
                anchor,
                offset_x,
                offset_y,
                width,
                height,
            },
        }
    }

    /// Resolves the layout box against parent boundaries into absolute pixel coordinates.
    pub fn resolve(&self, parent: Rect) -> Rect {
        match self.mode {
            LayoutMode::Relative => Rect::new(
                parent.x + self.win_left * parent.width,
                parent.y + self.win_top * parent.height,
                self.win_width * parent.width,
                self.win_height * parent.height,
            ),
            LayoutMode::Absolute => Rect::new(self.win_left, self.win_top, self.win_width, self.win_height),
            LayoutMode::Centered { width, height } => {
                let x = parent.x + (parent.width - width) * 0.5;
                let y = parent.y + (parent.height - height) * 0.5;
                Rect::new(x, y, width, height)
            }
            LayoutMode::Anchored {
                anchor,
                offset_x,
                offset_y,
                width,
                height,
            } => {
                let (ax, ay) = match anchor {
                    AnchorPoint::TopLeft => (parent.x, parent.y),
                    AnchorPoint::TopCenter => (parent.x + parent.width * 0.5, parent.y),
                    AnchorPoint::TopRight => (parent.x + parent.width, parent.y),
                    AnchorPoint::CenterLeft => (parent.x, parent.y + parent.height * 0.5),
                    AnchorPoint::Center => (parent.x + parent.width * 0.5, parent.y + parent.height * 0.5),
                    AnchorPoint::CenterRight => (parent.x + parent.width, parent.y + parent.height * 0.5),
                    AnchorPoint::BottomLeft => (parent.x, parent.y + parent.height),
                    AnchorPoint::BottomCenter => (parent.x + parent.width * 0.5, parent.y + parent.height),
                    AnchorPoint::BottomRight => (parent.x + parent.width, parent.y + parent.height),
                };
                Rect::new(ax + offset_x, ay + offset_y, width, height)
            }
        }
    }
}

/// Visual interaction state of a GUI component.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ComponentState {
    #[default]
    Normal,
    Hovered,
    Pressed,
    Disabled,
}

/// Mouse buttons supported by the input system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Mouse input events dispatched through the GUI hierarchy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MouseEvent {
    Move { x: f32, y: f32 },
    Down { button: MouseButton, x: f32, y: f32 },
    Up { button: MouseButton, x: f32, y: f32 },
    Scroll { delta: f32, x: f32, y: f32 },
}

/// Text alignment options.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextAlignment {
    #[default]
    Left,
    Center,
    Right,
}

/// High-level 2D canvas draw command.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCommand {
    Quad {
        bounds: Rect,
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        color: [f32; 4],
        texture: Option<TextureHandle>,
    },
    Border {
        bounds: Rect,
        thickness: f32,
        color: [f32; 4],
    },
    Text {
        text: String,
        position: [f32; 2],
        font_size: f32,
        color: [f32; 4],
        alignment: TextAlignment,
    },
}

/// 2D Canvas for accumulating immediate-mode UI draw commands and [`UIVertex`] geometry.
#[derive(Clone, Debug, Default)]
pub struct Canvas {
    pub viewport: Rect,
    pub commands: Vec<DrawCommand>,
    pub vertices: Vec<UIVertex>,
    pub indices: Vec<u32>,
}

impl Canvas {
    /// Creates a new canvas for a given viewport size.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            viewport: Rect::new(0.0, 0.0, width, height),
            commands: Vec::new(),
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Clears all recorded commands, vertices, and indices.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.vertices.clear();
        self.indices.clear();
    }

    /// Draws a solid colored rectangle and appends vertex data.
    pub fn draw_filled_rect(&mut self, bounds: Rect, color: [f32; 4]) {
        self.commands.push(DrawCommand::Quad {
            bounds,
            uv_min: [0.0, 0.0],
            uv_max: [1.0, 1.0],
            color,
            texture: None,
        });

        self.append_quad_vertices(bounds, [0.0, 0.0], [1.0, 1.0], color);
    }

    /// Draws a textured quad and appends vertex data.
    pub fn draw_textured_quad(
        &mut self,
        bounds: Rect,
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        color: [f32; 4],
        texture: TextureHandle,
    ) {
        self.commands.push(DrawCommand::Quad {
            bounds,
            uv_min,
            uv_max,
            color,
            texture: Some(texture),
        });

        self.append_quad_vertices(bounds, uv_min, uv_max, color);
    }

    /// Draws a rectangular border of given thickness and color.
    pub fn draw_border(&mut self, bounds: Rect, thickness: f32, color: [f32; 4]) {
        self.commands.push(DrawCommand::Border {
            bounds,
            thickness,
            color,
        });

        // Top edge
        self.draw_filled_rect(
            Rect::new(bounds.x, bounds.y, bounds.width, thickness),
            color,
        );
        // Bottom edge
        self.draw_filled_rect(
            Rect::new(bounds.x, bounds.y + bounds.height - thickness, bounds.width, thickness),
            color,
        );
        // Left edge
        self.draw_filled_rect(
            Rect::new(bounds.x, bounds.y + thickness, thickness, bounds.height - 2.0 * thickness),
            color,
        );
        // Right edge
        self.draw_filled_rect(
            Rect::new(bounds.x + bounds.width - thickness, bounds.y + thickness, thickness, bounds.height - 2.0 * thickness),
            color,
        );
    }

    /// Draws a text label.
    pub fn draw_text(
        &mut self,
        text: impl Into<String>,
        position: [f32; 2],
        font_size: f32,
        color: [f32; 4],
        alignment: TextAlignment,
    ) {
        self.commands.push(DrawCommand::Text {
            text: text.into(),
            position,
            font_size,
            color,
            alignment,
        });
    }

    /// Helper to append two indexed triangles (quad) to the vertex stream.
    fn append_quad_vertices(
        &mut self,
        bounds: Rect,
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        color: [f32; 4],
    ) {
        let base_idx = self.vertices.len() as u32;

        let v0 = UIVertex::new([bounds.x, bounds.y], [uv_min[0], uv_min[1]], color);
        let v1 = UIVertex::new([bounds.x + bounds.width, bounds.y], [uv_max[0], uv_min[1]], color);
        let v2 = UIVertex::new([bounds.x + bounds.width, bounds.y + bounds.height], [uv_max[0], uv_max[1]], color);
        let v3 = UIVertex::new([bounds.x, bounds.y + bounds.height], [uv_min[0], uv_max[1]], color);

        self.vertices.extend_from_slice(&[v0, v1, v2, v3]);
        // Two triangles: (0, 1, 2) and (0, 2, 3)
        self.indices.extend_from_slice(&[
            base_idx,
            base_idx + 1,
            base_idx + 2,
            base_idx,
            base_idx + 2,
            base_idx + 3,
        ]);
    }
}

/// Base trait for all GUI2K4 components.
pub trait GUIComponent: Send + Sync + Debug {
    /// Unique identifier for the component.
    fn id(&self) -> &str;

    /// Resolved bounding box in absolute pixels.
    fn bounds(&self) -> Rect;

    /// Sets the resolved bounding box.
    fn set_bounds(&mut self, bounds: Rect);

    /// Layout box specification.
    fn layout_box(&self) -> LayoutBox;

    /// Recalculates component bounds based on parent bounds.
    fn layout(&mut self, parent_bounds: Rect) {
        let b = self.layout_box().resolve(parent_bounds);
        self.set_bounds(b);
    }

    /// Current visual interaction state.
    fn state(&self) -> ComponentState;

    /// Updates visual interaction state.
    fn set_state(&mut self, state: ComponentState);

    /// Visibility toggle.
    fn is_visible(&self) -> bool;

    /// Sets visibility.
    fn set_visible(&mut self, visible: bool);

    /// Enabled toggle.
    fn is_enabled(&self) -> bool;

    /// Sets enabled.
    fn set_enabled(&mut self, enabled: bool);

    /// Handles a mouse input event. Returns `true` if the event was consumed.
    fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool;

    /// Renders the component onto the canvas.
    fn render(&self, canvas: &mut Canvas);

    /// Downcasting support.
    fn as_any(&self) -> &dyn Any;

    /// Mutable downcasting support.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Color theme styling for a button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonStyle {
    pub bg_normal: [f32; 4],
    pub bg_hovered: [f32; 4],
    pub bg_pressed: [f32; 4],
    pub bg_disabled: [f32; 4],
    pub border_color: [f32; 4],
    pub text_color: [f32; 4],
    pub border_thickness: f32,
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self {
            // UE2 / UT2004 metallic dark grey / cyan styling
            bg_normal: [0.18, 0.22, 0.26, 0.95],
            bg_hovered: [0.26, 0.35, 0.45, 1.0],
            bg_pressed: [0.12, 0.16, 0.20, 1.0],
            bg_disabled: [0.10, 0.10, 0.10, 0.5],
            border_color: [0.45, 0.60, 0.75, 1.0],
            text_color: [0.95, 0.95, 0.95, 1.0],
            border_thickness: 1.5,
        }
    }
}

/// Push button component responding to hover and click events.
pub struct GUIButton {
    pub id: String,
    pub caption: String,
    pub layout_box: LayoutBox,
    pub bounds: Rect,
    pub state: ComponentState,
    pub visible: bool,
    pub enabled: bool,
    pub style: ButtonStyle,
    pub click_count: u32,
    pub on_click: Option<Box<dyn FnMut(&mut GUIButton) + Send + Sync>>,
}

impl Debug for GUIButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GUIButton")
            .field("id", &self.id)
            .field("caption", &self.caption)
            .field("bounds", &self.bounds)
            .field("state", &self.state)
            .field("click_count", &self.click_count)
            .finish()
    }
}

impl GUIButton {
    pub fn new(id: impl Into<String>, caption: impl Into<String>, layout_box: LayoutBox) -> Self {
        Self {
            id: id.into(),
            caption: caption.into(),
            layout_box,
            bounds: Rect::default(),
            state: ComponentState::Normal,
            visible: true,
            enabled: true,
            style: ButtonStyle::default(),
            click_count: 0,
            on_click: None,
        }
    }

    pub fn with_click_handler<F>(mut self, handler: F) -> Self
    where
        F: FnMut(&mut GUIButton) + Send + Sync + 'static,
    {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl GUIComponent for GUIButton {
    fn id(&self) -> &str {
        &self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn layout_box(&self) -> LayoutBox {
        self.layout_box
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
        if !enabled {
            self.state = ComponentState::Disabled;
        } else if self.state == ComponentState::Disabled {
            self.state = ComponentState::Normal;
        }
    }

    fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        if !self.visible || !self.enabled {
            return false;
        }

        match *event {
            MouseEvent::Move { x, y } => {
                if self.bounds.contains(x, y) {
                    if self.state != ComponentState::Pressed {
                        self.state = ComponentState::Hovered;
                    }
                    true
                } else {
                    if self.state == ComponentState::Hovered {
                        self.state = ComponentState::Normal;
                    }
                    false
                }
            }
            MouseEvent::Down { button: MouseButton::Left, x, y } => {
                if self.bounds.contains(x, y) {
                    self.state = ComponentState::Pressed;
                    true
                } else {
                    false
                }
            }
            MouseEvent::Up { button: MouseButton::Left, x, y }
                if self.state == ComponentState::Pressed => {
                    if self.bounds.contains(x, y) {
                        self.state = ComponentState::Hovered;
                        self.click_count += 1;
                        if let Some(mut handler) = self.on_click.take() {
                            handler(self);
                            self.on_click = Some(handler);
                        }
                    } else {
                        self.state = ComponentState::Normal;
                    }
                    true
                }
            _ => false,
        }
    }

    fn render(&self, canvas: &mut Canvas) {
        if !self.visible {
            return;
        }

        let bg_color = match self.state {
            ComponentState::Normal => self.style.bg_normal,
            ComponentState::Hovered => self.style.bg_hovered,
            ComponentState::Pressed => self.style.bg_pressed,
            ComponentState::Disabled => self.style.bg_disabled,
        };

        canvas.draw_filled_rect(self.bounds, bg_color);
        canvas.draw_border(self.bounds, self.style.border_thickness, self.style.border_color);

        let center_x = self.bounds.x + self.bounds.width * 0.5;
        let center_y = self.bounds.y + self.bounds.height * 0.5;
        canvas.draw_text(
            &self.caption,
            [center_x, center_y],
            14.0,
            self.style.text_color,
            TextAlignment::Center,
        );
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Text label component.
pub struct GUILabel {
    pub id: String,
    pub caption: String,
    pub layout_box: LayoutBox,
    pub bounds: Rect,
    pub state: ComponentState,
    pub visible: bool,
    pub enabled: bool,
    pub color: [f32; 4],
    pub font_size: f32,
    pub alignment: TextAlignment,
}

impl Debug for GUILabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GUILabel")
            .field("id", &self.id)
            .field("caption", &self.caption)
            .field("bounds", &self.bounds)
            .finish()
    }
}

impl GUILabel {
    pub fn new(id: impl Into<String>, caption: impl Into<String>, layout_box: LayoutBox) -> Self {
        Self {
            id: id.into(),
            caption: caption.into(),
            layout_box,
            bounds: Rect::default(),
            state: ComponentState::Normal,
            visible: true,
            enabled: true,
            color: [1.0, 1.0, 1.0, 1.0],
            font_size: 14.0,
            alignment: TextAlignment::Left,
        }
    }

    pub fn with_alignment(mut self, alignment: TextAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }
}

impl GUIComponent for GUILabel {
    fn id(&self) -> &str {
        &self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn layout_box(&self) -> LayoutBox {
        self.layout_box
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

    fn handle_mouse_event(&mut self, _event: &MouseEvent) -> bool {
        false
    }

    fn render(&self, canvas: &mut Canvas) {
        if !self.visible {
            return;
        }

        let pos = match self.alignment {
            TextAlignment::Left => [self.bounds.x, self.bounds.y + self.bounds.height * 0.5],
            TextAlignment::Center => [self.bounds.x + self.bounds.width * 0.5, self.bounds.y + self.bounds.height * 0.5],
            TextAlignment::Right => [self.bounds.x + self.bounds.width, self.bounds.y + self.bounds.height * 0.5],
        };

        canvas.draw_text(&self.caption, pos, self.font_size, self.color, self.alignment);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Scrollable selection list component.
pub struct GUIListBox {
    pub id: String,
    pub items: Vec<String>,
    pub selected_index: Option<usize>,
    pub scroll_offset: usize,
    pub item_height: f32,
    pub layout_box: LayoutBox,
    pub bounds: Rect,
    pub state: ComponentState,
    pub visible: bool,
    pub enabled: bool,
    pub hovered_row: Option<usize>,
    pub on_select: Option<Box<dyn FnMut(usize, &str) + Send + Sync>>,
}

impl Debug for GUIListBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GUIListBox")
            .field("id", &self.id)
            .field("item_count", &self.items.len())
            .field("selected_index", &self.selected_index)
            .field("scroll_offset", &self.scroll_offset)
            .finish()
    }
}

impl GUIListBox {
    pub fn new(id: impl Into<String>, items: Vec<String>, layout_box: LayoutBox) -> Self {
        Self {
            id: id.into(),
            items,
            selected_index: None,
            scroll_offset: 0,
            item_height: 24.0,
            layout_box,
            bounds: Rect::default(),
            state: ComponentState::Normal,
            visible: true,
            enabled: true,
            hovered_row: None,
            on_select: None,
        }
    }

    pub fn with_select_handler<F>(mut self, handler: F) -> Self
    where
        F: FnMut(usize, &str) + Send + Sync + 'static,
    {
        self.on_select = Some(Box::new(handler));
        self
    }

    pub fn max_visible_rows(&self) -> usize {
        if self.item_height <= 0.0 {
            1
        } else {
            (self.bounds.height / self.item_height).floor() as usize
        }
    }

    pub fn select(&mut self, index: usize) {
        if index < self.items.len() {
            self.selected_index = Some(index);
            if let Some(mut handler) = self.on_select.take() {
                let item_text = self.items[index].clone();
                handler(index, &item_text);
                self.on_select = Some(handler);
            }
        }
    }

    pub fn selected_item(&self) -> Option<&str> {
        self.selected_index.and_then(|idx| self.items.get(idx).map(|s| s.as_str()))
    }
}

impl GUIComponent for GUIListBox {
    fn id(&self) -> &str {
        &self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn layout_box(&self) -> LayoutBox {
        self.layout_box
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
                if self.bounds.contains(x, y) {
                    let rel_y = y - self.bounds.y;
                    let row = (rel_y / self.item_height).floor() as usize;
                    let target_idx = self.scroll_offset + row;
                    if target_idx < self.items.len() {
                        self.hovered_row = Some(target_idx);
                    } else {
                        self.hovered_row = None;
                    }
                    self.state = ComponentState::Hovered;
                    true
                } else {
                    self.hovered_row = None;
                    if self.state == ComponentState::Hovered {
                        self.state = ComponentState::Normal;
                    }
                    false
                }
            }
            MouseEvent::Down { button: MouseButton::Left, x, y } => {
                if self.bounds.contains(x, y) {
                    let rel_y = y - self.bounds.y;
                    let row = (rel_y / self.item_height).floor() as usize;
                    let target_idx = self.scroll_offset + row;
                    if target_idx < self.items.len() {
                        self.select(target_idx);
                    }
                    true
                } else {
                    false
                }
            }
            MouseEvent::Scroll { delta, x, y }
                if self.bounds.contains(x, y) => {
                    if delta > 0.0 && self.scroll_offset > 0 {
                        self.scroll_offset -= 1;
                    } else if delta < 0.0 && (self.scroll_offset + self.max_visible_rows()) < self.items.len() {
                        self.scroll_offset += 1;
                    }
                    true
                }
            _ => false,
        }
    }

    fn render(&self, canvas: &mut Canvas) {
        if !self.visible {
            return;
        }

        // List background
        let bg_color = [0.10, 0.12, 0.14, 0.95];
        canvas.draw_filled_rect(self.bounds, bg_color);
        canvas.draw_border(self.bounds, 1.0, [0.3, 0.4, 0.5, 1.0]);

        let visible_count = self.max_visible_rows();
        let start = self.scroll_offset;
        let end = (start + visible_count).min(self.items.len());

        for i in start..end {
            let row_idx = i - start;
            let item_y = self.bounds.y + (row_idx as f32) * self.item_height;
            let item_bounds = Rect::new(self.bounds.x + 2.0, item_y + 1.0, self.bounds.width - 4.0, self.item_height - 2.0);

            let is_selected = self.selected_index == Some(i);
            let is_hovered = self.hovered_row == Some(i);

            if is_selected {
                // UT2004 selected highlight cyan
                canvas.draw_filled_rect(item_bounds, [0.2, 0.45, 0.65, 0.9]);
            } else if is_hovered {
                canvas.draw_filled_rect(item_bounds, [0.18, 0.25, 0.35, 0.5]);
            }

            let text_color = if is_selected {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.85, 0.85, 0.85, 1.0]
            };

            canvas.draw_text(
                &self.items[i],
                [item_bounds.x + 6.0, item_bounds.y + item_bounds.height * 0.5],
                13.0,
                text_color,
                TextAlignment::Left,
            );
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Root window or page container in the GUI2K4 menu hierarchy.
pub struct GUIPage {
    pub id: String,
    pub layout_box: LayoutBox,
    pub bounds: Rect,
    pub visible: bool,
    pub enabled: bool,
    pub b_modal: bool,
    pub background_color: Option<[f32; 4]>,
    pub components: Vec<Box<dyn GUIComponent>>,
}

impl Debug for GUIPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GUIPage")
            .field("id", &self.id)
            .field("bounds", &self.bounds)
            .field("components_len", &self.components.len())
            .finish()
    }
}

impl GUIPage {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            layout_box: LayoutBox::relative(0.0, 0.0, 1.0, 1.0),
            bounds: Rect::default(),
            visible: true,
            enabled: true,
            b_modal: false,
            background_color: Some([0.05, 0.07, 0.09, 0.85]),
            components: Vec::new(),
        }
    }

    pub fn with_layout(mut self, layout_box: LayoutBox) -> Self {
        self.layout_box = layout_box;
        self
    }

    pub fn with_modal(mut self, modal: bool) -> Self {
        self.b_modal = modal;
        self
    }

    /// Adds a child component to the page.
    pub fn add_component(&mut self, mut component: Box<dyn GUIComponent>) {
        component.layout(self.bounds);
        self.components.push(component);
    }

    /// Finds an immutable reference to a component by its unique ID.
    pub fn find_component(&self, id: &str) -> Option<&(dyn GUIComponent + '_)> {
        for comp in &self.components {
            if comp.id() == id {
                return Some(comp.as_ref());
            }
        }
        None
    }

    /// Finds a mutable reference to a component by its unique ID.
    pub fn find_component_mut(&mut self, id: &str) -> Option<&mut (dyn GUIComponent + '_)> {
        for comp in &mut self.components {
            if comp.id() == id {
                return Some(comp.as_mut());
            }
        }
        None
    }

    /// Recalculates layout for the page and all child components.
    pub fn layout(&mut self, screen_bounds: Rect) {
        self.bounds = self.layout_box.resolve(screen_bounds);
        for comp in &mut self.components {
            comp.layout(self.bounds);
        }
    }

    /// Dispatches mouse events through the component tree (topmost/reverse order for clicks).
    pub fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        if !self.visible || !self.enabled {
            return false;
        }

        // Iterate in reverse for down/click events so front components take priority
        for comp in self.components.iter_mut().rev() {
            if comp.handle_mouse_event(event) {
                return true;
            }
        }

        // Check if page consumes click if modal
        if self.b_modal {
            matches!(*event, MouseEvent::Down { x, y, .. } if self.bounds.contains(x, y))
        } else {
            false
        }
    }

    /// Renders page background and all child components.
    pub fn render(&self, canvas: &mut Canvas) {
        if !self.visible {
            return;
        }

        if let Some(bg) = self.background_color {
            canvas.draw_filled_rect(self.bounds, bg);
        }

        for comp in &self.components {
            if comp.is_visible() {
                comp.render(canvas);
            }
        }
    }
}

/// Central GUI manager coordinating pages, mouse event routing, and rendering.
pub struct GuiController {
    pub screen_width: f32,
    pub screen_height: f32,
    pub pages: Vec<GUIPage>,
    pub cursor_pos: (f32, f32),
}

impl Debug for GuiController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuiController")
            .field("screen_width", &self.screen_width)
            .field("screen_height", &self.screen_height)
            .field("page_count", &self.pages.len())
            .field("cursor_pos", &self.cursor_pos)
            .finish()
    }
}

impl GuiController {
    /// Creates a new GUI controller with the given display resolution.
    pub fn new(screen_width: f32, screen_height: f32) -> Self {
        Self {
            screen_width,
            screen_height,
            pages: Vec::new(),
            cursor_pos: (0.0, 0.0),
        }
    }

    /// Updates screen resolution and updates layout across all active pages.
    pub fn set_resolution(&mut self, width: f32, height: f32) {
        self.screen_width = width;
        self.screen_height = height;
        let screen_rect = Rect::new(0.0, 0.0, width, height);
        for page in &mut self.pages {
            page.layout(screen_rect);
        }
    }

    /// Pushes a new page onto the page stack.
    pub fn add_page(&mut self, mut page: GUIPage) {
        let screen_rect = Rect::new(0.0, 0.0, self.screen_width, self.screen_height);
        page.layout(screen_rect);
        self.pages.push(page);
    }

    /// Pops the topmost page from the stack.
    pub fn pop_page(&mut self) -> Option<GUIPage> {
        self.pages.pop()
    }

    /// Closes a page by ID. Returns `true` if a page was removed.
    pub fn close_page(&mut self, id: &str) -> bool {
        if let Some(pos) = self.pages.iter().position(|p| p.id == id) {
            self.pages.remove(pos);
            true
        } else {
            false
        }
    }

    /// Returns a reference to the active topmost page.
    pub fn top_page(&self) -> Option<&GUIPage> {
        self.pages.last()
    }

    /// Returns a mutable reference to the active topmost page.
    pub fn top_page_mut(&mut self) -> Option<&mut GUIPage> {
        self.pages.last_mut()
    }

    /// Number of active pages.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Routes mouse events to the topmost active page.
    pub fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        match *event {
            MouseEvent::Move { x, y }
            | MouseEvent::Down { x, y, .. }
            | MouseEvent::Up { x, y, .. }
            | MouseEvent::Scroll { x, y, .. } => {
                self.cursor_pos = (x, y);
            }
        }

        if let Some(page) = self.pages.last_mut() {
            page.handle_mouse_event(event)
        } else {
            false
        }
    }

    /// Renders all visible pages from bottom to top on the canvas.
    pub fn render(&self, canvas: &mut Canvas) {
        for page in &self.pages {
            if page.visible {
                page.render(canvas);
            }
        }
    }
}

// Aliases for alternate casing conventions
pub type GuiComponent = dyn GUIComponent;
pub type GuiPage = GUIPage;
pub type GuiButton = GUIButton;
pub type GuiListBox = GUIListBox;
pub type GuiLabel = GUILabel;
pub type GUIController = GuiController;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_anchoring_relative_and_absolute() {
        let screen = Rect::new(0.0, 0.0, 1920.0, 1080.0);

        // Relative layout: 10% left, 20% top, 50% width, 30% height
        let rel_box = LayoutBox::relative(0.1, 0.2, 0.5, 0.3);
        let resolved_rel = rel_box.resolve(screen);
        assert!((resolved_rel.x - 192.0).abs() < 1e-3);
        assert!((resolved_rel.y - 216.0).abs() < 1e-3);
        assert!((resolved_rel.width - 960.0).abs() < 1e-3);
        assert!((resolved_rel.height - 324.0).abs() < 1e-3);

        // Absolute layout: 100, 100, 400, 300
        let abs_box = LayoutBox::absolute(100.0, 100.0, 400.0, 300.0);
        let resolved_abs = abs_box.resolve(screen);
        assert_eq!(resolved_abs, Rect::new(100.0, 100.0, 400.0, 300.0));

        // Centered layout: 800x600 in 1920x1080
        let center_box = LayoutBox::centered(800.0, 600.0);
        let resolved_center = center_box.resolve(screen);
        assert_eq!(resolved_center, Rect::new(560.0, 240.0, 800.0, 600.0));

        // Anchored TopRight with negative offset
        let anchor_tr = LayoutBox::anchored(AnchorPoint::TopRight, -200.0, 10.0, 190.0, 50.0);
        let resolved_tr = anchor_tr.resolve(screen);
        assert_eq!(resolved_tr, Rect::new(1720.0, 10.0, 190.0, 50.0));
    }

    #[test]
    fn test_component_state_transitions() {
        let mut button = GUIButton::new("btn_test", "Click Me", LayoutBox::absolute(10.0, 10.0, 100.0, 40.0));
        button.set_bounds(Rect::new(10.0, 10.0, 100.0, 40.0));
        assert_eq!(button.state(), ComponentState::Normal);

        // Move over button -> Hovered
        let handled = button.handle_mouse_event(&MouseEvent::Move { x: 50.0, y: 30.0 });
        assert!(handled);
        assert_eq!(button.state(), ComponentState::Hovered);

        // Mouse Down inside -> Pressed
        let handled_down = button.handle_mouse_event(&MouseEvent::Down {
            button: MouseButton::Left,
            x: 50.0,
            y: 30.0,
        });
        assert!(handled_down);
        assert_eq!(button.state(), ComponentState::Pressed);

        // Move outside while pressed -> stays Pressed
        let handled_move_out = button.handle_mouse_event(&MouseEvent::Move { x: 200.0, y: 200.0 });
        assert!(!handled_move_out);
        assert_eq!(button.state(), ComponentState::Pressed);

        // Release outside -> Normal, click not fired
        let handled_up_out = button.handle_mouse_event(&MouseEvent::Up {
            button: MouseButton::Left,
            x: 200.0,
            y: 200.0,
        });
        assert!(handled_up_out);
        assert_eq!(button.state(), ComponentState::Normal);
        assert_eq!(button.click_count, 0);
    }

    #[test]
    fn test_gui_button_click_event() {
        let mut button = GUIButton::new("btn_click", "Action", LayoutBox::absolute(50.0, 50.0, 120.0, 30.0));
        button.set_bounds(Rect::new(50.0, 50.0, 120.0, 30.0));

        button.on_click = Some(Box::new(|btn| {
            btn.caption = "Clicked!".to_string();
        }));

        // Down inside
        button.handle_mouse_event(&MouseEvent::Down {
            button: MouseButton::Left,
            x: 60.0,
            y: 60.0,
        });
        assert_eq!(button.state(), ComponentState::Pressed);

        // Up inside -> click triggers callback
        button.handle_mouse_event(&MouseEvent::Up {
            button: MouseButton::Left,
            x: 60.0,
            y: 60.0,
        });
        assert_eq!(button.click_count, 1);
        assert_eq!(button.caption, "Clicked!");
    }

    #[test]
    fn test_gui_listbox_selection_and_scroll() {
        let items = vec![
            "Entry 0".to_string(),
            "Entry 1".to_string(),
            "Entry 2".to_string(),
            "Entry 3".to_string(),
            "Entry 4".to_string(),
        ];
        let mut list = GUIListBox::new("list_test", items, LayoutBox::absolute(0.0, 0.0, 200.0, 72.0));
        list.set_bounds(Rect::new(0.0, 0.0, 200.0, 72.0)); // 3 visible rows (72 / 24)

        assert_eq!(list.selected_index, None);
        assert_eq!(list.max_visible_rows(), 3);

        // Click row 1 (y = 30.0, which is row 1: 24..48)
        let handled = list.handle_mouse_event(&MouseEvent::Down {
            button: MouseButton::Left,
            x: 50.0,
            y: 30.0,
        });
        assert!(handled);
        assert_eq!(list.selected_index, Some(1));
        assert_eq!(list.selected_item(), Some("Entry 1"));

        // Scroll down
        let scrolled = list.handle_mouse_event(&MouseEvent::Scroll {
            delta: -1.0,
            x: 50.0,
            y: 30.0,
        });
        assert!(scrolled);
        assert_eq!(list.scroll_offset, 1);

        // Click first visible row now (row 0 in view is index 1 + 0 = 1, row 1 is index 2)
        list.handle_mouse_event(&MouseEvent::Down {
            button: MouseButton::Left,
            x: 50.0,
            y: 30.0,
        });
        assert_eq!(list.selected_index, Some(2));
    }

    #[test]
    fn test_gui_controller_page_stack_and_event_routing() {
        let mut controller = GuiController::new(1280.0, 720.0);
        assert_eq!(controller.page_count(), 0);

        let mut page1 = GUIPage::new("main_menu");
        let btn1 = GUIButton::new("start_game", "Start", LayoutBox::relative(0.4, 0.4, 0.2, 0.1));
        page1.add_component(Box::new(btn1));

        controller.add_page(page1);
        assert_eq!(controller.page_count(), 1);

        let active_page = controller.top_page().unwrap();
        assert_eq!(active_page.id, "main_menu");
        let start_btn = active_page.find_component("start_game").unwrap();
        // 0.4 * 1280 = 512, 0.4 * 720 = 288, 0.2 * 1280 = 256, 0.1 * 720 = 72
        assert_eq!(start_btn.bounds(), Rect::new(512.0, 288.0, 256.0, 72.0));

        // Event routing through controller: Hover over button
        let handled = controller.handle_mouse_event(&MouseEvent::Move { x: 550.0, y: 300.0 });
        assert!(handled);
        assert_eq!(controller.cursor_pos, (550.0, 300.0));

        // Add modal dialog
        let page2 = GUIPage::new("confirm_dialog").with_modal(true);
        controller.add_page(page2);
        assert_eq!(controller.page_count(), 2);
        assert_eq!(controller.top_page().unwrap().id, "confirm_dialog");

        // Close page
        let closed = controller.close_page("confirm_dialog");
        assert!(closed);
        assert_eq!(controller.page_count(), 1);
        assert_eq!(controller.top_page().unwrap().id, "main_menu");
    }

    #[test]
    fn test_canvas_ui_vertex_batch_generation() {
        let mut canvas = Canvas::new(800.0, 600.0);
        assert_eq!(canvas.vertices.len(), 0);
        assert_eq!(canvas.indices.len(), 0);

        canvas.draw_filled_rect(Rect::new(10.0, 20.0, 100.0, 50.0), [1.0, 0.5, 0.0, 1.0]);

        assert_eq!(canvas.vertices.len(), 4);
        assert_eq!(canvas.indices.len(), 6);
        assert_eq!(canvas.commands.len(), 1);

        // Check top-left vertex
        assert_eq!(canvas.vertices[0].position, [10.0, 20.0]);
        assert_eq!(canvas.vertices[0].color, [1.0, 0.5, 0.0, 1.0]);

        // Check bottom-right vertex
        assert_eq!(canvas.vertices[2].position, [110.0, 70.0]);

        // Check indices triangle topology
        assert_eq!(canvas.indices, vec![0, 1, 2, 0, 2, 3]);
    }
}
