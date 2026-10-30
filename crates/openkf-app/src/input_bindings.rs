//! Configurable Keybinding and Input Mapping System for OpenKF (Unreal Engine 2.5 / Killing Floor v1065)
//!
//! Provides comprehensive mapping between Win32 Virtual-Key (VK) codes, mouse buttons,
//! and game actions. Supports saving and loading bindings to/from INI configuration files
//! (both modern [Bindings] style and Killing Floor User.ini [Engine.Input] style).

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::Path;

/// Win32 Virtual Key Constants (as defined in winuser.h)
pub mod vk {
    pub const VK_LBUTTON: u32 = 0x01;
    pub const VK_RBUTTON: u32 = 0x02;
    pub const VK_CANCEL: u32 = 0x03;
    pub const VK_MBUTTON: u32 = 0x04;
    pub const VK_XBUTTON1: u32 = 0x05;
    pub const VK_XBUTTON2: u32 = 0x06;
    pub const VK_BACK: u32 = 0x08;
    pub const VK_TAB: u32 = 0x09;
    pub const VK_CLEAR: u32 = 0x0C;
    pub const VK_RETURN: u32 = 0x0D;
    pub const VK_SHIFT: u32 = 0x10;
    pub const VK_CONTROL: u32 = 0x11;
    pub const VK_MENU: u32 = 0x12; // Alt
    pub const VK_PAUSE: u32 = 0x13;
    pub const VK_CAPITAL: u32 = 0x14; // Caps Lock
    pub const VK_ESCAPE: u32 = 0x1B;
    pub const VK_SPACE: u32 = 0x20;
    pub const VK_PRIOR: u32 = 0x21; // Page Up
    pub const VK_NEXT: u32 = 0x22;  // Page Down
    pub const VK_END: u32 = 0x23;
    pub const VK_HOME: u32 = 0x24;
    pub const VK_LEFT: u32 = 0x25;
    pub const VK_UP: u32 = 0x26;
    pub const VK_RIGHT: u32 = 0x27;
    pub const VK_DOWN: u32 = 0x28;
    pub const VK_SELECT: u32 = 0x29;
    pub const VK_PRINT: u32 = 0x2A;
    pub const VK_EXECUTE: u32 = 0x2B;
    pub const VK_SNAPSHOT: u32 = 0x2C; // Print Screen
    pub const VK_INSERT: u32 = 0x2D;
    pub const VK_DELETE: u32 = 0x2E;
    pub const VK_HELP: u32 = 0x2F;

    // Digits 0x30 - 0x39
    pub const VK_0: u32 = 0x30;
    pub const VK_1: u32 = 0x31;
    pub const VK_2: u32 = 0x32;
    pub const VK_3: u32 = 0x33;
    pub const VK_4: u32 = 0x34;
    pub const VK_5: u32 = 0x35;
    pub const VK_6: u32 = 0x36;
    pub const VK_7: u32 = 0x37;
    pub const VK_8: u32 = 0x38;
    pub const VK_9: u32 = 0x39;

    // Letters 0x41 - 0x5A
    pub const VK_A: u32 = 0x41;
    pub const VK_B: u32 = 0x42;
    pub const VK_C: u32 = 0x43;
    pub const VK_D: u32 = 0x44;
    pub const VK_E: u32 = 0x45;
    pub const VK_F: u32 = 0x46;
    pub const VK_G: u32 = 0x47;
    pub const VK_H: u32 = 0x48;
    pub const VK_I: u32 = 0x49;
    pub const VK_J: u32 = 0x4A;
    pub const VK_K: u32 = 0x4B;
    pub const VK_L: u32 = 0x4C;
    pub const VK_M: u32 = 0x4D;
    pub const VK_N: u32 = 0x4E;
    pub const VK_O: u32 = 0x4F;
    pub const VK_P: u32 = 0x50;
    pub const VK_Q: u32 = 0x51;
    pub const VK_R: u32 = 0x52;
    pub const VK_S: u32 = 0x53;
    pub const VK_T: u32 = 0x54;
    pub const VK_U: u32 = 0x55;
    pub const VK_V: u32 = 0x56;
    pub const VK_W: u32 = 0x57;
    pub const VK_X: u32 = 0x58;
    pub const VK_Y: u32 = 0x59;
    pub const VK_Z: u32 = 0x5A;

    pub const VK_LWIN: u32 = 0x5B;
    pub const VK_RWIN: u32 = 0x5C;
    pub const VK_APPS: u32 = 0x5D;

    // Numpad 0x60 - 0x69
    pub const VK_NUMPAD0: u32 = 0x60;
    pub const VK_NUMPAD1: u32 = 0x61;
    pub const VK_NUMPAD2: u32 = 0x62;
    pub const VK_NUMPAD3: u32 = 0x63;
    pub const VK_NUMPAD4: u32 = 0x64;
    pub const VK_NUMPAD5: u32 = 0x65;
    pub const VK_NUMPAD6: u32 = 0x66;
    pub const VK_NUMPAD7: u32 = 0x67;
    pub const VK_NUMPAD8: u32 = 0x68;
    pub const VK_NUMPAD9: u32 = 0x69;
    pub const VK_MULTIPLY: u32 = 0x6A;
    pub const VK_ADD: u32 = 0x6B;
    pub const VK_SEPARATOR: u32 = 0x6C;
    pub const VK_SUBTRACT: u32 = 0x6D;
    pub const VK_DECIMAL: u32 = 0x6E;
    pub const VK_DIVIDE: u32 = 0x6F;

