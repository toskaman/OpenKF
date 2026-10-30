//! Win32 Raw Input Device Registration & Parsing Subsystem
//!
//! Provides clean, high-precision, unaccelerated mouse input capture via
//! the Win32 Raw Input API (`RegisterRawInputDevices`, `GetRawInputData`).
//!
//! Target: Generic Desktop Controls (UsagePage: 0x01, Usage: 0x02 - Mouse)
//! Flags: `RIDEV_INPUTSINK` (or foreground-focused capture)
//! Safe unregistration on teardown and exit.

use std::fmt;

// ---------------------------------------------------------------------------
// Win32 Types & Handles
// ---------------------------------------------------------------------------

pub type HWND = *mut std::ffi::c_void;
pub type HRAWINPUT = *mut std::ffi::c_void;
pub type HANDLE = *mut std::ffi::c_void;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;

// ---------------------------------------------------------------------------
// HID Usages & Pages (Generic Desktop)
// ---------------------------------------------------------------------------

/// HID Usage Page 0x01: Generic Desktop Controls
pub const HID_USAGE_PAGE_GENERIC: u16 = 0x01;

/// HID Usage 0x01: Pointer
pub const HID_USAGE_GENERIC_POINTER: u16 = 0x01;

/// HID Usage 0x02: Mouse
pub const HID_USAGE_GENERIC_MOUSE: u16 = 0x02;

/// HID Usage 0x04: Joystick
pub const HID_USAGE_GENERIC_JOYSTICK: u16 = 0x04;

/// HID Usage 0x05: Gamepad
pub const HID_USAGE_GENERIC_GAMEPAD: u16 = 0x05;

/// HID Usage 0x06: Keyboard
pub const HID_USAGE_GENERIC_KEYBOARD: u16 = 0x06;

// ---------------------------------------------------------------------------
// Win32 Raw Input Flags (RIDEV_*)
// ---------------------------------------------------------------------------

/// If set, this removes the top level collection from the inclusion list.
/// Tells the OS to stop reading from a device matching the top level collection.
/// `hwndTarget` must be NULL (0).
pub const RIDEV_REMOVE: u32 = 0x0000_0001;

/// If set, this specifies the top level collections to exclude when reading a page.
pub const RIDEV_EXCLUDE: u32 = 0x0000_0010;

/// If set, this specifies all devices whose top level collection is from the specified usUsagePage.
pub const RIDEV_PAGEONLY: u32 = 0x0000_0020;

/// If set, this prevents any devices specified by usUsagePage or usUsage from generating legacy messages.
/// For example, mouse raw input will not generate WM_LBUTTONDOWN, WM_MOUSEMOVE, etc.
pub const RIDEV_NOLEGACY: u32 = 0x0000_0030;

/// If set, enables caller to receive input even when not in foreground.
/// `hwndTarget` must be specified.
pub const RIDEV_INPUTSINK: u32 = 0x0000_0100;

/// If set, the mouse button click does not activate the other window.
pub const RIDEV_CAPTUREMOUSE: u32 = 0x0000_0200;

/// If set, the application-defined keyboard accelerator keys are not handled.
pub const RIDEV_NOHOTKEY: u32 = 0x0000_0200;

/// If set, application command keys are handled.
pub const RIDEV_APPKEYS: u32 = 0x0000_0400;

/// If set, enables arrival and removal notifications for devices in the collection.
pub const RIDEV_DEVNOTIFY: u32 = 0x0000_2000;

// ---------------------------------------------------------------------------
// Raw Input Commands & Types
// ---------------------------------------------------------------------------

/// WM_INPUT window message
pub const WM_INPUT: u32 = 0x00FF;

/// Raw input comes from a mouse.
pub const RIM_TYPEMOUSE: u32 = 0;

/// Raw input comes from a keyboard.
pub const RIM_TYPEKEYBOARD: u32 = 1;

/// Raw input comes from an HID device that is not a keyboard or mouse.
pub const RIM_TYPEHID: u32 = 2;

/// GetRawInputData command: Get raw input content from RAWINPUT structure.
pub const RID_INPUT: u32 = 0x1000_0003;

