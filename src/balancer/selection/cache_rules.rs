use memchr::memmem;

// Macro for building the blacklist
//
// For custom blacklist rules, please read the docs and code comments
#[allow(unused_macros)]
macro_rules! blacklist {
    () => {
        // The blacklist contains keywords that are included in responses we dont want to cache.
        // These responses are either to get the current state of the chain at the head or errors.
        //
        // For max performance, blacklist keywords should be sorted in the order of how common they are.

        ["latest", "blockNumber", "error", "missing"]
    };
}

// Return true if we are supposed to be caching the input.
//
// The default rust string contains does not use SIMD extensions
// memchr::memmem is way faster because it uses them
pub fn cache_method(rx: &str) -> bool {
    // If no-cache feature is on, return false
    #[cfg(feature = "no-cache")]
    return false;

    let blacklist = [
        "latest",
        "blockNumber",
        "earliest",
        "safe",
        "finalized",
        "pending",
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
    let blacklist = ["error", "-320", "-326", "-327"];

    for item in blacklist.iter() {
        if memmem::find(&rx.as_bytes(), item.as_bytes()).is_some() {
            return false;
        }
    }

    true
}
