use serde::{
    Deserialize,
    Serialize,
};
use std::path::PathBuf;

/// A list of options that can be applied to [`rocksdb::Options`].
#[non_exhaustive]
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct RocksDbOptionsRepr {
    pub enable: Option<bool>,
    // General options
    pub create_if_missing: Option<bool>,
    pub create_missing_column_families: Option<bool>,
    pub error_if_exists: Option<bool>,
    pub paranoid_checks: Option<bool>,
    // Paths
    pub db_paths: Option<Vec<DBPathRepr>>,
    pub db_log_dir: Option<PathBuf>,
    pub wal_dir: Option<PathBuf>,
    // Performance tuning
    pub increase_parallelism: Option<i32>,
    pub optimize_level_style_compaction: Option<usize>,
    pub optimize_universal_style_compaction: Option<usize>,
    pub optimize_for_point_lookup: Option<u64>,
    // File limits / concurrency
    pub max_open_files: Option<i32>,
    pub max_file_opening_threads: Option<i32>,
    pub max_background_jobs: Option<i32>,
    // Write buffers
    pub write_buffer_size: Option<usize>,
    pub max_write_buffer_number: Option<i32>,
    pub min_write_buffer_number: Option<i32>,
    pub target_file_size_base: Option<u64>,
    pub max_bytes_for_level_base: Option<u64>,
    pub max_bytes_for_level_multiplier: Option<f64>,
    // Level triggers
    pub level_zero_file_num_compaction_trigger: Option<i32>,
    pub level_zero_slowdown_writes_trigger: Option<i32>,
    pub level_zero_stop_writes_trigger: Option<i32>,
    pub disable_auto_compactions: Option<bool>,
    // Sync options
    pub use_fsync: Option<bool>,
    pub bytes_per_sync: Option<u64>,
    pub wal_bytes_per_sync: Option<u64>,
    // Compression
    pub compression_type: Option<String>,
    pub bottommost_compression_type: Option<String>,
}

impl From<RocksDbOptionsRepr> for rocksdb::Options {
    fn from(repr: RocksDbOptionsRepr) -> Self {
        let mut opts = Self::default();

        if let Some(create_if_missing) = repr.create_if_missing {
            opts.create_if_missing(create_if_missing);
        }
        if let Some(create_missing_cfs) = repr.create_missing_column_families {
            opts.create_missing_column_families(create_missing_cfs);
        }
        if let Some(enabled) = repr.error_if_exists {
            opts.set_error_if_exists(enabled);
        }
        if let Some(enabled) = repr.paranoid_checks {
            opts.set_paranoid_checks(enabled);
        }
        if let Some(ref paths) = repr.db_paths.map(|db_paths| {
            db_paths
                .into_iter()
                .map(|db_path| {
                    rocksdb::DBPath::new(db_path.path, db_path.target_size)
                        .expect("unable to form `rocksdb::DBPath` from config")
                })
                .collect::<Vec<rocksdb::DBPath>>()
        }) {
            opts.set_db_paths(paths);
        }
        if let Some(path) = repr.db_log_dir {
            opts.set_db_log_dir(path);
        }
        if let Some(path) = repr.wal_dir {
            opts.set_wal_dir(path);
        }
        if let Some(parallelism) = repr.increase_parallelism {
            opts.increase_parallelism(parallelism);
        }
        if let Some(memtable_memory_budget) = repr.optimize_level_style_compaction {
            opts.optimize_level_style_compaction(memtable_memory_budget);
        }
        if let Some(memtable_memory_budget) = repr.optimize_universal_style_compaction {
            opts.optimize_universal_style_compaction(memtable_memory_budget);
        }
        if let Some(block_cache_size_mb) = repr.optimize_for_point_lookup {
            opts.optimize_for_point_lookup(block_cache_size_mb);
        }
        if let Some(nfiles) = repr.max_open_files {
            opts.set_max_open_files(nfiles);
        }
        if let Some(nthreads) = repr.max_file_opening_threads {
            opts.set_max_file_opening_threads(nthreads);
        }
        if let Some(jobs) = repr.max_background_jobs {
            opts.set_max_background_jobs(jobs);
        }
        if let Some(size) = repr.write_buffer_size {
            opts.set_write_buffer_size(size);
        }
        if let Some(nbuf) = repr.max_write_buffer_number {
            opts.set_max_write_buffer_number(nbuf);
        }
        if let Some(nbuf) = repr.min_write_buffer_number {
            opts.set_min_write_buffer_number(nbuf);
        }
        if let Some(size) = repr.target_file_size_base {
            opts.set_target_file_size_base(size);
        }
        if let Some(size) = repr.max_bytes_for_level_base {
            opts.set_max_bytes_for_level_base(size);
        }
        if let Some(mul) = repr.max_bytes_for_level_multiplier {
            opts.set_max_bytes_for_level_multiplier(mul);
        }
        if let Some(n) = repr.level_zero_file_num_compaction_trigger {
            opts.set_level_zero_file_num_compaction_trigger(n);
        }
        if let Some(n) = repr.level_zero_slowdown_writes_trigger {
            opts.set_level_zero_slowdown_writes_trigger(n);
        }
        if let Some(n) = repr.level_zero_stop_writes_trigger {
            opts.set_level_zero_stop_writes_trigger(n);
        }
        if let Some(disable) = repr.disable_auto_compactions {
            opts.set_disable_auto_compactions(disable);
        }
        if let Some(useit) = repr.use_fsync {
            opts.set_use_fsync(useit);
        }
        if let Some(nbytes) = repr.bytes_per_sync {
            opts.set_bytes_per_sync(nbytes);
        }
        if let Some(nbytes) = repr.wal_bytes_per_sync {
            opts.set_wal_bytes_per_sync(nbytes);
        }
        if let Some(t) = repr.compression_type.map(|s| s.to_lowercase()) {
            opts.set_compression_type(into_compression_type(t));
        }
        if let Some(t) = repr.bottommost_compression_type.map(|s| s.to_lowercase()) {
            opts.set_bottommost_compression_type(into_compression_type(t));
        }

        opts
    }
}

/// Represents a (de)serializable [`rocksdb::DBPath`].
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DBPathRepr {
    pub path: PathBuf,
    pub target_size: u64,
}

fn into_compression_type(s: String) -> rocksdb::DBCompressionType {
    match s.as_ref() {
        "none" => rocksdb::DBCompressionType::None,
        "snappy" => rocksdb::DBCompressionType::Snappy,
        "zlib" => rocksdb::DBCompressionType::Zlib,
        "bz2" => rocksdb::DBCompressionType::Bz2,
        "lz4" => rocksdb::DBCompressionType::Lz4,
        "lz4hc" => rocksdb::DBCompressionType::Lz4hc,
        "zstd" => rocksdb::DBCompressionType::Zstd,
        _ => panic!("unknown compression type: {}", s),
    }
}
