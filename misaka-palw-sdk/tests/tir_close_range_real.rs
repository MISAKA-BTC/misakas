//! **RFC-0002 Phase F, `palw_tir_fence2`: the close sizing's range twin on real-size classes** — a
//! probe, not a gate (`#[ignore]`; the corpus differential in consensus-core is the gate).
//!
//! For each model asked (`PALW_RANGE_REAL=all` or a comma list of the names below) at each context
//! asked (`PALW_RANGE_CONTEXTS`, default `512,2048`):
//!
//! * the layout `check-architecture` admits with (64-lane commit tiles, `h_tile` 64, the logits at the
//!   widest carriable divisor of 4,096 lanes, `min_j C_j` halved until the cone work fits) — the one a
//!   registrant declares;
//! * the element twin's work within the 2^26 cap (the DAA-2,000 release), and uncapped when
//!   `PALW_RANGE_UNCAPPED=1` (for the differential); the range twin's work; the bounds of both, which
//!   must be equal commit point by commit point;
//! * admission v10 as a node runs it, under the release's rules and under `palw_tir_fence2`.

use std::path::PathBuf;
use std::time::Instant;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_close_range_v1::palw_tir_worst_closes_range_work_v1;
use kaspa_consensus_core::palw_tir_close_size_v1::{
    PALW_TIR_CLOSE_SIZING_WORK_CAP_V1, PalwTirCloseBoundV1, PalwTirCloseSizingV1, PalwTirParamFormV1, palw_tir_worst_closes_work_v1,
};
use kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1;
use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1;
use misaka_palw_sdk::tir_layout::{
    TirLayoutChoiceV1, tir_class_admission_offline_v1, tir_default_layout_v1, tir_program_with_scheme_v1,
};
use misaka_palw_tir::TirProgramV1;

const ROOT: Hash64 = Hash64::from_bytes([0xA1; 64]);

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// `testnet-12` with `palw_tir_v1` armed (as the offline gate arms it), and `palw_tir_fence2` when
/// `fence2`.
fn network(fence2: bool) -> (Params, PalwConsensusParamsV2) {
    let mut p = palw_t12_shipped_params();
    (kaspa_consensus_core::palw_tir_v1::PALW_T12_TIR_V1_ENTRY.set)(&mut p, Some(ForkActivation::new(1)));
    if fence2 {
        (kaspa_consensus_core::palw_tir_fence2_v1::PALW_T12_TIR_FENCE2_ENTRY.set)(&mut p, Some(ForkActivation::new(1)));
    }
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is a V2 network") };
    let bundle = bundle.clone();
    (p, bundle)
}

fn lowered(config: &str, window: u32) -> Result<TirProgramV1, String> {
    let text = std::fs::read_to_string(config).map_err(|e| format!("{config}: {e}"))?;
    let spec = misaka_palw_tir_lower::parse_config_str(&text).map_err(|e| e.to_string())?;
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).map_err(|e| e.to_string())?;
    let opts = misaka_palw_tir_lower::lower::LowerOpts { max_window: Some(window), ..Default::default() };
    Ok(misaka_palw_tir_lower::lower::lower(&hl, &opts).map_err(|e| e.to_string())?.program)
}

fn a16(g: kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1) -> Result<TirProgramV1, String> {
    let shape = misaka_palw_base0::artifact::Base0ShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        n_kv_heads: g.attn_kv_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        max_position: g.n_ctx as usize,
        ln_theta_gen_q: 0,
        eps_q: g.rms_eps_q,
    };
    misaka_palw_base0::tir_a16::a16_mirror_program(&shape, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL).map_err(|e| e.to_string())
}

fn container(path: &str) -> Result<TirProgramV1, String> {
    let artifact = misaka_palw_tir_exec::node::TirArtifactV1::open(std::path::Path::new(path))?;
    Ok(artifact.container().program.clone())
}

/// The models, by name: how each program is made at a context.
fn model(name: &str, context: u32) -> Result<TirProgramV1, String> {
    let real = |f: &str| repo().join("misaka-palw-tir-lower/tests/configs/real").join(f).display().to_string();
    let ckpt = |f: &str| format!("/Users/wata/Downloads/MISAKA-wt-b/hf-ckpt/{f}/config.json");
    match name {
        "a16-1.5b" => a16(kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B),
        "a16-3b" => a16(kaspa_consensus_core::palw_qwen25_profile::QWEN25_3B),
        "qwen2.5-1.5b" => lowered(&real("qwen2.5-1.5b-instruct.json"), context),
        "qwen3.5-0.8b" => lowered(&ckpt("Qwen/Qwen3.5-0.8B"), context),
        "qwen3.5-2b" => container("/Users/wata/Downloads/MISAKA-wt-b/tir-audit/real-gguf/q4km-class-v2.palwtir"),
        "smollm2-1.7b" => lowered(&ckpt("HuggingFaceTB/SmolLM2-1.7B-Instruct"), context),
        "llama-3.2-1b" => lowered(&real("llama-3.2-1b.json"), context),
        "gemma-3-1b" => lowered(&real("gemma-3-1b-it.json"), context),
        "qwen3-8b" => lowered(&real("qwen3-8b.json"), context),
        // Any real config the lowering's tests carry: `cfg:<file stem>`.
        other if other.starts_with("cfg:") => lowered(&real(&format!("{}.json", &other[4..])), context),
        other => Err(format!("no model {other}")),
    }
}

