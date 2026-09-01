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

use crate::config::params::BlockrateParams;
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
    #[error("unsupported class-DAA params version {got} (expected {expected})")]
    UnsupportedParamsVersion { got: u16, expected: u16 },
    #[error("history_retargets must be nonzero — a loop with no memory has no anchor but the boot target")]
    ZeroHistory,
    #[error(
        "the fold's memory is {memory_daa} DAA but the pruning horizon is {pruning_depth} — a node synced from a \
         pruning point could not re-derive the target, and two nodes would weigh the same block differently"
    )]
    MemoryOverflowsHorizon { memory_daa: u64, pruning_depth: u64 },
}

pub const PALW_CLASS_DAA_PARAMS_VERSION_V1: u16 = 1;

/// The consensus constants one class's DAA loop runs on, frozen per network and carried in the same
/// fence the class itself arrives in ([`crate::palw_credit::PalwCreditParamsV1`]) — so a network
/// cannot have a registered class without a retarget, or a retarget without a class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwClassDaaParamsV1 {
    pub version: u16,
    /// DAA scores per retarget boundary. Boundaries are ABSOLUTE multiples of this, so which spans
    /// close is a property of the chain rather than of where a reader started folding.
    pub retarget_interval_daa: u64,
    /// How many boundaries back the fold reads — the loop's MEMORY. Nothing older than
    /// `retarget_interval_daa × history_retargets` influences a target.
    ///
    /// Bounded on purpose. An unbounded iterative retarget cannot survive pruning: a node that
    /// synced from a pruning point cannot re-derive the chain of targets a long-running node holds,
    /// and the two would then weigh the same block differently — a partition, not a slow node.
    /// Bitcoin avoids this by DECLARING the target in the header; until a PoW-path digest binds
    /// `Header::palw_commitment` there is nowhere to declare it, so the loop forgets instead, inside
    /// the pruning horizon, by rule. [`Self::validate`] is what makes that a rule.
    pub history_retargets: u32,
    /// Per-adjustment clamp (≥ 2), passed through to [`adjust_class_target_v1`].
    pub max_factor: u32,
    /// The target in force before any boundary has been crossed, and the anchor of every fold.
    ///
    /// Validated nonzero: `palw_pwu::palw_expected_attempts_v1(0)` saturates, so a zero here is the
    /// MAXIMUM pwu on the network, not a neutral or empty value.
    pub boot_target: u128,
}

impl PalwClassDaaParamsV1 {
    /// Checked against THIS network's real constants, exactly as
    /// [`crate::palw_schedule::PalwScheduleParamsV1::validate`] is.
    pub fn validate(&self, blockrate: &BlockrateParams) -> Result<(), PalwClassDaaError> {
        if self.version != PALW_CLASS_DAA_PARAMS_VERSION_V1 {
            return Err(PalwClassDaaError::UnsupportedParamsVersion { got: self.version, expected: PALW_CLASS_DAA_PARAMS_VERSION_V1 });
        }
        if self.boot_target == 0 {
            return Err(PalwClassDaaError::ZeroPreviousTarget);
        }
        if self.retarget_interval_daa == 0 {
            return Err(PalwClassDaaError::ZeroRetargetInterval);
        }
        if self.max_factor < 2 {
            return Err(PalwClassDaaError::MaxFactorTooSmall);
        }
        if self.history_retargets == 0 {
            return Err(PalwClassDaaError::ZeroHistory);
        }
        // THE BINDING CONSTRAINT: the fold's memory must fit inside the pruning horizon.
        //
        // A node that synced from a pruning point cannot walk further back than the horizon, so a
        // memory longer than it makes the target depend on how much history the reader happens to
        // hold. Two nodes would then weigh the same block differently and prefer different tips —
        // and unlike a slow node that is a partition, because the disagreement is permanent. This is
        // the one inequality that is about consensus rather than about tuning.
        let memory = (self.retarget_interval_daa)
            .checked_mul(self.history_retargets as u64)
            .ok_or(PalwClassDaaError::MemoryOverflowsHorizon { memory_daa: u64::MAX, pruning_depth: blockrate.pruning_depth })?;
        if memory >= blockrate.pruning_depth {
            return Err(PalwClassDaaError::MemoryOverflowsHorizon { memory_daa: memory, pruning_depth: blockrate.pruning_depth });
        }
        Ok(())
    }

    /// The fold's memory in DAA scores — how far back a gatherer must walk, and no further.
    pub fn memory_daa(&self) -> u64 {
        self.retarget_interval_daa.saturating_mul(self.history_retargets as u64)
    }

