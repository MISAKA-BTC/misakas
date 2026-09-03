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
    ///
    /// **The ratio is `1..=1000` permille, and the upper bound is load-bearing** (ADR-0062
    /// SA-7(c), re-check). A bond's ceiling is `collateral × ratio / 1000`, while every charge the
    /// fold can make against it is capped by `slash_bond` at the collateral itself. So a ratio
    /// above unity lets one bond hold more concurrent exposure than it can ever pay: the first
    /// refutations drain the collateral, `slash_bond` returns early at a zero debit, and the rest
    /// are free — which is the exact behaviour the DA court's exposure ceiling exists to prevent,
    /// re-created by a genesis-time number. `PalwStateParamsV2::with_fp_exposure_ceiling` already
    /// refused it on the state side; this side only refused zero, and `palw_mode_v2` requires the
    /// two to be EQUAL rather than bounded, so the invariant "the K-th refutation is funded exactly
    /// like the first" rested on a bundle nobody checked. It is checked here, and again in
    /// `validate_ruleset_shape` for a bundle that arrives deserialized rather than constructed.
    pub fn new(max_exposure_ratio_permille: u32) -> Result<Self, PalwAdmissionV2Error> {
        if max_exposure_ratio_permille == 0 {
            return Err(PalwAdmissionV2Error::InvalidParams("a zero exposure ratio admits no work at all"));
        }
        if max_exposure_ratio_permille > 1000 {
            return Err(PalwAdmissionV2Error::InvalidParams(
                "an exposure ratio above 1000 permille lets a bond back more than it can ever pay — the refutations past its \
                 collateral are free",
            ));
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
    #[error("the trace retention {claimed} is not the derived obligation {derived} (the block's DAA plus the lattice windows)")]
    TraceRetentionNotDerived { claimed: u64, derived: u64 },
    #[error("the trace chunk count {claimed} is not the canonical {canonical}")]
    TraceChunkCountNotCanonical { claimed: u32, canonical: u32 },
    #[error("the trace manifest root {claimed} is not the one the trace root derives ({derived})")]
    TraceManifestNotDerived { claimed: Hash64, derived: Hash64 },
    #[error("class {0} has no epoch budget entry — a missing budget admits nothing, not everything")]
    EpochBudgetUnspecified(Hash64),
    #[error("epoch budget exceeded for class {class_id}: produced {produced} + claimed {claimed} > budget {budget}")]
    EpochBudgetExceeded { class_id: Hash64, produced: u128, claimed: u128, budget: u128 },
    #[error(
        "bond exposure ceiling exceeded: reserved {reserved} + this claim {claim} > ceiling {ceiling} \
         (collateral {collateral} × {ratio_permille}‰)"
    )]
    ExposureCeilingExceeded { reserved: u128, claim: u128, ceiling: u128, collateral: u64, ratio_permille: u32 },
    #[error(
        "this claim would escrow {escrow} sompi of reward against {reserved} of collateral, and the network \
         requires at least {required} ({backing_permille}‰ of the escrow) — the class's registered slash value \
         is too low for the reward its work draws"
    )]
    EscrowExceedsCollateralBacking { escrow: u64, reserved: u128, required: u128, backing_permille: u32 },
    #[error("attempt {0} already has a claim on this chain — one identity, one claim")]
    DuplicateAttempt(Hash64),
    #[error("arithmetic overflow in {0}")]
    Overflow(&'static str),
    #[error("class {class_id} is registered but weightless until DAA {activation_daa} — it holds no cadence share yet")]
    ClassNotYetActive { class_id: Hash64, activation_daa: u64 },
    /// ADR-0056 Decision 5: the class was reclaimed for producing nothing. Re-registering it is
    /// the way back, at the grant floor and a fresh soak.
    #[error("class {class_id} was reclaimed as dormant at DAA {since_daa} and holds no share")]
    ClassDormant { class_id: Hash64, since_daa: u64 },
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
    check_palw_producer_entitlement_v2_with_bootstrap(state, attempt, None)
}

