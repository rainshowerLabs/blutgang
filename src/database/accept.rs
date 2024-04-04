use crate::{
	CacheArgs,
	database::processing::cache_querry,
};


use std::sync::Arc;

use hyper::{
    body::Incoming,
    Request,
};

use tokio::{
	sync::{
		oneshot,
		mpsc,
	},
};

use serde_json::Value;

// Select either blake3 or xxhash based on the features
use blake3::{Hash};

#[cfg(feature = "xxhash")]
use xxhash_rust::xxh3::xxh3_64;
#[cfg(feature = "xxhash")]
use zerocopy::AsBytes; // Impls AsBytes trait for u64

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
	UserRequest(Request<Incoming>),
	// TODO: dont be a string plz
	Cache(Hash, Value, String),
}

/// Contains data to be sent to the DB thread for processing.
#[derive(Debug)]
pub struct DbRequest {
	request: RequestKind,
	sender: RequestSender,
}

async fn process_incoming(
	incoming: DbRequest,
	cache_args: Arc<CacheArgs>,
) {
	match incoming.request {
	    RequestKind::UserRequest(incoming) => todo!(),
	    RequestKind::Cache(key, value, mut rx) => {
			cache_querry(value, &mut rx, &key, cache_args)
	    },
	};
}

/// Processes incoming requests from clients and return responses
pub async fn database_processing(
	cache_args: Arc<CacheArgs>,
	mut rax: mpsc::UnboundedReceiver<DbRequest>,
) {
    loop {
        while let Some(incoming) = rax.recv().await {
        	let cache_args_clone = cache_args.clone();
        	tokio::spawn(async move {
	            process_incoming(incoming, cache_args_clone).await;
	        }
        );
        }
    }
}
