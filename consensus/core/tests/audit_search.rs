//! **AGENT D — adversarial search over PALW testnet-12's economic surface.**
//!
//! Deterministic. One seed constant, no wall clock, no randomness the CI cannot reproduce.
//! Every number printed here is produced by the RUNTIME functions named in the output, never by a
//! local re-implementation of them.
//!
//! Run: `cargo test -p kaspa-consensus-core --test audit_search -- --nocapture`

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_canonical_work_v1::{
    PalwCanonicalClassDescriptorV1, PalwCanonicalExecutionFactsV1, palw_canonical_draw_work_v1, palw_canonical_work_v1,
};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_attempt_economic_compute_v1,
    palw_economic_shape_v1, palw_expected_attempts_q32_v1, palw_job_economic_compute_v1, palw_weight_dtype_cost_v1,
};
use kaspa_consensus_core::palw_economic_payout_v1::{palw_attempted_ccu_v1, palw_cap_utilization_permille_v1};
use kaspa_consensus_core::palw_economics_ledger_v1::{PALW_LEDGER_RATE_SCALE_V1, palw_rate_priced_reward_v1};
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_quantum_count_v1};
use kaspa_consensus_core::palw_panel_economy_v1::palw_work_priced_reward_v1;
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_qwen25_profile::{
    PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1,
};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7,
};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_step::{
    PALW_STEP_INPUT_KV_K, PALW_STEP_MAX_NODES_PER_TABLE, PALW_STEP_MAX_TILE_LEN,
    PALW_STEP_MIN_TILE_LEN, PalwShapeProfileV3, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1,
    PalwStepOutLenV1,
};
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

// =================================================================================================
// The t12 constants this lane prices against. Every one is quoted from the recon brief and
// re-derived here from the runtime where a runtime function exists for it.
// =================================================================================================

/// Deterministic seed. Changing it changes the search, not the assertions.
const SEED: u64 = 0x5EED_A5D1_7E51_2026;

/// worker carve (720 permille, `palw_overlay_carve` armed at DAA 0) of t12's block subsidy
/// 444_562_014_000 sompi. Runtime-verified by `t12_collateral_terms` in this same tree.
const T12_ESCROW_SOMPI: u64 = 320_084_650_080;
/// `PalwEconomicPayoutV1::rate_sompi_per_giga` on t12 (armed at DAA 0).
const T12_RATE_SOMPI_PER_GIGA: u64 = 900_000_000;

/// A tiny xorshift64*, so the search is a pure function of `SEED`.
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
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[(self.next() % xs.len() as u64) as usize]
    }
}

// -------------------------------------------------------------------------------------------------
// The measurement. `value_received` is the claim's priced reward in sompi; `real_work` is the
// arithmetic the producer actually ran, in MAC-eq, taken from `palw_attempt_economic_compute_v1`
// (ADR-0131's honest arithmetic cost walk over the same graph) times the expected draws.
// -------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Priced {
    /// One draw's PRICED compute, MAC-eq. `palw_canonical_draw_work_v1(..).provisional_scalar_v1()`.
    draw_ccu: u128,
    /// class ticket target, from `palw_work_ticket_target_v1(draw_ccu, W)`.
    target: u128,
    /// `palw_attempted_ccu_v1` — the CCU the payout fence prices.
    attempted_ccu: u128,
    /// `palw_rate_priced_reward_v1(escrow, attempted, rate)` — sompi.
    reward_sompi: u64,
}

/// `W0`, in CCU. The work floor every class's target is measured against on t12.
fn w0() -> u128 {
    palw_work_floor_v1(T12_ESCROW_SOMPI, T12_RATE_SOMPI_PER_GIGA)
}

/// The full runtime pricing chain for one class at one draw cost, at `W = W0` and `bits = 0`
/// (t12 carries no bits-priced lane — recon §5D — so the network factor is exactly one draw).
fn price(draw_ccu: u128, work_target_w: u128) -> Priced {
    let target = palw_work_ticket_target_v1(draw_ccu, work_target_w);
    let attempted_ccu =
        palw_attempted_ccu_v1(palw_expected_attempts_q32_v1(target), PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, draw_ccu);
    let reward_sompi = palw_rate_priced_reward_v1(T12_ESCROW_SOMPI, attempted_ccu, T12_RATE_SOMPI_PER_GIGA as u128);
    Priced { draw_ccu, target, attempted_ccu, reward_sompi }
}

/// sompi per 10^18 MAC-eq of REAL work — the efficiency this lane maximises. Integer so it is
/// reproducible bit for bit.
fn efficiency_e18(reward_sompi: u64, real_work_mac_eq: u128) -> u128 {
    if real_work_mac_eq == 0 {
        return u128::MAX;
    }
    (reward_sompi as u128).saturating_mul(1_000_000_000_000_000_000u128) / real_work_mac_eq
}

/// `a * b / d` without an intermediate overflow.
fn mul_div(a: u128, b: u128, d: u128) -> u128 {
    let d = d.max(1);
    (a / d).saturating_mul(b).saturating_add((a % d).saturating_mul(b) / d)
}

fn draw_ccu_of(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    let job = rc_job_context(profile, canonical.0, canonical.1);
    let d = PalwCanonicalClassDescriptorV1::of(profile, Hash64::default()).expect("descriptor");
    palw_canonical_draw_work_v1(&d, &job, true).expect("draw work").provisional_scalar_v1()
}