    /// A parameter set a fence may carry on ANY shipped network: a 180-DAA interval remembered four
    /// boundaries back, clamped at 4x, booting from a target that expects ~2^20 attempts.
    ///
    /// Deliberately ONE constructor rather than a literal per call site: every consumer of the fence
    /// needs a `class_daa`, and five copies of five numbers is five chances for a fixture to stop
    /// exercising what it claims to.
    ///
    /// The memory is `180 × 4 = 720` DAA, which fits the TIGHTEST shipped pruning horizon — the 120 s
    /// PALW testnet's **1 144**. My first draft used a 720-DAA interval, whose 2 880 memory does not,
    /// and the doc claimed it was universally valid; the fence-installability test caught it. The
    /// bound is asserted against every shipped preset now rather than described.
    pub fn stage1_defaults() -> Self {
        Self {
            version: PALW_CLASS_DAA_PARAMS_VERSION_V1,
            retarget_interval_daa: 180,
            history_retargets: 4,
            max_factor: 4,
            boot_target: u128::MAX >> 20,
        }
    }

    /// The one-class domain set a single-registration fence implies: this class at the whole 1000‰.
    ///
    /// A fence carries exactly ONE registration, and a block does not record its class
    /// (`pow_layer0::check_palw_commitment_shape` requires an empty `palw_commitment` on PALW headers
    /// too), so "PALW header ⇒ that class" is EXACT here and would be a LIE on a multi-class network.
    /// Taking the share from this set rather than from a caller's argument is what makes the
    /// single-class assumption visible at the one place it holds — and a one-class set at 1000‰ makes
    /// the retarget a deliberate no-op, which is the honest behaviour when there is no second class
    /// to redistribute share with.
    pub fn single_class_domain(&self, class_id: Hash64) -> Result<PalwDifficultyDomainSetV1, PalwClassDaaError> {
        PalwDifficultyDomainSetV1::new(class_id, BTreeMap::from([(class_id, PALW_CLASS_SHARE_DENOMINATOR)]))
    }
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

