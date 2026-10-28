//! **Lane BUDGET (ADR-0176 / ADR-0177): the bond budget in the fold** — dormant behind `Params::palw_bond_budget_v1` (and, for the
//! allocation, `palw_model_bond_allocation_v1`), which no network can arm. A child module of `palw_state_v2`, as the kernel route's and
//! the provider court's folds are, so it reads the builder and writes the state only through the journaled writers: the engine's
//! journal ([`crate::palw_bond_budget_v1::PalwBudgetWriteV1`]) becomes deltas 190 (rows) and 191 (the header).
//!
//! # Where it runs (design `docs/design/palw/economics/bond-budget-and-model-allocation.md` §4)
//!
//! ```text
//! step 1f      tick: create the engine at the fence's first block (seeding the old live claims, §2.9), release every reservation
//!              whose reuse_not_before ≤ now (the ONLY release), roll the allocation epoch, accrue this block's carve
//! tag 140      a capital assignment (table 4)
//! apply_attempt   reserve (Q 1, B its block, R its escrow, F its contribution) before the first write; the claim's immature weight is
//!              held within F; the block's B consumed at once (the block exists). Riders take their share of the lead's block
//! FP commit    reserve (Q 1, B quanta blocks, R quanta carves, F quanta weights)
//! FP spend     strict B and R (the receipt block is paid whole by the coinbase), F clipped
//! finalize     F clipped, R clipped before the buyback/panel split (all legs), the claim closed
//! void         closed (nothing returns to the window)        retire: forgotten (the row leaves once out of the window)
//! weights      every re-derivation of a budgeted claim's Final weight (consistency, reversal, retirement) reads what was granted
//! ```
//!
//! Every hook returns at once below the fence (no mirror, or no engine): nothing is read and nothing is written, so a network that never
//! arms it folds byte for byte as before.

use super::*;
use crate::palw_bond_budget_v1::*;
#[path = "palw_bond_budget_fold_v1/consumer.rs"]
mod consumer;
#[path = "palw_bond_budget_fold_v1/material.rs"]
mod material;
pub(super) use consumer::sweep_paid_consumers_v1;
#[cfg(test)]
#[path = "palw_bond_budget_fold_v1/floor_material_tests.rs"]
mod floor_material_tests;

// BASE native Attempt/Rider allocations have no paid recovery session (the
// native admission and recovery quote enforce this same boundary).
fn native_floor_issuer_v1(params: &PalwStateParamsV2, claim: Option<&PalwClaimStateV2>, origin: PalwBudgetOriginV1) -> bool {
    matches!(origin, PalwBudgetOriginV1::Attempt | PalwBudgetOriginV1::Rider)
        && claim.is_some_and(|claim| matches!(claim.source, PalwClaimSourceV2::Attempt) && claim.class_id == params.base_class_id())
}

/// **RFC-0006 (RFC06-1): the audited write ceiling of a POINT engine op** — one whose rows are named by its arguments (a reservation
/// with its hold shadows, a consume, a close, a forget, one income hold/payment/cancel, one reporter binding). The heaviest audited
/// path, `reserve_income_v1` (reserve with its day/burst shadows 9, consume 2, close 4, forget 5) plus `queue_collected_income_v1`
/// (6), journals 26; 64 is the ceiling. Not a measured value and not consensus: a breach is detected after the op and refuses the
/// transition LOCALLY (`bond_budget_op`), and it binds only while native capacity tracking is on (off on every preset).
pub(super) const PALW_BUDGET_POINT_OP_WRITES_CEILING_V1: u64 = 64;
/// **RFC-0006 (RFC06-1): the audited write ceiling per listed or swept row** — a seeded old claim (a reservation with its shadows, 9),
/// a moved kernel credit (≤ 3), a due release (≤ 3), a model or bond of an epoch roll (≤ 2), an assignment entry (≤ 2): 16.
pub(super) const PALW_BUDGET_WRITES_PER_ROW_V1: u64 = 16;
/// **RFC-0006 (RFC06-1): derived-index nodes a write may add** — the deferred-award set and its per-bond count, the pending-reporter
/// count, and the material root/source/due indexes touched by one income row (at most four of them for one write).
pub(super) const PALW_BUDGET_INDEX_NODES_PER_WRITE_V1: u64 = 4;

/// RFC-0006 (RFC06-1): the rows a material sweep may touch — every material income row of the engine.
fn material_rows_v1(builder: &TransitionBuilder<'_>) -> usize {
    builder.state.bond_budget.as_ref().map_or(0, |b| b.material_income.len())
}

/// **RFC-0006 (RFC06-1): the growth an engine op is admitted for, before it runs** (see `bond_budget_op`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PalwBudgetGrowthV1 {
    /// Journal entries (each at most one new membership, its index nodes, three encodings and one delta slot).
    writes: u64,
    /// Listed or swept rows (each one row of op scratch).
    rows: u64,
    /// Capital-assignment rows the op may write (each `max_models_per_bond` entries of heap, twice).
    assignment_rows: u64,
}

impl PalwBudgetGrowthV1 {
    /// A point op.
    pub(super) const fn point() -> Self {
        Self { writes: PALW_BUDGET_POINT_OP_WRITES_CEILING_V1, rows: 0, assignment_rows: 0 }
    }
    /// An op over `n` listed or swept rows (plus its fixed part).
    pub(super) fn rows(n: usize) -> Self {
        let n = n as u64;
        Self {
            writes: PALW_BUDGET_POINT_OP_WRITES_CEILING_V1.saturating_add(n.saturating_mul(PALW_BUDGET_WRITES_PER_ROW_V1)),
            rows: n,
            assignment_rows: 0,
        }
    }
    /// The op may also write `n` capital-assignment rows.
    pub(super) fn with_assignments(mut self, n: usize) -> Self {
        self.assignment_rows = self.assignment_rows.saturating_add(n as u64);
        self
    }
}

impl PalwChainStateV2 {
    /// UTXO validation and producer preflight use the same first-fence legacy
    /// seeds and clock release as the native fold, before any fee is collected.
    pub fn round_fee_admission_v1(
        &self,
        params: &PalwStateParamsV2,
        ctx: &PalwBlockContextV2,
        escrow_carve: Option<crate::palw_reward_v2::PalwRewardParamsV2>,
    ) -> Option<PalwRoundFeeAdmissionV1> {
        let mirror = params.bond_budget.as_ref().filter(|mirror| mirror.active_at(ctx.daa_score))?;
        let extras = PalwTransitionExtrasV1 { escrow_carve, ..PalwTransitionExtrasV1::default() };
        // `new` is the tests' panicking constructor; a native-resource refusal here is simply no admission view.
        let mut builder = TransitionBuilder::try_new(self, params, false, false, false, false, &extras).ok()?;
        tick_bond_budget_v1(&mut builder, ctx).ok()?;
        Some(PalwRoundFeeAdmissionV1 {
            policy: mirror.policy.clone(),
            book: builder.state.bond_budget.take().expect("active tick creates the budget"),
            capital: self.bonds.iter().map(|(bond, row)| (*bond, row.collateral)).collect(),
            daa_score: ctx.daa_score,
        })
    }

    /// **Lane BUDGET: the bond budget's engine**, once the fence's first block has folded.
    pub fn bond_budget(&self) -> Option<&PalwBondBudgetStateV1> {
        self.bond_budget.as_ref()
    }

    /// **A claim's native income reservation** (codex RFC-8's kind-0 recipient hold, `native_income_id_v1(claim, bond, 0)`): the R its
    /// admission reserved for `bond`, which Final pays out of — a public read for fixtures and observers (R6781-F merge).
    pub fn bond_budget_native_claim_income_v1(
        &self,
        claim: &Hash64,
        bond: &PalwBondKeyV2,
    ) -> Option<&crate::palw_bond_budget_v1::PalwBudgetClaimRowV1> {
        self.bond_budget.as_ref()?.claim_row(&native_income_id_v1(*claim, bond, 0))
    }

    /// The durable R receipt for one actual native queue key and recipient.
    /// Several source/Panel/kernel grants may coalesce here without losing
    /// their aggregate monetary cap or actual coinbase payment clock.
    pub fn bond_budget_native_queue_income_v1(
        &self,
        queue: Hash64,
        bond: &PalwBondKeyV2,
    ) -> Option<&crate::palw_bond_budget_v1::PalwBudgetClaimRowV1> {
        self.bond_budget.as_ref()?.claim_row(&native_queued_income_id_v1(queue, bond))
    }

    /// **Does `bond`'s budget hold its capital at `now_daa`?** — the withdrawal hold (design §2.4): the capital that earned a window cannot
    /// leave, or be re-registered, before the window has passed; and (PESG §6) not while a claim is open or within `H_L` of its terminal
    /// step — the liability hold, a clock of its own. `false` with no engine.
    pub fn bond_budget_window_holds(&self, bond: &PalwBondKeyV2, now_daa: u64) -> bool {
        self.bond_budget.as_ref().is_some_and(|b| b.withdrawal_holds(bond, now_daa))
    }

    /// **What a budgeted claim was granted in Final weight** (hook H-4: rule E reads this, never the full contribution) — `None` for a
    /// claim with no row or a seeded old one.
    pub fn bond_budget_final_weight(&self, claim: &Hash64) -> Option<u128> {
        self.bond_budget.as_ref().and_then(|b| b.final_weight_granted(claim))
    }

    /// **Would the fold's strict budget draw take a receipt spend of `claim_id`?** (hook H-5: the processor asks it of the PARENT state
    /// before admitting a receipt block) — one block and `carve()` left on the claim's reservation; `true` for an unbudgeted claim, and
    /// the carve is not read then.
    pub fn bond_budget_spend_fits_v1(
        &self,
        params: &PalwStateParamsV2,
        claim_id: &Hash64,
        now_daa: u64,
        carve: impl FnOnce() -> u64,
    ) -> bool {
        let Some(budget) = self.bond_budget.as_ref() else { return true };
        let Some(blocks) = budget.remaining(claim_id, PalwBudgetDimV1::BlockUnits) else { return true };
        let carve = carve();
        blocks >= PALW_BUDGET_BLOCK_UNIT_V1 as u128
            && budget.remaining(claim_id, PalwBudgetDimV1::Reward).is_some_and(|reward| reward >= carve as u128)
            && self.bond_budget_fp_export_fits_v1(params, claim_id, now_daa, carve)
    }

    /// **PESG §6: the external export cap on a free-prompt claim's receipt spends** — each spend's worker share is minted straight into
    /// a coinbase output, so while the claim's liability holds (`now < final + H_L`) the spends paid so far plus this one are at most
    /// `⌊0.51 · reserved⌋`; once it has ended, unbounded. `true` for an unbudgeted claim or below the fence.
    pub fn bond_budget_fp_export_fits_v1(&self, params: &PalwStateParamsV2, claim_id: &Hash64, now_daa: u64, carve: u64) -> bool {
        let Some(mirror) = params.bond_budget().filter(|m| m.active_at(now_daa)) else { return true };
        let Some(row) =
            self.bond_budget.as_ref().and_then(|b| b.claim_row(claim_id)).filter(|r| r.origin != PalwBudgetOriginV1::Legacy)
        else {
            return true;
        };
        let Some(claim) = self.claims.get(claim_id) else { return true };
        let PalwClaimPhaseV2::Final { final_daa } = claim.phase else { return true };
        if now_daa >= final_daa.saturating_add(mirror.policy.liability_hold_daa) {
            return true;
        }
        let cap = palw_bond_budget_export_cap_v1(&mirror.policy, claim.reserved) as u128;
        (row.consumed.reward_sompi as u128).saturating_add(carve as u128) <= cap
    }

    /// **PESG §6: no payout before Final, on any leg the state attributes to a claim** — a claim not yet Final (nor voided) has no
    /// producer row in the payout queue, no vesting row (which names its seats' and model-allocation legs), and no execution-lane Final
    /// (the EXEC rights). Checked with the engine's invariants wherever a state is imported; every writer of those rows runs at Final.
    pub fn bond_budget_no_payout_before_final_v1(&self) -> Result<(), String> {
        for (id, claim) in &self.claims {
            if matches!(claim.phase, PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. }) {
                continue;
            }
            if self.pending_payouts.contains_key(id) {
                return Err(format!("claim {id} has a payout before its Final"));
            }
            if self.vesting.contains_key(id) {
                return Err(format!("claim {id} has a vesting row before its Final"));
            }
            if self.round_finals.contains_key(id) {
                return Err(format!("claim {id} holds execution-lane rights before its Final"));
            }
        }
        Ok(())
    }

    /// The caps of `bond` under `policy` at its current locked capital (RPC op 250's numbers).
    pub fn bond_budget_caps_v1(&self, policy: &PalwBondBudgetPolicyV1, bond: &PalwBondKeyV2) -> PalwBudgetVectorV1 {
        palw_bond_budget_caps_v1(policy, self.bonds.get(bond).map(|r| r.collateral).unwrap_or(0))
    }

    /// **A Final claim's weight, as the fold credited it**: the budget's grant where the claim holds a row, else `unbudgeted` (the old
    /// rule's expression, computed by the caller). The one answer every re-derivation reads, so `finalize_claim`, the consistency check,
    /// the reversal and the retirement cannot drift apart.
    pub(super) fn bond_budget_weight_or(&self, claim_id: &Hash64, unbudgeted: u128) -> u128 {
        self.bond_budget_final_weight(claim_id).unwrap_or(unbudgeted)
    }
}

/// The native and kernel claim hash domains remain distinct even if a caller
/// presents equal raw bytes. Neither lane can consume or close the other's row.
pub fn kernel_budget_claim_id_v1(claim: &misaka_palw_kernel::hash::Digest) -> Hash64 {
    let mut hash = keyed(b"misaka-palw/bond-budget/kernel-claim/v1");
    hash.update(claim);
    finish(hash)
}

/// One recipient's funded source promise, independent of carrier replay or
/// income from another market/EXEC/native claim. `source` is the signed immutable
/// source subject (job/seal before reveal), not an as-yet-unknown revealed claim.
pub(super) fn source_income_id_v1(context: Hash64, source: Hash64, bond: &PalwBondKeyV2) -> Hash64 {
    let mut hash = keyed(b"misaka-palw/bond-budget/source-income/v1");
    hash.update(context.as_byte_slice());
    hash.update(source.as_byte_slice());
    hash.update(&borsh::to_vec(bond).expect("a bond serializes"));
    finish(hash)
}

/// Actual recipient holds are independent of the issuer's allocation envelope.
/// Panel and EXEC funds are different rights even when the bond is the same.
pub(super) fn native_income_id_v1(root: Hash64, bond: &PalwBondKeyV2, kind: u8) -> Hash64 {
    let mut hash = keyed(b"misaka-palw/bond-budget/native-income/v1");
    hash.update(root.as_byte_slice());
    hash.update(&borsh::to_vec(bond).expect("a bond serializes"));
    hash.update(&[kind]);
    finish(hash)
}

/// Queue identity is exact, including coalesced Panel/kernel payouts. Moving
/// a finished hold here needs no new schema field and never changes total R.
pub(super) fn native_queued_income_id_v1(queue: Hash64, bond: &PalwBondKeyV2) -> Hash64 {
    let mut hash = keyed(b"misaka-palw/bond-budget/native-queued-income/v1");
    hash.update(queue.as_byte_slice());
    hash.update(&borsh::to_vec(bond).expect("a bond serializes"));
    finish(hash)
}

pub(super) fn native_reporter_binding_id_v1(offence: Hash64) -> Hash64 {
    let mut hash = keyed(b"misaka-palw/bond-budget/native-reporter-binding/v1");
    hash.update(offence.as_byte_slice());
    finish(hash)
}

pub(super) fn legacy_queue_liability_id_v1(queue: Hash64) -> Hash64 {
    let mut hash = keyed(b"misaka-palw/bond-budget/legacy-queue-liability/v1");
    hash.update(queue.as_byte_slice());
    finish(hash)
}

/// A tranche is unique to the immutable award and its already-queued progress.
/// It never coalesces with a different award's physical payment clock.
pub(super) fn reporter_tranche_queue_v1(award: Hash64, progress: u64) -> Hash64 {
    let mut hash = keyed(b"misaka-palw/bond-budget/reporter-tranche/v1");
    hash.update(award.as_byte_slice());
    hash.update(&progress.to_le_bytes());
    let mut bytes = finish(hash).as_bytes();
    bytes[0] = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_PAYOUT_KEY_PREFIX_V1;
    Hash64::from_bytes(bytes)
}

// ---- delta application ------------------------------------------------------------------------------------------------------

/// Delta 190, verify-then-install.
pub(super) fn apply_bond_budget_row_v1(
    state: &mut PalwChainStateV2,
    table: u8,
    key: &[u8],
    old: &Option<Vec<u8>>,
    new: &Option<Vec<u8>>,
    revert: bool,
) -> Result<(), PalwStateV2Error> {
    let budget = state.bond_budget.as_mut().ok_or(PalwStateV2Error::DeltaMismatch("a bond-budget row with no engine"))?;
    budget.apply_row(table, key, old, new, revert).map_err(PalwStateV2Error::DeltaMismatch)
}

