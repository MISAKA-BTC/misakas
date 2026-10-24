//! Historical census reads: current rows are measured without materializing a
//! Header/Vec/cache entry. Legacy conversion has a separate typed decode lease.
use super::*;
use bincode::Options;
use kaspa_database::prelude::DbKey;
use serde::de::{self, SeqAccess, Visitor};
use std::fmt;

#[derive(Default)]
struct HashListCount(u64);
impl<'de> Deserialize<'de> for HashListCount {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = HashListCount;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("header parent hashes")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut count = 0u64;
                while seq.next_element::<BlockHash>()?.is_some() {
                    count = count.checked_add(1).ok_or_else(|| de::Error::custom("parent count overflow"))?;
                }
                if count > u32::MAX as u64 {
                    return Err(de::Error::custom("parent count exceeds Borsh length"));
                }
                Ok(HashListCount(count))
            }
        }
        d.deserialize_seq(V)
    }
}
#[derive(Default)]
struct CountedParents {
    runs: u64,
    hashes: u64,
}
impl<'de> Deserialize<'de> for CountedParents {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = CountedParents;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("compressed parent runs")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = CountedParents::default();
                while let Some((_level, hashes)) = seq.next_element::<(u8, HashListCount)>()? {
                    out.runs = out.runs.checked_add(1).ok_or_else(|| de::Error::custom("run count overflow"))?;
                    out.hashes = out.hashes.checked_add(hashes.0).ok_or_else(|| de::Error::custom("hash count overflow"))?;
                }
                if out.runs > u32::MAX as u64 {
                    return Err(de::Error::custom("run count exceeds Borsh length"));
                }
                Ok(out)
            }
        }
        d.deserialize_seq(V)
    }
}
#[derive(Deserialize)]
struct HeaderMeasure<'a> {
    hash: BlockHash,
    version: u16,
    parents_by_level: CountedParents,
    hash_merkle_root: kaspa_consensus_core::MerkleRoot,
    accepted_id_merkle_root: kaspa_consensus_core::AcceptedIdMerkleRoot,
    utxo_commitment: Hash64,
    timestamp: u64,
    bits: u32,
    nonce: u64,
    pow_algo_id: u8,
    daa_score: u64,
    blue_work: kaspa_consensus_core::BlueWorkType,
    blue_score: u64,
    pruning_point: kaspa_consensus_core::PruningPoint,
    evm_payload_hash: Hash64,
    evm_commitment_root: Hash64,
    overlay_commitment_root: Hash64,
    palw_state_root: Hash64,
    #[serde(borrow)]
    palw_commitment: &'a [u8],
}
#[derive(Deserialize)]
struct RowMeasure<'a>(#[serde(borrow)] HeaderMeasure<'a>, u8);
fn measure_current(bytes: &[u8]) -> Result<u64, StoreError> {
    // Match bincode::serialize's fixed-width representation; reject trailing
    // fields so schema drift cannot silently produce a false length.
    let RowMeasure(h, _level) = bincode::DefaultOptions::new().with_fixint_encoding().reject_trailing_bytes().deserialize(bytes)?;
    let parents = h.parents_by_level;
    let payload = u32::try_from(h.palw_commitment.len())
        .map_err(|_| StoreError::DataInconsistency("header commitment exceeds Borsh length".into()))? as u64;
    // Exhaustive Header construction makes added/changed fields a compile error.
    // Only the two dynamic fields are empty; the counted suffix is exact.
    let empty = Header {
        hash: h.hash,
        version: h.version,
        parents_by_level: Default::default(),
        hash_merkle_root: h.hash_merkle_root,
        accepted_id_merkle_root: h.accepted_id_merkle_root,
        utxo_commitment: h.utxo_commitment,
        timestamp: h.timestamp,
        bits: h.bits,
        nonce: h.nonce,
        pow_algo_id: h.pow_algo_id,
        daa_score: h.daa_score,
        blue_work: h.blue_work,
        blue_score: h.blue_score,
        pruning_point: h.pruning_point,
        evm_payload_hash: h.evm_payload_hash,
        evm_commitment_root: h.evm_commitment_root,
        overlay_commitment_root: h.overlay_commitment_root,
        palw_state_root: h.palw_state_root,
        palw_commitment: Default::default(),
    };
    let base = borsh::object_length(&empty).map_err(|e| StoreError::DataInconsistency(e.to_string()))? as u64;
    parents
        .runs
        .checked_mul(5)
        .and_then(|n| parents.hashes.checked_mul(64).and_then(|h| n.checked_add(h)))
        .and_then(|n| n.checked_add(payload))
        .and_then(|n| n.checked_add(base))
        .ok_or_else(|| StoreError::DataInconsistency("header Borsh length overflow".into()))
}