    /// One class's share of the cadence, **looked up by its id**.
    ///
    /// `None` for a class this set does not hold, and that is the load-bearing half: a class with
    /// no share is not in the difficulty domain, so it has no expectation to retarget against.
    /// Answering `PALW_CLASS_SHARE_DENOMINATOR` — "it must be the only one" — is the failure this
    /// exists to prevent, because it is exactly right while one class is registered and silently
    /// wrong the moment a second becomes Active: both would then retarget against the whole
    /// cadence, each crediting itself work the other did, and both targets would ease until the
    /// chain ran at twice its intended rate.
    ///
    /// The same rule the class facts, the class target and the bonds view each ended up on —
    /// bind by lookup key, never by adjacency.
    pub fn share_permille(&self, class_id: &Hash64) -> Option<u16> {
        self.class_shares_permille.get(class_id).copied()
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

/// **An idle class converges toward the price the producing classes are actually paying, and
/// never past it** (ADR-0071 Decision 1, as amended at implementation).
///
/// [`retarget_over_span_v1`] measures a class against its share of what the span REALIZED, and the
/// renormalization that makes `Σ expected = Σ observed` is load-bearing: three separate audit
/// findings (H1, F1/F10/F27) were all the same shape — an expectation that does not sum back to
/// the realized total gives every class the same one-directional multiplier at every boundary, with
/// `max_factor` bounding each step and nothing bounding the walk. Measured once at 4^12 over twelve
/// boundaries, ending at a target of zero, from which no node can rejoin.
///
/// So the sum rule stays, and with it a blind spot that is exactly one case wide. A class that
/// produced ANY blocks is measured correctly — at 500‰ each, A producing 100 and B producing 20
/// gives A `observed 100 > expected 60` and B `observed 20 < expected 60`, so B eases. A class that
/// produced NOTHING is skipped, and that is the case the chain could never repair: ADR-0054's share
/// path decays its cadence but never touches its price, so a class whose target is too hard to win
/// even one block per epoch stays too hard forever. An entrant is "priced like the incumbent" at
/// registration and then never tracks the incumbent again.
///
/// **Why the price cannot simply be eased.** Silence is not evidence of trying. The chain sees
/// block counts, not attempts, so "locked out" and "nobody ran it" are the same observation, and a
/// rule that eases on silence lets a registrant buy cadence with patience instead of work: register,
/// wait for the target to walk to trivial, then take the class's whole epoch budget for free.
///
/// **What the price CAN be is an incumbent's.** `floor_price` is the hardest target any class that
/// actually produced in this span is paying. An idle class harder than that is paying more than
/// anyone and losing; it converges toward that price, `max_factor`-bounded per boundary, and stops
/// there. An idle class already easier than that is not locked out — nobody ran it — and does not
/// move. Nothing can ever be priced below what a producing class pays, so patience buys the
/// incumbent's price and never a better one, which is the same thing work buys.
///
/// Returns the class's next target. Arithmetically independent of `retarget_over_span_v1`: an idle
/// class is outside the sum by construction, so this cannot disturb any producer's expectation.
pub fn converge_idle_target_v1(current_target: u128, floor_price: u128, max_factor: u32) -> u128 {
    // Not locked out: it is at least as cheap as the cheapest price anyone paid to produce here.
    if current_target >= floor_price {
        return current_target;
    }
    let step = current_target.saturating_mul(max_factor.max(1) as u128);
    step.min(floor_price).max(current_target)
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

// =============================================================================================
// ADR-0054 — a class's cadence share follows its own production (the share-raise path)
// =============================================================================================

/// **What one class did with the cadence it was already given, over one closed epoch.**
///
/// Both numbers are the chain's own: `produced` is the epoch counter the transition increments per
/// accepted block, and `budget` is the cap ADR-0045 Decision 2 derived for that same epoch. Their
/// relation is the whole signal — a class that produced everything its budget allowed was stopped
/// by its SHARE and not by its ability, and a class that produced nothing was not using the share
/// it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClassEpochUseV1 {
    pub produced: u64,
    pub budget: u64,
}

/// The share table after one epoch of growth and decay, and the reason each permille moved.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PalwShareGrowthV1 {
    /// The new table. Always sums to the same denominator the input did.
    pub shares: BTreeMap<Hash64, u16>,
    /// Classes that filled their budget and took permille from the floor, with the amount.
    pub grew: BTreeMap<Hash64, u16>,
    /// Classes that produced nothing and returned permille to the floor, with the amount.
    pub decayed: BTreeMap<Hash64, u16>,
}

/// **The share-raise path: production earns cadence, silence returns it** (ADR-0054).
///
/// Until this existed a class's share was fixed the moment it was granted — `write_share` had one
/// caller, the activation grant — so a post-genesis entrant was pinned at
/// `min_grantable_share_permille` forever. That number is chosen so its holder's expectation is
/// exactly one block per epoch, and ADR-0045's budget caps it at exactly that, so the per-class
/// retarget had no reachable input: observed could only be 0 (skipped, audit H1) or 1 (its
/// expectation, a no-op). Measured on a two-class chain carrying the real `PALW-QWEN36` class, its
/// target did not move across four epochs in either state. The difficulty loop was not broken; it
/// was starved of the one quantity that could feed it.
///
/// # Why this is derived rather than granted
///
/// ADR-0038 Decision D's rule for prices is that they are discovered and never maintained, and
/// ADR-0045 left "automatic share re-allocation from class health" as the future object with "its
/// own authorization story". This is that re-allocation with NO authorization story, which is the
/// point: nobody submits it, so there is nothing to forge, nobody to bribe, and no key whose loss
/// freezes the table. A class grows only by producing every block its current share admits — work
/// it must actually perform, priced by its own difficulty — and shrinks only by producing nothing.
///
/// # The floor is the reservoir, and it has a reserve
///
/// Every permille moved comes from or returns to the BASE class, which starts holding the whole
/// table and is the one class every node can always run. Two bounds make that safe:
///
/// * the floor is never donated below `base_reserve_permille` (nor below the grant floor), so the
///   class that guarantees liveness always keeps enough cadence to carry the chain, and
/// * a class never falls below `grant_floor_permille`, because a share below it is a zero epoch
///   budget — a class that cannot produce is a frozen class wearing the wrong status.
///
/// Growth and decay are the same step — `max(1‰, share × growth‰ / 1000)` — so the mechanism is
/// symmetric and bounded: a class needs many consecutive productive epochs to reach a large share
/// and gives it back at the same rate. `growth_permille = 0` disables the whole rule, which is
/// what every network built before ADR-0054 runs at.
///
/// Consensus-inert on its own: this is arithmetic. The transition decides WHEN it runs (one closed
/// epoch boundary, immediately after the retarget and before the new epoch's budgets are derived).
pub fn derive_class_share_growth_v1(
    shares: &BTreeMap<Hash64, u16>,
    frozen: &std::collections::BTreeSet<Hash64>,
    use_by_class: &BTreeMap<Hash64, PalwClassEpochUseV1>,
    base_class_id: Hash64,
    growth_permille: u16,
    base_reserve_permille: u16,
    grant_floor_permille: u16,
) -> PalwShareGrowthV1 {
    let mut out = PalwShareGrowthV1 { shares: shares.clone(), ..Default::default() };
    if growth_permille == 0 || !shares.contains_key(&base_class_id) {
        return out;
    }
    let step = |share: u16| -> u16 {
        let scaled = (u32::from(share) * u32::from(growth_permille)) / 1000;
        u16::try_from(scaled.max(1)).unwrap_or(u16::MAX)
    };
    // The floor's own floor: whichever of the two bounds binds first.
    let base_reserve = base_reserve_permille.max(grant_floor_permille);

    // Decay first, then growth: a silent class's permille is back in the reservoir before the
    // productive ones draw on it, so one epoch can move a permille from an idle class to a busy
    // one. Growth-first would make the same pair of facts take two epochs, and which order runs
    // is a consensus rule either way — so it is stated here rather than left to map iteration.
    for (class_id, share) in shares.iter() {
        if *class_id == base_class_id || frozen.contains(class_id) {
            continue;
        }
        let produced = use_by_class.get(class_id).map(|u| u.produced).unwrap_or(0);
        if produced > 0 {
            continue;
        }
        let give = step(*share).min(share.saturating_sub(grant_floor_permille));
        if give == 0 {
            continue;
        }
        *out.shares.entry(*class_id).or_insert(*share) = share - give;
        *out.shares.entry(base_class_id).or_insert(0) += give;
        out.decayed.insert(*class_id, give);
    }

    for (class_id, share) in shares.iter() {
        if *class_id == base_class_id || frozen.contains(class_id) {
            continue;
        }
        let Some(used) = use_by_class.get(class_id) else { continue };
        // Filled its allowance: the budget stopped it, which is a statement about its SHARE.
        if used.budget == 0 || used.produced < used.budget {
            continue;
        }
        let base_share = out.shares.get(&base_class_id).copied().unwrap_or(0);
        let available = base_share.saturating_sub(base_reserve);
        // The current share, not the input's: a class cannot both decay and grow, but the floor's
        // balance moves under this loop as earlier classes draw on it.
        let current = out.shares.get(class_id).copied().unwrap_or(*share);
        let take = step(current).min(available);
        if take == 0 {
            continue;
        }
        *out.shares.entry(*class_id).or_insert(current) = current + take;
        *out.shares.entry(base_class_id).or_insert(0) = base_share - take;
        out.grew.insert(*class_id, take);
    }

    debug_assert_eq!(
        out.shares.values().map(|s| u32::from(*s)).sum::<u32>(),
        shares.values().map(|s| u32::from(*s)).sum::<u32>(),
        "every permille moved is a transfer with the floor, so the denominator is conserved"
    );
    out
}

#[cfg(test)]
mod tests {

