use crate::{
    config::error::ConfigError,
    log_err,
    Rpc,
};
use std::time::Instant;
use tokio::sync::mpsc;

#[derive(Debug)]
enum StartingLatencyResp {
    Ok(Rpc),
    Error(ConfigError),
}

// Get the average latency for a RPC
async fn set_starting_latency(
    mut rpc: Rpc,
    ma_length: f64,
    tx: mpsc::Sender<StartingLatencyResp>,
) -> Result<(), ConfigError> {
    let mut latencies = Vec::new();

    for _ in 0..ma_length as u32 {
        let start = Instant::now();
        match rpc.block_number().await {
            Ok(_) => {}
            Err(e) => {
                tx.send(StartingLatencyResp::Error(e.into())).await?;
                return Err(ConfigError::RpcError(
                    "Error awaiting block_number!".to_string(),
                ));
            }
        };
        let end = Instant::now();
        let latency = end.duration_since(start).as_nanos() as f64;
        latencies.push(latency);
    }

    let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
    rpc.update_latency(avg_latency);

    println!("{}: {}ns", rpc.url, rpc.status.latency);

    tx.send(StartingLatencyResp::Ok(rpc)).await?;

    Ok(())
}

// Do `ma_length`amount eth_blockNumber calls per rpc and then sort them by latency
pub async fn sort_by_latency(
    mut rpc_list: Vec<Rpc>,
    ma_length: f64,
) -> Result<Vec<Rpc>, ConfigError> {
    // Return empty vec if we dont supply any RPCs
    if rpc_list.is_empty() {
        log_err!("No RPCs supplied!");
        return Ok(Vec::new());
    }

    let (tx, mut rx) = mpsc::channel(rpc_list.len());

    // Iterate over each RPC
    for rpc in rpc_list.drain(..) {
        let tx = tx.clone();
        // Spawn a new asynchronous task for each RPC
        tokio::spawn(set_starting_latency(rpc, ma_length, tx));
    }

    let mut sorted_rpc_list = Vec::new();

    // Drop tx so we don't try to receive nothing
    drop(tx);

    // Collect results from tasks
    while let Some(rpc) = rx.recv().await {
        let rpc = match rpc {
            StartingLatencyResp::Ok(rax) => rax,
            StartingLatencyResp::Error(e) => {
                log_err!("{}", e);
                continue;
            }
        };
        sorted_rpc_list.push(rpc);
    }

    // Sort the RPCs by latency
    sorted_rpc_list.sort_by(|a, b| a.status.latency.partial_cmp(&b.status.latency).unwrap());

    Ok(sorted_rpc_list)
}

// #[cfg(test)]
// mod tests {
//     use tokio::time::sleep;
//     use super::*;

//     use tokio::sync::{mpsc};
//     use std::time::Duration;

//     #[tokio::test]
//     async fn test_set_starting_latency() {
//         let (tx, mut rx) = mpsc::channel(1);
//         let rpc = Rpc::new("http://test_rpc".to_string(), Some("ws://test_rpc".to_string()), 0, 0, 0.0);

//         tokio::spawn(async move {
//             set_starting_latency(rpc, 1.0, tx).await.unwrap();
//         });

//         // Simulate a delay to mimic the RPC response time
//         sleep(Duration::from_millis(50)).await;

//         match rx.recv().await {
//             Some(StartingLatencyResp::Ok(rpc)) => {
//                 // Assert based on expected latency
//                 assert!(rpc.status.latency > 0.0);
//             },
//             Some(StartingLatencyResp::Error(e)) => panic!("Expected Ok, got Error: {:?}", e),
//             None => panic!("Expected Some, got None"),
//         }
//     }

//     #[tokio::test]
//     async fn test_sort_by_latency() {
//         let rpc_list = vec![
//             Rpc::new("http://fast_rpc".to_string(), Some("ws://fast_rpc".to_string()), 0, 0, 0.0),
//             Rpc::new("http://slow_rpc".to_string(), Some("ws://slow_rpc".to_string()), 0, 0, 0.0),
//         ];

//         let sorted_rpc_list = sort_by_latency(rpc_list, 1.0).await.unwrap();

//         // Assert based on the expected order after sorting by latency
//         assert_eq!(sorted_rpc_list[0].url, "http://fast_rpc");
//         assert_eq!(sorted_rpc_list[1].url, "http://slow_rpc");
//     }

//     #[tokio::test]
//     async fn test_sort_by_latency_with_error_rpc() {
//         let rpc_list = vec![
//             Rpc::new("http://fast_rpc".to_string(), Some("ws://fast_rpc".to_string()), 0, 0, 0.0),
//             Rpc::new("http://error_rpc".to_string(), Some("ws://error_rpc".to_string()), 0, 0, 0.0),
//         ];

//         let sorted_rpc_list = sort_by_latency(rpc_list, 1.0).await.unwrap();

//         // Expecting only the fast_rpc as the error_rpc should be skipped
//         assert_eq!(sorted_rpc_list.len(), 1);
//         assert_eq!(sorted_rpc_list[0].url, "http://fast_rpc");
//     }

//     #[tokio::test]
//     async fn test_sort_by_latency_empty_list() {
//         let rpc_list = Vec::new();

//         let sorted_rpc_list = sort_by_latency(rpc_list, 1.0).await.unwrap();

//         assert!(sorted_rpc_list.is_empty());
//     }
// }
