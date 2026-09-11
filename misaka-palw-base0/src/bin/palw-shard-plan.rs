//! **`palw-shard-plan` — ADR-0099's generator: what a model costs a seat that holds a shard of
//! it, and the Measured Model Artifact an adder hands the network.**
//!
//! For the dense row, the hybrid row and the Kimi K3 stand-in (ADR-0097 §1.3) — and for any
//! manifest handed in — prints the artifact bytes per layer, the plans at each shard count with
//! the widest seat and the two transfer forms a shard seat has, the fewest shards a seat budget
//! allows, the stratified panel's licensing cost (ADR-0098), the leaves each shard commits per
//! position, and the 1M-position arithmetic past the geometry ceiling. With `--measured-out` it
//! writes each candidate's Measured Model Artifact; with `--verify` it recomputes one and says,
//! field by field, whether it agrees.
//!
//! Every figure is a generated artifact (ADR-0092 §5): a reader who needs a value runs this.
//!
//! ```text
//! palw-shard-plan [--manifest <model.json>] [--seat-gib <n>,<n>,…]
//!                 [--replay-ms <ms per position>] [--measured-on <host>]
//!                 [--measured-out <dir>] [--verify <measured.json>]
//! ```
//!
//! A manifest is `PalwModelManifestV1` as JSON — the family and the geometry's public numbers,
//! plus an optional `total_parameters` a card states:
//!
//! ```json
//! { "name": "…", "family": "hybrid-qwen36", "layer_count": 92, "full_attention_interval": 4,
//!   "hidden_dim": 7168, "vocab_size": 163840, "attn_heads": 96, "attn_kv_heads": 6,
//!   "attn_head_dim": 128, "rope_dims": 64, "gdn_k_heads": 56, "gdn_v_heads": 56,
//!   "gdn_head_dim": 128, "gdn_conv_kernel": 4, "n_experts": 896, "experts_per_token": 16,
//!   "moe_dim": 3072, "shared_dim": 6144, "attn_output_gate": 1, "total_parameters": 2800000000000 }
//! ```

use kaspa_consensus_core::config::params::{Params, palw_rc_shipped_params};
use kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1;
use kaspa_consensus_core::palw_fp_devnet_v3::{PALW_V2_PANEL_QUORUM, PALW_V2_PANEL_SEATS};
use kaspa_consensus_core::palw_measured_model_v1::{
    PalwMeasureInputsV1, PalwMeasuredCheckV1, PalwMeasuredModelV1, PalwModelManifestV1, palw_measure_model_v1,
    palw_measured_model_id_v1, palw_verify_measured_model_v1,
};
use kaspa_consensus_core::palw_mode_v2::PALW_STANDARD_TX_BYTES;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_fit_v1::{palw_widest_context_under_the_geometry_ceiling_v1, stand_ins};
use kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B;
use kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B;
use kaspa_consensus_core::palw_seat_coverage_v1::{
    palw_sharded_panel_v1, palw_shipped_draws_per_seat_v1, palw_widest_shard_count_in_one_transaction_v1,
};
use kaspa_consensus_core::palw_shard_licensing_v1::palw_shard_receipt_part_wire_bytes_v1;
use kaspa_consensus_core::palw_shard_plan_v1::{palw_shard_leaf_run_v1, palw_shard_plan_for_seat_v1, palw_shard_plan_v1};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, trace_scheme_id_v2};
use kaspa_hashes::Hash64;

const GIB: f64 = (1u64 << 30) as f64;
/// The point of judgement the fences are read at: every scheduled fence armed, `never()` dormant.
const EVER: u64 = u64::MAX - 1;
const CONTEXTS: [u32; 4] = [512, 32_768, 131_072, 1_048_576];

struct Args {
    manifest: Option<PalwModelManifestV1>,
    seat_gibs: Vec<u64>,
    replay_ms: Option<u64>,
    measured_on: String,
    measured_out: Option<String>,
    verify: Option<String>,
}

