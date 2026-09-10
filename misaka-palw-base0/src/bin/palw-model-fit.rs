//! **`palw-model-fit` — ADR-0097's generator: a model's fit is a lookup.**
//!
//! For each family this tree can price — the dense A16 row, the hybrid row, and a stand-in for
//! the widest model anyone has asked about (Kimi K3, as `stand_ins::KIMI_K3_AS_HYBRID_V1`) —
//! prints every wall the class meets on a shipped ruleset, with the number it needs and the number
//! the ruleset has, from the SAME predicates admission and the court run
//! (`kaspa_consensus_core::palw_model_fit_v1`). Then the sweep over contexts, the widest context
//! each wall admits, the answer to "does a 2M context fit" for every depth, and what a seat must
//! hold to replay one job.
//!
//! Every figure is a generated artifact (ADR-0092 §5, kept by ADR-0097): a reader who needs a value
//! runs this; a reader who finds a value in an ADR that this binary does not print has found a bug
//! in the document.
//!
//! ```text
//! palw-model-fit [--preset rc|devnet] [--daa <score>]
//! ```
//!
//! `--daa` is the point of judgement the fences are read at — default `u64::MAX - 1`, every
//! scheduled fence armed and every `never()` fence dormant — because the question a fit answers is
//! "on this ruleset, ever", not "at this block".

use kaspa_consensus_core::config::params::{Params, devnet_shipped_params, palw_rc_shipped_params};
use kaspa_consensus_core::palw_class_admission_v2::{PalwKaryCourtV1, palw_admission_shape_at_v1};
use kaspa_consensus_core::palw_context_ladder::{palw_a16_context_row_profile_v5, palw_qwen36_context_row_profile_v5};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_fit_v1::{
    PalwFitVerdictV1, PalwFitWallV1, PalwModelFitReportV1, palw_fewest_layers_refused_at_context_v1, palw_geometry_ceiling_fit_v1,
    palw_model_fit_v1, palw_widest_context_under_the_geometry_ceiling_v1, stand_ins,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_artifact_row_profile_v5};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, PalwStepError};

type Build = fn(u32) -> Result<PalwShapeProfileV3, PalwStepError>;

struct Candidate {
    name: &'static str,
    /// The width the family SHIPS a registered row at — what the first table prices.
    shipped_n_ctx: Option<u32>,
    layer_count: u16,
    build: Build,
    /// A lower bound on the artifact, at the integer family's one byte a weight, when the
    /// parameter count is public and the artifact is not in this tree.
    parameters: Option<u64>,
}

fn kimi_k3(n_ctx: u32) -> Result<PalwShapeProfileV3, PalwStepError> {
    qwen36_artifact_row_profile_v5(PalwQwen36GeometryV1 { n_ctx, ..stand_ins::KIMI_K3_AS_HYBRID_V1 })
}

fn candidates() -> Vec<Candidate> {
    vec![
        Candidate {
            name: "Qwen2.5-1.5B A16 graph-v5 (dense; the row testnet-11 registers at 512)",
            shipped_n_ctx: Some(kaspa_consensus_core::palw_qwen25_profile::QWEN25_A16_GRAPH_V5_N_CTX),
            layer_count: kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B.layer_count,
            build: palw_a16_context_row_profile_v5,
            parameters: None,
        },
        Candidate {
            name: "Qwen3.6-35B-A3B graph-v5 (hybrid; the family's row is at 8)",
            shipped_n_ctx: Some(kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B.n_ctx),
            layer_count: kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B.layer_count,
            build: palw_qwen36_context_row_profile_v5,
            parameters: None,
        },
        Candidate {
            name: "Kimi K3 stand-in (ADR-0097 §1.3; `stand_ins::KIMI_K3_AS_HYBRID_V1` — NOT a class)",
            shipped_n_ctx: None,
            layer_count: stand_ins::KIMI_K3_AS_HYBRID_V1.layer_count,
            build: kimi_k3,
            parameters: Some(stand_ins::KIMI_K3_TOTAL_PARAMETERS),
        },
    ]
}

