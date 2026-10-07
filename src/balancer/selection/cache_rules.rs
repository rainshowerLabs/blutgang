use memchr::memmem;

use crate::{
    balancer::format::NamedNumber,
    rpc::method::EthRpcMethod,
};

// Return true if we are supposed to be caching the input.
//
// This loop cannot be unrolled because it wouldn't work against mangled queries that would
// overall be valid. Too bad!
pub fn cache_method<M: AsRef<str>>(rx: M) -> bool {
    // If no-cache feature is on, return false
    #[cfg(feature = "no-cache")]
    return false;

    if has_block_tag(&rx) {
        return false;
    }

    // all of the below cannot be cached properly
    let blacklist = [
        EthRpcMethod::BlockNumber.as_ref(),
        EthRpcMethod::GetTransactionCount.as_ref(),
        EthRpcMethod::Subscribe.as_ref(),
        EthRpcMethod::Unsubscribe.as_ref(),
    ];
    // rx should look something like `{"id":1,"jsonrpc":"2.0","method":"eth_call","params":...`
    // Even tho rx should look like the example above, its still a valid request if the method
    // is first, so it will be skipped if we try to be smart and skip the first n charachters.
    //
    // We could potentially try to find `params` and then move from there but it would end up
    // being slower in most cases.
    for item in blacklist.iter() {
        if memmem::find(rx.as_ref().as_bytes(), item.as_bytes()).is_some() {
            return false;
        }
    }

    true
}

/// Returns true if the request still names a block by tag (`latest`, `safe`, ...).
///
/// A tag points at a different block as the chain moves, so a response to such a
/// request must never be written to or served from the cache.
pub fn has_block_tag<M: AsRef<str>>(rx: M) -> bool {
    let tags = [
        NamedNumber::Latest,
        NamedNumber::Earliest,
        NamedNumber::Safe,
        NamedNumber::Finalized,
        NamedNumber::Pending,
    ];

    tags.iter()
        .any(|tag| memmem::find(rx.as_ref().as_bytes(), tag.as_ref().as_bytes()).is_some())
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
