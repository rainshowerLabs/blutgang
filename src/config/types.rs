use crate::Rpc;
use sled::Config;
use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct Settings {
    pub rpc_list: Vec<Rpc>,
    pub do_clear: bool,
    pub address: SocketAddr,
    pub sled_config: Config,
}
