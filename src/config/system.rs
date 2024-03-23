// System consts
pub const WS_HEALTH_CHECK_USER_ID: u32 = 1;
pub const WS_SUB_MANAGER_ID: u32 = 2;
pub const MAGIC: u32 = 0xb153;

// Version consts, dont impact functionality
pub const VERSION_STR: &str = "Blutgang 0.3.3 Garreg Mach";
pub const TAGLINE: &str = "`Now there's a way forward.`";
use crate::Rpc;

use once_cell::sync::Lazy;

use prometheus_metric_storage::{
    MetricStorage,
    StorageRegistry,
};
use thiserror::Error;

use std::{
    collections::hash_map::HashMap,
    sync::{
        Arc,
        RwLock,
    },
    time::Duration,
};
use tokio::sync::{
    mpsc::{
        unbounded_channel,
        UnboundedReceiver,
        UnboundedSender,
    },
    oneshot,
    Notify,
};

//Canidate for metrics storage
//#[cfg(feature = "prometheusd")]
pub type MetricSender = UnboundedSender<RpcMetrics>;
//#[cfg(feature = "prometheusd")]
pub type MetricReceiver = UnboundedReceiver<RpcMetrics>;

#[derive(Debug)]
pub struct RegistryChannel {
    registry: Lazy<StorageRegistry>,
    notify: Notify,
}

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
// #[cfg(feature = "prometheusd")]
#[derive(MetricStorage, Clone, Debug)]
#[metric(subsystem = "rpc")]
pub struct RpcMetrics {
    #[metric(labels("path", "method", "status"), help = "Total number of requests")]
    requests: prometheus::IntCounterVec,
    #[metric(labels("path", "method"), help = "latency of rpc calls")]
    duration: prometheus::HistogramVec,
}
//#[cfg(feature = "prometheusd")]
impl RpcMetrics {
    pub fn init(registry: &StorageRegistry) -> Result<&Self, prometheus::Error> {
        RpcMetrics::instance(registry)
    }
    pub fn requests_complete(&self, path: &str, method: &str, status: &u16, duration: Duration) {
        let dt = duration.as_millis() as f64;
        self.requests
            .with_label_values(&[path, method, &status.to_string()])
            .inc();
        self.duration.with_label_values(&[path, method]).observe(dt)
    }
    pub fn push_latency(&self, path: &str, method: &str, dt: f64) {
        self.duration.with_label_values(&[path, method]).observe(dt)
    }
}

#[cfg(feature = "journald")]
pub fn log_journald(level: u32, message: &str) {
    use systemd::journal;
    journal::print(level, message);
}

// #[cfg(feature = "prometheusd")]
pub struct RpcMetricsReciever {
    inner: MetricReceiver,
    metrics: Option<RpcMetrics>,
}
// #[cfg(feature = "prometheusd")]
impl std::fmt::Debug for RpcMetricsReciever {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RpcMetricsReciever").finish()
    }
}
// #[cfg(feature = "prometheusd")]
pub struct RpcMetricsSender {
    inner: MetricSender,
    metrics: Option<RpcMetrics>,
}
// #[cfg(feature = "prometheusd")]
impl std::fmt::Debug for RpcMetricsSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RpcMetricsSender").finish()
    }
}
// #[cfg(feature = "prometheusd")]
#[derive(Debug)]
pub enum MetricsCommand<'a> {
    Flush(),
    Channel(&'a RpcMetricsSender, &'a RpcMetricsReciever),
    PushLatency(&'a RpcMetrics, &'a RegistryChannel, &'a str, &'a str, f64),
    PushRequest(
        &'a RpcMetrics,
        &'a RegistryChannel,
        &'a str,
        &'a str,
        &'a u16,
        Duration,
    ),
    PushError(
        &'a RpcMetrics,
        &'a RegistryChannel,
        &'a str,
        &'a str,
        &'a u16,
        Duration,
    ),
    Push(&'a RpcMetrics, &'a RegistryChannel),
    Pull(&'a RpcMetrics, &'a RegistryChannel),
}

// #[cfg(feature = "prometheusd")]
#[derive(Debug)]
pub enum MetricChannelCommand {
    AdminMsg(oneshot::Sender<RpcMetrics>),
    StatsMsg(oneshot::Sender<RpcMetrics>),
}

//#[cfg(feature = "prometheusd")]
pub async fn metrics_update_sink(mut metrics_rx: RpcMetricsReciever) {
    loop {
        while metrics_rx.inner.recv().await.is_some() {
            continue;
        }
    }
}
//#[cfg(feature = "prometheusd")]
pub async fn listen_for_metrics(_metric: RpcMetrics, _metrics_tx: RpcMetricsSender) {
    unimplemented!()
    // let mut tx.inner.
}

pub async fn metrics_listener(
    mut metrics_rx: RpcMetricsReciever,
    metrics_status: Arc<RwLock<RpcMetrics>>,
) {
    while let Some(update) = metrics_rx.inner.recv().await {
        //TODO: match for metrics type, latency, errors, requests
        //match metric {
        //MetricsType::Latency => {
        //}
        let mut metrics = metrics_status.write().unwrap();
        *metrics = update;
    }
}

