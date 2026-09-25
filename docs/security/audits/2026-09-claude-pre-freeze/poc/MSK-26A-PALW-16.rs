//! MSK-26A-PALW-16 — Free-prompt prefix-accounting rows are written at commitment for Provisional
//! claims and removed only on expiry, never on void, so unverified or junk commitments reduce
//! other producers' credit on any shared prefix.
//!
//! Audit commit : 3d2bd6dc5d77d37396d1b73c4c13526090923b92
//! Crate        : kaspa-consensus-core (consensus/core)
//! Command      :
//!   cp docs/security/audits/2026-09-claude-pre-freeze/poc/MSK-26A-PALW-16.rs \
//!      consensus/core/tests/audit_poc_msk_26a_palw_16.rs \
//!   && cargo test -p kaspa-consensus-core --test audit_poc_msk_26a_palw_16 -- --nocapture ; \
//!   rm consensus/core/tests/audit_poc_msk_26a_palw_16.rs
//!
//! PASS = the vulnerable behaviour is present: on testnet-12's shipped params, a junk
//! `FreePromptCommitted` from a griefer bond that is voided at `BindTimeout` (never bound, never
//! licensed, never paid, no slash) leaves its `fp_claim_prompts` row behind, and that row lowers the
//! credited compute (pwu) of a later HONEST commitment from a different bond that shares the prefix.
//! A second test shows the same-block front-run of an identical prompt copied from a mempool carrier.
//! Once the bug is fixed (a voided / never-certified claim's row is dropped or never activated) the
//! "attacked" pwu equals the clean pwu and these tests fail.
//!
//! Fixture: `dos_l5_common.rs` (testnet-12 `Params::from(testnet-12)` = `palw_t12_shipped_params()`,
//! its genesis fold, and the processor's extras). Exactly as `dos_repro_1` / `dos_repro_2` do, two
//! permissionless prerequisites of the FP lane are injected through the carriage (the pinned BASE-0
//! FreePrompt family a `FamilyCertified` drill writes, and the floor's registry lifecycle row); every
//! measured step then runs through the real `apply_palw_transition_v7`.

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, mainnet_shipped_params};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwCertifiedFamilyStateV2, PalwCertifiedLaneV1, PalwChainStateV2,
    PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateCarriageV2, PalwStateV2Error, PalwTransitionExtrasV1, PalwVoidReasonV2,
    apply_palw_transition_v7,
};

/// The processor's t12 FP extras (`palw_transition_extras_for` resolves both from the armed fences).
fn t12_extras(p: &Params, daa: u64) -> PalwTransitionExtrasV1 {
    let mut e = extras(p, daa);
    e.fp_derived_work_daa = p.palw_fp_derived_work_fence().map(|f| f.daa_score());
    e.fp_da_pins_active = p.palw_fp_da_pins_fence().is_some_and(|f| f.is_active(daa));
    e
}

fn fold_at(
    p: &Params,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let sp = bundle(p).state;
    let f = flags(p, ctx.daa_score);
    let e = t12_extras(p, ctx.daa_score);
    apply_palw_transition_v7(
        parent,
        &sp,
        None,
        ctx,
        objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &e,
    )
    .map(|(s, _, _)| s)
}

fn floor_profile() -> Box<kaspa_consensus_core::palw_step::PalwShapeProfileV3> {
    Box::new(
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor profile"),
    )
}

fn floor_work() -> kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1 {
    let fp = floor_profile();
    let (pf, dc) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&fp, pf, dc);
    kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&fp, &job).expect("floor work derives")
}

/// Same shim as dos_repro_1 / dos_repro_2: the pinned BASE-0 FreePrompt family and the floor's
/// registry lifecycle row, both real state objects the live network writes permissionlessly.
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

