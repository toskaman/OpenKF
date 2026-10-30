use crate::bitstream::BitWriter;
use crate::bunch::{ChannelType, FInBunch, FOutBunch};
use crate::error::NetworkResult;
use crate::replication::{ActorReplicationState, PropertySchema, ReplicationValue};
use crate::rpc::{RpcCall, RpcRegistry};
use std::collections::HashMap;

/// Unreal Engine 2.5 Network Message Types for Control Channel 0 (`NMT_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NetworkMessageType {
    Hello = 0,
    Upgrade = 1,
    Challenge = 2,
    Authenticate = 3,
    Login = 4,
    Welcome = 5,
    Failure = 6,
    Skip = 7,
    Ack = 8,
    DL = 9,
}

impl NetworkMessageType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Hello),
            1 => Some(Self::Upgrade),
            2 => Some(Self::Challenge),
            3 => Some(Self::Authenticate),
            4 => Some(Self::Login),
            5 => Some(Self::Welcome),
            6 => Some(Self::Failure),
            7 => Some(Self::Skip),
            8 => Some(Self::Ack),
            9 => Some(Self::DL),
            _ => None,
        }
    }
}

/// Events produced by Control Channel 0 during client-server handshake.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlEvent {
    HelloReceived { protocol_version: u32 },
    ChallengeReceived { challenge: u32 },
    LoginReceived { player_name: String, options: String },
    WelcomeReceived { map_name: String, player_id: u32 },
    FailureReceived { reason: String },
    UnknownMessage { msg_type: u8, text: String },
}

/// State of the Control Channel connection handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlState {
    Disconnected,
    SentHello,
    ReceivedChallenge,
    SentLogin,
    Connected,
    Failed,
}

/// Implementation of Control Channel 0 (`UControlChannel`).
#[derive(Debug)]
pub struct ControlChannel {
    pub state: ControlState,
    pub reliable_sequence: u32,
    pub protocol_version: u32,
}

impl Default for ControlChannel {
    fn default() -> Self {
        Self::new(1065) // Default retail Killing Floor v1065
    }
}

impl ControlChannel {
    pub fn new(protocol_version: u32) -> Self {
        Self {
            state: ControlState::Disconnected,
            reliable_sequence: 0,
            protocol_version,
        }
    }

    /// Generates an outgoing NMT_Hello bunch.
    pub fn create_hello_bunch(&mut self) -> FOutBunch {
        self.reliable_sequence = self.reliable_sequence.wrapping_add(1);
        let mut writer = BitWriter::new();
        writer.write_u8(NetworkMessageType::Hello as u8);
        writer.write_u32(self.protocol_version);

        self.state = ControlState::SentHello;
        FOutBunch::new(0, true)
            .with_open(ChannelType::Control)
            .with_sequence(self.reliable_sequence)
            .with_bit_writer(writer)
    }

    /// Generates an outgoing NMT_Login bunch.
    pub fn create_login_bunch(&mut self, player_name: &str, options: &str) -> FOutBunch {
        self.reliable_sequence = self.reliable_sequence.wrapping_add(1);
        let mut writer = BitWriter::new();
        writer.write_u8(NetworkMessageType::Login as u8);
        writer.write_string(player_name);
        writer.write_string(options);

        self.state = ControlState::SentLogin;
        FOutBunch::new(0, true)
            .with_sequence(self.reliable_sequence)
            .with_bit_writer(writer)
    }

    /// Processes an incoming bunch destined for Control Channel 0.
    pub fn process_bunch(&mut self, bunch: &FInBunch) -> NetworkResult<Option<ControlEvent>> {
        let mut reader = bunch.payload_reader();
        if reader.remaining_bits() < 8 {
            return Ok(None);
        }

        let msg_byte = reader.read_u8()?;
        let msg_type = NetworkMessageType::from_u8(msg_byte);

        match msg_type {
            Some(NetworkMessageType::Hello) => {
                let ver = reader.read_u32().unwrap_or(0);
                Ok(Some(ControlEvent::HelloReceived { protocol_version: ver }))
            }
            Some(NetworkMessageType::Challenge) => {
                let challenge = reader.read_u32().unwrap_or(0);
                self.state = ControlState::ReceivedChallenge;
                Ok(Some(ControlEvent::ChallengeReceived { challenge }))
            }
            Some(NetworkMessageType::Login) => {
                let player_name = reader.read_string().unwrap_or_default();
                let options = reader.read_string().unwrap_or_default();
                Ok(Some(ControlEvent::LoginReceived { player_name, options }))
            }
            Some(NetworkMessageType::Welcome) => {
                let map_name = reader.read_string().unwrap_or_default();
                let player_id = reader.read_u32().unwrap_or(0);
                self.state = ControlState::Connected;
                Ok(Some(ControlEvent::WelcomeReceived { map_name, player_id }))
            }
            Some(NetworkMessageType::Failure) => {
                let reason = reader.read_string().unwrap_or_else(|_| "Unknown failure".into());
                self.state = ControlState::Failed;
                Ok(Some(ControlEvent::FailureReceived { reason }))
            }
            _ => {
                let text = reader.read_string().unwrap_or_default();
                Ok(Some(ControlEvent::UnknownMessage { msg_type: msg_byte, text }))
            }
        }
    }
}

