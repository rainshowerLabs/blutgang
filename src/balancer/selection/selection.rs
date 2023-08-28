use crate::Rpc;
use memchr::memmem;

// TODO: make this generic
pub fn pick(list: &mut Vec<Rpc>) -> (Rpc, usize) {
    // If len is 1, return the only element
    if list.len() == 1 {
        return (list[0].clone(), 0);
    }

    // Sort by latency
    list.sort_by(|a, b| a.status.latency.partial_cmp(&b.status.latency).unwrap());

    if list[0].max_consecutive <= list[0].consecutive {
        list[1].consecutive = 1;
        list[0].consecutive = 0;
        return (list[1].clone(), 1);
    }

    list[0].consecutive += 1;
    (list[0].clone(), 0)
}

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
