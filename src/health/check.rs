use crate::health::safe_block::get_safe_block;
use crate::Rpc;

use std::println;
use std::sync::{
    Arc,
    RwLock,
};
use std::time::Duration;

use tokio::{
    sync::mpsc,
    time::{
        sleep,
        timeout,
    },
};

#[derive(Debug, Default)]
struct HeadResult {
    rpc_list_index: usize,
    reported_head: u64,
}

//Call check and safe_block in a loop
pub async fn health_check(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    poverty_list: Arc<RwLock<Vec<Rpc>>>,
    blocknum_tx: &tokio::sync::watch::Sender<u64>,
    finalized_tx: tokio::sync::watch::Sender<u64>,
    ttl: u128,
    health_check_ttl: u64,
    rolling_finality: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        sleep(Duration::from_millis(health_check_ttl)).await;
        check(&rpc_list, &poverty_list, blocknum_tx, &ttl).await?;
        get_safe_block(&rpc_list, &finalized_tx, health_check_ttl, rolling_finality).await?;
    }
}

async fn check(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    blocknum_tx: &tokio::sync::watch::Sender<u64>,
    ttl: &u128,
) -> Result<(), Box<dyn std::error::Error>> {
    print!("\x1b[35mInfo:\x1b[0m Checking RPC health... ");
    // Head blocks reported by each RPC, we also use it to mark delinquents
    //
    // If a head is marked at `0` that means that the rpc is delinquent
    let heads = head_check(rpc_list, *ttl).await?;

    // Remove RPCs that are falling behind
    let agreed_head = make_poverty(rpc_list, poverty_list, heads)?;
    // Send new blocknumber if modified
    let send_if_changed = |number: &mut u64| {
        if number != &agreed_head {
            *number = agreed_head;
            return true;
        }
        false
    };

    blocknum_tx.send_if_modified(send_if_changed);

    // Check if any rpc nodes made it out
    // Its ok if we call them twice because some might have been accidentally put here

    // Do a head check over the current poverty list to see if any nodes are back to normal
    let poverty_heads = head_check(poverty_list, *ttl).await?;

    escape_poverty(rpc_list, poverty_list, poverty_heads, agreed_head)?;

    println!("OK!");

    Ok(())
}

// Check what heads are reported by each RPC
pub async fn head_check(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    ttl: u128,
) -> Result<Vec<HeadResult>, Box<dyn std::error::Error>> {
    let len = rpc_list.read().unwrap().len();
    let mut heads = Vec::<HeadResult>::new();

    // If len == 0 return empty Vec
    if len == 0 {
        return Ok(heads);
    }

    // Create a vector to store the futures of all RPC requests
    let mut rpc_futures = Vec::new();

    // Create a channel for collecting results
    let (tx, mut rx) = mpsc::channel(len);

    // Iterate over all RPCs
    for i in 0..len {
        let rpc_clone = rpc_list.read().unwrap()[i].clone();
        let tx = tx.clone(); // Clone the sender for this RPC

        // Spawn a future for each RPC
        let rpc_future = async move {
            let a = rpc_clone.block_number();
            let result = timeout(Duration::from_millis(ttl.try_into().unwrap()), a).await;

            let head = match result {
                Ok(response) => response.unwrap_or(0), // Handle timeout as 0
                Err(_) => 0,                           // Handle timeout as 0
            };

            let head_result = HeadResult {
                rpc_list_index: i,
                reported_head: head,
            };

            // Send the result to the main thread through the channel
            tx.send(head_result)
                .await
                .expect("head check: Channel send error");
        };

        rpc_futures.push(rpc_future);
    }

    // Wait for all RPC futures concurrently
    for rpc_future in rpc_futures {
        tokio::spawn(rpc_future);
    }

    // Collect the results in order from the channel
    for _ in 0..len {
        if let Some(result) = rx.recv().await {
            heads.push(result);
        }
    }

    Ok(heads)
}

