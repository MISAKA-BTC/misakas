//! **`palw-seat-coverage` — ADR-0098's generator: the panel's coverage of one claim, measured.**
//!
//! Prints, from the draw the seats run (`palw_fp_interval_draw_v1`) and the closed form beside it
//! (`kaspa_consensus_core::palw_seat_coverage_v1`), the chance a free-prompt claim carrying one
//! forged leaf is caught by its panel's sampling — the number ADR-0081 Decision 8 asked for and
//! never took — then what a higher number costs a seat, how the capture arm's leaf sample compares,
//! and what a panel of seats that each hold a SHARD of the model costs to license.
//!
//! Every figure is a generated artifact (ADR-0092 §5, kept by ADR-0097 and ADR-0098): a reader who
//! needs a value runs this.
//!
//! ```text
//! palw-seat-coverage [--trials <n>]
//! ```

use kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5;
use kaspa_consensus_core::palw_fp_devnet_v3::{PALW_V2_PANEL_QUORUM, PALW_V2_PANEL_SEATS};
use kaspa_consensus_core::palw_mode_v2::PALW_STANDARD_TX_BYTES;
use kaspa_consensus_core::palw_model_fit_v1::stand_ins::KIMI_K3_TOTAL_PARAMETERS;
use kaspa_consensus_core::palw_seat_coverage_v1::{
    PPM, palw_draws_for_detection_v1, palw_seat_coverage_measured_v1, palw_seat_detection_ppm_v1,
    palw_seat_state_or_row_detection_ppm_v1, palw_sharded_panel_v1, palw_shipped_draws_per_seat_v1,
    palw_widest_shard_count_in_one_transaction_v1,
};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, trace_scheme_id_v2};
use kaspa_hashes::Hash64;

fn trials() -> u32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => 5_000,
        [flag, value] if flag == "--trials" => value.parse().unwrap_or_else(|e| panic!("--trials {value:?}: {e}")),
        other => panic!("unknown arguments {other:?}\nusage: palw-seat-coverage [--trials <n>]"),
    }
}

fn pct(ppm: u64) -> String {
    format!("{:.2} %", ppm as f64 / 10_000.0)
}

/// Forged claims a producer can file before the chance that at least one is caught reaches 90 %:
/// the smallest `m` with `1 − (1 − p)^m ≥ 0.9`.
fn claims_before_ninety(ppm: u64) -> String {
    if ppm >= PPM {
        return "1".into();
    }
    if ppm == 0 {
        return "never".into();
    }
    let p = ppm as f64 / PPM as f64;
    format!("{}", ((0.1f64).ln() / (1.0 - p).ln()).ceil() as u64)
}

fn job_context(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
    PalwJobContextV2 {
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
        trace_scheme_id: trace_scheme_id_v2(),
        cu_ruleset_id: Hash64::default(),
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: Hash64::default(),
        declared_prefill_tokens: prefill,
        exact_decode_tokens: decode,
        max_context_tokens: profile.n_ctx,
    }
}

