use crate::Rpc;
use std::sync::{Arc, RwLock};
use tokio::task;
use std::time::Instant;

pub async fn check(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &mut Arc<RwLock<Vec<String>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Head blocks reported by each RPC, we also use it to mark delinquents
    //
    // If a head is marked at `0` that means that the rpc is delinquent
    let mut heads = Vec::<u64>::new();

    // Iterate over all RPCs
    let rpc_list_guard = rpc_list.read().unwrap();
    for rpc in rpc_list_guard.iter() {
        let start = Instant::now();
        // Clone rpc_list for use inside the async block
        let rpc_clone = Arc::new(rpc.clone());

        // Spawn new task calling block_number for the rpc
        let reported_head = task::spawn(async move {
            rpc_clone.block_number().await
        });

        // Check every 5ms if we got a response, if after 300ms no response is received mark it as delinquent
        loop {
            if reported_head.is_finished() {
                // This unwrapping is fine
                heads.push(reported_head.await.unwrap().unwrap());
                break;
            }
            if start.elapsed().as_millis() > 300 {
                reported_head.abort();
                heads.push(0);
                break;
            }
        }
    }

    Ok(())
}