// Add unresponsive/erroring RPCs to the poverty list
fn make_poverty(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    heads: Vec<HeadResult>,
) -> Result<u64, Box<dyn std::error::Error>> {
    // Get the highest head reported by the RPCs
    let mut highest_head = 0;
    for head in &heads {
        if head.reported_head > highest_head {
            highest_head = head.reported_head;
        }
    }

    // Mark all RPCs that dont report the highest head as erroring
    let mut rpc_list_guard = rpc_list.write().unwrap();
    let mut poverty_list_guard = poverty_list.write().unwrap();

    for head in heads {
        if head.reported_head < highest_head {
            // Mark the RPC as erroring
            rpc_list_guard[head.rpc_list_index].status.is_erroring = true;

            // Add the RPC to the poverty list
            poverty_list_guard.push(rpc_list_guard[head.rpc_list_index].clone());
        }
    }

    // Go over rpc_list_guard and remove all erroring rpcs
    rpc_list_guard.retain(|rpc| !rpc.status.is_erroring);

    Ok(highest_head)
}

// Go over the `poverty_list` to see if any nodes are back to normal
fn escape_poverty(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_heads: Vec<HeadResult>,
    agreed_head: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check if any nodes made it 🗣️🔥🔥🔥
    let mut poverty_list_guard = poverty_list.write().unwrap();
    let mut rpc_list_guard = rpc_list.write().unwrap();

    for head_result in poverty_heads {
        if head_result.reported_head >= agreed_head {
            let mut rpc = poverty_list_guard[head_result.rpc_list_index].clone();
            rpc.status.is_erroring = false;

            // Move the RPC from the poverty list to the rpc list
            rpc_list_guard.push(rpc);

            // Remove the RPC from the poverty list
            poverty_list_guard[head_result.rpc_list_index]
                .status
                .is_erroring = false;
        }
    }

    // Only retain erroring RPCs
    poverty_list_guard.retain(|rpc| rpc.status.is_erroring);

    Ok(())
}

/*
 * Tests
 */
#[cfg(test)]
mod tests {
    use super::*;

    // Construct a hypothetical RPC and heads list for testing
    fn dummy_head_check() -> Vec<HeadResult> {
        vec![
            HeadResult {
                rpc_list_index: 0,
                reported_head: 18177557,
            },
            HeadResult {
                rpc_list_index: 1,
                reported_head: 18193012,
            },
            HeadResult {
                rpc_list_index: 2,
                reported_head: 0,
            },
        ]
    }

    #[test]
    fn test_poverty() {
        // Create a mock RPC list and poverty list
        let rpc1 = Rpc::default();
        let rpc2 = Rpc::default();
        let rpc3 = Rpc::default();

        let rpc_list = Arc::new(RwLock::new(vec![rpc1.clone(), rpc2.clone(), rpc3.clone()]));
        let poverty_list = Arc::new(RwLock::new(vec![]));

        // Test with dummy head results
        let heads = dummy_head_check();

        // Call the make_poverty function
        let result = make_poverty(&rpc_list, &poverty_list, heads);
        assert!(result.is_ok());

        // Check the state of RPCs after the test
        let rpc_list_guard = rpc_list.read().unwrap();
        let poverty_list_guard = poverty_list.read().unwrap();

        // Only 1 RPC should be in the rpc list
        assert_eq!(rpc_list_guard.len(), 1);

        // The poverty list should now contain 2 RPCs
        assert_eq!(poverty_list_guard.len(), 2);
    }

    #[test]
    fn test_escape() {
        // Create a mock RPC list and poverty list
        let mut rpc1 = Rpc::default();
        rpc1.status.is_erroring = true;

        let rpc2 = Rpc::default();
        let mut rpc3 = Rpc::default();
        rpc3.status.is_erroring = true;

        let rpc_list = Arc::new(RwLock::new(vec![rpc2.clone()]));
        let poverty_list = Arc::new(RwLock::new(vec![rpc1.clone(), rpc3.clone()]));

        // Test with dummy head results
        let heads = vec![
            HeadResult {
                rpc_list_index: 0,
                reported_head: 18177557,
            },
            HeadResult {
                rpc_list_index: 1,
                reported_head: 18193012,
            },
        ];

        // Call the escape_poverty function
        let result = escape_poverty(&rpc_list, &poverty_list, heads, 18193012);
        assert!(result.is_ok());

        // Check the state of RPCs after the test
        let rpc_list_guard = rpc_list.read().unwrap();
        let poverty_list_guard = poverty_list.read().unwrap();
        // RPC3 should have escaped poverty
        assert_eq!(rpc_list_guard.len(), 2);

        // The poverty list should have 1 RPC
        assert_eq!(poverty_list_guard.len(), 1);
    }
}
