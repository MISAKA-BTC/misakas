use crate::{BlockHash, BlueWorkType, PruningPoint, hashing};
use borsh::{BorshDeserialize, BorshSerialize};
use itertools::Itertools;
// kaspa-pq (ADR-0004 / design §12): `utxo_commitment` is now `Hash64`
// (64-byte BLAKE2b-512 of the LtHash state) like every other PQ consensus
// identity. `Hash` (= `Hash32`) is retained only for the legacy 32-byte
// kHeavyHash PoW path; every block-hash field/parent below uses `BlockHash`
// (= `Hash64`); the pruning point uses `PruningPoint` (also `Hash64`).
// kaspa-pq Selected-Parent EVM Lane (ADR-0020, design v0.2 §3.2): the L1 header
// carries a single 64-byte `evm_commitment_root` (`Hash64`); the 32-byte
// Ethereum trie roots live in the block body's `EvmExecutionHeader`.
use kaspa_hashes::Hash64;
use kaspa_utils::{
    iter::{IterExtensions, IterExtensionsRle},
    mem_size::MemSizeEstimator,
};
use serde::{Deserialize, Serialize};
use std::mem::size_of;

/// An efficient run-length encoding for the parent-by-level vector in the block header.
/// The i-th run `(cum_count, parents)` indicates that for all levels in the range `prev_cum_count..cum_count`,
/// the parents are `parents`.
///
/// Example: `[(3, [A]), (5, [B])]` means levels 0-2 have parents `[A]`,
/// and levels 3-4 have parents `[B]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CompressedParents(Vec<(u8, Vec<BlockHash>)>);

impl CompressedParents {
    pub fn expanded_len(&self) -> usize {
        self.0.last().map(|(cum, _)| *cum as usize).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&[BlockHash]> {
        if index >= self.expanded_len() {
            return None;
        }
        if index == 0 {
            // Fast path for the common case of getting the first level (direct parents)
            return Some(&self.0[0].1);
        }
        // `partition_point` returns the index of the first element for which the predicate is false.
        // The predicate `cum - 1 < index` checks if a run is before the desired `index`.
        // The first run for which this is false is the one that contains our index.
        let i = self.0.partition_point(|(cum, _)| (*cum as usize) - 1 < index);
        Some(&self.0[i].1)
    }

    pub fn expanded_iter(&self) -> impl Iterator<Item = &'_ [BlockHash]> {
        self.0.iter().map(|(cum, v)| (*cum as usize, v.as_slice())).expand_rle()
    }

    /// Adds a new level of parents. This extends the last run if parents_at_level
    /// is identical to the last level, otherwise it starts a new run
    pub fn push(&mut self, parents_at_level: Vec<BlockHash>) {
        match self.0.last_mut() {
            Some((count, last_parents)) if *last_parents == parents_at_level => {
                *count = count.checked_add(1).expect("exceeded max levels of 255");
            }
            Some((count, _)) => {
                let next_cum = count.checked_add(1).expect("exceeded max levels of 255");
                self.0.push((next_cum, parents_at_level));
            }
            None => {
                self.0.push((1, parents_at_level));
            }
        }
    }

    /// Sets the direct parents (level 0) to the given value, preserving all other levels.
    ///
    /// NOTE: inefficient implementation, should be used for testing purposes only.
    pub fn set_direct_parents(&mut self, direct_parents: Vec<BlockHash>) {
        if self.0.is_empty() {
            self.0.push((1, direct_parents));
            return;
        }
        let mut parents: Vec<Vec<BlockHash>> = std::mem::take(self).into();
        parents[0] = direct_parents;
        *self = parents.try_into().unwrap();
    }

    /// Returns the internal cumulative-sum run-length encoded representation.
    pub fn raw(&self) -> &[(u8, Vec<BlockHash>)] {
        &self.0
    }
}

use crate::errors::header::CompressedParentsError;

impl TryFrom<Vec<Vec<BlockHash>>> for CompressedParents {
    type Error = CompressedParentsError;

    fn try_from(parents: Vec<Vec<BlockHash>>) -> Result<Self, Self::Error> {
        if parents.len() > u8::MAX as usize {
            return Err(CompressedParentsError::LevelsExceeded);
        }

        // Casting count from usize to u8 is safe because of the check above
        Ok(Self(parents.into_iter().rle_cumulative().map(|(count, level)| (count as u8, level)).collect()))
    }
}