    /// **The locked-out repair, asserted as the three cases it has to tell apart** (ADR-0071
    /// Decision 1).
    #[test]
    fn an_idle_class_converges_to_the_incumbent_price_and_never_past_it() {
        const INCUMBENT: u128 = 1_000_000;
        // 1. Harder than every producer, so it is losing a lottery it pays more than anyone to
        //    enter. It moves — this is the case that had no repair at all.
        let stuck = INCUMBENT / 16;
        let eased = converge_idle_target_v1(stuck, INCUMBENT, 4);
        assert!(eased > stuck, "a class priced above every incumbent must be able to move");
        assert_eq!(eased, stuck * 4, "and by at most max_factor in one boundary");
        assert!(eased <= INCUMBENT, "never past the price a producing class is actually paying");
        // Repeated boundaries converge and then STOP at the incumbent's price.
        let mut t = stuck;
        for _ in 0..10 {
            t = converge_idle_target_v1(t, INCUMBENT, 4);
        }
        assert_eq!(t, INCUMBENT, "it converges to the incumbent price");
        assert_eq!(converge_idle_target_v1(t, INCUMBENT, 4), INCUMBENT, "and does not walk past it, ever");

        // 2. Already cheaper than the cheapest producer and still silent: nobody ran it. Easing
        //    here is the unbounded walk H1 closed, and buying cadence with patience.
        let idle_but_cheap = INCUMBENT * 8;
        assert_eq!(converge_idle_target_v1(idle_but_cheap, INCUMBENT, 4), idle_but_cheap, "silence is not evidence of trying");

        // 3. Exactly at the incumbent price is case 2, not case 1 — the boundary is inclusive so
        //    a class at parity cannot ratchet itself below parity one epoch at a time.
        assert_eq!(converge_idle_target_v1(INCUMBENT, INCUMBENT, 4), INCUMBENT);
    }

    /// The overflow arm, because a target near `u128::MAX` times `max_factor` is the one input
    /// that could wrap a difficulty into "impossibly hard" — the accident this codebase has
    /// already paid for once in `palw_pwu_v1`.
    #[test]
    fn converging_an_almost_maximal_target_saturates_rather_than_wrapping() {
        let huge = u128::MAX / 2;
        assert_eq!(converge_idle_target_v1(huge, u128::MAX, 8), u128::MAX, "saturating, then clamped to the price");
        assert_eq!(converge_idle_target_v1(u128::MAX, u128::MAX, 8), u128::MAX);
        // A zero factor would freeze the rule; it is treated as one, which is a no-op and not a
        // multiply-by-zero that hands back the hardest target representable.
        assert_eq!(converge_idle_target_v1(100, 1_000, 0), 100);
    }

