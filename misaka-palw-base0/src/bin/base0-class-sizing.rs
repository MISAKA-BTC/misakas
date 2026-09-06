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
use kaspa_consensus_core::palw_attn_court_v1::palw_attn_court_admits_row_v1;
use kaspa_consensus_core::palw_context_ladder::palw_close_assembly_daa_v1;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwCourtParamsV2};
use kaspa_consensus_core::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4;
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

    // ADR-0092 §5: the wall-clock budget, from the shipped bundle rather than from this file.
    let shipped = kaspa_consensus_core::config::params::palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &shipped.palw_consensus_mode else {
        panic!("the RC ships a ConsensusV2 bundle");
    };
    adr0092_wall_clock(bundle, &bundle.court, bundle.state.window_court());
}

/// **ADR-0092 §5 — the wall-clock question, generated.**
///
/// ADR-0092's finding is that the ladder is not what binds: `bisection_rounds` is
/// `ceil(log_arity(max_step_leaf_count))`, so one doubling of the provisioned space costs ONE
/// round, while every round costs `turn_deadline_daa` twice over and the total must fit
/// `window_court`. This section prints that budget rather than asserting it, and the predicate it
/// prints is the shipped one — [`palw_attn_court_admits_row_v1`] — so a ladder this table calls
/// admissible is one class admission admits.
///
/// Every figure here is a generated artifact. ADR-0092 §5 says so, and says why: a worked example
/// with no generator behind it is a number that drifts, and this repo has watched one drift twice.
fn adr0092_wall_clock(bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2, rc: &PalwCourtParamsV2, window_court: u64) {
    let reserve = palw_close_assembly_daa_v1(rc.max_close_chunks());
    println!("## ADR-0092 — what the court window buys\n");
    println!(
        "Fixed terms, read from the shipped RC bundle: turn deadline **{} DAA**, terminal rounds \
         **{}**, dissection arity **{}**, close chunks **{}**, close-assembly reserve **{reserve} DAA**, \
         court window **{window_court} DAA**.\n",
        rc.turn_deadline_daa(),
        rc.terminal_rounds(),
        rc.dissection_arity(),
        rc.max_close_chunks(),
    );
    println!(
        "`worst` is `(2 x (ladder rounds + history rounds) + terminal + root_claim) x turn_deadline` \
         (`PalwCourtParamsV2::worst_case_duration_with_history_daa`); a row is admissible when \
         `worst + reserve < window_court`.\n"
    );

    // **Decision 3's measurement, both sides of it.** The ADR refuses to assume that fewer rounds
    // is less cost, so the sweep prints every legal arity: the round count falls as `log_arity`,
    // and what one round CARRIES rises — `palw_attn_court_move_bytes_v1` is the other half and
    // belongs to the close-ceiling table. What this table settles is only the wall clock.
    let mut arities: Vec<u8> = vec![2, 4, 8, 16, 32, 64];
    if !arities.contains(&rc.dissection_arity()) {
        arities.push(rc.dissection_arity());
        arities.sort_unstable();
    }
    // History positions: zero (the ladder alone), the widths the shipped classes actually declare,
    // and two beyond them, so the table says where the shipped configuration sits AND where it
    // stops. 131,072 is the figure `palw_context_ladder`'s own test uses for a wide row.
    let histories: [u64; 5] = [0, 512, 4_096, 32_768, 131_072];

    // **Decision 3's answer is DERIVED, not chosen.** `palw_court_params_at_v2` computes the arity
    // from the widest registered site whenever the k-ary court is armed, and `palw_court_arity_v1`
    // is where both halves of the question meet: the clock (`moves x deadline + reserve <
    // window_court`, the SAME inequality admission applies) and the wire (a round must fit one
    // framed carrier at the widest lane count). `None` is a refusal, never a fallback to 2. So this
    // report prints what the ruleset derives rather than inviting anyone to pick a number.
    let (history_max, widest_lanes) = kaspa_consensus_core::palw_court_v2::palw_attn_widest_registered_site_v2(bundle);
    let derived = kaspa_consensus_core::palw_mode_v2::palw_court_arity_v1(
        window_court,
        rc.turn_deadline_daa(),
        rc.max_step_leaf_count(),
        history_max,
        PALW_ATTN_HISTORY_TILE_V4,
        rc.terminal_rounds(),
        widest_lanes,
        rc.max_close_chunks(),
    );
    println!("### The arity this ruleset derives\n");
    println!(
        "- widest registered site: **{history_max} history positions**, **{widest_lanes} lanes** \
         (`palw_attn_widest_registered_site_v2`)\n- derived dissection arity: **{}**\n- stored on the \
         bundle: **{}** (the value a court plays is the derived one whenever `palw_kary_court` is armed)\n",
        derived.map(|a| a.to_string()).unwrap_or_else(|| "**REFUSED** — no legal arity fits this window".into()),
        rc.dissection_arity(),
    );
    println!(
        "The leaf ladder is binary whatever the arity is (`PALW_COURT_LEAF_LADDER_ARITY_V1`), so the \
         arity buys HISTORY rounds only — which is why the zero-history row of the table below is the \
         same at every arity. An earlier derivation priced a k-ary leaf ladder no session plays and \
         reported 34 moves where the played dispute takes 60; that is audit D H-2 and this note is \
         what stops it being reintroduced from this report.\n"
    );

    println!("### The decision table — largest admissible `max_step_leaf_count`\n");
    println!("Rows are the history a dispute must dissect; columns are the court's arity. Each cell");
    println!("is the widest ladder whose worst-case prosecution still fits `window_court`.\n");
    print!("| history positions |");
    for a in &arities {
        print!(" arity {a}{} |", if *a == rc.dissection_arity() { " (shipped)" } else { "" });
    }
    println!();
    print!("|---|");
    for _ in &arities {
        print!("---|");
    }
    println!();
    for history in histories {
        print!("| {history} |");
        for arity in &arities {
            let widest = (18u32..=44)
                .filter_map(|exp| {
                    let ladder = 1u64 << exp;
                    let court = court_at(rc, ladder, *arity)?;
                    palw_attn_court_admits_row_v1(&court, history, PALW_ATTN_HISTORY_TILE_V4, window_court).ok().map(|_| exp)
                })
                .max();
            match widest {
                Some(exp) => print!(" 2^{exp} |"),
                None => print!(" **none** |"),
            }
        }
        println!();
    }
    println!();
    println!(
        "**Read this before choosing either number.** The shipped RC pairs arity {} with a ladder of \
         2^{} (`PALW_RC_COURT_MAX_STEP_LEAF_COUNT`). The row for zero history is what makes that pair \
         legal; a class whose dispute must also dissect a long history is bounded by its own row, and \
         at the shipped arity that bound falls well below the shipped ladder. ADR-0092 Decision 1 is \
         the choice of the ladder; Decision 3 is the choice of the arity; this table is the only place \
         they are priced against one another.\n",
        rc.dissection_arity(),
        kaspa_consensus_core::palw_fp_devnet_v3::COURT_MAX_STEP_LEAVES.trailing_zeros(),
    );
}

/// The RC's court at a different ladder and arity — every other ceiling kept, so the table varies
/// one thing at a time.
fn court_at(rc: &PalwCourtParamsV2, ladder: u64, arity: u8) -> Option<PalwCourtParamsV2> {
    PalwCourtParamsV2::with_cost_ceilings(
        ladder,
        rc.turn_deadline_daa(),
        rc.terminal_rounds(),
        rc.max_close_bytes(),
        rc.max_terminal_macs(),
        rc.max_operand_count(),
    )
    .ok()?
    .with_dissection_arity(arity)
    .ok()
}
