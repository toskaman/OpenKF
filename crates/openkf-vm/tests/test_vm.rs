use openkf_vm::*;

#[test]
fn test_64bit_frame_memory_layout() {
    assert_eq!(std::mem::size_of::<FFrame>(), 64); // 48 bytes C header + 16 bytes slice metadata
    assert_eq!(std::mem::align_of::<FFrame>(), 8);

    // Verify 64-bit pointer offsets
    assert_eq!(std::mem::offset_of!(FFrame, vtable), 0x00);
    assert_eq!(std::mem::offset_of!(FFrame, node), 0x08);
    assert_eq!(std::mem::offset_of!(FFrame, object), 0x10);
    assert_eq!(std::mem::offset_of!(FFrame, code), 0x18);
    assert_eq!(std::mem::offset_of!(FFrame, locals), 0x20);
    assert_eq!(std::mem::offset_of!(FFrame, previous_frame), 0x28);
}

#[test]
fn test_constants_and_literals() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();

    // Bytecode: [EX_IntConst, 42, 0, 0, 0, EX_EndOfScript]
    let code: Vec<u8> = vec![
        Opcode::IntConst as u8,
        42, 0, 0, 0,
        Opcode::EndOfScript as u8,
    ];

    let mut frame = FFrame::new(1, 1, &code, std::ptr::null_mut(), std::ptr::null_mut());
    let val = vm.step(&mut frame, &mut ctx).unwrap();
    assert_eq!(val, Value::Int(42));

    // Float constant 3.14159
    let f_bytes = 3.14159f32.to_le_bytes();
    let code_f = vec![
        Opcode::FloatConst as u8,
        f_bytes[0], f_bytes[1], f_bytes[2], f_bytes[3],
        Opcode::EndOfScript as u8,
    ];
    let mut frame_f = FFrame::new(1, 1, &code_f, std::ptr::null_mut(), std::ptr::null_mut());
    let val_f = vm.step(&mut frame_f, &mut ctx).unwrap();
    assert!((val_f.as_float() - 3.14159).abs() < 1e-5);

    // Fast literals: IntZero, IntOne, True, False, NoObject
    let code_fast = vec![
        Opcode::IntZero as u8,
        Opcode::IntOne as u8,
        Opcode::True as u8,
        Opcode::False as u8,
        Opcode::NoObject as u8,
    ];
    let mut frame_fast = FFrame::new(1, 1, &code_fast, std::ptr::null_mut(), std::ptr::null_mut());
    assert_eq!(vm.step(&mut frame_fast, &mut ctx).unwrap(), Value::Int(0));
    assert_eq!(vm.step(&mut frame_fast, &mut ctx).unwrap(), Value::Int(1));
    assert_eq!(vm.step(&mut frame_fast, &mut ctx).unwrap(), Value::Bool(true));
    assert_eq!(vm.step(&mut frame_fast, &mut ctx).unwrap(), Value::Bool(false));
    assert_eq!(vm.step(&mut frame_fast, &mut ctx).unwrap(), Value::Object(None));
}

#[test]
fn test_string_literals_ansi_and_unicode() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();

    // ANSI String: "KillingFloor\0"
    let mut code_str = vec![Opcode::StringConst as u8];
    code_str.extend_from_slice(b"KillingFloor\0");
    let mut frame_s = FFrame::new(1, 1, &code_str, std::ptr::null_mut(), std::ptr::null_mut());
    let val_s = vm.step(&mut frame_s, &mut ctx).unwrap();
    assert_eq!(val_s, Value::Str("KillingFloor".to_string()));

    // Unicode String: UTF-16LE "Zed\0"
    let mut code_uni = vec![Opcode::UnicodeStringConst as u8];
    for &ch in &[b'Z' as u16, b'e' as u16, b'd' as u16, 0u16] {
        code_uni.extend_from_slice(&ch.to_le_bytes());
    }
    let mut frame_u = FFrame::new(1, 1, &code_uni, std::ptr::null_mut(), std::ptr::null_mut());
    let val_u = vm.step(&mut frame_u, &mut ctx).unwrap();
    assert_eq!(val_u, Value::Str("Zed".to_string()));
}