    /// ADR-0038 Decision D: a share is looked up, and a class outside the domain has none.
    ///
    /// The single-class case returns the full denominator, which is why hardcoding it looked
    /// correct. The two-class case is the one that matters: each class must get its own share, and
    /// a class the set does not hold must get `None` rather than the denominator — two classes each
    /// retargeting against the whole cadence would credit themselves the work the other did, and
    /// both targets would ease until the chain ran at twice its intended rate.
    #[test]
    fn a_class_share_is_looked_up_and_a_stranger_has_none() {
        let a = Hash64::from_u64_word(0xA);
        let b = Hash64::from_u64_word(0xB);
        let stranger = Hash64::from_u64_word(0xF);

        let single = PalwDifficultyDomainSetV1::new(a, BTreeMap::from([(a, PALW_CLASS_SHARE_DENOMINATOR)])).unwrap();
        assert_eq!(single.share_permille(&a), Some(PALW_CLASS_SHARE_DENOMINATOR));
        assert_eq!(single.share_permille(&stranger), None, "a class outside the domain must not be handed the cadence");

        let split = PalwDifficultyDomainSetV1::new(a, BTreeMap::from([(a, 600), (b, 400)])).unwrap();
        assert_eq!(split.share_permille(&a), Some(600));
        assert_eq!(split.share_permille(&b), Some(400));
        assert_eq!(split.share_permille(&stranger), None);
        // The shares conserve, which is what makes 'each retargets against its own share' add up to
        // one cadence rather than to several.
        assert_eq!(
            split.share_permille(&a).unwrap() as u32 + split.share_permille(&b).unwrap() as u32,
            PALW_CLASS_SHARE_DENOMINATOR as u32
        );
    }
    use super::*;
    use crate::config::params::BlockrateParams;

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

    /// The fold's memory must fit inside the pruning horizon, and that is the one inequality here
    /// that is about consensus rather than tuning.
    ///
    /// A node synced from a pruning point cannot walk further back than the horizon. A memory longer
    /// than it makes the target depend on how much history the reader happens to hold, so two nodes
    /// weigh the same block differently and prefer different tips — permanently, which is a partition
    /// rather than a slow node.
    #[test]
    fn the_folds_memory_must_fit_inside_the_pruning_horizon() {
        let blockrate = BlockrateParams::new_deci_bps();
        let horizon = blockrate.pruning_depth;
        assert!(horizon > 0);

        let params = |interval: u64, history: u32| PalwClassDaaParamsV1 {
            version: PALW_CLASS_DAA_PARAMS_VERSION_V1,
            retarget_interval_daa: interval,
            history_retargets: history,
            max_factor: 4,
            boot_target: u128::MAX >> 20,
        };

        // One DAA short of the horizon is admissible; the horizon itself is not — a memory that
        // reaches exactly as far as the oldest block a pruned node holds cannot be re-derived by it.
        let fits = params(horizon - 1, 1);
        fits.validate(&blockrate).unwrap();
        assert_eq!(fits.memory_daa(), horizon - 1);
        assert_eq!(
            params(horizon, 1).validate(&blockrate),
            Err(PalwClassDaaError::MemoryOverflowsHorizon { memory_daa: horizon, pruning_depth: horizon })
        );
        // The product, not just the interval: a short interval with a long history overruns too.
        assert!(params(horizon / 2, 3).validate(&blockrate).is_err());
        assert!(params(horizon / 4, 3).validate(&blockrate).is_ok());
        // And it cannot be wrapped past the check.
        assert!(matches!(params(u64::MAX, u32::MAX).validate(&blockrate), Err(PalwClassDaaError::MemoryOverflowsHorizon { .. })));

        // A tighter network has a tighter bound — the check reads THIS network's constant, not a
        // constant of its own.
        let two_minute = BlockrateParams::new_two_minute_bps();
        assert!(two_minute.pruning_depth < horizon, "the fixture needs two genuinely different horizons");
        let admissible_on_deci = params(two_minute.pruning_depth, 1);
        admissible_on_deci.validate(&blockrate).unwrap();
        assert!(admissible_on_deci.validate(&two_minute).is_err(), "the same params must not pass on a tighter horizon");
    }

