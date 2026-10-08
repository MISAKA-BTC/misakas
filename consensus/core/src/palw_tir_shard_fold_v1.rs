//! **RFC-0006: the fold's half of layer-sharded panels** (armed on testnet-12 at DAA 5,300 under `Params::palw_tir_shard_v1`; dormant on every other preset). A child module of
//! `palw_state_v2`, as the improvement lanes' fold modules are, so it reads the builder and the state's tables directly.
//!
//! * [`apply_plan_declared_v1`] — `TirShardPlanDeclared` (tag 91): the registrant's plan, checked against the class's own
//!   program (decision 8: `S_L` between the fewest shards that fit the network's seat budget and twice that), the cell
//!   shares derived and stored;
//! * [`bind_record_v1`] — what a `PanelBound` writes for a claim whose panel was drawn per shard: the plan frozen, the
//!   outsider flag, each drawn seat's share of the work;
//! * [`apply_part_v1`] — `TirShardReceiptLicensed` (tag 92): one shard's part. Every structural fact is re-derived from the
//!   fold's own state (the sync walk folds the same object without the acceptance layer); the shard's verdict is
//!   `palw_tir_shard_part_verdict_v1`, the one function the acceptance layer and the assembler call. A licensed part locks
//!   each counted signer at `lock × share` (decision 6), credits the seats, lands its cells' counts, and the part that
//!   completes the plan licenses the claim with `basis_k` recounted over cells (RFC §4.4);
//! * [`apply_readiness_v1`] — `TirSeatReadinessProved` (tag 93): a possession proof over ONE shard's rows, recorded under
//!   the shard's readiness class — what makes a bond a candidate of the shard's draw and counts in its `ready_eff`;
//! * [`final_legs_v1`] — the pay at `Final`: the panel's pool divided by the drawn seats' shares (decision 6).

use super::*;
use crate::palw_tir_shard_v1 as rules;

/// The program and layer partition of an IR class under `s_l` shards, from the class's registered row.
fn geometry_v1(
    state: &PalwChainStateV2,
    class_id: &Hash64,
    s_l: u16,
) -> Result<(misaka_palw_tir::TirProgramV1, u32, Vec<std::ops::Range<usize>>), PalwStateV2Error> {
    let refused = |why: String| PalwStateV2Error::TirShardRefused(why);
    let record = state.tir_classes.get(class_id).ok_or_else(|| refused(format!("class {class_id} is not an IR class")))?;
    let program =
        misaka_palw_tir::TirProgramV1::decode_canonical(&record.program).map_err(|e| refused(format!("the class's program: {e}")))?;
    let max_context = record.facts.max_context;
    let weights = rules::palw_tir_shard_weights_v1(&program, max_context);
    let parts = rules::palw_tir_shard_partition_v1(&weights, s_l)
        .ok_or_else(|| refused(format!("a plan of {s_l} shards over {} layers", weights.layer_bytes.len())))?;
    Ok((program, max_context, parts))
}