#[allow(clippy::too_many_arguments)]
fn fp_commit(
    class_id: Hash64,
    bond: PalwBondKeyV2,
    pk: Vec<u8>,
    work_leaves: u64,
    prompt: &[u32],
    decode: u32,
    claim: Hash64,
    salt: u64,
) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::FreePromptCommitted {
        job_pin: kaspa_hashes::Hash64::default(),
        claim,
        class_id,
        bond,
        executor_pubkey: pk,
        work_leaves,
        prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(prompt),
        prompt_tokens: prompt.len() as u32,
        prompt_token_ids: prompt.to_vec(),
        decode_tokens_executed: decode,
        // Junk roots for the griefer: the fold checks none of them against an execution.
        trace_root: h(0x71_0000 + salt),
        output_root: h(0x72_0000 + salt),
        execution_root: h(0x73_0000 + salt),
        trace_chunk_count: 1,
        trace_retention_daa: 9_999_999,
        consumed_prefix_state: kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(class_id),
    }
}

fn leaves_of(s: &PalwChainStateV2, floor: &Hash64, pt: u32, dc: u32) -> u64 {
    let ladder = s.class_step_ladder_v1(floor, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
    kaspa_consensus_core::palw_step::step_leaf_count_of_tokens_capped_v1(&floor_profile(), pt, dc, ladder).expect("leaves derive")
}

const GRIEFER: u64 = 1;
const HONEST: u64 = 2;

/// testnet-12 as shipped, both bonds registered, the floor's FP profile published (DAA 1,001).
fn setup(p: &Params) -> (PalwChainStateV2, Hash64) {
    // Reachability: the price path and the row write are armed from genesis on testnet-12.
    assert_eq!(p.palw_fp_derived_work_fence().map(|f| f.daa_score()), Some(0), "t12 arms palw_fp_derived_work at DAA 0");
    assert_eq!(p.palw_canonical_work_daa(), Some(0), "t12 arms palw_canonical_work at DAA 0");
    let (floor, _leaves, _target, _slash) = genesis_classes(p)[0];
    let g = genesis_state(p);
    let collateral = at_least_the_floor(p, 100_000_000_000);
    let s = fold_at(p, &g, &ctx(1, 1_000, 1, 0), &[bond_obj(GRIEFER, collateral), bond_obj(HONEST, collateral)]).expect("bonds register");
    let s = seed_prereqs(p, &s);
    let publish = PalwConsensusObjectV2::ClassLaneCertified { class_id: floor, lane: PalwCertifiedLaneV1::FreePrompt, profile: floor_profile() };
    let s = fold_at(p, &s, &ctx(2, 1_001, 2, 0), &[publish]).expect("ClassLaneCertified publishes the floor's profile");
    assert!(s.fp_work_profile_of(&floor).is_some());
    (s, floor)
}

/// The honest producer's prompt: a 480-token shared prefix (a chat template / system prompt) plus
/// 32 of its own tokens, 256 decoded.
fn honest_prompt() -> Vec<u32> {
    let mut ids: Vec<u32> = (0..480u32).map(|i| 1_000 + i).collect();
    ids.extend((0..32u32).map(|i| 90_000 + i));
    ids
}

/// **Template-prefix griefing, voided by BindTimeout.** The griefer commits only the 480-token prefix
/// with junk roots, never serves anything, and its claim voids at `BindTimeout` with no slash. Its
/// row survives, and 100 epochs of honest commitments on that prefix are credited less.
#[test]
fn msk_26a_palw_16_voided_junk_commitment_discounts_honest_claim() {
    let p = t12();
    let (s0, floor) = setup(&p);
    let window_bind = bundle(&p).state.window_bind();
    let hold = bundle(&p).state.fp_abandon_hold_daa();
    let epoch_length = bundle(&p).state.epoch_length();
    println!("t12: window_bind={window_bind} fp_abandon_hold_daa={hold} epoch_length={epoch_length}");

    let honest_ids = honest_prompt();
    let (hpt, hdc) = (honest_ids.len() as u32, 256u32);
    let honest_leaves = leaves_of(&s0, &floor, hpt, hdc);

    // The griefer's commitment: the public 480-token prefix, decode 64, junk roots.
    let griefer_ids: Vec<u32> = honest_ids[..480].to_vec();
    let (gpt, gdc) = (griefer_ids.len() as u32, 64u32);
    let griefer_leaves = leaves_of(&s0, &floor, gpt, gdc);
    let griefer_claim = h(0x6121_EF00);
    let griefer_bond = bond_key(GRIEFER);
    let griefer_collateral_before = s0.bond(&griefer_bond).unwrap().collateral;
    let griefer_slashed_before = s0.bond(&griefer_bond).unwrap().slashed;

    // --- attacked branch ---------------------------------------------------------------------
    let commit_daa = 1_002u64;
    let s1 = fold_at(
        &p,
        &s0,
        &ctx(3, commit_daa, 3, 0),
        &[fp_commit(floor, griefer_bond, pubkey_of(GRIEFER), griefer_leaves, &griefer_ids, gdc, griefer_claim, 0xA)],
    )
    .expect("the griefer's junk commitment is accepted");
    assert!(matches!(s1.claim(&griefer_claim).unwrap().phase, PalwClaimPhaseV2::Provisional));
    {
        let gc = s1.claim(&griefer_claim).unwrap();
        println!(
            "griefer claim: reserved = {} sompi ({:.6} MSK), rights_reserved = {} sompi ({:.6} MSK) — the most a void could forfeit before any S1/S2 action",
            gc.reserved,
            msk(gc.reserved),
            gc.rights_reserved,
            msk(gc.rights_reserved)
        );
    }
    assert_eq!(s1.fp_claimed_prompt_ids_of(&floor, commit_daa + 1).len(), 1, "a row is written for a Provisional claim");

    // No panel ever binds; the bind window closes and the claim voids at BindTimeout.
    let void_daa = commit_daa + window_bind + 1;
    let s2 = fold_at(&p, &s1, &ctx(4, void_daa, 4, 0), &[]).expect("bind window closes");
    match s2.claim(&griefer_claim).map(|c| c.phase.clone()) {
        Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }) | None => {}
        other => panic!("expected the griefer's claim voided at BindTimeout, got {other:?}"),
    }
    // Past the abandon hold too, so the griefer has its whole reservation back.
    let release_daa = void_daa + hold + 1;
    let s3 = fold_at(&p, &s2, &ctx(5, release_daa, 5, 0), &[]).expect("abandon hold elapses");
    let gb = s3.bond(&griefer_bond).unwrap();
    assert_eq!(gb.collateral, griefer_collateral_before, "no collateral taken from the griefer");
    assert_eq!(gb.slashed, griefer_slashed_before, "the griefer is not slashed");
    assert_eq!(s3.reserved_exposure(&griefer_bond), 0, "the griefer's exposure is fully released");
    let rows = s3.fp_claimed_prompt_ids_of(&floor, release_daa + 1);
    assert_eq!(rows.len(), 1, "THE BUG: the voided, never-bound, never-paid claim's prompt row survives");
    assert_eq!(rows[0], griefer_ids.as_slice());

    let honest_daa = release_daa + 1;
    let honest_claim = h(0x40E5_7000);
    let honest_commit = fp_commit(floor, bond_key(HONEST), pubkey_of(HONEST), honest_leaves, &honest_ids, hdc, honest_claim, 0xB);
    let s4 = fold_at(&p, &s3, &ctx(6, honest_daa, 6, 0), std::slice::from_ref(&honest_commit)).expect("the honest commitment is accepted");
    let attacked_pwu = s4.claim(&honest_claim).unwrap().pwu;

    // --- clean branch: identical blocks, without the griefer's commitment ---------------------
    let c1 = fold_at(&p, &s0, &ctx(3, commit_daa, 3, 0), &[]).unwrap();
    let c2 = fold_at(&p, &c1, &ctx(4, void_daa, 4, 0), &[]).unwrap();
    let c3 = fold_at(&p, &c2, &ctx(5, release_daa, 5, 0), &[]).unwrap();
    let c4 = fold_at(&p, &c3, &ctx(6, honest_daa, 6, 0), std::slice::from_ref(&honest_commit)).expect("the honest commitment is accepted");
    let clean_pwu = c4.claim(&honest_claim).unwrap().pwu;

    let full = kaspa_consensus_core::palw_freeprompt_v3::fp_derive_credited_compute_v1(&floor_profile(), hpt, hdc, 0).unwrap();
    let after = kaspa_consensus_core::palw_freeprompt_v3::fp_derive_credited_compute_v1(&floor_profile(), hpt, hdc, 480).unwrap();
    println!("honest claim credited pwu: clean = {clean_pwu}, after voided junk prefix = {attacked_pwu}");
    println!("derived credited compute: accounted 0 = {full}, accounted 480 = {after}");
    println!("honest producer keeps {:.1}% of its credit", attacked_pwu as f64 * 100.0 / clean_pwu as f64);
    assert!(attacked_pwu < clean_pwu, "THE BUG: a voided junk commitment lowered an honest claim's credit");
    assert!(attacked_pwu as u128 <= after && clean_pwu as u128 <= full);
}

