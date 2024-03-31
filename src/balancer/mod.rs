//! # `balancer` module
//! 
//! The `balancer` module is the core and center of Blutgang.
//! It includes logic for handling and processing incoming JSON-RPC requests,
//! caching, and returning answers.
//!
//! In addition to this, it includes various helper fn's for formatting
//! and processing incoming data.

mod pipeline;
pub mod accept_http;
pub mod format;
pub mod processing;
mod response_errors;
pub mod selection;
