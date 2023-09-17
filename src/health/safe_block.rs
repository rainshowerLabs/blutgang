use crate::Rpc;
use std::sync::{
    Arc,
    RwLock,
};

async fn get_safe_block(rpc_list: &Arc<RwLock<Vec<Rpc>>>, health_check_ttl: u64) -> u64 {
    unimplemented!()
}
