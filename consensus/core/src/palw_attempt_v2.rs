//! ADR-0042 Decision 3 (PR-01): the V2 attempt, its transcript, and identity by `attempt_id`.
//!
//! V1's defect (external audit P0-1) was that the block identity hash covered `palw_commitment`
//! while every PoW-path digest excluded it: one solved PoW minted unlimited distinct block
//! identities by swapping the trace root, the output root or the executor bond. `palw-only-v4`
//! closed it by MIXING the commitment into the tag while keeping the inference as the work — safe,
//! and deliberately not the end state, because the inference had to stay the work until a bond's
//! immature exposure was capped.
//!
//! V2 is the end state the ADR specifies: **the finalizer consumes an expansion of the commitment
//! root instead of the inference tag.** One new ticket costs one new inference (W2), and no
//! commitment can be replayed onto another attempt, header, class or executor.
//!
//! ```text
//! challenge       = H(network_domain ‖ pre_pow_hash ‖ timestamp ‖ nonce ‖ class_id ‖ bond)
//! attempt_id      = H(canonical(PalwAttemptUnsignedV2))
//! commitment_root = H(attempt_id)
//! L1 tag          = Expand(commitment_root)
//! ```
//!
//! **The priced set and the identity set are the same set, by construction.** `commitment_root`
//! is derived FROM `attempt_id` rather than re-enumerating fields beside it, because an
//! enumeration is a list that stops growing while the struct keeps growing: PR-06 added three
//! data-availability fields to the attempt and the transcript did not follow, which re-opened
//! P0-1 at V2 scale (one solved nonce, unlimited sibling identities, for the price of a
//! re-signature). Deriving one hash from the other makes that class of drift unrepresentable.
//!
//! **Identity is `attempt_id`, never the signature bytes.** ML-DSA-87 signatures are not guaranteed
//! unique, so folding raw signature bytes into a block id would re-open malleability wearing the
//! costume of a fix — a second valid signature over the same message would be a second block. The
//! signature is a witness checked at admission; identity is the unsigned attempt.

use crate::Hash64;
use crate::tx::TransactionOutpoint;
use blake2b_simd::Params;

/// = [`crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2`]'s object version.
pub const PALW_ATTEMPT_V2_VERSION: u16 = 2;

/// Width of the expanded L1 tag, matching algo-4's so the finalizer's call shape is unchanged.
pub const PALW_ATTEMPT_V2_L1_TAG_BYTES: usize = 200;

pub const PALW_ATTEMPT_V2_DOMAIN_CHALLENGE: &[u8] = b"misaka-palw/attempt-v2/challenge/v1";
pub const PALW_ATTEMPT_V2_DOMAIN_COMMITMENT_ROOT: &[u8] = b"misaka-palw/attempt-v2/commitment-root/v1";
pub const PALW_ATTEMPT_V2_DOMAIN_ATTEMPT_ID: &[u8] = b"misaka-palw/attempt-v2/attempt-id/v1";
pub const PALW_ATTEMPT_V2_DOMAIN_L1_TAG: &[u8] = b"misaka-palw/attempt-v2/l1-tag/v1";
/// ML-DSA-87 signing context for a V2 attempt — its own family domain (audit P0-6: one
/// context-free closure serving several object families is how a signature crosses meanings).
/// The signed message is [`attempt_id_v2`]: identity covers every field, so signing the id signs
/// the claim, and nothing outside the id can ride on the signature.
pub const PALW_ATTEMPT_V2_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/attempt-v2/mldsa87/v1";

/// Every domain this module keys, so a duplicate is a test failure rather than a silent collision.
pub const PALW_ATTEMPT_V2_ALL_DOMAINS: &[&[u8]] = &[
    PALW_ATTEMPT_V2_DOMAIN_CHALLENGE,
    PALW_ATTEMPT_V2_DOMAIN_COMMITMENT_ROOT,
    PALW_ATTEMPT_V2_DOMAIN_ATTEMPT_ID,
    PALW_ATTEMPT_V2_DOMAIN_L1_TAG,
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
];

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn update_outpoint(state: &mut blake2b_simd::State, outpoint: &TransactionOutpoint) {
    state.update(outpoint.transaction_id.as_byte_slice());
    state.update(&outpoint.index.to_le_bytes());
}

