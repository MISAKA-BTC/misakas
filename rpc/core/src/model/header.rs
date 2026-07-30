use crate::RpcError;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    BlockHash, // PR-9.5e: block ids (hash, parents, pruning point) are Hash64
    BlueWorkType,
    header::{CompressedParents, Header, PalwHeaderFields},
};
use kaspa_hashes::Hash64; // kaspa-pq (ADR-0004 / design §12): utxo_commitment is 64-byte BLAKE2b-512
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

pub type RpcCompressedParents = CompressedParents;

/// Raw Rpc header type - without a cached header hash.
/// Used for mining APIs (get_block_template & submit_block)
///
/// PR-9.5c: `hash_merkle_root` / `accepted_id_merkle_root`
/// widened to `MerkleRoot` / `AcceptedIdMerkleRoot` (= `Hash64`)
/// per ADR-0008; on the RPC wire this is a longer hex string
/// (128 chars vs 64) — proto field types are unchanged.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcRawHeader {
    pub version: u16,
    pub parents_by_level: Vec<Vec<BlockHash>>,
    pub hash_merkle_root: kaspa_consensus_core::MerkleRoot,
    pub accepted_id_merkle_root: kaspa_consensus_core::AcceptedIdMerkleRoot,
    pub utxo_commitment: Hash64,
    /// Timestamp is in milliseconds
    pub timestamp: u64,
    pub bits: u32,
    pub nonce: u64,
    /// kaspa-pq Phase 2 PoW (ADR-0007): Layer-1 algorithm id (1 = kHeavyHash, 2 = Argon2id). Must
    /// round-trip so the get_block_template → miner → submit_block path mines/submits the
    /// network-correct algorithm.
    pub pow_algo_id: u8,
    pub daa_score: u64,
    pub blue_work: BlueWorkType,
    pub blue_score: u64,
    pub pruning_point: BlockHash,
    /// kaspa-pq EVM Lane v0.4 (ADR-0020 §4): both EVM commitments are part of
    /// the v2+ header-hash preimage, so they MUST round-trip through the
    /// mining (get_block_template → submit_block) and block RPCs — the
    /// pow_algo_id precedent. Zero on v0/v1 headers (hash-invisible there).
    pub evm_payload_hash: Hash64,
    pub evm_commitment_root: Hash64,
    /// kaspa-pq ADR-0022: the DNS/PoS-v2 overlay-state commitment. Part of the
    /// header-hash preimage on every version, so it MUST round-trip through the
    /// mining (get_block_template → submit_block) and block RPCs — the
    /// pow_algo_id / EVM-commitment precedent.
    pub overlay_commitment_root: Hash64,
    /// kaspa-pq ADR-0039 PALW: the ten Header-v3 fields. Part of the header-hash preimage only for
    /// version >= 3, so they MUST round-trip through the mining (get_block_template → submit_block) and
    /// block RPCs for a v3 header to re-hash identically — the pow_algo_id / EVM / overlay precedent.
    /// Zero on v0/v1/v2 headers (hash-invisible there).
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
    /// ADR-0039 C6: this block's own active beacon seed R_E (§11.2). Zero for pre-v3 headers.
    pub palw_beacon_seed: Hash64,
    /// PALW Header-v4 anti-spam accumulator commitment. Canonical only for v4+ headers.
    #[serde(default)]
    pub palw_spam_accumulator_commitment: Hash64,
    /// PALW Header-v4 objective stamp nonce. Canonical only for v4+ headers.
    #[serde(default)]
    pub palw_spam_nonce: u64,
    /// ADR-MA Header-v5 Compute Set registry references. Part of the v5+ hash preimage, so they
    /// MUST round-trip through the mining (get_block_template → submit_block) and block RPCs —
    /// the pow_algo_id / EVM / PALW precedent. Zero on pre-v5 headers (hash-invisible there).
    #[serde(default)]
    pub palw_compute_set_id: Hash64,
    #[serde(default)]
    pub palw_compute_policy_id: Hash64,
    #[serde(default)]
    pub palw_allocation_plan_id: Hash64,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcHeader {
    /// Cached hash
    pub hash: BlockHash,
    pub version: u16,
    pub parents_by_level: Vec<Vec<BlockHash>>,
    pub hash_merkle_root: kaspa_consensus_core::MerkleRoot,
    pub accepted_id_merkle_root: kaspa_consensus_core::AcceptedIdMerkleRoot,
    pub utxo_commitment: Hash64,
    /// Timestamp is in milliseconds
    pub timestamp: u64,
    pub bits: u32,
    pub nonce: u64,
    /// kaspa-pq Phase 2 PoW (ADR-0007): Layer-1 algorithm id (1 = kHeavyHash, 2 = Argon2id).
    pub pow_algo_id: u8,
    pub daa_score: u64,
    pub blue_work: BlueWorkType,
    pub blue_score: u64,
    pub pruning_point: BlockHash,
    /// kaspa-pq EVM Lane v0.4 (ADR-0020 §4): both EVM commitments are part of
    /// the v2+ header-hash preimage, so they MUST round-trip through the
    /// mining (get_block_template → submit_block) and block RPCs — the
    /// pow_algo_id precedent. Zero on v0/v1 headers (hash-invisible there).
    pub evm_payload_hash: Hash64,
    pub evm_commitment_root: Hash64,
    /// kaspa-pq ADR-0022: the DNS/PoS-v2 overlay-state commitment. Part of the
    /// header-hash preimage on every version, so it MUST round-trip through the
    /// mining (get_block_template → submit_block) and block RPCs — the
    /// pow_algo_id / EVM-commitment precedent.
    pub overlay_commitment_root: Hash64,
    /// kaspa-pq ADR-0039 PALW: the ten Header-v3 fields (see [`RpcRawHeader`]). Zero on pre-v3 headers.
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
    /// ADR-0039 C6: this block's own active beacon seed R_E (§11.2). Zero for pre-v3 headers.
    pub palw_beacon_seed: Hash64,
    /// PALW Header-v4 anti-spam accumulator commitment. Canonical only for v4+ headers.
    #[serde(default)]
    pub palw_spam_accumulator_commitment: Hash64,
    /// PALW Header-v4 objective stamp nonce. Canonical only for v4+ headers.
    #[serde(default)]
    pub palw_spam_nonce: u64,
    /// ADR-MA Header-v5 Compute Set registry references. Part of the v5+ hash preimage, so they
    /// MUST round-trip through the mining (get_block_template → submit_block) and block RPCs —
    /// the pow_algo_id / EVM / PALW precedent. Zero on pre-v5 headers (hash-invisible there).
    #[serde(default)]
    pub palw_compute_set_id: Hash64,
    #[serde(default)]
    pub palw_compute_policy_id: Hash64,
    #[serde(default)]
    pub palw_allocation_plan_id: Hash64,
}

