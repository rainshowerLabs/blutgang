// Errors
use std::error::Error;

#[derive(Debug)]
pub enum RpcError {
    Unresponsive,
    InvalidResponse(String),
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RpcError::Unresponsive => write!(f, "RPC is unresponsive"),
            RpcError::InvalidResponse(reason) => write!(f, "Invalid RPC response: {}", reason),
        }
    }
}

impl Error for RpcError {}
