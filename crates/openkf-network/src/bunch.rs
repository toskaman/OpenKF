use crate::bitstream::{BitReader, BitWriter};
use crate::error::{NetworkError, NetworkResult};

/// Maximum channel index supported by Unreal Engine 2.5 networking (0..1023).
pub const MAX_CHANNELS: u32 = 1024;

/// Unreal Engine 2.5 logical channel type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ChannelType {
    /// Channel 0: Handshake, authentication, connection management (`UControlChannel`).
    Control = 1,
    /// Channels 1..1023: Actor spawn, destruction, property replication, RPCs (`UActorChannel`).
    Actor = 2,
    /// Package and map transfer channel (`UFileChannel`).
    File = 3,
    /// Voice over IP transmission channel (`UVoiceChannel`).
    Voice = 4,
}

impl ChannelType {
    /// Decodes a 3-bit channel type identifier from the bitstream.
    pub fn from_u8(value: u8) -> NetworkResult<Self> {
        match value {
            1 => Ok(Self::Control),
            2 => Ok(Self::Actor),
            3 => Ok(Self::File),
            4 => Ok(Self::Voice),
            other => Err(NetworkError::InvalidChannelType(other)),
        }
    }

    /// Converts channel type to its 3-bit wire representation.
    #[inline]
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Incoming bunch received from a network packet destined for a specific channel.
/// Corresponds to `FInBunch` in Unreal Engine 2.5.
#[derive(Debug, Clone, PartialEq)]
pub struct FInBunch {
    /// Target channel index (0..1023).
    pub channel_index: u32,
    /// Channel type, present if `b_open` is true.
    pub channel_type: Option<ChannelType>,
    /// Monotonically increasing reliable sequence number per channel.
    pub ch_sequence: u32,
    /// 1 if this bunch opens a new channel.
    pub b_open: bool,
    /// 1 if this bunch closes the channel.
    pub b_close: bool,
    /// 1 if transmission requires reliable delivery.
    pub b_reliable: bool,
    /// 1 if this bunch is part of a multi-bunch fragmented payload.
    pub b_partial: bool,
    /// 1 if this is the initial fragment of a partial bunch.
    pub b_partial_initial: bool,
    /// 1 if this is the final fragment of a partial bunch.
    pub b_partial_final: bool,
    /// Raw payload bytes.
    pub payload: Vec<u8>,
    /// Exact number of bits in the payload (may not be byte-aligned).
    pub bit_count: usize,
}

/// Outgoing bunch queued to be serialized into a network packet.
/// Corresponds to `FOutBunch` in Unreal Engine 2.5.
#[derive(Debug, Clone, PartialEq)]
pub struct FOutBunch {
    /// Target channel index (0..1023).
    pub channel_index: u32,
    /// Channel type, present if `b_open` is true.
    pub channel_type: Option<ChannelType>,
    /// Monotonically increasing reliable sequence number per channel.
    pub ch_sequence: u32,
    /// 1 if this bunch opens a new channel.
    pub b_open: bool,
    /// 1 if this bunch closes the channel.
    pub b_close: bool,
    /// 1 if transmission requires reliable delivery.
    pub b_reliable: bool,
    /// 1 if this bunch is part of a multi-bunch fragmented payload.
    pub b_partial: bool,
    /// 1 if this is the initial fragment of a partial bunch.
    pub b_partial_initial: bool,
    /// 1 if this is the final fragment of a partial bunch.
    pub b_partial_final: bool,
    /// Raw payload bytes.
    pub payload: Vec<u8>,
    /// Exact number of bits in the payload.
    pub bit_count: usize,
}

impl FOutBunch {
    /// Creates a new outgoing bunch for a specified channel.
    pub fn new(channel_index: u32, b_reliable: bool) -> Self {
        Self {
            channel_index,
            channel_type: None,
            ch_sequence: 0,
            b_open: false,
            b_close: false,
            b_reliable,
            b_partial: false,
            b_partial_initial: false,
            b_partial_final: false,
            payload: Vec::new(),
            bit_count: 0,
        }
    }

    /// Sets the bunch as an opening bunch for the specified channel type.
    pub fn with_open(mut self, channel_type: ChannelType) -> Self {
        self.b_open = true;
        self.channel_type = Some(channel_type);
        self
    }

    /// Sets the bunch as a channel closing bunch.
    pub fn with_close(mut self) -> Self {
        self.b_close = true;
        self
    }

    /// Sets the reliable sequence number.
    pub fn with_sequence(mut self, seq: u32) -> Self {
        self.ch_sequence = seq;
        self
    }

    /// Sets the payload from raw bytes (byte-aligned).
    pub fn with_bytes(mut self, data: &[u8]) -> Self {
        self.payload = data.to_vec();
        self.bit_count = data.len() * 8;
        self
    }

    /// Sets the payload from a `BitWriter`.
    pub fn with_bit_writer(mut self, writer: BitWriter) -> Self {
        self.bit_count = writer.num_bits();
        self.payload = writer.into_bytes();
        self
    }

    /// Creates a `BitReader` targeting this bunch's payload.
    pub fn payload_reader(&self) -> BitReader<'_> {
        BitReader::with_bit_count(&self.payload, self.bit_count)
    }

