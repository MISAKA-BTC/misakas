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
//! commitment_root = H(challenge ‖ class_id ‖ bond ‖ trace_root ‖ output_root ‖ pwu)
//! L1 tag          = Expand(commitment_root)
//! attempt_id      = H(canonical(PalwAttemptUnsignedV2))
//! ```
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

/// The header-carriage wire magic for a V2 attempt envelope.
///
/// Distinct from the V1 `PBC1` and the free-prompt `PFS3`: a carriage of one family can never
/// decode as another even before its fields are read, so a header cannot smuggle one lane's
/// object into the other lane's validator.
pub const PALW_ATTEMPT_V2_CARRIAGE_MAGIC: [u8; 4] = *b"PAT2";

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

/// `H(challenge ‖ class_id ‖ bond ‖ trace_root ‖ output_root ‖ pwu)` — what the PoW expands.
pub fn commitment_root_v2(attempt: &PalwAttemptUnsignedV2) -> Hash64 {
    let mut state = keyed(PALW_ATTEMPT_V2_DOMAIN_COMMITMENT_ROOT);
    state.update(attempt.challenge.as_byte_slice());
    state.update(attempt.class_id.as_byte_slice());
    update_outpoint(&mut state, &attempt.executor_bond);
    state.update(attempt.trace_root.as_byte_slice());
    state.update(attempt.output_root.as_byte_slice());
    state.update(&attempt.pwu.to_le_bytes());
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
    #[error("the header carriage does not decode: {0}")]
    CarriageUndecodable(&'static str),
}

impl PalwAttemptEnvelopeV2 {
    /// The header-carriage wire form: magic, then borsh.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = PALW_ATTEMPT_V2_CARRIAGE_MAGIC.to_vec();
        out.extend(borsh::to_vec(self).expect("borsh serialization of a plain struct cannot fail"));
        out
    }

    /// Decode a header-extension payload: magic, then borsh, then an exact-length check —
    /// trailing bytes are refused, because a payload is not a container.
    pub fn decode(bytes: &[u8]) -> Result<Self, PalwAttemptV2Error> {
        let Some(body) = bytes.strip_prefix(&PALW_ATTEMPT_V2_CARRIAGE_MAGIC) else {
            return Err(PalwAttemptV2Error::CarriageUndecodable("payload does not start with the PAT2 magic"));
        };
        let mut slice = body;
        let decoded = <Self as borsh::BorshDeserialize>::deserialize(&mut slice)
            .map_err(|_| PalwAttemptV2Error::CarriageUndecodable("borsh body"))?;
        if !slice.is_empty() {
            return Err(PalwAttemptV2Error::CarriageUndecodable("trailing bytes"));
        }
        Ok(decoded)
    }

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
        }
    }

    fn envelope(a: PalwAttemptUnsignedV2) -> PalwAttemptEnvelopeV2 {
        PalwAttemptEnvelopeV2 { attempt: a, signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN] }
    }

    /// **ADR-0042 Decision 3a**: mutating any priced field fails the PoW.
    ///
    /// The audit's P0-1 remedy, as the consensus test it asks for. Six fields go into the
    /// commitment root and the L1 tag expands it, so every one of them must move the tag — a
    /// binding that covered five would leave the sixth swappable on a solved PoW, which is the
    /// whole attack at one field's reduced scale.
    #[test]
    fn every_priced_field_moves_the_pow_tag() {
        let base = attempt();
        let baseline = l1_tag_v2(commitment_root_v2(&base));

        let mut mutations: Vec<(&str, PalwAttemptUnsignedV2)> = Vec::new();
        let mut m = base.clone();
        m.trace_root = Hash64::from_u64_word(0xDEAD);
        mutations.push(("trace_root", m));
        let mut m = base.clone();
        m.output_root = Hash64::from_u64_word(0xBEEF);
        mutations.push(("output_root", m));
        let mut m = base.clone();
        m.pwu += 1;
        mutations.push(("pwu", m));
        let mut m = base.clone();
        m.class_id = Hash64::from_u64_word(0xC2);
        mutations.push(("class_id", m));
        let mut m = base.clone();
        m.executor_bond = op(2);
        mutations.push(("executor_bond", m));
        let mut m = base.clone();
        m.challenge = Hash64::from_u64_word(0x1234);
        mutations.push(("challenge", m));

        for (field, mutated) in mutations {
            assert_ne!(l1_tag_v2(commitment_root_v2(&mutated)), baseline, "mutating {field} left the PoW tag unchanged");
        }
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

    /// The header carriage round-trips, refuses a foreign family's magic, and refuses trailing
    /// bytes — a payload is not a container.
    #[test]
    fn the_attempt_carriage_round_trips_and_refuses_foreign_shapes() {
        let envelope = envelope(attempt());
        let bytes = envelope.encode();
        assert_eq!(&bytes[..4], &PALW_ATTEMPT_V2_CARRIAGE_MAGIC);
        assert_eq!(PalwAttemptEnvelopeV2::decode(&bytes).unwrap(), envelope);

        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(PalwAttemptEnvelopeV2::decode(&trailing), Err(PalwAttemptV2Error::CarriageUndecodable("trailing bytes"))));

        // The V1 and free-prompt magics are refused BEFORE any field is read.
        for foreign in [crate::palw_block_commitment::PALW_BLOCK_COMMITMENT_MAGIC, crate::palw_freeprompt_v3::PALW_FP_V3_SPEND_CARRIAGE_MAGIC] {
            let mut relabeled = bytes.clone();
            relabeled[..4].copy_from_slice(&foreign);
            assert!(PalwAttemptEnvelopeV2::decode(&relabeled).is_err(), "a foreign family's magic never decodes here");
        }
        assert!(PalwAttemptEnvelopeV2::decode(&[]).is_err(), "an empty carriage is not an attempt");
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
