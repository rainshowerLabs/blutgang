// System consts
pub const WS_HEALTH_CHECK_USER_ID: u32 = 1;
pub const WS_SUB_MANAGER_ID: u32 = 2;
pub const MAGIC: u32 = 0xb153;

// Version consts, dont impact functionality
pub const VERSION_STR: &str = "Blutgang 0.3.3 Garreg Mach";
pub const TAGLINE: &str = "`Now there's a way forward.`";
use atomic_refcell::AtomicRefCell;
use url::ParseError;
use crate::Rpc;
use futures::FutureExt;
use once_cell::sync::Lazy;
use prometheus::Registry;
use prometheus_metric_storage::{
    MetricStorage,
    StorageRegistry,
};
use thiserror::Error;

use std::{
    collections::hash_map::HashMap,
    future::IntoFuture,
    sync::{Arc, RwLock},
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

// Canidate for metrics storage
// // #[cfg(feature = "prometheusd")]
// type MetricsRegistry = AtomicRefCell<Lazy<StorageRegistry>>;

//Canidate for metrics storage
#[cfg(feature = "prometheusd")]
pub type MetricSender = UnboundedSender<RpcMetrics>;
#[cfg(feature = "prometheusd")]
pub type MetricReceiver = UnboundedReceiver<RpcMetrics>;

#[derive(Debug)]
pub struct RegistryChannel {
    registry: Lazy<StorageRegistry>,
    notify: Notify,
}

//WIP for a session typed pattern
// See: https://stanford-cs242.github.io/f19/lectures/09-1-session-types.html
// pub trait HasDualMetrics {
//     type DualMetrics;
// }
// impl HasDualMetrics for MetricSender {
//     type DualMetrics = MetricReceiver;
// }

// impl HasDualMetrics for MetricReceiver {
//     type DualMetrics = MetricSender;
// }
// Pub struct DualMetricsChannel <T: HasDualMetrics> {}
// impl DualMetricsChannel<MetricSender> {}
// impl DualMetricsChannel<MetricReceiver> {}
#[cfg(feature = "prometheusd")]
#[derive(Debug, Error)]
pub enum MetricsError {
    #[error(transparent)]
    WSError(#[from] crate::websocket::error::WsError),
    #[error(transparent)]
    RPCError(#[from] crate::rpc::error::RpcError),
}
#[cfg(feature = "prometheusd")]
impl From<MetricsError> for String {
    fn from(error: MetricsError) -> Self {
    error.to_string()
    }
}
#[cfg(feature = "prometheusd")]
#[derive(MetricStorage, Clone, Debug)]
#[metric(subsystem = "rpc")]
pub struct RpcMetrics {
    #[metric(labels("path", "method", "status"), help = "Total number of requests")]
    requests: prometheus::IntCounterVec,
    #[metric(labels("path", "method"), help = "latency of rpc calls")]
    duration: prometheus::HistogramVec,
}
#[cfg(feature = "prometheusd")]
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

#[cfg(feature = "prometheusd")]
pub struct RpcMetricsReciever {
    inner: MetricReceiver,
    pub name: &'static str,
    metrics: Option<RpcMetrics>,
}
#[cfg(feature = "prometheusd")]
impl std::fmt::Debug for RpcMetricsReciever {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RpcMetricsReciever").finish()
    }
}
#[cfg(feature = "prometheusd")]
pub struct RpcMetricsSender {
    inner: MetricSender,
    pub name: &'static str,
    metrics: Option<RpcMetrics>,
}
#[cfg(feature = "prometheusd")]
impl std::fmt::Debug for RpcMetricsSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RpcMetricsSender").finish()
    }
}
#[cfg(feature = "prometheusd")]
#[derive(Debug)]
pub enum MetricsCommand<'a> { 
    Flush(&'a RegistryChannel),
    Channel(&'a RpcMetricsSender, &'a RpcMetricsReciever),
    Push(&'a RpcMetrics, &'a RegistryChannel),
    Pull(&'a RpcMetrics, &'a RegistryChannel),
}

#[cfg(feature = "prometheusd")]
#[derive(Debug)]
pub enum MetricChannelCommand {
    AdminMsg(oneshot::Sender<RpcMetrics>),
    StatsMsg(oneshot::Sender<RpcMetrics>),
}

#[cfg(feature = "prometheusd")]
impl RegistryChannel {
    pub fn new() -> Self {
        RegistryChannel {
            registry: Lazy::new(|| {
                let registry =
                    Registry::new_custom(Some("blutgang_metrics_channel".to_string()), None)
                        .unwrap();
                StorageRegistry::new(registry)
            }),
            notify: Notify::new(),
        }
    }

    pub fn get_storage_registry(&self) -> &Lazy<StorageRegistry> {
        &self.registry
    }

    pub fn get_registry(&self) -> &Registry {
        &self.get_storage_registry().registry()
    }
    pub fn notify(&self) -> &Notify {
        &self.notify
    }
    //TODO: should (Sender, Reciver) be wrapped in Result, Error?
    //TODO: compare crossbeam_channel to mpsc
    pub fn channel(name: &'static str) -> (RpcMetricsSender, RpcMetricsReciever) {
        let (tx, rx) = unbounded_channel();
        let tx = RpcMetricsSender {
            inner: tx,
            name,
            metrics: None,
        };
        let rx = RpcMetricsReciever {
            inner: rx,
            name,
            metrics: None,
        };
        (tx, rx)
    }

    pub fn on_flush(&self, tx: RpcMetricsSender) {
        use crate::log_info;
        let command = MetricsCommand::Flush(self);
        log_info!("Flushing metrics");
        let command = MetricsCommand::Flush(self.clone());
        // tx.inner.send().unwrap();
        self.notify.notify_one();
    }

    pub fn encode_channel(&self) -> Result<String, ParseError> {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let mut buffer = vec![];
        encoder
            .encode(&self.registry.gather(), &mut buffer)
            .unwrap();
        let report = String::from_utf8(buffer.clone()).expect("Failed to parse prometheus report");
        buffer.clear();
        Ok(report)          
            
    }

    pub fn on_push(&self, path: &str, method: &str, status: u16, dt: f64, metrics: RpcMetrics, mut rx: RpcMetricsReciever, tx: RpcMetricsSender) {
        let command = MetricsCommand::Push(&metrics, self);
        self.notify.notify_one();
        metrics.requests.with_label_values(&[path, method, &status.to_string(), ]).inc();
        metrics.duration.with_label_values(&[path, method]).observe(dt);
        let send = tx.inner.send(metrics);
        let recv = rx.inner.recv();
    }

    pub fn on_push_latency(&self, path: &str, method: &str, dt: f64, metrics: RpcMetrics, mut rx: RpcMetricsReciever, tx: RpcMetricsSender) {
        let command = MetricsCommand::Push(&metrics, self);
        self.notify.notify_one();
        metrics.duration.with_label_values(&[path, method]).observe(dt);
        let send = tx.inner.send(metrics);
        let recv = rx.inner.recv();
    }

}

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
        pub rpc_list : Arc<RwLock<Vec<Rpc>>>,
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