#[test]
fn test_vector_and_rotator_constants() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();

    // Vector constant: (100.0, 200.0, -50.0)
    let mut code_v = vec![Opcode::VectorConst as u8];
    code_v.extend_from_slice(&100.0f32.to_le_bytes());
    code_v.extend_from_slice(&200.0f32.to_le_bytes());
    code_v.extend_from_slice(&(-50.0f32).to_le_bytes());

    let mut frame_v = FFrame::new(1, 1, &code_v, std::ptr::null_mut(), std::ptr::null_mut());
    let val_v = vm.step(&mut frame_v, &mut ctx).unwrap();
    assert_eq!(val_v, Value::Vector(FVector::new(100.0, 200.0, -50.0)));

    // Rotator constant: (16384, 32768, 0)
    let mut code_r = vec![Opcode::RotatorConst as u8];
    code_r.extend_from_slice(&16384i32.to_le_bytes());
    code_r.extend_from_slice(&32768i32.to_le_bytes());
    code_r.extend_from_slice(&0i32.to_le_bytes());

    let mut frame_r = FFrame::new(1, 1, &code_r, std::ptr::null_mut(), std::ptr::null_mut());
    let val_r = vm.step(&mut frame_r, &mut ctx).unwrap();
    assert_eq!(val_r, Value::Rotator(FRotator::new(16384, 32768, 0)));
}

#[test]
fn test_variable_assignment_and_let() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();

    // Script:
    // let local[0] = 500
    // return local[0]
    let code = vec![
        Opcode::Let as u8,
        Opcode::LocalVariable as u8, 0, // compact index 0
        Opcode::IntConst as u8, 244, 1, 0, 0, // 500
        Opcode::Return as u8,
        Opcode::LocalVariable as u8, 0,
        Opcode::EndOfScript as u8,
    ];

    let result = vm
        .execute_function(100, 1, &code, &[], 4, &mut ctx)
        .unwrap();
    assert_eq!(result, Value::Int(500));
}

#[test]
fn test_instance_variable_read_and_write() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();
    ctx.set_property(1, 0x20, Value::Int(1337)).unwrap();

    // Read instance property at offset 0x20, assign to local[1]
    let code = vec![
        Opcode::Let as u8,
        Opcode::LocalVariable as u8, 1,
        Opcode::InstanceVariable as u8, 0x20,
        Opcode::Return as u8,
        Opcode::LocalVariable as u8, 1,
        Opcode::EndOfScript as u8,
    ];

    let result = vm
        .execute_function(100, 1, &code, &[], 4, &mut ctx)
        .unwrap();
    assert_eq!(result, Value::Int(1337));
}

#[test]
fn test_branching_and_loops() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();

    // UnrealScript loop equivalent:
    // int sum = 0;
    // int i = 5;
    // while (i > 0) {
    //     sum += i;
    //     i--;
    // }
    // return sum;

    let mut code = Vec::new();

    // 0: sum = 0
    code.push(Opcode::Let as u8);
    code.push(Opcode::LocalVariable as u8); code.push(0);
    code.push(Opcode::IntZero as u8);

    // 4: i = 5
    code.push(Opcode::Let as u8);
    code.push(Opcode::LocalVariable as u8); code.push(1);
    code.push(Opcode::IntConstByte as u8); code.push(5);

    // 9: Loop condition start
    let loop_head = code.len() as u16;
    code.push(Opcode::JumpIfNot as u8);
    let jump_if_not_offset_pos = code.len();
    code.push(0); code.push(0); // placeholder for skip target

    // Condition: i > 0 (Native 150)
    code.push(Opcode::Native as u8);
    code.extend_from_slice(&150u16.to_le_bytes()); // Greater_IntInt
    code.push(Opcode::LocalVariable as u8); code.push(1);
    code.push(Opcode::IntZero as u8);
    code.push(Opcode::EndFunctionParms as u8);

    // Loop body: sum = sum + i
    code.push(Opcode::Let as u8);
    code.push(Opcode::LocalVariable as u8); code.push(0);
    code.push(Opcode::Native as u8);
    code.extend_from_slice(&144u16.to_le_bytes()); // Add_IntInt
    code.push(Opcode::LocalVariable as u8); code.push(0);
    code.push(Opcode::LocalVariable as u8); code.push(1);
    code.push(Opcode::EndFunctionParms as u8);

    // Loop body: i = i - 1
    code.push(Opcode::Let as u8);
    code.push(Opcode::LocalVariable as u8); code.push(1);
    code.push(Opcode::Native as u8);
    code.extend_from_slice(&145u16.to_le_bytes()); // Subtract_IntInt
    code.push(Opcode::LocalVariable as u8); code.push(1);
    code.push(Opcode::IntOne as u8);
    code.push(Opcode::EndFunctionParms as u8);

    // Jump back to loop head
    code.push(Opcode::Jump as u8);
    code.extend_from_slice(&loop_head.to_le_bytes());

    // Loop exit: return sum
    let loop_exit = code.len() as u16;
    code[jump_if_not_offset_pos..jump_if_not_offset_pos + 2]
        .copy_from_slice(&loop_exit.to_le_bytes());

    code.push(Opcode::Return as u8);
    code.push(Opcode::LocalVariable as u8); code.push(0);
    code.push(Opcode::EndOfScript as u8);

    let result = vm
        .execute_function(200, 1, &code, &[], 2, &mut ctx)
        .unwrap();
    // 5 + 4 + 3 + 2 + 1 = 15
    assert_eq!(result, Value::Int(15));
}

