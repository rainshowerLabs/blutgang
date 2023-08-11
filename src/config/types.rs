use crate::Rpc;

#[derive(Debug, Default, Clone)]
pub struct Settings {
    pub rpc_list: Vec<Rpc>,
    pub port: u16,
    pub db_path: String,
    pub cache_capacity: u64,
    pub print_profile: bool,
    pub flush_time: Option<u64>,
    pub do_clear: bool,
}
