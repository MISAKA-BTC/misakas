//! **The arbitrage GATE that ADR-0146 Rule R7 asks for, as a CI assertion.**
//!
//! `PalwCanonicalWorkVectorV1::provisional_scalar_v1`'s own doc states the rule this file
//! implements: *"ADR-0146 Rule R7 makes a measured arbitrage bound a GATE, not a note: a basis
//! that ships without a re-runnable search and a published worst-case ratio does not arm."*
//! testnet-12 arms that basis (`palw_canonical_work` at `ForkActivation(0)`), and the search does
//! not exist. `palw_arbitrage_search_v1` is not it: it compares
//! `palw_canonical_work_v1(..).provisional_scalar_v1()` against
//! `palw_attempt_economic_compute_v1(..)` of the SAME job, and those two are pinned equal to each
//! other by `the_provisional_scalar_is_adr_0131_of_the_executed_job`, so its headline
//! `1.000000x` is `x/x` and holds for any profile, any declaration and any future cost table.
//!
//! This file compares the declaration against the EXECUTION instead, and it is a gate:
//!
//!   1. §1 prices the three classes testnet-12 actually registers and takes the legitimate band
//!      from them. No hand-written constant is the band; the band is measured.
//!   2. §2 runs a deterministic seeded search over the registrant-writable profile scalars that
//!      `palw_economic_compute_v1` reads, admitting each candidate through the REAL gate
//!      (`verify_class_admission_v9`, as `processor.rs:6886` calls it) at testnet-12's own fence
//!      shape.
//!   3. §3 keeps only candidates that carry a RUNTIME WITNESS that the real work did not move:
//!      byte-identical node tables and an identical capped canonical leaf count. A seat replays
//!      the leaf tree; identical nodes and identical leaves mean identical arithmetic to verify.
//!      Everything a surviving candidate gained, it gained for free.
//!   4. §4 asserts the searched maximum of each of the four value ratios stays inside the band by
//!      [`TOLERANCE`], and prints the maximising input.
//!
//! **Every quantity is produced by the runtime functions named beside it.** Nothing here
//! re-implements a chain rule, and the denominators are not modelled: `real_work_mac_eq` is the
//! BASELINE class's own `palw_attempt_economic_compute_v1`, i.e. the arithmetic the honest class
//! this candidate is a relabelling of actually runs.
//!
//! Run: `cargo test -p kaspa-consensus-core --test audit_ratio_ci_guard -- --nocapture`
//! Measured wall time in this worktree, warm, debug profile: see `MEASURED_RUNTIME`.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_base0_profile::{
    PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context,
};
use kaspa_consensus_core::palw_class_admission_v2::{
    PalwClassAdmissionError, palw_admission_shape_at_v1, verify_class_admission_v9,
};
use kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1;
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_attempt_economic_compute_v1,
    palw_expected_attempts_q32_v1,
};
use kaspa_consensus_core::palw_economic_payout_v1::palw_attempted_ccu_v1;
use kaspa_consensus_core::palw_economics_ledger_v1::palw_rate_priced_reward_v1;
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_quantum_count_v1};
use kaspa_consensus_core::palw_fp_devnet_v3::palw_exposure_unit_pwu_v1;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_qwen25_profile::{
    PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1,
};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7,
};
use kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1;
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

// =================================================================================================
// Constants. One seed, no wall clock, no environment.
// =================================================================================================

/// The only source of variation in this file. Changing it changes WHICH inputs the search visits,
/// never what is asserted about them.
const SEED: u64 = 0x_A1B2_C3D4_2026_0923;

/// Candidates drawn per baseline class. Chosen so the whole file stays inside
/// [`MEASURED_RUNTIME`]; the failure this guard reports is found in the first few dozen.
const CANDIDATES_PER_BASELINE: usize = 400;

/// **The gate's published factor.** A registration admitted by testnet-12's own gate may earn at
/// most this multiple of the best ratio any class testnet-12 actually registers earns, per unit of
/// arithmetic a seat really replays.
///
/// Two is deliberately generous: the legitimate population is three points, and a fourth honest
/// class could plausibly sit a little above the best of them. It is not a tuning knob for making
/// this test pass — the ratios this search finds are four orders of magnitude past it.
const TOLERANCE: u128 = 2;

/// Wall time measured in this worktree, `cargo test -p kaspa-consensus-core --test
/// audit_ratio_ci_guard`, warm target dir, debug profile, 1,200 candidates through the real
/// admission gate. Recorded so a future change that makes the search quadratic is visible as a
/// number and not as a CI timeout. A release build is a constant factor faster.
const MEASURED_RUNTIME: &str = "measured 4.3 s + 0.1 s for the control";

