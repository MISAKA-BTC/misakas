#![allow(dead_code, unused_imports, unused_variables)]
//! **DoS repro 1 — a free-prompt Final mints receipt-lane block rights the seat lock does not
//! price and a conviction does not revoke.**
//!
//! Finding: `fp-final-receipt-rights-unpriced-unforfeitable`. Every number below comes out of the
//! shipped testnet-12 `Params`, its own genesis fold, and the production functions the fold itself
//! calls — nothing is a fixture. The claim is driven to `Final` through the REAL
//! `apply_palw_transition_v7` (via `dos_l5_common::fold`), a receipt block is spent through the REAL
//! `apply_receipt_spend`, a REAL `PanelFalseValid` conviction is folded, and the amplification is
//! priced with the same standalone functions the fold's seat-lock and receipt-lottery code use.
//!
//! Reachability note (why the class is BASE-0 and the family is injected): on t12 every FP claim is
//! priced in COMPUTE (`palw_canonical_work_daa = Some(0)`), so a commitment needs the class's
//! `fp_work_profile` published by a `ClassLaneCertified`, which in turn needs a chain-certified
//! FreePrompt family covering the class. The floor (BASE-0) is genesis-Active and its FreePrompt
//! family is the one this build pins in `palw_rc_fp_certified_families_v1()`; on the live network a
//! permissionless `FamilyCertified` drill writes it. We inject that one pinned family through the
//! carriage (a real family object, not a reimplementation) and then everything measured is the real
//! fold. The held model classes in `fp_certified_classes` reach the same door only once seven seats
//! prove readiness against the real artifact — heavier, and the economics are identical, only larger.
//!
//! Run: cargo test -p kaspa-consensus-core --test dos_repro_1_fp_final_receipt_rights_unpriced_unforfe -- --nocapture --test-threads=2

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_permit_value_sompi_v1, palw_realizable_before_maturity_v1, palw_seat_lock_required_v2,
};
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXEC_MAX_QUANTA_PER_SPAN_V1, PALW_EXECUTION_QUANTUM_V1, palw_execution_quantum_count_v1};
use kaspa_consensus_core::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1;
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_panel_var_v1::{PalwClaimFraudFactsV1, palw_max_fraud_gain_v1};
use kaspa_consensus_core::palw_reward_v2::{PalwRewardParamsV2, palw_reward_carve_v2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwCertifiedFamilyStateV2, PalwCertifiedLaneV1, PalwChainStateV2,
    PalwClaimPhaseV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwStateCarriageV2, PalwStateV2Error, PalwTransitionExtrasV1,
    apply_palw_transition_v7,
};
use kaspa_consensus_core::palw_work_target_v1::palw_work_floor_v1;

const MSK: f64 = 1e8;
fn to_msk(s: u128) -> f64 { s as f64 / MSK }

/// The attacker's bond: 1,000 MSK, or the registration floor where that is higher (the producer
/// floor — 13,000 MSK on testnet-12's regenesis params, `palw_bond_registration_floor_v1`).
fn attacker_collateral(p: &Params) -> u64 {
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &p.palw_consensus_mode else { unreachable!() };
    100_000_000_000u64.max(kaspa_consensus_core::palw_state_v2::palw_bond_registration_floor_v1(b.state.min_collateral_sompi(), true))
}
/// The fold's `mul_div_u128` is pub(crate); the operands here stay far below 2^128.
fn mul_div_u128(a: u128, b: u128, d: u128) -> u128 { a * b / d }

/// **The processor's t12 FP extras.** `dos_l5_common::extras` leaves `fp_derived_work_daa = None`
/// and `fp_da_pins_active = false`; the shipped `palw_transition_extras_for` (processor.rs:8351)
/// resolves both from the armed t12 fences, and the FP compute lane is inert without them. Copied
/// from `dos_repro_2`, the other FP-lane repro that folds t12.
fn t12_extras(p: &Params, daa: u64) -> PalwTransitionExtrasV1 {
    let mut e = extras(p, daa);
    e.fp_derived_work_daa = p.palw_fp_derived_work_fence().map(|f| f.daa_score());
    e.fp_da_pins_active = p.palw_fp_da_pins_fence().is_some_and(|f| f.is_active(daa));
    e
}

/// One block through the REAL fold with the t12 FP extras (round_lane stays `None`, as it is for
/// every block outside an open execution span — the same configuration `dos_l5_common` folds under).
///
/// `audit` is the 2026-09-23 audit fence (armed on testnet-12 from genesis). `false` is the fold every
/// network but testnet-12 runs, which is where the defect this repro measured still lives — and
/// must, byte for byte: a dormant network's receipt lane is not the fix's to change.
fn fold_at(p: &Params, audit: bool, parent: &PalwChainStateV2, ctx: &PalwBlockContextV2, objects: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let sp = bundle(p).state;
    let f = flags(p, ctx.daa_score);
    let mut e = t12_extras(p, ctx.daa_score);
    e.audit_2026_09_23_active = audit;
    if !audit {
        e.settled_anchor_depth = None;
    }
    apply_palw_transition_v7(
        parent, &sp, None, ctx, objects, work, &[], Hash64::default(),
        f.unavailable_abstains, f.capability_bound, f.uncertified_weightless, f.da_court, &e,
    )
    .map(|(s, _, _)| s)
}

/// The floor's registry work — the value `step_model_registry` copies into the floor's lifecycle
/// row, whose `economic_ccu_per_claim` IS the FP network quantum's numerator.
fn floor_work() -> kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1 {
    let fp = floor_profile();
    let (pf, dc) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&fp, pf, dc);
    kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&fp, &job).expect("floor work derives")
}

fn floor_profile() -> Box<kaspa_consensus_core::palw_step::PalwShapeProfileV3> {
    Box::new(
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor profile"),
    )
}

