//! High-Precision Raw Mouse Motion Accumulator & WM_INPUT Parser for OpenKF.
//!
//! Clean-room implementation providing direct 1:1 hardware polling bypass
//! for Unreal Engine 2.5 / Killing Floor v1065.
//! Eliminates Windows pointer ballistics, desktop DPI scaling, and engine smoothing curves.

use std::ffi::c_void;

// ============================================================================
// Win32 Raw Input API Constants (winuser.h)
// ============================================================================

/// Raw input device types (dwType)
pub const RIM_TYPEMOUSE: u32 = 0;
pub const RIM_TYPEKEYBOARD: u32 = 1;
pub const RIM_TYPEHID: u32 = 2;

/// GetRawInputData command flags (uiCommand)
pub const RID_INPUT: u32 = 0x10000003;
pub const RID_HEADER: u32 = 0x10000005;

/// Mouse motion indicator flags (usFlags)
pub const MOUSE_MOVE_RELATIVE: u16 = 0x0000;
pub const MOUSE_MOVE_ABSOLUTE: u16 = 0x0001;
pub const MOUSE_VIRTUAL_DESKTOP: u16 = 0x0002;
pub const MOUSE_ATTRIBUTES_CHANGED: u16 = 0x0004;
pub const MOUSE_MOVE_NOCOALESCE: u16 = 0x0008;

/// Mouse button transition flags (usButtonFlags)
pub const RI_MOUSE_LEFT_BUTTON_DOWN: u16   = 0x0001; // Button 1 Down
pub const RI_MOUSE_LEFT_BUTTON_UP: u16     = 0x0002; // Button 1 Up
pub const RI_MOUSE_RIGHT_BUTTON_DOWN: u16  = 0x0004; // Button 2 Down
pub const RI_MOUSE_RIGHT_BUTTON_UP: u16    = 0x0008; // Button 2 Up
pub const RI_MOUSE_MIDDLE_BUTTON_DOWN: u16 = 0x0010; // Button 3 Down
pub const RI_MOUSE_MIDDLE_BUTTON_UP: u16   = 0x0020; // Button 3 Up
pub const RI_MOUSE_BUTTON_4_DOWN: u16      = 0x0040; // XBUTTON 1 Down
pub const RI_MOUSE_BUTTON_4_UP: u16        = 0x0080; // XBUTTON 1 Up
pub const RI_MOUSE_BUTTON_5_DOWN: u16      = 0x0100; // XBUTTON 2 Down
pub const RI_MOUSE_BUTTON_5_UP: u16        = 0x0200; // XBUTTON 2 Up
pub const RI_MOUSE_WHEEL: u16              = 0x0400; // Vertical Wheel rotation
pub const RI_MOUSE_HWHEEL: u16             = 0x0800; // Horizontal Wheel rotation

/// RegisterRawInputDevices flags
pub const RIDEV_REMOVE: u32       = 0x00000001;
pub const RIDEV_EXCLUDE: u32      = 0x00000010;
pub const RIDEV_PAGEONLY: u32     = 0x00000020;
pub const RIDEV_NOLEGACY: u32     = 0x00000030;
pub const RIDEV_INPUTSINK: u32    = 0x00000100;
pub const RIDEV_CAPTUREMOUSE: u32 = 0x00000200;

/// HID Generic Desktop Page & Usages
pub const HID_USAGE_PAGE_GENERIC: u16 = 0x01;
pub const HID_USAGE_GENERIC_MOUSE: u16 = 0x02;

/// Standard Win32 wheel tick unit (120 units per notch)
pub const WHEEL_DELTA: i16 = 120;

// ============================================================================
// Win32 Raw Input Structures (Memory Layout Matched to x86_64 ABI)
// ============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawInputHeader {
    pub dw_type: u32,
    pub dw_size: u32,
    pub h_device: *mut c_void,
    pub w_param: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RawMouse {
    pub us_flags: u16,
    pub pad: u16,
    pub us_button_flags: u16,
    pub us_button_data: i16,
    pub ul_raw_buttons: u32,
    pub last_x: i32,
    pub last_y: i32,
    pub ul_extra_information: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawInput {
    pub header: RawInputHeader,
    pub mouse: RawMouse,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawInputDevice {
    pub us_usage_page: u16,
    pub us_usage: u16,
    pub dw_flags: u32,
    pub hwnd_target: *mut c_void,
}

// ============================================================================
// Win32 External FFI
// ============================================================================

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn GetRawInputData(
        h_raw_input: *mut c_void,
        ui_command: u32,
        p_data: *mut c_void,
        pcb_size: *mut u32,
        cb_size_header: u32,
    ) -> u32;

    fn RegisterRawInputDevices(
        p_raw_input_devices: *const RawInputDevice,
        ui_num_devices: u32,
        cb_size: u32,
    ) -> i32;
}

// ============================================================================
// Processed Mouse State & Delta
// ============================================================================

/// Persistent button states tracked across raw events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseButtons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub button4: bool,
    pub button5: bool,
}