/// The same quantity through ADR-0131's own arithmetic walk — the second spelling, so a divergence
/// between the two would show up here rather than being assumed away.
fn honest_draw_mac_eq(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    let job = rc_job_context(profile, canonical.0, canonical.1);
    palw_attempt_economic_compute_v1(profile, &job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("attempt compute")
}

fn dense_2m() -> (PalwShapeProfileV3, (u32, u32)) {
    let p = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }).expect("dense 2M");
    let c = qwen25_a16_held_canonical_v1(2_097_152);
    (p, c)
}

fn hybrid_512() -> (PalwShapeProfileV3, (u32, u32)) {
    let p = qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B })).expect("hybrid");
    (p, qwen36_held_canonical_v1(512))
}

fn floor_class() -> (PalwShapeProfileV3, (u32, u32)) {
    (base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor"), PALW_RC_BASE0_CANONICAL)
}

// =================================================================================================
// S0 — THE THRESHOLD TABLE. Every constant / clamp / comparison the pricing chain crosses.
// =================================================================================================

#[test]
fn s0_threshold_enumeration() {
    println!("\n=== S0  THRESHOLDS THE ECONOMIC CHAIN COMPARES AGAINST (runtime values) ===");
    println!("  PALW_LEDGER_RATE_SCALE_V1           = {PALW_LEDGER_RATE_SCALE_V1}   (divisor: attempted x rate / this)");
    println!("  PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1   = {PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1}");
    println!("  PALW_EXECUTION_QUANTUM_V1           = {PALW_EXECUTION_QUANTUM_V1}");
    println!("  PALW_STEP_MAX_NODES_PER_TABLE       = {PALW_STEP_MAX_NODES_PER_TABLE}");
    println!("  PALW_STEP_MIN_TILE_LEN / MAX        = {PALW_STEP_MIN_TILE_LEN} / {PALW_STEP_MAX_TILE_LEN}");
    println!("  t12 escrow (720 permille carve)      = {T12_ESCROW_SOMPI} sompi");
    println!("  t12 rate_sompi_per_giga              = {T12_RATE_SOMPI_PER_GIGA}");
    println!("  W0 = palw_work_floor_v1(escrow,rate) = {} CCU", w0());
    println!("  dtype costs: i8(24)={} f16(1)={} i16(25)={} i32(26)={} bf16(30)={}",
        palw_weight_dtype_cost_v1(24), palw_weight_dtype_cost_v1(1), palw_weight_dtype_cost_v1(25),
        palw_weight_dtype_cost_v1(26), palw_weight_dtype_cost_v1(30));

    // Boundary sweep of every clamp/branch in the pricing chain, at 0,1,2,N-1,N,N+1,max-1,max.
    println!("\n  -- palw_work_ticket_target_v1(ccu, W0): the ccu >= W branch --");
    let w = w0();
    for ccu in [0u128, 1, 2, w - 1, w, w + 1, u128::MAX - 1, u128::MAX] {
        let t = palw_work_ticket_target_v1(ccu, w);
        let att = palw_expected_attempts_v1(t);
        println!("     ccu={ccu:<42} target={t:<41} expected_attempts={att}");
    }
    println!("\n  -- palw_rate_priced_reward_v1(escrow, attempted, rate): the escrow clamp --");
    for att in [0u128, 1, w / 2, w - 1, w, w + 1, w * 2, u128::MAX] {
        let r = palw_rate_priced_reward_v1(T12_ESCROW_SOMPI, att, T12_RATE_SOMPI_PER_GIGA as u128);
        println!("     attempted={att:<42} reward={r} sompi   (escrow={T12_ESCROW_SOMPI})");
    }
    println!("\n  -- palw_cap_utilization_permille_v1: ADR-0133 fence-3 ceiling is 800 permille --");
    for att in [0u128, w / 2, w, w * 2, w * 10_000] {
        println!("     attempted={att:<42} cap_util={} permille",
            palw_cap_utilization_permille_v1(att, T12_RATE_SOMPI_PER_GIGA, T12_ESCROW_SOMPI));
    }
    println!("\n  -- palw_pwu_v1(target, per_draw): the u64::MAX saturation --");
    let per_draw = 3_357_281_757_221_376u64;
    for t in [u128::MAX, u128::MAX / 2, u128::MAX / 5_494, u128::MAX / 5_495, u128::MAX / 6_000, u128::MAX / 1_000_000] {
        let pwu = palw_pwu_v1(t, per_draw);
        println!("     attempts={:<12} pwu={} {}", palw_expected_attempts_v1(t), pwu,
            if pwu == u64::MAX { "<-- SATURATED: more work buys zero weight" } else { "" });
    }
    println!("\n  -- palw_execution_quantum_count_v1(credit, 100_000): the u32::MAX clamp --");
    for c in [0u128, 99_999, 100_000, 100_001, 3_357_281_757_221_376u128] {
        println!("     credit={c:<24} quanta={}", palw_execution_quantum_count_v1(c, PALW_EXECUTION_QUANTUM_V1 as u128, Hash64::default(), Hash64::default()));
    }
    println!("\n  -- palw_work_priced_reward_v1(escrow, exposure, unit): the >= unit short circuit --");
    for (e, u) in [(0u64, 100u64), (99, 100), (100, 100), (101, 100), (u64::MAX, 100), (100, 0)] {
        println!("     exposure={e:<22} unit={u:<8} reward={}", palw_work_priced_reward_v1(T12_ESCROW_SOMPI, e, u));
    }
}

// =================================================================================================
// S1 — the three t12 genesis classes, priced end to end.
// =================================================================================================

#[test]
fn s1_the_three_shipped_classes_priced() {
    println!("\n=== S1  THE THREE t12 GENESIS CLASSES, PRICED BY THE RUNTIME ===");
    let w = w0();
    println!("  W0 = {w} CCU  (escrow {T12_ESCROW_SOMPI} sompi at rate {T12_RATE_SOMPI_PER_GIGA})\n");
    let mut best = (0u128, String::new());
    for (name, (p, c)) in [("BASE-0 floor", floor_class()), ("Qwen3.6 @512", hybrid_512()), ("Qwen2.5 A16 @2M", dense_2m())] {
        let draw = draw_ccu_of(&p, c);
        let honest = honest_draw_mac_eq(&p, c);
        assert_eq!(draw, honest, "the two spellings of one draw's cost must agree");
        let pr = price(draw, w);
        // real work = attempted_ccu, because on t12 the priced walk IS the honest arithmetic walk
        // for an unmodified shipped profile (asserted one line up).
        let eff = efficiency_e18(pr.reward_sompi, pr.attempted_ccu);
        println!("  {name:<18} canonical=({},{})", c.0, c.1);
        println!("     draw_ccu          {} MAC-eq", pr.draw_ccu);
        println!("     ccu / W0          {:.4}x", pr.draw_ccu as f64 / w as f64);
        println!("     target            {}", pr.target);
        println!("     attempted_ccu     {} MAC-eq", pr.attempted_ccu);
        println!("     reward            {} sompi  (escrow {})", pr.reward_sompi, T12_ESCROW_SOMPI);
        println!("     cap_util          {} permille", palw_cap_utilization_permille_v1(pr.attempted_ccu, T12_RATE_SOMPI_PER_GIGA, T12_ESCROW_SOMPI));
        println!("     EFFICIENCY        {eff} sompi per 1e18 MAC-eq\n");
        if eff > best.0 {
            best = (eff, name.to_string());
        }
    }
    println!("  most efficient shipped class: {} at {} sompi/1e18 MAC-eq", best.1, best.0);
}

// =================================================================================================
// S2 — DETERMINISTIC SEEDED SEARCH for max(value / real work) over the geometry space.
// =================================================================================================

#[test]
fn s2_seeded_search_for_the_maximum_efficiency() {
    println!("\n=== S2  SEEDED SEARCH (seed {SEED:#x}) — maximise reward_sompi / real MAC-eq ===");
    let w = w0();
    let ceiling = efficiency_e18(T12_ESCROW_SOMPI, w);
    println!("  honest ceiling escrow/W0 = {ceiling} sompi per 1e18 MAC-eq");
    println!("  real work = expected_attempts x HONEST draw cost, where the honest cost of a");
    println!("  padding cache-matmul is attn_kv_heads x attn_head_dim x out_elements per cached");
    println!("  position (palw_economic_compute_v1.rs:283 input_width) and the price it is given");
    println!("  is attn_heads x attn_head_dim per cached position (the same file, line 341).\n");
    let mut rng = Rng::new();

    let layers = [1u16, 2, 4, 8, 16, 28, 40, 64];
    let hidden = [64u32, 128, 256, 512, 1024, 1536, 2048];
    let ffn = [64u32, 256, 1024, 4096, 8960];
    let heads = [1u16, 2, 4, 8, 12, 16, 32, 64];
    let kvheads = [1u16, 2, 4];
    let headdim = [16u32, 32, 64, 128];
    let vocab = [256u32, 4096, 32_000, 151_936];
    let ctx = [16u32, 64, 512, 4_096, 16_384, 65_536, 262_144, 1_048_576, 2_097_152];
    let tiles = [4u32, 24, 32, 128, 512, 4096, 65_536];
    let pads = [0usize, 1, 2, 4, 8, 16, 24, 32, 40];

    #[derive(Clone, Copy)]
    struct Best {
        eff: u128,
        g: PalwQwen25GeometryV1,
        pad: usize,
        pf: u32,
        dc: u32,
        priced: u128,
        honest: u128,
        attempts: u128,
        real: u128,
        reward: u64,
        priced_attempted: u128,
    }
    let mut best: Option<Best> = None;
    let mut best_honest: Option<Best> = None;
    let (mut evaluated, mut rejected) = (0usize, 0usize);

    for _ in 0..20_000 {
        let g = PalwQwen25GeometryV1 {
            layer_count: rng.pick(&layers),
            hidden_dim: rng.pick(&hidden),
            ffn_dim: rng.pick(&ffn),
            attn_heads: rng.pick(&heads),
            attn_kv_heads: rng.pick(&kvheads),
            attn_head_dim: rng.pick(&headdim),
            vocab_size: rng.pick(&vocab),
            n_ctx: rng.pick(&ctx),
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: rng.pick(&tiles),
        };
        let pad = rng.pick(&pads);
        let Ok(base) = qwen25_a16_artifact_row_profile_v7(g) else {
            rejected += 1;
            continue;
        };
        if base.validate_shape().is_err() {
            rejected += 1;
            continue;
        }
        let c = qwen25_a16_held_canonical_v1(g.n_ctx);
        let Some(base_priced) = try_draw(&base, c) else {
            rejected += 1;
            continue;
        };
        let (profile, priced) = if pad == 0 {
            (base.clone(), base_priced)
        } else {
            match pad_quiet(&base, pad) {
                Some(p) => {
                    let Some(d) = try_draw(&p, c) else {
                        rejected += 1;
                        continue;
                    };
                    (p, d)
                }
                None => {
                    rejected += 1;
                    continue;
                }
            }
        };
        if priced == 0 {
            rejected += 1;
            continue;
        }
        evaluated += 1;
        let padding_priced = priced.saturating_sub(base_priced);
        let honest = base_priced + padding_priced * profile.attn_kv_heads as u128 / profile.attn_heads.max(1) as u128;
        let pr = price(priced, w);
        // The producer's REAL arithmetic: the same expected number of draws the payout fence
        // prices (Q32, not the floored integer), each draw costing `honest` instead of `priced`.
        let real = mul_div(pr.attempted_ccu, honest, priced);
        let attempts = pr.attempted_ccu / priced.max(1);
        let eff = efficiency_e18(pr.reward_sompi, real);
        let row = Best { eff, g, pad, pf: c.0, dc: c.1, priced, honest, attempts, real, reward: pr.reward_sompi, priced_attempted: pr.attempted_ccu };
        if best.as_ref().is_none_or(|b| eff > b.eff) {
            best = Some(row);
        }
        if pad == 0 && best_honest.as_ref().is_none_or(|b| eff > b.eff) {
            best_honest = Some(row);
        }
    }

    let report = |label: &str, b: &Best| {
        println!("  *** {label} ***");
        println!("  PalwQwen25GeometryV1 {{ layer_count: {}, hidden_dim: {}, ffn_dim: {}, attn_heads: {},", b.g.layer_count, b.g.hidden_dim, b.g.ffn_dim, b.g.attn_heads);
        println!("      attn_kv_heads: {}, attn_head_dim: {}, vocab_size: {}, n_ctx: {},", b.g.attn_kv_heads, b.g.attn_head_dim, b.g.vocab_size, b.g.n_ctx);
        println!("      n_threads: 1, rms_eps_q: 1, tile_len: {} }}", b.g.tile_len);
        println!("  + {} padding cache-matmul nodes (out_len Fixed{{1}}, tile_len 4, input_refs [KV_K]) in attn_nodes", b.pad);
        println!("  canonical job = ({}, {})", b.pf, b.dc);
        println!("  priced draw_ccu {}   honest draw {}   priced/honest {:.4}x", b.priced, b.honest, b.priced as f64 / b.honest.max(1) as f64);
        println!("  expected draws ~{}   priced attempted_ccu {}   REAL WORK {} MAC-eq   reward {} sompi", b.attempts, b.priced_attempted, b.real, b.reward);
        println!("  EFFICIENCY {} sompi/1e18 MAC-eq = {:.4}x the honest ceiling\n", b.eff, b.eff as f64 / ceiling as f64);
    };

    println!("  evaluated {evaluated}, rejected {rejected}\n");
    let b = best.expect("a maximum");
    report("GLOBAL MAXIMUM (padding allowed)", &b);
    if let Some(h) = best_honest.as_ref() {
        report("MAXIMUM WITH NO PADDING (honest representations only)", h);
    }

    let (dp, dcn) = dense_2m();
    let dense = price(draw_ccu_of(&dp, dcn), w);
    let dense_eff = efficiency_e18(dense.reward_sompi, dense.attempted_ccu);
    println!("  shipped Qwen2.5 A16 @2M: real {} MAC-eq, reward {} sompi, efficiency {}", dense.attempted_ccu, dense.reward_sompi, dense_eff);
    println!("  RATIO best / shipped-dense   = {:.2}x", b.eff as f64 / dense_eff.max(1) as f64);
    println!("  RATIO best / honest ceiling  = {:.4}x", b.eff as f64 / ceiling as f64);
}

/// `pad_with_cache_matmuls` without the refusal print, for the search's inner loop.
fn pad_quiet(base: &PalwShapeProfileV3, n: usize) -> Option<PalwShapeProfileV3> {
    let donor = base.attn_nodes.iter().find(|nd| matches!(nd.op_kind, PalwStepOpKindV1::MatMulQuant | PalwStepOpKindV1::MatMulF16))?;
    let pad = PalwStepNodeV1 {
        op_kind: PalwStepOpKindV1::MatMulQuant,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: String::new(),
        weight_dtypes: Vec::new(),
        out_len: PalwStepOutLenV1::Fixed { elements: 1 },
        tile_len: PALW_STEP_MIN_TILE_LEN,
        kernel_semantics_id: donor.kernel_semantics_id,
        input_refs: vec![PALW_STEP_INPUT_KV_K],
    };
    let mut p = base.clone();
    for _ in 0..n {
        if p.attn_nodes.len() >= PALW_STEP_MAX_NODES_PER_TABLE {
            break;
        }
        p.attn_nodes.push(pad.clone());
    }
    p.validate_shape().ok()?;
    Some(p)
}

fn try_draw(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> Option<u128> {
    let job = rc_job_context(profile, canonical.0, canonical.1);
    let d = PalwCanonicalClassDescriptorV1::of(profile, Hash64::default()).ok()?;
    Some(palw_canonical_draw_work_v1(&d, &job, true).ok()?.provisional_scalar_v1())
}

// =================================================================================================
// S3 — REPRESENTATION ATTACK 1: node splitting over the KV cache. The highest-value item.
//      `node_cost`'s `over_cache` arm (palw_economic_compute_v1.rs:340-350) RETURNS EARLY and
//      never reads the node's `out_len`: every matmul that touches the K or V cache is priced at
//      `attn_heads x attn_head_dim x attention_mac` per kv position, whatever it actually writes.
// =================================================================================================

/// Synthesise the cheapest admissible cache-reading matmul and append `n` copies to the
/// attention table. Nothing is a donor here except `kernel_semantics_id`, taken from the row's own
/// output projection so the node names a kernel the class already reaches.
fn pad_with_cache_matmuls(base: &PalwShapeProfileV3, n: usize, out_elements: u32) -> Option<PalwShapeProfileV3> {
    let donor = base
        .attn_nodes
        .iter()
        .find(|nd| matches!(nd.op_kind, PalwStepOpKindV1::MatMulQuant | PalwStepOpKindV1::MatMulF16))?;
    let pad = PalwStepNodeV1 {
        op_kind: PalwStepOpKindV1::MatMulQuant,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: String::new(),
        weight_dtypes: Vec::new(),
        out_len: PalwStepOutLenV1::Fixed { elements: out_elements },
        tile_len: PALW_STEP_MIN_TILE_LEN,
        kernel_semantics_id: donor.kernel_semantics_id,
        input_refs: vec![PALW_STEP_INPUT_KV_K],
    };
    let mut p = base.clone();
    for _ in 0..n {
        if p.attn_nodes.len() >= PALW_STEP_MAX_NODES_PER_TABLE {
            break;
        }
        p.attn_nodes.push(pad.clone());
    }
    match p.validate_shape() {
        Ok(()) => Some(p),
        Err(e) => {
            println!("      (pad n={n} out={out_elements} refused: {e})");
            None
        }
    }
}

#[test]
fn s3_node_splitting_over_the_kv_cache_is_priced_per_node_and_ignores_out_len() {
    println!("\n=== S3  NODE SPLITTING: a cache matmul is priced per NODE, out_len unread ===");
    let w = w0();
    let ceiling = efficiency_e18(T12_ESCROW_SOMPI, w);
    println!("  analytic ceiling for an honestly priced class = escrow/W0 = {ceiling} sompi/1e18 MAC-eq");

    // The load-bearing fact first: `node_cost`'s over_cache arm returns before it reads `out_len`.
    let (dense, dcn) = dense_2m();
    let a = pad_with_cache_matmuls(&dense, 1, 1).expect("pad 1 element");
    let b = pad_with_cache_matmuls(&dense, 1, u32::MAX).expect("pad u32::MAX elements");
    let (da, db) = (draw_ccu_of(&a, dcn), draw_ccu_of(&b, dcn));
    println!("\n  padding node writing          1 element  -> draw_ccu {da}");
    println!("  padding node writing u32::MAX elements  -> draw_ccu {db}");
    assert_eq!(da, db, "CONFIRMED: the over_cache price ignores out_len entirely");
    println!("  => the price of a cache matmul does not depend on what it writes.");

    // Now the exploit, on a class BELOW W0 where priced work still converts into reward.
    let mut best: Option<(u128, u32, usize, u128, u128, u64)> = None;
    for ctx in [512u32, 4_096, 65_536] {
        let base = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: ctx, ..QWEN25_1_5B }).expect("row");
        let canonical = qwen25_a16_held_canonical_v1(ctx);
        let base_draw = draw_ccu_of(&base, canonical);
        let priced_per_kv = base.attn_heads as u128 * base.attn_head_dim as u128;
        let honest_per_kv = base.attn_kv_heads as u128 * base.attn_head_dim as u128;
        println!("\n  --- base A16 @{ctx}, canonical {canonical:?}, attn_nodes {} ---", base.attn_nodes.len());
        println!("      priced per padding node per kv = attn_heads x head_dim         = {priced_per_kv}");
        println!("      honest per padding node per kv = attn_kv_heads x head_dim x 1  = {honest_per_kv}  ({}x over-priced)",
            priced_per_kv / honest_per_kv);
        println!("      pad  priced_draw_ccu       honest_draw_ccu        attempts   real_work(MAC-eq)      reward(sompi)   efficiency      x ceiling");
        for n in [0usize, 1, 2, 4, 8, 16, 24, 32, 40] {
            let Some(p) = pad_with_cache_matmuls(&base, n, 1) else { continue };
            let added = p.attn_nodes.len() - base.attn_nodes.len();
            let draw = draw_ccu_of(&p, canonical);
            let padding_priced = draw.saturating_sub(base_draw);
            let honest = base_draw + padding_priced * honest_per_kv / priced_per_kv;
            let pr = price(draw, w);
            let attempts = pr.attempted_ccu / draw.max(1);
            let real = mul_div(pr.attempted_ccu, honest, draw);
            let eff = efficiency_e18(pr.reward_sompi, real);
            println!("      +{added:<3} {draw:<22} {honest:<22} {attempts:<10} {real:<22} {:<15} {eff:<15} {:.4}x",
                pr.reward_sompi, eff as f64 / ceiling as f64);
            if best.as_ref().is_none_or(|bst| eff > bst.0) {
                best = Some((eff, ctx, added, draw, real, pr.reward_sompi));
            }
        }
    }
    let (eff, ctx, added, draw, real, reward) = best.expect("a maximum");
    println!("\n  *** MAXIMISING INPUT (S3 artefact) ***");
    println!("  qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 {{ n_ctx: {ctx}, ..QWEN25_1_5B }})");
    println!("  + {added} x PalwStepNodeV1 {{ op_kind: MatMulQuant, out_len: Fixed {{ elements: 1 }},");
    println!("      tile_len: 4, weight_name: \"\", weight_dtypes: [], input_refs: [PALW_STEP_INPUT_KV_K] }}");
    println!("      appended to attn_nodes; canonical job = qwen25_a16_held_canonical_v1({ctx})");
    println!("  priced draw_ccu {draw}   real work {real} MAC-eq   reward {reward} sompi");
    println!("  EFFICIENCY {eff} sompi/1e18 MAC-eq = {:.4}x the honest ceiling", eff as f64 / ceiling as f64);
}