/// The floor's canonical per-draw scalar — the collateral basis the compute reservation is
/// renormalised into (`palw_fp_compute_reserved_v1` -> `palw_exposure_basis_v1`).
fn floor_draw() -> u128 {
    let p = floor_profile();
    let d = PalwCanonicalClassDescriptorV1::of(&p, Hash64::default()).unwrap();
    let (pf, dc) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let j = kaspa_consensus_core::palw_base0_profile::rc_job_context(&p, pf, dc);
    palw_canonical_draw_work_v1(&d, &j, true).unwrap().provisional_scalar_v1()
}

/// Seed BASE-0's two on-chain prerequisites the live network builds permissionlessly: (1) the
/// FreePrompt family this build pins in `palw_rc_fp_certified_families_v1()` (a real
/// `FamilyCertified` drill writes it), so the REAL `ClassLaneCertified` fold below can publish the
/// floor's profile; and (2) the floor's registry lifecycle row (a `step_model_registry` boundary
/// writes it), whose `economic_ccu_per_claim` defines the FP network quantum. Both are real state
/// objects, not reimplementations; everything MEASURED after this runs through the real fold.
fn seed_prereqs(p: &Params, s: &PalwChainStateV2) -> PalwChainStateV2 {
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwModelLifecycleRowV1, PalwModelLifecycleV1, palw_lifecycle_profile_v1,
    };
    let (floor, _l, target, _sv) = genesis_classes(p)[0];
    let fam = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_fp_certified_families_v1()
        .into_iter()
        .find(|f| f.drilled_class_id == floor_profile().shape_profile_id())
        .expect("BASE-0 fp family is pinned");
    let work = floor_work();
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = bundle(p).panel.seat_count();
    let expected_q32 = kaspa_consensus_core::palw_economic_compute_v1::palw_expected_attempts_q32_v1(target);
    let row = PalwModelLifecycleRowV1 {
        state: PalwModelLifecycleV1::Active,
        work,
        profile: palw_lifecycle_profile_v1(&work, expected_q32, &globals, false),
        since_span: 0,
        probes_passed: 0,
        probes_failed: 0,
        probes_passed_this_span: 0,
        probes_failed_this_span: 0,
        ready_seats: 0,
        inflight_claims: 0,
        utilization_permille: 0,
        admission_milli: 0,
        cap_utilization_permille: 0,
        priced_share_permille: 0,
    };
    let mut c = PalwStateCarriageV2::from_state(s);
    c.fp_certified_families.insert(fam.digest(), PalwCertifiedFamilyStateV2 { family: fam, certified_daa: 0 });
    c.model_lifecycles.insert(floor, row);
    c.into_state_v3(&bundle(p).state, None, false, p.palw_canonical_work_daa()).expect("carriage rebuilds")
}

fn fp_commit(class_id: Hash64, bond: PalwBondKeyV2, pk: Vec<u8>, work_leaves: u64, prompt: &[u32], decode: u32, claim: Hash64) -> PalwConsensusObjectV2 {
    fp_commit_on(class_id, bond, pk, work_leaves, prompt, decode, claim, h(0x73_0001))
}

/// [`fp_commit`] naming `execution_root` — two commits naming one root are two claims of one
/// execution, which is what ADR-0151 forfeits together.
#[allow(clippy::too_many_arguments)]
fn fp_commit_on(class_id: Hash64, bond: PalwBondKeyV2, pk: Vec<u8>, work_leaves: u64, prompt: &[u32], decode: u32, claim: Hash64, execution_root: Hash64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::FreePromptCommitted {
        claim,
        class_id,
        bond,
        executor_pubkey: pk,
        work_leaves,
        prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(prompt),
        prompt_tokens: prompt.len() as u32,
        prompt_token_ids: prompt.to_vec(),
        decode_tokens_executed: decode,
        trace_root: h(0x71_0001),
        output_root: h(0x72_0001),
        // Fail-closed field: a non-default execution root, so the commit is adjudicable.
        execution_root,
        trace_chunk_count: 1,
        trace_retention_daa: 9_999_999,
        consumed_prefix_state: kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(class_id),
    }
}

/// A receipt-lane block's own work: one certified quantum spent. The fold's `apply_receipt_spend`
/// gates only on phase==Final, index range, and the spent set (the lottery/beacon/window are the
/// admission layer's job). The `challenge` binds a header position but is not read by the fold.
fn fp_spend(claim_id: Hash64, quantum_index: u32, bond: &kaspa_consensus_core::tx::TransactionOutpoint, pk: &[u8]) -> kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendUnsignedV3 {
    kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendUnsignedV3 {
        version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
        network_domain: h(0x0D05_0012),
        challenge: kaspa_consensus_core::palw_freeprompt_v3::spend_challenge_v3(h(0x0D05_0012), h(0xB0 + quantum_index as u64), 1_700, 7, claim_id, quantum_index, bond),
        claim_id,
        quantum_index,
        beacon_block: h(0xBEAC),
        producer_bond: *bond,
        producer_pubkey: pk.to_vec(),
    }
}

/// The finding's `rights_of`: `claim_realizable_rights_v1` past both fences, on the claim's
/// exposure — the execution quanta the mint WOULD issue (ceiling 2^16) + 1, priced over the
/// maturity gap. It prices EXECUTION quanta from the class draw, never the FP receipt quanta.
fn rights_of(exposure: u64, window_challenge: u64, window_court: u64, ttpb: u64) -> u128 {
    let counted = palw_execution_quantum_count_v1(u128::from(exposure), u128::from(PALW_EXECUTION_QUANTUM_V1), Hash64::default(), Hash64::default())
        .min(PALW_EXEC_MAX_QUANTA_PER_SPAN_V1 as u32);
    let quanta = counted.saturating_add(1);
    palw_realizable_before_maturity_v1(quanta, window_challenge, window_court, ttpb, palw_permit_value_sompi_v1(PALW_T12_PERMIT_FEE_CEILING_SOMPI))
}

