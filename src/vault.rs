use crate::board::{Board, Column, Figure, Line, Position, Row, CASTLING_PATTERNS};
use crate::proto::{Move, MoveError};
use anyhow::{Context, Result};
use futures::channel::mpsc::UnboundedSender;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, MutexGuard};
use tokio::task::JoinHandle;
use tungstenite::protocol::Message;

type Tx = UnboundedSender<Message>;
type PeerMap = HashMap<SocketAddr, Arc<Mutex<Peer>>>;
type GameMap = HashMap<u64, Arc<Mutex<Game>>>;
type ReconnectMap = HashMap<String, Arc<Mutex<Game>>>;

pub enum PeerState {
    Unknown(Instant),
    Idle,
    MMQueue,
    HeartbeatWait(Instant),
    HeartbeatReady(Instant),
    Game {
        color: Color,
        game: Arc<Mutex<Game>>,
    },
}

impl PeerState {
    pub fn is_unknown(&self) -> bool {
        matches!(self, PeerState::Unknown(_))
    }
    pub fn is_mm_queue(&self) -> bool {
        matches!(self, PeerState::MMQueue)
    }
    pub fn is_hb_wait(&self) -> bool {
        matches!(self, PeerState::HeartbeatWait(_))
    }
    pub fn get_hb_wait_since(&self) -> Option<Instant> {
        match self {
            PeerState::HeartbeatWait(i) => Some(*i),
            _ => None,
        }
    }
    pub fn is_hb_ready(&self) -> bool {
        matches!(self, PeerState::HeartbeatReady(_))
    }
    pub fn get_hb_ready_since(&self) -> Option<Instant> {
        match self {
            PeerState::HeartbeatReady(i) => Some(*i),
            _ => None,
        }
    }
    pub fn is_game(&self) -> bool {
        matches!(self, PeerState::Game { .. })
    }
}

pub struct ClientInfo {
    pub name: String,
    pub version: String,
    pub protocol: String,
}

pub struct Peer {
    pub tx: Tx,
    pub player_name: Option<String>,
    pub state: PeerState,
    pub client_info: Option<ClientInfo>,
}

pub struct Vault {
    peers: Mutex<PeerMap>,
    idle: Mutex<PeerMap>,
    mm_queue: Mutex<PeerMap>,
    hb_wait: Mutex<PeerMap>,
    hb_ready: Mutex<PeerMap>,
    games: Mutex<GameMap>,
    reconnect: Mutex<ReconnectMap>,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Color {
    Red,
    Green,
    Blue,
    Yellow,
}

impl ToString for Color {
    fn to_string(&self) -> String {
        match self {
            Color::Red => String::from("Red"),
            Color::Green => String::from("Green"),
            Color::Blue => String::from("Blue"),
            Color::Yellow => String::from("Yellow"),
        }
    }
}

#[derive(PartialEq, Clone)]
pub enum PlayerState {
    NoState,
    Check,
    Checkmate,
    Stalemate,
    Lost,
}

impl PlayerState {
    /*pub fn is_idle(&self) -> bool {
        matches!(self, PlayerState::Idle)
    }*/
}

pub struct Player {
    pub color: Color,
    pub reconnect_id: String,
    pub time_remaining: Duration,
    pub state: PlayerState,
    pub peer: Arc<Mutex<Peer>>,
}

pub struct Complete {
    pub mv: Move,
    pub at: tokio::time::Instant,
}

pub struct WhoMove {
    pub color: Color,
    pub since: tokio::time::Instant,
    pub complete: Option<Complete>,
}

pub struct Game {
    pub id: u64,
    pub board: Board,
    pub red: Player,
    pub green: Player,
    pub blue: Player,
    pub yellow: Player,
    pub who_move: Option<WhoMove>,
    pub move_happen_signal: UnboundedSender<()>,
}

impl Game {
    pub fn players(&self) -> Vec<&Player> {
        vec![&self.red, &self.green, &self.blue, &self.yellow]
    }
    pub fn players_mut(&mut self) -> Vec<&mut Player> {
        vec![
            &mut self.red,
            &mut self.green,
            &mut self.blue,
            &mut self.yellow,
        ]
    }
    pub fn player(&self, color: &Color) -> &Player {
        match color {
            Color::Red => &self.red,
            Color::Green => &self.green,
            Color::Blue => &self.blue,
            Color::Yellow => &self.yellow,
        }
    }
    pub fn player_mut(&mut self, color: &Color) -> &mut Player {
        match color {
            Color::Red => &mut self.red,
            Color::Green => &mut self.green,
            Color::Blue => &mut self.blue,
            Color::Yellow => &mut self.yellow,
        }
    }

