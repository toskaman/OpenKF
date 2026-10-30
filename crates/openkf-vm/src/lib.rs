//! # OpenKF Bytecode Virtual Machine (`openkf-vm`)
//!
//! 64-bit clean-room reimplementation of the UnrealScript Virtual Machine,
//! expression evaluator, and native dispatch tables for *Killing Floor* (v1065 / UE2.5).

pub mod context;
pub mod frame;
pub mod interpreter;
pub mod natives;
pub mod opcodes;
pub mod types;

pub use context::{MockObject, MockVMContext, VMContext};
pub use frame::FFrame;
pub use interpreter::VirtualMachine;
pub use natives::NativeTable;
pub use opcodes::Opcode;
pub use types::{CastToken, FName, FRotator, FVector, LValue, VMError, Value};