#[derive(Default)]
struct LegacyParents {
    levels: u64,
    hashes: u64,
}
impl<'de> Deserialize<'de> for LegacyParents {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = LegacyParents;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("legacy parent levels")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = LegacyParents::default();
                while let Some(hashes) = seq.next_element::<HashListCount>()? {
                    out.levels = out.levels.checked_add(1).ok_or_else(|| de::Error::custom("legacy levels overflow"))?;
                    out.hashes = out.hashes.checked_add(hashes.0).ok_or_else(|| de::Error::custom("legacy hashes overflow"))?;
                }
                if out.levels > u8::MAX as u64 {
                    return Err(de::Error::custom("legacy parent levels exceed Header conversion"));
                }
                Ok(out)
            }
        }
        d.deserialize_seq(V)
    }
}
#[allow(dead_code)]
#[derive(Deserialize)]
struct LegacyMeasure {
    hash: BlockHash,
    version: u16,
    parents_by_level: LegacyParents,
    hash_merkle_root: kaspa_consensus_core::MerkleRoot,
    accepted_id_merkle_root: kaspa_consensus_core::AcceptedIdMerkleRoot,
    utxo_commitment: Hash64,
    timestamp: u64,
    bits: u32,
    nonce: u64,
    daa_score: u64,
    blue_work: kaspa_consensus_core::BlueWorkType,
    blue_score: u64,
    pruning_point: BlockHash,
}
#[allow(dead_code)]
#[derive(Deserialize)]
struct LegacyRowMeasure(LegacyMeasure, u8);

fn measure_legacy(bytes: &[u8]) -> Result<u64, StoreError> {
    use kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1;
    let LegacyRowMeasure(h, _) = bincode::DefaultOptions::new().with_fixint_encoding().reject_trailing_bytes().deserialize(bytes)?;
    // Decode and conversion overlap: original outer Vec + the compressed run
    // Vec, original hash elements, both header layouts and Arc bookkeeping.
    // Typed old/new growth slots (2*n + 4 metadata, 2*n hashes) cover
    // geometric Vec growth; they are not an encoded-byte multiplier.
    let p = h.parents_by_level;
    let requests = p
        .levels
        .checked_mul(2)
        .and_then(|n| n.checked_add(4))
        .and_then(|n| n.checked_mul((size_of::<Vec<BlockHash>>() + size_of::<(u8, Vec<BlockHash>)>()) as u64))
        .and_then(|n| p.hashes.checked_mul((2 * size_of::<BlockHash>()) as u64).and_then(|h| n.checked_add(h)))
        .and_then(|n| n.checked_add((size_of::<HeaderWithBlockLevel2>() + size_of::<Header>() + 2 * size_of::<usize>()) as u64))
        .ok_or_else(|| StoreError::LocalResourceUnavailable("legacy header decode request overflow".into()))?;
    let _lease = reserve_native_requests_v1(requests).map_err(|e| StoreError::LocalResourceUnavailable(e.to_string()))?;
    let decoded: HeaderWithBlockLevel2 = bincode::deserialize(bytes)?;
    let current: HeaderWithBlockLevel = decoded.into();
    borsh::object_length(current.header.as_ref()).map(|n| n as u64).map_err(|e| StoreError::DataInconsistency(e.to_string()))
}

impl DbHeadersStore {
    /// Exact Borsh carriage size for the read-only historical census. A cold
    /// current row does not allocate a Header or enter the full-header cache.
    /// Only actual key absence is absence; corrupt/storage/capacity failures
    /// remain errors for the census caller to report.
    pub fn historical_header_borsh_len_v2(&self, hash: BlockHash) -> Result<u64, StoreError> {
        if let Some(row) = self.headers_access.read_from_cache(hash) {
            return borsh::object_length(row.header.as_ref())
                .map(|n| n as u64)
                .map_err(|e| StoreError::DataInconsistency(e.to_string()));
        }
        let prefix: Vec<u8> = DatabaseStorePrefixes::CompressedHeaders.into();
        let key = DbKey::new(&prefix, hash);
        if let Some(bytes) = self.db.get_pinned(&key)? {
            return measure_current(&bytes);
        }
        let key = DbKey::new(&self.fallback_prefix, hash);
        match self.db.get_pinned(&key)? {
            Some(bytes) => measure_legacy(&bytes),
            None => Err(StoreError::KeyNotFound(key)),
        }
    }
}

#[cfg(test)]
#[path = "historical_measure_v2_tests.rs"]
mod tests;
