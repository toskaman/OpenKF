//! Intrinsic Native Function Table (`GNatives[4096]`) and High Native Math implementations.

use crate::types::{FRotator, VMError, Value};

pub type NativeHandler = fn(args: &[Value]) -> Result<Value, VMError>;

pub struct NativeTable {
    handlers: Vec<Option<NativeHandler>>,
}

impl Default for NativeTable {
    fn default() -> Self {
        let mut table = Self {
            handlers: vec![None; 4096],
        };
        table.register_all_defaults();
        table
    }
}

impl NativeTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, index: usize, handler: NativeHandler) {
        if index < self.handlers.len() {
            self.handlers[index] = Some(handler);
        }
    }

    pub fn dispatch(&self, index: usize, args: &[Value]) -> Result<Value, VMError> {
        if let Some(Some(handler)) = self.handlers.get(index) {
            handler(args)
        } else {
            Err(VMError::Execution(format!(
                "Unregistered native function index {}",
                index
            )))
        }
    }

    fn register_all_defaults(&mut self) {
        // String Primitives
        self.register(112, native_str_concat);
        self.register(115, native_str_eq);
        self.register(116, native_str_ne);
        self.register(120, native_str_less);
        self.register(121, native_str_greater);
        self.register(122, native_str_leq);
        self.register(123, native_str_geq);
        self.register(125, native_str_len);
        self.register(126, native_str_instr);
        self.register(127, native_str_mid);
        self.register(128, native_str_left);
        self.register(168, native_str_concat_space);
        self.register(237, native_str_right);
        self.register(238, native_str_caps);
        self.register(239, native_str_chr);
        self.register(240, native_str_asc);

        // Float operations
        self.register(129, native_float_add);
        self.register(130, native_float_sub);
        self.register(131, native_float_mul);
        self.register(132, native_float_div);
        self.register(133, native_float_less);
        self.register(134, native_float_greater);
        self.register(135, native_float_leq);
        self.register(136, native_float_geq);
        self.register(137, native_float_eq);
        self.register(138, native_float_ne);

        // Int operations
        self.register(144, native_int_add);
        self.register(145, native_int_sub);
        self.register(146, native_int_mul);
        self.register(147, native_int_div);
        self.register(148, native_int_mod);
        self.register(149, native_int_less);
        self.register(150, native_int_greater);
        self.register(151, native_int_leq);
        self.register(152, native_int_geq);
        self.register(153, native_int_eq);
        self.register(154, native_int_ne);

        // Math intrinsics
        self.register(186, native_sin);
        self.register(187, native_cos);
        self.register(192, native_sqrt);
        self.register(244, native_min);
        self.register(245, native_max);
        self.register(246, native_clamp);
        self.register(247, native_abs);

        // Vector Math
        self.register(211, native_vector_size);
        self.register(212, native_vector_normal);
        self.register(213, native_vector_dot);
        self.register(214, native_vector_cross);
        self.register(215, native_vector_add);
        self.register(216, native_vector_sub);
        self.register(217, native_vector_mul_float);
        self.register(218, native_float_mul_vector);
        self.register(219, native_vector_div_float);
        self.register(220, native_vector_eq);
        self.register(221, native_vector_ne);
        self.register(222, native_vector_mirror);

        // Rotator Math
        self.register(229, native_rotator_normalize);
        self.register(230, native_rotator_ortho);
        self.register(231, native_rotator_get_axes);
        self.register(232, native_rotator_add);
        self.register(233, native_rotator_sub);
        self.register(234, native_rotator_mul_float);
        self.register(235, native_rotator_eq);
        self.register(236, native_rotator_ne);

        // Bool operations
        self.register(254, native_bool_not);
        self.register(255, native_bool_and);
        self.register(256, native_bool_or);
    }
}

