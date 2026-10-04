//! **A vision-chat model as a declared RFC-0003 `Text` class, judged shape-only** (RFC-0002 §II.12.1: a vision-chat model counts
//! for its full input task only when the needed RFC-0003 profile exists).
//!
//! From the wrapper configuration alone (no weights, no repository code): the vision tower (`parse_vision`, a tower adapter that
//! claims the wrapper or the tower's own type) lowered shape-only to its version-2 program; the language model lowered with
//! [`ImageRows`] — the tower's rows placed at the prompt's image placeholder ids, M-RoPE positions for Qwen2/2.5-VL — and lifted to
//! the text stage (`Logits` over `TextStream`) reading the tower's `Final` rows; the two-stage pipeline of
//! `tests/vision.rs::text_stage`; the class (profile `Text`, one image slot) under declared layouts; and **the pipeline admission a
//! node would run** (`palw_gen_registration_preflight_at_v1`) with the `palw_gen_v1` fence's own `Text` ceilings
//! (`PalwGenFenceV1::testnet12_v1`) — **armed hypothetically at the judged height** (the fence is dormant on every network; so is FP
//! Job V5). A verdict here is a measurement of what the dormant fences would carry, never a live shape-ready.
//!
//! Shape-only conventions, recorded in every result: the image slot is 448×448 when the configuration declares no size; the image
//! rows' requantisation unit is 1.0 (a scale changes constants, not the program's structure or cost); the artifact root is a
//! placeholder (the inventory needs the weights).

use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_gen_admission_v1::{palw_gen_post_genesis_registration_v1, palw_gen_registration_preflight_at_v1};
use kaspa_consensus_core::palw_gen_class_v1::{
    PALW_GEN_CLASS_VERSION_V1, PalwGenClassV1, PalwGenImageOfferV1, PalwGenOffersV1, PalwGenProfileOffersV1,
};
use kaspa_consensus_core::palw_gen_v1::{PalwGenFenceV1, PalwGenProfileV1};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_hashes::Hash64;
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::pipeline::{Binding, StageDecl, TirPipelineV1, TripRule};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir_lower::lower::{IMAGE_ROWS_PARAM, ImageRows, LowerOpts};
use serde_json::{Value, json};

use crate::gen_class::{GenLayoutChoiceV1, gen_default_layouts_v1};
use crate::preflight::chain::PreflightNetwork;

/// The image slot when the configuration declares none (a census convention, recorded): 448×448, then 224×224 and 196×196 (the smallest
/// slot a registrant would declare) when the larger is refused.
pub const VLM_PROBE_IMAGE_SIZE_V1: (u32, u32) = (448, 448);
pub const VLM_PROBE_IMAGE_SIZES_V1: [(u32, u32); 3] = [(448, 448), (224, 224), (196, 196)];

/// The two programs and the pipeline of a vision-chat model's `Text` class at `max_context` positions.
pub struct VlmStagesV1 {
    pub tower: TirProgramV2,
    pub text: TirProgramV2,
    pub pipeline: TirPipelineV1,
    pub rows: usize,
    pub size: (u32, u32),
}

