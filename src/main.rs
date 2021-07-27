mod board;
mod proto;
mod vault;

use proto::{
    Connect, ConnectError, GameSession, GetInfo, Handshake, Init, MatchmakingQueue, Move, MoveCall,
    Pdu, PlayerRegister, PlayerRegisterError, PlayersStates, Protocol, Server, StartPosition,
    StartPositions, Update,
};

use board::{Board, Position};
use vault::{ClientInfo, Color, Complete, Game, Peer, PeerState, Player, PlayerState};

use tokio::sync::{Mutex, RwLock};
use tokio::time::{self};

use std::time::{Duration, Instant};

use env_logger::Builder;
use log::LevelFilter;
use log::{debug, error, info};

use std::{env, io::Error as IoError, net::SocketAddr, sync::Arc};

use futures::future::Either;
use futures_channel::mpsc::{unbounded, UnboundedReceiver};
use futures_util::{future, pin_mut, StreamExt};
use tokio::net::{TcpListener, TcpStream};

use anyhow::{Context, Result};

use std::string::ToString;

use crate::proto::MoveError;
use crate::vault::WhoMove;
use rand::{distributions::Alphanumeric, Rng};
use tokio::sync::mpsc::UnboundedSender;

type Vault = Arc<RwLock<vault::Vault>>;

const PROTO_VER: &str = "0";
const SERV_NAME: &str = "fpc-server-rs";
const SERV_VER: &str = "0.0.1";
static HB_DISP_TICK_PERIOD: Duration = Duration::from_secs(1);
static HB_WAIT_TIMEOUT: Duration = Duration::from_secs(2);
static HB_READY_TIMEOUT: Duration = Duration::from_secs(5);
static GS_INIT_PAUSE: Duration = Duration::from_secs(10);
static PLAYER_TIMER: Duration = Duration::from_secs(60);
static PLAYER_TIME_2: Duration = Duration::from_secs(5);

macro_rules! send_msg_to {
    ($peers:expr, $addr:expr, $msg:expr) => {
        $peers
            .read()
            .await
            .get_peers()
            .await
            .get($addr)
            .context(format!("get({}) from peer_map failed", $addr))?
            .lock()
            .await
            .tx
            .unbounded_send($msg)?;
    };
}

macro_rules! game_init_pdu {
    ($pause_time:expr, $reconnect_id:expr, $red:expr,
    $green:expr, $blue:expr, $yellow:expr) => {
        Pdu::GameSession(proto::GameSession::Init(Init {
            countdown: $pause_time,
            reconnect_id: $reconnect_id,
            start_positions: StartPositions {
                red: StartPosition {
                    player_name: $red,
                    left_rook: Position::d1,
                },
                blue: StartPosition {
                    player_name: $green,
                    left_rook: Position::a11,
                },
                yellow: StartPosition {
                    player_name: $blue,
                    left_rook: Position::k14,
                },
                green: StartPosition {
                    player_name: $yellow,
                    left_rook: Position::n4,
                },
            },
        }))
        .to_message()
    };
}

fn random_string() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

async fn process_hs_get_info(vault: &Vault, addr: &SocketAddr) -> Result<()> {
    let resp = Pdu::Handshake(Handshake::GetInfo(GetInfo::Ok {
        protocol: Protocol::SupportedVersion(vec![String::from(PROTO_VER)]),
    }))
    .to_message()?;
    send_msg_to!(vault, addr, resp);
    Ok(())
}

async fn process_hs_connect(
    vault: &Vault,
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
        }))
        .to_message()?;

        let lock = vault.write().await;
        let peers_lock = lock.get_peers().await;
        let peer = peers_lock
            .get(addr)
            .context(format!("get({}) from peer_map failed", addr))?;
        let mut peer_lock = peer.lock().await;

        if peer_lock.state.is_unknown() {
            peer_lock.tx.unbounded_send(resp)?;

            peer_lock.state = PeerState::Idle;
            peer_lock.client_info = Some(ClientInfo {
                name: String::from(name),
                version: String::from(version),
                protocol: String::from(proto_ver),
            });

            let mut idle_lock = lock.get_idle().await;
            idle_lock.insert(*addr, peer.clone());
        }
    } else {
        let resp = Pdu::Handshake(Handshake::Connect(Connect::Error(
            ConnectError::UnsupportedProtocolVersion {
                description: String::from("Unsupported client version"),
            },
        )))
        .to_message()?;
        send_msg_to!(vault, addr, resp);
    }
    Ok(())
}

