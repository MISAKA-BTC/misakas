//! **Lane BUDGET (ADR-0176 / ADR-0177): the bond budget in the fold** — dormant behind `Params::palw_bond_budget_v1` (and, for the
//! allocation, `palw_model_bond_allocation_v1`), which no network can arm. A child module of `palw_state_v2`, as the kernel route's and
//! the provider court's folds are, so it reads the builder and writes the state only through the journaled writers: the engine's
//! journal ([`crate::palw_bond_budget_v1::PalwBudgetWriteV1`]) becomes deltas 190 (rows) and 191 (the header).
//!
//! # Where it runs (design `docs/design/palw/bond-budget-and-model-allocation.md` §4)
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

impl PalwChainStateV2 {
    /// **Lane BUDGET: the bond budget's engine**, once the fence's first block has folded.
    pub fn bond_budget(&self) -> Option<&PalwBondBudgetStateV1> {
        self.bond_budget.as_ref()
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
}

impl PalwBudgetPlanV1 {
    pub(super) fn reservation(&self) -> &PalwBudgetVectorV1 {
        &self.reservation
    }
}

fn budget_refused(bond: PalwBondKeyV2, why: PalwBudgetRefusalV1) -> PalwStateV2Error {
    PalwStateV2Error::BondBudgetExhausted { bond, why: why.to_string() }
}

impl TransitionBuilder<'_> {
    /// The engine's journal, as deltas.
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