/// **Past the fence (testnet-12): the conviction takes the receipt rights back.** The claim the
/// conviction names is voided (#8) — its spent quantum's weight leaves `safe_weight` — and the next
/// receipt block of it is refused. The economics the finding priced (step 5) are unchanged by this
/// group: the escrow half of #5 (pricing the rights into the reservation) is the other session's.
///
/// Fails without the fix: before it, step 4 spent quantum 1 after the conviction and the phase
/// stayed `Final` (this file's own history, kept below as the pre-fence record).
fn dos_repro_1_fp_final_receipt_rights_unpriced_unforfeitable() {
    run(true);
}

/// **PRE-FENCE DEFECT RECORD** — the finding as measured, on the fold every network but
/// testnet-12 still runs: the conviction leaves the claim `Final` and a receipt block spends after
/// it. Kept runnable (`--ignored`) because the measurement is the reason for #5 and #8, and because
/// below the fence it must stay true byte for byte.
fn dos_repro_1_pre_fence_defect_record() {
    run(false);
}

fn run(audit: bool) {
    let p = t12();
    let b = bundle(&p);
    let (floor, _leaves, _target, slash) = genesis_classes(&p)[0];
    let bonds = genesis_bonds(&p);
    let ttpb = p.target_time_per_block();
    let window_challenge = b.state.window_challenge();
    let window_court = b.state.window_court();
    let rate = p.palw_economic_payout.expect("t12 arms Upgrade C").rate_sompi_per_giga;

    // --- economics read from the shipped params -------------------------------------------------
    let subsidy: u64 = T12_BLOCK_SUBSIDY_SOMPI;
    let carve = p.palw_overlay_carve.expect("t12 arms the overlay carve");
    // The worker escrow of one block's subsidy. A receipt-lane block is a NON-selected-parent blue,
    // so coinbase.rs:246-273 pays it this worker base UNESCROWED (escrow is withheld only from the
    // block whose transition created a claim). Cross-checked against the real §F subsidy split.
    let worker_escrow = palw_reward_carve_v2(subsidy, &PalwRewardParamsV2::new(carve.worker_carve_permille).unwrap()).worker;
    let split_worker_base = p
        .dns_params
        .as_ref()
        .and_then(|dns| kaspa_consensus_core::config::params::palw_overlay_fee_split_at_v1(dns, p.palw_overlay_carve, 1_002))
        .map(|fs| kaspa_consensus_core::dns_finality::split_block_subsidy(subsidy, &fs).worker_base_sompi);
    let worker_base = split_worker_base.unwrap_or(worker_escrow);
    let w0 = palw_work_floor_v1(worker_escrow, rate);
    let fdraw = floor_draw();

    // === 1. drive a REAL compute-priced FP claim to Final through the fold =======================
    let attacker = 1u64;
    let bond = bond_key(attacker);
    let pk = pubkey_of(attacker);
    let g = genesis_state(&p);
    let s = fold_at(&p, audit, &g, &ctx(1, 1_000, 1, 0), &[bond_obj(attacker, attacker_collateral(&p))], PalwBlockWorkV3::None).unwrap();
    let s = seed_prereqs(&p, &s);
    let publish = PalwConsensusObjectV2::ClassLaneCertified { class_id: floor, lane: PalwCertifiedLaneV1::FreePrompt, profile: floor_profile() };
    let s = fold_at(&p, audit, &s, &ctx(2, 1_001, 2, 0), &[publish], PalwBlockWorkV3::None).expect("real ClassLaneCertified publishes the profile");
    assert!(s.fp_work_profile_of(&floor).is_some(), "the floor's fp work profile is published");

    // The largest reachable single floor claim (prompt 512, decode 256; leaves the real cap derives).
    let profile = floor_profile();
    let ladder = s.class_step_ladder_v1(&floor, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
    let (pt, dc) = (512u32, 256u32);
    let ids: Vec<u32> = (0..pt).collect();
    let work_leaves = kaspa_consensus_core::palw_step::step_leaf_count_of_tokens_capped_v1(&profile, pt, dc, ladder).expect("floor leaves derive");
    let claim_id = h(0xF0_00A1);
    let commit = fp_commit(floor, bond, pk.clone(), work_leaves, &ids, dc, claim_id);
    let s = fold_at(&p, audit, &s, &ctx(3, 1_002, 3, 0), &[commit], PalwBlockWorkV3::None).expect("real FreePromptCommitted prices in compute");

    let claim = s.claim(&claim_id).expect("the claim exists").clone();
    let reserved = claim.reserved;
    let escrowed = claim.escrowed_reward;
    let claim_pwu = claim.pwu; // compute-era: credited compute in the provisional MAC-eq scalar
    let quanta = match &claim.source {
        kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2::FreePrompt { quanta, .. } => *quanta,
        _ => panic!("free-prompt claim"),
    };
    // **THE FIRST STRUCTURAL FACT (palw_state_v2.rs:17372): the FP commitment lane escrows 0.**
    assert_eq!(escrowed, 0, "a free-prompt claim escrows 0 — so option A (carry the escrow into the reservation) reaches nothing here");
    assert!(reserved > 0 && to_msk(reserved) < 1.0, "the whole reservation is the compute-in-floor-unit at 5 sompi: {} sompi ({:.6} MSK)", reserved, to_msk(reserved));

    // Bind a panel of 5 genesis seats, license with a Valid quorum of 3, sweep to Final.
    let seats: Vec<PalwPanelSeatV2> = bonds[0..5].iter().map(|(k, o, _)| PalwPanelSeatV2 { bond: *k, operator_id: *o }).collect();
    let s = fold_at(&p, audit, &s, &ctx(4, 1_003, 4, 0), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h(0xA1), seats: seats.clone() }], PalwBlockWorkV3::None)
        .expect("panel binds");
    let valid_seats: Vec<PalwBondKeyV2> = bonds[0..3].iter().map(|(k, _, _)| *k).collect();
    let receipts: Vec<PalwSeatReceiptV2> = valid_seats
        .iter()
        .map(|k| PalwSeatReceiptV2 { claim: claim_id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: *k, signed_daa: 1_004, signature: Vec::new() })
        .collect();
    let s = fold_at(&p, audit, &s, &ctx(5, 1_004, 5, 0), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts }], PalwBlockWorkV3::None)
        .expect("the Valid quorum licenses");
    // Sweep past the challenge window -> Final.
    let s = fold_at(&p, audit, &s, &ctx(6, 1_300, 6, 0), &[], PalwBlockWorkV3::None).expect("challenge window closes");
    let final_daa = match &s.claim(&claim_id).expect("claim still live").phase {
        PalwClaimPhaseV2::Final { final_daa } => *final_daa,
        other => panic!("expected Final, got {other:?}"),
    };

    // The seat locks the fold ACTUALLY wrote for the Valid signers (ground truth).
    let fold_seat_lock: u128 = s.slashable_lock(valid_seats[0], claim_id).map(|l| l.amount).unwrap_or(0);
    let fold_v1_slashable: u128 = valid_seats.iter().filter_map(|k| s.slashable_lock(*k, claim_id).map(|l| l.amount)).sum();
    assert!(fold_seat_lock > 0, "each Valid seat posted a lock");

    // === 2. spend a receipt block AFTER Final through the REAL fold ==============================
    let weight_before_spend0 = s.safe_weight();
    let s = fold_at(&p, audit, &s, &ctx(7, 1_800, 7, 0), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(claim_id, 0, &bond.0, &pk)))
        .expect("a certified quantum spends into a receipt block at Final");
    let weight_after_spend0 = s.safe_weight();
    assert!(weight_after_spend0 > 0, "the spend adds receipt-lane weight; the block earns the unescrowed worker base at the coinbase");
    assert!(matches!(s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "still Final after a spend");

    // === 3. a REAL PanelFalseValid conviction — and the phase does NOT move ======================
    // The seats DID sign Valid on a fabricated Final; PanelFalseValid is warranted. The
    // contradiction is an ExecutorEquivocation carriage (convicts_execution_v1 accepts it for the
    // Valid signers), which slashes ONLY the seat's lock. consume_objective_offence never writes
    // the claim's phase (palw_state_v2.rs:9633-9720).
    let accused = valid_seats[0];
    let equivocation = equivocation_carriage(&bond.0, &profile);
    let payload = kaspa_consensus_core::palw_offence_v1::PalwPanelFalseValidEvidenceV1 {
        version: kaspa_consensus_core::palw_offence_v1::PALW_PANEL_FALSE_VALID_VERSION_V1,
        claim_id,
        network_domain: h(0x0D05_0012),
        accused_seat: accused.0,
        valid_receipt: PalwSeatReceiptV2 { claim: claim_id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: accused, signed_daa: 1_004, signature: Vec::new() },
        executor_pubkey: pk.clone(),
        contradiction: kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::ExecutorEquivocation(equivocation),
    };
    let evidence = borsh::to_vec(&payload).unwrap();
    let evidence_id = kaspa_consensus_core::palw_offence_v1::palw_offence_evidence_digest_v1(&evidence);
    let seat_collateral_before = s.bond(&accused).unwrap().collateral;
    let s = fold_at(
        &p,
        audit,
        &s,
        &ctx(8, 1_810, 8, 0),
        &[PalwConsensusObjectV2::ObjectiveOffence { kind: kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValid, accused, evidence_id, evidence }],
        PalwBlockWorkV3::None,
    )
    .expect("the PanelFalseValid conviction folds");
    let seat_collateral_after = s.bond(&accused).unwrap().collateral;
    let convicted = seat_collateral_before - seat_collateral_after;
    assert!(convicted > 0, "the conviction slashed the seat's lock ({} sompi)", convicted);
    let weight_after_conviction = s.safe_weight();
    let post_conviction_spend = fold_at(&p, audit, &s, &ctx(9, 1_820, 9, 0), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(claim_id, 1, &bond.0, &pk)));
    let s = if audit {
        // **#8: the conviction voids the Final it names, and takes its spent weight back.**
        assert!(
            matches!(
                s.claim(&claim_id).unwrap().phase,
                PalwClaimPhaseV2::Voided { voided_daa: 1_810, reason: kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::CourtFraud }
            ),
            "past the fence a PanelFalseValid conviction voids the Final claim: {:?}",
            s.claim(&claim_id).unwrap().phase
        );
        assert_eq!(weight_after_conviction, weight_before_spend0, "the spent quantum's weight leaves safe_weight with the claim");
        let reloaded = PalwStateCarriageV2::from_state(&s)
            .into_state_v3(&b.state, Some(s.state_root()), flags(&p, 1_810).uncertified_weightless, p.palw_canonical_work_daa())
            .expect("the voided claim passes the loader's consistency check (no spends outside Final)");
        assert_eq!(reloaded.state_root(), s.state_root());
        // === 4. and the NEXT receipt block is refused ==============================================
        let refused = post_conviction_spend.expect_err("past the fence a receipt block of a convicted Final is refused");
        assert!(matches!(refused, PalwStateV2Error::WrongPhase { edge: "ReceiptSpend", .. }), "{refused:?}");
        s
    } else {
        // **THE KEY FACT (pre-fence): the conviction did NOT move the phase.**
        assert!(matches!(s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "a PanelFalseValid conviction leaves the claim Final");
        assert_eq!(weight_after_conviction, weight_after_spend0, "and its weight stays");
        // === 4. and so the NEXT receipt block still spends legally, after the conviction =========
        let s = post_conviction_spend.expect("a receipt block STILL spends after the conviction — the phase never moved");
        assert!(s.safe_weight() > weight_after_spend0, "the post-conviction spend adds more weight and earns another unescrowed worker base");
        s
    };

    // === 5. price the amplification — receipt payout vs slashable ================================
    // Both sides are LINEAR in the claim's fabricated compute C, so the ratio is scale-invariant:
    // read the fold's real claim, then also tabulate the finding's per-W0 basis with the same
    // standalone functions the fold's lock and the receipt lottery use.
    //
    // E[receipt blocks] at the pooled-target ceiling (fp_pooled_target_ceiling_v1) = C / W0.
    let e_wins_this_claim = claim_pwu as f64 / w0 as f64;
    let payout_this_claim = e_wins_this_claim * worker_base as f64;
    let amp_fold_v1 = payout_this_claim / fold_v1_slashable as f64;

    // Per one W0 of fabricated compute, WITH the finding's execution-rights term in the lock
    // (what the processor's round_lane makes the fold price — execution quanta the FP Final never
    // actually mints, never the receipt quanta it does):
    let reserved_w0 = mul_div_u128(w0, 7_708u128, fdraw) * 5;
    let exposure_w0 = (reserved_w0 / 5) as u64;
    let rights_w0 = rights_of(exposure_w0, window_challenge, window_court, ttpb);
    let facts_w0 = PalwClaimFraudFactsV1 { reserved: reserved_w0, escrowed_reward: 0, exposure_pwu: exposure_w0, slash_value_per_pwu: slash, extra_economic_rights_sompi: rights_w0 };
    let gain_w0 = palw_max_fraud_gain_v1(&facts_w0);
    let lock_w0 = palw_seat_lock_required_v2(gain_w0, PALW_PANEL_COLLUDING_QUORUM_V1);
    let v1_slash_w0 = lock_w0 * 3; // full Valid quorum (V1 door)
    let s2_slash_w0 = lock_w0; // one full-replay seat (S2 door)
    let payout_w0 = worker_base as u128; // E[wins] = 1 at the ceiling for one W0
    let amp_v1 = payout_w0 as f64 / v1_slash_w0 as f64;
    let amp_s2 = payout_w0 as f64 / s2_slash_w0 as f64;
    // Capital lost if an honest auditor catches it BEFORE Final: just the reservation.
    let amp_pre_final = payout_w0 as f64 / reserved_w0 as f64;

    println!("\n================ fp-final-receipt-rights-unpriced-unforfeitable (t12, real fold) ================");
    println!("worker_base per receipt block (unescrowed)  = {} sompi ({:.2} MSK)   [carve {:.2} MSK; split {:?}]", worker_base, to_msk(worker_base as u128), to_msk(worker_escrow as u128), split_worker_base);
    println!("W0 (escrow/rate)                             = {w0} MAC-eq ; floor draw = {fdraw}");
    println!("--- the FP claim the REAL fold drove to Final ---");
    println!("class = BASE-0 ; prompt {pt} decode {dc} ; work_leaves {work_leaves}");
    println!("escrowed_reward                              = {escrowed} sompi   <-- ZERO (17372); nothing for option A to carry");
    println!("reserved (compute in floor unit x 5 sompi)   = {reserved} sompi ({:.6} MSK)", to_msk(reserved));
    println!("credited compute C (claim.pwu)               = {claim_pwu} MAC-eq -> {quanta} quanta");
    println!("Final at DAA                                 = {final_daa}");
    println!("seat lock the FOLD wrote per Valid seat      = {fold_seat_lock} sompi ({:.6} MSK) ; V1(3 seats) {fold_v1_slashable} ({:.6} MSK)", to_msk(fold_seat_lock), to_msk(fold_v1_slashable));
    println!("E[receipt blocks] this claim (C/W0, ceiling) = {e_wins_this_claim:.6} ; E[payout] {:.4} MSK", payout_this_claim / MSK);
    println!("amplification for THIS claim (payout / V1)   = {amp_fold_v1:.0}x   (scale-invariant: same ratio at any C)");
    println!("--- a receipt block spent AFTER Final, then a REAL PanelFalseValid conviction ---");
    if audit {
        println!("conviction slashed seat collateral           = {convicted} sompi ; claim phase after = Voided(CourtFraud) (#8)");
        println!("post-conviction receipt spend                = REFUSED (safe_weight {weight_after_spend0} -> {weight_after_conviction} at the conviction)");
    } else {
        println!("conviction slashed seat collateral           = {convicted} sompi ; claim phase after = Final (UNCHANGED, pre-fence)");
        println!("post-conviction receipt spend                = accepted (safe_weight {} -> {})", weight_after_spend0, s.safe_weight());
    }
    println!("--- per ONE W0 of fabricated compute (finding basis, execution-rights term in the lock) ---");
    println!("reserved/W0 {} ({:.4} MSK) ; G(code) {} ({:.4} MSK) ; lock/seat {} ({:.4} MSK)", reserved_w0, to_msk(reserved_w0), gain_w0, to_msk(gain_w0), lock_w0, to_msk(lock_w0));
    println!("E[payout]/W0 {:.2} MSK ; V1 slashable {:.2} MSK ; S2 slashable {:.2} MSK", to_msk(payout_w0), to_msk(v1_slash_w0), to_msk(s2_slash_w0));
    println!("AMPLIFICATION  payout/V1 = {amp_v1:.0}x ; payout/S2 = {amp_s2:.0}x ; payout/(reserved lost pre-Final) = {amp_pre_final:.0}x");
    println!("=================================================================================================\n");

    // === the assertions the finding claims ======================================================
    // Priced on testnet-12's own economics — the fence also carries option A's escrow-backed
    // reservation, so the seat lock below it is a different number and the ratios are not this
    // record's to state. The pre-fence record is about the phase and the spend, above.
    if !audit {
        return;
    }
    assert!(worker_escrow as u128 == worker_base as u128 || split_worker_base.is_some(), "the receipt block's worker base is the unescrowed carve");
    // Rights are worth ~3,200.85 MSK and the seat lock is ~6.97 MSK/seat: >150x for V1, >450x for S2.
    assert!(amp_v1 > 150.0, "the full V1 Valid quorum's slashable is < 1/150 of one W0's receipt payout: {amp_v1:.1}x");
    assert!(amp_s2 > 450.0, "a single full-replay seat's slashable is < 1/450 of one W0's receipt payout: {amp_s2:.1}x");
    assert!(amp_pre_final > 500.0, "caught before Final the attacker loses only the reservation: {amp_pre_final:.1}x");
    // And the fold's own real claim reproduces the scale-invariant ratio.
    assert!(amp_fold_v1 > 150.0, "the fold-driven claim's payout/V1 also exceeds 150x: {amp_fold_v1:.1}x");
    // The lock never approaches the receipt payout it licenses.
    assert!((worker_base as u128) > 100 * fold_v1_slashable, "one receipt block's worker base dwarfs the whole V1 quorum's slashable");
}