impl TryFrom<Vec<(u8, Vec<BlockHash>)>> for CompressedParents {
    type Error = CompressedParentsError;
    fn try_from(parents: Vec<(u8, Vec<BlockHash>)>) -> Result<Self, Self::Error> {
        for ((last_cumulative_level, last_parents), (cumulative_level, parents)) in parents.iter().tuple_windows() {
            // Make sure any next cumulative_level is strictly greater than the last
            if cumulative_level <= last_cumulative_level {
                return Err(CompressedParentsError::LevelsNotStrictlyIncreasing);
            }
            // Verify compression, any consecutive runs must have different parents
            if last_parents == parents {
                return Err(CompressedParentsError::NotFullyCompressed);
            }
        }

        Ok(Self(parents))
    }
}

impl From<CompressedParents> for Vec<Vec<BlockHash>> {
    fn from(value: CompressedParents) -> Self {
        value.0.into_iter().map(|(cum, v)| (cum as usize, v)).expand_rle().collect()
    }
}

impl From<&CompressedParents> for Vec<Vec<BlockHash>> {
    fn from(value: &CompressedParents) -> Self {
        value.expanded_iter().map(|x| x.to_vec()).collect()
    }
}

/// @category Consensus
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    /// Cached hash
    pub hash: BlockHash,
    pub version: u16,
    pub parents_by_level: CompressedParents,
    /// PR-9.5c: widened to `MerkleRoot` (= [`Hash64`]). Receives
    /// the output of the kaspa-pq
    /// [`crate::merkle::calc_hash_merkle_root`] flow which now
    /// runs through the keyed BLAKE2b-512
    /// [`kaspa_hashes::MerkleBranchHash64`] hasher.
    pub hash_merkle_root: crate::MerkleRoot,
    /// PR-9.5c: widened to `AcceptedIdMerkleRoot` (= [`Hash64`]).
    /// Same rationale; underlying branch hasher is the
    /// domain-separated [`kaspa_hashes::AcceptedIdMerkleBranchHash64`].
    pub accepted_id_merkle_root: crate::AcceptedIdMerkleRoot,
    /// kaspa-pq (ADR-0004 / design §12): 64-byte BLAKE2b-512 UTXO-set commitment.
    pub utxo_commitment: Hash64,
    /// Timestamp is in milliseconds
    pub timestamp: u64,
    pub bits: u32,
    pub nonce: u64,
    /// kaspa-pq Phase 8/9 (PR-8.4, folded into PR-9.5d): Layer 1
    /// PoW algorithm discriminator. Part of the header identity and
    /// the pre-PoW hash so the same header body cannot be
    /// interpreted under two different Layer 1 algorithms. Phase 1
    /// admits only [`crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH`]
    /// (`= 1`); ASIC-hard variants are Phase 2+ hard-fork ADRs
    /// (ADR-0007). Placed after `nonce` and before `daa_score` to
    /// keep the `(timestamp, bits, nonce)` PoW triple contiguous
    /// (docs/hash64-migration-inventory.md §"Header hashing byte
    /// order").
    pub pow_algo_id: u8,
    pub daa_score: u64,
    pub blue_work: BlueWorkType,
    pub blue_score: u64,
    pub pruning_point: PruningPoint,

    // kaspa-pq Selected-Parent EVM Lane (ADR-0020, design v0.4 §4). Two EVM
    // commitments are present in every `Header` but only enter the header-hash
    // preimage when `version >= EVM_HEADER_VERSION` (see
    // `hashing::header::write_header_preimage`), so for all existing v0/v1
    // headers they are zero and hash-invisible — every current genesis hash and
    // block identity is unchanged. Under mergeset delayed acceptance (design
    // v0.4 §3) a block commits separately to (a) its OWN payload data — which
    // its selected child accepts/executes — and (b) the execution result of
    // accepting its mergeset's payloads. Hard-fork fields: the v2+ preimage
    // byte order (`evm_payload_hash` then `evm_commitment_root`, appended after
    // `pruning_point`) is frozen once testnet activates.
    /// Keyed BLAKE2b-512 (`MISAKA_EVM_PAYLOAD_HASH_CONTEXT`) over the borsh
    /// encoding of this block's own `EvmExecutionPayload` (design v0.4 §4.1) —
    /// the data commitment. Zero for pre-activation (v0/v1) headers.
    pub evm_payload_hash: Hash64,
    /// Keyed BLAKE2b-512 (`MISAKA_EVM_COMMITMENT_CONTEXT`) over the block body's
    /// `EvmExecutionHeader` — the mergeset-acceptance execution commitment
    /// (design v0.4 §4.1). Zero for pre-activation (v0/v1) headers.
    pub evm_commitment_root: Hash64,

    /// kaspa-pq ADR-0022: keyed BLAKE2b-512 (`MISAKA_OVERLAY_COMMITMENT_CONTEXT`)
    /// over the canonical `OverlaySnapshot` as-of this block — the DNS/PoS-v2
    /// overlay-state commitment that makes the overlay verifiable at a pruning
    /// point during pruned-IBD. The DNS overlay is genesis-active on every
    /// network (`dns_params.is_some()`), so unlike the two EVM commitments this
    /// field enters the header-hash preimage **unconditionally** (all versions);
    /// see `hashing::header::write_header_preimage`. Added to the preimage is a
    /// hard fork — every genesis hash is recomputed (ADR-0022 §8).
    pub overlay_commitment_root: Hash64,

    // ADR-0039 PALW Replica-GEMM audited-compute lane (design §13.1). Ten fields present in every
    // `Header` but entering the header-hash preimage ONLY when `version >= PALW_HEADER_VERSION`
    // (= 3), appended after `overlay_commitment_root` (see
    // `hashing::header::write_header_preimage`). So for every existing v0/v1/v2 header they are zero
    // and hash-invisible — no genesis hash or block identity changes. Frozen v3+ byte order
    // (design §13.2). Inert until the PALW activation fence; nothing mints a v3 header today.
    /// Cumulative blue HASH work (algo-3 floor). Zero for pre-v3 headers.
    pub blue_hash_work: BlueWorkType,
    /// Cumulative blue certified COMPUTE work (algo-4 lane), capped at 4× hash work into the
    /// effective `blue_work` (design §5.3 `E = H + min(C, 4H)`). Zero for pre-v3 headers.
    pub blue_compute_work: BlueWorkType,
    /// PALW batch the ticket belongs to. Zero for non-PALW headers.
    pub palw_batch_id: Hash64,
    /// Leaf index within the batch.
    pub palw_leaf_index: u32,
    /// First-class ticket nullifier (design §13 / invariant I-5): the DAG dedups tickets on this
    /// field, and the canonical algo-4 nonce equals its low 64 bits.
    pub palw_ticket_nullifier: Hash64,
    /// The `PalwBatchCertificateV2` hash this ticket activates under.
    pub palw_epoch_certificate_hash: Hash64,
    /// Consensus-derived fork-binding commitment (design §12.1, invariant I-4).
    pub palw_chain_commit: Hash64,
    /// The single target DAA interval this leaf draws in (design §12.2).
    pub palw_target_daa_interval: u64,
    /// Hash of the block-body ML-DSA `PalwBlockAuthorizationV1` (design §12.4).
    pub palw_authorization_hash: Hash64,
    /// [`crate::palw::PalwProofType`] discriminant (design §20.2).
    pub palw_proof_type: u8,
    /// ADR-0039 C6 — this block's own active beacon seed `R_E` (design §11.2). Present on EVERY v3
    /// header (both lanes), authenticated at the block's own virtual stage against the derived beacon
    /// state. It is the RETAINED, header-resident carrier of `R_E`: a descendant's clause-9 eligibility
    /// reads the LAGGED `R_E` from its finality-buried anchor's copy of this field — available at body
    /// stage on every node (headers sync before bodies, the anchor survives pruning), so the draw is a
    /// pure function of the past with no virtual-store read. Zero for pre-v3 headers.
    pub palw_beacon_seed: Hash64,

    // PALW public/value-network anti-spam extension. These fields are hash-invisible before Header
    // v4 and require a coordinated re-genesis before use. The commitment authenticates the exact
    // fork-local rolling state; the nonce is the sole post-authorization grinding degree of freedom.
    /// Commitment to the bounded fork-local anti-spam accumulator row derived for this block.
    pub palw_spam_accumulator_commitment: Hash64,
    /// Objective anti-spam stamp nonce. AUTH-02 intentionally substitutes only this field with zero.
    pub palw_spam_nonce: u64,

    // ADR-MA Header-v5 extension (§13): the generic Compute Set references. Hash-invisible before
    // Header v5 and forced to zero below the registry activation fence. With these three ids in
    // the header, ADDING/UPDATING/STOPPING a model never changes the header again — a block
    // names the exact immutable records that governed it, forever (§21.4).
    /// The Compute Set this PALW block was mined for (zero on hash-lane blocks).
    pub palw_compute_set_id: Hash64,
    /// The EXACT policy record in force at this block's source DAA (content id, §14).
    pub palw_compute_policy_id: Hash64,
    /// The EXACT allocation plan in force at this block's source DAA (content id, §14).
    pub palw_allocation_plan_id: Hash64,
}

