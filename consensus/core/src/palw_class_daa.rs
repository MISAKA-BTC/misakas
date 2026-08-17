//! ADR-0038 Decision D: per-class difficulty domains — prices are discovered, never
//! maintained.
//!
//! Every hand-set cross-class PWU coefficient is a standing arbitrage: the mispriced-cheap
//! class absorbs every miner and the multi-class resilience ADR-0038 buys dies as a
//! monoculture. So the static, canonical PWU score prices work *inside* a class only, and
//! *between* classes each Active class runs its own retarget against its own share of block
//! cadence — the multi-algo-chain construction. This module is that arithmetic, pure and
//! consensus-inert:
//!
//! * [`PalwDifficultyDomainSetV1`] — the Active classes and their cadence shares. The invariant
//!   `Σ class shares = 1000‰` holds at every mutation, and freezing a class redistributes its
//!   share over the survivors proportionally, deterministically (largest-remainder, class-id
//!   order).
//!
//!   **The residual survivor is the BASE class, not a hash lane** (ADR-0039 W6′, superseding
//!   ADR-0038 W6). An earlier shape carried an `anti_stall_floor_permille` that `validate`
//!   refused to let reach zero, and that took the whole 1000‰ when the last class was removed —
//!   i.e. it structurally guaranteed a spam-hash lane could always produce blocks. ADR-0039
//!   removes that lane: block production is PALW work, and the liveness floor is a portable
//!   integer-only class (`PALW-BASE-0`) held permanently Active, whose share may never be zero
//!   and which may never be removed. With no class able to produce certified work the network
//!   HALTS LOUDLY by design — a visible stop, not a silent fork onto cheap hashes.
//! * [`adjust_class_target_v1`] — one class's clamped integer retarget: observed vs expected
//!   blocks over a window, easier when starving, harder when flooding, never by more than
//!   `max_factor` per adjustment. GPU wall-clock never enters — an RTX 5090 second and a
//!   2019-GPU second are different amounts of nothing.
//!
//! The caller (Stage-1 wiring) owns windows, per-class DAA state storage and how targets
//! feed the lottery; this module owns the arithmetic being deterministic, clamped and
//! share-conserving on every node.

use kaspa_hashes::Hash64;
use std::collections::BTreeMap;
use thiserror::Error;

/// Shares are permille; the set's conservation invariant is against this denominator.
pub const PALW_CLASS_SHARE_DENOMINATOR: u16 = 1000;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwClassDaaError {
    #[error("shares sum to {got}‰ — must sum to exactly {denominator}‰")]
    SharesDoNotConserve { got: u32, denominator: u16 },
    #[error("the base class {class_id} is not in the domain set — the liveness floor may never be absent")]
    BaseClassAbsent { class_id: Hash64 },
    #[error("the base class {class_id} may not be removed — it is the liveness floor (ADR-0039 W6′)")]
    BaseClassNotRemovable { class_id: Hash64 },
    #[error("class {class_id} is not in the domain set")]
    UnknownClass { class_id: Hash64 },
    #[error("class {class_id} carries a zero share — a zero-share Active class is a frozen class wearing the wrong status")]
    ZeroShare { class_id: Hash64 },
    #[error("max_factor must be ≥ 2 (1 would freeze the retarget)")]
    MaxFactorTooSmall,
    #[error("expected_blocks must be nonzero")]
    ZeroExpectedBlocks,
    #[error("the previous target is zero — a zero target is the MAXIMUM-weight value, never a neutral one")]
    ZeroPreviousTarget,
    #[error("share {got}‰ is outside (0, 1000‰]")]
    ShareOutOfRange { got: u16 },
    #[error("retarget_interval_daa must be nonzero")]
    ZeroRetargetInterval,
    #[error("the span counted no DAA blocks — an empty span is unanswerable, not a starving class")]
    EmptySpan,
    #[error("census counts {class} class blocks in a span of {total} — a class cannot exceed its own span")]
    CensusExceedsSpan { class: u64, total: u64 },
    #[error("a chain step's DAA score went backward: parent {parent_daa} → block {block_daa}")]
    NonMonotonicStep { parent_daa: u64, block_daa: u64 },
    #[error("a step's DAA advance is {advance} but its census counted {counted} blocks — the gatherer miscounted")]
    StepCensusMismatch { advance: u64, counted: u64 },
    #[error("steps are not a contiguous chain: expected a parent at DAA {expected_parent_daa}, got {got}")]
    DiscontiguousSteps { expected_parent_daa: u64, got: u64 },
}

/// The Active difficulty domains: class → cadence share (permille), plus the anti-stall
/// floor's share. `Σ shares + floor = 1000` always ([`Self::validate`] is checked on every
/// constructor and mutation, so an inconsistent set is unrepresentable through this API).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDifficultyDomainSetV1 {
    /// The liveness floor: a class that is always Active, always holds a nonzero share, and can
    /// never be removed (ADR-0039 W6′). `PALW-BASE-0` in practice — the portable integer-only
    /// class whose catalog closes, so it can be audited and convicted on any CPU.
    ///
    /// This replaced an `anti_stall_floor_permille` that guaranteed a spam-hash lane. The floor is
    /// PALW work now; there is no hash lane to fall back to, and that is the point.
    pub base_class_id: Hash64,
    /// Active class → share (permille). BTreeMap: every iteration below is id-ordered, so
    /// every node redistributes identically.
    pub class_shares_permille: BTreeMap<Hash64, u16>,
}

impl PalwDifficultyDomainSetV1 {
    /// Build a validated set. `base_class_id` is the liveness floor and must be one of the shares.
    pub fn new(base_class_id: Hash64, class_shares_permille: BTreeMap<Hash64, u16>) -> Result<Self, PalwClassDaaError> {
        let set = Self { base_class_id, class_shares_permille };
        set.validate()?;
        Ok(set)
    }

    /// The conservation invariant, the zero-share rule, and the base class's presence.
    pub fn validate(&self) -> Result<(), PalwClassDaaError> {
        // The floor must BE a class in the set. There is no lane outside the classes any more, so
        // an absent base class is a set with no guaranteed producer at all (ADR-0039 W6′).
        if !self.class_shares_permille.contains_key(&self.base_class_id) {
            return Err(PalwClassDaaError::BaseClassAbsent { class_id: self.base_class_id });
        }
        if let Some((class_id, _)) = self.class_shares_permille.iter().find(|(_, share)| **share == 0) {
            return Err(PalwClassDaaError::ZeroShare { class_id: *class_id });
        }
        let classes: u32 = self.class_shares_permille.values().map(|s| *s as u32).sum();
        if classes != PALW_CLASS_SHARE_DENOMINATOR as u32 {
            return Err(PalwClassDaaError::SharesDoNotConserve { got: classes, denominator: PALW_CLASS_SHARE_DENOMINATOR });
        }
        Ok(())
    }

    /// Freeze/remove a class: its share redistributes over the survivors proportionally to
    /// their existing shares, deterministically (integer largest-remainder; remainder order
    /// is by descending fractional part then ascending class id, so ties break identically
    /// on every node). With zero survivors the floor absorbs everything — the anti-stall
    /// degradation of ADR-0038 Decision D, W6.
    pub fn remove_class(&mut self, class_id: &Hash64) -> Result<(), PalwClassDaaError> {
        // The floor is not removable. Freezing every other class leaves the base class holding the
        // whole cadence, which is the designed degraded mode; removing the base class would leave a
        // set with no producer, and there is no hash lane behind it to catch that (ADR-0039 W6′).
        if *class_id == self.base_class_id {
            return Err(PalwClassDaaError::BaseClassNotRemovable { class_id: *class_id });
        }
        let Some(removed_share) = self.class_shares_permille.remove(class_id) else {
            return Err(PalwClassDaaError::UnknownClass { class_id: *class_id });
        };
        let survivor_total: u64 = self.class_shares_permille.values().map(|s| *s as u64).sum();
        // Proportional integer split of `removed_share` with largest-remainder assignment.
        let mut assigned_total: u16 = 0;
        let mut remainders: Vec<(u64, Hash64)> = Vec::with_capacity(self.class_shares_permille.len());
        let mut additions: BTreeMap<Hash64, u16> = BTreeMap::new();
        for (id, share) in self.class_shares_permille.iter() {
            let numerator = removed_share as u64 * *share as u64;
            let addition = (numerator / survivor_total) as u16;
            let remainder = numerator % survivor_total;
            assigned_total += addition;
            additions.insert(*id, addition);
            remainders.push((remainder, *id));
        }
        // Distribute the residue: one permille each to the largest remainders, id ascending on ties.
        remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut residue = removed_share - assigned_total;
        for (_, id) in remainders {
            if residue == 0 {
                break;
            }
            *additions.get_mut(&id).expect("id from the same map") += 1;
            residue -= 1;
        }
        for (id, addition) in additions {
            *self.class_shares_permille.get_mut(&id).expect("id from the same map") += addition;
        }
        debug_assert!(self.validate().is_ok());
        Ok(())
    }

