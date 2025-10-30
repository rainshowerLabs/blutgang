use rust_tracing::deps::metrics;
use tokio::sync::{
    mpsc,
    oneshot,
};

const CACHE_HITS: &str = "cache_hits";
const CACHE_MISSES: &str = "cache_misses";
const DB_SIZE_MB: &str = "db_size_mb";
const ROCKSDB_SIZE_PROPERTY: &str = "rocksdb.total-sst-files-size";

/// Channel for sending requests to the database thread
///
/// The enclosing struct contains the request and a oneshot sender
/// for sending back a response.
pub type RequestBus<K, V> = mpsc::UnboundedSender<DbRequest<K, V>>;
pub type RequestSender = oneshot::Sender<Option<Vec<u8>>>;

/// Shared bytes trait for key value trait constraints on `sled` and `rocksdb` functions.
pub trait GenericBytes:
    Clone + std::cmp::Ord + AsRef<[u8]> + Into<Vec<u8>> + Sized + Send + Sync
{
}
impl<T> GenericBytes for T where
    T: Clone + std::cmp::Ord + AsRef<[u8]> + Into<Vec<u8>> + Sized + Send + Sync
{
}

/// Generic batch operation.
enum BatchOp<K, V>
where
    K: GenericBytes,
    V: GenericBytes,
{
    Insert(K, V),
    Delete(K),
}

/// Dumb generic batch type to handle conversions.
pub struct Batch<K, V>(Vec<BatchOp<K, V>>)
where
    K: GenericBytes,
    V: GenericBytes;
impl<K, V> Batch<K, V>
where
    K: GenericBytes,
    V: GenericBytes,
{
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }
    #[allow(dead_code)]
    pub fn insert(&mut self, key: K, value: V) {
        self.0.push(BatchOp::Insert(key, value))
    }
    pub fn delete(&mut self, key: K) {
        self.0.push(BatchOp::Delete(key))
    }
}
impl<K, V> From<Vec<BatchOp<K, V>>> for Batch<K, V>
where
    K: GenericBytes,
    V: GenericBytes,
{
    fn from(value: Vec<BatchOp<K, V>>) -> Self {
        Self(value)
    }
}

/// A generic database layer abstraction.
pub trait GenericDatabase: Send {
    type Error: std::fmt::Debug;
    type Config;

    /// Open a database from a config.
    fn open(config: &Self::Config) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// A database read operation.
    fn read<K: GenericBytes>(&self, key: K) -> Result<Option<Vec<u8>>, Self::Error>;

    /// A database write operation.
    fn write<K, V>(&self, key: K, val: V) -> Result<(), Self::Error>
    where
        K: GenericBytes,
        V: GenericBytes;

    /// A database batch operation.
    fn batch<K, V>(&self, batch: Batch<K, V>) -> Result<(), Self::Error>
    where
        K: GenericBytes,
        V: GenericBytes;

    /// A database flush operation.
    fn flush(&self) -> Result<(), Self::Error>;

    fn clear(&self) -> Result<(), Self::Error>;
}

impl GenericDatabase for sled::Db<{ crate::FANOUT }> {
    type Error = std::io::Error;
    type Config = sled::Config;

    fn open(config: &Self::Config) -> Result<Self, Self::Error> {
        sled::Config::open(config).inspect(|db| {
            match db.size_on_disk().map(|size| size / (1024 * 1024)) {
                Ok(size) => metrics::gauge!(DB_SIZE_MB).set(size as f64),
                Err(err) => tracing::warn!(?err, "failed to gauge database size"),
            }
        })
    }

    fn read<K: GenericBytes>(&self, key: K) -> Result<Option<Vec<u8>>, Self::Error> {
        self.get(key)
            .map(|opt| opt.map(|ia| ia.to_vec()))
            .inspect(|opt| {
                if opt.is_some() {
                    metrics::counter!(CACHE_HITS).increment(1);
                } else {
                    metrics::counter!(CACHE_MISSES).increment(1);
                }
            })
    }

    fn write<K, V>(&self, key: K, val: V) -> Result<(), Self::Error>
    where
        K: GenericBytes,
        V: GenericBytes,
    {
        // NOTE: Not sure if we ever used the returned value of a write for sled,
        // but we can't do that for rockdb so I'm assuming this is fine.
        self.insert(key, val.as_ref()).map(|_| ()).inspect(|_| {
            match self.size_on_disk().map(|size| size / (1024 * 1024)) {
                Ok(size) => metrics::gauge!(DB_SIZE_MB).set(size as f64),
                Err(err) => tracing::warn!(?err, "failed to gauge database size"),
            }
        })
    }

    fn batch<K, V>(&self, batch: Batch<K, V>) -> Result<(), Self::Error>
    where
        K: GenericBytes,
        V: GenericBytes,
    {
        let mut buf = sled::Batch::default();
        batch.0.into_iter().for_each(|op| {
            match op {
                BatchOp::Insert(key, value) => buf.insert(key.as_ref(), value.as_ref()),
                BatchOp::Delete(key) => buf.remove(key.as_ref()),
            }
        });
        self.apply_batch(buf).inspect(|_| {
            match self.size_on_disk().map(|size| size / (1024 * 1024)) {
                Ok(size) => metrics::gauge!(DB_SIZE_MB).set(size as f64),
                Err(err) => tracing::warn!(?err, "failed to gauge database size"),
            }
        })
    }