/// GetRawInputData command: Get header information from RAWINPUT structure.
pub const RID_HEADER: u32 = 0x1000_0005;

// ---------------------------------------------------------------------------
// Mouse Motion & Button Flags
// ---------------------------------------------------------------------------

/// Mouse coordinates are relative to the previous mouse location.
pub const MOUSE_MOVE_RELATIVE: u16 = 0x00;

/// Mouse coordinates are absolute.
pub const MOUSE_MOVE_ABSOLUTE: u16 = 0x01;

/// Mouse coordinates are mapped to the virtual desktop (for a multiple monitor system).
pub const MOUSE_VIRTUAL_DESKTOP: u16 = 0x02;

/// Re-query for mouse attributes.
pub const MOUSE_ATTRIBUTES_CHANGED: u16 = 0x04;

/// Do not coalesce mouse moves.
pub const MOUSE_MOVE_NOCOALESCE: u16 = 0x08;

// Button flags
pub const RI_MOUSE_LEFT_BUTTON_DOWN: u16 = 0x0001;
pub const RI_MOUSE_LEFT_BUTTON_UP: u16 = 0x0002;
pub const RI_MOUSE_RIGHT_BUTTON_DOWN: u16 = 0x0004;
pub const RI_MOUSE_RIGHT_BUTTON_UP: u16 = 0x0008;
pub const RI_MOUSE_MIDDLE_BUTTON_DOWN: u16 = 0x0010;
pub const RI_MOUSE_MIDDLE_BUTTON_UP: u16 = 0x0020;
pub const RI_MOUSE_BUTTON_4_DOWN: u16 = 0x0040;
pub const RI_MOUSE_BUTTON_4_UP: u16 = 0x0080;
pub const RI_MOUSE_BUTTON_5_DOWN: u16 = 0x0100;
pub const RI_MOUSE_BUTTON_5_UP: u16 = 0x0200;
pub const RI_MOUSE_WHEEL: u16 = 0x0400;
pub const RI_MOUSE_HWHEEL: u16 = 0x0800;

// Aliases
pub const RI_MOUSE_BUTTON_1_DOWN: u16 = RI_MOUSE_LEFT_BUTTON_DOWN;
pub const RI_MOUSE_BUTTON_1_UP: u16 = RI_MOUSE_LEFT_BUTTON_UP;
pub const RI_MOUSE_BUTTON_2_DOWN: u16 = RI_MOUSE_RIGHT_BUTTON_DOWN;
pub const RI_MOUSE_BUTTON_2_UP: u16 = RI_MOUSE_RIGHT_BUTTON_UP;
pub const RI_MOUSE_BUTTON_3_DOWN: u16 = RI_MOUSE_MIDDLE_BUTTON_DOWN;
pub const RI_MOUSE_BUTTON_3_UP: u16 = RI_MOUSE_MIDDLE_BUTTON_UP;

// ---------------------------------------------------------------------------
// Struct Definitions (C ABI)
// ---------------------------------------------------------------------------

/// Defines information for the raw input devices.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct RAWINPUTDEVICE {
    pub usUsagePage: u16,
    pub usUsage: u16,
    pub dwFlags: u32,
    pub hwndTarget: HWND,
}

/// Header for the raw input data.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct RAWINPUTHEADER {
    pub dwType: u32,
    pub dwSize: u32,
    pub hDevice: HANDLE,
    pub wParam: WPARAM,
}

/// Contains information about the state of the mouse.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct RAWMOUSE {
    pub usFlags: u16,
    pub usReserved: u16,
    pub usButtonFlags: u16,
    pub usButtonData: i16,
    pub ulRawButtons: u32,
    pub lLastX: i32,
    pub lLastY: i32,
    pub ulExtraInformation: u32,
}

/// Union of device-specific raw input payloads.
#[repr(C)]
#[derive(Clone, Copy)]
pub union RAWINPUTDATA {
    pub mouse: RAWMOUSE,
    pub raw: [u8; 32],
}

