//! **PALW testnet-12 adversarial-audit harness probe.**
//!
//! Every block below was COMPILED AND RUN in this worktree. It exists so an auditor can copy a
//! section into a new `consensus/core/tests/<name>.rs` and have a working reproduction in one step,
//! instead of rediscovering which builder is pub and which conversion is missing.
//!
//! Run:  cargo test -p kaspa-consensus-core --test audit_harness_probe -- --nocapture

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_base_params, palw_t12_shipped_params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_canonical_work_v1::{
    PalwCanonicalClassDescriptorV1, PalwCanonicalExecutionFactsV1, PalwCanonicalWorkVectorV1, palw_canonical_draw_work_v1,
    palw_canonical_work_v1, palw_claim_canonical_pwu_v1,
};
use kaspa_consensus_core::palw_chain_weight::{PalwBlockWeightV1, PalwChainWeightParamsV1, chain_weights_v1, compare_tips_v1};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1, palw_attempted_compute_per_claim_v1,
    palw_expected_attempts_q32_v1, palw_job_economic_compute_v1, palw_network_expected_attempts_q32_v1,
};
use kaspa_consensus_core::palw_economic_payout_v1::{
    PALW_ECONOMIC_PAYOUT_DEVNET_V1, PalwEconomicPayoutFoldV1, palw_attempted_ccu_v1, palw_cap_utilization_permille_v1,
    palw_network_draws_q32_from_bits_v1, palw_panel_share_permille_v1,
};
use kaspa_consensus_core::palw_execution_quanta_v1::{
    PALW_EXECUTION_QUANTUM_V1, palw_execution_canonical_work_id_v1, palw_execution_quantum_count_v1,
};
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1, palw_ticket_admits_v1};
use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_1_5B, PalwQwen25GeometryV1, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7,
};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_weight::PalwWorkRampStageV1;

// =================================================================================================
// 0. Auto-discovery
// =================================================================================================

/// **Item 1.** A new file under `consensus/core/tests/` is auto-discovered. No `Cargo.toml` edit,
/// no `[[test]]` stanza. Cargo's autodiscovery (`autotests`, default true on edition 2018+) makes
/// every `tests/*.rs` its own test binary named after the file.
#[test]
fn a_new_test_file_is_auto_discovered() {
    println!("audit_harness_probe: auto-discovery works, no Cargo.toml edit needed");
    assert_eq!(2 + 2, 4);
}

// =================================================================================================
// 1. The t12 Params, three ways
// =================================================================================================

/// **Three routes to testnet-12's `Params`, and they are NOT all the same object.**
#[test]
fn the_three_routes_to_t12_params() {
    let shipped: Params = palw_t12_shipped_params();
    let base: Params = palw_t12_base_params();
    let from_id: Params = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));

    println!("\n  palw_t12_shipped_params()  net={}", shipped.net);
    println!("  palw_t12_base_params()     net={}", base.net);
    println!("  Params::from(testnet-12)    net={}", from_id.net);
    println!("  shipped consensus_params_id {}", shipped.consensus_params_id());
    println!("  base    consensus_params_id {}", base.consensus_params_id());

    // `From<NetworkId>` routes to `palw_t12_shipped_params()` through `with_registered_models`,
    // which only rewrites `dns_params.vlt.model_cost_table` when the VLT shadow is active.
    assert_eq!(from_id.consensus_params_id(), shipped.consensus_params_id(), "From<NetworkId> == shipped, for fingerprint purposes");

    // `base` is the pre-genesis-card preset: no class rows, no genesis bonds, no premine UTXOs.
    // Auditing the LIVE economy means `shipped` (or `From`). Auditing a rule in isolation may
    // prefer `base`.
    // `base` is the pre-genesis-card preset: no class rows, no genesis bonds, no premine UTXOs.
    // Auditing the LIVE economy means `shipped` (or `From`). Auditing a rule in isolation may
    // prefer `base`. They are DIFFERENT objects:
    assert_ne!(base.consensus_params_id(), shipped.consensus_params_id(), "base != shipped");
}

// =================================================================================================
// 2. The three t12 class profiles, built from pub builders only
// =================================================================================================

