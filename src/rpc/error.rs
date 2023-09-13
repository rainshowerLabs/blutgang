// Errors
use std::error::Error;

#[derive(Debug)]
#[allow(dead_code)]
pub enum RpcError {
    Unresponsive,
    //InvalidHexFormat,
    OutOfBounds,
    InvalidResponse(String),
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RpcError::Unresponsive => write!(f, "RPC is unresponsive"),
            RpcError::OutOfBounds => write!(f, "Request out of bounds. Most likeley an unexpected response from the current RPC node."),
            RpcError::InvalidResponse(reason) => write!(f, "Invalid RPC response: {}", reason),
        }
    }
}

impl Error for RpcError {}
