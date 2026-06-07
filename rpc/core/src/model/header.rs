use crate::RpcError;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    BlockHash, // PR-9.5e: block ids (hash, parents, pruning point) are Hash64
    BlueWorkType,
    header::{CompressedParents, Header},
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
        })
    }
}

impl Serializer for RpcHeader {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;

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

        Ok(())
    }
}

impl Deserializer for RpcHeader {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;

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
        let pow_algo_id =
            if version >= 2 { load!(u8, reader)? } else { kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH };
        let daa_score = load!(u64, reader)?;
        let blue_work = load!(BlueWorkType, reader)?;
        let blue_score = load!(u64, reader)?;
        let pruning_point = load!(BlockHash, reader)?;

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
        ))
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
        ))
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
        }
    }
}

impl Serializer for RpcRawHeader {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;

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

        Ok(())
    }
}

impl Deserializer for RpcRawHeader {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;

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
        let pow_algo_id =
            if version >= 2 { load!(u8, reader)? } else { kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH };
        let daa_score = load!(u64, reader)?;
        let blue_work = load!(BlueWorkType, reader)?;
        let blue_score = load!(u64, reader)?;
        let pruning_point = load!(BlockHash, reader)?;

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
        })
    }
}
