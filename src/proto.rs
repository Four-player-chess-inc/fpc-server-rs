use crate::board::Position;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tungstenite::protocol::Message;

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
pub struct Call {
    pub player: String,
    pub timer: u64,
    pub timer_2: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    NoAction {},
    Capture(Position),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MakeElem {
    from: Position,
    to: Position,
    action: Action,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Move {
    Call(Call),
    Make(Vec<MakeElem>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameSession {
    Init(Init),
    Move(Move),
    Lost { player: String, description: String },
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
