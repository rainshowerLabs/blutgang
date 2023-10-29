use sled::Db;
use std::sync::Arc;

pub fn setup_data(cache: Arc<Db>) {
    // Insert kv pair `blutgang_is_lb` `true` to know what we're interacting with
    // `blutgang_is_lb` is cached as a blake3 cache
    let _ = cache.insert(
        [
            176, 76, 1, 109, 13, 127, 134, 25, 55, 111, 28, 182, 82, 155, 135, 143, 204, 161, 53,
            4, 158, 140, 22, 219, 138, 5, 57, 150, 8, 154, 17, 252,
        ],
        "{\"jsonrpc\":\"2.0\",\"id\":null,\"result\":\"blutgang v0.2.0 nc \
        Dedicated to the spirit that lives inside of the computer\"}",
    );
}
