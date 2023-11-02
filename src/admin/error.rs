// Errors
use std::error::Error;

#[derive(Debug)]
#[allow(dead_code)]
pub enum AdminError {
    InvalidMethod,
    InvalidSecret,
    RwError,
    Innacessible,
    OutOfBounds,
    InvalidResponse(String),
}

impl std::fmt::Display for AdminError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AdminError::InvalidMethod => write!(f, "Requested method does not exist."),
            AdminError::InvalidSecret => write!(f, "Invalid secret for protected method."),
            AdminError::RwError => write!(f, "Error while trying to read or write fromt/to disk."),
            AdminError::Innacessible => write!(f, "Could not access shared resource."),
            AdminError::OutOfBounds => {
                write!(f, "Request out of bounds.")
            }
            AdminError::InvalidResponse(reason) => write!(f, "Invalid RPC response: {}", reason),
        }
    }
}

impl Error for AdminError {}
