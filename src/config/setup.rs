use crate::Rpc;
use std::time::Instant;
use tokio::sync::mpsc;

// System consts
pub const WS_HEALTH_CHECK_USER_ID: u32 = 1;

// Version consts, dont impact functionality
pub const VERSION_STR: &str = "blutgang 0.3.0-canary2 Garreg Mach";
pub const TAGLINE: &str = "`Now there's a way forward.`";

// Do `ma_length`amount eth_blockNumber calls per rpc and then sort them by latency
pub async fn sort_by_latency(mut rpc_list: Vec<Rpc>, ma_length: f64) -> Vec<Rpc> {
    // Return empty vec if we dont supply any RPCs
    if rpc_list.is_empty() {
        println!("\x1b[31mErr:\x1b[0m No RPCs supplied!");
        return Vec::new();
    }

    let (tx, mut rx) = mpsc::channel(rpc_list.len());

    // Iterate over each RPC
    for mut rpc in rpc_list.drain(..) {
        let tx = tx.clone();

        // Spawn a new asynchronous task for each RPC
        tokio::spawn(async move {
            let mut latencies = Vec::new();

            for _ in 0..ma_length as u32 {
                let start = Instant::now();
                let _ = rpc.block_number().await.unwrap();
                let end = Instant::now();
                let latency = end.duration_since(start).as_nanos() as f64;
                latencies.push(latency);
            }

            let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
            rpc.update_latency(avg_latency);

            println!("{}: {}ns", rpc.url, rpc.status.latency);

            tx.send(rpc).await.expect("Failed to send RPC result.");
        });
    }

    drop(tx); // Drop the sender to signal that all tasks are done

    let mut sorted_rpc_list = Vec::new();

    // Collect results from tasks
    while let Some(rpc) = rx.recv().await {
        sorted_rpc_list.push(rpc);
    }

    // Sort the RPCs by latency
    sorted_rpc_list.sort_by(|a, b| a.status.latency.partial_cmp(&b.status.latency).unwrap());

    sorted_rpc_list
}
