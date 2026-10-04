//! **RFC-0009 stage B — the canonical evidence manifest, and what a claim's material is checked against.** (Consensus-core, because the provider
//! challenge court (`crate::palw_evidence_court_v1`) must verify an opening against exactly these rules; the transport lives in `misaka-palw-remote`.)
//!
//! **Stage B — evidence the miner does not have to serve.**
//!
//! Today the Panel pulls a claim's material from the producer's own node; a miner that switches its PC off after claiming leaves its
//! claim certifiable by nobody. This module is the pure part of the fix: a canonical
//! [`EvidenceManifestV1`] that names every chunk of a claim's material by hash, a [`StorageReceiptV1`] a provider signs to promise a set
//! of chunks, and and the checks (`verify_chunk`, `verify_claim_binding`) a Panel or the court runs: a byte is accepted only when it hashes to the manifest's
//! entry and the manifest agrees with the CLAIM's own roots.
//!
//! **What this is not.** A provider's signature is never proof of correctness (the bytes are checked against hashes the claim already
//! commits), and a storage receipt is a *promise*, not a proof that the chunk will be there when a future Panel asks. Responsibility for the material stays with the claim's
//! producer unless the claim was made under the provider court's fence, and nobody is slashed for a provider's silence except through that
//! court (`crate::palw_evidence_court_v1`). Nothing in this module reads or writes chain state.
//!
//! **No circularity.** The manifest carries `preclaim_id`, a function of the roots and the job nonce only — never of the claim id or of the
//! manifest. A claim id may later commit to `manifest_id` because the manifest never commits to the claim id.

use crate::Hash64;
use crate::tx::TransactionOutpoint;

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn put_len(state: &mut blake2b_simd::State, bytes: &[u8]) {
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(bytes);
}

pub const EVIDENCE_MANIFEST_VERSION: u16 = 1;
pub const DOMAIN_MANIFEST: &[u8] = b"misaka-palw/evidence/manifest/v1";
pub const DOMAIN_PRECLAIM: &[u8] = b"misaka-palw/evidence/preclaim/v1";
pub const DOMAIN_CHUNK: &[u8] = b"misaka-palw/evidence/chunk/v1";
pub const DOMAIN_STORAGE_RECEIPT: &[u8] = b"misaka-palw/evidence/storage-receipt/v1";
pub const DOMAIN_FETCH_ORDER: &[u8] = b"misaka-palw/evidence/fetch-order/v1";
pub const DOMAIN_CHALLENGE: &[u8] = b"misaka-palw/evidence/provider-challenge/v1";

/// `encoding == 1`: the bytes are served raw (what the retained material file holds).
pub const ENCODING_RAW: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ChunkEntryV1 {
    pub index: u32,
    pub len: u32,
    /// [`chunk_hash_v1`] of the served bytes.
    pub hash: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct EvidenceManifestV1 {
    pub version: u16,
    pub network_domain: Hash64,
    pub preclaim_id: Hash64,
    pub trace_root: Hash64,
    pub output_root: Hash64,
    pub execution_root: Hash64,
    pub trace_chunk_count: u32,
    pub retention_until_daa: u64,
    pub encoding: u8,
    pub max_expanded_bytes: u64,
    pub chunks: Vec<ChunkEntryV1>,
}

/// Limits fixed per fence from measurement and attack budget (RFC §4.1: "必要な複製数、byte cap … は実測と攻撃予算から fence ごとに固定").
/// The defaults are the library's, not consensus's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManifestLimits {
    pub max_chunks: u32,
    pub max_chunk_bytes: u32,
    pub max_total_bytes: u64,
}

