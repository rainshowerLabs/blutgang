//! # `config` module
//!
//! The config module is used on initial startup to configure Blutgang for use.
//! Includes parsing of the TOML config, CLI args, and various system parameters.

pub mod cache_setup;
pub mod cli_args;
pub mod error;
pub mod setup;
pub mod system;
pub mod types;
