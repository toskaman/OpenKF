use crate::bunch::FOutBunch;
use crate::channel::{ChannelDemuxer, DemuxEvent};
use crate::error::{NetworkError, NetworkResult};
use crate::packet::{build_packet, parse_packet, AckTracker, PacketHeader};
use crate::replication::{PropertySchema, PropertyType};
use crate::rpc::{RpcCall, RpcRegistry};
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};

/// Retail Unreal Engine 2.5 multiplayer network client (`UNetDriver` / `UNetConnection`).
/// Handles connection handshake, bitstream packet framing, bunch demultiplexing,
/// dirty property replication, and RPC dispatch over UDP.
#[derive(Debug)]
pub struct NetworkClient {
    /// Optional underlying UDP socket (configured in non-blocking mode).
    socket: Option<UdpSocket>,
    /// Remote server socket address.
    pub server_addr: Option<SocketAddr>,
    /// Monotonically increasing outgoing and incoming sequence tracker.
    pub ack_tracker: AckTracker,
    /// Channel demultiplexer managing Control Channel 0 and Actor Channels 1..1023.
    pub demuxer: ChannelDemuxer,
    /// Queue of outgoing bunches waiting to be framed into packets.
    pending_out_bunches: Vec<FOutBunch>,
    /// Registered actor class property replication schemas.
    pub schemas: HashMap<String, PropertySchema>,
    /// Registered remote procedure call definitions.
    pub rpc_registry: RpcRegistry,
}

impl Default for NetworkClient {
    fn default() -> Self {
        Self::new(1065) // Default Killing Floor v1065
    }
}

impl NetworkClient {
    /// Creates a new network client configured for retail protocol version (v1065).
    pub fn new(protocol_version: u32) -> Self {
        Self {
            socket: None,
            server_addr: None,
            ack_tracker: AckTracker::new(),
            demuxer: ChannelDemuxer::new(protocol_version),
            pending_out_bunches: Vec::new(),
            schemas: HashMap::new(),
            rpc_registry: RpcRegistry::new(),
        }
    }

    /// Initializes a local non-blocking UDP socket and targets the remote server address.
    pub fn connect_udp(&mut self, local_bind: SocketAddr, server_addr: SocketAddr) -> NetworkResult<()> {
        let socket = UdpSocket::bind(local_bind)?;
        socket.set_nonblocking(true)?;
        self.socket = Some(socket);
        self.server_addr = Some(server_addr);

        // Initiate handshake by queuing NMT_Hello on Control Channel 0
        let hello_bunch = self.demuxer.control_channel.create_hello_bunch();
        self.queue_bunch(hello_bunch);
        Ok(())
    }

    /// Queues an outgoing bunch for transmission in the next outgoing packet.
    pub fn queue_bunch(&mut self, bunch: FOutBunch) {
        self.pending_out_bunches.push(bunch);
    }

    /// Registers a class property schema for delta replication parsing.
    pub fn register_schema(&mut self, schema: PropertySchema) {
        self.schemas.insert(schema.class_name.clone(), schema);
    }

    /// Registers a replicated UnrealScript RPC function.
    pub fn register_rpc(
        &mut self,
        function_index: u32,
        name: impl Into<String>,
        param_types: Vec<PropertyType>,
        is_reliable: bool,
    ) {
        self.rpc_registry.register_function(function_index, name, param_types, is_reliable);
    }

    /// Dispatches an RPC call to a specific actor channel.
    pub fn send_rpc(&mut self, channel_id: u32, rpc: &RpcCall) -> NetworkResult<()> {
        let channel = self
            .demuxer
            .actor_channels
            .entry(channel_id)
            .or_insert_with(|| crate::channel::ActorChannel::new(channel_id));

        let bunch = channel.create_rpc_bunch(rpc);
        self.queue_bunch(bunch);
        Ok(())
    }

    /// Processes an incoming raw UDP datagram buffer.
    /// Unpacks header, checks acknowledgments, demultiplexes bunches, and returns generated events.
    pub fn process_packet(
        &mut self,
        data: &[u8],
        current_time_ms: u64,
    ) -> NetworkResult<Vec<DemuxEvent>> {
        let (header, bunches) = parse_packet(data)?;

        // Update acknowledgment tracker with incoming sequence number
        self.ack_tracker.record_received(header.sequence_number);

        // Process peer's acknowledgment of our previous packets and re-queue any lost reliable bunches
        let lost_bunches = self.ack_tracker.process_ack(
            header.ack_sequence,
            header.packet_loss,
            header.ack_bitfield,
            current_time_ms,
        );
        for lost in lost_bunches {
            self.queue_bunch(lost);
        }

        // Demultiplex received bunches across active channels
        let mut events = Vec::new();
        for bunch in bunches {
            if let Some(event) = self
                .demuxer
                .demux_bunch(bunch, &self.schemas, &self.rpc_registry)?
            {
                events.push(event);
            }
        }

        Ok(events)
    }

