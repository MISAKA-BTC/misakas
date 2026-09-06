//! **What a second class is worth, measured on this host** — the report behind the weight
//! decision (road-to-mainnet Gate 3).
//!
//! A class's contribution to fork choice is `palw_pwu_v1(class_target, pwu_per_inference)` =
//! *expected attempts* × *pwu per inference*. The retarget moves the first factor until the class
//! produces its `share_permille` of the cadence, so the only thing a **registration** decides is
//! the second — and the share, which says how many blocks the class is allowed to carry it on.
//!
//! Three numbers therefore decide a class's weight, and every one of them is measurable before
//! anything is registered:
//!
//! 1. **can it be adjudicated at all** — a family whose disputes have no arithmetic terminal can
//!    never be convicted, so weight on it is unbacked (ADR-0039);
//! 2. **`pwu_per_inference`** — counted from the canonical job by the admission gate, never
//!    declared;
//! 3. **seconds per inference on the hardware that must run it** — a class whose inference costs
//!    more than its slice of the cadence cannot hold the share it was granted, and a class whose
//!    seats must re-run it pays that cost on every claim.
//!
//! Run it where the producers run:
//! `cargo run --release -p misaka-palw-base0 --example class-weight-report`
//!
//! `--reps N` sets the timing repetitions (default 3). `--skip-timing` prints the structural half
//! only, which is the half that does not depend on the host.

use std::time::Instant;

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_base0_profile::rc_job_context;
use kaspa_consensus_core::palw_catalog_coverage::{PalwReachableKernelSetV1, verify_catalog_coverage_v1};
use kaspa_consensus_core::palw_class_admission_v2::derive_court_cost_v1;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwCourtParamsV2};
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_profile_v1};
use kaspa_consensus_core::palw_step::{
    PALW_STEP_MAX_LEAVES, PALW_STEP_MAX_TILE_LEN, PalwShapeProfileV3, step_leaf_count_capped_v1, worst_case_step_leaf_count_capped_v1,
};
use kaspa_consensus_core::palw_step_refute::catalogued_kernel_ids_v1;
use kaspa_hashes::Hash64;
use misaka_palw_base0::artifact::Base0ArtifactV1;
use misaka_palw_base0::classes::{ArtifactSourceV1, CanonicalClassV1, canonical_classes_v1};
use misaka_palw_base0::produce::{base0_execute_for_attempt_v1, base0_rc_job_v1};

/// The cadence every ConsensusV2 network is frozen at (`validate_palw_v2`).
const CADENCE_SECS: u64 = 120;

fn short(h: &Hash64) -> String {
    let s = h.to_string();
    format!("{}…", &s[..12])
}

/// Which ceiling refuses `(tile, n_ctx)`, in the order the admission gate applies them.
fn refusal(g: PalwQwen25GeometryV1, court: &PalwCourtParamsV2) -> Option<&'static str> {
    let Ok(profile) = qwen25_profile_v1(g) else { return Some("not expressible") };
    // Counted at the LADDER THIS NETWORK FROZE, which is the number the admission gate applies.
    // It was the executor's `PALW_STEP_MAX_LEAVES` constant, so on a network whose court is wider
    // this column refused geometries the chain admits.
    match worst_case_step_leaf_count_capped_v1(&profile, court.max_step_leaf_count()) {
        Err(_) => return Some("no step space"),
        Ok(w) if w > court.max_step_leaf_count() => return Some("ladder (max_step_leaf_count)"),
        Ok(_) => {}
    }
    let Ok(cost) = derive_court_cost_v1(&profile) else { return Some("no court cost") };
    if cost.max_close_bytes > court.max_close_bytes() {
        return Some("max_close_bytes");
    }
    if cost.max_terminal_macs > court.max_terminal_macs() {
        return Some("max_terminal_macs");
    }
    if u64::from(cost.max_operand_count) > u64::from(court.max_operand_count()) {
        return Some("max_operand_count");
    }
    None
}

/// Widest `n_ctx` this tile admits, by the same binary search the admission helper uses.
fn widest_n_ctx(tile: u32, model: PalwQwen25GeometryV1, court: &PalwCourtParamsV2) -> Option<u32> {
    let fits = |n_ctx: u32| refusal(PalwQwen25GeometryV1 { n_ctx, tile_len: tile, ..model }, court).is_none();
    if !fits(2) {
        return None;
    }
    let (mut lo, mut hi) = (2u32, model.n_ctx.max(2) + 1);
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if fits(mid) { lo = mid } else { hi = mid }
    }
    Some(lo)
}

