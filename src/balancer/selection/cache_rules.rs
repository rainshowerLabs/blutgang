use memchr::memmem;

// The default rust string contains does not use SIMD extensions
// memchr::memmem is way faster because it uses them
pub fn cache_method(rx: &str) -> bool {
    // If no-cache feature is on, return false
    #[cfg(feature = "no-cache")]
    return false;

    let blacklist = ["latest", "blockNumber", "error", "missing"];

    for item in blacklist.iter() {
        if memmem::find(rx.as_bytes(), item.as_bytes()).is_some() {
            return false;
        }
    }

    true
}
