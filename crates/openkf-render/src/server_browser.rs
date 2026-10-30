//! Killing Floor Server Browser GUI Component
//!
//! Provides the retail Killing Floor server browser interface, including:
//! - Multi-column server table (Server Name, Map, Players/Max, Wave, Difficulty, Ping)
//! - Ascending/Descending column sorting with interactive column header clicking
//! - Real-time search and filter controls (Name/Map search, hide empty, hide full, hide passworded, difficulty, ping)
//! - Favorites management and dedicated Favorites tab
//! - Direct IP connect modal dialog with validation and Unreal Engine connect URL formatting
//! - Full integration with OpenKF 2D UI system implementing [`crate::gui::GUIComponent`]

use std::any::Any;
use std::collections::HashSet;
use std::fmt::Debug;
use glam::Mat4;
use crate::error::RenderError;
use crate::gui::{
    Canvas, ComponentState, GUIComponent, LayoutBox, MouseButton, MouseEvent, Rect,
    TextAlignment,
};
use crate::hal::{PrimitiveType, RenderInterface, TransformType, Viewport};
use crate::vertex::UIVertex;

/// Server difficulty levels in retail Killing Floor (retail enum).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ServerDifficulty {
    Beginner = 0,
    Normal = 1,
    Hard = 2,
    Suicidal = 3,
    HellOnEarth = 4,
}

impl ServerDifficulty {
    /// Returns the user-facing display string for this difficulty.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Beginner => "Beginner",
            Self::Normal => "Normal",
            Self::Hard => "Hard",
            Self::Suicidal => "Suicidal",
            Self::HellOnEarth => "Hell on Earth",
        }
    }

    /// Parses a difficulty string or numeric level loosely.
    pub fn from_str_loose(s: &str) -> Option<Self> {
        let lower = s.to_ascii_lowercase();
        if lower.contains("beginner") || lower == "0" {
            Some(Self::Beginner)
        } else if lower.contains("normal") || lower == "1" {
            Some(Self::Normal)
        } else if lower.contains("hard") && !lower.contains("suicidal") && !lower.contains("hell") || lower == "2" {
            Some(Self::Hard)
        } else if lower.contains("suicidal") || lower == "3" {
            Some(Self::Suicidal)
        } else if lower.contains("hell") || lower.contains("hoe") || lower == "4" {
            Some(Self::HellOnEarth)
        } else {
            None
        }
    }
}

/// Represents a single discovered or queried Killing Floor game server.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerEntry {
    /// Server broadcast title (e.g. "[EU] London Zed House #1 - 24/7 Hard")
    pub server_name: String,
    /// Host IPv4 or domain name (e.g. "192.168.1.100")
    pub ip: String,
    /// Game communication port (default retail: 7707)
    pub port: u16,
    /// Steam / A2S query port (default retail: 7708 or 27015)
    pub query_port: u16,
    /// Active map name (e.g. "KF-BioticsLab", "KF-WestLondon")
    pub map_name: String,
    /// Currently connected player count
    pub player_count: u32,
    /// Maximum allowed player slots (typically 6 for retail KF)
    pub max_players: u32,
    /// Current wave number in the match (1..=max_waves)
    pub current_wave: u32,
    /// Total waves in the match (e.g. 4 for Short, 7 for Medium, 10 for Long)
    pub max_waves: u32,
    /// Game difficulty
    pub difficulty: ServerDifficulty,
    /// Latency / round-trip ping time in milliseconds
    pub ping: u32,
    /// Marked as favorite by player
    pub is_favorite: bool,
    /// Server requires a password to join
    pub is_password_protected: bool,
    /// Valve Anti-Cheat (VAC) secured
    pub is_vac_secured: bool,
    /// Mutator / rule tags
    pub tags: Vec<String>,
}

impl ServerEntry {
    pub fn new(
        server_name: impl Into<String>,
        ip: impl Into<String>,
        port: u16,
        map_name: impl Into<String>,
        player_count: u32,
        max_players: u32,
        current_wave: u32,
        max_waves: u32,
        difficulty: ServerDifficulty,
        ping: u32,
    ) -> Self {
        Self {
            server_name: server_name.into(),
            ip: ip.into(),
            port,
            query_port: port + 1,
            map_name: map_name.into(),
            player_count,
            max_players,
            current_wave,
            max_waves,
            difficulty,
            ping,
            is_favorite: false,
            is_password_protected: false,
            is_vac_secured: true,
            tags: Vec::new(),
        }
    }

    /// Returns `"IP:Port"` network endpoint identifier.
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }

    /// Formatted wave string: e.g. `"4/10"`, `"Boss"`, or `"Lobby"`.
    pub fn wave_display(&self) -> String {
        if self.current_wave == 0 && self.max_waves == 0 {
            "Lobby".to_string()
        } else if self.max_waves > 0 && self.current_wave > self.max_waves {
            "Boss Wave".to_string()
        } else if self.max_waves > 0 && self.current_wave == self.max_waves {
            format!("{}/{} (Final)", self.current_wave, self.max_waves)
        } else {
            format!("{}/{}", self.current_wave, self.max_waves)
        }
    }

    /// Formatted player occupancy string: e.g. `"5/6"`.
    pub fn players_display(&self) -> String {
        format!("{}/{}", self.player_count, self.max_players)
    }
}

/// Server browser columns available for display and sorting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserColumn {
    ServerName,
    Map,
    Players,
    Wave,
    Difficulty,
    Ping,
}

impl BrowserColumn {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ServerName => "Server Name",
            Self::Map => "Map",
            Self::Players => "Players",
            Self::Wave => "Wave",
            Self::Difficulty => "Difficulty",
            Self::Ping => "Ping",
        }
    }

    pub fn default_width(&self) -> f32 {
        match self {
            Self::ServerName => 320.0,
            Self::Map => 160.0,
            Self::Players => 90.0,
            Self::Wave => 110.0,
            Self::Difficulty => 110.0,
            Self::Ping => 70.0,
        }
    }
}

/// Sort direction for server table sorting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub fn toggle(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

/// Top-level navigation tabs in the server browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BrowserTab {
    #[default]
    Internet,
    Favorites,
    History,
    Lan,
}

/// Search and filter criteria for the server browser.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BrowserFilters {
    /// Case-insensitive search text matching server name, map name, or IP.
    pub search_text: String,
    /// Hide servers with 0 players.
    pub hide_empty: bool,
    /// Hide servers that have reached max players.
    pub hide_full: bool,
    /// Hide password-protected servers.
    pub hide_passworded: bool,
    /// Optional difficulty filter.
    pub difficulty: Option<ServerDifficulty>,
    /// Optional maximum ping threshold (in milliseconds).
    pub max_ping: Option<u32>,
}

