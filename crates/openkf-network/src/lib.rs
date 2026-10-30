//! # OpenKF Network
//!
//! Retail Unreal Engine 2.5 multiplayer network protocol client and replication engine.
//!
//! Based on reverse engineering and clean-room protocol analysis of *Killing Floor* (v1065, `Engine.dll` / `Core.dll`).
//!
//! ## Architecture
//! - [`bitstream`]: Bit-level packet serialization and deserialization (`UFBitReader` / `UFBitWriter`).
//! - [`bunch`]: Multiplexed logical sub-packets (`FInBunch` / `FOutBunch`) across channels 0..1023.
//! - [`packet`]: Physical UDP datagram framing with monotonically increasing packet sequences and selective acks.
//! - [`channel`]: Logical channel management (Control Channel 0 for handshake, Actor Channels 1..1023 for replication and RPCs).
//! - [`replication`]: Dirty property bitmask parsing and shadow buffer (`Recent[]`) delta serialization.
//! - [`rpc`]: Remote Procedure Call serialization for UnrealScript `reliable server` functions.
//! - [`wire`]: **verified** UE2 datagram/bunch framing (checked against real KF captures).
//! - [`client`]: High-level `NetworkClient` orchestrating connection lifecycle, sockets, acks, and replication.

pub mod bitstream;
pub mod bunch;
pub mod channel;
pub mod client;
pub mod client_connection;
pub mod error;
pub mod packet;
pub mod replication;
pub mod rpc;
pub mod wire;
pub mod package_map;
pub mod actor_open;
pub mod schema;
pub mod props;
pub mod session;
pub mod handshake;
pub mod download;
pub mod live;
pub mod steam;

pub use bitstream::{BitReader, BitWriter, UFBitReader, UFBitWriter};
pub use bunch::{ChannelType, FInBunch, FOutBunch, MAX_CHANNELS};
pub use channel::{
    ActorChannel, ActorChannelEvent, ChannelDemuxer, ControlChannel, ControlEvent, ControlState,
    DemuxEvent, NetworkMessageType,
};
pub use client::NetworkClient;
pub use client_connection::{AsBunchRef, ClientConnectionState, ClientConnectionStateMachine};
pub use error::{NetworkError, NetworkResult};
pub use packet::{build_packet, parse_packet, AckTracker, PacketHeader};
pub use replication::{
    ActorReplicationState, DirtyPropertyMask, PropertyDef, PropertySchema, PropertyType,
    ReplicationValue,
};
pub use rpc::{RpcCall, RpcFunctionDef, RpcParam, RpcRegistry};

/// Common prelude for openkf-network consumers.
pub mod prelude {
    pub use crate::bitstream::{BitReader, BitWriter, UFBitReader, UFBitWriter};
    pub use crate::bunch::{ChannelType, FInBunch, FOutBunch};
    pub use crate::channel::{ActorChannelEvent, ControlEvent, DemuxEvent};
    pub use crate::client::NetworkClient;
    pub use crate::client_connection::{ClientConnectionState, ClientConnectionStateMachine};
    pub use crate::error::{NetworkError, NetworkResult};
    pub use crate::packet::{PacketHeader};
    pub use crate::replication::{
        ActorReplicationState, DirtyPropertyMask, PropertySchema, PropertyType, ReplicationValue,
    };
    pub use crate::rpc::{RpcCall, RpcParam, RpcRegistry};
}
