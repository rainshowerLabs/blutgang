use crate::{
    config::system::{
        TAGLINE,
        VERSION_STR,
    },
    database::types::GenericDatabase,
};

/// Sets up the cache with various basic data about our current blutgang instance.
pub fn setup_data<DB: GenericDatabase>(cache: &DB, do_clear: bool) {
    // Clear database if specified
    if do_clear {
        cache.clear().unwrap();
        tracing::warn!("All data cleared from the database.");
    }

    let version_json = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"Blutgang {}; {}\"}}",
        VERSION_STR, TAGLINE
    );

    tracing::info!("Starting Blutgang {}", VERSION_STR);

    // Insert kv pair `blutgang_is_lb` `true` to know what we're interacting with
    // `blutgang_is_lb` is cached as a blake3 cache
    let _ = cache.write(
        [
            176, 76, 1, 109, 13, 127, 134, 25, 55, 111, 28, 182, 82, 155, 135, 143, 204, 161, 53,
            4, 158, 140, 22, 219, 138, 5, 57, 150, 8, 154, 17, 252,
        ],
        version_json.as_bytes(),
    );
    // Insert kv pair `web3_clientVersion` `true` to know what we're interacting with
    // `web3_clientVersion` is cached as a blake3 cache
    let _ = cache.write(
        [
            36, 20, 170, 125, 105, 107, 149, 148, 52, 126, 215, 218, 112, 55, 222, 60, 186, 44, 67,
            121, 225, 160, 31, 209, 9, 99, 81, 233, 137, 37, 62, 79,
        ],
        version_json.as_bytes(),
    );

    // Cache keys are blake3 hashes. Older builds could be compiled with an `xxhash`
    // feature that keyed the cache differently, so warn if we find one of their DBs.
    let _ = cache.write(b"blake3", b"true");
    if cache.read(b"xxhash").unwrap().is_some() {
        tracing::error!("Blutgang has detected that your DB is using xxhash while we're currently using blake3! \
            Please remove all cache entries and try again.");
    }
}