/// Build the stages from the wrapper configuration (shape-only).
pub fn vlm_stages_v1(config: &Value, max_context: u32, size: (u32, u32)) -> Result<VlmStagesV1, String> {
    use misaka_palw_tir_lower::{encoder, lower::vision};
    let text = config.to_string();
    // 1. The tower.
    let vspec = vision::parse_vision(&text, Some(size), None).map_err(|e| format!("tower: {e}"))?;
    let (vhl, _) = vision::hl_program(&vspec).map_err(|e| format!("tower hl: {e}"))?;
    let vlw = vision::lower_vision_with(&vhl, &vspec, true).map_err(|e| format!("tower lower: {e}"))?;
    let tower = encoder::vision_v2(&vlw).map_err(|e| format!("tower v2: {e}"))?;
    let out = &tower.blocks[tower.schedule.post as usize].nodes[tower.output.node() as usize].out.shape;
    let (rows, width) = match out.as_slice() {
        [misaka_palw_tir::Dim::Fixed(n), misaka_palw_tir::Dim::Fixed(w)] => (*n as usize, *w as usize),
        s => return Err(format!("the tower's output has shape {s:?}, not [rows, width]")),
    };
    // 2. The language model over the stream, its image rows an input.
    let placeholder = config
        .get("image_token_id")
        .or_else(|| config.get("image_token_index"))
        .and_then(Value::as_u64)
        .ok_or("the configuration names no image placeholder id (image_token_id / image_token_index)")? as u32;
    let mt = config.get("model_type").and_then(Value::as_str).unwrap_or("");
    let mrope = if matches!(mt, "qwen2_vl" | "qwen2_5_vl") {
        let vc = &config["vision_config"];
        let patch = vc.get("patch_size").and_then(Value::as_u64).unwrap_or(14) as u32;
        let merge = vc.get("spatial_merge_size").and_then(Value::as_u64).unwrap_or(2) as u32;
        let (gh, gw) = (size.0 / patch / merge, size.1 / patch / merge);
        if (gh * gw) as usize != rows {
            return Err(format!("the merged grid {gh}x{gw} is not the tower's {rows} rows"));
        }
        Some((gh, gw))
    } else {
        None
    };
    // The language model as the preflight reads it (the VLM adapters: a flat Qwen2-VL configuration, `text_config`, the
    // wrapper's token ids), not the fixture parser.
    let spec = misaka_palw_tir_lower::hf_schema::read_model_with(
        config,
        None,
        &misaka_palw_tir_lower::hf_schema::ReadOptions::default(),
        misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin(),
    )
    .map_err(|f| format!("text spec: {f}"))?
    .spec;
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).map_err(|e| format!("text hl: {e}"))?;
    let img = ImageRows { rows, width, unit: 1.0, placeholder, mrope };
    let opts = LowerOpts { max_window: Some(max_context), image_rows: Some(img), ..LowerOpts::default() };
    let lw = misaka_palw_tir_lower::lower::lower(&hl, &opts).map_err(|e| format!("text lower: {e}"))?;
    let iv = misaka_palw_tir::interval_v2::output_interval_v2(&tower).map_err(|e| format!("the tower's output interval: {e}"))?;
    let text_stage = encoder::lift_v2(
        &lw,
        &[(IMAGE_ROWS_PARAM, misaka_palw_tir::program_v2::InputSource::External { lo: iv.lo as i64, hi: iv.hi as i64 })],
        misaka_palw_tir::program_v2::OutputDecl::Logits { node: lw.program.logits, scheme_id: lw.program.logits_scheme_id },
    )
    .map_err(|e| format!("text v2: {e}"))?;
    // 3. The pipeline.
    let pipeline = TirPipelineV1 {
        version: 1,
        stages: vec![
            StageDecl {
                name: "vision".into(),
                program: 0,
                trip: TripRule::Fixed { n: 1 },
                max_trip: 1,
                tokens: None,
                bind: vec![Binding::JobImage { index: 0 }],
            },
            StageDecl {
                name: "text".into(),
                program: 1,
                trip: TripRule::TextStream,
                max_trip: max_context,
                tokens: None,
                bind: vec![Binding::StageFinal { stage: 0 }],
            },
        ],
        output_stage: 1,
    };
    Ok(VlmStagesV1 { tower, text: text_stage, pipeline, rows, size })
}

/// The class of `stages` under one layout choice.
fn class_of(s: &VlmStagesV1, choice: &GenLayoutChoiceV1, checkpoint: u32, max_context: u32) -> PalwGenClassV1 {
    let programs = [s.tower.clone(), s.text.clone()];
    PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile: PalwGenProfileV1::Text as u8,
        pipeline: s.pipeline.encode(),
        programs: programs.iter().map(|p| p.encode()).collect(),
        layouts: gen_default_layouts_v1(&s.pipeline, &programs, choice, checkpoint),
        output: OutputSpecV1::tokens(max_context),
        offers: PalwGenOffersV1 {
            steps: vec![],
            scalars: vec![],
            max_prompt_tokens: max_context.min(4_096),
            max_negative_tokens: 0,
            images: vec![PalwGenImageOfferV1 { h: s.size.0, w: s.size.1, tile_len: 64, token_equivalents: 1_000_000 }],
            max_source_tokens: 0,
            forced_prompt_prefix: vec![],
            source_token_floor: 0,
            profile: PalwGenProfileOffersV1::None,
        },
        tokenizer_id: Hash64::from_bytes([0; 64]),
    }
}