/// Delta 191, verify-then-install; `None → Some` creates the engine, `Some → None` (a revert of the creation) drops it — only when it
/// holds no row.
pub(super) fn apply_bond_budget_header_v1(
    state: &mut PalwChainStateV2,
    old: &Option<PalwBondBudgetHeaderV1>,
    new: &Option<PalwBondBudgetHeaderV1>,
    revert: bool,
) -> Result<(), PalwStateV2Error> {
    let (expected, install) = if revert { (new, old) } else { (old, new) };
    if state.bond_budget.as_ref().map(|b| &b.header) != expected.as_ref() {
        return Err(PalwStateV2Error::DeltaMismatch("the bond budget's header does not match the delta's expectation"));
    }
    match (install, state.bond_budget.as_mut()) {
        (Some(header), Some(budget)) => budget.header = header.clone(),
        (Some(header), None) => state.bond_budget = Some(PalwBondBudgetStateV1::from_header(header.clone())),
        (None, Some(budget)) => {
            if !budget.has_no_rows() {
                return Err(PalwStateV2Error::DeltaMismatch("the bond budget is dropped while it still holds rows"));
            }
            state.bond_budget = None;
        }
        (None, None) => {}
    }
    Ok(())
}

// ---- the builder's side ------------------------------------------------------------------------------------------------------

/// **A claim's planned reservation** — computed before the path's first write (so a refusal leaves the builder as it found it) and
/// committed after the claim is written.
#[derive(Clone, Debug)]
pub(super) struct PalwBudgetPlanV1 {
    policy: PalwBondBudgetPolicyV1,
    capital: u64,
    bond: PalwBondKeyV2,
    model: Option<Hash64>,
    accepted_daa: u64,
    ask: PalwBudgetAskV1,
    origin: PalwBudgetOriginV1,
    /// What the engine will reserve (the ask after the per-claim ceilings and the model's budget).
    reservation: PalwBudgetVectorV1,
    /// Block units consumed at once (the claim's own block exists already).
    consume_blocks_now: u64,
    /// ユーザー方針 2026-10-15: the claim's verification mode (Panel=1 day view / Panel=0), fixed at acceptance.
    mode: PalwHoldModeV1,
}

impl PalwBudgetPlanV1 {
    pub(super) fn reservation(&self) -> &PalwBudgetVectorV1 {
        &self.reservation
    }

    /// RFC-0008 RFC08-2's admission terms of the plan: `(bond, capital, accepted_daa, K, reserved R)`.
    pub(super) fn liability_terms_v1(&self) -> (PalwBondKeyV2, u64, u64, u128, u64) {
        (self.bond, self.capital, self.accepted_daa, self.ask.liability_sompi, self.reservation.reward_sompi)
    }

    /// **RFC-0017 (`palw_model_eligibility_v1`, an additive call-site hook — not a change to the engine's rules):** the same plan
    /// with no Final weight asked or reserved, for a claim of a `Probation` model whose Final creates no consensus credit. Every
    /// reader of the claim's F (`bond_budget_final_weight_v1`, `bond_budget_weight_or`, the immature clip) then reads 0
    /// consistently. Only `palw_model_eligibility_fold_v1`'s ticket asks for it, and only past its dormant fence.
    pub(super) fn without_final_weight_v1(mut self) -> Self {
        self.ask.final_weight = 0;
        self.reservation.final_weight = 0;
        self
    }
}

fn budget_refused(bond: PalwBondKeyV2, why: PalwBudgetRefusalV1) -> PalwStateV2Error {
    PalwStateV2Error::BondBudgetExhausted { bond, why: why.to_string() }
}

