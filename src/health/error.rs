// Errors
use crate::rpc::error::RpcError;
use crate::websocket::error::WsError;
use std::error::Error;

#[derive(Debug)]
#[allow(dead_code)]
pub enum HealthError {
    Unresponsive,
    TimedOut,
    GetSafeBlockError(String),
    //InvalidHexFormat,
    OutOfBounds,
    InvalidResponse(String),
}

impl std::fmt::Display for HealthError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            HealthError::Unresponsive => write!(f, "RPC is unresponsive"),
            HealthError::TimedOut => write!(f, "Health check timed out!"),
            HealthError::GetSafeBlockError(reason) => {
                write!(f, "Could not get safe block: {}", reason)
            }
            HealthError::OutOfBounds => {
                write!(
                    f,
                    "Request out of bounds. Most likeley a bad response from the current RPC node."
                )
            }
            HealthError::InvalidResponse(reason) => write!(f, "Invalid RPC response: {}", reason),
        }
    }
}

impl Error for HealthError {}

impl From<RpcError> for HealthError {
    fn from(error: RpcError) -> Self {
        HealthError::GetSafeBlockError(error.to_string())
    }
}

impl From<WsError> for HealthError {
    fn from(error: WsError) -> Self {
        HealthError::InvalidResponse(error.to_string())
    }
}