    /// Assembles pending bunches into a physical UDP packet, records transmission for ack tracking,
    /// and returns the assembled packet bytes.
    pub fn assemble_outgoing_packet(&mut self, current_time_ms: u64) -> NetworkResult<Option<Vec<u8>>> {
        if self.pending_out_bunches.is_empty() {
            return Ok(None);
        }

        let seq = self.ack_tracker.next_outgoing_sequence();
        let (ack_seq, loss, mask) = self
            .ack_tracker
            .record_received(self.ack_tracker.highest_incoming_sequence());

        let mut header = PacketHeader::new(seq, ack_seq, loss);
        if let Some(bitfield) = mask {
            header = header.with_bitfield(bitfield);
        }

        let bunches_to_send = std::mem::take(&mut self.pending_out_bunches);
        let reliable_bunches: Vec<FOutBunch> = bunches_to_send
            .iter()
            .filter(|b| b.b_reliable)
            .cloned()
            .collect();

        self.ack_tracker
            .record_sent(seq, current_time_ms, reliable_bunches);

        let packet_data = build_packet(&header, &bunches_to_send)?;
        Ok(Some(packet_data))
    }

    /// Non-blocking tick: receives any pending UDP packets from the socket,
    /// and flushes queued outgoing packets.
    pub fn tick(&mut self, current_time_ms: u64) -> NetworkResult<Vec<DemuxEvent>> {
        let mut events = Vec::new();

        // 1. Drain incoming socket datagrams without retaining an immutable borrow on self
        let mut incoming_datagrams = Vec::new();
        if let (Some(socket), Some(expected_addr)) = (&self.socket, self.server_addr) {
            let mut recv_buf = [0u8; 2048];
            loop {
                match socket.recv_from(&mut recv_buf) {
                    Ok((bytes_read, src_addr)) => {
                        if src_addr == expected_addr {
                            incoming_datagrams.push(recv_buf[..bytes_read].to_vec());
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        break;
                    }
                    Err(e) => return Err(NetworkError::Io(e)),
                }
            }
        }

        // Process collected datagrams
        for datagram in incoming_datagrams {
            let packet_events = self.process_packet(&datagram, current_time_ms)?;
            events.extend(packet_events);
        }

        // 2. Transmit queued outgoing packets
        if let Some(packet_data) = self.assemble_outgoing_packet(current_time_ms)? {
            if let (Some(socket), Some(server_addr)) = (&self.socket, self.server_addr) {
                socket.send_to(&packet_data, server_addr)?;
            }
        }

        Ok(events)
    }

    /// Current smoothed RTT in milliseconds.
    pub fn rtt_ms(&self) -> f32 {
        self.ack_tracker.rtt_ms()
    }

    /// Packet loss fraction (0.0 to 1.0).
    pub fn loss_rate(&self) -> f32 {
        self.ack_tracker.loss_rate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{ActorChannelEvent, ControlEvent};
    use crate::rpc::RpcParam;

    #[test]
    fn test_client_handshake_simulation() {
        let mut client = NetworkClient::new(1065);
        let hello_bunch = client.demuxer.control_channel.create_hello_bunch();
        client.queue_bunch(hello_bunch);

        let out_bytes = client.assemble_outgoing_packet(1000).unwrap().unwrap();
        assert!(!out_bytes.is_empty());

        // Simulate server sending back Challenge
        let mut server_client = NetworkClient::new(1065);
        let server_events = server_client.process_packet(&out_bytes, 1010).unwrap();
        assert_eq!(server_events.len(), 1);
        assert_eq!(
            server_events[0],
            DemuxEvent::Control(ControlEvent::HelloReceived { protocol_version: 1065 })
        );

        let mut challenge_writer = crate::bitstream::BitWriter::new();
        challenge_writer.write_u8(crate::channel::NetworkMessageType::Challenge as u8);
        challenge_writer.write_u32(0x1337BEEF);
        let challenge_bunch = FOutBunch::new(0, true).with_bit_writer(challenge_writer);

        server_client.queue_bunch(challenge_bunch);
        let server_out = server_client.assemble_outgoing_packet(1020).unwrap().unwrap();

        let client_events = client.process_packet(&server_out, 1030).unwrap();
        assert_eq!(client_events.len(), 1);
        assert_eq!(
            client_events[0],
            DemuxEvent::Control(ControlEvent::ChallengeReceived {
                challenge: 0x1337BEEF
            })
        );
    }

    #[test]
    fn test_client_rpc_and_replication_integration() {
        let mut client = NetworkClient::new(1065);
        client.register_rpc(
            42,
            "ServerBuyWeapon",
            vec![PropertyType::Int, PropertyType::String],
            true,
        );

        let mut schema = PropertySchema::new("KFPlayerPawn");
        let _health_idx = schema.add_property("Health", PropertyType::Int);
        client.register_schema(schema);

        // Send RPC
        let rpc = RpcCall::new(42, true)
            .with_param(RpcParam::Int(1000))
            .with_param(RpcParam::String("KFMod.Deagle".into()));
        client.send_rpc(5, &rpc).unwrap();

        let packet_data = client.assemble_outgoing_packet(1000).unwrap().unwrap();

        // Target receiver
        let mut receiver = NetworkClient::new(1065);
        receiver.register_rpc(
            42,
            "ServerBuyWeapon",
            vec![PropertyType::Int, PropertyType::String],
            true,
        );
        let events = receiver.process_packet(&packet_data, 1010).unwrap();

        assert_eq!(events.len(), 1);
        match &events[0] {
            DemuxEvent::Actor(ActorChannelEvent::RpcExecuted { channel_id, rpc, .. }) => {
                assert_eq!(*channel_id, 5);
                assert_eq!(rpc.function_index, 42);
                assert_eq!(rpc.parameters[0], RpcParam::Int(1000));
            }
            other => panic!("Unexpected event: {other:?}"),
        }
    }
}
