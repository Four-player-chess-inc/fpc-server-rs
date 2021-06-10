mod proto;

use proto::{
    Call, Connect, ConnectError, GetInfo, Handshake, Init, LeftRook, MatchmakingQueue, Move, Pdu,
    PlayerRegister, PlayerRegisterError, Protocol, Server, StartPosition, StartPositions,
};

use tokio::time::{self};

use std::time::{Duration, Instant};

use env_logger::Builder;
use log::LevelFilter;
use log::{debug, error, info, warn, Level};

use std::{
    collections::HashMap,
    env,
    io::Error as IoError,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use futures_channel::mpsc::{unbounded, UnboundedSender, UnboundedReceiver};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use tokio::net::{TcpListener, TcpStream};
use tungstenite::protocol::Message;

use anyhow::{Context, Result};

use std::string::ToString;

struct ClientInfo {
    name: String,
    version: String,
    protocol: String,
}

type Tx = UnboundedSender<Message>;

enum Color {
    Red,
    Green,
    Blue,
    Yellow,
}

//impl ToStringColor {
//    to_
//}

struct Channel {
    rx: UnboundedReceiver<Message>,
    tx: UnboundedSender<Message>
}

struct Player {
    color: Color,
    timer: Duration,
    channel: Option<Channel>
}

struct GameSession {
    game_id: u64,
    player_1: Player,
    player_2: Player,
    player_3: Player,
    player_4: Player,
}

impl GameSession {
    async fn test(&self) {

    }
}

struct PlayerSession {
    game_id: u64,
    color: Color,
}

impl PartialEq for PlayerSession {
    fn eq(&self, other: &Self) -> bool {
        self.game_id == other.game_id
    }
}

#[derive(PartialEq)]
enum PlayerState {
    Unknown,
    Idle,
    MMQueue,
    HeartbeatWait(Instant),
    HeartbeatReady(Instant),
    GameSession(u64),
}

impl PlayerState {
    fn get_hb_wait_since(&self) -> Option<Instant> {
        match self {
            PlayerState::HeartbeatWait(i) => Some(*i),
            _ => None,
        }
    }
    fn get_hb_ready_since(&self) -> Option<Instant> {
        match self {
            PlayerState::HeartbeatReady(i) => Some(*i),
            _ => None,
        }
    }
    fn is_hb_wait(&self) -> bool {
        matches!(self, PlayerState::HeartbeatWait(_))
    }
    fn is_hb_ready(&self) -> bool {
        matches!(self, PlayerState::HeartbeatReady(_))
    }
}

struct Peer {
    tx: Tx,
    player_name: Option<String>,
    state: PlayerState,
    client_info: Option<ClientInfo>,
}

//type Peer = Arc<Mutex<PeerData>>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Peer>>>;

const PROTO_VER: &str = "0";
const SERV_NAME: &str = "fpc-server-rs";
const SERV_VER: &str = "0.0.1";
static HB_DISP_TICK_PERIOD: tokio::time::Duration = tokio::time::Duration::from_secs(1);
static HB_WAIT_TIMEOUT: Duration = Duration::from_secs(2);
static HB_READY_TIMEOUT: Duration = Duration::from_secs(5);
static GS_INIT_PAUSE: tokio::time::Duration = tokio::time::Duration::from_secs(10);

fn process_hs_get_info(peer_map: &PeerMap, addr: &SocketAddr) -> Result<()> {
    let resp = Pdu::Handshake(Handshake::GetInfo(GetInfo::Ok {
        protocol: Protocol::SupportedVersion(vec![String::from(PROTO_VER)]),
    }));
    let resp = serde_json::to_string(&resp)?;
    peer_map
        .lock()
        .unwrap()
        .get(addr)
        .context(format!("get({}) from peer_map failed", addr))?
        .tx
        .unbounded_send(Message::Text(resp))?;
    Ok(())
}

fn process_hs_connect(
    peer_map: &PeerMap,
    addr: &SocketAddr,
    name: &str,
    version: &str,
    proto_ver: &str,
) -> Result<()> {
    if proto_ver == PROTO_VER {
        let resp = Pdu::Handshake(Handshake::Connect(Connect::Ok {
            server: Server {
                name: String::from(SERV_NAME),
                version: String::from(SERV_VER),
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
        me.state = PlayerState::Idle;
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
            .context(format!("get({}) from peer_map failed", addr))?
            .tx
            .unbounded_send(Message::Text(resp))?;
    }
    Ok(())
}

fn process_mm_player_reg(peer_map: &PeerMap, addr: &SocketAddr, name: &str) -> Result<()> {
    let mut lock = peer_map.lock().unwrap();
    let mut me = lock
        .get_mut(addr)
        .context(format!("get({}) from peer_map failed", addr))?;

    let resp: Pdu;
    match me.state {
        PlayerState::Idle => {
            me.player_name = Some(name.to_string());
            me.state = PlayerState::MMQueue;
            resp =
                Pdu::MatchmakingQueue(MatchmakingQueue::PlayerRegister(PlayerRegister::Ok {
                    // TODO: Insert hash
                    session_id: 5.to_string(),
                }));
        }
        PlayerState::HeartbeatReady(_)
        | PlayerState::HeartbeatWait(_)
        | PlayerState::MMQueue
        | PlayerState::GameSession(_) => {
            resp = Pdu::MatchmakingQueue(MatchmakingQueue::PlayerRegister(
                PlayerRegister::Error(PlayerRegisterError::AlreadyRegistered {
                    description: "You are already in matchmaking queue or active game session"
                        .to_string(),
                }),
            ));
        }
        PlayerState::Unknown => {
            resp = Pdu::MatchmakingQueue(MatchmakingQueue::PlayerRegister(
                PlayerRegister::Error(PlayerRegisterError::Handshake {
                    description: "pass handshake first"
                        .to_string(),
                }),
            ));
        }
    }
    let resp = serde_json::to_string(&resp)?;
    me.tx.unbounded_send(Message::Text(resp))?;
    Ok(())
}

fn process_mm_player_leave(peer_map: &PeerMap, addr: &SocketAddr) -> Result<()> {
    let mut lock = peer_map.lock().unwrap();
    let mut me = lock
        .get_mut(addr)
        .context(format!("get({}) from peer_map failed", addr))?;
    match me.state {
        PlayerState::MMQueue | PlayerState::HeartbeatWait(_) | PlayerState::HeartbeatReady(_) => {
            me.state = PlayerState::Idle;
        }
        _ => (),
    }
    Ok(())
}

fn process_mm_heartbeat_check(peer_map: &PeerMap, addr: &SocketAddr) -> Result<()> {
    let mut lock = peer_map.lock().unwrap();
    let mut me = lock
        .get_mut(addr)
        .context(format!("get({}) from peer_map failed", addr))?;
    if me.state.is_hb_wait() {
        me.state = PlayerState::HeartbeatReady(Instant::now());
    }
    Ok(())
}

fn process_msg(pdu: &Pdu, peer_map: &PeerMap, addr: &SocketAddr) -> Result<()> {
    match pdu {
        Pdu::Handshake(hs) => match hs {
            Handshake::GetInfo(gi) => match gi {
                GetInfo::Request {} => process_hs_get_info(peer_map, addr),
                _ => Ok(()),
            },
            Handshake::Connect(c) => match c {
                Connect::Client {
                    name,
                    version,
                    protocol,
                } => match protocol {
                    Protocol::Version(proto_ver) => {
                        process_hs_connect(peer_map, addr, name, version, proto_ver)
                    }
                    _ => Ok(()),
                },
                _ => Ok(()),
            },
        },
        Pdu::MatchmakingQueue(mq) => match mq {
            MatchmakingQueue::PlayerRegister(pr) => match pr {
                PlayerRegister::Name(name) => process_mm_player_reg(peer_map, addr, name),
                _ => Ok(()),
            },
            MatchmakingQueue::PlayerLeave {} => process_mm_player_leave(peer_map, addr),
            MatchmakingQueue::HeartbeatCheck {} => process_mm_heartbeat_check(peer_map, addr),
            _ => Ok(()),
        },
        Pdu::GameSession(_) => Ok(()),
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

    let (tx, rx) = unbounded();
    let peer = Peer {
        tx,
        player_name: None,
        state: PlayerState::Unknown,
        client_info: None,
    };
    // TODO: prevent duplicate
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
    future::select(broadcast_incoming, receive_from_others).await;

    debug!("{} disconnected", &addr);
    peer_map.lock().unwrap().remove(&addr);
}

async fn game_session_dispatcher(peer_map: PeerMap, game_id: u64) {
    tokio::time::sleep(GS_INIT_PAUSE).await;
    let call = Pdu::GameSession(proto::GameSession::Move(Move::Call(Call {
        player: "Red".to_string(),
        timer: 10,
        timer_2: 1,
    })));
    let call = serde_json::to_string(&call).unwrap();
    let call = Message::Text(call);
    let lock = peer_map.lock().unwrap();
    let gamers = lock
        .iter()
        .filter(
            |(_, peer)| matches!(&peer.state, PlayerState::GameSession(e) if e.game_id == game_id),
        )
        .map(|(_, peer)| peer);
    for peer in gamers {
        match peer.tx.unbounded_send(call.clone()) {
            Ok(_) => (),
            Err(e) => error!("unbounded_send failed \"{}\"", e),
        }
    }
}

// Looping infinitely. On loop tick, if we find at least 4 MMQueue players send HeartbeatCheck
// Also, kick (send kick pdu and change state to Idle) players, who did not response Heartbeat
// Also, change state HearbeatReady => MMQueue if timeout
// TODO: Disconnect Idle players?
async fn heartbeat_dispatcher(peer_map: PeerMap) {
    let mut interval = time::interval(HB_DISP_TICK_PERIOD);
    let heartbeat = Pdu::MatchmakingQueue(MatchmakingQueue::HeartbeatCheck {});
    let heartbeat = serde_json::to_string(&heartbeat).unwrap();
    let heartbeat = Message::Text(heartbeat);
    let mut game_id = 0;

    loop {
        interval.tick().await;
        let start = Instant::now();
        let mut lock = peer_map.lock().unwrap();

        // Send heartbeat to every 4 players which in MMQueue state
        let mut mm_queuers: Vec<&mut Peer> = lock
            .iter_mut()
            .filter(|(_, peer)| peer.state == PlayerState::MMQueue)
            .map(|(_, peer)| peer)
            .collect();
        for chunk in mm_queuers.chunks_exact_mut(4) {
            let now = Instant::now();
            for peer in chunk {
                match peer.tx.unbounded_send(heartbeat.clone()) {
                    Ok(_) => peer.state = PlayerState::HeartbeatWait(now),
                    Err(e) => error!("unbounded_send failed \"{}\"", e),
                }
            }
        }

        // Drop to Idle state all players who timeout their HeartbeatWait state
        let mm_waiters = lock
            .iter_mut()
            .filter(|(_, peer)| peer.state.is_hb_wait())
            .map(|(_, peer)| peer);
        let kick = Pdu::MatchmakingQueue(MatchmakingQueue::PlayerKick {
            discritpion: "Heartbeat timeout".to_string(),
        });
        let kick = serde_json::to_string(&kick).unwrap();
        let kick = Message::Text(kick);
        let now = Instant::now();
        for peer in mm_waiters {
            let wait_time = now.duration_since(peer.state.get_hb_wait_since().unwrap());
            if wait_time > HB_WAIT_TIMEOUT {
                match peer.tx.unbounded_send(kick.clone()) {
                    Ok(_) => {
                        peer.state = PlayerState::Idle;
                        peer.player_name = None;
                    }
                    Err(e) => error!("unbounded_send failed \"{}\"", e),
                }
            }
        }

        // HeartbeatReady => MMQueue if timeout
        // This require coz group of four player may not get ready
        // for a long time due to other players leave by HeartbeatWait timeout.
        let mm_ready = lock
            .iter_mut()
            .filter(|(_, peer)| peer.state.is_hb_ready())
            .map(|(_, peer)| peer);
        let now = Instant::now();
        for peer in mm_ready {
            let wait_time = now.duration_since(peer.state.get_hb_ready_since().unwrap());
            if wait_time > HB_READY_TIMEOUT {
                peer.state = PlayerState::MMQueue;
            }
        }

        // Now create GameSession form the HeartbeatReady players and broadcast init
        let mut mm_ready: Vec<&mut Peer> = lock
            .iter_mut()
            .filter(|(_, peer)| peer.state.is_hb_ready())
            .map(|(_, peer)| peer)
            .collect();
        for chunk in mm_ready.chunks_exact_mut(4) {
            let mut iter = chunk.iter_mut();
            let red = iter.next().unwrap();
            // TODO: prevent duplicate game_id
            red.state = PlayerState::GameSession(PlayerSession {
                game_id,
                color: Color::Red,
            });
            let green = iter.next().unwrap();
            green.state = PlayerState::GameSession(PlayerSession {
                game_id,
                color: Color::Green,
            });
            let blue = iter.next().unwrap();
            blue.state = PlayerState::GameSession(PlayerSession {
                game_id,
                color: Color::Blue,
            });
            let yellow = iter.next().unwrap();
            yellow.state = PlayerState::GameSession(PlayerSession {
                game_id,
                color: Color::Yellow,
            });

            let init_pdu = Pdu::GameSession(proto::GameSession::Init(Init {
                countdown: GS_INIT_PAUSE.as_secs(),
                start_positions: StartPositions {
                    red: StartPosition {
                        player_name: red.player_name.clone().unwrap(),
                        left_rook: LeftRook {
                            letter: 'D',
                            number: 1,
                        },
                    },
                    green: StartPosition {
                        player_name: green.player_name.clone().unwrap(),
                        left_rook: LeftRook {
                            letter: 'D',
                            number: 1,
                        },
                    },
                    blue: StartPosition {
                        player_name: blue.player_name.clone().unwrap(),
                        left_rook: LeftRook {
                            letter: 'D',
                            number: 1,
                        },
                    },
                    yellow: StartPosition {
                        player_name: green.player_name.clone().unwrap(),
                        left_rook: LeftRook {
                            letter: 'D',
                            number: 1,
                        },
                    },
                },
            }));
            let init_pdu = serde_json::to_string(&init_pdu).unwrap();
            let init_pdu = Message::Text(init_pdu);
            for peer in [red, green, blue, yellow].iter() {
                match peer.tx.unbounded_send(init_pdu.clone()) {
                    Ok(_) => (),
                    Err(e) => error!("unbounded_send failed \"{}\"", e),
                }
            }
            tokio::spawn(game_session_dispatcher(peer_map.clone(), game_id));
            game_id = game_id.wrapping_add(1);
        }

        println!("{:?}", Instant::now().duration_since(start));
    }
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

    tokio::spawn(heartbeat_dispatcher(state.clone()));

    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(state.clone(), stream, addr));
    }

    Ok(())
}