/// Contains the raw input from a device.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RAWINPUT {
    pub header: RAWINPUTHEADER,
    pub data: RAWINPUTDATA,
}

// ---------------------------------------------------------------------------
// Win32 FFI Bindings
// ---------------------------------------------------------------------------

#[link(name = "user32")]
extern "system" {
    fn RegisterRawInputDevices(
        pRawInputDevices: *const RAWINPUTDEVICE,
        uiNumDevices: u32,
        cbSize: u32,
    ) -> i32;

    fn GetRawInputData(
        hRawInput: HRAWINPUT,
        uiCommand: u32,
        pData: *mut std::ffi::c_void,
        pcbSize: *mut u32,
        cbSizeHeader: u32,
    ) -> u32;
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum RawInputError {
    #[error("Null target HWND provided for RIDEV_INPUTSINK raw input registration")]
    NullHwnd,

    #[error("Failed to register raw input device(s): OS error {0}")]
    RegistrationFailed(std::io::Error),

    #[error("Failed to unregister raw input device(s): OS error {0}")]
    UnregistrationFailed(std::io::Error),

    #[error("Failed to query raw input data size: OS error {0}")]
    QueryDataFailed(std::io::Error),

    #[error("Failed to read raw input data: OS error {0}")]
    ReadDataFailed(std::io::Error),

    #[error("Raw input buffer too small ({actual} bytes, expected at least {expected} bytes)")]
    BufferTooSmall { actual: usize, expected: usize },

    #[error("Raw input packet is not a mouse event (type ID: {0})")]
    NotMouseInput(u32),
}

// ---------------------------------------------------------------------------
// High-Level Parsed Mouse Event
// ---------------------------------------------------------------------------

/// High-precision, unaccelerated mouse movement and button event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RawMouseDelta {
    /// Relative horizontal movement (or absolute coordinate if is_absolute is true)
    pub dx: i32,
    /// Relative vertical movement (or absolute coordinate if is_absolute is true)
    pub dy: i32,
    /// Indicates whether coordinates are absolute rather than relative deltas
    pub is_absolute: bool,
    /// Raw button state flags (combination of RI_MOUSE_*_DOWN / UP)
    pub button_flags: u16,
    /// Vertical mouse wheel scroll delta (+120 forward, -120 backward)
    pub wheel_delta: i16,
    /// Horizontal mouse wheel scroll delta
    pub hwheel_delta: i16,
    /// Raw buttons state bitmask
    pub raw_buttons: u32,
}

impl RawMouseDelta {
    #[inline]
    pub fn is_relative(&self) -> bool {
        !self.is_absolute
    }

    #[inline]
    pub fn left_pressed(&self) -> bool {
        (self.button_flags & RI_MOUSE_LEFT_BUTTON_DOWN) != 0
    }

    #[inline]
    pub fn left_released(&self) -> bool {
        (self.button_flags & RI_MOUSE_LEFT_BUTTON_UP) != 0
    }

    #[inline]
    pub fn right_pressed(&self) -> bool {
        (self.button_flags & RI_MOUSE_RIGHT_BUTTON_DOWN) != 0
    }

    #[inline]
    pub fn right_released(&self) -> bool {
        (self.button_flags & RI_MOUSE_RIGHT_BUTTON_UP) != 0
    }

    #[inline]
    pub fn middle_pressed(&self) -> bool {
        (self.button_flags & RI_MOUSE_MIDDLE_BUTTON_DOWN) != 0
    }

    #[inline]
    pub fn middle_released(&self) -> bool {
        (self.button_flags & RI_MOUSE_MIDDLE_BUTTON_UP) != 0
    }

    #[inline]
    pub fn button_4_pressed(&self) -> bool {
        (self.button_flags & RI_MOUSE_BUTTON_4_DOWN) != 0
    }

    #[inline]
    pub fn button_4_released(&self) -> bool {
        (self.button_flags & RI_MOUSE_BUTTON_4_UP) != 0
    }

    #[inline]
    pub fn button_5_pressed(&self) -> bool {
        (self.button_flags & RI_MOUSE_BUTTON_5_DOWN) != 0
    }