/// Maps high native opcode token (0x70 .. 0x9F) to canonical GNatives index.
pub fn map_high_native_to_native_index(opcode: u8) -> usize {
    match opcode {
        // Vector math (0x70 .. 0x7B)
        0x70 => 211, // VSize
        0x71 => 212, // Normal
        0x72 => 213, // Dot
        0x73 => 214, // Cross
        0x74 => 215, // Add_VectorVector
        0x75 => 216, // Subtract_VectorVector
        0x76 => 217, // Multiply_VectorFloat
        0x77 => 218, // Multiply_FloatVector
        0x78 => 219, // Divide_VectorFloat
        0x79 => 220, // EqualEqual_VectorVector
        0x7A => 221, // NotEqual_VectorVector
        0x7B => 222, // MirrorVectorByNormal

        // Rotator math (0x80 .. 0x87)
        0x80 => 229, // Normalize
        0x81 => 230, // OrthoRotation
        0x82 => 231, // GetAxes
        0x83 => 232, // Add_RotatorRotator
        0x84 => 233, // Subtract_RotatorRotator
        0x85 => 234, // Multiply_RotatorFloat
        0x86 => 235, // EqualEqual_RotatorRotator
        0x87 => 236, // NotEqual_RotatorRotator

        // String primitives (0x90 .. 0x9F)
        0x90 => 125, // Len
        0x91 => 126, // InStr
        0x92 => 127, // Mid
        0x93 => 128, // Left
        0x94 => 237, // Right
        0x95 => 238, // Caps
        0x96 => 239, // Chr
        0x97 => 240, // Asc
        0x98 => 112, // Concat_StrStr ($)
        0x99 => 168, // Concat_StrStrSpace (@)
        0x9A => 115, // EqualEqual_StrStr
        0x9B => 116, // NotEqual_StrStr
        0x9C => 120, // Less_StrStr
        0x9D => 121, // Greater_StrStr
        0x9E => 122, // LessEqual_StrStr
        0x9F => 123, // GreaterEqual_StrStr
        _ => opcode as usize,
    }
}

// ==================== VECTOR MATH IMPLEMENTATIONS ====================

fn native_vector_size(args: &[Value]) -> Result<Value, VMError> {
    let v = args.first().ok_or(VMError::StackUnderflow)?.as_vector();
    Ok(Value::Float(v.size()))
}

fn native_vector_normal(args: &[Value]) -> Result<Value, VMError> {
    let v = args.first().ok_or(VMError::StackUnderflow)?.as_vector();
    Ok(Value::Vector(v.normal()))
}

fn native_vector_dot(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let v1 = args[0].as_vector();
    let v2 = args[1].as_vector();
    Ok(Value::Float(v1.dot(&v2)))
}

fn native_vector_cross(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let v1 = args[0].as_vector();
    let v2 = args[1].as_vector();
    Ok(Value::Vector(v1.cross(&v2)))
}

fn native_vector_add(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let v1 = args[0].as_vector();
    let v2 = args[1].as_vector();
    Ok(Value::Vector(v1 + v2))
}

fn native_vector_sub(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let v1 = args[0].as_vector();
    let v2 = args[1].as_vector();
    Ok(Value::Vector(v1 - v2))
}

fn native_vector_mul_float(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let v = args[0].as_vector();
    let f = args[1].as_float();
    Ok(Value::Vector(v * f))
}

fn native_float_mul_vector(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let f = args[0].as_float();
    let v = args[1].as_vector();
    Ok(Value::Vector(v * f))
}

fn native_vector_div_float(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let v = args[0].as_vector();
    let f = args[1].as_float();
    if f.abs() < 1e-8 {
        return Err(VMError::DivisionByZero);
    }
    Ok(Value::Vector(v / f))
}

fn native_vector_eq(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let v1 = args[0].as_vector();
    let v2 = args[1].as_vector();
    Ok(Value::Bool(v1 == v2))
}

fn native_vector_ne(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let v1 = args[0].as_vector();
    let v2 = args[1].as_vector();
    Ok(Value::Bool(v1 != v2))
}

fn native_vector_mirror(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let v = args[0].as_vector();
    let n = args[1].as_vector().normal();
    // V - 2 * (V . N) * N
    let mirrored = v - n * (2.0 * v.dot(&n));
    Ok(Value::Vector(mirrored))
}

// ==================== ROTATOR MATH IMPLEMENTATIONS ====================

fn native_rotator_normalize(args: &[Value]) -> Result<Value, VMError> {
    let r = args.first().ok_or(VMError::StackUnderflow)?.as_rotator();
    Ok(Value::Rotator(r.normalize()))
}

fn native_rotator_ortho(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 3 { return Err(VMError::StackUnderflow); }
    let x = args[0].as_vector();
    let y = args[1].as_vector();
    let z = args[2].as_vector();
    Ok(Value::Rotator(FRotator::ortho_rotation(x, y, z)))
}