    // Function keys 0x70 - 0x7B
    pub const VK_F1: u32 = 0x70;
    pub const VK_F2: u32 = 0x71;
    pub const VK_F3: u32 = 0x72;
    pub const VK_F4: u32 = 0x73;
    pub const VK_F5: u32 = 0x74;
    pub const VK_F6: u32 = 0x75;
    pub const VK_F7: u32 = 0x76;
    pub const VK_F8: u32 = 0x77;
    pub const VK_F9: u32 = 0x78;
    pub const VK_F10: u32 = 0x79;
    pub const VK_F11: u32 = 0x7A;
    pub const VK_F12: u32 = 0x7B;

    pub const VK_NUMLOCK: u32 = 0x90;
    pub const VK_SCROLL: u32 = 0x91;

    // Specific modifiers
    pub const VK_LSHIFT: u32 = 0xA0;
    pub const VK_RSHIFT: u32 = 0xA1;
    pub const VK_LCONTROL: u32 = 0xA2;
    pub const VK_RCONTROL: u32 = 0xA3;
    pub const VK_LMENU: u32 = 0xA4;
    pub const VK_RMENU: u32 = 0xA5;

    // OEM Punctuation
    pub const VK_OEM_1: u32 = 0xBA;      // ';:'
    pub const VK_OEM_PLUS: u32 = 0xBB;   // '=+'
    pub const VK_OEM_COMMA: u32 = 0xBC;  // ',<'
    pub const VK_OEM_MINUS: u32 = 0xBD;  // '-_'
    pub const VK_OEM_PERIOD: u32 = 0xBE; // '.>'
    pub const VK_OEM_2: u32 = 0xBF;      // '/?'
    pub const VK_OEM_3: u32 = 0xC0;      // '`~'
    pub const VK_OEM_4: u32 = 0xDB;      // '[{'
    pub const VK_OEM_5: u32 = 0xDC;      // '\|'
    pub const VK_OEM_6: u32 = 0xDD;      // ']}'
    pub const VK_OEM_7: u32 = 0xDE;      // ''"'
}

/// Game actions supported by OpenKF / Killing Floor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Forward,
    Backward,
    StrafeLeft,
    StrafeRight,
    Jump,
    Crouch,
    Fire,
    AltFire,
    Reload,
    Use,
    Voice,
    Flashlight,
    QuickHeal,
    QuickGrenade,
}