/// **#5, the forfeiture half: a receipt block of a convicted EXECUTION is refused, whichever claim
/// carries it.** Two free-prompt claims commit one execution root; both reach `Final`; a Valid signer
/// of the FIRST is convicted. Past the fence the SECOND — not named by the conviction, so still
/// `Final` — can no longer spend: `apply_receipt_spend` refuses a claim whose root is in ADR-0151's
/// forfeiture set (`ReceiptRightsForfeited`). Below the fence it spends, as before.
///
/// Fails without the fix: remove the forfeiture check in `apply_receipt_spend` and the sibling's
/// post-conviction spend folds (checked; the `expect_err` below is what trips).
fn dos_repro_1_a_sibling_of_the_convicted_execution_spends_nothing() {
    let refused = sibling_spend_after_conviction(true).expect_err("past the fence the sibling's spend is refused");
    assert!(matches!(refused, PalwStateV2Error::ReceiptRightsForfeited { execution_root, .. } if execution_root == h(0x73_0001)), "{refused:?}");
    sibling_spend_after_conviction(false).expect("below the fence the sibling still spends — the dormant fold is unchanged");
}

fn same_block(audit: bool, spend_named: bool) -> (bool, bool, Result<PalwChainStateV2, PalwStateV2Error>) {
    let p = t12();
    let (floor, _leaves, _target, _slash) = genesis_classes(&p)[0];
    let bonds = genesis_bonds(&p);
    let attacker = 1u64;
    let bond = bond_key(attacker);
    let pk = pubkey_of(attacker);
    let s = fold_at(&p, audit, &genesis_state(&p), &ctx(1, 1_000, 1, 0), &[bond_obj(attacker, attacker_collateral(&p))], PalwBlockWorkV3::None).unwrap();
    let s = seed_prereqs(&p, &s);
    let publish = PalwConsensusObjectV2::ClassLaneCertified { class_id: floor, lane: PalwCertifiedLaneV1::FreePrompt, profile: floor_profile() };
    let s = fold_at(&p, audit, &s, &ctx(2, 1_001, 2, 0), &[publish], PalwBlockWorkV3::None).unwrap();
    let profile = floor_profile();
    let ladder = s.class_step_ladder_v1(&floor, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
    let (pt, dc) = (512u32, 256u32);
    let ids: Vec<u32> = (0..pt).collect();
    let work_leaves = kaspa_consensus_core::palw_step::step_leaf_count_of_tokens_capped_v1(&profile, pt, dc, ladder).unwrap();
    let (named, sibling) = (h(0xF0_00A1), h(0xF0_00B2));
    let root = h(0x73_0001);
    let s = fold_at(&p, audit, &s, &ctx(3, 1_002, 3, 0), &[fp_commit_on(floor, bond, pk.clone(), work_leaves, &ids, dc, named, root)], PalwBlockWorkV3::None)
        .expect("the first claim commits");
    // Another prompt, so another work id (`fp_work_id_v1` is class, prompt, bond) — the same
    // declared execution root, which is what the forfeiture keys on.
    let other_ids: Vec<u32> = (1..=pt).collect();
    let s = fold_at(&p, audit, &s, &ctx(4, 1_003, 4, 0), &[fp_commit_on(floor, bond, pk.clone(), work_leaves, &other_ids, dc, sibling, root)], PalwBlockWorkV3::None)
        .expect("a second claim of the same execution commits");
    let seats: Vec<PalwPanelSeatV2> = bonds[0..5].iter().map(|(k, o, _)| PalwPanelSeatV2 { bond: *k, operator_id: *o }).collect();
    let valid_seats: Vec<PalwBondKeyV2> = bonds[0..3].iter().map(|(k, _, _)| *k).collect();
    let receipts = |claim: Hash64| -> Vec<PalwSeatReceiptV2> {
        valid_seats
            .iter()
            .map(|k| PalwSeatReceiptV2 { claim, verdict: PalwReceiptVerdictV2::Valid, seat_bond: *k, signed_daa: 1_006, signature: Vec::new() })
            .collect()
    };
    let s = fold_at(
        &p,
        audit,
        &s,
        &ctx(5, 1_005, 5, 0),
        &[
            PalwConsensusObjectV2::PanelBound { claim: named, anchor: h(0xA1), seats: seats.clone() },
            PalwConsensusObjectV2::PanelBound { claim: sibling, anchor: h(0xA2), seats: seats.clone() },
        ],
        PalwBlockWorkV3::None,
    )
    .expect("both panels bind");
    let s = fold_at(
        &p,
        audit,
        &s,
        &ctx(6, 1_006, 6, 0),
        &[
            PalwConsensusObjectV2::ReceiptLicensed { claim: named, receipts: receipts(named) },
            PalwConsensusObjectV2::ReceiptLicensed { claim: sibling, receipts: receipts(sibling) },
        ],
        PalwBlockWorkV3::None,
    )
    .expect("both license");
    let s = fold_at(&p, audit, &s, &ctx(7, 1_300, 7, 0), &[], PalwBlockWorkV3::None).unwrap();
    for id in [named, sibling] {
        assert!(matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "{id} Final: {:?}", s.claim(&id).unwrap().phase);
    }
    // Before the conviction the sibling spends like any Final.
    let s = fold_at(&p, audit, &s, &ctx(8, 1_800, 8, 0), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(sibling, 0, &bond.0, &pk)))
        .expect("the sibling spends before any conviction");
    let accused = valid_seats[0];
    let payload = kaspa_consensus_core::palw_offence_v1::PalwPanelFalseValidEvidenceV1 {
        version: kaspa_consensus_core::palw_offence_v1::PALW_PANEL_FALSE_VALID_VERSION_V1,
        claim_id: named,
        network_domain: h(0x0D05_0012),
        accused_seat: accused.0,
        valid_receipt: PalwSeatReceiptV2 { claim: named, verdict: PalwReceiptVerdictV2::Valid, seat_bond: accused, signed_daa: 1_006, signature: Vec::new() },
        executor_pubkey: pk.clone(),
        contradiction: kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::ExecutorEquivocation(equivocation_carriage(&bond.0, &profile)),
    };
    let evidence = borsh::to_vec(&payload).unwrap();
    let evidence_id = kaspa_consensus_core::palw_offence_v1::palw_offence_evidence_digest_v1(&evidence);
    // The PARENT state the processor's own-work admission reads (processor.rs palw_v2_check_receipt_spend
    // is called with `state`, the parent, at processor.rs:1903): claim Final, root not forfeited.
    let target = if spend_named { named } else { sibling };
    let parent_final = matches!(s.claim(&target).unwrap().phase, PalwClaimPhaseV2::Final { .. });
    let parent_forfeit = s.palw_execution_root_is_forfeited_v1(&root);
    // ONE block: its accepted objects carry the conviction (step 3), its own work spends (step 4).
    let r = fold_at(
        &p,
        audit,
        &s,
        &ctx(9, 1_810, 9, 0),
        &[PalwConsensusObjectV2::ObjectiveOffence { kind: kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValid, accused, evidence_id, evidence }],
        PalwBlockWorkV3::ReceiptSpend(&fp_spend(target, 1, &bond.0, &pk)),
    );
    (parent_final, parent_forfeit, r)
}

