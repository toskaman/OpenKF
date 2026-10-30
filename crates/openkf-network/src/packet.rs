use crate::bitstream::{BitReader, BitWriter};
use crate::bunch::{FInBunch, FOutBunch};
use crate::error::NetworkResult;
use std::collections::HashMap;

/// Network packet header framing for Unreal Engine 2.5 transport datagrams.
/// Follows `scratch/recon_12_network_client.md` Section 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketHeader {
    /// 32-bit monotonically increasing packet identifier.
    pub sequence_number: u32,
    /// Identifier of the highest contiguous packet received and acknowledged by peer.
    pub ack_sequence: u32,
    /// 1-bit high-water mark packet loss / resend indicator.
    pub packet_loss: bool,
    /// Optional 32-bit selective acknowledgment bitmask (relative to ack_sequence - 1).
    pub ack_bitfield: Option<u32>,
}

impl PacketHeader {
    /// Constructs a new packet header.
    pub fn new(sequence_number: u32, ack_sequence: u32, packet_loss: bool) -> Self {
        Self {
            sequence_number,
            ack_sequence,
            packet_loss,
            ack_bitfield: None,
        }
    }

    /// Sets the selective ack bitfield.
    pub fn with_bitfield(mut self, bitfield: u32) -> Self {
        self.ack_bitfield = Some(bitfield);
        self
    }

    /// Serializes packet header into a bitstream writer.
    pub fn write_to(&self, writer: &mut BitWriter) {
        // SequenceNumber (int32 / u32)
        writer.write_u32(self.sequence_number);
        // AckSequence (int32 / u32)
        writer.write_u32(self.ack_sequence);
        // bPacketLoss (1 bit)
        writer.write_bit(self.packet_loss);

        // Optional selective ack bitfield flag + 32-bit mask
        if let Some(mask) = self.ack_bitfield {
            writer.write_bit(true);
            writer.write_u32(mask);
        } else {
            writer.write_bit(false);
        }
    }

    /// Deserializes packet header from a bitstream reader.
    pub fn read_from(reader: &mut BitReader) -> NetworkResult<Self> {
        let sequence_number = reader.read_u32()?;
        let ack_sequence = reader.read_u32()?;
        let packet_loss = reader.read_bit()?;

        let has_bitfield = reader.read_bit()?;
        let ack_bitfield = if has_bitfield {
            Some(reader.read_u32()?)
        } else {
            None
        };

        Ok(Self {
            sequence_number,
            ack_sequence,
            packet_loss,
            ack_bitfield,
        })
    }
}

/// Metadata stored for sent packets to manage acknowledgments and RTT calculation.
#[derive(Debug, Clone)]
pub struct SentPacketRecord {
    pub sequence: u32,
    pub timestamp_ms: u64,
    pub reliable_bunches: Vec<FOutBunch>,
}

/// Acknowledgment and packet loss tracking engine (`UNetConnection` ack system).
#[derive(Debug)]
pub struct AckTracker {
    last_outgoing_sequence: u32,
    highest_incoming_sequence: u32,
    incoming_history_mask: u32,
    has_received_packet: bool,
    unacked_packets: HashMap<u32, SentPacketRecord>,
    smoothed_rtt_ms: f32,
    packets_sent: u64,
    packets_lost: u64,
}

impl Default for AckTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl AckTracker {
    /// Creates a new `AckTracker`.
    pub fn new() -> Self {
        Self {
            last_outgoing_sequence: 0,
            highest_incoming_sequence: 0,
            incoming_history_mask: 0,
            has_received_packet: false,
            unacked_packets: HashMap::new(),
            smoothed_rtt_ms: 50.0,
            packets_sent: 0,
            packets_lost: 0,
        }
    }

    /// Allocates and returns the next outgoing packet sequence number.
    pub fn next_outgoing_sequence(&mut self) -> u32 {
        self.last_outgoing_sequence = self.last_outgoing_sequence.wrapping_add(1);
        self.last_outgoing_sequence
    }

    /// Returns the current outgoing sequence counter.
    pub fn current_outgoing_sequence(&self) -> u32 {
        self.last_outgoing_sequence
    }