    #[inline]
    pub fn button_5_released(&self) -> bool {
        (self.button_flags & RI_MOUSE_BUTTON_5_UP) != 0
    }

    #[inline]
    pub fn has_movement(&self) -> bool {
        self.dx != 0 || self.dy != 0
    }

    #[inline]
    pub fn has_wheel(&self) -> bool {
        (self.button_flags & RI_MOUSE_WHEEL) != 0 && self.wheel_delta != 0
    }

    #[inline]
    pub fn has_hwheel(&self) -> bool {
        (self.button_flags & RI_MOUSE_HWHEEL) != 0 && self.hwheel_delta != 0
    }
}

// ---------------------------------------------------------------------------
// RawInputManager
// ---------------------------------------------------------------------------

/// Manages registration, unregistration, and parsing of raw input devices.
pub struct RawInputManager {
    registered_hwnd: Option<HWND>,
    flags: u32,
    auto_unregister_on_drop: bool,
}

impl fmt::Debug for RawInputManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawInputManager")
            .field("registered_hwnd", &self.registered_hwnd)
            .field("flags", &format_args!("0x{:08X}", self.flags))
            .field("auto_unregister_on_drop", &self.auto_unregister_on_drop)
            .finish()
    }
}

impl Default for RawInputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RawInputManager {
    /// Creates a new uninitialized `RawInputManager`.
    pub const fn new() -> Self {
        Self {
            registered_hwnd: None,
            flags: 0,
            auto_unregister_on_drop: true,
        }
    }

    /// Sets whether `RawInputManager` automatically calls `unregister()` on `Drop`.
    pub fn with_auto_unregister(mut self, auto: bool) -> Self {
        self.auto_unregister_on_drop = auto;
        self
    }

    /// Returns `true` if a mouse raw input device is currently registered.
    #[inline]
    pub fn is_registered(&self) -> bool {
        self.registered_hwnd.is_some()
    }

    /// Returns the target window handle currently registered, if any.
    #[inline]
    pub fn registered_hwnd(&self) -> Option<HWND> {
        self.registered_hwnd
    }

    /// Returns the active registration flags.
    #[inline]
    pub fn active_flags(&self) -> u32 {
        self.flags
    }

    /// Registers the mouse device with Generic Desktop Controls (Page 0x01, Usage 0x02)
    /// using `RIDEV_INPUTSINK` to capture unaccelerated raw mouse deltas.
    ///
    /// Requires a valid, non-null window handle.
    pub fn register_mouse(&mut self, hwnd: HWND) -> Result<(), RawInputError> {
        self.register_mouse_with_flags(hwnd, RIDEV_INPUTSINK)
    }

    /// Registers the mouse device for foreground-only capture (`flags = 0`).
    /// Events are only captured when `hwnd` has input focus.
    pub fn register_mouse_foreground(&mut self, hwnd: HWND) -> Result<(), RawInputError> {
        self.register_mouse_with_flags(hwnd, 0)
    }

    /// Registers the mouse device with caller-specified custom Win32 flags.
    pub fn register_mouse_with_flags(&mut self, hwnd: HWND, flags: u32) -> Result<(), RawInputError> {
        // RIDEV_INPUTSINK strictly requires a valid non-null target HWND
        if (flags & RIDEV_INPUTSINK) != 0 && hwnd.is_null() {
            return Err(RawInputError::NullHwnd);
        }

        let rid = RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_GENERIC_MOUSE,
            dwFlags: flags,
            hwndTarget: hwnd,
        };

        let result = unsafe {
            RegisterRawInputDevices(
                &rid,
                1,
                std::mem::size_of::<RAWINPUTDEVICE>() as u32,
            )
        };

        if result == 0 {
            let err = std::io::Error::last_os_error();
            return Err(RawInputError::RegistrationFailed(err));
        }

