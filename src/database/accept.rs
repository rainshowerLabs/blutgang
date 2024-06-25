use crate::database::types::DbRequest;
use crate::database::types::RequestKind;
use sled::Db;

use tokio::sync::mpsc;

/// Processes incoming requests from clients and return responses
pub async fn database_processing(mut rax: mpsc::UnboundedReceiver<DbRequest>, cache: Db) {
    loop {
        while let Some(incoming) = rax.recv().await {
            let rax = match incoming.request {
                RequestKind::Read(k) => cache.get(k),
                RequestKind::Write(k, v) => cache.insert(k, v),
                RequestKind::Batch(b) => {
                    cache.apply_batch(b);
                    Result::Ok(None)
                }
            };

            // Unwrap rax as None so its as if we didnt get anything from the DB
            let rax = rax.unwrap().or(None);

            incoming.sender.send(rax);
        }
    }
}
