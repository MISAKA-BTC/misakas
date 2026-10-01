//! **Declaring an IR class's layout** (RFC-0002 Phase F, F6's operator surface).
//!
//! The lowerer's converter (`palw-tir-fidelity --artifact-out`) writes a `PALWTIR1` container with
//! the program and its integer tensors and no layout, and a container without a layout is no class
//! yet: an IR class is its program under a declared layout (the step space's tiles, the history
//! tile, the checkpoint interval and the context), so one lowered artifact can be registered at more
//! than one layout. This module chooses the layout and writes it into the container
//! (`palw-class declare-layout`), so the path from a Hugging Face checkpoint to a registration is
//! `check-architecture → palw-tir-fidelity → declare-layout → preflight → register`.
//!
//! The layout is the one `check-architecture`'s IR mode admits with: every commit point tiled at
//! `tile_len` values (the logits node at the tiled scheme's 4,096 lanes under that scheme), every
//! state tiled at `tile_len`, the history in `h_chunk` rows, the context given (or the widest the
//! program and the network admit). Its checkpoint interval is the widest admission v10 accepts:
//! `tir_admit_v1`'s `min_j C_j` at those tiles, halved until the gate the node's registration runs
//! (`palw_tir_registration_preflight_at_v1`, at the fence's height) admits the class, so the
//! declared layout is registrable by construction or the refusal is named.

use std::path::Path;

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_step_refute::{PALW_LOGITS_TILE_LANES, tiled_logits_scheme_id_v1};
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1,
};
use kaspa_hashes::Hash64;
use misaka_palw_tir::TirProgramV1;

use crate::check_architecture::{IR_DEFAULT_H_CHUNK_V1, IR_DEFAULT_TILE_LEN_V1, tir_admit_inputs_v1, tir_ceilings_v1};

/// How the class commits its logits trace: the court's two schemes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirLogitsSchemeV1 {
    /// Every row whole (`flat_logits_scheme_id_v1`): the decode door carries every row.
    Flat,
    /// Rows in 4,096-lane tiles (`tiled_logits_scheme_id_v1`): a door carries one tile.
    Tiled,
}

impl TirLogitsSchemeV1 {
    pub fn id(self) -> Hash64 {
        match self {
            Self::Flat => kaspa_consensus_core::palw_step_refute::flat_logits_scheme_id_v1(),
            Self::Tiled => tiled_logits_scheme_id_v1(),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "flat" => Some(Self::Flat),
            "tiled" => Some(Self::Tiled),
            _ => None,
        }
    }
}

/// What an operator chooses; everything else is derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirLayoutChoiceV1 {
    /// The class's context in positions; `None` for the widest the program and the network admit.
    pub max_context: Option<u32>,
    pub tile_len: u32,
    pub h_chunk: u32,
    /// The logits scheme the class commits under. `None` keeps the program's, and chooses the tiled
    /// scheme for a program that names none — the lowerer leaves the field zero, and a class under
    /// no scheme has no decode court (admission v10 refuses it by name).
    pub logits_scheme: Option<TirLogitsSchemeV1>,
    /// The logits node's tile under the tiled scheme: a divisor of the scheme's 4,096 lanes.
    /// `None` searches from 4,096 down for the widest one whose terminal close the chain can carry
    /// (PALW-TIR-38: 4,096 weight rows of a 1,536-wide head are 6.3 MB against testnet-12's 3.2 MB).
    pub logits_tile: Option<u32>,
}

impl Default for TirLayoutChoiceV1 {
    fn default() -> Self {
        Self {
            max_context: None,
            tile_len: IR_DEFAULT_TILE_LEN_V1,
            h_chunk: IR_DEFAULT_H_CHUNK_V1,
            logits_scheme: None,
            logits_tile: None,
        }
    }
}

/// **`program` under the logits scheme `choice` names** — re-encoded canonically; the params and so
/// the inventory root are untouched (the root covers the declarations' bytes, never the scheme).
pub fn tir_program_with_scheme_v1(program: &TirProgramV1, choice: Option<TirLogitsSchemeV1>) -> Result<TirProgramV1, String> {
    let named = Hash64::from_bytes(program.logits_scheme_id);
    let scheme = match choice {
        Some(s) => s.id(),
        None if named != Hash64::from_bytes([0; 64]) => return Ok(program.clone()),
        None => TirLogitsSchemeV1::Tiled.id(),
    };
    let mut p = program.clone();
    p.logits_scheme_id.copy_from_slice(scheme.as_byte_slice());
    TirProgramV1::decode_canonical(&p.encode()).map_err(|e| format!("the program under the scheme does not decode: {e}"))
}

