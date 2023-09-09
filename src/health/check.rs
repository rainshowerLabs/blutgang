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
};

// call check n a loop
pub async fn health_check(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    poverty_list: Arc<RwLock<Vec<Rpc>>>,
    ttl: u128,
    health_check_ttl: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        sleep(Duration::from_millis(health_check_ttl)).await;
        check(&rpc_list, &poverty_list, &ttl).await?;
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
    let agreed_head = make_poverty(&rpc_list, poverty_list, heads)?;

    // Check if any rpc nodes made it out
    // Its ok if we call them twice because some might have been accidentally put here
    escape_poverty(&rpc_list, poverty_list, agreed_head, (*ttl).try_into().unwrap()).await?;
    #[cfg(not(feature = "tui"))]
    println!("OK!");

    Ok(())
}

// Check what heads are reported by each RPC
// TODO: check multiple RPCs at teh same time
async fn head_check(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    ttl: u128,
) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
    let len = rpc_list.read().unwrap().len();
    let mut heads = Vec::new();
    let mut threads = Vec::new();

    // Create a vector to store the futures of all RPC requests
    let mut rpc_futures = Vec::new();

    // Iterate over all RPCs
    for i in 0..len {
        let rpc_clone = rpc_list.read().unwrap()[i].clone();

        // Spawn a future for each RPC
        let rpc_future = async move {
            let a = rpc_clone.block_number();
            let result = timeout(Duration::from_millis(ttl.try_into().unwrap()), a).await;

            match result {
                Ok(response) => response.unwrap_or(0), // Handle timeout as 0
                Err(_) => 0,                           // Handle timeout as 0
            }
        };

        rpc_futures.push(rpc_future);
    }

    // Wait for all RPC futures concurrently and collect their results
    for rpc_future in rpc_futures {
        let result = tokio::spawn(rpc_future);
        threads.push(result);
    }

    // Collect the results using tokio::select!
    for _ in 0..len {
        select! {
            result = threads.pop().unwrap() => {
                heads.push(result.unwrap());
            }
        }
    }
    println!("Heads: {:?}", heads);

    Ok(heads)
}
// Add unresponsive/erroring RPCs to the poverty list
// TODO: Doesn't take into account RPCs getting updated in the meantime
fn make_poverty(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    heads: Vec<u64>,
) -> Result<u64, Box<dyn std::error::Error>> {
    // Get the highest head reported by the RPCs
    let mut highest_head = 0;
    for head in heads.iter() {
        if *head > highest_head {
            highest_head = *head;
        }
    }

    // Iterate over `rpc_list` and move those falling behind to the `poverty_list`
    // We also set their is_erroring status to true and their last erroring to the
    // current unix timestamps in seconds
    let mut rpc_list_guard = rpc_list.write().unwrap();
    let mut poverty_list_guard = poverty_list.write().unwrap();
    let mut rpc_list_positions: Vec<usize> = Vec::new();

    for i in 0..rpc_list_guard.len() {
        if heads[i] < highest_head {
            rpc_list_guard[i].status.is_erroring = true;
            rpc_list_guard[i].status.last_error = chrono::Utc::now().timestamp() as u64;
            rpc_list_positions.push(i);
            poverty_list_guard.push(rpc_list_guard[i].clone());
        }
    }

    for i in rpc_list_positions.iter().rev() {
        rpc_list_guard.remove(*i);
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