/// Events occurring on an individual Actor Channel.
#[derive(Debug, Clone, PartialEq)]
pub enum ActorChannelEvent {
    ActorSpawned {
        channel_id: u32,
        actor_id: u32,
        class_name: String,
    },
    ActorDestroyed {
        channel_id: u32,
        actor_id: u32,
    },
    PropertiesUpdated {
        channel_id: u32,
        actor_id: u32,
        properties: Vec<(u32, ReplicationValue)>,
    },
    RpcExecuted {
        channel_id: u32,
        actor_id: u32,
        rpc: RpcCall,
    },
}

/// Partial bunch reassembly buffer for handling fragmented payloads.
#[derive(Debug, Default)]
struct PartialReassembler {
    bits: Vec<bool>,
}

impl PartialReassembler {
    fn reset(&mut self) {
        self.bits.clear();
    }

    fn append_bunch(&mut self, bunch: &FInBunch) -> NetworkResult<()> {
        let mut reader = bunch.payload_reader();
        for _ in 0..bunch.bit_count {
            self.bits.push(reader.read_bit()?);
        }
        Ok(())
    }

    fn to_complete_bunch(&self, template: &FInBunch) -> FInBunch {
        let mut writer = BitWriter::new();
        for &bit in &self.bits {
            writer.write_bit(bit);
        }
        let bit_count = writer.num_bits();
        let payload = writer.into_bytes();

        FInBunch {
            channel_index: template.channel_index,
            channel_type: template.channel_type,
            ch_sequence: template.ch_sequence,
            b_open: template.b_open,
            b_close: template.b_close,
            b_reliable: template.b_reliable,
            b_partial: false,
            b_partial_initial: false,
            b_partial_final: false,
            payload,
            bit_count,
        }
    }
}

/// Actor Channel instance representing an active actor replication pipeline (`UActorChannel`).
#[derive(Debug)]
pub struct ActorChannel {
    pub channel_id: u32,
    pub actor_id: Option<u32>,
    pub actor_class: Option<String>,
    pub is_open: bool,
    pub reliable_sequence: u32,
    pub replication_state: ActorReplicationState,
    reassembler: PartialReassembler,
}

impl ActorChannel {
    pub fn new(channel_id: u32) -> Self {
        Self {
            channel_id,
            actor_id: None,
            actor_class: None,
            is_open: false,
            reliable_sequence: 0,
            replication_state: ActorReplicationState::new(),
            reassembler: PartialReassembler::default(),
        }
    }

    /// Processes an incoming bunch for this actor channel, reassembling partial bunches if fragmented,
    /// and dispatching actor lifecycle, property replication, or RPC execution.
    pub fn process_bunch(
        &mut self,
        bunch: &FInBunch,
        schema: Option<&PropertySchema>,
        rpc_registry: Option<&RpcRegistry>,
    ) -> NetworkResult<Option<ActorChannelEvent>> {
        // Handle bunch fragmentation
        if bunch.b_partial {
            if bunch.b_partial_initial {
                self.reassembler.reset();
            }
            self.reassembler.append_bunch(bunch)?;

            if bunch.b_partial_final {
                let reassembled = self.reassembler.to_complete_bunch(bunch);
                self.reassembler.reset();
                return self.process_reassembled_bunch(&reassembled, schema, rpc_registry);
            } else {
                // Waiting for remaining fragments
                return Ok(None);
            }
        }

        self.process_reassembled_bunch(bunch, schema, rpc_registry)
    }

