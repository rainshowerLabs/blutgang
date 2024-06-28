use crate::{
    admin::liveready::{
        HealthState,
        LiveReadyUpdate,
        LiveReadyUpdateSnd,
    },
    health::{
        error::HealthError,
        safe_block::{
            get_safe_block,
            NamedBlocknumbers,
        },
    },
    log_info,
    log_wrn,
    websocket::{
        subscription_manager::move_subscriptions,
        types::{
            WsChannelErr,
            WsconnMessage,
        },
    },
    IncomingResponse,
    Rpc,
    Settings,
    SubscriptionData,
};

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
        mpsc,
        oneshot,
    },
    time::{
        sleep,
        timeout,
    },
};

#[derive(Debug, Default)]
struct HeadResult {
    rpc_list_index: usize,
    is_syncing: bool,
    reported_head: u64,
}

#[derive(Debug)]
struct InnerResult {
    is_syncing: bool,
    reported_head: u64,
}

/// Call check and safe_block in a loop
pub async fn health_check(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    poverty_list: Arc<RwLock<Vec<Rpc>>>,
    finalized_tx: tokio::sync::watch::Sender<u64>,
    liveness_tx: LiveReadyUpdateSnd,
    named_numbers_rwlock: &Arc<RwLock<NamedBlocknumbers>>,
    config: &Arc<RwLock<Settings>>,
) -> Result<(), HealthError> {
    loop {
        let health_check_ttl = config.read().unwrap().health_check_ttl;
        let ttl = config.read().unwrap().ttl;
        let supress_rpc_check = config.read().unwrap().supress_rpc_check;

        sleep(Duration::from_millis(health_check_ttl)).await;

        check(
            &rpc_list,
            &poverty_list,
            &ttl,
            &liveness_tx,
            supress_rpc_check,
        )
        .await?;

        get_safe_block(
            &rpc_list,
            &finalized_tx,
            named_numbers_rwlock,
            health_check_ttl,
        )
        .await?;
    }
}

/// Track the head of each RPC and process them accordingly
async fn check(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    ttl: &u128,
    liveness_tx: &LiveReadyUpdateSnd,
    supress_rpc_check: bool,
) -> Result<(), HealthError> {
    if !supress_rpc_check {
        print!("\x1b[35mInfo:\x1b[0m Checking RPC health... ");
    }
    // Head blocks reported by each RPC, we also use it to mark delinquents
    //
    // If a head is marked at `0` that means that the rpc is delinquent
    let heads = head_check(rpc_list, *ttl).await?;

    // Remove RPCs that are falling behind
    let agreed_head = make_poverty(rpc_list, poverty_list, heads)?;

    // Check if any rpc nodes made it out
    // Its ok if we call them twice because some might have been accidentally put here

    // Do a head check over the current poverty list to see if any nodes are back to normal
    let poverty_heads = head_check(poverty_list, *ttl).await?;

    let to_send = escape_poverty(rpc_list, poverty_list, poverty_heads, agreed_head)?;

    // Send the current status of nodes to the liveness monitor
    let _ = liveness_tx.send(to_send).await;

    if !supress_rpc_check {
        println!("OK!");
    }

    Ok(())
}