#[test]
fn test_high_native_vector_math() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();

    // 0x70: VSize of (3.0, 4.0, 0.0) -> 5.0
    let mut code_vsize = vec![0x70]; // VSize opcode
    code_vsize.push(Opcode::VectorConst as u8);
    code_vsize.extend_from_slice(&3.0f32.to_le_bytes());
    code_vsize.extend_from_slice(&4.0f32.to_le_bytes());
    code_vsize.extend_from_slice(&0.0f32.to_le_bytes());

    let mut frame = FFrame::new(1, 1, &code_vsize, std::ptr::null_mut(), std::ptr::null_mut());
    let res = vm.step(&mut frame, &mut ctx).unwrap();
    assert!((res.as_float() - 5.0).abs() < 1e-5);

    // 0x72: Dot product: (1, 0, 0) . (0, 1, 0) -> 0.0
    let mut code_dot = vec![0x72]; // Dot opcode
    code_dot.push(Opcode::VectorConst as u8);
    code_dot.extend_from_slice(&1.0f32.to_le_bytes());
    code_dot.extend_from_slice(&0.0f32.to_le_bytes());
    code_dot.extend_from_slice(&0.0f32.to_le_bytes());
    code_dot.push(Opcode::VectorConst as u8);
    code_dot.extend_from_slice(&0.0f32.to_le_bytes());
    code_dot.extend_from_slice(&1.0f32.to_le_bytes());
    code_dot.extend_from_slice(&0.0f32.to_le_bytes());

    let mut frame_dot = FFrame::new(1, 1, &code_dot, std::ptr::null_mut(), std::ptr::null_mut());
    let res_dot = vm.step(&mut frame_dot, &mut ctx).unwrap();
    assert_eq!(res_dot.as_float(), 0.0);

    // 0x74: Add_VectorVector: (10, 20, 30) + (1, 2, 3) = (11, 22, 33)
    let mut code_add = vec![0x74];
    code_add.push(Opcode::VectorConst as u8);
    code_add.extend_from_slice(&10.0f32.to_le_bytes());
    code_add.extend_from_slice(&20.0f32.to_le_bytes());
    code_add.extend_from_slice(&30.0f32.to_le_bytes());
    code_add.push(Opcode::VectorConst as u8);
    code_add.extend_from_slice(&1.0f32.to_le_bytes());
    code_add.extend_from_slice(&2.0f32.to_le_bytes());
    code_add.extend_from_slice(&3.0f32.to_le_bytes());

    let mut frame_add = FFrame::new(1, 1, &code_add, std::ptr::null_mut(), std::ptr::null_mut());
    let res_add = vm.step(&mut frame_add, &mut ctx).unwrap();
    assert_eq!(res_add.as_vector(), FVector::new(11.0, 22.0, 33.0));
}

