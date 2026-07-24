//! PALW DA-01: canonical receipt availability objects and objective challenge state.
//!
//! `PalwReceiptDaObjectV1` and the public Header-v4 `PalwReceiptDaObjectV2` are the byte
//! representations committed by `PalwPublicLeafV1::receipt_da_root`. Their fixed 16-KiB
//! chunk tree binds object version,
//! total byte length, chunk count, chunk index, exact chunk length, and every byte under
//! disjoint keyed-hash domains. Both providers independently owe the beacon-selected
//! chunks even though the leaf carries one shared object root.
//!
//! The embedded legacy `ReplicaExecutionReceiptV1::receipt_da_root` fields are required
//! to be ZERO for object-v1. Requiring them to equal the outer root would be an impossible
//! self-reference: the root commits to the receipt signatures, while those signatures
//! cover the embedded field. ZERO is therefore a versioned canonical sentinel, not a
//! wildcard. The outer leaf root commits to both full signed receipts and both owner to
//! session authorizations.

use super::{
    PalwProviderBondMutation, PalwProviderBondRecord, PalwPublicLeafV1, ReplicaExecutionReceiptV1, ReplicaMatchRecordV1,
    effective_provider_bond_status, private_match_commitment,
};
use crate::dns_finality::{STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN, validator_id_from_pubkey};
use crate::tx::TransactionOutpoint;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{HASH64_SIZE, Hash64, blake2b_512_keyed};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use thiserror::Error;

pub const PALW_RECEIPT_DA_OBJECT_VERSION_V1: u16 = 1;
/// Public Header-v4/re-genesis object: two authenticated node-owned Receipt-v3 submissions.
pub const PALW_RECEIPT_DA_OBJECT_VERSION_V2: u16 = 2;
/// Node-anchored web-search snapshot (`SearchSnapshotV1`, ADR node-anchored-web-search-da).
/// Shares the DA-01 chunk tree; the version participates in every leaf/root preimage, so
/// snapshot roots and receipt roots live in disjoint hash domains. Receipt admission and the
/// receipt store dispatch on explicit V1/V2 arms and can never accept this class.
pub const PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1: u16 = 3;
pub const PALW_RECEIPT_DA_PROOF_VERSION_V1: u16 = 1;
pub const PALW_PROVIDER_SESSION_AUTH_VERSION_V1: u16 = 1;
pub const PALW_DA_CHALLENGE_VERSION_V1: u16 = 1;
pub const PALW_DA_RESPONSE_VERSION_V1: u16 = 1;
pub const PALW_DA_TIMEOUT_EVIDENCE_VERSION_V1: u16 = 1;
pub const PALW_DA_STATE_VERSION_V1: u16 = 1;
pub const PALW_DA_SNAPSHOT_VERSION_V1: u16 = 1;

/// Fixed chunking is consensus-facing: implementations must not choose their own size.
pub const PALW_DA_CHUNK_BYTES: usize = 16 * 1024;
pub const PALW_DA_MAX_OBJECT_BYTES: usize = 256 * 1024;
pub const PALW_DA_MAX_CHUNKS: usize = PALW_DA_MAX_OBJECT_BYTES / PALW_DA_CHUNK_BYTES;
pub const PALW_DA_MAX_PROOF_DEPTH: usize = PALW_DA_MAX_CHUNKS.ilog2() as usize;
/// Consensus-state bounds, intentionally independent of the larger P2P envelope limit. The DA state
/// is cloned by every child, so transport framing alone is not a resource bound.
pub const PALW_DA_MAX_OBLIGATIONS: usize = 131_072;
pub const PALW_DA_MAX_CHALLENGES: usize = 65_536;
pub const PALW_DA_MAX_CHALLENGE_COUNTERS: usize = 65_536;
pub const PALW_DA_MAX_TIMEOUT_EVIDENCE: usize = 65_536;
pub const PALW_DA_MAX_PRUNING_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
pub const PALW_DA_MAX_ONCHAIN_CHALLENGE_BYTES: usize = 16 * 1024;
pub const PALW_DA_MAX_ONCHAIN_RESPONSE_BYTES: usize = 32 * 1024;
pub const PALW_DA_MAX_ONCHAIN_TIMEOUT_BYTES: usize = 1024;
pub const PALW_DA_MAX_SESSION_EPOCHS: u64 = 64;
pub const PALW_DA_MAX_SAMPLES_PER_PROVIDER: u16 = 4;
/// Operator-side availability orchestration is deliberately much smaller than the consensus-state
/// bounds. A service snapshot is copied across the async/blocking boundary, so it must never mirror
/// an attacker-sized obligation table.
pub const PALW_DA_SERVICE_MAX_OBJECTS: usize = 64;
pub const PALW_DA_SERVICE_MAX_BYTES: usize = 8 * 1024 * 1024;
pub const PALW_DA_SERVICE_MAX_FETCH_TARGETS: usize = 64;
/// Bound one auxiliary-object GC transaction and, more importantly, the time for which it can hold
/// the virtual-state reorg fence. Additional stale objects are removed by subsequent sweeps.
pub const PALW_DA_GC_MAX_DELETIONS_PER_CYCLE: usize = 4_096;

/// Counts canonical Borsh bytes without allocating a second state-sized buffer. DA state is cloned
/// on each transition already; snapshot admission must add only an O(state) walk, not another
/// potentially 64-MiB allocation.
#[derive(Default)]
struct PalwDaEncodedLenCounter {
    len: usize,
}

