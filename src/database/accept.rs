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

/// Macro to abstract getting the data from the DB.
///
/// Returns `Option<InlineArray>`, where the result is `None` if
/// there was an error or data isn't present, or `Some` if the operation
/// completed successfully.
#[macro_export]
macro_rules! db_get {
    (
        $channel:expr,
        $data:expr
    ) => {{
        use crate::database::types::{
            DbRequest,
            RequestKind,
        };

        let (tx, rx) = tokio::sync::oneshot::channel();
        let req = DbRequest::new(RequestKind::Read($data), tx);

        let _ = $channel.send(req);

        rx.await
    }};
}

/// Macro to abstract inserting data into the DB.
///
/// Returns `Option<InlineArray>`, where the result is `None` if
/// there was an error or data isn't present, or `Some` if the operation
/// completed successfully.
#[macro_export]
macro_rules! db_insert {
    (
        $channel:expr,
        $data:expr
    ) => {{
        let (tx, rx) = oneshot::channel();
        let req = DbRequest::new(RequestKind::Write($data), tx);

        let _ = $channel.send(req);

        rx.await
    }};
}

/// Macro to abstract writing batch data to the DB.
///
/// Returns `Option<InlineArray>`, where the result is `None` if
/// there was an error or data isn't present, or `Some` if the operation
/// completed successfully.
#[macro_export]
macro_rules! db_batch {
    (
        $channel:expr,
        $data:expr
    ) => {{
        let (tx, rx) = oneshot::channel();
        let req = DbRequest::new(RequestKind::Batch($data), tx);

        let _ = $channel.send(req);

        rx
    }};
}