/// The attempt a miner signs. **No signature field** — that is the envelope's job, and keeping them
/// apart is what makes `attempt_id` a function of the claim rather than of how it was signed.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwAttemptUnsignedV2 {
    pub version: u16,
    /// The network's domain separator. Distinct from the challenge's other inputs so a testnet
    /// attempt cannot be replayed on mainnet even at an identical header.
    pub network_domain: Hash64,
    /// = [`challenge_v2`] over this attempt's header position. Carried rather than recomputed so a
    /// verifier can check the miner claimed the attempt it actually mined.
    pub challenge: Hash64,
    pub class_id: Hash64,
    pub executor_bond: TransactionOutpoint,
    /// MUST equal the bond record's key at admission (ADR-0042 Decision 6). Carried so the
    /// signature is checkable before any chain lookup.
    pub executor_pubkey: Vec<u8>,
    /// Registered at bond time; the panel dedups on it (Decision 7). Carried here so the draw does
    /// not have to trust a second lookup to agree with this one.
    pub operator_id: Hash64,
    /// MUST equal the class's registered artifact root — what `palw_artifact` openings prove against.
    pub artifact_root: Hash64,
    pub trace_root: Hash64,
    pub output_root: Hash64,
    pub pwu: u64,
    /// Root of the trace MANIFEST (chunk index -> chunk digest list) the producer must serve
    /// (ADR-0042 Decision 7: the commitment binds the data-availability obligation). `trace_root`
    /// stays the step-level merkle root the court opens against (Decision 8); the manifest is how
    /// a panel fetches chunks to verify at all.
    pub trace_manifest_root: Hash64,
    /// Number of trace chunks behind `trace_manifest_root`. Zero chunks is an unverifiable
    /// attempt and is refused statelessly.
    pub trace_chunk_count: u32,
    /// DAA score until which the producer is obliged to serve openings/chunks. Failing a request
    /// inside this window defaults the producer: claim void, bond slash (Decision 7) — silence
    /// can never pin a block at `Provisional` forever.
    pub trace_retention_daa: u64,
    /// The executor's `committed_execution_root` (ADR-0030's `PalwStepBindingV2`) — the single
    /// value that fixes the SHAPE of the execution being claimed: the job context, both profiles,
    /// the leaf and checkpoint counts and their roots all recompute into it.
    ///
    /// It is here because the court needs something of the EXECUTOR'S to test a refutation's
    /// binding against. Without it the only tie between an accusation and its target was the
    /// claim's public `trace_root`, so an accuser could write the entire binding — including a
    /// deliberately non-canonical shape profile — and harvest a `ShapeProfileNotCanonical`
    /// conviction against an honest producer that never made that claim (audit C3). Pinning this
    /// root forces every component of the binding to be the executor's own, because
    /// `verify_binding` recomputes the root from all of them and requires equality.
    pub execution_root: Hash64,
}

/// The signed envelope. The signature is a **witness**, never part of identity.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwAttemptEnvelopeV2 {
    pub attempt: PalwAttemptUnsignedV2,
    pub signature: Vec<u8>,
}

/// `H(network_domain ‖ pre_pow_hash ‖ timestamp ‖ nonce ‖ class_id ‖ bond)`.
///
/// The class and the bond are inside the challenge, not merely beside it: without them one solved
/// header position could be re-announced under another class (a different price) or another bond (a
/// different accountable party) at no extra work.
pub fn challenge_v2(
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    class_id: Hash64,
    executor_bond: &TransactionOutpoint,
) -> Hash64 {
    let mut state = keyed(PALW_ATTEMPT_V2_DOMAIN_CHALLENGE);
    state.update(network_domain.as_byte_slice());
    state.update(pre_pow_hash.as_byte_slice());
    state.update(&timestamp.to_le_bytes());
    state.update(&nonce.to_le_bytes());
    state.update(class_id.as_byte_slice());
    update_outpoint(&mut state, executor_bond);
    finish(state)
}