/// **ADR-0064 — the bond becomes usable in the block that registers it.**
///
/// `bootstrap_bond` is the record for THIS attempt's bond as declared by `BondRegistered` in this
/// block's own mergeset, and it is supplied only past `palw_bootstrap_activation`. With it, a bond
/// becomes usable by the block that ACCEPTS its registration rather than by that block's child —
/// one chain block earlier for a producer joining a live chain.
///
/// **Not a recovery mechanism, despite the ADR's original title** (see its correction): a block's
/// own body is never in its own mergeset, so the newcomer still needs somebody else's block to
/// carry the registration, and on a chain with no producers there is none.
///
/// This is not a new rule so much as the removal of a disagreement: `apply_palw_transition_v4`
/// applies accepted objects at step 3 and the block's own work at step 4, so the state machine
/// ALREADY accepts such a block. Only this pre-check, resolving against the walk state at the
/// selected parent, refuses — two answers to one question, and this time the answer that stops the
/// chain.
///
/// **Exactly one lookup moves.** The class, the class target, the pwu equality, the epoch budget,
/// the ticket and the exposure ceiling all keep reading the parent state. Widening this to the
/// whole admission would make item 6's strict `DerivedV1` equality compare a post-retarget target
/// against a pwu the producer derived pre-retarget, so on a multi-class chain the first block of
/// every epoch would be disqualified and the chain would stop every 1000 DAA. testnet-11 runs three
/// classes.
pub fn check_palw_producer_entitlement_v2_with_bootstrap(
    state: &PalwChainStateV2,
    attempt: &crate::palw_attempt_v2::PalwAttemptUnsignedV2,
    bootstrap_bond: Option<&crate::palw_state_v2::PalwBondStateV2>,
) -> Result<(), PalwAdmissionV2Error> {
    // 1 + 9. The bond, at the candidate point, in one status read.
    let bond_key = PalwBondKeyV2(attempt.executor_bond);
    let bond = state.bond(&bond_key).or(bootstrap_bond).ok_or(PalwAdmissionV2Error::BondMissing(bond_key))?;
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
        // ADR-0056 Decision 5: reclaimed for producing nothing. It holds no share and has no
        // budget entry, exactly like a pre-activation class, and it is refused for the same
        // reason — with its own name, because "you produced nothing for long enough that the chain
        // took the permille back" and "you have not activated yet" are different facts and an
        // operator has to be able to tell which one they are looking at.
        PalwClassStatusV2::Dormant { since_daa } => {
            return Err(PalwAdmissionV2Error::ClassDormant { class_id: attempt.class_id, since_daa });
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
    check_palw_attempt_admission_v2_with_bootstrap(state, state_params, admission, ctx, envelope, None)
}

/// [`check_palw_attempt_admission_v2`] with ADR-0064's mergeset bond view. See
/// [`check_palw_producer_entitlement_v2_with_bootstrap`] for what `bootstrap_bond` is and, more
/// importantly, for what deliberately does NOT read it.
///
/// **This is the envelope-only list.** Everything in it is decidable from the chain state and the
/// envelope, which is why the state machine can re-run it as a transition guard with no header in
/// hand. What it does NOT contain, since ADR-0072, is the class lottery: that ticket is a function
/// of the header position too, and it is checked beside the position in
/// [`check_palw_class_lottery_v3`] — from the composed entry point, the way the network lottery is
/// checked in the finalizer and never here.
pub fn check_palw_attempt_admission_v2_with_bootstrap(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    admission: &PalwAdmissionParamsV2,
    ctx: &PalwBlockContextV2,
    envelope: &PalwAttemptEnvelopeV2,
    bootstrap_bond: Option<&crate::palw_state_v2::PalwBondStateV2>,
) -> Result<Hash64, PalwAdmissionV2Error> {
    let attempt = &envelope.attempt;

    // Items 1–5: the producer's entitlement, shared verbatim with the coinbase's question.
    check_palw_producer_entitlement_v2_with_bootstrap(state, attempt, bootstrap_bond)?;
    let bond_key = PalwBondKeyV2(attempt.executor_bond);
    let bond = state.bond(&bond_key).or(bootstrap_bond).expect("entitlement resolved this bond");
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
            let target = state.class_target(&attempt.class_id).ok_or(PalwAdmissionV2Error::ClassTargetMissing(attempt.class_id))?;
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
        let budget =
            *budgets.budget_blocks.get(&attempt.class_id).ok_or(PalwAdmissionV2Error::EpochBudgetUnspecified(attempt.class_id))?;
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

    // 6b. The CLASS lottery is `check_palw_class_lottery_v3` (ADR-0072). Its ticket is a function
    //     of the header position as well as the attempt, so it is checked where the header is —
    //     the composed entry point — and this list, which the state machine re-runs with only the
    //     envelope in hand, deliberately holds nothing the envelope alone cannot decide.

    // 8. The exposure ceiling (closes P0-10): what this bond already backs, plus what this claim
    //    would reserve, against collateral × ratio. Floor on the ceiling — conservative.
    // **ADR-0056 Decision 3: the ceiling is checked against BOTH ledgers.** A bond's claims and
    // the classes it registered draw on one collateral, so flooding the registry and producing
    // blocks compete for the same capital — which is the whole mechanism, and it lives here, at
    // the one place a ceiling is applied. Two accumulators, summed at the point of use.
    let reserved = state
        .reserved_exposure(&bond_key)
        .checked_add(state.registration_exposure(&bond_key))
        .ok_or(PalwAdmissionV2Error::Overflow("total exposure"))?;
    // **Priced on ONE inference, not on the difficulty** — see `palw_exposure_pwu_v1`. Using
    // `attempt.pwu` here made the ceiling a function of the class target: a class that retargets
    // harder reserves more against unchanged collateral, so its own producers are locked out for
    // succeeding, and on the floor class that is the chain stopping. This must stay the same
    // expression the state uses when it writes `claim.reserved`, or the ceiling would be checked
    // against a number the ledger never records.
    let claim_exposure = (crate::palw_state_v2::palw_exposure_pwu_v1(class, attempt.pwu) as u128)
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

    // 9. **A claim may not escrow more reward than the collateral it puts at risk backs.**
    //
    //    `claim_exposure` above is the whole downside of producing: default on the data
    //    obligation, lose a court, and this is what burns. The upside is `escrow` — the block's
    //    worker carve, withheld at acceptance and released at `Final`. Nothing related the two,
    //    and priced against the market the token actually trades at they were four orders of
    //    magnitude apart: a live claim on testnet-11 escrowed 2,756 MSK while reserving 0.0015.
    //
    //    That gap is not a small fee, it is the sign flipping on the whole lattice. A producer
    //    that opens claims and vanishes costs the panel real work — five seats verify, the
    //    material is served and retained — and pays 0.00005 % of what the claim was worth. The
    //    exposure CEILING bounds how many such claims one bond may hold at once; it says nothing
    //    about whether holding one is worth anything, and a ceiling over a zero is still a zero.
    //
    //    So the ratio is a network constant, and the class's registered `slash_value_per_pwu` is
    //    what has to satisfy it — the same value every class must share with the floor
    //    (`SlashValueNotTheNetworks`), so this is one inequality for the whole chain rather than
    //    a per-class knob a registrant could set against its own claims.
    //
    //    Refused here, not at registration: the escrow is a function of THIS block's subsidy,
    //    which a registration cannot know and which the emission schedule moves. Refusing an
    //    attempt is the established shape for "the chain will not back this work" (items 7 and 8
    //    both do it).
    //
    //    **AND THE LIVENESS FLOOR IS EXEMPT, for the reason item 7 spells out ninety lines above.**
    //    This block's earlier text said a refusal here "costs its own producer the reward instead
    //    of stalling the chain". That is false on a `ConsensusV2` network and item 7 already
    //    explains why: the attempt lane is the only block type, so an attempt refused on the chain
    //    block's own header is `StatusDisqualifiedFromChain` — no block, so DAA does not advance,
    //    so nothing that depends on a clock can recover, because the clock IS the blocks.
    //
    //    That is not a footnote, it is the whole reason `min_slash_permille_of_escrow` has never
    //    left 0. `claim_exposure` is `pwu_per_inference x slash_value_per_pwu`, and both factors
    //    really are chain-fixed — the genesis gate pins the first to the catalog's counted step
    //    leaves, `SlashValueNotTheNetworks` pins the second — so the permille the floor can
    //    satisfy is a constant of the shipped economy and it is far below 1. A value of 1 refuses
    //    the floor's own attempt and halts the chain permanently. An economic gate whose only
    //    settable value is "off" is not a gate, and this exemption is what makes the parameter
    //    raisable at all: past it, a non-zero permille refuses the classes that are a producer's
    //    choice and can never refuse the one class every node must be able to produce.
    //
    //    **The figures this paragraph used to quote (0.00028 permille; 3,600x) were computed when
    //    `claim_exposure` was priced on `attempt.pwu`** — which under `DerivedV1` carries an extra
    //    `expected_attempts(class_target)` factor and therefore moved with every retarget. Pricing
    //    an exposure ceiling on the difficulty is what `palw_exposure_pwu_v1` removed; the numbers
    //    are smaller by exactly that factor and are deliberately not restated here, because the
    //    old ones were quoted as facts about the economy while being facts about one target.
    //
    //    What is still required to arm it is money, not code: 1 permille needs
    //    `slash_value_per_pwu` about 3,600x today's, which sizes genesis bond collateral at
    //    roughly 0.83 % of the 10B cap. 100 permille would need 83 % of it and is not shippable.
    //    Raise the two together or `From<NetworkId>` panics at boot on the genesis gate, and arm
    //    ADR-0065 D4 first — `void_and_slash` takes `claim.reserved` unconditionally, so a larger
    //    slash value multiplies the false convictions before it deters a single real one.
    if attempt.class_id != state_params.base_class_id() {
        let escrow = crate::palw_reward_v2::palw_reward_carve_v2(
            ctx.subsidy,
            &crate::palw_reward_v2::PalwRewardParamsV2::new(state_params.worker_carve_permille())
                .map_err(|_| PalwAdmissionV2Error::InvalidParams("the worker carve is not a legal permille"))?,
        )
        .worker as u128;
        let backing_permille = state_params.min_slash_permille_of_escrow() as u128;
        let required = escrow.checked_mul(backing_permille).ok_or(PalwAdmissionV2Error::Overflow("escrow backing"))? / 1000;
        if claim_exposure < required {
            return Err(PalwAdmissionV2Error::EscrowExceedsCollateralBacking {
                escrow: escrow as u64,
                reserved: claim_exposure,
                required,
                backing_permille: backing_permille as u32,
            });
        }
    }

    // 10. One identity, one claim — the chain-visible face of no-equivocation (see module doc
    //     for why the cross-branch face lives with the journal and the court).
    let attempt_id = attempt_id_v2(attempt);
    if state.claim(&attempt_id).is_some() {
        return Err(PalwAdmissionV2Error::DuplicateAttempt(attempt_id));
    }

    Ok(attempt_id)
}

/// **Item 6b, the CLASS lottery** (ADR-0039's per-class DAA), drawn per ADR-0072.
///
/// The network target decided this header is a block; this decides it is a block of this class,
/// against the target the retarget maintains. Without it the per-class retarget was arithmetic
/// nothing consumed — it ran every epoch, moved a number, and no admission, weight or selection
/// ever read it (audit H1's second half). A class target with no reader is a difficulty that does
/// not exist.
///
/// It stands outside the envelope-only list because its ticket is a function of the HEADER
/// POSITION as well as the attempt: [`crate::palw_attempt_v2::class_ticket_v3`] hashes the
/// execution commitment under the anchor a verifier derives from the template, class, bond and
/// nonce bucket, so the only way to another draw is another inference — the discipline the
/// receipt lane has had since ADR-0044 Decision 4. `class_ticket_v2` hashed the identity root,
/// which carried the position inside the challenge; that let the lottery sit in the envelope-only
/// list, and it also let one inference draw a fresh ticket per nonce. The anchor is never read off
/// the attempt (the accused does not set the question): the composed entry point derives it from
/// the header, after the stateless list has agreed that the carried domain and challenge ARE this
/// network's and this position's.
pub fn check_palw_class_lottery_v3(
    state: &PalwChainStateV2,
    attempt: &crate::palw_attempt_v2::PalwAttemptUnsignedV2,
    execution_anchor: Hash64,
) -> Result<(), PalwAdmissionV2Error> {
    let target = state.class_target(&attempt.class_id).ok_or(PalwAdmissionV2Error::ClassTargetMissing(attempt.class_id))?;
    let ticket = crate::palw_attempt_v2::class_ticket_v3(attempt, execution_anchor);
    if ticket > target.target {
        return Err(PalwAdmissionV2Error::ClassTicketAboveTarget { class_id: attempt.class_id, ticket, target: target.target });
    }
    Ok(())
}

/// **The data-availability pins** (ADR-0072 Decision 8): every field inside the priced bytes is an
/// equality against chain state, a value the panel replays, or the challenge — and the three DA
/// fields were none of those. A producer chose them, nothing read `trace_manifest_root`, nothing
/// pinned `trace_chunk_count` beyond `!= 0`, and any `trace_retention_daa` at or above the honest
/// minimum was harmless to it: 2^64 free draws on both lotteries from one inference, with honest
/// roots so the panel convicts nothing. So each is pinned to what it always should have been:
///
/// * `trace_chunk_count == PALW_ATTEMPT_V2_TRACE_CHUNKS` — the one shape every family serves;
/// * `trace_manifest_root == attempt_trace_manifest_root_v1(trace_root, count)` — a function of a
///   value the panel replays;
/// * `trace_retention_daa == daa_score + palw_min_trace_retention_daa_v1(params)` — the obligation
///   the chain defines, at the block's OWN DAA score (a merged block's, not the accepting block's,
///   which is why this takes the header's score rather than reading `ctx`).
///
/// Checked from the composed entry point, beside the lottery: the retention pin needs the header,
/// and the envelope-only list is what the state machine re-runs without one.
pub fn check_palw_attempt_da_pins_v1(
    state_params: &PalwStateParamsV2,
    attempt: &crate::palw_attempt_v2::PalwAttemptUnsignedV2,
    daa_score: u64,
) -> Result<(), PalwAdmissionV2Error> {
    let canonical = crate::palw_attempt_v2::PALW_ATTEMPT_V2_TRACE_CHUNKS;
    if attempt.trace_chunk_count != canonical {
        return Err(PalwAdmissionV2Error::TraceChunkCountNotCanonical { claimed: attempt.trace_chunk_count, canonical });
    }
    let derived = crate::palw_attempt_v2::attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
    if attempt.trace_manifest_root != derived {
        return Err(PalwAdmissionV2Error::TraceManifestNotDerived { claimed: attempt.trace_manifest_root, derived });
    }
    let derived = daa_score.saturating_add(crate::palw_producer_v2::palw_min_trace_retention_daa_v1(state_params));
    if attempt.trace_retention_daa != derived {
        return Err(PalwAdmissionV2Error::TraceRetentionNotDerived { claimed: attempt.trace_retention_daa, derived });
    }
    Ok(())
}

/// The composed admission a wiring layer should call: stateless shape → stateless signature →
/// the DA pins → the stateful list → the class lottery, in that order, one entry point — so no
/// pipeline can wire five layers and forget one. (The PoW itself is the finalizer's, upstream of
/// all of this.) `daa_score` is the HEADER's own — a merged block's, not the accepting block's.
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
    daa_score: u64,
    envelope: &PalwAttemptEnvelopeV2,
    verify_mldsa87: V,
) -> Result<Hash64, PalwAdmissionV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    check_palw_attempt_admission_full_v2_with_bootstrap(
        state,
        state_params,
        admission,
        ctx,
        network_domain,
        pre_pow_hash,
        timestamp,
        nonce,
        daa_score,
        envelope,
        verify_mldsa87,
        None,
    )
}