impl RpcHeader {
    pub fn direct_parents(&self) -> &[BlockHash] {
        if self.parents_by_level.is_empty() { &[] } else { &self.parents_by_level[0] }
    }
}

impl AsRef<RpcHeader> for RpcHeader {
    fn as_ref(&self) -> &RpcHeader {
        self
    }
}

impl From<Header> for RpcHeader {
    fn from(header: Header) -> Self {
        Self {
            hash: header.hash,
            version: header.version,
            parents_by_level: header.parents_by_level.into(),
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            pow_algo_id: header.pow_algo_id,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
            evm_payload_hash: header.evm_payload_hash,
            evm_commitment_root: header.evm_commitment_root,
            overlay_commitment_root: header.overlay_commitment_root,
            blue_hash_work: header.blue_hash_work,
            blue_compute_work: header.blue_compute_work,
            palw_batch_id: header.palw_batch_id,
            palw_leaf_index: header.palw_leaf_index,
            palw_ticket_nullifier: header.palw_ticket_nullifier,
            palw_epoch_certificate_hash: header.palw_epoch_certificate_hash,
            palw_chain_commit: header.palw_chain_commit,
            palw_target_daa_interval: header.palw_target_daa_interval,
            palw_authorization_hash: header.palw_authorization_hash,
            palw_proof_type: header.palw_proof_type,
            palw_beacon_seed: header.palw_beacon_seed,
            palw_spam_accumulator_commitment: header.palw_spam_accumulator_commitment,
            palw_spam_nonce: header.palw_spam_nonce,
            palw_compute_set_id: header.palw_compute_set_id,
            palw_compute_policy_id: header.palw_compute_policy_id,
            palw_allocation_plan_id: header.palw_allocation_plan_id,
        }
    }
}

impl From<&Header> for RpcHeader {
    fn from(header: &Header) -> Self {
        Self {
            hash: header.hash,
            version: header.version,
            parents_by_level: (&header.parents_by_level).into(),
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            pow_algo_id: header.pow_algo_id,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
            evm_payload_hash: header.evm_payload_hash,
            evm_commitment_root: header.evm_commitment_root,
            overlay_commitment_root: header.overlay_commitment_root,
            blue_hash_work: header.blue_hash_work,
            blue_compute_work: header.blue_compute_work,
            palw_batch_id: header.palw_batch_id,
            palw_leaf_index: header.palw_leaf_index,
            palw_ticket_nullifier: header.palw_ticket_nullifier,
            palw_epoch_certificate_hash: header.palw_epoch_certificate_hash,
            palw_chain_commit: header.palw_chain_commit,
            palw_target_daa_interval: header.palw_target_daa_interval,
            palw_authorization_hash: header.palw_authorization_hash,
            palw_proof_type: header.palw_proof_type,
            palw_beacon_seed: header.palw_beacon_seed,
            palw_spam_accumulator_commitment: header.palw_spam_accumulator_commitment,
            palw_spam_nonce: header.palw_spam_nonce,
            palw_compute_set_id: header.palw_compute_set_id,
            palw_compute_policy_id: header.palw_compute_policy_id,
            palw_allocation_plan_id: header.palw_allocation_plan_id,
        }
    }
}