fn sibling_spend_after_conviction(audit: bool) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let p = t12();
    let (floor, _leaves, _target, _slash) = genesis_classes(&p)[0];
    let bonds = genesis_bonds(&p);
    let attacker = 1u64;
    let bond = bond_key(attacker);
    let pk = pubkey_of(attacker);
    let s = fold_at(&p, audit, &genesis_state(&p), &ctx(1, 1_000, 1, 0), &[bond_obj(attacker, attacker_collateral(&p))], PalwBlockWorkV3::None).unwrap();
    let s = seed_prereqs(&p, &s);
    let publish = PalwConsensusObjectV2::ClassLaneCertified { class_id: floor, lane: PalwCertifiedLaneV1::FreePrompt, profile: floor_profile() };
    let s = fold_at(&p, audit, &s, &ctx(2, 1_001, 2, 0), &[publish], PalwBlockWorkV3::None).unwrap();
    let profile = floor_profile();
    let ladder = s.class_step_ladder_v1(&floor, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
    let (pt, dc) = (512u32, 256u32);
    let ids: Vec<u32> = (0..pt).collect();
    let work_leaves = kaspa_consensus_core::palw_step::step_leaf_count_of_tokens_capped_v1(&profile, pt, dc, ladder).unwrap();
    let (named, sibling) = (h(0xF0_00A1), h(0xF0_00B2));
    let root = h(0x73_0001);
    let s = fold_at(&p, audit, &s, &ctx(3, 1_002, 3, 0), &[fp_commit_on(floor, bond, pk.clone(), work_leaves, &ids, dc, named, root)], PalwBlockWorkV3::None)
        .expect("the first claim commits");
    // Another prompt, so another work id (`fp_work_id_v1` is class, prompt, bond) — the same
    // declared execution root, which is what the forfeiture keys on.
    let other_ids: Vec<u32> = (1..=pt).collect();
    let s = fold_at(&p, audit, &s, &ctx(4, 1_003, 4, 0), &[fp_commit_on(floor, bond, pk.clone(), work_leaves, &other_ids, dc, sibling, root)], PalwBlockWorkV3::None)
        .expect("a second claim of the same execution commits");
    let seats: Vec<PalwPanelSeatV2> = bonds[0..5].iter().map(|(k, o, _)| PalwPanelSeatV2 { bond: *k, operator_id: *o }).collect();
    let valid_seats: Vec<PalwBondKeyV2> = bonds[0..3].iter().map(|(k, _, _)| *k).collect();
    let receipts = |claim: Hash64| -> Vec<PalwSeatReceiptV2> {
        valid_seats
            .iter()
            .map(|k| PalwSeatReceiptV2 { claim, verdict: PalwReceiptVerdictV2::Valid, seat_bond: *k, signed_daa: 1_006, signature: Vec::new() })
            .collect()
    };
    let s = fold_at(
        &p,
        audit,
        &s,
        &ctx(5, 1_005, 5, 0),
        &[
            PalwConsensusObjectV2::PanelBound { claim: named, anchor: h(0xA1), seats: seats.clone() },
            PalwConsensusObjectV2::PanelBound { claim: sibling, anchor: h(0xA2), seats: seats.clone() },
        ],
        PalwBlockWorkV3::None,
    )
    .expect("both panels bind");
    let s = fold_at(
        &p,
        audit,
        &s,
        &ctx(6, 1_006, 6, 0),
        &[
            PalwConsensusObjectV2::ReceiptLicensed { claim: named, receipts: receipts(named) },
            PalwConsensusObjectV2::ReceiptLicensed { claim: sibling, receipts: receipts(sibling) },
        ],
        PalwBlockWorkV3::None,
    )
    .expect("both license");
    let s = fold_at(&p, audit, &s, &ctx(7, 1_300, 7, 0), &[], PalwBlockWorkV3::None).unwrap();
    for id in [named, sibling] {
        assert!(matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "{id} Final: {:?}", s.claim(&id).unwrap().phase);
    }
    // Before the conviction the sibling spends like any Final.
    let s = fold_at(&p, audit, &s, &ctx(8, 1_800, 8, 0), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(sibling, 0, &bond.0, &pk)))
        .expect("the sibling spends before any conviction");
    let accused = valid_seats[0];
    let payload = kaspa_consensus_core::palw_offence_v1::PalwPanelFalseValidEvidenceV1 {
        version: kaspa_consensus_core::palw_offence_v1::PALW_PANEL_FALSE_VALID_VERSION_V1,
        claim_id: named,
        network_domain: h(0x0D05_0012),
        accused_seat: accused.0,
        valid_receipt: PalwSeatReceiptV2 { claim: named, verdict: PalwReceiptVerdictV2::Valid, seat_bond: accused, signed_daa: 1_006, signature: Vec::new() },
        executor_pubkey: pk.clone(),
        contradiction: kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::ExecutorEquivocation(equivocation_carriage(&bond.0, &profile)),
    };
    let evidence = borsh::to_vec(&payload).unwrap();
    let evidence_id = kaspa_consensus_core::palw_offence_v1::palw_offence_evidence_digest_v1(&evidence);
    let s = fold_at(
        &p,
        audit,
        &s,
        &ctx(9, 1_810, 9, 0),
        &[PalwConsensusObjectV2::ObjectiveOffence { kind: kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValid, accused, evidence_id, evidence }],
        PalwBlockWorkV3::None,
    )
    .expect("the conviction of the first claim's seat folds");
    assert!(
        matches!(s.claim(&sibling).unwrap().phase, PalwClaimPhaseV2::Final { .. }),
        "the conviction names the first claim only; the sibling stays Final"
    );
    fold_at(&p, audit, &s, &ctx(10, 1_820, 10, 0), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(sibling, 1, &bond.0, &pk)))
}

