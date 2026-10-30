//! VM Context trait and testing mock environment for object memory, properties, and function dispatch.

use crate::types::{FName, VMError, Value};
use std::collections::HashMap;

/// Trait defining the external engine and world environment interface for the VM.
pub trait VMContext {
    /// Reads a property from an object instance at a given offset / index.
    fn get_property(&self, object_id: u64, offset: usize) -> Result<Value, VMError>;

    /// Writes a property into an object instance at a given offset / index.
    fn set_property(&mut self, object_id: u64, offset: usize, val: Value) -> Result<(), VMError>;

    /// Reads a property from a class default object.
    fn get_default_property(&self, class_id: u64, offset: usize) -> Result<Value, VMError>;

    /// Dispatches a virtual function call (`EX_VirtualFunction`).
    fn call_virtual(
        &mut self,
        object_id: u64,
        func_name: &FName,
        args: Vec<Value>,
    ) -> Result<Value, VMError>;

    /// Dispatches a non-virtual final function call (`EX_FinalFunction`).
    fn call_final(&mut self, func_id: u64, args: Vec<Value>) -> Result<Value, VMError>;

    /// Dispatches a non-state global version of a function (`EX_GlobalFunction`).
    fn call_global(
        &mut self,
        object_id: u64,
        func_name: &FName,
        args: Vec<Value>,
    ) -> Result<Value, VMError>;

    /// Instantiates a new UObject (`EX_New`).
    fn spawn_object(
        &mut self,
        class_id: u64,
        outer_id: Option<u64>,
        name: &FName,
        flags: u32,
    ) -> Result<u64, VMError>;

    /// Verifies if an object is an instance of the target class (`EX_DynamicCast`).
    fn dynamic_cast(&self, object_id: u64, target_class_id: u64) -> bool;

    /// Evaluates metaclass inheritance safety (`EX_MetaCast`).
    fn meta_cast(&self, class_id: u64, target_meta_class_id: u64) -> bool;

    /// Queries an iterator loop (`EX_Iterator`), returning a list of values to iterate over.
    fn query_iterator(&mut self, iter_func_id: u32, target_class_id: u64) -> Vec<Value>;
}

/// In-memory mock object representing an instance in tests.
#[derive(Debug, Clone, Default)]
pub struct MockObject {
    pub class_id: u64,
    pub properties: HashMap<usize, Value>,
}

/// Standalone test implementation of `VMContext`.
pub struct MockVMContext {
    pub objects: HashMap<u64, MockObject>,
    pub default_properties: HashMap<(u64, usize), Value>,
    pub next_object_id: u64,
    pub virtual_calls: Vec<(u64, FName, Vec<Value>)>,
    pub final_calls: Vec<(u64, Vec<Value>)>,
    pub iterator_items: HashMap<u32, Vec<Value>>,
}

impl Default for MockVMContext {
    fn default() -> Self {
        let mut ctx = Self {
            objects: HashMap::new(),
            default_properties: HashMap::new(),
            next_object_id: 1000,
            virtual_calls: Vec::new(),
            final_calls: Vec::new(),
            iterator_items: HashMap::new(),
        };
        // Register default root object 1
        ctx.objects.insert(1, MockObject::default());
        ctx
    }
}

impl MockVMContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_object(mut self, id: u64, class_id: u64) -> Self {
        self.objects.insert(
            id,
            MockObject {
                class_id,
                properties: HashMap::new(),
            },
        );
        self
    }
}

impl VMContext for MockVMContext {
    fn get_property(&self, object_id: u64, offset: usize) -> Result<Value, VMError> {
        if let Some(obj) = self.objects.get(&object_id) {
            Ok(obj.properties.get(&offset).cloned().unwrap_or(Value::None))
        } else {
            Err(VMError::NullReference)
        }
    }

    fn set_property(&mut self, object_id: u64, offset: usize, val: Value) -> Result<(), VMError> {
        if let Some(obj) = self.objects.get_mut(&object_id) {
            obj.properties.insert(offset, val);
            Ok(())
        } else {
            Err(VMError::NullReference)
        }
    }

    fn get_default_property(&self, class_id: u64, offset: usize) -> Result<Value, VMError> {
        Ok(self
            .default_properties
            .get(&(class_id, offset))
            .cloned()
            .unwrap_or(Value::None))
    }

    fn call_virtual(
        &mut self,
        object_id: u64,
        func_name: &FName,
        args: Vec<Value>,
    ) -> Result<Value, VMError> {
        self.virtual_calls.push((object_id, *func_name, args.clone()));
        // Return first argument or None as default mock response
        Ok(args.into_iter().next().unwrap_or(Value::None))
    }

    fn call_final(&mut self, func_id: u64, args: Vec<Value>) -> Result<Value, VMError> {
        self.final_calls.push((func_id, args.clone()));
        Ok(args.into_iter().next().unwrap_or(Value::None))
    }

    fn call_global(
        &mut self,
        object_id: u64,
        func_name: &FName,
        args: Vec<Value>,
    ) -> Result<Value, VMError> {
        self.call_virtual(object_id, func_name, args)
    }

    fn spawn_object(
        &mut self,
        class_id: u64,
        _outer_id: Option<u64>,
        _name: &FName,
        _flags: u32,
    ) -> Result<u64, VMError> {
        let id = self.next_object_id;
        self.next_object_id += 1;
        self.objects.insert(
            id,
            MockObject {
                class_id,
                properties: HashMap::new(),
            },
        );
        Ok(id)
    }

    fn dynamic_cast(&self, object_id: u64, target_class_id: u64) -> bool {
        if let Some(obj) = self.objects.get(&object_id) {
            obj.class_id == target_class_id || target_class_id == 0
        } else {
            false
        }
    }

    fn meta_cast(&self, class_id: u64, target_meta_class_id: u64) -> bool {
        class_id == target_meta_class_id || target_meta_class_id == 0
    }

    fn query_iterator(&mut self, iter_func_id: u32, _target_class_id: u64) -> Vec<Value> {
        self.iterator_items
            .get(&iter_func_id)
            .cloned()
            .unwrap_or_default()
    }
}