impl TransitionBuilder<'_> {
    pub(super) fn hold_native_recipient_v1(
        &mut self,
        root: Hash64,
        bond: PalwBondKeyV2,
        kind: u8,
        now: u64,
        until: u64,
        maximum: u64,
        kernel_credits: &[Hash64],
    ) -> Result<(), PalwStateV2Error> {
        if !self.params.bond_budget_active_at(now) || maximum == 0 {
            return Ok(());
        }
        self.hold_native_recipient_row_v1(root, bond, kind, now, until, maximum, kernel_credits)?;
        self.bind_material_income_root_v1(now, root, native_income_id_v1(root, &bond, kind))
    }

    fn hold_native_recipient_row_v1(
        &mut self,
        root: Hash64,
        bond: PalwBondKeyV2,
        kind: u8,
        now: u64,
        until: u64,
        maximum: u64,
        kernel_credits: &[Hash64],
    ) -> Result<(), PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(now) else { return Ok(()) };
        if maximum == 0 {
            return Ok(());
        }
        let capital = self.state.bonds.get(&bond).ok_or(PalwStateV2Error::MissingBond(bond))?.collateral;
        self.bond_budget_op(PalwBudgetGrowthV1::rows(kernel_credits.len()), |budget, _, j| {
            budget.hold_native_income_v1(
                &mirror.policy,
                capital,
                native_income_id_v1(root, &bond, kind),
                bond,
                now,
                until.max(now),
                maximum,
                kernel_credits,
                true,
                j,
            )
        })?
        .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("native income admission has no budget engine".into()))?
        .map_err(|why| budget_refused(bond, why))?;
        Ok(())
    }

    /// Freeze the actual named/valid-reveal winner's full maximum before an
    /// award can become income. This reservation is funded entitlement metadata,
    /// independent of currently available R, so evidence remains permissionless.
    pub(super) fn bind_native_reporter_v1(
        &mut self,
        now: u64,
        offence: Hash64,
        winner: PalwRewardWinnerV1,
        amount: u64,
        accepted: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !self.params.reporter_income_active_at(now) {
            return Ok(());
        }
        let actual = self.state.bonds.get(&winner.reporter).ok_or(PalwStateV2Error::MissingBond(winner.reporter))?;
        if actual.payout_payload != winner.payload {
            return Err(PalwStateV2Error::BondBudgetRefused("native reporter frozen payout differs".into()));
        }
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
            budget.bind_reporter_income_v1(
                native_reporter_binding_id_v1(offence),
                PalwReporterIncomeBindingV1 {
                    recipient: Some(winner.reporter),
                    payload: winner.payload,
                    maximum: amount,
                    accepted_daa: accepted,
                    awarded_daa: None,
                    queue: None,
                    paid_daa: None,
                    historical_boundary: None,
                },
                j,
            )
        })?
        .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("native reporter binding has no budget engine".into()))?
        .map_err(|why| budget_refused(winner.reporter, why))
    }

    /// Close a funded native reporter entitlement into the same tranche engine
    /// used by objective kernel courts. The immutable winner is rooted before
    /// classification; physical cash is credited only with real R/queue room.
    pub(super) fn award_native_reporter_v1(
        &mut self,
        now: u64,
        offence: Hash64,
        winner: PalwRewardWinnerV1,
        amount: u64,
    ) -> Result<bool, PalwStateV2Error> {
        if !self.params.reporter_income_active_at(now) {
            return Ok(false);
        }
        let verdict = self
            .state
            .consumed_offences
            .get(&offence)
            .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("reporter award has no rooted terminal verdict".into()))?;
        if verdict.accepted_daa > now || amount > verdict.collected {
            return Err(PalwStateV2Error::BondBudgetRefused("reporter award precedes verdict or exceeds collected collateral".into()));
        }
        let subject = verdict.claim_id;
        let material = self.params.material_income_active_at(now) && subject != Hash64::default();
        let id = native_reporter_binding_id_v1(offence);
        let policy = self.bond_budget_mirror_at(now).expect("checked active mirror").policy;
        let reporter_from = self.params.reporter_income_from().expect("checked reporter fence");
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
            let binding = budget.reporter_income.get(&id).copied().ok_or(PalwBudgetRefusalV1::NotOpen)?;
            if binding.recipient != Some(winner.reporter)
                || binding.payload != winner.payload
                || binding.maximum != amount
                || binding.awarded_daa.is_some()
                || binding.queue.is_some()
                || binding.paid_daa.is_some()
                || now < binding.accepted_daa
            {
                return Err(PalwBudgetRefusalV1::Assignment("native reporter award differs from its frozen maximum"));
            }
            if binding.accepted_daa < reporter_from {
                // Already accepted native reward rights retain their full cash
                // entitlement, even for a retired recipient or an overfull cap.
                // The actual winner came from rooted best/reveal provenance.
                budget.hold_native_income_v1(
                    &policy,
                    0,
                    native_income_id_v1(offence, &winner.reporter, 3),
                    winner.reporter,
                    binding.accepted_daa,
                    now,
                    amount,
                    &[],
                    false,
                    j,
                )?;
                budget.mark_reporter_awarded_v1(id, now, j)?;
                if material {
                    budget.bind_material_income_v1(subject, native_income_id_v1(offence, &winner.reporter, 3), j)?;
                }
                return Ok(false);
            }
            budget.stage_reporter_income_v1(id, winner.reporter, now, amount, j)?;
            budget.mark_reporter_awarded_v1(id, now, j)?;
            if material {
                budget.bind_material_income_v1(subject, id, j)?;
            }
            Ok(true)
        })?
        .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("native reporter award has no budget engine".into()))?
        .map_err(|why| budget_refused(winner.reporter, why))
    }

    /// The settlement's authenticated bond and immutable funded receipt are
    /// recorded even when the recipient has no R room. Conviction has already
    /// collected this exact award from real collateral; no money is issued here.
    pub(super) fn stage_reporter_recipient_v1(
        &mut self,
        award: Hash64,
        bond: PalwBondKeyV2,
        now: u64,
        amount: u64,
        subject: Hash64,
    ) -> Result<(), PalwStateV2Error> {
        if !self.params.reporter_income_active_at(now) {
            return Ok(());
        }
        if amount == 0 {
            return Ok(());
        }
        let payload = self.state.bonds.get(&bond).ok_or(PalwStateV2Error::MissingBond(bond))?.payout_payload;
        let material = self.params.material_income_active_at(now);
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
            // Preflight the existing immutable receipt before changing either
            // rooted table. Its payee is frozen now, never looked up by address.
            if budget.claim_row(&award).is_some_and(|row| {
                row.origin != PalwBudgetOriginV1::DeferredReporterIncome
                    || row.bond != bond
                    || row.accepted_daa != now
                    || row.reserved.reward_sompi != amount
            }) {
                return Err(PalwBudgetRefusalV1::Duplicate);
            }
            budget.bind_reporter_income_v1(
                award,
                PalwReporterIncomeBindingV1 {
                    recipient: Some(bond),
                    payload,
                    maximum: amount,
                    accepted_daa: now,
                    awarded_daa: Some(now),
                    queue: None,
                    paid_daa: None,
                    historical_boundary: None,
                },
                j,
            )?;
            budget.stage_reporter_income_v1(award, bond, now, amount, j)?;
            if material {
                budget.bind_material_income_v1(subject, award, j)?;
            }
            Ok(())
        })?
        .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("reporter award has no budget engine".into()))?
        .map_err(|why| budget_refused(bond, why))
    }

    pub(super) fn pay_native_recipient_v1(
        &mut self,
        root: Hash64,
        bond: PalwBondKeyV2,
        kind: u8,
        now: u64,
        amount: u64,
        queue_key: Hash64,
        finished: bool,
    ) -> Result<(), PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(now) else { return Ok(()) };
        if amount == 0 {
            if finished {
                self.cancel_native_recipient_v1(root, bond, kind)?;
            }
            return Ok(());
        }
        if kind == 0 && self.state.claim(&root).is_some_and(|claim| claim.bond == bond) {
            self.bind_issuer_material_income_root_v1(now, root)?;
        } else if kind == 1 {
            self.bind_selected_panel_material_income_root_v1(now, root, bond)?;
        } else {
            self.bind_material_income_root_v1(now, root, native_income_id_v1(root, &bond, kind))?;
        }
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
            budget.pay_native_income_v1(
                &mirror.policy,
                &native_income_id_v1(root, &bond, kind),
                now,
                amount,
                native_queued_income_id_v1(queue_key, &bond),
                finished,
                j,
            )
        })?
        .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("native payment has no budget engine".into()))?
        .map_err(|why| budget_refused(bond, why))
    }

    pub(super) fn cancel_native_recipient_v1(&mut self, root: Hash64, bond: PalwBondKeyV2, kind: u8) -> Result<(), PalwStateV2Error> {
        let id = native_income_id_v1(root, &bond, kind);
        if self
            .state
            .bond_budget
            .as_ref()
            .and_then(|b| b.claim_row(&id))
            .is_some_and(|r| r.live && matches!(r.origin, PalwBudgetOriginV1::NativeIncome | PalwBudgetOriginV1::NativeIncomePartial))
        {
            self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| budget.cancel_native_income_v1(&id, j))?
                .expect("checked engine")
                .expect("native hold");
        }
        Ok(())
    }

    /// Panel admission reserves every actual seat before its receipt can earn
    /// money. Flat Panels use their fixed per-seat maximum; shard Panels use
    /// the whole pool as a conservative maximum until their fixed shares are
    /// available (a bond may occupy several strata).
    pub(super) fn hold_panel_recipients_v1(
        &mut self,
        root: Hash64,
        seats: &[PalwPanelSeatV2],
        now: u64,
    ) -> Result<(), PalwStateV2Error> {
        if self.bond_budget_mirror_at(now).is_none() {
            return Ok(());
        }
        let claim = self.state.claims.get(&root).ok_or(PalwStateV2Error::MissingClaim(root))?.clone();
        if claim.escrowed_reward == 0 {
            return Ok(());
        }
        let bonds: BTreeSet<_> = seats.iter().map(|seat| seat.bond).collect();
        let permille = self
            .state
            .claim_economics
            .get(&root)
            .map_or(crate::palw_panel_economy_v1::PALW_PANEL_POOL_PERMILLE_V1 as u16, |e| e.panel_share_permille);
        let pool = (u128::from(claim.escrowed_reward) * u128::from(permille) / 1000) as u64;
        let shard = self.state.tir_shard_claims.contains_key(&root)
            || self.state.panel_v3.as_ref().and_then(|engine| engine.claim(&root)).is_some_and(|record| record.strata.is_some());
        let maximum = if shard { pool } else { pool / bonds.len().max(1) as u64 };
        let until = super::palw_exec_v2_rewards_v1::horizon(&self.state, self.params, &root)?.unwrap_or(claim.trace_retention_daa);
        let floor_issuer = self.params.material_income_active_at(now)
            && self
                .state
                .bond_budget
                .as_ref()
                .and_then(|book| book.claim_row(&root))
                .is_some_and(|row| row.bond == claim.bond && native_floor_issuer_v1(self.params, Some(&claim), row.origin));
        if floor_issuer && self.state.panels.get(&root).is_none_or(|panel| panel.seats.as_slice() != seats) {
            return Err(PalwStateV2Error::BondBudgetRefused("floor Panel income requires the exact stored selected seats".into()));
        }
        let checkpoint = self.try_checkpoint()?;
        for bond in bonds {
            let held = if floor_issuer && maximum > 0 {
                self.hold_native_recipient_row_v1(root, bond, 1, now, until, maximum, &[])
                    .and_then(|()| self.bind_selected_panel_material_income_root_v1(now, root, bond))
            } else {
                self.hold_native_recipient_v1(root, bond, 1, now, until, maximum, &[])
            };
            if let Err(error) = held {
                self.restore(checkpoint);
                return Err(error);
            }
        }
        Ok(())
    }

    pub(super) fn cancel_panel_recipients_v1(&mut self, root: Hash64) -> Result<(), PalwStateV2Error> {
        let bonds: Vec<_> =
            self.state.panels.get(&root).into_iter().flat_map(|panel| panel.seats.iter().map(|seat| seat.bond)).collect();
        for bond in bonds {
            self.cancel_native_recipient_v1(root, bond, 1)?;
        }
        Ok(())
    }

    /// An executor's full native work share is bounded by the funded original
    /// escrow. Kernel fee maxima are atomically reclassified, not added twice.
    pub(super) fn hold_exec_recipient_v1(
        &mut self,
        root_id: Hash64,
        bond: PalwBondKeyV2,
        evidence: Hash64,
        work: u64,
        now: u64,
    ) -> Result<(), PalwStateV2Error> {
        if self.bond_budget_mirror_at(now).is_none() {
            return Ok(());
        }
        let root = self.state.exec_v2.roots.get(&root_id).ok_or(PalwStateV2Error::MissingClaim(root_id))?;
        let claim = self.state.claims.get(&root_id).ok_or(PalwStateV2Error::MissingClaim(root_id))?;
        // The root executor already owns the single issuer allocation hold.
        if root.root_bond == bond {
            return Ok(());
        }
        let prior = self
            .state
            .exec_v2
            .slices
            .range((root_id, 0)..=(root_id, u32::MAX))
            .filter(|(_, slice)| slice.executor == bond)
            .try_fold(0u64, |sum, (_, slice)| sum.checked_add(slice.range.work()?))
            .ok_or(PalwStateV2Error::Overflow("EXEC recipient work"))?;
        let total = prior.checked_add(work).ok_or(PalwStateV2Error::Overflow("EXEC recipient work"))?;
        let allocation = self
            .state
            .bond_budget
            .as_ref()
            .and_then(|b| b.claim_row(&native_income_id_v1(root_id, &root.root_bond, 0)))
            .map_or(0, |row| row.reserved.reward_sompi.min(claim.escrowed_reward));
        let maximum = (u128::from(allocation) * u128::from(total) / u128::from(root.total_work)) as u64;
        if maximum == 0 {
            return Ok(());
        }
        let until = super::palw_exec_v2_rewards_v1::horizon(&self.state, self.params, &root_id)?.expect("existing root");
        let standalone = native_income_id_v1(evidence, &bond, 3);
        let budget = self.state.bond_budget.as_ref();
        let credit = if budget.and_then(|b| b.claim_row(&standalone)).is_some_and(|r| r.live && r.consumed.reward_sompi == 0) {
            standalone
        } else {
            kernel_budget_claim_id_v1(&evidence.as_bytes())
        };
        let credits = budget
            .and_then(|b| b.claim_row(&credit))
            .filter(|r| {
                r.bond == bond
                    && r.in_window
                    && r.live
                    && r.consumed.reward_sompi == 0
                    && matches!(
                        r.origin,
                        PalwBudgetOriginV1::KernelRoute | PalwBudgetOriginV1::Legacy | PalwBudgetOriginV1::NativeIncome
                    )
            })
            .map(|_| vec![credit])
            .unwrap_or_default();
        self.hold_native_recipient_v1(root_id, bond, 2, now, until, maximum, &credits)
    }

    /// Keep the original allocation ceiling for grant calculation while its
    /// monetary R is reclassified into the issuer's unpaid income hold. This
    /// creates no additional R and leaves Q/B/F/T in the original claim row.
    fn hold_issuer_income_v1(&mut self, ctx: &PalwBlockContextV2, claim_id: Hash64) -> Result<(), PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return Ok(()) };
        let Some(row) = self.state.bond_budget.as_ref().and_then(|b| b.claim_row(&claim_id)).copied() else { return Ok(()) };
        if !matches!(row.origin, PalwBudgetOriginV1::Attempt | PalwBudgetOriginV1::Rider | PalwBudgetOriginV1::Legacy)
            || row.reserved.reward_sompi == 0
        {
            return Ok(());
        }
        let capital = self.state.bonds.get(&row.bond).ok_or(PalwStateV2Error::MissingBond(row.bond))?.collateral;
        self.bond_budget_op(PalwBudgetGrowthV1::rows(1), |budget, _, j| {
            budget.hold_native_income_v1(
                &mirror.policy,
                capital,
                native_income_id_v1(claim_id, &row.bond, 0),
                row.bond,
                row.accepted_daa,
                ctx.daa_score,
                row.reserved.reward_sompi,
                &[claim_id],
                false,
                j,
            )
        })?
        .expect("claim engine")
        .map_err(|why| budget_refused(row.bond, why))?;
        self.bind_issuer_material_income_root_v1(ctx.daa_score, claim_id)
    }

    pub(super) fn reserve_round_income_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        income: PalwRoundFeeIncomeV1,
    ) -> Result<(), PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return Ok(()) };
        if income.maximum_sompi == 0 && income.actual_sompi == 0 {
            return Ok(());
        }
        let capital = self.state.bond(&income.bond).ok_or(PalwStateV2Error::MissingBond(income.bond))?.collateral;
        if self.state.bond_budget.is_none() {
            self.create_bond_budget_v1(ctx, &mirror)?;
        }
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
            budget.reserve_income_v1(
                &mirror.policy,
                capital,
                palw_round_income_id_v1(income.block, &income.bond),
                income.bond,
                ctx.daa_score,
                income.maximum_sompi,
                income.actual_sompi,
                PalwBudgetOriginV1::RoundFee,
                j,
            )
        })?
        .expect("created")
        .map_err(|why| budget_refused(income.bond, why))
    }

    /// The engine's journal, as deltas. The delta slots were admitted with the op (`bond_budget_op`); the row bytes MOVE into the
    /// entries (no copy).
    fn journal_bond_budget(&mut self, writes: Vec<PalwBudgetWriteV1>) {
        for write in writes {
            match write {
                PalwBudgetWriteV1::Header { old, new } => self.entries.push(PalwDeltaEntryV2::BondBudgetHeader { old, new }),
                PalwBudgetWriteV1::Row { table, key, old, new } => {
                    self.entries.push(PalwDeltaEntryV2::BondBudgetRow { table, key, old, new })
                }
            }
        }
    }

    /// **RFC-0006 (RFC06-1): admit an engine op's growth before it runs** — `growth.writes` journal entries, each at most one new
    /// membership of the widest engine table, [`PALW_BUDGET_INDEX_NODES_PER_WRITE_V1`] derived-index nodes and three canonical
    /// encodings (pinned Borsh: at least 1,024 bytes each, at most twice the widest row — an assignment row of
    /// `max_models_per_bond` entries), its delta slot and the op's own journal Vec; plus `growth.rows` rows of op scratch (the
    /// collected due/credit lists) and the assignment heap the op names. A no-op with tracking off (every preset).
    fn admit_bond_budget_growth_v1(&mut self, growth: PalwBudgetGrowthV1) -> Result<(), PalwStateV2Error> {
        use crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1 as Node;
        use std::mem::size_of;
        if !self.native_writers_tracking_v1() {
            return Ok(());
        }
        // An assignment row's width is the ALLOCATION policy's (ADR-0177); without that fence the widest a u16 count admits.
        let max_models = self
            .params
            .bond_budget()
            .and_then(|mirror| mirror.allocation.as_ref())
            .map_or(u64::from(u16::MAX), |(_, policy)| u64::from(policy.max_models_per_bond));
        let entry = size_of::<(Hash64, u64)>() as u64;
        let assignment_row = max_models.saturating_mul(2).saturating_mul(entry).saturating_add(64);
        let encoding = assignment_row.saturating_mul(2).max(1024);
        let widest_row = [
            size_of::<Node<Hash64, PalwBudgetClaimRowV1>>(),
            size_of::<Node<PalwBondKeyV2, PalwBudgetBondRowV1>>(),
            size_of::<Node<PalwBondKeyV2, PalwCapitalAssignmentRowV1>>(),
            size_of::<Node<Hash64, PalwModelBudgetRowV1>>(),
            size_of::<Node<Hash64, PalwReporterIncomeBindingV1>>(),
            size_of::<Node<Hash64, PalwMaterialIncomeV1>>(),
            size_of::<Node<Hash64, PalwPaidConsumerContractV1>>(),
            size_of::<Node<(u64, Hash64), ()>>(),
            size_of::<Node<(u64, PalwBondKeyV2), ()>>(),
        ]
        .into_iter()
        .max()
        .unwrap_or(0) as u64;
        let widest_index = [
            size_of::<Node<Hash64, ()>>(),
            size_of::<Node<(u64, Hash64), ()>>(),
            size_of::<Node<PalwBondKeyV2, usize>>(),
            size_of::<Node<Option<Hash64>, BTreeSet<Hash64>>>(),
        ]
        .into_iter()
        .max()
        .unwrap_or(0) as u64;
        let per_write = widest_row
            .saturating_add(widest_index.saturating_mul(PALW_BUDGET_INDEX_NODES_PER_WRITE_V1))
            .saturating_add(encoding.saturating_mul(3));
        let scratch = (size_of::<(Hash64, PalwBudgetClaimRowV1)>() as u64).saturating_add(widest_index).saturating_add(widest_row);
        let mut q = super::native_writers::NativeWriteQuoteV1::new("bond budget engine op");
        q.grown::<PalwBudgetWriteV1>(growth.writes)?
            .exact::<u8>(growth.writes.saturating_mul(per_write))?
            .exact::<u8>(growth.rows.saturating_mul(scratch))?
            .exact::<u8>(growth.assignment_rows.saturating_mul(assignment_row))?
            .slots(usize::try_from(growth.writes).unwrap_or(usize::MAX))?;
        self.admit_native_write_v1(q)
    }

    /// **Run `f` on the engine** with the rest of the state readable and a journal, then journal it. `Ok(None)` with no engine.
    ///
    /// **RFC-0006 (RFC06-1):** the op's growth is admitted BEFORE it runs ([`Self::admit_bond_budget_growth_v1`]) from the caller's
    /// typed bound (`growth`: a point op's audited ceiling, plus the rows a list or sweep op may touch). After it runs the journal
    /// is checked against the admitted writes: a breach is a local `NativeResource` refusal of the transition (fail closed, never a
    /// skipped write) — it means a bound below is wrong, and the transition is retried, never judged. With tracking off nothing
    /// is admitted or checked: the historical op, byte for byte.
    pub(super) fn bond_budget_op<R>(
        &mut self,
        growth: PalwBudgetGrowthV1,
        f: impl FnOnce(&mut PalwBondBudgetStateV1, &PalwChainStateV2, &mut Vec<PalwBudgetWriteV1>) -> R,
    ) -> Result<Option<R>, PalwStateV2Error> {
        if self.state.bond_budget.is_none() {
            return Ok(None);
        }
        self.admit_bond_budget_growth_v1(growth)?;
        let mut budget = self.state.bond_budget.take().expect("checked above");
        let mut journal = Vec::new();
        let out = f(&mut budget, &self.state, &mut journal);
        self.state.bond_budget = Some(budget);
        if self.native_writers_tracking_v1() && journal.len() as u64 > growth.writes {
            return Err(super::native_writers::native_err_v1(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                PalwBudgetWriteV1,
            >(
                "bond budget engine op",
                "the op journaled more writes than its admitted bound",
            )));
        }
        self.journal_bond_budget(journal);
        Ok(Some(out))
    }

    /// The fence's mirror, if the budget is in force at `daa`.
    pub(super) fn bond_budget_mirror_at(&self, daa: u64) -> Option<PalwBondBudgetMirrorV1> {
        self.params.bond_budget().filter(|mirror| mirror.active_at(daa)).cloned()
    }

    /// The model a claim of `class` on artifact `root` allocates against (design §3.7): the line that owns the root (the immutable
    /// registration under ADR-0175), else the class (its founding line's id).
    fn bond_budget_model_of(&self, class: &Hash64, root: Option<&Hash64>) -> Hash64 {
        root.and_then(|root| self.state.artifact_line_of_root(class, root, self.extras.artifact_root_ownership_active))
            .unwrap_or(*class)
    }

    /// **Plan a claim's reservation** (design §2.5): `None` below the fence (the claim is unbudgeted); `Err` when the bond's window or
    /// open-claim cap refuses it — before any write.
    pub(super) fn plan_bond_budget_v1(
        &self,
        ctx: &PalwBlockContextV2,
        bond: PalwBondKeyV2,
        model: Option<Hash64>,
        ask: PalwBudgetAskV1,
        origin: PalwBudgetOriginV1,
        consume_blocks_now: u64,
    ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
        self.plan_bond_budget_moded_v1(ctx, bond, model, ask, origin, consume_blocks_now, PalwHoldModeV1::for_origin(origin))
    }

    /// [`Self::plan_bond_budget_v1`] with the claim's verification mode (ユーザー方針 2026-10-15): a Panel=1 claim is also checked
    /// against its bond's day view; every claim against the burst view and the concurrent-liability cap (V3 policies only).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn plan_bond_budget_moded_v1(
        &self,
        ctx: &PalwBlockContextV2,
        bond: PalwBondKeyV2,
        model: Option<Hash64>,
        ask: PalwBudgetAskV1,
        origin: PalwBudgetOriginV1,
        consume_blocks_now: u64,
        mode: PalwHoldModeV1,
    ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return Ok(None) };
        let capital = self.state.bonds.get(&bond).map(|r| r.collateral).unwrap_or(0);
        // RFC-0016 P4 (user decision 2026-10-15): a user-paid escrow claim (the kernel route) is never clipped by, and never
        // reserves against, the coinbase-derived model-allocation budget; its whole reward is reserved inside the producer's
        // common R cap or the claim is refused here, before acceptance.
        let model = model
            .filter(|_| mirror.allocation_at(ctx.daa_score).is_some())
            .filter(|_| !crate::palw_bond_budget_v1::palw_budget_origin_is_user_escrow_v1(origin));
        // The engine is created by the tick at the fence's first block, so it exists here; a fixture that skipped the tick plans
        // against an empty engine (the same answers).
        let empty;
        let budget = match self.state.bond_budget.as_ref() {
            Some(budget) => budget,
            None => {
                empty = PalwBondBudgetStateV1::new(ctx.daa_score, &mirror.policy);
                &empty
            }
        };
        let reservation = budget
            .preview_claim_for_origin_moded_v1(&mirror.policy, capital, bond, model, ask, origin, mode)
            .map_err(|why| budget_refused(bond, why))?;
        if consume_blocks_now > reservation.block_units {
            return Err(budget_refused(bond, PalwBudgetRefusalV1::Short { dim: PalwBudgetDimV1::BlockUnits }));
        }
        // ADR-0177 (2026-10-14): P_m sets the model's reward-bearing block opportunity too (a rider rides its lead's block, which the
        // lead's acceptance counted). Refused, never clipped: a block opportunity is a whole block.
        if let (Some(m), Some(alloc)) = (model, mirror.allocation_at(ctx.daa_score))
            && alloc.average.is_some_and(|a| a.block_opportunity)
            && origin != PalwBudgetOriginV1::Rider
            // Allocation bootstrap (option (b)): an empty allocation limits no block opportunity.
            && !budget.allocation_bootstrapping_v1()
            && reservation.block_units > budget.model_blocks_available_v1(&m, ctx.daa_score)
        {
            return Err(budget_refused(bond, PalwBudgetRefusalV1::ModelBlockOpportunity));
        }
        Ok(Some(PalwBudgetPlanV1 {
            policy: mirror.policy,
            capital,
            bond,
            model,
            accepted_daa: ctx.daa_score,
            ask,
            origin,
            reservation,
            consume_blocks_now,
            mode,
        }))
    }

    /// **Commit a planned reservation** for `claim_id` (after the claim is written) and consume the claim's own block now.
    pub(super) fn commit_bond_budget_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim_id: Hash64,
        plan: PalwBudgetPlanV1,
    ) -> Result<(), PalwStateV2Error> {
        if plan.origin != PalwBudgetOriginV1::KernelRoute
            && plan.reservation.reward_sompi > 0
            && !self
                .state
                .claim(&claim_id)
                .is_some_and(|claim| claim.bond == plan.bond && native_floor_issuer_v1(self.params, Some(claim), plan.origin))
        {
            self.state.require_monetary_material_enrollment_v1(self.params, claim_id, ctx.daa_score, None)?;
        }
        if self.state.bond_budget.is_none() {
            let mirror = self.bond_budget_mirror_at(ctx.daa_score).expect("a plan exists only past the fence");
            self.create_bond_budget_v1(ctx, &mirror)?;
        }
        let bond = plan.bond;
        let block_opportunity = self
            .bond_budget_mirror_at(ctx.daa_score)
            .and_then(|mirror| mirror.allocation_at(ctx.daa_score).and_then(|alloc| alloc.average))
            .is_some_and(|a| a.block_opportunity);
        let out = self
            .bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
                // Allocation bootstrap (option (b)): an empty allocation reserves no block opportunity either.
                let bootstrapping = budget.allocation_bootstrapping_v1();
                let reserved = budget.reserve_claim_moded_v1(
                    &plan.policy,
                    plan.capital,
                    claim_id,
                    plan.bond,
                    plan.model,
                    plan.accepted_daa,
                    plan.ask,
                    plan.origin,
                    plan.mode,
                    j,
                )?;
                debug_assert_eq!(reserved, plan.reservation, "the plan and the commit read one engine state");
                // ADR-0177 (2026-10-14): the model's block opportunity is taken at acceptance (never returned early).
                if let Some(m) = plan.model
                    && plan.origin != PalwBudgetOriginV1::Rider
                    && block_opportunity
                    && !bootstrapping
                {
                    budget.reserve_model_blocks_v1(&m, reserved.block_units, j);
                }
                if plan.consume_blocks_now > 0 {
                    budget
                        .consume(&claim_id, PalwBudgetDimV1::BlockUnits, plan.consume_blocks_now as u128, true, j)
                        .expect("a fresh row")?;
                }
                Ok::<(), PalwBudgetRefusalV1>(())
            })?
            .expect("created above");
        // The plan read this same engine state, so the commit cannot refuse; were it to, the block fails (never a skip: the claim's other
        // rows are written by now).
        out.map_err(|why| {
            PalwStateV2Error::BondBudgetRefused(format!("bond {bond:?}: the planned reservation could not be committed: {why}"))
        })?;
        if matches!(plan.origin, PalwBudgetOriginV1::Attempt | PalwBudgetOriginV1::Rider) {
            self.hold_issuer_income_v1(ctx, claim_id)?;
        }
        Ok(())
    }

    /// Create the engine (delta 191, `None → Some`) and seed the window from every live old claim (design §2.9).
    fn create_bond_budget_v1(&mut self, ctx: &PalwBlockContextV2, mirror: &PalwBondBudgetMirrorV1) -> Result<(), PalwStateV2Error> {
        let budget = PalwBondBudgetStateV1::new(ctx.daa_score, &mirror.policy);
        self.entries.push(PalwDeltaEntryV2::BondBudgetHeader { old: None, new: Some(budget.header.clone()) });
        self.state.bond_budget = Some(budget);
        // Old live claims count against their bonds' new windows until their own `accepted + W`, never consumed (old rules pay them).
        let window = mirror.policy.window_daa;
        let carve = self.params.worker_carve_at(ctx.subsidy, self.extras.escrow_carve);
        let pricing = self.fp_pricing();
        let mut seeds: Vec<(Hash64, PalwBondKeyV2, u64, PalwBudgetVectorV1, u128)> = Vec::new();
        let mut source_seeds: Vec<(Hash64, PalwBondKeyV2, u64, u64, u64, Option<(u64, Option<Hash64>)>)> = Vec::new();
        for (id, claim) in &self.state.claims {
            if claim.accepted_daa.saturating_add(window) <= ctx.daa_score && (claim.phase.is_terminal() || claim.escrowed_reward == 0)
            {
                continue;
            }
            let vector = match (&claim.source, &claim.phase) {
                (_, PalwClaimPhaseV2::Voided { .. }) => continue,
                (PalwClaimSourceV2::Attempt, PalwClaimPhaseV2::Final { .. }) => continue,
                (PalwClaimSourceV2::Attempt, _) => {
                    let canonical = self.canonical_claim_weight(claim);
                    let contribution =
                        palw_claim_safe_contribution_v3(&self.state.class_shares, claim, self.uncertified_weightless, canonical);
                    PalwBudgetVectorV1 {
                        claims: 1,
                        block_units: PALW_BUDGET_BLOCK_UNIT_V1,
                        reward_sompi: claim.escrowed_reward,
                        final_weight: crate::palw_weight_cap_v1::palw_weight_final_safe_v1(self.params, claim, contribution),
                        round_rights: 0,
                    }
                }
                (PalwClaimSourceV2::FreePrompt { quanta, spent }, _) => {
                    let left = (*quanta as u64).saturating_sub(spent.len() as u64);
                    if left == 0 {
                        continue;
                    }
                    let per_quantum = palw_fp_spend_weight_v1(&self.state, claim, *quanta, &pricing);
                    PalwBudgetVectorV1 {
                        claims: 1,
                        block_units: PALW_BUDGET_BLOCK_UNIT_V1.saturating_mul(left),
                        reward_sompi: carve.saturating_mul(left),
                        final_weight: per_quantum.saturating_mul(left as u128),
                        round_rights: 0,
                    }
                }
            };
            let vector = if claim.accepted_daa.saturating_add(window) <= ctx.daa_score {
                PalwBudgetVectorV1 { claims: 1, reward_sompi: vector.reward_sompi, ..PalwBudgetVectorV1::ZERO }
            } else {
                vector
            };
            // PESG §6: an open old claim's liability collateral counts too (a Final one's FP spends hold none of their own).
            let liability = if matches!(claim.phase, PalwClaimPhaseV2::Final { .. }) { 0 } else { claim.reserved };
            seeds.push((*id, claim.bond, claim.accepted_daa, vector, liability));
        }
        // Old kernel work is also counted at the boundary, without changing
        // its already signed economics. No old budget is silently minted anew.
        if let Some(route) = &self.state.kernel_route {
            let ledger = route.ledger().map_err(PalwStateV2Error::BondBudgetRefused)?;
            {
                for (claim, row) in &ledger.claims {
                    if row.convicted || row.life.state.is_terminal() {
                        continue;
                    }
                    let Some(bond) = route.bond_key_of(&row.producer) else { continue };
                    seeds.push((
                        kernel_budget_claim_id_v1(claim),
                        bond,
                        row.committed_daa,
                        PalwBudgetVectorV1 {
                            claims: 1,
                            // Accepted EXEC work already owns one native allocation/share
                            // hold seeded below. Its Q row creates no second R.
                            reward_sompi: if self.state.exec_v2_root_of_kernel_claim_v1(&Hash64::from_bytes(*claim)).is_some() {
                                0
                            } else {
                                ledger.job_escrows.get(&row.job_id).map_or(0, |escrow| escrow.amount)
                            },
                            ..PalwBudgetVectorV1::ZERO
                        },
                        // PESG §6: a kernel claim reserves no V2 liability collateral of its own here.
                        0,
                    ));
                }
                // A fee promise is not a computation claim. Pending promises
                // remain liabilities past accepted+W and even H+W on a sparse
                // chain. Recent actual payment uses its payment+W clock.
                use misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeOutcomeV1;
                for ((context, producer), promise) in &ledger.beacon_source_fee_promises {
                    let paid = match promise.outcome {
                        None => None,
                        Some(BeaconSourceFeeOutcomeV1::Refunded { .. }) => continue,
                        Some(BeaconSourceFeeOutcomeV1::Paid { daa, .. }) => Some(daa),
                    };
                    let source = ledger
                        .beacon_sources
                        .get(&(*context, *producer))
                        .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("migrating fee source is missing".into()))?;
                    let bond = match route.bond_key_of(producer) {
                        Some(bond) => bond,
                        // A historical paid recipient whose immutable bond no
                        // longer exists cannot accept new R under that key.
                        None if paid.is_some() => continue,
                        None => {
                            return Err(PalwStateV2Error::BondBudgetRefused("migrating pending fee recipient bond is missing".into()));
                        }
                    };
                    let paid = match paid {
                        Some(daa) => {
                            let payload = self.state.bonds.get(&bond).map(|row| row.payout_payload);
                            let queue = payload.map(|p| crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&p));
                            let queued = queue.filter(|key| {
                                daa == ledger.daa
                                    && (self.state.pending_payouts.get(key).is_some_and(|row| row.amount >= promise.fee)
                                        || self.entries.iter().any(|entry| {
                                            matches!(entry,
                                                PalwDeltaEntryV2::Payout { key: drained, old: Some(row), new: None }
                                                    if drained == key && row.amount >= promise.fee
                                            )
                                        }))
                            });
                            Some((daa, queued))
                        }
                        None => None,
                    };
                    let subject = super::palw_kernel_route_fold_v1::source_income_subject_v1(&source.job, &source.seal);
                    source_seeds.push((
                        source_income_id_v1(Hash64::from_bytes(*context), subject, &bond),
                        bond,
                        promise.accepted_daa,
                        source.terms.hold_until,
                        promise.fee,
                        paid,
                    ));
                }
            }
        }
        let policy = mirror.policy.clone();
        // RFC-0006 (RFC06-1): every seeded old claim and old source promise is a listed row of the op.
        let seed_rows = seeds.len().saturating_add(source_seeds.len());
        self.bond_budget_op(PalwBudgetGrowthV1::rows(seed_rows), |budget, state, j| {
            for (id, bond, accepted, vector, liability) in seeds {
                let capital = state.bonds.get(&bond).map(|r| r.collateral).unwrap_or(0);
                // Signed old rights are counted even if the first window is over-full (codex: a refusal is an error, never
                // dropped); PESG §6's open liability collateral rides with each seed (integration).
                budget.reserve_liable(
                    &policy,
                    capital,
                    id,
                    bond,
                    None,
                    accepted,
                    vector,
                    liability,
                    PalwBudgetOriginV1::Legacy,
                    false,
                    j,
                )?;
            }
            for (id, bond, accepted, hold_until, fee, paid) in source_seeds {
                let capital = state.bonds.get(&bond).map(|r| r.collateral).unwrap_or(0);
                budget.seed_source_income_v1(&policy, capital, id, bond, accepted, hold_until, fee, j)?;
                if let Some((at, queue)) = paid {
                    if let Some(queue) = queue {
                        budget.queue_source_income_v1(
                            &policy,
                            &id,
                            hold_until,
                            at,
                            fee,
                            native_queued_income_id_v1(queue, &bond),
                            j,
                        )?;
                    } else {
                        // Old promises record logical Paid, not the physical
                        // coinbase DAA. Count existing mapped income once until
                        // first-fence + W as a conservative boundary hold;
                        // this is not a claim that money was paid at the fence.
                        budget.settle_source_income_v1(&policy, &id, hold_until, ctx.daa_score, fee, j)?;
                    }
                }
            }
            Ok(())
        })?
        .expect("created above")
        .map_err(|why: PalwBudgetRefusalV1| PalwStateV2Error::BondBudgetRefused(why.to_string()))?;
        let pending: Vec<_> = match self
            .state
            .kernel_route
            .as_ref()
            .map(|route| route.ledger())
            .transpose()
            .map_err(PalwStateV2Error::BondBudgetRefused)?
        {
            Some(ledger) => ledger
                .claims
                .iter()
                .filter(|(_, row)| !row.convicted && !row.life.state.is_terminal())
                .map(|(claim, row)| (Hash64::from_bytes(*claim), row.liability_until.unwrap_or(ctx.daa_score)))
                .collect(),
            None => Vec::new(),
        };
        for (claim, until) in pending {
            self.hold_kernel_income_v1(ctx, claim, until)?;
        }
        self.seed_native_income_rights_v1(ctx, mirror)
    }

    fn initialize_reporter_income_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        mirror: &PalwBondBudgetMirrorV1,
    ) -> Result<(), PalwStateV2Error> {
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| budget.activate_reporter_income_v2(j))?
            .expect("reporter fence requires budget");
        // Pending old winners retain authenticated bond provenance in their
        // rooted best record. Old awarded/pending cash does not: quarantine its
        // exact queue rather than inferring recipient ownership from addresses.
        let pending: Vec<_> =
            self.state.reward_pending.iter().filter_map(|(offence, row)| row.best.map(|winner| (*offence, *row, winner))).collect();
        for (offence, row, winner) in pending {
            let accepted = self
                .state
                .consumed_offences
                .get(&offence)
                .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("old pending reporter winner has no consumed provenance".into()))?
                .accepted_daa;
            self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
                budget.bind_reporter_income_v1(
                    native_reporter_binding_id_v1(offence),
                    PalwReporterIncomeBindingV1 {
                        recipient: Some(winner.reporter),
                        payload: winner.payload,
                        maximum: row.amount,
                        accepted_daa: accepted,
                        awarded_daa: None,
                        queue: None,
                        paid_daa: None,
                        historical_boundary: None,
                    },
                    j,
                )
            })?
            .expect("created migration")
            .map_err(|why| budget_refused(winner.reporter, why))?;
        }
        let mut unknown: BTreeMap<Hash64, (PalwPayoutV2, Option<u64>)> =
            self.state.pending_payouts.iter().map(|(queue, payout)| (*queue, (*payout, None))).collect();
        for entry in &self.entries {
            if let PalwDeltaEntryV2::Payout { key, old: Some(payout), new: None } = entry {
                unknown.insert(*key, (*payout, Some(ctx.daa_score)));
            }
        }
        for (offence, payout) in &self.state.reporter_rewards {
            unknown.insert(crate::palw_vesting_v1::palw_reporter_payout_key_v1(offence), (*payout, None));
        }
        let book = self.state.bond_budget.as_ref().expect("created migration");
        unknown.retain(|queue, _| {
            !book.claims.iter().any(|(id, row)| {
                row.origin == PalwBudgetOriginV1::NativeIncomeQueued && *id == native_queued_income_id_v1(*queue, &row.bond)
            })
        });
        let historical_reporter_income = self.state.reporter_counters.awarded_sompi;
        let boundary_until = ctx
            .daa_score
            .checked_add(mirror.policy.window_daa)
            .ok_or(PalwStateV2Error::Overflow("legacy reporter income boundary"))?;
        let mut boundary_hash = keyed(b"misaka-palw/bond-budget/legacy-reporter-history-boundary/v1");
        boundary_hash.update(&ctx.daa_score.to_le_bytes());
        let boundary_id = finish(boundary_hash);
        // RFC-0006 (RFC06-1): the boundary and every unknown legacy queue is a listed row of the op.
        let legacy_rows = unknown.len().saturating_add(1);
        self.bond_budget_op(PalwBudgetGrowthV1::rows(legacy_rows), |budget, _, j| {
            if historical_reporter_income > 0 {
                budget.bind_reporter_income_v1(
                    boundary_id,
                    PalwReporterIncomeBindingV1 {
                        recipient: None,
                        payload: Hash64::default(),
                        maximum: 0,
                        accepted_daa: ctx.daa_score,
                        awarded_daa: None,
                        queue: None,
                        paid_daa: None,
                        historical_boundary: Some((historical_reporter_income, boundary_until)),
                    },
                    j,
                )?;
            }
            for (queue, (payout, paid)) in unknown {
                if payout.amount == 0 {
                    continue;
                }
                budget.bind_reporter_income_v1(
                    legacy_queue_liability_id_v1(queue),
                    PalwReporterIncomeBindingV1 {
                        recipient: None,
                        payload: payout.payload,
                        maximum: payout.amount,
                        accepted_daa: ctx.daa_score,
                        awarded_daa: None,
                        queue: Some(queue),
                        paid_daa: paid,
                        historical_boundary: None,
                    },
                    j,
                )?;
            }
            Ok(())
        })?
        .expect("created migration")
        .map_err(|why: PalwBudgetRefusalV1| PalwStateV2Error::BondBudgetRefused(why.to_string()))
    }

    // ---- hooks ---------------------------------------------------------------------------------------------------------------

    /// Initial migration preserves already accepted native money, including
    /// retired claim payees frozen in vesting receipts. Old pending rights can
    /// make today's R overfull and refuse any additional monetary income.
    fn seed_native_income_rights_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        mirror: &PalwBondBudgetMirrorV1,
    ) -> Result<(), PalwStateV2Error> {
        let originals: Vec<_> = self
            .state
            .claims
            .iter()
            .filter(|(_, claim)| !claim.phase.is_terminal() && matches!(claim.source, PalwClaimSourceV2::Attempt))
            .map(|(id, _)| *id)
            .collect();
        for id in originals {
            self.hold_issuer_income_v1(ctx, id)?;
        }
        let mut seeds: BTreeMap<(Hash64, PalwBondKeyV2, u8), (u64, u64, u64)> = BTreeMap::new();
        for (root, row) in &self.state.vesting {
            if row.producer.amount > 0 {
                seeds.insert((*root, row.producer_bond, 0), (row.final_daa, row.expiry_daa, row.producer.amount));
            }
            for (bond, payout) in &row.seats {
                if payout.amount == 0 {
                    continue;
                }
                let seed = seeds.entry((*root, *bond, 1)).or_insert((row.final_daa, row.expiry_daa, 0));
                seed.2 = seed.2.checked_add(payout.amount).ok_or(PalwStateV2Error::Overflow("old Panel income"))?;
            }
        }
        for (root, row) in &self.state.exec_v2_rewards {
            for ((bond, _), payout) in &row.legs {
                let issuer = self.state.vesting.get(root).is_some_and(|native| native.producer_bond == *bond)
                    || self.state.claims.get(root).is_some_and(|claim| claim.bond == *bond);
                let seed = seeds.entry((*root, *bond, if issuer { 0 } else { 2 })).or_insert((row.final_daa, row.liability_until, 0));
                seed.1 = seed.1.max(row.liability_until);
                seed.2 = seed.2.checked_add(payout.amount).ok_or(PalwStateV2Error::Overflow("old EXEC income"))?;
            }
        }
        // Existing bound work has already been accepted; reserve its maximum
        // even if accepted+W elapsed before this fence.
        for (root, duty) in &self.state.panel_duties {
            let Some(claim) = self.state.claims.get(root).filter(|claim| !claim.phase.is_terminal()) else { continue };
            let permille = self
                .state
                .claim_economics
                .get(root)
                .map_or(crate::palw_panel_economy_v1::PALW_PANEL_POOL_PERMILLE_V1 as u16, |e| e.panel_share_permille);
            let pool = (u128::from(claim.escrowed_reward) * u128::from(permille) / 1000) as u64;
            let maximum = if self.state.tir_shard_claims.contains_key(root) { pool } else { pool / duty.seats.len().max(1) as u64 };
            for bond in duty.seats.keys() {
                if maximum > 0 {
                    seeds.insert((*root, *bond, 1), (claim.accepted_daa, claim.trace_retention_daa, maximum));
                }
            }
        }
        for (root_id, root) in &self.state.exec_v2.roots {
            if matches!(
                root.phase,
                crate::palw_work_slice_v2::PalwWorkRootPhaseV2::Settled { .. }
                    | crate::palw_work_slice_v2::PalwWorkRootPhaseV2::Voided { .. }
            ) {
                continue;
            }
            let claim = self.state.claims.get(root_id).ok_or(PalwStateV2Error::MissingClaim(*root_id))?;
            let mut work: BTreeMap<PalwBondKeyV2, u64> = BTreeMap::new();
            for (_, slice) in self.state.exec_v2.slices.range((*root_id, 0)..=(*root_id, u32::MAX)) {
                if slice.executor == root.root_bond {
                    continue;
                }
                let sum = work.entry(slice.executor).or_default();
                *sum = sum
                    .checked_add(slice.range.work().ok_or(PalwStateV2Error::Overflow("old EXEC work"))?)
                    .ok_or(PalwStateV2Error::Overflow("old EXEC work"))?;
            }
            let until = super::palw_exec_v2_rewards_v1::horizon(&self.state, self.params, root_id)?.expect("root horizon");
            for (bond, work) in work {
                let maximum = (u128::from(claim.escrowed_reward) * u128::from(work) / u128::from(root.total_work)) as u64;
                if maximum > 0 {
                    seeds.insert((*root_id, bond, 2), (root.opened_daa, until, maximum));
                }
            }
        }
        for ((root, bond, kind), (accepted, until, maximum)) in seeds {
            let capital = self.state.bonds.get(&bond).map_or(0, |row| row.collateral);
            self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
                budget.hold_native_income_v1(
                    &mirror.policy,
                    capital,
                    native_income_id_v1(root, &bond, kind),
                    bond,
                    accepted,
                    until.max(ctx.daa_score),
                    maximum,
                    &[],
                    false,
                    j,
                )
            })?
            .expect("migration engine")
            .map_err(|why| budget_refused(bond, why))?;
        }
        Ok(())
    }

    #[allow(dead_code)] // dormant: not yet wired to a caller (its fence is unarmed)
    pub(super) fn reserve_bond_income_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        bond: PalwBondKeyV2,
        maximum: u64,
        actual: u64,
        origin: PalwBudgetOriginV1,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_bond_income_for_queue_v1(ctx, bond, maximum, actual, origin, None)
    }

    /// Reserve the full collected fee before the fill and, when a positive
    /// cash leg exists, retain it through that exact native queue's payment.
    /// Pool-only owned income has no unpaid cash obligation and uses collection
    /// plus W; a pool diversion is still counted once in the owner's full R.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn reserve_bond_income_for_queue_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        bond: PalwBondKeyV2,
        maximum: u64,
        actual: u64,
        origin: PalwBudgetOriginV1,
        queue: Option<Hash64>,
    ) -> Result<(), PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return Ok(()) };
        if maximum == 0 && actual == 0 {
            return Ok(());
        }
        let checkpoint = self.try_checkpoint()?;
        let reserved = (|| {
            let capital = self.state.bond(&bond).ok_or(PalwStateV2Error::MissingBond(bond))?.collateral;
            if self.state.bond_budget.is_none() {
                self.create_bond_budget_v1(ctx, &mirror)?;
            }
            let mut hash = keyed(b"misaka-palw/bond-budget/income/v1");
            hash.update(ctx.block.as_byte_slice());
            hash.update(&borsh::to_vec(&bond).expect("a bond serializes"));
            hash.update(&[origin as u8]);
            hash.update(&(self.entries.len() as u64).to_le_bytes());
            let id = finish(hash);
            self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
                budget.reserve_income_v1(&mirror.policy, capital, id, bond, ctx.daa_score, maximum, actual, origin, j)?;
                if let Some(queue) = queue {
                    budget.queue_collected_income_v1(&id, native_queued_income_id_v1(queue, &bond), j)?;
                }
                Ok(())
            })?
            .expect("created")
            .map_err(|why| budget_refused(bond, why))
        })();
        if let Err(error) = reserved {
            self.restore(checkpoint);
            return Err(error);
        }
        Ok(())
    }

    /// Native paid-source admission reserves the signed fixed fee before its
    /// opt-in becomes accepted. This row creates no computation or Round right.
    pub(super) fn reserve_source_income_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        context: Hash64,
        source: Hash64,
        bond: PalwBondKeyV2,
        liability_until: u64,
        fee: u64,
    ) -> Result<(), PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return Ok(()) };
        let capital = self.state.bond(&bond).ok_or(PalwStateV2Error::MissingBond(bond))?.collateral;
        if self.state.bond_budget.is_none() {
            self.create_bond_budget_v1(ctx, &mirror)?;
        }
        let id = source_income_id_v1(context, source, &bond);
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
            budget.reserve_source_income_v1(&mirror.policy, capital, id, bond, ctx.daa_score, liability_until, fee, j)
        })?
        .expect("created")
        .map_err(|why| budget_refused(bond, why))
    }

    /// Settle through the same durable R row, including a tick delayed past
    /// H + W. Initial migration supplies every old pending row; a missing
    /// reservation cannot become an unbounded grandfathered payment.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn settle_source_income_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        context: Hash64,
        source: Hash64,
        bond: PalwBondKeyV2,
        accepted_daa: u64,
        liability_until: u64,
        fee: u64,
    ) -> Result<(), PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return Ok(()) };
        let budget = self
            .state
            .bond_budget
            .as_ref()
            .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("source income engine missing".into()))?;
        let id = source_income_id_v1(context, source, &bond);
        if budget.claim_row(&id).is_none_or(|row| row.accepted_daa != accepted_daa) {
            return Err(PalwStateV2Error::BondBudgetRefused("source income reservation is missing or has another acceptance".into()));
        }
        let payload = self.state.bond(&bond).ok_or(PalwStateV2Error::MissingBond(bond))?.payout_payload;
        let queue = native_queued_income_id_v1(crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload), &bond);
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
            budget.queue_source_income_v1(&mirror.policy, &id, liability_until, ctx.daa_score, fee, queue, j)
        })?
        .expect("checked")
        .map_err(|why| budget_refused(bond, why))
    }

    pub(super) fn cancel_source_income_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        context: Hash64,
        source: Hash64,
        bond: PalwBondKeyV2,
        accepted_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        if self.bond_budget_mirror_at(ctx.daa_score).is_none() {
            return Ok(());
        }
        let budget = self
            .state
            .bond_budget
            .as_ref()
            .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("source income engine missing".into()))?;
        let id = source_income_id_v1(context, source, &bond);
        if budget.claim_row(&id).is_none_or(|row| row.accepted_daa != accepted_daa) {
            return Err(PalwStateV2Error::BondBudgetRefused("source income reservation is missing or has another acceptance".into()));
        }
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| budget.cancel_source_income_v1(&id, j))?
            .expect("checked")
            .map_err(|why| budget_refused(bond, why))
    }

    /// Kernel Q belongs to its claim row; its unpaid monetary maximum uses
    /// role 3 until an authenticated EXEC binding transfers that same money.
    pub(super) fn hold_kernel_income_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim: Hash64,
        liability_until: u64,
    ) -> Result<(), PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return Ok(()) };
        let id = kernel_budget_claim_id_v1(&claim.as_bytes());
        let Some(row) = self.state.bond_budget.as_ref().and_then(|b| b.claim_row(&id)).copied() else { return Ok(()) };
        if row.reserved.reward_sompi == 0 {
            return Ok(());
        }
        let capital = self.state.bonds.get(&row.bond).map_or(0, |b| b.collateral);
        self.bond_budget_op(PalwBudgetGrowthV1::rows(1), |budget, _, j| {
            budget.hold_native_income_v1(
                &mirror.policy,
                capital,
                native_income_id_v1(claim, &row.bond, 3),
                row.bond,
                row.accepted_daa,
                liability_until.max(ctx.daa_score),
                row.reserved.reward_sompi,
                &[id],
                false,
                j,
            )
        })?
        .expect("kernel claim engine")
        .map_err(|why| budget_refused(row.bond, why))?;
        self.bind_material_income_root_v1(ctx.daa_score, claim, native_income_id_v1(claim, &row.bond, 3))
    }

    /// Called only after the kernel verified the consumer-frozen EXEC owner
    /// binding. It cannot leave a second standalone payment entitlement.
    pub(super) fn transfer_kernel_income_to_exec_v1(
        &mut self,
        root: Hash64,
        evidence: Hash64,
        bond: PalwBondKeyV2,
        now: u64,
        until: u64,
    ) -> Result<(), PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(now) else { return Ok(()) };
        let source = native_income_id_v1(evidence, &bond, 3);
        let Some(held) = self.state.bond_budget.as_ref().and_then(|b| b.claim_row(&source)).copied() else { return Ok(()) };
        let issuer = self.state.claims.get(&root).is_some_and(|claim| claim.bond == bond);
        if issuer {
            // Its fee replaces part of the immutable issuer allocation. The
            // separately admitted standalone maximum stays in the window
            // (no early quota recovery), but cannot enlarge that allocation
            // or retain an independent future payment entitlement.
            self.cancel_native_recipient_v1(evidence, bond, 3)?;
            return Ok(());
        }
        let kind = 2;
        let id = native_income_id_v1(root, &bond, kind);
        let maximum = self
            .state
            .bond_budget
            .as_ref()
            .and_then(|b| b.claim_row(&id))
            .map_or(held.reserved.reward_sompi, |row| row.reserved.reward_sompi.max(held.reserved.reward_sompi));
        let capital = self.state.bonds.get(&bond).map_or(0, |row| row.collateral);
        self.bond_budget_op(PalwBudgetGrowthV1::rows(1), |budget, _, j| {
            budget.hold_native_income_v1(&mirror.policy, capital, id, bond, now, until, maximum, &[source], false, j)
        })?
        .expect("kernel transfer engine")
        .map_err(|why| budget_refused(bond, why))?;
        self.bind_material_income_root_v1(now, root, id)
    }

    /// **RFC08 R8-WINDOW (`palw_horizon_extension_v1`)**: a consented dependency horizon extension holds the executor's EXEC income
    /// through the same longer horizon. The hold only lengthens (its release is `max(old, until + W)`); its funded maximum, its
    /// payments and every other leg are unchanged. Nothing to extend (no engine, the issuer's own cancelled hold, a zero hold): `Ok`.
    pub(super) fn extend_exec_income_hold_v1(
        &mut self,
        root: Hash64,
        bond: PalwBondKeyV2,
        now: u64,
        until: u64,
    ) -> Result<(), PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(now) else { return Ok(()) };
        let id = native_income_id_v1(root, &bond, 2);
        let Some(row) = self.state.bond_budget.as_ref().and_then(|b| b.claim_row(&id)).copied() else { return Ok(()) };
        if row.reserved.reward_sompi == 0 {
            return Ok(());
        }
        let capital = self.state.bonds.get(&bond).map_or(0, |b| b.collateral);
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
            budget.hold_native_income_v1(
                &mirror.policy,
                capital,
                id,
                bond,
                row.accepted_daa,
                until,
                row.reserved.reward_sompi,
                &[],
                false,
                j,
            )
        })?
        .expect("the engine holding the row exists")
        .map_err(|why| budget_refused(bond, why))
    }

    /// Kernel carriers create no reward block or consensus weight. Their one
    /// standalone fee is Q/R-reserved before the trusted consumer commits it.
    pub(super) fn plan_kernel_claim_budget_v1(
        &self,
        ctx: &PalwBlockContextV2,
        bond: PalwBondKeyV2,
        kernel_class: Hash64,
        reward: u64,
        kernel_class_is_optimistic: bool,
    ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else {
            // RFC-0015 §8.3: an OPV kernel claim needs a budget (`unbudgeted_panel_free_v1`); a Panel-licensed one keeps its rule.
            return if kernel_class_is_optimistic { self.unbudgeted_panel_free_v1() } else { Ok(None) };
        };
        // RFC-0016 P4 / ADR-0177 (lane GC-DUTY, Lead 2026-10-28): a kernel route claim is user escrow — `plan_bond_budget_moded_v1`
        // never applies the model allocation (S_m) to `PalwBudgetOriginV1::KernelRoute`. The native-class lookup that stood here was
        // discarded there anyway and only refused (and so dropped) an honest Panel=1 claim whose class had no `KernelBoundV1`.
        let _ = (kernel_class, &mirror);
        let model = None;
        // ユーザー方針 2026-10-15: an OPV claim holds Panel=0's 7-day span; a Panel-licensed kernel claim Panel=1's day.
        let mode = if kernel_class_is_optimistic { PalwHoldModeV1::PanelZero } else { PalwHoldModeV1::PanelOne };
        // ユーザー判断 2026-10-15 item 6: the producer's liability is reserved beside the user's escrow (never replaced by it).
        let claim_type = if kernel_class_is_optimistic {
            crate::palw_bond_formula_v2::PalwClaimTypeV2::KernelPanelZero
        } else {
            crate::palw_bond_formula_v2::PalwClaimTypeV2::KernelPanelOne
        };
        let liability = self.bond_formula_producer_liability_v2(ctx, claim_type, reward, 0);
        self.plan_bond_budget_moded_v1(
            ctx,
            bond,
            model,
            PalwBudgetAskV1 { block_units: 0, reward_sompi: reward, final_weight: 0, liability_sompi: liability },
            PalwBudgetOriginV1::KernelRoute,
            0,
            mode,
        )
    }

    /// Re-injected on every load; cache/restore never carries a prior branch's
    /// consumer allowance. Legacy claims keep their admitted payment rule.
    pub(super) fn kernel_final_reward_caps_v1(
        &self,
        ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
    ) -> BTreeMap<misaka_palw_kernel::hash::Digest, u64> {
        let Some(budget) = self.state.bond_budget.as_ref() else { return BTreeMap::new() };
        ledger
            .claims
            .iter()
            .filter_map(|(claim, row)| {
                let id = kernel_budget_claim_id_v1(claim);
                let income = self
                    .state
                    .kernel_route
                    .as_ref()
                    .and_then(|route| route.bond_key_of(&row.producer))
                    .and_then(|bond| budget.claim_row(&native_income_id_v1(Hash64::from_bytes(*claim), &bond, 3)));
                match budget.claim_row(&id) {
                    Some(reservation)
                        if matches!(reservation.origin, PalwBudgetOriginV1::Legacy | PalwBudgetOriginV1::KernelRoute) =>
                    {
                        Some((
                            *claim,
                            income
                                .filter(|held| held.live && held.in_window)
                                .map_or(0, |held| held.reserved.reward_sompi - held.consumed.reward_sompi),
                        ))
                    }
                    _ if row.committed_daa >= budget.header.created_daa => Some((*claim, 0)),
                    _ => None,
                }
            })
            .collect()
    }

    /// The kernel emits only the allowance injected from this same budget.
    /// EXEC fees are paid by the root's R consumption, never through this hook.
    pub(super) fn consume_kernel_final_reward_v1(
        &mut self,
        claim: &misaka_palw_kernel::hash::Digest,
        bond: PalwBondKeyV2,
        payload: Hash64,
        now: u64,
        amount: u64,
    ) -> Result<(), PalwStateV2Error> {
        if self.bond_budget_mirror_at(now).is_none() {
            return Ok(());
        }
        self.pay_native_recipient_v1(
            Hash64::from_bytes(*claim),
            bond,
            3,
            now,
            amount,
            crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload),
            true,
        )
    }

    pub(super) fn close_terminal_kernel_budgets_v1(
        &mut self,
        ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
    ) -> Result<(), PalwStateV2Error> {
        if self.state.bond_budget.is_none() {
            return Ok(());
        }
        for (claim, row) in &ledger.claims {
            if row.convicted || row.life.state.is_terminal() {
                let id = kernel_budget_claim_id_v1(claim);
                // PESG §6: the liability hold starts at this block (the ledger's DAA).
                self.bond_budget_close_v1(&id, ledger.daa)?;
                // Gate C (lane GC-DUTY, R-SEAT): a Final whose reward is HELD for SecurityCertified keeps its income row until the
                // certification decides — `settle_kernel_reward_hold_v1` pays it then, or cancels it on an uncertified outcome.
                // Cancelling it here at Final left the certified payout with no open row ("the claim holds no open budget
                // reservation"), refusing every block that carried the decision.
                let held_for_certification = !row.convicted
                    && crate::palw_security_certification_v1::palw_kernel_claim_reward_hold_v1(&self.state, claim).is_some();
                if !held_for_certification
                    && let Some(bond) = self.state.kernel_route.as_ref().and_then(|route| route.bond_key_of(&row.producer))
                {
                    self.cancel_native_recipient_v1(Hash64::from_bytes(*claim), bond, 3)?;
                }
                // The claim row remains public. Financial quota is forgotten
                // only after its window releases; EXEC uses its root allocation.
                self.bond_budget_forget_v1(&id)?;
            }
        }
        Ok(())
    }

    /// **An attempt's plan** (lead, merged or rider): Q 1, B `budget_block_units` (one block for a lead, its share for a rider),
    /// R its escrow, F its Final contribution; the block's units consumed at once.
    pub(super) fn plan_attempt_budget_v1(
        &self,
        ctx: &PalwBlockContextV2,
        claim: &PalwClaimStateV2,
        artifact_root: &Hash64,
        rider: bool,
        budget_block_units: u64,
    ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
        if self.bond_budget_mirror_at(ctx.daa_score).is_none() {
            return Ok(None);
        }
        let canonical = self.canonical_claim_weight(claim);
        let contribution = palw_claim_safe_contribution_v3(&self.state.class_shares, claim, self.uncertified_weightless, canonical);
        let ask = PalwBudgetAskV1 {
            block_units: budget_block_units,
            reward_sompi: claim.escrowed_reward,
            final_weight: crate::palw_weight_cap_v1::palw_weight_final_safe_v1(self.params, claim, contribution),
            liability_sompi: self.bond_formula_liability_v2(ctx, claim),
        };
        let origin = if rider { PalwBudgetOriginV1::Rider } else { PalwBudgetOriginV1::Attempt };
        let model = self.bond_budget_model_of(&claim.class_id, Some(artifact_root));
        let plan = self.plan_bond_budget_v1(ctx, claim.bond, Some(model), ask, origin, budget_block_units)?;
        // RFC-0008 RFC08-2 (dormant `palw_settlement_once_v1`): the combined liability predicate at admission.
        if let Some(plan) = &plan {
            self.admit_claim_liability_v1(
                ctx.daa_score,
                plan,
                super::palw_settlement_liability_fold_v1::PalwAdmissionLiabilityV1::V2Claim,
            )?;
        }
        Ok(plan)
    }

    /// **A free-prompt commitment's plan**: Q 1, B `quanta` blocks, R `quanta` carves of this block, F `quanta` per-quantum weights.
    pub(super) fn plan_free_prompt_budget_v1(
        &self,
        ctx: &PalwBlockContextV2,
        claim: &PalwClaimStateV2,
        quanta: u32,
    ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
        if self.bond_budget_mirror_at(ctx.daa_score).is_none() {
            return Ok(None);
        }
        let carve = self.params.worker_carve_at(ctx.subsidy, self.extras.escrow_carve);
        let per_quantum = palw_fp_spend_weight_v1(&self.state, claim, quanta, &self.fp_pricing());
        let ask = PalwBudgetAskV1 {
            block_units: PALW_BUDGET_BLOCK_UNIT_V1.saturating_mul(quanta as u64),
            reward_sompi: carve.saturating_mul(quanta as u64),
            final_weight: per_quantum.saturating_mul(quanta as u128),
            liability_sompi: self.bond_formula_liability_v2(ctx, claim),
        };
        let model = self.bond_budget_model_of(&claim.class_id, None);
        let plan = self.plan_bond_budget_v1(ctx, claim.bond, Some(model), ask, PalwBudgetOriginV1::FreePrompt, 0)?;
        // RFC-0008 RFC08-2 (dormant `palw_settlement_once_v1`): the combined liability predicate at admission.
        if let Some(plan) = &plan {
            self.admit_claim_liability_v1(
                ctx.daa_score,
                plan,
                super::palw_settlement_liability_fold_v1::PalwAdmissionLiabilityV1::V2Claim,
            )?;
        }
        Ok(plan)
    }

    /// **A receipt spend's draw** (design §2.6): one block and this block's carve, strict — the coinbase pays the worker share whole, so
    /// a short reservation refuses the spend — and the quantum's weight, clipped. Returns the weight to credit; the input unchanged for
    /// an unbudgeted claim. Closes the claim once every quantum is spent.
    pub(super) fn bond_budget_receipt_spend_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim_id: &Hash64,
        per_quantum: u128,
        last_quantum: bool,
    ) -> Result<u128, PalwStateV2Error> {
        if self.state.bond_budget.as_ref().and_then(|b| b.remaining(claim_id, PalwBudgetDimV1::BlockUnits)).is_none() {
            return Ok(per_quantum);
        }
        let carve = self.params.worker_carve_at(ctx.subsidy, self.extras.escrow_carve);
        let bond = self.state.claims.get(claim_id).map(|c| c.bond).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?;
        let hold_until = self.bond_budget_liability_hold_until_v1(ctx.daa_score);
        // Strict: checked whole before either is taken.
        let budget = self.state.bond_budget.as_ref().expect("checked");
        let short = |dim, need: u128| budget.remaining(claim_id, dim).is_some_and(|left| left < need);
        if short(PalwBudgetDimV1::BlockUnits, PALW_BUDGET_BLOCK_UNIT_V1 as u128) {
            return Err(budget_refused(bond, PalwBudgetRefusalV1::Short { dim: PalwBudgetDimV1::BlockUnits }));
        }
        if short(PalwBudgetDimV1::Reward, carve as u128) {
            return Err(budget_refused(bond, PalwBudgetRefusalV1::Short { dim: PalwBudgetDimV1::Reward }));
        }
        if !self.state.bond_budget_fp_export_fits_v1(self.params, claim_id, ctx.daa_score, carve) {
            return Err(PalwStateV2Error::BondBudgetExhausted {
                bond,
                why: "the spend would export more than 0.51 of the collateral its liability still holds (PESG §6)".to_string(),
            });
        }
        let granted = self
            .bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
                let blocks = budget.consume(claim_id, PalwBudgetDimV1::BlockUnits, PALW_BUDGET_BLOCK_UNIT_V1 as u128, true, j);
                let reward = budget.consume(claim_id, PalwBudgetDimV1::Reward, carve as u128, true, j);
                debug_assert!(matches!((blocks, reward), (Some(Ok(_)), Some(Ok(_)))), "checked whole above");
                let weight =
                    budget.consume(claim_id, PalwBudgetDimV1::FinalWeight, per_quantum, false, j).and_then(Result::ok).unwrap_or(0);
                if last_quantum {
                    budget.close_at(claim_id, Some(hold_until), j);
                }
                weight
            })?
            .expect("checked");
        Ok(granted)
    }

    /// **A rider batch takes its share of the lead's one block** (design §2.1): the lead's reserved and consumed block units fall by
    /// `n·⌊U/(1+n)⌋` — taken, in the same transition, by the riders' own reservations (each `⌊U/(1+n)⌋`), so the bond's window keeps
    /// exactly one block. Returns each rider's units: 0 where the lead is unbudgeted or out of its window (the block is counted where it
    /// already is). The lead's reward reservation is NOT lowered (conservative: never an early recovery).
    pub(super) fn bond_budget_share_lead_block_v1(&mut self, lead_id: &Hash64, riders: u64) -> Result<u64, PalwStateV2Error> {
        let (lead_units, each) = palw_rider_block_attribution_v1(riders);
        let eligible = self.state.bond_budget.as_ref().and_then(|b| b.claim_row(lead_id)).is_some_and(|row| {
            row.open
                && row.in_window
                && row.origin != PalwBudgetOriginV1::Legacy
                && row.consumed.block_units >= PALW_BUDGET_BLOCK_UNIT_V1
        });
        if !eligible {
            return Ok(0);
        }
        let out = PALW_BUDGET_BLOCK_UNIT_V1 - lead_units;
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| budget.transfer_block_units_v1(lead_id, out, j))?
            .expect("eligible")
            .map_err(|why| PalwStateV2Error::CapacityRiders(format!("the lead's block cannot be shared: {why}")))?;
        Ok(each)
    }

    /// **The Round draw with every bond's rights capped before its candidate set** (readiness §3e; design §3b): `false` below the fence
    /// (the caller mints as it always did). Past it, the bond's remaining Round rights — its caps less every reservation still in its
    /// window — bound its candidates, the draw allocates at most the window, and each bond's allocated tickets are reserved (and used)
    /// at this DAA, released at `+ W`.
    pub(super) fn bond_budget_round_draw_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        schedule: &mut crate::palw_execution_lane_v1::PalwExecScheduleV1,
        quantum: u64,
        open_round: u64,
        window_rounds: u64,
        forfeited: &std::collections::BTreeSet<Hash64>,
    ) -> Result<bool, PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return Ok(false) };
        if self.state.bond_budget.is_none() {
            // The pre-object clock creates and validates the book. Without it
            // no ticket may escape into the old uncapped fallback.
            schedule.quanta.clear();
            return Ok(true);
        }
        let policy = mirror.policy;
        {
            let budget = self.state.bond_budget.as_ref().expect("created above");
            let bonds = &self.state.bonds;
            // ADR-0176 C3 (user decision 2026-10-15): the long view, the day view's rate and the burst view (V3 policy; the long view
            // alone for V2, which is `palw_round_rights_remaining_v1` unchanged).
            let remaining = |bond: &PalwBondKeyV2| {
                budget.round_rights_remaining_rated_v1(&policy, bonds.get(bond).map(|r| r.collateral).unwrap_or(0), bond)
            };
            crate::palw_execution_lane_v1::palw_execution_schedule_assign_quanta_windowed_capped_v1(
                schedule,
                quantum,
                open_round,
                window_rounds,
                forfeited,
                &remaining,
            );
        }
        let mut allocated: BTreeMap<PalwBondKeyV2, u64> = BTreeMap::new();
        for q in &schedule.quanta {
            *allocated.entry(q.bond).or_default() += 1;
        }
        let span = schedule.span_index;
        let now = ctx.daa_score;
        let checkpoint = self.try_checkpoint()?;
        let allocated_rows = allocated.len();
        let reserved = self.bond_budget_op(PalwBudgetGrowthV1::rows(allocated_rows), |budget, state, j| {
            for (bond, n) in allocated {
                let capital = state.bonds.get(&bond).map(|r| r.collateral).unwrap_or(0);
                budget.reserve_round_rights_v1(&policy, capital, span, bond, now, n, j)?;
            }
            Ok::<(), PalwBudgetRefusalV1>(())
        })?;
        if !matches!(reserved, Some(Ok(()))) {
            // Refuse the whole draw atomically if migration, duplicate span
            // identity or an arithmetic limit makes a reservation unavailable.
            // No partially allocated ticket or virtual-processor panic survives.
            self.restore(checkpoint);
            schedule.quanta.clear();
        }
        Ok(true)
    }

    /// **A Final's weight under the budget** (`finalize_claim`): the grant, clipped to the claim's reservation; the input unchanged for
    /// an unbudgeted claim.
    pub(super) fn bond_budget_final_weight_v1(&mut self, claim_id: &Hash64, contribution: u128) -> Result<u128, PalwStateV2Error> {
        Ok(self
            .bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
                budget.consume(claim_id, PalwBudgetDimV1::FinalWeight, contribution, false, j)
            })?
            .flatten()
            .map(|granted| granted.unwrap_or(0))
            .unwrap_or(contribution))
    }

    /// **A Final's reward under the budget** (`finalize_claim`, before the buyback and the panel split, so every leg derives from it):
    /// the grant; the rest is never named — never minted. The input unchanged for an unbudgeted claim.
    pub(super) fn bond_budget_final_reward_v1(&mut self, claim_id: &Hash64, escrow: u64) -> Result<u64, PalwStateV2Error> {
        if let Some(bond) = self.state.claims.get(claim_id).map(|claim| claim.bond)
            && let Some(held) = self.state.bond_budget.as_ref().and_then(|b| b.claim_row(&native_income_id_v1(*claim_id, &bond, 0)))
        {
            return Ok(escrow.min(held.reserved.reward_sompi));
        }
        Ok(self
            .bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
                budget.consume(claim_id, PalwBudgetDimV1::Reward, escrow as u128, false, j)
            })?
            .flatten()
            .map(|granted| granted.unwrap_or(0) as u64)
            .unwrap_or(escrow))
    }

    /// **A claim is terminal** (an attempt's Final, a void, a conviction) at `now`: no further consumption; nothing returns to the window;
    /// the bond's liability hold runs to `now + H_L` (PESG §6).
    pub(super) fn bond_budget_close_v1(&mut self, claim_id: &Hash64, now: u64) -> Result<(), PalwStateV2Error> {
        let hold_until = self.bond_budget_liability_hold_until_v1(now);
        // ADR-0176 amendment (ユーザー判断 2026-10-15): a claim priced by `palw_bond_formula_v2` keeps its liability collateral counted
        // until its liability ends — terminal + H_L, and at least its court/DA/retention span — not merely its withdrawal.
        let formula = self
            .state
            .bond_budget
            .as_ref()
            .and_then(|budget| budget.claim_row(claim_id))
            .map(|row| (row.accepted_daa, row.origin))
            .filter(|(accepted, _)| self.params.bond_formula_v2_applies_at(*accepted));
        if let Some((accepted, origin)) = formula {
            let retention = self
                .params
                .bond_formula_v2()
                .map(|m| m.policy.pricing(crate::palw_bond_formula_v2::palw_claim_type_of_origin_v2(origin)).retention_hold_daa)
                .unwrap_or(0);
            let until = hold_until.max(accepted.saturating_add(retention));
            self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| budget.close_keeping_liability_v1(claim_id, until, j))?;
            return Ok(());
        }
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| budget.close_at(claim_id, Some(hold_until), j))?;
        Ok(())
    }

    /// `now + H_L` under the policy in force (0 below the fence: no engine, nothing to hold).
    fn bond_budget_liability_hold_until_v1(&self, now: u64) -> u64 {
        self.params.bond_budget().map(|mirror| now.saturating_add(mirror.policy.liability_hold_daa)).unwrap_or(0)
    }

    /// **PESG §6: what of a claim's Final reward may leave the chain's books at once** (into the payout queue that mints, or a market
    /// reserve a holder can sell out of) while its liability still holds `claim.reserved`: `⌊0.51 · reserved⌋` (the policy's permille,
    /// at most 510). `None` below the fence (unbounded, the old rule). A vesting row is not an export: it moves only once its lock — the
    /// claim's liability — has ended.
    pub(super) fn bond_budget_export_cap_v1(&self, claim_id: &Hash64, claim: &PalwClaimStateV2, final_daa: u64) -> Option<u64> {
        let mirror = self.bond_budget_mirror_at(final_daa)?;
        // An old claim (no row, or a Legacy seed) is paid by the rule it was accepted under.
        self.state.bond_budget.as_ref()?.claim_row(claim_id).filter(|row| row.origin != PalwBudgetOriginV1::Legacy)?;
        Some(palw_bond_budget_export_cap_v1(&mirror.policy, claim.reserved))
    }

    /// **A claim left the state** (retirement): its row leaves once it is out of the window too.
    pub(super) fn bond_budget_forget_v1(&mut self, claim_id: &Hash64) -> Result<(), PalwStateV2Error> {
        self.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| budget.forget(claim_id, j))?;
        Ok(())
    }
}

