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

#[cfg(feature = "prometheusd")]
pub fn log_prometheus(metric: &str) {
    use metrics_prometheus::Recorder;
    use prometheus::Gauge;
    let _metric = prometheus::Gauge::new(format!("{metric}"), "help").unwrap();
    let recorder = Recorder::builder()
        .with_metric(_metric.clone())
        .build_and_install();
}

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