impl TryFrom<RpcHeader> for Header {
    type Error = RpcError;
    fn try_from(header: RpcHeader) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: header.hash,
            version: header.version,
            parents_by_level: header.parents_by_level.try_into()?,
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            // kaspa-pq Phase 2 (ADR-0007): carry the declared Layer-1 algo id through the RPC.
            pow_algo_id: header.pow_algo_id,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
            // kaspa-pq EVM Lane v0.4: carry both EVM commitments through the RPC
            // (part of the v2+ hash preimage — the pow_algo_id precedent).
            evm_payload_hash: header.evm_payload_hash,
            evm_commitment_root: header.evm_commitment_root,
            overlay_commitment_root: header.overlay_commitment_root,
            // kaspa-pq ADR-0039 PALW: carry the ten Header-v3 fields through the RPC (v3 hash preimage;
            // zero and hash-invisible on a pre-v3 header).
            blue_hash_work: header.blue_hash_work,
            blue_compute_work: header.blue_compute_work,
            palw_batch_id: header.palw_batch_id,
            palw_leaf_index: header.palw_leaf_index,
            palw_ticket_nullifier: header.palw_ticket_nullifier,
            palw_epoch_certificate_hash: header.palw_epoch_certificate_hash,
            palw_chain_commit: header.palw_chain_commit,
            palw_target_daa_interval: header.palw_target_daa_interval,
            palw_authorization_hash: header.palw_authorization_hash,
            palw_proof_type: header.palw_proof_type,
            palw_beacon_seed: header.palw_beacon_seed,
            palw_spam_accumulator_commitment: header.palw_spam_accumulator_commitment,
            palw_spam_nonce: header.palw_spam_nonce,
            palw_compute_set_id: header.palw_compute_set_id,
            palw_compute_policy_id: header.palw_compute_policy_id,
            palw_allocation_plan_id: header.palw_allocation_plan_id,
        })
    }
}

impl TryFrom<&RpcHeader> for Header {
    type Error = RpcError;

    fn try_from(header: &RpcHeader) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: header.hash,
            version: header.version,
            parents_by_level: header.parents_by_level.clone().try_into()?,
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            // kaspa-pq Phase 2 (ADR-0007): carry the declared Layer-1 algo id through the RPC.
            pow_algo_id: header.pow_algo_id,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
            // kaspa-pq EVM Lane v0.4: carry both EVM commitments through the RPC
            // (part of the v2+ hash preimage — the pow_algo_id precedent).
            evm_payload_hash: header.evm_payload_hash,
            evm_commitment_root: header.evm_commitment_root,
            overlay_commitment_root: header.overlay_commitment_root,
            // kaspa-pq ADR-0039 PALW: carry the ten Header-v3 fields through the RPC (v3 hash preimage;
            // zero and hash-invisible on a pre-v3 header).
            blue_hash_work: header.blue_hash_work,
            blue_compute_work: header.blue_compute_work,
            palw_batch_id: header.palw_batch_id,
            palw_leaf_index: header.palw_leaf_index,
            palw_ticket_nullifier: header.palw_ticket_nullifier,
            palw_epoch_certificate_hash: header.palw_epoch_certificate_hash,
            palw_chain_commit: header.palw_chain_commit,
            palw_target_daa_interval: header.palw_target_daa_interval,
            palw_authorization_hash: header.palw_authorization_hash,
            palw_proof_type: header.palw_proof_type,
            palw_beacon_seed: header.palw_beacon_seed,
            palw_spam_accumulator_commitment: header.palw_spam_accumulator_commitment,
            palw_spam_nonce: header.palw_spam_nonce,
            palw_compute_set_id: header.palw_compute_set_id,
            palw_compute_policy_id: header.palw_compute_policy_id,
            palw_allocation_plan_id: header.palw_allocation_plan_id,
        })
    }
}