/// The PALW ticket/work, beacon, and re-genesis v4 anti-spam commitments carried by a PALW header. Bundled so the
/// mining-template / GHOSTDAG paths can set them in one shot without a 10-argument builder; every
/// field defaults to the inert zero via [`Default`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwHeaderFields {
    pub blue_hash_work: BlueWorkType,
    pub blue_compute_work: BlueWorkType,
    pub palw_batch_id: Hash64,
    pub palw_leaf_index: u32,
    pub palw_ticket_nullifier: Hash64,
    pub palw_epoch_certificate_hash: Hash64,
    pub palw_chain_commit: Hash64,
    pub palw_target_daa_interval: u64,
    pub palw_authorization_hash: Hash64,
    pub palw_proof_type: u8,
    pub palw_beacon_seed: Hash64,
    pub palw_spam_accumulator_commitment: Hash64,
    pub palw_spam_nonce: u64,
    pub palw_compute_set_id: Hash64,
    pub palw_compute_policy_id: Hash64,
    pub palw_allocation_plan_id: Hash64,
}

impl Header {
    #[allow(clippy::too_many_arguments)]
    pub fn new_finalized(
        version: u16,
        parents_by_level: CompressedParents,
        hash_merkle_root: crate::MerkleRoot,
        accepted_id_merkle_root: crate::AcceptedIdMerkleRoot,
        utxo_commitment: Hash64,
        timestamp: u64,
        bits: u32,
        nonce: u64,
        // PR-9.5d: Layer 1 PoW algorithm discriminator, positioned
        // after `nonce` to mirror the header-hash byte order.
        pow_algo_id: u8,
        daa_score: u64,
        blue_work: BlueWorkType,
        blue_score: u64,
        pruning_point: PruningPoint,
    ) -> Self {
        let mut header = Self {
            hash: Default::default(), // Temp init before the finalize below
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            utxo_commitment,
            nonce,
            timestamp,
            pow_algo_id,
            daa_score,
            bits,
            blue_work,
            blue_score,
            pruning_point,
            // ADR-0020: the EVM commitments default to zero. v0/v1 headers never
            // hash them; the EVM-version (v2+) mining/template path sets them via
            // `with_evm_payload_hash` / `with_evm_commitment` before the PoW finalize.
            evm_payload_hash: Hash64::default(),
            evm_commitment_root: Hash64::default(),
            // ADR-0022: defaults to zero; the block-template/genesis path sets the
            // real overlay commitment via `with_overlay_commitment`. Unlike the EVM
            // commitments it is hashed unconditionally, so this default participates
            // in the header hash (genesis recompute, ADR-0022 §8).
            overlay_commitment_root: Hash64::default(),
            // ADR-0039: the PALW fields default to the inert zero. v0/v1/v2 headers never hash them;
            // the PALW-version (v3) template path sets them via `with_palw_fields` before finalize.
            ..Self::palw_zero()
        };
        header.finalize();
        header
    }

