//! **Seam 3 — data availability for chat-context objects.**
//!
//! An auditor can only re-run a disputed job if the job's context is still retrievable. That is
//! what DA is for, and the node already has the whole machinery: a chunked commitment tree,
//! beacon-driven per-provider chunk sampling, obligations with retention, and challenge/response
//! with chunk proofs. This module does NOT re-implement any of it — it builds a canonical chat
//! object, commits it with [`palw_receipt_da_commitment`], samples with
//! [`palw_da_provider_sample_indices`], proves with [`palw_receipt_da_chunk_proof`], and
//! verifies with [`verify_palw_receipt_da_chunk`]. Same roots, same proofs, bit-for-bit.
//!
//! **Object version 4, and why the tree is re-implemented here.** Versions 1/2 are receipt
//! objects and 3 is the search snapshot; `palw_receipt_da_commitment` accepts exactly those
//! three. Widening that set in consensus-core would change what
//! `register_leaf_obligations` does with an on-chain leaf that declares a new version — a
//! consensus behavior change for a purely off-chain need, which is not a trade worth making.
//! So [`chat_da_commitment`] re-implements the SAME tree with the node's own exported domain
//! constants and a version tag of 4, and `commitment_matches_consensus_for_shared_versions`
//! proves byte-for-byte agreement with `palw_receipt_da_commitment` on versions 1/2/3. Same
//! algorithm, disjoint domain (the version is bound into every leaf, empty-leaf and root
//! preimage), zero consensus edits.
//!
//! **Honest transport boundary.** These obligations live in the BRIDGE's journal, not in
//! consensus: `PalwDaStateV1::register_leaf_obligations` is only reachable from an accepted
//! `0x32` leaf-chunk transaction, and the on-chain `0x3b` response lane refuses object versions
//! other than 1/2 anyway (`validate_da_response`). So the challenge/response here runs over the
//! bridge's HTTP with the same objects and proofs a chain lane would carry, and a failure
//! produces the same shape of evidence (`PalwDaTimeoutEvidenceV1`) that a node would act on —
//! it just is not submitted as a transaction by this process. That is the seam, stated plainly.

use kaspa_consensus_core::palw::da::{
    PALW_DA_CHUNK_BYTES, PALW_DA_CHUNK_EMPTY_DOMAIN, PALW_DA_CHUNK_LEAF_DOMAIN, PALW_DA_CHUNK_NODE_DOMAIN, PALW_DA_MAX_OBJECT_BYTES,
    PALW_DA_OBJECT_ROOT_DOMAIN, PalwBuriedBeaconV1, PalwDaPolicyV1, PalwDaTimeoutEvidenceV1, PalwReceiptDaCommitmentV1,
    palw_da_provider_sample_indices,
};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::{Hash64, blake2b_512_keyed};
use serde::{Deserialize, Serialize};

use crate::chain::{format_outpoint, parse_hash64, parse_outpoint};
use crate::match_key::{bytes_hex, decode_hex, hash64_hex};

/// The chat-context DA object version. 1/2 = receipt objects, 3 = search snapshot, 4 = this.
pub const PALW_CHAT_CONTEXT_DA_OBJECT_VERSION_V4: u16 = 4;
/// Digest domain for the decoded object (mirrors the snapshot class's `digest()` discipline).
pub const CHAT_CONTEXT_DIGEST_DOMAIN: &[u8] = b"misaka-palw-bridge-v1/chat-context-digest";

/// The retention/response policy. Reuses the node's own strict-testnet numbers rather than
/// inventing new ones (`retention_daa: 2000`, `response_window_daa: 200`,
/// `samples_per_provider: 1`, `min_beacon_burial_daa: 100`).
pub fn policy() -> PalwDaPolicyV1 {
    PalwDaPolicyV1::STRICT_TESTNET
}

/// Everything an auditor needs to re-run the job, and nothing else. Canonical fixed-order
/// little-endian encoding with length prefixes — no serde on the wire, so the bytes are the
/// spec (same discipline as `PalwSearchSnapshotV1::encode`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatContextObjectV4 {
    pub network_id: u32,
    pub job_challenge: Hash64,
    pub class_label: Vec<u8>,
    pub max_new: u32,
    pub prompt_token_ids: Vec<u32>,
    pub output_token_ids: Vec<u32>,
}

