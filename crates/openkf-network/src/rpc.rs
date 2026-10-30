use crate::bitstream::{BitReader, BitWriter};
use crate::bunch::FOutBunch;
use crate::error::{NetworkError, NetworkResult};
use crate::replication::PropertyType;
use glam::Vec3;
use std::collections::HashMap;

/// Parameter value passed in a Remote Procedure Call.
#[derive(Debug, Clone, PartialEq)]
pub enum RpcParam {
    Bool(bool),
    Byte(u8),
    Int(i32),
    Float(f32),
    String(String),
    Vector(Vec3),
    Rotator { pitch: i32, yaw: i32, roll: i32 },
    ActorRef(u32),
}

impl RpcParam {
    pub fn write_to(&self, writer: &mut BitWriter) {
        match self {
            Self::Bool(b) => writer.write_bit(*b),
            Self::Byte(b) => writer.write_u8(*b),
            Self::Int(i) => writer.write_i32(*i),
            Self::Float(f) => writer.write_f32(*f),
            Self::String(s) => writer.write_string(s),
            Self::Vector(v) => writer.write_vector(*v),
            Self::Rotator { pitch, yaw, roll } => writer.write_rotator(*pitch, *yaw, *roll),
            Self::ActorRef(id) => writer.write_compact_u32(*id),
        }
    }

    pub fn read_from(param_type: PropertyType, reader: &mut BitReader) -> NetworkResult<Self> {
        match param_type {
            PropertyType::Bool => Ok(Self::Bool(reader.read_bit()?)),
            PropertyType::Byte => Ok(Self::Byte(reader.read_u8()?)),
            PropertyType::Int => Ok(Self::Int(reader.read_i32()?)),
            PropertyType::Float => Ok(Self::Float(reader.read_f32()?)),
            PropertyType::String => Ok(Self::String(reader.read_string()?)),
            PropertyType::Vector => Ok(Self::Vector(reader.read_vector()?)),
            PropertyType::Rotator => {
                let (pitch, yaw, roll) = reader.read_rotator()?;
                Ok(Self::Rotator { pitch, yaw, roll })
            }
            PropertyType::ActorRef => Ok(Self::ActorRef(reader.read_compact_u32()?)),
        }
    }
}

/// Description of a replicated remote procedure call in UnrealScript (`Core.dll:0x272e0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcFunctionDef {
    pub function_index: u32,
    pub name: String,
    pub param_types: Vec<PropertyType>,
    pub is_reliable: bool,
}

/// Registry of known replicated RPC functions.
#[derive(Debug, Clone, Default)]
pub struct RpcRegistry {
    functions: HashMap<u32, RpcFunctionDef>,
    name_to_index: HashMap<String, u32>,
}

impl RpcRegistry {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            name_to_index: HashMap::new(),
        }
    }

    /// Registers a replicated function.
    pub fn register_function(
        &mut self,
        function_index: u32,
        name: impl Into<String>,
        param_types: Vec<PropertyType>,
        is_reliable: bool,
    ) {
        let name_str = name.into();
        self.name_to_index.insert(name_str.clone(), function_index);
        self.functions.insert(
            function_index,
            RpcFunctionDef {
                function_index,
                name: name_str,
                param_types,
                is_reliable,
            },
        );
    }

    pub fn get_by_index(&self, index: u32) -> Option<&RpcFunctionDef> {
        self.functions.get(&index)
    }

    pub fn get_by_name(&self, name: &str) -> Option<&RpcFunctionDef> {
        self.name_to_index.get(name).and_then(|idx| self.functions.get(idx))
    }
}

/// Replicated Remote Procedure Call (RPC) packet representation.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcCall {
    pub function_index: u32,
    pub function_name: Option<String>,
    pub parameters: Vec<RpcParam>,
    pub is_reliable: bool,
}

impl RpcCall {
    /// Creates a new RPC call.
    pub fn new(function_index: u32, is_reliable: bool) -> Self {
        Self {
            function_index,
            function_name: None,
            parameters: Vec::new(),
            is_reliable,
        }
    }

    /// Appends a parameter to the call.
    pub fn with_param(mut self, param: RpcParam) -> Self {
        self.parameters.push(param);
        self
    }

    /// Serializes this RPC call into a bitstream writer.
    pub fn write_to(&self, writer: &mut BitWriter) {
        // Function index serialized as compact integer
        writer.write_compact_u32(self.function_index);
        // Parameter payload
        for param in &self.parameters {
            param.write_to(writer);
        }
    }

    /// Deserializes an RPC call from a bitstream reader using its function definition.
    pub fn read_from(reader: &mut BitReader, registry: &RpcRegistry) -> NetworkResult<Self> {
        let function_index = reader.read_compact_u32()?;
        let def = registry.get_by_index(function_index).ok_or_else(|| {
            NetworkError::MalformedRpc(format!("Unknown RPC function index: {function_index}"))
        })?;

        let mut parameters = Vec::with_capacity(def.param_types.len());
        for &ptype in &def.param_types {
            let param = RpcParam::read_from(ptype, reader)?;
            parameters.push(param);
        }

        Ok(Self {
            function_index,
            function_name: Some(def.name.clone()),
            parameters,
            is_reliable: def.is_reliable,
        })
    }

    /// Converts this RPC call into an `FOutBunch` destined for an actor channel.
    pub fn to_bunch(&self, channel_id: u32) -> FOutBunch {
        let mut writer = BitWriter::new();
        self.write_to(&mut writer);
        FOutBunch::new(channel_id, self.is_reliable).with_bit_writer(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_serialization_roundtrip() {
        let mut registry = RpcRegistry::new();
        registry.register_function(
            101,
            "ServerBuyWeapon",
            vec![PropertyType::Int, PropertyType::String],
            true,
        );

        let call = RpcCall::new(101, true)
            .with_param(RpcParam::Int(1500))
            .with_param(RpcParam::String("KFMod.Shotgun".into()));

        let mut writer = BitWriter::new();
        call.write_to(&mut writer);

        let data = writer.into_bytes();
        let mut reader = BitReader::new(&data);
        let decoded = RpcCall::read_from(&mut reader, &registry).unwrap();

        assert_eq!(decoded.function_index, 101);
        assert_eq!(decoded.function_name.as_deref(), Some("ServerBuyWeapon"));
        assert_eq!(decoded.parameters.len(), 2);
        assert_eq!(decoded.parameters[0], RpcParam::Int(1500));
        assert_eq!(decoded.parameters[1], RpcParam::String("KFMod.Shotgun".into()));
    }
}