impl Serializer for RpcHeader {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &8, writer)?;

        store!(BlockHash, &self.hash, writer)?;
        store!(u16, &self.version, writer)?;
        store!(Vec<Vec<BlockHash>>, &self.parents_by_level, writer)?;
        // PR-9.5c: serialised as Hash64 (64 raw bytes on the wire).
        store!(kaspa_hashes::Hash64, &self.hash_merkle_root, writer)?;
        store!(kaspa_hashes::Hash64, &self.accepted_id_merkle_root, writer)?;
        store!(Hash64, &self.utxo_commitment, writer)?;
        store!(u64, &self.timestamp, writer)?;
        store!(u32, &self.bits, writer)?;
        store!(u64, &self.nonce, writer)?;
        store!(u8, &self.pow_algo_id, writer)?;
        store!(u64, &self.daa_score, writer)?;
        store!(BlueWorkType, &self.blue_work, writer)?;
        store!(u64, &self.blue_score, writer)?;
        store!(BlockHash, &self.pruning_point, writer)?;
        // kaspa-pq EVM Lane v0.4 (serializer v3): the two EVM commitments.
        store!(Hash64, &self.evm_payload_hash, writer)?;
        store!(Hash64, &self.evm_commitment_root, writer)?;
        // kaspa-pq ADR-0022 (serializer v4): the overlay-state commitment.
        store!(Hash64, &self.overlay_commitment_root, writer)?;
        // kaspa-pq ADR-0039 PALW (serializer v5): the ten Header-v3 fields.
        store!(BlueWorkType, &self.blue_hash_work, writer)?;
        store!(BlueWorkType, &self.blue_compute_work, writer)?;
        store!(Hash64, &self.palw_batch_id, writer)?;
        store!(u32, &self.palw_leaf_index, writer)?;
        store!(Hash64, &self.palw_ticket_nullifier, writer)?;
        store!(Hash64, &self.palw_epoch_certificate_hash, writer)?;
        store!(Hash64, &self.palw_chain_commit, writer)?;
        store!(u64, &self.palw_target_daa_interval, writer)?;
        store!(Hash64, &self.palw_authorization_hash, writer)?;
        store!(u8, &self.palw_proof_type, writer)?;
        // kaspa-pq ADR-0039 C6 (serializer v6): this block's own beacon seed R_E.
        store!(Hash64, &self.palw_beacon_seed, writer)?;
        // PALW Header-v4 anti-spam fields (serializer v7). Frozen order matches
        // the canonical v4 header suffix: accumulator commitment, then nonce.
        store!(Hash64, &self.palw_spam_accumulator_commitment, writer)?;
        store!(u64, &self.palw_spam_nonce, writer)?;
        // ADR-MA Header-v5 registry references were added in serializer v8.
        store!(Hash64, &self.palw_compute_set_id, writer)?;
        store!(Hash64, &self.palw_compute_policy_id, writer)?;
        store!(Hash64, &self.palw_allocation_plan_id, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcHeader {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let serializer_version = load!(u16, reader)?;

        let hash = load!(BlockHash, reader)?;
        let version = load!(u16, reader)?;
        let parents_by_level = load!(Vec<Vec<BlockHash>>, reader)?;
        // PR-9.5c: deserialised as Hash64 (64 raw bytes on the wire).
        let hash_merkle_root = load!(kaspa_hashes::Hash64, reader)?;
        let accepted_id_merkle_root = load!(kaspa_hashes::Hash64, reader)?;
        let utxo_commitment = load!(Hash64, reader)?;
        let timestamp = load!(u64, reader)?;
        let bits = load!(u32, reader)?;
        let nonce = load!(u64, reader)?;
        // kaspa-pq Phase 2 (ADR-0007): pow_algo_id added in serializer v2; v1 → default kHeavyHash.
        // (Fixed: this previously tested the SHADOWED header version (0/1), never the
        // serializer version — any v2 stream with a written algo id would desync.)
        let pow_algo_id =
            if serializer_version >= 2 { load!(u8, reader)? } else { kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH };
        let daa_score = load!(u64, reader)?;
        let blue_work = load!(BlueWorkType, reader)?;
        let blue_score = load!(u64, reader)?;
        let pruning_point = load!(BlockHash, reader)?;
        // kaspa-pq EVM Lane v0.4: added in serializer v3; older peers ⇒ zero.
        let (evm_payload_hash, evm_commitment_root) = if serializer_version >= 3 {
            (load!(Hash64, reader)?, load!(Hash64, reader)?)
        } else {
            (Default::default(), Default::default())
        };
        // kaspa-pq ADR-0022: overlay commitment added in serializer v4; older ⇒ zero.
        let overlay_commitment_root = if serializer_version >= 4 { load!(Hash64, reader)? } else { Default::default() };
        // kaspa-pq ADR-0039 PALW: the ten Header-v3 fields added in serializer v5; older peers ⇒ zero.
        #[rustfmt::skip]
        let (
            blue_hash_work, blue_compute_work, palw_batch_id, palw_leaf_index, palw_ticket_nullifier,
            palw_epoch_certificate_hash, palw_chain_commit, palw_target_daa_interval, palw_authorization_hash,
            palw_proof_type,
        ) = if serializer_version >= 5 {
            (
                load!(BlueWorkType, reader)?, load!(BlueWorkType, reader)?, load!(Hash64, reader)?,
                load!(u32, reader)?, load!(Hash64, reader)?, load!(Hash64, reader)?, load!(Hash64, reader)?,
                load!(u64, reader)?, load!(Hash64, reader)?, load!(u8, reader)?,
            )
        } else {
            Default::default()
        };
        // kaspa-pq ADR-0039 C6: beacon seed added in serializer v6; older peers => zero.
        let palw_beacon_seed = if serializer_version >= 6 { load!(Hash64, reader)? } else { Default::default() };
        // PALW Header-v4 anti-spam fields were added in serializer v7. Streams
        // from older peers decode them to their pre-v4 inert zero values.
        let (palw_spam_accumulator_commitment, palw_spam_nonce) =
            if serializer_version >= 7 { (load!(Hash64, reader)?, load!(u64, reader)?) } else { Default::default() };
        // ADR-MA Header-v5 registry references were added in serializer v8. Older streams
        // decode them to their pre-v5 inert zero values.
        let (palw_compute_set_id, palw_compute_policy_id, palw_allocation_plan_id) = if serializer_version >= 8 {
            (load!(Hash64, reader)?, load!(Hash64, reader)?, load!(Hash64, reader)?)
        } else {
            Default::default()
        };

        Ok(Self {
            hash,
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            utxo_commitment,
            timestamp,
            bits,
            nonce,
            pow_algo_id,
            daa_score,
            blue_work,
            blue_score,
            pruning_point,
            evm_payload_hash,
            evm_commitment_root,
            overlay_commitment_root,
            blue_hash_work,
            blue_compute_work,
            palw_batch_id,
            palw_leaf_index,
            palw_ticket_nullifier,
            palw_epoch_certificate_hash,
            palw_chain_commit,
            palw_target_daa_interval,
            palw_authorization_hash,
            palw_proof_type,
            palw_beacon_seed,
            palw_spam_accumulator_commitment,
            palw_spam_nonce,
            palw_compute_set_id,
            palw_compute_policy_id,
            palw_allocation_plan_id,
        })
    }
}