/// Direct IP connect modal dialog state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectConnectDialog {
    pub is_open: bool,
    pub ip_input: String,
    pub port_input: String,
    pub password_input: String,
    pub error_message: Option<String>,
}

impl Default for DirectConnectDialog {
    fn default() -> Self {
        Self {
            is_open: false,
            ip_input: String::new(),
            port_input: "7707".to_string(),
            password_input: String::new(),
            error_message: None,
        }
    }
}

impl DirectConnectDialog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self) {
        self.is_open = true;
        self.error_message = None;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.error_message = None;
    }

    pub fn validate_and_build(&mut self) -> Result<ConnectTarget, ConnectError> {
        let trimmed_ip = self.ip_input.trim();
        if trimmed_ip.is_empty() {
            let err = ConnectError::EmptyAddress;
            self.error_message = Some("Server IP or hostname cannot be empty".to_string());
            return Err(err);
        }

        let port: u16 = if self.port_input.trim().is_empty() {
            7707
        } else {
            self.port_input.trim().parse::<u16>().map_err(|_| {
                let err = ConnectError::InvalidPort(self.port_input.clone());
                self.error_message = Some("Port must be a valid number between 1 and 65535".to_string());
                err
            })?
        };

        if port == 0 {
            let err = ConnectError::InvalidPort("0".to_string());
            self.error_message = Some("Port 0 is reserved and cannot be used".to_string());
            return Err(err);
        }

        let password = if self.password_input.trim().is_empty() {
            None
        } else {
            Some(self.password_input.trim().to_string())
        };

        self.error_message = None;
        self.is_open = false;

        Ok(ConnectTarget {
            ip: trimmed_ip.to_string(),
            port,
            password,
            server_name: format!("Direct Connect ({}:{})", trimmed_ip, port),
        })
    }
}

/// Errors occurring during connection setup or validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectError {
    EmptyAddress,
    InvalidPort(String),
    ConnectionFailed(String),
}

/// Resulting connection payload containing host IP, port, password, and formatted Unreal URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectTarget {
    pub ip: String,
    pub port: u16,
    pub password: Option<String>,
    pub server_name: String,
}

impl ConnectTarget {
    /// Formats the target into a retail Unreal Engine 2.5 connect string URL.
    /// E.g. `"192.168.1.100:7707?password=secret"` or `"127.0.0.1:7707"`.
    pub fn to_unreal_url(&self) -> String {
        let mut url = format!("{}:{}", self.ip, self.port);
        if let Some(ref pw) = self.password {
            if !pw.is_empty() {
                url.push_str(&format!("?password={}", pw));
            }
        }
        url
    }
}

/// Retail Killing Floor Server Browser Page GUI Component.
pub struct ServerBrowserPage {
    /// Unique component identifier
    pub id: String,
    /// Bounding rectangle in screen pixels
    pub bounds: Rect,
    /// Layout box specification
    pub layout_box: LayoutBox,
    /// Interaction state
    pub state: ComponentState,
    /// Window visibility
    pub visible: bool,
    /// Component enabled toggle
    pub enabled: bool,
    /// Currently selected tab
    pub active_tab: BrowserTab,
    /// Master list of all received servers
    pub servers: Vec<ServerEntry>,
    /// Set of user favorite endpoints in `"IP:Port"` format
    pub favorites: HashSet<String>,
    /// Search and filtering settings
    pub filters: BrowserFilters,
    /// Active sort column
    pub sort_column: BrowserColumn,
    /// Active sort direction
    pub sort_direction: SortDirection,
    /// Index of selected server in the currently filtered view
    pub selected_index: Option<usize>,
    /// Direct connect popup dialog
    pub direct_connect_dialog: DirectConnectDialog,
    /// Last triggered connection target
    pub last_connect_target: Option<ConnectTarget>,
    /// Vertical scroll row offset for the server table
    pub scroll_offset: usize,
    /// Height of each server table row in pixels
    pub row_height: f32,
    /// Whether a server refresh is currently in progress
    pub is_refreshing: bool,
    /// Animation / refresh timer in seconds
    pub refresh_timer: f32,
}

impl Debug for ServerBrowserPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerBrowserPage")
            .field("id", &self.id)
            .field("bounds", &self.bounds)
            .field("visible", &self.visible)
            .field("servers_count", &self.servers.len())
            .field("active_tab", &self.active_tab)
            .field("selected_index", &self.selected_index)
            .field("direct_connect_open", &self.direct_connect_dialog.is_open)
            .finish()
    }
}

impl Default for ServerBrowserPage {
    fn default() -> Self {
        Self::with_bounds(Rect::new(0.0, 0.0, 1024.0, 768.0))
    }
}