#[test]
fn test_high_native_rotator_math() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();

    // 0x83: Add_RotatorRotator (10000, 20000, 0) + (5000, 5000, 1000)
    let mut code = vec![0x83];
    code.push(Opcode::RotatorConst as u8);
    code.extend_from_slice(&10000i32.to_le_bytes());
    code.extend_from_slice(&20000i32.to_le_bytes());
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(Opcode::RotatorConst as u8);
    code.extend_from_slice(&5000i32.to_le_bytes());
    code.extend_from_slice(&5000i32.to_le_bytes());
    code.extend_from_slice(&1000i32.to_le_bytes());

    let mut frame = FFrame::new(1, 1, &code, std::ptr::null_mut(), std::ptr::null_mut());
    let res = vm.step(&mut frame, &mut ctx).unwrap();
    assert_eq!(res.as_rotator(), FRotator::new(15000, 25000, 1000));
}

#[test]
fn test_high_native_string_primitives() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();

    // 0x90: Len("Clot") -> 4
    let mut code_len = vec![0x90];
    code_len.push(Opcode::StringConst as u8);
    code_len.extend_from_slice(b"Clot\0");
    let mut frame_len = FFrame::new(1, 1, &code_len, std::ptr::null_mut(), std::ptr::null_mut());
    let res_len = vm.step(&mut frame_len, &mut ctx).unwrap();
    assert_eq!(res_len, Value::Int(4));

    // 0x98: Concat_StrStr ($) "Trip" $ "wire" -> "Tripwire"
    let mut code_cat = vec![0x98];
    code_cat.push(Opcode::StringConst as u8);
    code_cat.extend_from_slice(b"Trip\0");
    code_cat.push(Opcode::StringConst as u8);
    code_cat.extend_from_slice(b"wire\0");
    let mut frame_cat = FFrame::new(1, 1, &code_cat, std::ptr::null_mut(), std::ptr::null_mut());
    let res_cat = vm.step(&mut frame_cat, &mut ctx).unwrap();
    assert_eq!(res_cat, Value::Str("Tripwire".to_string()));

    // 0x95: Caps("scrake") -> "SCRAKE"
    let mut code_caps = vec![0x95];
    code_caps.push(Opcode::StringConst as u8);
    code_caps.extend_from_slice(b"scrake\0");
    let mut frame_caps = FFrame::new(1, 1, &code_caps, std::ptr::null_mut(), std::ptr::null_mut());
    let res_caps = vm.step(&mut frame_caps, &mut ctx).unwrap();
    assert_eq!(res_caps, Value::Str("SCRAKE".to_string()));
}

#[test]
fn test_dynamic_arrays_operations() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();

    // Script:
    // DynArrayAdd(local[0], 3)
    // local[0][1] = 999
    // return local[0][1]
    let code = vec![
        // DynArrayAdd(local[0], 3)
        Opcode::DynArrayAdd as u8,
        Opcode::LocalVariable as u8, 0,
        Opcode::IntConstByte as u8, 3,

        // Let local[0][1] = 999
        Opcode::Let as u8,
        Opcode::DynArrayElement as u8,
        Opcode::LocalVariable as u8, 0,
        Opcode::IntConstByte as u8, 1,
        Opcode::IntConst as u8, 0xE7, 0x03, 0x00, 0x00, // 999

        // Return local[0][1]
        Opcode::Return as u8,
        Opcode::DynArrayElement as u8,
        Opcode::LocalVariable as u8, 0,
        Opcode::IntConstByte as u8, 1,
        Opcode::EndOfScript as u8,
    ];

    let result = vm
        .execute_function(300, 1, &code, &[Value::Array(Vec::new())], 1, &mut ctx)
        .unwrap();
    assert_eq!(result, Value::Int(999));
}