/// **`TirShardPlanDeclared`** (tag 91). The registrant's signature is the acceptance layer's; here: the fence, a registered
/// IR class with a registrant (a genesis class has none and is not sharded), declared once, and a shape the class's own
/// program and the network's seat budget accept. The cell shares are derived here, once, and stored.
pub(super) fn apply_plan_declared_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    class_id: &Hash64,
    s_l: u16,
    s_p: u16,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.tir_shard_active_at(ctx.daa_score) {
        return Err(PalwStateV2Error::TirShardDormant);
    }
    let refused = |why: String| PalwStateV2Error::TirShardRefused(why);
    let record = builder.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?;
    if record.registrant_bond.is_none() {
        return Err(PalwStateV2Error::ShardPlanFromGenesisClass(*class_id));
    }
    if builder.state.tir_shard_plans.contains_key(class_id) {
        return Err(PalwStateV2Error::ShardPlanAlreadyDeclared(*class_id));
    }
    let tir = builder.state.tir_classes.get(class_id).ok_or_else(|| refused(format!("class {class_id} is not an IR class")))?;
    let program =
        misaka_palw_tir::TirProgramV1::decode_canonical(&tir.program).map_err(|e| refused(format!("the class's program: {e}")))?;
    let max_context = tir.facts.max_context;
    // **Lane PA, S-1 / B-F5 (`palw_audit_1004_v1`)**: the shape that needs no derivation first, then the linear minimum — the old
    // quadratic search ran before any refusal, on a signature with no nonce, so a refused plan was a free replayable CPU burn.
    let audit_1004 = builder.params.audit_1004_active_at(ctx.daa_score);
    if audit_1004 {
        rules::palw_tir_shard_plan_prelim_v1(s_l, s_p, program.schedule.layers.len()).map_err(|e| refused(e.to_string()))?;
    }
    let weights = rules::palw_tir_shard_weights_v1(&program, max_context);
    let min = if audit_1004 {
        rules::palw_tir_shard_min_shards_fast_v1(&weights, rules::PALW_TIR_SHARD_SEAT_BUDGET_BYTES_V1)
    } else {
        rules::palw_tir_shard_min_shards_v1(&weights, rules::PALW_TIR_SHARD_SEAT_BUDGET_BYTES_V1)
    };
    rules::palw_tir_shard_plan_shape_v1(s_l, s_p, weights.layer_bytes.len(), min).map_err(|e| refused(e.to_string()))?;
    let (_, cell_permille) =
        rules::palw_tir_shard_cell_permille_v1(&program, max_context, s_l, s_p, builder.params.tir_fence2_active_at(ctx.daa_score))
            .map_err(|e| refused(e.to_string()))?;
    builder.write_tir_shard_plan(*class_id, Some(rules::PalwTirShardPlanV1 { s_l, s_p, declared_daa: ctx.daa_score, cell_permille }));
    Ok(())
}

/// **Which claims license by parts**: those whose panel bound with a per-shard record. Answers `false` on every network
/// that never armed the fence — no record exists there — so the whole-object doors are byte-identical.
pub(super) fn claim_licenses_tir_parts_v1(state: &PalwChainStateV2, claim_id: &Hash64) -> bool {
    state.tir_shard_claims.contains_key(claim_id)
}

/// The class's plan, if a claim of it bound NOW would be drawn per shard: past the fence, the class declared one.
pub(super) fn plan_of_class_v1<'s>(
    state: &'s PalwChainStateV2,
    params: &PalwStateParamsV2,
    class_id: &Hash64,
    daa_score: u64,
) -> Option<&'s rules::PalwTirShardPlanV1> {
    params.tir_shard_active_at(daa_score).then(|| state.tir_shard_plans.get(class_id)).flatten()
}

/// **What a `PanelBound` writes for a claim drawn per shard** (called after the panel record is written): the plan frozen at
/// the bind, whether the claim is outsider-judged, and each drawn seat's share of the claim's work — fixed here, so a plan
/// that is declared later cannot reprice a claim already bound. A panel whose length is not the plan's stratified shape
/// is a flat panel (bound before the plan, or of a class that declared none) and writes nothing.
pub(super) fn bind_record_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
    anchor: &Hash64,
    seats: &[PalwPanelSeatV2],
) -> Result<(), PalwStateV2Error> {
    let Some(plan) = plan_of_class_v1(&builder.state, builder.params, &claim.class_id, ctx.daa_score).cloned() else { return Ok(()) };
    let outsider = palw_claim_is_outsider_judged_v1(&builder.state, claim, builder.extras.admission_independence_daa);
    if seats.len() != usize::from(plan.s_l) * usize::from(rules::palw_tir_panel_stride_v1(outsider)) {
        return Ok(());
    }
    let drawn = rules::palw_tir_shard_drawn_permille_v1(&plan, anchor, claim_id, outsider);
    let record = rules::PalwTirShardClaimV1::bound(plan.s_l, plan.s_p, outsider, drawn)
        .ok_or(PalwStateV2Error::TirShardRefused("a plan of no shards".into()))?;
    builder.write_tir_shard_claim(*claim_id, Some(record));
    Ok(())
}

