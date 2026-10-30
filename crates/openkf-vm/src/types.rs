//! Core data structures, primitives, and value representations for the 64-bit UnrealScript VM.

use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FName {
    pub index: u32,
    pub number: u32,
}

impl FName {
    pub const NONE: FName = FName { index: 0, number: 0 };

    pub const fn new(index: u32, number: u32) -> Self {
        Self { index, number }
    }

    pub fn is_none(&self) -> bool {
        self.index == 0
    }
}

impl fmt::Display for FName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.number == 0 {
            write!(f, "Name_{}", self.index)
        } else {
            write!(f, "Name_{}_{}", self.index, self.number)
        }
    }
}

/// 3D Vector with standard Unreal single-precision floating point coordinates (X, Y, Z).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FVector {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl FVector {
    pub const ZERO: FVector = FVector { x: 0.0, y: 0.0, z: 0.0 };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn size_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn size(&self) -> f32 {
        self.size_squared().sqrt()
    }

    pub fn normal(&self) -> Self {
        let s = self.size();
        if s > 1e-6 {
            Self {
                x: self.x / s,
                y: self.y / s,
                z: self.z / s,
            }
        } else {
            Self::ZERO
        }
    }

    pub fn dot(&self, other: &Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(&self, other: &Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }
}

impl std::ops::Add for FVector {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl std::ops::Sub for FVector {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl std::ops::Mul<f32> for FVector {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl std::ops::Div<f32> for FVector {
    type Output = Self;
    fn div(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
            z: self.z / rhs,
        }
    }
}

/// Rotator specifying Pitch, Yaw, Roll in Unreal binary angle units (65536 units = 360 degrees).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FRotator {
    pub pitch: i32,
    pub yaw: i32,
    pub roll: i32,
}

impl FRotator {
    pub const ZERO: FRotator = FRotator { pitch: 0, yaw: 0, roll: 0 };

    pub const fn new(pitch: i32, yaw: i32, roll: i32) -> Self {
        Self { pitch, yaw, roll }
    }

    /// Normalizes rotator angles to -32768..32767 range.
    pub fn normalize(&self) -> Self {
        let norm = |a: i32| -> i32 {
            let mut v = a & 0xFFFF;
            if v >= 0x8000 {
                v -= 0x10000;
            }
            v
        };
        Self {
            pitch: norm(self.pitch),
            yaw: norm(self.yaw),
            roll: norm(self.roll),
        }
    }

    /// Extracts forward, right, and up axes vectors from rotation angles.
    pub fn get_axes(&self) -> (FVector, FVector, FVector) {
        let rad = std::f32::consts::PI / 32768.0;
        let cp = (self.pitch as f32 * rad).cos();
        let sp = (self.pitch as f32 * rad).sin();
        let cy = (self.yaw as f32 * rad).cos();
        let sy = (self.yaw as f32 * rad).sin();
        let cr = (self.roll as f32 * rad).cos();
        let sr = (self.roll as f32 * rad).sin();

        let x = FVector::new(cp * cy, cp * sy, sp);
        let y = FVector::new(
            sr * sp * cy - cr * sy,
            sr * sp * sy + cr * cy,
            -sr * cp,
        );
        let z = FVector::new(
            -(cr * sp * cy + sr * sy),
            -cr * sp * sy + sr * cy,
            cr * cp,
        );

        (x, y, z)
    }

    /// Constructs a rotator from orthonormal basis vectors.
    pub fn ortho_rotation(x: FVector, y: FVector, z: FVector) -> Self {
        let pitch = (x.z.atan2((x.x * x.x + x.y * x.y).sqrt()) * 32768.0 / std::f32::consts::PI) as i32;
        let yaw = (x.y.atan2(x.x) * 32768.0 / std::f32::consts::PI) as i32;
        let sy = (self_yaw_rad(yaw)).sin();
        let cy = (self_yaw_rad(yaw)).cos();
        let roll = ((cy * y.z - sy * y.x).atan2(cy * z.z - sy * z.x) * 32768.0 / std::f32::consts::PI) as i32;

        Self { pitch, yaw, roll }.normalize()
    }
}

fn self_yaw_rad(yaw: i32) -> f32 {
    yaw as f32 * std::f32::consts::PI / 32768.0
}

impl std::ops::Add for FRotator {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            pitch: self.pitch + rhs.pitch,
            yaw: self.yaw + rhs.yaw,
            roll: self.roll + rhs.roll,
        }
    }
}

impl std::ops::Sub for FRotator {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            pitch: self.pitch - rhs.pitch,
            yaw: self.yaw - rhs.yaw,
            roll: self.roll - rhs.roll,
        }
    }
}

