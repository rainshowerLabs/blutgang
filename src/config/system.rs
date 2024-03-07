// System consts
pub const WS_HEALTH_CHECK_USER_ID: u32 = 1;
pub const WS_SUB_MANAGER_ID: u32 = 2;
pub const MAGIC: u32 = 0xb153;

// Version consts, dont impact functionality
pub const VERSION_STR: &str = "Blutgang 0.3.2 Garreg Mach";
pub const TAGLINE: &str = "`Now there's a way forward.`";

#[cfg(feature = "journald")]
pub fn log_journald(level: u32, message: &str) {
    use systemd::journal;
    journal::print(level, message);
}

#[cfg(feature = "statsd")]
pub fn log_statsd(tags: &[str], message: &str) {
    unimplemented!()
}

// #[cfg(feature = "prometheusd")]
// pub fn log_prometheus(metric: &str) {
//     use metrics_prometheus::Recorder;
//     use prometheus::Gauge;
//     let _metric = prometheus::Gauge::new(format!("{metric}"), "help").unwrap();
//     let recorder = Recorder::builder()
//         .with_metric(_metric.clone())
//         .build_and_install();
// }

#[cfg(feature = "prometheusd")]
pub fn gather_metrics() -> Result<String, prometheus::Error> {
    use crate::log_info;
    use prometheus::gather;
    let report = prometheus::TextEncoder::new()
        .encode_to_string(&prometheus::default_registry().gather())?
        .trim()
        .to_string();
    log_info!("Prometheus metrics: {}", report);
    Ok(report)
    // gather::gather(&prometheus::gather(), &mut buffer).unwrap();
    // let output = String::from_utf8(buffer).unwrap();
    // println!("{}", output);
}

// #[cfg(feature = "prometheusd")]
// use crate::config::error::ConfigError;
// use prometheus::{proto::{Metric, MetricType, MetricFamily, LabelPair}, Encoder, Result};
// use std::{collections::HashMap, io::Write};
#[cfg(feature = "prometheusd")]
pub fn encode_metrics() {
    let report = gather_metrics();

    // let mut buffer = vec![];
    // encoder.encode(&metric_families, &mut buffer).unwrap();
    // let output = String::from_utf8(buffer).unwrap();
}

// #[cfg(feature = "prometheusd")]
// #[derive(Debug, Default)]
// pub struct JsonEncoder;
// #[cfg(feature = "prometheusd")]
// impl Encoder for JsonEncoder {
//     fn encode<W: Write>(&self, metric_families: &[MetricFamily], writer: &mut W) -> Result<()> {
//         let mut encoded : HashMap<String, f64> = HashMap::new();
//         for metric_family in metric_families {
//             let name = metric_family.get_name();
//             let metric_type = metric_family.get_field_type();
//             for metric in metric_family.get_metric() {
//                 match metric_type {
//                     MetricType::COUNTER => {
//                 encoded.entry(metric).and_modify;
//                 metric.get_counter().get_value();

//                     },
//                     MetricType::GAUGE => {
//                 encoded.entry(name.to_string(), metric);
//                 metric.get_gauge().get_value();

//                     },
//                     MetricType::HISTOGRAM => {
//                         let histogram = metric.get_histogram();
//                         encoded.insert(name.to_string(), histogram.get_sample_sum());
//                     },
//                     _ => {
//                         eprintln!("Unsupported metric type: {:?}", metric_type);
//                     },
//                 }
//             }
//         }
//         match serde_json::to_string(&encoded) {
//             Ok(_) => {
//                 writer.write_all(encoded.as_bytes())?;
//             },
//             Err(e) => eprintln!("Failed to encode metric as JSON! Error : {}", e.to_string())
//         }
//         Ok(())
//     }

//     fn format_type(&self) -> &str {
//         "json"
//     }
// }

#[cfg(feature = "dogstatd")]
pub fn log_dogstatsd(tags: &[str], message: &str) {
    use dogstatsd::{Client, Options, OptionsBuilder};
    let client = Client::new(Options::default()).unwrap();
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
macro_rules! dogstatd_latency {
    () => {};
}
#[macro_export]
macro_rules! dogstat_info {
    ($fmt:expr, $($arg:tt)*) => {
        let message = format!($fmt, $($arg)*);
        #[cfg(feature = "dogstatd")]
        {
            use $crate::config::system::log_dogstatsd;
            log_dogstatsd(6, &message);
        }
        println!("\x1b[35mInfo:\x1b[0m {}", message)
    };
    ($fmt:expr) => {
        #[cfg(feature = "dogstatd")]
        {
            use $crate::config::system::log_dogstatsd;
            log_dogstatsd(6, $fmt);

        }
        println!(concat!("\x1b[35mInfo:\x1b[0m ", $fmt))
    };
}

#[macro_export]
macro_rules! dogstatd_wrn {
    ($fmt:expr, $($arg:tt)*) => {
        let message = format!($fmt, $($arg)*);
        #[cfg(feature = "dogstatd")]
        {
            use $crate::config::system::log_dogstatsd;
            log_dogstatsd(4, &message);
        }
        println!("\x1b[93mWrn:\x1b[0m {}", message)
    };
    ($fmt:expr) => {
        #[cfg(feature = "dogstatd")]
        {
            use $crate::config::system::log_dogstatsd;
            log_dogstatsd(4, $fmt);
        }
        println!(concat!("\x1b[93mWrn:\x1b[0m ", $fmt))
    };
}

#[macro_export]
macro_rules! dogstatd_err {
    ($fmt:expr, $($arg:tt)*) => {
        let message = format!($fmt, $($arg)*);
        #[cfg(feature = "dogstatd")]
        {
            use $crate::config::system::log_dogstatsd;
            log_dogstatsd(3, &message);
        }
        println!("\x1b[31mErr:\x1b[0m {}", message)
    };
    ($fmt:expr) => {
        #[cfg(feature = "dogstatd")]
        {
            use $crate::config::system::log_dogstatsd;
            log_dogstatsd(3, $fmt);
        }
        println!(concat!("\x1b[31mErr:\x1b[0m ", $fmt))
    };
}