    /// The stage-1 defaults must be installable on EVERY shipped network, and the tightest horizon is
    /// the one that binds.
    ///
    /// Measured: MAINNET and SIMNET 1 080 000, DEVNET 10 800, TESTNET **1 144** — the 120 s PALW
    /// testnet. My first draft of these defaults used a 720-DAA interval, whose 2 880 memory does not
    /// fit 1 144, and its doc asserted universal validity in prose. Prose does not hold; this does.
    #[test]
    fn the_stage1_defaults_fit_every_shipped_pruning_horizon() {
        let defaults = PalwClassDaaParamsV1::stage1_defaults();
        assert_eq!(defaults.memory_daa(), 720);
        let presets = [
            ("mainnet", &crate::config::params::MAINNET_PARAMS),
            ("testnet", &crate::config::params::TESTNET_PARAMS),
            ("devnet", &crate::config::params::DEVNET_PARAMS),
            ("simnet", &crate::config::params::SIMNET_PARAMS),
        ];
        let mut tightest = u64::MAX;
        for (name, params) in presets {
            defaults.validate(&params.blockrate).unwrap_or_else(|e| panic!("{name}: {e}"));
            tightest = tightest.min(params.blockrate.pruning_depth);
        }
        assert_eq!(tightest, 1_144, "the tightest shipped horizon moved — re-check the defaults against it");
        assert!(defaults.memory_daa() < tightest);
        // And the margin is not accidental: a memory at the tightest horizon is refused there while
        // still passing on the roomiest, which is exactly the asymmetry the check exists for.
        let at_horizon = PalwClassDaaParamsV1 { retarget_interval_daa: tightest, history_retargets: 1, ..defaults };
        assert!(at_horizon.validate(&crate::config::params::MAINNET_PARAMS.blockrate).is_ok());
        assert!(at_horizon.validate(&crate::config::params::TESTNET_PARAMS.blockrate).is_err());
    }

    /// Every other degenerate value is a refusal, and each refusal is the fail-closed one.
    #[test]
    fn degenerate_class_daa_params_are_refused() {
        let blockrate = BlockrateParams::new_deci_bps();
        let good = PalwClassDaaParamsV1 {
            version: PALW_CLASS_DAA_PARAMS_VERSION_V1,
            retarget_interval_daa: 720,
            history_retargets: 4,
            max_factor: 4,
            boot_target: u128::MAX >> 20,
        };
        good.validate(&blockrate).unwrap();

        assert!(matches!(
            PalwClassDaaParamsV1 { version: 2, ..good }.validate(&blockrate),
            Err(PalwClassDaaError::UnsupportedParamsVersion { got: 2, .. })
        ));
        // A zero boot target is not "no target" — `palw_pwu` reads it as the MAXIMUM work on the
        // network, so it would make the class the heaviest thing on the DAG from genesis.
        assert_eq!(PalwClassDaaParamsV1 { boot_target: 0, ..good }.validate(&blockrate), Err(PalwClassDaaError::ZeroPreviousTarget));
        assert_eq!(
            PalwClassDaaParamsV1 { retarget_interval_daa: 0, ..good }.validate(&blockrate),
            Err(PalwClassDaaError::ZeroRetargetInterval)
        );
        assert_eq!(PalwClassDaaParamsV1 { max_factor: 1, ..good }.validate(&blockrate), Err(PalwClassDaaError::MaxFactorTooSmall));
        assert_eq!(PalwClassDaaParamsV1 { history_retargets: 0, ..good }.validate(&blockrate), Err(PalwClassDaaError::ZeroHistory));
    }

