//! Admin specific errors

#[derive(Debug, thiserror::Error)]
pub enum AdminError {
    #[error(transparent)]
    InvalidMethod(crate::admin::methods::Error<Option<String>>),
    #[error("Invalid params provided")]
    InvalidParams,
    #[error("Mismatching params length")]
    InvalidLen,
    #[error("Could not parse supplied params")]
    ParseError,
    #[error("Admin namespace is set to read-only")]
    WriteProtectionEnabled,
    #[error("Could not access shared resource")]
    Inaccessible,
    #[error("Request out of bounds")]
    OutOfBounds,
}