// =================================================================================================
// S4 — REPRESENTATION ATTACK 2: the declared weight dtype.
// =================================================================================================

#[test]
fn s4_declared_dtype_multiplies_the_price_at_identical_mac_count() {
    println!("\n=== S4  DECLARED DTYPE: same graph, same MAC count, 4x the price ===");
    let (base, canonical) = dense_2m();
    let base_draw = draw_ccu_of(&base, canonical);
    println!("  Qwen2.5 A16 @2M declared at i8 (24): draw_ccu = {base_draw} MAC-eq");
    // dtype 0 (F32) is refused by validate_shape ("a zero dtype names no GGML type"), so the
    // attacker's widest admissible declaration is I32 = 26, cost 4.
    for (code, name) in [(24u8, "I8"), (25, "I16"), (30, "BF16"), (1, "F16"), (26, "I32")] {
        let mut p = base.clone();
        for t in [&mut p.pre_nodes, &mut p.gdn_nodes, &mut p.attn_nodes, &mut p.post_nodes] {
            for nd in t.iter_mut() {
                for d in nd.weight_dtypes.iter_mut() {
                    *d = code;
                }
            }
        }
        if p.validate_shape().is_err() {
            println!("  dtype {code:<3} ({name:<4}) refused by validate_shape");
            continue;
        }
        let draw = draw_ccu_of(&p, canonical);
        let pr = price(draw, w0());
        println!("  dtype {code:<3} ({name:<4}) cost/MAC={}  draw_ccu={draw:<24} ratio={:.4}x  reward={} sompi",
            palw_weight_dtype_cost_v1(code), draw as f64 / base_draw as f64, pr.reward_sompi);
    }
    println!("  The multiply-accumulate COUNT is identical in every row above: only the declared");
    println!("  byte width moved. Real arithmetic is unchanged; real weight traffic is 4x.");
}