/// LValue representing a mutable storage target in memory.
#[derive(Debug, Clone, PartialEq)]
pub enum LValue {
    /// Slot in function frame's local variables buffer.
    Local(usize),
    /// Property on an object instance (object ID + property offset).
    Instance { object_id: u64, offset: usize },
    /// Element within a dynamic or fixed array.
    ArrayElement {
        array_target: Box<LValue>,
        index: usize,
    },
    /// Member of a struct.
    StructMember {
        struct_target: Box<LValue>,
        member_offset: usize,
    },
}

/// Dynamic value type in the UnrealScript runtime.
#[derive(Debug, Clone, PartialEq)]
#[derive(Default)]
pub enum Value {
    #[default]
    None,
    Byte(u8),
    Int(i32),
    Bool(bool),
    Float(f32),
    Str(String),
    Name(FName),
    Vector(FVector),
    Rotator(FRotator),
    Object(Option<u64>),
    Class(Option<u64>),
    Array(Vec<Value>),
    Struct(Vec<Value>),
    Delegate { object: Option<u64>, function: FName },
    LValue(LValue),
}


impl Value {
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Byte(b) => *b != 0,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Object(opt) => opt.is_some(),
            Value::Class(opt) => opt.is_some(),
            Value::Str(s) => !s.is_empty(),
            _ => false,
        }
    }

    pub fn as_int(&self) -> i32 {
        match self {
            Value::Int(i) => *i,
            Value::Byte(b) => *b as i32,
            Value::Float(f) => *f as i32,
            Value::Bool(b) if *b => { 1 },
            _ => 0,
        }
    }

    pub fn as_float(&self) -> f32 {
        match self {
            Value::Float(f) => *f,
            Value::Int(i) => *i as f32,
            Value::Byte(b) => *b as f32,
            _ => 0.0,
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => format!("{:.4}", f),
            Value::Byte(b) => b.to_string(),
            Value::Bool(b) => (if *b { "True" } else { "False" }).to_string(),
            Value::Name(n) => n.to_string(),
            Value::Vector(v) => format!("({},{},{})", v.x, v.y, v.z),
            Value::Rotator(r) => format!("({},{},{})", r.pitch, r.yaw, r.roll),
            Value::Object(Some(id)) => format!("Object_{:016x}", id),
            Value::Object(None) => "None".to_string(),
            Value::None => "None".to_string(),
            _ => String::new(),
        }
    }

    pub fn as_vector(&self) -> FVector {
        match self {
            Value::Vector(v) => *v,
            _ => FVector::ZERO,
        }
    }

    pub fn as_rotator(&self) -> FRotator {
        match self {
            Value::Rotator(r) => *r,
            _ => FRotator::ZERO,
        }
    }

    pub fn as_object(&self) -> Option<u64> {
        match self {
            Value::Object(opt) => *opt,
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::None => "None",
            Value::Byte(_) => "Byte",
            Value::Int(_) => "Int",
            Value::Bool(_) => "Bool",
            Value::Float(_) => "Float",
            Value::Str(_) => "String",
            Value::Name(_) => "Name",
            Value::Vector(_) => "Vector",
            Value::Rotator(_) => "Rotator",
            Value::Object(_) => "Object",
            Value::Class(_) => "Class",
            Value::Array(_) => "Array",
            Value::Struct(_) => "Struct",
            Value::Delegate { .. } => "Delegate",
            Value::LValue(_) => "LValue",
        }
    }
}

/// Primitive type conversion identifiers (for `EX_PrimitiveCast`).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastToken {
    InterfaceToObject = 0x36,
    IntToFloat = 0x38,
    FloatToInt = 0x39,
    ByteToInt = 0x3A,
    IntToByte = 0x3B,
    BoolToByte = 0x3C,
    ByteToBool = 0x3D,
    IntToBool = 0x3E,
    BoolToInt = 0x3F,
    FloatToBool = 0x40,
    BoolToFloat = 0x41,
    IntToString = 0x42,
    FloatToString = 0x43,
    BoolToString = 0x44,
    VectorToString = 0x45,
    RotatorToString = 0x46,
    NameToString = 0x47,
    StringToName = 0x48,
    StringToVector = 0x49,
    StringToRotator = 0x4A,
    VectorToRotator = 0x4B,
    RotatorToVector = 0x4C,
    ObjectToString = 0x4D,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum VMError {
    #[error("Execution error: {0}")]
    Execution(String),
    #[error("Divide by zero")]
    DivisionByZero,
    #[error("Script assertion failed at line {0}")]
    AssertionFailed(u16),
    #[error("Invalid or unsupported opcode 0x{0:02X}")]
    InvalidOpcode(u8),
    #[error("Call stack underflow")]
    StackUnderflow,
    #[error("Null object reference")]
    NullReference,
    #[error("Array index out of bounds: index {index}, length {len}")]
    IndexOutOfBounds { index: usize, len: usize },
    #[error("Type mismatch: expected {expected}, got {got}")]
    TypeMismatch {
        expected: &'static str,
        got: &'static str,
    },
    #[error("Halted by EX_Stop")]
    Halted,
    #[error("Script terminated normally")]
    Terminated,
}