impl Action {
    /// Canonical name string for action
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::Forward => "Forward",
            Action::Backward => "Backward",
            Action::StrafeLeft => "StrafeLeft",
            Action::StrafeRight => "StrafeRight",
            Action::Jump => "Jump",
            Action::Crouch => "Crouch",
            Action::Fire => "Fire",
            Action::AltFire => "AltFire",
            Action::Reload => "Reload",
            Action::Use => "Use",
            Action::Voice => "Voice",
            Action::Flashlight => "Flashlight",
            Action::QuickHeal => "QuickHeal",
            Action::QuickGrenade => "QuickGrenade",
        }
    }

    /// Parse action from string (case-insensitive, supporting UE2 aliases)
    pub fn from_str_loose(s: &str) -> Option<Self> {
        let clean = s.trim().to_ascii_lowercase();
        match clean.as_str() {
            "forward" | "moveforward" | "move_forward" => Some(Action::Forward),
            "backward" | "backwards" | "movebackward" | "move_backward" | "back" => Some(Action::Backward),
            "strafeleft" | "strafe_left" | "moveleft" | "move_left" => Some(Action::StrafeLeft),
            "straferight" | "strafe_right" | "moveright" | "move_right" => Some(Action::StrafeRight),
            "jump" => Some(Action::Jump),
            "crouch" | "duck" => Some(Action::Crouch),
            "fire" | "primaryfire" | "attack" => Some(Action::Fire),
            "altfire" | "alt_fire" | "secondaryfire" => Some(Action::AltFire),
            "reload" | "reloadweapon" => Some(Action::Reload),
            "use" => Some(Action::Use),
            "voice" | "talk" | "voicetalk" => Some(Action::Voice),
            "flashlight" | "toggleflashlight" | "torch" => Some(Action::Flashlight),
            "quickheal" | "quick_heal" | "heal" | "syringeself" => Some(Action::QuickHeal),
            "quickgrenade" | "quick_grenade" | "throwgrenade" | "grenade" => Some(Action::QuickGrenade),
            _ => None,
        }
    }

    /// List of all defined actions
    pub fn all() -> &'static [Action] {
        &[
            Action::Forward,
            Action::Backward,
            Action::StrafeLeft,
            Action::StrafeRight,
            Action::Jump,
            Action::Crouch,
            Action::Fire,
            Action::AltFire,
            Action::Reload,
            Action::Use,
            Action::Voice,
            Action::Flashlight,
            Action::QuickHeal,
            Action::QuickGrenade,
        ]
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Convert Win32 virtual-key code or mouse code to friendly name string
pub fn vk_to_name(vk_code: u32) -> String {
    match vk_code {
        vk::VK_LBUTTON => "LeftMouse".to_string(),
        vk::VK_RBUTTON => "RightMouse".to_string(),
        vk::VK_MBUTTON => "MiddleMouse".to_string(),
        vk::VK_XBUTTON1 => "Mouse4".to_string(),
        vk::VK_XBUTTON2 => "Mouse5".to_string(),
        vk::VK_BACK => "Backspace".to_string(),
        vk::VK_TAB => "Tab".to_string(),
        vk::VK_RETURN => "Enter".to_string(),
        vk::VK_SHIFT => "Shift".to_string(),
        vk::VK_CONTROL => "Ctrl".to_string(),
        vk::VK_MENU => "Alt".to_string(),
        vk::VK_PAUSE => "Pause".to_string(),
        vk::VK_CAPITAL => "CapsLock".to_string(),
        vk::VK_ESCAPE => "Escape".to_string(),
        vk::VK_SPACE => "Space".to_string(),
        vk::VK_PRIOR => "PageUp".to_string(),
        vk::VK_NEXT => "PageDown".to_string(),
        vk::VK_END => "End".to_string(),
        vk::VK_HOME => "Home".to_string(),
        vk::VK_LEFT => "Left".to_string(),
        vk::VK_UP => "Up".to_string(),
        vk::VK_RIGHT => "Right".to_string(),
        vk::VK_DOWN => "Down".to_string(),
        vk::VK_SNAPSHOT => "PrintScreen".to_string(),
        vk::VK_INSERT => "Insert".to_string(),
        vk::VK_DELETE => "Delete".to_string(),

        c @ 0x30..=0x39 => ((c as u8) as char).to_string(),
        c @ 0x41..=0x5A => ((c as u8) as char).to_string(),

        vk::VK_NUMPAD0 => "Num0".to_string(),
        vk::VK_NUMPAD1 => "Num1".to_string(),
        vk::VK_NUMPAD2 => "Num2".to_string(),
        vk::VK_NUMPAD3 => "Num3".to_string(),
        vk::VK_NUMPAD4 => "Num4".to_string(),
        vk::VK_NUMPAD5 => "Num5".to_string(),
        vk::VK_NUMPAD6 => "Num6".to_string(),
        vk::VK_NUMPAD7 => "Num7".to_string(),
        vk::VK_NUMPAD8 => "Num8".to_string(),
        vk::VK_NUMPAD9 => "Num9".to_string(),
        vk::VK_MULTIPLY => "Num*".to_string(),
        vk::VK_ADD => "Num+".to_string(),
        vk::VK_SUBTRACT => "Num-".to_string(),
        vk::VK_DECIMAL => "Num.".to_string(),
        vk::VK_DIVIDE => "Num/".to_string(),

        vk::VK_F1 => "F1".to_string(),
        vk::VK_F2 => "F2".to_string(),
        vk::VK_F3 => "F3".to_string(),
        vk::VK_F4 => "F4".to_string(),
        vk::VK_F5 => "F5".to_string(),
        vk::VK_F6 => "F6".to_string(),
        vk::VK_F7 => "F7".to_string(),
        vk::VK_F8 => "F8".to_string(),
        vk::VK_F9 => "F9".to_string(),
        vk::VK_F10 => "F10".to_string(),
        vk::VK_F11 => "F11".to_string(),
        vk::VK_F12 => "F12".to_string(),

        vk::VK_LSHIFT => "LShift".to_string(),
        vk::VK_RSHIFT => "RShift".to_string(),
        vk::VK_LCONTROL => "LCtrl".to_string(),
        vk::VK_RCONTROL => "RCtrl".to_string(),
        vk::VK_LMENU => "LAlt".to_string(),
        vk::VK_RMENU => "RAlt".to_string(),

        vk::VK_OEM_1 => ";".to_string(),
        vk::VK_OEM_PLUS => "=".to_string(),
        vk::VK_OEM_COMMA => ",".to_string(),
        vk::VK_OEM_MINUS => "-".to_string(),
        vk::VK_OEM_PERIOD => ".".to_string(),
        vk::VK_OEM_2 => "/".to_string(),
        vk::VK_OEM_3 => "`".to_string(),
        vk::VK_OEM_4 => "[".to_string(),
        vk::VK_OEM_5 => "\\".to_string(),
        vk::VK_OEM_6 => "]".to_string(),
        vk::VK_OEM_7 => "'".to_string(),

        other => format!("VK_0x{:02X}", other),
    }
}

