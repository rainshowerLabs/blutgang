use memchr::memmem;

// Macro for building the blacklist
//
// For custom blacklist rules, please read the docs and code comments
macro_rules! blacklist {
    () => {
        // The blacklist contains keywords that are included in responses we dont want to cache.
        // These responses are either to get the current state of the chain at the head or errors.
        //
        // For max performance, blacklist keywords should be sorted in the order of how common they are.

        ["latest", "blockNumber", "error", "missing"]
    };
}

// The default rust string contains does not use SIMD extensions
// memchr::memmem is way faster because it uses them
pub fn cache_method(rx: &str) -> bool {
    // If no-cache feature is on, return false
    #[cfg(feature = "no-cache")]
    return false;

    let blacklist = blacklist!();

    for item in blacklist.iter() {
        if memmem::find(rx.as_bytes(), item.as_bytes()).is_some() {
            return false;
        }
    }

    true
}