    /// Returns the highest incoming sequence number seen so far.
    pub fn highest_incoming_sequence(&self) -> u32 {
        self.highest_incoming_sequence
    }

    /// Records that an outgoing packet was transmitted.
    pub fn record_sent(
        &mut self,
        sequence: u32,
        timestamp_ms: u64,
        reliable_bunches: Vec<FOutBunch>,
    ) {
        self.packets_sent += 1;
        self.unacked_packets.insert(
            sequence,
            SentPacketRecord {
                sequence,
                timestamp_ms,
                reliable_bunches,
            },
        );
    }

    /// Records an incoming packet and returns the acknowledgment fields for the next outgoing header:
    /// `(ack_sequence, packet_loss, ack_bitfield)`.
    pub fn record_received(&mut self, sequence: u32) -> (u32, bool, Option<u32>) {
        if !self.has_received_packet {
            self.has_received_packet = true;
            self.highest_incoming_sequence = sequence;
            self.incoming_history_mask = 0;
            return (sequence, false, None);
        }

        let diff = sequence.wrapping_sub(self.highest_incoming_sequence) as i32;
        let mut packet_loss = false;

        if diff > 0 {
            // New higher sequence number
            let shift = diff as usize;
            if shift < 32 {
                self.incoming_history_mask = (self.incoming_history_mask << shift) | (1 << (shift - 1));
            } else {
                self.incoming_history_mask = 0;
            }
            if diff > 1 {
                packet_loss = true;
            }
            self.highest_incoming_sequence = sequence;
        } else if diff < 0 {
            // Late or out-of-order packet
            let offset = (-diff) as usize;
            if offset <= 32 {
                self.incoming_history_mask |= 1 << (offset - 1);
            }
        }

        let mask = if self.incoming_history_mask != 0 {
            Some(self.incoming_history_mask)
        } else {
            None
        };

        (self.highest_incoming_sequence, packet_loss, mask)
    }

    /// Processes peer acknowledgments. Returns any reliable bunches that were lost and must be resent.
    pub fn process_ack(
        &mut self,
        ack_seq: u32,
        _peer_loss: bool,
        ack_bitfield: Option<u32>,
        current_time_ms: u64,
    ) -> Vec<FOutBunch> {
        let mut lost_bunches = Vec::new();

        // 1. Acknowledge packet at ack_seq
        if let Some(record) = self.unacked_packets.remove(&ack_seq) {
            let sample_rtt = current_time_ms.saturating_sub(record.timestamp_ms) as f32;
            self.update_rtt(sample_rtt);
        }

        // 2. Selective acknowledgments via bitmask
        if let Some(mask) = ack_bitfield {
            for i in 1..=32 {
                if (mask & (1 << (i - 1))) != 0 {
                    let seq = ack_seq.wrapping_sub(i as u32);
                    if let Some(record) = self.unacked_packets.remove(&seq) {
                        let sample_rtt = current_time_ms.saturating_sub(record.timestamp_ms) as f32;
                        self.update_rtt(sample_rtt);
                    }
                }
            }
        }

        // 3. Mark unacknowledged packets older than ack_seq - 32 as dropped and reclaim reliable bunches
        let mut stale_sequences = Vec::new();
        for (&seq, record) in self.unacked_packets.iter() {
            let diff = ack_seq.wrapping_sub(seq) as i32;
            if diff > 32 {
                stale_sequences.push(seq);
                self.packets_lost += 1;
                for b in &record.reliable_bunches {
                    lost_bunches.push(b.clone());
                }
            }
        }

        for seq in stale_sequences {
            self.unacked_packets.remove(&seq);
        }

        lost_bunches
    }

    /// Updates smoothed RTT with exponential moving average.
    fn update_rtt(&mut self, sample_ms: f32) {
        if sample_ms > 0.0 && sample_ms < 5000.0 {
            self.smoothed_rtt_ms = self.smoothed_rtt_ms * 0.85 + sample_ms * 0.15;
        }
    }

    /// Current smoothed RTT in milliseconds.
    pub fn rtt_ms(&self) -> f32 {
        self.smoothed_rtt_ms
    }

