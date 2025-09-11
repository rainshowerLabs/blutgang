// System consts
pub const WS_HEALTH_CHECK_USER_ID: u32 = 1;
pub const WS_SUB_MANAGER_ID: u32 = 2;
pub const MAGIC: u32 = 0xb153;
/// DB fanout,
/// The default value of 1024 causes keys and values to be
/// more efficiently compressed when stored on disk,
/// but for larger-than-memory random workloads it may be advantageous
/// to lower LEAF_FANOUT to between 16 to 256, depending on your efficiency requirements.
/// A lower value will also cause contention to be reduced for frequently accessed data.
/// This value cannot be changed after creating the database.
pub const FANOUT: usize = 256;

// Version consts, dont impact functionality
pub const VERSION_STR: &str = "0.4.0 Arianrhod";
pub const TAGLINE: &str = "`I won't run`";