    /// The base class's effective share right now — 1000‰ exactly when every other class has been
    /// frozen out, which is the designed degraded mode: the whole cadence on the portable
    /// integer-only floor, still PALW work, still auditable and convictable.
    pub fn base_share_permille(&self) -> u16 {
        *self.class_shares_permille.get(&self.base_class_id).expect("validate() guarantees the base class is present")
    }
}

// =============================================================================================
// Decision 5 — the per-class epoch budget, DERIVED (no enforcement point; see the ADR amendment)
// =============================================================================================

/// Errors of the epoch-budget derivation. Every one of them is a refusal to produce a number, never
/// a cap of zero: a class that can never admit a block is starved, not capped, and the difference
/// matters because a starved class silently loses its whole domain.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwClassBudgetError {
    #[error("the epoch length is zero — blocks per epoch is the divisor and cannot be zero")]
    ZeroEpochLength,
    #[error("tolerance {got}‰ is below unity — a budget under the class's own expected production starves it")]
    ToleranceBelowUnity { got: u32 },
    #[error("tolerance {got}‰ exceeds the {max}‰ ceiling — a budget no epoch can approach is not a cap")]
    ToleranceAboveCeiling { got: u32, max: u32 },
    #[error("class {class_id} derives a zero epoch budget — a class that can never admit a block is starved, not capped")]
    ZeroBudget { class_id: Hash64 },
    #[error(
        "class {class_id} would be capped at {budget_pwu} pwu but its own cadence share expects {own_expected_pwu} — the \
         inequality is unsatisfiable for any class above the share-weighted mean pwu (ADR-0039 Decision 5 amendment (e))"
    )]
    StarvedClass { class_id: Hash64, budget_pwu: u128, own_expected_pwu: u128 },
    #[error("class {class_id} has a saturated pwu — two unequal classes would compare equal, so the share is meaningless")]
    SaturatedClassPwu { class_id: Hash64 },
    #[error("class {class_id} is not in the domain set, so it has no share and no budget")]
    UnknownClass { class_id: Hash64 },
    #[error("the epoch budget arithmetic overflowed computing {what}")]
    Overflow { what: &'static str },
}

/// The largest tolerance a budget may carry, in permille of the class's own expected production.
///
/// A ceiling is as necessary as the unity floor and was missing from the first draft: with only a
/// floor, `tolerance_permille = u32::MAX` yields a budget no honest epoch can approach, which is a
/// cap in name only and passes every test a floor-only rule can write. 4× is deliberately generous
/// — the cap exists for a *transiently* mis-tuned DAA, and a class legitimately running 4× its share
/// for a whole epoch is a retarget problem, not a flood.
pub const PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE: u32 = 4_000;

/// One class's epoch budget, in **pwu** — never in ramped `weight`.
///
/// The ADR clause says `weight(b)`, and that is amended: a block's weight is its pwu scaled by a
/// maturity stage, so `Σ weight(b)` is a moving sum and "the budget this block would exceed" is not
/// a predicate. `pwu` is immutable per block and already miner-independent. See the Decision 5
/// amendment in ADR-0039.
///
/// `budget_pwu` is private and there is no constructor but the derivation below, so a caller cannot
/// assert a budget it did not derive. That is worth stating precisely, because it is a weaker
/// guarantee than it looks: the derivation multiplies numbers the caller supplied, so the private
/// field prevents a fabricated STRUCT, not a fabricated INPUT. The inputs are constrained instead —
/// the divisor comes from the network's own epoch length, the shares from a validated domain set,
/// and the tolerance is fenced above and below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClassEpochBudgetV1 {
    /// The epoch this budget is for. A budget is only ever comparable within its own epoch.
    pub epoch: u64,
    /// The class it bounds.
    pub class_id: Hash64,
    budget_pwu: u128,
}

impl PalwClassEpochBudgetV1 {
    /// The ceiling, in pwu, on this class's production for this epoch.
    pub fn budget_pwu(&self) -> u128 {
        self.budget_pwu
    }
}

/// Every Active class's epoch budget, derived from the domain set's shares and each class's own
/// per-block pwu.
///
/// `epoch_len_blue_score` is the epoch DIVISOR — the network's blocks-per-epoch — and not a free
/// "how many blocks are in this epoch" argument. Those are the same number and taking it twice is
/// how they come to differ: the value that divides a blue score into an epoch index must be the
/// value that says how many blocks an epoch holds, or the budget is denominated in a different
/// epoch than the one it is enforced over.
///
/// `class_targets` and `pwu_per_inference` are per class; a class present in the domain set with no
/// entry in either is `UnknownClass` rather than a zero, because "we have no idea what this class's
/// work costs" must not read as "this class costs nothing".
///
/// Arithmetic: `W_e` is the sum over classes of `share · epoch_blocks · pwu(class)`, and each
/// class's budget is `W_e · share · tolerance` — with the permille denominators divided out ONCE at
/// the end. Dividing per class first truncates three times per class and the residues do not
/// cancel, so two nodes summing in different orders could differ; one division at the end is exact.
pub fn class_epoch_budgets_v1(
    epoch: u64,
    domains: &PalwDifficultyDomainSetV1,
    epoch_len_blue_score: u64,
    class_targets: &BTreeMap<Hash64, u128>,
    pwu_per_inference: &BTreeMap<Hash64, u64>,
    tolerance_permille: u32,
) -> Result<BTreeMap<Hash64, PalwClassEpochBudgetV1>, PalwClassBudgetError> {
    if epoch_len_blue_score == 0 {
        return Err(PalwClassBudgetError::ZeroEpochLength);
    }
    if tolerance_permille < PALW_CLASS_SHARE_DENOMINATOR as u32 {
        return Err(PalwClassBudgetError::ToleranceBelowUnity { got: tolerance_permille });
    }
    if tolerance_permille > PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE {
        return Err(PalwClassBudgetError::ToleranceAboveCeiling {
            got: tolerance_permille,
            max: PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE,
        });
    }
    domains.validate().map_err(|_| PalwClassBudgetError::Overflow { what: "an invalid domain set reached the budget" })?;

    // Per class: share‰ · epoch_blocks · pwu(class), UNDIVIDED. The permille denominators come out
    // once, at the end.
    let mut scaled: BTreeMap<Hash64, u128> = BTreeMap::new();
    let mut w_e_scaled: u128 = 0;
    for (class_id, share) in domains.class_shares_permille.iter() {
        let target = class_targets.get(class_id).ok_or(PalwClassBudgetError::UnknownClass { class_id: *class_id })?;
        let per_inference = pwu_per_inference.get(class_id).ok_or(PalwClassBudgetError::UnknownClass { class_id: *class_id })?;
        let class_pwu = crate::palw_pwu::palw_pwu_v1(*target, *per_inference);
        // `palw_pwu_v1` saturates at u64::MAX, and a saturated value has lost the magnitude the
        // share is applied to — two classes an order of magnitude apart would compare equal. Refuse
        // rather than compare meaningless numbers.
        if class_pwu == u64::MAX {
            return Err(PalwClassBudgetError::SaturatedClassPwu { class_id: *class_id });
        }
        let own = (*share as u128)
            .checked_mul(epoch_len_blue_score as u128)
            .and_then(|x| x.checked_mul(class_pwu as u128))
            .ok_or(PalwClassBudgetError::Overflow { what: "a class's expected epoch pwu" })?;
        w_e_scaled = w_e_scaled.checked_add(own).ok_or(PalwClassBudgetError::Overflow { what: "W_e" })?;
        scaled.insert(*class_id, own);
    }

    let denom = PALW_CLASS_SHARE_DENOMINATOR as u128;
    let mut out = BTreeMap::new();
    for (class_id, _) in scaled.iter() {
        let share = *domains.class_shares_permille.get(class_id).expect("iterating the same set") as u128;
        // W_e · share‰ · tolerance‰, then the three permille denominators (share inside W_e, share
        // here, tolerance here) divided out together.
        let numerator = w_e_scaled
            .checked_mul(share)
            .and_then(|x| x.checked_mul(tolerance_permille as u128))
            .ok_or(PalwClassBudgetError::Overflow { what: "a class's budget numerator" })?;
        let budget_pwu = numerator / (denom * denom * denom);
        if budget_pwu == 0 {
            return Err(PalwClassBudgetError::ZeroBudget { class_id: *class_id });
        }
        // THE INEQUALITY IS UNSATISFIABLE FOR A HEAVY CLASS, and that has to surface here rather
        // than as a mid-epoch liveness cliff.
        //
        // A class's own cadence share expects `L · share_c · pwu_c` pwu per epoch. Its budget is
        // `W_e · share_c · tol` where `W_e = L · Σ_k share_k · pwu_k`, so
        //
        //     budget_c ≥ expected_c   ⟺   tol · (share-weighted mean pwu) ≥ pwu_c
        //
        // The share cancels entirely: what remains is a comparison of this class's per-block pwu
        // against the mean. EVERY class above the mean is capped below its own cadence at unity
        // tolerance — the heaviest class cannot produce the share the DAA is targeting for it. No
        // per-class tolerance fixes it, because a per-class tolerance IS the cross-class coefficient
        // table ADR-0038 Decision D rejects. So this is refused as an incoherent parameter set, not
        // silently shipped as a cap that throttles the class the network most wants running.
        //
        // The first draft of this function did not check it, and the test suite did not catch it,
        // because every fixture gave both classes the SAME pwu — under which the comparison is
        // vacuously true. That is the fixture-calibration failure mode, and the test below now uses
        // a deliberately unequal spread.
        let own_expected_pwu = scaled.get(class_id).copied().expect("iterating the same set") / denom;
        if budget_pwu < own_expected_pwu {
            return Err(PalwClassBudgetError::StarvedClass { class_id: *class_id, budget_pwu, own_expected_pwu });
        }
        out.insert(*class_id, PalwClassEpochBudgetV1 { epoch, class_id: *class_id, budget_pwu });
    }
    Ok(out)
}