/// The BASE-0 integer floor class. `(prefill, decode) = PALW_RC_BASE0_CANONICAL = (8, 4)`.
pub fn floor_profile() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the shipped BASE-0 floor builds")
}

/// **t12's held DENSE class: Qwen2.5-1.5B @ n_ctx = 2,097,152 (2M).**
/// t12 uses the `_v7` artifact-row builder — NOT `palw_a16_context_row_profile_v5`, which is the
/// t11 @512 row.
pub fn dense_2m_profile() -> PalwShapeProfileV3 {
    qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }).expect("the t12 dense @2M row builds")
}

/// **t12's held HYBRID class: Qwen3.6-35B-A3B @ n_ctx = 512.**
/// Note `qwen36_geometry_artifact_eps` — the artifact's own epsilon, not the profile default.
pub fn hybrid_512_profile() -> PalwShapeProfileV3 {
    qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B }))
        .expect("the t12 hybrid @512 row builds")
}

/// The canonical job context of a profile. `rc_job_context` is PUB and is the same constructor the
/// registration builders use, so a test that calls it is building the chain's own object.
pub fn job_of(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
    rc_job_context(profile, prefill, decode)
}

/// The canonical class descriptor. `tokenizer_id` is `Hash64::default()` in every in-tree fixture.
pub fn descriptor_of(profile: &PalwShapeProfileV3) -> PalwCanonicalClassDescriptorV1<'_> {
    PalwCanonicalClassDescriptorV1::of(profile, Hash64::default()).expect("a shipped class has one weight format")
}

