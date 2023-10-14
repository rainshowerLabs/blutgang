use std::{
	time::Duration,
	sync::{
		RwLock,
		Arc,
	}
};

use crate::Rpc;

use tokio::{
	sync::watch,
	time::sleep,
};

use super::selection::pick;

// We don't really need to update whats the best RPC on *every* external
// RPC call, so we can just do it periodically and send the results via 
// a channel to our thread managing the call.
pub fn pick_index(
	rpc_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
	selection_ttl: Duration,
	rpc_index_tx: watch::Sender<Option<usize>>,
) -> Option<usize> {
	let mut current_index: Option<usize> = None;
	let mut len: usize = 0;
	loop {
		let _ = sleep(selection_ttl);

		let rpc_list_rwlock_guard = rpc_list_rwlock.write().unwrap();

		// Sort the rpc_list fromt the fastest to the least fast RPC.
		// TODO: WE REALlY DO NOT NEED TO SORT THIS!!!!! We're wasting cpu time on useless heap operations. Just find the 2 fastest and return their indexes.
		let index = pick(&mut rpc_list_rwlock_guard);

	    // Send new index if modified
	    let send_if_changed = |ch_index: &mut Option<usize>| {
	        if ch_index != &index {
	            *ch_index = index;
	            return true;
	        }

	        // The index might be the same but the rpc in the index might have changed
	        if len != rpc_list_rwlock_guard.len() {
	            // *ch_index = index; // prob dont need this since index !changed?
	            return true;
	        }

	        false
	    };

		len = rpc_list_rwlock_guard.len();

	    rpc_index_tx.send_if_modified(send_if_changed);
	}
}