    /// Serializes this bunch into a target `BitWriter`.
    pub fn write_to(&self, writer: &mut BitWriter) -> NetworkResult<()> {
        if self.channel_index >= MAX_CHANNELS {
            return Err(NetworkError::InvalidChannelIndex {
                channel_id: self.channel_index,
                max_channels: MAX_CHANNELS,
            });
        }

        // Bit 0: bOpen
        writer.write_bit(self.b_open);
        // Bit 1: bClose
        writer.write_bit(self.b_close);
        // Bit 2: bReliable
        writer.write_bit(self.b_reliable);

        // Bit 3..10: ChannelIndex (compact integer)
        writer.write_compact_u32(self.channel_index);

        // Bit 11..13: ChannelType (3 bits, present only if bOpen == 1)
        if self.b_open {
            let ch_type = self.channel_type.unwrap_or(ChannelType::Actor);
            writer.write_bits(ch_type.as_u8() as u64, 3);
        }

        // If reliable, write channel reliable sequence number
        if self.b_reliable {
            writer.write_compact_u32(self.ch_sequence);
        }

        // Partial bunch flags
        writer.write_bit(self.b_partial);
        if self.b_partial {
            writer.write_bit(self.b_partial_initial);
            writer.write_bit(self.b_partial_final);
        }

        // Payload bit count and data
        writer.write_compact_u32(self.bit_count as u32);
        if self.bit_count > 0 {
            let mut payload_reader = BitReader::with_bit_count(&self.payload, self.bit_count);
            writer.append_bits_from_reader(&mut payload_reader)?;
        }

        Ok(())
    }
}

impl FInBunch {
    /// Reads and deserializes an `FInBunch` from an incoming bitstream reader.
    pub fn read_from(reader: &mut BitReader) -> NetworkResult<Self> {
        // Bit 0: bOpen
        let b_open = reader.read_bit()?;
        // Bit 1: bClose
        let b_close = reader.read_bit()?;
        // Bit 2: bReliable
        let b_reliable = reader.read_bit()?;

        // ChannelIndex (compact integer)
        let channel_index = reader.read_compact_u32()?;
        if channel_index >= MAX_CHANNELS {
            return Err(NetworkError::InvalidChannelIndex {
                channel_id: channel_index,
                max_channels: MAX_CHANNELS,
            });
        }

        // ChannelType (3 bits, present only if bOpen == 1)
        let channel_type = if b_open {
            let raw_type = reader.read_bits(3)? as u8;
            Some(ChannelType::from_u8(raw_type)?)
        } else {
            None
        };

        // If reliable, read channel reliable sequence number
        let ch_sequence = if b_reliable {
            reader.read_compact_u32()?
        } else {
            0
        };

        // Partial bunch flags
        let b_partial = reader.read_bit()?;
        let (b_partial_initial, b_partial_final) = if b_partial {
            (reader.read_bit()?, reader.read_bit()?)
        } else {
            (false, false)
        };

        // Payload bit length and data
        let bit_count = reader.read_compact_u32()? as usize;
        let byte_count = bit_count.div_ceil(8);
        let mut payload = vec![0u8; byte_count];

        if bit_count > 0 {
            let mut payload_writer = BitWriter::with_capacity(byte_count);
            for _ in 0..bit_count {
                let bit = reader.read_bit()?;
                payload_writer.write_bit(bit);
            }
            payload = payload_writer.into_bytes();
        }

        Ok(Self {
            channel_index,
            channel_type,
            ch_sequence,
            b_open,
            b_close,
            b_reliable,
            b_partial,
            b_partial_initial,
            b_partial_final,
            payload,
            bit_count,
        })
    }

    /// Creates a `BitReader` targeting this bunch's payload.
    pub fn payload_reader(&self) -> BitReader<'_> {
        BitReader::with_bit_count(&self.payload, self.bit_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bunch_serialization_roundtrip() {
        let mut writer = BitWriter::new();
        let bunch_out = FOutBunch::new(1, true)
            .with_open(ChannelType::Actor)
            .with_sequence(42)
            .with_bytes(b"ReplicationPayloadData");

        bunch_out.write_to(&mut writer).unwrap();

        let bytes = writer.into_bytes();
        let mut reader = BitReader::new(&bytes);
        let bunch_in = FInBunch::read_from(&mut reader).unwrap();

        assert_eq!(bunch_in.channel_index, 1);
        assert_eq!(bunch_in.b_open, true);
        assert_eq!(bunch_in.b_close, false);
        assert_eq!(bunch_in.b_reliable, true);
        assert_eq!(bunch_in.channel_type, Some(ChannelType::Actor));
        assert_eq!(bunch_in.ch_sequence, 42);
        assert_eq!(bunch_in.payload, b"ReplicationPayloadData");
        assert_eq!(bunch_in.bit_count, b"ReplicationPayloadData".len() * 8);
    }

    #[test]
    fn test_control_channel_bunch() {
        let mut writer = BitWriter::new();
        let bunch_out = FOutBunch::new(0, true)
            .with_open(ChannelType::Control)
            .with_sequence(1)
            .with_bytes(b"\x00HELLO");

        bunch_out.write_to(&mut writer).unwrap();

        let bytes = writer.into_bytes();
        let mut reader = BitReader::new(&bytes);
        let bunch_in = FInBunch::read_from(&mut reader).unwrap();

        assert_eq!(bunch_in.channel_index, 0);
        assert_eq!(bunch_in.channel_type, Some(ChannelType::Control));
        assert_eq!(bunch_in.payload, b"\x00HELLO");
    }
}