async fn process_mm_player_reg(vault: &Vault, addr: &SocketAddr, name: &str) -> Result<()> {
    let lock = vault.write().await;
    let peers_lock = lock.get_peers().await;
    let peer = peers_lock
        .get(addr)
        .context(format!("get({}) from peer_map failed", addr))?;
    let mut peer_lock = peer.lock().await;
    match peer_lock.state {
        PeerState::Idle => {
            let resp =
                Pdu::MatchmakingQueue(MatchmakingQueue::PlayerRegister(PlayerRegister::Ok {}))
                    .to_message()?;
            peer_lock.tx.unbounded_send(resp)?;
            peer_lock.player_name = Some(name.to_string());
            peer_lock.state = PeerState::MMQueue;
            let mut mm_queue_lock = lock.get_mm_queue().await;
            mm_queue_lock.insert(*addr, peer.clone());
        }
        PeerState::HeartbeatReady(_)
        | PeerState::HeartbeatWait(_)
        | PeerState::MMQueue
        | PeerState::Game { .. } => {
            let resp = Pdu::MatchmakingQueue(MatchmakingQueue::PlayerRegister(
                PlayerRegister::Error(PlayerRegisterError::AlreadyRegistered {
                    description: "You are already in matchmaking queue or active game session"
                        .to_string(),
                }),
            ))
            .to_message()?;
            peer_lock.tx.unbounded_send(resp)?;
        }
        PeerState::Unknown(_) => {
            let resp = Pdu::MatchmakingQueue(MatchmakingQueue::PlayerRegister(
                PlayerRegister::Error(PlayerRegisterError::Handshake {
                    description: "pass handshake first".to_string(),
                }),
            ))
            .to_message()?;
            peer_lock.tx.unbounded_send(resp)?;
        }
    }
    Ok(())
}

async fn process_mm_player_leave(vault: &Vault, addr: &SocketAddr) -> Result<()> {
    let lock = vault.read().await;
    let peers_lock = lock.get_peers().await;
    let peer = peers_lock
        .get(addr)
        .context(format!("get({}) from peer_map failed", addr))?;
    let mut peer_lock = peer.lock().await;
    match peer_lock.state {
        PeerState::MMQueue | PeerState::HeartbeatWait(_) | PeerState::HeartbeatReady(_) => {
            peer_lock.state = PeerState::Idle;
        }
        _ => (),
    }
    Ok(())
}

async fn process_mm_heartbeat_check(vault: &Vault, addr: &SocketAddr) -> Result<()> {
    let lock = vault.write().await;
    let peers_lock = lock.get_peers().await;
    let peer = peers_lock
        .get(addr)
        .context(format!("get({}) from peer_map failed", addr))?;
    let mut peer_lock = peer.lock().await;
    if peer_lock.state.is_hb_wait() {
        peer_lock.state = PeerState::HeartbeatReady(Instant::now());
        let mut hb_ready_lock = lock.get_hb_ready().await;
        hb_ready_lock.insert(*addr, peer.clone());
    }
    Ok(())
}