/// testnet-12's block subsidy at every height (`CoinbaseManager::calc_block_subsidy`, verified in
/// this tree by `t12_collateral_terms`). `deflationary_phase_daa_score == 0` and
/// `crescendo_activation == ForkActivation(0)`, so it does not vary with height.
const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;

/// One giga MAC-eq. `rate_sompi_per_giga` is denominated against this
/// (`PALW_LEDGER_RATE_SCALE_V1`), so every ratio below is quoted per giga and the honest reward
/// ratio comes out as the rate itself.
const GIGA: u128 = 1_000_000_000;

// =================================================================================================
// §0 — the testnet-12 card and the live economic fences, read from `Params`, never hardcoded.
// =================================================================================================

fn t12() -> Params {
    palw_t12_shipped_params()
}

fn bundle_of(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("testnet-12 is a ConsensusV2 network"),
    }
}

/// The escrow one testnet-12 claim carries: the block subsidy through `palw_overlay_carve`'s
/// worker carve, which is armed at DAA 0 at 720 permille.
fn escrow_sompi(p: &Params) -> u64 {
    let carve = p.palw_overlay_carve.expect("t12 arms palw_overlay_carve").worker_carve_permille as u64;
    T12_BLOCK_SUBSIDY_SOMPI / 1_000 * carve
}

/// `PalwEconomicPayoutV1::rate_sompi_per_giga`, armed at DAA 0 on testnet-12.
fn rate_sompi_per_giga(p: &Params) -> u64 {
    p.palw_economic_payout.expect("t12 arms palw_economic_payout").rate_sompi_per_giga
}

// =================================================================================================
// §1 — the legitimate population: the three classes testnet-12 registers at genesis.
// =================================================================================================

fn floor_profile() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("BASE-0 floor profile")
}
fn hybrid_profile() -> PalwShapeProfileV3 {
    qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B }))
        .expect("Qwen3.6 graph-v7 @512")
}
fn dense_profile() -> PalwShapeProfileV3 {
    qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B })
        .expect("Qwen2.5 A16 graph-v7 @2M")
}

/// Each registered class with the canonical job its genesis registration carries. The jobs come
/// from the same pub builders the card itself uses.
fn population() -> Vec<(&'static str, PalwShapeProfileV3, (u32, u32))> {
    vec![
        ("BASE-0 floor", floor_profile(), PALW_RC_BASE0_CANONICAL),
        ("Qwen3.6 v7 @512", hybrid_profile(), qwen36_held_canonical_v1(512)),
        ("Qwen2.5 A16 v7 @2M", dense_profile(), qwen25_a16_held_canonical_v1(2_097_152)),
    ]
}

// =================================================================================================
// §2 — the runtime pricing chain, in the units the code uses.
//
//   ccu_declared (MAC-eq per draw)   palw_attempt_economic_compute_v1(profile, job, true, TABLE)
//     -> target (u128 ticket space)  palw_work_ticket_target_v1(ccu, W0)
//     -> draws                       palw_expected_attempts_v1 / _q32_v1
//     -> attempted_ccu (MAC-eq)      palw_attempted_ccu_v1
//     -> priced_reward (sompi)       palw_rate_priced_reward_v1(escrow, attempted, rate)
//     -> claim.pwu (MAC-eq, u64)     palw_pwu_v1(target, ccu)          [fork weight]
//     -> exec credit (MAC-eq)        the raw canonical draw, per palw_state_v2.rs:10975
//     -> permits (count)             palw_execution_quantum_count_v1(credit, 100_000, ..)
//     -> reserved (sompi)            palw_exposure_unit_pwu_v1(ccu, floor_leaves, floor_ccu) * 5
// =================================================================================================

/// `slash_value_per_pwu` on every testnet-12 class (`palw_genesis_v2.rs:538`). Sompi per
/// floor-normalised exposure pwu.
const SLASH_VALUE_PER_PWU: u128 = 5;

#[derive(Clone, Copy, Debug)]
struct Priced {
    /// What the chain believes one draw costs. MAC-eq.
    ccu_declared: u128,
    /// What the producer and the replaying seat really run for one draw. MAC-eq.
    ccu_real: u128,
    /// Integer expected draws per win, `palw_expected_attempts_v1`.
    draws: u64,
    /// `draws * ccu_real` — the arithmetic one PAID claim costs the producer. MAC-eq.
    real_work: u128,
    /// sompi.
    reward: u64,
    /// `claim.pwu`, MAC-eq clamped into u64.
    fork_weight: u64,
    /// The execution credit one Final carries, raw MAC-eq.
    credit: u128,
    /// Spend-once execution quanta the Final mints.
    permits: u32,
    /// The producer's slashable reservation for the claim. sompi.
    reserved: u128,
}

