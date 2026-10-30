use crate::bitstream::{BitReader, BitWriter};
use crate::error::NetworkResult;
use glam::Vec3;
use std::collections::HashMap;

/// Supported replicated property data types in Unreal Engine 2.5 replication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyType {
    Bool,
    Byte,
    Int,
    Float,
    String,
    Vector,
    Rotator,
    ActorRef,
}

/// Dynamic value for a replicated actor property.
#[derive(Debug, Clone, PartialEq)]
pub enum ReplicationValue {
    Bool(bool),
    Byte(u8),
    Int(i32),
    Float(f32),
    String(String),
    Vector(Vec3),
    Rotator { pitch: i32, yaw: i32, roll: i32 },
    ActorRef(u32),
    Raw(Vec<u8>),
}

impl ReplicationValue {
    /// Serializes property value into the bitstream.
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
            Self::Raw(bytes) => {
                writer.write_compact_u32(bytes.len() as u32);
                writer.write_bytes(bytes);
            }
        }
    }

    /// Deserializes property value according to expected property type.
    pub fn read_from(prop_type: PropertyType, reader: &mut BitReader) -> NetworkResult<Self> {
        match prop_type {
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

/// Replicated property definition within an Actor class schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyDef {
    pub index: u32,
    pub name: String,
    pub prop_type: PropertyType,
}

/// Class replication schema defining the layout and order of replicable fields.
#[derive(Debug, Clone, Default)]
pub struct PropertySchema {
    pub class_name: String,
    pub properties: Vec<PropertyDef>,
    name_to_index: HashMap<String, u32>,
}

impl PropertySchema {
    pub fn new(class_name: impl Into<String>) -> Self {
        Self {
            class_name: class_name.into(),
            properties: Vec::new(),
            name_to_index: HashMap::new(),
        }
    }

    /// Registers a property in this class replication schema.
    pub fn add_property(&mut self, name: impl Into<String>, prop_type: PropertyType) -> u32 {
        let name_str = name.into();
        let index = self.properties.len() as u32;
        self.name_to_index.insert(name_str.clone(), index);
        self.properties.push(PropertyDef {
            index,
            name: name_str,
            prop_type,
        });
        index
    }

    pub fn property_by_index(&self, index: u32) -> Option<&PropertyDef> {
        self.properties.get(index as usize)
    }

    pub fn property_index_by_name(&self, name: &str) -> Option<u32> {
        self.name_to_index.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.properties.len()
    }

    pub fn is_empty(&self) -> bool {
        self.properties.is_empty()
    }
}

/// Bitmask indicating which properties are dirty and included in the current replication bunch.
/// Disassembled from `Engine.dll:0xc93e0` (`GetOptimizedRepList`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DirtyPropertyMask {
    pub bits: Vec<bool>,
}

impl DirtyPropertyMask {
    /// Creates an empty mask for a schema of given size.
    pub fn new(size: usize) -> Self {
        Self {
            bits: vec![false; size],
        }
    }

    /// Sets property at index as dirty.
    pub fn mark_dirty(&mut self, index: usize) {
        if index >= self.bits.len() {
            self.bits.resize(index + 1, false);
        }
        self.bits[index] = true;
    }

    /// Checks if property at index is dirty.
    pub fn is_dirty(&self, index: usize) -> bool {
        self.bits.get(index).copied().unwrap_or(false)
    }

    /// Serializes the bitmask into the bitstream (1 bit per property in schema order).
    pub fn write_to(&self, writer: &mut BitWriter, count: usize) {
        for i in 0..count {
            let dirty = self.is_dirty(i);
            writer.write_bit(dirty);
        }
    }

    /// Deserializes the bitmask from the bitstream.
    pub fn read_from(reader: &mut BitReader, count: usize) -> NetworkResult<Self> {
        let mut bits = Vec::with_capacity(count);
        for _ in 0..count {
            let bit = reader.read_bit()?;
            bits.push(bit);
        }
        Ok(Self { bits })
    }
}

/// Actor replication state container tracking current values and the shadow buffer (`Recent[]`).
#[derive(Debug, Clone, Default)]
pub struct ActorReplicationState {
    pub current: HashMap<u32, ReplicationValue>,
    pub recent: HashMap<u32, ReplicationValue>,
}

