//! A chat server that broadcasts a message to all connections.
//!
//! This is a simple line-based server which accepts WebSocket connections,
//! reads lines from those connections, and broadcasts the lines to all other
//! connected clients.
//!
//! You can test this out by running:
//!
//!     cargo run --example server 127.0.0.1:12345
//!
//! And then in another window run:
//!
//!     cargo run --example client ws://127.0.0.1:12345/
//!
//! You can run the second command in multiple windows and then chat between the
//! two, seeing the messages from the other client as they're received. For all
//! connected clients they'll all join the same room and see everyone else's
//! messages.

mod proto;

use proto::{GetInfo, Handshake, Pdu, Protocol};

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

/*
static GET_INFO_RESPONSE: Pdu = Pdu::Handshake(
    Handshake::GetInfo(
        GetInfo::Ok {
            protocol: Protocol::SupportedVersion(["0"])
        }
    )
);
 */

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;
//type UnorderedPeers = Arc<Mutex>

fn process_msg(pdu: &Pdu, peer_map: &PeerMap, addr: &SocketAddr) {
    /*let peers = peer_map.lock().unwrap();
    peers
        .get(addr)
        .unwrap()
        .unbounded_send(Message::Text(serde_json::to_string(pdu).unwrap()))
        .unwrap();*/

    match pdu {
        Pdu::Handshake(hs) => match hs {
            Handshake::GetInfo(gi) => match gi {
                GetInfo::Request {} => {
                    if let Some(me) = peer_map.lock().unwrap().get(addr) {
                        let resp = Pdu::Handshake(Handshake::GetInfo(GetInfo::Ok {
                            protocol: Protocol::SupportedVersion(vec![String::from("0")]),
                        }));

                        match serde_json::to_string(&resp) {
                            Ok(resp) => {
                                if let Err(e) = me.unbounded_send(Message::Text(resp)) {
                                    error!("mpsc unbound_send failed with message \"{}\"", e);
                                }
                            }
                            Err(e) => error!("serialize failed with message \"{}\"", e),
                        }
                    } else {
                        error!("get({}) from peer_map failed while GetInfo::Request", addr);
                    }
                }
                _ => (),
            },
            _ => (),
        },
        Pdu::MatchmakingQueue(_) => (),
    }
    //println!("qweqwe");
}

async fn handle_connection(peer_map: PeerMap, raw_stream: TcpStream, addr: SocketAddr) {
    debug!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream).await;
    //.expect("Error during the websocket handshake occurred");

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
    peer_map.lock().unwrap().insert(addr, tx);

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
                process_msg(&p, &peer_map, &addr);
            }
            Err(e) => error!(
                "Parsing received message from peer {} failed with message \"{}\"",
                addr, e
            ),
        }

        /*let peers = peer_map.lock().unwrap();

        // We want to broadcast the message to everyone except ourselves.
        let broadcast_recipients = peers
            .iter()
            .filter(|(peer_addr, _)| peer_addr != &&addr)
            .map(|(_, ws_sink)| ws_sink);

        for recp in broadcast_recipients {
            recp.unbounded_send(msg.clone()).unwrap();
        }*/

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