const MODELS: [&str; 10] = [
    "a16-1.5b",
    "a16-3b",
    "qwen2.5-1.5b",
    "qwen3.5-0.8b",
    "qwen3.5-2b",
    "smollm2-1.7b",
    "llama-3.2-1b",
    "gemma-3-1b",
    "qwen3-8b",
    "a16-dummy",
];

fn class_of(program: &TirProgramV1, layout: &PalwTirLayoutV1) -> PalwTirClassV1 {
    PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: layout.clone(),
        tokenizer_id: Hash64::from_bytes([3; 64]),
    }
}

/// What admission v10's step 9 runs, by one twin: `(bounds or the refusal, work, seconds)`.
fn size(class: &PalwTirClassV1, range: bool, cap: u64) -> (Result<Vec<PalwTirCloseBoundV1>, String>, u64, f64) {
    let space = PalwTirStepSpaceV1::new(class).expect("a step space");
    let longest = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(
        class,
        class.class_id(&ROOT),
        (1, class.layout.max_context),
    )
    .expect("the longest job");
    let inventory = PalwTirInventoryIndexV1::new(&space.program).expect("an inventory");
    let sizing = PalwTirCloseSizingV1 { form: PalwTirParamFormV1::Multiproof, court: true, cap, stop_above: None };
    let t = Instant::now();
    let r = if range {
        palw_tir_worst_closes_range_work_v1(&space, &inventory, &longest, &sizing)
    } else {
        palw_tir_worst_closes_work_v1(&space, &inventory, &longest, &sizing)
    };
    let secs = t.elapsed().as_secs_f64();
    match r {
        Ok((b, w)) => (Ok(b), w, secs),
        Err(e) => (Err(e), cap, secs),
    }
}