    /// Packet loss fraction (0.0 to 1.0).
    pub fn loss_rate(&self) -> f32 {
        if self.packets_sent == 0 {
            0.0
        } else {
            (self.packets_lost as f32 / self.packets_sent as f32).min(1.0)
        }
    }
}

/// Assembles a physical UDP network packet from a header and an array of bunches.
pub fn build_packet(header: &PacketHeader, bunches: &[FOutBunch]) -> NetworkResult<Vec<u8>> {
    let mut writer = BitWriter::new();
    header.write_to(&mut writer);

    // Number of bunches contained in packet (compact integer)
    writer.write_compact_u32(bunches.len() as u32);

    for bunch in bunches {
        bunch.write_to(&mut writer)?;
    }

    Ok(writer.into_bytes())
}

/// Disassembles a raw physical UDP network packet into its header and contained bunches.
pub fn parse_packet(data: &[u8]) -> NetworkResult<(PacketHeader, Vec<FInBunch>)> {
    let mut reader = BitReader::new(data);
    let header = PacketHeader::read_from(&mut reader)?;

    let bunch_count = reader.read_compact_u32()? as usize;
    let mut bunches = Vec::with_capacity(bunch_count);

    for _ in 0..bunch_count {
        if reader.is_at_end() {
            break;
        }
        let bunch = FInBunch::read_from(&mut reader)?;
        bunches.push(bunch);
    }

    Ok((header, bunches))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bunch::ChannelType;

    #[test]
    fn test_packet_header_roundtrip() {
        let header = PacketHeader::new(1005, 998, true).with_bitfield(0b101001);
        let mut writer = BitWriter::new();
        header.write_to(&mut writer);

        let data = writer.into_bytes();
        let mut reader = BitReader::new(&data);
        let decoded = PacketHeader::read_from(&mut reader).unwrap();

        assert_eq!(decoded.sequence_number, 1005);
        assert_eq!(decoded.ack_sequence, 998);
        assert_eq!(decoded.packet_loss, true);
        assert_eq!(decoded.ack_bitfield, Some(0b101001));
    }

    #[test]
    fn test_full_packet_assembly_and_disassembly() {
        let header = PacketHeader::new(42, 41, false);
        let bunch1 = FOutBunch::new(0, true)
            .with_open(ChannelType::Control)
            .with_sequence(1)
            .with_bytes(b"HELLO_SERVER");

        let bunch2 = FOutBunch::new(5, false)
            .with_bytes(b"REPLICATED_PROPERTIES");

        let packet_bytes = build_packet(&header, &[bunch1, bunch2]).unwrap();

        let (parsed_header, parsed_bunches) = parse_packet(&packet_bytes).unwrap();
        assert_eq!(parsed_header.sequence_number, 42);
        assert_eq!(parsed_header.ack_sequence, 41);
        assert_eq!(parsed_bunches.len(), 2);

        assert_eq!(parsed_bunches[0].channel_index, 0);
        assert_eq!(parsed_bunches[0].channel_type, Some(ChannelType::Control));
        assert_eq!(parsed_bunches[0].payload, b"HELLO_SERVER");

        assert_eq!(parsed_bunches[1].channel_index, 5);
        assert_eq!(parsed_bunches[1].b_reliable, false);
        assert_eq!(parsed_bunches[1].payload, b"REPLICATED_PROPERTIES");
    }

    #[test]
    fn test_ack_tracker_sequence_and_rtt() {
        let mut tracker = AckTracker::new();
        let s1 = tracker.next_outgoing_sequence();
        let s2 = tracker.next_outgoing_sequence();
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);

        tracker.record_sent(s1, 1000, vec![]);
        tracker.record_sent(s2, 1010, vec![]);

        // Peer acknowledges s1 after 30ms
        tracker.process_ack(s1, false, None, 1030);
        assert!(tracker.rtt_ms() > 0.0);

        // Receive incoming packets
        let (ack, loss, mask) = tracker.record_received(10);
        assert_eq!(ack, 10);
        assert_eq!(loss, false);
        assert_eq!(mask, None);

        // Gap in incoming packets
        let (ack2, loss2, mask2) = tracker.record_received(12);
        assert_eq!(ack2, 12);
        assert_eq!(loss2, true); // packet 11 missing
        assert!(mask2.is_some());
    }
}