struct Ruleset {
    name: &'static str,
    params: Params,
    daa: u64,
}

impl Ruleset {
    fn bundle(&self) -> &PalwConsensusParamsV2 {
        match &self.params.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle,
            _ => panic!("{} ships no ConsensusV2 bundle", self.name),
        }
    }

    /// The court and the id form a registration of `profile` would be judged under at `daa` —
    /// `palw_admission_shape_at_v1`, the one spelling the acceptance path uses.
    fn shape(&self, profile: &PalwShapeProfileV3) -> Result<(Option<PalwKaryCourtV1>, PalwPromptIdsFormV1), String> {
        let shape = palw_admission_shape_at_v1(&self.params, self.bundle(), profile, self.daa)?;
        Ok((shape.court, self.params.palw_prompt_ids_form_at(self.daa)))
    }

    fn fit(&self, profile: &PalwShapeProfileV3) -> Result<PalwModelFitReportV1, String> {
        let (court, form) = self.shape(profile)?;
        Ok(palw_model_fit_v1(profile, self.bundle(), court, form))
    }
}

fn args() -> Ruleset {
    let mut preset = "rc".to_string();
    let mut daa = u64::MAX - 1;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--preset" => preset = it.next().unwrap_or_else(|| panic!("--preset needs a value")),
            "--daa" => daa = it.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("--daa needs a number")),
            other => panic!("unknown argument {other:?}\nusage: palw-model-fit [--preset rc|devnet] [--daa <score>]"),
        }
    }
    match preset.as_str() {
        "rc" | "testnet-11" => Ruleset { name: "testnet-11 (RC)", params: palw_rc_shipped_params(), daa },
        "devnet" => Ruleset { name: "devnet", params: devnet_shipped_params(), daa },
        other => panic!("--preset {other:?}: rc or devnet"),
    }
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} B") } else { format!("{value:.1} {}", UNITS[unit]) }
}

fn verdict_cell(report: &PalwModelFitReportV1, wall: PalwFitWallV1) -> String {
    match report.row(wall) {
        Some(row) => match row.verdict {
            PalwFitVerdictV1::Admitted => "admitted".to_string(),
            PalwFitVerdictV1::Refused => format!("**REFUSED** {} / {}", row.need, row.have),
            PalwFitVerdictV1::Unpriced => "unpriced".to_string(),
        },
        None => "—".to_string(),
    }
}

fn print_full_report(title: &str, report: &PalwModelFitReportV1) {
    println!("### {title}\n");
    println!(
        "- n_ctx **{}**, {} layers ({} attention, {} recurrent), fused site: **{}**; arity played **{}**, arity this row would derive: **{}**; prompt ids **{:?}** ({} bytes a close); one job answers at most **{}** tokens\n",
        report.n_ctx,
        report.layer_count,
        report.seat.attention_layers,
        report.seat.recurrent_layers,
        report.fused,
        report.arity_played,
        report.arity_derived_for_this_row.map(|a| a.to_string()).unwrap_or_else(|| "none fits".into()),
        report.prompt_ids_form,
        report.prompt_ids_term_on_close_bytes,
        report.answer_tokens_per_job,
    );
    println!("| wall | need | have | unit | verdict | note |");
    println!("|---|---|---|---|---|---|");
    for row in &report.rows {
        let need = if row.verdict == PalwFitVerdictV1::Unpriced { "—".to_string() } else { row.need.to_string() };
        println!(
            "| {} | {need} | {} | {} | {} | {} |",
            row.wall.name(),
            row.have,
            row.unit,
            match row.verdict {
                PalwFitVerdictV1::Admitted => "admitted",
                PalwFitVerdictV1::Refused => "**REFUSED**",
                PalwFitVerdictV1::Unpriced => "unpriced",
            },
            row.note.replace('|', "\\|")
        );
    }
    println!(
        "\n- verdict: **{}**{}\n",
        if report.admitted() { "ADMITTED on every wall" } else { "REFUSED" },
        if report.admitted() {
            String::new()
        } else {
            format!(
                " — by {}{}",
                report.refusing_walls().iter().map(|w| w.name()).collect::<Vec<_>>().join(", "),
                if report.unpriced_walls().is_empty() {
                    String::new()
                } else {
                    format!("; unpriced: {}", report.unpriced_walls().iter().map(|w| w.name()).collect::<Vec<_>>().join(", "))
                }
            )
        }
    );
}