    /// The inert PALW-field defaults (all zero). Used via struct-update (`..Header::palw_zero()`) at
    /// every `Header` literal-construction site (in this and downstream crates) so a new PALW field
    /// is defaulted in ONE place. Only the PALW fields of the returned value are meaningful; the rest
    /// are throwaway zero and are always overridden by the explicit fields in the struct literal.
    pub fn palw_zero() -> Self {
        // SAFETY of `zeroed`-free construction: every PALW field is a plain zeroable value; we build
        // a throwaway with only the PALW fields set, and the struct-update syntax copies just those.
        Self {
            hash: Default::default(),
            version: 0,
            parents_by_level: Default::default(),
            hash_merkle_root: Default::default(),
            accepted_id_merkle_root: Default::default(),
            utxo_commitment: Default::default(),
            timestamp: 0,
            bits: 0,
            nonce: 0,
            pow_algo_id: 0,
            daa_score: 0,
            blue_work: 0u64.into(),
            blue_score: 0,
            pruning_point: Default::default(),
            evm_payload_hash: Default::default(),
            evm_commitment_root: Default::default(),
            overlay_commitment_root: Default::default(),
            blue_hash_work: 0u64.into(),
            blue_compute_work: 0u64.into(),
            palw_batch_id: Default::default(),
            palw_leaf_index: 0,
            palw_ticket_nullifier: Default::default(),
            palw_epoch_certificate_hash: Default::default(),
            palw_chain_commit: Default::default(),
            palw_target_daa_interval: 0,
            palw_authorization_hash: Default::default(),
            palw_proof_type: 0,
            palw_beacon_seed: Default::default(),
            palw_spam_accumulator_commitment: Default::default(),
            palw_spam_nonce: 0,
            palw_compute_set_id: Default::default(),
            palw_compute_policy_id: Default::default(),
            palw_allocation_plan_id: Default::default(),
        }
    }

