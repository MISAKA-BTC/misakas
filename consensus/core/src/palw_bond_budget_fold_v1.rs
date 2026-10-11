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
        let mut builder = TransitionBuilder::new(self, params, false, false, false, false, &extras);
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

    /// **Does `bond` still hold a budget reservation inside its window at `now_daa`?** — the withdrawal hold (design §2.4): the capital
    /// that earned a window cannot leave, or be re-registered, before the window has passed. `false` with no engine.
    pub fn bond_budget_window_holds(&self, bond: &PalwBondKeyV2, now_daa: u64) -> bool {
        self.bond_budget.as_ref().is_some_and(|b| b.window_holds(bond, now_daa))
    }

    /// **What a budgeted claim was granted in Final weight** (hook H-4: rule E reads this, never the full contribution) — `None` for a
    /// claim with no row or a seeded old one.
    pub fn bond_budget_final_weight(&self, claim: &Hash64) -> Option<u128> {
        self.bond_budget.as_ref().and_then(|b| b.final_weight_granted(claim))
    }

    /// **Would the fold's strict budget draw take a receipt spend of `claim_id`?** (hook H-5: the processor asks it of the PARENT state
    /// before admitting a receipt block) — one block and `carve()` left on the claim's reservation; `true` for an unbudgeted claim, and
    /// the carve is not read then.
    pub fn bond_budget_spend_fits_v1(&self, claim_id: &Hash64, carve: impl FnOnce() -> u64) -> bool {
        let Some(budget) = self.bond_budget.as_ref() else { return true };
        let Some(blocks) = budget.remaining(claim_id, PalwBudgetDimV1::BlockUnits) else { return true };
        blocks >= PALW_BUDGET_BLOCK_UNIT_V1 as u128
            && budget.remaining(claim_id, PalwBudgetDimV1::Reward).is_some_and(|reward| reward >= carve() as u128)
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
pub(super) fn kernel_budget_claim_id_v1(claim: &misaka_palw_kernel::hash::Digest) -> Hash64 {
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
        let Some(mirror) = self.bond_budget_mirror_at(now) else { return Ok(()) };
        if maximum == 0 {
            return Ok(());
        }
        let capital = self.state.bonds.get(&bond).ok_or(PalwStateV2Error::MissingBond(bond))?.collateral;
        self.bond_budget_op(|budget, _, j| {
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
        })
        .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("native income admission has no budget engine".into()))?
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
                self.cancel_native_recipient_v1(root, bond, kind);
            }
            return Ok(());
        }
        self.bond_budget_op(|budget, _, j| {
            budget.pay_native_income_v1(
                &mirror.policy,
                &native_income_id_v1(root, &bond, kind),
                now,
                amount,
                native_queued_income_id_v1(queue_key, &bond),
                finished,
                j,
            )
        })
        .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("native payment has no budget engine".into()))?
        .map_err(|why| budget_refused(bond, why))
    }

    pub(super) fn cancel_native_recipient_v1(&mut self, root: Hash64, bond: PalwBondKeyV2, kind: u8) {
        let id = native_income_id_v1(root, &bond, kind);
        if self
            .state
            .bond_budget
            .as_ref()
            .and_then(|b| b.claim_row(&id))
            .is_some_and(|r| r.live && matches!(r.origin, PalwBudgetOriginV1::NativeIncome | PalwBudgetOriginV1::NativeIncomePartial))
        {
            self.bond_budget_op(|budget, _, j| budget.cancel_native_income_v1(&id, j)).expect("checked engine").expect("native hold");
        }
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
        let checkpoint = self.checkpoint();
        for bond in bonds {
            if let Err(error) = self.hold_native_recipient_v1(root, bond, 1, now, until, maximum, &[]) {
                self.restore(checkpoint);
                return Err(error);
            }
        }
        Ok(())
    }

    pub(super) fn cancel_panel_recipients_v1(&mut self, root: Hash64) {
        let bonds: Vec<_> =
            self.state.panels.get(&root).into_iter().flat_map(|panel| panel.seats.iter().map(|seat| seat.bond)).collect();
        for bond in bonds {
            self.cancel_native_recipient_v1(root, bond, 1);
        }
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
        self.bond_budget_op(|budget, _, j| {
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
        })
        .expect("claim engine")
        .map_err(|why| budget_refused(row.bond, why))
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
        self.bond_budget_op(|budget, _, j| {
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
        })
        .expect("created")
        .map_err(|why| budget_refused(income.bond, why))
    }

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
            self.create_bond_budget_v1(ctx, &mirror)?;
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
        let mut seeds: Vec<(Hash64, PalwBondKeyV2, u64, PalwBudgetVectorV1)> = Vec::new();
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
            seeds.push((*id, claim.bond, claim.accepted_daa, vector));
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
        self.bond_budget_op(|budget, state, j| {
            for (id, bond, accepted, vector) in seeds {
                let capital = state.bonds.get(&bond).map(|r| r.collateral).unwrap_or(0);
                // Signed old rights are counted even if the first window is over-full.
                budget.reserve(&policy, capital, id, bond, None, accepted, vector, PalwBudgetOriginV1::Legacy, false, j)?;
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
        })
        .expect("created above")
        .map_err(|why: PalwBudgetRefusalV1| PalwStateV2Error::BondBudgetRefused(why.to_string()))?;
        let pending: Vec<_> = self
            .state
            .kernel_route
            .as_ref()
            .map(|route| route.ledger())
            .transpose()
            .map_err(PalwStateV2Error::BondBudgetRefused)?
            .into_iter()
            .flat_map(|ledger| ledger.claims.into_iter())
            .filter(|(_, row)| !row.convicted && !row.life.state.is_terminal())
            .map(|(claim, row)| (Hash64::from_bytes(claim), row.liability_until.unwrap_or(ctx.daa_score)))
            .collect();
        for (claim, until) in pending {
            self.hold_kernel_income_v1(ctx, claim, until)?;
        }
        self.seed_native_income_rights_v1(ctx, mirror)
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
            self.bond_budget_op(|budget, _, j| {
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
            })
            .expect("migration engine")
            .map_err(|why| budget_refused(bond, why))?;
        }
        Ok(())
    }

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
        let checkpoint = self.checkpoint();
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
            self.bond_budget_op(|budget, _, j| {
                budget.reserve_income_v1(&mirror.policy, capital, id, bond, ctx.daa_score, maximum, actual, origin, j)?;
                if let Some(queue) = queue {
                    budget.queue_collected_income_v1(&id, native_queued_income_id_v1(queue, &bond), j)?;
                }
                Ok(())
            })
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
        self.bond_budget_op(|budget, _, j| {
            budget.reserve_source_income_v1(&mirror.policy, capital, id, bond, ctx.daa_score, liability_until, fee, j)
        })
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
        if !budget.claim_row(&id).is_some_and(|row| row.accepted_daa == accepted_daa) {
            return Err(PalwStateV2Error::BondBudgetRefused("source income reservation is missing or has another acceptance".into()));
        }
        let payload = self.state.bond(&bond).ok_or(PalwStateV2Error::MissingBond(bond))?.payout_payload;
        let queue = native_queued_income_id_v1(crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload), &bond);
        self.bond_budget_op(|budget, _, j| {
            budget.queue_source_income_v1(&mirror.policy, &id, liability_until, ctx.daa_score, fee, queue, j)
        })
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
        if !budget.claim_row(&id).is_some_and(|row| row.accepted_daa == accepted_daa) {
            return Err(PalwStateV2Error::BondBudgetRefused("source income reservation is missing or has another acceptance".into()));
        }
        self.bond_budget_op(|budget, _, j| budget.cancel_source_income_v1(&id, j))
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
        self.bond_budget_op(|budget, _, j| {
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
        })
        .expect("kernel claim engine")
        .map_err(|why| budget_refused(row.bond, why))
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
            self.cancel_native_recipient_v1(evidence, bond, 3);
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
        self.bond_budget_op(|budget, _, j| {
            budget.hold_native_income_v1(&mirror.policy, capital, id, bond, now, until, maximum, &[source], false, j)
        })
        .expect("kernel transfer engine")
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
    ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
        let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return Ok(None) };
        let model = if mirror.allocation_at(ctx.daa_score).is_some() {
            // Use the actual native class/line which authenticated the kernel
            // binding. An ambiguous or absent binding has no allocation right.
            let mut classes: Vec<_> = self
                .state
                .kernel_route
                .as_ref()
                .into_iter()
                .flat_map(|route| route.kernel_bindings_v1())
                .filter(|(_, binding)| binding.kernel_class == kernel_class)
                .map(|(class, _)| class)
                .collect();
            classes.sort();
            classes.dedup();
            if classes.len() != 1 {
                return Err(PalwStateV2Error::BondBudgetRefused(
                    "a kernel allocation requires exactly one authenticated native class".into(),
                ));
            }
            Some(self.bond_budget_model_of(&classes[0], self.state.class(&classes[0]).map(|row| &row.artifact_root)))
        } else {
            None
        };
        self.plan_bond_budget_v1(
            ctx,
            bond,
            model,
            PalwBudgetAskV1 { block_units: 0, reward_sompi: reward, final_weight: 0 },
            PalwBudgetOriginV1::KernelRoute,
            0,
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

    pub(super) fn close_terminal_kernel_budgets_v1(&mut self, ledger: &misaka_palw_kernel::ledger::KernelLedgerV1) {
        if self.state.bond_budget.is_none() {
            return;
        }
        for (claim, row) in &ledger.claims {
            if row.convicted || row.life.state.is_terminal() {
                let id = kernel_budget_claim_id_v1(claim);
                self.bond_budget_close_v1(&id);
                if let Some(bond) = self.state.kernel_route.as_ref().and_then(|route| route.bond_key_of(&row.producer)) {
                    self.cancel_native_recipient_v1(Hash64::from_bytes(*claim), bond, 3);
                }
                // The claim row remains public. Financial quota is forgotten
                // only after its window releases; EXEC uses its root allocation.
                self.bond_budget_forget_v1(&id);
            }
        }
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
        // Strict: checked whole before either is taken.
        let budget = self.state.bond_budget.as_ref().expect("checked");
        let short = |dim, need: u128| budget.remaining(claim_id, dim).is_some_and(|left| left < need);
        if short(PalwBudgetDimV1::BlockUnits, PALW_BUDGET_BLOCK_UNIT_V1 as u128) {
            return Err(budget_refused(bond, PalwBudgetRefusalV1::Short { dim: PalwBudgetDimV1::BlockUnits }));
        }
        if short(PalwBudgetDimV1::Reward, carve as u128) {
            return Err(budget_refused(bond, PalwBudgetRefusalV1::Short { dim: PalwBudgetDimV1::Reward }));
        }
        let granted = self
            .bond_budget_op(|budget, _, j| {
                let blocks = budget.consume(claim_id, PalwBudgetDimV1::BlockUnits, PALW_BUDGET_BLOCK_UNIT_V1 as u128, true, j);
                let reward = budget.consume(claim_id, PalwBudgetDimV1::Reward, carve as u128, true, j);
                debug_assert!(matches!((blocks, reward), (Some(Ok(_)), Some(Ok(_)))), "checked whole above");
                let weight =
                    budget.consume(claim_id, PalwBudgetDimV1::FinalWeight, per_quantum, false, j).and_then(Result::ok).unwrap_or(0);
                if last_quantum {
                    budget.close(claim_id, j);
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
            // The pre-object clock creates and validates the book. Without it
            // no ticket may escape into the old uncapped fallback.
            schedule.quanta.clear();
            return true;
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
        if let Some(bond) = self.state.claims.get(claim_id).map(|claim| claim.bond)
            && let Some(held) = self.state.bond_budget.as_ref().and_then(|b| b.claim_row(&native_income_id_v1(*claim_id, &bond, 0)))
        {
            return escrow.min(held.reserved.reward_sompi);
        }
        self.bond_budget_op(|budget, _, j| budget.consume(claim_id, PalwBudgetDimV1::Reward, escrow as u128, false, j))
            .flatten()
            .map(|granted| granted.unwrap_or(0) as u64)
            .unwrap_or(escrow)
    }

    /// **A claim is terminal** (an attempt's Final, a void, a conviction): no further consumption; nothing returns to the window.
    pub(super) fn bond_budget_close_v1(&mut self, claim_id: &Hash64) {
        self.bond_budget_op(|budget, _, j| budget.close(claim_id, j));
    }

    /// **A claim left the state** (retirement): its row leaves once it is out of the window too.
    pub(super) fn bond_budget_forget_v1(&mut self, claim_id: &Hash64) {
        self.bond_budget_op(|budget, _, j| budget.forget(claim_id, j));
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
        builder
            .bond_budget_op(|budget, _, j| budget.confirm_native_income_payment_v1(&mirror.policy, &id, now, j))
            .expect("created")
            .map_err(|why| PalwStateV2Error::BondBudgetRefused(why.to_string()))?;
    }
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
