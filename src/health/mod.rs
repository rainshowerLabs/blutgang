//! # `health` module
//!
//! Perhaps the most important piece of Blutgang,
//! the health module makes sure that every node is ready to accept requests,
//! healthy, not syncing, and not falling behind the head of the chain.
//!
//! In addition the health module also deals with reorgs, by removing reorged data
//! from the cache. When WebSockets are enabled, the health module will also
//! start *rewriting* requests and keeping active track of the head via a 
//! `newHeads` subscription. Requests that use named parameters like `latest` will
//! be rewritten to the block number `latest` represents, caching them or querying
//! them from the cache.

pub mod check;
pub mod error;
pub mod head_cache;
pub mod safe_block;