impl Priced {
    /// sompi of reward per giga MAC-eq of REAL arithmetic. The honest value is the rate itself.
    fn reward_per_giga(&self) -> u128 {
        ratio(self.reward as u128, self.real_work)
    }
    /// fork-weight pwu per giga MAC-eq of real arithmetic.
    fn weight_per_giga(&self) -> u128 {
        ratio(self.fork_weight as u128, self.real_work)
    }
    /// execution credit per giga MAC-eq of real arithmetic.
    fn credit_per_giga(&self) -> u128 {
        ratio(self.credit, self.real_work)
    }
    /// permits per exa (10^18) MAC-eq of real arithmetic — permits are small integers, so a giga
    /// denominator would floor most of the population to zero and hide the band.
    fn permits_per_exa(&self) -> u128 {
        ratio_scaled(self.permits as u128, self.real_work, GIGA * GIGA)
    }
}

fn ratio(numerator: u128, real_work: u128) -> u128 {
    ratio_scaled(numerator, real_work, GIGA)
}

/// `numerator * scale / real_work`, without an intermediate overflow and without a float.
fn ratio_scaled(numerator: u128, real_work: u128, scale: u128) -> u128 {
    if real_work == 0 {
        return u128::MAX;
    }
    let d = real_work;
    (numerator / d).saturating_mul(scale).saturating_add((numerator % d).saturating_mul(scale) / d)
}

