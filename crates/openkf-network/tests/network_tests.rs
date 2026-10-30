use glam::Vec3;
use openkf_network::prelude::*;
use openkf_network::{build_packet, parse_packet};

#[test]
fn test_bitstream_primitive_edge_cases() {
    let mut writer = BitWriter::new();
    // Test unaligned single bits
    writer.write_bit(true);
    writer.write_bit(true);
    writer.write_bit(false);
    // Write 5 bits (total 8 bits = 1 byte)
    writer.write_bits(0b11010, 5);
    // Write 13-bit bounded integer
    writer.write_bits(0x1ABC, 13);
    // Write f32 and f64
    writer.write_f32(-999.125);
    writer.write_f64(3.141592653589793);
    // Write vector
    writer.write_vector(Vec3::new(1234.5, -6789.25, 0.0));
    // Write string
    writer.write_string("OpenKF Retail v1065");

    let num_bits = writer.num_bits();
    let bytes = writer.into_bytes();
    let mut reader = BitReader::with_bit_count(&bytes, num_bits);

    assert_eq!(reader.read_bit().unwrap(), true);
    assert_eq!(reader.read_bit().unwrap(), true);
    assert_eq!(reader.read_bit().unwrap(), false);
    assert_eq!(reader.read_bits(5).unwrap(), 0b11010);
    assert_eq!(reader.read_bits(13).unwrap(), 0x1ABC);
    assert_eq!(reader.read_f32().unwrap(), -999.125);
    assert_eq!(reader.read_f64().unwrap(), 3.141592653589793);

    let v = reader.read_vector().unwrap();
    assert_eq!(v, Vec3::new(1234.5, -6789.25, 0.0));

    assert_eq!(reader.read_string().unwrap(), "OpenKF Retail v1065");
    assert!(reader.is_at_end());
}

#[test]
fn test_packet_header_ack_framing() {
    let header = PacketHeader::new(5000, 4995, false).with_bitfield(0b00000000_00000000_00000000_00010101);
    let mut writer = BitWriter::new();
    header.write_to(&mut writer);

    let bytes = writer.into_bytes();
    let mut reader = BitReader::new(&bytes);
    let parsed = PacketHeader::read_from(&mut reader).unwrap();

    assert_eq!(parsed.sequence_number, 5000);
    assert_eq!(parsed.ack_sequence, 4995);
    assert_eq!(parsed.packet_loss, false);
    assert_eq!(parsed.ack_bitfield, Some(0b00000000_00000000_00000000_00010101));
}

#[test]
fn test_multi_channel_bunch_multiplexing() {
    let header = PacketHeader::new(100, 99, false);

    // Bunch 1: Open Control channel (Channel 0)
    let bunch_ctrl = FOutBunch::new(0, true)
        .with_open(ChannelType::Control)
        .with_sequence(1)
        .with_bytes(b"CTRL_MSG");

    // Bunch 2: Open Actor channel (Channel 23)
    let mut spawn_writer = BitWriter::new();
    spawn_writer.write_u32(230); // Actor Net ID
    spawn_writer.write_string("KFMod.ZombieCrawler");
    let bunch_actor_open = FOutBunch::new(23, true)
        .with_open(ChannelType::Actor)
        .with_sequence(1)
        .with_bit_writer(spawn_writer);

    // Bunch 3: Property update on Actor channel 12
    let bunch_actor_update = FOutBunch::new(12, false)
        .with_bytes(&[0x11, 0x22, 0x33, 0x44]);

    // Bunch 4: Close Actor channel (Channel 5)
    let bunch_actor_close = FOutBunch::new(5, true).with_close().with_sequence(10);

    let packet_data = build_packet(
        &header,
        &[bunch_ctrl, bunch_actor_open, bunch_actor_update, bunch_actor_close],
    )
    .unwrap();

    let (parsed_header, parsed_bunches) = parse_packet(&packet_data).unwrap();

    assert_eq!(parsed_header.sequence_number, 100);
    assert_eq!(parsed_bunches.len(), 4);

    // Validate Bunch 1
    assert_eq!(parsed_bunches[0].channel_index, 0);
    assert_eq!(parsed_bunches[0].channel_type, Some(ChannelType::Control));
    assert_eq!(parsed_bunches[0].payload, b"CTRL_MSG");

    // Validate Bunch 2
    assert_eq!(parsed_bunches[1].channel_index, 23);
    assert_eq!(parsed_bunches[1].channel_type, Some(ChannelType::Actor));
    let mut spawn_reader = parsed_bunches[1].payload_reader();
    assert_eq!(spawn_reader.read_u32().unwrap(), 230);
    assert_eq!(spawn_reader.read_string().unwrap(), "KFMod.ZombieCrawler");

    // Validate Bunch 3
    assert_eq!(parsed_bunches[2].channel_index, 12);
    assert_eq!(parsed_bunches[2].b_reliable, false);
    assert_eq!(parsed_bunches[2].payload, &[0x11, 0x22, 0x33, 0x44]);

    // Validate Bunch 4
    assert_eq!(parsed_bunches[3].channel_index, 5);
    assert_eq!(parsed_bunches[3].b_close, true);
}