/// **Step 1f — the budget's clock** (design §2.4, §2.9, §3.2–3.4), before the sweeps and the objects: create the engine at the fence's
/// first block (seeding the live old claims), release every reservation whose `reuse_not_before ≤ now`, roll the allocation epoch and
/// accrue this block's carve. A no-op below the fence.
pub(super) fn tick_bond_budget_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let Some(mirror) = builder.bond_budget_mirror_at(ctx.daa_score) else { return Ok(()) };
    if builder.state.bond_budget.is_none() {
        builder.create_bond_budget_v1(ctx, &mirror)?;
    }
    let now = ctx.daa_score;
    if builder.params.reporter_income_active_at(now)
        && builder.state.bond_budget.as_ref().expect("created").header.version == PALW_BOND_BUDGET_STATE_VERSION_V1
    {
        builder.initialize_reporter_income_v2(ctx, &mirror)?;
    }
    if builder.params.material_income_active_at(now)
        && builder.state.bond_budget.as_ref().expect("created").header.version < PALW_BOND_BUDGET_STATE_VERSION_V3
    {
        builder.initialize_material_income_v3(ctx, &mirror.policy)?;
    }
    if builder.params.paid_consumer_lifecycle_active_at(now)
        && builder.state.bond_budget.as_ref().expect("created").header.version < PALW_BOND_BUDGET_STATE_VERSION_V4
    {
        // Existing accepted claims keep their old terms. Only new admissions
        // write a contract, never retroactive or synthetic readiness/Final.
        builder.bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| budget.activate_consumer_contract_v4(j))?;
    }
    // Step 1b drained these exact obligations into the validated coinbase.
    // Queued credits stay held through sparse DAA jumps until that payment.
    let drained: BTreeSet<_> = builder
        .entries
        .iter()
        .filter_map(|entry| match entry {
            PalwDeltaEntryV2::Payout { key, old: Some(payout), new: None } if payout.amount > 0 => Some(*key),
            _ => None,
        })
        .collect();
    let paid: Vec<_> = builder
        .state
        .bond_budget
        .as_ref()
        .expect("created")
        .claims
        .iter()
        .filter(|(_, row)| row.origin == PalwBudgetOriginV1::NativeIncomeQueued && row.live)
        .filter(|(id, row)| drained.iter().any(|queue| native_queued_income_id_v1(*queue, &row.bond) == **id))
        .map(|(id, _)| *id)
        .collect();
    for id in paid {
        if let Some(held) = builder.state.bond_budget.as_ref().and_then(|b| b.claim_row(&id))
            && let Some(queue) = drained.iter().find(|queue| native_queued_income_id_v1(**queue, &held.bond) == id)
        {
            super::palw_monetary_material_fold_v1::observe_payment(builder, ctx, id, *queue)?;
        }
        builder
            .bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
                budget.confirm_native_income_payment_v1(&mirror.policy, &id, now, j)
            })?
            .expect("created")
            .map_err(|why| PalwStateV2Error::BondBudgetRefused(why.to_string()))?;
    }
    let legacy_paid: Vec<_> = builder
        .state
        .bond_budget
        .as_ref()
        .expect("created")
        .reporter_income
        .iter()
        .filter(|(_, row)| {
            row.recipient.is_none() && row.paid_daa.is_none() && row.queue.is_some_and(|queue| drained.contains(&queue))
        })
        .map(|(id, _)| *id)
        .collect();
    for id in legacy_paid {
        builder
            .bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| budget.confirm_unattributed_payment_v1(id, now, j))?
            .expect("created")
            .map_err(|why| PalwStateV2Error::BondBudgetRefused(why.to_string()))?;
    }
    for queue in &drained {
        builder
            .bond_budget_op(PalwBudgetGrowthV1::rows(material_rows_v1(builder)), |budget, _, j| {
                budget.confirm_legacy_material_queue_v1(*queue, now, mirror.policy.window_daa, j)
            })?
            .expect("created")
            .map_err(|why| PalwStateV2Error::BondBudgetRefused(why.to_string()))?;
    }
    // RFC-0006 (RFC06-1): each sweep is admitted for the rows it may touch — every material income row, every reporter binding,
    // every reservation and liability hold due at `now` — counted from the engine before it runs.
    builder.bond_budget_op(PalwBudgetGrowthV1::rows(material_rows_v1(builder)), |budget, _, j| {
        budget.release_material_income_v1(now, j)
    })?;
    let reporter_rows = builder.state.bond_budget.as_ref().map_or(0, |b| b.reporter_income.len());
    builder.bond_budget_op(PalwBudgetGrowthV1::rows(reporter_rows), |budget, _, j| {
        budget.release_reporter_income_v1(&mirror.policy, now, j)
    })?;
    let due_rows = builder.state.bond_budget.as_ref().map_or(0, |b| {
        b.releases
            .range(..(now.saturating_add(1), Hash64::default()))
            .count()
            .saturating_add(b.liability_releases.iter().take_while(|(at, _)| *at <= now).count())
    });
    builder.bond_budget_op(PalwBudgetGrowthV1::rows(due_rows), |budget, _, j| budget.release_due(now, j))?;
    // Funded court debt is redeemed only after the real queue drain and its
    // W release. Full R or a full native queue defers cash, never conviction.
    // Each award/progress pair creates a separate exact physical payout row.
    let book = builder.state.bond_budget.as_ref().expect("created");
    let awards: Vec<_> = book
        .unpaid_reporter_awards_v1()
        .map(|id| {
            let row = book.claim_row(id).expect("derived unpaid award index");
            (*id, row.bond, row.consumed.reward_sompi)
        })
        .collect();
    for (award, bond, progress) in awards {
        if builder.state.pending_payouts.len() >= PALW_V2_MAX_PENDING_PAYOUTS {
            break;
        }
        let Some(record) = builder.state.bonds.get(&bond) else {
            return Err(PalwStateV2Error::MissingBond(bond));
        };
        let capital = record.collateral;
        let book = builder.state.bond_budget.as_ref().expect("created");
        let binding = book
            .reporter_income
            .get(&award)
            .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("reporter tranche has no rooted frozen payee".into()))?;
        if binding.recipient != Some(bond)
            || binding.maximum != book.claim_row(&award).expect("indexed award").reserved.reward_sompi
            || binding.awarded_daa.is_none()
        {
            return Err(PalwStateV2Error::BondBudgetRefused("reporter tranche differs from its immutable entitlement".into()));
        }
        let payload = binding.payload;
        let queue = reporter_tranche_queue_v1(award, progress);
        // A collision never replaces an existing native monetary obligation.
        if builder.state.pending_payouts.contains_key(&queue) {
            return Err(PalwStateV2Error::BondBudgetRefused("reporter tranche queue already exists".into()));
        }
        // **RFC-0008 once-only settlement (RFC08-1; dormant `palw_settlement_once_v1`)**: one tranche `(award, progress)` is
        // redeemed once across every consumer. A tranche another consumer already settled is skipped BEFORE the engine redeems
        // anything (no R moves, no payout row); a first tranche is marked with what it actually paid, after the engine computed
        // it and before the payout row is written. A reporter's slash-funded award stands behind no claim: the combined liability
        // predicate does not bind it (by name). `Dormant` below the fence: nothing read or written.
        let tranche = crate::palw_settlement_once_v1::PalwSettlementObligationV1::with_party(
            crate::palw_settlement_once_v1::PalwSettlementLegV1::ReporterAwardTranche,
            award.as_bytes(),
            [0; 64],
            progress,
        );
        if builder.params.settlement_once_active_at(now) && builder.settled_row_checked_v1(&tranche)?.is_some() {
            continue;
        }
        let amount = builder
            .bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
                budget.redeem_reporter_income_v1(
                    &mirror.policy,
                    capital,
                    &award,
                    now,
                    native_queued_income_id_v1(queue, &bond),
                    u64::MAX,
                    j,
                )
            })?
            .expect("created")
            .map_err(|why| budget_refused(bond, why))?;
        if amount > 0 {
            let once = builder.settle_once_liable_v1(
                now,
                tranche,
                crate::palw_settlement_once_v1::PalwSettlementConsumerV1::BondBudgetTick,
                amount,
                crate::palw_settlement_once_v1::PalwSettlementDebitV1::Held { amount },
                super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::NoLiableClaim(
                    "a reporter's slash-funded award stands behind no claim",
                ),
            )?;
            if !once.pays() {
                // Unreachable after the pre-check (the marker cannot appear between them): fail closed, never a second row.
                return Err(PalwStateV2Error::BondBudgetRefused("reporter tranche already settled".into()));
            }
            builder.write_payout(queue, Some(PalwPayoutV2 { payload, amount }))?;
        }
    }
    builder.rearm_material_retirements_v1(now)?;
    if let (Some(policy), Some(epoch)) = (mirror.allocation_at(now).cloned(), mirror.allocation_epoch_at(now)) {
        let rolled = builder.state.bond_budget.as_ref().and_then(|b| b.header.allocation).is_some_and(|a| a.index >= epoch);
        if !rolled {
            // ADR-0177 as decided 2026-10-14 (a V3 policy): the 30-day average of miner + claim-bound verifier capital.
            let verifier = if policy.average.is_some_and(|a| a.verifier_capital) {
                builder.bond_budget_verifier_capital_v1()
            } else {
                BTreeMap::new()
            };
            // RFC-0017 GAP-E1: a Registered / Lapsed model's capital counts 0 in the allocation (denominator, numerator and block
            // opportunity) past `palw_model_eligibility_v1`; below it (no mirror) every model counts as before.
            let params = builder.params;
            // RFC-0006 (RFC06-1): the roll reads every bond and rewrites every model row and every seasoning assignment.
            let (roll_rows, assignment_rows) = builder.state.bond_budget.as_ref().map_or((0, 0), |b| {
                (b.models.len().saturating_add(b.assignments.len()).saturating_add(builder.state.bonds.len()), b.assignments.len())
            });
            builder.bond_budget_op(PalwBudgetGrowthV1::rows(roll_rows).with_assignments(assignment_rows), |budget, state, j| {
                let capital_of = |bond: &PalwBondKeyV2| match state.bonds.get(bond) {
                    Some(record) => (record.collateral, matches!(record.status, PalwBondStatusV2::Active)),
                    None => (0, false),
                };
                let eligible_capital = |model: &Hash64, capital: u128| {
                    let class =
                        if state.class(model).is_some() { Some(*model) } else { state.model_line(model).map(|line| line.class_id) };
                    let stage = class.and_then(|class| {
                        super::palw_model_eligibility_fold_v1::eligibility_of_v1(state, params, &class, now).map(|(stage, _)| stage)
                    });
                    crate::palw_model_eligibility_v1::palw_eligibility_allocation_capital_v1(stage, capital)
                };
                // ADR-0179 (user decisions 2026-10-15 rounds 4–5, `palw_model_tier_v1`): past the tier fence each live row's weight is
                // `c_m · S̄_m^α` — 1.0 for `ApprovedModels(h)`, the fixed 0.3 for every registered model — applied ONCE here, with the
                // registered tier's aggregate cap κ (RFC-0016's entrant reserve and RFC-0017's Probation are moot past it: κ is the
                // one tier bound). Below it: the untiered roll, byte for byte.
                let tier_mirror = params.model_tier_v1().filter(|mirror| mirror.active_at(now));
                match (policy.average.is_some(), tier_mirror) {
                    (true, Some(mirror)) => {
                        let tier = |model: &Hash64| {
                            let tier = crate::palw_model_tier_v1::palw_model_tier_of_model_v1(state, params, model, now);
                            (
                                !tier.is_approved(),
                                crate::palw_model_tier_v1::palw_model_tier_coefficient_permille_v1(Some(mirror), tier, now),
                            )
                        };
                        budget.roll_epoch_v3_tiered(
                            &policy,
                            epoch,
                            now,
                            capital_of,
                            &verifier,
                            eligible_capital,
                            tier,
                            Some(mirror.registered_cap_permille),
                            j,
                        )
                    }
                    (true, None) => budget.roll_epoch_v3(&policy, epoch, now, capital_of, &verifier, eligible_capital, j),
                    (false, _) => budget.roll_epoch(&policy, epoch, now, capital_of, j),
                }
            })?;
        }
        let carve = builder.params.worker_carve_at(ctx.subsidy, builder.extras.escrow_carve);
        let model_rows = builder.state.bond_budget.as_ref().map_or(0, |b| b.models.len());
        builder.bond_budget_op(PalwBudgetGrowthV1::rows(model_rows), |budget, _, j| budget.accrue(carve, j))?;
    }
    // RFC07 R7-ECON (`palw_model_verification_gate_v1`, round 3): the closed epochs' OPVB contexts frozen, locked beacons' tallies
    // opened, decided tallies taken and deferred Final rewards released or withheld — after the roll, before the sweeps that Final
    // claims. A no-op below the gate's fence.
    super::palw_econ_contracts_fold_v1::tick_model_verification_gate_v1(builder, ctx)?;
    Ok(())
}