/// The floor's two measures, which are the exposure basis every reservation is normalised through
/// (`palw_exposure_basis_v2` / `palw_exposure_pwu_v3`).
fn exposure_basis() -> (u64, u128) {
    let p = floor_profile();
    let job = rc_job_context(&p, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    let ladder = floor_ladder_cap(&p, &job);
    let declared = step_leaf_count_capped_v1(&p, &job, ladder).expect("floor leaves");
    (declared, ccu_of(&p, &job))
}

fn floor_ladder_cap(profile: &PalwShapeProfileV3, _job: &PalwJobContextV2) -> u64 {
    let p = t12();
    let b = bundle_of(&p);
    let shape = palw_admission_shape_at_v1(&p, &b, profile, 0).expect("admission shape");
    match shape.ladder {
        Some(r) => r.ladder,
        None => palw_class_step_ladder_v1(b.court.max_step_leaf_count(), profile),
    }
}

/// `economic_ccu_per_claim` — the registry row's per-DRAW work, and by the pin at
/// `palw_canonical_work_v1.rs:889` also `provisional_scalar_v1()`, the fork-weight scalar.
fn ccu_of(profile: &PalwShapeProfileV3, job: &PalwJobContextV2) -> u128 {
    palw_attempt_economic_compute_v1(profile, job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("priced draw")
}

/// The whole chain for one (declared, real) pair at testnet-12's live constants.
fn price(p: &Params, ccu_declared: u128, ccu_real: u128, basis: (u64, u128)) -> Priced {
    let escrow = escrow_sompi(p);
    let rate = rate_sompi_per_giga(p);
    let w0 = palw_work_floor_v1(escrow, rate);

    let target = palw_work_ticket_target_v1(ccu_declared, w0);
    let draws = palw_expected_attempts_v1(target);
    let attempted_ccu =
        palw_attempted_ccu_v1(palw_expected_attempts_q32_v1(target), PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, ccu_declared);
    let reward = palw_rate_priced_reward_v1(escrow, attempted_ccu, rate as u128);
    let fork_weight = palw_pwu_v1(target, ccu_declared.min(u64::MAX as u128) as u64);

    // `record_round_final` credits the RAW derived draw (palw_state_v2.rs:10975 takes
    // `palw_exposure_pwu_v2`, not `_v3`) whenever `lane.execution_quantum > 0`, which it is on
    // testnet-12 from DAA 0.
    let credit = ccu_declared;
    let permits = palw_execution_quantum_count_v1(
        credit,
        PALW_EXECUTION_QUANTUM_V1 as u128,
        Hash64::default(),
        Hash64::default(),
    );

    let reserved = palw_exposure_unit_pwu_v1(ccu_declared, basis.0, basis.1) as u128 * SLASH_VALUE_PER_PWU;

    Priced {
        ccu_declared,
        ccu_real,
        draws,
        real_work: (draws as u128).saturating_mul(ccu_real),
        reward,
        fork_weight,
        credit,
        permits,
        reserved,
    }
}

// =================================================================================================
// §3 — the admission gate, exactly as the acceptance path calls it.
// =================================================================================================

/// `processor.rs:6886`'s call, at DAA 0 with testnet-12's fence shape. `share_permille` is 0
/// because `palw_admission_independence` is armed at 0 (`processor.rs:6779`). Returns the capped
/// canonical leaf count the gate itself computed.
fn admit(profile: &PalwShapeProfileV3, job: &PalwJobContextV2) -> Result<u64, PalwClassAdmissionError> {
    let p = t12();
    let b = bundle_of(&p);
    let shape = palw_admission_shape_at_v1(&p, &b, profile, 0).expect("admission shape");
    let ladder_cap = match shape.ladder {
        Some(r) => r.ladder,
        None => palw_class_step_ladder_v1(b.court.max_step_leaf_count(), profile),
    };
    let counted = step_leaf_count_capped_v1(profile, job, ladder_cap).unwrap_or(u64::MAX);
    let reg = PalwConsensusObjectV2::ClassRegistered {
        class_id: profile.shape_profile_id(),
        artifact_root: Hash64::default(),
        slash_value_per_pwu: SLASH_VALUE_PER_PWU as u64,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target: u128::MAX,
        share_permille: 0,
        activation_daa: 0,
        admission: None,
    };
    let certified = palw_rc_certified_families_v1();
    verify_class_admission_v9(
        &b,
        profile,
        job,
        &reg,
        &certified,
        &[],
        shape.ladder,
        shape.court,
        false,
        shape.token_lift,
        shape.fused_dissectable,
        p.palw_canonical_work_at(0),
        shape.held,
        shape.kimi_family,
        // 2026-09-23 audit C-4 fence, as the network resolves it.
        p.palw_audit_2026_09_23_active_at(0),
    )
    .map(|e| e.canonical_step_leaf_count)
}

/// **The real-work witness.** A candidate counts as an arbitrage only if the chain can be shown,
/// at runtime, to be verifying the same arithmetic it verified before the edit:
///
///   * every node table is byte-identical (`PalwStepNodeV1: PartialEq`), so the graph the
///     executor dispatches and the leaves a seat replays are the same operators over the same
///     widths, dtypes and input refs; and
///   * `step_leaf_count_capped_v1` — the gate's own count, under the gate's own ladder — is
///     unchanged, so the court's replay tree is the same size.
///
/// If either moves, the edit may have bought real work and the candidate is discarded. This is
/// what makes `real_work = draws * baseline_ccu` a measurement rather than an assumption.
fn real_work_is_unchanged(base: &PalwShapeProfileV3, cand: &PalwShapeProfileV3, base_leaves: u64, cand_leaves: u64) -> bool {
    base_leaves == cand_leaves
        && base.pre_nodes == cand.pre_nodes
        && base.gdn_nodes == cand.gdn_nodes
        && base.attn_nodes == cand.attn_nodes
        && base.post_nodes == cand.post_nodes
}

/// One legitimate class the search perturbs: name, profile, its canonical job, the leaf count the
/// gate counted for it, and the MAC-eq one honest draw of it really costs.
type Baseline = (&'static str, PalwShapeProfileV3, PalwJobContextV2, u64, u128);

#[derive(Default)]
struct Stats {
    admitted: usize,
    refused: usize,
    moved_real_work: usize,
    witnessed: usize,
}

/// Price one candidate against one baseline, or reject it. `None` means the chain refused the
/// registration or the real-work witness did not hold — either way it is not an arbitrage and it
/// is not counted.
fn evaluate(p: &Params, base: &Baseline, cand: &Candidate, basis: (u64, u128), stats: &mut Stats) -> Option<Priced> {
    let (_, base_profile, base_job, base_leaves, base_ccu) = base;
    let mut profile = base_profile.clone();
    cand.apply(&mut profile);

    // Priced on ITS profile, executing the BASELINE's graph — true only if the witness below
    // holds, so the witness is checked before anything is counted.
    let ccu_declared = match palw_attempt_economic_compute_v1(&profile, base_job, true, &PALW_ECONOMIC_COST_TABLE_V1) {
        Ok(v) => v,
        Err(_) => {
            stats.refused += 1;
            return None;
        }
    };
    let cand_leaves = match admit(&profile, base_job) {
        Ok(l) => l,
        Err(_) => {
            stats.refused += 1;
            return None;
        }
    };
    stats.admitted += 1;
    if !real_work_is_unchanged(base_profile, &profile, *base_leaves, cand_leaves) {
        stats.moved_real_work += 1;
        return None;
    }
    stats.witnessed += 1;
    Some(price(p, ccu_declared, *base_ccu, basis))
}

/// Greedily drop every edit the maximum does not need, so the printed input is a minimal
/// reproduction rather than the whole random draw that happened to find it.
fn minimise(p: &Params, base: &Baseline, cand: &Candidate, basis: (u64, u128), ratio: Ratio, target: u128) -> Candidate {
    let mut kept = cand.clone();
    let mut i = 0;
    while i < kept.edits.len() {
        let mut trial = kept.clone();
        let dropped = trial.edits.remove(i);
        let _ = dropped;
        // Load-bearing if dropping it would empty the candidate, or would move the ratio off the
        // maximum this candidate achieved.
        let load_bearing = trial.edits.is_empty()
            || evaluate(p, base, &trial, basis, &mut Stats::default()).map(|q| ratio.of(&q)) != Some(target);
        if load_bearing {
            i += 1;
        } else {
            kept = trial;
        }
    }
    kept
}

// =================================================================================================
// §4 — the search space and the deterministic generator.
// =================================================================================================

/// The registrant-writable profile scalars `palw_economic_compute_v1` reads and the node tables do
/// not carry. Every one of these is a DECLARATION: the executed graph is the node list, and these
/// eight numbers sit beside it and price it.
///
/// Derived, not guessed: `grep -o 'profile\.\w*' consensus/core/src/palw_economic_compute_v1.rs`
/// yields exactly `attn_head_dim, attn_heads, attn_kv_heads, gdn_conv_kernel, gdn_head_k_dim,
/// gdn_head_v_dim, gdn_heads, hidden_dim` plus the four node tables, `layer_count`/`layer_kind`
/// (which change how many layers execute, so they are excluded as real work) and
/// `shape_profile_id`/`validate_shape` (identity and the gate).
const LEVERS: [&str; 8] = [
    "attn_heads",
    "attn_head_dim",
    "attn_kv_heads",
    "hidden_dim",
    "gdn_heads",
    "gdn_head_k_dim",
    "gdn_head_v_dim",
    "gdn_conv_kernel",
];

/// The ladder each lever is drawn from. Small values probe the refusals, large ones probe the
/// ceiling; `u16::MAX` is the widest value a `u16` lever can carry at all.
const LADDER: [u64; 14] = [1, 2, 4, 16, 64, 128, 256, 1_024, 4_096, 16_384, 41_854, 65_535, 262_144, 1_048_576];

#[derive(Clone, Debug)]
struct Candidate {
    edits: Vec<(&'static str, u64)>,
}

impl Candidate {
    fn apply(&self, p: &mut PalwShapeProfileV3) {
        for (field, value) in &self.edits {
            let v = *value;
            match *field {
                "attn_heads" => p.attn_heads = v.min(u16::MAX as u64) as u16,
                "attn_head_dim" => p.attn_head_dim = v.min(u32::MAX as u64) as u32,
                "attn_kv_heads" => p.attn_kv_heads = v.min(u16::MAX as u64) as u16,
                "hidden_dim" => p.hidden_dim = v.min(u32::MAX as u64) as u32,
                "gdn_heads" => p.gdn_heads = v.min(u16::MAX as u64) as u16,
                "gdn_head_k_dim" => p.gdn_head_k_dim = v.min(u32::MAX as u64) as u32,
                "gdn_head_v_dim" => p.gdn_head_v_dim = v.min(u32::MAX as u64) as u32,
                "gdn_conv_kernel" => p.gdn_conv_kernel = v.min(u16::MAX as u64) as u16,
                other => panic!("unknown lever {other}"),
            }
        }
    }

    /// The values the profile ACTUALLY ends up carrying. A lever drawn above its field's width is
    /// clamped by [`Self::apply`], so printing the drawn rung would hand CI a reproduction that
    /// does not reproduce. Read back from the mutated profile instead.
    fn describe(&self, base: &PalwShapeProfileV3) -> String {
        let mut p = base.clone();
        self.apply(&mut p);
        self.edits
            .iter()
            .map(|(f, _)| {
                let v: u64 = match *f {
                    "attn_heads" => p.attn_heads as u64,
                    "attn_head_dim" => p.attn_head_dim as u64,
                    "attn_kv_heads" => p.attn_kv_heads as u64,
                    "hidden_dim" => p.hidden_dim as u64,
                    "gdn_heads" => p.gdn_heads as u64,
                    "gdn_head_k_dim" => p.gdn_head_k_dim as u64,
                    "gdn_head_v_dim" => p.gdn_head_v_dim as u64,
                    "gdn_conv_kernel" => p.gdn_conv_kernel as u64,
                    other => panic!("unknown lever {other}"),
                };
                format!("{f} := {v}")
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// xorshift64*, seeded once from [`SEED`]. The search is a pure function of that constant.
struct Rng(u64);
impl Rng {
    fn new() -> Self {
        Self(SEED)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// One candidate: a random non-empty subset of the levers, each at a random rung of the ladder.
fn draw_candidate(rng: &mut Rng) -> Candidate {
    let mask = 1 + rng.below((1u64 << LEVERS.len()) - 1);
    let mut edits = Vec::new();
    for (i, lever) in LEVERS.iter().enumerate() {
        if mask & (1 << i) != 0 {
            edits.push((*lever, LADDER[rng.below(LADDER.len() as u64) as usize]));
        }
    }
    Candidate { edits }
}

// =================================================================================================
// The four ratios, named once so the band, the search and the report cannot drift apart.
// =================================================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ratio {
    Reward,
    ForkWeight,
    Credit,
    Permit,
}

impl Ratio {
    const ALL: [Ratio; 4] = [Ratio::Reward, Ratio::ForkWeight, Ratio::Credit, Ratio::Permit];

    fn name(self) -> &'static str {
        match self {
            Ratio::Reward => "reward       sompi per giga real MAC-eq",
            Ratio::ForkWeight => "fork weight  pwu   per giga real MAC-eq",
            Ratio::Credit => "exec credit  MAC-eq per giga real MAC-eq",
            Ratio::Permit => "permits      count per exa  real MAC-eq",
        }
    }

    fn of(self, p: &Priced) -> u128 {
        match self {
            Ratio::Reward => p.reward_per_giga(),
            Ratio::ForkWeight => p.weight_per_giga(),
            Ratio::Credit => p.credit_per_giga(),
            Ratio::Permit => p.permits_per_exa(),
        }
    }
}

fn commas(mut n: u128) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut parts = Vec::new();
    while n > 0 {
        parts.push(format!("{:03}", n % 1000));
        n /= 1000;
    }
    let mut s = parts.pop().unwrap().trim_start_matches('0').to_string();
    if s.is_empty() {
        s = "0".into();
    }
    while let Some(p) = parts.pop() {
        s.push(',');
        s.push_str(&p);
    }
    s
}

// =================================================================================================
// THE GUARD
// =================================================================================================

#[test]
fn the_searched_maximum_value_per_real_work_stays_inside_the_legitimate_band() {
    let started = std::time::Instant::now();
    let p = t12();
    let basis = exposure_basis();
    let escrow = escrow_sompi(&p);
    let rate = rate_sompi_per_giga(&p);
    let w0 = palw_work_floor_v1(escrow, rate);

    println!("\n=== testnet-12 economic constants, read from Params (not hardcoded) ===");
    println!("  block subsidy            {} sompi", commas(T12_BLOCK_SUBSIDY_SOMPI as u128));
    println!("  worker carve             {} permille", p.palw_overlay_carve.unwrap().worker_carve_permille);
    println!("  claim escrow             {} sompi", commas(escrow as u128));
    println!("  rate_sompi_per_giga      {} sompi per 1e9 MAC-eq", commas(rate as u128));
    println!("  W0 (work floor)          {} CCU", commas(w0));
    println!("  exposure basis           floor declared {} leaves / floor derived {} MAC-eq", commas(basis.0 as u128), commas(basis.1));
    println!("  execution quantum        {}", commas(PALW_EXECUTION_QUANTUM_V1 as u128));

    // ---------------------------------------------------------------------------------------
    // §1 — the legitimate band, measured over the classes testnet-12 registers.
    // ---------------------------------------------------------------------------------------
    println!("\n=== §1 the legitimate population (the 3 classes t12 registers at genesis) ===");
    let mut band_max = [0u128; 4];
    let mut band_min = [u128::MAX; 4];
    let mut baselines: Vec<Baseline> = Vec::new();

    for (name, profile, canonical) in population() {
        let job = rc_job_context(&profile, canonical.0, canonical.1);
        let ccu = ccu_of(&profile, &job);
        let leaves = admit(&profile, &job).unwrap_or_else(|e| panic!("t12's own class {name} is refused by its own gate: {e:?}"));
        // An honest class declares what it executes: declared == real.
        let priced = price(&p, ccu, ccu, basis);

        println!(
            "\n  {name}\n    canonical job          ({}, {})\n    declared leaves        {}\n    ccu per draw           {} MAC-eq\n    expected draws         {}\n    real work per claim    {} MAC-eq\n    priced reward          {} sompi\n    fork weight (pwu)      {}\n    permits minted         {}\n    reserved (collateral)  {} sompi",
            canonical.0,
            canonical.1,
            commas(leaves as u128),
            commas(ccu),
            commas(priced.draws as u128),
            commas(priced.real_work),
            commas(priced.reward as u128),
            commas(priced.fork_weight as u128),
            commas(priced.permits as u128),
            commas(priced.reserved),
        );
        for (i, r) in Ratio::ALL.iter().enumerate() {
            let v = r.of(&priced);
            println!("    {:<42} {}", r.name(), commas(v));
            band_max[i] = band_max[i].max(v);
            band_min[i] = band_min[i].min(v);
        }
        baselines.push((name, profile, job, leaves, ccu));
    }

    println!("\n  --- the band ---");
    println!(
        "  (the reward band max sits slightly ABOVE rate_sompi_per_giga = {} because\n   palw_expected_attempts_v1 floors the hybrid's 2.24 draws to 2, understating its real work.\n   That makes the ceiling MORE permissive, so it is conservative in the attacker's favour.)",
        commas(rate as u128)
    );
    for (i, r) in Ratio::ALL.iter().enumerate() {
        println!("    {:<42} [{} .. {}]", r.name(), commas(band_min[i]), commas(band_max[i]));
    }
    assert!(band_max.iter().all(|m| *m > 0), "the legitimate band must be non-degenerate before anything is asserted against it");

    // ---------------------------------------------------------------------------------------
    // §2/§3 — the seeded search, gated by the real admission rule and the real-work witness.
    // ---------------------------------------------------------------------------------------
    println!("\n=== §2 seeded adversarial search (SEED = {SEED:#x}, {CANDIDATES_PER_BASELINE} candidates per baseline) ===");

    #[derive(Clone)]
    struct Best {
        value: u128,
        baseline: usize,
        candidate: Candidate,
        priced: Priced,
    }
    let mut best: [Option<Best>; 4] = [None, None, None, None];
    let mut stats = Stats::default();

    for (bi, b) in baselines.iter().enumerate() {
        let mut rng = Rng::new();
        for _ in 0..CANDIDATES_PER_BASELINE {
            let cand = draw_candidate(&mut rng);
            let Some(priced) = evaluate(&p, b, &cand, basis, &mut stats) else { continue };
            for (i, r) in Ratio::ALL.iter().enumerate() {
                let v = r.of(&priced);
                if best[i].as_ref().is_none_or(|b| v > b.value) {
                    best[i] = Some(Best { value: v, baseline: bi, candidate: cand.clone(), priced });
                }
            }
        }
    }

    println!(
        "  candidates {}   admitted {}   refused-by-the-gate {}   admitted-but-moved-real-work {}   counted {}",
        CANDIDATES_PER_BASELINE * baselines.len(),
        stats.admitted,
        stats.refused,
        stats.moved_real_work,
        stats.witnessed
    );
    assert!(stats.witnessed > 0, "the search admitted nothing the witness accepted — the guard would be vacuous");

    // Reduce each maximiser to a MINIMAL reproduction: drop every edit that the ratio does not
    // need. A CI failure should name two fields, not seven.
    for (i, r) in Ratio::ALL.iter().enumerate() {
        let b = best[i].as_ref().expect("a counted candidate exists").clone();
        let minimal = minimise(&p, &baselines[b.baseline], &b.candidate, basis, *r, b.value);
        let priced = evaluate(&p, &baselines[b.baseline], &minimal, basis, &mut Stats::default())
            .expect("the minimal candidate is still admitted and still witnessed");
        assert_eq!(r.of(&priced), b.value, "minimisation must not change the ratio it minimises");
        best[i] = Some(Best { value: b.value, baseline: b.baseline, candidate: minimal, priced });
    }

    // ---------------------------------------------------------------------------------------
    // §4 — the verdict.
    // ---------------------------------------------------------------------------------------
    println!("\n=== §3 the maximising inputs ===");
    let mut failures: Vec<String> = Vec::new();
    for (i, r) in Ratio::ALL.iter().enumerate() {
        let b = best[i].as_ref().expect("a counted candidate exists");
        let ceiling = band_max[i].saturating_mul(TOLERANCE);
        let over = if band_max[i] == 0 { u128::MAX } else { b.value / band_max[i] };
        let verdict = if b.value <= ceiling { "within band" } else { "OUT OF BAND" };

        println!("\n  {}", r.name());
        println!("    band max (legitimate)  {}", commas(band_max[i]));
        println!("    ceiling (x{TOLERANCE})           {}", commas(ceiling));
        println!("    searched max           {}   -> {verdict}, {}x the band", commas(b.value), commas(over));
        println!("    baseline               {}", baselines[b.baseline].0);
        println!("    maximising input       {}", b.candidate.describe(&baselines[b.baseline].1));
        println!(
            "    at that input          declared {} MAC-eq/draw vs real {} MAC-eq/draw ({}x), draws {}, real work {} MAC-eq",
            commas(b.priced.ccu_declared),
            commas(b.priced.ccu_real),
            commas(ratio_scaled(b.priced.ccu_declared, b.priced.ccu_real, 1)),
            commas(b.priced.draws as u128),
            commas(b.priced.real_work),
        );
        println!(
            "                           reward {} sompi, fork weight {} pwu, credit {} MAC-eq, permits {}, reserved {} sompi",
            commas(b.priced.reward as u128),
            commas(b.priced.fork_weight as u128),
            commas(b.priced.credit),
            commas(b.priced.permits as u128),
            commas(b.priced.reserved),
        );

        if b.value > ceiling {
            failures.push(format!(
                "{}: searched max {} is {}x the legitimate band max {} (ceiling x{TOLERANCE} = {}); maximising input: baseline {} with {}",
                r.name().trim(),
                commas(b.value),
                commas(over),
                commas(band_max[i]),
                commas(ceiling),
                baselines[b.baseline].0,
                b.candidate.describe(&baselines[b.baseline].1)
            ));
        }
    }

    println!("\n  runtime {:.2?} (debug profile, warm target dir)   [{MEASURED_RUNTIME}]", started.elapsed());

    assert!(
        failures.is_empty(),
        "\n\nADR-0146 R7 ARBITRAGE GATE FAILED — {} of {} value ratios exceed the legitimate band:\n\n{}\n\nEvery input above was admitted by verify_class_admission_v9 at testnet-12's own fence shape,\nover byte-identical node tables and an identical capped leaf count, i.e. the seats replay the\nsame arithmetic they would have replayed for the honest class.\n",
        failures.len(),
        Ratio::ALL.len(),
        failures.join("\n\n")
    );
}

// =================================================================================================
// THE CONTROL
//
// The guard above counts a candidate only when `real_work_is_unchanged` holds, and in the shipped
// search that predicate rejected 0 of 463 admitted candidates. A predicate that never fires is
// indistinguishable from `true`, and `true` would silently turn the guard into a measurement of
// nothing. This control shows the witness discriminates: it rejects both a graph edit and a leaf
// count that moved, and it shows why `layer_count` is excluded from LEVERS rather than forgotten.
// =================================================================================================

#[test]
fn the_real_work_witness_rejects_an_edit_that_buys_real_work() {
    let base = floor_profile();
    let job = rc_job_context(&base, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    let ladder = floor_ladder_cap(&base, &job);
    let base_leaves = step_leaf_count_capped_v1(&base, &job, ladder).expect("floor leaves");

    // (0) The witness accepts the identity — otherwise it would reject everything and the guard
    //     would be vacuous in the other direction.
    assert!(
        real_work_is_unchanged(&base, &base.clone(), base_leaves, base_leaves),
        "the witness must accept an unmodified profile"
    );

    // (1) A GRAPH edit. Widening one attention node's output is more arithmetic for the executor
    //     and more leaves for a seat to replay. The witness must refuse to call this free.
    let mut widened = base.clone();
    assert!(!widened.attn_nodes.is_empty(), "the floor carries attention nodes");
    widened.attn_nodes[0].out_len = match widened.attn_nodes[0].out_len {
        kaspa_consensus_core::palw_step::PalwStepOutLenV1::Fixed { elements } => {
            kaspa_consensus_core::palw_step::PalwStepOutLenV1::Fixed { elements: elements.saturating_mul(2).max(2) }
        }
        kaspa_consensus_core::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => {
            kaspa_consensus_core::palw_step::PalwStepOutLenV1::KvScaled { multiplier: multiplier.saturating_mul(2).max(2) }
        }
    };
    let widened_leaves = step_leaf_count_capped_v1(&widened, &job, ladder).unwrap_or(u64::MAX);
    assert!(
        !real_work_is_unchanged(&base, &widened, base_leaves, widened_leaves),
        "a widened node changes the arithmetic a seat replays; the witness must reject it"
    );

    // (2) The LEAF-COUNT half on its own: identical node tables, a leaf count that moved.
    assert!(
        !real_work_is_unchanged(&base, &base.clone(), base_leaves, base_leaves + 1),
        "a moved canonical leaf count must be rejected even when the node tables match"
    );

    // (3) Why `layer_count` is not a lever: it multiplies the graph, so it buys real work and is
    //     not a free declaration. Measured, not assumed.
    let mut deeper = base.clone();
    deeper.layer_count = base.layer_count.saturating_mul(2);
    let priced_base = ccu_of(&base, &job);
    let priced_deeper = palw_attempt_economic_compute_v1(&deeper, &job, true, &PALW_ECONOMIC_COST_TABLE_V1);
    let deeper_leaves = step_leaf_count_capped_v1(&deeper, &job, ladder).unwrap_or(u64::MAX);
    println!(
        "  layer_count {} -> {}: ccu {} -> {:?}, leaves {} -> {}",
        base.layer_count,
        deeper.layer_count,
        commas(priced_base),
        priced_deeper.as_ref().map(|v| commas(*v)),
        commas(base_leaves as u128),
        commas(deeper_leaves as u128)
    );
    assert!(
        deeper_leaves != base_leaves || priced_deeper.is_err(),
        "doubling layer_count must move the leaf count (or be refused) — if it did not, it would \
         belong in LEVERS as a free declaration and the search is missing it"
    );

    println!("  the real-work witness discriminates: identity accepted, graph edit and moved leaf count both rejected");
}