async fn process_move_make(vault: &Vault, addr: &SocketAddr, mv: &Move) -> Result<()> {
    let now = tokio::time::Instant::now();

    let forbidden_move_pdu =
        Pdu::GameSession(GameSession::Move(Move::Error(MoveError::ForbiddenMove {
            description: "not allowed move".to_string(),
        })))
        .to_message()?;

    match mv {
        Move::Basic { .. }
        | Move::Capture { .. }
        | Move::Promotion { .. }
        | Move::Castling { .. } => {
            let lock = vault.write().await;
            let peers_lock = lock.get_peers().await;
            let peer = peers_lock
                .get(addr)
                .context(format!("get({}) from peer_map failed", addr))?;
            let peer_lock = peer.lock().await;
            match &peer_lock.state {
                PeerState::Game { color, game } => {
                    let mut game_lock = game.lock().await;
                    if game_lock.validate_player_move(&mv, &color) {
                        game_lock.who_move.as_mut().unwrap().complete = Some(Complete {
                            mv: mv.clone(),
                            at: now,
                        });
                        game_lock.move_happen_signal.unbounded_send(())?;
                    } else {
                        peer_lock.tx.unbounded_send(forbidden_move_pdu)?;
                    }
                }
                _ => (),
            };
        }
        Move::NoMove {} | Move::Error(_) => (),
    };

    Ok(())

    /*let now = tokio::time::Instant::now();
    let lock = vault.write().await;
    let peers_lock = lock.get_peers().await;
    let peer = peers_lock
        .get(addr)
        .context(format!("get({}) from peer_map failed", addr))?;
    let mut peer_lock = peer.lock().await;
    if let PeerState::Game { color, game } = &mut peer_lock.state {
        let mut game_lock = game.lock().await;
        let player = game_lock.player_mut(&color);
        if let PlayerState::MoveCallWait {
            since,
            timeout_dispatcher,
        } = &player.state
        {
            /*match game_lock.make_turn(make) {

            }
            let turn_duration = now - *since;
            if turn_duration > PLAYER_TIME_2 {
                player.time_remaining -= turn_duration - PLAYER_TIME_2;
            }
            timeout_dispatcher.abort();*/
        }

        //peer_lock.state = PeerState::HeartbeatReady(Instant::now());
        //let mut hb_ready_lock = lock.get_hb_ready().await;
        //hb_ready_lock.insert(*addr, peer.clone());
    }
    Ok(())*/
}

async fn process_msg(pdu: &Pdu, vault: &Vault, addr: &SocketAddr) -> Result<()> {
    match pdu {
        Pdu::Handshake(hs) => match hs {
            Handshake::GetInfo(gi) => match gi {
                GetInfo::Request {} => process_hs_get_info(vault, addr).await,
                _ => Ok(()),
            },
            Handshake::Connect(c) => match c {
                Connect::Client {
                    name,
                    version,
                    protocol,
                } => match protocol {
                    Protocol::Version(proto_ver) => {
                        process_hs_connect(vault, addr, name, version, proto_ver).await
                    }
                    _ => Ok(()),
                },
                _ => Ok(()),
            },
        },
        Pdu::MatchmakingQueue(mq) => match mq {
            MatchmakingQueue::PlayerRegister(pr) => match pr {
                PlayerRegister::Name(name) => process_mm_player_reg(vault, addr, name).await,
                _ => Ok(()),
            },
            MatchmakingQueue::PlayerLeave {} => process_mm_player_leave(vault, addr).await,
            MatchmakingQueue::HeartbeatCheck {} => process_mm_heartbeat_check(vault, addr).await,
            _ => Ok(()),
        },
        Pdu::GameSession(gs) => match gs {
            GameSession::Move(mv) => process_move_make(vault, addr, mv).await,
            GameSession::Init(_) | GameSession::Update(_) => Ok(()),
        },
    }
}