/// **Tag 140: a bond's capital assignment** (ADR-0177 D3; design §3.1). The signature is the acceptance layer's; here the fence, an
/// Active bond and the engine's rules. A refusal drops the object (the acceptance rehearsal), the block standing.
pub(super) fn apply_capital_assignment_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    bond: &PalwBondKeyV2,
    assignments: &[(Hash64, u64)],
    sequence: u64,
) -> Result<(), PalwStateV2Error> {
    let refused = |why: &str| PalwStateV2Error::BondBudgetRefused(why.to_string());
    let Some(mirror) = builder.bond_budget_mirror_at(ctx.daa_score) else {
        return Err(refused("a capital assignment before palw_bond_budget_v1 is in force"));
    };
    let (Some(policy), Some(epoch)) = (mirror.allocation_at(ctx.daa_score).cloned(), mirror.allocation_epoch_at(ctx.daa_score)) else {
        return Err(refused("a capital assignment before palw_model_bond_allocation_v1 is in force"));
    };
    let record = builder.state.bonds.get(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?;
    if !matches!(record.status, PalwBondStatusV2::Active) {
        return Err(refused("a capital assignment from a bond that is not Active"));
    }
    let capital = record.collateral;
    if builder.state.bond_budget.is_none() {
        builder.create_bond_budget_v1(ctx, &mirror)?;
    }
    let bond = *bond;
    builder
        .bond_budget_op(PalwBudgetGrowthV1::rows(assignments.len()).with_assignments(1), |budget, state, j| {
            budget.assign(
                &policy,
                bond,
                assignments,
                sequence,
                capital,
                epoch,
                |m| state.classes.contains_key(m) || state.model_lines.contains_key(m),
                j,
            )
        })?
        .expect("created above")
        .map_err(|why| refused(&why.to_string()))
}

/// Independently funded text work grants no consensus block or Final weight.
/// Its original producer consent precedes this private, fixed Q/T reservation;
/// actual fee income is reserved separately in role 11 to avoid double R.
impl TransitionBuilder<'_> {
    /// **RFC-0015 §8.3 (lead default 2026-10-15, pending user review): no Panel-free claim is admitted unbudgeted.** `true` wherever
    /// the network declares the OPV policy (`palw_panel_free_v1`; the processor sets `budget_required` on every such network,
    /// calibration and test-armed worlds included — validate what ships).
    /// **ADR-0177 (2026-10-14): claim-bound verifier capital per bond and model** — every live Panel duty's seat exposure, attributed to
    /// the model its claim reserved against (claims without a budget model count nowhere). A waiting bond, a registration, an AMM /
    /// Position or an unpaid reward is never here: only collateral a seat has actually bound to a live claim.
    ///
    /// **ADR-0177 C5 (user decision 2026-10-15):** only capital with a defined verification liability, hold period and slashable scope
    /// counts ([`crate::palw_bond_budget_v1::palw_verifier_capital_counted_v1`]): a seat's exposure while its claim is live (not
    /// terminal). Capital merely bonded for Panel=0 / OPV has no source here and counts 0 until OPV's verifier-liability accounting
    /// exists ([`crate::palw_bond_budget_v1::PALW_OPV_VERIFIER_LIABILITY_ACCOUNTING_DEFINED_V1`]).
    pub(super) fn bond_budget_verifier_capital_v1(&self) -> BTreeMap<PalwBondKeyV2, BTreeMap<Hash64, u64>> {
        let mut out: BTreeMap<PalwBondKeyV2, BTreeMap<Hash64, u64>> = BTreeMap::new();
        let Some(budget) = self.state.bond_budget.as_ref() else { return out };
        for (claim_id, duty) in self.state.panel_duties.iter() {
            let Some((model, _)) = budget.claim_row(claim_id).and_then(|row| row.model) else { continue };
            let claim_live = self.state.claims.get(claim_id).is_some_and(|claim| !claim.phase.is_terminal());
            let counted = crate::palw_bond_budget_v1::palw_verifier_capital_counted_v1(
                crate::palw_bond_budget_v1::PalwVerifierCapitalSourceV1::PanelSeatExposure {
                    exposure: duty.seat_exposure,
                    claim_live,
                },
            );
            if counted == 0 {
                continue;
            }
            let amount = u64::try_from(counted).unwrap_or(u64::MAX);
            for seat in duty.seats.keys() {
                let entry = out.entry(*seat).or_default().entry(model).or_default();
                *entry = entry.saturating_add(amount);
            }
        }
        out
    }

    /// **A V2 claim's liability collateral for the budget** (PESG §6 `K`): `claim.reserved` (its weight term) below
    /// `palw_bond_formula_v2`; past it the formula's whole decomposed reservation — weight + right + collectable-loss top-up +
    /// court/DA/retention + a free prompt's priced receipt rights ([`crate::palw_state_v2::palw_claim_bond_reservation_v1`], the V2
    /// ledger's ONE expression).
    pub(super) fn bond_formula_liability_v2(&self, ctx: &PalwBlockContextV2, claim: &PalwClaimStateV2) -> u128 {
        if !self.params.bond_formula_v2_applies_at(ctx.daa_score) {
            return claim.reserved;
        }
        crate::palw_state_v2::palw_claim_bond_reservation_v1(self.params, claim).unwrap_or(u128::MAX)
    }

    /// **A kernel / native claim's producer liability** (ユーザー判断 2026-10-15 item 6): `below` under the old rule; past
    /// `palw_bond_formula_v2` at least the formula's total for the type (collectable loss on the reward the user escrowed + court/DA/
    /// retention) — the user's prepayment never removes it.
    pub(super) fn bond_formula_producer_liability_v2(
        &self,
        ctx: &PalwBlockContextV2,
        claim_type: crate::palw_bond_formula_v2::PalwClaimTypeV2,
        reward: u64,
        below: u128,
    ) -> u128 {
        use crate::palw_bond_formula_v2::{PalwBondFormulaInputV2, palw_bond_formula_split_v2};
        if !self.params.bond_formula_v2_applies_at(ctx.daa_score) {
            return below;
        }
        let (Some(formula), Some(budget)) = (self.params.bond_formula_v2(), self.params.bond_budget()) else { return below };
        let input = PalwBondFormulaInputV2 {
            claim_type,
            accepted_daa: ctx.daa_score,
            reward_ask_sompi: reward,
            final_weight_ask: 0,
            block_units_ask: 0,
            pwu_exposure_sompi: 0,
            reduction_allowed: false,
        };
        palw_bond_formula_split_v2(&formula.policy, &budget.policy, &input).map_or(u128::MAX, |split| split.total().max(below))
    }

    pub(super) fn panel_free_budget_required_v1(&self) -> bool {
        self.extras.kernel_route.as_ref().and_then(|e| e.opv.as_ref()).is_some_and(|o| o.budget_required)
    }

    /// What a Panel-free admission's plan answers below / without `palw_bond_budget_v1`: a refusal where the OPV policy requires a
    /// budget, `None` (unbudgeted, the historical rule) elsewhere.
    pub(super) fn unbudgeted_panel_free_v1(&self) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
        if self.panel_free_budget_required_v1() {
            Err(PalwStateV2Error::BondBudgetRefused(
                "RFC-0015 §8.3: a Panel-free claim is admitted only with a palw_bond_budget_v1 reservation (class-independent Q/B/R/F, common hold d + W)".into(),
            ))
        } else {
            Ok(None)
        }
    }
}