/// **Front-running an honest carrier from the mempool.** The griefer copies the honest commitment's
/// public prompt ids exactly and submits from another bond (a different work_id, so no DuplicateWork),
/// ordered ahead of it. The honest claim is credited only its decode half.
#[test]
fn msk_26a_palw_16_front_run_identical_prompt_credits_honest_decode_only() {
    let p = t12();
    let (s0, floor) = setup(&p);
    let ids = honest_prompt();
    let (pt, dc) = (ids.len() as u32, 256u32);
    let leaves = leaves_of(&s0, &floor, pt, dc);
    let honest_claim = h(0x40E5_7001);
    let griefer_claim = h(0x6121_EF01);
    let honest = fp_commit(floor, bond_key(HONEST), pubkey_of(HONEST), leaves, &ids, dc, honest_claim, 0xB);
    let junk = fp_commit(floor, bond_key(GRIEFER), pubkey_of(GRIEFER), leaves, &ids, dc, griefer_claim, 0xA);

    let clean = fold_at(&p, &s0, &ctx(3, 1_002, 3, 0), std::slice::from_ref(&honest)).expect("honest alone");
    let raced = fold_at(&p, &s0, &ctx(3, 1_002, 3, 0), &[junk, honest]).expect("both accepted");
    let clean_pwu = clean.claim(&honest_claim).unwrap().pwu;
    let raced_pwu = raced.claim(&honest_claim).unwrap().pwu;
    let decode_only = kaspa_consensus_core::palw_freeprompt_v3::fp_derive_credited_compute_v1(&floor_profile(), pt, dc, pt).unwrap();
    println!("front-run: honest pwu clean = {clean_pwu}, behind an identical junk commitment = {raced_pwu} (decode-only bound {decode_only})");
    println!("honest producer keeps {:.1}% of its credit", raced_pwu as f64 * 100.0 / clean_pwu as f64);
    assert!(raced_pwu < clean_pwu, "THE BUG: a junk copy of the carrier's prompt lowered the honest claim's credit");
    assert!(raced_pwu as u128 <= decode_only, "the honest claim is paid only its decode half");
}

/// Mainnet preset: the price path is not armed (reachability only).
#[test]
fn msk_26a_palw_16_mainnet_preset_has_no_derived_work_fence() {
    let m = mainnet_shipped_params();
    println!("mainnet_shipped_params: palw_fp_derived_work = {:?}", m.palw_fp_derived_work_fence());
    assert!(m.palw_fp_derived_work_fence().is_none());
}