/// **The pipeline admission of a vision-chat model's `Text` class**, `palw_gen_v1` armed hypothetically at `height` with its
/// testnet-12 ceilings: the first layout admitted (tiles 64/256/1,024, history tiles 64/32/16, checkpoint 64/8/1), else the
/// refusal at the first layout tried.
pub fn vlm_text_class_admission_v1(config: &Value, net: &PreflightNetwork, height: u64, max_context: u32, range_twin: bool) -> Value {
    let run = || -> Result<Value, String> {
        // The declared context first, then the census's narrower ones (a class admitted only at a narrower context is its own
        // stratum, as for the IR classes).
        let mut refusals: Vec<String> = Vec::new();
        let mut contexts = vec![max_context];
        contexts.extend([4_096u32, 2_048].into_iter().filter(|c| *c < max_context));
        for ctx in contexts {
            for size in VLM_PROBE_IMAGE_SIZES_V1 {
                match admit_at_size(config, net, height, ctx, size, range_twin) {
                    Ok(v) => return Ok(v),
                    Err(e) => refusals.push(format!("{ctx}@{}x{}: {}", size.0, size.1, e.chars().take(160).collect::<String>())),
                }
                if refusals.last().is_some_and(|r| r.contains("tower:") || r.contains("text spec") || r.contains("placeholder")) {
                    return Err(refusals.join(" | "));
                }
            }
        }
        Err(refusals.join(" | "))
    };
    let mut v = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => json!({"ok": false, "error": e.chars().take(1500).collect::<String>()}),
        Err(_) => json!({"ok": false, "error": "the probe panicked"}),
    };
    v["gate"] = json!(format!(
        "palw_gen_v1 (testnet12_v1 ceilings) armed hypothetically, judged at DAA {height}; FP Job V5 dormant; shape-only (image 448x448, 224x224, 196x196 when the config declares none, unit 1.0, placeholder root)"
    ));
    v["max_context"] = json!(max_context);
    v["range_twin"] = json!(range_twin);
    v
}

fn admit_at_size(
    config: &Value,
    net: &PreflightNetwork,
    height: u64,
    max_context: u32,
    size: (u32, u32),
    range_twin: bool,
) -> Result<Value, String> {
    {
        let stages = vlm_stages_v1(config, max_context, size)?;
        let mut armed = net.params.clone();
        // Armed at the IR flag day's own height (it needs palw_tir_v1 at or below it, and whatever FP Job V5 schedule the network
        // carries needs it below that), judged at `height`.
        let at = net.params.palw_tir_v1_fence().map(|f| f.activation.daa_score()).unwrap_or(1).max(1).min(height.max(1));
        armed.palw_gen_v1 = Some(PalwGenFenceV1::testnet12_v1(ForkActivation::new(at)));
        armed.sync_palw_gen_v1();
        // The generative range twin (dormant), armed with it when the census asks for it: the same bounds, fewer sizing steps.
        if range_twin {
            armed.palw_gen_range_twin_v1 = Some(ForkActivation::new(at));
            armed.sync_palw_gen_range_twin_v1();
        }
        armed.validate_palw_v2().map_err(|e| format!("palw_gen_v1 cannot be armed hypothetically at DAA {height}: {e}"))?;
        let bond = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
            kaspa_consensus_core::tx::TransactionId::from_bytes([0; 64]),
            0,
        ));
        // A short fixed sequence of layouts (the commit tile, the output tile, the history tile, the checkpoint interval), the
        // way the IR search narrows: the first refusal is kept when none admits.
        const LAYOUTS: [(u32, Option<u32>, u32, u32); 6] = [
            (64, Some(256), 32, 64),
            (16, Some(256), 32, 64),
            (256, Some(256), 32, 64),
            (64, Some(1024), 32, 8),
            (256, Some(4096), 16, 8),
            (64, Some(256), 16, 1),
        ];
        let mut first: Option<String> = None;
        for (tile_len, output_tile, h_chunk, checkpoint) in LAYOUTS {
            let choice = GenLayoutChoiceV1 { tile_len, output_tile, h_chunk, checkpoint_interval: checkpoint };
            let class = class_of(&stages, &choice, checkpoint, max_context);
            let object = palw_gen_post_genesis_registration_v1(
                class,
                Hash64::from_bytes([0; 64]),
                0,
                1 << 100,
                1,
                height,
                bond.clone(),
                Vec::new(),
            )
            .map_err(|e| format!("{} ({e})", e.code()));
            let verdict = object.and_then(|o| {
                palw_gen_registration_preflight_at_v1(&armed, &net.bundle, &o, height).map_err(|e| format!("{} ({e})", e.code()))
            });
            match verdict {
                Ok(a) => {
                    return Ok(json!({
                        "ok": true,
                        "tile_len": tile_len, "output_tile": output_tile, "h_tile": h_chunk, "checkpoint": checkpoint,
                        "max_step_leaves": a.entry.max_step_leaf_count,
                        "tower_rows": stages.rows,
                        "image": [stages.size.0, stages.size.1],
                        "context": max_context,
                    }));
                }
                Err(why) => {
                    if std::env::var_os("PALW_VLM_TRACE").is_some() {
                        eprintln!(
                            "vlm-trace: ctx {max_context} size {size:?} layout ({tile_len},{output_tile:?},{h_chunk},{checkpoint}): {why}"
                        );
                        if (why.contains("close bytes as carried") || why.contains("sizing work")) && range_twin {
                            worst_closes_trace(&stages, &choice, checkpoint, max_context);
                        }
                    }
                    first.get_or_insert(why);
                }
            }
        }
        Err(first.unwrap_or_default())
    }
}