impl TransitionBuilder<'_> {
    pub(super) fn plan_funded_execution_budget_v13(
        &self,
        ctx: &PalwBlockContextV2,
        producer: PalwBondKeyV2,
        class: &misaka_palw_kernel::flat_execution11::CheckedNativeExecutionClassV11,
        job: &misaka_palw_kernel::flat_execution11::ExecutionJobRecordV11,
    ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
        use misaka_palw_kernel::flat_execution11::ExecutionClassContextV11;
        if !class.can_admit()
            || class.native_admission_epoch() != Some((ctx.block.as_bytes(), ctx.daa_score))
            || job.class_id != class.record().id()
            || job.producer != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&producer)
            || job.admitted_daa != ctx.daa_score
            || job.reward == 0
            || job.escrow_original_debit != job.reward
            || job.escrow_debit_receipt == [0; 64]
        {
            return Err(PalwStateV2Error::BondBudgetRefused(
                "funded work budget lacks exact native original admission/producer/money".into(),
            ));
        }
        // RFC-0015 §8.3: native funded work is Panel-free; unbudgeted only where no OPV policy requires a budget.
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return self.unbudgeted_panel_free_v1() };
        if mirror.allocation_at(ctx.daa_score).is_some() {
            // BindModelArtifactV2 has no V2 model-line registration. A copied
            // candidate/descriptor/inventory digest cannot manufacture one.
            return Err(PalwStateV2Error::BondBudgetRefused(
                "funded work has no independently authenticated native model-line allocation".into(),
            ));
        }
        let plan = self.plan_bond_budget_v1(
            ctx,
            producer,
            None,
            PalwBudgetAskV1 {
                block_units: 0,
                reward_sompi: 0,
                final_weight: 0,
                liability_sompi: self.bond_formula_producer_liability_v2(
                    ctx,
                    crate::palw_bond_formula_v2::PalwClaimTypeV2::NativeFunded,
                    job.reward,
                    class.record().required_producer_reserve as u128,
                ),
            },
            PalwBudgetOriginV1::KernelRoute,
            0,
        )?;
        // RFC-0008 / ADR-0184 §1 (dormant `palw_settlement_once_v1`): the signed job deadline bounds the claim's Final floor.
        self.admit_native_claim_liability_v1(ctx.daa_score, plan.as_ref(), job.deadline_daa)?;
        Ok(plan)
    }
}
pub(super) fn funded_execution_budget_id_v13(job: &misaka_palw_kernel::hash::Digest) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/native/original-funded-work-budget/v13", job))
}