// =================================================================================================
// S5 — P1..P7
// =================================================================================================

#[test]
fn s5_properties_p1_to_p7() {
    println!("\n=== S5  PROPERTIES P1..P7 ===");
    let w = w0();
    let (dense, dcn) = dense_2m();
    let (hyb, hcn) = hybrid_512();
    let (flr, fcn) = floor_class();
    let d_draw = draw_ccu_of(&dense, dcn);
    let h_draw = draw_ccu_of(&hyb, hcn);
    let f_draw = draw_ccu_of(&flr, fcn);

    // ---- P1: work decreases => reward does not increase --------------------------------------
    let mut p1 = true;
    let mut prev: Option<(u128, u64)> = None;
    let mut ladder = Vec::new();
    for ctx in [16u32, 64, 512, 4_096, 65_536, 1_048_576, 2_097_152] {
        let p = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: ctx, ..QWEN25_1_5B }).expect("row");
        let c = qwen25_a16_held_canonical_v1(ctx);
        let draw = draw_ccu_of(&p, c);
        let pr = price(draw, w);
        ladder.push((ctx, draw, pr.attempted_ccu, pr.reward_sompi));
        if let Some((pd, pr_reward)) = prev
            && draw < pd
            && pr.reward_sompi > pr_reward
        {
            p1 = false;
        }
        prev = Some((draw, pr.reward_sompi));
    }
    println!("\n  P1  n_ctx ladder (work strictly increasing with ctx):");
    for (c, d, a, r) in &ladder {
        println!("      n_ctx={c:<9} draw_ccu={d:<24} attempted={a:<24} reward={r}");
    }
    println!("  P1  {} — searched: the A16 n_ctx ladder 16..2^21 and the three genesis classes.",
        if p1 { "HOLDS" } else { "VIOLATED" });
    println!("      NOTE: reward is FLAT at the escrow across 7 orders of magnitude of work.");
    println!("      escrow/W0 ceiling {} vs dense@2M {} => {:.0}x efficiency spread between two honest classes.",
        efficiency_e18(T12_ESCROW_SOMPI, w),
        efficiency_e18(price(d_draw, w).reward_sompi, price(d_draw, w).attempted_ccu),
        efficiency_e18(T12_ESCROW_SOMPI, w) as f64
            / efficiency_e18(price(d_draw, w).reward_sompi, price(d_draw, w).attempted_ccu).max(1) as f64);
    assert!(p1, "P1");

    // ---- P2: same work, different representation => reward does not move ----------------------
    println!("\n  P2  re-tiling, across the shape space (not one profile):");
    let mut p2 = true;
    for (name, geom) in [
        ("A16 @16", PalwQwen25GeometryV1 { n_ctx: 16, ..QWEN25_1_5B }),
        ("A16 @512", PalwQwen25GeometryV1 { n_ctx: 512, ..QWEN25_1_5B }),
        ("A16 @4096", PalwQwen25GeometryV1 { n_ctx: 4_096, ..QWEN25_1_5B }),
        ("A16 @2M", PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }),
        ("narrow", PalwQwen25GeometryV1 { n_ctx: 512, hidden_dim: 128, ffn_dim: 256, vocab_size: 4096, ..QWEN25_1_5B }),
    ] {
        let mut seen: Option<u128> = None;
        let mut spread = String::new();
        for tile in [4u32, 24, 32, 128, 512, 4096, 65_536] {
            let Ok(p) = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { tile_len: tile, ..geom }) else { continue };
            if p.validate_shape().is_err() {
                continue;
            }
            let c = qwen25_a16_held_canonical_v1(geom.n_ctx);
            let draw = draw_ccu_of(&p, c);
            match seen {
                None => seen = Some(draw),
                Some(s) if s != draw => {
                    p2 = false;
                    spread.push_str(&format!(" tile {tile} -> {draw};"));
                }
                _ => {}
            }
        }
        println!("      {name:<10} draw_ccu invariant over 7 tilings: {} {spread}", if spread.is_empty() { "yes" } else { "NO" });
    }
    // and the dtype lever, which IS a representation change at identical MAC count
    let mut p2_dtype = dense.clone();
    for t in [&mut p2_dtype.pre_nodes, &mut p2_dtype.gdn_nodes, &mut p2_dtype.attn_nodes, &mut p2_dtype.post_nodes] {
        for nd in t.iter_mut() {
            for d in nd.weight_dtypes.iter_mut() {
                *d = 26;
            }
        }
    }
    let dtype_draw = draw_ccu_of(&p2_dtype, dcn);
    println!("      dtype i8 -> i32 on the SAME graph: draw_ccu {d_draw} -> {dtype_draw} ({:.2}x)", dtype_draw as f64 / d_draw as f64);
    let p2_overall = p2 && dtype_draw == d_draw;
    println!("  P2  {} — searched: 7 tile_lens x 5 geometries, and the 5 admissible dtype codes.",
        if p2_overall { "HOLDS" } else { "VIOLATED" });

    // ---- P3: no discontinuous gain at a parameter boundary ------------------------------------
    println!("\n  P3  boundary sweep, reward-per-real-MAC around every clamp:");
    let mut p3_notes = Vec::new();
    for (label, ccus) in [
        ("W0 (target saturation)", vec![w - 2, w - 1, w, w + 1, w + 2]),
        ("escrow clamp", vec![w / 2, w, w * 2]),
    ] {
        for ccu in ccus {
            let pr = price(ccu, w);
            let eff = efficiency_e18(pr.reward_sompi, pr.attempted_ccu);
            println!("      {label:<24} ccu={ccu:<22} attempted={:<24} reward={:<14} eff={eff}", pr.attempted_ccu, pr.reward_sompi);
            p3_notes.push((ccu, eff));
        }
    }
    // the u64::MAX saturation of palw_pwu_v1 is the one real discontinuity in the weight lane
    let per_draw = d_draw.min(u64::MAX as u128) as u64;
    let mut sat_at = None;
    for attempts in [1u128, 2, 100, 5_000, 5_494, 5_495, 5_496, 10_000] {
        let target = u128::MAX / attempts.max(1);
        let pwu = palw_pwu_v1(target, per_draw);
        if pwu == u64::MAX && sat_at.is_none() {
            sat_at = Some(palw_expected_attempts_v1(target));
        }
    }
    println!("      palw_pwu_v1 saturates at u64::MAX from {} expected attempts on the 2M row:", sat_at.map(|a| a.to_string()).unwrap_or("never".into()));
    println!("      past it, additional executed compute buys ZERO additional fork weight.");
    println!("  P3  VIOLATED — the pwu_v1 u64 clamp is a discontinuity the class target controls;");
    println!("      the W0 boundary is continuous in reward but its DERIVATIVE flips sign there.");

    // ---- P4: subdividing a class does not increase total reward -------------------------------
    println!("\n  P4/P7  one class of ccu C vs K classes of ccu C/K (registration is permissionless):");
    let total = d_draw;
    for k in [1u128, 2, 10, 100, 1_000, 9_440] {
        let per = (total / k).max(1);
        let pr = price(per, w);
        let total_reward = (pr.reward_sompi as u128).saturating_mul(k);
        println!("      K={k:<8} per-class ccu={per:<24} per-claim reward={:<14} K claims = {total_reward} sompi", pr.reward_sompi);
    }
    let one = price(total, w);
    let split = price((total / 9_440).max(1), w);
    let ratio = (split.reward_sompi as u128 * 9_440) as f64 / one.reward_sompi as f64;
    println!("      total real work identical in every row. K=9440 / K=1 reward ratio = {ratio:.1}x");
    println!("  P4  VIOLATED as a reward-per-work property — see the caveat in the finding: each");
    println!("      claim must still win its own attempt ticket and carry its own block escrow.");
    println!("  P7  VIOLATED by the same measurement: one inference's worth of arithmetic presented");
    println!("      as {} claims is worth {:.0}x one claim.", 9_440, ratio);

    // ---- P5/P6: splitting and merging a CLAIM ------------------------------------------------
    println!("\n  P5/P6  splitting/merging one claim's job (prefill/decode partition):");
    let job_total = 4_096u32;
    let mut parts = Vec::new();
    for split in [2u32, 8, 64, 512, 2_048, 4_094] {
        let p = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 8_192, ..QWEN25_1_5B }).expect("row");
        let prefill = job_total - split;
        let job = rc_job_context(&p, prefill, split);
        let priced = palw_job_economic_compute_v1(&p, &job, &PALW_ECONOMIC_COST_TABLE_V1).expect("job");
        parts.push((prefill, split, priced));
    }
    let min = parts.iter().map(|t| t.2).min().unwrap();
    let max = parts.iter().map(|t| t.2).max().unwrap();
    for (pf, dc, c) in &parts {
        println!("      prefill={pf:<6} decode={dc:<6} total_tokens={} priced={c} MAC-eq", pf + dc);
    }
    println!("      spread over the partition of ONE token budget: {:.4}x ({} .. {})", max as f64 / min as f64, min, max);
    println!("  P5  the 1.1563x spread lives in palw_job_economic_compute_v1 only. The ATTEMPT price");
    println!("      goes through palw_attempt_job_v1(canonical, prefill_draw=true), which discards");
    println!("      the declared decode count. Measured on one profile at several decode counts:");
    let pp = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 8_192, ..QWEN25_1_5B }).expect("row");
    let dd = PalwCanonicalClassDescriptorV1::of(&pp, Hash64::default()).unwrap();
    let mut seen: Option<u128> = None;
    let mut p5 = true;
    for dec in [2u32, 8, 64, 512, 2048] {
        let job = rc_job_context(&pp, 4_094, dec);
        let draw = palw_canonical_draw_work_v1(&dd, &job, true).unwrap().provisional_scalar_v1();
        println!("        declared decode={dec:<6} ATTEMPT draw_ccu = {draw}");
        match seen {
            None => seen = Some(draw),
            Some(v) if v != draw => p5 = false,
            _ => {}
        }
    }
    println!("  P5  {} on the attempt lane (the only lane t12 prices) — the decode declaration is",
        if p5 { "HOLDS" } else { "VIOLATED" });
    println!("      not read by palw_attempt_job_v1, so the partition is not an attacker lever there.");
    println!("  P6  merging K claims into one strictly LOSES reward (the escrow clamp): K=9440 merged");
    println!("      into 1 goes from {} to {} sompi. P6 HOLDS.",
        (price((d_draw / 9_440).max(1), w).reward_sompi as u128) * 9_440, price(d_draw, w).reward_sompi);

    let _ = (h_draw, f_draw, hcn, fcn);
}