impl Default for ManifestLimits {
    fn default() -> Self {
        Self { max_chunks: 4_096, max_chunk_bytes: 1 << 20, max_total_bytes: 1 << 32 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvidenceError {
    #[error("unsupported manifest version {0}")]
    UnsupportedVersion(u16),
    #[error("unsupported encoding {0}")]
    UnsupportedEncoding(u8),
    #[error("the manifest names no chunks")]
    NoChunks,
    #[error("chunk {got} is out of order: the manifest must list 0, 1, 2, … without gaps (expected {expected})")]
    ChunkOrder { expected: u32, got: u32 },
    #[error("{0} chunks exceed the limit of {1}")]
    TooManyChunks(usize, u32),
    #[error("chunk {index} is {len} bytes, above the {max} limit")]
    ChunkTooLarge { index: u32, len: u32, max: u32 },
    #[error("the chunks total {total} bytes, above the manifest's declared maximum {max}")]
    TotalAboveDeclared { total: u64, max: u64 },
    #[error("the manifest declares {declared} bytes, above the limit of {limit}")]
    DeclaredAboveLimit { declared: u64, limit: u64 },
    #[error("the manifest names {got} trace chunks but the claim commits {expected}")]
    TraceChunkCountMismatch { got: u32, expected: u32 },
    #[error("the manifest's {0} root is not the claim's")]
    RootMismatch(&'static str),
    #[error("the manifest is for another network")]
    NetworkMismatch,
    #[error("the manifest does not retain past DAA {needed} (it promises {promised})")]
    RetentionTooShort { needed: u64, promised: u64 },
    #[error("chunk {index} is not in the manifest")]
    UnknownChunk { index: u32 },
    #[error("chunk {index} has the wrong length ({got}, the manifest says {expected})")]
    ChunkLength { index: u32, got: usize, expected: u32 },
    #[error("chunk {index} does not hash to the manifest's entry")]
    ChunkHashMismatch { index: u32 },
    #[error("the manifest's preclaim id is not the one its roots and nonce derive")]
    PreclaimMismatch,
}

/// The identity a claim may commit to. Total over every field, in canonical borsh.
pub fn manifest_id_v1(m: &EvidenceManifestV1) -> Hash64 {
    let mut s = keyed(DOMAIN_MANIFEST);
    put_len(&mut s, &borsh::to_vec(m).expect("borsh-serializable"));
    finish(s)
}

/// The identity a manifest carries INSTEAD of a claim id (which does not exist yet): the roots and the job nonce, under the network and
/// the executor bond. Two different executions cannot share it; it never depends on the manifest or the claim.
pub fn preclaim_id_v1(
    network_domain: Hash64,
    executor_bond: &TransactionOutpoint,
    job_nonce: &[u8; 32],
    trace_root: Hash64,
    output_root: Hash64,
    execution_root: Hash64,
) -> Hash64 {
    let mut s = keyed(DOMAIN_PRECLAIM);
    s.update(network_domain.as_bytes().as_slice());
    s.update(executor_bond.transaction_id.as_bytes().as_slice());
    s.update(&executor_bond.index.to_le_bytes());
    s.update(job_nonce);
    for root in [trace_root, output_root, execution_root] {
        s.update(root.as_bytes().as_slice());
    }
    finish(s)
}

/// The hash a chunk entry holds: bound to the network, the preclaim id and the chunk's position, so one chunk cannot be passed off as
/// another's or as another execution's.
pub fn chunk_hash_v1(network_domain: Hash64, preclaim_id: Hash64, index: u32, bytes: &[u8]) -> Hash64 {
    let mut s = keyed(DOMAIN_CHUNK);
    s.update(network_domain.as_bytes().as_slice());
    s.update(preclaim_id.as_bytes().as_slice());
    s.update(&index.to_le_bytes());
    put_len(&mut s, bytes);
    finish(s)
}

/// What the CLAIM commits (the fields of the on-chain commitment the manifest must agree with).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimRoots {
    pub network_domain: Hash64,
    pub trace_root: Hash64,
    pub output_root: Hash64,
    pub execution_root: Hash64,
    pub trace_chunk_count: u32,
    /// The DAA the claim obliges its material to be retained until.
    pub retention_deadline: u64,
}

impl EvidenceManifestV1 {
    /// Canonical shape: version, encoding, contiguous chunk indices from 0, per-chunk and total caps.
    pub fn validate_shape(&self, limits: &ManifestLimits) -> Result<(), EvidenceError> {
        if self.version != EVIDENCE_MANIFEST_VERSION {
            return Err(EvidenceError::UnsupportedVersion(self.version));
        }
        if self.encoding != ENCODING_RAW {
            return Err(EvidenceError::UnsupportedEncoding(self.encoding));
        }
        if self.chunks.is_empty() {
            return Err(EvidenceError::NoChunks);
        }
        if self.chunks.len() > limits.max_chunks as usize {
            return Err(EvidenceError::TooManyChunks(self.chunks.len(), limits.max_chunks));
        }
        if self.max_expanded_bytes > limits.max_total_bytes {
            return Err(EvidenceError::DeclaredAboveLimit { declared: self.max_expanded_bytes, limit: limits.max_total_bytes });
        }
        let mut total: u64 = 0;
        for (expected, chunk) in self.chunks.iter().enumerate() {
            let expected = expected as u32;
            if chunk.index != expected {
                return Err(EvidenceError::ChunkOrder { expected, got: chunk.index });
            }
            if chunk.len > limits.max_chunk_bytes {
                return Err(EvidenceError::ChunkTooLarge { index: chunk.index, len: chunk.len, max: limits.max_chunk_bytes });
            }
            total = total.saturating_add(chunk.len as u64);
        }
        if total > self.max_expanded_bytes {
            return Err(EvidenceError::TotalAboveDeclared { total, max: self.max_expanded_bytes });
        }
        Ok(())
    }

    /// The manifest says what the CLAIM says: same network, same three roots, enough trace chunks, retained long enough. A manifest that
    /// disagrees with the claim is a different execution's evidence, whoever signed for it.
    pub fn verify_claim_binding(&self, claim: &ClaimRoots) -> Result<(), EvidenceError> {
        if self.network_domain != claim.network_domain {
            return Err(EvidenceError::NetworkMismatch);
        }
        if self.trace_root != claim.trace_root {
            return Err(EvidenceError::RootMismatch("trace"));
        }
        if self.output_root != claim.output_root {
            return Err(EvidenceError::RootMismatch("output"));
        }
        if self.execution_root != claim.execution_root {
            return Err(EvidenceError::RootMismatch("execution"));
        }
        if self.trace_chunk_count != claim.trace_chunk_count {
            return Err(EvidenceError::TraceChunkCountMismatch { got: self.trace_chunk_count, expected: claim.trace_chunk_count });
        }
        if self.retention_until_daa < claim.retention_deadline {
            return Err(EvidenceError::RetentionTooShort { needed: claim.retention_deadline, promised: self.retention_until_daa });
        }
        Ok(())
    }

    /// The preclaim id this manifest carries is the one its roots and the job nonce derive.
    pub fn verify_preclaim(&self, executor_bond: &TransactionOutpoint, job_nonce: &[u8; 32]) -> Result<(), EvidenceError> {
        let expected =
            preclaim_id_v1(self.network_domain, executor_bond, job_nonce, self.trace_root, self.output_root, self.execution_root);
        (expected == self.preclaim_id).then_some(()).ok_or(EvidenceError::PreclaimMismatch)
    }

    /// One fetched chunk against the manifest: known index, exact length, hash equal to the entry. Never trusts who served it.
    pub fn verify_chunk(&self, index: u32, bytes: &[u8]) -> Result<(), EvidenceError> {
        let entry = self.chunks.get(index as usize).filter(|c| c.index == index).ok_or(EvidenceError::UnknownChunk { index })?;
        if bytes.len() != entry.len as usize {
            return Err(EvidenceError::ChunkLength { index, got: bytes.len(), expected: entry.len });
        }
        if chunk_hash_v1(self.network_domain, self.preclaim_id, index, bytes) != entry.hash {
            return Err(EvidenceError::ChunkHashMismatch { index });
        }
        Ok(())
    }

    /// Build a manifest from raw chunks (what the executor does once, before claiming). `chunks` are the served bytes in order.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        network_domain: Hash64,
        executor_bond: &TransactionOutpoint,
        job_nonce: &[u8; 32],
        trace_root: Hash64,
        output_root: Hash64,
        execution_root: Hash64,
        trace_chunk_count: u32,
        retention_until_daa: u64,
        chunks: &[Vec<u8>],
    ) -> Self {
        let preclaim_id = preclaim_id_v1(network_domain, executor_bond, job_nonce, trace_root, output_root, execution_root);
        let entries: Vec<ChunkEntryV1> = chunks
            .iter()
            .enumerate()
            .map(|(i, bytes)| ChunkEntryV1 {
                index: i as u32,
                len: bytes.len() as u32,
                hash: chunk_hash_v1(network_domain, preclaim_id, i as u32, bytes),
            })
            .collect();
        let total: u64 = chunks.iter().map(|c| c.len() as u64).sum();
        Self {
            version: EVIDENCE_MANIFEST_VERSION,
            network_domain,
            preclaim_id,
            trace_root,
            output_root,
            execution_root,
            trace_chunk_count,
            retention_until_daa,
            encoding: ENCODING_RAW,
            max_expanded_bytes: total,
            chunks: entries,
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------------------------------------------------------------

/// A provider's promise to hold a set of chunks until a DAA. Signed under the provider's own key; the signature is never evidence the
/// bytes are right (those are checked against the manifest) — only that THIS provider said so, which is what a future court needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageReceiptV1 {
    pub network_domain: Hash64,
    pub provider_id: Vec<u8>,
    pub manifest_id: Hash64,
    /// Half-open ranges of chunk indices held.
    pub chunk_ranges: Vec<(u32, u32)>,
    pub retain_until_daa: u64,
    pub signature: Vec<u8>,
}

impl StorageReceiptV1 {
    pub fn signing_digest(&self) -> Hash64 {
        let mut s = keyed(DOMAIN_STORAGE_RECEIPT);
        s.update(self.network_domain.as_bytes().as_slice());
        put_len(&mut s, &self.provider_id);
        s.update(self.manifest_id.as_bytes().as_slice());
        s.update(&(self.chunk_ranges.len() as u32).to_le_bytes());
        for (lo, hi) in &self.chunk_ranges {
            s.update(&lo.to_le_bytes());
            s.update(&hi.to_le_bytes());
        }
        s.update(&self.retain_until_daa.to_le_bytes());
        finish(s)
    }

    pub fn holds(&self, index: u32) -> bool {
        self.chunk_ranges.iter().any(|(lo, hi)| *lo <= index && index < *hi)
    }

    /// Does the receipt cover every chunk of `manifest` through `needed_until_daa`, for this manifest and network, under a signature
    /// `verify(provider_id, message, signature)` accepts?
    pub fn covers(
        &self,
        manifest: &EvidenceManifestV1,
        needed_until_daa: u64,
        verify: impl Fn(&[u8], &[u8], &[u8]) -> bool,
    ) -> bool {
        self.network_domain == manifest.network_domain
            && self.manifest_id == manifest_id_v1(manifest)
            && self.retain_until_daa >= needed_until_daa
            && (0..manifest.chunks.len() as u32).all(|i| self.holds(i))
            && verify(&self.provider_id, self.signing_digest().as_bytes().as_slice(), &self.signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn bond() -> TransactionOutpoint {
        TransactionOutpoint::new(h(0xB0), 1)
    }
    fn chunks() -> Vec<Vec<u8>> {
        vec![vec![1u8; 100], vec![2u8; 50], vec![3u8; 7]]
    }
    fn manifest() -> EvidenceManifestV1 {
        EvidenceManifestV1::build(h(9), &bond(), &[5u8; 32], h(1), h(2), h(3), 3, 10_000, &chunks())
    }
    fn claim() -> ClaimRoots {
        ClaimRoots { network_domain: h(9), trace_root: h(1), output_root: h(2), execution_root: h(3), trace_chunk_count: 3, retention_deadline: 9_000 }
    }

    #[test]
    fn a_built_manifest_is_canonical_and_agrees_with_its_claim() {
        let m = manifest();
        m.validate_shape(&ManifestLimits::default()).unwrap();
        m.verify_claim_binding(&claim()).unwrap();
        m.verify_preclaim(&bond(), &[5u8; 32]).unwrap();
        assert_eq!(m.max_expanded_bytes, 157);
        // The id is stable and total: changing anything changes it.
        let id = manifest_id_v1(&m);
        assert_eq!(id, manifest_id_v1(&manifest()));
        let mut other = m.clone();
        other.retention_until_daa += 1;
        assert_ne!(manifest_id_v1(&other), id);
    }

    #[test]
    fn the_preclaim_id_has_no_circularity_and_binds_what_distinguishes_an_execution() {
        let base = preclaim_id_v1(h(9), &bond(), &[5u8; 32], h(1), h(2), h(3));
        assert_eq!(base, manifest().preclaim_id);
        // It does not mention the manifest: rebuilding the manifest with other chunk bytes leaves it unchanged.
        let other_bytes = EvidenceManifestV1::build(h(9), &bond(), &[5u8; 32], h(1), h(2), h(3), 3, 10_000, &[vec![9u8; 4]]);
        assert_eq!(other_bytes.preclaim_id, base);
        for different in [
            preclaim_id_v1(h(8), &bond(), &[5u8; 32], h(1), h(2), h(3)),
            preclaim_id_v1(h(9), &TransactionOutpoint::new(h(0xB0), 2), &[5u8; 32], h(1), h(2), h(3)),
            preclaim_id_v1(h(9), &bond(), &[6u8; 32], h(1), h(2), h(3)),
            preclaim_id_v1(h(9), &bond(), &[5u8; 32], h(4), h(2), h(3)),
            preclaim_id_v1(h(9), &bond(), &[5u8; 32], h(1), h(4), h(3)),
            preclaim_id_v1(h(9), &bond(), &[5u8; 32], h(1), h(2), h(4)),
        ] {
            assert_ne!(different, base);
        }
        assert_eq!(manifest().verify_preclaim(&TransactionOutpoint::new(h(0xB0), 2), &[5u8; 32]), Err(EvidenceError::PreclaimMismatch));
    }

    #[test]
    fn shape_refusals_are_named() {
        let limits = ManifestLimits::default();
        let mut m = manifest();
        m.version = 2;
        assert_eq!(m.validate_shape(&limits), Err(EvidenceError::UnsupportedVersion(2)));
        let mut m = manifest();
        m.encoding = 9;
        assert_eq!(m.validate_shape(&limits), Err(EvidenceError::UnsupportedEncoding(9)));
        let mut m = manifest();
        m.chunks.clear();
        assert_eq!(m.validate_shape(&limits), Err(EvidenceError::NoChunks));
        let mut m = manifest();
        m.chunks.swap(0, 1);
        assert!(matches!(m.validate_shape(&limits), Err(EvidenceError::ChunkOrder { expected: 0, got: 1 })));
        let mut m = manifest();
        m.chunks[1].index = 5;
        assert!(matches!(m.validate_shape(&limits), Err(EvidenceError::ChunkOrder { expected: 1, got: 5 })));
        assert!(matches!(
            manifest().validate_shape(&ManifestLimits { max_chunks: 2, ..limits }),
            Err(EvidenceError::TooManyChunks(3, 2))
        ));
        assert!(matches!(
            manifest().validate_shape(&ManifestLimits { max_chunk_bytes: 60, ..limits }),
            Err(EvidenceError::ChunkTooLarge { index: 0, .. })
        ));
        let mut m = manifest();
        m.max_expanded_bytes = 10;
        assert!(matches!(m.validate_shape(&limits), Err(EvidenceError::TotalAboveDeclared { .. })));
        assert!(matches!(
            manifest().validate_shape(&ManifestLimits { max_total_bytes: 100, ..limits }),
            Err(EvidenceError::DeclaredAboveLimit { .. })
        ));
    }

    #[test]
    fn a_manifest_that_disagrees_with_the_claim_is_another_executions_evidence() {
        let mut c = claim();
        c.trace_root = h(0x77);
        assert_eq!(manifest().verify_claim_binding(&c), Err(EvidenceError::RootMismatch("trace")));
        let mut c = claim();
        c.output_root = h(0x77);
        assert_eq!(manifest().verify_claim_binding(&c), Err(EvidenceError::RootMismatch("output")));
        let mut c = claim();
        c.execution_root = h(0x77);
        assert_eq!(manifest().verify_claim_binding(&c), Err(EvidenceError::RootMismatch("execution")));
        let mut c = claim();
        c.network_domain = h(0x77);
        assert_eq!(manifest().verify_claim_binding(&c), Err(EvidenceError::NetworkMismatch));
        let mut c = claim();
        c.trace_chunk_count = 4;
        assert!(matches!(manifest().verify_claim_binding(&c), Err(EvidenceError::TraceChunkCountMismatch { got: 3, expected: 4 })));
        let mut c = claim();
        c.retention_deadline = 10_001;
        assert!(matches!(manifest().verify_claim_binding(&c), Err(EvidenceError::RetentionTooShort { .. })));
    }

    #[test]
    fn a_chunk_is_accepted_only_if_it_hashes_to_its_own_entry_at_its_own_position() {
        let m = manifest();
        let c = chunks();
        for (i, bytes) in c.iter().enumerate() {
            m.verify_chunk(i as u32, bytes).unwrap();
        }
        assert!(matches!(m.verify_chunk(0, &c[1]), Err(EvidenceError::ChunkLength { .. })), "another chunk's bytes at this position");
        let mut flipped = c[0].clone();
        flipped[0] ^= 1;
        assert_eq!(m.verify_chunk(0, &flipped), Err(EvidenceError::ChunkHashMismatch { index: 0 }));
        assert_eq!(m.verify_chunk(7, &c[0]), Err(EvidenceError::UnknownChunk { index: 7 }));
        // The same bytes under another execution's manifest do not verify: the hash binds the preclaim id.
        let foreign = EvidenceManifestV1::build(h(9), &bond(), &[6u8; 32], h(1), h(2), h(3), 3, 10_000, &c);
        assert_ne!(foreign.chunks[0].hash, m.chunks[0].hash);
        // Two equal-length chunks with equal bytes at different positions hash differently (position-bound).
        let twin = EvidenceManifestV1::build(h(9), &bond(), &[5u8; 32], h(1), h(2), h(3), 2, 10_000, &[vec![1u8; 8], vec![1u8; 8]]);
        assert_ne!(twin.chunks[0].hash, twin.chunks[1].hash);
    }

    #[test]
    fn a_storage_receipt_is_a_promise_for_this_manifest_this_network_and_a_long_enough_time() {
        let m = manifest();
        let mut r = StorageReceiptV1 {
            network_domain: h(9),
            provider_id: b"prov-1".to_vec(),
            manifest_id: manifest_id_v1(&m),
            chunk_ranges: vec![(0, 3)],
            retain_until_daa: 20_000,
            signature: vec![],
        };
        r.signature = r.signing_digest().as_bytes().as_slice()[..8].to_vec();
        let verify = |id: &[u8], msg: &[u8], sig: &[u8]| id == b"prov-1" && sig == &msg[..8];
        assert!(r.covers(&m, 10_000, verify));
        assert!(!r.covers(&m, 20_001, verify), "not long enough");
        let mut partial = r.clone();
        partial.chunk_ranges = vec![(0, 2)];
        partial.signature = partial.signing_digest().as_bytes().as_slice()[..8].to_vec();
        assert!(!partial.covers(&m, 10_000, verify), "a receipt for part of the chunks does not cover the manifest");
        let mut other_manifest = m.clone();
        other_manifest.retention_until_daa += 1;
        assert!(!r.covers(&other_manifest, 10_000, verify), "another manifest");
        let mut forged = r.clone();
        forged.retain_until_daa = 99_999; // changes what the signature covers
        assert!(!forged.covers(&m, 10_000, verify));
    }

}