impl TransitionBuilder<'_> {
    /// Original maximum reservation for the independently versioned text29
    /// book. Structural/public scope derivation alone is not native authority.
    /// The private current-block token binds the authenticated consent, shared
    /// principal nonce and paid monetary receipt before this budget is planned.
    pub(super) fn plan_native_text_budget_v29(
        &self,
        ctx: &PalwBlockContextV2,
        producer: PalwBondKeyV2,
        maximum: &crate::palw_native_text_stop_v29::CheckedNativeTextMaximumV29,
        funding: &crate::palw_native_text_stop_v29::NativeTextFundingV29,
        admission: &crate::palw_native_text_stop_v29::CheckedNativeTextAdmissionV29,
    ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
        let bad = |why: &str| PalwStateV2Error::BondBudgetRefused(why.into());
        admission.check_current(ctx.block.as_bytes(), ctx.daa_score, maximum, funding).map_err(|why| bad(&why))?;
        let m = maximum.record();
        let r = &funding.request;
        let extras = self.extras.kernel_route.as_ref().ok_or_else(|| bad("text29 budget lacks original consumer environment"))?;
        let final27 = extras.native_final.as_ref().ok_or_else(|| bad("text29 budget lacks trusted Final27 policy"))?;
        let original = extras.funded_execution.as_ref().ok_or_else(|| bad("text29 budget lacks original global21 producer terms"))?;
        let bond = self.state.bonds.get(&producer).ok_or_else(|| bad("text29 original producer bond absent"))?;
        if !matches!(bond.status, PalwBondStatusV2::Active)
            || r.producer != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&producer)
            || r.producer_operator != bond.operator_id.as_bytes()
            || r.requester_operator == r.producer_operator
            || m.final_policy27 != final27.policy.root()
            || final27.activation == crate::config::params::ForkActivation::never()
            || !final27.activation.is_active(ctx.daa_score)
            || original.activation == crate::config::params::ForkActivation::never()
            || !original.activation.is_active(ctx.daa_score)
            || funding.admitted_daa != ctx.daa_score
            || funding.maximum != *m
            || m.producer_reserve == 0
        {
            return Err(bad("text29 QT/R reservation changes original producer, policy, maximum or admission epoch"));
        }
        // RFC-0015 §8.3: native funded work is Panel-free; unbudgeted only where no OPV policy requires a budget.
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return self.unbudgeted_panel_free_v1() };
        if mirror.allocation_at(ctx.daa_score).is_some() {
            return Err(bad("text29 has no independently authenticated native model-line allocation"));
        }
        let plan = self.plan_bond_budget_v1(
            ctx,
            producer,
            None,
            PalwBudgetAskV1 {
                block_units: 0,
                reward_sompi: 0,
                final_weight: 0,
                liability_sompi: self.bond_formula_producer_liability_v2(
                    ctx,
                    crate::palw_bond_formula_v2::PalwClaimTypeV2::NativeFunded,
                    r.reward,
                    m.producer_reserve as u128,
                ),
            },
            PalwBudgetOriginV1::KernelRoute,
            0,
        )?;
        // RFC-0008 / ADR-0184 §1: the signed funding deadline bounds the claim's challenge end (its Final floor).
        self.admit_native_claim_liability_v1(ctx.daa_score, plan.as_ref(), funding.deadline)?;
        Ok(plan)
    }
}
pub(super) fn native_text_budget_id_v29(job: &misaka_palw_kernel::hash::Digest) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/native/original-text-maximum-budget/v29", job))
}