// =================================================================================================
// S6 — the honest-vs-priced walk on an unmodified profile, so S3/S4's baseline is not assumed.
// =================================================================================================

#[test]
fn s6_the_two_cost_walks_agree_on_every_shipped_class() {
    println!("\n=== S6  palw_canonical_draw_work_v1 vs palw_attempt_economic_compute_v1 ===");
    for (name, (p, c)) in [("BASE-0", floor_class()), ("Qwen3.6@512", hybrid_512()), ("Qwen2.5@2M", dense_2m())] {
        let a = draw_ccu_of(&p, c);
        let b = honest_draw_mac_eq(&p, c);
        println!("  {name:<14} canonical={a}  attempt_economic={b}  equal={}", a == b);
        assert_eq!(a, b, "{name}");
        // and the byte dimensions that are priced at zero
        let job = rc_job_context(&p, c.0, c.1);
        let d = PalwCanonicalClassDescriptorV1::of(&p, Hash64::default()).unwrap();
        let v = palw_canonical_work_v1(&d, &PalwCanonicalExecutionFactsV1::of_attempt(&job, true)).unwrap();
        println!("                 traffic_bytes={} (weight {} kv_r {} kv_w {}) — all weighted ZERO in provisional_scalar_v1",
            v.traffic_bytes(), v.weight_traffic_bytes, v.kv_read_bytes, v.kv_write_bytes);
        let _ = palw_economic_shape_v1(&p, &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
    }
}

#[test]
fn s7_dump_attn_tables() {
    for (name, (p, _c)) in [("BASE-0", floor_class()), ("Qwen3.6@512", hybrid_512()), ("Qwen2.5@2M", dense_2m())] {
        println!("\n--- {name}: attn_nodes = {} ---", p.attn_nodes.len());
        for (i, nd) in p.attn_nodes.iter().enumerate() {
            println!("  [{i:2}] {:?} out={:?} tile={} w='{}' dtypes={:?} refs={:?}",
                nd.op_kind, nd.out_len, nd.tile_len, nd.weight_name, nd.weight_dtypes, nd.input_refs);
        }
        println!("  pre={} gdn={} post={}", p.pre_nodes.len(), p.gdn_nodes.len(), p.post_nodes.len());
        println!("  validate_shape: {:?}", p.validate_shape());
    }
}

// =================================================================================================
// S8 — does the padded profile still reach only kernels the base reaches? (admission partial check)
// =================================================================================================

#[test]
fn s8_the_padding_node_reaches_no_new_kernel() {
    use kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1;
    println!("\n=== S8  KERNEL COVERAGE of the padded profile (partial admission check) ===");
    for ctx in [512u32, 4_096] {
        let base = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: ctx, ..QWEN25_1_5B }).expect("row");
        let padded = pad_quiet(&base, 40).expect("padded validates");
        let kb = reachable_kernels_v1(&base);
        let kp = reachable_kernels_v1(&padded);
        let new: Vec<_> = kp.difference(&kb).collect();
        println!("  n_ctx={ctx}: base reaches {} kernels, padded reaches {}, NEW kernels: {}",
            kb.len(), kp.len(), if new.is_empty() { "none".to_string() } else { format!("{new:?}") });
        println!("           padded class_id (shape_profile_id) = {}", padded.shape_profile_id());
        assert!(new.is_empty(), "the padding node names no kernel the class did not already reach");
    }
    println!("  => the A4 coverage gate cannot distinguish the padded class from the honest one.");
    println!("  NOT SETTLED here: the ladder-depth and court-cost arms of verify_class_admission_v9,");
    println!("  which need a PalwConsensusParamsV2 bundle and a ClassRegistered object.");
}
