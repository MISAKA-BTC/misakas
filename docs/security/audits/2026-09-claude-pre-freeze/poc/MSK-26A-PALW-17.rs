//! MSK-26A-PALW-17 — Prefix-accounting rows store only prompt token ids, not committed outputs,
//! so a continuation prompt that contains the bond's own previous answer is credited that
//! answer's prefill again.
//!
//! Audit commit: 3d2bd6dc5d77d37396d1b73c4c13526090923b92
//! Crate:        kaspa-consensus-core (consensus/core)
//! Command:
//!   cp docs/security/audits/2026-09-claude-pre-freeze/poc/MSK-26A-PALW-17.rs \
//!      consensus/core/tests/audit_poc_msk_26a_palw_17.rs && \
//!   cargo test -p kaspa-consensus-core --test audit_poc_msk_26a_palw_17 -- --nocapture ; \
//!   rm consensus/core/tests/audit_poc_msk_26a_palw_17.rs
//!
//! PASS = the vulnerable behaviour is present: on testnet-12's shipped params
//! (`Params::from(testnet-12)` == `palw_t12_shipped_params()`), through the REAL fold
//! (`apply_palw_transition_v7`, compute-priced free-prompt path, `palw_fp_derived_work` and
//! `palw_canonical_work` armed at DAA 0, `palw_fp_decode_rules` dormant):
//!   * the `PalwFpClaimPromptV1` row the fold writes for claim 1 holds claim 1's PROMPT ids only;
//!   * claim 2, whose prompt is claim 1's prompt followed by claim 1's d output tokens, is read as
//!     having an accounted prefix of |P| (not |P| + d - 1), so its credited compute contains the
//!     prefill of the d-1 positions claim 1 was already paid for as DECODE positions;
//!   * cutting one run (P, 2d) into two claims (P, d) + (P||O, d) — the SAME KV computation for a
//!     producer that kept its cache — is paid strictly more than the uncut run, contradicting
//!     ADR-0145 I2 ("commitment segmentation") and §8 "Cache correctness".
//! A second test measures the magnitude on testnet-12's genesis 8k dense class profile (the one
//! carried in the bundle's genesis `ClassRegistered`), via the same public pricing functions the
//! fold calls. The tests FAIL once rows (or the reading) account for committed output positions.

#[path = "dos_l5_common.rs"]
#[allow(dead_code)]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_STRUCTURAL_WORK_LEAVES_CAP, PalwFpPrefixStateV1, fp_accounted_prefix_tokens_v2, fp_derive_credited_compute_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwCertifiedFamilyStateV2, PalwCertifiedLaneV1, PalwChainStateV2,
    PalwConsensusObjectV2, PalwStateCarriageV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v7,
};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_of_tokens_capped_v1};

const ATTACKER: u64 = 1;

fn floor_profile() -> PalwShapeProfileV3 {
    kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
        .expect("floor profile")
}

fn floor_work() -> kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1 {
    let fp = floor_profile();
    let (pf, dc) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&fp, pf, dc);
    kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&fp, &job).expect("floor work derives")
}

/// The processor's t12 extras for the FP lane (as `dos_repro_2_free_prompt_flood_linear_per_block_cost`).
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
        &t12_extras(p, ctx.daa_score),
    )
    .map(|(s, _, _)| s)
}

/// Setup shim identical to `dos_repro_2`: the pinned BASE-0 free-prompt family and the floor's
/// registry row, which a live drill / execution-lane span would write with the same values.
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

fn attacker_pubkey() -> Vec<u8> {
    let mut v = vec![0xA7u8; kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN];
    v[..8].copy_from_slice(&ATTACKER.to_le_bytes());
    v
}

fn attacker_bond(p: &Params) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(ATTACKER),
        pubkey: attacker_pubkey(),
        operator_pubkey: operator_pubkey_of(ATTACKER),
        collateral: at_least_the_floor(p, 10_000_000_000_000),
        payout_payload: h(0x9A00 + ATTACKER),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