impl TransitionBuilder<'_> {
    /// Owning39 funded work shares the original bond Q/T reservation. A paid
    /// escrow job itself grants zero block, coinbase or Final-weight units;
    /// actual recipient income is held separately under original role 11.
    pub(super) fn plan_native_job_budget_v39(
        &self,
        ctx: &PalwBlockContextV2,
        producer: PalwBondKeyV2,
        class: &crate::palw_native_execution_v39::CheckedNativeClassV39,
        maximum: &misaka_palw_kernel::flat_job39::CheckedJobMaximumV39,
        quota: &misaka_palw_kernel::flat_quota11::owning39::CheckedQuotaV39,
        admission: &crate::palw_native_job_v39::CheckedNativeFundingV39,
    ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
        let bad = |why: &str| PalwStateV2Error::BondBudgetRefused(why.into());
        let extras = self.extras.kernel_route.as_ref().ok_or_else(|| bad("native39 budget lacks original native environment"))?;
        let owning = extras
            .native_execution39
            .as_ref()
            .filter(|f| f.activation != crate::config::params::ForkActivation::never() && f.activation.is_active(ctx.daa_score))
            .ok_or_else(|| bad("native39 budget lacks active owning policy"))?;
        let final27 = extras
            .native_final
            .as_ref()
            .filter(|f| f.activation != crate::config::params::ForkActivation::never() && f.activation.is_active(ctx.daa_score))
            .ok_or_else(|| bad("native39 budget lacks independent Final policy"))?;
        let funded = extras
            .funded_execution
            .as_ref()
            .filter(|f| f.activation != crate::config::params::ForkActivation::never() && f.activation.is_active(ctx.daa_score))
            .ok_or_else(|| bad("native39 budget lacks original shared execution policy"))?;
        let original = admission.original();
        original.check(class, maximum, quota, &owning.policies).map_err(|why| bad(&why))?;
        let bond = self.state.bonds.get(&producer).ok_or_else(|| bad("native39 original producer bond absent"))?;
        if !matches!(bond.status, PalwBondStatusV2::Active)
            || original.producer != producer
            || original.producer_operator != bond.operator_id.as_bytes()
            || original.requester_operator == original.producer_operator
            || original.admitted_block != ctx.block.as_bytes()
            || original.admitted != ctx.daa_score
            || original.reserve == 0
            || owning.policies.execution != funded.policies
            || owning.policies.final_policy != final27.policy.root()
            || extras.native_carriage_service.as_ref().map(|s| s.root()) != Some(owning.policies.physical_service)
        {
            return Err(bad("native39 original producer/current admission/shared terms/Final/physical policy differs"));
        }
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return self.unbudgeted_panel_free_v1() };
        if mirror.allocation_at(ctx.daa_score).is_some() {
            return Err(bad("native39 funded work has no authenticated native model-line allocation"));
        }
        let plan = self.plan_bond_budget_v1(
            ctx,
            producer,
            None,
            PalwBudgetAskV1 {
                block_units: 0,
                reward_sompi: 0,
                final_weight: 0,
                // ADR-0176 bond formula v2 (user decision 2026-10-15): the producer's liability for a Panel-free native funded
                // job, never below the original collectible reserve the producer signed.
                liability_sompi: self.bond_formula_producer_liability_v2(
                    ctx,
                    crate::palw_bond_formula_v2::PalwClaimTypeV2::NativeFunded,
                    original.reward,
                    original.reserve as u128,
                ),
            },
            PalwBudgetOriginV1::KernelRoute,
            0,
        )?;
        // RFC-0008 / ADR-0184 §1: the signed original deadline bounds `final_after` (the claim's Final floor).
        self.admit_native_claim_liability_v1(ctx.daa_score, plan.as_ref(), original.deadline)?;
        Ok(plan)
    }
}
pub(super) fn native_job_budget_id_v39(job: &misaka_palw_kernel::hash::Digest) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/native/original-owning-job-budget/v39", job))
}

// ---- Owning39 mining receipts on the shared bond budget (ADR-0176 Q/B/R/F, ADR-0177 allocation, PESG §6 0.51 export) ----
//
// A mining claim is a receipt-type claim (origin `FreePrompt`, the FP receipt lane's accounting): at acceptance it reserves
// B = quanta block units, R = quanta x the acceptance block's worker carve, F = quanta x weight-per-quantum and the bond formula
// v2 liability over its exposure, against the producer's common caps and the model's ADR-0177 allocation (`model = native model
// line`, so P_m clips and the per-model block opportunity binds; the tier coefficient c_m is the allocation's, applied once there,
// never here). Each receipt block then draws ONE block unit and its own carve strictly and its weight clipped. The original
// escrow credit of the components' jobs (origin KernelRoute, B = F = 0) is a different row and is never a receipt.
impl PalwChainStateV2 {
    /// The native receipt block's strict draw, asked against any state: a budget is REQUIRED (no unbudgeted native receipt), one
    /// block unit and `carve` must remain, and the exports so far plus `carve` stay within `floor(0.51 x liability)` while the
    /// liability holds.
    pub fn bond_budget_native_receipt_fits_v39(
        &self,
        params: &PalwStateParamsV2,
        budget_id: &Hash64,
        now_daa: u64,
        carve: u64,
    ) -> bool {
        let Some(mirror) = params.bond_budget().filter(|m| m.active_at(now_daa)) else { return false };
        let Some(budget) = self.bond_budget.as_ref() else { return false };
        let Some(row) = budget.claim_row(budget_id).filter(|r| r.open && r.origin == PalwBudgetOriginV1::FreePrompt) else {
            return false;
        };
        let blocks = budget.remaining(budget_id, PalwBudgetDimV1::BlockUnits).unwrap_or(0);
        let reward = budget.remaining(budget_id, PalwBudgetDimV1::Reward).unwrap_or(0);
        let cap = palw_bond_budget_export_cap_v1(&mirror.policy, row.liability_sompi) as u128;
        blocks >= PALW_BUDGET_BLOCK_UNIT_V1 as u128
            && reward >= carve as u128
            && (row.consumed.reward_sompi as u128).saturating_add(carve as u128) <= cap
    }
}
impl TransitionBuilder<'_> {
    /// Plan a native mining claim's reservation. Refused (never unbudgeted) below the budget fence.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn plan_native_mining_budget_v39(
        &self,
        ctx: &PalwBlockContextV2,
        producer: PalwBondKeyV2,
        model: Hash64,
        quanta: u64,
        weight_per_quantum: u128,
        exposure_sompi: u128,
    ) -> Result<PalwBudgetPlanV1, PalwStateV2Error> {
        let bad = |why: &str| PalwStateV2Error::BondBudgetRefused(why.into());
        if self.bond_budget_mirror_at(ctx.daa_score).is_none() {
            return Err(bad("native39 mining requires the ADR-0176 bond budget in force"));
        }
        let carve = self.params.worker_carve_at(ctx.subsidy, self.extras.escrow_carve);
        let block_units = PALW_BUDGET_BLOCK_UNIT_V1.checked_mul(quanta).ok_or_else(|| bad("native39 block units overflow"))?;
        let reward_sompi = carve.checked_mul(quanta).ok_or_else(|| bad("native39 reward ask overflow"))?;
        let final_weight = weight_per_quantum.checked_mul(quanta as u128).ok_or_else(|| bad("native39 weight ask overflow"))?;
        let liability_sompi =
            self.bond_formula_native_mining_liability_v39(ctx, reward_sompi, final_weight, block_units, exposure_sompi);
        let plan = self
            .plan_bond_budget_v1(
                ctx,
                producer,
                Some(model),
                PalwBudgetAskV1 { block_units, reward_sompi, final_weight, liability_sompi },
                PalwBudgetOriginV1::FreePrompt,
                0,
            )?
            .ok_or_else(|| bad("native39 mining reservation could not be planned"))?;
        // RFC-0008 / ADR-0184 §1: a mining claim is accepted only over Final components (its Master's Final is at or before this
        // block), so its Final floor is this block's DAA at the latest.
        self.admit_native_claim_liability_v1(ctx.daa_score, Some(&plan), ctx.daa_score)?;
        Ok(plan)
    }
    /// ADR-0176 bond formula v2 over the claim's whole ask and its slash exposure (type `NativeFunded`, the Panel-free mode);
    /// below the formula's fence the exposure itself.
    fn bond_formula_native_mining_liability_v39(
        &self,
        ctx: &PalwBlockContextV2,
        reward: u64,
        final_weight: u128,
        block_units: u64,
        exposure: u128,
    ) -> u128 {
        use crate::palw_bond_formula_v2::{PalwBondFormulaInputV2, PalwClaimTypeV2, palw_bond_formula_split_v2};
        if !self.params.bond_formula_v2_applies_at(ctx.daa_score) {
            return exposure;
        }
        let (Some(formula), Some(budget)) = (self.params.bond_formula_v2(), self.params.bond_budget()) else { return exposure };
        let input = PalwBondFormulaInputV2 {
            claim_type: PalwClaimTypeV2::NativeFunded,
            accepted_daa: ctx.daa_score,
            reward_ask_sompi: reward,
            final_weight_ask: final_weight,
            block_units_ask: block_units,
            pwu_exposure_sompi: exposure,
            reduction_allowed: false,
        };
        palw_bond_formula_split_v2(&formula.policy, &budget.policy, &input).map_or(u128::MAX, |split| split.total().max(exposure))
    }
    /// One receipt block's draw: strict B and R (the coinbase pays this block's worker share whole), the 0.51 export cap, and F
    /// clipped. Returns the weight to credit. Closes the row with the receipt's last quantum.
    pub(super) fn bond_budget_native_receipt_spend_v39(
        &mut self,
        ctx: &PalwBlockContextV2,
        budget_id: &Hash64,
        bond: PalwBondKeyV2,
        per_quantum: u128,
        last_quantum: bool,
    ) -> Result<u128, PalwStateV2Error> {
        let carve = self.params.worker_carve_at(ctx.subsidy, self.extras.escrow_carve);
        if !self.state.bond_budget_native_receipt_fits_v39(self.params, budget_id, ctx.daa_score, carve) {
            return Err(PalwStateV2Error::BondBudgetExhausted {
                bond,
                why: "a native receipt block exceeds its reservation or the 0.51 export of its liability (PESG §6)".to_string(),
            });
        }
        let hold_until = self.bond_budget_liability_hold_until_v1(ctx.daa_score);
        let granted = self
            .bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, j| {
                let blocks = budget.consume(budget_id, PalwBudgetDimV1::BlockUnits, PALW_BUDGET_BLOCK_UNIT_V1 as u128, true, j);
                let reward = budget.consume(budget_id, PalwBudgetDimV1::Reward, carve as u128, true, j);
                debug_assert!(matches!((blocks, reward), (Some(Ok(_)), Some(Ok(_)))), "checked whole above");
                let weight =
                    budget.consume(budget_id, PalwBudgetDimV1::FinalWeight, per_quantum, false, j).and_then(Result::ok).unwrap_or(0);
                if last_quantum {
                    budget.close_at(budget_id, Some(hold_until), j);
                }
                weight
            })?
            .ok_or(PalwStateV2Error::BondBudgetRefused("native39 receipt budget engine absent".into()))?;
        Ok(granted)
    }
}
