use crate::database::types::{
    Batch,
    DbRequest,
    GenericBytes,
    GenericDatabase,
    RequestKind,
};
use tokio::sync::{
    mpsc::UnboundedSender,
    oneshot::{
        self,
        Receiver,
    },
};

/// Processes incoming requests from clients and returns responses
pub async fn database_processing<K, V, DB>(
    mut rax: tokio::sync::mpsc::UnboundedReceiver<DbRequest<K, V>>,
    cache: DB,
) where
    DB: GenericDatabase,
    K: GenericBytes,
    V: GenericBytes,
{
    while let Some(incoming) = rax.recv().await {
        let result = match incoming.request {
            RequestKind::Read(k) => cache.read(k),
            RequestKind::Write(key, val) => cache.write(key, val).map(|_| None),
            RequestKind::Batch(b) => cache.batch(b).map(|_| None),
            RequestKind::Flush => cache.flush().map(|_| None),
        };

        if result.is_err() {
            tracing::error!("Db failed to send response back: {:?}", result.err());
            let _ = incoming.sender.send(None);
            continue;
        }

        let _ = incoming.sender.send(result.ok().flatten());
    }
}

/// Macro to abstract getting the data from the DB.
///
/// Returns `Option<V>`, where the result is `None` if
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

/// Abstracts inserting data into the DB.
pub async fn db_insert<K, V>(
    channel: &UnboundedSender<DbRequest<K, V>>,
    key: K,
    value: V,
) -> Receiver<Option<Vec<u8>>>
where
    K: GenericBytes,
    V: GenericBytes,
{
    let (tx, rx) = oneshot::channel();
    let req = DbRequest::new(RequestKind::Write(key, value), tx);
    let _ = channel.send(req);
    rx
}

/// Abstracts writing batch data to the DB.
pub async fn db_batch<K, V>(
    channel: &UnboundedSender<DbRequest<K, V>>,
    batch: Batch<K, V>,
) -> Receiver<Option<Vec<u8>>>
where
    K: GenericBytes,
    V: GenericBytes,
{
    let (tx, rx) = oneshot::channel();
    let req = DbRequest::new(RequestKind::Batch(batch), tx);
    let _ = channel.send(req);
    rx
}

/// Macro for flushing the DB.
#[macro_export]
macro_rules! db_flush {
    ($channel:expr) => {{
        use $crate::database::types::{
            DbRequest,
            RequestKind,
        };

        let (tx, rx) = tokio::sync::oneshot::channel();
        let req: DbRequest<_, _> = DbRequest::new(RequestKind::Flush, tx);

        let _ = $channel.send(req);

        rx
    }};
}