    /// **Run `f` on the engine** with the rest of the state readable and a journal, then journal it. `None` with no engine.
    fn bond_budget_op<R>(
        &mut self,
        f: impl FnOnce(&mut PalwBondBudgetStateV1, &PalwChainStateV2, &mut Vec<PalwBudgetWriteV1>) -> R,
    ) -> Option<R> {
        let mut budget = self.state.bond_budget.take()?;
        let mut journal = Vec::new();
        let out = f(&mut budget, &self.state, &mut journal);
        self.state.bond_budget = Some(budget);
        self.journal_bond_budget(journal);
        Some(out)
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
    fn plan_bond_budget_v1(
        &self,
        ctx: &PalwBlockContextV2,
        bond: PalwBondKeyV2,
        model: Option<Hash64>,
        ask: PalwBudgetAskV1,
        origin: PalwBudgetOriginV1,
        consume_blocks_now: u64,
    ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return Ok(None) };
        let capital = self.state.bonds.get(&bond).map(|r| r.collateral).unwrap_or(0);
        let model = model.filter(|_| mirror.allocation_at(ctx.daa_score).is_some());
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
        let reservation =
            budget.preview_claim_v1(&mirror.policy, capital, bond, model, ask).map_err(|why| budget_refused(bond, why))?;
        if consume_blocks_now > reservation.block_units {
            return Err(budget_refused(bond, PalwBudgetRefusalV1::Short { dim: PalwBudgetDimV1::BlockUnits }));
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
        }))
    }

    /// **Commit a planned reservation** for `claim_id` (after the claim is written) and consume the claim's own block now.
    pub(super) fn commit_bond_budget_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim_id: Hash64,
        plan: PalwBudgetPlanV1,
    ) -> Result<(), PalwStateV2Error> {
        if self.state.bond_budget.is_none() {
            let mirror = self.bond_budget_mirror_at(ctx.daa_score).expect("a plan exists only past the fence");
            self.create_bond_budget_v1(ctx, &mirror);
        }
        let bond = plan.bond;
        let out = self
            .bond_budget_op(|budget, _, j| {
                let reserved = budget.reserve_claim_v1(
                    &plan.policy,
                    plan.capital,
                    claim_id,
                    plan.bond,
                    plan.model,
                    plan.accepted_daa,
                    plan.ask,
                    plan.origin,
                    j,
                )?;
                debug_assert_eq!(reserved, plan.reservation, "the plan and the commit read one engine state");
                if plan.consume_blocks_now > 0 {
                    budget
                        .consume(&claim_id, PalwBudgetDimV1::BlockUnits, plan.consume_blocks_now as u128, true, j)
                        .expect("a fresh row")?;
                }
                Ok::<(), PalwBudgetRefusalV1>(())
            })
            .expect("created above");
        // The plan read this same engine state, so the commit cannot refuse; were it to, the block fails (never a skip: the claim's other
        // rows are written by now).
        out.map_err(|why| {
            PalwStateV2Error::BondBudgetRefused(format!("bond {bond:?}: the planned reservation could not be committed: {why}"))
        })
    }

    /// Create the engine (delta 191, `None → Some`) and seed the window from every live old claim (design §2.9).
    fn create_bond_budget_v1(&mut self, ctx: &PalwBlockContextV2, mirror: &PalwBondBudgetMirrorV1) {
        let budget = PalwBondBudgetStateV1::new(ctx.daa_score, &mirror.policy);
        self.entries.push(PalwDeltaEntryV2::BondBudgetHeader { old: None, new: Some(budget.header.clone()) });
        self.state.bond_budget = Some(budget);
        // Old live claims count against their bonds' new windows until their own `accepted + W`, never consumed (old rules pay them).
        let window = mirror.policy.window_daa;
        let carve = self.params.worker_carve_at(ctx.subsidy, self.extras.escrow_carve);
        let pricing = self.fp_pricing();
        let mut seeds: Vec<(Hash64, PalwBondKeyV2, u64, PalwBudgetVectorV1, u128)> = Vec::new();
        for (id, claim) in &self.state.claims {
            if claim.accepted_daa.saturating_add(window) <= ctx.daa_score {
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
            // PESG §6: an open old claim's liability collateral counts too (a Final one's FP spends hold none of their own).
            let liability = if matches!(claim.phase, PalwClaimPhaseV2::Final { .. }) { 0 } else { claim.reserved };
            seeds.push((*id, claim.bond, claim.accepted_daa, vector, liability));
        }
        let policy = mirror.policy.clone();
        self.bond_budget_op(|budget, state, j| {
            for (id, bond, accepted, vector, liability) in seeds {
                let capital = state.bonds.get(&bond).map(|r| r.collateral).unwrap_or(0);
                // A seed is counted, never refused (the window may start over-full); its id is new to an empty engine.
                let _ = budget.reserve_liable(
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
                );
            }
        });
    }

    // ---- hooks ---------------------------------------------------------------------------------------------------------------

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
            liability_sompi: claim.reserved,
        };
        let origin = if rider { PalwBudgetOriginV1::Rider } else { PalwBudgetOriginV1::Attempt };
        let model = self.bond_budget_model_of(&claim.class_id, Some(artifact_root));
        self.plan_bond_budget_v1(ctx, claim.bond, Some(model), ask, origin, budget_block_units)
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
            liability_sompi: claim.reserved,
        };
        let model = self.bond_budget_model_of(&claim.class_id, None);
        self.plan_bond_budget_v1(ctx, claim.bond, Some(model), ask, PalwBudgetOriginV1::FreePrompt, 0)
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
            .bond_budget_op(|budget, _, j| {
                let blocks = budget.consume(claim_id, PalwBudgetDimV1::BlockUnits, PALW_BUDGET_BLOCK_UNIT_V1 as u128, true, j);
                let reward = budget.consume(claim_id, PalwBudgetDimV1::Reward, carve as u128, true, j);
                debug_assert!(matches!((blocks, reward), (Some(Ok(_)), Some(Ok(_)))), "checked whole above");
                let weight =
                    budget.consume(claim_id, PalwBudgetDimV1::FinalWeight, per_quantum, false, j).and_then(Result::ok).unwrap_or(0);
                if last_quantum {
                    budget.close_at(claim_id, Some(hold_until), j);
                }
                weight
            })
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
        self.bond_budget_op(|budget, _, j| budget.transfer_block_units_v1(lead_id, out, j))
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
    ) -> bool {
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return false };
        if self.state.bond_budget.is_none() {
            self.create_bond_budget_v1(ctx, &mirror);
        }
        let policy = mirror.policy;
        {
            let budget = self.state.bond_budget.as_ref().expect("created above");
            let bonds = &self.state.bonds;
            let remaining = |bond: &PalwBondKeyV2| {
                palw_round_rights_remaining_v1(&policy, bonds.get(bond).map(|r| r.collateral).unwrap_or(0), budget.bond_row(bond))
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
        self.bond_budget_op(|budget, state, j| {
            for (bond, n) in allocated {
                let capital = state.bonds.get(&bond).map(|r| r.collateral).unwrap_or(0);
                let fitted = budget.reserve_round_rights_v1(&policy, capital, span, bond, now, n, j);
                debug_assert!(fitted.is_ok(), "the candidates were capped at the remaining rights of this same state: {fitted:?}");
            }
        });
        true
    }

    /// **A Final's weight under the budget** (`finalize_claim`): the grant, clipped to the claim's reservation; the input unchanged for
    /// an unbudgeted claim.
    pub(super) fn bond_budget_final_weight_v1(&mut self, claim_id: &Hash64, contribution: u128) -> u128 {
        self.bond_budget_op(|budget, _, j| budget.consume(claim_id, PalwBudgetDimV1::FinalWeight, contribution, false, j))
            .flatten()
            .map(|granted| granted.unwrap_or(0))
            .unwrap_or(contribution)
    }

    /// **A Final's reward under the budget** (`finalize_claim`, before the buyback and the panel split, so every leg derives from it):
    /// the grant; the rest is never named — never minted. The input unchanged for an unbudgeted claim.
    pub(super) fn bond_budget_final_reward_v1(&mut self, claim_id: &Hash64, escrow: u64) -> u64 {
        self.bond_budget_op(|budget, _, j| budget.consume(claim_id, PalwBudgetDimV1::Reward, escrow as u128, false, j))
            .flatten()
            .map(|granted| granted.unwrap_or(0) as u64)
            .unwrap_or(escrow)
    }

    /// **A claim is terminal** (an attempt's Final, a void, a conviction) at `now`: no further consumption; nothing returns to the window;
    /// the bond's liability hold runs to `now + H_L` (PESG §6).
    pub(super) fn bond_budget_close_v1(&mut self, claim_id: &Hash64, now: u64) {
        let hold_until = self.bond_budget_liability_hold_until_v1(now);
        self.bond_budget_op(|budget, _, j| budget.close_at(claim_id, Some(hold_until), j));
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
    pub(super) fn bond_budget_forget_v1(&mut self, claim_id: &Hash64) {
        self.bond_budget_op(|budget, _, j| budget.forget(claim_id, j));
    }
}

/// **Step 1f — the budget's clock** (design §2.4, §2.9, §3.2–3.4), before the sweeps and the objects: create the engine at the fence's
/// first block (seeding the live old claims), release every reservation whose `reuse_not_before ≤ now`, roll the allocation epoch and
/// accrue this block's carve. A no-op below the fence.
pub(super) fn tick_bond_budget_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) {
    let Some(mirror) = builder.bond_budget_mirror_at(ctx.daa_score) else { return };
    if builder.state.bond_budget.is_none() {
        builder.create_bond_budget_v1(ctx, &mirror);
    }
    let now = ctx.daa_score;
    builder.bond_budget_op(|budget, _, j| budget.release_due(now, j));
    if let (Some(policy), Some(epoch)) = (mirror.allocation_at(now).cloned(), mirror.allocation_epoch_at(now)) {
        let rolled = builder.state.bond_budget.as_ref().and_then(|b| b.header.allocation).is_some_and(|a| a.index >= epoch);
        if !rolled {
            builder.bond_budget_op(|budget, state, j| {
                budget.roll_epoch(
                    &policy,
                    epoch,
                    now,
                    |bond| match state.bonds.get(bond) {
                        Some(record) => (record.collateral, matches!(record.status, PalwBondStatusV2::Active)),
                        None => (0, false),
                    },
                    j,
                )
            });
        }
        let carve = builder.params.worker_carve_at(ctx.subsidy, builder.extras.escrow_carve);
        builder.bond_budget_op(|budget, _, j| budget.accrue(carve, j));
    }
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
        builder.create_bond_budget_v1(ctx, &mirror);
    }
    let bond = *bond;
    builder
        .bond_budget_op(|budget, state, j| {
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
        })
        .expect("created above")
        .map_err(|why| refused(&why.to_string()))
}
