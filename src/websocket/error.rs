use std::fmt;
use tokio::sync::{
    broadcast,
    mpsc,
};
use tokio_tungstenite::tungstenite;

#[derive(Debug)]
pub enum Error {
    Ws(String),
    Connection(String),
    MessageSendFailed(String),
    MessageReceptionFailed(String),
    ReceiverLagged(),
    ChannelClosed(),
    InvalidData(String),
    FailedParsing(),
    MissingSubscription(),
    EmptyList(String),
    // SubscriptionError(String),
    // RpcError(String),
    NoWsResponse,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Ws(msg) => write!(f, "Error in WS: {}", msg),
            Error::Connection(msg) => write!(f, "Connection Error: {}", msg),
            Error::MessageSendFailed(msg) => {
                write!(f, "Message Sending Internal Message Failed: {}", msg)
            }
            Error::MessageReceptionFailed(msg) => {
                write!(f, "Error While Receiving Internal Message: {}", msg)
            }
            Error::ReceiverLagged() => write!(f, "Receiver Lagged!"),
            Error::ChannelClosed() => write!(f, "Channel Closed!"),
            Error::InvalidData(msg) => write!(f, "Invalid Data: {}", msg),
            Error::FailedParsing() => write!(f, "Failed to Parse Input Data!"),
            Error::MissingSubscription() => {
                write!(f, "Tried to Perform Action On Non-Existing Subscription!")
            }
            Error::EmptyList(msg) => write!(f, "Tried to Access Empty List: {}", msg),
            // Error::SubscriptionError(msg) => write!(f, "Subscription Error: {}", msg),
            // Error::RpcError(msg) => write!(f, "RPC Error: {}", msg),
            Error::NoWsResponse => write!(f, "Failed to Receive Response from WS"),
        }
    }
}

impl From<tungstenite::Error> for Error {
    fn from(error: tungstenite::Error) -> Self {
        match error {
            tungstenite::Error::ConnectionClosed => {
                Error::Connection("Connection closed".to_string())
            }
            tungstenite::Error::AlreadyClosed => {
                Error::Connection("Connection already closed".to_string())
            }
            tungstenite::Error::Io(ref e) => Error::Connection(format!("IO Error: {}", e)),
            _ => Error::Connection(format!("WebSocket error: {:?}", error)),
        }
    }
}

// For handling errors when sending a broadcast message
impl<T: std::fmt::Debug> From<broadcast::error::SendError<T>> for Error {
    fn from(error: broadcast::error::SendError<T>) -> Self {
        Error::MessageSendFailed(format!("Failed to broadcast message: {:?}", error))
    }
}

// For handling errors when receiving a broadcast message
impl From<broadcast::error::RecvError> for Error {
    fn from(error: broadcast::error::RecvError) -> Self {
        match error {
            broadcast::error::RecvError::Lagged(_) => Error::ReceiverLagged(),
            broadcast::error::RecvError::Closed => Error::ChannelClosed(),
        }
    }
}

impl<T> From<mpsc::error::SendError<T>> for Error {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        Error::MessageSendFailed("Failed to send message through channel".to_string())
    }
}

impl From<&str> for Error {
    fn from(msg: &str) -> Self {
        Error::Ws(msg.to_string())
    }
}

impl std::error::Error for Error {}