/// **What a dissected commit point's worst root claim is made of** (the driver's arithmetic, spec 04b
/// §10.3): over the positions the sizing asks (1, the longest replay, the late alignment), per tile,
/// the finalize and its probes at the history's first row — the step leaves (preimages and paths),
/// the parameters, the token, the claim's values — printed for the tile whose claim is the largest.
fn root_breakdown(class: &PalwTirClassV1, bi: u8, ni: u16) {
    use kaspa_consensus_core::palw_tir_close_range_v1::{
        PalwTirCloseRangeRequestV1, PalwTirRangeCacheV1, PalwTirRangesV1, palw_tir_close_reads_range_split_v1,
    };
    use kaspa_consensus_core::palw_tir_close_size_v1::{PalwTirClosePriceV1, PalwTirCloseReadsV1};
    use misaka_palw_tir::demand::{DemandContext, history_length_v1};
    use std::collections::BTreeMap;
    let space = PalwTirStepSpaceV1::new(class).expect("a step space");
    let longest = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(
        class,
        class.class_id(&ROOT),
        (1, class.layout.max_context),
    )
    .expect("the longest job");
    let inventory = PalwTirInventoryIndexV1::new(&space.program).expect("an inventory");
    let price = PalwTirClosePriceV1::new(&space, &inventory, &longest, PalwTirParamFormV1::Multiproof).expect("a price");
    let block = &space.program.blocks[bi as usize];
    let node = &block.nodes[ni as usize];
    let reductions = kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_cone_reductions_v1(block, ni);
    let tile_len = space.commit_tile_len(bi, ni).expect("a commit point") as usize;
    let p_max = class.layout.max_context - 1;
    let occ = space.occurrences().iter().position(|(b, _)| *b == bi).expect("an occurrence") as u16;
    let mut cache = PalwTirRangeCacheV1::default();
    let mut read = |pos: u32, ranges: &PalwTirRangesV1, supplied: &[u16], target: u16, range: Option<(usize, usize)>| {
        let split = palw_tir_close_reads_range_split_v1(
            &space,
            &longest,
            &inventory,
            &mut cache,
            &PalwTirCloseRangeRequestV1 { ctx: DemandContext { pos, occurrence: occ }, target, ranges, supplied, range, both: false },
            u64::MAX,
            false,
            false,
        )
        .expect("a read");
        let mut r = split.reads;
        r.merge(&split.hist);
        (r, split.supplied)
    };
    let mut worst: Option<(u64, String)> = None;
    for pos in [1u32, p_max / 2, p_max] {
        let h = history_length_v1(&space.info, bi, pos).expect("info");
        let count: usize = node.out.resolve(h).iter().product();
        for first in (0..count).step_by(tile_len) {
            let tile = PalwTirRangesV1::single(first, (first + tile_len).min(count));
            let supplied: Vec<u16> = reductions.iter().copied().filter(|r| *r != ni).collect();
            let (finalize, finalize_supplied) = read(pos, &tile, &supplied, ni, None);
            let mut closure: BTreeMap<u16, PalwTirRangesV1> = finalize_supplied;
            if reductions.contains(&ni) {
                let own = closure.remove(&ni).unwrap_or_default().union(&tile);
                closure.insert(ni, own);
            }
            let mut root: PalwTirCloseReadsV1 = finalize.clone();
            let mut probed: BTreeMap<u16, PalwTirRangesV1> = BTreeMap::new();
            loop {
                let mut pending: BTreeMap<u16, PalwTirRangesV1> = BTreeMap::new();
                for (n, es) in &closure {
                    let seen = probed.entry(*n).or_default();
                    let new = es.minus(seen);
                    if !new.is_empty() {
                        *seen = seen.union(&new);
                        pending.insert(*n, new);
                    }
                }
                if pending.is_empty() {
                    break;
                }
                for (r, es) in pending {
                    let others: Vec<u16> = reductions.iter().copied().filter(|x| *x != r).collect();
                    let (probe, probe_supplied) = read(pos, &es, &others, r, Some((0, 1)));
                    for (n, e2) in probe_supplied {
                        let merged = closure.remove(&n).unwrap_or_default().union(&e2);
                        closure.insert(n, merged);
                    }
                    root.merge(&probe);
                }
            }
            let values: u64 = closure.values().map(|s| s.elements()).sum();
            let (steps, params) = (price.steps(&root, false), price.params(&root));
            let token = if root.token { price.units(&root, false) - steps - params } else { 0 };
            let total = price.root_frame(tile_len as u32, reductions.len()) + steps + params + token + values * 20;
            if worst.as_ref().is_none_or(|(w, _)| total > *w) {
                let hist_leaves = root.loose_steps.len();
                worst = Some((
                    total,
                    format!(
                        "pos {pos} tile {first}: frame {} + step leaves {} ({} leaves, {} hypothetical; widest {} lanes) + params {} ({} leaves) + token {token} + values {values} x 20 = {total} (x2 past the first row-parts, + slack)",
                        price.root_frame(tile_len as u32, reductions.len()),
                        steps,
                        root.steps.len(),
                        hist_leaves,
                        root.steps.values().max().copied().unwrap_or(0),
                        params,
                        root.params.len()
                    ),
                ));
            }
        }
    }
    if let Some((_, what)) = worst {
        eprintln!("  root claim of block {bi} node {ni}: {what}");
    }
}

fn pct(work: u64) -> f64 {
    100.0 * work as f64 / PALW_TIR_CLOSE_SIZING_WORK_CAP_V1 as f64
}

/// Is a refusal one a narrower checkpoint interval can fix?
fn interval_fixable(why: &str) -> bool {
    why.contains("cone evaluation work") || why.contains("close sizing work") || why.contains("close bytes")
}

/// The layout a registrant declares under `params`: the widest logits tile first, `C` halved on a
/// refusal a narrower interval fixes. `(layout, verdict, admissions asked)`.
fn declare(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    program: &TirProgramV1,
    context: u32,
    h_chunk: u32,
) -> (Option<PalwTirLayoutV1>, Result<(), String>, u32) {
    let mut first: Option<(PalwTirLayoutV1, Result<(), String>)> = None;
    let mut asked = 0u32;
    // Without a `Fixed` state the interval changes no close: only the logits tile does.
    let recurrent = program.states.iter().any(|s| matches!(s.kind, misaka_palw_tir::program::StateKind::Fixed { .. }));
    let tile_len = std::env::var("PALW_RANGE_TILE").ok().and_then(|t| t.parse().ok()).unwrap_or(64u32);
    for logits in [4096u32, 2048, 1024, 512, 256, 128, 64] {
        let choice =
            TirLayoutChoiceV1 { max_context: Some(context), logits_tile: Some(logits), h_chunk, tile_len, ..Default::default() };
        let mut layout = match tir_default_layout_v1(params, program, &choice) {
            Ok(l) => l,
            Err(e) => return (None, Err(e), asked),
        };
        loop {
            asked += 1;
            let verdict = tir_class_admission_offline_v1(params, bundle, &class_of(program, &layout), ROOT);
            first.get_or_insert_with(|| (layout.clone(), verdict.clone()));
            match verdict {
                Ok(()) => return (Some(layout), Ok(()), asked),
                Err(why)
                    if interval_fixable(&why) && layout.checkpoint_interval > 1 && (recurrent || !why.contains("close bytes")) =>
                {
                    layout.checkpoint_interval /= 2
                }
                // A close too wide to carry, or a tile past the terminal's MACs: the logits tile's to fix.
                Err(why) if why.contains("close bytes") || why.contains("tile multiply-accumulates") => break,
                Err(why) => {
                    if !interval_fixable(&why) {
                        return (Some(layout), Err(why), asked);
                    }
                    break;
                }
            }
        }
    }
    let (l, v) = first.expect("a layout was tried");
    (Some(l), v, asked)
}