/// **The layout `program` is tiled at**, with the checkpoint interval `tir_admit_v1` derives at those
/// tiles under the network's ceilings (`min_j C_j`) — before the registration gate has narrowed it.
pub fn tir_default_layout_v1(params: &Params, program: &TirProgramV1, choice: &TirLayoutChoiceV1) -> Result<PalwTirLayoutV1, String> {
    let (ceilings, _, _) = tir_ceilings_v1(params);
    let widest = program.history_bound.min(ceilings.max_context);
    let max_context = choice.max_context.unwrap_or(widest);
    if max_context == 0 || max_context > widest {
        return Err(format!("a context of {max_context} positions: the program's history bound and the network admit 1..={widest}"));
    }
    let tiled = Hash64::from_bytes(program.logits_scheme_id) == tiled_logits_scheme_id_v1();
    let mut commit_tiles = Vec::new();
    for (bi, block) in program.blocks.iter().enumerate() {
        for (ni, node) in block.nodes.iter().enumerate() {
            if node.commit {
                let logits = bi == program.schedule.post as usize && ni == program.logits as usize;
                commit_tiles.push(if logits && tiled {
                    choice.logits_tile.unwrap_or(PALW_LOGITS_TILE_LANES as u32)
                } else {
                    choice.tile_len
                });
            }
        }
    }
    let inputs = tir_admit_inputs_v1(&ceilings, choice.tile_len, choice.h_chunk);
    let admitted = misaka_palw_tir::admit::tir_admit_v1(&program.encode(), &inputs).map_err(|e| format!("tir_admit_v1: {e}"))?;
    Ok(PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context,
        checkpoint_interval: admitted.checkpoint_interval.max(1),
        h_tile: choice.h_chunk,
        commit_tiles,
        state_tiles: program.states.iter().map(|_| choice.tile_len).collect(),
    })
}

/// **What the logits node's terminal close weighs as carried** at `logits_tile` lanes, estimated from
/// the program and the inventory: `tir_admit_v1`'s opened bytes for the worst tile, the close frame,
/// and — which that count leaves out — the Merkle path every opened parameter piece rides with (the
/// inventory's depth × 64 bytes; a head of `token_bound` rows opens `logits_tile` of them, every
/// other parameter all of its rows). `None` where the program has no logits cone to measure. What
/// PALW-TIR-38 holds a tile to (`palw_tir_carriable_close_bytes_v1`): at 2,048 lanes the 1.5B A16
/// head's close is ≈ 5.9 MB against testnet-12's 3.2 MB.
pub fn tir_logits_close_carried_estimate_v1(
    params: &Params,
    program: &TirProgramV1,
    leaf_count: u32,
    logits_tile: u32,
    h_chunk: u32,
) -> Option<u64> {
    use misaka_palw_tir::admit::LeafV1;
    let (ceilings, _, _) = tir_ceilings_v1(params);
    let admitted =
        misaka_palw_tir::admit::tir_admit_v1(&program.encode(), &tir_admit_inputs_v1(&ceilings, logits_tile, h_chunk)).ok()?;
    let cone = admitted.cones.iter().find(|c| c.block == program.schedule.post && c.node == program.logits)?;
    let depth = u64::from(u32::BITS - leaf_count.max(2).saturating_sub(1).leading_zeros());
    let path_bytes = depth * 64;
    let piece = kaspa_consensus_core::palw_tir_artifact_v1::PALW_TIR_ROW_PIECE_BYTES_V1;
    let mut pieces = 0u64;
    for leaf in &cone.leaves {
        let LeafV1::Param(j) = leaf else { continue };
        let d = &program.params[*j as usize];
        let rows = u64::from(d.shape.first().copied().unwrap_or(1));
        let row_bytes = d.shape.iter().skip(1).map(|x| u64::from(*x)).product::<u64>() * d.dtype.width() as u64;
        let opened_rows = if rows == u64::from(program.token_bound) { u64::from(logits_tile).min(rows) } else { rows };
        pieces = pieces.saturating_add(opened_rows.saturating_mul(row_bytes.div_ceil(piece).max(1)));
    }
    Some(
        kaspa_consensus_core::palw_tir_admission_v1::PALW_TIR_CLOSE_FRAME_BYTES_V1
            .saturating_add(cone.tile_opened_bytes)
            .saturating_add(pieces.saturating_mul(path_bytes)),
    )
}

