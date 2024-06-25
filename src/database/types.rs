use sled::InlineArray;
use tokio::sync::{
    mpsc,
    oneshot,
};

/// Channel for sending requests to the database thread
///
/// The enclosing struct contains the request and a oneshot sender
/// for sending back a response.
pub type RequestBus<K, V> = mpsc::UnboundedSender<DbRequest<K, V>>;
pub type RequestSender<V> = oneshot::Sender<Option<V>>;
pub type RequestReceiver<V> = oneshot::Receiver<Option<V>>;

/// Specifies if we are reading or writing to the DB.
#[derive(Debug)]
pub enum RequestKind<K, V>
where
    K: AsRef<[u8]>,
    V: Into<InlineArray>,
{
    Read(K),
    Write(K, V),
    Batch(sled::Batch),
}

/// Contains data to be sent to the DB thread for processing.
#[derive(Debug)]
pub struct DbRequest<K, V>
where
    K: AsRef<[u8]>,
    V: Into<InlineArray>,
{
    pub request: RequestKind<K, V>,
    pub sender: RequestSender<V>,
}

impl<K, V> DbRequest<K, V>
where
    K: AsRef<[u8]>,
    V: Into<InlineArray>,
{
    pub fn new(request: RequestKind<K, V>, sender: RequestSender<V>) -> Self {
        DbRequest { request, sender }
    }
}