fn main() {
    let trials = trials();
    let k = palw_shipped_draws_per_seat_v1();
    let (panel, quorum) = (u32::from(PALW_V2_PANEL_SEATS), u32::from(PALW_V2_PANEL_QUORUM));
    let network = Hash64::from_u64_word(0x0098);
    let claim = Hash64::from_u64_word(0xc1a1);

    println!("# ADR-0098 — the panel's coverage of one claim\n");
    println!(
        "Shipped: a seat replays **k = {k}** intervals (`PALW_FP_SEAT_INTERVAL_SAMPLES_V1`); the panel is **{panel}** seats with a \
         quorum of **{quorum}** (`PALW_V2_PANEL_SEATS` / `PALW_V2_PANEL_QUORUM`, both presets). A licensing object is carried as soon as \
         the quorum's `Valid` receipts pool and a seat's duty ends with it, so between **{quorum}** and **{panel}** seats replay a claim \
         before it is licensed — both are tabled. Interval count: the dense A16 and BASE-0 families checkpoint every decode call, so a \
         claim of D decode tokens has **N = max(1, D − 1)** intervals; the hybrid checkpoints at `n_ctx`, so its claims have **N = 1** \
         and every seat replays the whole job. Measured over **{trials}** beacons per row, with the draw the seats run.\n"
    );

    // -----------------------------------------------------------------------------------------
    println!("## 1. The forger's best lie — one token, caught only by replaying its own interval\n");
    println!(
        "A lie the cache never records (the selected token, or the last layer's work after its K/V rows) is found only by a seat"
    );
    println!(
        "whose draw includes its interval. `closed` is `1 − ((N − k)/N)^s`; `measured` is the mean over intervals and, after the"
    );
    println!(
        "slash, the least-drawn interval (the forger's best place). The last column is how many such forged claims a producer files"
    );
    println!("before the chance that one is caught reaches 90 %.\n");
    println!(
        "| D (decode tokens) | N | s = {quorum}: closed | measured mean / min | s = {panel}: closed | measured mean / min | claims before 90 % (s = {panel}) |"
    );
    println!("|---|---|---|---|---|---|---|");
    for d in [2u32, 3, 4, 5, 6, 9, 17, 33, 65, 129, 256, 300, 512, 1024] {
        let n = d.saturating_sub(1).max(1);
        let mut cells = Vec::new();
        for s in [quorum, panel] {
            let closed = palw_seat_detection_ppm_v1(n, k, s);
            let measured = palw_seat_coverage_measured_v1(&network, &claim, n, k, s as u8, trials);
            cells.push(format!("{} | {} / {}", pct(closed), pct(measured.mean_ppm()), pct(measured.min_ppm())));
        }
        println!("| {d} | {n} | {} | {} | {} |", cells[0], cells[1], claims_before_ninety(palw_seat_detection_ppm_v1(n, k, panel)));
    }
    let p = palw_seat_detection_ppm_v1(299, k, panel) as f64 / PPM as f64;
    println!(
        "\nD = 300 is the claim testnet-11 licensed on 2026-09-05 (N = 299; each of its three seats replayed four intervals); D = 512 is the \
         dense row's whole window; D = 1024 is the free-prompt decode ceiling. The measured minimum is the least-drawn of N intervals over \
         {trials} beacons, so it sits below the closed form by the binomial spread — one standard deviation is {:.2} points at N = 299, \
         s = {panel} — and not by a bias: `palw_seat_coverage_v1`'s test holds every interval within six deviations of the closed form.\n",
        100.0 * (p * (1.0 - p) / f64::from(trials)).sqrt()
    );

    // -----------------------------------------------------------------------------------------
    let n299 = 299u32;
    println!("## 2. A lie the cache records — caught by ANY later draw, through the state root\n");
    println!(
        "Before replaying interval j a seat recomputes the cache up to j and compares its root with the committed one, so a lie that alters \
         the cache is caught by a seat whose draw includes its interval or any later one: `1 − (C(i, k) / C(N, k))^s`. At N = {n299}, s = {panel}:\n"
    );
    println!("| interval i | closed | measured |");
    println!("|---|---|---|");
    let measured = palw_seat_coverage_measured_v1(&network, &claim, n299, k, panel as u8, trials);
    for i in [0u32, 74, 149, 224, 269, 289, 298] {
        println!(
            "| {i} | {} | {} |",
            pct(palw_seat_state_or_row_detection_ppm_v1(n299, k, panel, i)),
            pct(measured.state_or_row_ppm(i))
        );
    }
    println!(
        "\nThe forger's best cache-altering lie is therefore at the last interval, where the number is section 1's. **Today this path is \
         discarded**: a seat whose recomputed root differs files nothing and records nothing (ADR-0098 §1.3, fixed by Decision 2).\n"
    );

    // -----------------------------------------------------------------------------------------
    println!("## 3. What a higher number costs — intervals per seat\n");
    println!(
        "The smallest k at which the panel reaches the target for the forger's best lie. A seat's work is k interval replays plus"
    );
    println!("the recompute to its latest draw, so k near N is re-running the job.\n");
    println!("| N | s | 50 % | 90 % | 99 % |");
    println!("|---|---|---|---|---|");
    for n in [299u32, 511, 1023] {
        for s in [quorum, panel] {
            let at = |target: u64| palw_draws_for_detection_v1(n, s, target).map(|k| k.to_string()).unwrap_or_else(|| "—".into());
            println!("| {n} | {s} | {} | {} | {} |", at(500_000), at(900_000), at(990_000));
        }
    }
    println!();

    // -----------------------------------------------------------------------------------------
    println!("## 4. The capture arm's leaf sample, for comparison\n");
    println!("A seat checking a served capture opens leaf 0 and then three leaves drawn uniformly (`fp_capture_samples_clear`). Its");
    println!("chance of landing on one forged leaf, for claims of the dense graph-v5 row at their real leaf counts:\n");
    println!("| job (prompt / decode) | step leaves | one seat (3 draws) | five seats (15 draws) |");
    println!("|---|---|---|---|");
    let profile = palw_a16_context_row_profile_v5(512).expect("the dense row builds");
    let ladder = kaspa_consensus_core::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT;
    for (prompt, decode) in [(8u32, 8u32), (51, 64), (51, 300), (200, 300)] {
        match step_leaf_count_capped_v1(&profile, &job_context(&profile, prompt, decode), ladder) {
            // Far below one ppm, so as odds: `1 − (1 − 1/L)^d ≈ d / L`.
            Ok(leaves) => println!("| {prompt} / {decode} | {leaves} | 1 in {} | 1 in {} |", leaves.div_ceil(3), leaves.div_ceil(15)),
            Err(e) => println!("| {prompt} / {decode} | refused: {e:?} | — | — |"),
        }
    }
    println!(
        "\nThe arm runs only for a capture that fits the 16 MiB material cap. Today a seat whose interval openings have not arrived yet \
         reaches it in the same round, so for such a claim this — not section 1 — can be the seat's whole check (§1.3, named in Decision 6).\n"
    );

    // -----------------------------------------------------------------------------------------
    println!("## 5. Seats that hold a shard — the stratified panel\n");
    println!(
        "Every seat holds one shard and is drawn for it; a claim is licensed when every shard has its own quorum of {quorum}. A one-leaf lie \
         lives in one shard and only that shard's seats can replay it, so the coverage is section 1's at s = seats per shard, whatever the \
         shard count. The per-seat artifact is the Kimi K3 stand-in's parameter count over the shards, at one byte a weight (a floor).\n"
    );
    println!(
        "| shards | seat's artifact (≥) | panel seats (5 / shard) | licensing receipts | licensing bytes | one standard transaction ({PALW_STANDARD_TX_BYTES} B)? | one-lie coverage, N = 299 |"
    );
    println!("|---|---|---|---|---|---|---|");
    for shards in [1u32, 2, 4, 8, 9, 16, 32, 64] {
        let row = palw_sharded_panel_v1(shards, panel, quorum, n299, k);
        let per_seat = KIMI_K3_TOTAL_PARAMETERS / u64::from(shards);
        println!(
            "| {shards} | {:.0} GiB | {} | {} | {} | {} | {} |",
            per_seat as f64 / (1u64 << 30) as f64,
            row.panel_seats,
            row.licensing_receipts,
            row.licensing_bytes,
            if row.fits_one_standard_transaction { "yes" } else { "**no**" },
            pct(row.detection_ppm)
        );
    }
    println!(
        "\nThe widest shard count whose per-shard quorum licenses in one standard transaction is **{}**.\n",
        palw_widest_shard_count_in_one_transaction_v1(quorum)
    );

    // -----------------------------------------------------------------------------------------
    println!("## 6. The expected cost of a lie\n");
    println!(
        "A conviction slashes exactly the claim's reservation (`void_and_slash`), so the producer's expected cost of one forged leaf, when \
         detection leads to a conviction, is the section 1 number times `claim.reserved`: **{}** of it on the 300-token claim with five \
         replaying seats, **{}** with three. The chain deters a false answer worth less than that; a false answer worth more is deterred \
         only by the challenge window's watchdog, which re-runs every licensed claim and needs the whole model to do it.",
        pct(palw_seat_detection_ppm_v1(n299, k, panel)),
        pct(palw_seat_detection_ppm_v1(n299, k, quorum)),
    );
    held_coverage_at_2m_v1(k, panel, quorum);
}

