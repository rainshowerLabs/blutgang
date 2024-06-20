use crate::{
    config::system::{
        TAGLINE,
        VERSION_STR,
    },
    log_err,
    log_info,
};
use sled::Db;

/// Sets up the cache with various basic data about our current blutgang instance.
pub fn setup_data(cache: &Db) {
    let version_json = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"{}; {}\"}}",
        VERSION_STR, TAGLINE
    );

    log_info!("Starting {}", VERSION_STR);

    // Insert kv pair `blutgang_is_lb` `true` to know what we're interacting with
    // `blutgang_is_lb` is cached as a blake3 cache
    let _ = cache.insert(
        [
            176, 76, 1, 109, 13, 127, 134, 25, 55, 111, 28, 182, 82, 155, 135, 143, 204, 161, 53,
            4, 158, 140, 22, 219, 138, 5, 57, 150, 8, 154, 17, 252,
        ],
        version_json.as_bytes(),
    );
    // Insert kv pair `web3_clientVersion` `true` to know what we're interacting with
    // `web3_clientVersion` is cached as a blake3 cache
    let _ = cache.insert(
        [
            36, 20, 170, 125, 105, 107, 149, 148, 52, 126, 215, 218, 112, 55, 222, 60, 186, 44, 67,
            121, 225, 160, 31, 209, 9, 99, 81, 233, 137, 37, 62, 79,
        ],
        version_json.as_bytes(),
    );

    // Insert which hashing algo we're using based on the selected features.
    // If `xxhash` is enabled we're using xxhash3, otherwise blake3.
    //
    // Print a warning if we see an keys are in an unexpectd hash format.
    if cfg!(feature = "xxhash") {
        let _ = cache.insert(b"xxhash", b"true");
        if cache.get(b"blake3").unwrap().is_some() {
            log_err!("Blutgang has detected that your DB is using blake3 while we're currently using xxhash! \
                Please remove all cache entries and try again.");
            println!("If you believe this is an error, please open a pull request!");
        }
    } else {
        let _ = cache.insert(b"blake3", b"true");
        if cache.get(b"xxhash").unwrap().is_some() {
            log_err!("Blutgang has detected that your DB is using xxhash while we're currently using blake3! \
                Please remove all cache entries and try again.");
            println!("If you believe this is an error, please open a pull request!");
        }
    }
}