/// A minimal `PalwEquivocationCarriageV1` for the ExecutorEquivocation contradiction, accusing the
/// claim's EXECUTOR bond (`accused`) — past the 2026-09-24 review fix `bind_panel_false_valid`
/// refuses one that accuses anyone else, or that the evidence carries under another key.
fn equivocation_carriage(accused: &kaspa_consensus_core::tx::TransactionOutpoint, profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3) -> kaspa_consensus_core::palw_carriage::PalwEquivocationCarriageV1 {
    let job_context = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, 512, 256);
    let att = |root: u64| kaspa_consensus_core::palw_slash::PalwExecutionAttestationV1 {
        version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
        executor_id: h(0x1),
        job_context_hash: h(0x2),
        full_logits_trace_root: h(root),
        committed_root: h(root),
        bond_outpoint: *accused,
        signature: Vec::new(),
    };
    kaspa_consensus_core::palw_carriage::PalwEquivocationCarriageV1 {
        version: kaspa_consensus_core::palw_carriage::PALW_CARRIAGE_VERSION_V1,
        accused_bond_outpoint: *accused,
        certificate: kaspa_consensus_core::palw_slash::PalwClassContradictionCertificateV1 {
            version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
            job_context,
            attestation_a: att(0xAA),
            attestation_b: att(0xBB),
        },
    }
}

