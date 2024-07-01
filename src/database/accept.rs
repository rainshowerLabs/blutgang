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
                RequestKind::Flush() => {
                    // TODO: return proper stats!
                    let _ = cache.flush().unwrap();
                    Ok(None)
                }
            };

            if result.is_err() {
                log_err!("Db failed to send response back: {:?}", result);
                let _ = incoming.sender.send(None);
                continue;
            }

            let _ = incoming.sender.send(result.unwrap());
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
        use $crate::database::types::{
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
        $k:expr,
        $v:expr
    ) => {{
        let (tx, rx) = oneshot::channel();
        let req = DbRequest::new(RequestKind::Write($k, $v), tx);

        let _ = $channel.send(req);

        rx
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

/// Macro for flushing the DB
///
/// Returns `Option<InlineArray>`, where the result is `None` if
/// there was an error or data isn't present, or `Some` if the operation
/// completed successfully.
#[macro_export]
macro_rules! db_flush {
    (
        $channel:expr
    ) => {{
        use $crate::database::types::{
            DbRequest,
            RequestKind,
        };

        use tokio::sync::oneshot;

        let (tx, rx) = oneshot::channel();
        let req = DbRequest::new(RequestKind::Flush(), tx);

        let _ = $channel.send(req);

        rx
    }};
}
