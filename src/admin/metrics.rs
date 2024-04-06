use prometheus_metric_storage::{
    MetricStorage,
    StorageRegistry,
};
use hyper::Request;
use std::convert::Infallible;
use std::{
    collections::hash_map::HashMap,
    sync::{
        Arc,
        RwLock,
        
    },
    time::Duration,
};

use tokio::sync::{
    oneshot,
    mpsc::{
        UnboundedReceiver,
        UnboundedSender,
        unbounded_channel,
    },
};

use thiserror::Error;

pub type MetricSender = UnboundedSender<RpcMetrics>;
pub type MetricReceiver = UnboundedReceiver<RpcMetrics>;

// #[cfg(feature = "prometheusd")]
#[derive(Debug, Error)]
pub enum MetricsError {
    #[error(transparent)]
    WSError(#[from] crate::websocket::error::WsError),
    #[error(transparent)]
    RPCError(#[from] crate::rpc::error::RpcError),
}

// #[cfg(feature = "prometheusd")]
impl From<MetricsError> for String {
    fn from(error: MetricsError) -> Self {
        error.to_string()
    }
}
#[derive(MetricStorage, Clone, Debug)]
#[metric(subsystem = "rpc")]
pub struct RpcMetrics {
    #[metric(labels("path", "method", "status"), help = "Total number of requests")]
    pub requests: prometheus::IntCounterVec,
    #[metric(labels("path", "method"), help = "latency of request")]
    pub duration: prometheus::HistogramVec,
}
impl RpcMetrics {
    pub fn init(registry: &StorageRegistry) -> Result<&Self, prometheus::Error> {
        RpcMetrics::instance(registry)
    }
    pub fn requests_complete(&self, path: &str, method: &str, status: &u16, dt: Duration) {
        self.requests
            .with_label_values(&[path, method, &status.to_string()])
            .inc();
        self.duration
            .with_label_values(&[path, method])
            .observe(dt.as_millis() as f64)
    }
}
#[cfg(feature = "prometheusd")]
#[derive(Debug)]
pub enum MetricsCommand<'a> {
    Flush(),
    Channel(&'a MetricSender, &'a MetricReceiver),
    PushLatency(&'a RpcMetrics, &'a str, &'a str, f64),
    PushRequest(&'a RpcMetrics, &'a str, &'a str, &'a u16, Duration),
    PushError(&'a RpcMetrics, &'a str, &'a str, &'a u16, Duration),
    Push(&'a RpcMetrics),
    Pull(&'a RpcMetrics),
}

#[cfg(feature = "prometheusd")]
#[derive(Debug)]
pub enum MetricChannelCommand {
    AdminMsg(oneshot::Sender<RpcMetrics>),
    StatsMsg(oneshot::Sender<RpcMetrics>),
}

#[cfg(not(feature = "prometheusd"))]
pub async fn metrics_update_sink(mut metrics_rx: MetricReceiver) {
    loop {
        while metrics_rx.recv().await.is_some() {
            continue;
        }
    }
}
#[cfg(feature = "prometheusd")]
pub async fn metrics_listener(
    mut metrics_rx: MetricReceiver,
    metrics_status: Arc<RwLock<RpcMetrics>>,
) {
    while let Some(update) = metrics_rx.recv().await {
        //TODO: match for metrics type, latency, errors, requests
        //match metric {
        //MetricsType::Latency => {
        //}
        let mut metrics = metrics_status.write().unwrap();
        *metrics = update;
    }
}
#[cfg(feature = "prometheusd")]
async fn metrics_processor(
    mut metrics_rx: MetricReceiver,
    registry_state: Arc<RwLock<StorageRegistry>>,
    // path: &str,
    // method: &str,
    // status: &u16,
    // duration: Duration,
) {
    use crate::log_info;
    let _metrics = RpcMetrics::init(&registry_state.read().unwrap()).unwrap();
    let mut interval = tokio::time::interval(Duration::from_millis(10));
    loop {
        interval.tick().await;
        while let Some(incoming) = metrics_rx.recv().await {
            incoming.requests_complete("test", "test", &200, Duration::from_millis(100));
            let test_report = metrics_encoder(registry_state.clone()).await;
            log_info!("prometheus metrics: {:?}", test_report);
            let _registry = registry_state.read().unwrap();
            // incoming.requests_complete(path, method, status, duration);
        }
    }
}
/// Accepts metrics request, encodes and prints
#[cfg(feature = "prometheusd")]
async fn accept_metrics_request(
    tx: Request<hyper::body::Incoming>,
    metrics_tx: Arc<MetricSender>,
    registry_state: Arc<RwLock<StorageRegistry>>,
) -> Result<String, Infallible> {
    if tx.uri().path() == "/ws_metrics" {
        return accept_ws_metrics(metrics_tx).await;
    } else if tx.uri().path() == "/http_metrics" {
        return accept_http_metrics(metrics_tx).await;
    }
    let metrics = metrics_encoder(registry_state).await;
    Ok(metrics)
}
#[cfg(feature = "prometheusd")]
async fn accept_ws_metrics(_metrics_tx: Arc<MetricSender>) -> Result<String, Infallible> {
    unimplemented!()
}
#[cfg(feature = "prometheusd")]
async fn accept_http_metrics(_metrics_tx: Arc<MetricSender>) -> Result<String, Infallible> {
    unimplemented!()
}
#[cfg(feature = "prometheusd")]
async fn metrics_encoder(storage_registry: Arc<RwLock<StorageRegistry>>) -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let mut buffer = vec![];
    let registry = storage_registry.read().unwrap().gather();
    encoder.encode(&registry, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}
#[cfg(feature = "prometheusd")]
pub async fn metrics_monitor(metrics_rx: MetricReceiver, storage_registry: StorageRegistry) {
    //TODO: figure ownership mess here
    let metrics_status = Arc::new(RwLock::new(
        RpcMetrics::instance(&storage_registry).unwrap().to_owned(),
    ));
    let metrics_stat_listener = metrics_status.clone();
    tokio::spawn(metrics_listener(metrics_rx, metrics_stat_listener));
    //metrics_processor(metrics_rx, metrics_status, "test", "test", &200, Duration::from_millis(100)).await;
}
// #[cfg(feature = "prometheusd")]
pub async fn metrics_channel() -> (MetricSender, MetricReceiver) {
    let (tx, rx) = unbounded_channel();
    (tx, rx)
}

#[cfg(feature = "prometheusd")]
pub async fn close(rx: &mut MetricReceiver) {
rx.close();
}
#[cfg(feature = "prometheusd")]
macro_rules! accept_prometheusd {
    (
     $io:expr,
     $http_tx:expr,
     $ws_tx:expr,
     $registry_state:expr,
     $metrics_request_tx:expr,
    ) => {
        if let Err(err) = http1::Builder::new()
            .serve_connection(
                $io,
                service_fn(|req| {
                    let response = accept_metrics_request(
                        req,
                        $http_tx,
                        $ws_tx,
                        $registry_state,
                        $metrics_request_tx.clone(),
                    );
                    response
                }),
            )
            .await
        {
            println!("Error serving prometheus metrics: {:?}", err);
        }
    };
}

pub mod test_mocks {
    use super::*;
    use crate::Rpc;
    //borrowed from admin
    fn create_test_rpc_list() -> Arc<RwLock<Vec<Rpc>>> {
        Arc::new(RwLock::new(vec![Rpc::new(
            "http://example.com".to_string(),
            None,
            5,
            1000,
            0.5,
        )]))
    }

    pub struct MockMetrics {
        pub rpc_list: Arc<RwLock<Vec<Rpc>>>,
        pub requests: HashMap<String, u64>,
        pub duration: HashMap<String, f64>,
    }
}

#[cfg(test)]
mod tests {

    //TODO: remove this after tests
    //sorry, im too lazy to make proper tests for this

    #[tokio::test]
    async fn test_prometheus_log() {
        // let rpc1 = Rpc::default();
        // let registry  = get_storage_registry();
        // let metrics = RpcMetrics::inst(registry);
        // let expected = "prometheus_metrics";
        // assert_eq!(report, expected);
        todo!()
    }
}