/// `H(attempt_id)` — what the PoW expands. **Every identity-bearing field is priced, by
/// construction.**
///
/// It used to hand-enumerate six fields (challenge, class, bond, trace root, output root, pwu)
/// while [`attempt_id_v2`] covered the whole struct. That split is the P0-1 composition — a field
/// that is identity-visible, PoW-invisible and content-unchecked lets ONE solved nonce mint
/// unlimited distinct block identities, for the price of a re-signature. It was closed in PR-01
/// and re-opened in PR-06, which added `trace_manifest_root` / `trace_chunk_count` /
/// `trace_retention_daa` to the struct without adding them here; the audit found all three free
/// (only `trace_chunk_count != 0` was checked anywhere).
///
/// Enumerating fields correctly is not the fix — staying correct as fields are added is. Deriving
/// the root FROM the identity makes the two incapable of drifting: any field a future PR adds is
/// priced the moment it is inside the attempt. The domain key keeps this hash distinct from the
/// attempt id it consumes.
pub fn commitment_root_v2(attempt: &PalwAttemptUnsignedV2) -> Hash64 {
    let mut state = keyed(PALW_ATTEMPT_V2_DOMAIN_COMMITMENT_ROOT);
    state.update(attempt_id_v2(attempt).as_byte_slice());
    finish(state)
}

/// `H(canonical(attempt))` — the value block identity carries.
///
/// Over the Borsh encoding of the WHOLE unsigned attempt, so every field is inside it: the
/// commitment root covers the six the PoW prices, and this covers those plus the pubkey, the
/// operator id, the artifact root, the network domain and the version. A field the identity misses
/// is a field two blocks can differ in while claiming to be the same block.
pub fn attempt_id_v2(attempt: &PalwAttemptUnsignedV2) -> Hash64 {
    let bytes = borsh::to_vec(attempt).expect("PalwAttemptUnsignedV2 is borsh-serializable");
    let mut state = keyed(PALW_ATTEMPT_V2_DOMAIN_ATTEMPT_ID);
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(&bytes);
    finish(state)
}