    // TODO: Consider collecting other metrics from flush.
    fn flush(&self) -> Result<(), Self::Error> {
        sled::Tree::<{ crate::FANOUT }>::flush(self)
            .map(|_| ())
            .inspect(|_| {
                match self.size_on_disk().map(|size| size / (1024 * 1024)) {
                    Ok(size) => metrics::gauge!(DB_SIZE_MB).set(size as f64),
                    Err(err) => tracing::warn!(?err, "failed to gauge database size"),
                }
            })
    }

    fn clear(&self) -> Result<(), Self::Error> {
        sled::Tree::<{ crate::FANOUT }>::clear(self).inspect(|_| {
            match self.size_on_disk().map(|size| size / (1024 * 1024)) {
                Ok(size) => metrics::gauge!(DB_SIZE_MB).set(size as f64),
                Err(err) => tracing::warn!(?err, "failed to gauge database size"),
            }
        })
    }
}

// Also important to note, some operations do behave differently between thread modes, such as
// column families, and we should have some kind of distinction for this option in the config docs.
//
// NOTE: If in the future the database size is heavily affected by WAL or other database files,
// we may also want to track that as part of `DB_SIZE_MB`. A list of properties can be found
// here: https://github.com/facebook/rocksdb/blob/08809f5e6cd9cc4bc3958dd4d59457ae78c76660/include/rocksdb/db.h#L654-L689.
impl<T: rocksdb::ThreadMode + Send> GenericDatabase for rocksdb::DBWithThreadMode<T> {
    type Error = rocksdb::Error;
    type Config = (rocksdb::Options, std::path::PathBuf);

    fn open(config: &Self::Config) -> Result<Self, Self::Error> {
        let (opts, path) = config;
        Self::open(opts, path).inspect(|db| {
            match db
                .property_int_value(ROCKSDB_SIZE_PROPERTY)
                .map(|opt| opt.map(|size| size / (1024 * 1024)))
            {
                Ok(size) => metrics::gauge!(DB_SIZE_MB).set(size.unwrap_or_default() as f64),
                Err(err) => tracing::warn!(?err, "failed to gauge database size"),
            }
        })
    }

    fn read<K: GenericBytes>(&self, key: K) -> Result<Option<Vec<u8>>, Self::Error> {
        self.get(key).inspect(|opt| {
            if opt.is_some() {
                metrics::counter!(CACHE_HITS).increment(1);
            } else {
                metrics::counter!(CACHE_MISSES).increment(1);
            }
        })
    }

    fn write<K, V>(&self, key: K, val: V) -> Result<(), Self::Error>
    where
        K: GenericBytes,
        V: GenericBytes,
    {
        self.put(key, val).inspect(|_| {
            match self
                .property_int_value(ROCKSDB_SIZE_PROPERTY)
                .map(|opt| opt.map(|size| size / (1024 * 1024)))
            {
                Ok(size) => metrics::gauge!(DB_SIZE_MB).set(size.unwrap_or_default() as f64),
                Err(err) => tracing::warn!(?err, "failed to gauge database size"),
            }
        })
    }

    fn batch<K, V>(&self, batch: Batch<K, V>) -> Result<(), Self::Error>
    where
        K: GenericBytes,
        V: GenericBytes,
    {
        let mut buf = rocksdb::WriteBatch::new();
        batch.0.iter().for_each(|op| {
            match op {
                BatchOp::Insert(key, value) => buf.put(key, value),
                BatchOp::Delete(key) => buf.delete(key),
            }
        });
        self.write(buf).inspect(|_| {
            match self
                .property_int_value(ROCKSDB_SIZE_PROPERTY)
                .map(|opt| opt.map(|size| size / (1024 * 1024)))
            {
                Ok(size) => metrics::gauge!(DB_SIZE_MB).set(size.unwrap_or_default() as f64),
                Err(err) => tracing::warn!(?err, "failed to gauge database size"),
            }
        })
    }

    fn flush(&self) -> Result<(), Self::Error> {
        self.flush().inspect(|_| {
            match self
                .property_int_value(ROCKSDB_SIZE_PROPERTY)
                .map(|opt| opt.map(|size| size / (1024 * 1024)))
            {
                Ok(size) => metrics::gauge!(DB_SIZE_MB).set(size.unwrap_or_default() as f64),
                Err(err) => tracing::warn!(?err, "failed to gauge database size"),
            }
        })
    }

    // Here we do a batch delete instead of `DB::destroy` to remove all SSTs but preserve everything else.
    // Since we call `GenericDatabase::batch`, metrics are collected there to avoid duplication.
    fn clear(&self) -> Result<(), Self::Error> {
        self.batch(Batch::<_, Box<[u8]>>::from(
            self.iterator(rocksdb::IteratorMode::Start)
                .filter_map(|item| item.map(|(key, _)| BatchOp::Delete(key)).ok())
                .collect::<Vec<BatchOp<_, _>>>(),
        ))
    }
}

/// Specifies if we are reading or writing to the DB.
pub enum RequestKind<K, V>
where
    K: GenericBytes,
    V: GenericBytes,
{
    Read(K),
    Write(K, V),
    Batch(Batch<K, V>),
    Flush,
}

/// Contains data to be sent to the DB thread for processing.
pub struct DbRequest<K, V>
where
    K: GenericBytes,
    V: GenericBytes,
{
    pub request: RequestKind<K, V>,
    pub sender: RequestSender,
}

impl<K, V> DbRequest<K, V>
where
    K: GenericBytes,
    V: GenericBytes,
{
    pub fn new(request: RequestKind<K, V>, sender: RequestSender) -> Self {
        DbRequest { request, sender }
    }
}