/// Convert friendly name string or hex/dec code to Win32 virtual-key code
pub fn name_to_vk(name: &str) -> Option<u32> {
    let clean = name.trim();
    if clean.is_empty() {
        return None;
    }

    // Try hex notation like "0x20" or "VK_0x20"
    if let Some(hex_part) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) {
        if let Ok(v) = u32::from_str_radix(hex_part, 16) {
            return Some(v);
        }
    }
    if let Some(hex_part) = clean.strip_prefix("VK_0x").or_else(|| clean.strip_prefix("vk_0x")) {
        if let Ok(v) = u32::from_str_radix(hex_part, 16) {
            return Some(v);
        }
    }

    // Single ASCII character (letters, numbers, basic symbols)
    if clean.len() == 1 {
        let ch = clean.chars().next().unwrap();
        if ch.is_ascii_alphabetic() {
            return Some(ch.to_ascii_uppercase() as u32);
        }
        if ch.is_ascii_digit() {
            return Some(ch as u32);
        }
        match ch {
            ';' => return Some(vk::VK_OEM_1),
            '=' | '+' => return Some(vk::VK_OEM_PLUS),
            ',' => return Some(vk::VK_OEM_COMMA),
            '-' => return Some(vk::VK_OEM_MINUS),
            '.' => return Some(vk::VK_OEM_PERIOD),
            '/' => return Some(vk::VK_OEM_2),
            '`' | '~' => return Some(vk::VK_OEM_3),
            '[' => return Some(vk::VK_OEM_4),
            '\\' => return Some(vk::VK_OEM_5),
            ']' => return Some(vk::VK_OEM_6),
            '\'' => return Some(vk::VK_OEM_7),
            ' ' => return Some(vk::VK_SPACE),
            _ => {}
        }
    }

    let lower = clean.to_ascii_lowercase();
    match lower.as_str() {
        // Mouse buttons
        "leftmouse" | "lbutton" | "mouse1" | "leftclick" => Some(vk::VK_LBUTTON),
        "rightmouse" | "rbutton" | "mouse2" | "rightclick" => Some(vk::VK_RBUTTON),
        "middlemouse" | "mbutton" | "mouse3" | "middleclick" => Some(vk::VK_MBUTTON),
        "mouse4" | "xbutton1" => Some(vk::VK_XBUTTON1),
        "mouse5" | "xbutton2" => Some(vk::VK_XBUTTON2),

        // Common keys
        "space" | "spacebar" => Some(vk::VK_SPACE),
        "enter" | "return" => Some(vk::VK_RETURN),
        "tab" => Some(vk::VK_TAB),
        "escape" | "esc" => Some(vk::VK_ESCAPE),
        "backspace" | "back" => Some(vk::VK_BACK),
        "shift" => Some(vk::VK_SHIFT),
        "lshift" | "leftshift" => Some(vk::VK_LSHIFT),
        "rshift" | "rightshift" => Some(vk::VK_RSHIFT),
        "ctrl" | "control" => Some(vk::VK_CONTROL),
        "lctrl" | "leftctrl" | "leftcontrol" => Some(vk::VK_LCONTROL),
        "rctrl" | "rightctrl" | "rightcontrol" => Some(vk::VK_RCONTROL),
        "alt" | "menu" => Some(vk::VK_MENU),
        "lalt" | "leftalt" => Some(vk::VK_LMENU),
        "ralt" | "rightalt" => Some(vk::VK_RMENU),
        "capslock" | "caps" | "capital" => Some(vk::VK_CAPITAL),
        "pause" => Some(vk::VK_PAUSE),

        // Navigation
        "up" | "uparrow" => Some(vk::VK_UP),
        "down" | "downarrow" => Some(vk::VK_DOWN),
        "left" | "leftarrow" => Some(vk::VK_LEFT),
        "right" | "rightarrow" => Some(vk::VK_RIGHT),
        "insert" | "ins" => Some(vk::VK_INSERT),
        "delete" | "del" => Some(vk::VK_DELETE),
        "home" => Some(vk::VK_HOME),
        "end" => Some(vk::VK_END),
        "pageup" | "pgup" | "prior" => Some(vk::VK_PRIOR),
        "pagedown" | "pgdn" | "next" => Some(vk::VK_NEXT),
        "printscreen" | "prtscn" | "snapshot" => Some(vk::VK_SNAPSHOT),

        // Function keys
        "f1" => Some(vk::VK_F1),
        "f2" => Some(vk::VK_F2),
        "f3" => Some(vk::VK_F3),
        "f4" => Some(vk::VK_F4),
        "f5" => Some(vk::VK_F5),
        "f6" => Some(vk::VK_F6),
        "f7" => Some(vk::VK_F7),
        "f8" => Some(vk::VK_F8),
        "f9" => Some(vk::VK_F9),
        "f10" => Some(vk::VK_F10),
        "f11" => Some(vk::VK_F11),
        "f12" => Some(vk::VK_F12),

        // Numpad
        "num0" | "numpad0" => Some(vk::VK_NUMPAD0),
        "num1" | "numpad1" => Some(vk::VK_NUMPAD1),
        "num2" | "numpad2" => Some(vk::VK_NUMPAD2),
        "num3" | "numpad3" => Some(vk::VK_NUMPAD3),
        "num4" | "numpad4" => Some(vk::VK_NUMPAD4),
        "num5" | "numpad5" => Some(vk::VK_NUMPAD5),
        "num6" | "numpad6" => Some(vk::VK_NUMPAD6),
        "num7" | "numpad7" => Some(vk::VK_NUMPAD7),
        "num8" | "numpad8" => Some(vk::VK_NUMPAD8),
        "num9" | "numpad9" => Some(vk::VK_NUMPAD9),
        "num*" | "multiply" => Some(vk::VK_MULTIPLY),
        "num+" | "add" => Some(vk::VK_ADD),
        "num-" | "subtract" => Some(vk::VK_SUBTRACT),
        "num." | "decimal" => Some(vk::VK_DECIMAL),
        "num/" | "divide" => Some(vk::VK_DIVIDE),

        _ => None,
    }
}