#[test]
#[ignore]
fn the_range_twin_on_real_size_classes() {
    let Ok(which) = std::env::var("PALW_RANGE_REAL") else {
        eprintln!("set PALW_RANGE_REAL=all or a comma list of {MODELS:?}");
        return;
    };
    let names: Vec<String> = if which == "all" {
        MODELS[..9].iter().map(|s| s.to_string()).collect()
    } else {
        which.split(',').map(|s| s.to_string()).collect()
    };
    let contexts: Vec<u32> = std::env::var("PALW_RANGE_CONTEXTS")
        .unwrap_or_else(|_| "512,2048".into())
        .split(',')
        .map(|s| s.parse().expect("a context"))
        .collect();
    let uncapped = std::env::var("PALW_RANGE_UNCAPPED").is_ok_and(|v| v == "1");
    let h_chunks: Vec<u32> =
        std::env::var("PALW_RANGE_HTILE").unwrap_or_else(|_| "64".into()).split(',').map(|s| s.parse().expect("an h_tile")).collect();
    let (release, release_bundle) = network(false);
    let (fence2, fence2_bundle) = network(true);
    for name in &names {
        for (&context, &h_chunk) in contexts.iter().flat_map(|c| h_chunks.iter().map(move |h| (c, h))) {
            let program = match model(name, context).and_then(|p| tir_program_with_scheme_v1(&p, None)) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{name} @ {context}: not lowered: {e}");
                    continue;
                }
            };
            let t = Instant::now();
            let (layout, verdict_f2, asked) = declare(&fence2, &fence2_bundle, &program, context, h_chunk);
            let Some(layout) = layout else {
                eprintln!("{name} @ {context}: no layout: {verdict_f2:?}");
                continue;
            };
            eprintln!(
                "{name} @ {context}: declared under fence2 in {asked} admissions ({:.1}s): C = {}, logits tile {:?}, h_tile {} -> {verdict_f2:?}",
                t.elapsed().as_secs_f64(),
                layout.checkpoint_interval,
                layout.commit_tiles.iter().max(),
                layout.h_tile
            );
            let class = class_of(&program, &layout);
            let verdict_release = tir_class_admission_offline_v1(&release, &release_bundle, &class, ROOT);
            let (range, rw, rs) = size(&class, true, u64::MAX);
            let (element, ew, es) = size(&class, false, if uncapped { u64::MAX } else { PALW_TIR_CLOSE_SIZING_WORK_CAP_V1 });
            let equal = match (&element, &range) {
                (Ok(a), Ok(b)) => {
                    assert_eq!(a, b, "{name} @ {context}: the twins bound differently");
                    "equal"
                }
                (Err(_), Ok(_)) => "element over its cap",
                _ => "range refused",
            };
            let worst = range.as_ref().ok().and_then(|b| b.iter().max_by_key(|x| x.close_bytes).cloned());
            let worst_root =
                range.as_ref().ok().and_then(|b| b.iter().filter(|x| x.dissected).max_by_key(|x| x.root_claim_bytes).cloned());
            if std::env::var("PALW_RANGE_ROOT").is_ok_and(|v| v == "1")
                && let Some(w) = &worst_root
            {
                root_breakdown(&class, w.block, w.node);
            }
            eprintln!(
                "{name} @ {context}: element {ew} steps ({:.1}% of the cap, {es:.1}s){} | range {rw} steps ({:.2}%, {rs:.1}s) | bounds {equal}",
                pct(ew),
                if element.is_err() { " — over" } else { "" },
                pct(rw)
            );
            eprintln!(
                "{name} @ {context}: worst close {:?} B, worst root claim {:?} B | release: {verdict_release:?} | fence2: {verdict_f2:?}",
                worst.map(|w| (w.block, w.node, w.close_bytes)),
                worst_root.map(|w| (w.block, w.node, w.root_claim_bytes))
            );
        }
    }
}