fn args() -> Args {
    let mut out = Args {
        manifest: None,
        seat_gibs: vec![24, 64, 128, 512],
        replay_ms: None,
        measured_on: "not measured".into(),
        measured_out: None,
        verify: None,
    };
    let mut it = std::env::args().skip(1);
    let usage = "usage: palw-shard-plan [--manifest <model.json>] [--seat-gib <n>,<n>,…] [--replay-ms <ms>] [--measured-on <host>] [--measured-out <dir>] [--verify <measured.json>]";
    while let Some(a) = it.next() {
        let mut value = || it.next().unwrap_or_else(|| panic!("{a} needs a value\n{usage}"));
        match a.as_str() {
            "--manifest" => {
                let path = value();
                let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
                out.manifest =
                    Some(serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path} is not a PalwModelManifestV1: {e}")));
            }
            "--seat-gib" => {
                out.seat_gibs =
                    value().split(',').map(|s| s.trim().parse().unwrap_or_else(|e| panic!("--seat-gib {s:?}: {e}"))).collect();
            }
            "--replay-ms" => out.replay_ms = Some(value().parse().unwrap_or_else(|e| panic!("--replay-ms: {e}"))),
            "--measured-on" => out.measured_on = value(),
            "--measured-out" => out.measured_out = Some(value()),
            "--verify" => out.verify = Some(value()),
            other => panic!("unknown argument {other:?}\n{usage}"),
        }
    }
    out
}

fn candidates(manifest: Option<PalwModelManifestV1>) -> Vec<PalwModelManifestV1> {
    let mut out = vec![
        PalwModelManifestV1::from_dense("Qwen2.5-1.5B A16 graph-v5 (dense)", &QWEN25_1_5B),
        PalwModelManifestV1::from_hybrid("Qwen3.6-35B-A3B graph-v5 (hybrid)", &QWEN36_35B_A3B, None),
        PalwModelManifestV1::from_hybrid(
            "Kimi K3 stand-in, formula (ADR-0097 §1.3; NOT a class)",
            &stand_ins::KIMI_K3_AS_HYBRID_V1,
            None,
        ),
        PalwModelManifestV1::from_hybrid(
            "Kimi K3 stand-in, the card's total (NOT a class)",
            &stand_ins::KIMI_K3_AS_HYBRID_V1,
            Some(stand_ins::KIMI_K3_TOTAL_PARAMETERS),
        ),
    ];
    if let Some(m) = manifest {
        out.push(m);
    }
    out
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / GIB)
}

fn slug(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
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

struct Ruleset {
    name: &'static str,
    params: Params,
    fingerprint_hex: String,
}

impl Ruleset {
    fn rc() -> Self {
        let params = palw_rc_shipped_params();
        let fingerprint_hex = format!("{}", params.consensus_params_id());
        Ruleset { name: "testnet-11 (RC)", params, fingerprint_hex }
    }
    fn bundle(&self) -> &PalwConsensusParamsV2 {
        match &self.params.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(b) => b,
            _ => panic!("the RC ships a ConsensusV2 bundle"),
        }
    }
    fn inputs<'a>(&'a self, seat_budgets: &'a [u64]) -> PalwMeasureInputsV1<'a> {
        PalwMeasureInputsV1 {
            ruleset: self.name,
            ruleset_fingerprint_hex: &self.fingerprint_hex,
            bundle: self.bundle(),
            contexts: &CONTEXTS,
            seat_budgets,
            max_shards: 1_024,
            held: None,
        }
    }
}