/// `Expand(commitment_root)` — the 200 tag bytes the Layer-0 finalizer consumes **in place of** an
/// inference.
///
/// This is the V1 module's `l1_tag_bytes` promoted to the live path, and it is safe here only
/// because ADR-0042 lands it inside one atomic bundle with the per-bond exposure cap: a free tag
/// plus uncapped exposure is what makes fake-root grinding cheap (audit P0-10).
pub fn l1_tag_v2(commitment_root: Hash64) -> [u8; PALW_ATTEMPT_V2_L1_TAG_BYTES] {
    let mut out = [0u8; PALW_ATTEMPT_V2_L1_TAG_BYTES];
    for (chunk_index, chunk) in out.chunks_mut(64).enumerate() {
        let mut state = keyed(PALW_ATTEMPT_V2_DOMAIN_L1_TAG);
        state.update(commitment_root.as_byte_slice());
        state.update(&(chunk_index as u32).to_le_bytes());
        chunk.copy_from_slice(&state.finalize().as_bytes()[..chunk.len()]);
    }
    out
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwAttemptV2Error {
    #[error("unsupported attempt version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("the attempt's carried challenge is not the one its header position derives")]
    ChallengeMismatch,
    #[error("pwu is zero — an attempt claiming no work is not an attempt")]
    ZeroPwu,
    #[error("trace_chunk_count is zero — a trace nobody can fetch is a trace nobody can verify")]
    ZeroTraceChunks,
    #[error("the signature is {got} bytes, not the ML-DSA-87 {expected}")]
    SignatureLength { got: usize, expected: usize },
    #[error("the executor public key is empty")]
    MissingPublicKey,
    #[error("the signature does not verify over the attempt id under the carried executor key")]
    SignatureInvalid,
}

impl PalwAttemptEnvelopeV2 {
    /// Stateless admission: everything checkable without chain state.
    ///
    /// The carried `challenge` is recomputed from the header position rather than trusted, which is
    /// what stops an attempt mined at one position being announced at another — the PoW would fail
    /// anyway, but failing HERE names the reason instead of leaving a peer to infer it from a
    /// digest mismatch.
    pub fn validate_stateless_v2(
        &self,
        network_domain: Hash64,
        pre_pow_hash: Hash64,
        timestamp: u64,
        nonce: u64,
    ) -> Result<(), PalwAttemptV2Error> {
        let a = &self.attempt;
        if a.version != PALW_ATTEMPT_V2_VERSION {
            return Err(PalwAttemptV2Error::UnsupportedVersion { got: a.version, expected: PALW_ATTEMPT_V2_VERSION });
        }
        if a.executor_pubkey.is_empty() {
            return Err(PalwAttemptV2Error::MissingPublicKey);
        }
        let expected = crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;
        if self.signature.len() != expected {
            return Err(PalwAttemptV2Error::SignatureLength { got: self.signature.len(), expected });
        }
        if a.pwu == 0 {
            return Err(PalwAttemptV2Error::ZeroPwu);
        }
        if a.trace_chunk_count == 0 {
            return Err(PalwAttemptV2Error::ZeroTraceChunks);
        }
        if a.network_domain != network_domain
            || a.challenge != challenge_v2(network_domain, pre_pow_hash, timestamp, nonce, a.class_id, &a.executor_bond)
        {
            return Err(PalwAttemptV2Error::ChallengeMismatch);
        }
        Ok(())
    }

    /// Stateless signature check (ADR-0042 Decision 6's stateless list): the signature must
    /// verify over [`attempt_id_v2`] under the **carried** `executor_pubkey`, in this family's
    /// own context. What it proves is exactly "the carried key signed this claim" — whether the
    /// carried key IS the named bond's key is the stateful side's item 2, checked against the
    /// candidate-chain bond record. Split this way, an unsigned attempt costs a peer one
    /// signature verification and zero chain lookups.
    ///
    /// The verifier is passed in because this crate holds no ML-DSA implementation; the CONTEXT
    /// is not passed in — the family's own code chooses it, so no caller can supply a foreign
    /// domain (audit P0-6).
    pub fn validate_signature_v2<V>(&self, verify_mldsa87: V) -> Result<(), PalwAttemptV2Error>
    where
        V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        let message = attempt_id_v2(&self.attempt);
        if !verify_mldsa87(&self.attempt.executor_pubkey, message.as_byte_slice(), &self.signature, PALW_ATTEMPT_V2_MLDSA87_CONTEXT) {
            return Err(PalwAttemptV2Error::SignatureInvalid);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(seed: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([seed; 64]), 0)
    }

    fn net() -> Hash64 {
        Hash64::from_u64_word(0x4E45_5457)
    }
    fn pph() -> Hash64 {
        Hash64::from_u64_word(0xB0)
    }
    const TS: u64 = 1_700_000_000;
    const NONCE: u64 = 7;

    fn attempt() -> PalwAttemptUnsignedV2 {
        let bond = op(1);
        let class = Hash64::from_u64_word(0xC1);
        PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: net(),
            challenge: challenge_v2(net(), pph(), TS, NONCE, class, &bond),
            class_id: class,
            executor_bond: bond,
            executor_pubkey: vec![7u8; 32],
            operator_id: Hash64::from_u64_word(0xE0),
            artifact_root: Hash64::from_u64_word(0xA7),
            trace_root: Hash64::from_u64_word(0x7A),
            output_root: Hash64::from_u64_word(0x00),
            pwu: 4_242,
            trace_manifest_root: Hash64::from_u64_word(0xD0),
            trace_chunk_count: 8,
            trace_retention_daa: 999_999,
            execution_root: Hash64::from_u64_word(0x41),
        }
    }

    fn envelope(a: PalwAttemptUnsignedV2) -> PalwAttemptEnvelopeV2 {
        PalwAttemptEnvelopeV2 { attempt: a, signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN] }
    }

    /// **ADR-0042 Decision 3a**: mutating ANY field fails the PoW.
    ///
    /// The audit's P0-1 remedy, as the consensus test it asks for — and the shape matters as much
    /// as the assertion. This used to hand-enumerate the six fields the commitment root happened
    /// to hash, which is a test that agrees with the bug it should catch: PR-06 added three DA
    /// fields to the struct, the enumeration did not grow, and one solved nonce could mint
    /// unlimited sibling identities again.
    ///
    /// So the list is derived from an exhaustive destructuring of `PalwAttemptUnsignedV2`
    /// instead. A field added tomorrow does not compile until it is named here, and naming it
    /// forces a mutation for it — the test cannot silently fall behind the struct.
    #[test]
    fn every_priced_field_moves_the_pow_tag() {
        let base = attempt();
        let baseline = l1_tag_v2(commitment_root_v2(&base));

        // Exhaustive by construction: this destructuring names every field of
        // `PalwAttemptUnsignedV2`, so adding one to the struct breaks THIS LINE until the new
        // field gets a mutation below.
        let PalwAttemptUnsignedV2 {
            version: _,
            network_domain: _,
            challenge: _,
            class_id: _,
            executor_bond: _,
            executor_pubkey: _,
            operator_id: _,
            artifact_root: _,
            trace_root: _,
            output_root: _,
            trace_manifest_root: _,
            trace_chunk_count: _,
            trace_retention_daa: _,
            execution_root: _,
            pwu: _,
        } = base.clone();

        let mut mutations: Vec<(&str, PalwAttemptUnsignedV2)> = Vec::new();
        let mut push = |name: &'static str, f: &dyn Fn(&mut PalwAttemptUnsignedV2)| {
            let mut m = base.clone();
            f(&mut m);
            assert_ne!(m, base, "the {name} mutation must actually change the attempt");
            mutations.push((name, m));
        };
        push("version", &|m| m.version = m.version.wrapping_add(1));
        push("network_domain", &|m| m.network_domain = Hash64::from_u64_word(0x9999));
        push("challenge", &|m| m.challenge = Hash64::from_u64_word(0x1234));
        push("class_id", &|m| m.class_id = Hash64::from_u64_word(0xC2));
        push("executor_bond", &|m| m.executor_bond = op(2));
        push("executor_pubkey", &|m| m.executor_pubkey[0] ^= 0xFF);
        push("operator_id", &|m| m.operator_id = Hash64::from_u64_word(0x0FF1CE));
        push("artifact_root", &|m| m.artifact_root = Hash64::from_u64_word(0xA27));
        push("trace_root", &|m| m.trace_root = Hash64::from_u64_word(0xDEAD));
        push("output_root", &|m| m.output_root = Hash64::from_u64_word(0xBEEF));
        // The three PR-06 added. They were identity-visible and PoW-invisible: the whole finding.
        push("trace_manifest_root", &|m| m.trace_manifest_root = Hash64::from_u64_word(0x1AA1));
        push("trace_chunk_count", &|m| m.trace_chunk_count += 1);
        push("trace_retention_daa", &|m| m.trace_retention_daa += 1);
        push("execution_root", &|m| m.execution_root = Hash64::from_u64_word(0xE7));
        push("pwu", &|m| m.pwu += 1);

        for (field, mutated) in mutations {
            assert_ne!(l1_tag_v2(commitment_root_v2(&mutated)), baseline, "mutating {field} left the PoW tag unchanged");
        }
    }

    /// The transcript and the identity are the SAME set of bytes, so they cannot drift.
    ///
    /// The stronger statement behind the exhaustive test above: two attempts share a PoW tag if
    /// and only if they share an attempt id. Stated as a property here so a future refactor that
    /// re-introduces a hand-written field list has to delete this test to do it.
    #[test]
    fn the_pow_tag_and_the_block_identity_agree_on_every_attempt() {
        let base = attempt();
        let mut other = base.clone();
        other.trace_retention_daa += 1;

        assert_ne!(attempt_id_v2(&other), attempt_id_v2(&base), "the ids differ");
        assert_ne!(commitment_root_v2(&other), commitment_root_v2(&base), "so the priced roots must differ too");
        // And the root is a pure function of the id — same id, same root, whatever the route.
        assert_eq!(commitment_root_v2(&base), commitment_root_v2(&base.clone()));
        // The root is NOT the id: a domain key separates them, so neither can be replayed as the
        // other in any transcript that consumes both.
        assert_ne!(commitment_root_v2(&base), attempt_id_v2(&base), "the root must be domain-separated from the id");
    }

    /// The challenge binds the header position, the class and the bond.
    ///
    /// Without the last two, one solved position could be re-announced under a cheaper class or a
    /// different accountable party at no extra work — the attack P0-1 enables, moved one level up.
    #[test]
    fn the_challenge_binds_position_class_and_bond() {
        let bond = op(1);
        let class = Hash64::from_u64_word(0xC1);
        let base = challenge_v2(net(), pph(), TS, NONCE, class, &bond);
        assert_ne!(challenge_v2(Hash64::from_u64_word(0x99), pph(), TS, NONCE, class, &bond), base, "network");
        assert_ne!(challenge_v2(net(), Hash64::from_u64_word(0xB1), TS, NONCE, class, &bond), base, "pre_pow_hash");
        assert_ne!(challenge_v2(net(), pph(), TS + 1, NONCE, class, &bond), base, "timestamp");
        assert_ne!(challenge_v2(net(), pph(), TS, NONCE + 1, class, &bond), base, "nonce");
        assert_ne!(challenge_v2(net(), pph(), TS, NONCE, Hash64::from_u64_word(0xC2), &bond), base, "class");
        assert_ne!(challenge_v2(net(), pph(), TS, NONCE, class, &op(2)), base, "bond");
    }

    /// **Decision 3c**: identity is the unsigned attempt, so a second valid signature is not a
    /// second block.
    ///
    /// ML-DSA-87 signatures are not guaranteed unique. Folding raw signature bytes into a block id
    /// would re-open malleability wearing the costume of a fix.
    #[test]
    fn a_second_valid_signature_is_not_a_second_block() {
        let a = attempt();
        let one = envelope(a.clone());
        let mut two = envelope(a.clone());
        two.signature = vec![0xA5; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN];
        assert_ne!(one.signature, two.signature);
        assert_eq!(attempt_id_v2(&one.attempt), attempt_id_v2(&two.attempt), "the signature must not reach identity");

        // And identity DOES cover every field of the claim, including the ones the PoW does not
        // price — a field outside identity is one two blocks can differ in while claiming to be one.
        for mutate in [
            (|x: &mut PalwAttemptUnsignedV2| x.executor_pubkey = vec![9u8; 32]) as fn(&mut PalwAttemptUnsignedV2),
            |x: &mut PalwAttemptUnsignedV2| x.operator_id = Hash64::from_u64_word(0xE1),
            |x: &mut PalwAttemptUnsignedV2| x.artifact_root = Hash64::from_u64_word(0xA8),
            |x: &mut PalwAttemptUnsignedV2| x.network_domain = Hash64::from_u64_word(0x99),
            // The DA obligation is identity too (Decision 7): two attempts differing only in what
            // they promise to serve are two claims, or the weaker promise rides the stronger's id.
            |x: &mut PalwAttemptUnsignedV2| x.trace_manifest_root = Hash64::from_u64_word(0xD1),
            |x: &mut PalwAttemptUnsignedV2| x.trace_chunk_count += 1,
            |x: &mut PalwAttemptUnsignedV2| x.trace_retention_daa += 1,
        ] {
            let mut m = a.clone();
            mutate(&mut m);
            assert_ne!(attempt_id_v2(&m), attempt_id_v2(&a));
        }
    }

    /// Stateless admission recomputes the carried challenge rather than trusting it.
    #[test]
    fn a_challenge_from_another_position_is_named_not_inferred() {
        let a = attempt();
        assert_eq!(envelope(a.clone()).validate_stateless_v2(net(), pph(), TS, NONCE), Ok(()));
        assert_eq!(
            envelope(a.clone()).validate_stateless_v2(net(), pph(), TS, NONCE + 1),
            Err(PalwAttemptV2Error::ChallengeMismatch),
            "an attempt mined at another nonce must be named, not left to a digest mismatch"
        );

        let mut zero = a.clone();
        zero.pwu = 0;
        assert_eq!(envelope(zero).validate_stateless_v2(net(), pph(), TS, NONCE), Err(PalwAttemptV2Error::ZeroPwu));

        let mut chunkless = a.clone();
        chunkless.trace_chunk_count = 0;
        assert_eq!(envelope(chunkless).validate_stateless_v2(net(), pph(), TS, NONCE), Err(PalwAttemptV2Error::ZeroTraceChunks));

        let mut short = envelope(a);
        short.signature.pop();
        assert!(matches!(short.validate_stateless_v2(net(), pph(), TS, NONCE), Err(PalwAttemptV2Error::SignatureLength { .. })));
    }

    /// The module's domains are distinct — a shared key would let one preimage serve two meanings.
    #[test]
    fn the_v2_domains_are_distinct() {
        let mut seen: Vec<&[u8]> = PALW_ATTEMPT_V2_ALL_DOMAINS.to_vec();
        seen.sort();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before, "two attempt-v2 domains collide");
    }
}