    fn process_reassembled_bunch(
        &mut self,
        bunch: &FInBunch,
        schema: Option<&PropertySchema>,
        rpc_registry: Option<&RpcRegistry>,
    ) -> NetworkResult<Option<ActorChannelEvent>> {
        // Check for channel opening / Actor spawn
        if bunch.b_open {
            self.is_open = true;
            let mut reader = bunch.payload_reader();
            let actor_id = if reader.remaining_bits() >= 32 {
                reader.read_u32()?
            } else {
                self.channel_id
            };
            let class_name = if reader.remaining_bits() > 0 {
                reader.read_string().unwrap_or_else(|_| "Actor".into())
            } else {
                "Actor".into()
            };

            self.actor_id = Some(actor_id);
            self.actor_class = Some(class_name.clone());

            return Ok(Some(ActorChannelEvent::ActorSpawned {
                channel_id: self.channel_id,
                actor_id,
                class_name,
            }));
        }

        // Check for channel closing / Actor destruction
        if bunch.b_close {
            self.is_open = false;
            let actor_id = self.actor_id.unwrap_or(self.channel_id);
            return Ok(Some(ActorChannelEvent::ActorDestroyed {
                channel_id: self.channel_id,
                actor_id,
            }));
        }

        // Payload processing: Replicated Properties vs RPC
        let mut reader = bunch.payload_reader();
        if reader.remaining_bits() == 0 {
            return Ok(None);
        }

        let actor_id = self.actor_id.unwrap_or(self.channel_id);

        // 1-bit discriminator: false = property replication, true = RPC event
        let is_rpc = reader.read_bit()?;
        if is_rpc {
            if let Some(registry) = rpc_registry {
                let rpc = RpcCall::read_from(&mut reader, registry)?;
                return Ok(Some(ActorChannelEvent::RpcExecuted {
                    channel_id: self.channel_id,
                    actor_id,
                    rpc,
                }));
            }
        } else if let Some(sch) = schema {
            let updated = self.replication_state.deserialize_and_apply(sch, &mut reader)?;
            return Ok(Some(ActorChannelEvent::PropertiesUpdated {
                channel_id: self.channel_id,
                actor_id,
                properties: updated,
            }));
        }

        Ok(None)
    }

    /// Queues an RPC call into an outgoing bunch.
    pub fn create_rpc_bunch(&mut self, rpc: &RpcCall) -> FOutBunch {
        if rpc.is_reliable {
            self.reliable_sequence = self.reliable_sequence.wrapping_add(1);
        }
        let mut writer = BitWriter::new();
        // 1-bit discriminator: true = RPC
        writer.write_bit(true);
        rpc.write_to(&mut writer);

        FOutBunch::new(self.channel_id, rpc.is_reliable)
            .with_sequence(self.reliable_sequence)
            .with_bit_writer(writer)
    }

    /// Queues dirty property replication into an outgoing bunch.
    pub fn create_replication_bunch(
        &mut self,
        schema: &PropertySchema,
    ) -> NetworkResult<Option<FOutBunch>> {
        let mut writer = BitWriter::new();
        // 1-bit discriminator: false = Property Replication
        writer.write_bit(false);

        let dirty_count = self.replication_state.diff_and_serialize(schema, &mut writer)?;
        if dirty_count == 0 {
            return Ok(None);
        }

        Ok(Some(
            FOutBunch::new(self.channel_id, false).with_bit_writer(writer),
        ))
    }
}

/// Demultiplexer managing active channels across the network connection.
#[derive(Debug)]
pub struct ChannelDemuxer {
    pub control_channel: ControlChannel,
    pub actor_channels: HashMap<u32, ActorChannel>,
}

impl Default for ChannelDemuxer {
    fn default() -> Self {
        Self::new(1065)
    }
}

impl ChannelDemuxer {
    pub fn new(protocol_version: u32) -> Self {
        Self {
            control_channel: ControlChannel::new(protocol_version),
            actor_channels: HashMap::new(),
        }
    }

    /// Dispatches an incoming bunch to the appropriate channel.
    pub fn demux_bunch(
        &mut self,
        bunch: FInBunch,
        schema_registry: &HashMap<String, PropertySchema>,
        rpc_registry: &RpcRegistry,
    ) -> NetworkResult<Option<DemuxEvent>> {
        if bunch.channel_index == 0 {
            let ctrl_event = self.control_channel.process_bunch(&bunch)?;
            Ok(ctrl_event.map(DemuxEvent::Control))
        } else {
            let ch_idx = bunch.channel_index;
            let channel = self
                .actor_channels
                .entry(ch_idx)
                .or_insert_with(|| ActorChannel::new(ch_idx));

            let schema = channel
                .actor_class
                .as_ref()
                .and_then(|cls| schema_registry.get(cls));

            let actor_event = channel.process_bunch(&bunch, schema, Some(rpc_registry))?;

            if bunch.b_close {
                self.actor_channels.remove(&ch_idx);
            }

            Ok(actor_event.map(DemuxEvent::Actor))
        }
    }
}

