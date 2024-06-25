use sled::{
    Db,
    InlineArray,
};

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
enum RequestKind {
    Read(String),
    Write(String, String),
}

/// Contains data to be sent to the DB thread for processing.
#[derive(Debug)]
pub struct DbRequest {
    request: RequestKind,
    sender: RequestSender,
}

/// Processes incoming requests from clients and return responses
pub async fn database_processing(mut rax: mpsc::UnboundedReceiver<DbRequest>, cache: Db) {
    loop {
        while let Some(incoming) = rax.recv().await {
            let rax = match incoming.request {
                RequestKind::Read(k) => cache.get(k),
                RequestKind::Write(k, v) => cache.insert(k, v),
            };

            // Unwrap rax as None so its as if we didnt get anything from the DB
            let rax = rax.unwrap().or(None);

            incoming.sender.send(rax);
        }
    }
}
