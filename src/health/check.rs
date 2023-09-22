use crate::health::safe_block::get_safe_block;
use crate::Rpc;

use std::println;
use std::sync::{
    Arc,
    RwLock,
};
use std::time::Duration;

use tokio::{
    select,
    time::{
        sleep,
        timeout,
    },
    sync::mpsc,
};

// call check n a loop
pub async fn health_check(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    poverty_list: Arc<RwLock<Vec<Rpc>>>,
    finalized: Arc<RwLock<u64>>,
    ttl: u128,
    health_check_ttl: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        sleep(Duration::from_millis(health_check_ttl)).await;
        check(&rpc_list, &poverty_list, &ttl).await?;
        get_safe_block(&rpc_list, &finalized, health_check_ttl).await?;
    }
}

async fn check(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    ttl: &u128,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(feature = "tui"))]
    println!("Checking RPC health...");
    // Head blocks reported by each RPC, we also use it to mark delinquents
    //
    // If a head is marked at `0` that means that the rpc is delinquent
    let heads = head_check(&rpc_list, *ttl).await?;

    // Remove RPCs that are falling behind
    println!("rpc_list len: {:?}", rpc_list.read().unwrap().len());
    let agreed_head = make_poverty(&rpc_list, poverty_list, heads)?;
    println!("rpc_list len: {:?}", rpc_list.read().unwrap().len());

    // Check if any rpc nodes made it out
    // Its ok if we call them twice because some might have been accidentally put here
    escape_poverty(
        &rpc_list,
        poverty_list,
        agreed_head,
        (*ttl).try_into().unwrap(),
    )
    .await?;
    println!("rpc_list len: {:?}", rpc_list.read().unwrap().len());

    #[cfg(not(feature = "tui"))]
    println!("OK!");

    Ok(())
}

// Check what heads are reported by each RPC
async fn head_check(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    ttl: u128,
) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
    let len = rpc_list.read().unwrap().len();
    // If len == 0 return empty Vec
    if len == 0 {
        return Ok(Vec::<u64>::new());
    }

    let mut heads = Vec::new();

    // Create a vector to store the futures of all RPC requests
    let mut rpc_futures = Vec::new();

    // Create a channel for collecting results in order
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

            // Send the result to the main thread through the channel
            tx.send(head).await.expect("Channel send error");
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

    println!("Heads: {:?}", heads);

    Ok(heads)
}


// Add unresponsive/erroring RPCs to the poverty list
fn make_poverty(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    heads: Vec<u64>,
) -> Result<u64, Box<dyn std::error::Error>> {
    // Get the highest head reported by the RPCs
    let mut highest_head = 0;
    for head in heads.iter() {
        if head > &highest_head {
            highest_head = *head;
        }
    }
    println!("Highest head: {:?}", highest_head);

    // Iterate over `rpc_list` and move those falling behind to the `poverty_list`
    // We also set their is_erroring status to true and their last erroring to the
    // current unix timestamps in seconds
    let mut rpc_list_guard = rpc_list.write().unwrap();
    let mut poverty_list_guard = poverty_list.write().unwrap();

    let mut i = 0;

    while i < rpc_list_guard.len() {
        // If the RPC is not following the head, nuke it to poverty
        if heads[i] < highest_head {
            println!("RPC {} is falling behind",  rpc_list_guard[i].url);

            rpc_list_guard[i].status.is_erroring = true;
            rpc_list_guard[i].status.last_error = chrono::Utc::now().timestamp() as u64;

            // Remove the element from rpc_list_guard and append it to poverty_list_guard
            let removed_rpc = rpc_list_guard.remove(i);
            poverty_list_guard.push(removed_rpc);
        } else {
            i += 1; // Move to the next element if not removed
        }
    }

    Ok(highest_head)
}

// Go over the `poverty_list` to see if any nodes are back to normal
async fn escape_poverty(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    ttl: u64,
    agreed_head: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Do a head check over the current poverty list to see if any nodes are back to normal
    let poverty_heads = head_check(&poverty_list, ttl.into()).await?;
    println!("Poverty heads: {:?}", poverty_heads);

    // Check if any nodes made it 🗣️🔥🔥🔥
    let mut poverty_list_guard = poverty_list.write().unwrap();
    let mut rpc_list_guard = rpc_list.write().unwrap();

    let mut i = 0;
    while i < poverty_list_guard.len() {
        if poverty_heads[i] >= agreed_head {
            // Remove from poverty list and add to rpc list
            let mut removed_rpc = poverty_list_guard.remove(i);
            // Remove erroring status from the rpc
            removed_rpc.status.is_erroring = false;

            rpc_list_guard.push(removed_rpc);
        } else {
            i += 1; // Move to the next element if not removed
        }
    }

    Ok(())
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::env;

//     // Helper function to create a test Rpc struct
//     fn create_test_rpcs() -> Vec<Rpc> {
//         let llama = Rpc::new(env::var("RPC0").expect("RPC0 not found in .env"), 5);
//         let builder0x69 = Rpc::new(env::var("RPC1").expect("RPC1 not found in .env"), 5);
//         let tenderly = Rpc::new(env::var("RPC2").expect("RPC2 not found in .env"), 5);

//         vec![llama, builder0x69, tenderly]
//     }

//     #[tokio::test]
//     async fn test_head_check() {
//         // Create a test Rpc list with at least one Rpc
//         let rpc_list = Arc::new(RwLock::new(create_test_rpcs()));

//         // Call the head_check function with test arguments
//         let result = head_check(&rpc_list, 700).await;
//         assert!(result.is_ok());

//         // Add assertions for the expected behavior of head_check
//         // based on the test Rpc instance's behavior.
//     }

//     #[tokio::test]
//     async fn test_make_poverty() {
//         // Create a test Rpc list with at least one Rpc
//         let rpc_list = Arc::new(RwLock::new(create_test_rpcs()));
//         // Create an empty test poverty list
//         let poverty_list = Arc::new(RwLock::new(Vec::new()));

//         // Call the make_poverty function with test arguments
//         let result = make_poverty(&rpc_list, &poverty_list, vec![0]).unwrap();
//         assert_eq!(result, 0);

//         // Add assertions for the expected behavior of make_poverty
//         // based on the test Rpc instance's behavior and input data.
//     }

//     #[tokio::test]
//     async fn test_escape_poverty() {
//         // Create a test Rpc list with at least one Rpc
//         let rpc_list = Arc::new(RwLock::new(create_test_rpcs()));
//         // Create a test poverty list with at least one Rpc
//         let poverty_list = Arc::new(RwLock::new(create_test_rpcs()));

//         // Call the escape_poverty function with test arguments
//         let result = escape_poverty(&rpc_list, &poverty_list, 700, 0).await;
//         assert!(result.is_ok());

//         // Add assertions for the expected behavior of escape_poverty
//         // based on the test Rpc instance's behavior and input data.
//     }
// }
