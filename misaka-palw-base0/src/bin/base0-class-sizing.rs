//! **What a second BASE-0 class would cost the ruleset** — the numbers behind "add Qwen scale later".
//!
//! The plan is: ship the 4-layer liveness floor now, add a Qwen-scale BASE-0 class afterwards. The
//! parts of that plan the tree already answers, measured rather than assumed:
//!
//! * a class's `max_step_leaf_count` is a **bundle** field (`PalwCourtParamsV2`), and the bundle is
//!   what `palw_ruleset_id_v2` hashes. A class deeper than the court's ladder therefore cannot join
//!   a running chain at all — it needs a new ruleset, which is a flag day.
//! * but the ladder is `ceil(log2(leaves)) + terminal` rounds, so provisioning it for a class that
//!   does not exist yet costs **logarithmically**. This binary is how much.
//!
//! Everything else about the second class is derivable from its geometry, so it is checked here
//! too: that the coverage gate passes (it must — the ops are the same ten), that every dimension
//! fits `MAX_DOT_LEN`, and what the artifact would weigh.

use kaspa_consensus_core::palw_base0_profile::{
    PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, PALW_RC_BASE0_WORST_CASE, PalwBase0GeometryV1, base0_profile_v1,
};
use kaspa_consensus_core::palw_catalog_coverage::{PalwReachableKernelSetV1, verify_catalog_coverage_v1};
use kaspa_consensus_core::palw_step::{PALW_STEP_MAX_LEAVES, PalwShapeProfileV3, step_leaf_count, worst_case_step_leaf_count_v1};
use kaspa_consensus_core::palw_step_refute::catalogued_kernel_ids_v1;
use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, trace_scheme_id_v2};
use kaspa_hashes::Hash64;

/// A Qwen-2B-scale geometry that BASE-0 can actually express.
///
/// Three departures from the pinned Qwen, and each is forced rather than chosen:
///
/// * **no GQA** — `wk`/`wv` are square in the artifact, so `n_head_kv = n_heads`;
/// * **no GatedDeltaNet** — BASE-0 is a plain decoder-only transformer, which rules out the
///   hybrid Qwen3.5 and points at a dense model;
/// * **vocab 128_256, not 151_936** — `Base0ShapeV1::validate` bounds every dimension by
///   `MAX_DOT_LEN` (131_071), so a Qwen-family vocabulary is refused and a Llama-3-class one fits.
///
/// So this is "Qwen scale", not "Qwen". Named here so the second class's numbers are checked long
/// before anyone can register it — the one thing that cannot be derived is `artifact_root`, and
/// that is the weights.
const QWEN_SCALE: PalwBase0GeometryV1 = PalwBase0GeometryV1 {
    layer_count: 28,
    hidden_dim: 1536,
    ffn_dim: 8960,
    attn_heads: 12,
    attn_head_dim: 128,
    vocab_size: 128_256,
    n_ctx: 4_096,
    n_threads: 1,
    rms_eps_q: 1 << 8,
    tile_len: 64,
};

/// Prefill/decode pairs a Qwen-scale class might name as canonical and worst case. The worst case
/// is what sets the ladder, so it is swept rather than guessed.
const QWEN_WORST_CASES: &[(u32, u32)] = &[(64, 64), (512, 128), (2048, 512)];

fn job_context(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
    let mut ctx = PalwJobContextV2 {
        version: PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"misaka-palw-rc".to_vec(),
        job_id: Hash64::default(),
        job_nullifier: Hash64::default(),
        assignment_id: Hash64::default(),
        execution_seed: [0; 32],
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: profile.shape_profile_id(),
        trace_scheme_id: Hash64::default(),
        cu_ruleset_id: Hash64::default(),
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: Hash64::default(),
        declared_prefill_tokens: prefill,
        exact_decode_tokens: decode,
        max_context_tokens: profile.n_ctx,
    };
    ctx.trace_scheme_id = trace_scheme_id_v2();
    ctx
}

/// `ceil(log2(n))` for `n >= 2`, by the same `next_power_of_two` identity `PalwCourtParamsV2` uses.
fn bisection_rounds(leaves: u64) -> u32 {
    leaves.max(2).next_power_of_two().trailing_zeros()
}

/// int8 weight bytes, from the shape alone: two `[vocab][d_model]` tables plus four square
/// projections and three FFN matrices per layer.
fn artifact_bytes(g: &PalwBase0GeometryV1) -> u128 {
    let d = g.hidden_dim as u128;
    let ff = g.ffn_dim as u128;
    let per_layer = 4 * d * d + 2 * ff * d + d * ff;
    2 * (g.vocab_size as u128) * d + (g.layer_count as u128) * per_layer
}

