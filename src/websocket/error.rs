//! WebSocket module errors.

use std::fmt;
use tokio::sync::{
    broadcast,
    mpsc,
};

#[derive(Debug)]
pub enum WsError {
    Ws(String),
    Connection(String),
    MessageSendFailed(String),
    MessageReceptionFailed(String),
    ReceiverLagged(),
    ChannelClosed(),
    InvalidData(String),
    NoIdInResponse(String),
    FailedParsing(),
    MissingSubscription(),
    EmptyList(String),
    // SubscriptionError(String),
    // RpcError(String),
    NoWsResponse,
}

impl fmt::Display for WsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            WsError::Ws(msg) => write!(f, "Error in WS: {}", msg),
            WsError::Connection(msg) => write!(f, "Connection Error: {}", msg),
            WsError::MessageSendFailed(msg) => {
                write!(f, "Message Sending Internal Message Failed: {}", msg)
            }
            WsError::MessageReceptionFailed(msg) => {
                write!(f, "Error While Receiving Internal Message: {}", msg)
            }
            WsError::ReceiverLagged() => write!(f, "Receiver Lagged!"),
            WsError::ChannelClosed() => write!(f, "Channel Closed!"),
            WsError::InvalidData(msg) => write!(f, "Invalid Data: {}", msg),
            WsError::NoIdInResponse(msg) => write!(f, "No ID in response: {}", msg),
            WsError::FailedParsing() => write!(f, "Failed to Parse Input Data!"),
            WsError::MissingSubscription() => {
                write!(f, "Tried to Perform Action On Non-Existing Subscription!")
            }
            WsError::EmptyList(msg) => write!(f, "Tried to Access Empty List: {}", msg),
            // Error::SubscriptionError(msg) => write!(f, "Subscription Error: {}", msg),
            // Error::RpcError(msg) => write!(f, "RPC Error: {}", msg),
            WsError::NoWsResponse => write!(f, "Failed to Receive Response from WS"),
        }
    }
}

impl From<tungstenite::Error> for WsError {
    fn from(error: tungstenite::Error) -> Self {
        match error {
            tungstenite::Error::ConnectionClosed => {
                WsError::Connection("Connection closed".to_string())
            }
            tungstenite::Error::AlreadyClosed => {
                WsError::Connection("Connection already closed".to_string())
            }
            tungstenite::Error::Io(ref e) => WsError::Connection(format!("IO Error: {}", e)),
            _ => WsError::Connection(format!("WebSocket error: {:?}", error)),
        }
    }
}

// For handling errors when sending a broadcast message
impl<T: std::fmt::Debug> From<broadcast::error::SendError<T>> for WsError {
    fn from(error: broadcast::error::SendError<T>) -> Self {
        WsError::MessageSendFailed(format!("Failed to broadcast message: {:?}", error))
    }
}

// For handling errors when receiving a broadcast message
impl From<broadcast::error::RecvError> for WsError {
    fn from(error: broadcast::error::RecvError) -> Self {
        match error {
            broadcast::error::RecvError::Lagged(_) => WsError::ReceiverLagged(),
            broadcast::error::RecvError::Closed => WsError::ChannelClosed(),
        }
    }
}

impl<T> From<mpsc::error::SendError<T>> for WsError {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        WsError::MessageSendFailed("Failed to send message through channel".to_string())
    }
}

impl From<&str> for WsError {
    fn from(msg: &str) -> Self {
        WsError::Ws(msg.to_string())
    }
}

impl std::error::Error for WsError {}