const MAX_TOKENS: usize = 32_768;
const MAX_LABEL: usize = 128;

impl ChatContextObjectV4 {
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut out =
            Vec::with_capacity(64 + self.class_label.len() + (self.prompt_token_ids.len() + self.output_token_ids.len()) * 4);
        out.extend_from_slice(&PALW_CHAT_CONTEXT_DA_OBJECT_VERSION_V4.to_le_bytes());
        out.extend_from_slice(&self.network_id.to_le_bytes());
        out.extend_from_slice(self.job_challenge.as_byte_slice());
        out.extend_from_slice(&(self.class_label.len() as u64).to_le_bytes());
        out.extend_from_slice(&self.class_label);
        out.extend_from_slice(&self.max_new.to_le_bytes());
        for (list, _name) in [(&self.prompt_token_ids, "prompt"), (&self.output_token_ids, "output")] {
            out.extend_from_slice(&(list.len() as u64).to_le_bytes());
            for id in list.iter() {
                out.extend_from_slice(&id.to_le_bytes());
            }
        }
        Ok(out)
    }

    pub fn decode_strict(bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = 0usize;
        let mut take = |n: usize| -> Result<&[u8], String> {
            let end = cursor.checked_add(n).ok_or("length overflow")?;
            if end > bytes.len() {
                return Err(format!("truncated object: wanted {n} more bytes at {cursor}"));
            }
            let slice = &bytes[cursor..end];
            cursor = end;
            Ok(slice)
        };
        let version = u16::from_le_bytes(take(2)?.try_into().expect("2"));
        if version != PALW_CHAT_CONTEXT_DA_OBJECT_VERSION_V4 {
            return Err(format!("object version {version} is not the chat-context class (4)"));
        }
        let network_id = u32::from_le_bytes(take(4)?.try_into().expect("4"));
        let job_challenge = Hash64::from_bytes(take(64)?.try_into().expect("64"));
        let label_len = u64::from_le_bytes(take(8)?.try_into().expect("8")) as usize;
        if label_len > MAX_LABEL {
            return Err(format!("class label {label_len} bytes exceeds {MAX_LABEL}"));
        }
        let class_label = take(label_len)?.to_vec();
        let max_new = u32::from_le_bytes(take(4)?.try_into().expect("4"));
        let mut lists = Vec::with_capacity(2);
        for _ in 0..2 {
            let count = u64::from_le_bytes(take(8)?.try_into().expect("8")) as usize;
            if count > MAX_TOKENS {
                return Err(format!("token list {count} exceeds {MAX_TOKENS}"));
            }
            let raw = take(count * 4)?;
            lists.push(raw.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().expect("4"))).collect::<Vec<u32>>());
        }
        if cursor != bytes.len() {
            return Err(format!("trailing bytes: {} unread", bytes.len() - cursor));
        }
        let output_token_ids = lists.pop().expect("2 lists");
        let prompt_token_ids = lists.pop().expect("2 lists");
        let object = Self { network_id, job_challenge, class_label, max_new, prompt_token_ids, output_token_ids };
        object.validate()?;
        Ok(object)
    }

    fn validate(&self) -> Result<(), String> {
        if self.class_label.is_empty() || self.class_label.len() > MAX_LABEL {
            return Err("class label must be 1..=128 bytes".into());
        }
        if self.prompt_token_ids.is_empty() {
            return Err("prompt token ids must be non-empty".into());
        }
        if self.prompt_token_ids.len() > MAX_TOKENS || self.output_token_ids.len() > MAX_TOKENS {
            return Err(format!("token lists must be <= {MAX_TOKENS}"));
        }
        if self.job_challenge == Hash64::default() {
            return Err("job challenge is all-zero".into());
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<Hash64, String> {
        Ok(blake2b_512_keyed(CHAT_CONTEXT_DIGEST_DOMAIN, &self.encode()?))
    }

    /// The chunk-tree commitment over this object (chat class, version 4).
    pub fn commitment(&self) -> Result<PalwReceiptDaCommitmentV1, String> {
        chat_da_commitment(PALW_CHAT_CONTEXT_DA_OBJECT_VERSION_V4, &self.encode()?)
    }
}

// ---- the DA chunk tree (algorithm-identical to consensus; see module docs) ----------------

fn expected_chunk_count(object_len: usize) -> Result<u16, String> {
    if object_len == 0 {
        return Err("empty DA object".into());
    }
    if object_len > PALW_DA_MAX_OBJECT_BYTES {
        return Err(format!("DA object {object_len} bytes exceeds {PALW_DA_MAX_OBJECT_BYTES}"));
    }
    Ok(object_len.div_ceil(PALW_DA_CHUNK_BYTES) as u16)
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
    let mut preimage = Vec::with_capacity(128);
    preimage.extend_from_slice(left.as_byte_slice());
    preimage.extend_from_slice(right.as_byte_slice());
    blake2b_512_keyed(PALW_DA_CHUNK_NODE_DOMAIN, &preimage)
}

fn finalize_root(object_version: u16, object_len: u32, chunk_count: u16, apex: &Hash64) -> Hash64 {
    let mut preimage = Vec::with_capacity(8 + 64);
    preimage.extend_from_slice(&object_version.to_le_bytes());
    preimage.extend_from_slice(&object_len.to_le_bytes());
    preimage.extend_from_slice(&chunk_count.to_le_bytes());
    preimage.extend_from_slice(apex.as_byte_slice());
    blake2b_512_keyed(PALW_DA_OBJECT_ROOT_DOMAIN, &preimage)
}

fn tree_leaves(object_version: u16, object: &[u8]) -> Result<(u16, Vec<Hash64>), String> {
    let chunk_count = expected_chunk_count(object.len())?;
    let object_len = object.len() as u32;
    let width = (chunk_count as usize).next_power_of_two();
    let mut leaves = Vec::with_capacity(width);
    for (index, chunk) in object.chunks(PALW_DA_CHUNK_BYTES).enumerate() {
        leaves.push(chunk_leaf_hash(object_version, object_len, chunk_count, index as u16, chunk));
    }
    for index in leaves.len()..width {
        leaves.push(empty_leaf_hash(object_version, object_len, chunk_count, index as u16));
    }
    Ok((chunk_count, leaves))
}

/// The chunk-tree commitment. Algorithm-identical to `palw_receipt_da_commitment`; proven by
/// `commitment_matches_consensus_for_shared_versions`.
pub fn chat_da_commitment(object_version: u16, object: &[u8]) -> Result<PalwReceiptDaCommitmentV1, String> {
    let (chunk_count, mut level) = tree_leaves(object_version, object)?;
    while level.len() > 1 {
        level = level.chunks_exact(2).map(|pair| node_hash(&pair[0], &pair[1])).collect();
    }
    let apex = level.pop().ok_or("empty DA tree")?;
    Ok(PalwReceiptDaCommitmentV1 {
        object_version,
        object_len: object.len() as u32,
        chunk_count,
        root: finalize_root(object_version, object.len() as u32, chunk_count, &apex),
    })
}

/// The sibling path for one chunk (`level[index ^ 1]` at each level).
pub fn chat_chunk_proof(object_version: u16, object: &[u8], chunk_index: u16) -> Result<(Vec<u8>, Vec<Hash64>), String> {
    let (chunk_count, mut level) = tree_leaves(object_version, object)?;
    if chunk_index >= chunk_count {
        return Err(format!("chunk {chunk_index} out of range (count {chunk_count})"));
    }
    let start = chunk_index as usize * PALW_DA_CHUNK_BYTES;
    let end = (start + PALW_DA_CHUNK_BYTES).min(object.len());
    let chunk = object[start..end].to_vec();
    let mut index = chunk_index as usize;
    let mut siblings = Vec::new();
    while level.len() > 1 {
        siblings.push(level[index ^ 1]);
        level = level.chunks_exact(2).map(|pair| node_hash(&pair[0], &pair[1])).collect();
        index /= 2;
    }
    Ok((chunk, siblings))
}

/// Verify a chunk against a root. Mirrors `verify_palw_receipt_da_chunk` including its
/// exact-depth requirement (depth is derived from the PADDED width).
pub fn verify_chat_chunk(
    expected_root: &Hash64,
    object_version: u16,
    object_len: u32,
    chunk_count: u16,
    chunk_index: u16,
    chunk: &[u8],
    siblings: &[Hash64],
) -> Result<(), String> {
    if chunk_count == 0 || chunk_index >= chunk_count {
        return Err("chunk index out of range".into());
    }
    let expected_depth = (chunk_count as usize).next_power_of_two().ilog2() as usize;
    if siblings.len() != expected_depth {
        return Err(format!("proof depth {} != expected {expected_depth}", siblings.len()));
    }
    let mut node = chunk_leaf_hash(object_version, object_len, chunk_count, chunk_index, chunk);
    let mut index = chunk_index as usize;
    for sibling in siblings {
        node = if index & 1 == 0 { node_hash(&node, sibling) } else { node_hash(sibling, &node) };
        index /= 2;
    }
    if finalize_root(object_version, object_len, chunk_count, &node) != *expected_root {
        return Err("chunk proof does not reproduce the object root".into());
    }
    Ok(())
}

/// The commitment as it travels on the wire.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaCommitmentWire {
    pub object_version: u16,
    pub object_len: u32,
    pub chunk_count: u16,
    pub root_hex: String,
}

