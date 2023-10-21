use crate::{
    balancer::selection::selection::pick,
    LatencyData,
    Rpc,
};

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::{
    sync::{
        Arc,
        RwLock,
    },
    time::Duration,
};

use tokio::{
    sync::{
        broadcast,
        watch,
    },
    time::sleep,
};

#[derive(Clone, Debug)]
pub struct CurrentRpc {
    pub rpc: Rpc,
    pub index: usize,
}

// We don't really need to update whats the best RPC on *every* external
// RPC call, so we can just do it periodically and send the results via
// a channel to our thread managing the call.
pub async fn pick_index(
    rpc_list_rwlock: &mut Arc<RwLock<Vec<Rpc>>>,
    selection_ttl: Duration,
    rpc_index_tx: watch::Sender<Option<CurrentRpc>>,
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
        let len_clone = len.clone(); // we have to clone due to the closure
        len = rpc_list_rwlock_guard.len();

        let send_if_changed = |ch_rpc: &mut Option<CurrentRpc>| {
            if index.is_none() {
                return false;
            } else if ch_rpc.is_none() {
                let current_rpc = CurrentRpc {
                    rpc: rpc_list_rwlock_guard[index.unwrap()].clone(),
                    index: index.unwrap(),
                };

                *ch_rpc = Some(current_rpc);
                return true;
            }

            // reqwest::Client doesnst impl PartialEq so do just compare the urls. yay for storing redundant data!
            if ch_rpc.clone().unwrap().rpc.url != rpc_list_rwlock_guard[index.unwrap()].url {
                let current_rpc = CurrentRpc {
                    rpc: rpc_list_rwlock_guard[index.unwrap()].clone(),
                    index: index.unwrap(),
                };

                *ch_rpc = Some(current_rpc);
                return true;
            }

            // The index might be the same but the rpc in the index might have changed
            if len_clone != rpc_list_rwlock_guard.len() {
                // *ch_rpc = index; // prob dont need this since index !changed?
                return true;
            }

            false
        };

        rpc_index_tx.send_if_modified(send_if_changed);
    }
}

// Update the latencies from the `latency_rx` channel
async fn update_latency(
    rpc_list_rwlock: &mut Arc<RwLock<Vec<Rpc>>>,
    mut latency_rx: broadcast::Receiver<LatencyData>,
) -> Option<()> {
    let mut latency_map: HashMap<usize, Vec<f64>> = HashMap::new();

    loop {
        if latency_rx.is_empty() {
            break;
        }

        let latency_data = latency_rx.recv().await;

        match latency_map.entry(latency_data.as_ref().unwrap().index) {
            Entry::Vacant(e) => {
                e.insert(vec![latency_data.as_ref().unwrap().latency as f64]);
            }
            Entry::Occupied(mut e) => {
                e.get_mut().push(latency_data.unwrap().latency as f64);
            }
        }
    }

    // Iter over the map and update the latencies
    let mut rpc_list_rwlock_guard = rpc_list_rwlock.write().unwrap();

    for (index, latency_data) in latency_map {
        rpc_list_rwlock_guard[index].update_latency(latency_data);
    }

    Some(())
}