/// Bindings error types
#[derive(Debug, thiserror::Error)]
pub enum BindingsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error on line {line}: {msg}")]
    Parse { line: usize, msg: String },
}

/// Configurable Keybinding System
#[derive(Debug, Clone)]
pub struct InputBindings {
    /// Mapping of action to set of bound VK codes
    action_to_keys: HashMap<Action, Vec<u32>>,
    /// Reverse mapping of VK code to action (first/primary action bound)
    key_to_action: HashMap<u32, Action>,
    /// Currently physically pressed VK codes
    pressed_keys: HashSet<u32>,
}

impl Default for InputBindings {
    fn default() -> Self {
        Self::default_killing_floor()
    }
}

impl InputBindings {
    /// Create empty input bindings
    pub fn new() -> Self {
        Self {
            action_to_keys: HashMap::new(),
            key_to_action: HashMap::new(),
            pressed_keys: HashSet::new(),
        }
    }

    /// Retail Killing Floor default bindings
    pub fn default_killing_floor() -> Self {
        let mut bindings = Self::new();
        // Movement
        bindings.bind_key(Action::Forward, vk::VK_W);
        bindings.bind_key(Action::Forward, vk::VK_UP);
        bindings.bind_key(Action::Backward, vk::VK_S);
        bindings.bind_key(Action::Backward, vk::VK_DOWN);
        bindings.bind_key(Action::StrafeLeft, vk::VK_A);
        bindings.bind_key(Action::StrafeLeft, vk::VK_LEFT);
        bindings.bind_key(Action::StrafeRight, vk::VK_D);
        bindings.bind_key(Action::StrafeRight, vk::VK_RIGHT);

        // Movement Actions
        bindings.bind_key(Action::Jump, vk::VK_SPACE);
        bindings.bind_key(Action::Crouch, vk::VK_CONTROL);
        bindings.bind_key(Action::Crouch, vk::VK_C);

        // Combat
        bindings.bind_key(Action::Fire, vk::VK_LBUTTON);
        bindings.bind_key(Action::AltFire, vk::VK_RBUTTON);
        bindings.bind_key(Action::Reload, vk::VK_R);
        bindings.bind_key(Action::QuickGrenade, vk::VK_MBUTTON);
        bindings.bind_key(Action::QuickGrenade, vk::VK_G);

        // Interaction & Equipment
        bindings.bind_key(Action::Use, vk::VK_E);
        bindings.bind_key(Action::Voice, vk::VK_V);
        bindings.bind_key(Action::Voice, vk::VK_CAPITAL);
        bindings.bind_key(Action::Flashlight, vk::VK_F);
        bindings.bind_key(Action::QuickHeal, vk::VK_Q);

        bindings
    }

    /// Bind a virtual key to an action
    pub fn bind_key(&mut self, action: Action, vk_code: u32) {
        let keys = self.action_to_keys.entry(action).or_default();
        if !keys.contains(&vk_code) {
            keys.push(vk_code);
        }
        self.key_to_action.insert(vk_code, action);
    }

    /// Unbind a specific virtual key
    pub fn unbind_key(&mut self, vk_code: u32) {
        if let Some(action) = self.key_to_action.remove(&vk_code) {
            if let Some(keys) = self.action_to_keys.get_mut(&action) {
                keys.retain(|&k| k != vk_code);
            }
        }
        self.pressed_keys.remove(&vk_code);
    }

    /// Unbind all keys for a specific action
    pub fn unbind_action(&mut self, action: Action) {
        if let Some(keys) = self.action_to_keys.remove(&action) {
            for k in keys {
                self.key_to_action.remove(&k);
                self.pressed_keys.remove(&k);
            }
        }
    }

    /// Clear all bindings and pressed states
    pub fn clear(&mut self) {
        self.action_to_keys.clear();
        self.key_to_action.clear();
        self.pressed_keys.clear();
    }

    /// Handle key/button down event. Returns true if this press changed state.
    pub fn handle_key_down(&mut self, vk_code: u32) -> bool {
        
        self.pressed_keys.insert(vk_code)
    }