    /// Set the PALW ticket/work commitments (and v4 anti-spam fields) and re-finalize the header hash.
    /// Consuming builder used by the PALW-version (v3) mining-template / GHOSTDAG paths and by tests.
    /// For v0/v1/v2 headers the fields stay hash-invisible regardless of value; callers that set them
    /// are expected to have bumped `version` to `PALW_HEADER_VERSION`.
    pub fn with_palw_fields(mut self, f: PalwHeaderFields) -> Self {
        self.blue_hash_work = f.blue_hash_work;
        self.blue_compute_work = f.blue_compute_work;
        self.palw_batch_id = f.palw_batch_id;
        self.palw_leaf_index = f.palw_leaf_index;
        self.palw_ticket_nullifier = f.palw_ticket_nullifier;
        self.palw_epoch_certificate_hash = f.palw_epoch_certificate_hash;
        self.palw_chain_commit = f.palw_chain_commit;
        self.palw_target_daa_interval = f.palw_target_daa_interval;
        self.palw_authorization_hash = f.palw_authorization_hash;
        self.palw_proof_type = f.palw_proof_type;
        self.palw_beacon_seed = f.palw_beacon_seed;
        self.palw_spam_accumulator_commitment = f.palw_spam_accumulator_commitment;
        self.palw_spam_nonce = f.palw_spam_nonce;
        self.palw_compute_set_id = f.palw_compute_set_id;
        self.palw_compute_policy_id = f.palw_compute_policy_id;
        self.palw_allocation_plan_id = f.palw_allocation_plan_id;
        self.finalize();
        self
    }

    /// True iff ANY PALW header field is non-zero. A pre-v3 header must have
    /// them all zero (they are hash-invisible until `version >= PALW_HEADER_VERSION`, so a non-zero
    /// value would be block-id malleability) — enforced by `check_header_version` while PALW is inactive.
    pub fn has_nonzero_palw_fields(&self) -> bool {
        let zero64 = Hash64::default();
        let zero_work = BlueWorkType::from(0u64);
        self.blue_hash_work != zero_work
            || self.blue_compute_work != zero_work
            || self.palw_batch_id != zero64
            || self.palw_leaf_index != 0
            || self.palw_ticket_nullifier != zero64
            || self.palw_epoch_certificate_hash != zero64
            || self.palw_chain_commit != zero64
            || self.palw_target_daa_interval != 0
            || self.palw_authorization_hash != zero64
            || self.palw_proof_type != 0
            || self.palw_beacon_seed != zero64
            || self.palw_spam_accumulator_commitment != zero64
            || self.palw_spam_nonce != 0
            || self.palw_compute_set_id != zero64
            || self.palw_compute_policy_id != zero64
            || self.palw_allocation_plan_id != zero64
    }