fn native_rotator_get_axes(args: &[Value]) -> Result<Value, VMError> {
    let r = args.first().ok_or(VMError::StackUnderflow)?.as_rotator();
    let (x, _y, _z) = r.get_axes();
    // In UnrealScript, GetAxes converts rotator to forward vector when used as expression
    Ok(Value::Vector(x))
}

fn native_rotator_add(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let r1 = args[0].as_rotator();
    let r2 = args[1].as_rotator();
    Ok(Value::Rotator((r1 + r2).normalize()))
}

fn native_rotator_sub(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let r1 = args[0].as_rotator();
    let r2 = args[1].as_rotator();
    Ok(Value::Rotator((r1 - r2).normalize()))
}

fn native_rotator_mul_float(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let r = args[0].as_rotator();
    let f = args[1].as_float();
    Ok(Value::Rotator(FRotator::new(
        (r.pitch as f32 * f) as i32,
        (r.yaw as f32 * f) as i32,
        (r.roll as f32 * f) as i32,
    ).normalize()))
}

fn native_rotator_eq(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let r1 = args[0].as_rotator().normalize();
    let r2 = args[1].as_rotator().normalize();
    Ok(Value::Bool(r1 == r2))
}

fn native_rotator_ne(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let r1 = args[0].as_rotator().normalize();
    let r2 = args[1].as_rotator().normalize();
    Ok(Value::Bool(r1 != r2))
}

// ==================== STRING PRIMITIVES IMPLEMENTATIONS ====================

fn native_str_len(args: &[Value]) -> Result<Value, VMError> {
    let s = args.first().ok_or(VMError::StackUnderflow)?.as_str();
    Ok(Value::Int(s.len() as i32))
}

fn native_str_instr(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let s = args[0].as_str();
    let sub = args[1].as_str();
    match s.find(&sub) {
        Some(idx) => Ok(Value::Int(idx as i32)),
        None => Ok(Value::Int(-1)),
    }
}

fn native_str_mid(args: &[Value]) -> Result<Value, VMError> {
    if args.is_empty() { return Err(VMError::StackUnderflow); }
    let s = args[0].as_str();
    let start = args.get(1).map(|v| v.as_int().max(0) as usize).unwrap_or(0);
    let count = args.get(2).map(|v| v.as_int().max(0) as usize).unwrap_or(s.len());

    let res: String = s.chars().skip(start).take(count).collect();
    Ok(Value::Str(res))
}

fn native_str_left(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let s = args[0].as_str();
    let count = args[1].as_int().max(0) as usize;
    let res: String = s.chars().take(count).collect();
    Ok(Value::Str(res))
}

fn native_str_right(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let s = args[0].as_str();
    let count = args[1].as_int().max(0) as usize;
    let total_chars = s.chars().count();
    let skip = total_chars.saturating_sub(count);
    let res: String = s.chars().skip(skip).collect();
    Ok(Value::Str(res))
}

fn native_str_caps(args: &[Value]) -> Result<Value, VMError> {
    let s = args.first().ok_or(VMError::StackUnderflow)?.as_str();
    Ok(Value::Str(s.to_uppercase()))
}

fn native_str_chr(args: &[Value]) -> Result<Value, VMError> {
    let code = args.first().ok_or(VMError::StackUnderflow)?.as_int() as u8;
    Ok(Value::Str(String::from(code as char)))
}

fn native_str_asc(args: &[Value]) -> Result<Value, VMError> {
    let s = args.first().ok_or(VMError::StackUnderflow)?.as_str();
    let c = s.bytes().next().unwrap_or(0);
    Ok(Value::Int(c as i32))
}

fn native_str_concat(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let s1 = args[0].as_str();
    let s2 = args[1].as_str();
    Ok(Value::Str(format!("{}{}", s1, s2)))
}

fn native_str_concat_space(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let s1 = args[0].as_str();
    let s2 = args[1].as_str();
    Ok(Value::Str(format!("{} {}", s1, s2)))
}

fn native_str_eq(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let s1 = args[0].as_str();
    let s2 = args[1].as_str();
    Ok(Value::Bool(s1.eq_ignore_ascii_case(&s2)))
}

fn native_str_ne(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let s1 = args[0].as_str();
    let s2 = args[1].as_str();
    Ok(Value::Bool(!s1.eq_ignore_ascii_case(&s2)))
}

