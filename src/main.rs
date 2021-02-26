mod proto;

use proto::{Connect, ConnectError, GetInfo, Handshake, Pdu, Protocol, Server};

use env_logger::Builder;
use log::LevelFilter;
use log::{debug, error, info, log_enabled, warn, Level};

use futures_util::SinkExt;

use std::{
    collections::HashMap,
    env,
    io::Error as IoError,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use serde::Serialize;
use tokio::net::{TcpListener, TcpStream};
use tungstenite::protocol::Message;

use anyhow::{Context, Result};

struct ClientInfo {
    name: String,
    version: String,
    protocol: String,
}

struct Peer {
    tx: Tx,
    client_info: Option<ClientInfo>,
}

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Peer>>>;
//type UnorderedPeers = Arc<Mutex>

const PROTO_VER: &str = "0";

fn process_get_info(peer_map: &PeerMap, addr: &SocketAddr) -> Result<()> {
    let resp = Pdu::Handshake(Handshake::GetInfo(GetInfo::Ok {
        protocol: Protocol::SupportedVersion(vec![String::from(PROTO_VER)]),
    }));
    let resp = serde_json::to_string(&resp)?;
    peer_map
        .lock()
        .unwrap()
        .get(addr)
        .context(format!(
            "get({}) from peer_map failed while GetInfo::Request",
            addr
        ))?
        .tx
        .unbounded_send(Message::Text(resp))?;
    Ok(())
}

fn process_connect(
    peer_map: &PeerMap,
    addr: &SocketAddr,
    name: &str,
    version: &str,
    proto_ver: &str,
) -> Result<()> {
    if proto_ver == PROTO_VER {
        let resp = Pdu::Handshake(Handshake::Connect(Connect::Ok {
            server: Server {
                name: String::from("fpc-server-rs"),
                version: String::from("0.0.1"),
            },
        }));
        let resp = serde_json::to_string(&resp)?;
        let mut lock = peer_map.lock().unwrap();
        let mut me = lock
            .get_mut(addr)
            .context(format!("get({}) from peer_map failed", addr))?;
        me.client_info = Some(ClientInfo {
            name: String::from(name),
            version: String::from(version),
            protocol: String::from(proto_ver),
        });
        me.tx.unbounded_send(Message::Text(resp))?;
    } else {
        let resp = Pdu::Handshake(Handshake::Connect(Connect::Error(
            ConnectError::UnsupportedProtocolVersion {
                description: String::from("Unsupported client version"),
            },
        )));
        let resp = serde_json::to_string(&resp)?;
        peer_map
            .lock()
            .unwrap()
            .get(addr)
            .context(format!(
                "get({}) from peer_map failed while GetInfo::Request",
                addr
            ))?
            .tx
            .unbounded_send(Message::Text(resp))?;
    }
    Ok(())
}

fn process_msg(pdu: &Pdu, peer_map: &PeerMap, addr: &SocketAddr) -> Result<()> {
    match pdu {
        Pdu::Handshake(hs) => match hs {
            Handshake::GetInfo(gi) => match gi {
                GetInfo::Request {} => process_get_info(peer_map, addr),
                _ => Ok(()),
            },
            Handshake::Connect(c) => match c {
                Connect::Client {
                    name,
                    version,
                    protocol,
                } => match protocol {
                    Protocol::Version(proto_ver) => {
                        process_connect(peer_map, addr, name, version, proto_ver)
                    }
                    _ => Ok(()),
                },
                _ => Ok(()),
            },
            _ => Ok(()),
        },
        Pdu::MatchmakingQueue(_) => Ok(()),
    }
}

async fn handle_connection(peer_map: PeerMap, raw_stream: TcpStream, addr: SocketAddr) {
    debug!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream).await;

    let ws_stream = match ws_stream {
        Ok(s) => s,
        Err(e) => {
            error!(
                "Error during the websocket handshake occurred from \"{}\" \"{}\"",
                addr, e
            );
            return;
        }
    };

    debug!("WebSocket connection established from: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();
    let peer = Peer {
        tx,
        client_info: None,
    };
    peer_map.lock().unwrap().insert(addr, peer);

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        let pdu = serde_json::from_str::<Pdu>(msg.to_text().unwrap());
        debug!(
            "Received a message from {}: \"{}\"",
            addr,
            msg.to_text().unwrap()
        );

        match pdu {
            Ok(p) => {
                debug!("{:?}", p);
                if let Err(e) = process_msg(&p, &peer_map, &addr) {
                    error!("{}", e);
                }
            }
            Err(e) => error!(
                "Parsing received message from peer {} failed with message \"{}\"",
                addr, e
            ),
        }

        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    //broadcast_incoming.await;
    future::select(broadcast_incoming, receive_from_others).await;

    debug!("{} disconnected", &addr);
    peer_map.lock().unwrap().remove(&addr);
}

#[tokio::main]
async fn main() -> Result<(), IoError> {
    let mut builder = Builder::new();
    builder.filter(Some("server_rs"), LevelFilter::Debug).init();

    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:8080".to_string());

    let state = PeerMap::new(Mutex::new(HashMap::new()));

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", addr);

    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(state.clone(), stream, addr));
    }

    Ok(())
}