impl TryFrom<RpcRawHeader> for Header {
    type Error = RpcError;

    fn try_from(header: RpcRawHeader) -> Result<Self, Self::Error> {
        Ok(Self::new_finalized(
            header.version,
            header.parents_by_level.try_into()?,
            header.hash_merkle_root,
            header.accepted_id_merkle_root,
            header.utxo_commitment,
            header.timestamp,
            header.bits,
            header.nonce,
            // kaspa-pq Phase 2 (ADR-0007): carry the declared Layer-1 algo id through the RPC.
            header.pow_algo_id,
            header.daa_score,
            header.blue_work,
            header.blue_score,
            header.pruning_point,
        )
        // kaspa-pq EVM Lane v0.4: restore both EVM commitments (v2+ preimage).
        .with_evm_payload_hash(header.evm_payload_hash)
        .with_evm_commitment(header.evm_commitment_root)
        // kaspa-pq ADR-0022: restore the overlay-state commitment (re-genesis preimage).
        .with_overlay_commitment(header.overlay_commitment_root)
        // kaspa-pq ADR-0039 PALW: restore the ten Header-v3 fields (v3 hash preimage; zero and
        // hash-invisible on a pre-v3 header, so a wrong value fails the hash check, never a silent fork).
        .with_palw_fields(PalwHeaderFields {
            blue_hash_work: header.blue_hash_work,
            blue_compute_work: header.blue_compute_work,
            palw_batch_id: header.palw_batch_id,
            palw_leaf_index: header.palw_leaf_index,
            palw_ticket_nullifier: header.palw_ticket_nullifier,
            palw_epoch_certificate_hash: header.palw_epoch_certificate_hash,
            palw_chain_commit: header.palw_chain_commit,
            palw_target_daa_interval: header.palw_target_daa_interval,
            palw_authorization_hash: header.palw_authorization_hash,
            palw_proof_type: header.palw_proof_type,
            palw_beacon_seed: header.palw_beacon_seed,
            palw_spam_accumulator_commitment: header.palw_spam_accumulator_commitment,
            palw_spam_nonce: header.palw_spam_nonce,
            palw_compute_set_id: header.palw_compute_set_id,
            palw_compute_policy_id: header.palw_compute_policy_id,
            palw_allocation_plan_id: header.palw_allocation_plan_id,
        }))
    }
}

impl TryFrom<&RpcRawHeader> for Header {
    type Error = RpcError;

    fn try_from(header: &RpcRawHeader) -> Result<Self, Self::Error> {
        Ok(Self::new_finalized(
            header.version,
            header.parents_by_level.clone().try_into()?,
            header.hash_merkle_root,
            header.accepted_id_merkle_root,
            header.utxo_commitment,
            header.timestamp,
            header.bits,
            header.nonce,
            // kaspa-pq Phase 2 (ADR-0007): carry the declared Layer-1 algo id through the RPC.
            header.pow_algo_id,
            header.daa_score,
            header.blue_work,
            header.blue_score,
            header.pruning_point,
        )
        // kaspa-pq EVM Lane v0.4: restore both EVM commitments (v2+ preimage).
        .with_evm_payload_hash(header.evm_payload_hash)
        .with_evm_commitment(header.evm_commitment_root)
        // kaspa-pq ADR-0022: restore the overlay-state commitment (re-genesis preimage).
        .with_overlay_commitment(header.overlay_commitment_root)
        // kaspa-pq ADR-0039 PALW: restore the ten Header-v3 fields (v3 hash preimage; zero and
        // hash-invisible on a pre-v3 header, so a wrong value fails the hash check, never a silent fork).
        .with_palw_fields(PalwHeaderFields {
            blue_hash_work: header.blue_hash_work,
            blue_compute_work: header.blue_compute_work,
            palw_batch_id: header.palw_batch_id,
            palw_leaf_index: header.palw_leaf_index,
            palw_ticket_nullifier: header.palw_ticket_nullifier,
            palw_epoch_certificate_hash: header.palw_epoch_certificate_hash,
            palw_chain_commit: header.palw_chain_commit,
            palw_target_daa_interval: header.palw_target_daa_interval,
            palw_authorization_hash: header.palw_authorization_hash,
            palw_proof_type: header.palw_proof_type,
            palw_beacon_seed: header.palw_beacon_seed,
            palw_spam_accumulator_commitment: header.palw_spam_accumulator_commitment,
            palw_spam_nonce: header.palw_spam_nonce,
            palw_compute_set_id: header.palw_compute_set_id,
            palw_compute_policy_id: header.palw_compute_policy_id,
            palw_allocation_plan_id: header.palw_allocation_plan_id,
        }))
    }
}