        self.registered_hwnd = Some(hwnd);
        self.flags = flags;
        Ok(())
    }

    /// Unregisters the mouse device (Page 0x01, Usage 0x02) using `RIDEV_REMOVE`.
    ///
    /// Cleans up OS raw input routing. Safe to call multiple times or if never registered.
    pub fn unregister(&mut self) -> Result<(), RawInputError> {
        if !self.is_registered() {
            return Ok(());
        }

        let rid = RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_GENERIC_MOUSE,
            dwFlags: RIDEV_REMOVE,
            hwndTarget: std::ptr::null_mut(),
        };

        let result = unsafe {
            RegisterRawInputDevices(
                &rid,
                1,
                std::mem::size_of::<RAWINPUTDEVICE>() as u32,
            )
        };

        if result == 0 {
            let err = std::io::Error::last_os_error();
            return Err(RawInputError::UnregistrationFailed(err));
        }

        self.registered_hwnd = None;
        self.flags = 0;
        Ok(())
    }

    /// Parses a Win32 `WM_INPUT` `lparam` message handle into high-precision `RawMouseDelta`.
    pub fn parse_lparam(lparam: isize) -> Result<RawMouseDelta, RawInputError> {
        unsafe {
            let mut data_size: u32 = 0;
            let header_size = std::mem::size_of::<RAWINPUTHEADER>() as u32;

            // 1. Query required buffer size
            let res = GetRawInputData(
                lparam as HRAWINPUT,
                RID_INPUT,
                std::ptr::null_mut(),
                &mut data_size,
                header_size,
            );

            if res == u32::MAX || data_size == 0 {
                let err = std::io::Error::last_os_error();
                return Err(RawInputError::QueryDataFailed(err));
            }

            // 2. Fetch raw input payload
            let mut buffer = vec![0u8; data_size as usize];
            let read_bytes = GetRawInputData(
                lparam as HRAWINPUT,
                RID_INPUT,
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                &mut data_size,
                header_size,
            );

            if read_bytes == u32::MAX {
                let err = std::io::Error::last_os_error();
                return Err(RawInputError::ReadDataFailed(err));
            }

            Self::parse_buffer(&buffer)
        }
    }

    /// Pure buffer parser for `RAWINPUT` packet. Extracts unaccelerated mouse deltas.
    pub fn parse_buffer(bytes: &[u8]) -> Result<RawMouseDelta, RawInputError> {
        let header_size = std::mem::size_of::<RAWINPUTHEADER>();
        let mouse_size = std::mem::size_of::<RAWMOUSE>();
        let min_required = header_size + mouse_size;

        if bytes.len() < min_required {
            return Err(RawInputError::BufferTooSmall {
                actual: bytes.len(),
                expected: min_required,
            });
        }

        let header = unsafe { &*(bytes.as_ptr() as *const RAWINPUTHEADER) };
        if header.dwType != RIM_TYPEMOUSE {
            return Err(RawInputError::NotMouseInput(header.dwType));
        }

        let mouse = unsafe { &*(bytes.as_ptr().add(header_size) as *const RAWMOUSE) };

        let is_absolute = (mouse.usFlags & MOUSE_MOVE_ABSOLUTE) != 0;
        let mut wheel = 0i16;
        let mut hwheel = 0i16;

        if (mouse.usButtonFlags & RI_MOUSE_WHEEL) != 0 {
            wheel = mouse.usButtonData;
        }
        if (mouse.usButtonFlags & RI_MOUSE_HWHEEL) != 0 {
            hwheel = mouse.usButtonData;
        }

        Ok(RawMouseDelta {
            dx: mouse.lLastX,
            dy: mouse.lLastY,
            is_absolute,
            button_flags: mouse.usButtonFlags,
            wheel_delta: wheel,
            hwheel_delta: hwheel,
            raw_buttons: mouse.ulRawButtons,
        })
    }
}

