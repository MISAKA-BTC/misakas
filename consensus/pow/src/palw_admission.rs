//! ADR-0038 Decision A as ONE call: the whole admission predicate for a PALW block.
//!
//! The predicate has six conjuncts:
//!
//! ```text
//! valid_palw_block =
//!     valid_header
//!   ∧ spam_hash(header, nonce) < spam_target
//!   ∧ header carries palw_commitment_root
//!   ∧ executor references an Active bond of an Active ExecutionClass
//!   ∧ palw_ticket < class_target
//!   ∧ well-formed carriage
//! ```
//!
//! Their pieces landed separately, each in the layer that could hold its inputs:
//! `check_palw_commitment_shape` (bytes), `validate_executor_bond_v1` (a bond view),
//! `validate_against_class_v1` (class state), `palw_ticket_admits_v1` (arithmetic). Separate is
//! right for the pieces and wrong for the caller: five checks a pipeline must remember to make is
//! four ways to fail open, and the 2026-08-17 audit's recurring finding is exactly that —
//! "landed but not wired", and "one missing makes another fail-open".
//!
//! So the pipeline gets one function. It cannot perform four of the five, and it cannot get the
//! payee from anywhere but the bond that admitted the block.
//!
//! **This does not decide when the rule applies.** The caller passes `bound`, from
//! `Params::is_palw_commitment_bound(daa_score)` — `false` on every shipped preset, where the rule
//! is the pre-ADR one (a PALW header's commitment must be empty) and this returns
//! [`PalwAdmission::NotBound`]. Nothing here is live on any network today.

use kaspa_consensus_core::dns_finality::{ActiveBondView, StakeBondRecord};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_block_commitment::{PalwBlockCommitmentError, PalwBlockCommitmentV1};
use kaspa_consensus_core::palw_pwu::{palw_ticket_admits_v1, palw_ticket_v1};
use kaspa_consensus_core::pow_layer0::{PowLayer0Error, check_palw_commitment_shape};
use thiserror::Error;

use crate::StateLayer0;

/// What a block's class contributes to admission — the same facts
/// `kaspa_consensus_core::palw_facts::PalwClassFactsV1` carries, taken by value here so this crate
/// needs no view trait of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwAdmissionClassFacts {
    /// The class's DAA target **for this block**, folded over the block's own selected-parent
    /// chain. Never the class's target now — see `palw_facts::PalwClassFactsViewV1`.
    pub class_target: u128,
    /// The class's registered normative operation count per canonical inference.
    pub pwu_per_inference: u64,
    /// **ADR-0069 Decision 7: whether the class may carry fork-choice WEIGHT here.** Carried so one
    /// fold answers every class fact a block needs, and **deliberately not read by
    /// [`check_palw_block_admission_v1`]** — the whole shape of Decision 7 is that an uncertified
    /// class still produces, still gossips and is still paid; only its weight is zero. Admission
    /// reading this bit would turn "weighs nothing" into "may not build", which is the rule
    /// ADR-0069 Decision 5 explicitly refused.
    ///
    /// See `palw_facts::PalwClassFactsV1::weight_bearing` for why granted share is the predicate.
    pub weight_bearing: bool,
}

