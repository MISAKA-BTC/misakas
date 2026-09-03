//! **What a second class costs the ruleset** — the numbers behind "add Qwen scale later".
//!
//! Two facts the tree already fixes, measured rather than assumed:
//!
//! * a class's ladder depth is `PalwCourtParamsV2::max_step_leaf_count`, a **bundle** field, and
//!   the bundle is what `palw_ruleset_id_v2` hashes. A class deeper than the ladder cannot join a
//!   running chain at all — it needs a new ruleset, which is a flag day;
//! * a class is admissible only if its **longest** job — the whole context as prefill, which is
//!   what `worst_case_step_leaf_count_capped_v1` counts — fits THE RULESET'S LADDER. Checking the
//!   typical job instead would admit a class an attacker picks the job length for.
//!
//! The ladder is `--ladder <n>`, defaulting to the RC ruleset's `COURT_MAX_STEP_LEAVES` (2^26) and
//! never to the executor's `PALW_STEP_MAX_LEAVES` (2^22): counting an admission question at the
//! executor's constant printed `INADMISSIBLE` for classes the chain admits, and every row below
//! now says which ladder it was counted at.
//!
//! The Qwen geometries are the MEASURED ones from `palw_qwen25_profile`, not a sketch: the second
//! class's graph is that module's and this binary only prices it.

use kaspa_consensus_core::palw_base0_profile::{
    PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, PALW_RC_BASE0_WORST_CASE, PalwBase0GeometryV1, base0_profile_v1,
};
use kaspa_consensus_core::palw_catalog_coverage::{PalwReachableKernelSetV1, verify_catalog_coverage_v1};
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, QWEN25_3B, qwen25_profile_v1};
use kaspa_consensus_core::palw_step::{
    PALW_STEP_MAX_LEAVES, PalwShapeProfileV3, step_leaf_count_capped_v1, worst_case_step_leaf_count_capped_v1,
};
use kaspa_consensus_core::palw_step_refute::catalogued_kernel_ids_v1;
use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, trace_scheme_id_v2};
use kaspa_hashes::Hash64;

/// The ladder every count in this report is taken against — `--ladder <n>`, else the RC ruleset's.
fn ladder() -> u64 {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let v = if a == "--ladder" { args.next() } else { a.strip_prefix("--ladder=").map(str::to_string) };
        if let Some(v) = v {
            return v.parse().unwrap_or_else(|e| panic!("--ladder: {e}"));
        }
    }
    kaspa_consensus_core::palw_fp_devnet_v3::COURT_MAX_STEP_LEAVES
}

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

/// `ceil(log2(n))` for `n >= 2`, by the `next_power_of_two` identity `PalwCourtParamsV2` uses.
fn bisection_rounds(leaves: u64) -> u32 {
    leaves.max(2).next_power_of_two().trailing_zeros()
}

fn report(name: &str, profile: &PalwShapeProfileV3, jobs: &[(u32, u32)]) {
    let kernel_ids: std::collections::BTreeSet<Hash64> =
        [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes]
            .into_iter()
            .flatten()
            .map(|n| n.kernel_semantics_id)
            .collect();
    let covered = verify_catalog_coverage_v1(&PalwReachableKernelSetV1 {
        execution_class_id: profile.shape_profile_id(),
        kernel_ids: kernel_ids.clone(),
    });

    println!("### {name}");
    println!(
        "- reachable kernels {} — coverage against this build's adjudication table ({}): **{}**",
        kernel_ids.len(),
        catalogued_kernel_ids_v1().len(),
        if covered.is_ok() { "PASS" } else { "FAIL" }
    );
    match worst_case_step_leaf_count_capped_v1(profile, ladder()) {
        Ok(w) => println!(
            "- longest job (whole context as prefill — a what-if over the GEOMETRY, not a registered class): **{w}** leaves, {} rounds — ADMISSIBLE at ladder {}",
            bisection_rounds(w),
            ladder()
        ),
        Err(e) => println!(
            "- longest job (a what-if over the GEOMETRY, not a registered class): **INADMISSIBLE at ladder {}** — {e:?}",
            ladder()
        ),
    }
    println!();
    println!("| job (prefill/decode) | step leaves | rounds |");
    println!("|---|---|---|");
    for (prefill, decode) in jobs {
        match step_leaf_count_capped_v1(profile, &job_context(profile, *prefill, *decode), ladder()) {
            Ok(leaves) => println!("| {prefill}/{decode} | {leaves} | {} |", bisection_rounds(leaves)),
            Err(e) => println!("| {prefill}/{decode} | refused: {e:?} | — |"),
        }
    }
    println!();
}