impl Drop for RawInputManager {
    fn drop(&mut self) {
        if self.auto_unregister_on_drop && self.is_registered() {
            let _ = self.unregister();
        }
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_win32_struct_sizes_and_alignments() {
        assert_eq!(
            std::mem::size_of::<RAWINPUTDEVICE>(),
            16,
            "RAWINPUTDEVICE must be 16 bytes on 64-bit Windows"
        );
        assert_eq!(
            std::mem::align_of::<RAWINPUTDEVICE>(),
            8,
            "RAWINPUTDEVICE must have 8-byte alignment"
        );

        assert_eq!(
            std::mem::size_of::<RAWINPUTHEADER>(),
            24,
            "RAWINPUTHEADER must be 24 bytes on 64-bit Windows"
        );
        assert_eq!(
            std::mem::align_of::<RAWINPUTHEADER>(),
            8,
            "RAWINPUTHEADER must have 8-byte alignment"
        );

        assert_eq!(
            std::mem::size_of::<RAWMOUSE>(),
            24,
            "RAWMOUSE must be 24 bytes"
        );
    }

    #[test]
    fn test_raw_input_manager_initial_state() {
        let mgr = RawInputManager::new();
        assert!(!mgr.is_registered());
        assert_eq!(mgr.registered_hwnd(), None);
        assert_eq!(mgr.active_flags(), 0);
    }

    #[test]
    fn test_register_null_hwnd_with_input_sink_rejected() {
        let mut mgr = RawInputManager::new();
        let result = mgr.register_mouse(std::ptr::null_mut());
        assert!(matches!(result, Err(RawInputError::NullHwnd)));
        assert!(!mgr.is_registered());
    }

    #[test]
    fn test_unregister_when_unregistered_succeeds_as_noop() {
        let mut mgr = RawInputManager::new();
        assert!(mgr.unregister().is_ok());
        assert!(!mgr.is_registered());
    }

    #[test]
    fn test_parse_buffer_relative_mouse_movement() {
        let mut buffer = vec![0u8; 48];

        // Header: dwType = RIM_TYPEMOUSE, dwSize = 48
        let header = unsafe { &mut *(buffer.as_mut_ptr() as *mut RAWINPUTHEADER) };
        header.dwType = RIM_TYPEMOUSE;
        header.dwSize = 48;
        header.hDevice = 0x1234 as HANDLE;
        header.wParam = 0;

        // Mouse: relative dx = 42, dy = -17, left button down
        let mouse = unsafe { &mut *(buffer.as_mut_ptr().add(24) as *mut RAWMOUSE) };
        mouse.usFlags = MOUSE_MOVE_RELATIVE;
        mouse.lLastX = 42;
        mouse.lLastY = -17;
        mouse.usButtonFlags = RI_MOUSE_LEFT_BUTTON_DOWN;
        mouse.usButtonData = 0;
        mouse.ulRawButtons = 1;

        let delta = RawInputManager::parse_buffer(&buffer).expect("Buffer parsing should succeed");

        assert!(delta.is_relative());
        assert!(!delta.is_absolute);
        assert_eq!(delta.dx, 42);
        assert_eq!(delta.dy, -17);
        assert!(delta.has_movement());
        assert!(delta.left_pressed());
        assert!(!delta.left_released());
        assert!(!delta.right_pressed());
        assert!(!delta.has_wheel());
        assert_eq!(delta.wheel_delta, 0);
        assert_eq!(delta.raw_buttons, 1);
    }

    #[test]
    fn test_parse_buffer_absolute_mouse_movement() {
        let mut buffer = vec![0u8; 48];
        let header = unsafe { &mut *(buffer.as_mut_ptr() as *mut RAWINPUTHEADER) };
        header.dwType = RIM_TYPEMOUSE;
        header.dwSize = 48;

        let mouse = unsafe { &mut *(buffer.as_mut_ptr().add(24) as *mut RAWMOUSE) };
        mouse.usFlags = MOUSE_MOVE_ABSOLUTE | MOUSE_VIRTUAL_DESKTOP;
        mouse.lLastX = 1920;
        mouse.lLastY = 1080;

        let delta = RawInputManager::parse_buffer(&buffer).unwrap();
        assert!(delta.is_absolute);
        assert!(!delta.is_relative());
        assert_eq!(delta.dx, 1920);
        assert_eq!(delta.dy, 1080);
    }

    #[test]
    fn test_parse_buffer_wheel_events() {
        let mut buffer = vec![0u8; 48];
        let header = unsafe { &mut *(buffer.as_mut_ptr() as *mut RAWINPUTHEADER) };
        header.dwType = RIM_TYPEMOUSE;
        header.dwSize = 48;

        let mouse = unsafe { &mut *(buffer.as_mut_ptr().add(24) as *mut RAWMOUSE) };
        mouse.usButtonFlags = RI_MOUSE_WHEEL;
        mouse.usButtonData = 120; // 1 notch forward

        let delta = RawInputManager::parse_buffer(&buffer).unwrap();
        assert!(delta.has_wheel());
        assert_eq!(delta.wheel_delta, 120);
        assert_eq!(delta.hwheel_delta, 0);

        // Negative notch (backward)
        mouse.usButtonData = -120;
        let delta_back = RawInputManager::parse_buffer(&buffer).unwrap();
        assert!(delta_back.has_wheel());
        assert_eq!(delta_back.wheel_delta, -120);

        // Horizontal wheel
        mouse.usButtonFlags = RI_MOUSE_HWHEEL;
        mouse.usButtonData = 120;
        let delta_h = RawInputManager::parse_buffer(&buffer).unwrap();
        assert!(!delta_h.has_wheel());
        assert!(delta_h.has_hwheel());
        assert_eq!(delta_h.hwheel_delta, 120);
    }

    #[test]
    fn test_parse_buffer_all_button_states() {
        let mut buffer = vec![0u8; 48];
        let header = unsafe { &mut *(buffer.as_mut_ptr() as *mut RAWINPUTHEADER) };
        header.dwType = RIM_TYPEMOUSE;
        header.dwSize = 48;

        let mouse = unsafe { &mut *(buffer.as_mut_ptr().add(24) as *mut RAWMOUSE) };
        mouse.usButtonFlags = RI_MOUSE_RIGHT_BUTTON_DOWN
            | RI_MOUSE_MIDDLE_BUTTON_DOWN
            | RI_MOUSE_BUTTON_4_DOWN
            | RI_MOUSE_BUTTON_5_DOWN;

        let delta = RawInputManager::parse_buffer(&buffer).unwrap();
        assert!(!delta.left_pressed());
        assert!(delta.right_pressed());
        assert!(delta.middle_pressed());
        assert!(delta.button_4_pressed());
        assert!(delta.button_5_pressed());

        mouse.usButtonFlags = RI_MOUSE_RIGHT_BUTTON_UP
            | RI_MOUSE_MIDDLE_BUTTON_UP
            | RI_MOUSE_BUTTON_4_UP
            | RI_MOUSE_BUTTON_5_UP
            | RI_MOUSE_LEFT_BUTTON_UP;

        let delta_up = RawInputManager::parse_buffer(&buffer).unwrap();
        assert!(delta_up.left_released());
        assert!(delta_up.right_released());
        assert!(delta_up.middle_released());
        assert!(delta_up.button_4_released());
        assert!(delta_up.button_5_released());
    }

    #[test]
    fn test_parse_buffer_rejects_non_mouse_input() {
        let mut buffer = vec![0u8; 48];
        let header = unsafe { &mut *(buffer.as_mut_ptr() as *mut RAWINPUTHEADER) };
        header.dwType = RIM_TYPEKEYBOARD;
        header.dwSize = 48;

        let result = RawInputManager::parse_buffer(&buffer);
        assert!(matches!(result, Err(RawInputError::NotMouseInput(RIM_TYPEKEYBOARD))));
    }

    #[test]
    fn test_parse_buffer_truncated_buffer_fails() {
        let small_buffer = vec![0u8; 20];
        let result = RawInputManager::parse_buffer(&small_buffer);
        assert!(matches!(
            result,
            Err(RawInputError::BufferTooSmall {
                actual: 20,
                expected: 48
            })
        ));
    }

    #[test]
    fn test_drop_behavior_clean() {
        // Ensuring Drop executes without panic
        {
            let _mgr = RawInputManager::new();
        }
        {
            let mgr = RawInputManager::new().with_auto_unregister(false);
            assert!(!mgr.auto_unregister_on_drop);
        }
    }
}