/// [`check_palw_attempt_admission_full_v2`] with ADR-0064's mergeset bond view.
///
/// The stateless binding and the SIGNATURE run first and are untouched by the fence: a
/// mergeset-declared bond still has to be named by an attempt its own key signed. The bootstrap
/// record answers "is this bond registered", never "did its holder authorise this".
#[allow(clippy::too_many_arguments)]
pub fn check_palw_attempt_admission_full_v2_with_bootstrap<V>(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    admission: &PalwAdmissionParamsV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    daa_score: u64,
    envelope: &PalwAttemptEnvelopeV2,
    verify_mldsa87: V,
    bootstrap_bond: Option<&crate::palw_state_v2::PalwBondStateV2>,
) -> Result<Hash64, PalwAdmissionV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    envelope.validate_stateless_v2(network_domain, pre_pow_hash, timestamp, nonce)?;
    envelope.validate_signature_v2(verify_mldsa87)?;
    check_palw_attempt_da_pins_v1(state_params, &envelope.attempt, daa_score)?;
    let attempt_id = check_palw_attempt_admission_v2_with_bootstrap(state, state_params, admission, ctx, envelope, bootstrap_bond)?;
    // The anchor is derived HERE, from the header, after the stateless list has agreed that the
    // carried domain IS this network's and the carried challenge IS this position's — so it names
    // the job this header was paid for by, and the attempt has had no say in it (ADR-0072).
    let execution_anchor = crate::palw_attempt_v2::execution_anchor_v3(
        network_domain,
        pre_pow_hash,
        envelope.attempt.class_id,
        &envelope.attempt.executor_bond,
        nonce,
    );
    check_palw_class_lottery_v3(state, &envelope.attempt, execution_anchor)?;
    Ok(attempt_id)
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

    /// **ADR-0072**: the class ticket is drawn under the anchor a VERIFIER derives from the header.
    /// Every fixture sits in nonce bucket 0, so one anchor per (class, bond) is exactly the value
    /// `check_palw_attempt_admission_full_v2` would derive for it.
    fn anchor_of(env: &PalwAttemptEnvelopeV2) -> Hash64 {
        crate::palw_attempt_v2::execution_anchor_v3(h64(NET), h64(PPH), env.attempt.class_id, &env.attempt.executor_bond, 0)
    }

    /// The envelope-only list and then the class lottery, in the order the composed entry point
    /// runs them — for the tests whose fixture class has a REAL target (the base fixture's is MAX,
    /// which admits every ticket, so the list alone is the whole question there).
    fn check_with_lottery(
        state: &PalwChainStateV2,
        state_params: &PalwStateParamsV2,
        admission: &PalwAdmissionParamsV2,
        ctx: &PalwBlockContextV2,
        envelope: &PalwAttemptEnvelopeV2,
    ) -> Result<Hash64, PalwAdmissionV2Error> {
        check_palw_attempt_da_pins_v1(state_params, &envelope.attempt, ctx.daa_score)?;
        let id = check_palw_attempt_admission_v2(state, state_params, admission, ctx, envelope)?;
        check_palw_class_lottery_v3(state, &envelope.attempt, anchor_of(envelope))?;
        Ok(id)
    }

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn state_params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 1000, 0).unwrap()
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
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
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
                trace_manifest_root: crate::palw_attempt_v2::attempt_trace_manifest_root_v1(h64(31), 1),
                trace_chunk_count: 1,
                // The envelope-only list never reads this; the tests that reach the composed
                // entry point's DA pins set it to the derived value for their block (`pinned_at`).
                trace_retention_daa: 999_999,
                execution_root: h64(41),
            },
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        }
    }

    /// The retention the DA pin demands for a block at `daa` (ADR-0072 Decision 8).
    fn pinned_at(mut env: PalwAttemptEnvelopeV2, daa: u64) -> PalwAttemptEnvelopeV2 {
        env.attempt.trace_retention_daa =
            daa.saturating_add(crate::palw_producer_v2::palw_min_trace_retention_daa_v1(&state_params()));
        env
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
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
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
        // Pinned for this block, so the DA pins (which run before the stateful list) let the
        // attack reach the item it is an attack on.
        let foreign = pinned_at(attempt_for_bond(100, 1, bond_outpoint(1), vec![9; 4], h64(0x21)), 101);
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
            101,
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
            101,
            &honest,
            strict,
        )
        .expect_err("a garbage signature must not admit");
        assert!(matches!(err, PalwAdmissionV2Error::Stateless(PalwAttemptV2Error::SignatureInvalid)));
    }

    /// **ADR-0064 — the deadlock, and the one lookup that closes it.**
    ///
    /// A chain whose producers have all stopped cannot restart: producing needs an attempt naming a
    /// registered bond, and a bond is registered by an object that needs a block. This asserts the
    /// deadlock is real (a bond the parent state has never seen is refused), that supplying the
    /// mergeset-declared record admits exactly that block, and — the part that matters — that
    /// nothing ELSE is relaxed by it.
    ///
    /// Both positions, because a switch no fixture exercises in both is how four heartbeat findings
    /// reached the audit.
    #[test]
    fn a_bond_registered_in_this_blocks_own_mergeset_admits_this_blocks_own_attempt() {
        let sp = state_params();
        let ap = admission_params();
        let c = ctx(2, 101, 2);

        // A state that knows the CLASS but has never seen this bond — a chain with no live
        // producer, and a newcomer holding a registration nobody has yet accepted.
        let objects = vec![PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(500),
            initial_target: u128::MAX,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        }];
        let (classes_only, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100, 1), &objects, None).unwrap();

        let env = attempt(10, 1);
        let bond_key = PalwBondKeyV2(env.attempt.executor_bond);

        // FENCE OFF: this is the deadlock. Nobody can produce, so nobody can register, so nobody
        // can produce.
        let err = check_palw_attempt_admission_v2(&classes_only, &sp, &ap, &c, &env).unwrap_err();
        assert!(
            matches!(err, PalwAdmissionV2Error::BondMissing(k) if k == bond_key),
            "without the fence the chain is wedged shut, which is the state ADR-0064 exists for: {err:?}"
        );

        // FENCE ON: the record this block's own mergeset declares, built by the SAME constructor the
        // state transition uses, so the pre-check and the transition cannot disagree about what a
        // fresh bond is.
        let declared = crate::palw_state_v2::palw_bond_state_from_registration_v2(
            &[7u8; 4],
            &op_key(0x21),
            1_000,
            kaspa_hashes::Hash64::from_u64_word(0x9A11),
            101,
            Default::default(),
        );
        check_palw_attempt_admission_v2_with_bootstrap(&classes_only, &sp, &ap, &c, &env, Some(&declared))
            .expect("one ordinary block carrying its own registration is how a stopped chain restarts");

        // **The narrowing.** The bootstrap record is not a skeleton key: a bond it declares is still
        // subject to every other item. Here the same registration is offered for a class the chain
        // does not have, and admission still refuses — on the CLASS, not the bond.
        let mut wrong_class = attempt(10, 1);
        wrong_class.attempt.class_id = h64(999);
        wrong_class.attempt.challenge = challenge_v2(h64(NET), h64(PPH), TS, 1, h64(999), &wrong_class.attempt.executor_bond);
        let err =
            check_palw_attempt_admission_v2_with_bootstrap(&classes_only, &sp, &ap, &c, &wrong_class, Some(&declared)).unwrap_err();
        assert!(
            matches!(err, PalwAdmissionV2Error::ClassMissing(_)),
            "only the BOND lookup moves; everything else still reads the parent state: {err:?}"
        );

        // And it is not a way to smuggle a mismatched key: item 2 still compares the carried key to
        // the bond's own, whichever registry answered.
        let impostor = crate::palw_state_v2::palw_bond_state_from_registration_v2(
            &[9u8; 4],
            &op_key(0x21),
            1_000,
            kaspa_hashes::Hash64::from_u64_word(0x9A11),
            101,
            Default::default(),
        );
        let err = check_palw_attempt_admission_v2_with_bootstrap(&classes_only, &sp, &ap, &c, &env, Some(&impostor)).unwrap_err();
        assert!(
            matches!(err, PalwAdmissionV2Error::BondKeyMismatch),
            "a mergeset-declared bond is still the bond it says it is: {err:?}"
        );
    }

    /// **P0-10.** Claims reserve `pwu × slash_value` against `collateral × ratio`; the claim that
    /// would cross the ceiling is refused, and a claim RESOLVING re-opens exactly the headroom it
    /// held. Ceiling here: 1000 sompi × 500‰ = 500; each 50-pwu claim reserves 250.
    /// **Audit H1's second half: the per-class target has a reader now.**
    ///
    /// The retarget ran at every epoch boundary, moved a number, and nothing on the V2 lane ever
    /// compared anything to it — so "per-class DAA" was arithmetic with no lottery behind it, and
    /// a strangled target produced no symptom until it hit zero. Admission draws the class ticket
    /// from the attempt's execution commitment (ADR-0072), so it is a function of the inference
    /// and cannot be ground without running another one.
    #[test]
    fn the_class_target_is_what_admits_a_block_of_that_class() {
        let sp = state_params();
        let ap = admission_params();

        // A target of MAX admits every ticket; one below the attempt's ticket admits none.
        let env = pinned_at(attempt(10, 1), 101);
        let ticket = crate::palw_attempt_v2::class_ticket_v3(&env.attempt, anchor_of(&env));
        assert!(ticket > 0, "a zero ticket would make the check vacuous");

        let state_with = |target: u128| {
            let objects = vec![
                PalwConsensusObjectV2::ClassRegistered {
                    class_id: h64(1),
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
                    payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
                    capable_classes: Default::default(),
                    signature: Vec::new(),
                },
            ];
            apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100, 1), &objects, None).unwrap().0
        };

        // Exactly at the target admits — the comparison is inclusive, so a target is reachable
        // rather than asymptotic.
        let at = state_with(ticket);
        check_with_lottery(&at, &sp, &ap, &ctx(2, 101, 2), &env).expect("a ticket equal to the target admits");

        // One below refuses.
        let under = state_with(ticket - 1);
        let err = check_with_lottery(&under, &sp, &ap, &ctx(2, 101, 2), &env)
            .expect_err("a ticket above the target is not a block of this class");
        assert!(matches!(err, PalwAdmissionV2Error::ClassTicketAboveTarget { .. }), "got {err:?}");

        // The ticket is a function of the EXECUTION (ADR-0072): a different nonce in the same
        // bucket is the SAME ticket — re-rolling it costs a new inference, not a re-hash — and a
        // different execution is a different ticket.
        let other = pinned_at(attempt(10, 2), 101);
        assert_eq!(crate::palw_attempt_v2::class_ticket_v3(&other.attempt, anchor_of(&other)), ticket, "a nonce is not a draw");
        let mut rerun = pinned_at(attempt(10, 1), 101);
        rerun.attempt.execution_root = h64(42);
        assert_ne!(crate::palw_attempt_v2::class_ticket_v3(&rerun.attempt, anchor_of(&rerun)), ticket, "an inference is");
        // …and it is not the L1 tag under another name.
        let tag = crate::palw_attempt_v2::l1_tag_v2(crate::palw_attempt_v2::execution_commitment_v3(&env.attempt, anchor_of(&env)));
        let mut tag_le = [0u8; 16];
        tag_le.copy_from_slice(&tag[..16]);
        assert_ne!(u128::from_le_bytes(tag_le), ticket, "the class lottery is domain-separated from the PoW tag");
    }

    /// **A field the producer may choose and no rule pins is a nonce by another name** — the
    /// ADR-0072 review's finding, kept as the test that would have caught it. Before Decision 8,
    /// sweeping `trace_retention_daa` over one execution gave a distinct ticket and a distinct
    /// Layer-0 tag per value and admitted about one in 2^9 of them at a 2^-9 target: free draws
    /// on both lotteries, with honest roots, so the panel had nothing to convict. Now every DA
    /// field is pinned by equality at the composed entry point, and at most the ONE derived value
    /// of each survives to the lottery at all.
    #[test]
    fn a_free_field_inside_the_priced_bytes_is_a_nonce_by_another_name() {
        let target = u128::MAX >> 9;
        let state = state_with_derived_class(target);
        let admission = admission_params_with_derived_class();
        let c = ctx(4, 1_002, 4);
        let pwu = crate::palw_pwu::palw_pwu_v1(target, 7);
        let base = derived_class_attempt(pwu, 1);

        // Retention: only the derived value reaches the lottery.
        let mut reached = Vec::new();
        let pinned = base.attempt.trace_retention_daa;
        for retention in (0u64..4096).filter(|r| *r != pinned).chain([pinned, u64::MAX]) {
            let mut env = base.clone();
            env.attempt.trace_retention_daa = retention;
            match check_with_lottery(&state, &state_params(), &admission, &c, &env) {
                Err(PalwAdmissionV2Error::TraceRetentionNotDerived { claimed, .. }) => assert_eq!(claimed, retention),
                Ok(_) | Err(PalwAdmissionV2Error::ClassTicketAboveTarget { .. }) => reached.push(retention),
                Err(other) => panic!("a retention sweep must fail at the pin or reach the lottery, got {other:?}"),
            }
        }
        assert_eq!(reached, vec![pinned], "exactly one retention value is admissible");

        // Chunk count: only the canonical one.
        for count in [0u32, 2, 3, 8, 1 << 20, u32::MAX] {
            let mut env = base.clone();
            env.attempt.trace_chunk_count = count;
            let err = check_with_lottery(&state, &state_params(), &admission, &c, &env).expect_err("a re-chunking is refused");
            assert!(
                matches!(err, PalwAdmissionV2Error::TraceChunkCountNotCanonical { claimed, .. } if claimed == count),
                "got {err:?}"
            );
        }

        // Manifest root: only the one the trace root derives.
        for word in 0u64..64 {
            let mut env = base.clone();
            env.attempt.trace_manifest_root = h64(0x3300 + word);
            let err = check_with_lottery(&state, &state_params(), &admission, &c, &env).expect_err("a free manifest is refused");
            assert!(matches!(err, PalwAdmissionV2Error::TraceManifestNotDerived { .. }), "got {err:?}");
        }
        // …and the derived one, with a different trace root, is a different execution — which the
        // panel replays. That is the only way the manifest moves.
        let mut rerun = base.clone();
        rerun.attempt.trace_root = h64(0x7B);
        rerun.attempt.trace_manifest_root = crate::palw_attempt_v2::attempt_trace_manifest_root_v1(h64(0x7B), 1);
        match check_with_lottery(&state, &state_params(), &admission, &c, &rerun) {
            Ok(_) | Err(PalwAdmissionV2Error::ClassTicketAboveTarget { .. }) => {}
            Err(other) => panic!("a re-derived manifest reaches the lottery, got {other:?}"),
        }
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
            artifact_root: h64(22),
            slash_value_per_pwu: 5,
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
        // Every test of the derived class sits at `ctx(4, 1_002, 4)`, and the composed entry
        // point's DA pin wants the retention derived for THAT block.
        pinned_at(env, 1_002)
    }

    /// The derived class's target is real (unlike the fixture class's pass-everything MAX), so an
    /// ADMIT case must also win the item-6b lottery: hunt the deterministic ticket space for an
    /// EXECUTION that lands under `target`. Not a nonce — under ADR-0072 every nonce in the bucket
    /// draws the same ticket, and what a producer re-rolls is the inference; here that is the
    /// execution root. Refusal cases need no hunt — item 6 fires before 6b.
    fn derived_class_attempt_admitting(pwu: u64, target: u128) -> PalwAttemptEnvelopeV2 {
        for execution in 0..512u64 {
            let mut env = derived_class_attempt(pwu, 1);
            env.attempt.execution_root = h64(0x4100 + execution);
            if crate::palw_attempt_v2::class_ticket_v3(&env.attempt, anchor_of(&env)) <= target {
                return env;
            }
        }
        panic!("no admitting execution in 512 draws at target {target} — the ticket space is broken, not unlucky");
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
        // Two expected draws, and a draw is an inference (ADR-0072), so two executions at seven
        // per inference.
        assert_eq!(derived, 14, "two executions at seven per inference");

        // The one legal value admits (with an execution that also wins the 6b lottery).
        check_with_lottery(&state, &state_params(), &admission, &c, &derived_class_attempt_admitting(derived, u128::MAX / 2))
            .expect("the derived claim admits");

        // The H3 attack — claim the maximum — is refused by equality, not by a ceiling.
        let err = check_with_lottery(&state, &state_params(), &admission, &c, &derived_class_attempt(u64::MAX, 2)).unwrap_err();
        assert_eq!(err, PalwAdmissionV2Error::PwuClaimNotDerived { claimed: u64::MAX, derived });

        // And so is one unit off in either direction — there is no tolerance band, because
        // neither factor is something the miner chooses.
        for wrong in [derived - 1, derived + 1] {
            let err = check_with_lottery(&state, &state_params(), &admission, &c, &derived_class_attempt(wrong, 3)).unwrap_err();
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

        // The EASY side has to stay easy: `derived_class_attempt_admitting` searches 512 executions
        // for a ticket, so a target far under 2^-9 has no admitting draw to find. The HARD side
        // never reaches the lottery — item 6 refuses its pwu first — so it can be as hard as it
        // likes. Under ADR-0072 a draw is an execution, so the two derivations differ by exactly
        // the ratio of the targets; there is no longer a bucket below which they would agree.
        let easy_target = u128::MAX / 2; // 2 draws = 2 executions → pwu 14
        let hard_target = u128::MAX >> 24; // 2^24 draws = 2^24 executions → pwu 7 · 2^24
        let easy = state_with_derived_class(easy_target);
        let hard = state_with_derived_class(hard_target);
        let easy_pwu = crate::palw_pwu::palw_pwu_v1(easy_target, 7);
        let hard_pwu = crate::palw_pwu::palw_pwu_v1(hard_target, 7);
        assert!(hard_pwu > easy_pwu, "a harder chain must derive a larger value or this proves nothing");

        check_with_lottery(&easy, &state_params(), &admission, &c, &derived_class_attempt_admitting(easy_pwu, easy_target))
            .expect("the easy chain's one legal value");
        let err = check_with_lottery(&hard, &state_params(), &admission, &c, &derived_class_attempt(easy_pwu, 1)).unwrap_err();
        assert_eq!(
            err,
            PalwAdmissionV2Error::PwuClaimNotDerived { claimed: easy_pwu, derived: hard_pwu },
            "the harder chain derives a different — larger — legal value for the same class"
        );
        // The hard chain's own legal value is asserted through the refusal above rather than by
        // producing a block for it: a 2^-24 target has no admitting draw inside the 512 executions
        // the fixture searches — which is the same fact its pwu now states.
        assert_eq!(hard_pwu, crate::palw_pwu::palw_pwu_v1(hard_target, 7), "the refusal named the hard chain's derivation");
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
            alone.budget_blocks[&h64(1)],
            1_000,
            "a sole producer is measured against itself — it may hold the whole span it is the whole of"
        );
        // …and a class that did NOT produce is measured against the set PLUS itself, so it can
        // re-enter without being either strangled or handed the whole epoch.
        assert_eq!(alone.budget_blocks[&h64(2)], 500, "a re-entrant competes with the incumbents plus itself");
        // Two producers share, and neither is affected by a third class sitting out.
        let both = derive(vec![(h64(1), 400u16), (h64(2), 400u16), (h64(3), 200u16)], vec![h64(1), h64(2)], 1_000, 1_000);
        assert_eq!(both.budget_blocks[&h64(1)], 500, "400/800 of the span, not 400/1000");
        assert_eq!(both.budget_blocks[&h64(2)], 500);

        // And the chain really installs it: the fixture's class holds the whole table, so its
        // budget is the epoch's span.
        let state = base_state();
        let budgets = state.epoch_budgets().expect("the chain derives budgets for its own epoch");
        assert_eq!(budgets.epoch_index, 101 / state_params().epoch_length());
        assert_eq!(budgets.budget_blocks[&h64(1)], state_params().epoch_length(), "whole share, whole span");
    }

    /// **A class added to a running chain is NOT budgeted until the next boundary.** A defect,
    /// asserted as it ships rather than as it should be.
    ///
    /// `ensure_epoch_budgets` returns as soon as a budget exists for the current epoch, while
    /// `activate_due_classes` runs one step earlier in the same transition and grants share
    /// mid-epoch to any class reaching its `activation_daa` — the post-genesis registration path.
    /// So the class is Active, holds share, and `palw_producer_facts_v2` reads its budget through
    /// `unwrap_or(0)`. What the operator sees is "this class's epoch budget is already spent",
    /// which states the opposite of what happened: nothing was spent, nothing was granted.
    ///
    /// **Also covering the table fixes it, and that fix was reverted.** On a chain that has
    /// already run it is not a fix: it recomputes the table at the block where the class
    /// activated, changing that block's PALW state root. Measured on testnet-11 — the class
    /// registered at 05:30:37, block 981c9fde… at 05:33:04 committed 932853…, and a node carrying
    /// the change computes 4ba21e… and disqualifies it. Such a node cannot sync the chain at all.
    /// Fencing by DAA would need a field in `PalwStateParamsV2`, which is borsh-serialized into
    /// `palw_ruleset_id_v2` and thus into `consensus_params_id`, so even a fence set to "never"
    /// moves the fingerprint and stops every node peering. The fix belongs to a network that
    /// carries it from genesis.
    ///
    /// The floor cannot reveal any of this, being exempt from the budget: from the only class that
    /// keeps producing, a missing table and a missing entry look the same.
    #[test]
    fn a_class_that_activates_mid_epoch_is_not_budgeted_until_the_next_boundary() {
        let state = base_state();
        let epoch_length = state_params().epoch_length();
        assert_eq!(100 / epoch_length, 101 / epoch_length, "the fixture must sit inside one epoch");
        assert!(
            state.epoch_budgets().is_some_and(|b| b.epoch_index == 100 / epoch_length),
            "the chain has already installed this epoch's budget before the entrant arrives"
        );

        let entrant = PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(2),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(500),
            initial_target: u128::MAX,
            share_permille: 500,
            activation_daa: 101,
            admission: None,
        };
        let (next, _) = apply_palw_transition_v2(&state, &state_params(), &ctx(2, 101, 2), std::slice::from_ref(&entrant), None)
            .expect("registering a class on a running chain is the supported path");

        assert_eq!(next.class_share_permille(&h64(2)), Some(500), "the entrant activated and holds share");
        let budgets = next.epoch_budgets().expect("the chain carries a budget table");
        assert_eq!(
            budgets.budget_blocks.get(&h64(2)).copied().unwrap_or(0),
            0,
            "and holds NO budget in this epoch — share without budget, which is the defect this \
             test exists to keep visible rather than to hide"
        );
    }

    /// **The next boundary grants what the mid-epoch activation did not** — which is why the
    /// defect above is a delay and not a permanent exclusion, and why a fleet running the
    /// unfixed code recovers on its own without being upgraded.
    ///
    /// `ensure_epoch_budgets` recomputes whenever the stored table is for another epoch. The fix
    /// changes only WHEN it recomputes, never WHAT `derive_epoch_budgets_v2` returns, so a chain
    /// that crosses a boundary lands on the same table either way. Asserted because a live network
    /// was told it would recover at its next boundary, and "it will fix itself" is exactly the kind
    /// of claim that should not rest on reading the control flow.
    #[test]
    fn a_boundary_grants_a_budget_the_mid_epoch_activation_missed() {
        let state = base_state();
        let epoch_length = state_params().epoch_length();

        // The entrant activates mid-epoch, which is where it gets share and no budget.
        let entrant = PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(2),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(500),
            initial_target: u128::MAX,
            share_permille: 500,
            activation_daa: 101,
            admission: None,
        };
        let (mid, _) = apply_palw_transition_v2(&state, &state_params(), &ctx(2, 101, 2), std::slice::from_ref(&entrant), None)
            .expect("the entrant registers");
        assert_eq!(mid.class_share_permille(&h64(2)), Some(500), "it holds share from activation");

        // Now cross into the next epoch, carrying nothing — just a block.
        let next_epoch_daa = (101 / epoch_length + 1) * epoch_length;
        let (after, _) = apply_palw_transition_v2(&mid, &state_params(), &ctx(3, next_epoch_daa, 3), &[], None)
            .expect("a block lands in the next epoch");

        let budgets = after.epoch_budgets().expect("the boundary installs a table");
        assert_eq!(budgets.epoch_index, next_epoch_daa / epoch_length, "and it is this epoch's");
        assert!(
            budgets.budget_blocks.get(&h64(2)).copied().unwrap_or(0) > 0,
            "the entrant has a budget from the boundary on — this is the self-recovery, and it does \
             not depend on the mid-epoch fix"
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

    /// **…and the permissive HIGH end** (ADR-0062 SA-7(c), re-check).
    ///
    /// The DA court's exposure ceiling is `collateral × ratio / 1000`; `slash_bond` can never debit
    /// more than `collateral` in total and returns early at a zero debit. So the ceiling is a bound
    /// on money — "the K-th refutation is funded exactly like the first" — only while the ratio is
    /// at most unity. Above it, one bond holds more concurrent accusations than it can pay for, the
    /// first refutations empty it and the rest are free: the griefing the ceiling exists to stop,
    /// re-created by a genesis-time constant. The invariant was written in prose in
    /// `palw_state_v2`'s margin and enforced nowhere on this side, which is a sentence, not a rule.
    #[test]
    fn an_exposure_ratio_above_unity_is_refused_where_the_value_is_admitted() {
        assert!(PalwAdmissionParamsV2::new(1_000).is_ok(), "unity itself is admissible — a bond may back exactly its collateral");
        let err = PalwAdmissionParamsV2::new(1_001).expect_err("above unity the ceiling outruns what any refutation can collect");
        assert!(matches!(err, PalwAdmissionV2Error::InvalidParams(_)), "got {err:?}");
        assert!(
            crate::palw_state_v2::PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 1000, 0)
                .unwrap()
                .with_fp_exposure_ceiling(1_001)
                .is_err(),
            "and the state side, which carries the same number for the transition, refuses it too"
        );
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
            101,
            &env,
            |_k, _m, _s, _c| true,
        )
        .expect_err("a mispositioned attempt fails statelessly");
        assert!(matches!(err, PalwAdmissionV2Error::Stateless(PalwAttemptV2Error::ChallengeMismatch)));
    }

    /// The rule ships OFF, and that is load-bearing rather than timid: testnet-11's registered
    /// slash value cannot satisfy any non-zero backing, so a default that switched it on would
    /// refuse every attempt the live chain has ever accepted.
    #[test]
    fn a_dormant_backing_admits_what_it_always_admitted() {
        let params = state_params();
        assert_eq!(params.min_slash_permille_of_escrow(), 0, "the default is off");
        // A block with a real subsidy, so the escrow is non-zero and only the dormant rule can
        // be what lets this through.
        let mut context = ctx(2, 101, 2);
        context.subsidy = 1_000_000_000;
        check_palw_attempt_admission_v2(&base_state(), &params, &admission_params(), &context, &attempt(10, 1))
            .expect("a dormant rule refuses nothing");
    }

    /// **The live network's own ratio, refused.** Class 2 reserves 5 sompi per pwu, so a 10-pwu
    /// attempt risks 50 against a carve of 620 sompi — the same shape as testnet-11's 0.0015 MSK
    /// against 2,756, and the reason this rule exists.
    ///
    /// Over an ENTRANT class, not the floor: the floor is exempt (see
    /// `the_liveness_floor_is_never_refused_for_its_backing`), so asserting the refusal on it would
    /// have been asserting the one case this rule must never produce. That is what these tests did
    /// before the exemption existed, and it is why the rule looked shippable when it was not.
    #[test]
    fn a_claim_that_earns_far_more_than_it_risks_is_refused() {
        let params = state_params().with_worker_carve_permille(620).unwrap().with_min_slash_permille_of_escrow(100).unwrap();
        let mut context = ctx(2, 101, 2);
        context.subsidy = 1_000; // carve = 620; 100‰ of it is 62, and the claim reserves 50.
        let entrant = attempt_for_class(h64(2), 10, 1, bond_outpoint(2), vec![8; 4], op_id(0x22));
        let err = check_palw_attempt_admission_v2(&two_class_state(), &params, &admission_params(), &context, &entrant)
            .expect_err("50 does not back 620");
        match err {
            PalwAdmissionV2Error::EscrowExceedsCollateralBacking { escrow, reserved, required, backing_permille } => {
                assert_eq!((escrow, reserved, required, backing_permille), (620, 50, 62, 100));
            }
            other => panic!("expected the backing refusal, got {other:?}"),
        }
    }

    /// Enough collateral behind the same reward, and it admits. The producer's own `pwu` is what
    /// moves here — the class's slash value is the network's and no attempt may vary it — so the
    /// rule prices WORK against reward rather than gating who may produce.
    #[test]
    fn the_same_reward_is_admitted_once_the_work_backs_it() {
        let params = state_params().with_worker_carve_permille(620).unwrap().with_min_slash_permille_of_escrow(100).unwrap();
        let mut context = ctx(2, 101, 2);
        context.subsidy = 1_000;
        // 13 pwu x 5 = 65 >= 62.
        let entrant = attempt_for_class(h64(2), 13, 1, bond_outpoint(2), vec![8; 4], op_id(0x22));
        check_palw_attempt_admission_v2(&two_class_state(), &params, &admission_params(), &context, &entrant)
            .expect("65 backs 620 at 100‰");
    }

    /// **The liveness floor is never refused for its backing, and that is what makes the parameter
    /// settable at all.**
    ///
    /// On a `ConsensusV2` network the attempt lane is the only block type, so an attempt refused on
    /// the chain block's own header is `StatusDisqualifiedFromChain` — no block, so DAA does not
    /// advance, so there is no clock to recover on. Item 7 exempts the floor for exactly this and
    /// says so at length; item 9 did not, and `claim_exposure` is
    /// `pwu_per_inference x slash_value_per_pwu` with both factors chain-fixed, so the permille the
    /// floor can satisfy is a constant of the shipped economy and far below 1. A value of 1 would
    /// have halted the chain permanently.
    ///
    /// So this asserts the floor is admitted at a backing it CANNOT satisfy — the same numbers the
    /// entrant is refused for two tests above, which is what makes it a difference in the rule
    /// rather than in the fixture.
    #[test]
    fn the_liveness_floor_is_never_refused_for_its_backing() {
        let params = state_params().with_worker_carve_permille(620).unwrap().with_min_slash_permille_of_escrow(1000).unwrap();
        assert_eq!(params.base_class_id(), h64(1), "class 1 is the floor in this fixture");
        let mut context = ctx(2, 101, 2);
        context.subsidy = 1_000; // carve 620; at 1000‰ the requirement is 620 and the claim reserves 50.
        check_palw_attempt_admission_v2(&base_state(), &params, &admission_params(), &context, &attempt(10, 1))
            .expect("the floor produces whatever the backing is set to, or the chain has no clock");

        // And the same attempt on an entrant class, at the same numbers, is refused — the gate is
        // exempting the floor, not switched off.
        let entrant = attempt_for_class(h64(2), 10, 1, bond_outpoint(2), vec![8; 4], op_id(0x22));
        assert!(
            matches!(
                check_palw_attempt_admission_v2(&two_class_state(), &params, &admission_params(), &context, &entrant),
                Err(PalwAdmissionV2Error::EscrowExceedsCollateralBacking { .. })
            ),
            "an entrant class is still priced"
        );
    }

    /// A block that funds no escrow has nothing to back, whatever the backing is set to — the
    /// inequality must not become a floor on producing out of a zero-subsidy block.
    #[test]
    fn a_block_with_no_subsidy_needs_no_backing() {
        let params = state_params().with_worker_carve_permille(620).unwrap().with_min_slash_permille_of_escrow(1000).unwrap();
        check_palw_attempt_admission_v2(&base_state(), &params, &admission_params(), &ctx(2, 101, 2), &attempt(1, 1))
            .expect("no escrow, no requirement");
    }

    /// The pwu the shipped floor class derives for one block. Pinned here rather than computed so
    /// that a change to the derivation fails this test with both numbers in the message, instead
    /// of silently re-deriving whatever the code now says — which is how it caught ADR-0071
    /// Decision 2 (15,416 was two expected TRIES' worth while both tries fell inside one nonce
    /// bucket, so it became 7,708: the one execution the block commits to) and how it caught
    /// ADR-0072 going the other way: two expected draws at the floor's `2^-1` target are two
    /// EXECUTIONS now that the ticket is the execution, so 2 × 7,708 again.
    ///
    /// **ADR-0076 moved it a third time, and this time the derivation did not change** — the
    /// floor's TARGET did. The floor no longer shares the model tiers' seed: it holds 22‰ of a
    /// table whose dearest class counts 348× its work, so its seed is `MAX/12,663` and one block
    /// is 12,664 expected executions rather than two. 12,664 × 7,708 = 97,606,404. A floor block
    /// weighing six thousand times more than it did is the arithmetic working: the floor produces
    /// six thousand times less often, and `palw_pwu`'s identity keeps a class's weight per unit of
    /// real work where it was.
    const PALW_RC_FLOOR_DERIVED_PWU: u64 = 97_606_404;

    /// **The economic deterrent is armable on the network this build ships — that is the claim,
    /// and it is not the same as the rule being correct.**
    ///
    /// Every test above runs on the synthetic `state_params()` fixture. They establish that the
    /// inequality prices entrants and exempts the floor. What they cannot establish is the thing
    /// that actually blocked this parameter for its whole life: on a `ConsensusV2` network the
    /// attempt lane is the ONLY block type, so a refusal on the chain block's own header is
    /// `StatusDisqualifiedFromChain` — no block, no DAA, no clock. A backing that refused the
    /// floor would not cost a producer a reward, it would stop the chain, and no fixture built out
    /// of hand-made params can show that it does not.
    ///
    /// So this arms the maximum backing on the SHIPPED bundle, boots the shipped genesis through
    /// the real transition, and produces under the floor class with a real subsidy behind it. The
    /// backing lives in `PalwStateParamsV2`, inside the bundle, so arming it is a mint-time choice
    /// rather than a fence — which makes "can a network be minted with it on" exactly the question.
    #[test]
    fn the_escrow_backing_can_be_raised_from_zero_on_the_shipped_network() {
        use crate::config::params::palw_rc_shipped_params;
        use crate::palw_mode_v2::PalwConsensusMode;
        use crate::palw_state_v2::PalwChainStateV2;

        let mut params = palw_rc_shipped_params();
        let PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else {
            panic!("the shipped RC preset carries a V2 bundle");
        };
        assert_eq!(bundle.state.min_slash_permille_of_escrow(), 0, "it ships off, and that was not a choice");

        // 1000‰ — the whole escrow, the largest value the constructor admits. If any setting can
        // halt the floor this one does, so a pass here covers every smaller arming.
        bundle.state = bundle.state.clone().with_min_slash_permille_of_escrow(1000).expect("1000 permille is legal");
        params.validate_palw_v2().expect("a network may be minted with the backing fully armed");

        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { unreachable!() };
        let sp = bundle.state.clone();
        let genesis_ctx =
            PalwBlockContextV2 { block: params.genesis.hash, daa_score: params.genesis.daa_score, blue_score: 0, subsidy: 0 };
        let (booted, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &genesis_ctx, &bundle.genesis_objects, None)
            .expect("the shipped genesis applies");

        let (bond, pubkey, operator_pubkey) = bundle
            .genesis_objects
            .iter()
            .find_map(|o| match o {
                PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } => {
                    Some((*bond, pubkey.clone(), operator_pubkey.clone()))
                }
                _ => None,
            })
            .expect("the shipped genesis registers bonds");
        let class_id = sp.base_class_id();
        let artifact_root = bundle
            .genesis_objects
            .iter()
            .find_map(|o| match o {
                PalwConsensusObjectV2::ClassRegistered { class_id: c, artifact_root, .. } if *c == class_id => Some(*artifact_root),
                _ => None,
            })
            .expect("the floor class is registered at genesis");

        // A block with a real subsidy behind it, so the escrow is large and the requirement at
        // 1000‰ is the whole of it — the case that would refuse the floor if it were not exempt.
        let context =
            PalwBlockContextV2 { block: h64(0xB1), daa_score: params.genesis.daa_score + 1, blue_score: 1, subsidy: 50_000_000_000 };
        let floor = PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain: h64(0xD0),
                challenge: challenge_v2(h64(0xD0), h64(PPH), TS, 1, class_id, &bond.0),
                class_id,
                executor_bond: bond.0,
                executor_pubkey: pubkey,
                operator_id: crate::palw_state_v2::palw_operator_id_v2(&operator_pubkey),
                artifact_root,
                trace_root: h64(31),
                output_root: h64(32),
                // The floor pins its pwu with `DerivedV1`, so this is the only value it accepts —
                // and it is why `claim_exposure` is chain-fixed and the backing cannot be met by
                // a producer choosing to risk more. Item 9's whole problem in one field.
                pwu: PALW_RC_FLOOR_DERIVED_PWU,
                trace_manifest_root: h64(33),
                trace_chunk_count: 4,
                trace_retention_daa: 999_999,
                execution_root: h64(41),
            },
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        };
        check_palw_attempt_admission_v2(&booted, &sp, &bundle.admission, &context, &floor)
            .expect("the liveness floor produces with the backing fully armed, or the network has no clock");

        // And the exemption is doing that, not a zero escrow: at these numbers the requirement is
        // the entire worker carve and the claim reserves a small fraction of it, so the same
        // attempt on any class that is NOT the floor is refused. `a_claim_that_earns_far_more_than_
        // it_risks_is_refused` shows that refusal end-to-end on a two-class fixture; here it is
        // enough to record that the escrow this block funds is real.
        let carve = crate::palw_reward_v2::palw_reward_carve_v2(
            context.subsidy,
            &crate::palw_reward_v2::PalwRewardParamsV2::new(sp.worker_carve_permille()).unwrap(),
        )
        .worker;
        assert!(carve > 0, "the block funds a real escrow, so the exemption is what admitted the floor");
    }

    #[test]
    fn a_backing_above_the_whole_escrow_is_refused_at_construction() {
        assert!(state_params().with_min_slash_permille_of_escrow(1001).is_err(), "a claim cannot risk more than it can earn");
        assert!(state_params().with_min_slash_permille_of_escrow(1000).is_ok(), "risking exactly the reward is legal");
    }
}
