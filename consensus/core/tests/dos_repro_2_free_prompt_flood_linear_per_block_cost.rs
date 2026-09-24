//! DoS repro 2 — free-prompt commitment flood: linear rooted state and a full per-block rehash.
//!
//! Finding under test (upheld by both reviewers): a `FreePromptCommitted` puts ~612 rooted bytes
//! per commitment into state for ~3,600 DAA at ~no collateral cost, and every chain block then pays
//! a full state rehash (`state_root`, palw_state_v2.rs:7282), a whole-state clone
//! (`TransitionBuilder::new`, :8621) and a tip rewrite (processor.rs:2127). The only consensus
//! limit is block mass; rent for the object is 0 (`palw_object_rent_ceiling_v1`, :4372).
//!
//! WHAT THIS TEST DOES DIFFERENTLY FROM the harness `dos_l1_claim_flood.rs`: it folds through the
//! REAL testnet-12 extras. testnet-12 arms `palw_fp_derived_work` and `palw_canonical_work` from
//! genesis (`palw_t12_arm_every_rule_from_genesis`), so the FP lane does NOT take the leaves path
//! the finding text and `dos_l1` measured — it takes the COMPUTE path
//! (`palw_fp_commitment_price_from_state_v1`, the `in_compute` branch, palw_state_v2.rs:17632). The
//! common `dos_l5_common::extras` leaves `fp_derived_work_daa = None`, which does NOT match the
//! processor; `t12_extras` below resolves it from the armed fence the way the processor's
//! `palw_transition_extras_for` does.
//!
//! TWO real gates the finding text omitted, both established here against the real fold:
//!   (a) the compute path refuses every commitment until the class has published an on-chain
//!       `fp_work_profile` (`FreePromptClassHasNoWorkProfile`), which needs a `FamilyCertified`
//!       drill (real grading) plus a `ClassLaneCertified`. A permissionless, ONE-TIME setup — not
//!       free, but rooted forever once done, after which the flood proceeds.
//!   (b) the compute path ALSO roots one `fp_claim_prompts` row per commitment (retained ~100
//!       epochs) and scans every prior paid prompt (`fp_accounted_prefix_tokens_v2`, O(claims)) on
//!       each new commitment — MORE rooted bytes and MORE per-commit CPU than the leaves path.
//!
//! Setup shims (documented, not measured): the BASE-0 FreePrompt family and the floor's registry
//! `model_lifecycle` row are injected via the carriage, because a live `FamilyCertified` drill and
//! the execution-lane span step that opens the row both need inputs a consensus-core test cannot
//! supply (a real model run; the processor-only `round_lane` extras). Both hold the exact values
//! the real fold would write. Everything MEASURED runs through `apply_palw_transition_v7`.
//!
//! Memory: the largest state holds N_MAX claims (a few MiB). Nothing is sized from an attacker
//! count; at-scale numbers are extrapolations printed with their formula.
//!
//! Run: cargo test -p kaspa-consensus-core --test dos_repro_2_free_prompt_flood_linear_per_block_cost -- --nocapture --test-threads=2

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwCertifiedFamilyStateV2, PalwCertifiedLaneV1, PalwChainStateV2,
    PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateCarriageV2, PalwStateV2Error, PalwTransitionExtrasV1, PalwVoidReasonV2,
    apply_palw_transition_v7, palw_object_rent_ceiling_v1,
};
use std::time::Instant;

const N_STEP: usize = 500;
const N_MAX: usize = 3_000;
const ATTACKER: u64 = 1;
/// 1,000 MSK, or the registration floor where that is higher (the producer floor, 13,000 MSK on
/// testnet-12's regenesis params — `palw_bond_registration_floor_v1`).
fn attacker_collateral(p: &Params) -> u64 {
    at_least_the_floor(p, 100_000_000_000)
}

fn floor_profile() -> Box<kaspa_consensus_core::palw_step::PalwShapeProfileV3> {
    Box::new(
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor profile"),
    )
}

/// The floor's registry work — the value `step_model_registry` copies into the floor's row.
fn floor_work() -> kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1 {
    let fp = floor_profile();
    let (pf, dc) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&fp, pf, dc);
    kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&fp, &job).expect("floor work derives")
}

