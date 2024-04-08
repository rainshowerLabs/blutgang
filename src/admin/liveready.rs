use crate::admin::metrics::{
    MetricReceiver,
    MetricSender,
};
use crate::RpcMetrics;
use prometheus::core::Collector;
use prometheus_metric_storage::StorageRegistry;
use std::fmt;
use std::time::Duration;
use std::{
    convert::Infallible,
    sync::{
        Arc,
        RwLock,
    },
};

use http_body_util::Full;

use tokio::sync::{
    mpsc,
    oneshot,
};

use hyper::body::Bytes;

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum ReadinessState {
    Ready,
    #[default]
    Setup,
}

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum HealthState {
    #[default]
    Healthy, // Everything nominal
    MissingRpcs, // Some RPCs are not following the head but otherwise ok
    Unhealthy,   // Nothing works
}

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub struct LiveReady {
    readiness: ReadinessState,
    health: HealthState,
}

#[derive(Debug, Clone)]
pub struct LiveReadyMetrics {
    readiness: ReadinessState,
    health: HealthState,
    metrics: RpcMetrics,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LiveReadyUpdate {
    Readiness(ReadinessState),
    Health(HealthState),
}

// These 2 are used to send and receive updates related to the current
// health of blutgang.
pub type LiveReadyUpdateRecv = mpsc::Receiver<LiveReadyUpdate>;
pub type LiveReadyUpdateSnd = mpsc::Sender<LiveReadyUpdate>;

// These are used to request/return updates about health
// pub type LiveReadyRecv = oneshot::Receiver<LiveReady>;
pub type LiveReadySnd = oneshot::Sender<LiveReady>;

pub type LiveReadyRequestRecv = mpsc::Receiver<LiveReadySnd>;
pub type LiveReadyRequestSnd = mpsc::Sender<LiveReadySnd>;

// #[cfg(feature = "prometheusd")]
pub type LRMetricsTx = oneshot::Sender<LiveReadyMetrics>;
// #[cfg(feature = "prometheusd")]
pub type LRMetricsRequestRx = mpsc::Receiver<LRMetricsTx>;
// #[cfg(feature = "prometheusd")]
pub type LRMetricsRequestTx = mpsc::Sender<LRMetricsTx>;

// Macros to make returning statuses less ugly in code
macro_rules! ok {
    () => {
        Ok(hyper::Response::builder()
            .status(200)
            .body(Full::new(Bytes::from("OK")))
            .unwrap())
    };
}

macro_rules! partial_ok {
    () => {
        Ok(hyper::Response::builder()
            .status(202)
            .body(Full::new(Bytes::from("RPC")))
            .unwrap())
    };
}

macro_rules! nok {
    () => {
        Ok(hyper::Response::builder()
            .status(503)
            .body(Full::new(Bytes::from("NOK")))
            .unwrap())
    };
}

// Listen for liveness update messages and update the current status accordingly
async fn liveness_listener(
    mut liveness_receiver: LiveReadyUpdateRecv,
    liveness_status: Arc<RwLock<LiveReady>>,
) {
    while let Some(update) = liveness_receiver.recv().await {
        match update {
            LiveReadyUpdate::Readiness(state) => {
                let mut liveness = liveness_status.write().unwrap();
                liveness.readiness = state;
            }
            LiveReadyUpdate::Health(state) => {
                let mut liveness = liveness_status.write().unwrap();
                liveness.health = state;
            }
        }
    }
}
// #[cfg(feature = "prometheusd")]
async fn liveness_listener_metrics(
    mut liveness_receiver: LiveReadyUpdateRecv,
    mut metrics_receiver: MetricReceiver,
    liveness_status: Arc<RwLock<LiveReadyMetrics>>,
) {
    while let Some(update) = liveness_receiver.recv().await {
        match update {
            LiveReadyUpdate::Readiness(state) => {
                let dt = std::time::Instant::now();
                let mut liveness = liveness_status.write().unwrap();
                liveness.readiness = state;
                //liveness.metrics.requests_complete(
                //    "/liveready_readiness",
                //    "LiveReadyUpdate::Readiness",
                //    //should I try using ok! macro here?
                //    &200,
                //    dt.elapsed(),
                // );
            }
            LiveReadyUpdate::Health(state) => {
                let dt = std::time::Instant::now();
                let mut liveness = liveness_status.write().unwrap();
                liveness.health = state;
                // liveness.metrics.requests_complete(
                //     "/liveready_health",
                //     "LiveReadyUpdate::Health",
                //     &200,
                //     dt.elapsed(),
                // );
            }
        }
    }
}

// Receives requests about current status updates and returns the current liveness
async fn liveness_request_processor(
    mut liveness_request_receiver: LiveReadyRequestRecv,
    liveness_status: Arc<RwLock<LiveReady>>,
) {
    loop {
        while let Some(incoming) = liveness_request_receiver.recv().await {
            let current_status = *liveness_status.read().unwrap();
            let _ = incoming.send(current_status);
        }
    }
}
#[cfg(feature = "prometheusd")]
async fn liveness_request_processor_metrics(
    mut liveness_request_receiver: LRMetricsRequestRx,
    mut metrics_receiver: MetricReceiver,
    liveness_status: Arc<RwLock<LiveReadyMetrics>>,
) {
    loop {
        while let Some(incoming) = liveness_request_receiver.recv().await {
            let current_status_health = liveness_status.read().unwrap().health;
            let current_status_readiness = liveness_status.read().unwrap().readiness;
            let current_status_metrics = liveness_status.read().unwrap().metrics.clone();
            let current_liveready = LiveReadyMetrics {
                health: current_status_health,
                readiness: current_status_readiness,
                metrics: current_status_metrics,
            };
            let _ = incoming.send(current_liveready);
        }
    }
}

// Monitor for new liveness updates and update the statuses accordingly.
//
// Also handles incoming requests about the current status.
pub(in crate::r#admin) async fn liveness_monitor(
    liveness_receiver: LiveReadyUpdateRecv,
    liveness_request_receiver: LiveReadyRequestRecv,
) {
    let liveness_status = Arc::new(RwLock::new(LiveReady::default()));

    // Spawn thread for listening and updating the current liveness status
    let liveness_status_listener = liveness_status.clone();
    tokio::spawn(liveness_listener(
        liveness_receiver,
        liveness_status_listener,
    ));

    // Listens to incoming requests about the current liveness
    liveness_request_processor(liveness_request_receiver, liveness_status).await;
}

pub async fn accept_readiness_request(
    liveness_request_sender: LiveReadyRequestSnd,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    let (tx, rx) = oneshot::channel();

    let _ = liveness_request_sender.send(tx).await;

    let rax = match rx.await {
        Ok(v) => v,
        Err(_) => {
            return nok!();
        }
    };

    if rax.readiness == ReadinessState::Ready {
        return ok!();
    }

    nok!()
}

//#[cfg(feature = "prometheusd")]
pub async fn on_accept_metrics_write_readiness(
    response: hyper::Response<Full<Bytes>>,
    LRMetrics: LiveReadyMetrics,
    dt: Duration,
) -> Result<(), Infallible> {
    let rax = match response.status().as_str() {
        "200" => {
            LRMetrics
                .metrics
                .requests_complete("/liveready_health", "Readiness::Ready", &200, dt);
        }
        "503" => {
            LRMetrics
                .metrics
                .requests_complete("/liveready_health", "Readiness::Setup", &503, dt);
        }
        _ => {
            LRMetrics.metrics.requests_complete(
                "/liveready_health",
                "Readiness::Unknown",
                &500,
                dt,
            );
        }
    };
    LRMetrics.metrics.requests.collect();
    LRMetrics.metrics.duration.collect();
    Ok(rax)
}

// #[cfg(feature = "prometheusd")]
pub async fn accept_readiness_request_metrics(
    liveness_request_sender: LRMetricsRequestTx,
    metrics_sender: MetricSender,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    use crate::admin::metrics::metrics_channel;
    //unbounded with metrics_channel or oneshot channel here?
    let dt = std::time::Instant::now();
    let (tx, rx) = oneshot::channel();

    let _ = liveness_request_sender.send(tx).await;

    let rax = match rx.await {
        Ok(v) => v,
        Err(_) => {
            return nok!();
        }
    };

    if rax.readiness == ReadinessState::Ready {
        return ok!();
    }
    // rax.metrics.requests_complete("liveready_readiness", &readiness_str, status, dt);

    nok!()
}

pub async fn accept_health_request(
    liveness_request_sender: LiveReadyRequestSnd,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    let (tx, rx) = oneshot::channel();

    let _ = liveness_request_sender.send(tx).await;

    let rax = match rx.await {
        Ok(v) => v,
        Err(_) => {
            return nok!();
        }
    };

    match rax.health {
        HealthState::Healthy => ok!(),
        HealthState::MissingRpcs => partial_ok!(),
        HealthState::Unhealthy => nok!(),
    }
}
#[cfg(feature = "prometheusd")]
pub async fn accept_health_request_metrics(
    liveness_request_sender: LiveReadyRequestSnd,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    use crate::admin::metrics::metrics_channel;
    let (tx, rx) = oneshot::channel();

    let _ = liveness_request_sender.send(tx).await;

    let rax = match rx.await {
        Ok(v) => v,
        Err(_) => {
            return nok!();
        }
    };

    match rax.health {
        HealthState::Healthy => ok!(),
        HealthState::MissingRpcs => partial_ok!(),
        HealthState::Unhealthy => nok!(),
    }
}

// Just a sink used to immediately discard request in cases where admin is disabled
pub async fn liveness_update_sink(mut liveness_rx: LiveReadyUpdateRecv) {
    loop {
        while (liveness_rx.recv().await).is_some() {
            continue;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rpc;
    use prometheus::core::Collector;

    use tokio::sync::{
        mpsc,
        oneshot,
    };
    use tokio::time::sleep;
    use tokio::time::Duration;
    // RUST_LOG=info cargo test --features prometheusd -- test_liveness_listener_with_metrics --nocapture
    #[cfg(feature = "prometheusd")]
    #[tokio::test]
    async fn test_liveness_listener_with_metrics() {
        use crate::admin::metrics::metrics_channel;
        use crate::log_info;
        let (metrics_tx, metrics_recv) = metrics_channel().await;
        let (update_snd, update_recv) = mpsc::channel(10);
        let storage = StorageRegistry::default();
        let _metrics = RpcMetrics::init(&storage).unwrap();
        let liveness_metrics_state = Arc::new(RwLock::new(LiveReadyMetrics {
            readiness: ReadinessState::Ready,
            health: HealthState::Healthy,
            metrics: _metrics.clone(),
        }));
        let liveness_metrics_clone = liveness_metrics_state.clone();
        tokio::spawn(async move {
            liveness_listener_metrics(update_recv, metrics_recv, liveness_metrics_clone).await;
        });
        metrics_tx.send(_metrics.clone()).unwrap();
        update_snd
            .send(LiveReadyUpdate::Readiness(ReadinessState::Ready))
            .await
            .unwrap();
        update_snd
            .send(LiveReadyUpdate::Health(HealthState::MissingRpcs))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await; // Give time for async updates
        log_info!(
            "metrics: {:?}",
            liveness_metrics_state.read().unwrap().metrics
        );
        log_info!("metrics_tx: {:?}", metrics_tx);
    }

    #[cfg(feature = "prometheusd")]
    #[tokio::test]
    // RUST_LOG=info cargo test --features prometheusd -- test_accept_readiness_metrics_request_correct_response --nocapture
    async fn test_accept_readiness_metrics_request_correct_response() {
        use crate::admin::metrics::metrics_channel;
        use crate::log_info;
        let (request_snd, request_recv) = mpsc::channel(1);
        let storage = StorageRegistry::default();
        let _metrics = RpcMetrics::init(&storage).unwrap();
        let dt = std::time::Instant::now();
        let (metrics_tx, metrics_recv) = metrics_channel().await;

        let liveness_status = Arc::new(RwLock::new(LiveReadyMetrics {
            readiness: ReadinessState::Ready,
            health: HealthState::Healthy,
            metrics: _metrics.clone(),
        }));
        tokio::spawn(liveness_request_processor_metrics(
            request_recv,
            metrics_recv,
            liveness_status.clone(),
        ));
        let response = accept_readiness_request_metrics(request_snd.clone(), metrics_tx.clone())
            .await
            .unwrap();
        let lr_metric = liveness_status.write().unwrap().clone();
        on_accept_metrics_write_readiness(response.clone(), lr_metric, dt.elapsed());
        log_info!(
            "readiness response status for healthy rpc: {:?}, on_accept_metrics_write_readiness: metrics: {:?}",
            response.status(),
            liveness_status.read().unwrap().metrics.requests.collect()
        );
        let dt = std::time::Instant::now();
        //TODO: Not sure if this is good
        let (tx, _rx) = oneshot::channel::<LiveReadyMetrics>();
        metrics_tx
            .send(liveness_status.read().unwrap().metrics.clone())
            .unwrap();
        liveness_status.write().unwrap().readiness = ReadinessState::Setup;
        liveness_status.write().unwrap().metrics.requests_complete(
            "/liveready_health",
            "LiveReadyUpdate::Health::Setup",
            &503,
            dt.elapsed(),
        );
        let response = accept_readiness_request_metrics(request_snd, metrics_tx)
            .await
            .unwrap();
        //TODO: Not sure if raw dogging metrics request field is good idea here
        log_info!(
            "readiness response status for setup: {:?}, metrics: {:?}",
            response.status(),
            liveness_status.read().unwrap().metrics.requests.collect()
        );
    }
    #[cfg(not(feature = "prometheusd"))]
    #[tokio::test]
    async fn test_metrics_sink_discards_updates() {
        let (tx, rx) = mpsc::channel(10);
        let storage = StorageRegistry::default();
        let _metrics = RpcMetrics::init(&storage).unwrap();
        let dt = std::time::Instant::now();
        let (metrics_tx, metrics_rx) = crate::admin::metrics::metrics_channel().await;

        // Simulate a sink that discards updates
        tokio::spawn(async move {
            crate::admin::metrics::metrics_update_sink(metrics_rx).await;
        });
        tx.send(LiveReadyUpdate::Readiness(ReadinessState::Ready))
            .await
            .unwrap();
        tx.send(LiveReadyUpdate::Health(HealthState::MissingRpcs))
            .await
            .unwrap();
        assert!(
            true,
            "Successfully discarded updates without affecting the test flow"
        );
    }
    #[tokio::test]
    async fn test_liveness_listener_updates_status() {
        let (update_snd, update_recv) = mpsc::channel(10);
        let liveness_status = Arc::new(RwLock::new(LiveReady::default()));

        // Simulate sending updates
        let liveness_status_clone = liveness_status.clone();
        tokio::spawn(async move {
            liveness_listener(update_recv, liveness_status_clone).await;
        });

        update_snd
            .send(LiveReadyUpdate::Readiness(ReadinessState::Ready))
            .await
            .unwrap();
        update_snd
            .send(LiveReadyUpdate::Health(HealthState::MissingRpcs))
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await; // Give time for async updates

        assert_eq!(
            liveness_status.read().unwrap().readiness,
            ReadinessState::Ready
        );
        assert_eq!(
            liveness_status.read().unwrap().health,
            HealthState::MissingRpcs
        );
    }

    #[tokio::test]
    async fn test_accept_readiness_request_returns_correct_response() {
        let (request_snd, request_recv) = mpsc::channel(1);
        let liveness_status = Arc::new(RwLock::new(LiveReady {
            readiness: ReadinessState::Ready,
            health: HealthState::Healthy,
        }));

        tokio::spawn(liveness_request_processor(
            request_recv,
            liveness_status.clone(),
        ));

        let response = accept_readiness_request(request_snd.clone()).await.unwrap();
        assert_eq!(response.status(), 200);

        // Testing with readiness set to Setup
        let (tx, _rx) = oneshot::channel();
        request_snd.send(tx).await.unwrap();
        liveness_status.write().unwrap().readiness = ReadinessState::Setup;
        let response = accept_readiness_request(request_snd).await.unwrap();
        assert_eq!(response.status(), 503);
    }

    #[tokio::test]
    async fn test_accept_health_request_returns_correct_response() {
        let (request_snd, request_recv) = mpsc::channel(1);
        let liveness_status = Arc::new(RwLock::new(LiveReady {
            readiness: ReadinessState::Ready,
            health: HealthState::Healthy,
        }));

        tokio::spawn(liveness_request_processor(
            request_recv,
            liveness_status.clone(),
        ));

        // Test with healthy state
        let response = accept_health_request(request_snd.clone()).await.unwrap();
        assert_eq!(response.status(), 200);

        // Test with MissingRpcs state
        let (tx, _rx) = oneshot::channel();
        request_snd.send(tx).await.unwrap();
        liveness_status.write().unwrap().health = HealthState::MissingRpcs;
        let response = accept_health_request(request_snd.clone()).await.unwrap();
        assert_eq!(response.status(), 202);

        // Test with Unhealthy state
        let (tx, _rx) = oneshot::channel();
        request_snd.send(tx).await.unwrap();
        liveness_status.write().unwrap().health = HealthState::Unhealthy;
        let response = accept_health_request(request_snd).await.unwrap();
        assert_eq!(response.status(), 503);
    }

    #[tokio::test]
    async fn test_liveness_update_sink_discards_updates() {
        let (update_snd, update_recv) = mpsc::channel(10);

        // Simulate a sink that discards updates
        tokio::spawn(async move {
            liveness_update_sink(update_recv).await;
        });

        update_snd
            .send(LiveReadyUpdate::Readiness(ReadinessState::Ready))
            .await
            .unwrap();
        update_snd
            .send(LiveReadyUpdate::Health(HealthState::MissingRpcs))
            .await
            .unwrap();

        // No assertion here as we're testing the sink's ability to simply discard incoming messages
        assert!(
            true,
            "Successfully discarded updates without affecting the test flow"
        );
    }

    #[tokio::test]
    async fn test_update_and_request_liveness_status_concurrently() {
        let (update_snd, update_recv) = mpsc::channel(10);
        let (request_snd, request_recv) = mpsc::channel(10);
        let liveness_status = Arc::new(RwLock::new(LiveReady::default()));
        let liveness_status_clone = liveness_status.clone();

        tokio::spawn(async move {
            liveness_listener(update_recv, liveness_status_clone).await;
        });
        tokio::spawn(async move {
            liveness_request_processor(request_recv, liveness_status).await;
        });

        // Send updates
        update_snd
            .send(LiveReadyUpdate::Readiness(ReadinessState::Ready))
            .await
            .unwrap();
        update_snd
            .send(LiveReadyUpdate::Health(HealthState::Unhealthy))
            .await
            .unwrap();

        // Request status immediately after sending updates
        let (response_tx, response_rx) = oneshot::channel();
        request_snd.send(response_tx).await.unwrap();

        // Ensure the status reflects the last update sent
        let received_status = response_rx.await.expect("Failed to receive response");
        assert_eq!(received_status.readiness, ReadinessState::Ready);
        assert_eq!(received_status.health, HealthState::Unhealthy);

        // Send another set of updates and request again
        update_snd
            .send(LiveReadyUpdate::Health(HealthState::Healthy))
            .await
            .unwrap();
        let (new_response_tx, new_response_rx) = oneshot::channel();
        request_snd.send(new_response_tx).await.unwrap();

        let new_received_status = new_response_rx
            .await
            .expect("Failed to receive new response");
        assert_eq!(new_received_status.health, HealthState::Healthy);

        // Testing edge cases
        // Sending None update (Shouldn't change the status)
        update_snd
            .send(LiveReadyUpdate::Health(HealthState::Healthy))
            .await
            .unwrap();
        let (edge_response_tx, edge_response_rx) = oneshot::channel();
        request_snd.send(edge_response_tx).await.unwrap();

        let edge_received_status = edge_response_rx
            .await
            .expect("Failed to receive edge response");
        assert_eq!(edge_received_status.readiness, ReadinessState::Ready);
        assert_eq!(edge_received_status.health, HealthState::Healthy);
    }

    #[tokio::test]
    async fn test_empty_updates_does_not_change_status() {
        let (update_snd, update_recv) = mpsc::channel(1);
        let liveness_status = Arc::new(RwLock::new(LiveReady {
            readiness: ReadinessState::Ready,
            health: HealthState::Healthy,
        }));

        let liveness_status_clone = liveness_status.clone();

        tokio::spawn(async move {
            liveness_listener(update_recv, liveness_status_clone).await;
        });

        // Intentionally not sending any updates
        drop(update_snd);

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await; // Give time for any potential updates

        assert_eq!(
            liveness_status.read().unwrap().readiness,
            ReadinessState::Ready
        );
        assert_eq!(liveness_status.read().unwrap().health, HealthState::Healthy);
    }

    #[tokio::test]
    async fn test_health_state_transitions() {
        let rpc_list = Arc::new(RwLock::new(vec![Rpc::default(), Rpc::default()]));
        let poverty_list = Arc::new(RwLock::new(Vec::new()));
        let (update_snd, update_recv) = mpsc::channel(10);
        let liveness_status = Arc::new(RwLock::new(LiveReady::default()));

        let liveness_status_clone = liveness_status.clone();

        tokio::spawn(async move {
            liveness_listener(update_recv, liveness_status_clone).await;
        });

        // Force add RPCs to poverty list and send health update
        let mut rpcs_in_poverty = rpc_list.write().unwrap().drain(..).collect::<Vec<_>>();
        poverty_list.write().unwrap().append(&mut rpcs_in_poverty);
        update_snd
            .send(LiveReadyUpdate::Health(HealthState::Unhealthy))
            .await
            .unwrap();

        // Check for Unhealthy status
        sleep(Duration::from_millis(50)).await; // Allow for processing
        assert_eq!(
            liveness_status.read().unwrap().health,
            HealthState::Unhealthy
        );
        assert!(rpc_list.read().unwrap().is_empty());
        assert_eq!(poverty_list.read().unwrap().len(), 2);

        // Remove RPCs from poverty list, simulate recovery, and send health update
        let mut recovered_rpcs = poverty_list.write().unwrap().drain(..).collect::<Vec<_>>();
        rpc_list.write().unwrap().append(&mut recovered_rpcs);
        update_snd
            .send(LiveReadyUpdate::Health(HealthState::Healthy))
            .await
            .unwrap();

        // Check for Healthy status
        sleep(Duration::from_millis(50)).await; // Allow for processing
        assert_eq!(liveness_status.read().unwrap().health, HealthState::Healthy);
        assert_eq!(rpc_list.read().unwrap().len(), 2);
        assert!(poverty_list.read().unwrap().is_empty());

        // Reverse the process: simulate RPCs failing again and moving back to poverty list
        let mut rpcs_in_poverty_again = rpc_list.write().unwrap().drain(..).collect::<Vec<_>>();
        poverty_list
            .write()
            .unwrap()
            .append(&mut rpcs_in_poverty_again);
        update_snd
            .send(LiveReadyUpdate::Health(HealthState::Unhealthy))
            .await
            .unwrap();

        // Check for Unhealthy status again
        sleep(Duration::from_millis(50)).await; // Allow for processing
        assert_eq!(
            liveness_status.read().unwrap().health,
            HealthState::Unhealthy
        );
        assert!(rpc_list.read().unwrap().is_empty());
        assert_eq!(poverty_list.read().unwrap().len(), 2);

        // Recover again and verify
        let mut recovered_rpcs_again = poverty_list.write().unwrap().drain(..).collect::<Vec<_>>();
        rpc_list.write().unwrap().append(&mut recovered_rpcs_again);
        update_snd
            .send(LiveReadyUpdate::Health(HealthState::Healthy))
            .await
            .unwrap();

        // Final check for Healthy status
        sleep(Duration::from_millis(50)).await; // Allow for processing
        assert_eq!(liveness_status.read().unwrap().health, HealthState::Healthy);
        assert_eq!(rpc_list.read().unwrap().len(), 2);
        assert!(poverty_list.read().unwrap().is_empty());
    }
}
