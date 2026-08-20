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
use std::collections::BTreeMap;

/// Admission's own network constants. Constructed only through [`PalwAdmissionParamsV2::new`];
/// like the state params, they are part of the atomic ruleset bundle (ADR-0042 Decision 1) and
/// the fingerprint commits to them (Decision 11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwAdmissionParamsV2 {
    /// Decision 6's `max_exposure_ratio`, in permille of the bond's slashable collateral. The
    /// ceiling is `collateral × ratio / 1000`, floored — conservative, and identical everywhere.
    max_exposure_ratio_permille: u32,
    /// Per-class epoch production budget in pwu (ADR-0039 D5's share table, as the number this
    /// predicate needs). A class absent from this table CANNOT be admitted — a missing budget is
    /// a missing fact, and ADR-0039 D5e already refuses starved tables at startup, so a zero
    /// budget is refused at construction rather than carried as a dead class.
    class_epoch_budget_pwu: BTreeMap<Hash64, u128>,
}

impl PalwAdmissionParamsV2 {
    pub fn new(
        max_exposure_ratio_permille: u32,
        class_epoch_budget_pwu: BTreeMap<Hash64, u128>,
    ) -> Result<Self, PalwAdmissionV2Error> {
        if max_exposure_ratio_permille == 0 {
            return Err(PalwAdmissionV2Error::InvalidParams("a zero exposure ratio admits no work at all"));
        }
        if class_epoch_budget_pwu.values().any(|budget| *budget == 0) {
            return Err(PalwAdmissionV2Error::InvalidParams("a zero epoch budget is a starved class (ADR-0039 D5e)"));
        }
        Ok(Self { max_exposure_ratio_permille, class_epoch_budget_pwu })
    }

    pub fn max_exposure_ratio_permille(&self) -> u32 {
        self.max_exposure_ratio_permille
    }

    pub fn class_epoch_budget_pwu(&self, class_id: &Hash64) -> Option<u128> {
        self.class_epoch_budget_pwu.get(class_id).copied()
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
    #[error("class {0} does not exist at the candidate chain point")]
    ClassMissing(Hash64),
    #[error("class {0} is frozen and admits no new work")]
    ClassFrozen(Hash64),
    #[error("the carried artifact root is not the class's registered root")]
    ArtifactRootMismatch,
    #[error("claimed pwu {claimed} exceeds the class rule's ceiling {ceiling}")]
    PwuExceedsClassRule { claimed: u64, ceiling: u64 },
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
    if let PalwClassStatusV2::Frozen { .. } = class.status {
        return Err(PalwAdmissionV2Error::ClassFrozen(attempt.class_id));
    }

    // 5. The artifact the trace claims to open against is the one the class registered.
    if class.artifact_root != attempt.artifact_root {
        return Err(PalwAdmissionV2Error::ArtifactRootMismatch);
    }

    // 6. The pwu claim against the class rule (pwu ≥ 1 is stateless).
    match class.pwu_rule {
        PalwPwuRuleV2::MaxPerAttempt(ceiling) => {
            if attempt.pwu > ceiling {
                return Err(PalwAdmissionV2Error::PwuExceedsClassRule { claimed: attempt.pwu, ceiling });
            }
        }
    }

    // 7. The epoch budget, as a predicate on the PRODUCING block's own selected-chain class
    //    production: the counter the candidate chain accumulated, read at the producing block's
    //    epoch. A counter from an older epoch contributes zero — the epoch rolled over.
    let budget =
        admission.class_epoch_budget_pwu(&attempt.class_id).ok_or(PalwAdmissionV2Error::EpochBudgetUnspecified(attempt.class_id))?;
    let epoch_index = ctx.daa_score / state_params.epoch_length();
    let produced = match state.epoch_counter(&attempt.class_id) {
        Some(counter) if counter.epoch_index == epoch_index => counter.produced_pwu,
        _ => 0,
    };
    let claimed = attempt.pwu as u128;
    let would_produce = produced.checked_add(claimed).ok_or(PalwAdmissionV2Error::Overflow("epoch production"))?;
    if would_produce > budget {
        return Err(PalwAdmissionV2Error::EpochBudgetExceeded { class_id: attempt.class_id, produced, claimed, budget });
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
            crate::palw_state_v2::PalwClassDaaV2Params::new([(h64(1), 1000u16)].into_iter().collect(), 4).unwrap(),
        )
        .unwrap()
    }

    fn admission_params() -> PalwAdmissionParamsV2 {
        PalwAdmissionParamsV2::new(500, [(h64(1), 10_000u128)].into_iter().collect()).unwrap()
    }