/// The processor's t12 extras for the FP lane: `common::extras` leaves `fp_derived_work_daa = None`
/// and `fp_da_pins_active = false`; the shipped processor resolves both from armed t12 fences.
fn t12_extras(p: &Params, daa: u64) -> PalwTransitionExtrasV1 {
    let mut e = extras(p, daa);
    e.fp_derived_work_daa = p.palw_fp_derived_work_fence().map(|f| f.daa_score());
    e.fp_da_pins_active = p.palw_fp_da_pins_fence().is_some_and(|f| f.is_active(daa));
    e
}

fn fold_t12(
    p: &Params,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let sp = bundle(p).state;
    let f = flags(p, ctx.daa_score);
    apply_palw_transition_v7(
        parent, &sp, None, ctx, objects, PalwBlockWorkV3::None, &[], Hash64::default(), f.unavailable_abstains,
        f.capability_bound, f.uncertified_weightless, f.da_court, &t12_extras(p, ctx.daa_score),
    )
    .map(|(s, _, _)| s)
}

/// Inject the on-chain BASE-0 FreePrompt family and the floor's registry row (see module doc).
fn seed_prereqs(p: &Params, s: &PalwChainStateV2) -> PalwChainStateV2 {
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwModelLifecycleRowV1, PalwModelLifecycleV1, palw_lifecycle_profile_v1,
    };
    let (floor, _l, target, _sv) = genesis_classes(p)[0];
    let fam = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_fp_certified_families_v1()
        .into_iter()
        .find(|f| f.drilled_class_id == floor_profile().shape_profile_id())
        .expect("BASE-0 fp family is pinned by this build");
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

/// A real ML-DSA-87 public key is 2,592 bytes; the bond and every commitment carry it verbatim.
fn attacker_pubkey() -> Vec<u8> {
    let mut v = vec![0xA7u8; kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN];
    v[..8].copy_from_slice(&ATTACKER.to_le_bytes());
    v
}

/// The attacker's bond, carrying the real-length ML-DSA key the fold matches every commitment
/// against (`BondKeyMismatch`). Signature is checked at acceptance (node side), not in the fold.
fn attacker_bond(p: &Params) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(ATTACKER),
        pubkey: attacker_pubkey(),
        operator_pubkey: operator_pubkey_of(ATTACKER),
        collateral: attacker_collateral(p),
        payout_payload: h(0x9A00 + ATTACKER),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

/// A minimal free-prompt job: a DISTINCT single-token prompt (so nothing is credited to a shared
/// prefix — no `ZeroQuanta`) and one decode token. The smallest priceable job on the floor's graph.
fn fp_commit(class_id: Hash64, bond: PalwBondKeyV2, pk: Vec<u8>, work_leaves: u64, i: u64) -> PalwConsensusObjectV2 {
    let ids = vec![i as u32];
    PalwConsensusObjectV2::FreePromptCommitted {
        job_pin: kaspa_hashes::Hash64::default(),
        claim: h(0xF0_0000_0000 + i),
        class_id,
        bond,
        executor_pubkey: pk,
        work_leaves,
        prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&ids),
        prompt_tokens: 1,
        prompt_token_ids: ids,
        decode_tokens_executed: 1,
        trace_root: h(0x71_0000_0000 + i),
        output_root: h(0x72_0000_0000 + i),
        execution_root: h(0x73_0000_0000 + i),
        trace_chunk_count: 1,
        trace_retention_daa: 9_999_999,
        consumed_prefix_state: kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(class_id),
    }
}

