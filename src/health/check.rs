use crate::Rpc;
use std::sync::{Arc, RwLock};
use tokio::task;
use std::time::Instant;

pub async fn check(
    rpc_list: &mut Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &mut Arc<RwLock<Vec<Rpc>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Head blocks reported by each RPC, we also use it to mark delinquents
    //
    // If a head is marked at `0` that means that the rpc is delinquent
    let heads = head_check(rpc_list, 300).await?;

    // Remove RPCs that are falling behind
    let agreed_head = make_poverty(rpc_list, poverty_list, heads)?;

    // Check if any rpc nodes made it out
   	// Its ok if we call them twice because some might have been accidentally put here
   	escape_poverty(rpc_list, poverty_list, agreed_head).await?;

    Ok(())
}

// Check what heads are reported by each RPC
async fn head_check(
	rpc_list: &Arc<RwLock<Vec<Rpc>>>,
	ttl: u128,
) -> Result<Vec::<u64>, Box<dyn std::error::Error>> {
    let mut heads = Vec::<u64>::new();

    // lifetime fuckery
    let rpc_list_guard = rpc_list.read().unwrap();
    // Iterate over all RPCs
    for rpc in rpc_list_guard.iter() {
        let start = Instant::now();
        // more lifetime fuckery, heap go brrr
        let rpc_clone = Arc::new(rpc.clone());

        // Spawn new task calling block_number for the rpc
        let reported_head = task::spawn(async move {
            rpc_clone.block_number().await
        });

        // Check every 5ms if we got a response, if after ttl ms no response is received mark it as delinquent
        loop {
            if reported_head.is_finished() {
                // This unwrapping is fine
                heads.push(reported_head.await.unwrap().unwrap());
                break;
            }
            if start.elapsed().as_millis() > ttl {
                reported_head.abort();
                heads.push(0);
                break;
            }
        }
    }
	Ok(heads)
}

// Add unresponsive/erroring RPCs to the poverty list
// TODO: Doesn't take into account RPCs getting updated in the meantime
fn make_poverty(
	rpc_list: &mut Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &mut Arc<RwLock<Vec<Rpc>>>,
    heads: Vec::<u64>,
) -> Result<u64, Box<dyn std::error::Error>> {
	// Average `heads` and round it up so we get what the majority of nodes are reporting
	// We are being optimistic and assuming that the majority is correct
	let average_head = heads.iter().sum::<u64>() / heads.len() as u64 + 1;

	// Iterate over `rpc_list` and move those falling behind to the `poverty_list`
	// We also set their is_erroring status to true and their last erroring to the
	// current unix timestamps in seconds
	let mut rpc_list_guard = rpc_list.write().unwrap();
	let mut poverty_list_guard = poverty_list.write().unwrap();
	for i in 0..rpc_list_guard.len() {
		if heads[i] < average_head {
			rpc_list_guard[i].status.is_erroring = true;
			rpc_list_guard[i].status.last_error = chrono::Utc::now().timestamp() as u64;
			poverty_list_guard.push(rpc_list_guard.remove(i));
		}
	}

	Ok(average_head)
}

// Go over the `poverty_list` to see if any nodes are back to normal
async fn escape_poverty(
	rpc_list: &mut Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &mut Arc<RwLock<Vec<Rpc>>>,
    agreed_head: u64,
) -> Result<(), Box<dyn std::error::Error>> {
	// Do a head check over the current poverty list to see if any nodes are back to normal
	let poverty_heads = head_check(poverty_list, 150).await?;
	// Check if any nodes made it 🗣️🔥🔥🔥
	let mut poverty_list_guard = poverty_list.write().unwrap();
	for i in 0..poverty_list_guard.len() {
		if poverty_heads[i] >= agreed_head {
			// Remove from poverty list and add to rpc list
			rpc_list.write().unwrap().push(poverty_list_guard.remove(i));
		}
	}

	Ok(())
}