/// Whether one more block of `class_id` fits its epoch budget.
///
/// `admitted_pwu` is the class's own production SO FAR in this epoch, along the chain the caller is
/// deciding on. This function is arithmetic and nothing else: it does not know where that number
/// came from, and the honest reading is that the guarantee lives entirely in the caller. That is why
/// no admission path calls it yet — the ADR amendment records that the cap has no formulation whose
/// accumulator a validating node can reconstruct for the block it is validating.
///
/// The budget must be the budget for THIS class and THIS epoch; mismatches are refused rather than
/// coerced, so a caller cannot present a roomier class's ceiling. That check catches a wiring
/// mistake, not an attacker — both sides come from the caller — and the doc says so rather than
/// implying a defence.
pub fn class_epoch_pwu_fits_v1(
    budget: &PalwClassEpochBudgetV1,
    class_id: &Hash64,
    epoch: u64,
    admitted_pwu: u128,
    block_pwu: u64,
) -> Result<bool, PalwClassBudgetError> {
    if budget.class_id != *class_id || budget.epoch != epoch {
        return Err(PalwClassBudgetError::UnknownClass { class_id: *class_id });
    }
    let after = admitted_pwu.checked_add(block_pwu as u128).ok_or(PalwClassBudgetError::Overflow { what: "the epoch accumulator" })?;
    Ok(after <= budget.budget_pwu)
}

/// One class's clamped retarget over its own window. `current_target` is the class lottery
/// target (bigger = easier); a class that produced fewer blocks than its share expected gets
/// easier, one that flooded gets harder — by the exact observed ratio, clamped to
/// `[current/max_factor, current×max_factor]` per adjustment so one weird window cannot
/// cliff a class (the same reason Bitcoin clamps ×4). `observed_blocks = 0` (a dead-quiet
/// window) is the full easing clamp, not a division.
pub fn adjust_class_target_v1(
    current_target: u128,
    observed_blocks: u64,
    expected_blocks: u64,
    max_factor: u32,
) -> Result<u128, PalwClassDaaError> {
    if max_factor < 2 {
        return Err(PalwClassDaaError::MaxFactorTooSmall);
    }
    if expected_blocks == 0 {
        return Err(PalwClassDaaError::ZeroExpectedBlocks);
    }
    let floor = current_target / max_factor as u128;
    let ceiling = current_target.saturating_mul(max_factor as u128);
    if observed_blocks == 0 {
        return Ok(ceiling);
    }
    // new = current × expected / observed, in u256-free arithmetic: split current into
    // high/low halves so the multiply cannot overflow u128.
    let scaled = mul_div_u128(current_target, expected_blocks as u128, observed_blocks as u128);
    Ok(scaled.clamp(floor, ceiling))
}

// =============================================================================================
// The retarget as a pure fold over one chain's steps (ADR-0038 Decision D)
// =============================================================================================

/// One chain step, as the gatherer measured it: the DAA scores either side, and the DAA-counted
/// blocks the step merged split by class.
///
/// `total_daa_blocks` MUST equal `block_daa - parent_daa`, and [`fold_class_target_v1`] refuses a
/// step where it does not. The check is the whole point of carrying both: the gatherer walks a DAG
/// and this fold cannot see the DAG, so this is the only place a miscount can be caught — a
/// double-counted selected parent (`mergeset_blues[0]` IS the selected parent), a forgotten
/// `mergeset_non_daa` filter, a skipped chain block. A miscount that got through would ease a
/// class's target and inflate the pwu of every block it produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRetargetStepV1 {
    pub parent_daa: u64,
    pub block_daa: u64,
    /// DAA-counted blocks of THIS class the step merged.
    pub class_daa_blocks: u64,
    /// DAA-counted blocks of every class the step merged.
    pub total_daa_blocks: u64,
}

/// One class's realized production over one closed span, all classes' totals alongside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwClassSpanCensusV1 {
    pub class_daa_blocks: u64,
    pub total_daa_blocks: u64,
}

/// One class's retarget over one closed span, **with the expectation derived here**.
///
/// [`adjust_class_target_v1`] takes `expected_blocks` as an argument, so any caller that computes it
/// is a caller that owns the rule — the shape this codebase kept finding in audit after audit (a
/// hardcoded `bonded: true`, a hardcoded `frozen: false`, a `weight` cap whose unit the caller
/// picked). The share rule IS the rule, so it lives here and nothing outside may supply the number.
///
/// **The expectation is a share of REALIZED production, not a wall-clock cadence.** A class expects
/// `share_c · total`, where `total` is what the span actually produced. Two consequences, and both
/// are the reason it is written this way:
///
/// * `Σ_c expected_c = total`, so this loop only ever redistributes share BETWEEN classes. Block
///   interval stays `DifficultyManager::calculate_difficulty_bits`'s job, and the two retargets
///   cannot fight each other over one cadence.
/// * no timestamp enters, so no host's clock reaches fork-choice weight — which ADR-0038 Decision D
///   refuses in one line.
///
/// It also makes a ONE-class network a deliberate no-op: at 1000‰ with every block in the class,
/// observed equals expected exactly and the target never moves. That is the correct behaviour for
/// the single-registration fence every shipped preset would carry, and it is the property that
/// catches a mutation to a cadence-based expectation.
pub fn retarget_over_span_v1(
    current_target: u128,
    census: &PalwClassSpanCensusV1,
    share_permille: u16,
    max_factor: u32,
) -> Result<u128, PalwClassDaaError> {
    if current_target == 0 {
        return Err(PalwClassDaaError::ZeroPreviousTarget);
    }
    if share_permille == 0 || share_permille > PALW_CLASS_SHARE_DENOMINATOR {
        return Err(PalwClassDaaError::ShareOutOfRange { got: share_permille });
    }
    if census.total_daa_blocks == 0 {
        return Err(PalwClassDaaError::EmptySpan);
    }
    if census.class_daa_blocks > census.total_daa_blocks {
        return Err(PalwClassDaaError::CensusExceedsSpan { class: census.class_daa_blocks, total: census.total_daa_blocks });
    }
    // `share · total / 1000`, rounded to nearest so a class is not systematically under-expected by
    // truncation on every span — a floor here biases every class toward "produced more than
    // expected" and hardens targets network-wide over time.
    let denominator = PALW_CLASS_SHARE_DENOMINATOR as u64;
    let expected = (census.total_daa_blocks * share_permille as u64 + denominator / 2) / denominator;
    // A span too short for this class's share to round up to one block expects nothing, and nothing
    // is unanswerable rather than a miss: retargeting on it would ease the target of every
    // small-share class on every short span.
    if expected == 0 {
        return Ok(current_target);
    }
    adjust_class_target_v1(current_target, census.class_daa_blocks, expected, max_factor)
}

