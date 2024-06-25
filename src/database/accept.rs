use crate::{
        database::types::{
        DbRequest,
        RequestKind,
    },
    log_err,
};

use sled::{
    Db,
    InlineArray,
};

use tokio::sync::mpsc;

/// Processes incoming requests from clients and returns responses
pub async fn database_processing<K, V>(mut rax: mpsc::UnboundedReceiver<DbRequest<K, V>>, cache: Db)
where
    K: AsRef<[u8]>,
    V: Into<InlineArray> + Clone + From<InlineArray>,
{
    loop {
        while let Some(incoming) = rax.recv().await {
            let result = match incoming.request {
                RequestKind::Read(k) => cache.get(k.as_ref()).map(|opt| opt.map(|ivec| V::from(InlineArray::from(&ivec[..])))),
                RequestKind::Write(k, v) => {
                    cache.insert(k.as_ref(), v.into()).map(|opt| opt.map(|ivec| V::from(InlineArray::from(&ivec[..]))))
                }
                RequestKind::Batch(b) => {
                    cache.apply_batch(b).map(|_| None)
                }
            };

            let rax = result.unwrap_or(None);

            if let Err(_) = incoming.sender.send(rax) {
                log_err!("Db failed to send response back!");
            }
        }
    }
}
