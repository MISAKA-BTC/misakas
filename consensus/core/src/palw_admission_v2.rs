//! V2 attempt admission — the STATEFUL side of ADR-0042 Decision 6, read entirely from the
//! candidate-scoped [`PalwChainStateV2`] (PR-04).
//!
//! Admission is two phases, and the split is load-bearing:
//!
//! * **Stateless** (`palw_attempt_v2`): version, sizes, challenge recompute, and the signature
//!   verifying over the attempt id under the **carried** key — provable with zero chain lookups,
//!   so an unsigned attempt costs a peer one verification and nothing else.
//! * **Stateful** (this module): the ten facts below, every one read from the candidate chain's
//!   own state — never from the node's sink, which is the P0-4 discipline PR-03 built the
//!   substrate for.
//!
//! Decision 6's list, in this module's checking order:
//!
//! ```text
//!  1. the bond outpoint exists and is Active at the candidate chain point
//!  9. …and is not in retirement / withdrawal wait        (one status read, two refusals)
//!  2. the bond record's pubkey == the commitment's executor_pubkey        ← closes P0-2
//!  3. operator_id matches the bond registration
//!  4. the class exists and is Active (not frozen)
//!  5. artifact_root == the class's registered root
//!  6. PWU is consistent with the class rules
//!  7. within the class epoch budget — a predicate on the PRODUCING block's own selected-chain
//!     class production (never ADR-0039 D5c's broken mergeset formulation)
//!  8. within the bond's exposure ceiling                                   ← closes P0-10
//! 10. no equivocation by the same bond (the chain-visible face: a duplicate attempt id)
//! ```
//!
//! Item 2 is what makes the stateless signature check MEAN something: stateless proves "the
//! carried key signed this claim"; item 2 proves "the carried key is the named bond's key."
//! Without the equality, W8 degrades to "name any Active bond and sign with your own key" — the
//! audit's P0-2, one namespace over.
//!
//! Item 10's admission-time face is deliberately narrow. Inside one candidate chain, the same
//! bond signing two different attempts at one header position cannot both be accepted — the
//! duplicate-id check is the whole of what a candidate-scoped reader can see. Cross-branch
//! equivocation (two signatures at sibling positions) is prevented producer-side by the signer's
//! anti-equivocation journal (PR-05) and punished by the court (PR-07); admission looking across
//! branches would violate the very candidate purity this ruleset exists to establish.
//!
//! Missing facts are errors, never permissive zeros — a class with no epoch budget entry does
//! not admit "for free"; it does not admit at all.

use crate::Hash64;
use crate::palw_attempt_v2::{PalwAttemptEnvelopeV2, PalwAttemptV2Error, attempt_id_v2};
use crate::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwClassStatusV2, PalwPwuRuleV2, PalwStateParamsV2,
};

/// Admission's own network constants. Constructed only through [`PalwAdmissionParamsV2::new`];
/// like the state params, they are part of the atomic ruleset bundle (ADR-0042 Decision 1) and
/// the fingerprint commits to them (Decision 11).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwAdmissionParamsV2 {
    /// Decision 6's `max_exposure_ratio`, in permille of the bond's slashable collateral. The
    /// ceiling is `collateral × ratio / 1000`, floored — conservative, and identical everywhere.
    max_exposure_ratio_permille: u32,
}

impl PalwAdmissionParamsV2 {
    /// **No epoch-budget table here any more (ADR-0045 Decision 2).**
    ///
    /// It carried a static per-class pwu ceiling: a number with no sizing basis and the wrong
    /// currency, since under `DerivedV1` an attempt's pwu tracks the class TARGET, so the same
    /// budget bought fewer blocks the harder a class got — a class that became popular would hard
    /// stop its own chain for the rest of every epoch. The chain derives `budget_blocks` from the
    /// share table now (`derive_epoch_budgets_v2`), which is where a cadence cap belongs, and a
    /// second copy here would be a second answer.
    pub fn new(max_exposure_ratio_permille: u32) -> Result<Self, PalwAdmissionV2Error> {
        if max_exposure_ratio_permille == 0 {
            return Err(PalwAdmissionV2Error::InvalidParams("a zero exposure ratio admits no work at all"));
        }
        Ok(Self { max_exposure_ratio_permille })
    }