/// The whole retarget rule as a pure function of one chain's steps: fold from `boot_target` in chain
/// order (oldest first), retargeting once at each boundary the steps cross.
///
/// A boundary is an absolute multiple of `interval_daa`, so which spans close is a property of the
/// chain and not of where the fold happened to start. The trailing span is deliberately NOT applied
/// — it has not closed yet, and applying it would make a target depend on how far along the current
/// span the reader is.
///
/// Spans need not be equal length. Because each span's expectation scales with its OWN realized
/// total, a leading span that begins mid-interval is measured correctly rather than short, which is
/// what makes a bounded-memory fold legitimate rather than an approximation.
///
/// This exists so the rule is reachable by a unit test with no database. The consensus-side caller
/// gathers steps and owns no arithmetic.
pub fn fold_class_target_v1(
    boot_target: u128,
    steps: &[PalwRetargetStepV1],
    share_permille: u16,
    interval_daa: u64,
    max_factor: u32,
) -> Result<u128, PalwClassDaaError> {
    if boot_target == 0 {
        return Err(PalwClassDaaError::ZeroPreviousTarget);
    }
    if interval_daa == 0 {
        return Err(PalwClassDaaError::ZeroRetargetInterval);
    }
    let mut target = boot_target;
    let mut open = PalwClassSpanCensusV1::default();
    let mut previous_block_daa: Option<u64> = None;
    for step in steps {
        if step.block_daa < step.parent_daa {
            return Err(PalwClassDaaError::NonMonotonicStep { parent_daa: step.parent_daa, block_daa: step.block_daa });
        }
        // The gatherer's own consistency check — see `PalwRetargetStepV1`.
        if step.block_daa - step.parent_daa != step.total_daa_blocks {
            return Err(PalwClassDaaError::StepCensusMismatch {
                advance: step.block_daa - step.parent_daa,
                counted: step.total_daa_blocks,
            });
        }
        if step.class_daa_blocks > step.total_daa_blocks {
            return Err(PalwClassDaaError::CensusExceedsSpan { class: step.class_daa_blocks, total: step.total_daa_blocks });
        }
        // Steps must be a contiguous chain in order, or the fold is silently measuring a different
        // chain than the caller thinks it walked.
        if let Some(previous) = previous_block_daa
            && previous != step.parent_daa
        {
            return Err(PalwClassDaaError::DiscontiguousSteps { expected_parent_daa: previous, got: step.parent_daa });
        }
        previous_block_daa = Some(step.block_daa);

        open.class_daa_blocks += step.class_daa_blocks;
        open.total_daa_blocks += step.total_daa_blocks;
        // A step that crosses at least one boundary closes the open span — ONCE, however many
        // multiples of the interval it jumped.
        //
        // Retargeting once per boundary crossed was the obvious reading and it is wrong: the second
        // crossing would close an EMPTY census, which is `EmptySpan`, and a step merging a wide
        // mergeset legitimately jumps several intervals. There is no honest alternative either,
        // because a step's merged blocks have no order that would let them be split between the
        // intervals they span. So the span runs from the previous crossing to this one, and its
        // expectation scales with its own realized total — which is exactly what makes an unequal
        // span measured correctly rather than short. Retargeting once is also the conservative
        // direction: the per-adjustment clamp binds once instead of `k` times.
        if step.block_daa / interval_daa > step.parent_daa / interval_daa {
            target = retarget_over_span_v1(target, &open, share_permille, max_factor)?;
            open = PalwClassSpanCensusV1::default();
        }
    }
    Ok(target)
}