impl From<&Header> for RpcRawHeader {
    fn from(header: &Header) -> Self {
        Self {
            version: header.version,
            parents_by_level: header.parents_by_level.clone().into(),
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            pow_algo_id: header.pow_algo_id,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
            evm_payload_hash: header.evm_payload_hash,
            evm_commitment_root: header.evm_commitment_root,
            overlay_commitment_root: header.overlay_commitment_root,
            blue_hash_work: header.blue_hash_work,
            blue_compute_work: header.blue_compute_work,
            palw_batch_id: header.palw_batch_id,
            palw_leaf_index: header.palw_leaf_index,
            palw_ticket_nullifier: header.palw_ticket_nullifier,
            palw_epoch_certificate_hash: header.palw_epoch_certificate_hash,
            palw_chain_commit: header.palw_chain_commit,
            palw_target_daa_interval: header.palw_target_daa_interval,
            palw_authorization_hash: header.palw_authorization_hash,
            palw_proof_type: header.palw_proof_type,
            palw_beacon_seed: header.palw_beacon_seed,
            palw_spam_accumulator_commitment: header.palw_spam_accumulator_commitment,
            palw_spam_nonce: header.palw_spam_nonce,
            palw_compute_set_id: header.palw_compute_set_id,
            palw_compute_policy_id: header.palw_compute_policy_id,
            palw_allocation_plan_id: header.palw_allocation_plan_id,
        }
    }
}

impl From<Header> for RpcRawHeader {
    fn from(header: Header) -> Self {
        Self {
            version: header.version,
            parents_by_level: header.parents_by_level.into(),
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            pow_algo_id: header.pow_algo_id,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
            evm_payload_hash: header.evm_payload_hash,
            evm_commitment_root: header.evm_commitment_root,
            overlay_commitment_root: header.overlay_commitment_root,
            blue_hash_work: header.blue_hash_work,
            blue_compute_work: header.blue_compute_work,
            palw_batch_id: header.palw_batch_id,
            palw_leaf_index: header.palw_leaf_index,
            palw_ticket_nullifier: header.palw_ticket_nullifier,
            palw_epoch_certificate_hash: header.palw_epoch_certificate_hash,
            palw_chain_commit: header.palw_chain_commit,
            palw_target_daa_interval: header.palw_target_daa_interval,
            palw_authorization_hash: header.palw_authorization_hash,
            palw_proof_type: header.palw_proof_type,
            palw_beacon_seed: header.palw_beacon_seed,
            palw_spam_accumulator_commitment: header.palw_spam_accumulator_commitment,
            palw_spam_nonce: header.palw_spam_nonce,
            palw_compute_set_id: header.palw_compute_set_id,
            palw_compute_policy_id: header.palw_compute_policy_id,
            palw_allocation_plan_id: header.palw_allocation_plan_id,
        }
    }
}