    pub fn max_exposure_ratio_permille(&self) -> u32 {
        self.max_exposure_ratio_permille
    }
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwAdmissionV2Error {
    #[error("invalid admission params: {0}")]
    InvalidParams(&'static str),
    #[error("stateless validation failed: {0}")]
    Stateless(#[from] PalwAttemptV2Error),
    #[error("the named executor bond {0:?} does not exist at the candidate chain point")]
    BondMissing(PalwBondKeyV2),
    #[error("the named executor bond {0:?} is retiring and may back no new work")]
    BondRetiring(PalwBondKeyV2),
    #[error("the carried executor key is not the bond record's key — the signature authorises nothing about this bond")]
    BondKeyMismatch,
    #[error("the carried operator id is not the bond registration's operator id")]
    OperatorMismatch,
    #[error("class {0} has no target at this chain point — a class with no difficulty admits everything or nothing")]
    ClassTargetMissing(Hash64),
    #[error("the attempt's class ticket {ticket} is above class {class_id}'s target {target}")]
    ClassTicketAboveTarget { class_id: Hash64, ticket: u128, target: u128 },
    #[error("class {0} does not exist at the candidate chain point")]
    ClassMissing(Hash64),
    #[error("class {0} is frozen and admits no new work")]
    ClassFrozen(Hash64),
    #[error("the carried artifact root is not the class's registered root")]
    ArtifactRootMismatch,
    #[error("claimed pwu {claimed} exceeds the class rule's ceiling {ceiling}")]
    PwuExceedsClassRule { claimed: u64, ceiling: u64 },
    #[error("claimed pwu {claimed} is not the derived {derived} — pwu is chain state, not a miner input (ADR-0045 Decision 1)")]
    PwuClaimNotDerived { claimed: u64, derived: u64 },
    #[error("class {0} has no epoch budget entry — a missing budget admits nothing, not everything")]
    EpochBudgetUnspecified(Hash64),
    #[error("epoch budget exceeded for class {class_id}: produced {produced} + claimed {claimed} > budget {budget}")]
    EpochBudgetExceeded { class_id: Hash64, produced: u128, claimed: u128, budget: u128 },
    #[error(
        "bond exposure ceiling exceeded: reserved {reserved} + this claim {claim} > ceiling {ceiling} \
         (collateral {collateral} × {ratio_permille}‰)"
    )]
    ExposureCeilingExceeded { reserved: u128, claim: u128, ceiling: u128, collateral: u64, ratio_permille: u32 },
    #[error("attempt {0} already has a claim on this chain — one identity, one claim")]
    DuplicateAttempt(Hash64),
    #[error("arithmetic overflow in {0}")]
    Overflow(&'static str),
    #[error("class {class_id} is registered but weightless until DAA {activation_daa} — it holds no cadence share yet")]
    ClassNotYetActive { class_id: Hash64, activation_daa: u64 },
}

/// **Items 1–5 alone: is the party behind this attempt entitled to produce at all?**
///
/// The split exists because the subsidy and the claim are two different questions asked at two
/// different places. `check_palw_attempt_admission_v2` below answers "does this candidate chain
/// accept this claim", which is a chain-block question and includes the resource items — the
/// epoch budget, the class lottery, the exposure ceiling, the no-duplicate rule. A merged blue
/// block asks something narrower and is asked it by the COINBASE: this block is being paid a
/// worker share, so is its producer a bonded, registered, unfrozen participant, or is it a
/// stranger who solved a hash?
///
/// Running the full list there would be wrong in the other direction: a resource item can refuse
/// an honest merged blue for reasons that belong to the chain block (the chain block spent the
/// epoch's budget, the same attempt is already claimed) and it would go unpaid for someone else's
/// consumption. Entitlement is exactly the part that is about the producer.
///
/// Items 1–5 verbatim, and the composed admission below calls THIS rather than restating them —
/// two copies of "who may produce" is how the two answers drift apart.
pub fn check_palw_producer_entitlement_v2(
    state: &PalwChainStateV2,
    attempt: &crate::palw_attempt_v2::PalwAttemptUnsignedV2,
) -> Result<(), PalwAdmissionV2Error> {
    // 1 + 9. The bond, at the candidate point, in one status read.
    let bond_key = PalwBondKeyV2(attempt.executor_bond);
    let bond = state.bond(&bond_key).ok_or(PalwAdmissionV2Error::BondMissing(bond_key))?;
    if let PalwBondStatusV2::Retiring { .. } = bond.status {
        return Err(PalwAdmissionV2Error::BondRetiring(bond_key));
    }

    // 2. The carried key IS the bond's key — what turns the stateless signature into W8.
    if bond.pubkey != attempt.executor_pubkey {
        return Err(PalwAdmissionV2Error::BondKeyMismatch);
    }

    // 3. One operator identity per bond, fixed at registration (panel-seat dedup rests on it).
    if bond.operator_id != attempt.operator_id {
        return Err(PalwAdmissionV2Error::OperatorMismatch);
    }

    // 4. The class, at the candidate point.
    let class = state.class(&attempt.class_id).ok_or(PalwAdmissionV2Error::ClassMissing(attempt.class_id))?;
    match class.status {
        PalwClassStatusV2::Active => {}
        PalwClassStatusV2::Frozen { .. } => return Err(PalwAdmissionV2Error::ClassFrozen(attempt.class_id)),
        // Condition 12: registered, adjudicable, and carrying no weight yet. It holds no cadence
        // share, so an attempt of it would be a block whose class was granted no permille — and
        // the epoch budget, which is derived FROM the share table, would have no entry for it
        // either. Refusing at the class rather than letting it fall through to a missing budget
        // says which of the two facts is the reason.
        PalwClassStatusV2::Registered { activation_daa, .. } => {
            return Err(PalwAdmissionV2Error::ClassNotYetActive { class_id: attempt.class_id, activation_daa });
        }
    }

    // 5. The artifact the trace claims to open against is the one the class registered.
    if class.artifact_root != attempt.artifact_root {
        return Err(PalwAdmissionV2Error::ArtifactRootMismatch);
    }

    Ok(())
}

/// The stateful admission verdict for one attempt against one candidate chain point.
///
/// Returns the attempt id — the identity every accepted consumer keys on — so a caller that
/// admits and then applies cannot recompute a different one in between.
pub fn check_palw_attempt_admission_v2(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    admission: &PalwAdmissionParamsV2,
    ctx: &PalwBlockContextV2,
    envelope: &PalwAttemptEnvelopeV2,
) -> Result<Hash64, PalwAdmissionV2Error> {
    let attempt = &envelope.attempt;

    // Items 1–5: the producer's entitlement, shared verbatim with the coinbase's question.
    check_palw_producer_entitlement_v2(state, attempt)?;
    let bond_key = PalwBondKeyV2(attempt.executor_bond);
    let bond = state.bond(&bond_key).expect("entitlement resolved this bond");
    let class = state.class(&attempt.class_id).expect("entitlement resolved this class");

    // 6. The pwu claim against the class rule (pwu ≥ 1 is stateless).
    match class.pwu_rule {
        PalwPwuRuleV2::MaxPerAttempt(ceiling) => {
            if attempt.pwu > ceiling {
                return Err(PalwAdmissionV2Error::PwuExceedsClassRule { claimed: attempt.pwu, ceiling });
            }
        }
        // ADR-0045 Decision 1: EQUALITY, not a bound. Both factors are chain facts — the class's
        // target at this candidate point and its registered per-inference cost — so any other
        // value is a mistake or a weight-inflation attempt, and both are rejections. The target
        // is fetched here rather than shared with item 6b below on purpose: 6b's job is the
        // lottery, this one's is the price, and each names its own missing-fact error.
        PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => {
            let target =
                state.class_target(&attempt.class_id).ok_or(PalwAdmissionV2Error::ClassTargetMissing(attempt.class_id))?;
            let derived = crate::palw_pwu::palw_pwu_v1(target.target, pwu_per_inference);
            if attempt.pwu != derived {
                return Err(PalwAdmissionV2Error::PwuClaimNotDerived { claimed: attempt.pwu, derived });
            }
        }
    }

    // 7. **The epoch budget, in BLOCKS, from the chain's own derived table (ADR-0045 Decision 2).**
    //
    //    It used to read a static per-class pwu ceiling carried in the bundle — a number with no
    //    sizing basis and the wrong currency. Under `DerivedV1` an attempt's pwu is a function of
    //    the class TARGET, so a pwu budget shrinks in block terms exactly as a class gets harder:
    //    a class that got popular would stall its own chain for getting popular, permanently, for
    //    the rest of every epoch. The share also cancels out of a pwu inequality entirely
    //    (ADR-0045's amendment defect (e)), so the cap could not express "this class's share of
    //    cadence" — which is the only thing it was ever for.
    //
    //    The chain derives `budget_blocks` at each epoch from the share table, the epoch's DAA
    //    span and the tolerance, so the cap is sized by the class's own share and cannot bind on
    //    a chain running at cadence. A class with no budget in THIS epoch admits nothing: a
    //    missing budget is a missing fact, never a permissive zero.
    //    **And the floor is exempt, because a cadence-sharing rule that can stop the chain is not
    //    a cadence-sharing rule.**
    //
    //    The census denominator (ADR-0045 Decision 2, now implemented) fixes the steady state: a
    //    sole producer is measured against itself, so its budget is the whole epoch and cannot
    //    bind. It does not fix the transition. A class that produced in the closed epoch is
    //    measured against the set INCLUDING the classes that produced then — and if those go idle
    //    now, it exhausts its slice and every further attempt is refused. On a `ConsensusV2`
    //    network that is not a slowdown: the attempt lane is the only block type
    //    (`required_algo_id_for_mode` demands `algorithm_id` of every header), so no block is
    //    produced, DAA does not advance, the epoch never ends, and the budget that would refill
    //    never refills. There is no clock to escape on, because the clock IS the blocks.
    //
    //    So the escape has to be structural, and ADR-0039 W6′ already names the structure: BASE-0
    //    is the permanently-Active liveness floor, the class every operator can run. Exempting it
    //    is what makes the deadlock unrepresentable — the floor can always produce, so DAA always
    //    advances, so every other class's epoch always ends and every other class's cap is a
    //    slowdown rather than a death. `ClassFrozen` refuses the floor for the same reason.
    //
    //    The cap keeps doing what Decision 2 says it is for ("a transiently mis-tuned DAA flooding
    //    the DAG") on every class that is not the floor; for the floor itself the control is its
    //    own per-class retarget, which is the instrument sized for it.
    if attempt.class_id != state_params.base_class_id() {
        let epoch_index = ctx.daa_score / state_params.epoch_length();
        let budgets = state
            .epoch_budgets()
            .filter(|b| b.epoch_index == epoch_index)
            .ok_or(PalwAdmissionV2Error::EpochBudgetUnspecified(attempt.class_id))?;
        let budget = *budgets
            .budget_blocks
            .get(&attempt.class_id)
            .ok_or(PalwAdmissionV2Error::EpochBudgetUnspecified(attempt.class_id))?;
        let produced = match state.epoch_counter(&attempt.class_id) {
            Some(counter) if counter.epoch_index == epoch_index => counter.produced_blocks,
            _ => 0,
        };
        // This attempt is one block of this class.
        let would_produce = produced.checked_add(1).ok_or(PalwAdmissionV2Error::Overflow("epoch production"))?;
        if would_produce > budget {
            return Err(PalwAdmissionV2Error::EpochBudgetExceeded {
                class_id: attempt.class_id,
                produced: produced as u128,
                claimed: 1,
                budget: budget as u128,
            });
        }
    }

    // 6b. The CLASS lottery (ADR-0039's per-class DAA). The network target decided this header
    //     is a block; this decides it is a block of this class, against the target the retarget
    //     maintains. Without it the per-class retarget was arithmetic nothing consumed — it ran
    //     every epoch, moved a number, and no admission, weight or selection ever read it (audit
    //     H1's second half). A class target with no reader is a difficulty that does not exist.
    let target = state.class_target(&attempt.class_id).ok_or(PalwAdmissionV2Error::ClassTargetMissing(attempt.class_id))?;
    let ticket = crate::palw_attempt_v2::class_ticket_v2(attempt);
    if ticket > target.target {
        return Err(PalwAdmissionV2Error::ClassTicketAboveTarget { class_id: attempt.class_id, ticket, target: target.target });
    }

    // 8. The exposure ceiling (closes P0-10): what this bond already backs, plus what this claim
    //    would reserve, against collateral × ratio. Floor on the ceiling — conservative.
    let reserved = state.reserved_exposure(&bond_key);
    let claim_exposure = (attempt.pwu as u128)
        .checked_mul(class.slash_value_per_pwu as u128)
        .ok_or(PalwAdmissionV2Error::Overflow("claim exposure"))?;
    let ceiling = (bond.collateral as u128)
        .checked_mul(admission.max_exposure_ratio_permille as u128)
        .ok_or(PalwAdmissionV2Error::Overflow("exposure ceiling"))?
        / 1000;
    let would_reserve = reserved.checked_add(claim_exposure).ok_or(PalwAdmissionV2Error::Overflow("reserved exposure"))?;
    if would_reserve > ceiling {
        return Err(PalwAdmissionV2Error::ExposureCeilingExceeded {
            reserved,
            claim: claim_exposure,
            ceiling,
            collateral: bond.collateral,
            ratio_permille: admission.max_exposure_ratio_permille,
        });
    }

    // 10. One identity, one claim — the chain-visible face of no-equivocation (see module doc
    //     for why the cross-branch face lives with the journal and the court).
    let attempt_id = attempt_id_v2(attempt);
    if state.claim(&attempt_id).is_some() {
        return Err(PalwAdmissionV2Error::DuplicateAttempt(attempt_id));
    }

    Ok(attempt_id)
}

/// The composed admission a wiring layer should call: stateless shape → stateless signature →
/// the stateful list, in that order, one entry point — so no pipeline can wire three layers and
/// forget one. (The PoW itself is the finalizer's, upstream of all of this.)
#[allow(clippy::too_many_arguments)]
pub fn check_palw_attempt_admission_full_v2<V>(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    admission: &PalwAdmissionParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    envelope: &PalwAttemptEnvelopeV2,
    verify_mldsa87: V,
) -> Result<Hash64, PalwAdmissionV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    envelope.validate_stateless_v2(network_domain, pre_pow_hash, timestamp, nonce)?;
    envelope.validate_signature_v2(verify_mldsa87)?;
    check_palw_attempt_admission_v2(state, state_params, admission, ctx, envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PalwAttemptUnsignedV2, challenge_v2};
    use crate::palw_state_v2::{PalwConsensusObjectV2, PalwStateDeltaV2, apply_palw_transition_v2};
    use crate::tx::{TransactionId, TransactionOutpoint};
    use kaspa_hashes::Hash64;

    const NET: u64 = 999;
    const PPH: u64 = 5;
    const TS: u64 = 1_700;

    /// Operator identities are DERIVED from a key now, so the fixtures carry a key and let the
    /// state machine mint the id — the same path a real registration takes.
    fn op_key(v: u64) -> Vec<u8> {
        vec![v as u8; 8]
    }

    fn op_id(v: u64) -> Hash64 {
        crate::palw_state_v2::palw_operator_id_v2(&op_key(v))
    }

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn state_params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(
            100,
            10,
            10,
            20,
            500,
            1000,
            h64(1),
            4,
            1000,
            100,
            1000,
            0,
        )
        .unwrap()
    }