/// The widest `n_ctx` in `1..=hi` at which `wall` admits the row, by bisection on the assumption
/// that a wall's verdict is monotone in the context — true of every wall here by construction
/// (each `need` is non-decreasing in `n_ctx`), and stated because a bisection over a non-monotone
/// predicate returns a boundary rather than the boundary.
fn widest_admitted(ruleset: &Ruleset, build: Build, wall: PalwFitWallV1, hi: u32) -> Option<u32> {
    let admits = |n_ctx: u32| -> bool {
        build(n_ctx)
            .ok()
            .and_then(|p| ruleset.fit(&p).ok())
            .and_then(|r| r.row(wall).map(|row| row.verdict == PalwFitVerdictV1::Admitted))
            .unwrap_or(false)
    };
    if hi == 0 || !admits(1) {
        return None;
    }
    if admits(hi) {
        return Some(hi);
    }
    let (mut lo, mut hi) = (1u32, hi);
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if admits(mid) { lo = mid } else { hi = mid }
    }
    Some(lo)
}

fn main() {
    let ruleset = args();
    let bundle = ruleset.bundle();
    let court = bundle.court;
    println!("# ADR-0097 — a model's fit is a lookup\n");
    println!(
        "Ruleset **{}**, fences read at DAA **{}**: `palw_kary_court` {}, `palw_context_ladder` {}, prompt ids **{:?}**. \
         Ladder **2^{}**, close ceiling **{} bytes** / **{} carriers**, turn deadline **{} DAA**, terminal rounds **{}**, court window **{} DAA**, \
         standard transaction **{} bytes**, free-prompt decode ceiling **{} tokens**.\n",
        ruleset.name,
        ruleset.daa,
        if ruleset.params.palw_kary_court_active_at(ruleset.daa) { "armed" } else { "dormant" },
        if ruleset.params.palw_context_ladder.is_some_and(|f| f.is_active(ruleset.daa)) { "armed" } else { "dormant" },
        ruleset.params.palw_prompt_ids_form_at(ruleset.daa),
        court.max_step_leaf_count().trailing_zeros(),
        court.max_close_bytes(),
        court.max_close_chunks(),
        court.turn_deadline_daa(),
        court.terminal_rounds(),
        bundle.state.window_court(),
        kaspa_consensus_core::palw_mode_v2::PALW_STANDARD_TX_BYTES,
        bundle.freeprompt.max_decode_tokens(),
    );
    println!("Where each ceiling lives, and therefore what it costs to move:\n");
    println!("| wall | the ceiling lives in |");
    println!("|---|---|");
    for wall in PalwFitWallV1::ALL {
        println!("| {} | `{}` |", wall.name(), wall.ceiling_lives_in());
    }
    println!();

    // ---------------------------------------------------------------------------------------
    println!("## 1. The rows this ruleset's genesis registers, and each family at its own width\n");
    println!(
        "First the rows the ruleset itself carries (`genesis_objects`, `ClassRegistered` with a carried profile) — the positive control: the chain admitted these when it was cut. Then each family at the width its geometry constant declares.\n"
    );
    let mut carried = 0usize;
    for object in &bundle.genesis_objects {
        let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
            class_id, admission: Some(carriage), ..
        } = object
        else {
            continue;
        };
        carried += 1;
        let title = format!("genesis row `{class_id}` (n_ctx {}, {} layers)", carriage.profile.n_ctx, carriage.profile.layer_count);
        match ruleset.fit(&carriage.profile) {
            Ok(report) => print_full_report(&title, &report),
            Err(e) => println!("### {title}\n\n- the ruleset has no admission shape for its own row: {e}\n"),
        }
    }
    if carried == 0 {
        println!("- this ruleset's genesis registers no row with a carried profile.\n");
    }
    for candidate in candidates() {
        let Some(n_ctx) = candidate.shipped_n_ctx else { continue };
        match (candidate.build)(n_ctx) {
            Ok(profile) => match ruleset.fit(&profile) {
                Ok(report) => print_full_report(candidate.name, &report),
                Err(e) => println!("### {}\n\n- the ruleset has no admission shape for this row: {e}\n", candidate.name),
            },
            Err(e) => println!("### {}\n\n- the family cannot build its own shipped row: {e:?}\n", candidate.name),
        }
    }

    // ---------------------------------------------------------------------------------------
    println!("## 2. The sweep — every wall at every context, every candidate\n");
    println!(
        "`REFUSED need / have`; a row the family cannot BUILD is refused at the geometry ceiling before any other wall can price it. `unpriced` on the four close walls is what a row the ladder refuses says of its close: the derivation's walk is capped at the ladder (audit D H-5), so the close of a row deeper than the ladder is not a number.\n"
    );
    let contexts: [u32; 8] = [512, 2_048, 8_192, 32_768, 131_072, 524_288, 1_048_576, 2_097_152];
    for candidate in candidates() {
        println!("### {}\n", candidate.name);
        print!("| n_ctx |");
        for wall in PalwFitWallV1::ALL {
            print!(" {} |", wall.name());
        }
        println!(" verdict |");
        print!("|---|");
        for _ in PalwFitWallV1::ALL {
            print!("---|");
        }
        println!("---|");
        for n_ctx in contexts {
            print!("| {n_ctx} |");
            let geometry = palw_geometry_ceiling_fit_v1(n_ctx, candidate.layer_count);
            match (candidate.build)(n_ctx) {
                Ok(profile) => match ruleset.fit(&profile) {
                    Ok(report) => {
                        for wall in PalwFitWallV1::ALL {
                            print!(" {} |", verdict_cell(&report, wall));
                        }
                        println!(
                            " {} |",
                            if report.admitted() {
                                "**admitted**".to_string()
                            } else {
                                format!(
                                    "refused by {}",
                                    report.refusing_walls().iter().map(|w| w.name()).collect::<Vec<_>>().join(", ")
                                )
                            }
                        );
                    }
                    Err(e) => println!(" no admission shape: {e} |"),
                },
                Err(e) => {
                    print!(
                        " {} |",
                        if geometry.verdict == PalwFitVerdictV1::Refused {
                            format!("**REFUSED** {} / {}", geometry.need, geometry.have)
                        } else {
                            "admitted".into()
                        }
                    );
                    for _ in 1..PalwFitWallV1::ALL.len() {
                        print!(" — |");
                    }
                    println!(" the family refuses to build the row: {e:?} |");
                }
            }
        }
        println!();
    }

    // ---------------------------------------------------------------------------------------
    println!("## 3. The widest context each wall admits, per candidate\n");
    println!(
        "Each cell is the largest `n_ctx` at which THAT wall alone admits the row (bisection; every wall's `need` is non-decreasing in the context). The row's fit is the minimum of its cells. The four close cells cannot exceed the ladder's: past it the close is unpriced, not admitted.\n"
    );
    print!("| candidate |");
    for wall in PalwFitWallV1::ALL {
        print!(" {} |", wall.name());
    }
    println!(" fit |");
    print!("|---|");
    for _ in PalwFitWallV1::ALL {
        print!("---|");
    }
    println!("---|");
    for candidate in candidates() {
        let hi = palw_widest_context_under_the_geometry_ceiling_v1(candidate.layer_count);
        print!("| {} |", candidate.name.split(" (").next().unwrap_or(candidate.name));
        let mut fit = u32::MAX;
        for wall in PalwFitWallV1::ALL {
            match widest_admitted(&ruleset, candidate.build, wall, hi) {
                Some(w) => {
                    fit = fit.min(w);
                    print!(" {w} |");
                }
                None => {
                    fit = 0;
                    print!(" none |");
                }
            }
        }
        println!(" **{}** |", if fit == u32::MAX { "—".to_string() } else { fit.to_string() });
    }
    println!();

    // ---------------------------------------------------------------------------------------
    println!("## 4. Does a 2M context fit? The geometry ceiling, for every depth\n");
    println!(
        "`PALW_STEP_MAX_ENUMERATION` bounds `n_ctx × layer_count` at **{}** (`PalwShapeProfileV3::validate_geometry`). At 2^21 positions the fewest layers refused is **{}**; at 2^20, **{}**.\n",
        kaspa_consensus_core::palw_step::PALW_STEP_MAX_ENUMERATION,
        palw_fewest_layers_refused_at_context_v1(1 << 21).map(|l| l.to_string()).unwrap_or_else(|| "none".into()),
        palw_fewest_layers_refused_at_context_v1(1 << 20).map(|l| l.to_string()).unwrap_or_else(|| "none".into()),
    );
    println!("| layers | widest context the ceiling admits | 2^21 (2M) | 2^20 (1M) | 2^17 (128K) |");
    println!("|---|---|---|---|---|");
    for layers in [1u16, 8, 9, 16, 28, 40, 64, 92, 93, 128, 256, 1024] {
        let cell = |n_ctx: u32| {
            if palw_geometry_ceiling_fit_v1(n_ctx, layers).verdict == PalwFitVerdictV1::Admitted { "admitted" } else { "**REFUSED**" }
        };
        println!(
            "| {layers} | {} | {} | {} | {} |",
            palw_widest_context_under_the_geometry_ceiling_v1(layers),
            cell(1 << 21),
            cell(1 << 20),
            cell(1 << 17)
        );
    }
    println!();

    // ---------------------------------------------------------------------------------------
    println!("## 5. What a seat must hold to replay one job\n");
    println!(
        "From the geometry and the state map's own row widths (i32 cache: `kv_heads × head_dim × 4` a position a layer). No verdict: no ruleset states what a host has. The artifact is the converter's number and is not here; for a model whose artifact this tree does not hold, the parameter count is printed as a lower bound at one byte a weight.\n"
    );
    println!("| candidate | n_ctx | attention cache | recurrent state | prompt ids | artifact (lower bound) |");
    println!("|---|---|---|---|---|---|");
    for candidate in candidates() {
        for n_ctx in [512u32, 32_768, 131_072] {
            let Ok(profile) = (candidate.build)(n_ctx) else { continue };
            let Ok(report) = ruleset.fit(&profile) else { continue };
            println!(
                "| {} | {n_ctx} | {} | {} | {} | {} |",
                candidate.name.split(" (").next().unwrap_or(candidate.name),
                human(report.seat.kv_cache_bytes),
                human(report.seat.recurrent_state_bytes),
                human(report.seat.prompt_ids_bytes),
                candidate.parameters.map(human).unwrap_or_else(|| "the converter's".into()),
            );
        }
        // Past the geometry ceiling there is no profile; the cache is still arithmetic, and the
        // question "1M" is the one this section exists for.
        if let Ok(profile) = (candidate.build)(512) {
            let per_position = (profile.attn_kv_heads as u64) * (profile.attn_head_dim as u64) * 4 * 2;
            let attention_layers = (0..profile.layer_count)
                .filter(|&l| profile.layer_kind(l) == kaspa_consensus_core::palw_step::PalwLayerKindV1::Attention)
                .count() as u64;
            for n_ctx in [1_048_576u64, 2_097_152] {
                println!(
                    "| {} | {n_ctx} (no profile: past the geometry ceiling) | {} | as above | {} | {} |",
                    candidate.name.split(" (").next().unwrap_or(candidate.name),
                    human(attention_layers * per_position * n_ctx),
                    human(n_ctx * 4),
                    candidate.parameters.map(human).unwrap_or_else(|| "the converter's".into()),
                );
            }
        }
    }
    println!();
}