impl Serializer for RpcRawHeader {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &8, writer)?;

        store!(u16, &self.version, writer)?;
        store!(Vec<Vec<BlockHash>>, &self.parents_by_level, writer)?;
        // PR-9.5c: serialised as Hash64.
        store!(kaspa_hashes::Hash64, &self.hash_merkle_root, writer)?;
        store!(kaspa_hashes::Hash64, &self.accepted_id_merkle_root, writer)?;
        store!(Hash64, &self.utxo_commitment, writer)?;
        store!(u64, &self.timestamp, writer)?;
        store!(u32, &self.bits, writer)?;
        store!(u64, &self.nonce, writer)?;
        store!(u8, &self.pow_algo_id, writer)?;
        store!(u64, &self.daa_score, writer)?;
        store!(BlueWorkType, &self.blue_work, writer)?;
        store!(u64, &self.blue_score, writer)?;
        store!(BlockHash, &self.pruning_point, writer)?;
        // kaspa-pq EVM Lane v0.4 (serializer v3): the two EVM commitments.
        store!(Hash64, &self.evm_payload_hash, writer)?;
        store!(Hash64, &self.evm_commitment_root, writer)?;
        // kaspa-pq ADR-0022 (serializer v4): the overlay-state commitment.
        store!(Hash64, &self.overlay_commitment_root, writer)?;
        // kaspa-pq ADR-0039 PALW (serializer v5): the ten Header-v3 fields.
        store!(BlueWorkType, &self.blue_hash_work, writer)?;
        store!(BlueWorkType, &self.blue_compute_work, writer)?;
        store!(Hash64, &self.palw_batch_id, writer)?;
        store!(u32, &self.palw_leaf_index, writer)?;
        store!(Hash64, &self.palw_ticket_nullifier, writer)?;
        store!(Hash64, &self.palw_epoch_certificate_hash, writer)?;
        store!(Hash64, &self.palw_chain_commit, writer)?;
        store!(u64, &self.palw_target_daa_interval, writer)?;
        store!(Hash64, &self.palw_authorization_hash, writer)?;
        store!(u8, &self.palw_proof_type, writer)?;
        // kaspa-pq ADR-0039 C6 (serializer v6): this block's own beacon seed R_E.
        store!(Hash64, &self.palw_beacon_seed, writer)?;
        // PALW Header-v4 anti-spam fields (serializer v7).
        store!(Hash64, &self.palw_spam_accumulator_commitment, writer)?;
        store!(u64, &self.palw_spam_nonce, writer)?;
        // ADR-MA Header-v5 registry references were added in serializer v8.
        store!(Hash64, &self.palw_compute_set_id, writer)?;
        store!(Hash64, &self.palw_compute_policy_id, writer)?;
        store!(Hash64, &self.palw_allocation_plan_id, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcRawHeader {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let serializer_version = load!(u16, reader)?;

        let version = load!(u16, reader)?;
        let parents_by_level = load!(Vec<Vec<BlockHash>>, reader)?;
        // PR-9.5c: deserialised as Hash64.
        let hash_merkle_root = load!(kaspa_hashes::Hash64, reader)?;
        let accepted_id_merkle_root = load!(kaspa_hashes::Hash64, reader)?;
        let utxo_commitment = load!(Hash64, reader)?;
        let timestamp = load!(u64, reader)?;
        let bits = load!(u32, reader)?;
        let nonce = load!(u64, reader)?;
        // kaspa-pq Phase 2 (ADR-0007): pow_algo_id added in serializer v2; v1 → default kHeavyHash.
        // (Fixed: this previously tested the SHADOWED header version (0/1), never the
        // serializer version — any v2 stream with a written algo id would desync.)
        let pow_algo_id =
            if serializer_version >= 2 { load!(u8, reader)? } else { kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH };
        let daa_score = load!(u64, reader)?;
        let blue_work = load!(BlueWorkType, reader)?;
        let blue_score = load!(u64, reader)?;
        let pruning_point = load!(BlockHash, reader)?;
        // kaspa-pq EVM Lane v0.4: added in serializer v3; older peers ⇒ zero.
        let (evm_payload_hash, evm_commitment_root) = if serializer_version >= 3 {
            (load!(Hash64, reader)?, load!(Hash64, reader)?)
        } else {
            (Default::default(), Default::default())
        };
        // kaspa-pq ADR-0022: overlay commitment added in serializer v4; older ⇒ zero.
        let overlay_commitment_root = if serializer_version >= 4 { load!(Hash64, reader)? } else { Default::default() };
        // kaspa-pq ADR-0039 PALW: the ten Header-v3 fields added in serializer v5; older peers ⇒ zero.
        #[rustfmt::skip]
        let (
            blue_hash_work, blue_compute_work, palw_batch_id, palw_leaf_index, palw_ticket_nullifier,
            palw_epoch_certificate_hash, palw_chain_commit, palw_target_daa_interval, palw_authorization_hash,
            palw_proof_type,
        ) = if serializer_version >= 5 {
            (
                load!(BlueWorkType, reader)?, load!(BlueWorkType, reader)?, load!(Hash64, reader)?,
                load!(u32, reader)?, load!(Hash64, reader)?, load!(Hash64, reader)?, load!(Hash64, reader)?,
                load!(u64, reader)?, load!(Hash64, reader)?, load!(u8, reader)?,
            )
        } else {
            Default::default()
        };
        // kaspa-pq ADR-0039 C6: beacon seed added in serializer v6; older peers => zero.
        let palw_beacon_seed = if serializer_version >= 6 { load!(Hash64, reader)? } else { Default::default() };
        // PALW Header-v4 anti-spam fields were added in serializer v7. Older
        // streams retain the pre-v4 inert zero values.
        let (palw_spam_accumulator_commitment, palw_spam_nonce) =
            if serializer_version >= 7 { (load!(Hash64, reader)?, load!(u64, reader)?) } else { Default::default() };
        // ADR-MA Header-v5 registry references were added in serializer v8. Older streams
        // decode them to their pre-v5 inert zero values.
        let (palw_compute_set_id, palw_compute_policy_id, palw_allocation_plan_id) = if serializer_version >= 8 {
            (load!(Hash64, reader)?, load!(Hash64, reader)?, load!(Hash64, reader)?)
        } else {
            Default::default()
        };

        Ok(Self {
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            utxo_commitment,
            timestamp,
            bits,
            nonce,
            pow_algo_id,
            daa_score,
            blue_work,
            blue_score,
            pruning_point,
            evm_payload_hash,
            evm_commitment_root,
            overlay_commitment_root,
            blue_hash_work,
            blue_compute_work,
            palw_batch_id,
            palw_leaf_index,
            palw_ticket_nullifier,
            palw_epoch_certificate_hash,
            palw_chain_commit,
            palw_target_daa_interval,
            palw_authorization_hash,
            palw_proof_type,
            palw_beacon_seed,
            palw_spam_accumulator_commitment,
            palw_spam_nonce,
            palw_compute_set_id,
            palw_compute_policy_id,
            palw_allocation_plan_id,
        })
    }
}

#[cfg(test)]
mod palw_rpc_tests {
    use super::*;
    use kaspa_hashes::Hash64;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn v3_header() -> Header {
        Header::new_finalized(
            3,
            vec![vec![h(1)]].try_into().unwrap(),
            h(2),
            h(3),
            h(4),
            100,
            0x1d00ffff,
            5,
            4,
            60,
            BlueWorkType::from_u64(9999),
            70,
            h(5),
        )
        .with_palw_fields(PalwHeaderFields {
            blue_hash_work: BlueWorkType::from_u64(500),
            blue_compute_work: BlueWorkType::from_u64(120),
            palw_batch_id: h(10),
            palw_leaf_index: 7,
            palw_ticket_nullifier: h(11),
            palw_epoch_certificate_hash: h(12),
            palw_chain_commit: h(13),
            palw_target_daa_interval: 2400,
            palw_authorization_hash: h(14),
            palw_proof_type: 1,
            palw_beacon_seed: h(15),
            palw_spam_accumulator_commitment: Default::default(),
            palw_spam_nonce: 0,
            palw_compute_set_id: Default::default(),
            palw_compute_policy_id: Default::default(),
            palw_allocation_plan_id: Default::default(),
        })
    }

