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

    let blacklist = ["latest", "blockNumber", "earliest", "safe", "finalized", "pending"];
    // rx should look something like `{"id":1,"jsonrpc":"2.0","method":"eth_call","params":...`
    // This means that we should be able to read from the 26(id is optional). char to skip the parts of the
    // string we can never(in theory) encounter blacklist keywords.
    for item in blacklist.iter() {
        if memmem::find(&rx[26..].as_bytes(), item.as_bytes()).is_some() {
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

    let blacklist = ["Error", "error", "missing", "bad"];

    for item in blacklist.iter() {
        if memmem::find(&rx.as_bytes(), item.as_bytes()).is_some() {
            return false;
        }
    }

    true
}
