use std::sync::Arc;

use kaspa_consensus_core::{BlockHash, BlockHasher, BlockLevel, header::Header};
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess};
use kaspa_database::prelude::{CachePolicy, DB};
use kaspa_database::prelude::{StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
// kaspa-pq (ADR-0004 / design §12): Header2.utxo_commitment is 64-byte Hash64.
use kaspa_hashes::Hash64;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

pub trait HeaderStoreReader {
    fn get_daa_score(&self, hash: BlockHash) -> Result<u64, StoreError>;
    fn get_blue_score(&self, hash: BlockHash) -> Result<u64, StoreError>;
    fn get_timestamp(&self, hash: BlockHash) -> Result<u64, StoreError>;
    fn get_bits(&self, hash: BlockHash) -> Result<u32, StoreError>;
    fn get_header(&self, hash: BlockHash) -> Result<Arc<Header>, StoreError>;
    fn get_header_with_block_level(&self, hash: BlockHash) -> Result<HeaderWithBlockLevel, StoreError>;
    fn get_compact_header_data(&self, hash: BlockHash) -> Result<CompactHeaderData, StoreError>;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HeaderWithBlockLevel {
    pub header: Arc<Header>,
    pub block_level: BlockLevel,
}

impl MemSizeEstimator for HeaderWithBlockLevel {
    fn estimate_mem_bytes(&self) -> usize {
        self.header.as_ref().estimate_mem_bytes() + size_of::<Self>()
    }
}

pub trait HeaderStore: HeaderStoreReader {
    // This is append only
    fn insert(&self, hash: BlockHash, header: Arc<Header>, block_level: BlockLevel) -> Result<(), StoreError>;
    fn delete(&self, hash: BlockHash) -> Result<(), StoreError>;
}

/// A temporary struct for backward compatibility. This struct is used to deserialize old header data with
/// parents_by_level as Vec<Vec<BlockHash>>.
///
/// PR-9.5c: merkle root fields widened to `Hash64` to match the
/// post-cascade `Header` shape; this means an upstream-Kaspa DB
/// with 32-byte merkle roots simply will not deserialise (which
/// is the kaspa-pq behaviour mandated by ADR-0001 — no DB
/// migration). PR-9.5f formally bumps the DB namespace and
/// rejects old-shape DBs at open time.
#[derive(Clone, Debug, Deserialize)]
struct Header2 {
    pub hash: BlockHash,
    pub version: u16,
    pub parents_by_level: Vec<Vec<BlockHash>>,
    pub hash_merkle_root: kaspa_consensus_core::MerkleRoot,
    pub accepted_id_merkle_root: kaspa_consensus_core::AcceptedIdMerkleRoot,
    // kaspa-pq (ADR-0004 / design §12): 64-byte BLAKE2b-512 UTXO-set commitment.
    pub utxo_commitment: Hash64,
    pub timestamp: u64,
    pub bits: u32,
    pub nonce: u64,
    pub daa_score: u64,
    pub blue_work: kaspa_consensus_core::BlueWorkType,
    pub blue_score: u64,
    pub pruning_point: BlockHash,
}

#[derive(Clone, Deserialize)]
struct HeaderWithBlockLevel2 {
    header: Header2,
    block_level: BlockLevel,
}
impl From<HeaderWithBlockLevel2> for HeaderWithBlockLevel {
    fn from(value: HeaderWithBlockLevel2) -> Self {
        Self {
            header: Header {
                hash: value.header.hash,
                version: value.header.version,
                parents_by_level: value.header.parents_by_level.try_into().unwrap(),
                hash_merkle_root: value.header.hash_merkle_root,
                accepted_id_merkle_root: value.header.accepted_id_merkle_root,
                utxo_commitment: value.header.utxo_commitment,
                timestamp: value.header.timestamp,
                bits: value.header.bits,
                nonce: value.header.nonce,
                // PR-9.5d: the Header2 back-compat shim reads pre-kaspa-pq
                // data with no pow_algo_id; default to Phase 1 kHeavyHash.
                // (Per ADR-0001 this shim is dead for kaspa-pq — old DBs are
                // rejected — but it must still type-check.)
                pow_algo_id: kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
                daa_score: value.header.daa_score,
                blue_work: value.header.blue_work,
                blue_score: value.header.blue_score,
                pruning_point: value.header.pruning_point,
                // ADR-0020: `Header2` is the pre-EVM (v0/v1) back-compat shape and
                // carries no EVM fields; default them to zero. Dead under ADR-0001
                // (old DBs are rejected) but must type-check.
                evm_payload_hash: Default::default(),
                evm_commitment_root: Default::default(),
                // ADR-0022: pre-overlay-commitment back-compat shim; default to zero.
                // Dead under ADR-0001 (old DBs are rejected) but must type-check.
                overlay_commitment_root: Default::default(),
            }
            .into(),
            block_level: value.block_level,
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct CompactHeaderData {
    pub daa_score: u64,
    pub timestamp: u64,
    pub bits: u32,
    pub blue_score: u64,
}

impl MemSizeEstimator for CompactHeaderData {}

impl From<&Header> for CompactHeaderData {
    fn from(header: &Header) -> Self {
        Self { daa_score: header.daa_score, timestamp: header.timestamp, bits: header.bits, blue_score: header.blue_score }
    }
}

/// A DB + cache implementation of `HeaderStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbHeadersStore {
    db: Arc<DB>,
    compact_headers_access: CachedDbAccess<BlockHash, CompactHeaderData, BlockHasher>,
    headers_access: CachedDbAccess<BlockHash, HeaderWithBlockLevel, BlockHasher>,
    fallback_prefix: Vec<u8>,
}

impl DbHeadersStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy, compact_cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            compact_headers_access: CachedDbAccess::new(
                Arc::clone(&db),
                compact_cache_policy,
                DatabaseStorePrefixes::HeadersCompact.into(),
            ),
            headers_access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::CompressedHeaders.into()),
            fallback_prefix: DatabaseStorePrefixes::Headers.into(),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy, compact_cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy, compact_cache_policy)
    }

    pub fn has(&self, hash: BlockHash) -> StoreResult<bool> {
        self.headers_access.has_with_fallback(self.fallback_prefix.as_ref(), hash)
    }

    pub fn insert_batch(
        &self,
        batch: &mut WriteBatch,
        hash: BlockHash,
        header: Arc<Header>,
        block_level: BlockLevel,
    ) -> Result<(), StoreError> {
        if self.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.headers_access.write(BatchDbWriter::new(batch), hash, HeaderWithBlockLevel { header: header.clone(), block_level })?;
        self.compact_headers_access.write(BatchDbWriter::new(batch), hash, header.as_ref().into())?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.compact_headers_access.delete(BatchDbWriter::new(batch), hash)?;
        self.headers_access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl HeaderStoreReader for DbHeadersStore {
    fn get_daa_score(&self, hash: BlockHash) -> Result<u64, StoreError> {
        if let Some(header_with_block_level) = self.headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.daa_score);
        }
        Ok(self.compact_headers_access.read(hash)?.daa_score)
    }

    fn get_blue_score(&self, hash: BlockHash) -> Result<u64, StoreError> {
        if let Some(header_with_block_level) = self.headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.blue_score);
        }
        Ok(self.compact_headers_access.read(hash)?.blue_score)
    }

    fn get_timestamp(&self, hash: BlockHash) -> Result<u64, StoreError> {
        if let Some(header_with_block_level) = self.headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.timestamp);
        }
        Ok(self.compact_headers_access.read(hash)?.timestamp)
    }

    fn get_bits(&self, hash: BlockHash) -> Result<u32, StoreError> {
        if let Some(header_with_block_level) = self.headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.bits);
        }
        Ok(self.compact_headers_access.read(hash)?.bits)
    }

    fn get_header(&self, hash: BlockHash) -> Result<Arc<Header>, StoreError> {
        Ok(self.headers_access.read_with_fallback::<HeaderWithBlockLevel2>(self.fallback_prefix.as_ref(), hash)?.header)
    }

    fn get_header_with_block_level(&self, hash: BlockHash) -> Result<HeaderWithBlockLevel, StoreError> {
        self.headers_access.read_with_fallback::<HeaderWithBlockLevel2>(self.fallback_prefix.as_ref(), hash)
    }

    fn get_compact_header_data(&self, hash: BlockHash) -> Result<CompactHeaderData, StoreError> {
        if let Some(header_with_block_level) = self.headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.as_ref().into());
        }
        self.compact_headers_access.read(hash)
    }
}

impl HeaderStore for DbHeadersStore {
    fn insert(&self, hash: BlockHash, header: Arc<Header>, block_level: u8) -> Result<(), StoreError> {
        if self.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        if self.compact_headers_access.has(hash)? {
            return Err(StoreError::DataInconsistency(format!("store has compact data for {} but is missing full data", hash)));
        }
        let mut batch = WriteBatch::default();
        self.compact_headers_access.write(BatchDbWriter::new(&mut batch), hash, header.as_ref().into())?;
        self.headers_access.write(BatchDbWriter::new(&mut batch), hash, HeaderWithBlockLevel { header, block_level })?;
        self.db.write(batch)?;
        Ok(())
    }

    fn delete(&self, hash: BlockHash) -> Result<(), StoreError> {
        let mut batch = WriteBatch::default();
        self.compact_headers_access.delete(BatchDbWriter::new(&mut batch), hash)?;
        self.headers_access.delete(BatchDbWriter::new(&mut batch), hash)?;
        self.db.write(batch)?;
        Ok(())
    }
}
