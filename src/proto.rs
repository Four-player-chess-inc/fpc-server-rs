use crate::board::{Figure, Position};
use crate::proto::MatchmakingQueue::PlayerKick;
use crate::vault;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tungstenite::protocol::Message;
use tungstenite::stream::Mode::Plain;

// Handshake //////////////////////////////////

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    SupportedVersion(Vec<String>),
    Version(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GetInfoError {
    UnspecifiedError { description: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GetInfo {
    Request {},
    Ok { protocol: Protocol },
    Error(GetInfoError),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Server {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectError {
    UnsupportedProtocolVersion { description: String },
    UnspecifiedError { description: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Connect {
    Client {
        name: String,
        version: String,
        protocol: Protocol,
    },
    Ok {
        server: Server,
    },
    Error(ConnectError),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Handshake {
    GetInfo(GetInfo),
    Connect(Connect),
}

// MatchmakingQueue ///////////////////////////
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerRegisterError {
    BadName { description: String },
    AlreadyRegistered { description: String },
    Handshake { description: String },
    UnspecifiedError { description: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerRegister {
    Name(String),
    Ok {},
    Error(PlayerRegisterError),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchmakingQueue {
    PlayerRegister(PlayerRegister),
    PlayerLeave {},
    HeartbeatCheck {},
    PlayerKick { discritpion: String },
}

// GameSession ///////////////////////////
/*#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Position {
    pub column: char,
    pub row: u8,
}*/

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StartPosition {
    pub player_name: String,
    pub left_rook: Position,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StartPositions {
    pub red: StartPosition,
    pub green: StartPosition,
    pub blue: StartPosition,
    pub yellow: StartPosition,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Init {
    pub countdown: u64,
    pub reconnect_id: String,
    pub start_positions: StartPositions,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    NoAction {},
    Capture(Position),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum MoveError {
    ForbiddenMove { description: String },
    UnspecifiedError { description: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Move {
    Basic {
        from: Position,
        to: Position,
    },
    Capture {
        from: Position,
        to: Position,
    },
    Promotion {
        from: Position,
        to: Position,
        into: Figure,
    },
    Castling {
        rook: Position,
    },
    NoMove {},
    Error(MoveError),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameSession {
    Init(Init),
    Move(Move),
    Update(Update),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum MoveCall {
    NoCall {},
    Call {
        player: String,
        timer: u64,
        timer_2: u64,
    },
}

impl MoveCall {
    pub fn is_no_call(&self) -> bool {
        matches!(self, MoveCall::NoCall {})
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemainingPieces {
    Clear,
    TurnToStone,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerState {
    NoState {},
    Check {},
    Checkmate {},
    Stalemate {},
    Lost { remaining_pieces: RemainingPieces },
}

impl From<vault::PlayerState> for PlayerState {
    fn from(vault_state: vault::PlayerState) -> Self {
        match vault_state {
            vault::PlayerState::NoState => PlayerState::NoState {},
            vault::PlayerState::Check => PlayerState::Check {},
            vault::PlayerState::Checkmate => PlayerState::Checkmate {},
            vault::PlayerState::Stalemate => PlayerState::Stalemate {},
            vault::PlayerState::Lost => PlayerState::Lost {
                remaining_pieces: RemainingPieces::Clear,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlayersStates {
    pub red: PlayerState,
    pub blue: PlayerState,
    pub yellow: PlayerState,
    pub green: PlayerState,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Update {
    pub move_call: MoveCall,
    pub move_previous: Move,
    pub players_states: PlayersStates,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pdu {
    Handshake(Handshake),
    MatchmakingQueue(MatchmakingQueue),
    GameSession(GameSession),
}

impl Pdu {
    pub fn to_message(&self) -> Result<Message> {
        let json = serde_json::to_string(self)?;
        Ok(Message::Text(json))
    }
}