impl ActorReplicationState {
    pub fn new() -> Self {
        Self {
            current: HashMap::new(),
            recent: HashMap::new(),
        }
    }

    /// Sets or updates a property value.
    pub fn set_property(&mut self, index: u32, value: ReplicationValue) {
        self.current.insert(index, value);
    }

    /// Diffs `current` against `recent` cache (Unreal's `ReplicateActor` comparison),
    /// serializes the dirty property bitmask and the serialized values into `BitWriter`.
    /// Updates `recent` shadow buffer upon completion.
    pub fn diff_and_serialize(
        &mut self,
        schema: &PropertySchema,
        writer: &mut BitWriter,
    ) -> NetworkResult<usize> {
        let mut mask = DirtyPropertyMask::new(schema.len());
        let mut dirty_count = 0;

        for (i, def) in schema.properties.iter().enumerate() {
            let current_val = self.current.get(&def.index);
            let recent_val = self.recent.get(&def.index);

            if current_val != recent_val && current_val.is_some() {
                mask.mark_dirty(i);
                dirty_count += 1;
            }
        }

        // Write the bitmask
        mask.write_to(writer, schema.len());

        // Write each dirty property value
        for (i, def) in schema.properties.iter().enumerate() {
            if mask.is_dirty(i) {
                if let Some(val) = self.current.get(&def.index) {
                    val.write_to(writer);
                    // Update shadow buffer
                    self.recent.insert(def.index, val.clone());
                }
            }
        }

        Ok(dirty_count)
    }

    /// Deserializes dirty properties from incoming `BitReader` using the schema,
    /// and updates `current` state. Returns a vector of changed `(property_index, value)` pairs.
    pub fn deserialize_and_apply(
        &mut self,
        schema: &PropertySchema,
        reader: &mut BitReader,
    ) -> NetworkResult<Vec<(u32, ReplicationValue)>> {
        let mask = DirtyPropertyMask::read_from(reader, schema.len())?;
        let mut updated = Vec::new();

        for (i, def) in schema.properties.iter().enumerate() {
            if mask.is_dirty(i) {
                let val = ReplicationValue::read_from(def.prop_type, reader)?;
                self.current.insert(def.index, val.clone());
                self.recent.insert(def.index, val.clone());
                updated.push((def.index, val));
            }
        }

        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_property_replication_diff_and_apply() {
        let mut schema = PropertySchema::new("KFPawn");
        let health_idx = schema.add_property("Health", PropertyType::Int);
        let pos_idx = schema.add_property("Location", PropertyType::Vector);
        let name_idx = schema.add_property("PlayerName", PropertyType::String);

        let mut server_state = ActorReplicationState::new();
        server_state.set_property(health_idx, ReplicationValue::Int(100));
        server_state.set_property(pos_idx, ReplicationValue::Vector(Vec3::new(10.0, 20.0, 30.0)));
        server_state.set_property(name_idx, ReplicationValue::String("Player1".into()));

        // Initial replication
        let mut writer = BitWriter::new();
        let dirties = server_state.diff_and_serialize(&schema, &mut writer).unwrap();
        assert_eq!(dirties, 3);

        // Client receives and applies
        let data = writer.into_bytes();
        let mut reader = BitReader::new(&data);
        let mut client_state = ActorReplicationState::new();
        let updated = client_state.deserialize_and_apply(&schema, &mut reader).unwrap();
        assert_eq!(updated.len(), 3);
        assert_eq!(client_state.current.get(&health_idx), Some(&ReplicationValue::Int(100)));

        // Server updates only Health
        server_state.set_property(health_idx, ReplicationValue::Int(75));
        let mut delta_writer = BitWriter::new();
        let delta_dirties = server_state.diff_and_serialize(&schema, &mut delta_writer).unwrap();
        assert_eq!(delta_dirties, 1);

        // Client applies delta
        let delta_data = delta_writer.into_bytes();
        let mut delta_reader = BitReader::new(&delta_data);
        let delta_updated = client_state.deserialize_and_apply(&schema, &mut delta_reader).unwrap();
        assert_eq!(delta_updated.len(), 1);
        assert_eq!(delta_updated[0].0, health_idx);
        assert_eq!(client_state.current.get(&health_idx), Some(&ReplicationValue::Int(75)));
    }
}