/// **`TirShardReceiptLicensed`** (tag 92): one shard's part of a claim drawn per shard.
///
/// Refused by name: the fence, a claim that is not `PanelBound` or was not drawn per shard, a shard out of range or already
/// licensed, a receipt that is not this claim's / this shard's / a seat of this shard's slice / once, a `Valid` whose mask is
/// not the seat's assignment, a `Sampled`, and a shard whose cells are not each attested by two class seats with the outsider's
/// `Valid`. A part whose counted signers cannot post their (scaled) lock is INERT — nothing moves, the shard stays open
/// for a backed part, else the receipt window voids the claim. A backed part locks, credits, lands its counts; the part
/// that completes the plan licenses the claim.
pub(super) fn apply_part_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    part: &rules::PalwTirShardPartV1,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.tir_shard_active_at(ctx.daa_score) {
        return Err(PalwStateV2Error::TirShardDormant);
    }
    let claim_id = part.claim;
    let shard = part.shard;
    let refused = |why: String| PalwStateV2Error::ShardPartRefused { claim: claim_id, shard: u32::from(shard), why };
    let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
    let PalwClaimPhaseV2::PanelBound { .. } = claim.phase else {
        return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "TirShardReceiptLicensed" });
    };
    let record = builder.state.tir_shard_claims.get(&claim_id).cloned().ok_or(PalwStateV2Error::NotLicensedByParts(claim_id))?;
    if shard >= record.s_l {
        return Err(PalwStateV2Error::ShardIndexOutOfRange { shard: u32::from(shard), count: u32::from(record.s_l) });
    }
    if record.progress.is_licensed(u32::from(shard)) {
        return Err(PalwStateV2Error::ShardAlreadyLicensed { claim: claim_id, shard: u32::from(shard) });
    }
    let panel = builder.state.panels.get(&claim_id).cloned().ok_or(PalwStateV2Error::EmptyPanel)?;
    let slice = rules::palw_tir_panel_shard_slice_v1(&panel.seats, record.s_l, record.outsider, shard)
        .ok_or(PalwStateV2Error::NotLicensedByParts(claim_id))?
        .to_vec();
    if part.receipts.is_empty() || part.receipts.len() > rules::PALW_TIR_SHARD_PART_MAX_RECEIPTS_V1 {
        return Err(refused(format!("{} receipts", part.receipts.len())));
    }
    let mut counted: Vec<(PalwBondKeyV2, crate::palw_panel_v2::PalwReceiptVerdictV2, crate::palw_verification_v2::PalwSegmentMaskV2)> =
        Vec::new();
    for signed in &part.receipts {
        let receipt = &signed.receipt;
        if receipt.claim != claim_id {
            return Err(refused(format!("a receipt names claim {}", receipt.claim)));
        }
        if signed.shard != shard {
            return Err(refused(format!("a receipt is for shard {}", signed.shard)));
        }
        if !slice.iter().any(|seat| seat.bond == receipt.seat_bond) {
            return Err(refused(format!("{:?} is not a seat of this shard", receipt.seat_bond)));
        }
        if counted.iter().any(|(bond, _, _)| *bond == receipt.seat_bond) {
            return Err(refused(format!("seat {:?} answered twice", receipt.seat_bond)));
        }
        if matches!(receipt.verdict, crate::palw_panel_v2::PalwReceiptVerdictV2::Sampled) {
            return Err(refused("a Sampled receipt counts nowhere".into()));
        }
        counted.push((receipt.seat_bond, receipt.verdict, signed.segments));
    }
    let verdict =
        rules::palw_tir_shard_part_verdict_v1(&slice, &panel.anchor, &claim_id, shard, record.s_p, record.outsider, &counted);
    let (signers, cell_counts) = match verdict {
        rules::PalwTirShardPartVerdictV1::Licensed { signers, cell_counts } => (signers, cell_counts),
        rules::PalwTirShardPartVerdictV1::Short(why) => {
            return Err(PalwStateV2Error::TirShardPartShort { claim: claim_id, shard: u32::from(shard), why });
        }
    };
    let receipts_v2: Vec<crate::palw_panel_v2::PalwSeatReceiptV2> = part.receipts.iter().map(|r| r.receipt.clone()).collect();
    // The price a counted signer posts: `lock_{k'}` of the claim, `k' = max(2, this shard's weakest cell)`, scaled by the
    // signer's share of the claim's work (decision 6) and floored. Priced ONCE here, from the same reads
    // `rcore_backed_set` prices a licence with, so a lock written now is the lock the licence would post.
    let shard_k = rules::palw_tir_shard_basis_k_v1(&cell_counts).max(PALW_RCORE_FINAL_BASIS_K_V1);
    let g_res = builder.read().rcore_g_res(&claim_id, &claim);
    let buyback = builder.read().rcore_buyback_bound(&claim_id, &claim);
    let step = crate::palw_audit_door_v1::palw_capacity_seat_step_v1(builder.params, &claim);
    let full_lock = crate::palw_aggregate_liability_v1::palw_seat_lock_v2(
        palw_rcore_lock_v1(g_res, claim.escrowed_reward, buyback, shard_k),
        step,
    );
    let plan_cells = builder.state.tir_shard_plans.get(&claim.class_id).map(|p| p.cell_permille.clone()).unwrap_or_default();
    let price_of = |bond: &PalwBondKeyV2, mask: crate::palw_verification_v2::PalwSegmentMaskV2| -> u128 {
        // The seat's share of the work: the class seats by their assigned cells, the outsider by the whole shard.
        let share = rules::palw_tir_cells_share_permille_v1(&plan_cells, record.s_p, shard, mask);
        let _ = bond;
        rules::palw_tir_shard_lock_v1(full_lock, share)
    };
    // SR-6 on parts: a counted `Valid` whose signer cannot post its price (or whose bond is frozen) backs nothing, and a
    // part with such a signer is inert.
    let now = ctx.daa_score;
    for (bond, mask) in &signers {
        if crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(&builder.state, bond) {
            return Ok(());
        }
        let price = price_of(bond, *mask);
        // A bond that sits in several shards posts the shards' prices one after another: the first lock is covered by its duty,
        // each later shard's price is an increment that needs its own room.
        let held = builder.state.slashable_locks.get(&(*bond, claim_id)).is_some();
        let owed = if held { price } else { price.saturating_sub(palw_seat_duty_of_v1(&builder.state, &claim_id, bond)) };
        if owed > builder.gate_room(bond, now, PalwRcoreGateV1::Work) {
            return Ok(());
        }
    }
    let seat_verdicts = palw_seat_verdicts_of_v2(&receipts_v2);
    builder.slash_dissenting_seats(&claim_id, &claim, &seat_verdicts, true)?;
    builder.credit_seat_receipts(claim_id, &receipts_v2, now);
    for (bond, mask) in &signers {
        let price = price_of(bond, *mask);
        match builder.state.slashable_locks.get(&(*bond, claim_id)).copied() {
            // A second shard of the same bond: the lock grows by this shard's price and no longer names one cell's mask
            // (`segments: 0` is the unscoped record: the seat is liable wherever on the claim it attested).
            Some(lock) => builder.write_slashable_lock(
                (*bond, claim_id),
                Some(crate::palw_panel_var_v1::PalwSlashableLockV1 {
                    amount: lock.amount.saturating_add(price),
                    attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                    segments: 0,
                    ..lock
                }),
            ),
            None => builder.lock_valid_seat_rcore(*bond, claim_id, price, *mask, record.s_p.max(1), now),
        }
    }
    // Land the part: the shard's cell counts, the counted signers, the abstention latch, the progress.
    let mut next = record.clone();
    for (j, count) in cell_counts.iter().enumerate() {
        if let Some(slot) = next.cell_counts.get_mut(usize::from(shard) * usize::from(record.s_p) + j) {
            *slot = *count;
        }
    }
    next.counted.extend(signers.iter().map(|(bond, mask)| (*bond, shard, *mask)));
    next.unserved_seen |=
        part.receipts.iter().any(|r| !matches!(r.receipt.verdict, crate::palw_panel_v2::PalwReceiptVerdictV2::Valid));
    next.progress.mark(u32::from(shard)).map_err(|e| refused(e.to_string()))?;
    if !next.progress.is_complete() {
        builder.write_tir_shard_claim(claim_id, Some(next));
        return Ok(());
    }
    // The last part: the claim licenses, `basis_k` recounted over cells (RFC §4.4), the door recorded.
    let basis_k = next.basis_k();
    let door = crate::palw_economic_safety_v1::PalwLicenceDoorTagV1::ShardPart {
        quorum_per_shard: rules::PALW_TIR_SHARD_ATTESTERS_PER_CELL_V1 + u16::from(record.outsider),
    };
    let mut staged = claim.clone();
    staged.rcore = PalwClaimRcoreV1 {
        licence_door: Some(door),
        basis_k,
        escrow_released: false,
        // A served bit is a seat of a panel of at most 32; a sharded panel has more and releases its escrow at `Final`.
        served_mask: 0,
        unserved_seen: next.unserved_seen,
        g_res_sompi: if claim.rcore.licence_door.is_some() { claim.rcore.g_res_sompi } else { g_res },
    };
    staged.rcore.escrow_released = palw_rcore_release_due_v1(builder.params, &builder.state, &claim_id, &staged);
    builder.write_tir_shard_claim(claim_id, Some(next));
    builder.license_claim(claim_id, staged, now)
}