/// REVIEW (fence lens): a block whose accepted objects convict an execution and whose OWN work
/// spends a receipt of that execution passes the processor's own-work admission (parent state:
/// claim Final, root not forfeited) and is then refused by the fold's step 4, so the whole block
/// is disqualified ("PALW state"). Below the fence the same block folds.
///
/// **Pinned as the intended rule (2026-09-24 review):** unlike an own attempt, an own receipt spend
/// is NOT skipped on this refusal — a spend escrows nothing, and the coinbase pays the selected
/// parent's worker share whatever its work turned out to be, so a skip would pay one receipt block
/// for a convicted execution (what #5 closed). Every node refuses the block alike; the fold's
/// step-4 comment says so. The refusal is the phase (#8 voided the named claim) or the forfeiture
/// (#5, a sibling on the same root) — asserted by name so the test cannot pass on some other error.
#[test]
fn review_fence_same_block_conviction_and_own_spend() {
    use kaspa_consensus_core::palw_state_v2::PalwStateV2Error;
    for spend_named in [true, false] {
        let (parent_final, parent_forfeit, r) = same_block(true, spend_named);
        assert!(parent_final && !parent_forfeit, "the parent state admits the spend");
        let err = r.expect_err("past the fence the fold refuses the block's own spend");
        println!("spend_named={spend_named}: fold error past the fence: {err:?}");
        if spend_named {
            assert!(matches!(err, PalwStateV2Error::WrongPhase { edge: "ReceiptSpend", .. }), "{err:?}");
        } else {
            assert!(matches!(err, PalwStateV2Error::ReceiptRightsForfeited { .. }), "{err:?}");
        }
        let (_, _, below) = same_block(false, spend_named);
        below.expect("below the fence the same block folds");
    }
}
