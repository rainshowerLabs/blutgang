// System consts
pub const WS_HEALTH_CHECK_USER_ID: u32 = 1;
pub const WS_SUB_MANAGER_ID: u32 = 2;
pub const MAGIC: u32 = 0xb153;
/// DB fanout
/// The default value of 1024 causes keys and values to be
/// more efficiently compressed when stored on disk,
/// but for larger-than-memory random workloads it may be advantageous
/// to lower LEAF_FANOUT to between 16 to 256, depending on your efficiency requirements.
/// A lower value will also cause contention to be reduced for frequently accessed data.
/// This value cannot be changed after creating the database.
pub const FANOUT: usize = 512;

// Version consts, dont impact functionality
pub const VERSION_STR: &str = "Blutgang 0.4.0 Arianrhod";
pub const TAGLINE: &str = "`I won't run`";

#[cfg(feature = "journald")]
pub fn log_journald(level: u32, message: &str) {
    use systemd::journal;
    journal::print(level, message);
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
