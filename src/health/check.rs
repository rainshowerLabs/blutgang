// check if all RPCs are responding and if they are all on the same head
use crate::Rpc;
use std::sync::{ Arc, RwLock};
use tokio::task;
use std::time::Instant;

pub async fn check(
	rpc_list: &mut Arc<RwLock<Vec<Rpc>>>,
	poverty_list: &mut Arc<RwLock<Vec<String>>>,
) -> Result<(), Box<dyn std::error::Error>> {
	// Head blocks reported by each RPC, we also use it to mark delinquents
	//
	// If a head is marked at `0` that means that the rpc is delinquent
	let mut heads = Vec::<u64>::new();

	// Iterate over all RPCs
	for rpc in rpc_list.read().unwrap().iter_mut() {
		let start = Instant::now();
		// Spawn new thread calling block_number for the rpc
		let reported_head = task::spawn(async move {
			let head: u64 = rpc.block_number().await.unwrap();
			head
		});

		// Check every 5ms if we got a response, if after 300ms no response is received mark it as delinquent
		let mut delinquent = false;
		loop {
			if reported_head.is_finished() {
				// This unwrapping fiendish
				heads.push(reported_head.await.unwrap());
				break;
			}
			if start.elapsed().as_millis() > 300 {
				delinquent = true;
				break;
			}
			task::yield_now().await;
		}

		let end = Instant::now();
		let end = end.duration_since(start);
	}
	Ok(())

}
