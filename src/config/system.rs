// System consts
pub const WS_HEALTH_CHECK_USER_ID: u32 = 1;
pub const WS_SUB_MANAGER_ID: u32 = 2;
pub const MAGIC: u32 = 0xb153;

// Version consts, dont impact functionality
pub const VERSION_STR: &str = "Blutgang 0.3.2 Garreg Mach";
pub const TAGLINE: &str = "`Now there's a way forward.`";
use atomic_refcell::AtomicRefCell;
use once_cell::sync::Lazy;
use prometheus::Registry;
use prometheus_metric_storage::{
    MetricStorage,
    StorageRegistry,
};

use std::time::Duration;
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
pub type MetricSender = UnboundedSender<RpcMetrics>;
pub type MetricReceiver = UnboundedReceiver<RpcMetrics>;
struct RegistryChannel {
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
#[derive(MetricStorage, Clone, Debug)]
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

#[cfg(feature = "journald")]
pub fn log_journald(level: u32, message: &str) {
    use systemd::journal;
    journal::print(level, message);
}

// #[cfg(feature = "prometheusd")]
// #[cfg(feature = "prometheusd")]
pub fn get_storage_registry() -> &'static StorageRegistry {
    &METRICS_REGISTRY
}
// #[cfg(feature = "prometheusd")]
// #[cfg(feature = "prometheusd")]
pub fn get_registry() -> &'static Registry {
    get_storage_registry().registry()
}

//TODO: should (Sender, Reciver) be wrapped in Result, Error?
pub fn registry_channel() -> (MetricSender, MetricReceiver) {
    let _ch: Lazy<RegistryChannel> = Lazy::new(|| {
        RegistryChannel {
            registry: Lazy::new(|| {
                let registry =
                    Registry::new_custom(Some("blutgang_metrics_channel".to_string()), None)
                        .unwrap();
                StorageRegistry::new(registry)
            }),
            notify: Notify::new(),
        }
    });
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    (tx, rx)
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

#[macro_export]
macro_rules! prometheusd_latency {
        ($fmt:expr, $($arg:tt)*) => {
        let rpc_path = format!($fmt, $($arg)*);
        // #[cfg(feature = "prometheusd")]
        {
            use $crate::config::system::RpcMetrics;
            use $crate::config::system::get_storage_registry;
            let registry = get_storage_registry();
            let metric = RpcMetrics::inst(registry).unwrap();
            let start = std::time::Instant::now();
            move |status: u16| {
                let duration = start.elapsed();
                metric.requests_complete(duration);
            }
        }
        };
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
