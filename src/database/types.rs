use sled::InlineArray;
use tokio::sync::{
    mpsc,
    oneshot,
};

/// Channel for sending requests to the database thread
///
/// The enclosing struct contains the request and a oneshot sender
/// for sending back a response.
pub type RequestBus = mpsc::UnboundedSender<DbRequest>;
pub type RequestSender = oneshot::Sender<Option<InlineArray>>;
pub type RequestReceiver = oneshot::Receiver<Option<InlineArray>>;

/// Specifies if we are reading or writing to the DB.
#[derive(Debug)]
pub enum RequestKind
{
    Read(Vec<u8>),
    Write(Vec<u8>, InlineArray),
    Batch(sled::Batch),
}

/// Contains data to be sent to the DB thread for processing.
#[derive(Debug)]
pub struct DbRequest
{
    pub request: RequestKind,
    pub sender: RequestSender,
}

impl DbRequest
{
    pub fn new(request: RequestKind, sender: RequestSender) -> Self {
        DbRequest { request: request.clone(), sender }
    }
}