/// Check what heads are reported by each RPC
async fn head_check(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    ttl: u128,
) -> Result<Vec<HeadResult>, HealthError> {
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
            let (send_tx, send_rx) = oneshot::channel();

            // Check the current block number
            let a = async move {
                let block_number = rpc_clone.block_number().await.unwrap_or(0);
                let syncing = rpc_clone.syncing().await.unwrap_or(true);

                let rax = InnerResult {
                    is_syncing: syncing,
                    reported_head: block_number,
                };

                let _ = send_tx.send(rax);
            };
            tokio::spawn(a);

            let result = timeout(Duration::from_millis(ttl.try_into().unwrap()), send_rx).await;

            let result = match result {
                Ok(Ok(response)) => response,
                // Handle timeout as failiure
                Err(_) | Ok(Err(_)) => {
                    InnerResult {
                        is_syncing: true,
                        reported_head: 0,
                    }
                }
            };

            let head_result = HeadResult {
                rpc_list_index: i,
                is_syncing: result.is_syncing,
                reported_head: result.reported_head,
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

/// Add unresponsive/erroring RPCs to the poverty list
fn make_poverty(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    heads: Vec<HeadResult>,
) -> Result<u64, HealthError> {
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
        if head.reported_head < highest_head || head.is_syncing {
            // Mark the RPC as erroring
            rpc_list_guard[head.rpc_list_index].status.is_erroring = true;
            log_wrn!(
                "{} is falling behind! Removing froma active RPC pool.",
                rpc_list_guard[head.rpc_list_index].name
            );

            // Add the RPC to the poverty list
            poverty_list_guard.push(rpc_list_guard[head.rpc_list_index].clone());
        }
    }

    // Go over rpc_list_guard and remove all erroring rpcs
    rpc_list_guard.retain(|rpc| !rpc.status.is_erroring);

    Ok(highest_head)
}

/// Go over the `poverty_list` to see if any nodes are back to normal.
///
/// Update liveness statuses when done.
fn escape_poverty(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_heads: Vec<HeadResult>,
    agreed_head: u64,
) -> Result<crate::LiveReadyUpdate, HealthError> {
    // Check if any nodes made it 🗣️🔥🔥🔥
    let mut poverty_list_guard = poverty_list.write().unwrap();
    let mut rpc_list_guard = rpc_list.write().unwrap();

    for head_result in poverty_heads {
        if head_result.reported_head >= agreed_head && !head_result.is_syncing {
            let mut rpc = poverty_list_guard[head_result.rpc_list_index].clone();
            rpc.status.is_erroring = false;
            log_info!(
                "{} is following the head again! Added to active RPC pool.",
                rpc.name
            );

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

    //todo: i dont like this but its whatever
    let to_send;
    let is_pov_empty = poverty_list_guard.is_empty();
    let is_rpc_empty = rpc_list_guard.is_empty();
    if !is_rpc_empty && is_pov_empty {
        to_send = LiveReadyUpdate::Health(HealthState::Healthy);
    } else if !is_pov_empty && !is_rpc_empty {
        to_send = LiveReadyUpdate::Health(HealthState::MissingRpcs);
    } else {
        to_send = LiveReadyUpdate::Health(HealthState::Unhealthy);
    }

    Ok(to_send)
}

/// Remove the RPC that dropped out ws_conn and add it to the poverty list.
pub async fn send_dropped_to_poverty(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    incoming_tx: &mpsc::UnboundedSender<WsconnMessage>,
    rx: broadcast::Receiver<IncomingResponse>,
    sub_data: &Arc<SubscriptionData>,
    ws_conn_index: usize,
) -> Result<(), HealthError> {
    {
        let mut rpc_list_guard = rpc_list.write().unwrap();
        let mut poverty_list_guard = poverty_list.write().unwrap();

        // Check if the RPC is in the rpc_list
        if let Some(rpc) = rpc_list_guard.get(ws_conn_index) {
            // Add the RPC to the poverty list
            poverty_list_guard.push(rpc.clone());

            // Remove the RPC from the rpc_list
            rpc_list_guard.remove(ws_conn_index);
        }
    }

    // Move subscriptions away from that node
    move_subscriptions(incoming_tx, rx, sub_data, ws_conn_index).await?;

    Ok(())
}

/// Listen for dropped ws connections and handle them.
pub async fn dropped_listener(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    poverty_list: Arc<RwLock<Vec<Rpc>>>,
    mut ws_err_rx: mpsc::UnboundedReceiver<WsChannelErr>,
    incoming_tx: mpsc::UnboundedSender<WsconnMessage>,
    rx: broadcast::Receiver<IncomingResponse>,
    sub_data: Arc<SubscriptionData>,
) -> Result<(), HealthError> {
    loop {
        let ws_err = ws_err_rx.recv().await;

        match ws_err {
            Some(WsChannelErr::Closed(index)) => {
                send_dropped_to_poverty(
                    &rpc_list,
                    &poverty_list,
                    &incoming_tx,
                    rx.resubscribe(),
                    &sub_data,
                    index,
                )
                .await
                .unwrap_or(());
                incoming_tx.send(WsconnMessage::Reconnect()).unwrap_or(());
            }
            None => {
                return Err(HealthError::InvalidResponse(
                    "Expected WsChannelErr!".to_string(),
                ))
            }
        };
    }
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
                is_syncing: false,
                reported_head: 18177557,
            },
            HeadResult {
                rpc_list_index: 1,
                is_syncing: false,
                reported_head: 18193012,
            },
            HeadResult {
                rpc_list_index: 2,
                is_syncing: false,
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
                is_syncing: false,
                reported_head: 18177557,
            },
            HeadResult {
                rpc_list_index: 1,
                is_syncing: false,
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

    #[test]
    fn test_escape_sync() {
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
                is_syncing: false,
                reported_head: 18193012,
            },
            HeadResult {
                rpc_list_index: 1,
                is_syncing: true,
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