    fn admission_params() -> PalwAdmissionParamsV2 {
        PalwAdmissionParamsV2::new(500).unwrap()
    }

    fn bond_outpoint(v: u64) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 }
    }

    fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(block), daa_score: daa, blue_score: blue, subsidy: 0 }
    }

    /// Class 1 (rule: ≤ 500 pwu, 5 sompi/pwu) and bond 1 (key 0x07…, operator 0x21, 1000 sompi).
    fn base_state() -> PalwChainStateV2 {
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                terms: crate::palw_state_v2::PalwClassTermsV2::deterministic_default(),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(500),
                // Every ticket passes, so the OTHER nine items are what these tests measure.
                // The class lottery has its own test below, where the target is the variable.
                initial_target: u128::MAX,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: PalwBondKeyV2(bond_outpoint(1)),
                pubkey: vec![7; 4],
                operator_pubkey: op_key(0x21),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11), signature: Vec::new() },
        ];
        let (state, _) =
            apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
        state
    }

    fn attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
        attempt_for_bond(pwu, nonce, bond_outpoint(1), vec![7; 4], op_id(0x21))
    }

    fn attempt_for_bond(
        pwu: u64,
        nonce: u64,
        bond: TransactionOutpoint,
        pubkey: Vec<u8>,
        operator_id: Hash64,
    ) -> PalwAttemptEnvelopeV2 {
        let challenge = challenge_v2(h64(NET), h64(PPH), TS, nonce, h64(1), &bond);
        PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain: h64(NET),
                challenge,
                class_id: h64(1),
                executor_bond: bond,
                executor_pubkey: pubkey,
                operator_id,
                artifact_root: h64(11),
                trace_root: h64(31),
                output_root: h64(32),
                pwu,
                trace_manifest_root: h64(33),
                trace_chunk_count: 4,
                trace_retention_daa: 999_999,
                execution_root: h64(41),
            },
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        }
    }

    /// The same attempt, for a class other than the liveness floor — what the epoch budget still
    /// caps now that the floor is exempt from it.
    fn attempt_for_class(
        class_id: Hash64,
        pwu: u64,
        nonce: u64,
        bond: TransactionOutpoint,
        pubkey: Vec<u8>,
        operator_id: Hash64,
    ) -> PalwAttemptEnvelopeV2 {
        let mut env = attempt_for_bond(pwu, nonce, bond, pubkey, operator_id);
        env.attempt.class_id = class_id;
        env.attempt.challenge = challenge_v2(h64(NET), h64(PPH), TS, nonce, class_id, &bond);
        env
    }

    /// The floor (h64(1)) at 500‰ and an entrant (h64(2)) at 500‰, both registered at genesis so
    /// both hold an epoch-0 budget, plus a bond fat enough that item 8 is not the variable.
    fn two_class_state() -> PalwChainStateV2 {
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                terms: crate::palw_state_v2::PalwClassTermsV2::deterministic_default(),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(500),
                initial_target: u128::MAX,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(2),
                terms: crate::palw_state_v2::PalwClassTermsV2::deterministic_default(),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(500),
                initial_target: u128::MAX,
                share_permille: 500,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: PalwBondKeyV2(bond_outpoint(2)),
                pubkey: vec![8; 4],
                operator_pubkey: op_key(0x22),
                collateral: 2_000_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11), signature: Vec::new() },
        ];
        let (state, _) =
            apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
        state
    }

    fn admit(state: &PalwChainStateV2, c: &PalwBlockContextV2, env: &PalwAttemptEnvelopeV2) -> Result<Hash64, PalwAdmissionV2Error> {
        check_palw_attempt_admission_v2(state, &state_params(), &admission_params(), c, env)
    }

    fn apply_attempt(state: &PalwChainStateV2, c: &PalwBlockContextV2, env: &PalwAttemptEnvelopeV2) -> PalwChainStateV2 {
        let (next, _d): (PalwChainStateV2, PalwStateDeltaV2) =
            apply_palw_transition_v2(state, &state_params(), c, &[], Some(env)).unwrap();
        next
    }

    #[test]
    fn a_conforming_attempt_admits_and_returns_its_identity() {
        let state = base_state();
        let env = attempt(100, 1);
        let id = admit(&state, &ctx(2, 101, 2), &env).expect("a conforming attempt admits");
        assert_eq!(id, attempt_id_v2(&env.attempt), "admission returns the identity the state machine will key on");
    }

    // ---- the two named red tests (threat model P0-2, P0-10) ----

    /// **P0-2, both faces.** An attacker with no stake names a victim's bond: the carried key is
    /// the attacker's, the bond record's is the victim's — item 2 refuses before any signature
    /// question arises. And with the RIGHT key but a garbage signature, the stateless layer
    /// refuses in the full composer. Neither path admits a block under someone else's stake.
    #[test]
    fn palw_v2_foreign_bond_garbage_signature_rejected() {
        let state = base_state();
        let c = ctx(2, 101, 2);

        // Face 1: the attacker's own key on the victim's bond. Even a signature that VERIFIES
        // under the carried key is refused: the carried key is not the bond's.
        let foreign = attempt_for_bond(100, 1, bond_outpoint(1), vec![9; 4], h64(0x21));
        let always_valid = |_k: &[u8], _m: &[u8], _s: &[u8], _c: &[u8]| true;
        let err = check_palw_attempt_admission_full_v2(
            &state,
            &state_params(),
            &admission_params(),
            &c,
            h64(NET),
            h64(PPH),
            TS,
            1,
            &foreign,
            always_valid,
        )
        .expect_err("a foreign key on someone else's bond must not admit");
        assert_eq!(err, PalwAdmissionV2Error::BondKeyMismatch);

        // Face 2: the right key, a garbage signature. The verifier says no; admission says no —
        // and the context handed to the verifier is this family's own (P0-6).
        let honest = attempt(100, 1);
        let strict = |_k: &[u8], _m: &[u8], sig: &[u8], context: &[u8]| {
            assert_eq!(context, PALW_ATTEMPT_V2_MLDSA87_CONTEXT, "the family chooses its own context; callers cannot");
            sig.iter().any(|b| *b != 0x5A) // the fixture's 0x5A fill is "garbage"
        };
        let err = check_palw_attempt_admission_full_v2(
            &state,
            &state_params(),
            &admission_params(),
            &c,
            h64(NET),
            h64(PPH),
            TS,
            1,
            &honest,
            strict,
        )
        .expect_err("a garbage signature must not admit");
        assert!(matches!(err, PalwAdmissionV2Error::Stateless(PalwAttemptV2Error::SignatureInvalid)));
    }

    /// **P0-10.** Claims reserve `pwu × slash_value` against `collateral × ratio`; the claim that
    /// would cross the ceiling is refused, and a claim RESOLVING re-opens exactly the headroom it
    /// held. Ceiling here: 1000 sompi × 500‰ = 500; each 50-pwu claim reserves 250.
    /// **Audit H1's second half: the per-class target has a reader now.**
    ///
    /// The retarget ran at every epoch boundary, moved a number, and nothing on the V2 lane ever
    /// compared anything to it — so "per-class DAA" was arithmetic with no lottery behind it, and
    /// a strangled target produced no symptom until it hit zero. Admission draws the class ticket
    /// from the attempt's own commitment root, so it is a function of the whole attempt and
    /// cannot be ground without new proof of work.
    #[test]
    fn the_class_target_is_what_admits_a_block_of_that_class() {
        let sp = state_params();
        let ap = admission_params();

        // A target of MAX admits every ticket; one below the attempt's ticket admits none.
        let env = attempt(10, 1);
        let ticket = crate::palw_attempt_v2::class_ticket_v2(&env.attempt);
        assert!(ticket > 0, "a zero ticket would make the check vacuous");

        let state_with = |target: u128| {
            let objects = vec![
                PalwConsensusObjectV2::ClassRegistered {
                    class_id: h64(1),
                    terms: crate::palw_state_v2::PalwClassTermsV2::deterministic_default(),
                    artifact_root: h64(11),
                    slash_value_per_pwu: 5,
                    pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                    initial_target: target,
                    share_permille: 1000,
                    activation_daa: 0,
                    admission: None,
                },
                PalwConsensusObjectV2::BondRegistered {
                    bond: PalwBondKeyV2(bond_outpoint(1)),
                    pubkey: vec![7; 4],
                    operator_pubkey: op_key(0x21),
                    collateral: 1_000_000,
                    payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11), signature: Vec::new() },
            ];
            apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100, 1), &objects, None).unwrap().0
        };

        // Exactly at the target admits — the comparison is inclusive, so a target is reachable
        // rather than asymptotic.
        let at = state_with(ticket);
        check_palw_attempt_admission_v2(&at, &sp, &ap, &ctx(2, 101, 2), &env).expect("a ticket equal to the target admits");

        // One below refuses.
        let under = state_with(ticket - 1);
        let err = check_palw_attempt_admission_v2(&under, &sp, &ap, &ctx(2, 101, 2), &env)
            .expect_err("a ticket above the target is not a block of this class");
        assert!(matches!(err, PalwAdmissionV2Error::ClassTicketAboveTarget { .. }), "got {err:?}");

        // The ticket is a function of the WHOLE attempt: a different nonce is a different ticket,
        // which is why re-rolling it costs a new proof of work rather than a re-hash.
        let other = attempt(10, 2);
        assert_ne!(crate::palw_attempt_v2::class_ticket_v2(&other.attempt), ticket, "the ticket follows the attempt");
        // …and it is not the L1 tag under another name.
        let tag = crate::palw_attempt_v2::l1_tag_v2(crate::palw_attempt_v2::commitment_root_v2(&env.attempt));
        let mut tag_le = [0u8; 16];
        tag_le.copy_from_slice(&tag[..16]);
        assert_ne!(u128::from_le_bytes(tag_le), ticket, "the class lottery is domain-separated from the PoW tag");
    }

    #[test]
    fn palw_v2_bond_exposure_ceiling_enforced() {
        let state = base_state();

        // First claim: 250 of 500 — admits, and is applied to the chain.
        let first = attempt(50, 1);
        admit(&state, &ctx(2, 101, 2), &first).expect("first claim fits");
        let state = apply_attempt(&state, &ctx(2, 101, 2), &first);
        assert_eq!(state.reserved_exposure(&PalwBondKeyV2(bond_outpoint(1))), 250);

        // Second claim: 250 more — exactly at the ceiling, admits.
        let second = attempt(50, 2);
        admit(&state, &ctx(3, 102, 3), &second).expect("the ceiling is inclusive");
        let state = apply_attempt(&state, &ctx(3, 102, 3), &second);

        // Third claim: any further work at all crosses the ceiling.
        let third = attempt(1, 3);
        let err = admit(&state, &ctx(4, 103, 4), &third).expect_err("over-exposure must not admit");
        assert!(matches!(err, PalwAdmissionV2Error::ExposureCeilingExceeded { reserved: 500, claim: 5, ceiling: 500, .. }));

        // A claim resolving (bind timeout at daa > 111) releases its reservation, and the same
        // third claim now admits: the ceiling tracks LIVE exposure, not history.
        let (state, _) = apply_palw_transition_v2(&state, &state_params(), &ctx(5, 120, 5), &[], None).unwrap();
        assert_eq!(state.reserved_exposure(&PalwBondKeyV2(bond_outpoint(1))), 0, "both claims void-timed-out and released");
        admit(&state, &ctx(6, 121, 6), &third).expect("released exposure is headroom again");
    }

    // ---- ADR-0045 Decision 1: pwu has exactly one legal value ----

    /// A second class under `DerivedV1`, beside the fixture's ceiling class. The target is
    /// `u128::MAX / 2` (two expected attempts), the per-inference cost 7 — so the one legal
    /// claim is 14, and everything else is the H3 attack, refused by equality rather than
    /// bounded by a ceiling.
    fn state_with_derived_class(initial_target: u128) -> PalwChainStateV2 {
        let objects = vec![PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(2),
            terms: crate::palw_state_v2::PalwClassTermsV2::deterministic_default(),
            artifact_root: h64(22),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 7 },
            initial_target,
            share_permille: 100,
            activation_daa: 0,
            admission: None,
        }];
        let (state, _) = apply_palw_transition_v2(&base_state(), &state_params(), &ctx(2, 101, 2), &objects, None).unwrap();
        // One block past the epoch boundary (epoch_length 1000), because ADR-0045 Decision 2's
        // budgets are derived per epoch from the share table AS IT STOOD when the epoch opened: a
        // class registered mid-epoch has no budget until the next boundary, and a missing budget
        // is a missing fact rather than a permissive zero. On a live network the rule never bites
        // — classes only enter at genesis, which IS the first block — but a fixture that
        // registers at daa 101 has to cross a boundary before its class can produce.
        let (state, _) = apply_palw_transition_v2(&state, &state_params(), &ctx(3, 1_001, 3), &[], None).unwrap();
        state
    }

    fn derived_class_attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
        let mut env = attempt(pwu, nonce);
        env.attempt.class_id = h64(2);
        env.attempt.artifact_root = h64(22);
        env
    }

    /// The derived class's target is real (unlike the fixture class's pass-everything MAX), so an
    /// ADMIT case must also win the item-6b lottery: hunt the deterministic ticket space for a
    /// nonce that lands under `target`. Refusal cases need no hunt — item 6 fires before 6b.
    fn derived_class_attempt_admitting(pwu: u64, target: u128) -> PalwAttemptEnvelopeV2 {
        for nonce in 0..512 {
            let env = derived_class_attempt(pwu, nonce);
            if crate::palw_attempt_v2::class_ticket_v2(&env.attempt) <= target {
                return env;
            }
        }
        panic!("no admitting nonce in 512 draws at target {target} — the ticket space is broken, not unlucky");
    }

    fn admission_params_with_derived_class() -> PalwAdmissionParamsV2 {
        PalwAdmissionParamsV2::new(500).unwrap()
    }

    #[test]
    fn derived_pwu_admits_exactly_one_value() {
        let state = state_with_derived_class(u128::MAX / 2);
        let admission = admission_params_with_derived_class();
        // Epoch 1, where the derived class has a budget (see `state_with_derived_class`).
        let c = ctx(4, 1_002, 4);
        let derived = crate::palw_pwu::palw_pwu_v1(u128::MAX / 2, 7);
        assert_eq!(derived, 14, "two expected attempts at seven per inference");

        // The one legal value admits (with a nonce that also wins the 6b lottery).
        check_palw_attempt_admission_v2(&state, &state_params(), &admission, &c, &derived_class_attempt_admitting(derived, u128::MAX / 2))
            .expect("the derived claim admits");

        // The H3 attack — claim the maximum — is refused by equality, not by a ceiling.
        let err =
            check_palw_attempt_admission_v2(&state, &state_params(), &admission, &c, &derived_class_attempt(u64::MAX, 2))
                .unwrap_err();
        assert_eq!(err, PalwAdmissionV2Error::PwuClaimNotDerived { claimed: u64::MAX, derived });

        // And so is one unit off in either direction — there is no tolerance band, because
        // neither factor is something the miner chooses.
        for wrong in [derived - 1, derived + 1] {
            let err =
                check_palw_attempt_admission_v2(&state, &state_params(), &admission, &c, &derived_class_attempt(wrong, 3))
                    .unwrap_err();
            assert!(matches!(err, PalwAdmissionV2Error::PwuClaimNotDerived { .. }), "got {err:?}");
        }
    }

    /// The equality is anchored to the CANDIDATE state's target, not to any constant of the
    /// class: the same claim value admits under one target and is refused under another, with
    /// the harder chain demanding proportionally more work per block.
    #[test]
    fn derived_pwu_reads_the_target_at_the_candidate_point() {
        let admission = admission_params_with_derived_class();
        let c = ctx(4, 1_002, 4);

        let easy = state_with_derived_class(u128::MAX / 2); // 2 attempts expected → pwu 14
        let hard = state_with_derived_class(u128::MAX / 8); // 8 attempts expected → pwu 56

        check_palw_attempt_admission_v2(&easy, &state_params(), &admission, &c, &derived_class_attempt_admitting(14, u128::MAX / 2))
            .expect("14 is the easy chain's one legal value");
        let err = check_palw_attempt_admission_v2(&hard, &state_params(), &admission, &c, &derived_class_attempt(14, 1))
            .unwrap_err();
        assert_eq!(
            err,
            PalwAdmissionV2Error::PwuClaimNotDerived { claimed: 14, derived: 56 },
            "the harder chain derives a different — larger — legal value for the same class"
        );
        check_palw_attempt_admission_v2(&hard, &state_params(), &admission, &c, &derived_class_attempt_admitting(56, u128::MAX / 8))
            .expect("56 is the hard chain's one legal value");
    }

    // ---- the remaining items, one refusal each ----

    #[test]
    fn every_stateful_item_refuses_its_own_violation() {
        let state = base_state();
        let c = ctx(2, 101, 2);

        // 1. Unknown bond.
        let unknown_bond = attempt_for_bond(100, 1, bond_outpoint(9), vec![7; 4], h64(0x21));
        assert!(matches!(admit(&state, &c, &unknown_bond), Err(PalwAdmissionV2Error::BondMissing(_))));

        // 9. Retiring bond.
        let (retiring, _) = apply_palw_transition_v2(
            &state,
            &state_params(),
            &c,
            &[PalwConsensusObjectV2::BondRetireRequested { bond: PalwBondKeyV2(bond_outpoint(1)), signature: vec![0xEE; 8] }],
            None,
        )
        .unwrap();
        assert!(matches!(admit(&retiring, &ctx(3, 102, 3), &attempt(100, 1)), Err(PalwAdmissionV2Error::BondRetiring(_))));

        // 3. Wrong operator id.
        let wrong_operator = attempt_for_bond(100, 1, bond_outpoint(1), vec![7; 4], h64(0x99));
        assert_eq!(admit(&state, &c, &wrong_operator).unwrap_err(), PalwAdmissionV2Error::OperatorMismatch);

        // 4a. Unknown class.
        let mut unknown_class = attempt(100, 1);
        unknown_class.attempt.class_id = h64(2);
        assert!(matches!(admit(&state, &c, &unknown_class), Err(PalwAdmissionV2Error::ClassMissing(_))));

        // 4b. Frozen class — an ENTRANT, because ADR-0039 W6′ now refuses freezing the liveness
        // floor (a `ClassFrozen` naming BASE-0 would end the chain: the attempt lane is the only
        // block type and admission refuses a frozen class).
        let two = two_class_state();
        let entrant = attempt_for_class(h64(2), 100, 1, bond_outpoint(2), vec![8; 4], op_id(0x22));
        assert!(admit(&two, &ctx(2, 101, 2), &entrant).is_ok(), "the entrant admits while it is unfrozen");
        let (frozen, _) =
            apply_palw_transition_v2(&two, &state_params(), &ctx(2, 101, 2), &[crate::palw_state_v2::tests::freeze(h64(2))], None)
                .unwrap();
        assert!(matches!(admit(&frozen, &ctx(3, 102, 3), &entrant), Err(PalwAdmissionV2Error::ClassFrozen(_))));

        // 5. Wrong artifact root.
        let mut wrong_artifact = attempt(100, 1);
        wrong_artifact.attempt.artifact_root = h64(0xBAD);
        assert_eq!(admit(&state, &c, &wrong_artifact).unwrap_err(), PalwAdmissionV2Error::ArtifactRootMismatch);

        // 6. Over the class's pwu ceiling (rule is 500).
        let over_rule = attempt(501, 1);
        assert!(matches!(
            admit(&state, &c, &over_rule),
            Err(PalwAdmissionV2Error::PwuExceedsClassRule { claimed: 501, ceiling: 500 })
        ));

        // 10. Duplicate identity. Small enough (50 pwu → 250 of the 500 ceiling) that re-admitting
        // the same claim clears items 1–8 and is refused by the LAST check, the identity's —
        // which is the point: a duplicate is a duplicate even when everything else still fits.
        let env = attempt(50, 1);
        let applied = apply_attempt(&state, &c, &env);
        assert!(matches!(admit(&applied, &ctx(3, 102, 3), &env), Err(PalwAdmissionV2Error::DuplicateAttempt(_))));
    }

    /// **The epoch budget is a count of BLOCKS, sized from the class's share (ADR-0045 D2).**
    ///
    /// It used to be a static per-class pwu ceiling in the bundle — a number with no sizing basis
    /// and the wrong currency. Under `DerivedV1` an attempt's pwu is a function of the class
    /// TARGET, so the same pwu budget buys fewer and fewer blocks as a class gets harder: a class
    /// that got popular would hard-stop its own chain for the rest of every epoch, for getting
    /// popular. The chain derives `budget_blocks` instead, from the share table, the epoch's DAA
    /// span and the tolerance.
    #[test]
    fn the_epoch_budget_is_blocks_derived_from_the_class_share() {
        use crate::palw_state_v2::derive_epoch_budgets_v2;

        use std::collections::BTreeSet;
        let none: BTreeSet<Hash64> = BTreeSet::new();
        let derive = |shares: Vec<(Hash64, u16)>, competing: Vec<Hash64>, span: u64, tol: u32| {
            derive_epoch_budgets_v2(
                &shares.into_iter().collect(),
                &none,
                &competing.into_iter().collect::<BTreeSet<_>>(),
                span,
                tol,
                0,
            )
        };

        // The sizing basis, stated as arithmetic. With an EMPTY census the denominator is the
        // whole unfrozen table (ADR-0045 Decision 2's fresh-chain rule), which is the shape this
        // derivation always had: a class holding the whole table at unity tolerance gets the
        // epoch's whole expected production, so the cap cannot bind on a chain running at cadence.
        let whole = derive(vec![(h64(1), 1000u16)], vec![], 1_000, 1_000);
        assert_eq!(whole.budget_blocks[&h64(1)], 1_000, "the whole table gets the whole epoch");
        let half = derive(vec![(h64(1), 500u16), (h64(2), 500u16)], vec![], 1_000, 1_000);
        assert_eq!(half.budget_blocks[&h64(1)], 500, "half the share, half the blocks");
        let generous = derive(vec![(h64(1), 500u16), (h64(2), 500u16)], vec![], 1_000, 1_500);
        assert_eq!(generous.budget_blocks[&h64(1)], 750, "tolerance is the headroom above the share");
        // Never zero: a class with a share may produce, and a zero budget would freeze it under
        // the name of a cap.
        let tiny = derive(vec![(h64(1), 1u16), (h64(2), 999u16)], vec![], 10, 1_000);
        assert_eq!(tiny.budget_blocks[&h64(1)], 1, "10 · 1‰ rounds to nothing; one block is the floor");

        // **The census denominator (ADR-0045 Decision 2), which this derivation did not have.**
        //
        // `denom_c = Σ shares over the competing set` — the closed epoch's producers. The code
        // read `denom_c = 1000‰`, the whole table, always, so a class whose permille sat idle
        // shrank every producer's cap. That is H1's defect arriving at the budget as a HARD
        // refusal rather than as a slow retarget walk, and on a `ConsensusV2` network a hard
        // refusal of every attempt stops the chain outright.
        let alone = derive(vec![(h64(1), 500u16), (h64(2), 500u16)], vec![h64(1)], 1_000, 1_000);
        assert_eq!(
            alone.budget_blocks[&h64(1)], 1_000,
            "a sole producer is measured against itself — it may hold the whole span it is the whole of"
        );
        // …and a class that did NOT produce is measured against the set PLUS itself, so it can
        // re-enter without being either strangled or handed the whole epoch.
        assert_eq!(alone.budget_blocks[&h64(2)], 500, "a re-entrant competes with the incumbents plus itself");
        // Two producers share, and neither is affected by a third class sitting out.
        let both = derive(
            vec![(h64(1), 400u16), (h64(2), 400u16), (h64(3), 200u16)],
            vec![h64(1), h64(2)],
            1_000,
            1_000,
        );
        assert_eq!(both.budget_blocks[&h64(1)], 500, "400/800 of the span, not 400/1000");
        assert_eq!(both.budget_blocks[&h64(2)], 500);

        // And the chain really installs it: the fixture's class holds the whole table, so its
        // budget is the epoch's span.
        let state = base_state();
        let budgets = state.epoch_budgets().expect("the chain derives budgets for its own epoch");
        assert_eq!(budgets.epoch_index, 101 / state_params().epoch_length());
        assert_eq!(budgets.budget_blocks[&h64(1)], state_params().epoch_length(), "whole share, whole span");
    }

    /// **A class added to a running chain must be able to produce in the epoch it activates in.**
    ///
    /// `ensure_epoch_budgets` returns early once a budget exists for the current epoch, and its
    /// justification is written above it: "the share table is genesis-fixed (a class cannot enter
    /// through a transaction), so a budget derived mid-epoch equals the one the boundary would
    /// have derived."
    ///
    /// That premise no longer holds. `activate_due_classes` grants share at the block where a
    /// registered class reaches its `activation_daa`, which is mid-epoch for any class that did
    /// not enter at genesis — exactly the post-genesis registration path that lets an operator add
    /// a model without re-minting the network. The share lands; the budget table for the epoch in
    /// flight is never recomputed; `palw_producer_facts_v2` reads `unwrap_or(0)`.
    ///
    /// The result is a class that is Active, holds share, and can produce nothing until the next
    /// boundary. What the operator sees is `ready_to_produce` refusing with "this class's epoch
    /// budget is already spent" — which states the opposite of what happened. Nothing was spent;
    /// nothing was ever granted. On a chain whose epoch is 1000 blocks at a 120 s cadence that is
    /// a day and a half of a newly added model mining nothing and reporting exhaustion.
    ///
    /// The floor cannot reveal this, because the floor is exempt from the budget: a missing table
    /// and a missing entry look identical from the only class that keeps producing through both.
    #[test]
    fn a_class_that_activates_mid_epoch_can_produce_in_that_epoch() {
        let state = base_state();
        let epoch_length = state_params().epoch_length();
        // The fixture's block and the entrant's are the same epoch — this is the mid-epoch case,
        // not a boundary crossing, and the assertion below is meaningless without it.
        assert_eq!(100 / epoch_length, 101 / epoch_length, "the fixture must sit inside one epoch");
        assert!(
            state.epoch_budgets().is_some_and(|b| b.epoch_index == 100 / epoch_length),
            "the chain has already installed this epoch's budget before the entrant arrives"
        );

        let entrant = PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(2),
            terms: crate::palw_state_v2::PalwClassTermsV2::deterministic_default(),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(500),
            initial_target: u128::MAX,
            share_permille: 500,
            // Due at the very block that registers it: the shape `--palw-register-class` produces
            // for a class meant to start at once.
            activation_daa: 101,
            admission: None,
        };
        let (next, _) =
            apply_palw_transition_v2(&state, &state_params(), &ctx(2, 101, 2), std::slice::from_ref(&entrant), None)
                .expect("registering a class on a running chain is the supported path");

        assert_eq!(
            next.class_share_permille(&h64(2)),
            Some(500),
            "the entrant activated and holds share — this half already works"
        );
        let budgets = next.epoch_budgets().expect("the chain carries a budget table");
        assert!(
            budgets.budget_blocks.get(&h64(2)).copied().unwrap_or(0) > 0,
            "a class holding share must hold budget in the same epoch: share without budget is an \
             Active class that can produce nothing, reporting its budget as spent"
        );
    }

    /// **The cap still binds — on the classes it is for, which is every class but the floor.**
    ///
    /// A class whose budget is exhausted admits nothing more THIS epoch, and admits again in the
    /// next one. It is an ENTRANT here, not the base class, because the base class is now exempt:
    /// see `the_liveness_floor_is_never_capped_so_the_chain_can_always_end_an_epoch` for why an
    /// exemption is the only deadlock-free shape.
    #[test]
    fn the_epoch_budget_counts_this_chains_production_in_this_epoch() {
        let mut state = two_class_state();
        let budget = state.epoch_budgets().unwrap().budget_blocks[&h64(2)];
        // 500‰ of the epoch's span, against an empty census's whole-table denominator.
        assert_eq!(budget, state_params().epoch_length() / 2);

        let entrant = |nonce: u64| attempt_for_class(h64(2), 1, nonce, bond_outpoint(2), vec![8; 4], op_id(0x22));
        // Fill the budget exactly, all inside one epoch. (Bind-timeout voids along the way release
        // EXPOSURE but never production — the budget counts what was produced.)
        for i in 0..budget {
            let env = entrant(100 + i);
            // One DAA score for all of them: the transition requires `daa_score` not to DECREASE
            // and `blue_score` to strictly increase, and the whole run must stay inside one epoch
            // for the budget to be the one being filled.
            let c = ctx(3 + i, 102, 3 + i);
            admit(&state, &c, &env).unwrap_or_else(|e| panic!("block {i} fits the budget: {e}"));
            state = apply_attempt(&state, &c, &env);
        }
        // One more in the same epoch is refused, and the error names the count, not a pwu sum.
        let overflow = entrant(90_001);
        let err = admit(&state, &ctx(9_000, 150, 9_000), &overflow).expect_err("the budget is exhausted");
        assert!(matches!(err, PalwAdmissionV2Error::EpochBudgetExceeded { produced, claimed: 1, .. } if produced == budget as u128));

        // The same attempt admits in the NEXT epoch: the counter rolled over — but only once the
        // chain has been there, because that is when the next epoch's budgets are derived.
        let (next_epoch, _) = apply_palw_transition_v2(&state, &state_params(), &ctx(9_001, 1_000, 9_001), &[], None).unwrap();
        assert!(admit(&next_epoch, &ctx(9_002, 1_001, 9_002), &overflow).is_ok(), "a new epoch is new budget");

        // A class the epoch's table does not name admits nothing: a missing budget is a missing
        // fact, never a permissive zero. (It fails earlier than item 7 — the class is not
        // registered at all — so the point stands on the budget table itself.)
        assert!(
            !next_epoch.epoch_budgets().unwrap().budget_blocks.contains_key(&h64(0xAB5E)),
            "an unregistered class has no budget entry"
        );
    }

    /// **The deadlock the epoch budget could reach, and the structure that makes it
    /// unrepresentable (audit lane #3).**
    ///
    /// On a `ConsensusV2` network the attempt lane is the ONLY block type —
    /// `required_algo_id_for_mode` demands the bundle's `algorithm_id` of every header — so
    /// refusing every attempt does not slow the chain, it stops it. And the epoch that would
    /// refill the budget ends by DAA, which advances only with blocks. There is no clock to
    /// escape on, because the clock IS the blocks.
    ///
    /// The census denominator fixes the steady state but not the transition: a class measured
    /// against last epoch's producers exhausts its slice the moment those producers go idle. So
    /// the escape is structural — ADR-0039 W6′'s liveness floor is exempt from the cap, and
    /// `ClassFrozen` refuses the floor for the same reason. The floor can always produce, so DAA
    /// always advances, so every other class's cap is a slowdown rather than a death.
    #[test]
    fn the_liveness_floor_is_never_capped_so_the_chain_can_always_end_an_epoch() {
        let mut state = two_class_state();
        let budget = state.epoch_budgets().unwrap().budget_blocks[&h64(1)];
        assert_eq!(
            budget,
            state_params().epoch_length() / 2,
            "the floor holds a budget entry like anyone else — it is simply not consulted"
        );

        // Produce FAR past the floor's own budget, inside one epoch, with the entrant idle. This
        // is exactly the state that used to be terminal.
        let floor = |nonce: u64| attempt_for_class(h64(1), 1, nonce, bond_outpoint(2), vec![8; 4], op_id(0x22));
        for i in 0..(budget + 5) {
            let env = floor(100 + i);
            let c = ctx(3 + i, 102, 3 + i);
            admit(&state, &c, &env).unwrap_or_else(|e| panic!("the liveness floor is never refused by the cap: {e}"));
            state = apply_attempt(&state, &c, &env);
        }
        let produced = state.epoch_counter(&h64(1)).unwrap().produced_blocks;
        assert!(produced > budget, "the floor really produced past the cap ({produced} > {budget})");

        // And the entrant, which sat out, is still capped — the exemption is the floor's alone.
        let entrant = attempt_for_class(h64(2), 1, 90_001, bond_outpoint(2), vec![8; 4], op_id(0x22));
        let c = ctx(9_000, 102, 9_000);
        assert!(admit(&state, &c, &entrant).is_ok(), "an entrant under its own budget still admits");

        // The floor may not be frozen either — a `ClassFrozen` naming it would end the chain by
        // the other door (admission refuses a frozen class, and the floor is the only class every
        // operator can run).
        let err = apply_palw_transition_v2(
            &two_class_state(),
            &state_params(),
            &ctx(2, 101, 2),
            &[crate::palw_state_v2::tests::freeze(h64(1))],
            None,
        )
        .unwrap_err();
        assert_eq!(err, crate::palw_state_v2::PalwStateV2Error::BaseClassMayNotFreeze(h64(1)));
    }

    #[test]
    fn admission_params_refuse_the_permissive_zeros() {
        assert!(PalwAdmissionParamsV2::new(0).is_err(), "zero ratio admits no work at all");
        assert!(PalwAdmissionParamsV2::new(1).is_ok());
    }

    /// The full composer refuses stateless violations before touching state: a wrong-position
    /// challenge never reaches the bond lookup.
    #[test]
    fn the_full_composer_runs_stateless_first() {
        let state = base_state();
        let mut env = attempt(100, 1);
        env.attempt.challenge = h64(0xDEAD);
        let err = check_palw_attempt_admission_full_v2(
            &state,
            &state_params(),
            &admission_params(),
            &ctx(2, 101, 2),
            h64(NET),
            h64(PPH),
            TS,
            1,
            &env,
            |_k, _m, _s, _c| true,
        )
        .expect_err("a mispositioned attempt fails statelessly");
        assert!(matches!(err, PalwAdmissionV2Error::Stateless(PalwAttemptV2Error::ChallengeMismatch)));
    }
}