/// What [`tir_declare_layout_v1`] wrote.
#[derive(Clone, Debug)]
pub struct TirDeclaredLayoutV1 {
    pub layout: PalwTirLayoutV1,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    /// The gate's verdict ([`tir_class_admission_offline_v1`]): `Ok` when admission v10 admits the
    /// class, else the refusal at the widest interval.
    pub admission: Result<(), String>,
    /// Where the gate was asked ([`TirOfflineGateV1::note`]).
    pub admission_at: String,
    /// The file digest of the container written.
    pub file_digest: [u8; 64],
}

/// **The ruleset an offline admission is asked under**: the network's own where `palw_tir_v1` is
/// armed on it (at the fence's height), else — on testnet-12, whose IR fence is a post-launch flag
/// day not yet scheduled — the same ruleset with the fence armed at DAA 1, so a layout is judged by
/// the gate it will meet rather than refused for the fence alone.
pub struct TirOfflineGateV1 {
    pub params: Params,
    pub daa: u64,
    pub hypothetical: bool,
}

impl TirOfflineGateV1 {
    pub fn of(params: &Params) -> Self {
        if let Some(fence) = params.palw_tir_v1_fence() {
            return Self { params: params.clone(), daa: fence.activation.daa_score(), hypothetical: false };
        }
        let mut armed = params.clone();
        if armed.net == kaspa_consensus_core::config::drill::palw_drill_network_v1() {
            (kaspa_consensus_core::palw_tir_v1::PALW_T12_TIR_V1_ENTRY.set)(
                &mut armed,
                Some(kaspa_consensus_core::config::params::ForkActivation::new(1)),
            );
            return Self { params: armed, daa: 1, hypothetical: true };
        }
        Self { params: armed, daa: 0, hypothetical: false }
    }

    pub fn note(&self) -> String {
        if self.hypothetical {
            "as if palw_tir_v1 were armed (it is dormant on this network: a post-launch flag day)".to_string()
        } else {
            format!("at DAA {} (palw_tir_v1 on this network)", self.daa)
        }
    }
}

/// **The registration gate a node would run, asked of `class` offline** — admission v10 under
/// [`TirOfflineGateV1`], at the network's genesis pricing, weightless. `Ok(())` or the refusal by
/// code.
pub fn tir_class_admission_offline_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    class: &PalwTirClassV1,
    artifact_root: Hash64,
) -> Result<(), String> {
    let gate = TirOfflineGateV1::of(params);
    let (params, at) = (&gate.params, gate.daa);
    let class_id = class.class_id(&artifact_root);
    let program = class.decode_program().map_err(|e| format!("the program does not decode: {e}"))?;
    let canonical = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_attempt_canonical_v1(class)
        .ok_or_else(|| format!("a context of {} positions is too narrow for a canonical job", class.layout.max_context))?;
    let facts = kaspa_consensus_core::palw_tir_attempt_v1::PalwTirJobFactsV1::of(class, &program, class_id);
    let job = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_job_context_v1(&facts, canonical);
    let bond = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        kaspa_consensus_core::tx::TransactionId::from_bytes([0; 64]),
        0,
    ));
    let object = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_post_genesis_registration_v1(
        class.clone(),
        job,
        artifact_root,
        0,
        u128::MAX,
        1,
        0,
        bond,
        Vec::new(),
        bundle.court.max_step_leaf_count(),
    )
    .map_err(|e| format!("{} ({e})", e.code()))?;
    crate::tir_registration::tir_registration_preflight_v1(params, bundle, &object, at, &[])
}

