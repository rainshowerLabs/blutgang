// System consts
pub const WS_HEALTH_CHECK_USER_ID: u32 = 1;
pub const WS_SUB_MANAGER_ID: u32 = 2;
pub const MAGIC: u32 = 0xb153;

// Version consts, dont impact functionality
pub const VERSION_STR: &str = "Blutgang 0.3.2 Garreg Mach";
pub const TAGLINE: &str = "`Now there's a way forward.`";
use atomic_refcell::AtomicRefCell;
use crossbeam_channel::{unbounded, Receiver, Sender};
use once_cell::sync::Lazy;
use prometheus::Registry;
use prometheus_metric_storage::{
    MetricStorage,
    StorageRegistry,
};
use simd_json::Object;

use std::time::Duration;
use std::collections::hash_map::HashMap;
use tokio::sync::{
    mpsc::{
        UnboundedReceiver,
        UnboundedSender,
    },
    Notify,
};
//Some goofy Rust stuff
// #[cfg(feature = "prometheusd")]
// #[cfg(feature = "prometheusd")]
static METRICS_REGISTRY: Lazy<StorageRegistry> = Lazy::new(|| {
    let registry = Registry::new_custom(Some("blutgang".to_string()), None).unwrap();
    StorageRegistry::new(registry)
});

// Canidate for metrics storage
// #[cfg(feature = "prometheusd")]
type MetricsRegistry = AtomicRefCell<Lazy<StorageRegistry>>;

//Canidate for metrics storage
// #[cfg(feature = "prometheusd")]
type RegistryServer = Lazy<StorageRegistry>;
type RegistryClient = Lazy<StorageRegistry>;
pub type MetricSender = Sender<RpcMetrics>;
pub type MetricReceiver = Receiver<RpcMetrics>;

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

// #[cfg(feature = "prometheusd")]
#[derive(MetricStorage, Clone, )]
#[metric(subsystem = "rpc")]
pub struct RpcMetrics {
    #[metric(labels("path", "method", "status"), help = "Total number of requests")]
    requests: prometheus::IntCounterVec,
    #[metric(labels("path", "method"), help = "latency of rpc calls")]
    duration: prometheus::HistogramVec,
}
// #[cfg(feature = "prometheusd")]
// #[cfg(feature = "prometheusd")]
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

//  #[cfg(feature = "prometheusd")]
pub fn encode(registry: &prometheus::Registry) -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let mut buffer = vec![];
    encoder.encode(&registry.gather(), &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

// pub fn parse_metrics(report: String) -> String {
//         let mut latency_buffer : HashMap<String, u32> = HashMap::new();
//         let mut method = String::new();
//         let mut path = String::new();
//         let mut sum = 0.0;
//         let mut count = 0;
//         for line in report.lines() {
//             let parts: Vec<_> = line.split_whitespace().collect();
//             if parts.len() == 4 {
//                 let latency_edge = parts[2].split(',').find(|&x| x.starts_with("le=")).unwrap_or("");
//                 let le_value = latency_edge.trim_start_matches("le=\"").trim_end_matches('"');
//                 let count_value: u32 = parts[3].parse().unwrap_or(0);
//                 if method.is_empty() && path.is_empty() {
//                     let method_part = parts[2].split(',').find(|&x| x.starts_with("method=")).unwrap_or("");
//                     method = method_part.trim_start_matches("method=\"").trim_end_matches('"').to_string();
//                     let path_part = parts[2].split(',').find(|&x| x.starts_with("path=")).unwrap_or("");
//                     path = path_part.trim_start_matches("path=\"").trim_end_matches('"').to_string();
//                 }
//                 latency_buffer.insert(le_value.to_string(), count_value);
//             } else if line.starts_with("blutgang_rpc_duration_sum") {
//             sum = line.split_whitespace().last().unwrap().parse().unwrap_or(0.0);
//             } else if line.starts_with("blutgang_rpc_duration_count") {
//                 count = line.split_whitespace().last().unwrap().parse().unwrap_or(0);
//                             }
//         }
//         let parsed_metrics = String::from("path: ") + &path + ", method: " + &method + ", latency sum: " + &sum.to_string() + ", count: " + &count.to_string() + ", latency edge: " + latency_buffer.)
// }


#[cfg(feature = "journald")]
pub fn log_journald(level: u32, message: &str) {
    use systemd::journal;
    journal::print(level, message);
}

// #[cfg(feature = "prometheusd")]
pub fn get_storage_registry() -> &'static StorageRegistry {
    &METRICS_REGISTRY
}
// #[cfg(feature = "prometheusd")]
pub fn get_registry() -> &'static Registry {
    get_storage_registry().registry()
}

// #[cfg(feature = "prometheusd")]
pub struct RpcMetricsReciever {
    inner: Receiver<RpcMetrics>,
    name: &'static str,
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
    inner: Sender<RpcMetrics>,
    name: &'static str,
    metrics: Option<RpcMetrics>,
}
// #[cfg(feature = "prometheusd")]
impl std::fmt::Debug for RpcMetricsSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RpcMetricsSender").finish()
    }
}

// #[cfg(feature = "prometheusd")]
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
        let (tx, rx) = unbounded();
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

    pub fn encode_channel(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let mut buffer = vec![];
        encoder
            .encode(&self.registry.gather(), &mut buffer)
            .unwrap();
        String::from_utf8(buffer).unwrap()
    }
        pub fn push_metrics(
        metric: RpcMetrics,
        path: &str,
        method: &str,
        dt: f64,
        mut rx: RpcMetricsReciever,
        tx: RpcMetricsSender,
    ) {
            let send = tx.inner.send(metric);
            let recv = rx.inner.recv();
            if let Ok(_) = send {
                rx.metrics = Some(recv.unwrap());
                rx.metrics.unwrap().push_latency(path, method, dt);
            }
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