    fn next_moved_player_inner(&mut self, check_seq: &[Color]) -> Option<&mut Player> {
        for color in check_seq {
            // let mut player = self.player_mut(color);
            match self.player_mut(color).state {
                PlayerState::NoState
                | PlayerState::Check
                | PlayerState::Checkmate
                | PlayerState::Stalemate => return Some(self.player_mut(color)),
                PlayerState::Lost => continue,
            }
        }
        None
    }

    pub fn next_moved_player_mut(&mut self) -> Option<&mut Player> {
        let no_lost_state_players_count = self
            .players()
            .iter()
            .filter(|p| p.state != PlayerState::Lost)
            .count();

        return match &self.who_move {
            Some(wm) => {
                if no_lost_state_players_count > 1 {
                    match wm.color {
                        Color::Red => {
                            let check_seq = [Color::Blue, Color::Yellow, Color::Green];
                            self.next_moved_player_inner(&check_seq)
                        }
                        Color::Blue => {
                            let check_seq = [Color::Yellow, Color::Green, Color::Red];
                            self.next_moved_player_inner(&check_seq)
                        }
                        Color::Yellow => {
                            let check_seq = [Color::Green, Color::Red, Color::Blue];
                            self.next_moved_player_inner(&check_seq)
                        }
                        Color::Green => {
                            let check_seq = [Color::Red, Color::Blue, Color::Yellow];
                            self.next_moved_player_inner(&check_seq)
                        }
                    }
                } else {
                    None
                }
            }
            None => {
                if no_lost_state_players_count == 4 {
                    Some(self.player_mut(&Color::Red))
                } else {
                    None
                }
            }
        };
    }

    pub async fn broadcast(&self, message: Message) -> Result<()> {
        for player in self.players() {
            player
                .peer
                .lock()
                .await
                .tx
                .unbounded_send(message.clone())?
        }
        Ok(())
    }

    pub fn current_move_player(&self) -> Option<&Player> {
        let color = self.who_move.as_ref()?.color.clone();
        Some(self.player(&color))
    }

    pub fn current_move_player_mut(&mut self) -> Option<&mut Player> {
        let color = self.who_move.as_ref()?.color.clone();
        Some(self.player_mut(&color))
    }

    pub fn validate_player_move(&self, mv: &Move, color: &Color) -> bool {
        if let Some(wm) = &self.who_move {
            if wm.color == *color {
                return true;
            }
        }
        false
    }

    pub fn validate_move(&self, mv: &Move) -> Result<(), MoveError> {
        Ok(())
    }