    /// kaspa-pq Selected-Parent EVM Lane (ADR-0020, design v0.4 §4.1): set the
    /// EVM execution commitment root and re-finalize the header hash. Consuming
    /// builder used by the EVM-version (v2+) mining/template path and by tests.
    /// For v0/v1 headers the commitment stays hash-invisible regardless of
    /// value, but callers that set it are expected to also have bumped `version`
    /// to `EVM_HEADER_VERSION`.
    pub fn with_evm_commitment(mut self, evm_commitment_root: Hash64) -> Self {
        self.evm_commitment_root = evm_commitment_root;
        self.finalize();
        self
    }

    /// kaspa-pq Selected-Parent EVM Lane (ADR-0020, design v0.4 §4.1): set the
    /// block's own EVM payload data commitment and re-finalize the header hash.
    /// Same version-gating semantics as [`Header::with_evm_commitment`].
    pub fn with_evm_payload_hash(mut self, evm_payload_hash: Hash64) -> Self {
        self.evm_payload_hash = evm_payload_hash;
        self.finalize();
        self
    }

    /// kaspa-pq ADR-0022: set the DNS/PoS-v2 overlay-state commitment and
    /// re-finalize the header hash. Consuming builder used by the block-template
    /// path (and genesis construction) to carry the `OverlaySnapshot` digest.
    /// The field is hashed on every version, so this always changes the hash.
    pub fn with_overlay_commitment(mut self, overlay_commitment_root: Hash64) -> Self {
        self.overlay_commitment_root = overlay_commitment_root;
        self.finalize();
        self
    }

    /// Finalizes the header and recomputes the header hash
    pub fn finalize(&mut self) {
        self.hash = hashing::header::hash(self);
    }

    pub fn direct_parents(&self) -> &[BlockHash] {
        match self.parents_by_level.get(0) {
            Some(parents) => parents,
            None => &[],
        }
    }

    /// WARNING: To be used for test purposes only
    pub fn from_precomputed_hash(hash: BlockHash, parents: Vec<BlockHash>) -> Header {
        Header {
            version: crate::constants::BLOCK_VERSION,
            hash,
            parents_by_level: vec![parents].try_into().unwrap(),
            hash_merkle_root: Default::default(),
            accepted_id_merkle_root: Default::default(),
            utxo_commitment: Default::default(),
            nonce: 0,
            timestamp: 0,
            // PR-9.5d: default to the Phase 1 kHeavyHash algo id.
            pow_algo_id: crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
            daa_score: 0,
            bits: 0,
            blue_work: 0.into(),
            blue_score: 0,
            pruning_point: Default::default(),
            // ADR-0020: this test ctor pins `version = BLOCK_VERSION` (= 1), so
            // the EVM commitments are hash-invisible; default them to zero.
            evm_payload_hash: Default::default(),
            evm_commitment_root: Default::default(),
            // ADR-0022: hashed unconditionally; default to zero for this test ctor.
            overlay_commitment_root: Default::default(),
            // ADR-0039: PALW fields default to the inert zero (this ctor pins version = BLOCK_VERSION
            // = 1 < PALW_HEADER_VERSION, so they are hash-invisible anyway).
            ..Self::palw_zero()
        }
    }
}

impl AsRef<Header> for Header {
    fn as_ref(&self) -> &Header {
        self
    }
}

impl MemSizeEstimator for Header {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
            + self.parents_by_level.0.iter().map(|(_, l)| l.len()).sum::<usize>() * size_of::<BlockHash>()
            + self.parents_by_level.0.len() * size_of::<(u8, Vec<BlockHash>)>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_math::Uint192;
    use serde_json::Value;

    fn hash(val: u8) -> BlockHash {
        BlockHash::from(val as u64)
    }

