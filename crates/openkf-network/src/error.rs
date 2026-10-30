use thiserror::Error;

/// Result type for network operations.
pub type NetworkResult<T> = Result<T, NetworkError>;

/// Errors that can occur during network packet processing, bitstream serialization,
/// bunch demultiplexing, or property replication.
#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("Bitstream underflow: requested {needed_bits} bits, but only {available_bits} available")]
    BitstreamUnderflow {
        needed_bits: usize,
        available_bits: usize,
    },

    #[error("Bitstream overflow: exceeded capacity of {max_bits} bits")]
    BitstreamOverflow {
        max_bits: usize,
    },

    #[error("Invalid packet header: {0}")]
    InvalidPacketHeader(String),

    #[error("Invalid channel index {channel_id}: maximum allowed is {max_channels}")]
    InvalidChannelIndex {
        channel_id: u32,
        max_channels: u32,
    },

    #[error("Invalid channel type identifier: {0}")]
    InvalidChannelType(u8),

    #[error("Channel {0} is closed")]
    ChannelClosed(u32),

    #[error("Channel {0} was not found")]
    ChannelNotFound(u32),

    #[error("Channel {channel_id} sequence mismatch: expected {expected}, got {received}")]
    ChannelSequenceMismatch {
        channel_id: u32,
        expected: u32,
        received: u32,
    },

    #[error("Packet sequence mismatch: expected {expected}, got {received}")]
    PacketSequenceMismatch {
        expected: u32,
        received: u32,
    },

    #[error("Malformed bunch: {0}")]
    MalformedBunch(String),

    #[error("Malformed replication stream: {0}")]
    MalformedReplication(String),

    #[error("Malformed RPC payload: {0}")]
    MalformedRpc(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Network connection disconnected: {0}")]
    Disconnected(String),
}