async fn handle_connection(vault: Vault, raw_stream: TcpStream, addr: SocketAddr) {
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
        state: PeerState::Unknown(Instant::now()),
        client_info: None,
    };
    //peer_map.lock().unwrap().insert(addr, peer);
    if let Err(_) = vault.read().await.try_insert_peer(addr, peer).await {
        error!("Duplicate address insert \"{}\"", addr);
    }

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.fold((&addr, &vault), |arg, msg| async move {
        let msg = msg.unwrap();
        let pdu = serde_json::from_str::<Pdu>(msg.to_text().unwrap());
        debug!(
            "Received raw message from {}: \"{}\"",
            addr,
            msg.to_text().unwrap()
        );
        match pdu {
            Ok(p) => {
                debug!("Parsed pdu: {:?}", p);
                if let Err(e) = process_msg(&p, arg.1, arg.0).await {
                    error!("Error while process_msg() {}", e);
                }
            }
            Err(e) => {
                error!(
                    "Parsing received message from peer {} failed with message \"{}\"",
                    addr, e
                );
            }
        }
        arg
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    debug!("{} disconnected", &addr);
    vault.read().await.remove_peer(&addr).await;
}

async fn move_call_dispatch(
    vault: Vault,
    mut move_received: UnboundedReceiver<()>,
    game_id: u64,
) -> Result<()> {
    let mut player_time_remaining = Duration::from_secs(0);

    // after GS_INIT_PAUSE broadcast first update
    {
        tokio::time::sleep(GS_INIT_PAUSE).await;

        let lock = vault.write().await;
        let games_lock = lock.get_games().await;
        let game = games_lock
            .get(&game_id)
            .context("game_session game lookup failed")?;
        let mut game_lock = game.lock().await;

        let first_moved_player = game_lock.next_moved_player_mut().unwrap();

        let call = Pdu::GameSession(GameSession::Update(Update {
            move_call: MoveCall::Call {
                player: first_moved_player.color.clone().to_string(),
                timer: PLAYER_TIMER.as_secs(),
                timer_2: PLAYER_TIME_2.as_secs(),
            },
            move_previous: Move::NoMove {},
            players_states: PlayersStates {
                red: proto::PlayerState::NoState {},
                blue: proto::PlayerState::NoState {},
                yellow: proto::PlayerState::NoState {},
                green: proto::PlayerState::NoState {},
            },
        }))
        .to_message()?;

        player_time_remaining = first_moved_player.time_remaining;

        game_lock.who_move = Some(WhoMove {
            color: first_moved_player.color.clone(),
            since: tokio::time::Instant::now(),
            complete: None,
        });

        game_lock.broadcast(call).await;
    }

    // Process player move and timeout
    loop {
        let move_timeout = tokio::time::sleep(player_time_remaining + PLAYER_TIME_2);
        pin_mut!(move_timeout);

        // left move timeout, right receive move message
        let branch = future::select(move_timeout, move_received.next()).await;
        {
            let lock = vault.write().await;
            let games_lock = lock.get_games().await;
            let game = games_lock
                .get(&game_id)
                .context("game_session game lookup failed")?;
            let mut game_lock = game.lock().await;

            let mut move_previous = Move::NoMove {};
            match branch {
                // when timeout
                Either::Left(_) => {
                    //let who_move = game_lock.who_move.as_ref().unwrap();
                    //let color = game_lock.who_move.as_ref().unwrap().color.clone();
                    /* This block prevent situation when
                    process_move_make receive move message
                    process_move_make lock game mutex
                    process_move_make send message over channel
                    move_call_dispatch select move_timeout
                    move_call_dispatch wait lock game mutex
                    process_move_make release lock
                    move_call_dispatch lock game mutex
                    move_call_dispatch loop to next iteration
                        and get move_received from past turn */
                    if game_lock.who_move.as_ref().unwrap().complete.is_some() {
                        // important next!
                        move_received.next().await;
                        let mv = game_lock
                            .who_move
                            .as_ref()
                            .unwrap()
                            .complete
                            .as_ref()
                            .unwrap()
                            .mv
                            .clone();
                        game_lock.apply_move(&mv);
                        move_previous = mv;
                        //TODO: process move
                    } else {
                        let player = game_lock.current_move_player_mut().unwrap();
                        player.state = PlayerState::Lost;
                        player.time_remaining = Duration::from_secs(0);
                    }
                }
                // when move received
                Either::Right(_) => {
                    let mv = game_lock
                        .who_move
                        .as_ref()
                        .unwrap()
                        .complete
                        .as_ref()
                        .unwrap()
                        .mv
                        .clone();
                    game_lock.apply_move(&mv);
                    move_previous = mv;
                }
            }

            let mut move_call = MoveCall::NoCall {};

            // find first no lost state player
            // if he checknmate or stalemate, lost him
            while let Some(player) = game_lock.next_moved_player_mut() {
                match player.state {
                    PlayerState::Checkmate | PlayerState::Stalemate | PlayerState::Lost => {
                        player.state = PlayerState::Lost
                    }

                    PlayerState::NoState | PlayerState::Check => {
                        player_time_remaining = player.time_remaining;
                        move_call = MoveCall::Call {
                            player: player.color.clone().to_string(),
                            timer: player.time_remaining.as_secs(),
                            timer_2: PLAYER_TIME_2.as_secs(),
                        };
                        game_lock.who_move = Some(WhoMove {
                            color: player.color.clone(),
                            since: tokio::time::Instant::now(),
                            complete: None,
                        });
                        break;
                    }
                }
            }

            let players_states = PlayersStates {
                red: game_lock.player(&Color::Red).state.clone().into(),
                blue: game_lock.player(&Color::Blue).state.clone().into(),
                yellow: game_lock.player(&Color::Yellow).state.clone().into(),
                green: game_lock.player(&Color::Green).state.clone().into(),
            };

            let update = Pdu::GameSession(GameSession::Update(Update {
                move_call: move_call.clone(),
                move_previous,
                players_states,
            }))
            .to_message()?;

            game_lock.broadcast(update).await?;

            if move_call.is_no_call() {
                game_lock.who_move = None;
                break;
            }
        }

        /*if game_lock.next_moved_player_mut().is_none() {
            break;
        }*/

        // if player timeout
        /*if let Either::Right(b) = branch {
            let b = b.clone();
            move_received.close();
            let player = game_lock.current_move_player_mut();
            println!("{:?}", b);
            //player
        }*/

        //println!("{:?}", branch);
    }

    Ok(())
}

/*async fn move_call_dispatch(
    vault: Vault,
    game_id: u64,
    player_color: Color,
    timeout: Duration,
) -> Result<()> {
    tokio::time::sleep(timeout).await;

    let lock = vault.write().await;
    let games_lock = lock.get_games().await;
    let game = games_lock
        .get(&game_id)
        .context("game_session game lookup failed")?;
    let mut game_lock = game.lock().await;

    let players = game_lock.players_mut();

    let lost_pdu = Pdu::GameSession(proto::GameSession::Lost {
        player: player_color.to_string(),
        description: "time over".to_string(),
    })
    .to_message()?;

    for player in players {
        if player_color == player.color {
            player.state = PlayerState::Lost;
            player.time_remaining = Duration::from_secs(0);
        }
        player
            .peer
            .lock()
            .await
            .tx
            .unbounded_send(lost_pdu.clone())?;
    }
    Ok(())
}*/

// Looping infinitely. On loop tick, if we find at least 4 MMQueue players, send HeartbeatCheck
// Also, kick (send kick pdu and change state to Idle) players, who did not response on HeartbeatCheck
// Also, change state HearbeatReady => MMQueue if timeout
// TODO: Disconnect Idle players?
async fn matchmaking_dispatcher(vault: Vault) {
    let mut interval = time::interval(HB_DISP_TICK_PERIOD);

    let heartbeat_pdu = Pdu::MatchmakingQueue(MatchmakingQueue::HeartbeatCheck {})
        .to_message()
        .unwrap();
    let kick_pdu = Pdu::MatchmakingQueue(MatchmakingQueue::PlayerKick {
        discritpion: "Heartbeat timeout".to_string(),
    })
    .to_message()
    .unwrap();

    //Err::<(),()>(()).unwrap();
    let mut game_id = 0;

    loop {
        interval.tick().await;
        let start = Instant::now();

        let lock = vault.write().await;

        // MMQueue => HeartbeatWait
        // Send heartbeat to every 4 players which in MMQueue state
        {
            let mm_queue_lock = lock.get_mm_queue().await;
            let mut hb_wait_lock = lock.get_hb_wait().await;
            let mut tmp_peers = Vec::new();
            for (key, peer) in mm_queue_lock.iter() {
                let peer_lock = peer.lock().await;
                if peer_lock.state.is_mm_queue() {
                    tmp_peers.push((key, peer.clone(), peer_lock));
                    if tmp_peers.len() == 4 {
                        let now = Instant::now();
                        for tmp_peer in &mut tmp_peers {
                            match tmp_peer.2.tx.unbounded_send(heartbeat_pdu.clone()) {
                                Ok(_) => {
                                    tmp_peer.2.state = PeerState::HeartbeatWait(now);
                                    hb_wait_lock.insert(*tmp_peer.0, tmp_peer.1.clone());
                                }
                                Err(e) => error!("unbounded_send failed \"{}\"", e),
                            }
                        }
                        tmp_peers.clear();
                    }
                }
            }
        }

        // HeartbeatWait => Idle
        // Drop to Idle state all players who timeout their HeartbeatWait state
        {
            //let lock = peers.write().await;
            let now = Instant::now();
            let hb_wait_lock = lock.get_hb_wait().await;
            let mut idle = lock.get_idle().await;
            for (key, peer) in hb_wait_lock.iter() {
                let mut peer_lock = peer.lock().await;
                match peer_lock.state.get_hb_wait_since() {
                    Some(hb_wait_since) => {
                        let wait_time = now.duration_since(hb_wait_since);
                        if wait_time > HB_WAIT_TIMEOUT {
                            match peer_lock.tx.unbounded_send(kick_pdu.clone()) {
                                Ok(_) => {
                                    peer_lock.state = PeerState::Idle;
                                    peer_lock.player_name = None;
                                    idle.insert(*key, peer.clone());
                                }
                                Err(e) => error!("unbounded_send failed \"{}\"", e),
                            }
                        }
                    }
                    None => (),
                }
            }
        }

        // HeartbeatReady => MMQueue if timeout
        // This require coz group of four player may not get ready
        // for a long time due to other players leave by HeartbeatWait timeout.
        {
            let now = Instant::now();
            let hb_ready_lock = lock.get_hb_ready().await;
            let mut mm_queue_lock = lock.get_mm_queue().await;
            for (key, peer) in hb_ready_lock.iter() {
                let mut peer_lock = peer.lock().await;
                match peer_lock.state.get_hb_ready_since() {
                    Some(hb_ready_since) => {
                        let wait_time = now.duration_since(hb_ready_since);
                        if wait_time > HB_READY_TIMEOUT {
                            peer_lock.state = PeerState::MMQueue;
                            mm_queue_lock.insert(*key, peer.clone());
                        }
                    }
                    None => (),
                }
            }
        }

        // Now create GameSession form the HeartbeatReady players and broadcast init
        {
            let hb_ready_lock = lock.get_hb_ready().await;
            let mut games_lock = lock.get_games().await;
            let mut reconnect_lock = lock.get_reconnect().await;
            let mut tmp_peers = Vec::new();
            for (key, peer) in hb_ready_lock.iter() {
                let peer_lock = peer.lock().await;
                if peer_lock.state.is_hb_ready() {
                    tmp_peers.push((key, peer.clone(), peer_lock));
                    if tmp_peers.len() == 4 {
                        let mut iter = tmp_peers.iter_mut();
                        let red = iter.next().unwrap();
                        let blue = iter.next().unwrap();
                        let yellow = iter.next().unwrap();
                        let green = iter.next().unwrap();

                        // TODO: check unique
                        let red_reconnect_id = random_string();
                        let blue_reconnect_id = random_string();
                        let yellow_reconnect_id = random_string();
                        let green_reconnect_id = random_string();

                        let (sender, receiver) = unbounded();

                        let game = Arc::new(Mutex::new(Game {
                            id: game_id,
                            board: Board::new(),
                            red: Player {
                                color: Color::Red,
                                reconnect_id: red_reconnect_id.clone(),
                                time_remaining: PLAYER_TIMER,
                                state: PlayerState::NoState,
                                peer: red.1.clone(),
                            },
                            blue: Player {
                                color: Color::Blue,
                                reconnect_id: blue_reconnect_id.clone(),
                                time_remaining: PLAYER_TIMER,
                                state: PlayerState::NoState,
                                peer: blue.1.clone(),
                            },
                            yellow: Player {
                                color: Color::Yellow,
                                reconnect_id: yellow_reconnect_id.clone(),
                                time_remaining: PLAYER_TIMER,
                                state: PlayerState::NoState,
                                peer: yellow.1.clone(),
                            },
                            green: Player {
                                color: Color::Green,
                                reconnect_id: green_reconnect_id.clone(),
                                time_remaining: PLAYER_TIMER,
                                state: PlayerState::NoState,
                                peer: green.1.clone(),
                            },
                            who_move: None,
                            move_happen_signal: sender,
                        }));

                        games_lock.insert(game_id, game.clone());
                        reconnect_lock.insert(red_reconnect_id.clone(), game.clone());
                        reconnect_lock.insert(blue_reconnect_id.clone(), game.clone());
                        reconnect_lock.insert(yellow_reconnect_id.clone(), game.clone());
                        reconnect_lock.insert(green_reconnect_id.clone(), game.clone());

                        red.2.state = PeerState::Game {
                            color: Color::Red,
                            game: game.clone(),
                        };
                        blue.2.state = PeerState::Game {
                            color: Color::Blue,
                            game: game.clone(),
                        };
                        yellow.2.state = PeerState::Game {
                            color: Color::Yellow,
                            game: game.clone(),
                        };
                        green.2.state = PeerState::Game {
                            color: Color::Green,
                            game: game.clone(),
                        };

                        let red_name = red.2.player_name.clone().unwrap();
                        let blue_name = blue.2.player_name.clone().unwrap();
                        let yellow_name = red.2.player_name.clone().unwrap();
                        let green_name = green.2.player_name.clone().unwrap();

                        let red_pdu = game_init_pdu!(
                            GS_INIT_PAUSE.as_secs(),
                            red_reconnect_id,
                            red_name.clone(),
                            green_name.clone(),
                            blue_name.clone(),
                            yellow_name.clone()
                        )
                        .unwrap();
                        let blue_pdu = game_init_pdu!(
                            GS_INIT_PAUSE.as_secs(),
                            blue_reconnect_id,
                            red_name.clone(),
                            green_name.clone(),
                            blue_name.clone(),
                            yellow_name.clone()
                        )
                        .unwrap();
                        let yellow_pdu = game_init_pdu!(
                            GS_INIT_PAUSE.as_secs(),
                            yellow_reconnect_id,
                            red_name.clone(),
                            green_name.clone(),
                            blue_name.clone(),
                            yellow_name.clone()
                        )
                        .unwrap();
                        let green_pdu = game_init_pdu!(
                            GS_INIT_PAUSE.as_secs(),
                            green_reconnect_id,
                            red_name.clone(),
                            green_name.clone(),
                            blue_name.clone(),
                            yellow_name.clone()
                        )
                        .unwrap();

                        for (peer, pdu) in [
                            (&red.2, red_pdu),
                            (&blue.2, blue_pdu),
                            (&yellow.2, yellow_pdu),
                            (&green.2, green_pdu),
                        ]
                        .iter()
                        {
                            match peer.tx.unbounded_send(pdu.clone()) {
                                Ok(_) => (),
                                Err(e) => error!("unbounded_send failed \"{}\"", e),
                            }
                        }

                        tokio::spawn(move_call_dispatch(vault.clone(), receiver, game_id));

                        game_id = game_id.wrapping_add(1);
                        tmp_peers.clear();
                    }
                }
            }
        }
        debug!(
            "peers:{},  idle:{},  mm_queue:{},  hb_wait:{},  hb_ready:{},  reconnect:{},  tick:{:?}",
            lock.get_peers().await.len(),
            lock.get_idle().await.len(),
            lock.get_mm_queue().await.len(),
            lock.get_hb_wait().await.len(),
            lock.get_hb_ready().await.len(),
            lock.get_reconnect().await.len(),
            Instant::now().duration_since(start)
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), IoError> {
    let mut builder = Builder::new();
    builder.filter(Some("server_rs"), LevelFilter::Debug).init();

    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:8080".to_string());

    let vault = Arc::new(RwLock::new(vault::Vault::new()));

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", addr);

    tokio::spawn(matchmaking_dispatcher(vault.clone()));

    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(vault.clone(), stream, addr));
    }

    Ok(())
}