async fn metrics_processor(
    mut metrics_rx: RpcMetricsReciever,
    metrics_status: Arc<RwLock<RpcMetrics>>,
    path: &str,
    method: &str,
    status: &u16,
    duration: Duration,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(10));
    loop {
        interval.tick().await;
        while let Some(incoming) = metrics_rx.inner.recv().await {
            let _current_metrics = metrics_status.read().unwrap();
            incoming.requests_complete(path, method, status, duration);
        }
    }
}

// async fn metrics_encoder(mut metrics_rx: RpcMetricsReciever) -> String {
//     let encoder = prometheus::TextEncoder::new();
//     let mut buffer = vec![];
//     encoder.
// }

pub async fn metrics_monitor(metrics_rx: RpcMetricsReciever, storage_registry: StorageRegistry) {
    //TODO: figure ownership mess here
    let metrics_status = Arc::new(RwLock::new(
        RpcMetrics::instance(&storage_registry).unwrap().to_owned(),
    ));
    let metrics_stat_listener = metrics_status.clone();
    tokio::spawn(metrics_listener(metrics_rx, metrics_stat_listener));
    //metrics_processor(metrics_rx, metrics_status, "test", "test", &200, Duration::from_millis(100)).await;
}

pub async fn metrics_channel() -> (RpcMetricsSender, RpcMetricsReciever) {
    let (tx, rx) = unbounded_channel();
    let tx = RpcMetricsSender {
        inner: tx,
        metrics: None,
    };
    let rx = RpcMetricsReciever {
        inner: rx,
        metrics: None,
    };
    (tx, rx)
}

//#[cfg(feature = "prometheusd")]
// pub async fn spawn_metrics_channel(metrics_status: Arc<RwLock<Vec<Rpc>>>, mut rx: MetricReceiver) {
//     use crate::log_info;
//     let (tx, rx) = unbounded_channel();
//     let tx = RpcMetricsSender {
//         inner: tx,
//         metrics: None,
//     };
//     let rx = RpcMetricsReciever {
//         inner: rx,
//         metrics: None,
//     };
//     tokio::spawn();
//     }

// struct MetricsChannel {
//     metrics: RpcMetrics,
//     rx: RpcMetricsReciever,
// }

#[macro_export]
macro_rules! log_info {
    ($fmt:expr, $($arg:tt)*) => {
        let message = format!($fmt, $($arg)*);
        #[cfg(feature = "journald")]
        {
            use $crate::config::system::log_journald;
            log_journald(6, &message);
        }
        println!("\x1b[35mInfo:\x1b[0m {}", message)
    };
    ($fmt:expr) => {
        #[cfg(feature = "journald")]
        {
            use $crate::config::system::log_journald;
            log_journald(6, $fmt);
        }
        println!(concat!("\x1b[35mInfo:\x1b[0m ", $fmt))
    };
}

#[macro_export]
macro_rules! log_wrn {
    ($fmt:expr, $($arg:tt)*) => {
        let message = format!($fmt, $($arg)*);
        #[cfg(feature = "journald")]
        {
            use $crate::config::system::log_journald;
            log_journald(4, &message);
        }
        println!("\x1b[93mWrn:\x1b[0m {}", message)
    };
    ($fmt:expr) => {
        #[cfg(feature = "journald")]
        {
            use $crate::config::system::log_journald;
            log_journald(4, $fmt);
        }
        println!(concat!("\x1b[93mWrn:\x1b[0m ", $fmt))
    };
}

#[macro_export]
macro_rules! log_err {
    ($fmt:expr, $($arg:tt)*) => {
        let message = format!($fmt, $($arg)*);
        #[cfg(feature = "journald")]
        {
            use $crate::config::system::log_journald;
            log_journald(3, &message);
        }
        println!("\x1b[31mErr:\x1b[0m {}", message)
    };
    ($fmt:expr) => {
        #[cfg(feature = "journald")]
        {
            use $crate::config::system::log_journald;
            log_journald(3, $fmt);
        }
        println!(concat!("\x1b[31mErr:\x1b[0m ", $fmt))
    };
}

//WIP: macros
// #[macro_export]
// macro_rules! prometheusd_latency {
//         ($fmt:expr, $($arg:tt)*) => {
//         let rpc_path = format!($fmt, $($arg)*);
//         // #[cfg(feature = "prometheusd")]
//         {
//             use $crate::config::system::RpcMetrics;
//             use $crate::config::system::get_storage_registry;
//             let registry = get_storage_registry();
//             let metric = RpcMetrics::inst(registry).unwrap();
//             let start = std::time::Instant::now();
//             move |status: u16| {
//                 let duration = start.elapsed();
//                 metric.requests_complete(duration);
//             }
//         }
//         };
// }
//
pub mod test_mocks {
    use super::*;
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
        unimplemented!();
    }
}
