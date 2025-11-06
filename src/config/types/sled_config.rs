use serde::{
    Deserialize,
    Serialize,
};
use std::path::PathBuf;

/// A list of options that can be applied to [`sled::Config`].
#[non_exhaustive]
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SledConfigRepr {
    pub enable: Option<bool>,
    /// The base directory for storing the database.
    pub path: Option<PathBuf>,
    /// Cache size in **bytes**. Default is 512mb.
    pub cache_capacity_bytes: Option<usize>,
    /// The percentage of the cache that is dedicated to the
    /// scan-resistant entry cache.
    pub entry_cache_percent: Option<u8>,
    /// Start a background thread that flushes data to disk
    /// every few milliseconds. Defaults to every 200ms.
    pub flush_every_ms: Option<usize>,
    /// The zstd compression level to use when writing data to disk. Defaults to 3.
    pub zstd_compression_level: Option<i32>,
}

impl From<SledConfigRepr> for sled::Config {
    fn from(repr: SledConfigRepr) -> Self {
        let mut opts = Self::default();

        if let Some(path) = repr.path {
            opts = opts.path(path);
        }
        if let Some(to) = repr.cache_capacity_bytes {
            opts = opts.cache_capacity_bytes(to);
        }
        if let Some(to) = repr.entry_cache_percent {
            opts = opts.entry_cache_percent(to);
        }
        if let Some(to) = repr.flush_every_ms {
            opts = opts.flush_every_ms(Some(to));
        }
        if let Some(to) = repr.zstd_compression_level {
            opts = opts.zstd_compression_level(to);
        }

        opts
    }
}