#[test]
fn test_dirty_property_replication_bitmask() {
    let mut schema = PropertySchema::new("KFHumanPawn");
    let health_idx = schema.add_property("Health", PropertyType::Int);
    let shield_idx = schema.add_property("ShieldStrength", PropertyType::Float);
    let vel_idx = schema.add_property("Velocity", PropertyType::Vector);
    let alive_idx = schema.add_property("bIsAlive", PropertyType::Bool);
    let nick_idx = schema.add_property("PlayerNick", PropertyType::String);

    let mut server_state = ActorReplicationState::new();
    server_state.set_property(health_idx, ReplicationValue::Int(100));
    server_state.set_property(shield_idx, ReplicationValue::Float(50.0));
    server_state.set_property(vel_idx, ReplicationValue::Vector(Vec3::new(1.0, 2.0, 3.0)));
    server_state.set_property(alive_idx, ReplicationValue::Bool(true));
    server_state.set_property(nick_idx, ReplicationValue::String("MrFoster".into()));

    // Server serializes all properties
    let mut writer = BitWriter::new();
    let serialized_count = server_state.diff_and_serialize(&schema, &mut writer).unwrap();
    assert_eq!(serialized_count, 5);

    // Client receives initial state
    let client_buf = writer.into_bytes();
    let mut client_reader = BitReader::new(&client_buf);
    let mut client_state = ActorReplicationState::new();
    let updated = client_state
        .deserialize_and_apply(&schema, &mut client_reader)
        .unwrap();
    assert_eq!(updated.len(), 5);
    assert_eq!(
        client_state.current.get(&nick_idx),
        Some(&ReplicationValue::String("MrFoster".into()))
    );

    // No changes on server -> diff should be 0
    let mut noop_writer = BitWriter::new();
    let noop_count = server_state.diff_and_serialize(&schema, &mut noop_writer).unwrap();
    assert_eq!(noop_count, 0);

    // Server changes only ShieldStrength and bIsAlive
    server_state.set_property(shield_idx, ReplicationValue::Float(25.0));
    server_state.set_property(alive_idx, ReplicationValue::Bool(false));

    let mut delta_writer = BitWriter::new();
    let delta_count = server_state.diff_and_serialize(&schema, &mut delta_writer).unwrap();
    assert_eq!(delta_count, 2);

    let delta_buf = delta_writer.into_bytes();
    let mut delta_reader = BitReader::new(&delta_buf);
    let delta_updated = client_state
        .deserialize_and_apply(&schema, &mut delta_reader)
        .unwrap();
    assert_eq!(delta_updated.len(), 2);
    assert_eq!(
        client_state.current.get(&shield_idx),
        Some(&ReplicationValue::Float(25.0))
    );
    assert_eq!(
        client_state.current.get(&alive_idx),
        Some(&ReplicationValue::Bool(false))
    );
    // Unchanged properties remain intact
    assert_eq!(
        client_state.current.get(&health_idx),
        Some(&ReplicationValue::Int(100))
    );
}

