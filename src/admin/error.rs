// Errors
use std::error::Error;

#[derive(Debug)]
#[allow(dead_code)]
pub enum AdminError {
    InvalidMethod,
    InvalidParams,
    InvalidLen,
    ParseError,
    WriteProtectionEnabled,
    InvalidSecret,
    RwError,
    Inaccessible,
    OutOfBounds,
    InvalidResponse(String),
}

impl std::fmt::Display for AdminError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AdminError::InvalidMethod => write!(f, "Requested method does not exist"),
            AdminError::InvalidParams => write!(f, "Invalid params provided"),
            AdminError::InvalidLen => write!(f, "Mismatching params length"),
            AdminError::ParseError => write!(f, "Could not parse supplied params"),
            AdminError::WriteProtectionEnabled => write!(f, "Admin namespace is set to read-only"),
            AdminError::InvalidSecret => write!(f, "Invalid secret for protected method"),
            AdminError::RwError => write!(f, "Error while trying to read or write fromt/to disk"),
            AdminError::Inaccessible => write!(f, "Could not access shared resource."),
            AdminError::OutOfBounds => {
                write!(f, "Request out of bounds.")
            }
            AdminError::InvalidResponse(reason) => write!(f, "Invalid RPC response: {}", reason),
        }
    }
}

impl Error for AdminError {}