/// Diagnostics (`PALW_VLM_TRACE`): the heaviest closes of a class, by stage and commit point, sized by the range twin uncapped.
fn worst_closes_trace(stages: &VlmStagesV1, choice: &GenLayoutChoiceV1, checkpoint: u32, max_context: u32) {
    let class = class_of(stages, choice, checkpoint, max_context);
    let programs = [stages.tower.clone(), stages.text.clone()];
    let Some(inv) = kaspa_consensus_core::palw_gen_artifact_v1::PalwGenInventoryIndexV1::new(&programs) else { return };
    let sized = kaspa_consensus_core::palw_gen_close_price_v1::palw_gen_worst_closes_of_class_v1(
        &class,
        &stages.pipeline,
        &programs,
        inv.leaf_count(),
        true,
        u64::MAX,
        u64::MAX,
        u64::MAX / 4,
        kaspa_consensus_core::palw_tir_close_range_v1::PalwTirCloseTwinV1::Range,
    );
    match sized {
        Ok((bounds, work)) => {
            let mut all: Vec<(u64, usize, u8, u16, Option<u16>)> = bounds
                .iter()
                .enumerate()
                .flat_map(|(s, st)| st.iter().map(move |b| (b.close_bytes, s, b.block, b.node, b.checkpoint)))
                .collect();
            all.sort_unstable_by(|a, b| b.cmp(a));
            let tables =
                kaspa_consensus_core::palw_gen_close_price_v1::PalwGenClassSizingV1::new(&class, &stages.pipeline, &programs).ok();
            for (bytes, s, b, n, ck) in all.iter().take(8) {
                let p = &programs[*s];
                let node = &p.blocks[*b as usize].nodes[*n as usize];
                eprintln!(
                    "vlm-close: {bytes} B stage {s} block {b} node {n} {:?} out {:?} checkpoint {ck:?}",
                    node.prim, node.out.shape
                );
                // The first tile's reads at position 0, by kind.
                let Some(z) = tables.as_ref().and_then(|t| t.stage(&class, &stages.pipeline, &programs, *s).ok()) else { continue };
                let occ = z.space.occurrences().iter().position(|(blk, _)| *blk == *b).unwrap_or(0) as u16;
                let tile = z.space.commit_tile_len(*b, *n).unwrap_or(64) as usize;
                let ranges = kaspa_consensus_core::palw_tir_close_range_v1::PalwTirRangesV1::single(0, tile);
                let req = kaspa_consensus_core::palw_tir_close_range_v1::PalwTirCloseRangeRequestV1 {
                    ctx: misaka_palw_tir::demand::DemandContext { pos: 0, occurrence: occ },
                    target: *n,
                    ranges: &ranges,
                    supplied: &[],
                    range: None,
                    both: false,
                };
                let mut cache = kaspa_consensus_core::palw_tir_close_range_v1::PalwTirRangeCacheV1::default();
                if let Ok(split) = kaspa_consensus_core::palw_tir_close_range_v1::palw_tir_close_reads_range_split_gen_v1(
                    &z.space,
                    &z.job,
                    &z.inventory,
                    &mut cache,
                    &req,
                    u64::MAX / 4,
                    false,
                    false,
                    Some(&z.model),
                ) {
                    let r = &split.reads;
                    eprintln!(
                        "vlm-close:   tile 0 reads {} step leaves ({} values), {} param leaves, {} wild rows, {} edges, {} image tiles",
                        r.steps.len(),
                        r.steps.values().map(|v| *v as u64).sum::<u64>(),
                        r.params.len(),
                        r.wild_rows.len(),
                        r.edges.len(),
                        r.images.len()
                    );
                }
            }
            eprintln!("vlm-close: work {work}");
        }
        Err(e) => eprintln!("vlm-close: sizing refused: {e}"),
    }
}
