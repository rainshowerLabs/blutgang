// System consts
pub const WS_HEALTH_CHECK_USER_ID: u32 = 1;
pub const WS_SUB_MANAGER_ID: u32 = 2;
pub const MAGIC: u32 = 0xb153;

// Version consts, dont impact functionality
pub const VERSION_STR: &str = "Blutgang 0.3.1 Garreg Mach";
pub const TAGLINE: &str = "`Now there's a way forward.`";

#[macro_export]
macro_rules! log_info {
    ($fmt:expr, $($arg:tt)*) => {
        println!(concat!("\x1b[35mInfo:\x1b[0m ", $fmt), $($arg)*)
    };
    ($fmt:expr) => {
        println!(concat!("\x1b[35mInfo:\x1b[0m ", $fmt))
    };
}

#[macro_export]
macro_rules! log_wrn {
    ($fmt:expr, $($arg:tt)*) => {
        println!(concat!("\x1b[93mWrn:\x1b[0m ", $fmt), $($arg)*)
    };
    ($fmt:expr) => {
        println!(concat!("\x1b[93mWrn:\x1b[0m ", $fmt))
    };
}

#[macro_export]
macro_rules! log_err {
    ($fmt:expr, $($arg:tt)*) => {
        println!(concat!("\x1b[31mErr:\x1b[0m ", $fmt), $($arg)*)
    };
    ($fmt:expr) => {
        println!(concat!("\x1b[31mErr:\x1b[0m ", $fmt))
    };
}