fn main() {
    let args = args();
    let ruleset = Ruleset::rc();
    let court_for = |profile: &PalwShapeProfileV3| {
        let shape =
            palw_admission_shape_at_v1(&ruleset.params, ruleset.bundle(), profile, EVER).expect("the RC has an admission shape");
        (shape.court, ruleset.params.palw_prompt_ids_form_at(EVER))
    };
    let seat_budgets: Vec<u64> = args.seat_gibs.iter().map(|g| g * (1u64 << 30)).collect();

    // ---- --verify: recompute one document and say what agrees.
    if let Some(path) = &args.verify {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let doc: PalwMeasuredModelV1 =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path} is not a PalwMeasuredModelV1: {e}"));
        let verdict = palw_verify_measured_model_v1(&doc, ruleset.inputs(&seat_budgets), &court_for);
        println!("# Measured Model Artifact — verification on {}\n", ruleset.name);
        println!("- model **{}**, id `{}`\n", doc.manifest.name, palw_measured_model_id_v1(&doc));
        println!("| field | verdict |");
        println!("|---|---|");
        for c in &verdict.checks {
            match c {
                PalwMeasuredCheckV1::Recomputed { field } => println!("| {field} | recomputed, equal |"),
                PalwMeasuredCheckV1::Mismatch { field, expected, got } => {
                    println!("| {field} | **MISMATCH** — recomputed `{expected}`, the document says `{got}` |")
                }
                PalwMeasuredCheckV1::SelfReported { field, verified_by } => {
                    println!("| {field} | self-reported; verified by {verified_by} |")
                }
                PalwMeasuredCheckV1::NeedsTheArtifact { field } => {
                    println!(
                        "| {field} | needs the artifact: measured from its inventory; `palw-class verify --artifact` recomputes it |"
                    )
                }
            }
        }
        println!("\n- deterministic half: **{}**", if verdict.deterministic_ok() { "agrees" } else { "REFUSED" });
        if !verdict.deterministic_ok() {
            std::process::exit(1);
        }
        return;
    }

    let (panel, quorum) = (u32::from(PALW_V2_PANEL_SEATS), u32::from(PALW_V2_PANEL_QUORUM));
    println!("# ADR-0099 — the plan a model shards into, and what a shard costs a seat\n");
    println!(
        "Ruleset **{}** (`{}…`). A shard is a contiguous run of layers (shard 0 with the embedding, the last with the logits); a plan \
         for k shards is the contiguous partition minimising the widest seat — artifact plus the state its layers hold at the class's \
         context — and it is derived, never chosen. Artifact bytes are the family formula's at one byte a weight (a floor: no norms, no \
         biases); where a card states a total the formula does not reach, the card's row is a second candidate. A shard seat resumes an \
         interval in one of two forms, both committed material: **recompute** (the previous shard's rows for every earlier position) or \
         **resume** (its own layers' state chunks at the interval's start); the table prints both. A profile exists only under the \
         geometry ceiling (ADR-0097), so the 1M row is arithmetic on the geometry.\n",
        ruleset.name,
        &ruleset.fingerprint_hex[..16],
    );

    for manifest in candidates(args.manifest.clone()) {
        let artifact = manifest.artifact_bytes();
        println!("## {}\n", manifest.name);
        println!(
            "- family `{:?}`, {} layers, hidden {}; artifact **{}** ({}); embedding {} · unembedding {} · heaviest layer {} · lightest layer {}\n",
            manifest.family,
            manifest.layer_count,
            manifest.hidden_dim,
            gib(artifact.total()),
            artifact.basis,
            gib(artifact.pre),
            gib(artifact.post),
            gib(artifact.layers.iter().copied().max().unwrap_or(0)),
            gib(artifact.layers.iter().copied().min().unwrap_or(0)),
        );

        for n_ctx in [512u32, 131_072] {
            let Ok(profile) = manifest.profile(n_ctx) else {
                println!(
                    "### n_ctx {n_ctx}: no profile — past the geometry ceiling ({})\n",
                    palw_widest_context_under_the_geometry_ceiling_v1(manifest.layer_count)
                );
                continue;
            };
            println!("### n_ctx {n_ctx}\n");
            println!(
                "| shards | widest seat (artifact + cache + state) | its layers | per seat per job: recompute / resume | licensing receipts / bytes / one tx? |"
            );
            println!("|---|---|---|---|---|");
            for shards in [1u32, 2, 4, 8, 16, 32, 64] {
                if shards > u32::from(manifest.layer_count) {
                    continue;
                }
                match palw_shard_plan_v1(&profile, &artifact, shards) {
                    Ok(plan) => {
                        let widest = plan.shards.iter().max_by_key(|s| s.seat_bytes()).expect("a shard");
                        let (recompute, resume) = widest.resume_transfer_bytes(u64::from(n_ctx), plan.boundary_row_bytes);
                        let lic = palw_sharded_panel_v1(shards, panel, quorum, 299, palw_shipped_draws_per_seat_v1());
                        println!(
                            "| {shards} | {} ({} + {} + {}) | {} ({}–{}) | {} / {} | {} / {} / {} |",
                            gib(plan.widest_seat_bytes),
                            gib(widest.artifact_bytes),
                            gib(widest.kv_cache_bytes),
                            gib(widest.recurrent_state_bytes),
                            widest.layer_count,
                            widest.first_layer,
                            widest.first_layer + widest.layer_count - 1,
                            gib(recompute),
                            gib(resume),
                            lic.licensing_receipts,
                            lic.licensing_bytes,
                            if lic.fits_one_standard_transaction { "yes" } else { "**no**" },
                        );
                    }
                    Err(e) => println!("| {shards} | {e} | | | |"),
                }
            }
            println!(
                "\n- licensing per shard (ADR-0100 Decision 4): one part of {} bytes a shard at quorum {quorum}, fitting one \
                 standard transaction ({} bytes) at every shard count; the whole-object form above fits up to {} shards",
                palw_shard_receipt_part_wire_bytes_v1(u64::from(quorum)),
                PALW_STANDARD_TX_BYTES,
                palw_widest_shard_count_in_one_transaction_v1(quorum),
            );
            println!("\nFewest shards a seat can hold one of:\n");
            println!("| seat | shards | widest seat | boundary rows per job (all shards) |");
            println!("|---|---|---|---|");
            for (g, budget) in args.seat_gibs.iter().zip(seat_budgets.iter()) {
                match palw_shard_plan_for_seat_v1(&profile, &artifact, *budget, u32::from(manifest.layer_count)) {
                    Ok(plan) => println!(
                        "| {g} GiB | **{}** | {} | {} |",
                        plan.shard_count,
                        gib(plan.widest_seat_bytes),
                        gib(plan.boundary_bytes_per_job(u64::from(n_ctx)))
                    ),
                    Err(e) => println!("| {g} GiB | — | {e} | |"),
                }
            }
            println!();
        }

        if let Ok(profile) = manifest.profile(512) {
            let ctx = job_context(&profile, 8, 4);
            println!("### Leaves per shard, a job of 8 prefill + 4 decode tokens at n_ctx 512 (4 shards)\n");
            match palw_shard_plan_v1(&profile, &artifact, 4.min(u32::from(manifest.layer_count))) {
                Ok(plan) => {
                    println!(
                        "| shard | layers | slots | leaves at prefill position 0 | at the last prefill position | at decode call 1 |"
                    );
                    println!("|---|---|---|---|---|---|");
                    for s in &plan.shards {
                        let at = |call: u32, position: u32| {
                            palw_shard_leaf_run_v1(&profile, &ctx, s, call, position)
                                .map(|(f, n)| format!("{n} (from leaf {f})"))
                                .unwrap_or("—".into())
                        };
                        println!(
                            "| {} | {}–{} | {}..{} | {} | {} | {} |",
                            s.index,
                            s.first_layer,
                            s.first_layer + s.layer_count - 1,
                            s.first_slot,
                            s.first_slot + s.slot_count,
                            at(0, 0),
                            at(0, 7),
                            at(1, 0)
                        );
                    }
                    println!();
                }
                Err(e) => println!("- no 4-shard plan: {e}\n"),
            }
        }

        println!("### At 1,048,576 positions (arithmetic on the geometry; no profile exists past the ceiling)\n");
        let kv_row = u64::from(manifest.attn_kv_heads) * u64::from(manifest.attn_head_dim) * 4;
        let attention_layers = (0..manifest.layer_count)
            .filter(|i| {
                manifest.full_attention_interval == 0
                    || (u32::from(*i) + 1).is_multiple_of(u32::from(manifest.full_attention_interval))
            })
            .count() as u64;
        let per_layer_cache = kv_row * 2 * 1_048_576;
        println!(
            "- one attention layer's cache **{}**; all {} attention layers **{}**; one boundary's rows for the job **{}**",
            gib(per_layer_cache),
            attention_layers,
            gib(per_layer_cache * attention_layers),
            gib(u64::from(manifest.hidden_dim) * 4 * 1_048_576),
        );
        for (g, budget) in args.seat_gibs.iter().zip(seat_budgets.iter()) {
            let by_artifact = artifact.total().div_ceil((*budget).max(1));
            let with_cache = artifact.total().saturating_add(per_layer_cache * attention_layers).div_ceil((*budget).max(1));
            println!(
                "- a {g} GiB seat: at least **{by_artifact}** shards for the artifact alone, **{with_cache}** with every cache — lower bounds (a shard is whole layers)"
            );
        }
        println!();

        // ---- the Measured Model Artifact
        let doc = palw_measure_model_v1(
            &manifest,
            ruleset.inputs(&seat_budgets),
            &court_for,
            if Some(&manifest) == args.manifest.as_ref() { args.replay_ms } else { None },
            if Some(&manifest) == args.manifest.as_ref() { &args.measured_on } else { "not measured" },
        );
        println!("### Measured Model Artifact (`{}`)\n", palw_measured_model_id_v1(&doc));
        println!("| n_ctx | class id | fit | refused by | cache | recurrent | fewest shards per seat budget |");
        println!("|---|---|---|---|---|---|---|");
        for row in &doc.deterministic.rows {
            println!(
                "| {} | {} | {} | {} | {} | {} | {} |",
                row.n_ctx,
                row.shape_profile_id_hex.as_deref().map(|h| format!("`{}…`", &h[..12])).unwrap_or("— (no profile)".into()),
                if row.fit_admitted { "admitted" } else { "**refused**" },
                row.refusing_walls.join(", "),
                gib(row.kv_cache_bytes),
                gib(row.recurrent_state_bytes),
                row.plans
                    .iter()
                    .map(|p| format!(
                        "{} GiB → {}",
                        p.seat_budget_bytes >> 30,
                        p.shard_count.map(|s| s.to_string()).unwrap_or("none".into())
                    ))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
        }
        println!(
            "\n- self-reported: replay {} ms/position on {:?} → {} positions within window_receipt; {}\n",
            doc.self_reported.replay_ms_per_position.map(|m| m.to_string()).unwrap_or("—".into()),
            doc.self_reported.measured_on,
            doc.self_reported.positions_within_window_receipt.map(|p| p.to_string()).unwrap_or("—".into()),
            doc.self_reported.verified_by,
        );
        if let Some(dir) = &args.measured_out {
            std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("{dir}: {e}"));
            let path = format!("{dir}/{}.measured.json", slug(&manifest.name));
            std::fs::write(&path, serde_json::to_string_pretty(&doc).expect("json")).unwrap_or_else(|e| panic!("{path}: {e}"));
            println!("- written: `{path}`\n");
        }
    }
    print_held_fetch_column_v1(&args);
}