/// **The calibration-length rule** (tir/lower's `fidelity::check_calibration_length`, freeze-v1):
/// a recurrent program — one with any `Fixed` state (Mamba, Jamba, the Qwen hybrids, RWKV) — drifts
/// past the longest sequence it was calibrated on, so it is declared at a context no longer than the
/// `calibrated_context` its converter recorded, unless the conversion waived the rule
/// (`--allow-short-calibration`, recorded as `calibration_length_rule.rule = "waived"`). An attention-only program is not
/// bound. A recurrent artifact that records no calibration length is refused: its context cannot be
/// shown to be covered.
pub fn tir_calibration_covers_context_v1(program: &TirProgramV1, meta: &serde_json::Value, max_context: u32) -> Result<(), String> {
    let recurrent = program.states.iter().any(|s| matches!(s.kind, misaka_palw_tir::program::StateKind::Fixed { .. }));
    if !recurrent {
        return Ok(());
    }
    // palw-tir-fidelity's record of the rule (`calibration_length_rule`: `met`, `not recurrent` or
    // `waived`); an artifact written before it carries none, which is not a waiver.
    if meta.get("calibration_length_rule").and_then(|r| r.get("rule")).and_then(|r| r.as_str()) == Some("waived") {
        return Ok(());
    }
    match meta.get("calibrated_context").and_then(|c| c.as_u64()) {
        Some(calibrated) if u64::from(max_context) <= calibrated => Ok(()),
        Some(calibrated) => Err(format!(
            "a recurrent program calibrated on sequences of at most {calibrated} positions is not declared at a context of \
             {max_context}: its fixed state drifts past its calibration (calibrate on one sequence as long as the context, or \
             pass a --max-context of at most {calibrated})"
        )),
        None => Err("a recurrent program whose artifact records no calibrated context: the calibration-length rule cannot be shown \
             (convert it with palw-tir-fidelity, which records calibrated_context)"
            .to_string()),
    }
}

/// **The lowering window covers the context**: a program lowered with `--max-window W` (the converter
/// records `max_window` in its provenance) attends to at most the last `W` positions, which is the
/// model only for jobs of at most `W` positions — so it is declared at no longer a context. A
/// program lowered without one (no `max_window`, or `null`) keeps the model's own windows.
pub fn tir_window_covers_context_v1(meta: &serde_json::Value, max_context: u32) -> Result<(), String> {
    match meta.get("max_window").and_then(|w| w.as_u64()) {
        Some(window) if u64::from(max_context) > window => Err(format!(
            "the program was lowered for a window of {window} positions (--max-window) and is not declared at a context \
             of {max_context}: past its window it attends to fewer positions than the model does"
        )),
        _ => Ok(()),
    }
}

/// **The layout chosen for a program, and what the registration gate says of it** — a layout [`TirLayoutChoiceV1`]
/// derives (the logits tile searched, the checkpoint interval halved from `min_j C_j`) until admission v10
/// admits the class, or the widest one's refusal where none is. `declare-layout` writes it into a container;
/// the model preflight asks it of a program with no weights (the artifact root a placeholder, the leaf count an
/// estimate: neither enters the gate's verdict beyond the close's path length).
#[derive(Clone, Debug)]
pub struct TirChosenLayoutV1 {
    pub layout: PalwTirLayoutV1,
    pub admission: Result<(), String>,
}