    /// ADR-0039 §13: `has_nonzero_palw_fields` is false for the all-zero (inert / pre-v3) header and
    /// true for a header with ANY single PALW field set — so `check_header_version` rejects a pre-v3
    /// header carrying hash-invisible non-zero PALW data (malleability).
    #[test]
    fn has_nonzero_palw_fields_detects_each_field() {
        let base = Header::from_precomputed_hash(Hash64::default(), vec![]);
        assert!(!base.has_nonzero_palw_fields(), "inert header is all-zero");
        // Each field, one at a time, must flip the predicate.
        let one64 = Hash64::from_bytes([1u8; 64]);
        let one_work = BlueWorkType::from(1u64);
        let mutate = |f: &dyn Fn(&mut Header)| {
            let mut h = Header::from_precomputed_hash(Hash64::default(), vec![]);
            f(&mut h);
            assert!(h.has_nonzero_palw_fields());
        };
        mutate(&|h| h.blue_hash_work = one_work);
        mutate(&|h| h.blue_compute_work = one_work);
        mutate(&|h| h.palw_batch_id = one64);
        mutate(&|h| h.palw_leaf_index = 1);
        mutate(&|h| h.palw_ticket_nullifier = one64);
        mutate(&|h| h.palw_epoch_certificate_hash = one64);
        mutate(&|h| h.palw_chain_commit = one64);
        mutate(&|h| h.palw_target_daa_interval = 1);
        mutate(&|h| h.palw_authorization_hash = one64);
        mutate(&|h| h.palw_proof_type = 1);
        mutate(&|h| h.palw_beacon_seed = one64);
        mutate(&|h| h.palw_spam_accumulator_commitment = one64);
        mutate(&|h| h.palw_spam_nonce = 1);
    }

    fn vec_from(slice: &[u8]) -> Vec<BlockHash> {
        slice.iter().map(|&v| hash(v)).collect()
    }

    fn serialize_parents(parents: &[Vec<BlockHash>]) -> Vec<u8> {
        let compressed: CompressedParents = (parents.to_vec()).try_into().unwrap();
        bincode::serialize(&compressed).unwrap()
    }

    fn deserialize_parents(bytes: &[u8]) -> bincode::Result<Vec<Vec<BlockHash>>> {
        let parents: CompressedParents = bincode::deserialize(bytes)?;
        Ok(parents.into())
    }

    #[test]
    fn test_header_ser() {
        let header = Header::new_finalized(
            1,
            vec![vec![1.into()]].try_into().unwrap(),
            Default::default(),
            Default::default(),
            Default::default(),
            234,
            23,
            567,
            // PR-9.5d: pow_algo_id (Phase 1 kHeavyHash).
            crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
            0,
            // kaspa-pq PR-8.5: BlueWorkType widened to Uint576; widen the
            // test fixture too. Low 3 limbs carry the original Uint192
            // pattern so the value (and any borsh-roundtrip hash of it)
            // is reproducible from the upstream history.
            Uint192([0x1234567890abcfed, 0xc0dec0ffeec0ffee, 0x1234567890abcdef]).into(),
            u64::MAX,
            Default::default(),
        );
        let json = serde_json::to_string(&header).unwrap();
        println!("{}", json);

        let v = serde_json::from_str::<Value>(&json).unwrap();
        let blue_work = v.get("blueWork").expect("missing `blueWork` property");
        let blue_work = blue_work.as_str().expect("`blueWork` is not a string");
        // kaspa-pq PR-8.5: BlueWorkType widened to Uint576 (72 bytes).
        // The hex form is 144 chars; the low 24 bytes are the original
        // upstream Uint192 pattern, the high 48 bytes are zero (the
        // `Uint192 -> Uint576` conversion zero-extends).
        assert_eq!(
            blue_work,
            "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001234567890abcdefc0dec0ffeec0ffee1234567890abcfed",
        );
        let blue_score = v.get("blueScore").expect("missing `blueScore` property");
        let blue_score: u64 = blue_score.as_u64().expect("blueScore is not a u64 compatible value");
        assert_eq!(blue_score, u64::MAX);

        let h = serde_json::from_str::<Header>(&json).unwrap();
        assert!(h.blue_score == header.blue_score && h.blue_work == header.blue_work);
    }

    #[test]
    fn parents_vrle_round_trip_multiple_runs() {
        let parents = vec![
            vec_from(&[1, 2, 3]),
            vec_from(&[1, 2, 3]),
            vec_from(&[1, 2, 3]),
            vec_from(&[4, 5]),
            vec_from(&[4, 5]),
            vec_from(&[6]),
        ];

        let bytes = serialize_parents(&parents);
        let decoded = deserialize_parents(&bytes).unwrap();
        assert_eq!(decoded, parents);
    }