#[test]
fn test_rpc_packet_serialization_and_dispatch() {
    let mut registry = RpcRegistry::new();
    registry.register_function(
        205,
        "ServerFireWeapon",
        vec![PropertyType::Int, PropertyType::Vector, PropertyType::Rotator],
        true,
    );

    let rpc = RpcCall::new(205, true)
        .with_param(RpcParam::Int(1)) // FireMode
        .with_param(RpcParam::Vector(Vec3::new(100.0, 200.0, 50.0))) // Trace start
        .with_param(RpcParam::Rotator {
            pitch: 1024,
            yaw: 2048,
            roll: 0,
        });

    let bunch = rpc.to_bunch(7);
    assert_eq!(bunch.channel_index, 7);
    assert_eq!(bunch.b_reliable, true);

    let mut reader = bunch.payload_reader();
    let decoded_rpc = RpcCall::read_from(&mut reader, &registry).unwrap();

    assert_eq!(decoded_rpc.function_index, 205);
    assert_eq!(decoded_rpc.function_name.as_deref(), Some("ServerFireWeapon"));
    assert_eq!(decoded_rpc.parameters.len(), 3);
    assert_eq!(decoded_rpc.parameters[0], RpcParam::Int(1));
    assert_eq!(
        decoded_rpc.parameters[1],
        RpcParam::Vector(Vec3::new(100.0, 200.0, 50.0))
    );
    assert_eq!(
        decoded_rpc.parameters[2],
        RpcParam::Rotator {
            pitch: 1024,
            yaw: 2048,
            roll: 0
        }
    );
}

#[test]
fn test_end_to_end_network_client_lifecycle() {
    let mut client = NetworkClient::new(1065);
    let mut server = NetworkClient::new(1065);

    // 1. Client connects (sends Hello on channel 0)
    let hello = client.demuxer.control_channel.create_hello_bunch();
    client.queue_bunch(hello);
    let c_packet1 = client.assemble_outgoing_packet(1000).unwrap().unwrap();

    // 2. Server receives packet and processes Hello
    let s_events = server.process_packet(&c_packet1, 1010).unwrap();
    assert_eq!(
        s_events,
        vec![DemuxEvent::Control(ControlEvent::HelloReceived { protocol_version: 1065 })]
    );

    // 3. Server replies with Challenge
    let mut chal_writer = BitWriter::new();
    chal_writer.write_u8(openkf_network::channel::NetworkMessageType::Challenge as u8);
    chal_writer.write_u32(0xCAFEBABE);
    server.queue_bunch(FOutBunch::new(0, true).with_bit_writer(chal_writer));
    let s_packet1 = server.assemble_outgoing_packet(1015).unwrap().unwrap();

    // 4. Client processes Challenge and sends Login
    let c_events = client.process_packet(&s_packet1, 1025).unwrap();
    assert_eq!(
        c_events,
        vec![DemuxEvent::Control(ControlEvent::ChallengeReceived { challenge: 0xCAFEBABE })]
    );

    let login = client.demuxer.control_channel.create_login_bunch("KF_Player", "?class=KFMod.KFHumanPawn");
    client.queue_bunch(login);
    let c_packet2 = client.assemble_outgoing_packet(1030).unwrap().unwrap();

    // 5. Server processes Login and sends Welcome
    let s_login_events = server.process_packet(&c_packet2, 1040).unwrap();
    assert_eq!(
        s_login_events,
        vec![DemuxEvent::Control(ControlEvent::LoginReceived {
            player_name: "KF_Player".into(),
            options: "?class=KFMod.KFHumanPawn".into(),
        })]
    );

    let mut welcome_writer = BitWriter::new();
    welcome_writer.write_u8(openkf_network::channel::NetworkMessageType::Welcome as u8);
    welcome_writer.write_string("KF-BioticsLab.rom");
    welcome_writer.write_u32(1); // Player ID 1
    server.queue_bunch(FOutBunch::new(0, true).with_bit_writer(welcome_writer));

    // Also server spawns player actor on Channel 1
    let mut spawn_writer = BitWriter::new();
    spawn_writer.write_u32(1001); // Actor ID
    spawn_writer.write_string("KFPlayerPawn");
    server.queue_bunch(
        FOutBunch::new(1, true)
            .with_open(ChannelType::Actor)
            .with_bit_writer(spawn_writer),
    );

    let s_packet2 = server.assemble_outgoing_packet(1050).unwrap().unwrap();

    // 6. Client processes Welcome and ActorSpawned
    let c_welcome_events = client.process_packet(&s_packet2, 1060).unwrap();
    assert_eq!(c_welcome_events.len(), 2);
    assert_eq!(
        c_welcome_events[0],
        DemuxEvent::Control(ControlEvent::WelcomeReceived {
            map_name: "KF-BioticsLab.rom".into(),
            player_id: 1,
        })
    );
    assert_eq!(
        c_welcome_events[1],
        DemuxEvent::Actor(ActorChannelEvent::ActorSpawned {
            channel_id: 1,
            actor_id: 1001,
            class_name: "KFPlayerPawn".into(),
        })
    );
}