/// The largest `n_ctx` at which a geometry is still admissible, by binary search on the monotone
/// whole-context count.
fn max_adjudicable_ctx(build: &dyn Fn(u32, u32) -> Option<PalwShapeProfileV3>, tile_len: u32) -> u32 {
    let fits = |n_ctx: u32| build(n_ctx, tile_len).and_then(|p| worst_case_step_leaf_count_capped_v1(&p, ladder()).ok()).is_some();
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
    println!("# What a second class costs the ruleset\n");
    println!(
        "Every count below is taken at ladder **{}** (executor constant {PALW_STEP_MAX_LEAVES}, RC ruleset {}).\n",
        ladder(),
        kaspa_consensus_core::palw_fp_devnet_v3::COURT_MAX_STEP_LEAVES
    );

    let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor geometry is expressible");
    report("PALW-BASE-0, the RC liveness floor", &floor, &[PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_WORST_CASE]);

    // Synthetic provisioning jobs (prefill/decode) over the full geometry — what-ifs for the
    // ruleset, NOT catalog rows: the registered A16 rows are at n_ctx 16/18/512 and every one
    // fits the RC ladder (measured 2026-09-04: v2/v3@512 59,000,848, v5@512 52,778,128).
    let jobs = [(64u32, 64u32), (512, 128), (2048, 512)];
    let q15 = qwen25_profile_v1(QWEN25_1_5B).expect("the measured 1.5B geometry is expressible");
    report("Qwen2.5-1.5B (measured, as shipped in `palw_qwen25_profile`)", &q15, &jobs);
    let q3 = qwen25_profile_v1(QWEN25_3B).expect("the measured 3B geometry is expressible");
    report("Qwen2.5-3B (measured)", &q3, &jobs);

    // The decision that expires at genesis: `max_step_leaf_count` is inside the ruleset id, so the
    // ladder a network freezes is the deepest class it can ever admit.
    println!("## The provisioning question\n");
    println!("| ladder provisioned for | max_step_leaf_count | rounds |");
    println!("|---|---|---|");
    let floor_worst = worst_case_step_leaf_count_capped_v1(&floor, ladder()).expect("the floor is admissible");
    println!("| the floor alone | {floor_worst} | {} |", bisection_rounds(floor_worst));
    println!("| the executor's constant | {PALW_STEP_MAX_LEAVES} | {} |", bisection_rounds(PALW_STEP_MAX_LEAVES));
    println!("| this report's ladder | {} | {} |", ladder(), bisection_rounds(ladder()));
    println!();

    // ---------------------------------------------------------------------------------------
    // **The A16 tier at the widths the launch actually needs.**
    //
    // Everything above measures `qwen25_profile_v1`, and the class that SHIPS is the A16
    // projection — `qwen25_a16_profile_v2`, what `palw_a16_context_row_profile_v1` carries and
    // what the registered `.palwart` produces. They are different profiles with different leaf
    // costs, so quoting the plain row's numbers at the A16 row is how "176 positions" and "39
    // positions" both get to be true measurements of different things. Measured here so nobody
    // has to reconcile them from two tools again.
    //
    // The cap column is the point: `PALW_STEP_MAX_LEAVES` is a CONSTANT the executor hardcodes
    // today, and ADR-0080 W1b makes it a ruleset field. Once it is a field, the question stops
    // being "does the class fit" and becomes "what value admits the answers we promised" — so
    // this prints the widths several candidate caps buy, beside the grammar floors that have to
    // fit inside them.
    println!("## The A16 tier: what each ruleset leaf cap buys\n");
    println!("(floors from `misaka-palw-derive/tests/grammar_floor.rs`: cad 38, music 60, scene 104 decode tokens)\n");
    println!("| ruleset leaf cap | widest admissible n_ctx (A16) |");
    println!("|---|---|");
    for shift in [22u32, 23, 24, 26, 28, 32] {
        let cap = 1u64 << shift;
        let fits = |n_ctx: u32| {
            kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v1(n_ctx)
                .ok()
                .and_then(|prof| kaspa_consensus_core::palw_step::worst_case_step_leaf_count_capped_v1(&prof, cap).ok())
                .is_some()
        };
        let mut lo = 0u32;
        let mut hi = 16_384u32;
        if !fits(2) {
            println!("| 2^{shift} | 0 — not even a two-position job |");
            continue;
        }
        while lo + 1 < hi {
            let mid = lo + (hi - lo) / 2;
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        println!("| 2^{shift} | {lo} |");
    }
    println!();

    // `tile_len` is the only knob that moves a class's longest job, and it buys context in exchange
    // for court granularity: a dispute localises to a tile, so a tile is how much arithmetic one
    // terminal adjudication has to redo.
    println!("## `tile_len` buys context, and pays in court granularity\n");
    println!("(the cap is `PALW_STEP_MAX_LEAVES` = {PALW_STEP_MAX_LEAVES}; shipped `tile_len` is 64 for the floor, 128 for Qwen)\n");
    println!("| tile_len | floor | Qwen2.5-1.5B | Qwen2.5-3B |");
    println!("|---|---|---|---|");
    let floor_build =
        |n_ctx: u32, tile_len: u32| base0_profile_v1(PalwBase0GeometryV1 { n_ctx, tile_len, ..PALW_RC_BASE0_GEOMETRY }).ok();
    let q15_build = |n_ctx: u32, tile_len: u32| qwen25_profile_v1(PalwQwen25GeometryV1 { n_ctx, tile_len, ..QWEN25_1_5B }).ok();
    let q3_build = |n_ctx: u32, tile_len: u32| qwen25_profile_v1(PalwQwen25GeometryV1 { n_ctx, tile_len, ..QWEN25_3B }).ok();
    for tile_len in [64u32, 128, 256, 512, 1024, 2048, 4096, 8192, 16_384, 32_768, 65_536] {
        println!(
            "| {tile_len} | {} | {} | {} |",
            max_adjudicable_ctx(&floor_build, tile_len),
            max_adjudicable_ctx(&q15_build, tile_len),
            max_adjudicable_ctx(&q3_build, tile_len),
        );
    }
    println!();

    // The question the table above is really being asked: the shipped geometries DECLARE n_ctx
    // 4096 at tile_len 128, and at that tile they are not admissible at any context worth having.
    println!("## The smallest `tile_len` that admits each class at its own declared `n_ctx`\n");
    println!("| class | declared n_ctx | shipped tile_len | smallest admitting tile_len |");
    println!("|---|---|---|---|");
    for (name, ctx, shipped, build) in [
        ("Qwen2.5-1.5B", QWEN25_1_5B.n_ctx, QWEN25_1_5B.tile_len, &q15_build as &dyn Fn(u32, u32) -> Option<PalwShapeProfileV3>),
        ("Qwen2.5-3B", QWEN25_3B.n_ctx, QWEN25_3B.tile_len, &q3_build),
    ] {
        let found = [128u32, 256, 512, 1024, 2048, 4096, 8192, 16_384, 32_768, 65_536]
            .into_iter()
            .find(|tile| build(ctx, *tile).and_then(|p| worst_case_step_leaf_count_capped_v1(&p, ladder()).ok()).is_some());
        println!(
            "| {name} | {ctx} | {shipped} | {} |",
            found.map(|t| t.to_string()).unwrap_or_else(|| "**none up to MAX_TILE_LEN**".into())
        );
    }
}