/// **`TirSeatReadinessProved`** (tag 93): a possession proof over one shard's rows. The multiproof reconstructs the class's
/// registered artifact root, the leaves it opened are the carrier-bounded prefix of the challenge this `(class, shard, bond,
/// span)` draws from THE SHARD'S OWN rows, and the span is the current one or one the landing window admits. The row it
/// writes is under the shard's readiness class ([`rules::palw_tir_shard_ready_class_v1`]).
pub(super) fn apply_readiness_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    bond: &PalwBondKeyV2,
    class_id: &Hash64,
    shard: u16,
    span: u64,
    proof: &crate::palw_artifact::PalwArtifactMultiproofV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_model_registry_v1 as registry;
    if !builder.params.tir_shard_active_at(ctx.daa_score) {
        return Err(PalwStateV2Error::TirShardDormant);
    }
    let refused = |why: String| PalwStateV2Error::ReadinessProofRefused(why);
    let Some(fold) = builder.model_registry_fold() else { return Err(PalwStateV2Error::ModelRegistryDormant) };
    let span_daa = fold.span_daa;
    let record = builder.state.bonds.get(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?;
    if !matches!(record.status, PalwBondStatusV2::Active) {
        return Err(PalwStateV2Error::BondNotActive(*bond));
    }
    let class = builder.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?;
    let plan = builder
        .state
        .tir_shard_plans
        .get(class_id)
        .ok_or_else(|| refused(format!("class {class_id} declared no shard plan")))?
        .clone();
    if shard >= plan.s_l {
        return Err(refused(format!("shard {shard} of a plan of {}", plan.s_l)));
    }
    if proof.opened.len() > registry::PALW_READINESS_V2_CHUNKS_V1 as usize
        || proof.operand_bytes() > registry::PALW_READINESS_V2_OPERAND_MAX_BYTES_V1
    {
        return Err(refused("the proof opens more than one carrier holds".into()));
    }
    crate::palw_artifact::verify_artifact_multiproof_v1(proof, class.artifact_root).map_err(|e| refused(format!("{e}")))?;
    let span_now = crate::palw_execution_lane_v1::palw_execution_span_v1(ctx.daa_score, span_daa);
    let landing = registry::palw_readiness_landing_spans_v1(span_daa);
    if span > span_now || span_now - span > landing {
        return Err(refused(format!("the proof names span {span} at span {span_now} (a proof lands within {landing} spans)")));
    }
    let ready_class = rules::palw_tir_shard_ready_class_v1(class_id, plan.s_l, shard);
    builder.refuse_readiness_proof_not_newer_v1(bond, &ready_class, span)?;
    // The shard's own rows: the challenge draws from exactly the leaves a shard seat holds.
    let (program, _, parts) = geometry_v1(&builder.state, class_id, plan.s_l)?;
    let layers = parts.get(usize::from(shard)).cloned().ok_or_else(|| refused("a shard of the partition".into()))?;
    let ranges = rules::palw_tir_shard_inventory_ranges_v1(&program, layers, shard == 0, shard + 1 == plan.s_l);
    let draw = rules::palw_tir_shard_readiness_leaves_v1(
        class_id,
        plan.s_l,
        shard,
        bond,
        span,
        &ranges,
        registry::PALW_READINESS_V2_CHUNKS_V1 as usize,
    );
    let opened: Vec<(u32, usize)> = proof.opened.iter().map(|(index, operand)| (*index, operand.bytes.len())).collect();
    registry::palw_readiness_v2_opening_is_the_challenge_v1(&draw, &opened).map_err(refused)?;
    builder.write_seat_readiness(
        (*bond, ready_class),
        Some(registry::PalwSeatReadinessRowV1 {
            proved_daa: span.saturating_mul(span_daa.max(1)),
            proved_span: span,
            leaf_index: draw.first().copied().unwrap_or(0),
            proof_version: 2,
            chunks: proof.opened.len().min(u32::MAX as usize) as u32,
        }),
    );
    Ok(())
}

