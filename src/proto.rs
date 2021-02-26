use serde::{Deserialize, Serialize};

// Handshake //////////////////////////////////
#[serde(rename_all = "snake_case")]
#[derive(Debug, Serialize, Deserialize)]
pub enum Protocol {
    SupportedVersion(Vec<String>),
    Version(String),
}

#[serde(rename_all = "snake_case")]
#[derive(Debug, Serialize, Deserialize)]
pub enum GetInfoError {
    UnspecifiedError { description: String },
}

#[serde(rename_all = "snake_case")]
#[derive(Debug, Serialize, Deserialize)]
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

#[serde(rename_all = "snake_case")]
#[derive(Debug, Serialize, Deserialize)]
pub enum ConnectError {
    UnsupportedProtocolVersion { description: String },
    UnspecifiedError { description: String },
}

#[serde(rename_all = "snake_case")]
#[derive(Debug, Serialize, Deserialize)]
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

#[serde(rename_all = "snake_case")]
#[derive(Debug, Serialize, Deserialize)]
pub enum Handshake {
    GetInfo(GetInfo),
    Connect(Connect),
}

// MatchmakingQueue ///////////////////////////
#[serde(rename_all = "snake_case")]
#[derive(Debug, Serialize, Deserialize)]
pub enum PlayerRegisterError {
    BadName { description: String },
    AlreadyRegistered { description: String },
    QueueIsFull { description: String },
    UnspecifiedError { description: String },
}

#[serde(rename_all = "snake_case")]
#[derive(Debug, Serialize, Deserialize)]
pub enum PlayerRegister {
    Name(String),
    Ok { id: u8 },
    Error(PlayerRegisterError),
}

#[serde(rename_all = "snake_case")]
#[derive(Debug, Serialize, Deserialize)]
pub enum MatchmakingQueue {
    PlayerRegister(PlayerRegister),
    PlayerLeave {},
}

#[serde(rename_all = "snake_case")]
#[derive(Debug, Serialize, Deserialize)]
pub enum Pdu {
    Handshake(Handshake),
    MatchmakingQueue(MatchmakingQueue),
}