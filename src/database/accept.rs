use crate::{
    database::types::{
        DbRequest,
        RequestKind,
    },
    log_err,
};

use sled::Db;

use tokio::sync::mpsc;

/// Processes incoming requests from clients and returns responses
pub async fn database_processing(mut rax: mpsc::UnboundedReceiver<DbRequest>, cache: Db) {
    loop {
        while let Some(incoming) = rax.recv().await {
            let result = match incoming.request {
                RequestKind::Read(k) => cache.get(k),
                RequestKind::Write(k, v) => cache.insert(k, v),
                RequestKind::Batch(b) => cache.apply_batch(b).map(|_| None),
            };

            let rax = result.unwrap_or(None);

            if let Err(_) = incoming.sender.send(rax) {
                log_err!("Db failed to send response back!");
            }
        }
    }
}