/// **The pay of a `Final` claim drawn per shard** (decision 6): each credited seat is paid its share of the panel's pool by
/// the work it vouched for, the producer the exact rest, the reserve what no credited seat is owed. `None` for a claim with no
/// per-shard record (every other claim pays as it always did).
pub(super) fn final_legs_v1(
    state: &PalwChainStateV2,
    claim_id: &Hash64,
    reward: u64,
    pool_permille: u16,
    credited: &[PalwBondKeyV2],
) -> Option<(u64, Vec<(PalwBondKeyV2, u64)>, u64)> {
    let record = state.tir_shard_claims.get(claim_id)?;
    let panel = state.panels.get(claim_id)?;
    if panel.seats.len() != record.drawn_permille.len() {
        return None;
    }
    let drawn: Vec<u32> = record.drawn_permille.clone();
    // A seat is paid for the shard it vouched for: `(bond, shard)` is in the claim's counted signers (a bond that sits in
    // several shards is credited, and paid, shard by shard). `credited` is the duty row's credit, the same fact per bond.
    let stride = usize::from(rules::palw_tir_panel_stride_v1(record.outsider));
    let flags: Vec<bool> = panel
        .seats
        .iter()
        .enumerate()
        .map(|(i, seat)| {
            let shard = (i / stride) as u16;
            credited.contains(&seat.bond) && record.counted.iter().any(|(bond, s, _)| *bond == seat.bond && *s == shard)
        })
        .collect();
    let split = rules::palw_tir_shard_split_v1(reward, pool_permille, &drawn, &flags);
    let legs =
        panel.seats.iter().zip(&split.paid).filter(|(_, amount)| **amount > 0).map(|(seat, amount)| (seat.bond, *amount)).collect();
    Some((split.producer, legs, split.reserve))
}