fn setup(p: &Params) -> (PalwChainStateV2, Hash64, u64) {
    let (floor, _leaves, _t, _s) = genesis_classes(p)[0];
    let g = genesis_state(p);
    // Attacker registers its own bond (a transaction; consensus checks its signature at acceptance).
    let s = fold_t12(p, &g, &ctx(1, 1_000, 1, 0), &[attacker_bond(p)]).expect("bond registers");
    let s = seed_prereqs(p, &s);
    // The floor's FP profile is published by the REAL ClassLaneCertified fold (permissionless).
    let publish =
        PalwConsensusObjectV2::ClassLaneCertified { class_id: floor, lane: PalwCertifiedLaneV1::FreePrompt, profile: floor_profile() };
    let s = fold_t12(p, &s, &ctx(2, 1_001, 2, 0), &[publish]).expect("floor FP profile publishes");
    assert!(s.fp_work_profile_of(&floor).is_some(), "the floor's FP work profile is now rooted");
    // The minimal job's derived work leaves, off the same graph the fold re-derives against.
    let ladder = s.class_step_ladder_v1(&floor, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
    let wl = kaspa_consensus_core::palw_step::step_leaf_count_of_tokens_capped_v1(&floor_profile(), 1, 1, ladder).expect("derivable");
    (s, floor, wl)
}

fn carriage_bytes(s: &PalwChainStateV2) -> usize {
    borsh::to_vec(&PalwStateCarriageV2::from_state(s)).expect("carriage").len()
}

#[test]
fn dos_repro_2_free_prompt_flood_linear_per_block_cost() {
    let p = t12();
    let (base, floor, wl) = setup(&p);
    let bond = bond_key(ATTACKER);
    let pk = attacker_pubkey();

    // ---- 0. the object and its price (real fold, t12 compute path) -----------------------------
    let one = fp_commit(floor, bond, pk.clone(), wl, 1);
    let obj_bytes = borsh::to_vec(&one).unwrap().len();
    let rent = palw_object_rent_ceiling_v1(&one);
    let priced = fold_t12(&p, &base, &ctx(3, 1_002, 3, 0), &[one]).expect("first commit folds");
    let (first_reserved, first_pwu) = priced.claims_iter().next().map(|(_, c)| (c.reserved, c.pwu)).unwrap();
    let sp = bundle(&p).state;
    let ceiling = attacker_collateral(&p) as u128 * sp.fp_max_exposure_ratio_permille() as u128 / 1000;

    println!("=== FREE-PROMPT FLOOD on the REAL testnet-12 fold (compute path; fp_derived_work + canonical_work armed at DAA 0) ===");
    println!("object wire bytes                 = {obj_bytes} (incl. the 2,592 B ML-DSA executor key + a 1-token prompt)");
    println!("consensus rent for the object     = {rent} sompi  (palw_object_rent_ceiling_v1: FreePromptCommitted is unpriced)");
    println!("reserve per minimal claim         = {first_reserved} sompi ({:.7} MSK), pwu {first_pwu}  [COMPUTE path, not the finding's leaves 4,815-38,540]", msk(first_reserved));
    println!("bond fp exposure ceiling (50%)    = {ceiling} sompi -> {} concurrent minimal claims on one 1,000-MSK bond", ceiling / first_reserved.max(1));
    assert_eq!(rent, 0, "the finding's 'rent is 0' holds on the real t12 object");

    // ---- 1. flood: N minimal commitments, measure rooted bytes and per-block cost -------------
    let base_bytes = carriage_bytes(&base);
    println!("\n{:>7} {:>13} {:>10} {:>12} {:>12} {:>12}", "claims", "carriage B", "B/claim", "root ms", "clone ms", "empty-fold ms");
    let mut s = base.clone();
    let mut blue = 10u64;
    let mut n = 0usize;
    let mut last = (0f64, 0f64, 0f64, 0usize, 0usize);
    while n < N_MAX {
        for i in n..n + N_STEP {
            let obj = fp_commit(floor, bond, pk.clone(), wl, i as u64 + 1);
            // Fixed DAA 1,002: below every claim's window_bind deadline, so nothing voids and the
            // whole flood stays live (Provisional) at once — the steady state the finding names.
            s = fold_t12(&p, &s, &ctx(0x1000 + blue, 1_002, blue, 0), &[obj]).unwrap_or_else(|e| panic!("commit #{i} folds: {e:?}"));
            blue += 1;
        }
        n += N_STEP;
        let bytes = carriage_bytes(&s);
        let claims = s.claims_iter().count();
        let prompt_rows = s.fp_claimed_prompt_ids_of(&floor, 1_002).len();
        // state_root(): the full rehash every chain block pays before it accepts.
        let t = Instant::now();
        let mut acc = 0u8;
        for _ in 0..5 {
            acc ^= s.state_root().as_byte_slice()[0];
        }
        let root_ms = t.elapsed().as_secs_f64() * 1e3 / 5.0;
        // TransitionBuilder::new clones the whole parent state, once per fold and again per merged work.
        let t = Instant::now();
        for _ in 0..5 {
            let c = s.clone();
            acc ^= c.claims_iter().count() as u8;
        }
        let clone_ms = t.elapsed().as_secs_f64() * 1e3 / 5.0;
        // One empty block on top: what every node pays per block while these claims live.
        let t = Instant::now();
        let _ = fold_t12(&p, &s, &ctx(0x2000 + blue, 1_002, blue, 0), &[]).unwrap();
        let fold_ms = t.elapsed().as_secs_f64() * 1e3;
        std::hint::black_box(acc);
        println!("{n:>7} {bytes:>13} {:>10} {root_ms:>12.3} {clone_ms:>12.3} {fold_ms:>12.3}   (claims {claims}, prompt rows {prompt_rows})", (bytes - base_bytes) / n);
        last = (root_ms, clone_ms, fold_ms, bytes - base_bytes, claims);
    }
    let per_claim_bytes = last.3 / n;
    let per_claim_root_ms = last.0 / n as f64;
    assert_eq!(last.4, n, "every flood claim is still live (Provisional) — the steady state");
    assert!(per_claim_bytes > 0, "each commitment leaves rooted bytes; measured {per_claim_bytes} B/claim");

    // ---- 2. consensus per-block bound (mass) and abandoned-claim lifetime ----------------------
    let carrier_bytes = obj_bytes + kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN + 200; // + commitment signature + tx overhead
    let body = (p.max_block_mass / kaspa_consensus_core::constants::TRANSIENT_BYTE_TO_MASS_FACTOR) as usize;
    let per_block = body / carrier_bytes;
    let life = sp.window_bind() + sp.fp_abandon_hold_daa() + sp.claim_retirement_daa();
    let steady = per_block as u64 * life;
    println!("\nconsensus per-block bound (mass)  = {body} B body (max_block_mass {} / {}) / ~{carrier_bytes} B/carrier = {per_block} commitments/block",
        p.max_block_mass, kaspa_consensus_core::constants::TRANSIENT_BYTE_TO_MASS_FACTOR);
    println!("abandoned-claim rooted lifetime   = window_bind {} + abandon_hold {} + retirement {} = {life} DAA",
        sp.window_bind(), sp.fp_abandon_hold_daa(), sp.claim_retirement_daa());
    println!("steady-state live claims (1 bond) = {per_block} x {life} = {steady}");
    println!("extrapolated at steady state       : rooted {:.1} MiB, state_root {:.1} ms/block (debug build; per-claim {per_claim_bytes} B, {per_claim_root_ms:.4} ms root)",
        steady as f64 * per_claim_bytes as f64 / (1024.0 * 1024.0), steady as f64 * per_claim_root_ms);
    println!("mergeset_size_limit {} x clone     : step 4b clones the whole state once per merged work (checkpoint) — up to {} clones/block",
        p.mergeset_size_limit(), p.mergeset_size_limit());

    // ---- 3. lifecycle of one abandoned commitment: void without slash, permanent residue -------
    let commit = fp_commit(floor, bond, pk.clone(), wl, 900_001);
    let id = h(0xF0_0000_0000 + 900_001);
    let s0 = fold_t12(&p, &base, &ctx(0x5000, 2_000, 5_000, 0), &[commit]).expect("commit folds");
    let reserved_at_accept = s0.reserved_exposure(&bond);
    let collateral_before = s0.bond(&bond).unwrap().collateral;
    // Advance strictly past window_bind: the claim voids at BindTimeout (no panel bound, no slash).
    let after_bind = fold_t12(&p, &s0, &ctx(0x5001, 2_000 + sp.window_bind() + 1, 5_001, 0), &[]).expect("bind window sweeps");
    let void_phase = format!("{:?}", after_bind.claim(&id).map(|c| c.phase.clone()));
    let voided_no_slash =
        matches!(after_bind.claim(&id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }));
    let reserved_on_hold = after_bind.reserved_exposure(&bond);
    let liability_after_void = after_bind.panel_liability(&id).is_some();
    // Advance past the abandon hold: reservation released in full.
    let after_hold = fold_t12(&p, &after_bind, &ctx(0x5002, 2_000 + sp.window_bind() + sp.fp_abandon_hold_daa() + 2, 5_002, 0), &[])
        .expect("abandon hold releases");
    let reserved_after_hold = after_hold.reserved_exposure(&bond);
    // Advance past retirement: the claim record is dropped.
    let after_retire = fold_t12(
        &p,
        &after_hold,
        &ctx(0x5003, 2_000 + sp.window_bind() + sp.fp_abandon_hold_daa() + sp.claim_retirement_daa() + 3, 5_003, 0),
        &[],
    )
    .expect("retirement sweeps");
    let claim_gone = after_retire.claim(&id).is_none();
    let liability_after_retire = after_retire.panel_liability(&id).is_some();
    let prompt_row_after_retire = after_retire.fp_claimed_prompt_ids_of(&floor, 2_000 + sp.window_bind() + sp.fp_abandon_hold_daa() + sp.claim_retirement_daa() + 3).len();
    let collateral_after = after_retire.bond(&bond).unwrap().collateral;

    println!("\n=== lifecycle of ONE abandoned commitment (no panel, no material served) ===");
    println!("reserved at accept                = {reserved_at_accept} sompi; after bind window: phase {void_phase}");
    println!("void reason is BindTimeout (no slash) = {voided_no_slash}; reserved while on abandon hold = {reserved_on_hold}");
    println!("panel_liability row after void    = {liability_after_void} (objective_offence armed on t12)");
    println!("reserved after abandon hold        = {reserved_after_hold} (released in full)");
    println!("after retirement: claim record gone = {claim_gone}; panel_liability row still rooted = {liability_after_retire}; paid-prompt rows still rooted = {prompt_row_after_retire}");
    println!("attacker collateral {collateral_before} -> {collateral_after}  (slashed {})", collateral_before - collateral_after);

    assert!(voided_no_slash, "an abandoned FP commitment voids at BindTimeout, not by slash");
    assert_eq!(reserved_after_hold, 0, "the reservation is returned in full — no collateral is spent");
    assert_eq!(collateral_after, collateral_before, "the attacker loses no collateral");
    assert!(claim_gone, "the claim record is pruned at retirement");
    // The residue the finding named: a liability row the retired claim left rooted for ever. Closed by
    // 2026-09-24 DoS audit #12 (a) — a `BindTimeout` no `Valid` signed writes no liability row past
    // the audit fence (and a liability that is written is pruned past its evidence horizon).
    assert!(!liability_after_void, "#12 (a): an abandoned FP commitment writes no panel_liability row at its BindTimeout");
    assert!(!liability_after_retire, "#12 (a): and none outlives the claim");
    assert!(prompt_row_after_retire >= 1, "and a PERMANENT paid-prompt row (compute-path only; retained ~100 epochs, outlives the claim)");

    // ---- 4. the amplification ------------------------------------------------------------------
    println!("\n=== amplification (defender cost / attacker cost) ===");
    println!("attacker per claim  : carrier fee (NODE POLICY; 0 on a self-mined block) + reserve {first_reserved} sompi locked ~{} DAA and RETURNED + 0 slashed",
        sp.window_bind() + sp.fp_abandon_hold_daa());
    println!("defender per claim  : {per_claim_bytes} rooted B on every node re-hashed into state_root each block for ~{life} DAA,");
    println!("                      + a ~100-epoch paid-prompt row that OUTLIVES the claim (no liability row since #12 (a)),");
    println!("                      + an O(claims) fp_accounted_prefix_tokens_v2 scan on every new commitment,");
    println!("                      + the whole-carriage tip rewrite (processor.rs:2127) each chain block.");
    println!("ONE-TIME gate the finding omitted: the FP compute lane is inert until a permissionless FamilyCertified drill");
    println!("  (real grading) + ClassLaneCertified publish the floor's fp_work_profile; after that any bond floods.");
}