fn report(name: &str, g: PalwBase0GeometryV1, cases: &[(u32, u32)]) {
    let profile = base0_profile_v1(g).expect("the geometry is expressible");
    let reachable: std::collections::BTreeSet<Hash64> =
        [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes]
            .into_iter()
            .flatten()
            .map(|n| n.kernel_semantics_id)
            .collect();
    let covered = verify_catalog_coverage_v1(&PalwReachableKernelSetV1 {
        execution_class_id: profile.shape_profile_id(),
        kernel_ids: reachable.clone(),
    });

    println!("### {name}");
    println!(
        "- {} layers, d_model {}, d_ff {}, vocab {}, ctx {}",
        g.layer_count, g.hidden_dim, g.ffn_dim, g.vocab_size, g.n_ctx
    );
    println!("- int8 artifact: **{:.2} GB**", artifact_bytes(&g) as f64 / 1e9);
    println!(
        "- reachable kernels: {} — coverage against this build's adjudication table ({} entries): **{}**",
        reachable.len(),
        catalogued_kernel_ids_v1().len(),
        if covered.is_ok() { "PASS" } else { "FAIL" }
    );
    println!();
    println!("| prefill/decode | step leaves | bisection rounds |");
    println!("|---|---|---|");
    for (prefill, decode) in cases {
        match step_leaf_count(&profile, &job_context(&profile, *prefill, *decode)) {
            Ok(leaves) => println!("| {prefill}/{decode} | {leaves} | {} |", bisection_rounds(leaves)),
            Err(e) => println!("| {prefill}/{decode} | refused: {e:?} | — |"),
        }
    }
    println!();
}

/// The largest `n_ctx` at which a geometry is still ADJUDICABLE — `worst_case_step_leaf_count_v1`
/// is the whole context as prefill plus one decode, and a class that fails it is one an attacker
/// picks the job length for. Binary search, because the count is monotone in the context.
fn max_adjudicable_ctx(base: PalwBase0GeometryV1, tile_len: u32) -> u32 {
    let fits = |n_ctx: u32| -> bool {
        let g = PalwBase0GeometryV1 { n_ctx, tile_len, ..base };
        base0_profile_v1(g).ok().and_then(|p| worst_case_step_leaf_count_v1(&p).ok()).is_some()
    };
    if !fits(2) {
        return 0;
    }
    let (mut lo, mut hi) = (2u32, 1u32 << 20);
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if fits(mid) { lo = mid } else { hi = mid }
    }
    lo
}

fn main() {
    println!("# What a second BASE-0 class costs the ruleset\n");
    report("The RC liveness floor (`PALW_RC_BASE0_GEOMETRY`)", PALW_RC_BASE0_GEOMETRY, &[
        PALW_RC_BASE0_CANONICAL,
        PALW_RC_BASE0_WORST_CASE,
    ]);
    report("A Qwen-scale BASE-0 class (`QWEN_SCALE`)", QWEN_SCALE, QWEN_WORST_CASES);

    // The number the plan turns on: provisioning the court's ladder now for a class that does not
    // exist yet. `max_step_leaf_count` is a bundle field and the bundle is the ruleset id, so this
    // is the difference between "the second class joins" and "the second class is a flag day".
    let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor geometry is expressible");
    let big = base0_profile_v1(QWEN_SCALE).expect("the Qwen-scale geometry is expressible");
    let floor_worst = step_leaf_count(&floor, &job_context(&floor, PALW_RC_BASE0_WORST_CASE.0, PALW_RC_BASE0_WORST_CASE.1))
        .expect("the floor's worst case counts");
    println!("## The provisioning question\n");
    println!("| ladder provisioned for | max_step_leaf_count | rounds | extra rounds vs the floor |");
    println!("|---|---|---|---|");
    let floor_rounds = bisection_rounds(floor_worst);
    println!("| the floor alone | {floor_worst} | {floor_rounds} | — |");
    for (prefill, decode) in QWEN_WORST_CASES {
        if let Ok(leaves) = step_leaf_count(&big, &job_context(&big, *prefill, *decode)) {
            let rounds = bisection_rounds(leaves);
            println!("| Qwen scale at {prefill}/{decode} | {leaves} | {rounds} | **+{}** |", rounds - floor_rounds);
        }
    }
    println!();

    // The constraint the sweep above ran into. A class is admissible only if its LONGEST job — the
    // whole context as prefill — fits `PALW_STEP_MAX_LEAVES`, and `tile_len` is the only knob that
    // moves it. Larger tiles buy context and cost the court granularity: a dispute localises to a
    // tile, so a tile is how much arithmetic a single adjudication has to redo.
    println!("## `tile_len` buys context, and pays in court granularity\n");
    println!("(cap is `PALW_STEP_MAX_LEAVES` = {PALW_STEP_MAX_LEAVES})\n");
    println!("| tile_len | max adjudicable n_ctx — floor geometry | — Qwen scale |");
    println!("|---|---|---|");
    for tile_len in [64u32, 128, 256, 512, 1024, 2048, 4096] {
        println!(
            "| {tile_len} | {} | {} |",
            max_adjudicable_ctx(PALW_RC_BASE0_GEOMETRY, tile_len),
            max_adjudicable_ctx(QWEN_SCALE, tile_len),
        );
    }
}