impl ServerBrowserPage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_bounds(bounds: Rect) -> Self {
        Self {
            id: "ServerBrowserPage".to_string(),
            bounds,
            layout_box: LayoutBox::absolute(bounds.x, bounds.y, bounds.width, bounds.height),
            state: ComponentState::Normal,
            visible: true,
            enabled: true,
            active_tab: BrowserTab::Internet,
            servers: Vec::new(),
            favorites: HashSet::new(),
            filters: BrowserFilters::default(),
            sort_column: BrowserColumn::Ping,
            sort_direction: SortDirection::Ascending,
            selected_index: None,
            direct_connect_dialog: DirectConnectDialog::new(),
            last_connect_target: None,
            scroll_offset: 0,
            row_height: 24.0,
            is_refreshing: false,
            refresh_timer: 0.0,
        }
    }

    /// Replaces the active server list and synchronizes favorite flags.
    pub fn set_server_list(&mut self, mut servers: Vec<ServerEntry>) {
        for s in &mut servers {
            let ep = s.endpoint();
            s.is_favorite = self.favorites.contains(&ep);
        }
        self.servers = servers;

        // Reset or clamp selection
        let count = self.filtered_servers().len();
        if let Some(idx) = self.selected_index {
            if idx >= count {
                self.selected_index = if count > 0 { Some(count - 1) } else { None };
            }
        }
    }

    /// Adds or updates a single server entry.
    pub fn upsert_server(&mut self, mut server: ServerEntry) {
        server.is_favorite = self.favorites.contains(&server.endpoint());
        if let Some(pos) = self.servers.iter().position(|s| s.endpoint() == server.endpoint()) {
            self.servers[pos] = server;
        } else {
            self.servers.push(server);
        }
    }

    /// Clears all servers from the browser.
    pub fn clear_servers(&mut self) {
        self.servers.clear();
        self.selected_index = None;
    }

    /// Handles clicking the Connect button or double-clicking a server.
    ///
    /// If the direct connect modal dialog is open, attempts validation and returns
    /// the direct connection target.
    /// Otherwise, if a server in the table is selected, returns that server's connection target.
    pub fn on_connect_clicked(&mut self) -> Option<ConnectTarget> {
        if self.direct_connect_dialog.is_open {
            match self.direct_connect_dialog.validate_and_build() {
                Ok(target) => {
                    self.last_connect_target = Some(target.clone());
                    Some(target)
                }
                Err(_) => None,
            }
        } else if let Some(idx) = self.selected_index {
            let filtered = self.filtered_servers();
            if let Some(srv) = filtered.get(idx) {
                let target = ConnectTarget {
                    ip: srv.ip.clone(),
                    port: srv.port,
                    password: None,
                    server_name: srv.server_name.clone(),
                };
                self.last_connect_target = Some(target.clone());
                Some(target)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Returns a vector of references to servers matching active filters and tab, sorted by `sort_column`.
    pub fn filtered_servers(&self) -> Vec<&ServerEntry> {
        let search = self.filters.search_text.trim().to_ascii_lowercase();

        let mut result: Vec<&ServerEntry> = self
            .servers
            .iter()
            .filter(|s| {
                // Tab filter
                match self.active_tab {
                    BrowserTab::Favorites => {
                        if !s.is_favorite && !self.favorites.contains(&s.endpoint()) {
                            return false;
                        }
                    }
                    BrowserTab::Lan => {
                        let is_lan = s.ip.starts_with("192.168.")
                            || s.ip.starts_with("10.")
                            || s.ip.starts_with("172.16.")
                            || s.ip == "127.0.0.1"
                            || s.ip == "localhost";
                        if !is_lan {
                            return false;
                        }
                    }
                    BrowserTab::Internet | BrowserTab::History => {}
                }

                // Search query filter (matches server name, map name, or IP)
                if !search.is_empty() {
                    let name_match = s.server_name.to_ascii_lowercase().contains(&search);
                    let map_match = s.map_name.to_ascii_lowercase().contains(&search);
                    let ip_match = s.ip.contains(&search);
                    if !name_match && !map_match && !ip_match {
                        return false;
                    }
                }

                // Hide empty
                if self.filters.hide_empty && s.player_count == 0 {
                    return false;
                }

                // Hide full
                if self.filters.hide_full && s.player_count >= s.max_players {
                    return false;
                }

                // Hide passworded
                if self.filters.hide_passworded && s.is_password_protected {
                    return false;
                }

                // Difficulty filter
                if let Some(req_diff) = self.filters.difficulty {
                    if s.difficulty != req_diff {
                        return false;
                    }
                }

                // Max ping filter
                if let Some(max_p) = self.filters.max_ping {
                    if s.ping > max_p {
                        return false;
                    }
                }

                true
            })
            .collect();

        // Sort results
        let dir = self.sort_direction;
        result.sort_by(|a, b| {
            let cmp = match self.sort_column {
                BrowserColumn::ServerName => a.server_name.to_lowercase().cmp(&b.server_name.to_lowercase()),
                BrowserColumn::Map => a.map_name.to_lowercase().cmp(&b.map_name.to_lowercase()),
                BrowserColumn::Players => a.player_count.cmp(&b.player_count),
                BrowserColumn::Wave => a.current_wave.cmp(&b.current_wave),
                BrowserColumn::Difficulty => a.difficulty.cmp(&b.difficulty),
                BrowserColumn::Ping => a.ping.cmp(&b.ping),
            };

            match dir {
                SortDirection::Ascending => cmp,
                SortDirection::Descending => cmp.reverse(),
            }
        });

        result
    }

    /// Changes or toggles sort column.
    pub fn sort_by(&mut self, col: BrowserColumn) {
        if self.sort_column == col {
            self.sort_direction = self.sort_direction.toggle();
        } else {
            self.sort_column = col;
            self.sort_direction = match col {
                BrowserColumn::Players | BrowserColumn::Wave => SortDirection::Descending,
                _ => SortDirection::Ascending,
            };
        }
    }

    /// Toggles the favorite status of a server endpoint.
    pub fn toggle_favorite(&mut self, endpoint: &str) {
        if self.favorites.contains(endpoint) {
            self.favorites.remove(endpoint);
        } else {
            self.favorites.insert(endpoint.to_string());
        }

        for s in &mut self.servers {
            if s.endpoint() == endpoint {
                s.is_favorite = self.favorites.contains(endpoint);
            }
        }
    }

    pub fn is_favorite(&self, endpoint: &str) -> bool {
        self.favorites.contains(endpoint)
    }

    pub fn add_favorite(&mut self, endpoint: impl Into<String>) {
        let ep = endpoint.into();
        self.favorites.insert(ep.clone());
        for s in &mut self.servers {
            if s.endpoint() == ep {
                s.is_favorite = true;
            }
        }
    }

    pub fn remove_favorite(&mut self, endpoint: &str) {
        self.favorites.remove(endpoint);
        for s in &mut self.servers {
            if s.endpoint() == endpoint {
                s.is_favorite = false;
            }
        }
    }

    /// Selects a server in the filtered view by index.
    pub fn select_server(&mut self, index: usize) {
        let count = self.filtered_servers().len();
        if index < count {
            self.selected_index = Some(index);
        }
    }

    /// Returns the currently selected server, if any.
    pub fn selected_server(&self) -> Option<&ServerEntry> {
        let idx = self.selected_index?;
        let filtered = self.filtered_servers();
        filtered.get(idx).copied()
    }

    /// Sets the search filter text.
    pub fn set_search_text(&mut self, text: impl Into<String>) {
        self.filters.search_text = text.into();
        self.selected_index = None;
    }

    /// Switches navigation tab.
    pub fn set_tab(&mut self, tab: BrowserTab) {
        self.active_tab = tab;
        self.selected_index = None;
        self.scroll_offset = 0;
    }

    /// Opens the direct connect dialog.
    pub fn open_direct_connect_dialog(&mut self) {
        self.direct_connect_dialog.open();
    }

    /// Closes the direct connect dialog.
    pub fn close_direct_connect_dialog(&mut self) {
        self.direct_connect_dialog.close();
    }

    /// Sets direct connect IP.
    pub fn set_direct_connect_ip(&mut self, ip: impl Into<String>) {
        self.direct_connect_dialog.ip_input = ip.into();
    }

    /// Sets direct connect port.
    pub fn set_direct_connect_port(&mut self, port: u16) {
        self.direct_connect_dialog.port_input = port.to_string();
    }

    /// Sets direct connect password.
    pub fn set_direct_connect_password(&mut self, password: impl Into<String>) {
        self.direct_connect_dialog.password_input = password.into();
    }

    /// Confirms direct connect input.
    pub fn confirm_direct_connect(&mut self) -> Result<ConnectTarget, ConnectError> {
        self.direct_connect_dialog.validate_and_build()
    }

    /// Triggers simulated server refresh.
    pub fn trigger_refresh(&mut self) {
        self.is_refreshing = true;
        self.refresh_timer = 0.0;
    }

    /// Updates periodic logic like refresh timers.
    pub fn update(&mut self, delta_time: f32) {
        if self.is_refreshing {
            self.refresh_timer += delta_time;
            if self.refresh_timer >= 1.5 {
                self.is_refreshing = false;
            }
        }
    }

    // --- Layout & Hit Testing Helpers ---

    /// Returns the rectangle of the tab bar.
    pub fn tab_bar_rect(&self) -> Rect {
        Rect::new(self.bounds.x + 10.0, self.bounds.y + 40.0, self.bounds.width - 20.0, 30.0)
    }

    /// Returns the rectangle of the search / filter bar.
    pub fn filter_bar_rect(&self) -> Rect {
        Rect::new(self.bounds.x + 10.0, self.bounds.y + 75.0, self.bounds.width - 20.0, 36.0)
    }

    /// Returns the rectangle of the table header.
    pub fn table_header_rect(&self) -> Rect {
        Rect::new(self.bounds.x + 10.0, self.bounds.y + 115.0, self.bounds.width - 20.0, 26.0)
    }

    /// Returns the rectangle of the server table body.
    pub fn table_body_rect(&self) -> Rect {
        Rect::new(
            self.bounds.x + 10.0,
            self.bounds.y + 142.0,
            self.bounds.width - 20.0,
            self.bounds.height - 200.0,
        )
    }

    /// Max visible rows fitting in the table body.
    pub fn max_visible_rows(&self) -> usize {
        let body = self.table_body_rect();
        if self.row_height <= 0.0 {
            1
        } else {
            (body.height / self.row_height).floor() as usize
        }
    }

    /// Returns the rectangle of the bottom toolbar (Refresh, Connect, Direct IP).
    pub fn bottom_toolbar_rect(&self) -> Rect {
        Rect::new(
            self.bounds.x + 10.0,
            self.bounds.y + self.bounds.height - 50.0,
            self.bounds.width - 20.0,
            40.0,
        )
    }

    /// Returns the bounds for the Direct Connect modal dialog.
    pub fn direct_connect_modal_rect(&self) -> Rect {
        let width = 420.0;
        let height = 240.0;
        let x = self.bounds.x + (self.bounds.width - width) * 0.5;
        let y = self.bounds.y + (self.bounds.height - height) * 0.5;
        Rect::new(x, y, width, height)
    }

    /// Handles mouse click event at `(x, y)`.
    pub fn handle_click(&mut self, x: f32, y: f32) -> bool {
        if !self.visible {
            return false;
        }

        // If direct connect modal dialog is open, handle clicks inside or close
        if self.direct_connect_dialog.is_open {
            let modal = self.direct_connect_modal_rect();
            if modal.contains(x, y) {
                // Connect button inside dialog
                let btn_connect = Rect::new(modal.x + 100.0, modal.y + modal.height - 45.0, 100.0, 30.0);
                let btn_cancel = Rect::new(modal.x + 220.0, modal.y + modal.height - 45.0, 100.0, 30.0);

                if btn_connect.contains(x, y) {
                    let _ = self.on_connect_clicked();
                    return true;
                } else if btn_cancel.contains(x, y) {
                    self.close_direct_connect_dialog();
                    return true;
                }
                return true;
            }
            return true;
        }

        // Check Tab clicks
        let tab_bar = self.tab_bar_rect();
        if tab_bar.contains(x, y) {
            let tab_width = 110.0;
            let tabs = [
                BrowserTab::Internet,
                BrowserTab::Favorites,
                BrowserTab::History,
                BrowserTab::Lan,
            ];
            for (i, tab) in tabs.iter().enumerate() {
                let rect = Rect::new(tab_bar.x + (i as f32) * tab_width, tab_bar.y, tab_width, tab_bar.height);
                if rect.contains(x, y) {
                    self.set_tab(*tab);
                    return true;
                }
            }
        }

        // Check Column header clicks for sorting
        let header = self.table_header_rect();
        if header.contains(x, y) {
            let columns = [
                BrowserColumn::ServerName,
                BrowserColumn::Map,
                BrowserColumn::Players,
                BrowserColumn::Wave,
                BrowserColumn::Difficulty,
                BrowserColumn::Ping,
            ];
            let mut cur_x = header.x;
            for col in &columns {
                let col_width = col.default_width();
                let col_rect = Rect::new(cur_x, header.y, col_width, header.height);
                if col_rect.contains(x, y) {
                    self.sort_by(*col);
                    return true;
                }
                cur_x += col_width;
            }
        }

        // Check Server table row clicks
        let body = self.table_body_rect();
        if body.contains(x, y) {
            let rel_y = y - body.y;
            if rel_y >= 0.0 {
                let row_idx = (rel_y / self.row_height) as usize;
                let target_idx = self.scroll_offset + row_idx;
                let count = self.filtered_servers().len();
                if target_idx < count {
                    self.select_server(target_idx);
                    return true;
                }
            }
        }

        // Check Bottom toolbar buttons
        let toolbar = self.bottom_toolbar_rect();
        if toolbar.contains(x, y) {
            let btn_refresh = Rect::new(toolbar.x, toolbar.y + 5.0, 100.0, 30.0);
            let btn_direct = Rect::new(toolbar.x + 110.0, toolbar.y + 5.0, 110.0, 30.0);
            let btn_connect = Rect::new(toolbar.x + toolbar.width - 120.0, toolbar.y + 5.0, 120.0, 30.0);

            if btn_refresh.contains(x, y) {
                self.trigger_refresh();
                return true;
            } else if btn_direct.contains(x, y) {
                self.open_direct_connect_dialog();
                return true;
            } else if btn_connect.contains(x, y) {
                let _ = self.on_connect_clicked();
                return true;
            }
        }

        false
    }

    /// Handles double-click on server row to directly trigger connection.
    pub fn handle_double_click(&mut self, x: f32, y: f32) -> Option<ConnectTarget> {
        let body = self.table_body_rect();
        if body.contains(x, y) {
            let rel_y = y - body.y;
            if rel_y >= 0.0 {
                let row_idx = (rel_y / self.row_height) as usize;
                let target_idx = self.scroll_offset + row_idx;
                let count = self.filtered_servers().len();
                if target_idx < count {
                    self.select_server(target_idx);
                    return self.on_connect_clicked();
                }
            }
        }
        None
    }

    /// Internal rendering of UI quads and text into a [`Canvas`].
    pub fn render_canvas(&self, canvas: &mut Canvas) {
        // 1. Background Window Quad (Dark KF metallic charcoal)
        canvas.draw_filled_rect(self.bounds, [0.08, 0.08, 0.09, 0.98]);
        canvas.draw_border(self.bounds, 1.5, [0.45, 0.12, 0.12, 1.0]);

        // 2. Title Header Quad (KF Crimson Red)
        let header_rect = Rect::new(self.bounds.x, self.bounds.y, self.bounds.width, 36.0);
        canvas.draw_filled_rect(header_rect, [0.22, 0.04, 0.04, 1.0]);
        canvas.draw_text(
            "KILLING FLOOR - SERVER BROWSER",
            [header_rect.x + 16.0, header_rect.y + 18.0],
            15.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Left,
        );

        // 3. Tab Bar Quads
        let tab_bar = self.tab_bar_rect();
        let tab_width = 110.0;
        let tabs = [
            (BrowserTab::Internet, "Internet"),
            (BrowserTab::Favorites, "Favorites"),
            (BrowserTab::History, "History"),
            (BrowserTab::Lan, "LAN"),
        ];
        for (i, (tab, title)) in tabs.iter().enumerate() {
            let rect = Rect::new(tab_bar.x + (i as f32) * tab_width, tab_bar.y, tab_width - 4.0, tab_bar.height);
            let is_active = *tab == self.active_tab;
            let color = if is_active {
                [0.40, 0.08, 0.08, 1.0]
            } else {
                [0.14, 0.14, 0.16, 1.0]
            };
            canvas.draw_filled_rect(rect, color);
            canvas.draw_border(rect, 1.0, [0.3, 0.3, 0.35, 1.0]);
            canvas.draw_text(
                *title,
                [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5],
                13.0,
                if is_active { [1.0, 1.0, 1.0, 1.0] } else { [0.7, 0.7, 0.7, 1.0] },
                TextAlignment::Center,
            );
        }

        // 4. Search / Filter bar background
        let filter_bar = self.filter_bar_rect();
        canvas.draw_filled_rect(filter_bar, [0.12, 0.12, 0.14, 1.0]);
        canvas.draw_border(filter_bar, 1.0, [0.25, 0.25, 0.28, 1.0]);

        let filter_text = if self.filters.search_text.is_empty() {
            "Filter: <All Maps & Names>".to_string()
        } else {
            format!("Filter: \"{}\"", self.filters.search_text)
        };
        canvas.draw_text(
            &filter_text,
            [filter_bar.x + 12.0, filter_bar.y + filter_bar.height * 0.5],
            13.0,
            [0.85, 0.85, 0.85, 1.0],
            TextAlignment::Left,
        );

        // 5. Table Header Quads
        let table_hdr = self.table_header_rect();
        canvas.draw_filled_rect(table_hdr, [0.16, 0.16, 0.19, 1.0]);
        canvas.draw_border(table_hdr, 1.0, [0.35, 0.35, 0.40, 1.0]);

        let columns = [
            (BrowserColumn::ServerName, 320.0),
            (BrowserColumn::Map, 160.0),
            (BrowserColumn::Players, 90.0),
            (BrowserColumn::Wave, 110.0),
            (BrowserColumn::Difficulty, 110.0),
            (BrowserColumn::Ping, 70.0),
        ];

        let mut cur_x = table_hdr.x;
        for (col, w) in &columns {
            let is_sorted = self.sort_column == *col;
            let arrow = if is_sorted {
                match self.sort_direction {
                    SortDirection::Ascending => " ▲",
                    SortDirection::Descending => " ▼",
                }
            } else {
                ""
            };
            let col_title = format!("{}{}", col.as_str(), arrow);
            canvas.draw_text(
                &col_title,
                [cur_x + 8.0, table_hdr.y + table_hdr.height * 0.5],
                12.0,
                if is_sorted { [1.0, 0.85, 0.3, 1.0] } else { [0.85, 0.85, 0.85, 1.0] },
                TextAlignment::Left,
            );
            cur_x += w;
        }

        // 6. Server Table Rows
        let body = self.table_body_rect();
        canvas.draw_filled_rect(body, [0.06, 0.06, 0.07, 1.0]);
        canvas.draw_border(body, 1.0, [0.25, 0.25, 0.28, 1.0]);

        let filtered = self.filtered_servers();
        let max_visible = self.max_visible_rows();
        let start = self.scroll_offset;
        let end = (start + max_visible).min(filtered.len());

        for i in start..end {
            let row_idx = i - start;
            let row_y = body.y + (row_idx as f32) * self.row_height;
            let row_rect = Rect::new(body.x + 1.0, row_y, body.width - 2.0, self.row_height - 1.0);

            let is_selected = self.selected_index == Some(i);
            let row_color = if is_selected {
                [0.45, 0.12, 0.12, 0.95] // Selected row crimson
            } else if i % 2 == 0 {
                [0.10, 0.10, 0.12, 0.8] // Alternating dark row
            } else {
                [0.08, 0.08, 0.09, 0.8]
            };
            canvas.draw_filled_rect(row_rect, row_color);

            let srv = filtered[i];
            let mut col_x = row_rect.x;

            // Col 1: Server Name
            let name_display = if srv.is_favorite {
                format!("★ {}", srv.server_name)
            } else {
                srv.server_name.clone()
            };
            canvas.draw_text(
                &name_display,
                [col_x + 6.0, row_y + self.row_height * 0.5],
                12.0,
                if is_selected { [1.0, 1.0, 1.0, 1.0] } else { [0.9, 0.9, 0.9, 1.0] },
                TextAlignment::Left,
            );
            col_x += 320.0;

            // Col 2: Map
            canvas.draw_text(
                &srv.map_name,
                [col_x + 6.0, row_y + self.row_height * 0.5],
                12.0,
                [0.85, 0.85, 0.85, 1.0],
                TextAlignment::Left,
            );
            col_x += 160.0;

            // Col 3: Players
            canvas.draw_text(
                srv.players_display(),
                [col_x + 6.0, row_y + self.row_height * 0.5],
                12.0,
                [0.85, 0.85, 0.85, 1.0],
                TextAlignment::Left,
            );
            col_x += 90.0;

            // Col 4: Wave
            canvas.draw_text(
                srv.wave_display(),
                [col_x + 6.0, row_y + self.row_height * 0.5],
                12.0,
                [0.85, 0.85, 0.85, 1.0],
                TextAlignment::Left,
            );
            col_x += 110.0;

            // Col 5: Difficulty
            canvas.draw_text(
                srv.difficulty.as_str(),
                [col_x + 6.0, row_y + self.row_height * 0.5],
                12.0,
                [0.85, 0.85, 0.85, 1.0],
                TextAlignment::Left,
            );
            col_x += 110.0;

            // Col 6: Ping
            canvas.draw_text(
                format!("{} ms", srv.ping),
                [col_x + 6.0, row_y + self.row_height * 0.5],
                12.0,
                if srv.ping < 50 {
                    [0.2, 0.9, 0.2, 1.0] // Green for low ping
                } else if srv.ping < 120 {
                    [0.9, 0.8, 0.2, 1.0] // Yellow
                } else {
                    [0.9, 0.3, 0.2, 1.0] // Red for high ping
                },
                TextAlignment::Left,
            );
        }

        // 7. Bottom toolbar
        let toolbar = self.bottom_toolbar_rect();
        canvas.draw_filled_rect(toolbar, [0.12, 0.12, 0.14, 1.0]);
        canvas.draw_border(toolbar, 1.0, [0.3, 0.3, 0.35, 1.0]);

        // Refresh button
        let btn_refresh = Rect::new(toolbar.x + 5.0, toolbar.y + 5.0, 95.0, 30.0);
        let refresh_label = if self.is_refreshing { "Refreshing..." } else { "Refresh" };
        canvas.draw_filled_rect(btn_refresh, [0.18, 0.18, 0.22, 1.0]);
        canvas.draw_border(btn_refresh, 1.0, [0.4, 0.4, 0.45, 1.0]);
        canvas.draw_text(
            refresh_label,
            [btn_refresh.x + btn_refresh.width * 0.5, btn_refresh.y + btn_refresh.height * 0.5],
            13.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Center,
        );

        // Direct IP button
        let btn_direct = Rect::new(toolbar.x + 110.0, toolbar.y + 5.0, 110.0, 30.0);
        canvas.draw_filled_rect(btn_direct, [0.18, 0.18, 0.22, 1.0]);
        canvas.draw_border(btn_direct, 1.0, [0.4, 0.4, 0.45, 1.0]);
        canvas.draw_text(
            "Direct IP...",
            [btn_direct.x + btn_direct.width * 0.5, btn_direct.y + btn_direct.height * 0.5],
            13.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Center,
        );

        // Connect button
        let btn_connect = Rect::new(toolbar.x + toolbar.width - 120.0, toolbar.y + 5.0, 115.0, 30.0);
        canvas.draw_filled_rect(btn_connect, [0.45, 0.10, 0.10, 1.0]);
        canvas.draw_border(btn_connect, 1.0, [0.7, 0.2, 0.2, 1.0]);
        canvas.draw_text(
            "Connect",
            [btn_connect.x + btn_connect.width * 0.5, btn_connect.y + btn_connect.height * 0.5],
            14.0,
            [1.0, 1.0, 1.0, 1.0],
            TextAlignment::Center,
        );

        // 8. Direct Connect Modal Overlay
        if self.direct_connect_dialog.is_open {
            // Darkened backdrop
            canvas.draw_filled_rect(self.bounds, [0.0, 0.0, 0.0, 0.65]);

            // Modal dialog window
            let modal = self.direct_connect_modal_rect();
            canvas.draw_filled_rect(modal, [0.12, 0.12, 0.15, 1.0]);
            canvas.draw_border(modal, 1.5, [0.5, 0.15, 0.15, 1.0]);

            // Modal title bar
            let modal_title = Rect::new(modal.x, modal.y, modal.width, 32.0);
            canvas.draw_filled_rect(modal_title, [0.30, 0.06, 0.06, 1.0]);
            canvas.draw_text(
                "DIRECT IP CONNECT",
                [modal_title.x + 12.0, modal_title.y + 16.0],
                13.0,
                [1.0, 1.0, 1.0, 1.0],
                TextAlignment::Left,
            );

            // IP Input Box
            let ip_box = Rect::new(modal.x + 20.0, modal.y + 60.0, modal.width - 40.0, 28.0);
            canvas.draw_filled_rect(ip_box, [0.06, 0.06, 0.08, 1.0]);
            canvas.draw_border(ip_box, 1.0, [0.35, 0.35, 0.40, 1.0]);
            let ip_display = if self.direct_connect_dialog.ip_input.is_empty() {
                "e.g. 192.168.1.100 or kf.game.net".to_string()
            } else {
                self.direct_connect_dialog.ip_input.clone()
            };
            canvas.draw_text(
                &ip_display,
                [ip_box.x + 8.0, ip_box.y + ip_box.height * 0.5],
                12.0,
                if self.direct_connect_dialog.ip_input.is_empty() { [0.5, 0.5, 0.5, 1.0] } else { [1.0, 1.0, 1.0, 1.0] },
                TextAlignment::Left,
            );

            // Port Input Box
            let port_box = Rect::new(modal.x + 20.0, modal.y + 115.0, 120.0, 28.0);
            canvas.draw_filled_rect(port_box, [0.06, 0.06, 0.08, 1.0]);
            canvas.draw_border(port_box, 1.0, [0.35, 0.35, 0.40, 1.0]);
            canvas.draw_text(
                &self.direct_connect_dialog.port_input,
                [port_box.x + 8.0, port_box.y + port_box.height * 0.5],
                12.0,
                [1.0, 1.0, 1.0, 1.0],
                TextAlignment::Left,
            );

            // Error message if any
            if let Some(ref err) = self.direct_connect_dialog.error_message {
                canvas.draw_text(
                    err,
                    [modal.x + 20.0, modal.y + 160.0],
                    11.0,
                    [0.95, 0.25, 0.25, 1.0],
                    TextAlignment::Left,
                );
            }

            // Connect & Cancel buttons
            let btn_confirm = Rect::new(modal.x + 100.0, modal.y + modal.height - 45.0, 100.0, 30.0);
            canvas.draw_filled_rect(btn_confirm, [0.45, 0.10, 0.10, 1.0]);
            canvas.draw_border(btn_confirm, 1.0, [0.7, 0.2, 0.2, 1.0]);
            canvas.draw_text(
                "Connect",
                [btn_confirm.x + btn_confirm.width * 0.5, btn_confirm.y + btn_confirm.height * 0.5],
                13.0,
                [1.0, 1.0, 1.0, 1.0],
                TextAlignment::Center,
            );

            let btn_cancel = Rect::new(modal.x + 220.0, modal.y + modal.height - 45.0, 100.0, 30.0);
            canvas.draw_filled_rect(btn_cancel, [0.20, 0.20, 0.25, 1.0]);
            canvas.draw_border(btn_cancel, 1.0, [0.4, 0.4, 0.45, 1.0]);
            canvas.draw_text(
                "Cancel",
                [btn_cancel.x + btn_cancel.width * 0.5, btn_cancel.y + btn_cancel.height * 0.5],
                13.0,
                [1.0, 1.0, 1.0, 1.0],
                TextAlignment::Center,
            );
        }
    }

    /// Generates modern 2D [`UIVertex`] vertex and index buffers for rendering.
    pub fn generate_ui_vertices(&self) -> (Vec<UIVertex>, Vec<u32>) {
        let mut canvas = Canvas::new(self.bounds.width, self.bounds.height);
        self.render_canvas(&mut canvas);
        (canvas.vertices, canvas.indices)
    }

    /// Direct low-level render to modern hardware [`RenderInterface`].
    pub fn render_hal(&self, interface: &mut dyn RenderInterface) -> Result<(), RenderError> {
        if !self.visible {
            return Ok(());
        }

        // Configure viewport matching page bounds
        interface.set_viewport(Viewport {
            x: self.bounds.x,
            y: self.bounds.y,
            width: self.bounds.width,
            height: self.bounds.height,
            min_depth: 0.0,
            max_depth: 1.0,
        });

        // Set 2D orthographic projection matrix (screen-space pixels)
        let ortho = Mat4::orthographic_rh(
            0.0,
            self.bounds.width,
            self.bounds.height,
            0.0,
            -1.0,
            1.0,
        );
        interface.set_transform(TransformType::Projection, ortho);
        interface.set_transform(TransformType::View, Mat4::IDENTITY);
        interface.set_transform(TransformType::World, Mat4::IDENTITY);

        let (vertices, indices) = self.generate_ui_vertices();
        if !indices.is_empty() {
            let num_primitives = (indices.len() / 3) as u32;
            interface.draw_indexed_primitive(
                PrimitiveType::TriangleList,
                0,
                num_primitives,
                0,
                vertices.len() as u32,
            );
        }

        Ok(())
    }
}

// --- Implementation of GUIComponent Trait ---

impl GUIComponent for ServerBrowserPage {
    fn id(&self) -> &str {
        &self.id
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.layout_box = LayoutBox::absolute(bounds.x, bounds.y, bounds.width, bounds.height);
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
            MouseEvent::Down { button: MouseButton::Left, x, y } => {
                self.handle_click(x, y)
            }
            MouseEvent::Scroll { delta, x, y } => {
                let body = self.table_body_rect();
                if body.contains(x, y) {
                    if delta > 0.0 && self.scroll_offset > 0 {
                        self.scroll_offset -= 1;
                    } else if delta < 0.0 {
                        let filtered_len = self.filtered_servers().len();
                        let max_vis = self.max_visible_rows();
                        if self.scroll_offset + max_vis < filtered_len {
                            self.scroll_offset += 1;
                        }
                    }
                    true
                } else {
                    false
                }
            }
            MouseEvent::Move { x, y } => {
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
        if !self.visible {
            return;
        }
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

    fn sample_servers() -> Vec<ServerEntry> {
        vec![
            ServerEntry::new(
                "[EU] London Zed House #1 - 24/7 Hard",
                "192.168.1.10",
                7707,
                "KF-WestLondon",
                5,
                6,
                4,
                10,
                ServerDifficulty::Hard,
                35,
            ),
            ServerEntry::new(
                "[US] Biotics Lab Survival (Normal)",
                "10.0.0.5",
                7707,
                "KF-BioticsLab",
                6,
                6,
                10,
                10,
                ServerDifficulty::Normal,
                110,
            ),
            ServerEntry::new(
                "[RU] Hell on Earth Meat Grinder",
                "185.220.101.5",
                7707,
                "KF-Farm",
                0,
                6,
                1,
                10,
                ServerDifficulty::HellOnEarth,
                180,
            ),
            ServerEntry::new(
                "[FR] Paris Manor Casual (Beginner)",
                "192.168.1.20",
                7707,
                "KF-Manor",
                2,
                6,
                0,
                0,
                ServerDifficulty::Beginner,
                20,
            ),
            ServerEntry::new(
                "[DE] Suicidal Offices Challenge",
                "178.62.200.1",
                7707,
                "KF-Offices",
                3,
                6,
                11,
                10,
                ServerDifficulty::Suicidal,
                45,
            ),
        ]
    }

    #[test]
    fn test_server_entry_displays() {
        let mut s = ServerEntry::new(
            "Test Server",
            "127.0.0.1",
            7707,
            "KF-BioticsLab",
            4,
            6,
            7,
            10,
            ServerDifficulty::Hard,
            42,
        );
        assert_eq!(s.endpoint(), "127.0.0.1:7707");
        assert_eq!(s.players_display(), "4/6");
        assert_eq!(s.wave_display(), "7/10");

        // Boss wave test
        s.current_wave = 11;
        assert_eq!(s.wave_display(), "Boss Wave");

        // Final wave test
        s.current_wave = 10;
        assert_eq!(s.wave_display(), "10/10 (Final)");

        // Lobby test
        s.current_wave = 0;
        s.max_waves = 0;
        assert_eq!(s.wave_display(), "Lobby");
    }

    #[test]
    fn test_server_browser_page_initial_state() {
        let page = ServerBrowserPage::new();
        assert!(page.is_visible());
        assert_eq!(page.servers.len(), 0);
        assert_eq!(page.active_tab, BrowserTab::Internet);
        assert_eq!(page.sort_column, BrowserColumn::Ping);
        assert_eq!(page.sort_direction, SortDirection::Ascending);
        assert_eq!(page.selected_index, None);
        assert!(!page.direct_connect_dialog.is_open);
    }

    #[test]
    fn test_set_server_list_and_sorting() {
        let mut page = ServerBrowserPage::new();
        page.set_server_list(sample_servers());
        assert_eq!(page.servers.len(), 5);

        // Default sort is Ping Ascending
        let filtered = page.filtered_servers();
        assert_eq!(filtered.len(), 5);
        assert_eq!(filtered[0].server_name, "[FR] Paris Manor Casual (Beginner)"); // ping 20
        assert_eq!(filtered[1].server_name, "[EU] London Zed House #1 - 24/7 Hard"); // ping 35
        assert_eq!(filtered[2].server_name, "[DE] Suicidal Offices Challenge"); // ping 45
        assert_eq!(filtered[3].server_name, "[US] Biotics Lab Survival (Normal)"); // ping 110
        assert_eq!(filtered[4].server_name, "[RU] Hell on Earth Meat Grinder"); // ping 180

        // Toggle sort on Ping -> Descending
        page.sort_by(BrowserColumn::Ping);
        assert_eq!(page.sort_direction, SortDirection::Descending);
        let sorted_desc = page.filtered_servers();
        assert_eq!(sorted_desc[0].ping, 180);
        assert_eq!(sorted_desc[4].ping, 20);

        // Sort by Players (defaults to Descending)
        page.sort_by(BrowserColumn::Players);
        let players_sorted = page.filtered_servers();
        assert_eq!(players_sorted[0].player_count, 6);
        assert_eq!(players_sorted[4].player_count, 0);

        // Sort by Server Name Ascending
        page.sort_by(BrowserColumn::ServerName);
        let name_sorted = page.filtered_servers();
        assert!(name_sorted[0].server_name.contains("[DE]"));
        assert!(name_sorted[4].server_name.contains("[US]"));
    }

    #[test]
    fn test_search_and_filters() {
        let mut page = ServerBrowserPage::new();
        page.set_server_list(sample_servers());

        // Search text matching server name
        page.set_search_text("London");
        let res = page.filtered_servers();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].server_name, "[EU] London Zed House #1 - 24/7 Hard");

        // Search text matching map name
        page.set_search_text("biotics");
        let res_map = page.filtered_servers();
        assert_eq!(res_map.len(), 1);
        assert_eq!(res_map[0].map_name, "KF-BioticsLab");

        // Clear search text
        page.set_search_text("");

        // Hide full servers (US server has 6/6)
        page.filters.hide_full = true;
        let non_full = page.filtered_servers();
        assert_eq!(non_full.len(), 4);
        assert!(!non_full.iter().any(|s| s.player_count >= s.max_players));

        // Hide empty servers (RU server has 0 players)
        page.filters.hide_empty = true;
        let non_empty = page.filtered_servers();
        assert_eq!(non_empty.len(), 3);
        assert!(!non_empty.iter().any(|s| s.player_count == 0));

        // Difficulty filter
        page.filters.difficulty = Some(ServerDifficulty::Hard);
        let hard_only = page.filtered_servers();
        assert_eq!(hard_only.len(), 1);
        assert_eq!(hard_only[0].difficulty, ServerDifficulty::Hard);

        // Max ping filter
        page.filters.difficulty = None;
        page.filters.hide_full = false;
        page.filters.hide_empty = false;
        page.filters.max_ping = Some(40);
        let low_ping = page.filtered_servers();
        assert_eq!(low_ping.len(), 2); // 20ms and 35ms
    }

    #[test]
    fn test_favorites_tab_and_management() {
        let mut page = ServerBrowserPage::new();
        page.set_server_list(sample_servers());

        let ep = "192.168.1.10:7707";
        assert!(!page.is_favorite(ep));

        // Add favorite
        page.toggle_favorite(ep);
        assert!(page.is_favorite(ep));
        assert!(page.servers.iter().find(|s| s.endpoint() == ep).unwrap().is_favorite);

        // Switch to Favorites tab
        page.set_tab(BrowserTab::Favorites);
        let favs = page.filtered_servers();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].endpoint(), ep);

        // Remove favorite
        page.remove_favorite(ep);
        assert!(!page.is_favorite(ep));
        let favs_empty = page.filtered_servers();
        assert_eq!(favs_empty.len(), 0);
    }

    #[test]
    fn test_direct_connect_dialog_validation() {
        let mut page = ServerBrowserPage::new();

        page.open_direct_connect_dialog();
        assert!(page.direct_connect_dialog.is_open);

        // Test empty IP error
        let err = page.confirm_direct_connect().unwrap_err();
        assert_eq!(err, ConnectError::EmptyAddress);
        assert!(page.direct_connect_dialog.error_message.is_some());

        // Test invalid port
        page.set_direct_connect_ip("192.168.1.50");
        page.direct_connect_dialog.port_input = "not_a_port".to_string();
        let port_err = page.confirm_direct_connect().unwrap_err();
        assert!(matches!(port_err, ConnectError::InvalidPort(_)));

        // Test valid direct connect
        page.set_direct_connect_port(7707);
        page.set_direct_connect_password("killingfloor123");
        let target = page.confirm_direct_connect().unwrap();
        assert_eq!(target.ip, "192.168.1.50");
        assert_eq!(target.port, 7707);
        assert_eq!(target.password, Some("killingfloor123".to_string()));
        assert_eq!(target.to_unreal_url(), "192.168.1.50:7707?password=killingfloor123");
        assert!(!page.direct_connect_dialog.is_open);
    }

    #[test]
    fn test_on_connect_clicked_selection() {
        let mut page = ServerBrowserPage::new();
        page.set_server_list(sample_servers());

        // When nothing is selected, returns None
        assert_eq!(page.on_connect_clicked(), None);

        // Select row 0 (which is the lowest ping server by default sort: Paris Manor)
        page.select_server(0);
        assert!(page.selected_server().is_some());

        let target = page.on_connect_clicked().unwrap();
        assert_eq!(target.ip, "192.168.1.20");
        assert_eq!(target.port, 7707);
        assert_eq!(target.to_unreal_url(), "192.168.1.20:7707");
    }

    #[test]
    fn test_on_connect_clicked_modal_precedence() {
        let mut page = ServerBrowserPage::new();
        page.set_server_list(sample_servers());
        page.select_server(0);

        // Open direct connect modal dialog
        page.open_direct_connect_dialog();
        page.set_direct_connect_ip("play.zedhouse.net");
        page.set_direct_connect_port(7707);

        // on_connect_clicked should prioritize the active direct connect dialog
        let target = page.on_connect_clicked().unwrap();
        assert_eq!(target.ip, "play.zedhouse.net");
        assert_eq!(target.port, 7707);
        assert_eq!(target.server_name, "Direct Connect (play.zedhouse.net:7707)");
    }

    #[test]
    fn test_gui_component_lifecycle_and_ui_vertices() {
        let mut page = ServerBrowserPage::new();
        page.set_server_list(sample_servers());

        assert_eq!(page.id(), "ServerBrowserPage");
        assert!(page.is_visible());

        page.set_visible(false);
        assert!(!page.is_visible());
        page.set_visible(true);

        // Update timer
        page.trigger_refresh();
        assert!(page.is_refreshing);
        page.update(1.6);
        assert!(!page.is_refreshing);

        // Generate UI vertices
        let (vertices, indices) = page.generate_ui_vertices();
        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
        // Each quad is 4 vertices and 6 indices
        assert_eq!(indices.len() % 6, 0);
        assert_eq!(vertices.len() % 4, 0);

        // Modal dialog adds additional quads
        page.open_direct_connect_dialog();
        let (modal_vertices, modal_indices) = page.generate_ui_vertices();
        assert!(modal_vertices.len() > vertices.len());
        assert!(modal_indices.len() > indices.len());
    }

    #[test]
    fn test_mouse_input_handling() {
        let mut page = ServerBrowserPage::new();
        page.set_server_list(sample_servers());

        // Click on row 0 in table body
        let body = page.table_body_rect();
        let click_y = body.y + 10.0;
        let event = MouseEvent::Down {
            button: MouseButton::Left,
            x: body.x + 50.0,
            y: click_y,
        };
        let handled = page.handle_mouse_event(&event);
        assert!(handled);
        assert_eq!(page.selected_index, Some(0));

        // Double-click row 1
        let dbl_click_y = body.y + 24.0 + 10.0;
        let target = page.handle_double_click(body.x + 50.0, dbl_click_y);
        assert!(target.is_some());
        assert_eq!(page.selected_index, Some(1));
    }
}