    fn v4_header() -> Header {
        let mut header = v3_header();
        header.version = 4;
        header.palw_spam_accumulator_commitment = h(16);
        header.palw_spam_nonce = 0x0123_4567_89ab_cdef;
        header.finalize();
        header
    }

    fn downgrade_v7_stream_to_v6(mut bytes: Vec<u8>) -> Vec<u8> {
        let mut encoded_v6 = Vec::new();
        store!(u16, &6, &mut encoded_v6).unwrap();

        let mut v7_suffix = Vec::new();
        store!(Hash64, &Hash64::default(), &mut v7_suffix).unwrap();
        store!(u64, &0, &mut v7_suffix).unwrap();

        bytes[..encoded_v6.len()].copy_from_slice(&encoded_v6);
        bytes.truncate(bytes.len() - v7_suffix.len());
        bytes
    }

    /// The mining path (`get_block_template` → `RpcRawHeader` → `submit_block`) RE-DERIVES the hash from
    /// fields, so a preserved v3 hash proves the ten PALW fields round-trip through the RPC DTO.
    #[test]
    fn rpc_raw_header_roundtrip_rederives_v3_hash() {
        let header = v3_header();
        let raw: RpcRawHeader = (&header).into();
        let back: Header = (&raw).try_into().unwrap();
        assert_eq!(header.hash, back.hash, "v3 hash re-derived from RpcRawHeader");
        assert_eq!(back.palw_ticket_nullifier, h(11));
        assert_eq!(back.blue_hash_work, BlueWorkType::from_u64(500));
    }

    #[test]
    fn rpc_header_carries_palw_fields() {
        let header = v3_header();
        let rpc: RpcHeader = (&header).into();
        assert_eq!(rpc.blue_compute_work, BlueWorkType::from_u64(120));
        assert_eq!(rpc.palw_batch_id, h(10));
        let back: Header = (&rpc).try_into().unwrap();
        assert_eq!(back.palw_proof_type, 1);
    }

    /// The current wRPC serializer retains the Header-v3 fields while adding
    /// the Header-v4 anti-spam suffix at serializer version 7.
    #[test]
    fn rpc_header_serializer_v7_roundtrips_palw_v4() {
        let rpc: RpcHeader = (&v4_header()).into();
        let mut buf = Vec::new();
        Serializer::serialize(&rpc, &mut buf).unwrap();
        let back = <RpcHeader as Deserializer>::deserialize(&mut &buf[..]).unwrap();
        assert_eq!(back.palw_batch_id, h(10));
        assert_eq!(back.palw_target_daa_interval, 2400);
        assert_eq!(back.palw_proof_type, 1);
        assert_eq!(back.blue_hash_work, BlueWorkType::from_u64(500));
        assert_eq!(back.palw_spam_accumulator_commitment, h(16));
        assert_eq!(back.palw_spam_nonce, 0x0123_4567_89ab_cdef);
    }

    /// Mining RPC raw headers use the same v7 suffix, and conversion back into
    /// consensus re-derives the exact Header-v4 block identity.
    #[test]
    fn rpc_raw_header_serializer_v7_roundtrips_antispam_and_hash() {
        let header = v4_header();
        let raw: RpcRawHeader = (&header).into();
        let mut buf = Vec::new();
        Serializer::serialize(&raw, &mut buf).unwrap();
        let back = <RpcRawHeader as Deserializer>::deserialize(&mut &buf[..]).unwrap();
        assert_eq!(back.palw_spam_accumulator_commitment, h(16));
        assert_eq!(back.palw_spam_nonce, 0x0123_4567_89ab_cdef);

        let consensus: Header = (&back).try_into().unwrap();
        assert_eq!(consensus.hash, header.hash, "v4 hash re-derived after raw wRPC round-trip");
    }

    #[test]
    fn serializer_v6_streams_default_v4_antispam_fields_to_zero() {
        let rpc: RpcHeader = (&v4_header()).into();
        let mut rpc_bytes = Vec::new();
        Serializer::serialize(&rpc, &mut rpc_bytes).unwrap();
        let rpc_bytes = downgrade_v7_stream_to_v6(rpc_bytes);
        let rpc_back = <RpcHeader as Deserializer>::deserialize(&mut &rpc_bytes[..]).unwrap();
        assert_eq!(rpc_back.palw_spam_accumulator_commitment, Hash64::default());
        assert_eq!(rpc_back.palw_spam_nonce, 0);

        let raw: RpcRawHeader = (&v4_header()).into();
        let mut raw_bytes = Vec::new();
        Serializer::serialize(&raw, &mut raw_bytes).unwrap();
        let raw_bytes = downgrade_v7_stream_to_v6(raw_bytes);
        let raw_back = <RpcRawHeader as Deserializer>::deserialize(&mut &raw_bytes[..]).unwrap();
        assert_eq!(raw_back.palw_spam_accumulator_commitment, Hash64::default());
        assert_eq!(raw_back.palw_spam_nonce, 0);
    }
}