/// The outcome of the admission predicate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwAdmission<'a> {
    /// The commitment fence is not installed at this block's DAA. The pre-ADR rule applied and
    /// held: the header carries no commitment, and there is nothing further to check.
    NotBound,
    /// Every conjunct held. Carries the bond that admitted the block, so the payee is the bond
    /// that acted rather than a second lookup that could resolve to a different record.
    Admitted { commitment: PalwBlockCommitmentV1, executor_bond: &'a StakeBondRecord, ticket: u128 },
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum PalwAdmissionError {
    #[error("the header's palw_commitment is not admissible: {0}")]
    Commitment(#[from] PowLayer0Error),
    #[error("the commitment does not admit: {0}")]
    Claim(#[from] PalwBlockCommitmentError),
    #[error("pwu claim: {0}")]
    Pwu(#[from] kaspa_consensus_core::palw_pwu::PalwPwuError),
    /// The lottery clause. Reported with both sides because a ticket that missed by little and one
    /// that missed by a lot are different operational stories.
    #[error("the commitment's ML-DSA-87 signature does not verify under the executor bond's key")]
    CommitmentSignatureInvalid,
    #[error("palw ticket {ticket} does not admit under the class target {class_target}")]
    TicketDoesNotAdmit { ticket: u128, class_target: u128 },
    /// The header's PoW could not be computed at all — a runtime fault, not a verdict on the
    /// block. Kept distinct so a caller never reads "this node cannot check" as "this block is
    /// invalid", which is the P0-1 lesson from the same audit.
    #[error("this node could not compute the header's Layer-0 digest: {0}")]
    Unresolvable(PowLayer0Error),
    /// The block names a class this node cannot resolve facts for at this chain point. An
    /// unresolved fact is an error, never a permissive zero — the same rule
    /// `palw_facts::PalwFactsError::Unresolved` states for the weight side.
    #[error("this node cannot resolve class facts for {class_id} at this chain point")]
    ClassUnresolved { class_id: kaspa_hashes::Hash64 },
}

/// ADR-0038 Decision A, whole. `bound` comes from the network's fence, never from the caller's
/// judgement.
///
/// Order is deliberate: the cheap, PoW-independent checks run before the digest is computed, which
/// on a PALW network is a full LLM inference. That is the same ordering discipline the
/// pruning-proof paths carry (audit P0-3), applied here so a malformed commitment or an
/// unbonded executor never buys one.
///
/// `class_facts` is a RESOLVER rather than a value, and keyed by the commitment's own class id.
/// Two reasons, and the second is why this signature exists at all:
///
/// * a value handed in beside the class is not bound to it — the §3.2 defect, closed the same way
///   `palw_facts::PalwClassFactsViewV1` closes it;
/// * a value would have to be produced even when the fence is OFF, where it is never read. That
///   made the call impossible to place in a pipeline that has no class store yet, which is exactly
///   the position every shipped network is in. Lazy, the unbound path costs nothing and the call
///   can sit in the pipeline today, inert.
pub fn check_palw_block_admission_v1<'a, F, V>(
    header: &Header,
    bonds: &'a ActiveBondView,
    class_facts: F,
    network_id: &[u8],
    bound: bool,
    verify_mldsa87: V,
) -> Result<PalwAdmission<'a>, PalwAdmissionError>
where
    F: FnOnce(&kaspa_hashes::Hash64) -> Option<PalwAdmissionClassFacts>,
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    // Shape first, and unconditionally: this also enforces the pre-ADR rule when the fence is off,
    // so an unfenced network still refuses a header that carries bytes nothing validates.
    check_palw_commitment_shape(header.pow_algo_id, &header.palw_commitment, bound)?;
    if !bound {
        return Ok(PalwAdmission::NotBound);
    }
    // Decoding cannot fail here — `check_palw_commitment_shape` decoded and shape-checked the same
    // bytes above — but it is re-derived rather than smuggled out of that call, so this function
    // has one source for the value it goes on to use.
    let commitment = PalwBlockCommitmentV1::decode(&header.palw_commitment)?;

    // W8, before the inference: no bond, no block.
    let executor_bond = commitment.validate_executor_bond_v1(bonds, header.daa_score)?;

    // …and the bond must have SIGNED this commitment (external audit P0-2). Without it W8 read
    // "name any Active bond outpoint and attach bytes of the right length": `validate_shape` checks
    // the signature's LENGTH, `validate_executor_bond_v1` checks the named bond is Active, and
    // nothing checked that the bond's holder authorised anything. An attacker with no stake could
    // produce blocks under a victim's bond, and every downstream attribution — payee, slash target,
    // panel exclusion — pointed at the victim.
    //
    // The verifier is passed in because this crate holds no curve, but the CONTEXT is not: it is
    // applied here, so no caller can supply the wrong domain. That is the shape audit P0-6 asks
    // for — the defect there was one context-free closure serving three object families, and the
    // repair is that the family's own code chooses the domain.
    //
    // Before the ticket, which is the expensive part: an unsigned commitment must cost a peer a
    // signature verification, not an inference.
    let attempt_digest =
        commitment.message(network_id, kaspa_consensus_core::hashing::header::pre_pow_hash_64(header), header.timestamp, header.nonce);
    if !verify_mldsa87(
        &executor_bond.validator_pubkey,
        attempt_digest.as_bytes().as_slice(),
        &commitment.signature,
        kaspa_consensus_core::palw_block_commitment::PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT,
    ) {
        return Err(PalwAdmissionError::CommitmentSignatureInvalid);
    }

    // Resolved BY the block's own class id, and only now that the block has earned the lookup.
    let facts = class_facts(&commitment.execution_class_id)
        .ok_or(PalwAdmissionError::ClassUnresolved { class_id: commitment.execution_class_id })?;

    // The pwu claim is chain state restated, and has exactly one legal value.
    commitment.validate_against_class_v1(facts.class_target, facts.pwu_per_inference)?;

    // Only now the expensive part.
    let state = StateLayer0::new(header, network_id);
    let digest = state.calculate_pow_layer0(header.nonce).map_err(PalwAdmissionError::Unresolvable)?;
    let ticket = palw_ticket_v1(&digest);
    if !palw_ticket_admits_v1(ticket, facts.class_target) {
        return Err(PalwAdmissionError::TicketDoesNotAdmit { ticket, class_target: facts.class_target });
    }
    Ok(PalwAdmission::Admitted { commitment, executor_bond, ticket })
}

/// The `commitment_root` a header's PALW commitment announces — **what receipts target**.
///
/// Every consumer of that value (the receipt filter, the panel draw, the dispute's announced root)
/// needs it, and none of them holds a header's pre-PoW derivation. This function is here rather
/// than beside them for the reason admission states about its own re-decode: *one source for the
/// value it goes on to use*. Two call sites deriving it independently is two chances to disagree
/// about which bytes the network is committing to, and a disagreement there does not fail loudly —
/// it makes one node's receipts target a root no other node recognises, so quorum silently never
/// forms.
///
/// `None` when the header carries no decodable commitment, which is every header on a network
/// whose fence is off.
pub fn palw_header_commitment_root_v1(
    header: &kaspa_consensus_core::header::Header,
    network_id: &[u8],
) -> Option<kaspa_hashes::Hash64> {
    let commitment = PalwBlockCommitmentV1::decode(&header.palw_commitment).ok()?;
    let pre_pow_hash = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
    let challenge = commitment.challenge_for(network_id, pre_pow_hash, header.timestamp, header.nonce);
    Some(commitment.commitment_root(challenge))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::BlueWorkType;
    use kaspa_consensus_core::dns_finality::{BondStatus, STAKE_ATTESTATION_SIG_LEN};
    use kaspa_consensus_core::palw_block_commitment::PALW_BLOCK_COMMITMENT_VERSION_V1;
    use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_PALW_LLM};
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use kaspa_hashes::{Hash64, ZERO_HASH64};

    const NETWORK: &[u8] = b"devnet";
    /// The easiest possible target: every ticket admits, so the lottery never decides a test that
    /// is about something else. The one test that IS about the lottery tightens it.
    const EASY: PalwAdmissionClassFacts =
        PalwAdmissionClassFacts { class_target: u128::MAX, pwu_per_inference: 100, weight_bearing: true };

    /// Accepts iff the signature is the fixture's own `[0x5A; SIG_LEN]` under a non-empty key and
    /// the block-commitment context. Stands in for ML-DSA-87, which lives outside this crate.
    ///
    /// Deliberately NOT "accept everything": the point of these tests is that admission ASKS, and a
    /// permissive stub would let the P0-2 regression back in without a single assertion changing.
    fn accept_fixture_signature(key: &[u8], _message: &[u8], signature: &[u8], context: &[u8]) -> bool {
        !key.is_empty()
            && signature == vec![0x5A; STAKE_ATTESTATION_SIG_LEN].as_slice()
            && context == kaspa_consensus_core::palw_block_commitment::PALW_BLOCK_COMMITMENT_MLDSA87_CONTEXT
    }

    fn outpoint(seed: u64) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_u64_word(seed), 0)
    }

    fn bond_record(op: TransactionOutpoint) -> kaspa_consensus_core::dns_finality::StakeBondRecord {
        kaspa_consensus_core::dns_finality::StakeBondRecord {
            version: 1,
            bond_outpoint: op,
            owner_pubkey_hash: Hash64::from_u64_word(8),
            validator_pubkey_hash: Hash64::from_u64_word(7),
            validator_pubkey: vec![7u8; 32],
            amount: 20_000,
            activation_daa_score: 0,
            created_daa_score: 0,
            unbonding_period_blocks: 100,
            owner_reward_spk_payload: [0u8; 64],
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
            status: BondStatus::Active,
        }
    }

    fn bonds_with(op: TransactionOutpoint) -> ActiveBondView {
        ActiveBondView::from_records([(op, bond_record(op))])
    }

    /// `pwu_claim` must equal `expected_attempts(target) × pwu_per_inference`; at the easiest
    /// target that is `1 × pwu_per_inference`.
    fn commitment(op: TransactionOutpoint, facts: PalwAdmissionClassFacts) -> PalwBlockCommitmentV1 {
        let attempts = kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1(facts.class_target);
        PalwBlockCommitmentV1 {
            version: PALW_BLOCK_COMMITMENT_VERSION_V1,
            execution_class_id: Hash64::from_u64_word(0xC1),
            executor_bond_outpoint: op,
            trace_root: Hash64::from_u64_word(4),
            output_root: Hash64::from_u64_word(5),
            pwu_claim: attempts.saturating_mul(facts.pwu_per_inference),
            signature: vec![0x5A; STAKE_ATTESTATION_SIG_LEN],
        }
    }

    /// A PALW header (algo 4). Every test in THIS module asserts a conjunct that returns before
    /// the Layer-0 digest is computed, so none of them needs the pinned model. The two that do
    /// need it — the happy path and the lottery — live in `tests/palw_admission_fixture.rs`,
    /// where the model-free fixture tag family can be selected for the whole binary.
    /// **Audit P0-2**: naming an Active bond is not holding one.
    ///
    /// The attack this closes needs no key at all — write any Active bond's outpoint into the
    /// commitment, attach bytes of the right LENGTH, and W8 used to pass: `validate_shape` measured
    /// the signature, `validate_executor_bond_v1` confirmed the named bond was Active, and nobody
    /// asked whether that bond's holder had authorised anything. Every downstream attribution —
    /// payee, slash target, panel exclusion — then pointed at the victim.
    ///
    /// The refusal must also land BEFORE the digest: an unsigned commitment should cost a peer one
    /// signature verification, never an inference. That ordering is what the second assertion pins —
    /// this test runs with no fixture tag family and no model, so reaching the digest would fail
    /// differently (`Unresolvable`), and `CommitmentSignatureInvalid` is only reachable if the check
    /// is upstream of it.
    #[test]
    fn a_commitment_nobody_signed_is_refused_before_the_inference() {
        let op = outpoint(1);
        let bonds = ActiveBondView::from_records([(op, bond_record(op))]);
        let mut c = commitment(op, EASY);
        c.signature = vec![0xAA; STAKE_ATTESTATION_SIG_LEN]; // right length, wrong bytes

        assert!(
            matches!(
                check_palw_block_admission_v1(&header(c.encode()), &bonds, |_| Some(EASY), NETWORK, true, accept_fixture_signature),
                Err(PalwAdmissionError::CommitmentSignatureInvalid)
            ),
            "a commitment the bond did not sign must be refused"
        );

        // And the domain is chosen by admission, not by the caller: a verifier that only accepts
        // some OTHER context sees the block-commitment context and refuses.
        let wrong_domain = |_k: &[u8], _m: &[u8], _s: &[u8], context: &[u8]| context == b"some-other-domain".as_slice();
        assert!(matches!(
            check_palw_block_admission_v1(&header(commitment(op, EASY).encode()), &bonds, |_| Some(EASY), NETWORK, true, wrong_domain),
            Err(PalwAdmissionError::CommitmentSignatureInvalid)
        ));
    }

    /// The announced root is a function of the whole attempt, and the negative is what matters.
    ///
    /// Receipts target this value, so anything that moves it must move every receipt with it. The
    /// nonce and the timestamp are inside the challenge the root expands from — two attempts over
    /// the same payload are different claims, and a root that ignored them would let a miner
    /// re-announce one panel's work under a second attempt.
    #[test]
    fn the_announced_root_binds_the_attempt_not_just_the_payload() {
        let bytes = commitment(outpoint(1), EASY).encode();
        let base = header(bytes.clone());
        let root = palw_header_commitment_root_v1(&base, NETWORK).expect("decodable");
        assert_eq!(palw_header_commitment_root_v1(&base, NETWORK), Some(root), "not a pure function");

        let mut other_nonce = header(bytes.clone());
        other_nonce.nonce = base.nonce.wrapping_add(1);
        assert_ne!(palw_header_commitment_root_v1(&other_nonce, NETWORK), Some(root), "the nonce must move the root");

        let mut other_time = header(bytes.clone());
        other_time.timestamp = base.timestamp.wrapping_add(1);
        assert_ne!(palw_header_commitment_root_v1(&other_time, NETWORK), Some(root), "the timestamp must move the root");

        assert_ne!(palw_header_commitment_root_v1(&base, b"other-net"), Some(root), "the network must move the root");
        assert_eq!(palw_header_commitment_root_v1(&header(Vec::new()), NETWORK), None, "no commitment, no root");
    }

    fn header(commitment_bytes: Vec<u8>) -> Header {
        Header::new_finalized(
            1,
            vec![vec![1.into()]].try_into().unwrap(),
            ZERO_HASH64,
            ZERO_HASH64,
            ZERO_HASH64,
            1_000_000,
            0x207fffff,
            7,
            POW_ALGO_ID_PALW_LLM,
            500, // daa_score — the point the bond must be Active at
            BlueWorkType::from_u64(0),
            0,
            ZERO_HASH64,
        )
        .with_palw_commitment(commitment_bytes)
    }

    /// Unfenced is the pre-ADR rule and nothing more happens: no bond is consulted, no digest is
    /// computed. A network that has not installed the fence is untouched by any of this.
    #[test]
    fn unfenced_is_the_old_rule_and_stops_there() {
        let empty = ActiveBondView::from_records([]);
        assert_eq!(
            check_palw_block_admission_v1(&header(Vec::new()), &empty, |_| Some(EASY), NETWORK, false, accept_fixture_signature)
                .unwrap(),
            PalwAdmission::NotBound
        );
        // And an unfenced header carrying bytes is still refused, by the shape rule.
        let carrying = header(commitment(outpoint(2), EASY).encode());
        assert!(matches!(
            check_palw_block_admission_v1(&carrying, &empty, |_| Some(EASY), NETWORK, false, accept_fixture_signature),
            Err(PalwAdmissionError::Commitment(_))
        ));
    }

    /// W8. The single conjunct ADR-0038 calls load-bearing: remove the bond and the block does not
    /// admit, however well-formed everything else is.
    #[test]
    fn no_bond_no_block() {
        let op = outpoint(2);
        let h = header(commitment(op, EASY).encode());
        let nobody = ActiveBondView::from_records([]);
        assert!(matches!(
            check_palw_block_admission_v1(&h, &nobody, |_| Some(EASY), NETWORK, true, accept_fixture_signature),
            Err(PalwAdmissionError::Claim(PalwBlockCommitmentError::ExecutorBondNotActive { .. }))
        ));
        // Someone else's active bond does not stand in.
        let theirs = bonds_with(outpoint(99));
        assert!(matches!(
            check_palw_block_admission_v1(&h, &theirs, |_| Some(EASY), NETWORK, true, accept_fixture_signature),
            Err(PalwAdmissionError::Claim(PalwBlockCommitmentError::ExecutorBondNotActive { .. }))
        ));
    }

    /// The pwu claim is chain state restated and has exactly one legal value — a claim of anything
    /// else is refused even with a good bond and an admitting ticket.
    #[test]
    fn an_inflated_pwu_claim_does_not_admit() {
        let op = outpoint(2);
        let bonds = bonds_with(op);
        let mut c = commitment(op, EASY);
        c.pwu_claim = c.pwu_claim.saturating_mul(1_000);
        assert!(matches!(
            check_palw_block_admission_v1(&header(c.encode()), &bonds, |_| Some(EASY), NETWORK, true, accept_fixture_signature),
            Err(PalwAdmissionError::Pwu(_))
        ));
    }

    /// A non-PALW header must carry nothing, fence or no fence — there the field is hash-invisible
    /// and a non-empty one is block-hash malleability.
    #[test]
    fn a_non_palw_header_is_not_fenced() {
        let op = outpoint(2);
        let bonds = bonds_with(op);
        let mut h = header(commitment(op, EASY).encode());
        h.pow_algo_id = POW_ALGO_ID_KHEAVYHASH;
        for bound in [false, true] {
            assert!(matches!(
                check_palw_block_admission_v1(&h, &bonds, |_| Some(EASY), NETWORK, bound, accept_fixture_signature),
                Err(PalwAdmissionError::Commitment(_))
            ));
        }
    }
}