/// A free-prompt commitment for `ids` with `decode` generated tokens, `work_leaves` derived from
/// the class graph exactly as the fold re-derives it (so the fold's equality check passes).
fn fp_commit(s: &PalwChainStateV2, class_id: Hash64, bond: PalwBondKeyV2, ids: Vec<u32>, decode: u32, tag: u64) -> PalwConsensusObjectV2 {
    let profile = s.fp_work_profile_of(&class_id).expect("published").clone();
    let ladder = s.class_step_ladder_v1(&class_id, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
    let work_leaves = step_leaf_count_of_tokens_capped_v1(&profile, ids.len() as u32, decode, ladder).expect("derivable");
    PalwConsensusObjectV2::FreePromptCommitted {
        job_pin: Hash64::default(),
        claim: h(0xF0_0000_0000 + tag),
        class_id,
        bond,
        executor_pubkey: attacker_pubkey(),
        work_leaves,
        prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&ids),
        prompt_tokens: ids.len() as u32,
        prompt_token_ids: ids,
        decode_tokens_executed: decode,
        trace_root: h(0x71_0000_0000 + tag),
        output_root: h(0x72_0000_0000 + tag),
        execution_root: h(0x73_0000_0000 + tag),
        trace_chunk_count: 1,
        trace_retention_daa: 9_999_999,
        consumed_prefix_state: PalwFpPrefixStateV1::genesis(class_id),
    }
}

/// `pwu` as the fold quantizes it: `floor(credited / quanta) × quanta`.
fn quantized(credited: u128, quantum: u128) -> u128 {
    let quanta = (credited / quantum).min(1 << 16);
    (credited / quanta) * quanta
}