/// **ADR-0103 Decision 7: the fetch column, on the held rows, at 2M.** For each held row (graph-v7:
/// the dense lineage and the K3 stand-in on the held composition) at `2^21` positions: per shard
/// count, the widest shard's fetch at the job's last interval and what resuming it costs at three
/// seat links; then the fewest shards a seat can hold AND resume inside testnet-11's
/// `window_receipt` (the drill's margin), per link. Arithmetic over the plan and the family's
/// measured replay row; the certification drill is what makes a width a number (ADR-0075 D7).
fn print_held_fetch_column_v1(args: &Args) {
    use kaspa_consensus_core::palw_held_context_v1::{
        palw_held_interval_positions_v1, palw_held_replay_row_v1, palw_held_seat_budget_ms_v1,
    };
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_artifact_row_profile_v7};
    use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_profile_v7};
    use kaspa_consensus_core::palw_shard_plan_v1::{
        PalwSeatResumeBudgetV1, palw_qwen25_artifact_bytes_v1, palw_qwen36_artifact_bytes_v1,
        palw_shard_plan_for_seat_within_window_v1, palw_shard_resume_ms_v1,
    };
    let n_ctx = 1u32 << 21;
    let window_receipt = match &palw_rc_shipped_params().palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.window_receipt(),
        _ => return,
    };
    let links: [(&str, u64); 3] = [("10 Gbit/s", 1_250_000_000), ("1 Gbit/s", 125_000_000), ("100 Mbit/s", 12_500_000)];
    let seat = args.seat_gibs.iter().copied().max().unwrap_or(256) << 30;
    println!("## ADR-0103 Decision 7 — the fetch column, on the held rows, at 2^21 positions\n");
    println!(
        "The seat's budget is `window_receipt` = {window_receipt} DAA over the drill's margin = **{} ms**; a seat of {} GiB. The fetch is the \
         widest shard's state at the last interval's start (`n_ctx − P`); a shard replays `P` positions of its share of the layers.\n",
        palw_held_seat_budget_ms_v1(window_receipt),
        seat >> 30
    );
    let rows: Vec<(&str, Result<PalwShapeProfileV3, String>, kaspa_consensus_core::palw_shard_plan_v1::PalwArtifactBytesV1)> = vec![
        (
            "Qwen2.5-1.5B A16 graph-v7 (dense, held)",
            qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B }).map_err(|e| format!("{e:?}")),
            palw_qwen25_artifact_bytes_v1(&QWEN25_1_5B),
        ),
        (
            "Kimi K3 stand-in as graph-v7 (card total; NOT a class)",
            qwen36_profile_v7(PalwQwen36GeometryV1 { n_ctx, ..stand_ins::KIMI_K3_AS_HYBRID_V1 }).map_err(|e| format!("{e:?}")),
            palw_qwen36_artifact_bytes_v1(&PalwQwen36GeometryV1 { n_ctx, ..stand_ins::KIMI_K3_AS_HYBRID_V1 })
                .scaled_to_total(stand_ins::KIMI_K3_TOTAL_PARAMETERS),
        ),
    ];
    for (name, profile, artifact) in rows {
        let Ok(profile) = profile else {
            println!("### {name}\n\n- the held row does not build at 2^21\n");
            continue;
        };
        let p = palw_held_interval_positions_v1(&profile);
        let rate = palw_held_replay_row_v1(&profile).replay_ms_per_position();
        let budget_at = |bandwidth: u64| PalwSeatResumeBudgetV1 {
            seat_budget_bytes: seat,
            bandwidth_bytes_per_second: bandwidth,
            window_receipt_daa: window_receipt,
            replay_ms_per_position: rate,
            interval_positions: p,
        };
        println!("### {name} — P = {p} positions, {rate} ms a position (the family's replay row)\n");
        print!("| shards | widest seat | widest fetch at the last interval |");
        for (link, _) in links {
            print!(" resume at {link} |");
        }
        println!();
        println!("|---|---|---|---|---|---|");
        for shards in [1u32, 2, 4, 8, 16, 32, 64] {
            if shards > u32::from(profile.layer_count) {
                continue;
            }
            let Ok(plan) = palw_shard_plan_v1(&profile, &artifact, shards) else { continue };
            let start = u64::from(n_ctx - p);
            let widest_fetch = plan.shards.iter().map(|s| s.fetch_bytes_at_v1(plan.kv_row_bytes, start)).max().unwrap_or(0);
            print!("| {shards} | {} | {} |", gib(plan.widest_seat_bytes), gib(widest_fetch));
            for (_, bandwidth) in links {
                let slowest = plan
                    .shards
                    .iter()
                    .map(|s| palw_shard_resume_ms_v1(&plan, s, profile.layer_count, &budget_at(bandwidth)))
                    .max()
                    .unwrap_or(0);
                print!(" {:.1} h |", slowest as f64 / 3_600_000.0);
            }
            println!();
        }
        println!("\nFewest shards a {} GiB seat can hold AND resume inside the window:\n", seat >> 30);
        println!("| link | shards |");
        println!("|---|---|");
        for (link, bandwidth) in links {
            match palw_shard_plan_for_seat_within_window_v1(&profile, &artifact, &budget_at(bandwidth), u32::from(profile.layer_count))
            {
                Ok(plan) => println!("| {link} | **{}** |", plan.shard_count),
                Err(e) => println!("| {link} | {e} |"),
            }
        }
        println!();
    }
}
