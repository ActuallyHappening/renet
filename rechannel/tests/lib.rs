use bytes::Bytes;
use rechannel::{
    error::DisconnectionReason,
    remote_connection::{ConnectionConfig, RemoteConnection},
    serialize_disconnect_packet,
    server::{RechannelServer, ServerEvent},
};

use bincode::{self, Options};
use rand::prelude::*;
use serde::{Deserialize, Serialize};

use std::time::Duration;

pub fn init_log() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TestMessage {
    value: u64,
}

#[test]
fn test_remote_connection_reliable_channel() {
    init_log();
    let mut server = RechannelServer::new(ConnectionConfig::default());
    let mut client = RemoteConnection::new(ConnectionConfig::default());
    let client_id = 0u64;
    server.add_client(client_id);

    let number_messages = 100;
    let mut current_message_number = 0;

    for i in 0..number_messages {
        let message = TestMessage { value: i };
        let message = bincode::options().serialize(&message).unwrap();
        server.send_message(client_id, 0, message);
    }

    loop {
        let packets = server.get_packets_to_send(client_id).unwrap();
        for packet in packets.into_iter() {
            client.process_packet(&packet).unwrap();
        }

        while let Some(message) = client.receive_message(0) {
            let message: TestMessage = bincode::options().deserialize(&message).unwrap();
            assert_eq!(current_message_number, message.value);
            current_message_number += 1;
        }

        if current_message_number == number_messages {
            break;
        }
    }

    assert_eq!(number_messages, current_message_number);
}

#[test]
fn test_server_reliable_channel() {
    init_log();
    let mut server = RechannelServer::new(ConnectionConfig::default());
    let mut client = RemoteConnection::new(ConnectionConfig::default());
    let client_id = 0u64;
    server.add_client(client_id);

    let number_messages = 100;
    let mut current_message_number = 0;

    for i in 0..number_messages {
        let message = TestMessage { value: i };
        let message = bincode::options().serialize(&message).unwrap();
        client.send_message(0, Bytes::from(message));
    }

    loop {
        let packets = client.get_packets_to_send().unwrap();
        for packet in packets.into_iter() {
            server.process_packet_from(&packet, client_id).unwrap();
        }

        while let Some(message) = server.receive_message(client_id, 0) {
            let message: TestMessage = bincode::options().deserialize(&message).unwrap();
            assert_eq!(current_message_number, message.value);
            current_message_number += 1;
        }

        if current_message_number == number_messages {
            break;
        }
    }

    assert_eq!(number_messages, current_message_number);
}

#[test]
fn test_server_disconnect_client() {
    init_log();
    let mut server = RechannelServer::new(ConnectionConfig::default());
    let mut client = RemoteConnection::new(ConnectionConfig::default());
    let client_id = 0u64;
    server.add_client(client_id);
    server.disconnect(client_id);

    let events: Vec<ServerEvent> = server.events().copied().collect();
    assert!(matches!(events[0], ServerEvent::ClientConnected { client_id: 0 }));
    assert!(matches!(
        events[1],
        ServerEvent::ClientDisconnected {
            client_id: 0,
            reason: DisconnectionReason::DisconnectedByServer
        }
    ));

    let packet = serialize_disconnect_packet(DisconnectionReason::DisconnectedByServer).unwrap();
    client.process_packet(&packet).unwrap();

    let client_reason = client.disconnected().unwrap();
    assert_eq!(client_reason, DisconnectionReason::DisconnectedByServer);
}

#[test]
fn test_client_disconnect() {
    init_log();
    let mut server = RechannelServer::new(ConnectionConfig::default());
    let mut client = RemoteConnection::new(ConnectionConfig::default());
    let client_id = 0u64;
    server.add_client(client_id);

    client.disconnect();
    let reason = client.disconnected().unwrap();
    assert_eq!(reason, DisconnectionReason::DisconnectedByClient);

    let packet = serialize_disconnect_packet(reason).unwrap();
    server.process_packet_from(&packet, client_id).unwrap();
    server.update(Duration::ZERO);

    let events: Vec<ServerEvent> = server.events().copied().collect();
    assert!(matches!(
        events[0],
        ServerEvent::ClientDisconnected {
            client_id: 0,
            reason: DisconnectionReason::DisconnectedByClient
        }
    ));
}

struct ClientStatus {
    connection: RemoteConnection,
    received_messages: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestUsage {
    value: Vec<u8>,
}

impl Default for TestUsage {
    fn default() -> Self {
        Self { value: vec![255; 2500] }
    }
}

use std::collections::HashMap;

#[test]
fn test_usage() {
    // TODO: we can't distinguish the log between the clients
    init_log();
    let mut rng = rand::thread_rng();
    let mut server = RechannelServer::new(ConnectionConfig::default());

    let mut clients_status: HashMap<u64, ClientStatus> = HashMap::new();
    let mut sent_messages = 0;

    for i in 0..8u64 {
        let connection = RemoteConnection::new(ConnectionConfig::default());
        let status = ClientStatus {
            connection,
            received_messages: 0,
        };
        clients_status.insert(i, status);
        server.add_client(i);
    }

    loop {
        for (connection_id, status) in clients_status.iter_mut() {
            status.connection.update(Duration::from_millis(100)).unwrap();
            if status.connection.receive_message(0).is_some() {
                status.received_messages += 1;
                if status.received_messages > 32 {
                    panic!("Received more than 32 messages!");
                }
            }
            if status.received_messages == 32 {
                status.connection.disconnect();
                let reason = status.connection.disconnected().unwrap();
                let packet = serialize_disconnect_packet(reason).unwrap();
                server.process_packet_from(&packet, *connection_id).unwrap();
                continue;
            }

            let client_packets = status.connection.get_packets_to_send().unwrap();
            let server_packets = server.get_packets_to_send(*connection_id).unwrap();

            for packet in client_packets.iter() {
                // 10% packet loss emulation
                if rng.gen::<f64>() < 0.9 {
                    server.process_packet_from(packet, *connection_id).unwrap();
                }
            }

            for packet in server_packets.iter() {
                // 10% packet loss emulation
                if rng.gen::<f64>() < 0.9 {
                    status.connection.process_packet(packet).unwrap();
                }
            }
        }

        server.update(Duration::from_millis(100));
        clients_status.retain(|_, c| c.connection.is_connected());

        if sent_messages < 32 {
            let message = bincode::options().serialize(&TestUsage::default()).unwrap();
            server.broadcast_message(0, message);
            sent_messages += 1
        }

        if !server.has_clients() {
            return;
        }
    }
}