/// Unified high-level event returned by the channel demultiplexer.
#[derive(Debug, Clone, PartialEq)]
pub enum DemuxEvent {
    Control(ControlEvent),
    Actor(ActorChannelEvent),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replication::PropertyType;
    use crate::rpc::RpcParam;

    #[test]
    fn test_control_channel_flow() {
        let mut ctrl = ControlChannel::new(1065);
        let hello_bunch = ctrl.create_hello_bunch();
        assert_eq!(hello_bunch.channel_index, 0);
        assert_eq!(hello_bunch.channel_type, Some(ChannelType::Control));
        assert_eq!(ctrl.state, ControlState::SentHello);

        // Server answers with Challenge
        let mut challenge_writer = BitWriter::new();
        challenge_writer.write_u8(NetworkMessageType::Challenge as u8);
        challenge_writer.write_u32(0x99887766);
        let in_bunch = FInBunch {
            channel_index: 0,
            channel_type: None,
            ch_sequence: 1,
            b_open: false,
            b_close: false,
            b_reliable: true,
            b_partial: false,
            b_partial_initial: false,
            b_partial_final: false,
            bit_count: challenge_writer.num_bits(),
            payload: challenge_writer.into_bytes(),
        };

        let event = ctrl.process_bunch(&in_bunch).unwrap();
        assert_eq!(
            event,
            Some(ControlEvent::ChallengeReceived {
                challenge: 0x99887766
            })
        );
        assert_eq!(ctrl.state, ControlState::ReceivedChallenge);
    }

    #[test]
    fn test_actor_channel_partial_reassembly() {
        let mut channel = ActorChannel::new(10);
        let large_data = vec![0xAB; 256];

        // Fragment 1
        let mut b1_writer = BitWriter::new();
        b1_writer.write_bytes(&large_data[..128]);
        let b1 = FInBunch {
            channel_index: 10,
            channel_type: None,
            ch_sequence: 1,
            b_open: false,
            b_close: false,
            b_reliable: true,
            b_partial: true,
            b_partial_initial: true,
            b_partial_final: false,
            bit_count: b1_writer.num_bits(),
            payload: b1_writer.into_bytes(),
        };

        let ev1 = channel.process_bunch(&b1, None, None).unwrap();
        assert_eq!(ev1, None); // Incomplete

        // Fragment 2
        let mut b2_writer = BitWriter::new();
        b2_writer.write_bytes(&large_data[128..]);
        let b2 = FInBunch {
            channel_index: 10,
            channel_type: None,
            ch_sequence: 2,
            b_open: false,
            b_close: false,
            b_reliable: true,
            b_partial: true,
            b_partial_initial: false,
            b_partial_final: true,
            bit_count: b2_writer.num_bits(),
            payload: b2_writer.into_bytes(),
        };

        let _ = channel.process_bunch(&b2, None, None).unwrap();
        assert_eq!(channel.reassembler.bits.len(), 0); // Reset after complete
    }

    #[test]
    fn test_actor_channel_rpc_dispatch() {
        let mut channel = ActorChannel::new(4);
        channel.actor_id = Some(42);

        let mut registry = RpcRegistry::new();
        registry.register_function(10, "ServerFire", vec![PropertyType::Int], true);

        let rpc = RpcCall::new(10, true).with_param(RpcParam::Int(1));
        let out_bunch = channel.create_rpc_bunch(&rpc);

        let in_bunch = FInBunch {
            channel_index: 4,
            channel_type: None,
            ch_sequence: 1,
            b_open: false,
            b_close: false,
            b_reliable: true,
            b_partial: false,
            b_partial_initial: false,
            b_partial_final: false,
            bit_count: out_bunch.bit_count,
            payload: out_bunch.payload,
        };

        let event = channel
            .process_bunch(&in_bunch, None, Some(&registry))
            .unwrap();
        match event {
            Some(ActorChannelEvent::RpcExecuted { channel_id, actor_id, rpc }) => {
                assert_eq!(channel_id, 4);
                assert_eq!(actor_id, 42);
                assert_eq!(rpc.function_index, 10);
            }
            _ => panic!("Expected RpcExecuted event"),
        }
    }
}