pub fn tir_choose_layout_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    program: &TirProgramV1,
    tokenizer_id: Hash64,
    artifact_root: Hash64,
    leaf_count: u32,
    choice: &TirLayoutChoiceV1,
) -> Result<TirChosenLayoutV1, String> {
    let program_bytes = program.encode();
    let carriable = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1(&bundle.court);
    let class_of = |layout: &PalwTirLayoutV1| PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program_bytes.clone(),
        layout: layout.clone(),
        tokenizer_id,
    };
    // The logits tiles to try: the one asked for, else — under the tiled scheme — every divisor of
    // the scheme's 4,096 lanes, widest first, until the terminal close is one the chain can carry.
    let tiled = Hash64::from_bytes(program.logits_scheme_id) == tiled_logits_scheme_id_v1();
    let logits_tiles: Vec<Option<u32>> = match (choice.logits_tile, tiled) {
        (Some(t), _) => vec![Some(t)],
        (None, true) => (2..=12).rev().map(|k| Some(1u32 << k)).collect(),
        (None, false) => vec![None],
    };
    let mut first: Option<(PalwTirLayoutV1, Result<(), String>)> = None;
    let mut found: Option<(PalwTirLayoutV1, Result<(), String>)> = None;
    'tiles: for logits_tile in logits_tiles {
        // PALW-TIR-38 with the paths counted: a tile whose close the chain cannot carry is never
        // declared, whatever the admission's opened-bytes count says.
        if let Some(t) = logits_tile
            && choice.logits_tile.is_none()
            && tir_logits_close_carried_estimate_v1(params, program, leaf_count, t, choice.h_chunk).is_some_and(|est| est > carriable)
        {
            continue;
        }
        let mut layout = tir_default_layout_v1(params, program, &TirLayoutChoiceV1 { logits_tile, ..*choice })?;
        let mut admission = tir_class_admission_offline_v1(params, bundle, &class_of(&layout), artifact_root);
        first.get_or_insert_with(|| (layout.clone(), admission.clone()));
        while let Err(why) = &admission {
            // A close too wide to carry is the logits tile's to fix; anything else, the interval's.
            if why.contains("close bytes") {
                continue 'tiles;
            }
            if layout.checkpoint_interval <= 1 {
                break;
            }
            layout.checkpoint_interval /= 2;
            admission = tir_class_admission_offline_v1(params, bundle, &class_of(&layout), artifact_root);
        }
        if admission.is_ok() {
            found = Some((layout, admission));
            break;
        }
    }
    // None admitted: keep the widest, whose refusal is the one an operator acts on.
    let (layout, admission) = found.or(first).ok_or("no layout to try")?;
    Ok(TirChosenLayoutV1 { layout, admission })
}

/// **Write `input`'s program and tensors to `output` as a class** — under the logits scheme `choice`
/// names ([`tir_program_with_scheme_v1`]) and a declared layout: `choice` tiled, the logits at the
/// widest divisor of 4,096 lanes whose terminal close the chain can carry (PALW-TIR-38), its
/// checkpoint interval the widest admission v10 accepts (halved from `min_j C_j` down to 1) — with
/// `model_id` recorded in the container's provenance when given. The inventory root is the input's (the scheme
/// and the layout enter the class id, never the root).
pub fn tir_declare_layout_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    input: &Path,
    output: &Path,
    choice: &TirLayoutChoiceV1,
    model_id: Option<&str>,
) -> Result<TirDeclaredLayoutV1, String> {
    let artifact = misaka_palw_tir_exec::node::TirArtifactV1::open(input)?;
    let container = artifact.container();
    let program = &tir_program_with_scheme_v1(&container.program, choice.logits_scheme)?;
    let (artifact_root, leaf_count) = artifact.inventory_root()?;
    let chosen = tir_choose_layout_v1(
        params,
        bundle,
        program,
        Hash64::from_bytes(container.header.tokenizer_id),
        artifact_root,
        leaf_count,
        choice,
    )?;
    let (layout, admission) = (chosen.layout, chosen.admission);
    let class_of = |layout: &PalwTirLayoutV1| PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: layout.clone(),
        tokenizer_id: Hash64::from_bytes(container.header.tokenizer_id),
    };
    let mut meta = serde_json::from_str::<serde_json::Value>(&container.header.meta).unwrap_or_else(|_| serde_json::json!({}));
    tir_calibration_covers_context_v1(program, &meta, layout.max_context)?;
    tir_window_covers_context_v1(&meta, layout.max_context)?;
    if !meta.is_object() {
        meta = serde_json::json!({ "provenance": meta });
    }
    if let Some(id) = model_id {
        meta["model_id"] = serde_json::Value::String(id.to_string());
    }
    let layout_bytes = borsh::to_vec(&layout).map_err(|e| format!("the layout does not serialize: {e}"))?;
    if input == output {
        return Err("write the declared artifact to another path than the input (the input is mapped while it is read)".into());
    }
    let file_digest = misaka_palw_tir_artifact::write_container_v1(
        output,
        program,
        layout_bytes,
        container.header.tokenizer_id,
        meta.to_string(),
        &mut |j, l| container.read_tensor_bytes(j, l).map_err(|e| e.to_string()),
    )
    .map_err(|e| format!("{}: {e}", output.display()))?;
    let class_id = class_of(&layout).class_id(&artifact_root);
    let admission_at = TirOfflineGateV1::of(params).note();
    Ok(TirDeclaredLayoutV1 { layout, class_id, artifact_root, admission, admission_at, file_digest })
}