    fn bond_outpoint(v: u64) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 }
    }

    fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(block), daa_score: daa, blue_score: blue }
    }

    /// Class 1 (rule: ≤ 500 pwu, 5 sompi/pwu) and bond 1 (key 0x07…, operator 0x21, 1000 sompi).
    fn base_state() -> PalwChainStateV2 {
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(500),
                initial_target: u128::MAX / 2,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: PalwBondKeyV2(bond_outpoint(1)),
                pubkey: vec![7; 4],
                operator_id: h64(0x21),
                collateral: 1_000,
            },
        ];
        let (state, _) =
            apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
        state
    }

    fn attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
        attempt_for_bond(pwu, nonce, bond_outpoint(1), vec![7; 4], h64(0x21))
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
            },
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        }
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
            &[PalwConsensusObjectV2::BondRetireRequested { bond: PalwBondKeyV2(bond_outpoint(1)) }],
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

        // 4b. Frozen class.
        let (frozen, _) =
            apply_palw_transition_v2(&state, &state_params(), &c, &[PalwConsensusObjectV2::ClassFrozen { class_id: h64(1) }], None)
                .unwrap();
        assert!(matches!(admit(&frozen, &ctx(3, 102, 3), &attempt(100, 1)), Err(PalwAdmissionV2Error::ClassFrozen(_))));

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

    /// Item 7: the predicate is on the producing block's own selected-chain production. The same
    /// class that exhausted THIS epoch's budget admits again in the next epoch — and a class with
    /// no budget entry admits nowhere.
    #[test]
    fn the_epoch_budget_counts_this_chains_production_in_this_epoch() {
        // Budget 10_000 pwu; rule ceiling 500 per attempt; epoch length 1000. A dedicated
        // high-collateral bond (ceiling 200_000 × 500‰ = 100_000 ≫ any live reserve here) keeps
        // item 8 out of the way: this test is about item 7 and only item 7.
        let mut state = {
            let objects = vec![PalwConsensusObjectV2::BondRegistered {
                bond: PalwBondKeyV2(bond_outpoint(2)),
                pubkey: vec![8; 4],
                operator_id: h64(0x22),
                collateral: 200_000,
            }];
            let (s, _) = apply_palw_transition_v2(&base_state(), &state_params(), &ctx(2, 101, 2), &objects, None).unwrap();
            s
        };
        let rich_attempt = |pwu: u64, nonce: u64| attempt_for_bond(pwu, nonce, bond_outpoint(2), vec![8; 4], h64(0x22));
        // 20 × 500 pwu = exactly the budget, inside epoch 0. (Bind-timeout voids along the way
        // release EXPOSURE but never production — the budget counts what was produced.)
        for i in 0..20u64 {
            let env = rich_attempt(500, 100 + i);
            let c = ctx(3 + i, 102 + i, 3 + i);
            admit(&state, &c, &env).unwrap_or_else(|e| panic!("claim {i} fits the budget: {e}"));
            state = apply_attempt(&state, &c, &env);
        }
        // The 21st in the same epoch is refused.
        let overflow = rich_attempt(1, 990);
        let err = admit(&state, &ctx(30, 130, 30), &overflow).expect_err("the budget is exhausted");
        assert!(matches!(err, PalwAdmissionV2Error::EpochBudgetExceeded { produced: 10_000, .. }));

        // The same attempt admits in the NEXT epoch (daa 1000+): the counter rolled over.
        assert!(admit(&state, &ctx(31, 1_000, 31), &overflow).is_ok(), "a new epoch is new budget");

        // A class with no budget entry: refused as unspecified, never as unlimited.
        let empty_budget = PalwAdmissionParamsV2::new(500, BTreeMap::new()).unwrap();
        let err = check_palw_attempt_admission_v2(&state, &state_params(), &empty_budget, &ctx(31, 1_000, 31), &overflow)
            .expect_err("no budget entry, no admission");
        assert!(matches!(err, PalwAdmissionV2Error::EpochBudgetUnspecified(_)));
    }

    #[test]
    fn admission_params_refuse_the_permissive_zeros() {
        assert!(PalwAdmissionParamsV2::new(0, BTreeMap::new()).is_err(), "zero ratio");
        assert!(PalwAdmissionParamsV2::new(1, [(h64(1), 0u128)].into_iter().collect()).is_err(), "zero budget = starved class");
        assert!(PalwAdmissionParamsV2::new(1, [(h64(1), 1u128)].into_iter().collect()).is_ok());
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