    #[test]
    fn parents_vrle_round_trip_single_run() {
        let repeated = vec_from(&[9, 8, 7]);
        let parents = vec![repeated.clone(), repeated.clone(), repeated.clone()];

        let bytes = serialize_parents(&parents);
        let decoded = deserialize_parents(&bytes).unwrap();
        assert_eq!(decoded, parents);
    }

    #[test]
    fn parents_vrle_round_trip_empty() {
        let bytes = serialize_parents(&[]);
        let decoded = deserialize_parents(&bytes).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn compressed_parents_len_and_get() {
        // Test with multiple runs of different lengths
        let first = vec_from(&[1]);
        let second = vec_from(&[2, 3]);
        let third = vec_from(&[4]);
        let parents = vec![first.clone(), first.clone(), second.clone(), second.clone(), third.clone()];
        let compressed = CompressedParents::try_from(parents.clone()).unwrap();

        assert_eq!(compressed.expanded_len(), parents.len());
        assert!(!compressed.is_empty());

        // Test `get` at various positions
        assert_eq!(compressed.get(0), Some(first.as_slice()), "get first element");
        assert_eq!(compressed.get(1), Some(first.as_slice()), "get element in the middle of a run");
        assert_eq!(compressed.get(2), Some(second.as_slice()), "get first element of a new run");
        assert_eq!(compressed.get(3), Some(second.as_slice()), "get element in the middle of a new run");
        assert_eq!(compressed.get(4), Some(third.as_slice()), "get last element");
        assert_eq!(compressed.get(5), None, "get out of bounds (just over)");
        assert_eq!(compressed.get(10), None, "get out of bounds (far over)");

        let collected: Vec<&[BlockHash]> = compressed.expanded_iter().collect();
        let expected: Vec<&[BlockHash]> = parents.iter().map(|v| v.as_slice()).collect();
        assert_eq!(collected, expected);

        // Test with an empty vec
        let parents_empty: Vec<Vec<BlockHash>> = vec![];
        let compressed_empty: CompressedParents = parents_empty.try_into().unwrap();
        assert_eq!(compressed_empty.expanded_len(), 0);
        assert!(compressed_empty.is_empty());
        assert_eq!(compressed_empty.get(0), None);

        // Test with a single run
        let parents_single_run = vec![first.clone(), first.clone(), first.clone()];
        let compressed_single_run: CompressedParents = parents_single_run.try_into().unwrap();
        assert_eq!(compressed_single_run.expanded_len(), 3);
        assert_eq!(compressed_single_run.get(0), Some(first.as_slice()));
        assert_eq!(compressed_single_run.get(1), Some(first.as_slice()));
        assert_eq!(compressed_single_run.get(2), Some(first.as_slice()));
        assert_eq!(compressed_single_run.get(3), None);
    }

    #[test]
    fn test_compressed_parents_push() {
        let mut compressed = CompressedParents(Vec::new());
        let level1 = vec_from(&[1, 2]);
        let level2 = vec_from(&[3, 4]);

        // 1. Push to empty
        compressed.push(level1.clone());
        assert_eq!(compressed.expanded_len(), 1);
        assert_eq!(compressed.0, vec![(1, level1.clone())]);

        // 2. Push same (extend run)
        compressed.push(level1.clone());
        assert_eq!(compressed.expanded_len(), 2);
        assert_eq!(compressed.0, vec![(2, level1.clone())]);

        // 3. Push different (new run)
        compressed.push(level2.clone());
        assert_eq!(compressed.expanded_len(), 3);
        assert_eq!(compressed.0, vec![(2, level1), (3, level2)]);
    }

    #[test]
    fn compressed_parents_binary_format_matches_runs() {
        let parents = vec![vec_from(&[1, 2, 3]), vec_from(&[1, 2, 3]), vec_from(&[4])];
        let compressed: CompressedParents = parents.try_into().unwrap();

        let encoded = bincode::serialize(&compressed).unwrap();
        let expected = bincode::serialize(&compressed.0).unwrap();
        assert_eq!(encoded, expected);
    }
}