/// Extracted delta motion packet
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseDelta {
    /// Instantaneous X delta (negative = left, positive = right)
    pub dx: i32,
    /// Instantaneous Y delta (negative = up, positive = down)
    pub dy: i32,
    /// Motion flags from driver (MOUSE_MOVE_RELATIVE, MOUSE_MOVE_ABSOLUTE, etc.)
    pub flags: u16,
    /// Raw button state flags transition for this event
    pub button_flags: u16,
    /// Wheel data or raw button payload
    pub button_data: i16,
    /// Wheel delta in standard units
    pub wheel_delta: i16,
    /// Persistent button states after applying this event
    pub buttons: MouseButtons,
    /// True if original packet reported absolute coordinates
    pub is_absolute: bool,
}

// ============================================================================
// Raw Mouse Motion Parser & Zero-Ballistic Accumulator
// ============================================================================

/// High-performance accumulator with zero smoothing or ballistic curves.
#[derive(Debug, Clone)]
pub struct MouseMotionParser {
    /// Cumulative X delta since last reset / consume
    accumulated_x: i64,
    /// Cumulative Y delta since last reset / consume
    accumulated_y: i64,
    /// Total raw packet count processed
    packet_count: u64,
    /// Current persistent held button states
    buttons: MouseButtons,
    /// Last absolute X reference coordinate
    last_absolute_x: Option<i32>,
    /// Last absolute Y reference coordinate
    last_absolute_y: Option<i32>,
    /// Last wheel delta extracted
    last_wheel_delta: i16,
}

impl Default for MouseMotionParser {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseMotionParser {
    /// Creates a new uninitialized mouse accumulator
    pub fn new() -> Self {
        Self {
            accumulated_x: 0,
            accumulated_y: 0,
            packet_count: 0,
            buttons: MouseButtons::default(),
            last_absolute_x: None,
            last_absolute_y: None,
            last_wheel_delta: 0,
        }
    }

    /// Primary Win32 WM_INPUT handler.
    ///
    /// Takes the `LPARAM` from a `WM_INPUT` message, queries `GetRawInputData`,
    /// verifies mouse device origin, extracts raw deltas, and accumulates without smoothing.
    pub fn process_wm_input(&mut self, lparam: isize) -> Option<MouseDelta> {
        #[cfg(windows)]
        {
            if lparam == 0 {
                return None;
            }

            let h_raw = lparam as *mut c_void;
            let header_size = std::mem::size_of::<RawInputHeader>() as u32;
            let mut required_size: u32 = 0;

            // 1. Query required buffer size
            let res = unsafe {
                GetRawInputData(
                    h_raw,
                    RID_INPUT,
                    std::ptr::null_mut(),
                    &mut required_size,
                    header_size,
                )
            };

            if res == u32::MAX || required_size == 0 {
                return None;
            }

            // 2. Allocate buffer and fetch payload
            let mut buffer = vec![0u8; required_size as usize];
            let bytes_copied = unsafe {
                GetRawInputData(
                    h_raw,
                    RID_INPUT,
                    buffer.as_mut_ptr() as *mut c_void,
                    &mut required_size,
                    header_size,
                )
            };

            if bytes_copied == u32::MAX || (bytes_copied as usize) < std::mem::size_of::<RawInputHeader>() {
                return None;
            }

            self.parse_raw_buffer(&buffer)
        }

        #[cfg(not(windows))]
        {
            let _ = lparam;
            None
        }
    }

    /// Parses a raw byte buffer containing a Win32 RAWINPUT structure.
    pub fn parse_raw_buffer(&mut self, buffer: &[u8]) -> Option<MouseDelta> {
        let min_size = std::mem::size_of::<RawInputHeader>() + std::mem::size_of::<RawMouse>();
        if buffer.len() < min_size {
            return None;
        }

        let header_ptr = buffer.as_ptr() as *const RawInputHeader;
        let header = unsafe { std::ptr::read_unaligned(header_ptr) };

        if header.dw_type != RIM_TYPEMOUSE {
            return None;
        }

        let mouse_offset = std::mem::size_of::<RawInputHeader>();
        let mouse_ptr = unsafe { buffer.as_ptr().add(mouse_offset) } as *const RawMouse;
        let mouse = unsafe { std::ptr::read_unaligned(mouse_ptr) };

        Some(self.process_raw_mouse(&mouse))
    }

