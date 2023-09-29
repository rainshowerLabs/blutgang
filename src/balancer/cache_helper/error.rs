// Errors
use std::error::Error;

#[derive(Debug)]
#[allow(dead_code)]
pub enum CacheManagerError {
    NumberParseError,
    CannotRetrieve,
    //InvalidResponse(String),
}

impl std::fmt::Display for CacheManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            CacheManagerError::NumberParseError => write!(f, "Cannot parse number!"),
            CacheManagerError::CannotRetrieve => {
                write!(f, "Cannot retrieve response from DB!")
            }
        }
    }
}

impl Error for CacheManagerError {}