impl DaCommitmentWire {
    pub fn from_commitment(c: &PalwReceiptDaCommitmentV1) -> Self {
        Self { object_version: c.object_version, object_len: c.object_len, chunk_count: c.chunk_count, root_hex: hash64_hex(&c.root) }
    }
    pub fn root(&self) -> Result<Hash64, String> {
        parse_hash64(&self.root_hex)
    }
    pub fn matches(&self, c: &PalwReceiptDaCommitmentV1) -> bool {
        self.object_version == c.object_version
            && self.object_len == c.object_len
            && self.chunk_count == c.chunk_count
            && self.root_hex == hash64_hex(&c.root)
    }
}

/// One provider's retention duty for one object.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaObligation {
    pub obligation_id_hex: String,
    pub job_id: String,
    pub provider_bond: String,
    pub commitment: DaCommitmentWire,
    /// Which chunk THIS provider must be able to prove (beacon-sampled, not chosen).
    pub chunk_index: u16,
    pub beacon_epoch: u64,
    pub created_daa_score: u64,
    pub retention_until_daa_score: u64,
    pub status: DaObligationStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DaObligationStatus {
    Pending,
    Challenged { deadline_daa_score: u64 },
    Satisfied,
    TimedOut,
}

/// Obligation id, same preimage shape the node uses (job id stands in for batch/leaf, which do
/// not exist off-chain).
pub fn obligation_id(
    job_id: &str,
    provider_bond: &TransactionOutpoint,
    root: &Hash64,
    chunk_index: u16,
    beacon: &PalwBuriedBeaconV1,
) -> Hash64 {
    let mut preimage = Vec::new();
    preimage.extend_from_slice(&(job_id.len() as u64).to_le_bytes());
    preimage.extend_from_slice(job_id.as_bytes());
    preimage.extend_from_slice(provider_bond.transaction_id.as_byte_slice());
    preimage.extend_from_slice(&provider_bond.index.to_le_bytes());
    preimage.extend_from_slice(root.as_byte_slice());
    preimage.extend_from_slice(&chunk_index.to_le_bytes());
    preimage.extend_from_slice(&beacon.epoch.to_le_bytes());
    preimage.extend_from_slice(beacon.anchor_hash.as_byte_slice());
    blake2b_512_keyed(b"misaka-palw-bridge-v1/da-obligation-id", &preimage)
}