    fn apply_castling(&mut self, rook_pos: Position) -> Result<(), MoveError> {
        let rook = self.board.piece(rook_pos);
        if rook.is_none() {
            return Err(MoveError::ForbiddenMove {
                description: "empty rook cell".to_string(),
            });
        }

        let rook = rook.unwrap();
        if rook.already_move() {
            return Err(MoveError::ForbiddenMove {
                description: "rook already move".to_string(),
            });
        }

        let king = self.board.find_king(rook.color);
        if king.is_none() {
            return Err(MoveError::ForbiddenMove {
                description: "empty king cell".to_string(),
            });
        }

        let (king, king_pos) = king.unwrap().piece_pos();
        if king.already_move() {
            return Err(MoveError::ForbiddenMove {
                description: "king already move".to_string(),
            });
        }

        let castling_pattern = CASTLING_PATTERNS.get(&(rook_pos, king_pos)).unwrap();

        if castling_pattern
            .space_between
            .iter()
            .any(|pos| self.board.piece(*pos).is_some())
        {
            return Err(MoveError::ForbiddenMove {
                description: "cells between rook and king not empty".to_string(),
            });
        }

        let current_move_player = self.current_move_player().unwrap();
        if current_move_player.state == PlayerState::Check {
            return Err(MoveError::ForbiddenMove {
                description: "player under check".to_string(),
            });
        }

        let king_path_attackers = castling_pattern
            .king_path
            .iter()
            .map(|path_pos| self.board.attackers_on_position(*path_pos))
            .filter(|attackers| attackers.is_some())
            .map(|attackers| attackers.unwrap())
            .flatten()
            .filter(|attacker| attacker.piece().color != current_move_player.color);

        if king_path_attackers.count() > 0 {
            return Err(MoveError::ForbiddenMove {
                description: "king castling path is under attack".to_string(),
            });
        }

        self.board
            .piece_move(rook_pos, castling_pattern.rook_end_pos);
        self.board
            .piece_move(rook_pos, castling_pattern.rook_end_pos);
        Ok(())
    }

    fn apply_capture(&self, from: Position, to: Position) -> Result<(), MoveError> {
        Ok(())
    }

    pub fn apply_move(&mut self, mv: &Move) -> Result<(), MoveError> {
        match mv {
            Move::Basic { from, to } => {}
            Move::Capture { from, to } => {
                return self.apply_capture(*from, *to);
            }
            Move::Castling { rook } => {
                return self.apply_castling(*rook);
            }
            Move::Promotion { from, to, into } => {}
            Move::NoMove {} | Move::Error(_) => (),
        }
        Ok(())
    }
}

/*impl Peer {
    pub fn set_state(&self, state: PeerState) {}
}*/

impl<'a> Vault {
    pub fn new() -> Vault {
        Vault {
            peers: Mutex::new(PeerMap::new()),
            idle: Mutex::new(PeerMap::new()),
            mm_queue: Mutex::new(PeerMap::new()),
            hb_wait: Mutex::new(PeerMap::new()),
            hb_ready: Mutex::new(PeerMap::new()),
            games: Mutex::new(GameMap::new()),
            reconnect: Mutex::new(ReconnectMap::new()),
        }
    }
    pub async fn try_insert_peer(&self, sock_addr: SocketAddr, peer: Peer) -> Result<(), ()> {
        let mut peers = self.peers.lock().await;
        match peers.contains_key(&sock_addr) {
            true => Err(()),
            false => {
                peers.insert(sock_addr, Arc::new(Mutex::new(peer)));
                Ok(())
            }
        }
    }

    pub async fn remove_peer(&self, sock_addr: &SocketAddr) {
        let mut peers = self.peers.lock().await;
        if let Some(peer) = peers.remove(sock_addr) {
            // change state to Unknown, gc will clean it later
            peer.lock().await.state = PeerState::Unknown(Instant::now())
        }
    }

    pub async fn get_peers(&'a self) -> MutexGuard<'a, PeerMap> {
        self.peers.lock().await
    }
    pub async fn get_idle(&'a self) -> MutexGuard<'a, PeerMap> {
        self.idle.lock().await
    }
    pub async fn get_mm_queue(&'a self) -> MutexGuard<'a, PeerMap> {
        self.mm_queue.lock().await
    }
    pub async fn get_hb_wait(&'a self) -> MutexGuard<'a, PeerMap> {
        self.hb_wait.lock().await
    }
    pub async fn get_hb_ready(&'a self) -> MutexGuard<'a, PeerMap> {
        self.hb_ready.lock().await
    }
    pub async fn get_games(&'a self) -> MutexGuard<'a, GameMap> {
        self.games.lock().await
    }
    pub async fn get_reconnect(&'a self) -> MutexGuard<'a, ReconnectMap> {
        self.reconnect.lock().await
    }
}
