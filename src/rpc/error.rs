//! RPC type errors

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("Invalid RPC response: {0}")]
    InvalidResponse(String),

    #[error("Failed to send message: {0}")]
    SendError(String),

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
}

impl From<simd_json::Error> for RpcError {
    fn from(value: simd_json::Error) -> Self {
        RpcError::InvalidResponse(format!("Error while trying to parse JSON: {value:?}"))
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for RpcError {
    fn from(error: tokio::sync::mpsc::error::SendError<T>) -> Self {
        RpcError::SendError(error.to_string())
    }
}