/// `⌊a × b / d⌋` where `b`, `d` fit u64 widened to u128. Exact by the identity
/// `a·b = (a/d)·d·b + (a%d)·b`, so `⌊a·b/d⌋ = (a/d)·b + ⌊(a%d)·b/d⌋` — the second term's
/// product fits u128 because both factors are < 2^64. The first term saturates only when the
/// TRUE quotient exceeds u128, and every saturated value still lands above the caller's
/// ceiling clamp, so saturation never changes the clamped result.
fn mul_div_u128(a: u128, b: u128, d: u128) -> u128 {
    (a / d).saturating_mul(b).saturating_add((a % d) * b / d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(seed: u64) -> Hash64 {
        Hash64::from_u64_word(seed)
    }

    // -----------------------------------------------------------------------------------------
    // Decision 5 — the derived epoch budget
    // -----------------------------------------------------------------------------------------

    fn two_class_domains() -> (Hash64, Hash64, PalwDifficultyDomainSetV1) {
        let base = id(0xB0);
        let other = id(0xC1);
        let set = PalwDifficultyDomainSetV1::new(base, BTreeMap::from([(base, 600u16), (other, 400u16)])).unwrap();
        (base, other, set)
    }

    /// Shares govern the split, the tolerance scales it, and every input is bound.
    #[test]
    fn an_epoch_budget_is_the_class_share_of_the_epochs_own_work() {
        let (base, other, domains) = two_class_domains();
        // Equal per-block pwu for both classes keeps the arithmetic checkable by hand: with one
        // easy target and one per-inference cost each, W_e is (600 + 400) · L · pwu, and each
        // class's budget is that times its own share times the tolerance.
        let targets = BTreeMap::from([(base, u128::MAX / 4), (other, u128::MAX / 4)]);
        let costs = BTreeMap::from([(base, 1_000u64), (other, 1_000u64)]);
        let budgets = class_epoch_budgets_v1(7, &domains, 100, &targets, &costs, 1_000).unwrap();
        assert_eq!(budgets.len(), 2);
        let b = budgets[&base];
        let o = budgets[&other];
        assert_eq!((b.epoch, b.class_id), (7, base));
        // 600/400 shares over the same W_e: the budgets are in exactly that ratio.
        assert_eq!(b.budget_pwu() * 400, o.budget_pwu() * 600, "budgets follow the shares exactly");
        assert!(b.budget_pwu() > o.budget_pwu());

        // The tolerance scales linearly, and the epoch length does too — both are bound.
        let generous = class_epoch_budgets_v1(7, &domains, 100, &targets, &costs, 2_000).unwrap();
        assert_eq!(generous[&base].budget_pwu(), b.budget_pwu() * 2);
        let longer = class_epoch_budgets_v1(7, &domains, 200, &targets, &costs, 1_000).unwrap();
        assert_eq!(longer[&base].budget_pwu(), b.budget_pwu() * 2);
        // A different epoch is a different budget object even at identical numbers, because a
        // budget is only ever comparable within its own epoch.
        assert_ne!(class_epoch_budgets_v1(8, &domains, 100, &targets, &costs, 1_000).unwrap()[&base], b);
    }

    /// The tolerance is fenced on BOTH sides. A floor-only rule passes every test while admitting a
    /// budget no epoch can approach, which is a cap in name only.
    #[test]
    fn a_tolerance_outside_the_fence_is_refused_in_both_directions() {
        let (base, other, domains) = two_class_domains();
        let targets = BTreeMap::from([(base, u128::MAX / 4), (other, u128::MAX / 4)]);
        let costs = BTreeMap::from([(base, 1_000u64), (other, 1_000u64)]);
        assert_eq!(
            class_epoch_budgets_v1(1, &domains, 100, &targets, &costs, 999),
            Err(PalwClassBudgetError::ToleranceBelowUnity { got: 999 })
        );
        assert_eq!(
            class_epoch_budgets_v1(1, &domains, 100, &targets, &costs, PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE + 1),
            Err(PalwClassBudgetError::ToleranceAboveCeiling {
                got: PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE + 1,
                max: PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE
            })
        );
        assert_eq!(
            class_epoch_budgets_v1(1, &domains, 100, &targets, &costs, u32::MAX),
            Err(PalwClassBudgetError::ToleranceAboveCeiling { got: u32::MAX, max: PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE }),
            "an unbounded tolerance is the shape a floor-only fence lets through"
        );
        // The exact boundaries hold.
        assert!(class_epoch_budgets_v1(1, &domains, 100, &targets, &costs, 1_000).is_ok());
        assert!(class_epoch_budgets_v1(1, &domains, 100, &targets, &costs, PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE).is_ok());
        // And the divisor is not optional.
        assert_eq!(class_epoch_budgets_v1(1, &domains, 0, &targets, &costs, 1_000), Err(PalwClassBudgetError::ZeroEpochLength));
    }

    /// A class that could never admit a block is STARVED, not capped, and that is an error.
    ///
    /// The failure is quiet by nature: a zero budget looks like a working cap and silently removes a
    /// class's whole domain. It is reachable at honest parameters — a tiny share on a short epoch
    /// with a cheap class truncates to nothing.
    #[test]
    fn a_class_that_can_never_admit_a_block_is_refused_not_capped() {
        let base = id(0xB0);
        let dust = id(0xD1);
        let domains = PalwDifficultyDomainSetV1::new(base, BTreeMap::from([(base, 999u16), (dust, 1u16)])).unwrap();
        // A hard target: one expected attempt, so pwu(class) is just `pwu_per_inference`.
        let targets = BTreeMap::from([(base, u128::MAX), (dust, u128::MAX)]);
        let costs = BTreeMap::from([(base, 1u64), (dust, 1u64)]);
        assert_eq!(
            class_epoch_budgets_v1(1, &domains, 2, &targets, &costs, 1_000),
            Err(PalwClassBudgetError::ZeroBudget { class_id: dust }),
            "a truncated-to-zero budget must be an error, never a silent cap of nothing"
        );
    }

    /// **The ADR's inequality is unsatisfiable for any class above the share-weighted mean pwu**, and
    /// the derivation refuses such a set instead of shipping a cap that throttles it.
    ///
    /// `budget_c ≥ expected_c ⟺ tolerance · mean_pwu ≥ pwu_c` — the share cancels out entirely, so
    /// what is left is a comparison of one class's per-block pwu against the mean. At unity
    /// tolerance every class above the mean is capped below its own cadence share, which is the
    /// opposite of what the DAA is targeting for it.
    ///
    /// This defect survived the first draft of the whole module because every fixture gave both
    /// classes the SAME pwu, under which the comparison is vacuously true. Measured: shares 600/400
    /// with pwu 100/10 000 gives the heavy class 0.406x its own expected production.
    #[test]
    fn a_class_heavier_than_the_mean_is_refused_rather_than_throttled() {
        let base = id(0xB0);
        let heavy = id(0xC1);
        let domains = PalwDifficultyDomainSetV1::new(base, BTreeMap::from([(base, 600u16), (heavy, 400u16)])).unwrap();
        let easy = u128::MAX / 4;
        let targets = BTreeMap::from([(base, easy), (heavy, easy)]);
        // 100x apart: the heavy class is far above the share-weighted mean.
        let costs = BTreeMap::from([(base, 100u64), (heavy, 10_000u64)]);

        let err = class_epoch_budgets_v1(1, &domains, 100, &targets, &costs, 1_000).unwrap_err();
        match err {
            PalwClassBudgetError::StarvedClass { class_id, budget_pwu, own_expected_pwu } => {
                assert_eq!(class_id, heavy, "the class above the mean is the starved one");
                assert!(budget_pwu < own_expected_pwu);
                // The measured ratio, pinned: 0.406x. Its being well below 1 is the whole finding.
                assert_eq!(budget_pwu * 1_000 / own_expected_pwu, 406);
            }
            other => panic!("expected StarvedClass, got {other:?}"),
        }

        // A tolerance large enough absorbs THIS spread — 2.464x by measurement — which is exactly why
        // a per-class tolerance would be the cross-class coefficient table ADR-0038 Decision D
        // rejects: the value needed depends on the spread, so it prices classes against each other.
        assert!(class_epoch_budgets_v1(1, &domains, 100, &targets, &costs, 2_463).is_err());
        assert!(class_epoch_budgets_v1(1, &domains, 100, &targets, &costs, 2_464).is_ok());

        // The tolerance a class needs is BOUNDED, and the bound is `1000/share_c`‰ — not the pwu
        // spread. As `pwu_c` grows it dominates the mean too, so `pwu_c / mean → 1000/share_c`. My
        // first version of this test claimed a wide enough spread is never expressible, which is
        // FALSE and the suite caught it: at share 400‰ the requirement converges to 2 500‰ no matter
        // how heavy the class gets, so a 10 000x spread is admissible at tolerance 3 000‰.
        let wilder = BTreeMap::from([(base, 100u64), (heavy, 1_000_000u64)]);
        assert!(class_epoch_budgets_v1(1, &domains, 100, &targets, &wilder, 2_499).is_err());
        assert!(class_epoch_budgets_v1(1, &domains, 100, &targets, &wilder, 2_500).is_ok(), "the bound is 1000/400 = 2 500‰");
        let absurd = BTreeMap::from([(base, 1u64), (heavy, u64::MAX / 4)]);
        assert!(
            class_epoch_budgets_v1(1, &domains, 100, &targets, &absurd, 2_500).is_ok(),
            "the requirement converges to the share bound and never exceeds it"
        );

        // What the fence's 4 000‰ ceiling therefore means, exactly: it protects any class with
        // share ≥ 250‰ unconditionally, and can NEVER protect a class below that against a heavy
        // enough pwu. A 100‰ class needs up to 10 000‰, which is not expressible.
        let thin = id(0xD2);
        let thin_domains = PalwDifficultyDomainSetV1::new(base, BTreeMap::from([(base, 900u16), (thin, 100u16)])).unwrap();
        let thin_targets = BTreeMap::from([(base, easy), (thin, easy)]);
        let thin_costs = BTreeMap::from([(base, 1u64), (thin, 1_000_000_000u64)]);
        for tolerance in [1_000, 2_500, PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE] {
            assert!(
                matches!(
                    class_epoch_budgets_v1(1, &thin_domains, 100, &thin_targets, &thin_costs, tolerance),
                    Err(PalwClassBudgetError::StarvedClass { .. })
                ),
                "a 100‰ class needs up to 10 000‰ and the ceiling is 4 000‰ (tolerance {tolerance})"
            );
        }

        // An equal-pwu set is coherent at unity — the case the original fixtures used, and the
        // reason the defect was invisible. Kept as the contrast rather than deleted.
        let flat = BTreeMap::from([(base, 1_000u64), (heavy, 1_000u64)]);
        let ok = class_epoch_budgets_v1(1, &domains, 100, &targets, &flat, 1_000).unwrap();
        assert_eq!(ok.len(), 2);
    }

    /// A missing per-class fact is `UnknownClass`, never a zero — "we do not know what this class's
    /// work costs" must not read as "this class costs nothing", which would give it the whole epoch.
    #[test]
    fn a_class_with_no_recorded_cost_has_no_budget_rather_than_a_free_one() {
        let (base, other, domains) = two_class_domains();
        let targets = BTreeMap::from([(base, u128::MAX / 4), (other, u128::MAX / 4)]);
        let only_base = BTreeMap::from([(base, 1_000u64)]);
        assert_eq!(
            class_epoch_budgets_v1(1, &domains, 100, &targets, &only_base, 1_000),
            Err(PalwClassBudgetError::UnknownClass { class_id: other })
        );
        let only_base_target = BTreeMap::from([(base, u128::MAX / 4)]);
        let costs = BTreeMap::from([(base, 1_000u64), (other, 1_000u64)]);
        assert_eq!(
            class_epoch_budgets_v1(1, &domains, 100, &only_base_target, &costs, 1_000),
            Err(PalwClassBudgetError::UnknownClass { class_id: other })
        );
    }

    /// A saturated pwu has lost the magnitude the share is applied to, so it is refused rather than
    /// compared. `palw_pwu_v1` saturates at `u64::MAX`, and two classes an order of magnitude apart
    /// would then carry identical budgets.
    #[test]
    fn a_saturated_class_pwu_is_refused_rather_than_silently_flattened() {
        let (base, other, domains) = two_class_domains();
        // The easiest possible target maximizes expected attempts; a huge per-inference cost then
        // saturates the product.
        let targets = BTreeMap::from([(base, u128::MAX), (other, u128::MAX)]);
        let costs = BTreeMap::from([(base, u64::MAX), (other, u64::MAX)]);
        let err = class_epoch_budgets_v1(1, &domains, 100, &targets, &costs, 1_000).unwrap_err();
        assert!(matches!(err, PalwClassBudgetError::SaturatedClassPwu { .. }), "got {err:?}");
    }

    /// The fits predicate is arithmetic, refuses a budget for another class or epoch, and does not
    /// overflow at the extremes.
    #[test]
    fn the_fits_predicate_bounds_the_epoch_and_refuses_a_foreign_budget() {
        let (base, other, domains) = two_class_domains();
        let targets = BTreeMap::from([(base, u128::MAX / 4), (other, u128::MAX / 4)]);
        let costs = BTreeMap::from([(base, 1_000u64), (other, 1_000u64)]);
        let budgets = class_epoch_budgets_v1(7, &domains, 100, &targets, &costs, 1_000).unwrap();
        let b = budgets[&base];
        let cap = b.budget_pwu();

        assert!(class_epoch_pwu_fits_v1(&b, &base, 7, 0, 1).unwrap());
        assert!(class_epoch_pwu_fits_v1(&b, &base, 7, cap - 1, 1).unwrap(), "exactly filling the budget still fits");
        assert!(!class_epoch_pwu_fits_v1(&b, &base, 7, cap, 1).unwrap(), "one pwu past it does not");

        // A budget for another class or another epoch is refused, not coerced.
        assert!(class_epoch_pwu_fits_v1(&b, &other, 7, 0, 1).is_err());
        assert!(class_epoch_pwu_fits_v1(&b, &base, 8, 0, 1).is_err());
        // And the accumulator cannot be wrapped into fitting.
        assert!(matches!(class_epoch_pwu_fits_v1(&b, &base, 7, u128::MAX, u64::MAX), Err(PalwClassBudgetError::Overflow { .. })));
    }

    /// Order-invariance and exactness: the budget divides ONCE.
    ///
    /// Dividing per class first truncates three permille denominators per class and the residues do
    /// not cancel, so the sum would depend on how the classes were grouped. This pins that the
    /// derivation is a function of the set and not of its traversal.
    #[test]
    fn the_budget_divides_once_so_it_is_exact_and_order_free() {
        let base = id(0xB0);
        let (a, b, c) = (id(0xA1), id(0xA2), id(0xA3));
        let domains =
            PalwDifficultyDomainSetV1::new(base, BTreeMap::from([(base, 397u16), (a, 201u16), (b, 199u16), (c, 203u16)])).unwrap();
        // A COHERENT spread: every class's pwu is within the tolerance of the share-weighted mean, so
        // nothing is starved and the test is about the arithmetic. The original fixture used a
        // 2000x spread and now correctly fails `StarvedClass` — which is how that defect was found.
        // Equal targets keep pwu proportional to the registered cost, so the spread is legible.
        let easy = u128::MAX / 3;
        let targets = BTreeMap::from([(base, easy), (a, easy), (b, easy), (c, easy)]);
        let costs = BTreeMap::from([(base, 7_919u64), (a, 9_973u64), (b, 8_093u64), (c, 8_111u64)]);
        let budgets = class_epoch_budgets_v1(3, &domains, 137, &targets, &costs, 1_337).unwrap();

        // W_e computed independently here, undivided, then each budget checked against it exactly.
        let denom = PALW_CLASS_SHARE_DENOMINATOR as u128;
        let mut w_e_scaled: u128 = 0;
        for (id, share) in domains.class_shares_permille.iter() {
            let pwu = crate::palw_pwu::palw_pwu_v1(targets[id], costs[id]) as u128;
            w_e_scaled += (*share as u128) * 137u128 * pwu;
        }
        for (id, share) in domains.class_shares_permille.iter() {
            let expected = w_e_scaled * (*share as u128) * 1_337u128 / (denom * denom * denom);
            assert_eq!(budgets[id].budget_pwu(), expected, "class {id:?} budget is not the single-division value");
        }
        // Non-trivial: at these numbers a per-class-first division really does differ.
        let mut truncated_total: u128 = 0;
        for (id, share) in domains.class_shares_permille.iter() {
            let pwu = crate::palw_pwu::palw_pwu_v1(targets[id], costs[id]) as u128;
            truncated_total += (*share as u128) * 137u128 * pwu / denom;
        }
        assert_ne!(truncated_total, w_e_scaled / denom, "the fixture must actually exercise the residue");
    }

    // -----------------------------------------------------------------------------------------
    // The retarget fold
    // -----------------------------------------------------------------------------------------

    /// One step of `n` DAA blocks, `c` of them this class's, starting at `from`.
    fn step(from: u64, total: u64, class: u64) -> PalwRetargetStepV1 {
        PalwRetargetStepV1 { parent_daa: from, block_daa: from + total, class_daa_blocks: class, total_daa_blocks: total }
    }

    /// **A one-class network is a deliberate no-op**, and that is the property that catches a
    /// cadence-based expectation.
    ///
    /// The expectation is a share of REALIZED production, so at 1000‰ with every block in the class
    /// observed equals expected exactly and the target never moves — however many boundaries are
    /// crossed, and whatever the block rate. A wall-clock expectation would move it, start fighting
    /// `calculate_difficulty_bits` over one cadence, and put a host's clock into fork-choice weight.
    #[test]
    fn a_single_class_holding_the_whole_share_never_retargets() {
        let boot = u128::MAX >> 20;
        for (label, steps) in [
            ("one block per step", (0..40u64).map(|i| step(i, 1, 1)).collect::<Vec<_>>()),
            ("wide mergesets", (0..10u64).map(|i| step(i * 7, 7, 7)).collect()),
            ("uneven steps", vec![step(0, 3, 3), step(3, 11, 11), step(14, 1, 1), step(15, 25, 25)]),
        ] {
            let folded = fold_class_target_v1(boot, &steps, 1_000, 10, 4).unwrap();
            assert_eq!(folded, boot, "{label}: a class that IS the network cannot miss its share");
        }
        // And the rate is irrelevant: ten times the blocks over the same boundaries is still a no-op.
        let dense: Vec<_> = (0..40u64).map(|i| step(i * 10, 10, 10)).collect();
        assert_eq!(fold_class_target_v1(boot, &dense, 1_000, 10, 4).unwrap(), boot);
    }

    /// A class that misses its share of realized production gets EASIER, by the observed ratio,
    /// clamped; one that exceeds it gets harder.
    #[test]
    fn a_class_that_misses_its_share_eases_and_one_that_floods_hardens() {
        let boot = u128::MAX >> 20;
        // Two classes at 500‰ each. Over a 20-block span this class produced 5, expecting 10.
        let steps = vec![step(0, 20, 5)];
        let eased = fold_class_target_v1(boot, &steps, 500, 20, 4).unwrap();
        assert_eq!(eased, boot * 2, "5 of an expected 10 doubles the target");

        // The mirror: 15 of an expected 10 hardens it by exactly 10/15.
        let hardened = fold_class_target_v1(boot, &[step(0, 20, 15)], 500, 20, 4).unwrap();
        assert_eq!(hardened, boot * 10 / 15);

        // Zero production is the full easing clamp, not a division.
        assert_eq!(fold_class_target_v1(boot, &[step(0, 20, 0)], 500, 20, 4).unwrap(), boot * 4);
        // And the clamp binds in the hardening direction too.
        assert_eq!(fold_class_target_v1(boot, &[step(0, 1_000, 1_000)], 1, 1_000, 4).unwrap(), boot / 4);
    }

    /// The trailing span is not applied: a target must not depend on how far into the open span the
    /// reader happens to be.
    #[test]
    fn only_closed_spans_retarget() {
        let boot = u128::MAX >> 20;
        // 19 blocks of a 20-block interval: no boundary crossed, no retarget, however lopsided.
        assert_eq!(fold_class_target_v1(boot, &[step(0, 19, 0)], 500, 20, 4).unwrap(), boot);
        // The 20th closes it.
        assert_ne!(fold_class_target_v1(boot, &[step(0, 20, 0)], 500, 20, 4).unwrap(), boot);
        // Reading one block further into the NEXT span does not change the answer.
        assert_eq!(
            fold_class_target_v1(boot, &[step(0, 20, 0), step(20, 1, 0)], 500, 20, 4).unwrap(),
            fold_class_target_v1(boot, &[step(0, 20, 0)], 500, 20, 4).unwrap()
        );
    }

    /// Boundaries are ABSOLUTE multiples of the interval, so which spans close is a property of the
    /// chain and not of where the fold started — the property a bounded-memory fold needs to be a
    /// rule rather than an approximation. And a leading partial span is measured against its OWN
    /// realized total, so it is not systematically under-expected.
    #[test]
    fn boundaries_are_absolute_and_a_partial_leading_span_is_measured_correctly() {
        let boot = u128::MAX >> 20;
        // Start mid-interval at DAA 15 and run to 40: boundaries at 20 and 40 close two spans, the
        // first only five blocks long. Each expects HALF OF ITS OWN total, so the five-block span is
        // measured against 3 expected rather than against a full interval's 10 — a leading partial
        // span is not systematically starved. Producing none of either is two full easings, and the
        // clamp binds PER ADJUSTMENT, so they compose to 16x rather than being re-clamped to 4x.
        let late = fold_class_target_v1(boot, &[step(15, 5, 0), step(20, 20, 0)], 500, 20, 4).unwrap();
        assert_eq!(late, boot * 16, "two adjustments, each clamped at 4x");

        // A step that jumps TWO boundaries at once closes ONE span, on the census it accumulated. Its
        // merged blocks have no order that would let them be split between the intervals they span,
        // and the second crossing would otherwise close an empty census.
        let wide = fold_class_target_v1(boot, &[step(0, 45, 45)], 1_000, 20, 4).unwrap();
        assert_eq!(wide, boot, "a class that is the whole network stays exact across the jump");
        // Pinned against the per-crossing reading: a 45-block jump at 500‰ producing nothing eases
        // ONCE (4x), not twice (16x).
        assert_eq!(fold_class_target_v1(boot, &[step(0, 45, 0)], 500, 20, 4).unwrap(), boot * 4);
    }

    /// A span too short for a small share to round up to one expected block is unanswerable, not a
    /// miss — otherwise every short span would ease every small class's target.
    #[test]
    fn a_span_that_expects_less_than_one_block_does_not_retarget() {
        let boot = u128::MAX >> 20;
        // 1‰ of 4 blocks rounds to 0.
        assert_eq!(fold_class_target_v1(boot, &[step(0, 4, 0)], 1, 4, 4).unwrap(), boot);
        // 1‰ of 500 rounds to 1, and producing none of it eases.
        assert_eq!(fold_class_target_v1(boot, &[step(0, 500, 0)], 1, 500, 4).unwrap(), boot * 4);
        // Rounding is to NEAREST, not down: 1‰ of 500 is exactly 0.5 and rounds up to 1. A floor
        // would bias every class toward "produced more than expected" and harden the network's
        // targets over time.
        assert_eq!(
            retarget_over_span_v1(boot, &PalwClassSpanCensusV1 { class_daa_blocks: 1, total_daa_blocks: 500 }, 1, 4).unwrap(),
            boot,
            "one produced against one expected is exact"
        );
    }

    /// Every degenerate input is a refusal, and the refusals are the fail-closed ones: a zero target
    /// is the MAXIMUM-weight value, and a miscounted step would ease a class and inflate its pwu.
    #[test]
    fn the_fold_refuses_every_incoherent_input_rather_than_easing_a_target() {
        let boot = u128::MAX >> 20;
        let good = vec![step(0, 20, 10)];
        assert!(fold_class_target_v1(boot, &good, 500, 20, 4).is_ok());

        assert_eq!(fold_class_target_v1(0, &good, 500, 20, 4), Err(PalwClassDaaError::ZeroPreviousTarget));
        assert_eq!(fold_class_target_v1(boot, &good, 500, 0, 4), Err(PalwClassDaaError::ZeroRetargetInterval));
        assert_eq!(fold_class_target_v1(boot, &good, 0, 20, 4), Err(PalwClassDaaError::ShareOutOfRange { got: 0 }));
        assert_eq!(fold_class_target_v1(boot, &good, 1_001, 20, 4), Err(PalwClassDaaError::ShareOutOfRange { got: 1_001 }));

        // The gatherer's consistency check: an advance that does not equal the census is a miscount,
        // and a miscount in the class's favour is exactly what must not be believed.
        let miscounted = vec![PalwRetargetStepV1 { parent_daa: 0, block_daa: 20, class_daa_blocks: 5, total_daa_blocks: 10 }];
        assert_eq!(
            fold_class_target_v1(boot, &miscounted, 500, 20, 4),
            Err(PalwClassDaaError::StepCensusMismatch { advance: 20, counted: 10 })
        );
        let backward = vec![PalwRetargetStepV1 { parent_daa: 20, block_daa: 10, class_daa_blocks: 0, total_daa_blocks: 0 }];
        assert_eq!(
            fold_class_target_v1(boot, &backward, 500, 20, 4),
            Err(PalwClassDaaError::NonMonotonicStep { parent_daa: 20, block_daa: 10 })
        );
        let over = vec![PalwRetargetStepV1 { parent_daa: 0, block_daa: 10, class_daa_blocks: 11, total_daa_blocks: 10 }];
        assert_eq!(fold_class_target_v1(boot, &over, 500, 20, 4), Err(PalwClassDaaError::CensusExceedsSpan { class: 11, total: 10 }));
        // A gap in the walk means the fold is measuring a different chain than the caller walked.
        let gapped = vec![step(0, 10, 5), step(11, 10, 5)];
        assert_eq!(
            fold_class_target_v1(boot, &gapped, 500, 20, 4),
            Err(PalwClassDaaError::DiscontiguousSteps { expected_parent_daa: 10, got: 11 })
        );
        // An empty span reaching the span rule directly is unanswerable rather than a miss.
        assert_eq!(retarget_over_span_v1(boot, &PalwClassSpanCensusV1::default(), 500, 4), Err(PalwClassDaaError::EmptySpan));
        // `retarget_over_span_v1` is public, so its own consistency check has to hold for a caller
        // that does not come through the fold — the fold catches this per STEP and would otherwise
        // be the only thing standing between an impossible census and an eased target.
        assert_eq!(
            retarget_over_span_v1(boot, &PalwClassSpanCensusV1 { class_daa_blocks: 11, total_daa_blocks: 10 }, 500, 4),
            Err(PalwClassDaaError::CensusExceedsSpan { class: 11, total: 10 })
        );
        assert_eq!(
            retarget_over_span_v1(0, &PalwClassSpanCensusV1 { class_daa_blocks: 1, total_daa_blocks: 10 }, 500, 4),
            Err(PalwClassDaaError::ZeroPreviousTarget)
        );
    }

    /// No steps is the boot target, not an error and not a retarget — the genesis case.
    #[test]
    fn an_empty_chain_folds_to_the_boot_target() {
        let boot = u128::MAX >> 20;
        assert_eq!(fold_class_target_v1(boot, &[], 1_000, 10, 4).unwrap(), boot);
        assert_eq!(fold_class_target_v1(boot, &[], 1, 10, 4).unwrap(), boot);
    }

    /// `id(0)` is the base class in every fixture below — the liveness floor.
    fn base() -> Hash64 {
        id(0)
    }

    fn set(base_share: u16, shares: &[(u64, u16)]) -> PalwDifficultyDomainSetV1 {
        let mut map: BTreeMap<Hash64, u16> = shares.iter().map(|(s, p)| (id(*s), *p)).collect();
        map.insert(base(), base_share);
        PalwDifficultyDomainSetV1::new(base(), map).unwrap()
    }

    /// Construction enforces conservation to exactly 1000‰, refuses a zero-share class, and
    /// refuses a set whose base class is absent — the floor must BE a class now, because there is
    /// no hash lane outside the classes to fall back on (ADR-0039 W6′).
    #[test]
    fn construction_enforces_conservation_and_the_base_class() {
        assert!(PalwDifficultyDomainSetV1::new(base(), [(base(), 10), (id(1), 500), (id(2), 490)].into()).is_ok());
        assert!(matches!(
            PalwDifficultyDomainSetV1::new(base(), [(base(), 10), (id(1), 500), (id(2), 500)].into()),
            Err(PalwClassDaaError::SharesDoNotConserve { got: 1010, .. })
        ));
        // The base class must be present...
        assert!(matches!(
            PalwDifficultyDomainSetV1::new(base(), [(id(1), 1000)].into()),
            Err(PalwClassDaaError::BaseClassAbsent { .. })
        ));
        // ...and, like every class, must hold a nonzero share.
        assert!(matches!(
            PalwDifficultyDomainSetV1::new(base(), [(base(), 0), (id(1), 1000)].into()),
            Err(PalwClassDaaError::ZeroShare { .. })
        ));
    }

    /// Freezing a class conserves shares exactly, splits proportionally, and is
    /// deterministic (whole-permille arithmetic, largest-remainder for the residue).
    #[test]
    fn removal_redistributes_proportionally_and_conserves() {
        let mut s = set(10, &[(1, 600), (2, 300), (3, 90)]);
        s.remove_class(&id(1)).unwrap();
        // 600 splits over the survivors, which now INCLUDE the base class (10): total 400, so
        // 600×10/400 = 15, 600×300/400 = 450, 600×90/400 = 135 — exact, no residue.
        assert_eq!(s.base_share_permille(), 10 + 15);
        assert_eq!(s.class_shares_permille[&id(2)], 300 + 450);
        assert_eq!(s.class_shares_permille[&id(3)], 90 + 135);
        assert!(s.validate().is_ok());
    }

    /// Freezing every other class leaves the BASE class holding the whole cadence — degraded,
    /// visible, and still PALW work (ADR-0039 W6′). The base class itself is not removable, so the
    /// set can never reach "no producer"; with nothing able to certify work the network halts
    /// loudly rather than falling back to a hash lane, which is the trade ADR-0039 makes
    /// deliberately.
    #[test]
    fn the_base_class_is_the_degraded_mode_and_cannot_be_removed() {
        let mut s = set(10, &[(1, 990)]);
        s.remove_class(&id(1)).unwrap();
        assert_eq!(s.base_share_permille(), 1000, "the floor takes the whole cadence");
        assert_eq!(s.class_shares_permille.len(), 1, "and it is still a CLASS, not a lane outside them");
        assert!(s.validate().is_ok());
        // The floor cannot be removed, and an unknown class is an error rather than a no-op.
        assert_eq!(s.remove_class(&base()), Err(PalwClassDaaError::BaseClassNotRemovable { class_id: base() }));
        assert_eq!(s.remove_class(&id(1)), Err(PalwClassDaaError::UnknownClass { class_id: id(1) }));
    }

    /// Sequential removals conserve at every step regardless of order, and both orders end
    /// with the survivor holding everything but the floor.
    #[test]
    fn removal_order_conserves_and_is_a_function_of_the_order() {
        let mut outcomes = Vec::new();
        for order in [[1u64, 2], [2, 1]] {
            let mut s = set(10, &[(1, 500), (2, 300), (3, 190)]);
            s.remove_class(&id(order[0])).unwrap();
            assert!(s.validate().is_ok(), "conservation holds at every step");
            s.remove_class(&id(order[1])).unwrap();
            assert!(s.validate().is_ok());
            outcomes.push((s.base_share_permille(), s.class_shares_permille[&id(3)]));
        }
        // Conservation is absolute in both orders.
        for (base, c3) in &outcomes {
            assert_eq!(*base as u32 + *c3 as u32, PALW_CLASS_SHARE_DENOMINATOR as u32);
        }
        // But the split is NOT order-independent, and this is a real property, not a rounding
        // nicety: largest-remainder redistribution is path-dependent, and putting the liveness floor
        // INSIDE the class map (ADR-0039 W6′) made it observable — the base class now takes a share
        // of every redistribution. MEASURED: removing 1 then 2 lands (50, 950); 2 then 1 lands
        // (49, 951).
        assert_eq!(outcomes[0], (50, 950));
        assert_eq!(outcomes[1], (49, 951));
        assert_ne!(outcomes[0], outcomes[1], "if these ever agree, the note above is stale");

        // The consequence is a REQUIREMENT ON THE CALLER, which is why it is pinned here: the
        // removal order must be a function of the chain (the order classes were frozen in), never
        // of a node's iteration order over a pending set. Two nodes applying the same freezes in
        // different orders would carry shares differing by the residue and then retarget apart.
    }

    /// The retarget: starving classes ease by the observed ratio, flooding classes harden,
    /// both clamped at ×max_factor per window, and a silent window is the full easing clamp.
    #[test]
    fn retarget_follows_ratio_and_clamps() {
        // Starving: expected 100, observed 50 ⇒ ×2 easier.
        assert_eq!(adjust_class_target_v1(1_000_000, 50, 100, 4).unwrap(), 2_000_000);
        // Flooding: observed 400 ⇒ ÷4 — exactly at the clamp.
        assert_eq!(adjust_class_target_v1(1_000_000, 400, 100, 4).unwrap(), 250_000);
        // Beyond the clamp in both directions.
        assert_eq!(adjust_class_target_v1(1_000_000, 1_000, 100, 4).unwrap(), 250_000);
        assert_eq!(adjust_class_target_v1(1_000_000, 1, 100, 4).unwrap(), 4_000_000);
        // Dead-quiet window: full easing clamp, no division by zero.
        assert_eq!(adjust_class_target_v1(1_000_000, 0, 100, 4).unwrap(), 4_000_000);
        // Balanced window: unchanged.
        assert_eq!(adjust_class_target_v1(1_000_000, 100, 100, 4).unwrap(), 1_000_000);
        // Guard rails.
        assert_eq!(adjust_class_target_v1(1, 1, 1, 1), Err(PalwClassDaaError::MaxFactorTooSmall));
        assert_eq!(adjust_class_target_v1(1, 1, 0, 4), Err(PalwClassDaaError::ZeroExpectedBlocks));
    }

    /// The wide-target corner: a near-max u128 target retargets without overflow and lands
    /// inside the clamp (the mul_div split is exercised above the 64-bit boundary).
    #[test]
    fn retarget_is_overflow_free_at_wide_targets() {
        let wide = u128::MAX / 8;
        let eased = adjust_class_target_v1(wide, 50, 100, 4).unwrap();
        assert_eq!(eased, wide * 2);
        let hardened = adjust_class_target_v1(wide, 200, 100, 4).unwrap();
        assert_eq!(hardened, wide / 2);
    }

    /// Borsh roundtrip of the domain set.
    #[test]
    fn domain_set_roundtrips_borsh() {
        let s = set(10, &[(1, 500), (2, 490)]);
        assert_eq!(s, borsh::from_slice::<PalwDifficultyDomainSetV1>(&borsh::to_vec(&s).unwrap()).unwrap());
    }
}
