//! The async storage seam the [`crate::engine`] drives, plus adapters over the
//! core stores.
//!
//! The core [`TransferStore`](misaka_evm_indexer_core::TransferStore) is
//! synchronous (the in-memory reference impl); the PostgreSQL backend is async.
//! [`IndexStore`] is the async super-seam the engine actually uses, with two
//! adapters: [`MemIndexStore`] (wraps the sync core store — used by the engine
//! tests, so the driver is exercised without a database) and `PgIndexStore`
//! (the `pg` feature, wraps the audited PostgreSQL backend). Both expose the
//! SAME methods, so the engine is identical against either.

use alloy_primitives::U256;
use async_trait::async_trait;
use misaka_evm_indexer_core::{IndexedBlock, LocatedTransfer, MemStore, TransferStore};

/// The async persistence seam. Mirrors [`TransferStore`](misaka_evm_indexer_core::TransferStore)
/// but `async`, so a database backend fits. Write methods are idempotent (safe to
/// replay after a reconnect), exactly as the core trait specifies.
#[async_trait]
pub trait IndexStore {
    type Error: std::fmt::Debug + std::fmt::Display + Send + Sync + 'static;

    async fn apply_block(&mut self, block: &IndexedBlock, transfers: &[LocatedTransfer]) -> Result<(), Self::Error>;
    async fn revert_block(&mut self, block_hash: &[u8; 32]) -> Result<(), Self::Error>;
    async fn set_finalized(&mut self, up_to_number: u64) -> Result<(), Self::Error>;
    async fn head(&self) -> Result<Option<IndexedBlock>, Self::Error>;
    async fn canonical_block_at(&self, number: u64) -> Result<Option<IndexedBlock>, Self::Error>;

    // Read surface for the query API (§10.6 follow-on); cheap to expose now so
    // the seam is complete and Mem/Pg are proven to answer identically.
    async fn erc20_balance(&self, token: [u8; 20], owner: [u8; 20]) -> Result<U256, Self::Error>;
    async fn erc721_owner(&self, collection: [u8; 20], token_id: U256) -> Result<Option<[u8; 20]>, Self::Error>;
    async fn erc1155_balance(&self, collection: [u8; 20], token_id: U256, owner: [u8; 20]) -> Result<U256, Self::Error>;
}

/// In-memory adapter over the core [`MemStore`]. The engine's tests run against
/// this, so the reconcile/backfill driver is verified with no database.
#[derive(Debug, Default)]
pub struct MemIndexStore {
    inner: MemStore,
}

impl MemIndexStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl IndexStore for MemIndexStore {
    // The core MemStore is `Infallible`.
    type Error = std::convert::Infallible;

    async fn apply_block(&mut self, block: &IndexedBlock, transfers: &[LocatedTransfer]) -> Result<(), Self::Error> {
        self.inner.apply_block(block, transfers)
    }
    async fn revert_block(&mut self, block_hash: &[u8; 32]) -> Result<(), Self::Error> {
        self.inner.revert_block(block_hash)
    }
    async fn set_finalized(&mut self, up_to_number: u64) -> Result<(), Self::Error> {
        self.inner.set_finalized(up_to_number)
    }
    async fn head(&self) -> Result<Option<IndexedBlock>, Self::Error> {
        self.inner.head()
    }
    async fn canonical_block_at(&self, number: u64) -> Result<Option<IndexedBlock>, Self::Error> {
        self.inner.canonical_block_at(number)
    }
    async fn erc20_balance(&self, token: [u8; 20], owner: [u8; 20]) -> Result<U256, Self::Error> {
        self.inner.erc20_balance(token, owner)
    }
    async fn erc721_owner(&self, collection: [u8; 20], token_id: U256) -> Result<Option<[u8; 20]>, Self::Error> {
        self.inner.erc721_owner(collection, token_id)
    }
    async fn erc1155_balance(&self, collection: [u8; 20], token_id: U256, owner: [u8; 20]) -> Result<U256, Self::Error> {
        self.inner.erc1155_balance(collection, token_id, owner)
    }
}

/// PostgreSQL adapter (`pg` feature) over the audited `misaka-evm-indexer-pg`
/// backend. It forwards to the same load → apply → write-back logic the PG crate
/// already implements; this is pure delegation, so no balance/reorg logic is
/// duplicated here.
#[cfg(feature = "pg")]
mod pg {
    use super::*;
    use misaka_evm_indexer_pg::{PgError, PgStore};

    /// Wraps a connected [`PgStore`].
    pub struct PgIndexStore {
        inner: PgStore,
    }

    impl PgIndexStore {
        /// Connect and run the (idempotent) schema migration.
        pub async fn connect(url: &str) -> Result<Self, PgError> {
            let inner = PgStore::connect(url).await?;
            inner.migrate().await?;
            Ok(Self { inner })
        }
    }

    #[async_trait]
    impl IndexStore for PgIndexStore {
        type Error = PgError;

        async fn apply_block(&mut self, block: &IndexedBlock, transfers: &[LocatedTransfer]) -> Result<(), Self::Error> {
            self.inner.apply_block(block, transfers).await
        }
        async fn revert_block(&mut self, block_hash: &[u8; 32]) -> Result<(), Self::Error> {
            self.inner.revert_block(block_hash).await
        }
        async fn set_finalized(&mut self, up_to_number: u64) -> Result<(), Self::Error> {
            self.inner.set_finalized(up_to_number).await
        }
        async fn head(&self) -> Result<Option<IndexedBlock>, Self::Error> {
            self.inner.head().await
        }
        async fn canonical_block_at(&self, number: u64) -> Result<Option<IndexedBlock>, Self::Error> {
            self.inner.canonical_block_at(number).await
        }
        async fn erc20_balance(&self, token: [u8; 20], owner: [u8; 20]) -> Result<U256, Self::Error> {
            self.inner.erc20_balance(token, owner).await
        }
        async fn erc721_owner(&self, collection: [u8; 20], token_id: U256) -> Result<Option<[u8; 20]>, Self::Error> {
            self.inner.erc721_owner(collection, token_id).await
        }
        async fn erc1155_balance(&self, collection: [u8; 20], token_id: U256, owner: [u8; 20]) -> Result<U256, Self::Error> {
            self.inner.erc1155_balance(collection, token_id, owner).await
        }
    }
}

#[cfg(feature = "pg")]
pub use pg::PgIndexStore;
