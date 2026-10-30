//! 64-bit Execution Stack Frame (`FFrame`) layout and bytecode stream cursor.

use crate::types::{FName, FVector, FRotator, VMError};
use std::ptr;

/// 64-bit UnrealScript Virtual Machine execution stack frame.
///
/// Disassembled from `Core.dll` with 64-bit pointer expansion:
/// - `0x00`: `vtable` - Virtual method table pointer (8 bytes)
/// - `0x08`: `node` - Executing UFunction / UState pointer or ID (8 bytes)
/// - `0x10`: `object` - Context `this` UObject pointer or ID (8 bytes)
/// - `0x18`: `code` - Program counter pointing into bytecode array (8 bytes)
/// - `0x20`: `locals` - Stack-allocated local variable storage (8 bytes)
/// - `0x28`: `previous_frame` - Linked list back to caller stack frame (8 bytes)
///
/// Total 64-bit size: 48 bytes (0x30), alignment: 8 bytes.
#[repr(C)]
pub struct FFrame {
    pub vtable: *const (),
    pub node: u64,
    pub object: u64,
    pub code: *const u8,
    pub locals: *mut u8,
    pub previous_frame: *mut FFrame,

    // Safe bounds tracking metadata for the bytecode slice
    pub(crate) code_start: *const u8,
    pub(crate) code_len: usize,
}

// Ensure 64-bit layout assertions
const _: () = {
    assert!(std::mem::size_of::<*const ()>() == 8);
    // The core C-compatible header fields occupy 48 bytes
    assert!(std::mem::offset_of!(FFrame, vtable) == 0x00);
    assert!(std::mem::offset_of!(FFrame, node) == 0x08);
    assert!(std::mem::offset_of!(FFrame, object) == 0x10);
    assert!(std::mem::offset_of!(FFrame, code) == 0x18);
    assert!(std::mem::offset_of!(FFrame, locals) == 0x20);
    assert!(std::mem::offset_of!(FFrame, previous_frame) == 0x28);
};

impl FFrame {
    /// Creates a new execution stack frame.
    pub fn new(
        node: u64,
        object: u64,
        code_slice: &[u8],
        locals_ptr: *mut u8,
        previous_frame: *mut FFrame,
    ) -> Self {
        Self {
            vtable: ptr::null(),
            node,
            object,
            code: code_slice.as_ptr(),
            locals: locals_ptr,
            previous_frame,
            code_start: code_slice.as_ptr(),
            code_len: code_slice.len(),
        }
    }

    /// Current relative offset (instruction pointer / PC) from start of bytecode.
    #[inline]
    pub fn pc(&self) -> usize {
        (self.code as usize).saturating_sub(self.code_start as usize)
    }

    /// Sets program counter to target relative bytecode offset.
    #[inline]
    pub fn set_pc(&mut self, offset: usize) -> Result<(), VMError> {
        if offset > self.code_len {
            return Err(VMError::Execution(format!(
                "Jump destination 0x{:04X} exceeds bytecode length {}",
                offset, self.code_len
            )));
        }
        self.code = unsafe { self.code_start.add(offset) };
        Ok(())
    }

    /// Checks if instruction pointer has reached or passed the end of the script.
    #[inline]
    pub fn is_eof(&self) -> bool {
        self.pc() >= self.code_len
    }

    /// Reads single byte from instruction pointer and advances.
    #[inline]
    pub fn read_u8(&mut self) -> u8 {
        if self.is_eof() {
            return 0x47; // EX_EndOfScript
        }
        unsafe {
            let b = *self.code;
            self.code = self.code.add(1);
            b
        }
    }

    /// Peeks single byte without advancing.
    #[inline]
    pub fn peek_u8(&self) -> u8 {
        if self.is_eof() {
            return 0x47;
        }
        unsafe { *self.code }
    }

    /// Reads 16-bit little endian integer.
    pub fn read_u16(&mut self) -> u16 {
        let b0 = self.read_u8() as u16;
        let b1 = self.read_u8() as u16;
        b0 | (b1 << 8)
    }

    /// Reads 32-bit little endian integer.
    pub fn read_i32(&mut self) -> i32 {
        let b0 = self.read_u8() as u32;
        let b1 = self.read_u8() as u32;
        let b2 = self.read_u8() as u32;
        let b3 = self.read_u8() as u32;
        (b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)) as i32
    }

    /// Reads 32-bit IEEE-754 floating point number.
    pub fn read_f32(&mut self) -> f32 {
        let i = self.read_i32() as u32;
        f32::from_bits(i)
    }

    /// Reads standard Unreal CompactIndex (`Core.dll:0x10147af0`).
    pub fn read_compact_index(&mut self) -> i32 {
        let b0 = self.read_u8();
        let mut more = (b0 & 0x40) != 0;
        let sign = (b0 & 0x80) != 0;
        let mut val = (b0 & 0x3F) as i32;

        if more {
            let b1 = self.read_u8();
            more = (b1 & 0x80) != 0;
            let v1 = (b1 & 0x7F) as i32;
            if more {
                let b2 = self.read_u8();
                more = (b2 & 0x80) != 0;
                let v2 = (b2 & 0x7F) as i32;
                if more {
                    let b3 = self.read_u8();
                    more = (b3 & 0x80) != 0;
                    let v3 = (b3 & 0x7F) as i32;
                    if more {
                        let b4 = self.read_u8();
                        val |= (((b4 & 0x7F) as i32) << 27) | (v3 << 20) | (v2 << 13) | (v1 << 6);
                    } else {
                        val |= (v3 << 20) | (v2 << 13) | (v1 << 6);
                    }
                } else {
                    val |= (v2 << 13) | (v1 << 6);
                }
            } else {
                val |= v1 << 6;
            }
        }

        if sign {
            -val
        } else {
            val
        }
    }

    /// Reads FName serialized as (Index CompactIndex, Number CompactIndex).
    pub fn read_name(&mut self) -> FName {
        let index = self.read_compact_index() as u32;
        let number = self.read_compact_index() as u32;
        FName::new(index, number)
    }

    /// Reads null-terminated ANSI string.
    pub fn read_string(&mut self) -> String {
        let mut bytes = Vec::new();
        loop {
            let b = self.read_u8();
            if b == 0 || self.is_eof() {
                break;
            }
            bytes.push(b);
        }
        String::from_utf8_lossy(&bytes).to_string()
    }

    /// Reads null-terminated UTF-16LE string.
    pub fn read_unicode_string(&mut self) -> String {
        let mut u16_words = Vec::new();
        loop {
            let w = self.read_u16();
            if w == 0 || self.is_eof() {
                break;
            }
            u16_words.push(w);
        }
        String::from_utf16_lossy(&u16_words)
    }

    /// Reads 12-byte FVector (3 x f32).
    pub fn read_vector(&mut self) -> FVector {
        let x = self.read_f32();
        let y = self.read_f32();
        let z = self.read_f32();
        FVector::new(x, y, z)
    }

    /// Reads 12-byte FRotator (3 x i32).
    pub fn read_rotator(&mut self) -> FRotator {
        let pitch = self.read_i32();
        let yaw = self.read_i32();
        let roll = self.read_i32();
        FRotator::new(pitch, yaw, roll)
    }
}