/// Register one provider's obligations for a job — the chunk indices come from the REAL
/// beacon-driven sampler, so neither the provider nor the bridge chooses them.
pub fn register_obligations(
    job_id: &str,
    provider_bond: &str,
    commitment: &DaCommitmentWire,
    beacon: &PalwBuriedBeaconV1,
    now_daa_score: u64,
) -> Result<Vec<DaObligation>, String> {
    let policy = policy();
    let outpoint = parse_outpoint(provider_bond)?;
    let root = commitment.root()?;
    // `leaf_hash` has no off-chain analogue; the object root stands in on both arguments so the
    // draw stays bound to the exact object.
    let indices = palw_da_provider_sample_indices(
        beacon,
        &outpoint,
        &root,
        &root,
        commitment.chunk_count,
        policy.samples_per_provider,
        policy.min_beacon_burial_daa,
    )
    .map_err(|e| format!("da sample: {e:?}"))?;
    Ok(indices
        .into_iter()
        .map(|chunk_index| DaObligation {
            obligation_id_hex: hash64_hex(&obligation_id(job_id, &outpoint, &root, chunk_index, beacon)),
            job_id: job_id.to_string(),
            provider_bond: format_outpoint(&outpoint),
            commitment: commitment.clone(),
            chunk_index,
            beacon_epoch: beacon.epoch,
            created_daa_score: now_daa_score,
            retention_until_daa_score: now_daa_score.saturating_add(policy.retention_daa),
            status: DaObligationStatus::Pending,
        })
        .collect())
}