#[test]
fn msk_26a_palw_17_continuation_is_paid_the_previous_answer_again_on_the_t12_fold() {
    let p = t12();
    // Reachability: the shipped testnet-12 ruleset arms both pricing fences at DAA 0 and leaves
    // the decode-only rule dormant.
    assert_eq!(p.palw_fp_derived_work_fence().map(|f| f.daa_score()), Some(0), "fp_derived_work armed at genesis");
    assert_eq!(p.palw_canonical_work_daa(), Some(0), "canonical_work armed at genesis");
    assert!(p.palw_fp_decode_rules_fence().is_none(), "fp_decode_rules dormant");

    // --- setup: attacker bond, floor's FP profile published by the real ClassLaneCertified fold ---
    let (floor, _l, _t, _s) = genesis_classes(&p)[0];
    let g = genesis_state(&p);
    let s = fold_t12(&p, &g, &ctx(1, 1_000, 1, 0), &[attacker_bond(&p)]).expect("bond registers");
    let s = seed_prereqs(&p, &s);
    let publish = PalwConsensusObjectV2::ClassLaneCertified {
        class_id: floor,
        lane: PalwCertifiedLaneV1::FreePrompt,
        profile: Box::new(floor_profile()),
    };
    let base = fold_t12(&p, &s, &ctx(2, 1_001, 2, 0), &[publish]).expect("floor FP profile publishes");
    let profile = base.fp_work_profile_of(&floor).expect("floor profile rooted").clone();
    let n_ctx = profile.n_ctx;
    let bond = bond_key(ATTACKER);

    // A conversation on the floor's graph (n_ctx 12): prompt P of 2 tokens, d = 5 generated.
    let p_ids: Vec<u32> = vec![11, 12];
    let d: u32 = 5;
    // The producer's own answer O to (P, d). The chain never sees these ids — only output_root.
    let o_ids: Vec<u32> = vec![21, 22, 23, 24, 25];
    let mut p_o = p_ids.clone();
    p_o.extend_from_slice(&o_ids);
    let pl = p_ids.len() as u32;
    assert!(pl + 2 * d - 1 <= n_ctx, "the uncut run fits the class context");

    // --- path A: one uncut run (P, 2d) --------------------------------------------------------
    let uncut = fp_commit(&base, floor, bond, p_ids.clone(), 2 * d, 100);
    let sa = fold_t12(&p, &base, &ctx(3, 1_002, 3, 0), &[uncut]).expect("uncut run folds");
    let pwu_uncut = sa.claim(&h(0xF0_0000_0000 + 100)).expect("claim A").pwu as u128;

    // --- path B: the same run cut into (P, d) then (P || O, d) by the same bond ----------------
    let c1 = fp_commit(&base, floor, bond, p_ids.clone(), d, 1);
    let s1 = fold_t12(&p, &base, &ctx(3, 1_002, 3, 0), &[c1]).expect("claim 1 folds");
    let pwu1 = s1.claim(&h(0xF0_0000_0000 + 1)).expect("claim 1").pwu as u128;

    // The row the fold wrote for claim 1: the prompt ids only — nothing about O.
    let rows: Vec<Vec<u32>> = s1.fp_claimed_prompt_ids_of(&floor, 1_003).into_iter().map(|r| r.to_vec()).collect();
    println!("rows after claim 1 = {rows:?}");
    assert_eq!(rows, vec![p_ids.clone()], "the paid-prompt row records P only, not P || O[..d-1]");
    let row_refs: Vec<&[u32]> = rows.iter().map(|r| r.as_slice()).collect();
    let accounted = fp_accounted_prefix_tokens_v2(&p_o, &row_refs);
    assert_eq!(accounted, pl, "the chain's reading of claim 2's paid prefix is |P|, not |P| + d - 1");

    let c2 = fp_commit(&s1, floor, bond, p_o.clone(), d, 2);
    let s2 = fold_t12(&p, &s1, &ctx(4, 1_003, 4, 0), &[c2]).expect("claim 2 (P || O) folds");
    let pwu2 = s2.claim(&h(0xF0_0000_0000 + 2)).expect("claim 2").pwu as u128;

    // What claim 2 costs a producer that kept claim 1's KV cache: positions |P|+1..|P|+d-1 are
    // already in its cache (they were claim 1's decode positions), so only the last output token
    // is new prefill — the same derivation with the reused prefix at |P| + d - 1.
    let quantum = {
        use kaspa_consensus_core::palw_state_v2::PalwFpPricingV1;
        PalwFpPricingV1::of(&bundle(&p).state, p.palw_canonical_work_daa()).network_quantum(&base)
    };
    let credited2_chain = fp_derive_credited_compute_v1(&profile, pl + d, d, pl).unwrap();
    let credited2_cache = fp_derive_credited_compute_v1(&profile, pl + d, d, pl + d - 1).unwrap();
    let credited1 = fp_derive_credited_compute_v1(&profile, pl, d, 0).unwrap();
    let credited_uncut = fp_derive_credited_compute_v1(&profile, pl, 2 * d, 0).unwrap();
    assert_eq!(pwu2, quantized(credited2_chain, quantum), "the fold paid claim 2 the |P|-accounted derivation");
    assert_eq!(pwu1, quantized(credited1, quantum));
    assert_eq!(pwu_uncut, quantized(credited_uncut, quantum));
    // The cached cut performs exactly the uncut run's computation (same positions, same KV reads/writes).
    assert_eq!(credited1 + credited2_cache, credited_uncut, "cut-with-cache == uncut, in the chain's own unit");

    println!("network quantum (MAC-eq)           = {quantum}");
    println!("uncut (P,2d) pwu                   = {pwu_uncut}");
    println!("cut: claim1 (P,d) pwu              = {pwu1}");
    println!("cut: claim2 (P||O,d) pwu           = {pwu2}  (cache-holder's real compute {credited2_cache})");
    println!(
        "cut total / uncut                  = {:.4}  (identical KV computation)",
        (pwu1 + pwu2) as f64 / pwu_uncut as f64
    );
    println!("claim2 credited / real (cache)     = {:.4}", credited2_chain as f64 / credited2_cache as f64);

    // THE BAD OUTCOME: the same run, cut at an answer boundary, is paid more than uncut, and
    // claim 2 is paid more than the compute a cache-holding producer performs.
    assert!(pwu1 + pwu2 > pwu_uncut, "segmentation at an answer boundary raises the credit");
    assert!(pwu2 > credited2_cache, "claim 2 is credited the prefill of positions already paid as claim 1's decode");
}

