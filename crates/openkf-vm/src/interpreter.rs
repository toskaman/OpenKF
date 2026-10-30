//! 64-bit UnrealScript Virtual Machine interpreter and expression evaluator.

use crate::context::VMContext;
use crate::frame::FFrame;
use crate::natives::{map_high_native_to_native_index, NativeTable};
use crate::opcodes::Opcode;
use crate::types::{FName, LValue, VMError, Value};
use std::collections::HashMap;

/// 64-bit UnrealScript Virtual Machine.
pub struct VirtualMachine {
    pub natives: NativeTable,
    pub locals_stack: Vec<Vec<Value>>,
    pub switch_stack: Vec<Value>,
    pub label_tables: HashMap<u64, HashMap<FName, u16>>,
    pub return_value: Value,
    pub has_returned: bool,
    pub is_halted: bool,
}

impl Default for VirtualMachine {
    fn default() -> Self {
        Self {
            natives: NativeTable::new(),
            locals_stack: Vec::new(),
            switch_stack: Vec::new(),
            label_tables: HashMap::new(),
            return_value: Value::None,
            has_returned: false,
            is_halted: false,
        }
    }
}

impl VirtualMachine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Executes a function's bytecode stream to completion.
    pub fn execute_function(
        &mut self,
        func_node: u64,
        object_id: u64,
        bytecode: &[u8],
        args: &[Value],
        local_count: usize,
        context: &mut dyn VMContext,
    ) -> Result<Value, VMError> {
        let total_locals = local_count.max(args.len());
        let mut locals = vec![Value::None; total_locals];
        for (i, arg) in args.iter().enumerate() {
            locals[i] = arg.clone();
        }

        self.locals_stack.push(locals);
        self.return_value = Value::None;
        self.has_returned = false;
        self.is_halted = false;

        let mut frame = FFrame::new(
            func_node,
            object_id,
            bytecode,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );

        while !frame.is_eof() && !self.has_returned && !self.is_halted {
            let _ = self.step(&mut frame, context)?;
        }

        self.locals_stack.pop();
        Ok(self.return_value.clone())
    }

    /// Single VM execution step (`FFrame::Step`).
    pub fn step(
        &mut self,
        frame: &mut FFrame,
        context: &mut dyn VMContext,
    ) -> Result<Value, VMError> {
        if frame.is_eof() || self.has_returned || self.is_halted {
            return Ok(Value::None);
        }

        self.evaluate_expression(frame, context)
    }

    /// Evaluates an expression token and returns either a computed Value or an LValue.
    pub fn evaluate_expression(
        &mut self,
        frame: &mut FFrame,
        context: &mut dyn VMContext,
    ) -> Result<Value, VMError> {
        let opcode_byte = frame.read_u8();

        // 1. High Native dispatch (0x70 .. 0x9F)
        if (0x70..=0x9F).contains(&opcode_byte) {
            return self.evaluate_high_native(opcode_byte, frame, context);
        }

        // 2. Core 72 Language Opcodes (0x00 .. 0x47)
        let op = match Opcode::from_u8(opcode_byte) {
            Some(o) => o,
            None => return Err(VMError::InvalidOpcode(opcode_byte)),
        };

        match op {
            Opcode::LocalVariable => {
                let idx = frame.read_compact_index() as usize;
                Ok(Value::LValue(LValue::Local(idx)))
            }

            Opcode::InstanceVariable => {
                let offset = frame.read_compact_index() as usize;
                Ok(Value::LValue(LValue::Instance {
                    object_id: frame.object,
                    offset,
                }))
            }

            Opcode::DefaultVariable => {
                let class_id = frame.read_compact_index() as u64;
                let offset = frame.read_compact_index() as usize;
                context.get_default_property(class_id, offset)
            }

            Opcode::Return => {
                let ret_val = self.evaluate_rvalue(frame, context)?;
                self.return_value = ret_val.clone();
                self.has_returned = true;
                Ok(ret_val)
            }

            Opcode::Switch => {
                let _size = frame.read_u8();
                let cond = self.evaluate_rvalue(frame, context)?;
                self.switch_stack.push(cond);
                Ok(Value::None)
            }

            Opcode::Jump => {
                let target = frame.read_u16() as usize;
                frame.set_pc(target)?;
                Ok(Value::None)
            }

            Opcode::JumpIfNot => {
                let target = frame.read_u16() as usize;
                let cond = self.evaluate_rvalue(frame, context)?;
                if !cond.as_bool() {
                    frame.set_pc(target)?;
                }
                Ok(Value::None)
            }

            Opcode::Stop => {
                self.is_halted = true;
                Ok(Value::None)
            }

            Opcode::Assert => {
                let line = frame.read_u16();
                let cond = self.evaluate_rvalue(frame, context)?;
                if !cond.as_bool() {
                    return Err(VMError::AssertionFailed(line));
                }
                Ok(Value::None)
            }

            Opcode::Case => {
                let next_case_offset = frame.read_u16() as usize;
                if next_case_offset == 0xFFFF {
                    // Default case
                    Ok(Value::None)
                } else {
                    let case_val = self.evaluate_rvalue(frame, context)?;
                    let current_switch = self.switch_stack.last().cloned().unwrap_or(Value::None);
                    if current_switch != case_val {
                        frame.set_pc(next_case_offset)?;
                    }
                    Ok(Value::None)
                }
            }

            Opcode::Nothing => Ok(Value::None),

            Opcode::LabelTable => {
                let mut map = HashMap::new();
                loop {
                    let name = frame.read_name();
                    if name.is_none() {
                        break;
                    }
                    let offset = frame.read_u16();
                    map.insert(name, offset);
                }
                self.label_tables.insert(frame.node, map);
                Ok(Value::None)
            }

            Opcode::GotoLabel => {
                let name = self.evaluate_rvalue(frame, context)?;
                if let Value::Name(n) = name {
                    if let Some(table) = self.label_tables.get(&frame.node) {
                        if let Some(&target) = table.get(&n) {
                            frame.set_pc(target as usize)?;
                        }
                    }
                }
                Ok(Value::None)
            }

            Opcode::EatString => {
                let _ = self.evaluate_rvalue(frame, context)?;
                Ok(Value::None)
            }

            Opcode::Let => {
                let lval = self.evaluate_lvalue(frame, context)?;
                let rval = self.evaluate_rvalue(frame, context)?;
                self.write_lvalue(&lval, rval.clone(), frame, context)?;
                Ok(rval)
            }

            Opcode::DynArrayElement => {
                let arr_lval = self.evaluate_lvalue(frame, context)?;
                let idx_val = self.evaluate_rvalue(frame, context)?;
                let index = idx_val.as_int().max(0) as usize;
                Ok(Value::LValue(LValue::ArrayElement {
                    array_target: Box::new(arr_lval),
                    index,
                }))
            }

            Opcode::New => {
                let outer = self.evaluate_rvalue(frame, context)?.as_object();
                let name = match self.evaluate_rvalue(frame, context)? {
                    Value::Name(n) => n,
                    _ => FName::NONE,
                };
                let flags = self.evaluate_rvalue(frame, context)?.as_int() as u32;
                let class_id = self.evaluate_rvalue(frame, context)?.as_int() as u64;

                let new_id = context.spawn_object(class_id, outer, &name, flags)?;
                Ok(Value::Object(Some(new_id)))
            }

            Opcode::ClassContext => {
                let _class = self.evaluate_rvalue(frame, context)?;
                let _skip = frame.read_u16();
                self.evaluate_rvalue(frame, context)
            }

            Opcode::MetaCast => {
                let meta_class_id = frame.read_compact_index() as u64;
                let val = self.evaluate_rvalue(frame, context)?;
                if let Value::Class(Some(cid)) = val {
                    if context.meta_cast(cid, meta_class_id) {
                        Ok(Value::Class(Some(cid)))
                    } else {
                        Ok(Value::Class(None))
                    }
                } else {
                    Ok(Value::Class(None))
                }
            }

            Opcode::LetBool => {
                let lval = self.evaluate_lvalue(frame, context)?;
                let rval = self.evaluate_rvalue(frame, context)?;
                let bool_val = Value::Bool(rval.as_bool());
                self.write_lvalue(&lval, bool_val.clone(), frame, context)?;
                Ok(bool_val)
            }

            Opcode::EndParmValue | Opcode::EndFunctionParms => Ok(Value::None),

            Opcode::SelfToken => Ok(Value::Object(Some(frame.object))),

            Opcode::Skip => {
                let skip_offset = frame.read_u16() as usize;
                let val = self.evaluate_rvalue(frame, context)?;
                if !val.as_bool() {
                    frame.set_pc(skip_offset)?;
                }
                Ok(val)
            }

            Opcode::Context => {
                let ctx_val = self.evaluate_rvalue(frame, context)?;
                let skip_offset = frame.read_u16() as usize;

                if let Some(target_obj) = ctx_val.as_object() {
                    let prev_obj = frame.object;
                    frame.object = target_obj;
                    let result = self.evaluate_rvalue(frame, context);
                    frame.object = prev_obj;
                    result
                } else {
                    frame.set_pc(skip_offset)?;
                    Ok(Value::None)
                }
            }

            Opcode::ArrayElement => {
                let idx_val = self.evaluate_rvalue(frame, context)?;
                let arr_lval = self.evaluate_lvalue(frame, context)?;
                let index = idx_val.as_int().max(0) as usize;
                Ok(Value::LValue(LValue::ArrayElement {
                    array_target: Box::new(arr_lval),
                    index,
                }))
            }

            Opcode::VirtualFunction => {
                let func_name = frame.read_name();
                let args = self.collect_function_args(frame, context)?;
                context.call_virtual(frame.object, &func_name, args)
            }

            Opcode::FinalFunction => {
                let func_id = frame.read_compact_index() as u64;
                let args = self.collect_function_args(frame, context)?;
                context.call_final(func_id, args)
            }

            Opcode::IntConst => {
                let val = frame.read_i32();
                Ok(Value::Int(val))
            }

            Opcode::FloatConst => {
                let val = frame.read_f32();
                Ok(Value::Float(val))
            }

            Opcode::StringConst => {
                let s = frame.read_string();
                Ok(Value::Str(s))
            }

            Opcode::ObjectConst => {
                let id = frame.read_compact_index() as u64;
                Ok(Value::Object(if id != 0 { Some(id) } else { None }))
            }

            Opcode::NameConst => {
                let name = frame.read_name();
                Ok(Value::Name(name))
            }

            Opcode::RotatorConst => {
                let rot = frame.read_rotator();
                Ok(Value::Rotator(rot))
            }

            Opcode::VectorConst => {
                let vec = frame.read_vector();
                Ok(Value::Vector(vec))
            }

            Opcode::ByteConst => {
                let b = frame.read_u8();
                Ok(Value::Byte(b))
            }

            Opcode::IntZero => Ok(Value::Int(0)),
            Opcode::IntOne => Ok(Value::Int(1)),
            Opcode::True => Ok(Value::Bool(true)),
            Opcode::False => Ok(Value::Bool(false)),

            Opcode::Native => {
                let native_idx = frame.read_u16() as usize;
                let args = self.collect_function_args(frame, context)?;
                self.natives.dispatch(native_idx, &args)
            }

            Opcode::NoObject => Ok(Value::Object(None)),

            Opcode::IntConstByte => {
                let b = frame.read_u8();
                Ok(Value::Int(b as i32))
            }

            Opcode::BoolVariable => {
                let lval = self.evaluate_lvalue(frame, context)?;
                let val = self.read_lvalue(&lval, frame, context)?;
                Ok(Value::Bool(val.as_bool()))
            }

            Opcode::DynamicCast => {
                let target_class_id = frame.read_compact_index() as u64;
                let target_obj = self.evaluate_rvalue(frame, context)?;
                if let Some(oid) = target_obj.as_object() {
                    if context.dynamic_cast(oid, target_class_id) {
                        Ok(Value::Object(Some(oid)))
                    } else {
                        Ok(Value::Object(None))
                    }
                } else {
                    Ok(Value::Object(None))
                }
            }

            Opcode::Iterator => {
                let _iter_expr = self.evaluate_rvalue(frame, context)?;
                let skip_offset = frame.read_u16() as usize;
                // Basic loop iterator step
                frame.set_pc(skip_offset)?;
                Ok(Value::None)
            }

            Opcode::StructCmpEq => {
                let _struct_type = frame.read_compact_index();
                let v1 = self.evaluate_rvalue(frame, context)?;
                let v2 = self.evaluate_rvalue(frame, context)?;
                Ok(Value::Bool(v1 == v2))
            }

            Opcode::StructCmpNe => {
                let _struct_type = frame.read_compact_index();
                let v1 = self.evaluate_rvalue(frame, context)?;
                let v2 = self.evaluate_rvalue(frame, context)?;
                Ok(Value::Bool(v1 != v2))
            }

            Opcode::UnicodeStringConst => {
                let s = frame.read_unicode_string();
                Ok(Value::Str(s))
            }

            Opcode::StructMember => {
                let member_offset = frame.read_compact_index() as usize;
                let struct_lval = self.evaluate_lvalue(frame, context)?;
                Ok(Value::LValue(LValue::StructMember {
                    struct_target: Box::new(struct_lval),
                    member_offset,
                }))
            }

            Opcode::DynArrayLength => {
                let arr_lval = self.evaluate_lvalue(frame, context)?;
                let arr_val = self.read_lvalue(&arr_lval, frame, context)?;
                if let Value::Array(vec) = arr_val {
                    Ok(Value::Int(vec.len() as i32))
                } else {
                    Ok(Value::Int(0))
                }
            }

            Opcode::GlobalFunction => {
                let func_name = frame.read_name();
                let args = self.collect_function_args(frame, context)?;
                context.call_global(frame.object, &func_name, args)
            }

            Opcode::PrimitiveCast => {
                let cast_token = frame.read_u8();
                let val = self.evaluate_rvalue(frame, context)?;
                self.evaluate_primitive_cast(cast_token, val)
            }

            Opcode::DynArrayRemove => {
                let arr_lval = self.evaluate_lvalue(frame, context)?;
                let idx_val = self.evaluate_rvalue(frame, context)?;
                let count_val = self.evaluate_rvalue(frame, context)?;
                let index = idx_val.as_int().max(0) as usize;
                let count = count_val.as_int().max(0) as usize;

                let mut arr_val = self.read_lvalue(&arr_lval, frame, context)?;
                if let Value::Array(ref mut vec) = arr_val {
                    for _ in 0..count {
                        if index < vec.len() {
                            vec.remove(index);
                        }
                    }
                }
                self.write_lvalue(&arr_lval, arr_val, frame, context)?;
                Ok(Value::None)
            }

            Opcode::DebugInfo => {
                let _line = frame.read_i32();
                let _pos = frame.read_i32();
                Ok(Value::None)
            }

            Opcode::DelegateFunction => {
                let _prop = frame.read_compact_index();
                let _args = self.collect_function_args(frame, context)?;
                Ok(Value::None)
            }

            Opcode::DelegateProperty => {
                let func_name = frame.read_name();
                Ok(Value::Delegate {
                    object: Some(frame.object),
                    function: func_name,
                })
            }

            Opcode::LetDelegate => {
                let lval = self.evaluate_lvalue(frame, context)?;
                let rval = self.evaluate_rvalue(frame, context)?;
                self.write_lvalue(&lval, rval.clone(), frame, context)?;
                Ok(rval)
            }

            Opcode::DynArrayAdd => {
                let arr_lval = self.evaluate_lvalue(frame, context)?;
                let count_val = self.evaluate_rvalue(frame, context)?;
                let count = count_val.as_int().max(0) as usize;

                let mut arr_val = self.read_lvalue(&arr_lval, frame, context)?;
                if let Value::Array(ref mut vec) = arr_val {
                    for _ in 0..count {
                        vec.push(Value::None);
                    }
                }
                self.write_lvalue(&arr_lval, arr_val, frame, context)?;
                Ok(Value::None)
            }

            Opcode::DynArrayInsert => {
                let arr_lval = self.evaluate_lvalue(frame, context)?;
                let idx_val = self.evaluate_rvalue(frame, context)?;
                let count_val = self.evaluate_rvalue(frame, context)?;
                let index = idx_val.as_int().max(0) as usize;
                let count = count_val.as_int().max(0) as usize;

                let mut arr_val = self.read_lvalue(&arr_lval, frame, context)?;
                if let Value::Array(ref mut vec) = arr_val {
                    for _ in 0..count {
                        if index <= vec.len() {
                            vec.insert(index, Value::None);
                        }
                    }
                }
                self.write_lvalue(&arr_lval, arr_val, frame, context)?;
                Ok(Value::None)
            }

            Opcode::DynArrayRemoveAll => {
                let arr_lval = self.evaluate_lvalue(frame, context)?;
                let mut arr_val = self.read_lvalue(&arr_lval, frame, context)?;
                if let Value::Array(ref mut vec) = arr_val {
                    vec.clear();
                }
                self.write_lvalue(&arr_lval, arr_val, frame, context)?;
                Ok(Value::None)
            }

            Opcode::EndOfScript => {
                self.has_returned = true;
                Ok(Value::None)
            }

            Opcode::Unknown2A
            | Opcode::Unknown2B
            | Opcode::Unknown30
            | Opcode::Unknown31
            | Opcode::Unknown3E
            | Opcode::Unknown3F
            | Opcode::Unknown40
            | Opcode::Unknown41
            | Opcode::Unknown45
            | Opcode::Unknown46 => Ok(Value::None),

            Opcode::HighNativeVectorStart
            | Opcode::HighNativeRotatorStart
            | Opcode::HighNativeStringStart => {
                self.evaluate_high_native(op as u8, frame, context)
            }
        }
    }

    /// Evaluates high native operations (0x70 .. 0x9F) with automatic argument parsing.
    fn evaluate_high_native(
        &mut self,
        opcode_byte: u8,
        frame: &mut FFrame,
        context: &mut dyn VMContext,
    ) -> Result<Value, VMError> {
        let arity = match opcode_byte {
            // Unary vector math
            0x70 | 0x71 => 1,
            // Binary vector math
            0x72..=0x7B => 2,
            // Unary rotator math
            0x80 | 0x82 => 1,
            // Ternary rotator ortho
            0x81 => 3,
            // Binary rotator math
            0x83..=0x87 => 2,
            // String unary: Len, Caps, Chr, Asc
            0x90 | 0x95 | 0x96 | 0x97 => 1,
            // String ternary: Mid
            0x92 => 3,
            // String binary: InStr, Left, Right, Concat, Compare
            0x91 | 0x93 | 0x94 | 0x98..=0x9F => 2,
            _ => 1,
        };

        let mut args = Vec::with_capacity(arity);
        for _ in 0..arity {
            args.push(self.evaluate_rvalue(frame, context)?);
        }

        let native_idx = map_high_native_to_native_index(opcode_byte);
        self.natives.dispatch(native_idx, &args)
    }

    /// Evaluates an expression ensuring it produces an RValue (dereferencing LValues).
    pub fn evaluate_rvalue(
        &mut self,
        frame: &mut FFrame,
        context: &mut dyn VMContext,
    ) -> Result<Value, VMError> {
        let val = self.evaluate_expression(frame, context)?;
        match val {
            Value::LValue(lval) => self.read_lvalue(&lval, frame, context),
            other => Ok(other),
        }
    }

    /// Evaluates an expression ensuring it produces an assignable LValue.
    pub fn evaluate_lvalue(
        &mut self,
        frame: &mut FFrame,
        context: &mut dyn VMContext,
    ) -> Result<LValue, VMError> {
        let val = self.evaluate_expression(frame, context)?;
        match val {
            Value::LValue(lval) => Ok(lval),
            _ => Err(VMError::TypeMismatch {
                expected: "LValue",
                got: "RValue",
            }),
        }
    }

    /// Reads value from an LValue target.
    pub fn read_lvalue(
        &self,
        lval: &LValue,
        _frame: &FFrame,
        context: &dyn VMContext,
    ) -> Result<Value, VMError> {
        match lval {
            LValue::Local(slot) => {
                if let Some(locals) = self.locals_stack.last() {
                    Ok(locals.get(*slot).cloned().unwrap_or(Value::None))
                } else {
                    Err(VMError::StackUnderflow)
                }
            }
            LValue::Instance { object_id, offset } => context.get_property(*object_id, *offset),
            LValue::ArrayElement { array_target, index } => {
                let arr_val = self.read_lvalue(array_target, _frame, context)?;
                if let Value::Array(vec) = arr_val {
                    if *index < vec.len() {
                        Ok(vec[*index].clone())
                    } else {
                        Err(VMError::IndexOutOfBounds {
                            index: *index,
                            len: vec.len(),
                        })
                    }
                } else {
                    Err(VMError::TypeMismatch {
                        expected: "Array",
                        got: arr_val.type_name(),
                    })
                }
            }
            LValue::StructMember {
                struct_target,
                member_offset,
            } => {
                let struct_val = self.read_lvalue(struct_target, _frame, context)?;
                if let Value::Struct(vec) = struct_val {
                    Ok(vec.get(*member_offset).cloned().unwrap_or(Value::None))
                } else {
                    Err(VMError::TypeMismatch {
                        expected: "Struct",
                        got: struct_val.type_name(),
                    })
                }
            }
        }
    }

    /// Writes value into an LValue target.
    pub fn write_lvalue(
        &mut self,
        lval: &LValue,
        val: Value,
        _frame: &mut FFrame,
        context: &mut dyn VMContext,
    ) -> Result<(), VMError> {
        match lval {
            LValue::Local(slot) => {
                if let Some(locals) = self.locals_stack.last_mut() {
                    if *slot >= locals.len() {
                        locals.resize(*slot + 1, Value::None);
                    }
                    locals[*slot] = val;
                    Ok(())
                } else {
                    Err(VMError::StackUnderflow)
                }
            }
            LValue::Instance { object_id, offset } => {
                context.set_property(*object_id, *offset, val)
            }
            LValue::ArrayElement { array_target, index } => {
                let mut arr_val = self.read_lvalue(array_target, _frame, context)?;
                if let Value::Array(ref mut vec) = arr_val {
                    if *index >= vec.len() {
                        vec.resize(*index + 1, Value::None);
                    }
                    vec[*index] = val;
                    self.write_lvalue(array_target, arr_val, _frame, context)
                } else {
                    Err(VMError::TypeMismatch {
                        expected: "Array",
                        got: arr_val.type_name(),
                    })
                }
            }
            LValue::StructMember {
                struct_target,
                member_offset,
            } => {
                let mut struct_val = self.read_lvalue(struct_target, _frame, context)?;
                if let Value::Struct(ref mut vec) = struct_val {
                    if *member_offset >= vec.len() {
                        vec.resize(*member_offset + 1, Value::None);
                    }
                    vec[*member_offset] = val;
                    self.write_lvalue(struct_target, struct_val, _frame, context)
                } else {
                    Err(VMError::TypeMismatch {
                        expected: "Struct",
                        got: struct_val.type_name(),
                    })
                }
            }
        }
    }

    /// Collects arguments for a function call until EX_EndFunctionParms (0x15).
    fn collect_function_args(
        &mut self,
        frame: &mut FFrame,
        context: &mut dyn VMContext,
    ) -> Result<Vec<Value>, VMError> {
        let mut args = Vec::new();
        while !frame.is_eof() {
            let next_byte = frame.peek_u8();
            if next_byte == Opcode::EndFunctionParms as u8 {
                let _ = frame.read_u8(); // consume delimiter
                break;
            }
            args.push(self.evaluate_rvalue(frame, context)?);
        }
        Ok(args)
    }

    /// Handles primitive type casts (`EX_PrimitiveCast`).
    fn evaluate_primitive_cast(&self, cast_token: u8, val: Value) -> Result<Value, VMError> {
        match cast_token {
            0x38 /* IntToFloat */ => Ok(Value::Float(val.as_int() as f32)),
            0x39 /* FloatToInt */ => Ok(Value::Int(val.as_float() as i32)),
            0x3A /* ByteToInt */ => Ok(Value::Int(val.as_int())),
            0x3B /* IntToByte */ => Ok(Value::Byte(val.as_int() as u8)),
            0x3C /* BoolToByte */ => Ok(Value::Byte(if val.as_bool() { 1 } else { 0 })),
            0x3D /* ByteToBool */ => Ok(Value::Bool(val.as_int() != 0)),
            0x3E /* IntToBool */ => Ok(Value::Bool(val.as_int() != 0)),
            0x3F /* BoolToInt */ => Ok(Value::Int(if val.as_bool() { 1 } else { 0 })),
            0x40 /* FloatToBool */ => Ok(Value::Bool(val.as_float() != 0.0)),
            0x41 /* BoolToFloat */ => Ok(Value::Float(if val.as_bool() { 1.0 } else { 0.0 })),
            0x42 /* IntToString */ => Ok(Value::Str(val.as_str())),
            0x43 /* FloatToString */ => Ok(Value::Str(val.as_str())),
            0x44 /* BoolToString */ => Ok(Value::Str(val.as_str())),
            0x45 /* VectorToString */ => Ok(Value::Str(val.as_str())),
            0x46 /* RotatorToString */ => Ok(Value::Str(val.as_str())),
            0x47 /* NameToString */ => Ok(Value::Str(val.as_str())),
            _ => Ok(val),
        }
    }
}