/// **Item 4.** All three t12 classes build from pub builders, and their canonical jobs are the
/// chain's own.
#[test]
fn the_three_t12_classes_build_from_pub_builders() {
    let floor = floor_profile();
    let dense = dense_2m_profile();
    let hybrid = hybrid_512_profile();

    let floor_job = job_of(&floor, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let dense_job = job_of(&dense, dp, dd);
    let (hp, hd) = qwen36_held_canonical_v1(512);
    let hybrid_job = job_of(&hybrid, hp, hd);

    println!("\n  class                     n_ctx        shape_profile_id      canonical (prefill,decode)");
    for (name, p, j) in [
        ("BASE-0 floor", &floor, &floor_job),
        ("Qwen2.5 dense @2M", &dense, &dense_job),
        ("Qwen3.6 hybrid @512", &hybrid, &hybrid_job),
    ] {
        println!(
            "  {name:24} {:>9}  {:.16}  ({}, {})",
            p.n_ctx,
            p.shape_profile_id(),
            j.declared_prefill_tokens,
            j.exact_decode_tokens
        );
    }
    assert_ne!(dense.shape_profile_id(), hybrid.shape_profile_id());
}

// =================================================================================================
// 3. The canonical work vector — every dimension with its UNIT
// =================================================================================================

/// **Item 4 / units.** `palw_canonical_draw_work_v1(descriptor, canonical_job, prefill_draw)`
/// returns a 10-dimension vector: 7 dimensions in MAC-equivalents, 3 in BYTES.
/// `provisional_scalar_v1()` == `arithmetic_mac_eq()` and DROPS all three byte dimensions.
#[test]
fn the_canonical_work_vector_and_its_units() {
    let dense = dense_2m_profile();
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let job = job_of(&dense, dp, dd);
    let d = descriptor_of(&dense);

    // `prefill_draw = true` is t12's rule: an attempt executes prefill + ONE generated token.
    let v: PalwCanonicalWorkVectorV1 = palw_canonical_draw_work_v1(&d, &job, true).expect("dense draw work");

    println!("\n  Qwen2.5 dense @2M, one DRAW (prefill_draw = true):");
    println!("    dense_matmul            {:>22}  MAC-eq", v.dense_matmul);
    println!("    routed_expert_matmul    {:>22}  MAC-eq", v.routed_expert_matmul);
    println!("    attention_prefill       {:>22}  MAC-eq", v.attention_prefill);
    println!("    attention_decode        {:>22}  MAC-eq", v.attention_decode);
    println!("    recurrence              {:>22}  MAC-eq", v.recurrence);
    println!("    normalization           {:>22}  MAC-eq", v.normalization);
    println!("    other_verified_ops      {:>22}  MAC-eq", v.other_verified_ops);
    println!("    weight_traffic_bytes    {:>22}  BYTES", v.weight_traffic_bytes);
    println!("    kv_read_bytes           {:>22}  BYTES", v.kv_read_bytes);
    println!("    kv_write_bytes          {:>22}  BYTES", v.kv_write_bytes);
    println!("    -> arithmetic_mac_eq()  {:>22}  MAC-eq", v.arithmetic_mac_eq());
    println!("    -> traffic_bytes()      {:>22}  BYTES", v.traffic_bytes());
    println!("    -> provisional_scalar() {:>22}  MAC-eq  (== arithmetic_mac_eq; bytes weigh 0)", v.provisional_scalar_v1());

    assert_eq!(v.provisional_scalar_v1(), v.arithmetic_mac_eq(), "the scalar collapse drops every byte dimension");

    // The documented identity: the provisional scalar equals ADR-0131's economic compute (CCU) of
    // the SAME executed job. Two names, one number — so "MAC-eq" and "CCU" are interchangeable
    // HERE and an auditor must check whether any given call site preserves that.
    let ccu = palw_attempt_economic_compute_v1(&dense, &job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("dense attempt ccu");
    println!("    palw_attempt_economic_compute_v1 -> {ccu} CCU");
    assert_eq!(v.provisional_scalar_v1(), ccu, "MAC-eq scalar == CCU of the executed job");
}

/// **The FULL canonical job vs ONE DRAW — the prefill_draw seam.**
///
/// `palw_job_economic_compute_v1` prices the job the context DECLARES (all `exact_decode_tokens`).
/// `palw_attempt_economic_compute_v1(.., prefill_draw = true)` prices what an attempt actually
/// EXECUTES (prefill + one generated token). On t12 the second is the payable one; the first is
/// what a declaration can inflate.
#[test]
fn the_declared_job_is_not_the_executed_draw() {
    println!("\n  class                  declared-job CCU        executed-draw CCU     ratio");
    let floor = floor_profile();
    let dense = dense_2m_profile();
    let hybrid = hybrid_512_profile();
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let (hp, hd) = qwen36_held_canonical_v1(512);
    for (name, p, j) in [
        ("BASE-0 floor", &floor, job_of(&floor, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1)),
        ("Qwen2.5 dense @2M", &dense, job_of(&dense, dp, dd)),
        ("Qwen3.6 hybrid @512", &hybrid, job_of(&hybrid, hp, hd)),
    ] {
        let declared = palw_job_economic_compute_v1(p, &j, &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
        let executed = palw_attempt_economic_compute_v1(p, &j, true, &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
        println!("  {name:20} {declared:>20}  {executed:>20}  {:>8.4}x", declared as f64 / executed.max(1) as f64);
    }
}

/// `palw_canonical_work_v1` takes EXECUTION FACTS directly, which is the lever for a
/// cache/prefix-reuse audit: `uncached(prefill, generated)` vs `of_attempt(job, prefill_draw)`.
#[test]
fn execution_facts_are_the_direct_lever() {
    let dense = dense_2m_profile();
    let d = descriptor_of(&dense);
    for (prefill, generated) in [(1u32, 1u32), (63, 1), (512, 1), (512, 2)] {
        let facts = PalwCanonicalExecutionFactsV1::uncached(prefill, generated);
        let v = palw_canonical_work_v1(&d, &facts).expect("work");
        println!("  uncached(prefill={prefill:>4}, generated={generated}) -> {:>20} MAC-eq", v.arithmetic_mac_eq());
    }
}

// =================================================================================================
// 4. PWU, expected attempts, and the Q32 pair
// =================================================================================================

/// **Units warning, verified.** `palw_expected_attempts_v1` returns a u64 INTEGER count of draws;
/// `palw_expected_attempts_q32_v1` returns the SAME quantity in Q32 fixed point. They are not
/// interchangeable and the integer one floors.
#[test]
fn expected_attempts_integer_versus_q32() {
    println!("\n  class_target                 int attempts     q32 attempts       q32>>32   floor loss");
    for target in [u128::MAX, u128::MAX / 2, u128::MAX / 3, u128::MAX / 1000, 1u128 << 100] {
        let int = palw_expected_attempts_v1(target);
        let q32 = palw_expected_attempts_q32_v1(target);
        println!("  {:>28}  {int:>14}  {q32:>14}  {:>12}  {:>10}", format!("{target:x}"), q32 >> 32, q32 - ((int as u128) << 32));
    }
    // pwu = expected_attempts x pwu_per_inference, saturating.
    let target = u128::MAX / 1000;
    let attempts = palw_expected_attempts_v1(target);
    let pwu = palw_pwu_v1(target, 7_708);
    println!("\n  palw_pwu_v1(target, pwu_per_inference=7_708) = {pwu}  (= {attempts} attempts x 7_708 declared PWU)");
    assert_eq!(pwu as u128, (attempts as u128).saturating_mul(7_708).min(u64::MAX as u128));

    assert!(palw_ticket_admits_v1(0, u128::MAX), "the easiest target admits every ticket");
}

/// **The declared-vs-derived seam — where t12's collateral bug lived.**
/// `palw_claim_canonical_pwu_v1(declared_pwu, declared_per_inference, derived_per_draw)` re-prices
/// a claim: it replaces ONLY the declared per-inference factor with the derived one.
#[test]
fn the_declared_to_derived_repricing_seam() {
    let dense = dense_2m_profile();
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let job = job_of(&dense, dp, dd);
    let derived_per_draw = palw_canonical_draw_work_v1(&descriptor_of(&dense), &job, true).unwrap().provisional_scalar_v1();

    let declared_per_inference: u64 = 7_708; // the BASE-0 floor's declared leaves
    let attempts: u64 = 1_000;
    let declared_pwu = declared_per_inference.saturating_mul(attempts);

    let re = palw_claim_canonical_pwu_v1(declared_pwu, declared_per_inference, derived_per_draw);
    println!("\n  declared_pwu        {declared_pwu}            (declared PWU = leaves x attempts)");
    println!("  declared/inference  {declared_per_inference}                 (declared PWU per inference)");
    println!("  derived/draw        {derived_per_draw}   (MAC-eq per draw)");
    println!("  re-priced           {re:?}   (MAC-eq x attempts)");
    println!("  ratio derived/declared = {:.3}x", derived_per_draw as f64 / declared_per_inference as f64);
    assert_eq!(re, Some((attempts as u128).saturating_mul(derived_per_draw)));
}

// =================================================================================================
// 5. The ADR-0132 payout fold — CCU to sompi
// =================================================================================================

/// **The whole reward path in one test**, with every unit named.
#[test]
fn the_payout_fold_from_ccu_to_sompi() {
    let bits: u32 = 0x1e00_ffff; // a floor-ish compact target; an auditor should sweep this
    let fold = PalwEconomicPayoutFoldV1 {
        block_bits: bits,
        rate_sompi_per_giga: PALW_ECONOMIC_PAYOUT_DEVNET_V1.rate_sompi_per_giga,
        panel_share_alpha_permille: PALW_ECONOMIC_PAYOUT_DEVNET_V1.panel_share_alpha_permille,
        panel_share_min_permille: PALW_ECONOMIC_PAYOUT_DEVNET_V1.panel_share_min_permille,
        panel_share_max_permille: PALW_ECONOMIC_PAYOUT_DEVNET_V1.panel_share_max_permille,
        cap_utilization_max_permille: PALW_ECONOMIC_PAYOUT_DEVNET_V1.cap_utilization_max_permille,
    };

    let dense = dense_2m_profile();
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let job = job_of(&dense, dp, dd);
    let draw_ccu = palw_attempt_economic_compute_v1(&dense, &job, true, &PALW_ECONOMIC_COST_TABLE_V1).unwrap();

    let class_target = u128::MAX / 4;
    let class_q32 = palw_expected_attempts_q32_v1(class_target);
    let net_q32 = palw_network_draws_q32_from_bits_v1(bits);
    let attempted = palw_attempted_ccu_v1(class_q32, net_q32, draw_ccu);

    // The escrow of one t12 block's worker carve, sompi.
    let escrow_sompi: u64 = 320_084_640_000;
    let reward = kaspa_consensus_core::palw_economics_ledger_v1::palw_rate_priced_reward_v1(
        escrow_sompi,
        attempted,
        fold.rate_sompi_per_giga as u128,
    );
    let util = palw_cap_utilization_permille_v1(attempted, fold.rate_sompi_per_giga, escrow_sompi);
    let share = palw_panel_share_permille_v1(
        attempted,
        draw_ccu.saturating_mul(5),
        fold.panel_share_alpha_permille,
        fold.panel_share_min_permille,
        fold.panel_share_max_permille,
    );

    println!("\n  draw_ccu               {draw_ccu:>24}  CCU (== MAC-eq)");
    println!("  class attempts (Q32)   {class_q32:>24}  Q32 draws");
    println!("  network attempts (Q32) {net_q32:>24}  Q32 draws   (bits = {bits:#x})");
    println!("  attempted_ccu (C_P)    {attempted:>24}  CCU");
    println!("  rate                   {:>24}  sompi per 1e9 CCU", fold.rate_sompi_per_giga);
    println!("  escrow                 {escrow_sompi:>24}  sompi");
    println!("  priced reward          {reward:>24}  sompi   (= min(escrow, C_P x rate / 1e9))");
    println!("  cap utilization        {util:>24}  permille (UNCAPPED reward vs escrow)");
    println!("  panel share            {share:>24}  permille");
    assert!(reward <= escrow_sompi, "the escrow is the ceiling");

    // Sanity on the network factor: also reachable as the non-payout spelling.
    assert_eq!(net_q32, palw_network_expected_attempts_q32_v1(bits), "two spellings of the network Q32 factor agree");
}

/// `palw_attempted_compute_per_claim_v1` is the ADR-0131 (pre-0132) spelling: it takes the INTEGER
/// attempt count and has NO network factor. Both are pub. Mixing them is a unit error.
#[test]
fn the_two_attempted_compute_spellings_differ() {
    let dense = dense_2m_profile();
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let job = job_of(&dense, dp, dd);
    let draw = palw_attempt_economic_compute_v1(&dense, &job, true, &PALW_ECONOMIC_COST_TABLE_V1).unwrap();

    let target = u128::MAX / 4;
    let v0131 = palw_attempted_compute_per_claim_v1(palw_expected_attempts_v1(target), draw);
    let v0132 = palw_attempted_ccu_v1(palw_expected_attempts_q32_v1(target), palw_network_draws_q32_from_bits_v1(0x1e00_ffff), draw);
    println!("\n  ADR-0131 attempted_compute_per_claim  {v0131:>28} CCU  (integer attempts, no network factor)");
    println!("  ADR-0132 attempted_ccu                {v0132:>28} CCU  (Q32 attempts x network Q32)");
    println!("  ratio 0132/0131 = {:.6}x", v0132 as f64 / v0131.max(1) as f64);
}

// =================================================================================================
// 6. Execution quanta
// =================================================================================================

/// `PALW_EXECUTION_QUANTUM_V1 = 100_000`. The unit of `credited_work` fed to
/// `palw_execution_quantum_count_v1` decides how many permits one job mints — the classic
/// leaves-vs-MAC-eq confusion, reproduced here.
#[test]
fn execution_quanta_count_depends_on_the_unit_of_credited_work() {
    let dense = dense_2m_profile();
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let job = job_of(&dense, dp, dd);
    let mac_eq = palw_canonical_draw_work_v1(&descriptor_of(&dense), &job, true).unwrap().provisional_scalar_v1();
    let declared_leaves: u128 = 7_708;

    let seed = Hash64::default();
    let final_id = palw_execution_canonical_work_id_v1(Hash64::default());
    let q = PALW_EXECUTION_QUANTUM_V1 as u128;

    let n_mac = palw_execution_quantum_count_v1(mac_eq, q, seed, final_id);
    let n_leaf = palw_execution_quantum_count_v1(declared_leaves, q, seed, final_id);
    println!("\n  PALW_EXECUTION_QUANTUM_V1 = {}", PALW_EXECUTION_QUANTUM_V1);
    println!("  credited_work = {mac_eq:>22} MAC-eq  -> {n_mac:>6} quanta");
    println!("  credited_work = {declared_leaves:>22} leaves  -> {n_leaf:>6} quanta");
    println!("  the SAME execution mints {}x more permits when credited in MAC-eq", n_mac.max(1) / n_leaf.max(1));
}

// =================================================================================================
// 7. Fork choice / chain weight
// =================================================================================================

/// `chain_weights_v1` takes a `&[Option<PalwBlockWeightV1>]` — one entry per block, `None` refused.
/// `pwu` here is a u64 in PWU; the stage decides whether it lands in `safe`, `live` or both.
#[test]
fn chain_weights_and_tip_comparison() {
    let params = PalwChainWeightParamsV1 { penalty_sompi_per_pwu: 1_000, immature_bound_permille: 50 };
    let b = |pwu: u64, stage: PalwWorkRampStageV1| Some(PalwBlockWeightV1 { pwu, stage });

    let honest = vec![b(100, PalwWorkRampStageV1::Final), b(100, PalwWorkRampStageV1::Final), b(100, PalwWorkRampStageV1::Final)];
    let private = vec![b(100, PalwWorkRampStageV1::Provisional), b(100, PalwWorkRampStageV1::Provisional), b(100, PalwWorkRampStageV1::Provisional)];

    let hw = chain_weights_v1(&honest, &params).expect("honest");
    let pw = chain_weights_v1(&private, &params).expect("private");
    println!("\n  honest  safe={:>6} live={:>6}  (PWU)", hw.safe, hw.live);
    println!("  private safe={:>6} live={:>6}  (PWU)", pw.safe, pw.live);
    println!("  compare_tips_v1(honest, private) = {:?}", compare_tips_v1(&hw, &pw));

    // A `None` is REFUSED, not skipped.
    let holed = vec![b(100, PalwWorkRampStageV1::Final), None];
    assert!(chain_weights_v1(&holed, &params).is_err(), "an unresolvable block is an error");
}

// =================================================================================================
// 8. The three-way class identity split (2026-09-23, ea7ad7df)
// =================================================================================================

/// `PalwArtifactDigestV1`, `PalwInventoryRootV1` and `PalwClassIdV1` are three distinct newtypes
/// over `Hash64` with NO inter-conversion. This test documents what an auditor can and cannot do.
#[test]
fn the_class_identity_types_are_mutually_unconvertible() {
    use kaspa_consensus_core::palw_class_identity_v1::{PalwArtifactDigestV1, PalwClassIdV1, PalwInventoryRootV1};
    let d = PalwArtifactDigestV1::measured_over_the_file(Hash64::default());
    let r = PalwInventoryRootV1::rooted_over_the_inventory(Hash64::default());
    let c = PalwClassIdV1::of_this_graph(Hash64::default());
    println!("\n  artifact digest {d}\n  inventory root  {r}\n  class id        {c}");
    // There is deliberately no `From<PalwArtifactDigestV1> for PalwInventoryRootV1`. If one
    // appears, that is the t11/t12 class-root accident coming back.
}

// =================================================================================================
// 9. What is NOT reachable from here
// =================================================================================================

/// **Item 2, the negative half — verified by the fact that this test cannot call them.**
///
/// `decide_credit_v1` IS pub, but its `PalwCreditParamsV1` needs a `PalwClassRegistrationV1`, and
/// the only two builders for that in-tree are `pub(crate)`:
///   * `palw_registry::tests::fleet_registration()`   (palw_registry.rs:557, `pub(crate) fn`)
///   * `palw_registry::tests::base0_registration()`   (palw_registry.rs:627, `pub(crate) fn`)
/// An integration test must therefore construct `PalwClassRegistrationV1` field by field (~25
/// fields including a full `PalwShapeProfileV3`), or the credit audit must live in an in-file
/// `#[cfg(test)] mod tests` inside `consensus/core/src/palw_credit.rs`.
#[test]
fn the_credit_path_has_no_pub_fixture() {
    println!("\n  decide_credit_v1: pub, but PalwClassRegistrationV1 has only pub(crate) builders.");
    println!("  -> credit audits go in consensus/core/src/palw_credit.rs's own #[cfg(test)] mod,");
    println!("     or build PalwClassRegistrationV1 by hand.");
}
