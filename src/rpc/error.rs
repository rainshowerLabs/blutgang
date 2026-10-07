//! RPC type errors

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("Invalid RPC response: {0}")]
    InvalidResponse(String),

    #[error("Failed to send message: {0}")]
    SendError(String),

    #[error(transparent)]
    ReqwestError(reqwest::Error),
}

impl From<reqwest::Error> for RpcError {
    /// Drops the URL from the error: RPC URLs often carry an API key, and
    /// these errors end up in logs.
    fn from(error: reqwest::Error) -> Self {
        RpcError::ReqwestError(error.without_url())
    }
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