    /// The single-class domain is where the one-registration assumption becomes visible, and it
    /// composes with the fold into the no-op the assumption implies.
    #[test]
    fn the_single_class_domain_makes_the_retarget_a_deliberate_no_op() {
        let blockrate = BlockrateParams::new_deci_bps();
        let params = PalwClassDaaParamsV1 {
            version: PALW_CLASS_DAA_PARAMS_VERSION_V1,
            retarget_interval_daa: 20,
            history_retargets: 4,
            max_factor: 4,
            boot_target: u128::MAX >> 20,
        };
        params.validate(&blockrate).unwrap();
        let class = id(0xC1);
        let domains = params.single_class_domain(class).unwrap();
        assert_eq!(domains.base_class_id, class, "the one class is also the liveness floor");
        assert_eq!(domains.base_share_permille(), PALW_CLASS_SHARE_DENOMINATOR);
        assert_eq!(domains.class_shares_permille.len(), 1);

        // Folded over a chain where every block is that class: the target never moves. On a
        // one-class network there is no second class to redistribute share with, so a retarget that
        // moved anything would be measuring cadence — which is the other loop's job.
        let steps: Vec<_> = (0..50u64).map(|i| step(i, 1, 1)).collect();
        let folded = fold_class_target_v1(
            params.boot_target,
            &steps,
            domains.base_share_permille(),
            params.retarget_interval_daa,
            params.max_factor,
        )
        .unwrap();
        assert_eq!(folded, params.boot_target);

        // The same chain with blocks NOT attributed to the class does move it — so the no-op above is
        // a consequence of the attribution, not of the fold being inert.
        let foreign: Vec<_> = (0..50u64).map(|i| step(i, 1, 0)).collect();
        assert_ne!(
            fold_class_target_v1(params.boot_target, &foreign, domains.base_share_permille(), params.retarget_interval_daa, 4)
                .unwrap(),
            params.boot_target
        );
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

    // =========================================================================================
    // ADR-0054 — the share-raise path
    // =========================================================================================

    const BASE: u64 = 0xBA5E;
    const ENTRANT: u64 = 0x9E4;
    const OTHER: u64 = 0x07;

    fn shares(rows: &[(u64, u16)]) -> BTreeMap<Hash64, u16> {
        rows.iter().map(|(id, s)| (Hash64::from_u64_word(*id), *s)).collect()
    }

    fn used(rows: &[(u64, u64, u64)]) -> BTreeMap<Hash64, PalwClassEpochUseV1> {
        rows.iter()
            .map(|(id, produced, budget)| (Hash64::from_u64_word(*id), PalwClassEpochUseV1 { produced: *produced, budget: *budget }))
            .collect()
    }

    fn grow(table: &[(u64, u16)], use_rows: &[(u64, u64, u64)], growth: u16, reserve: u16, floor: u16) -> PalwShareGrowthV1 {
        derive_class_share_growth_v1(
            &shares(table),
            &std::collections::BTreeSet::new(),
            &used(use_rows),
            Hash64::from_u64_word(BASE),
            growth,
            reserve,
            floor,
        )
    }

    /// **The path exists at all.** An entrant that filled its budget takes a step of cadence from
    /// the floor — the state a minimum-share class was permanently stuck in before ADR-0054.
    #[test]
    fn a_class_that_filled_its_budget_takes_a_step_from_the_floor() {
        let out = grow(&[(BASE, 999), (ENTRANT, 1)], &[(BASE, 999, 999), (ENTRANT, 1, 1)], 250, 500, 1);
        assert_eq!(out.shares[&Hash64::from_u64_word(ENTRANT)], 2, "1 permille plus max(1, 25% of 1)");
        assert_eq!(out.shares[&Hash64::from_u64_word(BASE)], 998, "and the floor funded exactly that");
        assert_eq!(out.grew[&Hash64::from_u64_word(ENTRANT)], 1);
        assert!(out.decayed.is_empty());
    }

    /// The step is a fraction of the class's OWN share, so growth accelerates as a class earns —
    /// and a large share never moves by a single permille per epoch.
    #[test]
    fn the_step_is_relative_to_the_share_it_grows() {
        let out = grow(&[(BASE, 900), (ENTRANT, 100)], &[(BASE, 900, 900), (ENTRANT, 100, 100)], 250, 500, 1);
        assert_eq!(out.grew[&Hash64::from_u64_word(ENTRANT)], 25, "a quarter of 100 permille");
        assert_eq!(out.shares[&Hash64::from_u64_word(BASE)], 875);
    }

    /// **Producing SOMETHING is not producing everything.** A class under its budget is running at
    /// its natural rate and its share is already right — nothing moves.
    #[test]
    fn a_class_under_its_budget_neither_grows_nor_decays() {
        let out = grow(&[(BASE, 900), (ENTRANT, 100)], &[(BASE, 940, 900), (ENTRANT, 60, 100)], 250, 500, 1);
        assert!(out.grew.is_empty(), "it did not fill its allowance");
        assert!(out.decayed.is_empty(), "and it is not silent either");
        assert_eq!(out.shares[&Hash64::from_u64_word(ENTRANT)], 100);
    }

    /// Silence returns cadence. Without it the floor bleeds one way and a dead class holds a
    /// permille nobody can reclaim — the case ADR-0045 left open.
    #[test]
    fn a_silent_class_gives_its_step_back_to_the_floor() {
        let out = grow(&[(BASE, 900), (ENTRANT, 100)], &[(BASE, 1000, 900)], 250, 500, 1);
        assert_eq!(out.decayed[&Hash64::from_u64_word(ENTRANT)], 25);
        assert_eq!(out.shares[&Hash64::from_u64_word(ENTRANT)], 75);
        assert_eq!(out.shares[&Hash64::from_u64_word(BASE)], 925);
    }

    /// A decayed class keeps its seat: the grant floor is where decay stops, because a share below
    /// it is a zero epoch budget — a class that cannot produce is a frozen class in the wrong
    /// costume.
    #[test]
    fn decay_stops_at_the_grant_floor() {
        let out = grow(&[(BASE, 998), (ENTRANT, 2)], &[(BASE, 998, 998)], 250, 500, 2);
        assert!(out.decayed.is_empty(), "it is already at the floor");
        assert_eq!(out.shares[&Hash64::from_u64_word(ENTRANT)], 2);
    }

    /// **The liveness floor's reserve is a hard bound.** The one class every node can run keeps its
    /// cadence however many classes are hungry — ADR-0039 W6 prime as a quantity, not a status.
    #[test]
    fn growth_stops_at_the_floors_reserve() {
        let out = grow(&[(BASE, 501), (ENTRANT, 499)], &[(BASE, 501, 501), (ENTRANT, 499, 499)], 250, 500, 1);
        assert_eq!(out.grew[&Hash64::from_u64_word(ENTRANT)], 1, "only the one permille above the reserve was available");
        assert_eq!(out.shares[&Hash64::from_u64_word(BASE)], 500);

        let at_reserve = grow(&[(BASE, 500), (ENTRANT, 500)], &[(BASE, 500, 500), (ENTRANT, 500, 500)], 250, 500, 1);
        assert!(at_reserve.grew.is_empty(), "at the reserve the floor funds nothing");
        assert_eq!(at_reserve.shares[&Hash64::from_u64_word(BASE)], 500);
    }

    /// Two classes drawing in one epoch cannot together breach the reserve: the second reads the
    /// balance the first left, not the one the epoch started with.
    #[test]
    fn two_growing_classes_share_one_reserve() {
        let out = grow(
            &[(BASE, 502), (ENTRANT, 249), (OTHER, 249)],
            &[(BASE, 502, 502), (ENTRANT, 249, 249), (OTHER, 249, 249)],
            250,
            500,
            1,
        );
        assert_eq!(out.shares[&Hash64::from_u64_word(BASE)], 500, "the floor stops at its reserve");
        let gained: u32 = out.grew.values().map(|g| u32::from(*g)).sum();
        assert_eq!(gained, 2, "and the two classes took only what was above it");
    }

    /// The denominator is conserved at every mutation — the property ADR-0045 Decision 3 makes a
    /// construction rather than an assertion, held through a rule that moves permille every epoch.
    #[test]
    fn every_move_conserves_the_denominator() {
        for (table, rows) in [
            (vec![(BASE, 999u16), (ENTRANT, 1)], vec![(BASE, 999u64, 999u64), (ENTRANT, 1, 1)]),
            (vec![(BASE, 800), (ENTRANT, 100), (OTHER, 100)], vec![(BASE, 800, 800), (ENTRANT, 100, 100)]),
            (vec![(BASE, 600), (ENTRANT, 200), (OTHER, 200)], vec![(BASE, 1000, 600)]),
        ] {
            let out = grow(&table, &rows, 250, 500, 1);
            assert_eq!(
                out.shares.values().map(|s| u32::from(*s)).sum::<u32>(),
                1000,
                "the table sums to the denominator after growth and decay"
            );
        }
    }

    /// **Off by default.** Every network built before ADR-0054 runs at growth zero, and the rule
    /// must be exactly the identity there — turning a feature on by omission is how a fixture's
    /// meaning changes underneath it.
    #[test]
    fn a_zero_growth_step_moves_nothing() {
        let table = [(BASE, 999u16), (ENTRANT, 1)];
        let out = grow(&table, &[(BASE, 999, 999), (ENTRANT, 1, 1)], 0, 500, 1);
        assert_eq!(out.shares, shares(&table));
        assert!(out.grew.is_empty() && out.decayed.is_empty());
    }

    /// A frozen class is not measured in either direction — ADR-0045's "freeze and unfreeze move no
    /// share", held through a rule that would otherwise decay it while it is unable to produce by
    /// construction.
    #[test]
    fn a_frozen_class_is_left_alone() {
        let frozen: std::collections::BTreeSet<Hash64> = [Hash64::from_u64_word(ENTRANT)].into_iter().collect();
        let out = derive_class_share_growth_v1(
            &shares(&[(BASE, 900), (ENTRANT, 100)]),
            &frozen,
            &used(&[(BASE, 1000, 900)]),
            Hash64::from_u64_word(BASE),
            250,
            500,
            1,
        );
        assert!(out.decayed.is_empty(), "a frozen class produced nothing BY CONSTRUCTION");
        assert_eq!(out.shares[&Hash64::from_u64_word(ENTRANT)], 100);
    }

    /// The floor is the reservoir and never grows on its own account, however much of its budget it
    /// fills — otherwise the one class that always produces would collect the table.
    #[test]
    fn the_floor_never_grows_itself() {
        let out = grow(&[(BASE, 900), (ENTRANT, 100)], &[(BASE, 900, 900), (ENTRANT, 100, 100)], 250, 500, 1);
        assert!(!out.grew.contains_key(&Hash64::from_u64_word(BASE)));
        assert!(out.shares[&Hash64::from_u64_word(BASE)] < 900, "it funded the entrant instead");
    }
}