    /// Pure delta calculation and zero-ballistic accumulation from a RAWMOUSE payload.
    pub fn process_raw_mouse(&mut self, mouse: &RawMouse) -> MouseDelta {
        let is_absolute = (mouse.us_flags & MOUSE_MOVE_ABSOLUTE) != 0;

        let (dx, dy) = if is_absolute {
            let dx = match self.last_absolute_x {
                Some(prev_x) => mouse.last_x - prev_x,
                None => 0,
            };
            let dy = match self.last_absolute_y {
                Some(prev_y) => mouse.last_y - prev_y,
                None => 0,
            };

            self.last_absolute_x = Some(mouse.last_x);
            self.last_absolute_y = Some(mouse.last_y);
            (dx, dy)
        } else {
            (mouse.last_x, mouse.last_y)
        };

        // Zero-smoothing accumulation: pure direct 1:1 hardware counts
        self.accumulated_x += dx as i64;
        self.accumulated_y += dy as i64;
        self.packet_count += 1;

        // Button state machine
        if (mouse.us_button_flags & RI_MOUSE_LEFT_BUTTON_DOWN) != 0 {
            self.buttons.left = true;
        }
        if (mouse.us_button_flags & RI_MOUSE_LEFT_BUTTON_UP) != 0 {
            self.buttons.left = false;
        }

        if (mouse.us_button_flags & RI_MOUSE_RIGHT_BUTTON_DOWN) != 0 {
            self.buttons.right = true;
        }
        if (mouse.us_button_flags & RI_MOUSE_RIGHT_BUTTON_UP) != 0 {
            self.buttons.right = false;
        }

        if (mouse.us_button_flags & RI_MOUSE_MIDDLE_BUTTON_DOWN) != 0 {
            self.buttons.middle = true;
        }
        if (mouse.us_button_flags & RI_MOUSE_MIDDLE_BUTTON_UP) != 0 {
            self.buttons.middle = false;
        }

        if (mouse.us_button_flags & RI_MOUSE_BUTTON_4_DOWN) != 0 {
            self.buttons.button4 = true;
        }
        if (mouse.us_button_flags & RI_MOUSE_BUTTON_4_UP) != 0 {
            self.buttons.button4 = false;
        }

        if (mouse.us_button_flags & RI_MOUSE_BUTTON_5_DOWN) != 0 {
            self.buttons.button5 = true;
        }
        if (mouse.us_button_flags & RI_MOUSE_BUTTON_5_UP) != 0 {
            self.buttons.button5 = false;
        }

        // Wheel delta handling
        let wheel_delta = if (mouse.us_button_flags & RI_MOUSE_WHEEL) != 0 {
            let wd = mouse.us_button_data;
            self.last_wheel_delta = wd;
            wd
        } else {
            0
        };

        MouseDelta {
            dx,
            dy,
            flags: mouse.us_flags,
            button_flags: mouse.us_button_flags,
            button_data: mouse.us_button_data,
            wheel_delta,
            buttons: self.buttons,
            is_absolute,
        }
    }

    /// Returns current cumulative raw delta (dx, dy)
    pub fn accumulated(&self) -> (i64, i64) {
        (self.accumulated_x, self.accumulated_y)
    }

    /// Returns cumulative raw X delta
    pub fn accumulated_dx(&self) -> i64 {
        self.accumulated_x
    }

    /// Returns cumulative raw Y delta
    pub fn accumulated_dy(&self) -> i64 {
        self.accumulated_y
    }

    /// Consumes and returns the accumulated deltas as (i32, i32), resetting counters to 0.
    pub fn consume_accumulated(&mut self) -> (i32, i32) {
        let res = (self.accumulated_x as i32, self.accumulated_y as i32);
        self.accumulated_x = 0;
        self.accumulated_y = 0;
        res
    }

    /// Resets accumulator to zero without clearing button state
    pub fn reset_accumulator(&mut self) {
        self.accumulated_x = 0;
        self.accumulated_y = 0;
    }

    /// Returns total number of raw packets received
    pub fn packet_count(&self) -> u64 {
        self.packet_count
    }

