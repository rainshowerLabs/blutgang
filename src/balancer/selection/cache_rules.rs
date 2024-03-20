use memchr::memmem;

// Return true if we are supposed to be caching the input.
//
// This loop cannot be unrolled because it wouldn't work against mangled queries that would
// overall be valid. Too bad!
pub fn cache_method(rx: &str) -> bool {
    // If no-cache feature is on, return false
    #[cfg(feature = "no-cache")]
    return false;

    // all of the below cannot be cached properly
    let blacklist = [
        "latest",
        "eth_blockNumber",
        "earliest",
        "safe",
        "finalized",
        "pending",
        "eth_subscribe",
        "eth_unsubscribe",
    ];
    // rx should look something like `{"id":1,"jsonrpc":"2.0","method":"eth_call","params":...`
    // Even tho rx should look like the example above, its still a valid request if the method
    // is first, so it will be skipped if we try to be smart and skip the first n charachters.
    //
    // We could potentially try to find `params` and then move from there but it would end up
    // being slower in most cases.
    for item in blacklist.iter() {
        if memmem::find(rx.as_bytes(), item.as_bytes()).is_some() {
            return false;
        }
    }

    true
}

// Same as cache_method but for results
pub fn cache_result(rx: &str) -> bool {
    // If no-cache feature is on, return false
    #[cfg(feature = "no-cache")]
    return false;

    // just checking if `error` is present should be enough, but include the beggining error
    // codes juuuust to be extra safe
    //
    // `null` can appear in results if the node is malfunctioning and we shouldnt try and cache it as a result
    let blacklist = ["error", "-32", "null"];

    for item in blacklist.iter() {
        if memmem::find(rx.as_bytes(), item.as_bytes()).is_some() {
            return false;
        }
    }

    true
}
