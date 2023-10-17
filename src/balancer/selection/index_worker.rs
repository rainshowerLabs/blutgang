use std::collections::hash_map::Entry;
use std::collections::HashMap;
use crate::LatencyData;
use std::{
    sync::{
        Arc,
        RwLock,
    },
    time::Duration,
};

use crate::Rpc;

use tokio::{
    sync::{
        watch,
        broadcast,
    },
    time::sleep,
};


use super::selection::pick;

// We don't really need to update whats the best RPC on *every* external
// RPC call, so we can just do it periodically and send the results via
// a channel to our thread managing the call.
pub async fn pick_index(
    rpc_list_rwlock: &mut Arc<RwLock<Vec<Rpc>>>,
    selection_ttl: Duration,
    rpc_index_tx: watch::Sender<Option<usize>>,
    latency_rx: broadcast::Receiver<LatencyData>,
) -> Option<usize> {
    let mut len: usize = 0;
    loop {
        let _ = sleep(selection_ttl).await;

        // Update the latencies
        let rx2 = latency_rx.resubscribe();
        update_latency(rpc_list_rwlock, rx2).await;

        let mut rpc_list_rwlock_guard = rpc_list_rwlock.write().unwrap();

        // Sort the rpc_list fromt the fastest to the least fast RPC.
        // TODO: WE REALlY DO NOT NEED TO SORT THIS!!!!! We're wasting cpu time on useless heap operations. Just find the 2 fastest and return their indexes.
        let index = pick(&mut rpc_list_rwlock_guard);

        // Send new index if modified
        let len_clone = len.clone();
        len = rpc_list_rwlock_guard.len();

        let send_if_changed = |ch_index: &mut Option<usize>| {
            if ch_index != &index {
                *ch_index = index;
                return true;
            }

            // The index might be the same but the rpc in the index might have changed
            if len_clone != rpc_list_rwlock_guard.len() {
                // *ch_index = index; // prob dont need this since index !changed?
                return true;
            }

            false
        };

        rpc_index_tx.send_if_modified(send_if_changed);
    }
}

// Update the latencies from the `latency_rx` channel
async fn update_latency(
    rpc_list_rwlock : &mut Arc<RwLock<Vec<Rpc>>>,
    mut latency_rx: broadcast::Receiver<LatencyData>,
) -> Option<()> {
    let mut latency_map: HashMap<usize, Vec<f64>> = HashMap::new();

    loop {
        if latency_rx.len() == 0 {
            break;
        }

        let latency_data = latency_rx.recv().await;

        match latency_map.entry(latency_data.as_ref().unwrap().index) {
            Entry::Vacant(e) => { e.insert(vec![latency_data.as_ref().unwrap().latency as f64]); },
            Entry::Occupied(mut e) => { e.get_mut().push(latency_data.unwrap().latency as f64); }
        }

    }

    // Iter over the map and update the latencies
    let mut rpc_list_rwlock_guard = rpc_list_rwlock.write().unwrap();

    for (index, latency_data) in latency_map {
        rpc_list_rwlock_guard[index].update_latency(latency_data);
    }

    Some(())
}