#[test]
fn msk_26a_palw_17_magnitude_on_the_t12_8k_dense_genesis_class() {
    let p = t12();
    // The 8k dense class's profile exactly as testnet-12's genesis bundle carries it.
    let b = bundle(&p);
    let profile = b
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { admission: Some(carriage), .. } if carriage.profile.n_ctx == 8_192 => {
                Some(carriage.profile.clone())
            }
            _ => None,
        })
        .expect("testnet-12 genesis registers an 8k class");
    let ladder_ref = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v7(8_192).expect("8k v7 row");
    println!(
        "genesis 8k class id {} (a16 v7@8192 graph id {}; same kernels/dims: {})",
        profile.shape_profile_id(),
        ladder_ref.shape_profile_id(),
        profile.n_ctx == ladder_ref.n_ctx
    );
    // Whether some free-prompt family this build pins covers the 8k class's kernels — the
    // `ClassLaneCertified` rule (`reachable ⊆ family.kernel_ids`) that opens its FP lane.
    let reachable = kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&profile);
    let covering: Vec<_> = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_fp_certified_families_v1()
        .into_iter()
        .filter(|f| reachable.is_subset(&f.kernel_ids))
        .map(|f| f.family_id)
        .collect();
    println!("8k class: pinned free-prompt families covering its kernels = {}", covering.len());

    // A self-generated conversation: first prompt 256 tokens, 1,024 generated per turn
    // (max_decode_tokens), each next prompt = previous prompt || previous answer.
    let (p0, d, turns) = (256u32, 1_024u32, 6u32);
    let (max_prompt, max_decode) = (b.freeprompt.max_prompt_tokens(), b.freeprompt.max_decode_tokens());
    let one_tx_ids = kaspa_consensus_core::palw_mode_v2::PALW_STANDARD_TX_BYTES / 4;
    println!("t12 ruleset: max_prompt_tokens {max_prompt}, max_decode_tokens {max_decode}, held one-tx id cap {one_tx_ids}");
    assert!(d <= max_decode, "decode per claim within the shipped ruleset");
    assert!(((p0 + (turns - 1) * d) as u64) <= one_tx_ids.min(max_prompt as u64), "every turn's prompt rides one standard tx");
    assert!(p0 + turns * d <= profile.n_ctx, "the whole conversation fits the 8k context");
    let mut prompt_len = p0;
    let mut rows: Vec<Vec<u32>> = Vec::new();
    let mut ids: Vec<u32> = (0..p0).map(|i| 1_000 + i).collect();
    let (mut paid_total, mut real_total) = (0u128, 0u128);
    for t in 0..turns {
        let row_refs: Vec<&[u32]> = rows.iter().map(|r| r.as_slice()).collect();
        let accounted = fp_accounted_prefix_tokens_v2(&ids, &row_refs);
        let paid = fp_derive_credited_compute_v1(&profile, prompt_len, d, accounted).unwrap();
        // Cache-holder: everything but the previous turn's last sampled token is already in KV.
        let cached = if t == 0 { 0 } else { prompt_len - 1 };
        let real = fp_derive_credited_compute_v1(&profile, prompt_len, d, cached).unwrap();
        println!(
            "turn {t}: prompt {prompt_len:>5} accounted {accounted:>5} (cache holds {cached:>5})  paid/real = {:.3}",
            paid as f64 / real as f64
        );
        if t > 0 {
            assert_eq!(accounted, prompt_len - d, "the chain accounts only the previous PROMPT");
            assert!(paid * 100 > real * 150, "a continuation turn is credited > 1.5x the cache-holder's compute");
        }
        paid_total += paid;
        real_total += real;
        rows.push(ids.clone());
        // Next prompt: this prompt || this turn's answer (ids the chain never sees).
        ids.extend((0..d).map(|i| 50_000 + t * d + i));
        prompt_len += d;
    }
    let ratio = paid_total as f64 / real_total as f64;
    println!("{turns} turns: paid {paid_total} vs real {real_total} -> {ratio:.3}x");
    assert!(ratio > 1.5, "a looped self-continuation is paid > 1.5x its real compute");
}