    /// Returns current held button state
    pub fn buttons(&self) -> MouseButtons {
        self.buttons
    }
}

/// Registers the target window to receive raw mouse input.
pub fn register_raw_input_mouse(hwnd_target: *mut c_void, background_sink: bool) -> bool {
    #[cfg(windows)]
    {
        let dw_flags = if background_sink {
            RIDEV_INPUTSINK
        } else {
            0
        };

        let device = RawInputDevice {
            us_usage_page: HID_USAGE_PAGE_GENERIC,
            us_usage: HID_USAGE_GENERIC_MOUSE,
            dw_flags,
            hwnd_target,
        };

        let cb_size = std::mem::size_of::<RawInputDevice>() as u32;
        let success = unsafe {
            RegisterRawInputDevices(&device, 1, cb_size)
        };

        success != 0
    }

    #[cfg(not(windows))]
    {
        let _ = (hwnd_target, background_sink);
        false
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouse_move_relative_simple() {
        let mut parser = MouseMotionParser::new();
        let mouse = RawMouse {
            us_flags: MOUSE_MOVE_RELATIVE,
            pad: 0,
            us_button_flags: 0,
            us_button_data: 0,
            ul_raw_buttons: 0,
            last_x: 15,
            last_y: -8,
            ul_extra_information: 0,
        };

        let delta = parser.process_raw_mouse(&mouse);
        assert_eq!(delta.dx, 15);
        assert_eq!(delta.dy, -8);
        assert!(!delta.is_absolute);
        assert_eq!(parser.accumulated(), (15, -8));
        assert_eq!(parser.packet_count(), 1);
    }

    #[test]
    fn test_zero_smoothing_linear_accumulation() {
        let mut parser = MouseMotionParser::new();

        // 3 consecutive packet movements
        parser.process_raw_mouse(&RawMouse {
            us_flags: MOUSE_MOVE_RELATIVE,
            last_x: 5,
            last_y: 10,
            ..Default::default()
        });

        parser.process_raw_mouse(&RawMouse {
            us_flags: MOUSE_MOVE_RELATIVE,
            last_x: -2,
            last_y: 3,
            ..Default::default()
        });

        parser.process_raw_mouse(&RawMouse {
            us_flags: MOUSE_MOVE_RELATIVE,
            last_x: 10,
            last_y: -4,
            ..Default::default()
        });

        // Exact sum 5 - 2 + 10 = 13, 10 + 3 - 4 = 9
        assert_eq!(parser.accumulated(), (13, 9));
        assert_eq!(parser.packet_count(), 3);

        // Consume resets counters
        let consumed = parser.consume_accumulated();
        assert_eq!(consumed, (13, 9));
        assert_eq!(parser.accumulated(), (0, 0));
    }

    #[test]
    fn test_mouse_move_absolute_conversion() {
        let mut parser = MouseMotionParser::new();

        // Packet 1: Initial reference coordinate
        let d1 = parser.process_raw_mouse(&RawMouse {
            us_flags: MOUSE_MOVE_ABSOLUTE,
            last_x: 1000,
            last_y: 2000,
            ..Default::default()
        });
        assert_eq!(d1.dx, 0);
        assert_eq!(d1.dy, 0);
        assert!(d1.is_absolute);

        // Packet 2: Moved to (1050, 1980) -> delta (+50, -20)
        let d2 = parser.process_raw_mouse(&RawMouse {
            us_flags: MOUSE_MOVE_ABSOLUTE,
            last_x: 1050,
            last_y: 1980,
            ..Default::default()
        });
        assert_eq!(d2.dx, 50);
        assert_eq!(d2.dy, -20);
        assert_eq!(parser.accumulated(), (50, -20));

        // Packet 3: Moved to (1070, 2010) -> delta (+20, +30)
        let d3 = parser.process_raw_mouse(&RawMouse {
            us_flags: MOUSE_MOVE_ABSOLUTE,
            last_x: 1070,
            last_y: 2010,
            ..Default::default()
        });
        assert_eq!(d3.dx, 20);
        assert_eq!(d3.dy, 30);
        assert_eq!(parser.accumulated(), (70, 10));
    }

    #[test]
    fn test_button_flags_transitions() {
        let mut parser = MouseMotionParser::new();

        // Left button down
        let d1 = parser.process_raw_mouse(&RawMouse {
            us_button_flags: RI_MOUSE_LEFT_BUTTON_DOWN,
            ..Default::default()
        });
        assert!(d1.buttons.left);
        assert!(!d1.buttons.right);

        // Right button down (both held)
        let d2 = parser.process_raw_mouse(&RawMouse {
            us_button_flags: RI_MOUSE_RIGHT_BUTTON_DOWN,
            ..Default::default()
        });
        assert!(d2.buttons.left);
        assert!(d2.buttons.right);

        // Left button released
        let d3 = parser.process_raw_mouse(&RawMouse {
            us_button_flags: RI_MOUSE_LEFT_BUTTON_UP,
            ..Default::default()
        });
        assert!(!d3.buttons.left);
        assert!(d3.buttons.right);

        // Buttons 3, 4, 5
        let d4 = parser.process_raw_mouse(&RawMouse {
            us_button_flags: RI_MOUSE_MIDDLE_BUTTON_DOWN | RI_MOUSE_BUTTON_4_DOWN | RI_MOUSE_BUTTON_5_DOWN,
            ..Default::default()
        });
        assert!(d4.buttons.middle);
        assert!(d4.buttons.button4);
        assert!(d4.buttons.button5);

        // Release right and middle
        let d5 = parser.process_raw_mouse(&RawMouse {
            us_button_flags: RI_MOUSE_RIGHT_BUTTON_UP | RI_MOUSE_MIDDLE_BUTTON_UP,
            ..Default::default()
        });
        assert!(!d5.buttons.right);
        assert!(!d5.buttons.middle);
        assert!(d5.buttons.button4);
        assert!(d5.buttons.button5);
    }

    #[test]
    fn test_wheel_rotation() {
        let mut parser = MouseMotionParser::new();

        let delta = parser.process_raw_mouse(&RawMouse {
            us_button_flags: RI_MOUSE_WHEEL,
            us_button_data: WHEEL_DELTA,
            ..Default::default()
        });

        assert_eq!(delta.wheel_delta, 120);

        // Inactive wheel packet returns 0
        let delta_idle = parser.process_raw_mouse(&RawMouse {
            us_button_flags: 0,
            us_button_data: 0,
            ..Default::default()
        });
        assert_eq!(delta_idle.wheel_delta, 0);
    }

    #[test]
    fn test_parse_raw_buffer_simulation() {
        let mut parser = MouseMotionParser::new();

        let header = RawInputHeader {
            dw_type: RIM_TYPEMOUSE,
            dw_size: 48,
            h_device: std::ptr::null_mut(),
            w_param: 0,
        };

        let mouse = RawMouse {
            us_flags: MOUSE_MOVE_RELATIVE,
            pad: 0,
            us_button_flags: RI_MOUSE_LEFT_BUTTON_DOWN,
            us_button_data: 0,
            ul_raw_buttons: 0,
            last_x: 42,
            last_y: -17,
            ul_extra_information: 0,
        };

        let raw = RawInput { header, mouse };
        let buffer = unsafe {
            std::slice::from_raw_parts(
                &raw as *const RawInput as *const u8,
                std::mem::size_of::<RawInput>(),
            )
        };

        let delta = parser.parse_raw_buffer(buffer).expect("Buffer parse failed");
        assert_eq!(delta.dx, 42);
        assert_eq!(delta.dy, -17);
        assert!(delta.buttons.left);
        assert_eq!(parser.accumulated(), (42, -17));
    }

    #[test]
    fn test_parse_raw_buffer_wrong_type_rejected() {
        let mut parser = MouseMotionParser::new();

        let header = RawInputHeader {
            dw_type: RIM_TYPEKEYBOARD, // Not mouse
            dw_size: 48,
            h_device: std::ptr::null_mut(),
            w_param: 0,
        };

        let mouse = RawMouse::default();
        let raw = RawInput { header, mouse };
        let buffer = unsafe {
            std::slice::from_raw_parts(
                &raw as *const RawInput as *const u8,
                std::mem::size_of::<RawInput>(),
            )
        };

        assert!(parser.parse_raw_buffer(buffer).is_none());
    }

    #[test]
    fn test_wm_input_null_lparam_handled_safely() {
        let mut parser = MouseMotionParser::new();
        let res = parser.process_wm_input(0);
        assert!(res.is_none());
    }

    #[test]
    fn test_zero_ballistic_curve_linearity() {
        // Linearity test: f(x1 + x2) == f(x1) + f(x2)
        let mut p1 = MouseMotionParser::new();
        let mut p2 = MouseMotionParser::new();

        // p1 receives two micro-steps
        p1.process_raw_mouse(&RawMouse { last_x: 4, last_y: 6, ..Default::default() });
        p1.process_raw_mouse(&RawMouse { last_x: 6, last_y: 14, ..Default::default() });

        // p2 receives one aggregated step
        p2.process_raw_mouse(&RawMouse { last_x: 10, last_y: 20, ..Default::default() });

        // Zero ballistic curve means exact 1:1 parity
        assert_eq!(p1.accumulated(), p2.accumulated());
        assert_eq!(p1.accumulated(), (10, 20));
    }
}
