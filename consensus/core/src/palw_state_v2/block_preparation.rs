//! Shared deterministic pre-object work. Each caller preserves its historical
//! migration placement; this extraction deliberately does not normalize journal
//! order or move the caller-specific material-retirement rearm.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ArtifactMigrationOrderV1 { BeforeClassRetarget, AfterMeshSweep }

pub(super) fn prepare_block_objects_v1(
    builder: &mut TransitionBuilder<'_>, parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2, facts: PalwBlockFactsV1<'_>, rules: PalwRulesAtBlockV1<'_>,
    migration: ArtifactMigrationOrderV1,
) -> Result<(), PalwStateV2Error> {
    for claim_id in builder.state.pending_payouts.keys().copied().take(PALW_V2_MAX_PAYOUTS_PER_BLOCK).collect::<Vec<_>>() {
        builder.write_payout(claim_id, None)?;
    }
    // Dedicated checker fee/refund ACK reads only this exact consumed prefix;
    // principal refunds do not depend on an R engine being active.
    palw_source_verifier_economics_fold_v2::acknowledge_consumed_prefix(builder,ctx)?;
    // 1c′. ADR-0089 Decision 6: the settlement list the parent decided was carried and settled
    //      by THIS block's EVM step; it leaves the state here, before this block's own is written.
    for seq in builder.state.evm_settlements.keys().copied().collect::<Vec<_>>() {
        builder.write_evm_settlement(seq, None)?;
    }

    // 1c″. Lane BUDGET (ADR-0176 / ADR-0177): the bond budget's clock — the engine created at the fence's first block (the live old
    //      claims seeded), every reservation whose `reuse_not_before` has come released (the ONLY release), the allocation epoch rolled
    //      and this block's carve accrued; before the execution lane's span rotation (whose Round draw reads the bonds' remaining
    //      rights, readiness §3e), the sweeps and the objects. Mirrored in `palw_v2_pre_object_base_v1`. A no-op below
    //      `palw_bond_budget_v1`, which no network can arm.
    palw_bond_budget_fold_v1::tick_bond_budget_v1(builder, ctx)?;
    // RFC-0016 (dormant): the reissue snapshot at φ, the pending queue's TTL voids and dequeues, the heartbeat-dependent counter.
    palw_rfc0016_fold_v1::tick_rfc0016_v1(builder, ctx)?;
    for income in facts.round_fee_incomes() {
        builder.reserve_round_income_v1(ctx, *income)?;
    }
    // G14 lane escrow-rfc08 (RFC-0035 A): the funded-escrow outpoints UTXO acceptance spent under a qualified plan settle here,
    // once, against the parent's rooted rows — journaled in this block's delta beside its UTXO diff. Empty below the fence.
    for settlement in facts.funded_escrow_settlements() {
        builder.settle_funded_escrow_v1(settlement)?;
    }
    // G14 r5 (lane r5-escrow-settlement): then the escrow clock — liabilities resolved by the parent state's verdicts, discharges
    // or passed hard deadlines clear (releasing the claim's bond reservation with them), and settled rows past their retention
    // retire. Reads only rooted state and `extras.funded_escrow_retention_daa`; a no-op wherever no row exists.
    builder.sweep_funded_escrows_v1(ctx.daa_score)?;
    // 1d. ADR-0125: the execution lane's span boundary — before the sweeps, so an attempt finalized
    //     by this block is a credit in THIS block's span, never in the one its finals just closed.
    if let Some(lane) = rules.round_lane() {
        builder.rotate_round_lane(ctx, lane.schedule_span_daa)?;
    }
    // 1e. ADR-0165: the floor state's TIME step — a Probe that has run its slots and a Normal whose last BLUE REAL attempt is
    //     too old expire here, before the sweeps and before any attempt, so own and merged attempts read one state.
    builder.advance_floor_state(ctx.daa_score)?;

    // 2. Deadline sweeps — everything strictly past is resolved before this block says anything.
    //    (A deadline equal to ctx.daa_score is still actionable by this block's objects.) Claims
    //    first, then the court backstop: the order is fixed, and fixed IS the requirement.
    // EXEC expiry is checked before REAL Final deadlines, including after a DAA jump.
    // Mirrored in the pre-object base: an expired ready root refunds escrow instead of settling.
    palw_exec_v2_fold::sweep_expired_roots_v2(builder, ctx)?;
    palw_bond_budget_fold_v1::sweep_paid_consumers_v1(builder, ctx)?;
    sweep_deadlines(builder, ctx)?;
    // ADR-0152 R-4 (S-7): the reveal windows this block is past close here, after the claim sweep
    // (whose convictions, if any, open windows that run from this block) and before the court
    // backstop (M3's `sweep_da_sessions` lands between the two, ADR §3.3 row 2). Mirrored in
    // `palw_v2_pre_object_base_v1`. A no-op below `Params::palw_rcore_plus`.
    sweep_reward_reveals(builder, ctx)?;
    // ADR-0152 DA-7 (M3): the data-availability sessions whose disclose window closed with a unit
    // unanswered default, in `(deadline, claim, accuser)` order — after the claim sweep (V-4's step 2),
    // before the courts.
    sweep_da_sessions(builder, ctx)?;
    palw_legacy_answer_chunks_fold_v2::sweep(builder, ctx)?;
    palw_terminal_carriage_fold_v1::sweep(builder, ctx)?;
    palw_staged_court_fold_v1::sweep(builder, ctx)?;
    // Lane LG14-A: every dispute reservation whose claim's hard deadline is behind this block lapses, right after the DA sweep.
    palw_legacy_public_filer_fold_v1::sweep_legacy_disputes(builder, ctx)?;
    // A declared close is swept at ITS OWN window before the session's backstop is read (audit
    // H-2c). Before the backstop, because the two can coincide — `assembly_deadline_daa` is never
    // above `session.deadline_daa` — and the group's window is the tighter statement: the party
    // that announced a close and did not file it defaulted when its own window ran out, not when
    // the whole prosecution did.
    sweep_court_close_deadlines(builder, ctx)?;
    sweep_court_deadlines(builder, ctx)?;
    palw_bond_budget_fold_v1::sweep_paid_consumers_v1(builder, ctx)?;
    // 2a′. Lane F2-lock (post-launch, `Params::palw_final_lock_life_retro`): the crossing block re-dates
    //      the long post-`Final` seat locks to `max(F + 1,000, H)`, once — after the sweeps (a claim this
    //      block finalizes is dated past `H` already) and before anything reads a bond's room. Mirrored in
    //      `palw_v2_pre_object_base_v1` at the same place. A no-op off the crossing block.
    apply_final_lock_life_retro(builder, parent, ctx)?;

    // 2b. Per-class retarget (PR-09): crossing a global epoch boundary closes the previous
    //     epoch as one span and retargets every share-bearing, unfrozen class against it. Runs
    //     between the sweeps and the objects — a fixed slot, because fixed IS the requirement.
    if migration == ArtifactMigrationOrderV1::BeforeClassRetarget {
        apply_artifact_root_ownership_migration(builder)?;
    }
    apply_class_retargets(builder, parent, ctx)?;
    // ADR-0145 §6 / ADR-0148: the epoch boundary sweeps the paid-prompt rows whose retention ran
    // out — before this block's objects, so no commitment here is priced against one.
    sweep_fp_prompt_rows(builder, parent, ctx)?;
    // 2026-09-24 DoS audit #12 (a): the same boundary drops the panel locks and liability rows
    // past their evidence horizon (past `palw_audit_2026_09_23` only).
    sweep_panel_obligations(builder, parent, ctx)?;
    // ADR-0152 R-3 (S-7, V3S-11): the reporter commitments past their TTL that no open reward can
    // still be won by — before this block's objects, so a reveal here never meets a commitment
    // this block is about to prune. Mirrored in `palw_v2_pre_object_base_v1`. A no-op below
    // `Params::palw_rcore_plus`.
    sweep_reporter_commitments(builder, ctx)?;
    // RFC-0007: the vertex rounds past their evidence window, before this block's objects (mirrored in
    // `palw_v2_pre_object_base_v1`). A no-op on a chain with no row.
    palw_vertex_fold_v1::sweep_vertex_rounds_v1(builder, ctx)?;
    palw_mesh_fold_v1::sweep_mesh_v1(builder, ctx)?;
    // ADR-0143 Decision 6: the one-time canonicalisation, before any object of this block is
    // folded — a block that crosses the fence AND founds a line must see the index the migration
    // built, not the empty one it found.
    if migration == ArtifactMigrationOrderV1::AfterMeshSweep {
        apply_artifact_root_ownership_migration(builder)?;
    }
    // ADR-0137 (shadow): the work target's step, beside the class DAA's, off the same closed epoch.
    apply_work_target_shadow(builder, parent, ctx)?;
    // ADR-0137 / ADR-0132 S: the chain's own work target, past its fence.
    apply_work_target(builder, parent, ctx)?;

    // 2c. ADR-0054's share-raise path, on the same closed span the retarget just measured: a class
    //     that produced every block its budget allowed takes a step of cadence from the floor, and
    //     one that produced nothing gives a step back. It runs HERE — after the retarget, which
    //     reads the same census, and before the objects and `ensure_epoch_budgets`, so the new
    //     epoch's budgets are derived from the table this leaves behind rather than from the one
    //     the closed epoch had. Off entirely when `class_growth_permille` is zero.
    apply_class_share_growth(builder, parent, ctx)?;
    // 2d. Reclamation (this line's ADR Decision 5), at the same boundary and off the same census.
    //     AFTER the growth step, because a class the growth rule just decayed to nothing and a
    //     class that has been silent for `reclaim_epochs` are different facts, and only the second
    //     one takes a class out of the table.
    apply_class_reclamation(builder, parent, ctx)?;
    // 2e. RFC-0004 (spec 17 §17.5.2): the improvement protocol's sweep — the bounded retirement, every
    //     due line's transitions and vesting, the bounded earnings flush — before this block's objects,
    //     so a candidate or a case here meets the epoch at this DAA. Mirrored in
    //     `palw_v2_pre_object_base_v1`. A no-op below `palw_improvement_v1`. Then the material lane's
    //     sweep: artifact deadlines, expiries and the bounded retirement of decided epochs' material.
    palw_improve_fold_v1::advance_improvement_v1(builder, ctx)?;
    palw_improve_material_fold_v1::settle_improvement_material_v1(builder, ctx)?;
    // 2f. **RFC-0010: the permissionless Panel's engine against the PRE-OBJECT base** — validated terminal outcomes release,
    //     claims past the checkpoint depth seal against the PARENT checkpoint, closed contribution windows make claims ready or
    //     end them `BeaconUnavailable`, and due assignments (first draws and receipt-timeout retries) bind against the
    //     one-ledger headroom as it stands before anything this block carries. Mirrored in `palw_v2_pre_object_base_v1`. A no-op
    //     below `palw_permissionless_panel_v1`.
    palw_panel_v3_fold_v1::advance_v1(builder, parent, ctx)?;
    palw_monetary_material_fold_v1::tick(builder, ctx)?;

    Ok(())
}