    /// Handle key/button up event. Returns true if this release changed state.
    pub fn handle_key_up(&mut self, vk_code: u32) -> bool {
        self.pressed_keys.remove(&vk_code)
    }

    /// Check if a specific game action is currently active (pressed down)
    pub fn is_action_active(&self, action: Action) -> bool {
        if let Some(keys) = self.action_to_keys.get(&action) {
            for &k in keys {
                if self.is_key_pressed(k) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a specific virtual-key code is currently pressed
    pub fn is_key_pressed(&self, vk_code: u32) -> bool {
        if self.pressed_keys.contains(&vk_code) {
            return true;
        }

        // Generic modifier equivalence
        match vk_code {
            vk::VK_SHIFT => {
                self.pressed_keys.contains(&vk::VK_LSHIFT) || self.pressed_keys.contains(&vk::VK_RSHIFT)
            }
            vk::VK_CONTROL => {
                self.pressed_keys.contains(&vk::VK_LCONTROL) || self.pressed_keys.contains(&vk::VK_RCONTROL)
            }
            vk::VK_MENU => {
                self.pressed_keys.contains(&vk::VK_LMENU) || self.pressed_keys.contains(&vk::VK_RMENU)
            }
            vk::VK_LSHIFT | vk::VK_RSHIFT => self.pressed_keys.contains(&vk::VK_SHIFT),
            vk::VK_LCONTROL | vk::VK_RCONTROL => self.pressed_keys.contains(&vk::VK_CONTROL),
            vk::VK_LMENU | vk::VK_RMENU => self.pressed_keys.contains(&vk::VK_MENU),
            _ => false,
        }
    }

    /// Get all keys bound to a given action
    pub fn get_bound_keys(&self, action: Action) -> Vec<u32> {
        self.action_to_keys.get(&action).cloned().unwrap_or_default()
    }

    /// Get the primary action associated with a specific key
    pub fn get_action_for_key(&self, vk_code: u32) -> Option<Action> {
        self.key_to_action.get(&vk_code).copied()
    }

    /// List all currently active actions
    pub fn get_active_actions(&self) -> Vec<Action> {
        Action::all()
            .iter()
            .copied()
            .filter(|&a| self.is_action_active(a))
            .collect()
    }

    /// Reset all pressed states (e.g. when game loses focus or window is minimized)
    pub fn reset_all_states(&mut self) {
        self.pressed_keys.clear();
    }

    /// Export current bindings to an INI formatted string
    pub fn save_to_ini(&self) -> String {
        let mut out = String::new();
        out.push_str("; ==========================================================\n");
        out.push_str("; OpenKF Input Configuration - Action & Keybindings\n");
        out.push_str("; Compatible with Unreal Engine 2.5 / Killing Floor v1065\n");
        out.push_str("; ==========================================================\n\n");
        out.push_str("[Bindings]\n");

        for &action in Action::all() {
            let keys = self.get_bound_keys(action);
            if !keys.is_empty() {
                let key_names: Vec<String> = keys.iter().map(|&k| vk_to_name(k)).collect();
                out.push_str(&format!("{}={}\n", action.as_str(), key_names.join(", ")));
            } else {
                out.push_str(&format!("{}=\n", action.as_str()));
            }
        }

        out
    }

    /// Load bindings from an INI string.
    ///
    /// Supports both:
    /// 1. Action-centric format: `[Bindings]` -> `Forward = W, Up`
    /// 2. Key-centric format (UE2 User.ini): `[Engine.Input]` -> `W = MoveForward`
    pub fn load_from_ini(&mut self, ini_content: &str) -> Result<(), BindingsError> {
        let mut _current_section = String::new();

        for line in ini_content.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
                continue;
            }

            // Section header [Section]
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                _current_section = trimmed[1..trimmed.len() - 1].trim().to_ascii_lowercase();
                continue;
            }

            // Key = Value split
            if let Some((left, right)) = trimmed.split_once('=') {
                let key_part = left.trim();
                let val_part = right.trim();

                // Format 1: Action = Key1, Key2, ...
                if let Some(action) = Action::from_str_loose(key_part) {
                    if val_part.is_empty() {
                        self.unbind_action(action);
                    } else {
                        self.unbind_action(action);
                        for k_str in val_part.split(',') {
                            let k_clean = k_str.trim();
                            if !k_clean.is_empty() {
                                if let Some(vk) = name_to_vk(k_clean) {
                                    self.bind_key(action, vk);
                                }
                            }
                        }
                    }
                    continue;
                }

                // Format 2: Key = Action (Killing Floor User.ini style)
                if let Some(vk) = name_to_vk(key_part) {
                    // Extract command/action name
                    let cmd = val_part
                        .split('|')
                        .next()
                        .unwrap_or(val_part)
                        .trim();
                    if let Some(action) = Action::from_str_loose(cmd) {
                        self.bind_key(action, vk);
                        continue;
                    }
                }
            }
        }

        Ok(())
    }

