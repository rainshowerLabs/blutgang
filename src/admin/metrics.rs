use crate::Rpc;
use hex::encode;
use http_body_util::Full;
use hyper::body;
use hyper::server::conn::http1;
use hyper::{
    body::Bytes,
    body::Incoming,
    service::service_fn,
    Request,
};
use measured::text::TextEncoder;
use prometheus::{
    histogram_opts,
    opts,
    register_histogram,
    register_histogram_vec,
    Error,
};
use prometheus_metric_storage::{
    MetricStorage,
    StorageRegistry,
};
use serde_json::{
    json,
    Value,
    Value::Null,
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

use crate::{
    admin::error::AdminError,
    Settings,
};

use measured::{
    text::BufferedTextEncoder,
    MetricGroup,
};
use tokio::sync::{
    mpsc::{
        unbounded_channel,
        UnboundedReceiver,
        UnboundedSender,
    },
    oneshot,
};
use tokio::time::interval;
//TODO: have fns accept a refernce to StorageRegistry
// refer to https://docs.rs/prometheus-metric-storage/latest/prometheus_metric_storage/#metric-storage-registry
type CounterMap = HashMap<(String, u64), RpcMetrics>;
pub type MetricSender = UnboundedSender<RpcMetrics>;
pub type MetricReceiver = UnboundedReceiver<RpcMetrics>;
pub type MetricUpdateSender = tokio::sync::mpsc::Sender<MetricsUpdate>;
pub type MetricUpdateReciever = tokio::sync::mpsc::Receiver<MetricsUpdate>;

const VERSION_LABEL: [(&str, &str); 1] = [("version", env!("CARGO_PKG_VERSION"))];

// #[cfg(feature = "prometheusd")]
// pub(crate) async fn metrics_service(
//     registry_rwlock: Arc<RwLock<StorageRegistry>>,
//     config_rwlock: Arc<RwLock<Settings>>,
// ) {
//     use crate::log_info;
//     use hyper_util_blutgang::rt::TokioIo;
//     use tokio::{
//         net::TcpListener,
//         sync::mpsc,
//     };

//     let (metrics_request_tx, metrics_request_rx) = metrics_update_channel().await;
//     let (metrics_tx, metrics_rx) = metrics_channel().await;
//     let (addr, update_interval) = {
//         let config_guard = config_rwlock.read().unwrap();
//         (
//             config_guard.metrics.address,
//             config_guard.metrics.count_update_interval,
//         )
//     };
//     let registry_guard = registry_rwlock.read().unwrap();
//     let metrics = RpcMetrics::new("metrics_service").to_owned();
//     let metrics_rwlock = Arc::new(RwLock::new(metrics));
//     let encoder_rwlock = Arc::new(RwLock::new(BufferedTextEncoder::new()));
//     let service = PrometheusHandle::new(
//         encoder_rwlock,
//         metrics_rwlock,
//         registry_rwlock.clone(),
//         config_rwlock,
//     );
//     let listener = TcpListener::bind(addr).await;

//     log_info!("Bound metrics to : {}", addr);
//     let mut interval = tokio::time::interval(Duration::from_secs(update_interval));
//     let (stream) = listener.unwrap().accept().await;
//     let io = TokioIo::new(stream.unwrap());
//     // http1::Builder::new()
//     //     .serve_connection(io , service_fn(|req| {
//     //         let response = metrics_handler(service.clone());
//     //         response
//     //                 });
// }

#[derive(Clone, Debug)]
pub struct RpcMetrics {
    pub duration: prometheus::HistogramVec,
}
impl RpcMetrics {
    pub fn new(label: &'static str) -> Self {
        let opts = histogram_opts!(label, "RPC request latency");
        Self {
            duration: register_histogram_vec!(opts, &["url", label],).unwrap(),
        }
    }
    pub fn requests_complete(&self, url: &str, method: &str, status: &str, dt: Duration) {
        self.duration
            .with_label_values(&[url, method])
            .observe(dt.as_millis() as f64)
    }
}
pub async fn forward_metrics(
    tx: Request<body::Incoming>,
    metrics: Arc<RwLock<RpcMetrics>>,
) -> Result<hyper::Response<Full<Bytes>>, hyper::Error> {
    use prometheus::{
        Encoder,
        TextEncoder,
    };
    let encoder = TextEncoder::new();
    let metrics_guard = metrics.read().unwrap();

    // let mut tx = crate::balancer::format::incoming_to_value(tx).await.unwrap();

    let dt = metrics_guard
        .duration
        .with_label_values(&["url", "method"])
        .start_timer();
    let registry = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&registry, &mut buffer).unwrap();
    let body = hyper::body::Bytes::from(buffer);
    let body = Full::from(body);
    let response = hyper::Response::builder().status(200).body(body).unwrap();
    dt.observe_duration();
    Ok(response)
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MetricsUpdate {
    Http,
    Websocket,
    Database,
}

#[cfg(not(feature = "prometheusd"))]
pub async fn metrics_update_sink(mut metrics_rx: MetricUpdateReciever) {
    loop {
        while metrics_rx.recv().await.is_some() {
            continue;
        }
    }
}

// #[cfg(feature = "prometheusd")]
// pub async fn listen_for_metrics_requests(
//     config: Arc<RwLock<Settings>>,
//     rpc_list: Arc<RwLock<Vec<Rpc>>>,
// ) -> Result<(), Box<dyn std::error::Error>> {
//     use crate::config::{
//         cache_setup::setup_data,
//         cli_args::create_match,
//     };
//     use crate::log_info;
//     let (address, interval) = {
//         let config_guard = config.read().unwrap();
//         (
//             config_guard.metrics.address,
//             config_guard.metrics.count_update_interval,
//         )
//     };

//     let (metrics_request_tx, metrics_request_rx) = metrics_update_channel().await;
//     let (metrics_tx, metrics_rx) = metrics_channel().await;
//     let metrics_request_tx_rwlock = Arc::new(RwLock::new(metrics_request_tx.clone()));
//     let metrics_tx_rwlock = Arc::new(RwLock::new(metrics_tx.clone()));
//     tokio::spawn(metrics_monitor(
//         metrics_request_rx,
//         // registry_status.clone(),
//         rpc_list.clone(),
//         rpc_position,
//         dt,
//     ));
//     metrics_server(
//         metrics_tx_rwlock,
//         // registry_status.clone(),
//         metrics_tx,
//         address,
//         interval,
//     )
//     .await;
//     let metrics_report = forward_metrics(registry_status).await;
//     log_info!("metrics response: {:?}", metrics_report);
//     Ok(())
// }
//#[cfg(feature = "prometheusd")]
//pub async fn metrics_listener(
//    mut metrics_rx: MetricUpdateReciever,
//    metrics_status: Arc<RwLock<RpcMetrics>>,
//    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
//    position: usize,
//    dt: Duration,
//) {
//    while let Some(update) = metrics_rx.recv().await {
//        match update {
//            MetricsUpdate::Http => {
//                let mut metrics_guard = metrics_status.write().unwrap();
//                let rpc_list_guard = rpc_list.read().unwrap_or_else(|e| e.into_inner());
//                metrics_guard.requests_complete(
//                    &rpc_list_guard[position].name,
//                    &"http",
//                    &"200",
//                    dt,
//                );
//            }
//            MetricsUpdate::Websocket => {
//                let mut metrics_guard = metrics_status.write().unwrap();
//                let rpc_list_guard = rpc_list.read().unwrap_or_else(|e| e.into_inner());
//                metrics_guard.requests_complete(
//                    &rpc_list_guard[position].name,
//                    &"WebSocket",
//                    &"200",
//                    dt,
//                );
//            }
//            MetricsUpdate::Database => {
//                let mut metrics_guard = metrics_status.write().unwrap();
//                let rpc_list_guard = rpc_list.read().unwrap_or_else(|e| e.into_inner());
//                metrics_guard.requests_complete(
//                    &rpc_list_guard[position].name,
//                    &"Database",
//                    &"200",
//                    dt,
//                );
//            }
//        }
//    }
//}
///// Matches for command, accepts metrics request, encodes and prints
//#[cfg(feature = "prometheusd")]
///// Accepts metrics request, encodes and prints
//#[cfg(feature = "prometheusd")]
//pub async fn write_metrics_response(
//    tx: Request<hyper::body::Incoming>,
//    metrics_tx: Arc<RwLock<MetricSender>>,
//    // registry_state: Arc<RwLock<StorageRegistry>>,
//) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
//    use crate::{
//        balancer::format::incoming_to_value,
//        log_info,
//    };
//    use serde_json::{
//        json,
//        Value,
//        Value::Null,
//    };

//    let metrics_report = metrics_encoder(registry_state).await;
//    let response = Ok(hyper::Response::builder()
//        .status(200)
//        .body(Full::from(Bytes::from(metrics_report)))
//        .unwrap());
//    log_info!("metrics response: {:?}", response);
//    (response)
//}
//#[cfg(feature = "prometheusd")]
//async fn metrics_encoder() -> String {
//    use prometheus::Encoder;
//    let encoder = prometheus::TextEncoder::new();
//    let mut buffer = vec![];
//    let registry = prometheus::gather();
//    encoder.encode(&registry, &mut buffer).unwrap();
//    String::from_utf8(buffer).unwrap()
//}

////listens for updates to metrics and updates the storage registry
//#[cfg(feature = "prometheusd")]
//pub(in crate::r#admin) async fn metrics_monitor(
//    metrics_rx: MetricUpdateReciever,
//    // storage_registry: Arc<RwLock<StorageRegistry>>,
//    rpc_list: Arc<RwLock<Vec<Rpc>>>,
//    rpc_postion: usize,
//    dt: Duration,
//) {
//    // let registry;
//    // let registry_guard = storage_registry.read().unwrap();
//    // registry = registry_guard;
//    let metrics_status = Arc::new(RwLock::new(
//        RpcMetrics::new("metrics_monitor").unwrap().to_owned(),
//    ));
//    let metrics_stat_listener = metrics_status.clone();
//    let rpc_list_clone = Arc::clone(&rpc_list);
//    tokio::spawn(async move {
//        let _ = metrics_listener(
//            metrics_rx,
//            metrics_stat_listener,
//            &rpc_list_clone,
//            rpc_postion,
//            dt,
//        )
//        .await;
//    });
//}
pub async fn metrics_channel() -> (MetricSender, MetricReceiver) {
    let (tx, rx) = unbounded_channel();
    (tx, rx)
}

pub async fn metrics_update_channel() -> (MetricUpdateSender, MetricUpdateReciever) {
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    (tx, rx)
}

#[cfg(feature = "prometheusd")]
pub async fn metrics_server(
    metrics_rw: Arc<RwLock<RpcMetrics>>,
    address: std::net::SocketAddr,
    update_interval: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::accept_prometheusd;
    use crate::log_info;
    use hyper_util_blutgang::rt::TokioIo;
    use tokio::{
        net::TcpListener,
        sync::mpsc,
    };
    let listener = TcpListener::bind(address).await?;
    log_info!("Bound metrics to : {}", address);
    let mut interval = tokio::time::interval(Duration::from_secs(update_interval));
    loop {
        interval.tick().await;
        let (stream, socketaddr) = listener.accept().await?;
        log_info!("Metrics connection from: {}", socketaddr);
        let io = TokioIo::new(stream);
        let metrics_rw_clone = Arc::clone(&metrics_rw);

        tokio::task::spawn(async move {
            accept_prometheusd!(io, &metrics_rw_clone,);
        });
    }
}

#[cfg(feature = "prometheusd")]
#[macro_export]
macro_rules! accept_prometheusd {
    (
     $io:expr,
     $metrics: expr,
    ) => {
        if let Err(err) = http1::Builder::new()
            .serve_connection(
                $io,
                service_fn(|req| {
                    let response = forward_metrics(req, Arc::clone($metrics));
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
    use crate::admin::metrics::RpcMetrics;
    use crate::Rpc;
    use rand::Rng;
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

    #[derive(Debug)]
    struct MockRpcs {
        pub rpc_list: Arc<RwLock<Vec<Rpc>>>,
    }

    #[derive(Debug)]
    pub struct MockRpcMetrics {
        pub inner: RpcMetrics,
    }

    impl MockRpcMetrics {
        fn gen_metrics(&self, mut rng: rand::rngs::StdRng) {
            for _ in 0..5 {
                let rand_status = rng.gen_range(0..=2);
                let rand_duration = rng.gen_range(1..=100);
                match rand_status {
                    0 => {
                        self.inner.requests_complete(
                            "test",
                            "test",
                            &"200",
                            Duration::from_millis(rand_duration),
                        )
                    }
                    1 => {
                        self.inner.requests_complete(
                            "test",
                            "test",
                            &"202",
                            Duration::from_millis(rand_duration),
                        )
                    }
                    2 => {
                        self.inner.requests_complete(
                            "test",
                            "test",
                            &"503",
                            Duration::from_millis(rand_duration),
                        )
                    }
                    _ => {
                        self.inner.requests_complete(
                            "test",
                            "test",
                            &"500",
                            Duration::from_millis(rand_duration),
                        )
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[cfg(feature = "prometheusd")]
mod tests {
    use self::test_mocks::MockRpcMetrics;
    use crate::admin::metrics::{
        listen_for_metrics_requests,
        metrics_channel,
        metrics_monitor,
    };

    use super::*;
    use crate::config::{
        cache_setup::setup_data,
        cli_args::create_match,
        types::Settings,
    };
    use crate::log_info;
    use sled::Db;
    fn create_test_rpc_list() -> Arc<RwLock<Vec<Rpc>>> {
        Arc::new(RwLock::new(vec![Rpc::new(
            "http://example.com".to_string(),
            None,
            5,
            1000,
            0.5,
        )]))
    }

    // Helper function to create a test cache
    fn create_test_cache() -> Db {
        let db = sled::Config::new().temporary(true);
        let db = db.open().unwrap();

        (db)
    }

    async fn mock_setup() {
        let (metrics_tx, metrics_rx) = metrics_channel().await;
        let (metrics_update_tx, metrics_update_rx) = metrics_update_channel().await;
        let config = Arc::new(RwLock::new(Settings::new(create_match()).await));
        let config_metrics = Arc::clone(&config);
        let metrics_addr = config_metrics.read().unwrap().metrics.address.clone();
        let update_interval = config_metrics
            .read()
            .unwrap()
            .metrics
            .count_update_interval
            .clone();
        log_info!(
            "mock metrics settings: address: {}, update interval: {}",
            metrics_addr,
            update_interval
        );

        let storage_registry = prometheus_metric_storage::StorageRegistry::default();
        //TODO: why do i gotta clone this?
        let mock_metrics = MockRpcMetrics {
            inner: RpcMetrics::init(&storage_registry).unwrap().clone(),
        };
        let metrics_tx_rwlock = Arc::new(RwLock::new(metrics_tx));
        let registry_rwlock = Arc::new(RwLock::new(storage_registry));
        let registry_clone = Arc::clone(&registry_rwlock);
        let rpc_list = create_test_rpc_list();
        let dt = std::time::Instant::now();
        tokio::task::spawn(async move {
            log_info!("Prometheus enabled, accepting metrics at prometheus port");
            let _ = listen_for_metrics_requests(
                config_metrics,
                metrics_update_rx,
                registry_rwlock,
                rpc_list,
                0,
                dt.elapsed(),
            )
            .await;
        });
    }
    async fn assert_metrics() {}
    #[cfg(feature = "prometheusd")]
    #[tokio::test]
    //RUST_LOG=info cargo test --config example_config.toml -F prometheusd
    async fn test_prometheus_listener() {
        use crate::config::cli_args::create_match;
        let mut config = Settings::default();
        Arc::new(RwLock::new(config));

        let storage = StorageRegistry::default();
        let storage_arc = Arc::new(RwLock::new(storage));
        let config = Arc::new(RwLock::new(Settings::new(create_match()).await));
        let config_metrics = Arc::clone(&config);
        let rpc_list = create_test_rpc_list();
        let dt = std::time::Instant::now();
        log_info!(
            "Initial metrics state: {:?}",
            storage_arc.read().unwrap().gather()
        );
        let dt = std::time::Instant::now();
        let (metrics_tx, metrics_rx) = metrics_channel().await;
        let (metrics_update_tx, metrics_update_rx) = metrics_update_channel().await;
        let storage_clone = Arc::clone(&storage_arc);
        let storage_guard = storage_arc.read().unwrap();
        let mut rpc_metrics = RpcMetrics::init(&storage_guard).unwrap();
        rpc_metrics.requests_complete("test", "test", &"200", dt.elapsed());
        listen_for_metrics_requests(
            config_metrics,
            metrics_update_rx,
            storage_clone,
            rpc_list,
            0,
            dt.elapsed(),
        )
        .await;
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
            rpc_metrics.requests_complete("test", "test", &"200", dt.elapsed());
            // listen_for_metrics_requests(metrics_rx, storage_clone.clone());
            let test_report = metrics_encoder(storage_arc.clone()).await;
            log_info!("metrics state: {:?}", test_report);
        }
    }
}