fn native_str_less(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_str() < args[1].as_str()))
}

fn native_str_greater(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_str() > args[1].as_str()))
}

fn native_str_leq(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_str() <= args[1].as_str()))
}

fn native_str_geq(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_str() >= args[1].as_str()))
}

// ==================== ARITHMETIC & LOGIC ====================

fn native_float_add(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Float(args[0].as_float() + args[1].as_float()))
}

fn native_float_sub(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Float(args[0].as_float() - args[1].as_float()))
}

fn native_float_mul(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Float(args[0].as_float() * args[1].as_float()))
}

fn native_float_div(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let d = args[1].as_float();
    if d.abs() < 1e-8 { return Err(VMError::DivisionByZero); }
    Ok(Value::Float(args[0].as_float() / d))
}

fn native_float_less(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_float() < args[1].as_float()))
}

fn native_float_greater(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_float() > args[1].as_float()))
}

fn native_float_leq(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_float() <= args[1].as_float()))
}

fn native_float_geq(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_float() >= args[1].as_float()))
}

fn native_float_eq(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool((args[0].as_float() - args[1].as_float()).abs() < 1e-6))
}

fn native_float_ne(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool((args[0].as_float() - args[1].as_float()).abs() >= 1e-6))
}

fn native_int_add(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Int(args[0].as_int().wrapping_add(args[1].as_int())))
}

fn native_int_sub(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Int(args[0].as_int().wrapping_sub(args[1].as_int())))
}

fn native_int_mul(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Int(args[0].as_int().wrapping_mul(args[1].as_int())))
}

fn native_int_div(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let d = args[1].as_int();
    if d == 0 { return Err(VMError::DivisionByZero); }
    Ok(Value::Int(args[0].as_int() / d))
}

fn native_int_mod(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let d = args[1].as_int();
    if d == 0 { return Err(VMError::DivisionByZero); }
    Ok(Value::Int(args[0].as_int() % d))
}

fn native_int_less(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_int() < args[1].as_int()))
}

fn native_int_greater(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_int() > args[1].as_int()))
}

fn native_int_leq(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_int() <= args[1].as_int()))
}

fn native_int_geq(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_int() >= args[1].as_int()))
}

fn native_int_eq(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_int() == args[1].as_int()))
}

fn native_int_ne(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_int() != args[1].as_int()))
}

fn native_bool_not(args: &[Value]) -> Result<Value, VMError> {
    let b = args.first().ok_or(VMError::StackUnderflow)?.as_bool();
    Ok(Value::Bool(!b))
}

fn native_bool_and(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_bool() && args[1].as_bool()))
}

fn native_bool_or(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    Ok(Value::Bool(args[0].as_bool() || args[1].as_bool()))
}

fn native_sin(args: &[Value]) -> Result<Value, VMError> {
    let a = args.first().ok_or(VMError::StackUnderflow)?.as_float();
    Ok(Value::Float(a.sin()))
}

fn native_cos(args: &[Value]) -> Result<Value, VMError> {
    let a = args.first().ok_or(VMError::StackUnderflow)?.as_float();
    Ok(Value::Float(a.cos()))
}

fn native_sqrt(args: &[Value]) -> Result<Value, VMError> {
    let a = args.first().ok_or(VMError::StackUnderflow)?.as_float();
    if a < 0.0 {
        return Err(VMError::Execution("Sqrt of negative number".to_string()));
    }
    Ok(Value::Float(a.sqrt()))
}

fn native_min(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let a = args[0].as_float();
    let b = args[1].as_float();
    Ok(Value::Float(a.min(b)))
}

fn native_max(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 2 { return Err(VMError::StackUnderflow); }
    let a = args[0].as_float();
    let b = args[1].as_float();
    Ok(Value::Float(a.max(b)))
}

fn native_clamp(args: &[Value]) -> Result<Value, VMError> {
    if args.len() < 3 { return Err(VMError::StackUnderflow); }
    let val = args[0].as_float();
    let min = args[1].as_float();
    let max = args[2].as_float();
    Ok(Value::Float(val.clamp(min, max)))
}

fn native_abs(args: &[Value]) -> Result<Value, VMError> {
    let a = args.first().ok_or(VMError::StackUnderflow)?.as_float();
    Ok(Value::Float(a.abs()))
}