fn reachable_kernels(profile: &PalwShapeProfileV3) -> std::collections::BTreeSet<Hash64> {
    [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes]
        .into_iter()
        .flatten()
        .map(|n| n.kernel_semantics_id)
        .collect()
}

fn artifact_bytes(a: &Base0ArtifactV1) -> usize {
    let s = &a.shape;
    let d = s.d_model();
    // Weights only — the tables and the rope are rounding error beside them.
    let per_layer = d * d * 2 + d * s.n_kv_heads * s.d_head * 2 + d * s.d_ff * 3;
    s.vocab * d * 2 + s.n_layers * per_layer
}

struct Measured {
    label: String,
    class_id: Hash64,
    geometry: String,
    canonical_job: (u32, u32),
    pwu_per_inference: u64,
    worst_case_leaves: u64,
    coverage: bool,
    artifact_bytes: usize,
    /// `None` when timing was skipped or the class has no artifact this host can hold.
    secs_per_inference: Option<f64>,
}

fn time_integer_class(class: &CanonicalClassV1, reps: u32) -> Result<(f64, usize), String> {
    let seed = match class.source {
        ArtifactSourceV1::Derived(seed) => seed,
        // A converted class's weights are not a function of anything this host holds. Their VALUES
        // do not change what a forward pass costs — only the shape does — so the cost is measured
        // over a derived artifact of the class's own shape, and the report says so rather than
        // pretending the real checkpoint was loaded.
        ArtifactSourceV1::Converted | ArtifactSourceV1::ConvertedA16 => 0x5EED_0BA5_E000_0001,
    };
    let artifact = Base0ArtifactV1::derive_deterministic(class.artifact_shape, seed).map_err(|e| format!("{e:?}"))?;
    let bytes = artifact_bytes(&artifact);
    let (prefill, decode) = class.canonical_job;
    // `Flat` is the form every shipped preset runs (`PalwPromptIdsFormV1::Flat`); ADR-0081 D3's
    // Merkle form is genesis-only and no class this report walks is registered under it.
    let (job, prompt) = base0_rc_job_v1(
        &class.profile,
        Hash64::from_u64_word(7),
        artifact.shape.vocab,
        prefill,
        decode,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    );
    // One warm-up: the first pass faults in the pages the derivation just wrote.
    base0_execute_for_attempt_v1(&artifact, &class.profile, &job, &prompt).map_err(|e| format!("{e:?}"))?;
    let start = Instant::now();
    for _ in 0..reps {
        base0_execute_for_attempt_v1(&artifact, &class.profile, &job, &prompt).map_err(|e| format!("{e:?}"))?;
    }
    Ok((start.elapsed().as_secs_f64() / reps as f64, bytes))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let reps: u32 = args.iter().position(|a| a == "--reps").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(3);
    let timing = !args.iter().any(|a| a == "--skip-timing");

    let params: Params = NetworkId::with_suffix(NetworkType::Testnet, 11).into();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        eprintln!("this build's testnet-11 declares no ConsensusV2 ruleset — nothing to price");
        std::process::exit(2);
    };
    let court = bundle.court;

    println!("# The second class's weight, measured");
    println!();
    println!("host: {} {} | reps {reps}", std::env::consts::OS, std::env::consts::ARCH);
    println!("network: testnet-11 | base class {} | cadence {CADENCE_SECS}s", short(&bundle.base_class_id));
    println!();
    println!("## 1. The court this network shipped");
    println!();
    println!("| ceiling | value |");
    println!("| --- | ---: |");
    println!("| max_step_leaf_count (the ladder) | {} |", court.max_step_leaf_count());
    println!("| PALW_STEP_MAX_LEAVES (the EXECUTOR's constant, not a ceiling here) | {PALW_STEP_MAX_LEAVES} |");
    println!("| max_close_bytes | {} |", court.max_close_bytes());
    println!("| max_terminal_macs | {} |", court.max_terminal_macs());
    println!("| max_operand_count | {} |", court.max_operand_count());
    println!();
    println!("These are bundle fields, so they are inside `palw_ruleset_id_v2`: **a running chain");
    println!("cannot raise them**. Every geometry below is admissible or refused against THESE.");
    println!();

    println!("## 2. What the ceilings admit for Qwen2.5-1.5B");
    println!();
    println!("The model declares `n_ctx {} / tile {}`. What it may register at:", QWEN25_1_5B.n_ctx, QWEN25_1_5B.tile_len);
    println!();
    println!("| tile_len | widest n_ctx | refused at tile's own minimum by |");
    println!("| ---: | ---: | --- |");
    let mut tile = 64u32;
    let mut best: Option<(u32, u32)> = None;
    while tile <= PALW_STEP_MAX_TILE_LEN {
        match widest_n_ctx(tile, QWEN25_1_5B, &court) {
            Some(n) => {
                println!("| {tile} | {n} | — |");
                if best.map(|(_, bn)| n > bn).unwrap_or(true) {
                    best = Some((tile, n));
                }
            }
            None => {
                let why = refusal(PalwQwen25GeometryV1 { n_ctx: 2, tile_len: tile, ..QWEN25_1_5B }, &court)
                    .unwrap_or("(admissible, but the search says otherwise)");
                println!("| {tile} | — | {why} |");
            }
        }
        tile *= 2;
    }
    println!();

    let mut rows: Vec<Measured> = Vec::new();

    // --- the integer family: the floor, and every Qwen the court admits -------------------------
    for class in canonical_classes_v1(&court) {
        let canonical = rc_job_context(&class.profile, class.canonical_job.0, class.canonical_job.1);
        let pwu = step_leaf_count_capped_v1(&class.profile, &canonical, court.max_step_leaf_count()).unwrap_or(0);
        let worst = worst_case_step_leaf_count_capped_v1(&class.profile, court.max_step_leaf_count()).unwrap_or(0);
        let kernels = reachable_kernels(&class.profile);
        let coverage = verify_catalog_coverage_v1(&PalwReachableKernelSetV1 {
            execution_class_id: class.profile.shape_profile_id(),
            kernel_ids: kernels.clone(),
        })
        .is_ok();
        let (secs, bytes) = if timing {
            match time_integer_class(&class, reps) {
                Ok((s, b)) => (Some(s), b),
                Err(e) => {
                    eprintln!("timing {} failed: {e}", class.model_id);
                    (None, 0)
                }
            }
        } else {
            (None, 0)
        };
        rows.push(Measured {
            label: format!(
                "{}{}",
                class.model_id,
                match class.source {
                    ArtifactSourceV1::Derived(_) => " (derived)",
                    ArtifactSourceV1::Converted => " (converted)",
                    ArtifactSourceV1::ConvertedA16 => " (converted, A16)",
                }
            ),
            class_id: class.class_id(),
            geometry: format!("tile {} / n_ctx {}", class.inventory_geometry.tile_len, class.inventory_geometry.n_ctx),
            canonical_job: class.canonical_job,
            pwu_per_inference: pwu,
            worst_case_leaves: worst,
            coverage,
            artifact_bytes: bytes,
            secs_per_inference: secs,
        });
    }

    // **Family M's row is gone, and its absence is the measurement.** ADR-0053 withdrew the
    // second execution family and deleted the crate this block read its catalog from: a tolerance
    // can acquit but never convict, so half the economy would have been non-convictable work. What
    // replaced it is in the rows above — Qwen3.6 runs in the integer runtime with a complete kernel
    // catalog, so the model the black box existed to serve is priced here like every other class.

    println!("## 3. The classes, priced");
    println!();
    // **No family column, and no "court?" column.** ADR-0053 left exactly one execution family, so
    // both would print the same value on every row — and a constant column reads as a choice that
    // was made per class. Every registered class is court-adjudicable by construction now; what is
    // still worth a column is whether its COVERAGE gate passes, which is a fact about the class.
    println!("| class | geometry | canonical job | pwu/inference | worst-case leaves | coverage | weights | s/inference |");
    println!("| --- | --- | ---: | ---: | ---: | :-: | ---: | ---: |");
    for r in &rows {
        println!(
            "| {} `{}` | {} | {}+{} | {} | {} | {} | {} | {} |",
            r.label,
            short(&r.class_id),
            r.geometry,
            r.canonical_job.0,
            r.canonical_job.1,
            r.pwu_per_inference,
            if r.worst_case_leaves == 0 { "n/a".to_string() } else { r.worst_case_leaves.to_string() },
            if r.coverage { "PASS" } else { "GAP" },
            if r.artifact_bytes == 0 { "—".to_string() } else { format!("{:.2} GiB", r.artifact_bytes as f64 / (1 << 30) as f64) },
            r.secs_per_inference.map(|s| format!("{s:.3}")).unwrap_or_else(|| "—".to_string()),
        );
    }
    println!();

    println!("## 4. What a share is worth, and what it costs to hold");
    println!();
    println!("Weight per block is `expected_attempts × pwu_per_inference`, and a share of s‰ is");
    println!("s blocks out of every 1000-block epoch. At the frozen {CADENCE_SECS}s cadence an epoch is");
    println!(
        "{:.1} h, so a class with share s has one block every {} × 1000/s seconds.",
        1000.0 * CADENCE_SECS as f64 / 3600.0,
        CADENCE_SECS
    );
    println!();
    println!("| class | pwu/inference | weight per block at 1 expected attempt | s/inference | inferences per block-slot at 1‰ |");
    println!("| --- | ---: | ---: | ---: | ---: |");
    for r in &rows {
        let per_block = palw_pwu_v1(u128::MAX, r.pwu_per_inference);
        let slot_secs = CADENCE_SECS as f64 * 1000.0; // one block per epoch
        println!(
            "| {} | {} | {} | {} | {} |",
            r.label,
            r.pwu_per_inference,
            per_block,
            r.secs_per_inference.map(|s| format!("{s:.3}")).unwrap_or_else(|| "—".to_string()),
            r.secs_per_inference.map(|s| format!("{:.0}", slot_secs / s)).unwrap_or_else(|| "—".to_string()),
        );
    }
    println!();
    println!("The last column is the honest capacity question: a producer gets ONE inference per");
    println!("block template and then grinds nonces against it, so `slot ÷ s_per_inference` is how");
    println!("many independent tickets the class can afford between the blocks it is allowed to win.");
    println!();

    // --- what a USEFUL Qwen geometry would have cost at genesis ---------------------------------
    //
    // The table above says what this ruleset admits. This one says what a ruleset would have to
    // have declared to admit a context worth having — the numbers for the NEXT mint, since none of
    // them can move on a running chain.
    println!("## 5. The ceilings a useful context would have needed");
    println!();
    println!("Every value below is a **bundle** field. They are decided once, at the mint, and the");
    println!("class that wants them cannot be registered onto a chain that declared less.");
    println!();
    println!(
        "| tile_len | n_ctx | worst-case leaves (`≥` = the count stopped at {}, the widest ladder any ruleset may freeze) | max_close_bytes | max_terminal_macs | operands | vs shipped |",
        kaspa_consensus_core::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES
    );
    println!("| ---: | ---: | ---: | ---: | ---: | ---: | --- |");
    for (tile, n_ctx) in [(64u32, 90u32), (64, 512), (128, 512), (512, 2048), (2048, 2048), (2048, 4096)] {
        let g = PalwQwen25GeometryV1 { n_ctx, tile_len: tile, ..QWEN25_1_5B };
        let Ok(profile) = qwen25_profile_v1(g) else {
            println!("| {tile} | {n_ctx} | — | — | — | — | not expressible |");
            continue;
        };
        // **Never `unwrap_or(0)` here.** The counter reports the running total it refused ON, and
        // a zero in this column would read as "costs nothing" for exactly the geometries that cost
        // too much — the first draft of this table printed 0 for four of six rows.
        //
        // The refused number is a LOWER BOUND, not the geometry's real worst case: the walk stops
        // the moment the running total crosses the cap, which is why two different contexts can
        // report the same figure. It answers "does this fit", never "by how much does it miss".
        // **Counted at the STRUCTURAL top, not at any shipped ladder.** This table answers "what
        // would a ruleset have had to declare", so truncating it at what some ruleset did declare
        // is the question begging its own answer — and truncating it at the EXECUTOR's constant,
        // which is what it did, answered a ruleset question with a code constant.
        let (leaves, leaves_over_code_cap) = match worst_case_step_leaf_count_capped_v1(
            &profile,
            kaspa_consensus_core::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
        ) {
            Ok(n) => (n, false),
            Err(kaspa_consensus_core::palw_step::PalwStepError::TooManyLeaves { got, .. }) => (got, true),
            Err(_) => (0, false),
        };
        let Ok(cost) = derive_court_cost_v1(&profile) else {
            println!("| {tile} | {n_ctx} | {leaves} | — | — | — | no court cost |");
            continue;
        };
        let over = |need: u64, have: u64| -> String {
            if need <= have { "ok".to_string() } else { format!("{:.0}×", need as f64 / have as f64) }
        };
        println!(
            "| {tile} | {n_ctx} | {}{} | {} | {} | {} | ladder {} · open {} · macs {} · operands {} |",
            if leaves_over_code_cap { "≥" } else { "" },
            leaves,
            cost.max_close_bytes,
            cost.max_terminal_macs,
            cost.max_operand_count,
            if leaves_over_code_cap { "OVER any ladder".to_string() } else { over(leaves, court.max_step_leaf_count()) },
            over(cost.max_close_bytes, court.max_close_bytes()),
            over(cost.max_terminal_macs, court.max_terminal_macs()),
            over(u64::from(cost.max_operand_count), u64::from(court.max_operand_count())),
        );
    }
    println!();
    println!("adjudication table: {} kernels", catalogued_kernel_ids_v1().len());
}