/// A challenge the bridge issues against one obligation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaChallengeWire {
    pub obligation_id_hex: String,
    pub job_id: String,
    pub provider_bond: String,
    pub chunk_index: u16,
    pub object_root_hex: String,
    pub opened_daa_score: u64,
    pub response_deadline_daa_score: u64,
}

/// A provider's answer: the chunk plus its Merkle path.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DaResponseWire {
    pub obligation_id_hex: String,
    pub provider_bond: String,
    pub chunk_index: u16,
    pub chunk_hex: String,
    pub siblings_hex: Vec<String>,
}

impl DaResponseWire {
    /// Build a response from the object bytes the provider retained.
    pub fn prove(obligation: &DaObligation, object_bytes: &[u8]) -> Result<Self, String> {
        let (chunk, siblings) = chat_chunk_proof(obligation.commitment.object_version, object_bytes, obligation.chunk_index)?;
        Ok(Self {
            obligation_id_hex: obligation.obligation_id_hex.clone(),
            provider_bond: obligation.provider_bond.clone(),
            chunk_index: obligation.chunk_index,
            chunk_hex: bytes_hex(&chunk),
            siblings_hex: siblings.iter().map(hash64_hex).collect(),
        })
    }

    /// Verify against the obligation.
    pub fn verify(&self, obligation: &DaObligation) -> Result<(), String> {
        if self.chunk_index != obligation.chunk_index {
            return Err(format!(
                "response answers chunk {} but the obligation samples chunk {}",
                self.chunk_index, obligation.chunk_index
            ));
        }
        let mut siblings = Vec::with_capacity(self.siblings_hex.len());
        for sibling in &self.siblings_hex {
            siblings.push(parse_hash64(sibling)?);
        }
        verify_chat_chunk(
            &obligation.commitment.root()?,
            obligation.commitment.object_version,
            obligation.commitment.object_len,
            obligation.commitment.chunk_count,
            self.chunk_index,
            &decode_hex(&self.chunk_hex)?,
            &siblings,
        )
    }
}

