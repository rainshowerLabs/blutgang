use http_body_util::Full;
use hyper::{
    body::Bytes,
    server::conn::http1,
    service::service_fn,
    Request,
};
use prometheus_metric_storage::{
    MetricStorage,
    StorageRegistry,
};
use std::convert::Infallible;
use std::{
    collections::hash_map::HashMap,
    sync::{
        Arc,
        RwLock,
    },
    time::Duration,
};

use crate::Settings;
use tokio::sync::{
    mpsc::{
        unbounded_channel,
        UnboundedReceiver,
        UnboundedSender,
    },
    oneshot,
};
use tokio::time::interval;
type CounterMap = HashMap<(String, u64), RpcMetrics>;
pub type MetricSender = UnboundedSender<RpcMetrics>;
pub type MetricReceiver = UnboundedReceiver<RpcMetrics>;
const VERSION_LABEL: [(&str, &str); 1] = [("version", env!("CARGO_PKG_VERSION"))];
// #[cfg(feature = "prometheusd")]
#[derive(Debug)]
pub enum MetricsError {
    WSError(String),
    RpcError(String),
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
pub async fn listen_for_metrics_requests(
    config: Arc<RwLock<Settings>>,
    metrics_rx: MetricReceiver,
    registry_status: Arc<RwLock<StorageRegistry>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::config::{
        cache_setup::setup_data,
        cli_args::create_match,
    };
    let (address, interval) = {
        let config_guard = config.read().unwrap();
        (
            config_guard.metrics.address,
            config_guard.metrics.count_update_interval,
        )
    };
    let (metrics_request_tx, metrics_request_rx) = metrics_channel().await;
    // let registry = registry_status.read().unwrap();
    let _metrics_request_tx = Arc::new(RwLock::new(metrics_request_tx.clone()));
    // tokio::spawn(metrics_monitor(metrics_rx, registry));
    metrics_server(
        _metrics_request_tx,
        registry_status,
        metrics_request_tx,
        address,
        config,
    )
    .await
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
pub async fn metrics_processor(
    mut metrics_rx: MetricReceiver,
    registry_state: Arc<RwLock<StorageRegistry>>,
) {
    use crate::log_info;
    let _metrics = RpcMetrics::init(&registry_state.read().unwrap()).unwrap();
    loop {
        while let Some(incoming) = metrics_rx.recv().await {
            incoming.requests_complete("test", "test", &200, Duration::from_millis(100));
            let test_report = metrics_encoder(registry_state.clone()).await;
            log_info!("prometheus metrics: {:?}", test_report);
            let _registry = registry_state.read().unwrap();
        }
    }
}
/// Accepts metrics request, encodes and prints
#[cfg(feature = "prometheusd")]
pub async fn accept_metrics_request(
    tx: Request<hyper::body::Incoming>,
    metrics_tx: Arc<RwLock<MetricSender>>,
    registry_state: Arc<RwLock<StorageRegistry>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    use crate::balancer::format::incoming_to_value;
    use serde_json::{
        json,
        Value,
        Value::Null,
    };

    let metrics_report = metrics_encoder(registry_state).await;
    let response = Ok(hyper::Response::builder()
        .status(200)
        .body(Full::from(Bytes::from(metrics_report)))
        .unwrap());
    (response)
}
#[cfg(feature = "prometheusd")]
async fn accept_ws_metrics(
    _metrics_tx: Arc<MetricSender>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    unimplemented!()
}
#[cfg(feature = "prometheusd")]
async fn accept_http_metrics(
    _metrics_tx: Arc<MetricSender>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
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
pub async fn metrics_monitor(
    metrics_rx: MetricReceiver,
    storage_registry: Arc<RwLock<StorageRegistry>>,
) {
    //TODO: figure ownership mess here
    let registry;
    let registry_guard = storage_registry.read().unwrap();
    registry = registry_guard;
    let metrics_status = Arc::new(RwLock::new(
        RpcMetrics::instance(&registry).unwrap().to_owned(),
    ));
    let metrics_stat_listener = metrics_status.clone();
    tokio::spawn(metrics_listener(metrics_rx, metrics_stat_listener));
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
pub async fn metrics_server(
    metrics_tx: Arc<RwLock<MetricSender>>,
    registry_state: Arc<RwLock<StorageRegistry>>,
    metrics_request_tx: MetricSender,
    address: std::net::SocketAddr,
    config: Arc<RwLock<Settings>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::accept_prometheusd;
    use crate::log_info;
    use hyper_util_blutgang::rt::TokioIo;
    use tokio::{
        net::TcpListener,
        sync::mpsc,
    };
    let update_interval = {
        let config_guard = config.read().unwrap();
        config_guard.metrics.count_update_interval
    };
    let listener = TcpListener::bind(address).await?;
    log_info!("Bound metrics to : {}", address);
    let mut interval = tokio::time::interval(Duration::from_secs(update_interval));

    loop {
        //TODO: add interval here
        interval.tick().await;
        let (stream, socket_addr) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let tx_clone = Arc::clone(&metrics_tx);
        let registry_clone = Arc::clone(&registry_state);
        tokio::task::spawn(async move {
            accept_prometheusd!(io, &tx_clone, &registry_clone, metrics_request_tx,);
        });
    }
}

// #[cfg(feature = "prometheusd")]
// #[macro_export]
// macro_rules! set_metric {
//     ($data: expr, $version:expr  $type_label:expr) => {

//     };
// }

#[cfg(feature = "prometheusd")]
#[macro_export]
macro_rules! accept_prometheusd {
    (
     $io:expr,
     $http_tx:expr,
     $registry_state:expr,
     $metrics_request_tx:expr,
    ) => {
        use crate::admin::metrics::accept_metrics_request;
        if let Err(err) = http1::Builder::new()
            .serve_connection(
                $io,
                service_fn(|req| {
                    let response = accept_metrics_request(
                        req,
                        Arc::clone($http_tx),
                        Arc::clone($registry_state),
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
    use super::*;
    use crate::log_info;
    #[cfg(feature = "prometheusd")]
    #[tokio::test]
    //RUST_LOG=info cargo test --features prometheusd -- test_prometheus_listener --nocapture
    async fn test_prometheus_listener() {
        let storage = StorageRegistry::default();
        let storage_arc = Arc::new(RwLock::new(storage));
        let dt = std::time::Instant::now();
        log_info!(
            "Initial metrics state: {:?}",
            storage_arc.read().unwrap().gather()
        );
        let dt = std::time::Instant::now();
        let (metrics_tx, metrics_rx) = metrics_channel().await;
        let storage_clone = Arc::clone(&storage_arc);
        let storage_guard = storage_arc.read().unwrap();
        let mut rpc_metrics = RpcMetrics::init(&storage_guard).unwrap();
        rpc_metrics.requests_complete("test", "test", &200, dt.elapsed());
        listen_for_metrics_requests(metrics_rx, storage_clone);
        let test_report = metrics_encoder(storage_arc.clone()).await;
        log_info!("metrics state: {:?}", test_report);
    }
    #[cfg(feature = "prometheusd")]
    #[tokio::test]
    //RUST_LOG=info cargo test --features prometheusd -- test_prometheus_server --nocapture
    async fn test_prometheus_server() {
        use crate::config::types::Settings;
        use crate::create_match;
        use hyper_util_blutgang::rt::TokioIo;
        use tokio::net::TcpListener;

        let storage = StorageRegistry::default();
        let storage_arc = Arc::new(RwLock::new(storage));
        let dt = std::time::Instant::now();
        log_info!(
            "Initial metrics state: {:?}",
            storage_arc.read().unwrap().gather()
        );
        let dt = std::time::Instant::now();
        let (metrics_tx, metrics_rx) = metrics_channel().await;
        let storage_clone = Arc::clone(&storage_arc);
        let storage_guard = storage_arc.read().unwrap();
        let mut rpc_metrics = RpcMetrics::init(&storage_guard).unwrap();
        for _ in 0..3 {
            rpc_metrics.requests_complete("test", "test", &200, dt.elapsed());
            // listen_for_metrics_requests(metrics_rx, storage_clone.clone());
            let test_report = metrics_encoder(storage_arc.clone()).await;
            log_info!("metrics state: {:?}", test_report);
        }
    }
}
