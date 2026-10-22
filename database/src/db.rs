use rocksdb::{DBWithThreadMode, MultiThreaded};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;

pub use conn_builder::ConnBuilder;
use kaspa_utils::fd_budget::FDGuard;
pub use rocksdb_preset::{DEFAULT_PRESET_CONSENSUS_BLOCK_CACHE_BYTES, RocksDbPreset};

mod conn_builder;
mod rocksdb_preset;
mod storage_bounds;
pub use storage_bounds::StorageDbBounds;

/// The DB type used for Kaspad stores
pub struct DB {
    inner: DBWithThreadMode<MultiThreaded>,
    _fd_guard: FDGuard,
    native_single_directory_quota_path: Option<PathBuf>,
}

impl DB {
    /// Provenance is set only by this crate's audited factory with no external
    /// WAL path. An arbitrary RocksDB opened through DB::new remains unknown.
    pub(super) fn new_from_native_directory_factory_v1(
        inner: DBWithThreadMode<MultiThreaded>,
        fd_guard: FDGuard,
        single_directory: bool,
    ) -> Self {
        let native_single_directory_quota_path = single_directory.then(|| inner.path().to_path_buf());
        Self { inner, _fd_guard: fd_guard, native_single_directory_quota_path }
    }
    pub fn native_single_directory_quota_path_v1(&self) -> Option<&std::path::Path> {
        self.native_single_directory_quota_path.as_deref().filter(|path| *path == self.inner.path())
    }

    pub fn new(inner: DBWithThreadMode<MultiThreaded>, fd_guard: FDGuard) -> Self {
        Self { inner, _fd_guard: fd_guard, native_single_directory_quota_path: None }
    }
}

impl DerefMut for DB {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.native_single_directory_quota_path = None;
        &mut self.inner
    }
}

impl Deref for DB {
    type Target = DBWithThreadMode<MultiThreaded>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Deletes an existing DB if it exists
pub fn delete_db(db_dir: PathBuf) {
    if !db_dir.exists() {
        return;
    }
    let options = rocksdb::Options::default();
    let path = db_dir.to_str().unwrap();
    <DBWithThreadMode<MultiThreaded>>::destroy(&options, path).expect("DB is expected to be deletable");
}
