use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
    sync::{
        Arc,
        RwLock,
    },
};

use sled::IVec;

use crate::rpc::error::RpcError;

pub fn manage_cache(
    head_cache: &Arc<RwLock<BTreeMap<u64, HashMap<String, IVec>>>>,
    blocknum_rx: &tokio::sync::watch::Receiver<u64>,
    cache: &Arc<sled::Db>,
) -> Result<(), RpcError> {
    Ok(())
}
