use crate::Rpc;
use std::time::Instant;

// Do `ma_length`amount eth_blockNumber calls per rpc and then sort them by latency
pub async fn sort_by_latency(mut rpc_list: Vec<Rpc>, ma_lenght: f64) -> Vec<Rpc> {
    for rpc in rpc_list.iter_mut() {
        for _ in 0..ma_lenght as u32 {
            let start = Instant::now();
            let _ = rpc.block_number().await.unwrap();
            let end = Instant::now();
            rpc.update_latency(end.duration_since(start).as_nanos() as f64, ma_lenght);
        }
        println!("{}: {}ns", rpc.url, rpc.status.latency);
    }
    rpc_list.sort_by(|a, b| a.status.latency.partial_cmp(&b.status.latency).unwrap());
    rpc_list
}
