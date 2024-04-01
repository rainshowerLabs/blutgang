use sled::Db;

use tokio::{
	sync::{
		oneshot,
		mpsc,
	},
};

use serde_json::Value;

/// Channel for sending requests to the database thread
///
/// The enclosing struct contains the request and a oneshot sender
/// for sending back a response.
pub type RequestBus = mpsc::UnboundedSender<DbRequest>;
pub type RequestSender = oneshot::Sender<Value>;

pub type Error = Box<dyn std::error::Error>;

/// Specifies if the request we're sending is intended to be cached
/// of if we're sending a new request to the DB.
#[derive(Debug)]
enum RequestKind {
	UserRequest(Value),
	Cache(Value),
}

/// Contains data to be sent to the DB thread for processing.
#[derive(Debug)]
pub struct DbRequest {
	request: RequestKind,
	sender: RequestSender,
}

fn process_incoming(
	incoming: DbRequest,
	db: &Db,
) -> Result<(), Error> {

	Ok(())
}

/// Processes incoming requests from clients and return responses
pub async fn database_processing(
	db: Db,
	mut rax: mpsc::UnboundedReceiver<DbRequest>,
) {
    loop {
        while let Some(incoming) = rax.recv().await {
        	process_incoming(incoming, &db);
        }
    }
}