    /// Load bindings from an INI file
    pub fn load_from_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), BindingsError> {
        let content = fs::read_to_string(path)?;
        self.load_from_ini(&content)
    }

    /// Save bindings to an INI file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), BindingsError> {
        let content = self.save_to_ini();
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_killing_floor_bindings() {
        let bindings = InputBindings::default_killing_floor();
        let fwd_keys = bindings.get_bound_keys(Action::Forward);
        assert!(fwd_keys.contains(&vk::VK_W));
        assert!(fwd_keys.contains(&vk::VK_UP));

        let fire_keys = bindings.get_bound_keys(Action::Fire);
        assert!(fire_keys.contains(&vk::VK_LBUTTON));

        let altfire_keys = bindings.get_bound_keys(Action::AltFire);
        assert!(altfire_keys.contains(&vk::VK_RBUTTON));

        let reload_keys = bindings.get_bound_keys(Action::Reload);
        assert!(reload_keys.contains(&vk::VK_R));

        let use_keys = bindings.get_bound_keys(Action::Use);
        assert!(use_keys.contains(&vk::VK_E));

        let heal_keys = bindings.get_bound_keys(Action::QuickHeal);
        assert!(heal_keys.contains(&vk::VK_Q));

        let grenade_keys = bindings.get_bound_keys(Action::QuickGrenade);
        assert!(grenade_keys.contains(&vk::VK_MBUTTON) || grenade_keys.contains(&vk::VK_G));
    }

    #[test]
    fn test_bind_key_and_state_tracking() {
        let mut bindings = InputBindings::new();
        bindings.bind_key(Action::Forward, vk::VK_W);

        assert!(!bindings.is_action_active(Action::Forward));

        // Key down
        let new_press = bindings.handle_key_down(vk::VK_W);
        assert!(new_press);
        assert!(bindings.is_action_active(Action::Forward));

        // Duplicate key down does not register as new press
        assert!(!bindings.handle_key_down(vk::VK_W));
        assert!(bindings.is_action_active(Action::Forward));

        // Key up
        let released = bindings.handle_key_up(vk::VK_W);
        assert!(released);
        assert!(!bindings.is_action_active(Action::Forward));
    }

    #[test]
    fn test_multi_key_same_action() {
        let mut bindings = InputBindings::new();
        bindings.bind_key(Action::Forward, vk::VK_W);
        bindings.bind_key(Action::Forward, vk::VK_UP);

        bindings.handle_key_down(vk::VK_W);
        assert!(bindings.is_action_active(Action::Forward));

        // Press secondary key while primary is held
        bindings.handle_key_down(vk::VK_UP);
        assert!(bindings.is_action_active(Action::Forward));

        // Release primary key; secondary is still held
        bindings.handle_key_up(vk::VK_W);
        assert!(bindings.is_action_active(Action::Forward));

        // Release secondary key; action now inactive
        bindings.handle_key_up(vk::VK_UP);
        assert!(!bindings.is_action_active(Action::Forward));
    }

    #[test]
    fn test_mouse_button_actions() {
        let mut bindings = InputBindings::new();
        bindings.bind_key(Action::Fire, vk::VK_LBUTTON);
        bindings.bind_key(Action::AltFire, vk::VK_RBUTTON);
        bindings.bind_key(Action::QuickGrenade, vk::VK_MBUTTON);

        assert!(!bindings.is_action_active(Action::Fire));
        bindings.handle_key_down(vk::VK_LBUTTON);
        assert!(bindings.is_action_active(Action::Fire));

        bindings.handle_key_down(vk::VK_RBUTTON);
        assert!(bindings.is_action_active(Action::AltFire));

        bindings.handle_key_down(vk::VK_MBUTTON);
        assert!(bindings.is_action_active(Action::QuickGrenade));

        bindings.handle_key_up(vk::VK_LBUTTON);
        bindings.handle_key_up(vk::VK_RBUTTON);
        bindings.handle_key_up(vk::VK_MBUTTON);

        assert!(!bindings.is_action_active(Action::Fire));
        assert!(!bindings.is_action_active(Action::AltFire));
        assert!(!bindings.is_action_active(Action::QuickGrenade));
    }

    #[test]
    fn test_modifier_key_generic_equivalence() {
        let mut bindings = InputBindings::new();
        bindings.bind_key(Action::Crouch, vk::VK_CONTROL);

        // Press left control -> generic control should activate crouch
        bindings.handle_key_down(vk::VK_LCONTROL);
        assert!(bindings.is_action_active(Action::Crouch));

        bindings.handle_key_up(vk::VK_LCONTROL);
        assert!(!bindings.is_action_active(Action::Crouch));
    }

    #[test]
    fn test_unbind_key_and_unbind_action() {
        let mut bindings = InputBindings::new();
        bindings.bind_key(Action::Jump, vk::VK_SPACE);
        bindings.bind_key(Action::Jump, vk::VK_NUMPAD0);

        bindings.handle_key_down(vk::VK_SPACE);
        assert!(bindings.is_action_active(Action::Jump));

        bindings.unbind_key(vk::VK_SPACE);
        // Space unbind removes key and clears its pressed state
        assert!(!bindings.is_action_active(Action::Jump));
        assert_eq!(bindings.get_bound_keys(Action::Jump), vec![vk::VK_NUMPAD0]);

        bindings.unbind_action(Action::Jump);
        assert!(bindings.get_bound_keys(Action::Jump).is_empty());
    }

    #[test]
    fn test_reset_all_states() {
        let mut bindings = InputBindings::default_killing_floor();
        bindings.handle_key_down(vk::VK_W);
        bindings.handle_key_down(vk::VK_LBUTTON);
        bindings.handle_key_down(vk::VK_SPACE);

        assert!(bindings.is_action_active(Action::Forward));
        assert!(bindings.is_action_active(Action::Fire));
        assert!(bindings.is_action_active(Action::Jump));

        bindings.reset_all_states();

        assert!(!bindings.is_action_active(Action::Forward));
        assert!(!bindings.is_action_active(Action::Fire));
        assert!(!bindings.is_action_active(Action::Jump));
    }

    #[test]
    fn test_ini_save_and_load_roundtrip() {
        let mut original = InputBindings::new();
        original.bind_key(Action::Forward, vk::VK_W);
        original.bind_key(Action::Forward, vk::VK_UP);
        original.bind_key(Action::Fire, vk::VK_LBUTTON);
        original.bind_key(Action::QuickHeal, vk::VK_Q);
        original.bind_key(Action::Voice, vk::VK_V);

        let ini_str = original.save_to_ini();
        assert!(ini_str.contains("Forward=W, Up") || ini_str.contains("Forward=W,Up"));
        assert!(ini_str.contains("Fire=LeftMouse"));
        assert!(ini_str.contains("QuickHeal=Q"));

        let mut loaded = InputBindings::new();
        loaded.load_from_ini(&ini_str).unwrap();

        assert_eq!(loaded.get_bound_keys(Action::Forward), vec![vk::VK_W, vk::VK_UP]);
        assert_eq!(loaded.get_bound_keys(Action::Fire), vec![vk::VK_LBUTTON]);
        assert_eq!(loaded.get_bound_keys(Action::QuickHeal), vec![vk::VK_Q]);
        assert_eq!(loaded.get_bound_keys(Action::Voice), vec![vk::VK_V]);
    }

    #[test]
    fn test_ini_killing_floor_user_ini_format() {
        let user_ini = r#"
[Engine.Input]
Aliases[0]=(Command="Button bFire | Fire",Alias=Fire)
W=MoveForward
S=MoveBackward
A=StrafeLeft
D=StrafeRight
Space=Jump
Ctrl=Duck
LeftMouse=Fire
RightMouse=AltFire
R=ReloadWeapon
E=Use
F=ToggleFlashlight
Q=QuickHeal
MiddleMouse=ThrowGrenade
V=Voice
"#;

        let mut bindings = InputBindings::new();
        bindings.load_from_ini(user_ini).unwrap();

        assert_eq!(bindings.get_bound_keys(Action::Forward), vec![vk::VK_W]);
        assert_eq!(bindings.get_bound_keys(Action::Backward), vec![vk::VK_S]);
        assert_eq!(bindings.get_bound_keys(Action::StrafeLeft), vec![vk::VK_A]);
        assert_eq!(bindings.get_bound_keys(Action::StrafeRight), vec![vk::VK_D]);
        assert_eq!(bindings.get_bound_keys(Action::Jump), vec![vk::VK_SPACE]);
        assert_eq!(bindings.get_bound_keys(Action::Crouch), vec![vk::VK_CONTROL]);
        assert_eq!(bindings.get_bound_keys(Action::Fire), vec![vk::VK_LBUTTON]);
        assert_eq!(bindings.get_bound_keys(Action::AltFire), vec![vk::VK_RBUTTON]);
        assert_eq!(bindings.get_bound_keys(Action::Reload), vec![vk::VK_R]);
        assert_eq!(bindings.get_bound_keys(Action::Use), vec![vk::VK_E]);
        assert_eq!(bindings.get_bound_keys(Action::Flashlight), vec![vk::VK_F]);
        assert_eq!(bindings.get_bound_keys(Action::QuickHeal), vec![vk::VK_Q]);
        assert_eq!(bindings.get_bound_keys(Action::QuickGrenade), vec![vk::VK_MBUTTON]);
        assert_eq!(bindings.get_bound_keys(Action::Voice), vec![vk::VK_V]);
    }

    #[test]
    fn test_file_io_save_and_load() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("openkf_test_bindings.ini");

        let bindings = InputBindings::default_killing_floor();
        bindings.save_to_file(&file_path).unwrap();

        let mut reloaded = InputBindings::new();
        reloaded.load_from_file(&file_path).unwrap();

        assert!(reloaded.get_bound_keys(Action::Forward).contains(&vk::VK_W));
        assert!(reloaded.get_bound_keys(Action::Fire).contains(&vk::VK_LBUTTON));

        let _ = fs::remove_file(file_path);
    }
}