/// The evidence a failed obligation produces — the node's own unsigned timeout-evidence object
/// (objective: its validity is that the deadline passed). The bridge journals it; submitting it
/// as a `0x3c` transaction is a node-operator action, not something this process does.
pub fn timeout_evidence(network_id: u32, obligation: &DaObligation) -> Result<PalwDaTimeoutEvidenceV1, String> {
    Ok(PalwDaTimeoutEvidenceV1 {
        version: 1,
        network_id,
        challenge_id: parse_hash64(&obligation.obligation_id_hex)?,
        provider_bond: parse_outpoint(&obligation.provider_bond)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object() -> ChatContextObjectV4 {
        ChatContextObjectV4 {
            network_id: 111,
            job_challenge: Hash64::from_bytes([7u8; 64]),
            class_label: b"qi35-serve-metal-v3-phase-d.v1".to_vec(),
            max_new: 256,
            prompt_token_ids: (0..500).collect(),
            output_token_ids: vec![10, 20, 30],
        }
    }

    fn beacon() -> PalwBuriedBeaconV1 {
        PalwBuriedBeaconV1 {
            epoch: 12,
            seed: Hash64::from_bytes([0xab; 64]),
            anchor_hash: Hash64::from_bytes([0xcd; 64]),
            anchor_daa_score: 1_200,
            observed_daa_score: 1_500,
        }
    }

    #[test]
    fn object_codec_roundtrips_and_rejects_junk() {
        let object = object();
        let bytes = object.encode().unwrap();
        assert_eq!(&bytes[..2], &4u16.to_le_bytes(), "version prefix is 4");
        assert_eq!(ChatContextObjectV4::decode_strict(&bytes).unwrap(), object);

        // Trailing bytes, truncation, and wrong version are all refused.
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(ChatContextObjectV4::decode_strict(&extra).is_err());
        assert!(ChatContextObjectV4::decode_strict(&bytes[..bytes.len() - 1]).is_err());
        let mut wrong = bytes.clone();
        wrong[0] = 3;
        assert!(ChatContextObjectV4::decode_strict(&wrong).is_err());
    }

    #[test]
    fn version_four_gives_a_disjoint_root_domain() {
        let bytes = object().encode().unwrap();
        let v4 = chat_da_commitment(4, &bytes).unwrap();
        let v2 = chat_da_commitment(2, &bytes).unwrap();
        assert_ne!(v4.root, v2.root, "the version is bound into the root preimage");
        assert_eq!(v4.object_len as usize, bytes.len());
    }

    /// THE parity proof: on every version consensus supports, this module's tree and the node's
    /// own `palw_receipt_da_commitment` / `palw_receipt_da_chunk_proof` /
    /// `verify_palw_receipt_da_chunk` agree byte-for-byte. Version 4 is therefore the same
    /// algorithm in a domain consensus has not claimed — not a lookalike.
    #[test]
    fn commitment_matches_consensus_for_shared_versions() {
        use kaspa_consensus_core::palw::da::{palw_receipt_da_chunk_proof, palw_receipt_da_commitment, verify_palw_receipt_da_chunk};
        // Sizes that exercise 1, 2, 3 (padded to 4) and 5 (padded to 8) chunks, plus a partial
        // final chunk — the padding domain and the odd-width fold are where a lookalike breaks.
        for len in [1usize, 16_384, 16_385, 40_000, 70_000] {
            let object: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            for version in [1u16, 2, 3] {
                let theirs = palw_receipt_da_commitment(version, &object).unwrap();
                let ours = chat_da_commitment(version, &object).unwrap();
                assert_eq!(ours.root, theirs.root, "root differs at len {len} version {version}");
                assert_eq!(ours.chunk_count, theirs.chunk_count);
                assert_eq!(ours.object_len, theirs.object_len);

                for chunk_index in 0..theirs.chunk_count {
                    let their_proof = palw_receipt_da_chunk_proof(version, &object, chunk_index).unwrap();
                    let (our_chunk, our_siblings) = chat_chunk_proof(version, &object, chunk_index).unwrap();
                    assert_eq!(our_chunk, their_proof.chunk, "chunk bytes differ");
                    assert_eq!(our_siblings, their_proof.siblings, "sibling path differs");
                    // Cross-verify both ways: their proof through our verifier and ours through theirs.
                    verify_chat_chunk(
                        &theirs.root,
                        version,
                        theirs.object_len,
                        theirs.chunk_count,
                        chunk_index,
                        &their_proof.chunk,
                        &their_proof.siblings,
                    )
                    .unwrap();
                    verify_palw_receipt_da_chunk(&ours.root, &their_proof).unwrap();
                }
            }
        }
    }

    #[test]
    fn sampled_chunk_is_provable_and_a_wrong_chunk_is_not() {
        let object = object();
        let bytes = object.encode().unwrap();
        let commitment = DaCommitmentWire::from_commitment(&object.commitment().unwrap());
        let bond = format!("{}:0", "11".repeat(64));
        let obligations = register_obligations("job-1", &bond, &commitment, &beacon(), 1_500).unwrap();
        assert_eq!(obligations.len(), 1, "STRICT_TESTNET samples one chunk per provider");
        let obligation = &obligations[0];
        assert!(obligation.chunk_index < commitment.chunk_count);
        assert_eq!(obligation.retention_until_daa_score, 1_500 + policy().retention_daa);

        // The honest provider proves the sampled chunk.
        let response = DaResponseWire::prove(obligation, &bytes).unwrap();
        response.verify(obligation).unwrap();

        // Answering a different chunk is refused even with a valid proof for that chunk.
        if commitment.chunk_count > 1 {
            let other = (obligation.chunk_index + 1) % commitment.chunk_count;
            let mut wrong = DaResponseWire::prove(&DaObligation { chunk_index: other, ..obligation.clone() }, &bytes).unwrap();
            wrong.obligation_id_hex = obligation.obligation_id_hex.clone();
            assert!(wrong.verify(obligation).is_err(), "wrong chunk index");
        }

        // Corrupted chunk bytes fail the node's verifier.
        let mut tampered = response.clone();
        let mut chunk = decode_hex(&tampered.chunk_hex).unwrap();
        chunk[0] ^= 0xff;
        tampered.chunk_hex = bytes_hex(&chunk);
        assert!(tampered.verify(obligation).is_err(), "tampered chunk must fail the Merkle path");
    }

    #[test]
    fn sampling_is_beacon_bound_and_per_provider() {
        let object = object();
        let commitment = DaCommitmentWire::from_commitment(&object.commitment().unwrap());
        let bond_a = format!("{}:0", "11".repeat(64));
        let bond_b = format!("{}:0", "22".repeat(64));
        let a = register_obligations("job-1", &bond_a, &commitment, &beacon(), 1_500).unwrap();
        let a_again = register_obligations("job-1", &bond_a, &commitment, &beacon(), 1_500).unwrap();
        assert_eq!(a, a_again, "deterministic");
        let b = register_obligations("job-1", &bond_b, &commitment, &beacon(), 1_500).unwrap();
        assert_ne!(a[0].obligation_id_hex, b[0].obligation_id_hex, "per-provider obligations are distinct");

        // A different beacon epoch redraws.
        let mut later = beacon();
        later.epoch = 13;
        later.seed = Hash64::from_bytes([0x99; 64]);
        let redrawn = register_obligations("job-1", &bond_a, &commitment, &later, 1_500).unwrap();
        assert_ne!(a[0].obligation_id_hex, redrawn[0].obligation_id_hex);
    }

    #[test]
    fn unburied_beacon_is_refused() {
        let object = object();
        let commitment = DaCommitmentWire::from_commitment(&object.commitment().unwrap());
        let mut shallow = beacon();
        shallow.observed_daa_score = shallow.anchor_daa_score + 1; // < min_beacon_burial_daa (100)
        let err = register_obligations("job-1", &format!("{}:0", "11".repeat(64)), &commitment, &shallow, 1_500).unwrap_err();
        assert!(err.contains("BeaconNotBuried"), "{err}");
    }

    #[test]
    fn timeout_evidence_names_the_defaulting_bond() {
        let object = object();
        let commitment = DaCommitmentWire::from_commitment(&object.commitment().unwrap());
        let bond = format!("{}:3", "11".repeat(64));
        let obligation = register_obligations("job-1", &bond, &commitment, &beacon(), 1_500).unwrap().remove(0);
        let evidence = timeout_evidence(111, &obligation).unwrap();
        assert_eq!(evidence.network_id, 111);
        assert_eq!(format_outpoint(&evidence.provider_bond), bond);
    }
}