#[test]
fn test_virtual_and_final_function_calls() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();

    // Virtual function call: ProcessEvent on target with argument 42
    let code_virt = vec![
        Opcode::VirtualFunction as u8,
        15, 0, // FName index 15, number 0
        Opcode::IntConst as u8, 42, 0, 0, 0,
        Opcode::EndFunctionParms as u8,
        Opcode::EndOfScript as u8,
    ];

    let mut frame_v = FFrame::new(1, 1, &code_virt, std::ptr::null_mut(), std::ptr::null_mut());
    let res_v = vm.step(&mut frame_v, &mut ctx).unwrap();
    assert_eq!(res_v, Value::Int(42));
    assert_eq!(ctx.virtual_calls.len(), 1);
    assert_eq!(ctx.virtual_calls[0].1, FName::new(15, 0));

    // Final function direct call: function ID 777 (CompactIndex: [0x49, 0x0C])
    let code_final = vec![
        Opcode::FinalFunction as u8,
        0x49, 0x0C, // CompactIndex for 777
        Opcode::IntConst as u8, 100, 0, 0, 0,
        Opcode::EndFunctionParms as u8,
        Opcode::EndOfScript as u8,
    ];
    let mut frame_f = FFrame::new(1, 1, &code_final, std::ptr::null_mut(), std::ptr::null_mut());
    let res_f = vm.step(&mut frame_f, &mut ctx).unwrap();
    assert_eq!(res_f, Value::Int(100));
    assert_eq!(ctx.final_calls.len(), 1);
    assert_eq!(ctx.final_calls[0].0, 777);
}

#[test]
fn test_primitive_casts() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();

    // IntToFloat (0x38): int 50 -> float 50.0
    let code_cast = vec![
        Opcode::PrimitiveCast as u8,
        0x38,
        Opcode::IntConstByte as u8, 50,
        Opcode::EndOfScript as u8,
    ];
    let mut frame = FFrame::new(1, 1, &code_cast, std::ptr::null_mut(), std::ptr::null_mut());
    let res = vm.step(&mut frame, &mut ctx).unwrap();
    assert_eq!(res, Value::Float(50.0));

    // FloatToInt (0x39): float 42.9 -> int 42
    let mut code_f2i = vec![Opcode::PrimitiveCast as u8, 0x39, Opcode::FloatConst as u8];
    code_f2i.extend_from_slice(&42.9f32.to_le_bytes());
    code_f2i.push(Opcode::EndOfScript as u8);
    let mut frame_f2i = FFrame::new(1, 1, &code_f2i, std::ptr::null_mut(), std::ptr::null_mut());
    let res_i = vm.step(&mut frame_f2i, &mut ctx).unwrap();
    assert_eq!(res_i, Value::Int(42));
}

#[test]
fn test_switch_case_evaluation() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new();

    // Switch on value 2:
    // case 1: return 10
    // case 2: return 20
    // default: return 0
    let code = vec![
        Opcode::Switch as u8,
        1, // size byte
        Opcode::IntConstByte as u8, 2, // switch value = 2

        // Case 1 (offset to next case is 15)
        Opcode::Case as u8,
        15, 0, // jump to next case at offset 15 if not matching
        Opcode::IntConstByte as u8, 1,
        Opcode::Return as u8,
        Opcode::IntConstByte as u8, 10,

        // Case 2 (at offset 15, offset to next case is 26)
        Opcode::Case as u8,
        26, 0,
        Opcode::IntConstByte as u8, 2,
        Opcode::Return as u8,
        Opcode::IntConstByte as u8, 20,

        // Default case (at offset 26, next offset is 0xFFFF)
        Opcode::Case as u8,
        0xFF, 0xFF,
        Opcode::Return as u8,
        Opcode::IntConstByte as u8, 0,
        Opcode::EndOfScript as u8,
    ];

    let res = vm
        .execute_function(400, 1, &code, &[], 0, &mut ctx)
        .unwrap();
    assert_eq!(res, Value::Int(20));
}

#[test]
fn test_context_remote_call() {
    let mut vm = VirtualMachine::new();
    let mut ctx = MockVMContext::new().with_object(42, 100);
    ctx.set_property(42, 0x10, Value::Int(9999)).unwrap();

    // Context execution: Target(42).InstanceVariable(0x10)
    let code = vec![
        Opcode::Context as u8,
        Opcode::ObjectConst as u8, 42,
        10, 0, // skip offset if null
        Opcode::InstanceVariable as u8, 0x10,
        Opcode::EndOfScript as u8,
    ];

    let mut frame = FFrame::new(1, 1, &code, std::ptr::null_mut(), std::ptr::null_mut());
    let res = vm.step(&mut frame, &mut ctx).unwrap();
    assert_eq!(res, Value::Int(9999));
}