impl std::io::Write for PalwDaEncodedLenCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.len = self.len.checked_add(bytes.len()).ok_or_else(|| std::io::Error::other("PALW DA canonical length overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub const PALW_DA_CHUNK_LEAF_DOMAIN: &[u8] = b"misaka-palw-da-chunk-leaf-v1";
pub const PALW_DA_CHUNK_EMPTY_DOMAIN: &[u8] = b"misaka-palw-da-chunk-empty-v1";
pub const PALW_DA_CHUNK_NODE_DOMAIN: &[u8] = b"misaka-palw-da-chunk-node-v1";
pub const PALW_DA_OBJECT_ROOT_DOMAIN: &[u8] = b"misaka-palw-da-object-root-v1";
pub const PALW_PROVIDER_SESSION_AUTH_DOMAIN: &[u8] = b"misaka-palw-provider-session-v1";
pub const PALW_DA_SAMPLE_DOMAIN: &[u8] = b"misaka-palw-da-provider-sample-v1";
pub const PALW_DA_OBLIGATION_ID_DOMAIN: &[u8] = b"misaka-palw-da-obligation-id-v1";
pub const PALW_DA_CHALLENGE_SIGNING_DOMAIN: &[u8] = b"misaka-palw-da-challenge-sign-v1";
pub const PALW_DA_CHALLENGE_ID_DOMAIN: &[u8] = b"misaka-palw-da-challenge-id-v1";
pub const PALW_DA_RESPONSE_SIGNING_DOMAIN: &[u8] = b"misaka-palw-da-response-sign-v1";
pub const PALW_DA_RESPONSE_ID_DOMAIN: &[u8] = b"misaka-palw-da-response-id-v1";
pub const PALW_DA_TIMEOUT_ID_DOMAIN: &[u8] = b"misaka-palw-da-timeout-id-v1";
pub const PALW_DA_STATE_ROOT_DOMAIN: &[u8] = b"misaka-palw-da-state-root-v1";
pub const PALW_DA_SNAPSHOT_DOMAIN: &[u8] = b"misaka-palw-da-snapshot-v1";

pub const PALW_REPLICA_RECEIPT_V1_MLDSA87_CONTEXT: &[u8] = b"PALWReplicaReceiptV1";
pub const PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT: &[u8] = b"PALWProviderSessionV1";
pub const PALW_DA_CHALLENGE_V1_MLDSA87_CONTEXT: &[u8] = b"PALWDAChallengeV1";
pub const PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT: &[u8] = b"PALWDAResponseV1";

#[inline]
fn push_hash(out: &mut Vec<u8>, hash: &Hash64) {
    out.extend_from_slice(hash.as_byte_slice());
}

#[inline]
fn push_outpoint(out: &mut Vec<u8>, outpoint: &TransactionOutpoint) {
    push_hash(out, &outpoint.transaction_id);
    out.extend_from_slice(&outpoint.index.to_le_bytes());
}

#[inline]
fn push_var(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwProviderSessionAuthorizationV1 {
    pub version: u16,
    pub network_id: u32,
    pub provider_bond: TransactionOutpoint,
    pub owner_public_key: Vec<u8>,
    pub session_public_key: Vec<u8>,
    pub valid_from_epoch: u64,
    pub valid_until_epoch: u64,
    pub authorization_nonce: Hash64,
    pub signature: Vec<u8>,
}

impl PalwProviderSessionAuthorizationV1 {
    pub fn signing_hash(&self) -> Hash64 {
        let mut preimage = Vec::with_capacity(192 + self.owner_public_key.len() + self.session_public_key.len());
        preimage.extend_from_slice(&self.version.to_le_bytes());
        preimage.extend_from_slice(&self.network_id.to_le_bytes());
        push_outpoint(&mut preimage, &self.provider_bond);
        push_var(&mut preimage, &self.owner_public_key);
        push_var(&mut preimage, &self.session_public_key);
        preimage.extend_from_slice(&self.valid_from_epoch.to_le_bytes());
        preimage.extend_from_slice(&self.valid_until_epoch.to_le_bytes());
        push_hash(&mut preimage, &self.authorization_nonce);
        blake2b_512_keyed(PALW_PROVIDER_SESSION_AUTH_DOMAIN, &preimage)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwReceiptDaObjectV1 {
    pub version: u16,
    pub network_id: u32,
    pub batch_id: Hash64,
    pub leaf_index: u32,
    pub receipt_a: ReplicaExecutionReceiptV1,
    pub receipt_b: ReplicaExecutionReceiptV1,
    pub match_record: ReplicaMatchRecordV1,
    pub session_authorization_a: PalwProviderSessionAuthorizationV1,
    pub session_authorization_b: PalwProviderSessionAuthorizationV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwReceiptDaCommitmentV1 {
    pub object_version: u16,
    pub object_len: u32,
    pub chunk_count: u16,
    pub root: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwReceiptDaChunkProofV1 {
    pub version: u16,
    pub object_version: u16,
    pub object_len: u32,
    pub chunk_count: u16,
    pub chunk_index: u16,
    pub chunk: Vec<u8>,
    pub siblings: Vec<Hash64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PalwDaError {
    #[error("unsupported DA object/proof version {0}")]
    UnsupportedVersion(u16),
    #[error("DA object length {got} is outside 1..={max}")]
    ObjectSize { got: usize, max: usize },
    #[error("DA object is not canonical borsh")]
    NonCanonicalObject,
    #[error("DA chunk metadata is inconsistent")]
    ChunkMetadata,
    #[error("DA chunk index is out of range")]
    ChunkIndex,
    #[error("DA chunk length is not the fixed/terminal length")]
    ChunkLength,
    #[error("DA Merkle proof has the wrong depth")]
    ProofDepth,
    #[error("DA Merkle proof does not match the committed root")]
    WrongProof,
    #[error("DA object is bound to another network/batch/leaf")]
    ObjectBinding,
    #[error("embedded receipt_da_root must be the object-v1 ZERO sentinel")]
    EmbeddedRootNotZero,
    #[error("provider bond is missing, mismatched, or inactive")]
    ProviderBond,
    #[error("provider owner key does not match the bond")]
    ProviderOwner,
    #[error("owner-to-session authorization is malformed or out of range")]
    SessionAuthorization,
    #[error("owner-to-session authorization signature is invalid")]
    SessionAuthorizationSignature,
    #[error("replica receipt is malformed or does not match the leaf")]
    Receipt,
    #[error("replica receipt signature is invalid")]
    ReceiptSignature,
    #[error("replica receipts disagree on job/model/runtime/output/trace/schedule")]
    ReplicaMismatch,
    #[error("private match record/hash/commitment does not match the receipts and leaf")]
    MatchCommitment,
    #[error("buried beacon is not old enough")]
    BeaconNotBuried,
    #[error("DA policy is invalid")]
    Policy,
    #[error("DA obligation is missing or in the wrong state")]
    ObligationState,
    #[error("DA challenge is duplicate, late, self-targeted, or not sampled")]
    Challenge,
    #[error("DA challenger is not an active authorized bond owner")]
    ChallengerBond,
    #[error("DA challenge rate limit exceeded")]
    ChallengeRateLimit,
    #[error("DA challenge signature is invalid")]
    ChallengeSignature,
    #[error("DA response is late, mismatched, duplicate, or unauthorized")]
    Response,
    #[error("DA response signature is invalid")]
    ResponseSignature,
    #[error("DA timeout evidence is premature, duplicate, or mismatched")]
    TimeoutEvidence,
    #[error("DA fork-local state capacity is exhausted")]
    Capacity,
}

/// Fail-closed node admission failures for a complete receipt DA object. Unlike chunk transport,
/// admission resolves the committed leaf and both provider bonds at one selected-chain snapshot and
/// runs the full V1/V2 signature and match verifier before durable storage or P2P publication.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PalwDaAdmissionError {
    #[error("PALW DA admission is disabled on this network")]
    Disabled,
    #[error("PALW leaf {batch_id}/{leaf_index} is not present at the selected-chain sink")]
    LeafNotFound { batch_id: Hash64, leaf_index: u32 },
    #[error("PALW provider bond {0} is not present at the selected-chain sink")]
    ProviderNotFound(TransactionOutpoint),
    #[error("PALW DA object version {0} does not match a supported committed leaf schema")]
    UnsupportedObjectVersion(u16),
    #[error("PALW DA object failed full semantic admission: {0}")]
    InvalidObject(String),
    #[error("PALW DA admission store failure: {0}")]
    Store(String),
}

/// One selected-chain Object-v2 which the node may need to recover. This is an internal node-service
/// view, not a wire or consensus encoding. `deadline_daa_score` is the open challenge deadline when
/// challenged and otherwise the retention deadline; challenged targets sort ahead of background
/// obligation probes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDaFetchTargetV1 {
    pub obligation_id: Hash64,
    pub batch_id: Hash64,
    pub leaf_index: u32,
    pub object_root: Hash64,
    pub object_len: u32,
    pub chunk_count: u16,
    pub required_chunk_index: u16,
    pub deadline_daa_score: u64,
    pub challenged: bool,
}

/// Bytes already admitted by the complete selected-chain semantic verifier and revalidated against
/// their content-addressed durable-store key. Only canonical Object-v2 bytes are returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDaServingObjectV1 {
    pub object_root: Hash64,
    pub bytes: std::sync::Arc<Vec<u8>>,
}

/// Bounded, fork-coherent input to the node's DA rehydration/fetch service.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwDaServiceSnapshotV1 {
    pub selected_parent: crate::BlockHash,
    pub current_daa_score: u64,
    pub serving_objects: Vec<PalwDaServingObjectV1>,
    pub fetch_targets: Vec<PalwDaFetchTargetV1>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwDaObjectGcStatsV1 {
    pub selected_parent: crate::BlockHash,
    pub retained_roots: usize,
    pub scanned_objects: usize,
    pub deleted_objects: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PalwDaServiceError {
    #[error("PALW DA service is disabled on this network")]
    Disabled,
    #[error("PALW DA service snapshot store failure: {0}")]
    Store(String),
    #[error("PALW DA selected-chain state is internally inconsistent: {0}")]
    Inconsistent(String),
    #[error("PALW DA selected parent changed during object GC; deleted zero objects")]
    StaleSnapshot,
}

fn expected_chunk_count(object_len: usize) -> Result<u16, PalwDaError> {
    if object_len == 0 || object_len > PALW_DA_MAX_OBJECT_BYTES {
        return Err(PalwDaError::ObjectSize { got: object_len, max: PALW_DA_MAX_OBJECT_BYTES });
    }
    let count = object_len.div_ceil(PALW_DA_CHUNK_BYTES);
    if count == 0 || count > PALW_DA_MAX_CHUNKS {
        return Err(PalwDaError::ChunkMetadata);
    }
    Ok(count as u16)
}

#[inline]
fn supported_object_version(version: u16) -> bool {
    matches!(
        version,
        PALW_RECEIPT_DA_OBJECT_VERSION_V1 | PALW_RECEIPT_DA_OBJECT_VERSION_V2 | PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1
    )
}

/// Read the consensus DA object-version prefix shared by every canonical object schema.
///
/// This does not decode or semantically validate the version-specific body. It is intended for
/// generic chunk-proof producers, which commit to every byte and therefore only need the version
/// domain before constructing the proof. Object admission remains responsible for canonical
/// version-specific decoding.
pub fn palw_receipt_da_object_version(object: &[u8]) -> Result<u16, PalwDaError> {
    let prefix: [u8; 2] = object.get(..2).ok_or(PalwDaError::NonCanonicalObject)?.try_into().expect("two-byte slice");
    let version = u16::from_le_bytes(prefix);
    if !supported_object_version(version) {
        return Err(PalwDaError::UnsupportedVersion(version));
    }
    Ok(version)
}

fn chunk_leaf_hash(object_version: u16, object_len: u32, chunk_count: u16, chunk_index: u16, chunk: &[u8]) -> Hash64 {
    let mut preimage = Vec::with_capacity(14 + chunk.len());
    preimage.extend_from_slice(&object_version.to_le_bytes());
    preimage.extend_from_slice(&object_len.to_le_bytes());
    preimage.extend_from_slice(&chunk_count.to_le_bytes());
    preimage.extend_from_slice(&chunk_index.to_le_bytes());
    preimage.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
    preimage.extend_from_slice(chunk);
    blake2b_512_keyed(PALW_DA_CHUNK_LEAF_DOMAIN, &preimage)
}

fn empty_leaf_hash(object_version: u16, object_len: u32, chunk_count: u16, padded_index: u16) -> Hash64 {
    let mut preimage = Vec::with_capacity(10);
    preimage.extend_from_slice(&object_version.to_le_bytes());
    preimage.extend_from_slice(&object_len.to_le_bytes());
    preimage.extend_from_slice(&chunk_count.to_le_bytes());
    preimage.extend_from_slice(&padded_index.to_le_bytes());
    blake2b_512_keyed(PALW_DA_CHUNK_EMPTY_DOMAIN, &preimage)
}

fn node_hash(left: &Hash64, right: &Hash64) -> Hash64 {
    let mut preimage = Vec::with_capacity(2 * HASH64_SIZE);
    push_hash(&mut preimage, left);
    push_hash(&mut preimage, right);
    blake2b_512_keyed(PALW_DA_CHUNK_NODE_DOMAIN, &preimage)
}

fn finalize_root(object_version: u16, object_len: u32, chunk_count: u16, apex: &Hash64) -> Hash64 {
    let mut preimage = Vec::with_capacity(8 + HASH64_SIZE);
    preimage.extend_from_slice(&object_version.to_le_bytes());
    preimage.extend_from_slice(&object_len.to_le_bytes());
    preimage.extend_from_slice(&chunk_count.to_le_bytes());
    push_hash(&mut preimage, apex);
    blake2b_512_keyed(PALW_DA_OBJECT_ROOT_DOMAIN, &preimage)
}

fn tree_leaves(object_version: u16, object: &[u8], chunk_count: u16) -> Vec<Hash64> {
    let width = (chunk_count as usize).next_power_of_two();
    let mut leaves = Vec::with_capacity(width);
    for index in 0..width {
        if index < chunk_count as usize {
            let start = index * PALW_DA_CHUNK_BYTES;
            let end = ((index + 1) * PALW_DA_CHUNK_BYTES).min(object.len());
            leaves.push(chunk_leaf_hash(object_version, object.len() as u32, chunk_count, index as u16, &object[start..end]));
        } else {
            leaves.push(empty_leaf_hash(object_version, object.len() as u32, chunk_count, index as u16));
        }
    }
    leaves
}

pub fn palw_receipt_da_commitment(object_version: u16, object: &[u8]) -> Result<PalwReceiptDaCommitmentV1, PalwDaError> {
    if !supported_object_version(object_version) {
        return Err(PalwDaError::UnsupportedVersion(object_version));
    }
    let chunk_count = expected_chunk_count(object.len())?;
    let mut level = tree_leaves(object_version, object, chunk_count);
    while level.len() > 1 {
        level = level.chunks_exact(2).map(|pair| node_hash(&pair[0], &pair[1])).collect();
    }
    Ok(PalwReceiptDaCommitmentV1 {
        object_version,
        object_len: object.len() as u32,
        chunk_count,
        root: finalize_root(object_version, object.len() as u32, chunk_count, &level[0]),
    })
}

pub fn palw_receipt_da_object_bytes(object: &PalwReceiptDaObjectV1) -> Result<Vec<u8>, PalwDaError> {
    if object.version != PALW_RECEIPT_DA_OBJECT_VERSION_V1 {
        return Err(PalwDaError::UnsupportedVersion(object.version));
    }
    let bytes = borsh::to_vec(object).map_err(|_| PalwDaError::NonCanonicalObject)?;
    expected_chunk_count(bytes.len())?;
    Ok(bytes)
}

pub fn palw_receipt_da_object_commitment(object: &PalwReceiptDaObjectV1) -> Result<PalwReceiptDaCommitmentV1, PalwDaError> {
    let bytes = palw_receipt_da_object_bytes(object)?;
    palw_receipt_da_commitment(object.version, &bytes)
}

pub fn palw_receipt_da_chunk_proof(
    object_version: u16,
    object: &[u8],
    chunk_index: u16,
) -> Result<PalwReceiptDaChunkProofV1, PalwDaError> {
    let chunk_count = expected_chunk_count(object.len())?;
    if !supported_object_version(object_version) {
        return Err(PalwDaError::UnsupportedVersion(object_version));
    }
    if chunk_index >= chunk_count {
        return Err(PalwDaError::ChunkIndex);
    }
    let start = chunk_index as usize * PALW_DA_CHUNK_BYTES;
    let end = (start + PALW_DA_CHUNK_BYTES).min(object.len());
    let mut level = tree_leaves(object_version, object, chunk_count);
    let mut index = chunk_index as usize;
    let mut siblings = Vec::with_capacity(level.len().ilog2() as usize);
    while level.len() > 1 {
        siblings.push(level[index ^ 1]);
        index /= 2;
        level = level.chunks_exact(2).map(|pair| node_hash(&pair[0], &pair[1])).collect();
    }
    Ok(PalwReceiptDaChunkProofV1 {
        version: PALW_RECEIPT_DA_PROOF_VERSION_V1,
        object_version,
        object_len: object.len() as u32,
        chunk_count,
        chunk_index,
        chunk: object[start..end].to_vec(),
        siblings,
    })
}

pub fn verify_palw_receipt_da_chunk(expected_root: &Hash64, proof: &PalwReceiptDaChunkProofV1) -> Result<(), PalwDaError> {
    if proof.version != PALW_RECEIPT_DA_PROOF_VERSION_V1 {
        return Err(PalwDaError::UnsupportedVersion(proof.version));
    }
    if !supported_object_version(proof.object_version) {
        return Err(PalwDaError::UnsupportedVersion(proof.object_version));
    }
    let expected_count = expected_chunk_count(proof.object_len as usize)?;
    if proof.chunk_count != expected_count || proof.chunk_count as usize > PALW_DA_MAX_CHUNKS {
        return Err(PalwDaError::ChunkMetadata);
    }
    if proof.chunk_index >= proof.chunk_count {
        return Err(PalwDaError::ChunkIndex);
    }
    let expected_len = if proof.chunk_index + 1 == proof.chunk_count {
        proof.object_len as usize - proof.chunk_index as usize * PALW_DA_CHUNK_BYTES
    } else {
        PALW_DA_CHUNK_BYTES
    };
    if proof.chunk.len() != expected_len || proof.chunk.len() > PALW_DA_CHUNK_BYTES {
        return Err(PalwDaError::ChunkLength);
    }
    let expected_depth = (proof.chunk_count as usize).next_power_of_two().ilog2() as usize;
    if proof.siblings.len() != expected_depth || proof.siblings.len() > PALW_DA_MAX_PROOF_DEPTH {
        return Err(PalwDaError::ProofDepth);
    }
    let mut node = chunk_leaf_hash(proof.object_version, proof.object_len, proof.chunk_count, proof.chunk_index, &proof.chunk);
    let mut index = proof.chunk_index as usize;
    for sibling in &proof.siblings {
        node = if index & 1 == 0 { node_hash(&node, sibling) } else { node_hash(sibling, &node) };
        index >>= 1;
    }
    let got = finalize_root(proof.object_version, proof.object_len, proof.chunk_count, &node);
    if got != *expected_root {
        return Err(PalwDaError::WrongProof);
    }
    Ok(())
}

pub fn verify_palw_provider_session_authorization(
    network_id: u32,
    provider_bond: TransactionOutpoint,
    session_public_key: &[u8],
    required_from_epoch: u64,
    required_until_epoch: u64,
    bond: &PalwProviderBondRecord,
    auth: &PalwProviderSessionAuthorizationV1,
    verify_signature: &mut impl FnMut(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> Result<(), PalwDaError> {
    if auth.version != PALW_PROVIDER_SESSION_AUTH_VERSION_V1
        || auth.network_id != network_id
        || auth.provider_bond != provider_bond
        || auth.provider_bond != bond.bond_outpoint
        || auth.owner_public_key.len() != STAKE_VALIDATOR_PUBKEY_LEN
        || auth.session_public_key.len() != STAKE_VALIDATOR_PUBKEY_LEN
        || auth.signature.len() != STAKE_ATTESTATION_SIG_LEN
        || auth.authorization_nonce == Hash64::default()
        || auth.valid_from_epoch > auth.valid_until_epoch
        || auth.valid_until_epoch.saturating_sub(auth.valid_from_epoch) > PALW_DA_MAX_SESSION_EPOCHS
        || required_from_epoch > required_until_epoch
        || auth.valid_from_epoch > required_from_epoch
        || auth.valid_until_epoch < required_until_epoch
        || auth.session_public_key != session_public_key
    {
        return Err(PalwDaError::SessionAuthorization);
    }
    if auth.owner_public_key != bond.owner_public_key || validator_id_from_pubkey(&auth.owner_public_key) != bond.owner_pubkey_hash {
        return Err(PalwDaError::ProviderOwner);
    }
    let digest = auth.signing_hash();
    if !verify_signature(&auth.owner_public_key, digest.as_byte_slice(), &auth.signature, PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT) {
        return Err(PalwDaError::SessionAuthorizationSignature);
    }
    Ok(())
}

fn verify_receipt(
    leaf: &PalwPublicLeafV1,
    receipt: &ReplicaExecutionReceiptV1,
    bond: &PalwProviderBondRecord,
    verify_signature: &mut impl FnMut(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> Result<(), PalwDaError> {
    let shape_capacity = bond.capacity_by_shape.iter().any(|(shape, capacity)| *shape == receipt.shape_id && *capacity != 0);
    if receipt.version != 1
        || receipt.session_public_key.len() != STAKE_VALIDATOR_PUBKEY_LEN
        || receipt.signature.len() != STAKE_ATTESTATION_SIG_LEN
        || receipt.job_nullifier != leaf.job_nullifier
        || receipt.model_profile_id != leaf.model_profile_id
        || receipt.runtime_class_id != leaf.runtime_class_id
        || receipt.shape_id != leaf.shape_id
        || receipt.quantum_count != leaf.quantum_count
        || receipt.completed_at_epoch > leaf.registered_epoch
        || !bond.runtime_classes.contains(&receipt.runtime_class_id)
        || !shape_capacity
    {
        return Err(PalwDaError::Receipt);
    }
    if receipt.receipt_da_root != Hash64::default() {
        return Err(PalwDaError::EmbeddedRootNotZero);
    }
    let digest = receipt.signing_hash();
    if !verify_signature(
        &receipt.session_public_key,
        digest.as_byte_slice(),
        &receipt.signature,
        PALW_REPLICA_RECEIPT_V1_MLDSA87_CONTEXT,
    ) {
        return Err(PalwDaError::ReceiptSignature);
    }
    Ok(())
}

/// Decode and fully verify the object committed by `leaf.receipt_da_root`.
///
/// The callback is the sole crypto dependency boundary. Consensus supplies
/// `kaspa_txscript::verify_mldsa87_with_context`; core tests can use a deterministic
/// verifier without introducing a core -> txscript dependency cycle.
#[allow(clippy::too_many_arguments)]
pub fn verify_palw_receipt_da_object(
    network_id: u32,
    leaf: &PalwPublicLeafV1,
    provider_a: &PalwProviderBondRecord,
    provider_b: &PalwProviderBondRecord,
    pov_daa_score: u64,
    object_bytes: &[u8],
    mut verify_signature: impl FnMut(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> Result<PalwReceiptDaObjectV1, PalwDaError> {
    expected_chunk_count(object_bytes.len())?;
    let object = PalwReceiptDaObjectV1::try_from_slice(object_bytes).map_err(|_| PalwDaError::NonCanonicalObject)?;
    if object.version != PALW_RECEIPT_DA_OBJECT_VERSION_V1 {
        return Err(PalwDaError::UnsupportedVersion(object.version));
    }
    if object.network_id != network_id
        || leaf.receipt_da_object_version != PALW_RECEIPT_DA_OBJECT_VERSION_V1
        || object.batch_id != leaf.batch_id
        || object.leaf_index != leaf.leaf_index
        || object.receipt_a.provider_bond != leaf.provider_a_bond
        || object.receipt_b.provider_bond != leaf.provider_b_bond
        || provider_a.bond_outpoint != leaf.provider_a_bond
        || provider_b.bond_outpoint != leaf.provider_b_bond
        || leaf.provider_a_bond == leaf.provider_b_bond
    {
        return Err(PalwDaError::ObjectBinding);
    }
    let canonical = palw_receipt_da_object_bytes(&object)?;
    if canonical != object_bytes {
        return Err(PalwDaError::NonCanonicalObject);
    }
    let commitment = palw_receipt_da_commitment(object.version, object_bytes)?;
    if commitment.root != leaf.receipt_da_root {
        return Err(PalwDaError::WrongProof);
    }
    if commitment.object_len != leaf.receipt_da_object_len || commitment.chunk_count != leaf.receipt_da_chunk_count {
        return Err(PalwDaError::ChunkMetadata);
    }
    if provider_a.version != 1
        || provider_b.version != 1
        || !matches!(effective_provider_bond_status(provider_a, pov_daa_score), super::PalwProviderBondStatus::Active)
        || !matches!(effective_provider_bond_status(provider_b, pov_daa_score), super::PalwProviderBondStatus::Active)
    {
        return Err(PalwDaError::ProviderBond);
    }

    verify_palw_provider_session_authorization(
        network_id,
        object.receipt_a.provider_bond,
        &object.receipt_a.session_public_key,
        object.receipt_a.completed_at_epoch,
        object.receipt_a.completed_at_epoch,
        provider_a,
        &object.session_authorization_a,
        &mut verify_signature,
    )?;
    verify_palw_provider_session_authorization(
        network_id,
        object.receipt_b.provider_bond,
        &object.receipt_b.session_public_key,
        object.receipt_b.completed_at_epoch,
        object.receipt_b.completed_at_epoch,
        provider_b,
        &object.session_authorization_b,
        &mut verify_signature,
    )?;
    verify_receipt(leaf, &object.receipt_a, provider_a, &mut verify_signature)?;
    verify_receipt(leaf, &object.receipt_b, provider_b, &mut verify_signature)?;

    let a = &object.receipt_a;
    let b = &object.receipt_b;
    let m = &object.match_record;
    if a.job_nullifier != b.job_nullifier
        || a.job_set_commitment != b.job_set_commitment
        || a.model_profile_id != b.model_profile_id
        || a.runtime_class_id != b.runtime_class_id
        || a.shape_id != b.shape_id
        || a.quantum_count != b.quantum_count
        || a.output_commitment != b.output_commitment
        || a.canonical_gemm_trace_root != b.canonical_gemm_trace_root
        || a.operation_schedule_commitment != b.operation_schedule_commitment
    {
        return Err(PalwDaError::ReplicaMismatch);
    }
    let receipt_a_hash = a.hash();
    let receipt_b_hash = b.hash();
    if m.receipt_a_hash != receipt_a_hash
        || m.receipt_b_hash != receipt_b_hash
        || m.job_nullifier != a.job_nullifier
        || m.output_commitment != a.output_commitment
        || m.canonical_gemm_trace_root != a.canonical_gemm_trace_root
        || m.operation_schedule_commitment != a.operation_schedule_commitment
        || m.matched_at_epoch < a.completed_at_epoch.max(b.completed_at_epoch)
        || m.matched_at_epoch > leaf.registered_epoch
    {
        return Err(PalwDaError::MatchCommitment);
    }
    let expected_match = private_match_commitment(
        &a.output_commitment,
        &a.canonical_gemm_trace_root,
        &a.operation_schedule_commitment,
        &a.job_set_commitment,
        &receipt_a_hash,
        &receipt_b_hash,
    );
    if expected_match != leaf.private_match_commitment {
        return Err(PalwDaError::MatchCommitment);
    }
    Ok(object)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwDaPolicyV1 {
    pub min_beacon_burial_daa: u64,
    pub retention_daa: u64,
    pub response_window_daa: u64,
    pub samples_per_provider: u16,
    pub max_challenges_per_bond_per_epoch: u16,
}

impl PalwDaPolicyV1 {
    pub const STRICT_TESTNET: Self = Self {
        min_beacon_burial_daa: 100,
        retention_daa: 2_000,
        response_window_daa: 200,
        samples_per_provider: 1,
        max_challenges_per_bond_per_epoch: 4,
    };

    pub fn is_valid(&self) -> bool {
        self.min_beacon_burial_daa != 0
            && self.retention_daa > self.response_window_daa
            && self.response_window_daa != 0
            && (1..=PALW_DA_MAX_SAMPLES_PER_PROVIDER).contains(&self.samples_per_provider)
            && self.max_challenges_per_bond_per_epoch != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwBuriedBeaconV1 {
    pub epoch: u64,
    pub seed: Hash64,
    pub anchor_hash: Hash64,
    pub anchor_daa_score: u64,
    pub observed_daa_score: u64,
}

pub fn palw_da_provider_sample_indices(
    beacon: &PalwBuriedBeaconV1,
    provider_bond: &TransactionOutpoint,
    leaf_hash: &Hash64,
    object_root: &Hash64,
    chunk_count: u16,
    sample_count: u16,
    min_burial_daa: u64,
) -> Result<Vec<u16>, PalwDaError> {
    if beacon.observed_daa_score < beacon.anchor_daa_score.saturating_add(min_burial_daa) {
        return Err(PalwDaError::BeaconNotBuried);
    }
    if chunk_count == 0
        || chunk_count as usize > PALW_DA_MAX_CHUNKS
        || sample_count == 0
        || sample_count > PALW_DA_MAX_SAMPLES_PER_PROVIDER
        || sample_count > chunk_count
    {
        return Err(PalwDaError::Policy);
    }
    let mut scored = Vec::with_capacity(chunk_count as usize);
    for index in 0..chunk_count {
        let mut preimage = Vec::with_capacity(4 * HASH64_SIZE + 8 + 4 + 2);
        push_hash(&mut preimage, &beacon.seed);
        push_hash(&mut preimage, &beacon.anchor_hash);
        preimage.extend_from_slice(&beacon.epoch.to_le_bytes());
        push_outpoint(&mut preimage, provider_bond);
        push_hash(&mut preimage, leaf_hash);
        push_hash(&mut preimage, object_root);
        preimage.extend_from_slice(&chunk_count.to_le_bytes());
        preimage.extend_from_slice(&index.to_le_bytes());
        scored.push((blake2b_512_keyed(PALW_DA_SAMPLE_DOMAIN, &preimage), index));
    }
    scored.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut selected: Vec<u16> = scored.into_iter().take(sample_count as usize).map(|(_, index)| index).collect();
    selected.sort_unstable();
    Ok(selected)
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwDaChallengeV1 {
    pub version: u16,
    pub network_id: u32,
    pub obligation_id: Hash64,
    pub challenge_epoch: u64,
    pub opened_daa_score: u64,
    pub response_deadline_daa_score: u64,
    pub challenger_bond: TransactionOutpoint,
    pub challenger_owner_public_key: Vec<u8>,
    pub challenge_nonce: Hash64,
    pub signature: Vec<u8>,
}

impl PalwDaChallengeV1 {
    pub fn signing_hash(&self) -> Hash64 {
        let mut preimage = Vec::with_capacity(256 + self.challenger_owner_public_key.len());
        preimage.extend_from_slice(&self.version.to_le_bytes());
        preimage.extend_from_slice(&self.network_id.to_le_bytes());
        push_hash(&mut preimage, &self.obligation_id);
        preimage.extend_from_slice(&self.challenge_epoch.to_le_bytes());
        preimage.extend_from_slice(&self.opened_daa_score.to_le_bytes());
        preimage.extend_from_slice(&self.response_deadline_daa_score.to_le_bytes());
        push_outpoint(&mut preimage, &self.challenger_bond);
        push_var(&mut preimage, &self.challenger_owner_public_key);
        push_hash(&mut preimage, &self.challenge_nonce);
        blake2b_512_keyed(PALW_DA_CHALLENGE_SIGNING_DOMAIN, &preimage)
    }

    pub fn challenge_id(&self) -> Hash64 {
        let mut preimage = Vec::with_capacity(HASH64_SIZE + 8 + self.signature.len());
        push_hash(&mut preimage, &self.signing_hash());
        push_var(&mut preimage, &self.signature);
        blake2b_512_keyed(PALW_DA_CHALLENGE_ID_DOMAIN, &preimage)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwDaResponseV1 {
    pub version: u16,
    pub network_id: u32,
    pub challenge_id: Hash64,
    pub provider_bond: TransactionOutpoint,
    pub provider_owner_public_key: Vec<u8>,
    pub chunk_proof: PalwReceiptDaChunkProofV1,
    pub signature: Vec<u8>,
}

impl PalwDaResponseV1 {
    pub fn signing_hash(&self) -> Hash64 {
        let proof = borsh::to_vec(&self.chunk_proof).expect("borsh");
        let mut preimage = Vec::with_capacity(192 + self.provider_owner_public_key.len() + proof.len());
        preimage.extend_from_slice(&self.version.to_le_bytes());
        preimage.extend_from_slice(&self.network_id.to_le_bytes());
        push_hash(&mut preimage, &self.challenge_id);
        push_outpoint(&mut preimage, &self.provider_bond);
        push_var(&mut preimage, &self.provider_owner_public_key);
        push_var(&mut preimage, &proof);
        blake2b_512_keyed(PALW_DA_RESPONSE_SIGNING_DOMAIN, &preimage)
    }

    pub fn response_id(&self) -> Hash64 {
        let mut preimage = Vec::with_capacity(HASH64_SIZE + 8 + self.signature.len());
        push_hash(&mut preimage, &self.signing_hash());
        push_var(&mut preimage, &self.signature);
        blake2b_512_keyed(PALW_DA_RESPONSE_ID_DOMAIN, &preimage)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwDaTimeoutEvidenceV1 {
    pub version: u16,
    pub network_id: u32,
    pub challenge_id: Hash64,
    pub provider_bond: TransactionOutpoint,
}

impl PalwDaTimeoutEvidenceV1 {
    pub fn evidence_id(&self) -> Hash64 {
        let mut preimage = Vec::with_capacity(2 + 4 + 2 * HASH64_SIZE + 4);
        preimage.extend_from_slice(&self.version.to_le_bytes());
        preimage.extend_from_slice(&self.network_id.to_le_bytes());
        push_hash(&mut preimage, &self.challenge_id);
        push_outpoint(&mut preimage, &self.provider_bond);
        blake2b_512_keyed(PALW_DA_TIMEOUT_ID_DOMAIN, &preimage)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub enum PalwDaObligationStatusV1 {
    Pending,
    Challenged(Hash64),
    Satisfied(Hash64),
    TimedOut(Hash64),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwDaObligationV1 {
    pub version: u16,
    pub obligation_id: Hash64,
    pub batch_id: Hash64,
    pub leaf_index: u32,
    pub leaf_hash: Hash64,
    pub object_root: Hash64,
    pub object_len: u32,
    pub chunk_count: u16,
    pub chunk_index: u16,
    pub provider_bond: TransactionOutpoint,
    pub beacon_epoch: u64,
    pub beacon_anchor: Hash64,
    pub created_daa_score: u64,
    pub retention_until_daa_score: u64,
    pub status: PalwDaObligationStatusV1,
}

impl PalwDaObligationV1 {
    #[allow(clippy::too_many_arguments)]
    fn derive_id(
        batch_id: &Hash64,
        leaf_index: u32,
        leaf_hash: &Hash64,
        object_root: &Hash64,
        provider_bond: &TransactionOutpoint,
        chunk_index: u16,
        beacon_epoch: u64,
        beacon_anchor: &Hash64,
    ) -> Hash64 {
        let mut preimage = Vec::with_capacity(5 * HASH64_SIZE + 4 + 4 + 2 + 8);
        push_hash(&mut preimage, batch_id);
        preimage.extend_from_slice(&leaf_index.to_le_bytes());
        push_hash(&mut preimage, leaf_hash);
        push_hash(&mut preimage, object_root);
        push_outpoint(&mut preimage, provider_bond);
        preimage.extend_from_slice(&chunk_index.to_le_bytes());
        preimage.extend_from_slice(&beacon_epoch.to_le_bytes());
        push_hash(&mut preimage, beacon_anchor);
        blake2b_512_keyed(PALW_DA_OBLIGATION_ID_DOMAIN, &preimage)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub enum PalwDaChallengeStatusV1 {
    Open,
    Responded(Hash64),
    TimedOut(Hash64),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwDaChallengeStateV1 {
    pub challenge: PalwDaChallengeV1,
    pub provider_bond: TransactionOutpoint,
    pub object_root: Hash64,
    pub chunk_index: u16,
    pub status: PalwDaChallengeStatusV1,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize,
)]
pub struct PalwDaChallengeCounterKeyV1 {
    pub challenger_txid: Hash64,
    pub challenger_index: u32,
    pub epoch: u64,
}

impl PalwDaChallengeCounterKeyV1 {
    fn new(challenger_bond: TransactionOutpoint, epoch: u64) -> Self {
        Self { challenger_txid: challenger_bond.transaction_id, challenger_index: challenger_bond.index, epoch }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwDaStateV1 {
    pub version: u16,
    pub obligations: BTreeMap<Hash64, PalwDaObligationV1>,
    pub challenges: BTreeMap<Hash64, PalwDaChallengeStateV1>,
    pub challenge_counts: BTreeMap<PalwDaChallengeCounterKeyV1, u16>,
    pub timeout_evidence: BTreeSet<Hash64>,
    /// Exact timeout-slash delta contributed by the block whose key stores this state. A child clears
    /// it before applying its own transactions. Keeping the one-block delta makes selected-chain
    /// apply/revert exact even after older terminal challenge history is compacted.
    pub block_slashed_providers: Vec<TransactionOutpoint>,
}

impl Default for PalwDaStateV1 {
    fn default() -> Self {
        Self {
            version: PALW_DA_STATE_VERSION_V1,
            obligations: BTreeMap::new(),
            challenges: BTreeMap::new(),
            challenge_counts: BTreeMap::new(),
            timeout_evidence: BTreeSet::new(),
            block_slashed_providers: Vec::new(),
        }
    }
}

impl kaspa_utils::mem_size::MemSizeEstimator for PalwDaStateV1 {
    fn estimate_mem_units(&self) -> usize {
        (self.obligations.len()
            + self.challenges.len()
            + self.challenge_counts.len()
            + self.timeout_evidence.len()
            + self.block_slashed_providers.len())
        .max(1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDaUndoV1 {
    previous: Box<PalwDaStateV1>,
}

impl PalwDaStateV1 {
    fn undo(&self) -> PalwDaUndoV1 {
        PalwDaUndoV1 { previous: Box::new(self.clone()) }
    }

    /// Exact canonical size of `PalwDaPruningSnapshotV1 { version: 1, pruning_point, state: self }`.
    /// The pruning-point value is irrelevant to its fixed 64-byte encoding. Counting through the
    /// Borsh serializer keeps this definition locked to the actual wire encoding as fields evolve.
    pub fn canonical_snapshot_encoded_len(&self) -> Option<usize> {
        let mut counter = PalwDaEncodedLenCounter::default();
        BorshSerialize::serialize(&PALW_DA_SNAPSHOT_VERSION_V1, &mut counter).ok()?;
        BorshSerialize::serialize(&Hash64::default(), &mut counter).ok()?;
        BorshSerialize::serialize(self, &mut counter).ok()?;
        Some(counter.len)
    }

    fn snapshot_fits_with_reserve(&self, reserve_bytes: usize, max_bytes: usize) -> bool {
        self.canonical_snapshot_encoded_len().and_then(|len| len.checked_add(reserve_bytes)).is_some_and(|len| len <= max_bytes)
    }

    fn finish_snapshot_bounded_mutation_with_budget(
        &mut self,
        undo: PalwDaUndoV1,
        reserve_bytes: usize,
        max_bytes: usize,
    ) -> Result<PalwDaUndoV1, PalwDaError> {
        if self.snapshot_fits_with_reserve(reserve_bytes, max_bytes) {
            Ok(undo)
        } else {
            self.revert(undo);
            Err(PalwDaError::Capacity)
        }
    }

    fn finish_snapshot_bounded_mutation(&mut self, undo: PalwDaUndoV1, reserve_bytes: usize) -> Result<PalwDaUndoV1, PalwDaError> {
        self.finish_snapshot_bounded_mutation_with_budget(undo, reserve_bytes, PALW_DA_MAX_PRUNING_SNAPSHOT_BYTES)
    }

    /// Canonical digest used by live Header-v4 frontier commitments and pruning imports alike. It
    /// deliberately excludes the containing block/pruning-point coordinate; callers bind that
    /// coordinate in their outer commitment, while identical DA state always hashes identically.
    pub fn state_root(&self) -> Hash64 {
        blake2b_512_keyed(PALW_DA_STATE_ROOT_DOMAIN, &borsh::to_vec(self).expect("borsh"))
    }

    /// Start a child's delta while retaining inherited obligations/challenges.
    pub fn begin_child_block(&mut self) {
        self.block_slashed_providers.clear();
    }

    pub fn record_block_slash(&mut self, provider_bond: TransactionOutpoint) -> Result<(), PalwDaError> {
        if self.block_slashed_providers.contains(&provider_bond) {
            return Ok(());
        }
        if self.block_slashed_providers.len() >= PALW_DA_MAX_TIMEOUT_EVIDENCE {
            return Err(PalwDaError::Capacity);
        }
        self.block_slashed_providers.push(provider_bond);
        if self.snapshot_fits_with_reserve(0, PALW_DA_MAX_PRUNING_SNAPSHOT_BYTES) {
            Ok(())
        } else {
            self.block_slashed_providers.pop();
            Err(PalwDaError::Capacity)
        }
    }

    pub fn revert(&mut self, undo: PalwDaUndoV1) {
        *self = *undo.previous;
    }

    pub fn register_leaf_obligations(
        &mut self,
        leaf: &PalwPublicLeafV1,
        commitment: PalwReceiptDaCommitmentV1,
        beacon: &PalwBuriedBeaconV1,
        policy: &PalwDaPolicyV1,
        created_daa_score: u64,
    ) -> Result<(Vec<Hash64>, PalwDaUndoV1), PalwDaError> {
        if self.version != PALW_DA_STATE_VERSION_V1
            || !policy.is_valid()
            || !supported_object_version(commitment.object_version)
            || commitment.object_version != leaf.receipt_da_object_version
            || commitment.root != leaf.receipt_da_root
            || commitment.object_len != leaf.receipt_da_object_len
            || commitment.chunk_count != leaf.receipt_da_chunk_count
            || expected_chunk_count(commitment.object_len as usize)? != commitment.chunk_count
        {
            return Err(PalwDaError::Policy);
        }
        let leaf_hash = leaf.leaf_hash();
        let mut ids = Vec::with_capacity(2 * policy.samples_per_provider as usize);
        let mut pending = Vec::with_capacity(2 * policy.samples_per_provider as usize);
        for provider_bond in [leaf.provider_a_bond, leaf.provider_b_bond] {
            let samples = palw_da_provider_sample_indices(
                beacon,
                &provider_bond,
                &leaf_hash,
                &commitment.root,
                commitment.chunk_count,
                policy.samples_per_provider,
                policy.min_beacon_burial_daa,
            )?;
            for chunk_index in samples {
                let obligation_id = PalwDaObligationV1::derive_id(
                    &leaf.batch_id,
                    leaf.leaf_index,
                    &leaf_hash,
                    &commitment.root,
                    &provider_bond,
                    chunk_index,
                    beacon.epoch,
                    &beacon.anchor_hash,
                );
                if self.obligations.contains_key(&obligation_id) || pending.iter().any(|(id, _)| *id == obligation_id) {
                    return Err(PalwDaError::ObligationState);
                }
                pending.push((
                    obligation_id,
                    PalwDaObligationV1 {
                        version: 1,
                        obligation_id,
                        batch_id: leaf.batch_id,
                        leaf_index: leaf.leaf_index,
                        leaf_hash,
                        object_root: commitment.root,
                        object_len: commitment.object_len,
                        chunk_count: commitment.chunk_count,
                        chunk_index,
                        provider_bond,
                        beacon_epoch: beacon.epoch,
                        beacon_anchor: beacon.anchor_hash,
                        created_daa_score,
                        retention_until_daa_score: created_daa_score.saturating_add(policy.retention_daa),
                        status: PalwDaObligationStatusV1::Pending,
                    },
                ));
                ids.push(obligation_id);
            }
        }
        if self.obligations.len().saturating_add(pending.len()) > PALW_DA_MAX_OBLIGATIONS {
            return Err(PalwDaError::Capacity);
        }
        // Both providers' sample derivations and every duplicate/cap check complete before state is
        // changed, so any rejected registration is atomic without caller-side repair.
        let undo = self.undo();
        self.obligations.extend(pending);
        let undo = self.finish_snapshot_bounded_mutation(undo, 0)?;
        Ok((ids, undo))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_challenge(
        &mut self,
        challenge: PalwDaChallengeV1,
        challenger_record: &PalwProviderBondRecord,
        network_id: u32,
        current_daa_score: u64,
        current_epoch: u64,
        policy: &PalwDaPolicyV1,
        mut verify_signature: impl FnMut(&[u8], &[u8], &[u8], &[u8]) -> bool,
    ) -> Result<PalwDaUndoV1, PalwDaError> {
        if !policy.is_valid()
            || challenge.version != PALW_DA_CHALLENGE_VERSION_V1
            || challenge.network_id != network_id
            || challenge.challenge_epoch != current_epoch
            || challenge.opened_daa_score != current_daa_score
            || challenge.challenge_nonce == Hash64::default()
            || challenge.challenger_owner_public_key.len() != STAKE_VALIDATOR_PUBKEY_LEN
            || challenge.signature.len() != STAKE_ATTESTATION_SIG_LEN
        {
            return Err(PalwDaError::Challenge);
        }
        let challenge_id = challenge.challenge_id();
        if self.challenges.contains_key(&challenge_id) {
            return Err(PalwDaError::Challenge);
        }
        if self.challenges.len() >= PALW_DA_MAX_CHALLENGES {
            return Err(PalwDaError::Capacity);
        }
        let Some(obligation) = self.obligations.get(&challenge.obligation_id) else {
            return Err(PalwDaError::ObligationState);
        };
        let expected_deadline = current_daa_score.saturating_add(policy.response_window_daa).min(obligation.retention_until_daa_score);
        if !matches!(obligation.status, PalwDaObligationStatusV1::Pending)
            || current_daa_score > obligation.retention_until_daa_score
            || challenge.response_deadline_daa_score != expected_deadline
            || challenge.challenger_bond == obligation.provider_bond
        {
            return Err(PalwDaError::Challenge);
        }
        if challenger_record.bond_outpoint != challenge.challenger_bond
            || challenger_record.owner_public_key != challenge.challenger_owner_public_key
            || validator_id_from_pubkey(&challenge.challenger_owner_public_key) != challenger_record.owner_pubkey_hash
            || !matches!(effective_provider_bond_status(challenger_record, current_daa_score), super::PalwProviderBondStatus::Active)
        {
            return Err(PalwDaError::ChallengerBond);
        }
        let key = PalwDaChallengeCounterKeyV1::new(challenge.challenger_bond, current_epoch);
        if !self.challenge_counts.contains_key(&key) && self.challenge_counts.len() >= PALW_DA_MAX_CHALLENGE_COUNTERS {
            return Err(PalwDaError::Capacity);
        }
        if self.challenge_counts.get(&key).copied().unwrap_or(0) >= policy.max_challenges_per_bond_per_epoch {
            return Err(PalwDaError::ChallengeRateLimit);
        }
        let digest = challenge.signing_hash();
        if !verify_signature(
            &challenge.challenger_owner_public_key,
            digest.as_byte_slice(),
            &challenge.signature,
            PALW_DA_CHALLENGE_V1_MLDSA87_CONTEXT,
        ) {
            return Err(PalwDaError::ChallengeSignature);
        }
        let undo = self.undo();
        let obligation = self.obligations.get_mut(&challenge.obligation_id).expect("checked above");
        obligation.status = PalwDaObligationStatusV1::Challenged(challenge_id);
        *self.challenge_counts.entry(key).or_insert(0) += 1;
        self.challenges.insert(
            challenge_id,
            PalwDaChallengeStateV1 {
                provider_bond: obligation.provider_bond,
                object_root: obligation.object_root,
                chunk_index: obligation.chunk_index,
                challenge,
                status: PalwDaChallengeStatusV1::Open,
            },
        );
        self.finish_snapshot_bounded_mutation(undo, 0)
    }

    pub fn apply_response(
        &mut self,
        response: PalwDaResponseV1,
        provider_record: &PalwProviderBondRecord,
        network_id: u32,
        current_daa_score: u64,
        mut verify_signature: impl FnMut(&[u8], &[u8], &[u8], &[u8]) -> bool,
    ) -> Result<PalwDaUndoV1, PalwDaError> {
        if response.version != PALW_DA_RESPONSE_VERSION_V1
            || response.network_id != network_id
            || response.provider_owner_public_key.len() != STAKE_VALIDATOR_PUBKEY_LEN
            || response.signature.len() != STAKE_ATTESTATION_SIG_LEN
        {
            return Err(PalwDaError::Response);
        }
        let Some(challenge_state) = self.challenges.get(&response.challenge_id) else {
            return Err(PalwDaError::Response);
        };
        if !matches!(challenge_state.status, PalwDaChallengeStatusV1::Open)
            || response.provider_bond != challenge_state.provider_bond
            || provider_record.bond_outpoint != response.provider_bond
            || provider_record.owner_public_key != response.provider_owner_public_key
            || validator_id_from_pubkey(&response.provider_owner_public_key) != provider_record.owner_pubkey_hash
            || provider_record.slashed_at_daa_score.is_some_and(|score| score <= current_daa_score)
            || current_daa_score > challenge_state.challenge.response_deadline_daa_score
            || response.chunk_proof.chunk_index != challenge_state.chunk_index
            || response.chunk_proof.object_len
                != self.obligations.get(&challenge_state.challenge.obligation_id).ok_or(PalwDaError::ObligationState)?.object_len
        {
            return Err(PalwDaError::Response);
        }
        verify_palw_receipt_da_chunk(&challenge_state.object_root, &response.chunk_proof)?;
        let digest = response.signing_hash();
        if !verify_signature(
            &response.provider_owner_public_key,
            digest.as_byte_slice(),
            &response.signature,
            PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT,
        ) {
            return Err(PalwDaError::ResponseSignature);
        }
        let undo = self.undo();
        let response_id = response.response_id();
        let challenge_state = self.challenges.get_mut(&response.challenge_id).expect("checked above");
        challenge_state.status = PalwDaChallengeStatusV1::Responded(response_id);
        let obligation =
            self.obligations.get_mut(&challenge_state.challenge.obligation_id).expect("challenge always references an obligation");
        obligation.status = PalwDaObligationStatusV1::Satisfied(response_id);
        self.finish_snapshot_bounded_mutation(undo, 0)
    }

    pub fn apply_timeout_evidence(
        &mut self,
        evidence: PalwDaTimeoutEvidenceV1,
        network_id: u32,
        current_daa_score: u64,
    ) -> Result<(PalwProviderBondMutation, PalwDaUndoV1), PalwDaError> {
        if evidence.version != PALW_DA_TIMEOUT_EVIDENCE_VERSION_V1 {
            return Err(PalwDaError::UnsupportedVersion(evidence.version));
        }
        if evidence.network_id != network_id {
            return Err(PalwDaError::TimeoutEvidence);
        }
        let evidence_id = evidence.evidence_id();
        if self.timeout_evidence.contains(&evidence_id) {
            return Err(PalwDaError::TimeoutEvidence);
        }
        if self.timeout_evidence.len() >= PALW_DA_MAX_TIMEOUT_EVIDENCE {
            return Err(PalwDaError::Capacity);
        }
        let Some(challenge_state) = self.challenges.get(&evidence.challenge_id) else {
            return Err(PalwDaError::TimeoutEvidence);
        };
        if !matches!(challenge_state.status, PalwDaChallengeStatusV1::Open)
            || challenge_state.provider_bond != evidence.provider_bond
            || current_daa_score <= challenge_state.challenge.response_deadline_daa_score
        {
            return Err(PalwDaError::TimeoutEvidence);
        }
        let obligation_id = challenge_state.challenge.obligation_id;
        // The virtual processor records the emitted slash outpoint in this block's exact reorg delta
        // immediately after this transition. Reserve its canonical bytes here so a timeout accepted at
        // the snapshot boundary cannot make that mandatory follow-up fail-stop.
        let block_slash_reserve = if self.block_slashed_providers.contains(&evidence.provider_bond) {
            0
        } else {
            let mut counter = PalwDaEncodedLenCounter::default();
            BorshSerialize::serialize(&evidence.provider_bond, &mut counter).map_err(|_| PalwDaError::Capacity)?;
            counter.len
        };
        let undo = self.undo();
        self.timeout_evidence.insert(evidence_id);
        self.challenges.get_mut(&evidence.challenge_id).expect("checked above").status =
            PalwDaChallengeStatusV1::TimedOut(evidence_id);
        self.obligations.get_mut(&obligation_id).ok_or(PalwDaError::ObligationState)?.status =
            PalwDaObligationStatusV1::TimedOut(evidence_id);
        let undo = self.finish_snapshot_bounded_mutation(undo, block_slash_reserve)?;
        Ok((PalwProviderBondMutation::Slash(evidence.provider_bond, current_daa_score), undo))
    }

    /// A certificate cannot advance a batch until every provider-specific sampled chunk has an
    /// objective, timely response. An empty obligation set is not success.
    pub fn certificate_allowed(&self, batch_id: &Hash64) -> bool {
        let mut found = false;
        for obligation in self.obligations.values().filter(|obligation| &obligation.batch_id == batch_id) {
            found = true;
            if !matches!(obligation.status, PalwDaObligationStatusV1::Satisfied(_)) {
                return false;
            }
        }
        found
    }

    /// Rewards are withheld while any live retention obligation is unresolved and forever after a
    /// proven timeout. This is provider-wide so moving to another batch cannot evade a failure.
    pub fn reward_allowed(&self, provider_bond: &TransactionOutpoint, current_daa_score: u64) -> bool {
        self.obligations.values().filter(|o| &o.provider_bond == provider_bond).all(|obligation| match obligation.status {
            PalwDaObligationStatusV1::Satisfied(_) => true,
            PalwDaObligationStatusV1::Pending => current_daa_score > obligation.retention_until_daa_score,
            PalwDaObligationStatusV1::Challenged(_) | PalwDaObligationStatusV1::TimedOut(_) => false,
        })
    }

    /// Exit waits out every retention window and refuses unresolved challenges or proven failures.
    pub fn exit_allowed(&self, provider_bond: &TransactionOutpoint, current_daa_score: u64) -> bool {
        self.obligations.values().filter(|o| &o.provider_bond == provider_bond).all(|obligation| {
            current_daa_score > obligation.retention_until_daa_score
                && matches!(obligation.status, PalwDaObligationStatusV1::Pending | PalwDaObligationStatusV1::Satisfied(_))
        })
    }

    /// Remove history only after the fork-local PALW lifecycle view says a batch cannot be referenced
    /// again. This is the safe terminal compaction point for both successful and timed-out batches:
    /// the certificate gate remains fail-closed for an empty set, while a timed-out provider's slash
    /// has already reached the selected-chain registry before its batch ages out of that view.
    pub fn retain_referenceable_batches(&mut self, referenceable: &BTreeSet<Hash64>, current_epoch: u64) {
        self.obligations.retain(|_, obligation| referenceable.contains(&obligation.batch_id));
        let obligations = &self.obligations;
        self.challenges.retain(|_, challenge| obligations.contains_key(&challenge.challenge.obligation_id));
        let live_timeout_evidence: BTreeSet<Hash64> = self
            .challenges
            .values()
            .filter_map(|challenge| match challenge.status {
                PalwDaChallengeStatusV1::TimedOut(evidence_id) => Some(evidence_id),
                _ => None,
            })
            .collect();
        self.timeout_evidence.retain(|id| live_timeout_evidence.contains(id));
        // A challenge transaction must declare the current epoch, so older counters can never affect
        // a future rate-limit decision.
        self.challenge_counts.retain(|key, _| key.epoch >= current_epoch);
    }

    /// Once a certificate has passed the DA gate on this fork, its obligations cannot influence a
    /// later transition. Remove the terminal batch immediately so successful traffic does not wait
    /// for pruning to reclaim state.
    pub fn remove_certified_batch(&mut self, batch_id: &Hash64) {
        let keep: BTreeSet<Hash64> =
            self.obligations.values().filter(|obligation| &obligation.batch_id != batch_id).map(|o| o.batch_id).collect();
        self.retain_referenceable_batches(&keep, 0);
    }

    /// Validate cardinalities and every state cross-reference. Local block-state reads and pruning
    /// imports share this predicate, so a decoder accepting bytes is never mistaken for semantic
    /// state validity.
    pub fn validate_structure(&self) -> bool {
        if self.version != PALW_DA_STATE_VERSION_V1
            || self.obligations.len() > PALW_DA_MAX_OBLIGATIONS
            || self.challenges.len() > PALW_DA_MAX_CHALLENGES
            || self.challenge_counts.len() > PALW_DA_MAX_CHALLENGE_COUNTERS
            || self.timeout_evidence.len() > PALW_DA_MAX_TIMEOUT_EVIDENCE
            || self.block_slashed_providers.len() > PALW_DA_MAX_TIMEOUT_EVIDENCE
            || self.block_slashed_providers.iter().copied().collect::<HashSet<_>>().len() != self.block_slashed_providers.len()
            || self.challenge_counts.values().any(|count| *count == 0)
        {
            return false;
        }
        if !self.obligations.iter().all(|(id, obligation)| {
            *id == obligation.obligation_id
                && obligation.version == 1
                && obligation.chunk_count != 0
                && obligation.chunk_index < obligation.chunk_count
                && obligation.chunk_count as usize <= PALW_DA_MAX_CHUNKS
                && expected_chunk_count(obligation.object_len as usize).is_ok_and(|count| count == obligation.chunk_count)
        }) {
            return false;
        }
        if !self.challenges.iter().all(|(id, challenge)| {
            if *id != challenge.challenge.challenge_id() {
                return false;
            }
            let Some(obligation) = self.obligations.get(&challenge.challenge.obligation_id) else {
                return false;
            };
            if challenge.challenge.version != PALW_DA_CHALLENGE_VERSION_V1
                || challenge.challenge.challenger_owner_public_key.len() != STAKE_VALIDATOR_PUBKEY_LEN
                || challenge.challenge.signature.len() != STAKE_ATTESTATION_SIG_LEN
                || challenge.challenge.challenge_nonce == Hash64::default()
                || challenge.challenge.opened_daa_score < obligation.created_daa_score
                || challenge.challenge.response_deadline_daa_score < challenge.challenge.opened_daa_score
                || challenge.challenge.response_deadline_daa_score > obligation.retention_until_daa_score
                || challenge.challenge.challenger_bond == obligation.provider_bond
                || challenge.provider_bond != obligation.provider_bond
                || challenge.object_root != obligation.object_root
                || challenge.chunk_index != obligation.chunk_index
            {
                return false;
            }
            match (challenge.status, obligation.status) {
                (PalwDaChallengeStatusV1::Open, PalwDaObligationStatusV1::Challenged(challenge_id)) => challenge_id == *id,
                (PalwDaChallengeStatusV1::Responded(response_id), PalwDaObligationStatusV1::Satisfied(got)) => response_id == got,
                (PalwDaChallengeStatusV1::TimedOut(evidence_id), PalwDaObligationStatusV1::TimedOut(got)) => {
                    evidence_id == got && self.timeout_evidence.contains(&evidence_id)
                }
                _ => false,
            }
        }) {
            return false;
        }
        self.timeout_evidence.iter().all(|evidence_id| {
            self.challenges
                .values()
                .any(|challenge| matches!(challenge.status, PalwDaChallengeStatusV1::TimedOut(id) if &id == evidence_id))
        }) && self.snapshot_fits_with_reserve(0, PALW_DA_MAX_PRUNING_SNAPSHOT_BYTES)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwDaPruningSnapshotV1 {
    pub version: u16,
    pub pruning_point: Hash64,
    pub state: PalwDaStateV1,
}

impl PalwDaPruningSnapshotV1 {
    pub fn snapshot_root(&self) -> Hash64 {
        blake2b_512_keyed(PALW_DA_SNAPSHOT_DOMAIN, &borsh::to_vec(self).expect("borsh"))
    }

    pub fn validate(&self) -> bool {
        self.version == PALW_DA_SNAPSHOT_VERSION_V1 && self.state.validate_structure()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw::{PalwProofType, PalwProviderBondStatus, ProviderBondView, is_provider_bond_releasable_at};
    use crate::tx::{ScriptPublicKey, ScriptVec};

    const NETWORK: u32 = 0x51_44_41_31;

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; HASH64_SIZE])
    }

    fn fake_signature(public_key: &[u8], message: &[u8], context: &[u8]) -> Vec<u8> {
        let mut preimage = Vec::with_capacity(public_key.len() + message.len() + context.len() + 24);
        push_var(&mut preimage, public_key);
        push_var(&mut preimage, message);
        push_var(&mut preimage, context);
        let digest = blake2b_512_keyed(b"misaka-palw-da-test-sign-v1", &preimage);
        (0..STAKE_ATTESTATION_SIG_LEN).map(|index| digest.as_byte_slice()[index % HASH64_SIZE]).collect()
    }

    fn fake_verify(public_key: &[u8], message: &[u8], signature: &[u8], context: &[u8]) -> bool {
        fake_signature(public_key, message, context) == signature
    }

    fn bond(byte: u8, owner_public_key: Vec<u8>) -> PalwProviderBondRecord {
        let outpoint = TransactionOutpoint::new(h(byte), byte as u32);
        PalwProviderBondRecord {
            version: 1,
            bond_outpoint: outpoint,
            owner_pubkey_hash: validator_id_from_pubkey(&owner_public_key),
            owner_public_key,
            operator_group_id: h(byte.wrapping_add(1)),
            runtime_classes: vec![h(0x31)],
            capacity_by_shape: vec![(7, 2)],
            reward_key_root: h(byte.wrapping_add(2)),
            amount_sompi: 1_000_000,
            activation_daa_score: 1,
            created_daa_score: 1,
            unbond_delay_epochs: 10,
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
        }
    }

    struct Fixture {
        leaf: PalwPublicLeafV1,
        object: PalwReceiptDaObjectV1,
        bytes: Vec<u8>,
        commitment: PalwReceiptDaCommitmentV1,
        provider_a: PalwProviderBondRecord,
        provider_b: PalwProviderBondRecord,
    }

    fn fixture(embedded_root: Hash64) -> Fixture {
        let owner_a = vec![0xa1; STAKE_VALIDATOR_PUBKEY_LEN];
        let owner_b = vec![0xa2; STAKE_VALIDATOR_PUBKEY_LEN];
        let session_a = vec![0xb1; STAKE_VALIDATOR_PUBKEY_LEN];
        let session_b = vec![0xb2; STAKE_VALIDATOR_PUBKEY_LEN];
        let provider_a = bond(0x11, owner_a.clone());
        let provider_b = bond(0x22, owner_b.clone());

        let mut auth_a = PalwProviderSessionAuthorizationV1 {
            version: 1,
            network_id: NETWORK,
            provider_bond: provider_a.bond_outpoint,
            owner_public_key: owner_a,
            session_public_key: session_a.clone(),
            valid_from_epoch: 3,
            valid_until_epoch: 9,
            authorization_nonce: h(0xc1),
            signature: vec![],
        };
        auth_a.signature =
            fake_signature(&auth_a.owner_public_key, auth_a.signing_hash().as_byte_slice(), PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT);
        let mut auth_b = PalwProviderSessionAuthorizationV1 {
            version: 1,
            network_id: NETWORK,
            provider_bond: provider_b.bond_outpoint,
            owner_public_key: owner_b,
            session_public_key: session_b.clone(),
            valid_from_epoch: 3,
            valid_until_epoch: 9,
            authorization_nonce: h(0xc2),
            signature: vec![],
        };
        auth_b.signature =
            fake_signature(&auth_b.owner_public_key, auth_b.signing_hash().as_byte_slice(), PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT);

        let receipt = |provider_bond: TransactionOutpoint, session_public_key: Vec<u8>| ReplicaExecutionReceiptV1 {
            version: 1,
            provider_bond,
            session_public_key,
            job_nullifier: h(0x41),
            job_set_commitment: h(0x42),
            model_profile_id: h(0x30),
            runtime_class_id: h(0x31),
            shape_id: 7,
            quantum_count: 2,
            output_commitment: h(0x43),
            canonical_gemm_trace_root: h(0x44),
            operation_schedule_commitment: h(0x45),
            receipt_da_root: embedded_root,
            completed_at_epoch: 5,
            signature: vec![],
        };
        let mut receipt_a = receipt(provider_a.bond_outpoint, session_a);
        receipt_a.signature = fake_signature(
            &receipt_a.session_public_key,
            receipt_a.signing_hash().as_byte_slice(),
            PALW_REPLICA_RECEIPT_V1_MLDSA87_CONTEXT,
        );
        let mut receipt_b = receipt(provider_b.bond_outpoint, session_b);
        receipt_b.signature = fake_signature(
            &receipt_b.session_public_key,
            receipt_b.signing_hash().as_byte_slice(),
            PALW_REPLICA_RECEIPT_V1_MLDSA87_CONTEXT,
        );
        let receipt_a_hash = receipt_a.hash();
        let receipt_b_hash = receipt_b.hash();
        let match_record = ReplicaMatchRecordV1 {
            receipt_a_hash,
            receipt_b_hash,
            job_nullifier: receipt_a.job_nullifier,
            output_commitment: receipt_a.output_commitment,
            canonical_gemm_trace_root: receipt_a.canonical_gemm_trace_root,
            operation_schedule_commitment: receipt_a.operation_schedule_commitment,
            matched_at_epoch: 5,
        };
        let match_commitment = private_match_commitment(
            &receipt_a.output_commitment,
            &receipt_a.canonical_gemm_trace_root,
            &receipt_a.operation_schedule_commitment,
            &receipt_a.job_set_commitment,
            &receipt_a_hash,
            &receipt_b_hash,
        );
        let object = PalwReceiptDaObjectV1 {
            version: 1,
            network_id: NETWORK,
            batch_id: h(0x51),
            leaf_index: 3,
            receipt_a,
            receipt_b,
            match_record,
            session_authorization_a: auth_a,
            session_authorization_b: auth_b,
        };
        let bytes = palw_receipt_da_object_bytes(&object).unwrap();
        let commitment = palw_receipt_da_commitment(object.version, &bytes).unwrap();
        let reward_spk = ScriptPublicKey::new(0, ScriptVec::from_slice(&[0x51]));
        let leaf = PalwPublicLeafV1 {
            version: 1,
            batch_id: object.batch_id,
            leaf_index: object.leaf_index,
            job_nullifier: h(0x41),
            ticket_nullifier_commitment: h(0x52),
            model_profile_id: h(0x30),
            runtime_class_id: h(0x31),
            shape_id: 7,
            quantum_count: 2,
            proof_type: PalwProofType::ReplicaExactV1.as_u8(),
            provider_a_bond: provider_a.bond_outpoint,
            provider_b_bond: provider_b.bond_outpoint,
            provider_a_reward_script: reward_spk.clone(),
            provider_b_reward_script: reward_spk,
            ticket_authority_pk_hash: h(0x53),
            private_match_commitment: match_commitment,
            receipt_da_object_version: PALW_RECEIPT_DA_OBJECT_VERSION_V1,
            receipt_da_root: commitment.root,
            receipt_da_object_len: commitment.object_len,
            receipt_da_chunk_count: commitment.chunk_count,
            receipt_v3_compute_set_id: Hash64::default(),
            receipt_v3_job_challenge: Hash64::default(),
            receipt_v3_issued_epoch: 0,
            receipt_v3_expires_epoch: 0,
            registered_epoch: 6,
            activation_epoch: 9,
            expiry_epoch: 20,
            leaf_bond_sompi: 1_000_000,
        };
        Fixture { leaf, object, bytes, commitment, provider_a, provider_b }
    }

    #[test]
    fn canonical_object_round_trips_and_verifies_every_binding() {
        let f = fixture(Hash64::default());
        assert!(f.bytes.len() > 2 * PALW_DA_CHUNK_BYTES);
        let got = verify_palw_receipt_da_object(NETWORK, &f.leaf, &f.provider_a, &f.provider_b, 10, &f.bytes, fake_verify).unwrap();
        assert_eq!(got, f.object);
        assert_eq!(palw_receipt_da_object_commitment(&got).unwrap(), f.commitment);
    }

    #[test]
    fn embedded_root_zero_rule_breaks_the_self_reference_and_is_strict() {
        let f = fixture(h(0xee));
        assert_eq!(
            verify_palw_receipt_da_object(NETWORK, &f.leaf, &f.provider_a, &f.provider_b, 10, &f.bytes, fake_verify,),
            Err(PalwDaError::EmbeddedRootNotZero)
        );
    }

    #[test]
    fn chunk_proofs_bind_length_count_index_bytes_and_domain() {
        let f = fixture(Hash64::default());
        for index in 0..f.commitment.chunk_count {
            let proof = palw_receipt_da_chunk_proof(1, &f.bytes, index).unwrap();
            verify_palw_receipt_da_chunk(&f.commitment.root, &proof).unwrap();
        }
        let mut proof = palw_receipt_da_chunk_proof(1, &f.bytes, 0).unwrap();
        proof.chunk[0] ^= 1;
        assert_eq!(verify_palw_receipt_da_chunk(&f.commitment.root, &proof), Err(PalwDaError::WrongProof));
        let mut proof = palw_receipt_da_chunk_proof(1, &f.bytes, 0).unwrap();
        proof.object_len -= 1;
        assert!(matches!(
            verify_palw_receipt_da_chunk(&f.commitment.root, &proof),
            Err(PalwDaError::ChunkMetadata | PalwDaError::ChunkLength | PalwDaError::WrongProof)
        ));
        let mut proof = palw_receipt_da_chunk_proof(1, &f.bytes, 0).unwrap();
        proof.siblings.pop();
        assert_eq!(verify_palw_receipt_da_chunk(&f.commitment.root, &proof), Err(PalwDaError::ProofDepth));
        // Cross-class swap: version 3 is a real (search-snapshot) class, but every chunk/root
        // preimage includes the version, so a receipt proof relabeled as version 3 lands in a
        // disjoint hash domain and cannot match the committed receipt root.
        let mut proof = palw_receipt_da_chunk_proof(1, &f.bytes, 0).unwrap();
        proof.object_version = PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1;
        assert_eq!(verify_palw_receipt_da_chunk(&f.commitment.root, &proof), Err(PalwDaError::WrongProof));
        // A genuinely unknown version stays a version error.
        let mut proof = palw_receipt_da_chunk_proof(1, &f.bytes, 0).unwrap();
        proof.object_version = PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1 + 1;
        assert_eq!(
            verify_palw_receipt_da_chunk(&f.commitment.root, &proof),
            Err(PalwDaError::UnsupportedVersion(PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1 + 1))
        );
    }

    #[test]
    fn object_rejects_forged_receipt_owner_session_and_private_commitment() {
        let f = fixture(Hash64::default());
        let mut forged = f.object.clone();
        forged.receipt_a.signature[0] ^= 1;
        let bytes = palw_receipt_da_object_bytes(&forged).unwrap();
        let mut leaf = f.leaf.clone();
        leaf.receipt_da_root = palw_receipt_da_commitment(1, &bytes).unwrap().root;
        assert_eq!(
            verify_palw_receipt_da_object(NETWORK, &leaf, &f.provider_a, &f.provider_b, 10, &bytes, fake_verify),
            Err(PalwDaError::ReceiptSignature)
        );

        let mut wrong_owner = f.object.clone();
        wrong_owner.session_authorization_a.owner_public_key[0] ^= 1;
        let digest = wrong_owner.session_authorization_a.signing_hash();
        wrong_owner.session_authorization_a.signature = fake_signature(
            &wrong_owner.session_authorization_a.owner_public_key,
            digest.as_byte_slice(),
            PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT,
        );
        let bytes = palw_receipt_da_object_bytes(&wrong_owner).unwrap();
        leaf.receipt_da_root = palw_receipt_da_commitment(1, &bytes).unwrap().root;
        assert_eq!(
            verify_palw_receipt_da_object(NETWORK, &leaf, &f.provider_a, &f.provider_b, 10, &bytes, fake_verify),
            Err(PalwDaError::ProviderOwner)
        );

        let mut wrong_match = f.leaf.clone();
        wrong_match.private_match_commitment = h(0xff);
        assert_eq!(
            verify_palw_receipt_da_object(NETWORK, &wrong_match, &f.provider_a, &f.provider_b, 10, &f.bytes, fake_verify),
            Err(PalwDaError::MatchCommitment)
        );
    }

    #[test]
    fn decoding_is_size_bounded_and_rejects_trailing_bytes_and_v1_repacking() {
        let f = fixture(Hash64::default());
        assert_eq!(palw_receipt_da_object_version(&f.bytes), Ok(PALW_RECEIPT_DA_OBJECT_VERSION_V1));
        let mut v2_prefixed = f.bytes.clone();
        v2_prefixed[..2].copy_from_slice(&PALW_RECEIPT_DA_OBJECT_VERSION_V2.to_le_bytes());
        assert_eq!(palw_receipt_da_object_version(&v2_prefixed), Ok(PALW_RECEIPT_DA_OBJECT_VERSION_V2));
        assert_eq!(palw_receipt_da_object_version(&[1]), Err(PalwDaError::NonCanonicalObject));
        assert_eq!(palw_receipt_da_object_version(&0u16.to_le_bytes()), Err(PalwDaError::UnsupportedVersion(0)));
        let mut trailing = f.bytes.clone();
        trailing.push(0);
        let mut leaf = f.leaf.clone();
        leaf.receipt_da_root = palw_receipt_da_commitment(1, &trailing).unwrap().root;
        assert_eq!(
            verify_palw_receipt_da_object(NETWORK, &leaf, &f.provider_a, &f.provider_b, 10, &trailing, fake_verify),
            Err(PalwDaError::NonCanonicalObject)
        );
        assert_eq!(
            palw_receipt_da_commitment(1, &vec![0; PALW_DA_MAX_OBJECT_BYTES + 1]),
            Err(PalwDaError::ObjectSize { got: PALW_DA_MAX_OBJECT_BYTES + 1, max: PALW_DA_MAX_OBJECT_BYTES })
        );
        let mut old_version = f.object.clone();
        old_version.version = 0;
        assert_eq!(palw_receipt_da_object_bytes(&old_version), Err(PalwDaError::UnsupportedVersion(0)));
    }

    fn buried_beacon() -> PalwBuriedBeaconV1 {
        PalwBuriedBeaconV1 { epoch: 7, seed: h(0x71), anchor_hash: h(0x72), anchor_daa_score: 100, observed_daa_score: 250 }
    }

    #[test]
    fn sampling_is_buried_provider_specific_and_fork_local() {
        let f = fixture(Hash64::default());
        let beacon = buried_beacon();
        // A provider-specific hash is allowed to land on the same finite index by chance. Exercise a
        // family of roots and require at least one differing draw; asserting every pair differs would
        // incorrectly reject legitimate collisions in a three-chunk index space.
        assert!((0u8..=u8::MAX).any(|byte| {
            let root = h(byte);
            let a = palw_da_provider_sample_indices(
                &beacon,
                &f.provider_a.bond_outpoint,
                &f.leaf.leaf_hash(),
                &root,
                f.commitment.chunk_count,
                1,
                100,
            )
            .unwrap();
            let b = palw_da_provider_sample_indices(
                &beacon,
                &f.provider_b.bond_outpoint,
                &f.leaf.leaf_hash(),
                &root,
                f.commitment.chunk_count,
                1,
                100,
            )
            .unwrap();
            a != b
        }));
        let young = PalwBuriedBeaconV1 { observed_daa_score: 199, ..beacon };
        assert_eq!(
            palw_da_provider_sample_indices(
                &young,
                &f.provider_a.bond_outpoint,
                &f.leaf.leaf_hash(),
                &f.commitment.root,
                f.commitment.chunk_count,
                1,
                100,
            ),
            Err(PalwDaError::BeaconNotBuried)
        );
        assert!((0u8..=u8::MAX).any(|byte| {
            let fork = PalwBuriedBeaconV1 { anchor_hash: h(byte), ..beacon };
            palw_da_provider_sample_indices(
                &beacon,
                &f.provider_a.bond_outpoint,
                &f.leaf.leaf_hash(),
                &f.commitment.root,
                f.commitment.chunk_count,
                1,
                100,
            )
            .unwrap()
                != palw_da_provider_sample_indices(
                    &fork,
                    &f.provider_a.bond_outpoint,
                    &f.leaf.leaf_hash(),
                    &f.commitment.root,
                    f.commitment.chunk_count,
                    1,
                    100,
                )
                .unwrap()
        }));
    }

    fn challenge_for(
        obligation_id: Hash64,
        challenger: &PalwProviderBondRecord,
        opened: u64,
        deadline: u64,
        nonce: u8,
    ) -> PalwDaChallengeV1 {
        let mut challenge = PalwDaChallengeV1 {
            version: 1,
            network_id: NETWORK,
            obligation_id,
            challenge_epoch: 9,
            opened_daa_score: opened,
            response_deadline_daa_score: deadline,
            challenger_bond: challenger.bond_outpoint,
            challenger_owner_public_key: challenger.owner_public_key.clone(),
            challenge_nonce: h(nonce),
            signature: vec![],
        };
        challenge.signature = fake_signature(
            &challenge.challenger_owner_public_key,
            challenge.signing_hash().as_byte_slice(),
            PALW_DA_CHALLENGE_V1_MLDSA87_CONTEXT,
        );
        challenge
    }

    #[test]
    fn challenge_response_timeout_gates_and_revert_are_objective() {
        let f = fixture(Hash64::default());
        let mut state = PalwDaStateV1::default();
        let policy = PalwDaPolicyV1::STRICT_TESTNET;
        let (ids, registration_undo) = state.register_leaf_obligations(&f.leaf, f.commitment, &buried_beacon(), &policy, 300).unwrap();
        assert_eq!(ids.len(), 2);
        assert!(!state.certificate_allowed(&f.leaf.batch_id));
        assert!(!state.reward_allowed(&f.provider_a.bond_outpoint, 301));
        assert!(!state.exit_allowed(&f.provider_a.bond_outpoint, 301));

        let challenger = bond(0x33, vec![0xc3; STAKE_VALIDATOR_PUBKEY_LEN]);
        assert_eq!(effective_provider_bond_status(&challenger, 400), PalwProviderBondStatus::Active);
        let obligation = state.obligations.get(&ids[0]).unwrap().clone();
        let challenge = challenge_for(ids[0], &challenger, 400, 600, 0xd1);
        let challenge_id = challenge.challenge_id();
        let challenge_undo = state.apply_challenge(challenge.clone(), &challenger, NETWORK, 400, 9, &policy, fake_verify).unwrap();
        assert_eq!(state.apply_challenge(challenge, &challenger, NETWORK, 400, 9, &policy, fake_verify), Err(PalwDaError::Challenge));

        let provider = if obligation.provider_bond == f.provider_a.bond_outpoint { &f.provider_a } else { &f.provider_b };
        let proof = palw_receipt_da_chunk_proof(1, &f.bytes, obligation.chunk_index).unwrap();
        let mut response = PalwDaResponseV1 {
            version: 1,
            network_id: NETWORK,
            challenge_id,
            provider_bond: provider.bond_outpoint,
            provider_owner_public_key: provider.owner_public_key.clone(),
            chunk_proof: proof,
            signature: vec![],
        };
        response.signature = fake_signature(
            &response.provider_owner_public_key,
            response.signing_hash().as_byte_slice(),
            PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT,
        );
        let challenged_state = state.clone();
        let response_undo = state.apply_response(response.clone(), provider, NETWORK, 500, fake_verify).unwrap();
        assert!(matches!(state.obligations[&ids[0]].status, PalwDaObligationStatusV1::Satisfied(_)));
        assert_eq!(state.apply_response(response, provider, NETWORK, 500, fake_verify), Err(PalwDaError::Response));
        state.revert(response_undo);
        assert_eq!(state, challenged_state);

        let evidence =
            PalwDaTimeoutEvidenceV1 { version: 1, network_id: NETWORK, challenge_id, provider_bond: provider.bond_outpoint };
        assert_eq!(state.apply_timeout_evidence(evidence, NETWORK, 600), Err(PalwDaError::TimeoutEvidence));
        let (mutation, timeout_undo) = state.apply_timeout_evidence(evidence, NETWORK, 601).unwrap();
        assert_eq!(mutation, PalwProviderBondMutation::Slash(provider.bond_outpoint, 601));
        assert!(!state.reward_allowed(&provider.bond_outpoint, 2_500));
        assert!(!state.exit_allowed(&provider.bond_outpoint, 2_500));
        state.record_block_slash(provider.bond_outpoint).unwrap();
        let mut provider_view = ProviderBondView::from_records([(provider.bond_outpoint, provider.clone())]);
        provider_view.apply(std::slice::from_ref(&mutation));
        let slashed = provider_view.get(&provider.bond_outpoint).unwrap();
        assert_eq!(slashed.amount_sompi, provider.amount_sompi, "slash preserves the objective locked amount");
        assert_eq!(effective_provider_bond_status(slashed, 601), PalwProviderBondStatus::Slashed);
        assert!(provider_view.active_provider_bond_at(&provider.bond_outpoint, 601).is_none(), "slashed bond cannot back reward");
        assert!(!is_provider_bond_releasable_at(slashed, u64::MAX, 100), "slashed bond can never authorize exit");
        let mut compacted = state.clone();
        compacted.retain_referenceable_batches(&BTreeSet::new(), 10);
        assert!(compacted.obligations.is_empty() && compacted.challenges.is_empty() && compacted.timeout_evidence.is_empty());
        assert_eq!(compacted.block_slashed_providers, vec![provider.bond_outpoint], "one-block reorg delta survives compaction");
        assert_eq!(state.apply_timeout_evidence(evidence, NETWORK, 602), Err(PalwDaError::TimeoutEvidence));
        state.revert(timeout_undo);
        assert_eq!(state, challenged_state);
        state.revert(challenge_undo);
        assert!(matches!(state.obligations[&ids[0]].status, PalwDaObligationStatusV1::Pending));
        state.revert(registration_undo);
        assert_eq!(state, PalwDaStateV1::default());
    }

    #[test]
    fn pruning_snapshot_rejects_unreachable_challenge_shape() {
        let f = fixture(Hash64::default());
        let policy = PalwDaPolicyV1::STRICT_TESTNET;
        let challenger = bond(0x33, vec![0xc3; STAKE_VALIDATOR_PUBKEY_LEN]);
        let mut valid = PalwDaStateV1::default();
        let (ids, _) = valid.register_leaf_obligations(&f.leaf, f.commitment, &buried_beacon(), &policy, 300).unwrap();
        valid
            .apply_challenge(challenge_for(ids[0], &challenger, 400, 600, 0xd1), &challenger, NETWORK, 400, 9, &policy, fake_verify)
            .unwrap();
        assert!(PalwDaPruningSnapshotV1 { version: 1, pruning_point: h(0xf0), state: valid.clone() }.validate());

        // Re-key both cross-references after each mutation so rejection is attributable to the fixed
        // challenge shape itself, not merely to the challenge-id/map-key invariant.
        let mutate = |state: &mut PalwDaStateV1, f: &mut dyn FnMut(&mut PalwDaChallengeV1)| {
            let (old_id, mut challenge_state) = state.challenges.pop_first().expect("one challenge");
            let obligation_id = challenge_state.challenge.obligation_id;
            assert!(matches!(
                state.obligations[&obligation_id].status,
                PalwDaObligationStatusV1::Challenged(id) if id == old_id
            ));
            f(&mut challenge_state.challenge);
            let new_id = challenge_state.challenge.challenge_id();
            challenge_state.status = PalwDaChallengeStatusV1::Open;
            state.obligations.get_mut(&obligation_id).unwrap().status = PalwDaObligationStatusV1::Challenged(new_id);
            state.challenges.insert(new_id, challenge_state);
        };
        let rejected = |mut f: Box<dyn FnMut(&mut PalwDaChallengeV1)>| {
            let mut state = valid.clone();
            mutate(&mut state, f.as_mut());
            assert!(!PalwDaPruningSnapshotV1 { version: 1, pruning_point: h(0xf0), state }.validate());
        };

        rejected(Box::new(|challenge| challenge.version = PALW_DA_CHALLENGE_VERSION_V1 + 1));
        rejected(Box::new(|challenge| challenge.challenger_owner_public_key.pop().map(|_| ()).unwrap()));
        rejected(Box::new(|challenge| challenge.challenger_owner_public_key.push(0)));
        rejected(Box::new(|challenge| challenge.signature.pop().map(|_| ()).unwrap()));
        rejected(Box::new(|challenge| challenge.signature.push(0)));
        rejected(Box::new(|challenge| challenge.challenge_nonce = Hash64::default()));
        rejected(Box::new(|challenge| challenge.opened_daa_score = 299));
        rejected(Box::new(|challenge| challenge.response_deadline_daa_score = challenge.opened_daa_score - 1));
        rejected(Box::new(|challenge| challenge.response_deadline_daa_score = 2_301));
    }

    #[test]
    fn rate_limit_and_snapshot_seam_are_bounded_and_deduplicated() {
        let f = fixture(Hash64::default());
        let mut state = PalwDaStateV1::default();
        let policy = PalwDaPolicyV1 { max_challenges_per_bond_per_epoch: 1, ..PalwDaPolicyV1::STRICT_TESTNET };
        let (ids, _) = state.register_leaf_obligations(&f.leaf, f.commitment, &buried_beacon(), &policy, 300).unwrap();
        let challenger = bond(0x33, vec![0xc3; STAKE_VALIDATOR_PUBKEY_LEN]);
        state
            .apply_challenge(challenge_for(ids[0], &challenger, 400, 600, 0xd1), &challenger, NETWORK, 400, 9, &policy, fake_verify)
            .unwrap();
        assert_eq!(
            state.apply_challenge(
                challenge_for(ids[1], &challenger, 401, 601, 0xd2),
                &challenger,
                NETWORK,
                401,
                9,
                &policy,
                fake_verify,
            ),
            Err(PalwDaError::ChallengeRateLimit)
        );
        let snapshot = PalwDaPruningSnapshotV1 { version: 1, pruning_point: h(0xf1), state: state.clone() };
        assert!(snapshot.validate());
        let bytes = borsh::to_vec(&snapshot).unwrap();
        assert_eq!(state.canonical_snapshot_encoded_len(), Some(bytes.len()), "the admission counter must equal canonical Borsh");
        assert!(state.snapshot_fits_with_reserve(0, bytes.len()), "the exact byte boundary is inclusive");
        assert!(!state.snapshot_fits_with_reserve(0, bytes.len() - 1), "one byte below the canonical size is rejected");

        // Every size-increasing transition finishes through this rollback seam. Exercise it with a
        // small exact budget rather than allocating a 64-MiB fixture: the canonical boundary is the
        // same production predicate, and a rejected mutation must restore byte-identical state.
        let before_overflow = state.clone();
        let undo = state.undo();
        state.block_slashed_providers.push(TransactionOutpoint::new(h(0xf2), 0));
        assert_eq!(state.finish_snapshot_bounded_mutation_with_budget(undo, 0, bytes.len()), Err(PalwDaError::Capacity));
        assert_eq!(state, before_overflow, "snapshot-capacity rejection is atomic");

        let decoded = PalwDaPruningSnapshotV1::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.snapshot_root(), snapshot.snapshot_root());
        assert_eq!(decoded, snapshot);
    }

    #[test]
    fn re_genesis_wire_table_reserves_0x39_and_admits_only_da_0x3a_to_0x3c() {
        use crate::palw::{PalwTxError, validate_palw_overlay_payload};
        use crate::subnets::{
            SUBNETWORK_ID_PALW_CROSS_FORK_SLASHING_RESERVED, SUBNETWORK_ID_PALW_DA_CHALLENGE, SUBNETWORK_ID_PALW_DA_RESPONSE,
            SUBNETWORK_ID_PALW_DA_TIMEOUT_EVIDENCE,
        };

        assert_eq!(SUBNETWORK_ID_PALW_CROSS_FORK_SLASHING_RESERVED.palw_tx_kind(), Some(0x39));
        assert_eq!(SUBNETWORK_ID_PALW_DA_CHALLENGE.palw_tx_kind(), Some(0x3a));
        assert_eq!(SUBNETWORK_ID_PALW_DA_RESPONSE.palw_tx_kind(), Some(0x3b));
        assert_eq!(SUBNETWORK_ID_PALW_DA_TIMEOUT_EVIDENCE.palw_tx_kind(), Some(0x3c));
        assert_eq!(validate_palw_overlay_payload(0x39, &[]), Err(PalwTxError::UnsupportedKind(0x39)));

        let f = fixture(Hash64::default());
        let mut state = PalwDaStateV1::default();
        let policy = PalwDaPolicyV1::STRICT_TESTNET;
        let (ids, _) = state.register_leaf_obligations(&f.leaf, f.commitment, &buried_beacon(), &policy, 300).unwrap();
        let challenger = bond(0x33, vec![0xc3; STAKE_VALIDATOR_PUBKEY_LEN]);
        let challenge = challenge_for(ids[0], &challenger, 400, 600, 0xd1);
        assert_eq!(validate_palw_overlay_payload(0x3a, &borsh::to_vec(&challenge).unwrap()), Ok(()));
        let challenge_id = challenge.challenge_id();
        let obligation = state.obligations[&ids[0]].clone();
        let provider = if obligation.provider_bond == f.provider_a.bond_outpoint { &f.provider_a } else { &f.provider_b };
        let proof = palw_receipt_da_chunk_proof(1, &f.bytes, obligation.chunk_index).unwrap();
        let mut response = PalwDaResponseV1 {
            version: 1,
            network_id: NETWORK,
            challenge_id,
            provider_bond: provider.bond_outpoint,
            provider_owner_public_key: provider.owner_public_key.clone(),
            chunk_proof: proof,
            signature: vec![],
        };
        response.signature = fake_signature(
            &response.provider_owner_public_key,
            response.signing_hash().as_byte_slice(),
            PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT,
        );
        assert_eq!(validate_palw_overlay_payload(0x3b, &borsh::to_vec(&response).unwrap()), Ok(()));
        let timeout = PalwDaTimeoutEvidenceV1 { version: 1, network_id: NETWORK, challenge_id, provider_bond: provider.bond_outpoint };
        assert_eq!(validate_palw_overlay_payload(0x3c, &borsh::to_vec(&timeout).unwrap()), Ok(()));
    }

    #[test]
    fn da_hash_and_signature_domains_are_pairwise_distinct() {
        let hash_domains = [
            PALW_DA_CHUNK_LEAF_DOMAIN,
            PALW_DA_CHUNK_EMPTY_DOMAIN,
            PALW_DA_CHUNK_NODE_DOMAIN,
            PALW_DA_OBJECT_ROOT_DOMAIN,
            PALW_PROVIDER_SESSION_AUTH_DOMAIN,
            PALW_DA_SAMPLE_DOMAIN,
            PALW_DA_OBLIGATION_ID_DOMAIN,
            PALW_DA_CHALLENGE_SIGNING_DOMAIN,
            PALW_DA_CHALLENGE_ID_DOMAIN,
            PALW_DA_RESPONSE_SIGNING_DOMAIN,
            PALW_DA_RESPONSE_ID_DOMAIN,
            PALW_DA_TIMEOUT_ID_DOMAIN,
            PALW_DA_SNAPSHOT_DOMAIN,
        ];
        let unique: BTreeSet<&[u8]> = hash_domains.into_iter().collect();
        assert_eq!(unique.len(), hash_domains.len());
        let signature_contexts = [
            PALW_REPLICA_RECEIPT_V1_MLDSA87_CONTEXT,
            PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT,
            PALW_DA_CHALLENGE_V1_MLDSA87_CONTEXT,
            PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT,
        ];
        let unique: BTreeSet<&[u8]> = signature_contexts.into_iter().collect();
        assert_eq!(unique.len(), signature_contexts.len());
    }
}