/// **ADR-0103 Decision 9: the deterrent at 2M, priced before it is armed** — ADR-0098 Decision 1's
/// inverse table over the held unit. Under the held regime a claim's intervals are runs of `P`
/// positions over the WHOLE job (`N = ⌈(prefill + decode calls) / P⌉`), so a one-token lie
/// anywhere in a 2M prompt is caught by a seat whose draw includes its interval: `1 − ((N − k)/N)^s`,
/// which falls as `s·k / N`. The table prints the shipped `k` at the held `N`, and the `k` and the
/// `s` a panel would need for 50 / 90 / 99 %. This ADR chooses no number; the card's author reads it.
fn held_coverage_at_2m_v1(k: u32, panel: u32, quorum: u32) {
    use kaspa_consensus_core::palw_held_context_v1::palw_held_interval_positions_v1;
    use kaspa_consensus_core::palw_model_fit_v1::stand_ins::KIMI_K3_AS_HYBRID_V1;
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7};
    use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_profile_v7};
    use kaspa_consensus_core::palw_seat_coverage_v1::palw_seats_for_detection_v1;
    println!("\n\n## 7. The deterrent at 2M under the held regime (ADR-0103 Decision 9)\n");
    println!(
        "The held unit (Decision 2) is `P` positions over the whole job, prefill included, so a 2M-position claim of D decode tokens has \
         **N = ⌈(2,097,152 + D − 1) / P⌉** intervals and a one-token lie in the prompt is caught only by a seat whose draw includes its \
         interval. `P` is the class's (`palw_held_interval_positions_v1`: the wire's, or the draw's). Nothing here arms anything: it is \
         the table a network that mints with the fence reads before it chooses `k` and `s`.\n"
    );
    let rows: Vec<(&str, Option<u32>)> = vec![
        (
            "Qwen2.5-1.5B A16 graph-v7 (dense, held)",
            qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 1 << 21, ..QWEN25_1_5B })
                .ok()
                .map(|p| palw_held_interval_positions_v1(&p)),
        ),
        (
            "Kimi K3 stand-in as graph-v7 (NOT a class)",
            qwen36_profile_v7(PalwQwen36GeometryV1 { n_ctx: 1 << 21, ..KIMI_K3_AS_HYBRID_V1 }).ok().map(|p| palw_held_interval_positions_v1(&p)),
        ),
    ];
    println!("| row | P | N (D = 1,024) | shipped k = {k}, s = {quorum} | s = {panel} | k for 50 / 90 / 99 % at s = {panel} | s for 90 % at k = {k} |");
    println!("|---|---|---|---|---|---|---|");
    for (name, width) in rows {
        let Some(width) = width else {
            println!("| {name} | — | the held row does not build | | | | |");
            continue;
        };
        let steps = (1u64 << 21) + 1_023;
        let n = u32::try_from(steps.div_ceil(u64::from(width))).unwrap_or(u32::MAX);
        let ks = [500_000u64, 900_000, 990_000]
            .iter()
            .map(|t| palw_draws_for_detection_v1(n, panel, *t).map(|k| k.to_string()).unwrap_or_else(|| "—".into()))
            .collect::<Vec<_>>()
            .join(" / ");
        let seats = palw_seats_for_detection_v1(n, k, 900_000, 1_000_000).map(|s| s.to_string()).unwrap_or_else(|| "> 10^6".into());
        println!(
            "| {name} | {width} | {n} | {} | {} | {ks} | {seats} |",
            pct(palw_seat_detection_ppm_v1(n, k, quorum)),
            pct(palw_seat_detection_ppm_v1(n, k, panel)),
        );
    }
    println!(
        "\nAt a fixed `k` the catch vanishes with the prompt (`s·k / N`): either `k` or `s` scales with `N` — `s × k × P` positions is the \
         panel's whole replay, linear in the context and spread over seats, each bounded by its window — or the bonded watchdog, certain \
         at one inference a claim, is the deterrent and its cost at 2M is that inference's (ADR-0103 Decision 9; ADR-0098 Decision 4).\n"
    );
}
