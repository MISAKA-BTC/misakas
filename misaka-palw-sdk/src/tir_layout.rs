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
//! program and the network admit). Its checkpoint interval is the widest the court's cone-work check
//! admits ([`tir_court_checkpoint_interval_v1`]: admission v10's step 5 solved for `C`, a `Fixed`
//! state's elementwise replay counted with its MACs and transcendentals), then asked of the gate the
//! node's registration runs (`palw_tir_registration_preflight_at_v1`, at the fence's height) — so the
//! declared layout is registrable by construction or the refusal is named, at the interval declared.

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
/// `min_j C_j` sizes a state's replay by its MACs and transcendentals alone; what a declared layout
/// uses is [`tir_court_checkpoint_interval_v1`].
pub fn tir_default_layout_v1(params: &Params, program: &TirProgramV1, choice: &TirLayoutChoiceV1) -> Result<PalwTirLayoutV1, String> {
    let (ceilings, _, _) = tir_ceilings_v1(params);
    let inputs = tir_admit_inputs_v1(&ceilings, choice.tile_len, choice.h_chunk);
    let admitted = misaka_palw_tir::admit::tir_admit_v1(&program.encode(), &inputs).map_err(|e| format!("tir_admit_v1: {e}"))?;
    Ok(PalwTirLayoutV1 { checkpoint_interval: admitted.checkpoint_interval.max(1), ..tir_layout_tiles_v1(params, program, choice)? })
}

/// **The widest checkpoint interval a declared layout starts from**: the legacy ceiling's cap on
/// `C_j` (`TirCeilingsV1::legacy_court_v1`, 2^16).
pub const TIR_DECLARED_MAX_CHECKPOINT_INTERVAL_V1: u32 = 1 << 16;

/// The text admission v10's cone-work refusal carries (step 5): exactly bounded by
/// [`tir_court_checkpoint_interval_v1`].
const TIR_CONE_WORK_REFUSAL_V1: &str = "IR cone evaluation work";

/// **The refusals a narrower checkpoint interval can fix** — each grows with the replay a `Fixed`
/// state's checkpoint leaves: the cone-work check (step 5), and PALW-TIR-38's carried closes and the
/// sizing's own work (step 9), which read that replay. Every other refusal — the ladder at the
/// context, the counted pwu, state bytes, the whole-close ceiling — does not shrink with it.
const TIR_INTERVAL_REFUSALS_V1: [&str; 3] = [TIR_CONE_WORK_REFUSAL_V1, "IR close sizing work", "IR terminal close bytes as carried"];

fn interval_bound_refusal(why: &str) -> bool {
    TIR_INTERVAL_REFUSALS_V1.iter().any(|what| why.contains(what))
}

/// The text admission v10's refusal of a dissected cone's root claim carries (step 9).
const TIR_ROOT_CLAIM_REFUSAL_V1: &str = "IR dissection root claim bytes";

/// **The refusals a narrower history tile can fix** (spec 04b §10.3): a dissected cone's root claim
/// probes the history's first row, which the court reads through its complete history tile — `h_tile`
/// rows of every sub-row the probe touches — and the close sizing walks every alignment of the history
/// tiles, so both shrink with `h_tile`. Measured at 512 positions on testnet-12's court: Gemma-3-1B's
/// root claim is 114,604 bytes at `h_tile` 64 and 74,036 at 32 against the 100,000-byte carrier;
/// Qwen3-8B's 109,804 and 85,228; Qwen3.5's 105,797 and 64,837.
const TIR_H_TILE_REFUSALS_V1: [&str; 2] = [TIR_ROOT_CLAIM_REFUSAL_V1, "IR close sizing work"];

fn h_tile_bound_refusal(why: &str) -> bool {
    TIR_H_TILE_REFUSALS_V1.iter().any(|what| why.contains(what))
}

/// **The search declare-layout runs**, over the gate `admit`: the logits tiles in order (the caller
/// has dropped any whose close is estimated past the carriage); for each, the history tile from
/// `h_chunk`, halved while the refusal is one a narrower history tile fixes; at each, the court's
/// checkpoint interval (`interval`), halved while the refusal is one a narrower interval fixes — for a
/// `recurrent` program only: without a `Fixed` state the interval changes no leaf and no close. The
/// first layout admitted is returned, at the widest logits tile, history tile and interval that admit;
/// if none is, the first refusal (the widest logits tile and history tile, at the court's interval),
/// the one an operator acts on.
fn tir_declare_search_v1(
    layout_for: &mut dyn FnMut(Option<u32>, u32) -> Result<PalwTirLayoutV1, String>,
    interval: &mut dyn FnMut(&PalwTirLayoutV1) -> Result<u32, String>,
    admit: &mut dyn FnMut(&PalwTirLayoutV1) -> Result<(), String>,
    logits_tiles: &[Option<u32>],
    h_chunk: u32,
    recurrent: bool,
) -> Result<(PalwTirLayoutV1, Result<(), String>), String> {
    let mut first: Option<(PalwTirLayoutV1, Result<(), String>)> = None;
    for &logits_tile in logits_tiles {
        let mut h_tile = h_chunk.max(1);
        loop {
            let mut layout = layout_for(logits_tile, h_tile)?;
            let mut admission = match interval(&layout) {
                Ok(c) => {
                    layout.checkpoint_interval = c;
                    admit(&layout)
                }
                Err(why) => Err(why),
            };
            let at_bound = (layout.clone(), admission.clone());
            while let Err(why) = &admission {
                if !recurrent || !interval_bound_refusal(why) || layout.checkpoint_interval <= 1 {
                    break;
                }
                layout.checkpoint_interval /= 2;
                admission = admit(&layout);
            }
            let why = match admission {
                Ok(()) => return Ok((layout, Ok(()))),
                Err(why) => why,
            };
            first.get_or_insert(at_bound);
            // A root claim past its carrier, or a sizing past its work, at every interval: the next,
            // narrower history tile at the same logits tile.
            if h_tile_bound_refusal(&why) && h_tile > 1 {
                h_tile /= 2;
                continue;
            }
            // A close too wide to carry is the logits tile's to fix: the next, narrower one.
            break;
        }
    }
    first.ok_or_else(|| "no layout to try".to_string())
}

/// **The layout's tiles, its history chunk and its context** — everything but the checkpoint
/// interval, which is left at [`TIR_DECLARED_MAX_CHECKPOINT_INTERVAL_V1`] for the caller to size.
pub fn tir_layout_tiles_v1(params: &Params, program: &TirProgramV1, choice: &TirLayoutChoiceV1) -> Result<PalwTirLayoutV1, String> {
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
    Ok(PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context,
        checkpoint_interval: TIR_DECLARED_MAX_CHECKPOINT_INTERVAL_V1,
        h_tile: choice.h_chunk,
        commit_tiles,
        state_tiles: program.states.iter().map(|_| choice.tile_len).collect(),
    })
}

/// **The widest checkpoint interval the court's cone-work check admits for `layout`'s tiles** —
/// admission v10's step 5 (`palw_tir_class_admission_v1`) solved for `C`, at the gate
/// [`TirOfflineGateV1`] names. Every commit point's cone, at its own tile length (its terminal chunk
/// under the k-ary court where it reduces over the history), costs its tile's MACs, elementwise and
/// transcendentals plus `(C − 1)` times the per-position replay of every `Fixed` state it reads, and
/// the whole must stay within the court's evaluation limit (`palw_tir_court_limits_v1`, 4 ×
/// `max_terminal_macs`: 2^26 on the frozen court):
///
/// `C = min over cones ⌊(limit − tile_work) / Σ per_position⌋ + 1`, at most
/// [`TIR_DECLARED_MAX_CHECKPOINT_INTERVAL_V1`].
///
/// `tir_admit_v1`'s `min_j C_j` counts a replay's MACs and transcendentals only, so a recurrent state
/// whose update is elementwise work — a convolution window's shift, a gated-delta update — passes it
/// at an interval the court refuses (Qwen3.5: `min_j C_j` 1,024, the conv window 67,584 elementwise a
/// position). `Err` names a cone whose tile alone exceeds the limit: no interval admits it.
pub fn tir_court_checkpoint_interval_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    program: &TirProgramV1,
    layout: &PalwTirLayoutV1,
) -> Result<u32, String> {
    use misaka_palw_tir::admit::{LeafV1, TirAdmitInputsV1, TirCeilingsV1};
    let gate = TirOfflineGateV1::of(params);
    let rules = kaspa_consensus_core::palw_tir_admission_v1::PalwTirAdmissionRulesV1::at(&gate.params, gate.daa);
    let dissects = rules.as_ref().is_some_and(|rules| rules.court.is_some());
    let (ceilings, _, _) = tir_ceilings_v1(&gate.params);
    // Admission v10's own `tir_admit_v1` ceilings: the court's tile bounds are its step 5's, never
    // tir/core's legacy caps.
    let admit = TirCeilingsV1 {
        max_tile_macs: u64::MAX,
        max_tile_transcendentals: u64::MAX,
        max_tile_opened_bytes: u64::MAX,
        max_tile_operands: u64::MAX,
        max_position_macs: ceilings.max_macs_per_position,
        max_position_transcendentals: u64::MAX,
        max_state_bytes: ceilings.max_state_bytes,
        max_step_leaves: u64::MAX,
        max_checkpoint_interval: TIR_DECLARED_MAX_CHECKPOINT_INTERVAL_V1,
        max_cone_work: ceilings.max_cone_work,
    };
    let limits = kaspa_consensus_core::palw_court_v2::palw_tir_court_limits_v1(&bundle.court);
    let limit = limits.max_elements.min(limits.max_terms);
    let bytes = program.encode();
    let mut runs = std::collections::BTreeMap::new();
    for &tile_len in layout.commit_tiles.iter().collect::<std::collections::BTreeSet<_>>() {
        let inputs = TirAdmitInputsV1 { tile_len, h_chunk: layout.h_tile, ceilings: admit };
        let admitted =
            misaka_palw_tir::admit::tir_admit_v1(&bytes, &inputs).map_err(|e| format!("tir_admit_v1 at tile {tile_len}: {e}"))?;
        runs.insert(tile_len, admitted);
    }
    let mut widest = u64::from(TIR_DECLARED_MAX_CHECKPOINT_INTERVAL_V1);
    let mut tiles = layout.commit_tiles.iter();
    for (bi, block) in program.blocks.iter().enumerate() {
        for (ni, node) in block.nodes.iter().enumerate() {
            if !node.commit {
                continue;
            }
            let tile_len = tiles.next().ok_or("the layout names fewer commit tiles than the program has commit points")?;
            let run = &runs[tile_len];
            let cone = run
                .cones
                .iter()
                .find(|c| c.block as usize == bi && c.node as usize == ni)
                .ok_or_else(|| format!("tir_admit_v1 costs no cone for commit point ({bi}, {ni})"))?;
            let tile = if !cone.h_reductions.is_empty() && dissects { cone.terminal() } else { &cone.tile };
            let tile_work = tile.macs.saturating_add(tile.elementwise).saturating_add(tile.transcendentals);
            let per = cone
                .leaves
                .iter()
                .filter_map(|leaf| match leaf {
                    LeafV1::State(j) => run.states.iter().find(|s| s.state == *j),
                    _ => None,
                })
                .map(|s| s.per_position.macs.saturating_add(s.per_position.elementwise).saturating_add(s.per_position.transcendentals))
                .fold(0u64, u64::saturating_add);
            if per == 0 {
                continue;
            }
            if tile_work > limit {
                return Err(format!(
                    "commit point ({bi}, {ni}) at tile {tile_len}: its tile alone costs {tile_work} against the court's                      evaluation limit of {limit}, so no checkpoint interval admits it"
                ));
            }
            widest = widest.min((limit - tile_work) / per + 1);
        }
    }
    Ok(widest.max(1) as u32)
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

/// **Write `input`'s program and tensors to `output` as a class** — under the logits scheme `choice`
/// names ([`tir_program_with_scheme_v1`]) and a declared layout: `choice` tiled, the logits at the
/// widest divisor of 4,096 lanes whose terminal close the chain can carry (PALW-TIR-38), its history
/// tile `choice.h_chunk` halved while a root claim or the sizing's work is refused, its checkpoint
/// interval the widest the court's cone-work check admits ([`tir_court_checkpoint_interval_v1`],
/// stepped down only if the gate still refuses a recurrent class for what a narrower interval fixes;
/// [`tir_declare_search_v1`]) — with `model_id` recorded in the container's provenance when given. A
/// refusal is reported at the widest layout tried, so it names what blocks the class there. The
/// inventory root is the input's (the scheme and the layout enter the class id, never the root).
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
    let program_bytes = program.encode();
    let (artifact_root, leaf_count) = artifact.inventory_root()?;
    let carriable = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1(&bundle.court);
    let class_of = |layout: &PalwTirLayoutV1| PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program_bytes.clone(),
        layout: layout.clone(),
        tokenizer_id: Hash64::from_bytes(container.header.tokenizer_id),
    };
    // The logits tiles to try: the one asked for, else — under the tiled scheme — every divisor of
    // the scheme's 4,096 lanes, widest first, until the terminal close is one the chain can carry.
    let tiled = Hash64::from_bytes(program.logits_scheme_id) == tiled_logits_scheme_id_v1();
    let logits_tiles: Vec<Option<u32>> = match (choice.logits_tile, tiled) {
        (Some(t), _) => vec![Some(t)],
        (None, true) => (2..=12).rev().map(|k| Some(1u32 << k)).collect(),
        (None, false) => vec![None],
    };
    // PALW-TIR-38 with the paths counted: a logits tile whose close the chain cannot carry is never
    // declared, whatever the admission's opened-bytes count says.
    let logits_tiles: Vec<Option<u32>> = logits_tiles
        .into_iter()
        .filter(|t| {
            !(t.is_some()
                && choice.logits_tile.is_none()
                && t.and_then(|t| tir_logits_close_carried_estimate_v1(params, program, leaf_count, t, choice.h_chunk))
                    .is_some_and(|est| est > carriable))
        })
        .collect();
    let recurrent = program.states.iter().any(|s| matches!(s.kind, misaka_palw_tir::program::StateKind::Fixed { .. }));
    // The widest layout the gate admits, else the refusal at the widest one tried.
    let (layout, admission) = tir_declare_search_v1(
        &mut |logits_tile, h_chunk| tir_layout_tiles_v1(params, program, &TirLayoutChoiceV1 { logits_tile, h_chunk, ..*choice }),
        &mut |layout| tir_court_checkpoint_interval_v1(params, bundle, program, layout),
        &mut |layout| tir_class_admission_offline_v1(params, bundle, &class_of(layout), artifact_root),
        &logits_tiles,
        choice.h_chunk,
        recurrent,
    )?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwCourtParamsV2};
    use std::path::PathBuf;
    use std::sync::Arc;

    const CONTEXT: u32 = 32;

    /// A layout of one commit tile and the logits tile, at `h_tile`, the interval left for the search.
    fn stub_layout(logits_tile: Option<u32>, h_tile: u32) -> Result<PalwTirLayoutV1, String> {
        Ok(PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 512,
            checkpoint_interval: TIR_DECLARED_MAX_CHECKPOINT_INTERVAL_V1,
            h_tile,
            commit_tiles: vec![64, logits_tile.unwrap_or(4096)],
            state_tiles: vec![64],
        })
    }

    fn root_claim_refusal(bytes: u64) -> String {
        format!(
            "COURT_COST_EXCEEDS_CEILING (the class's {TIR_ROOT_CLAIM_REFUSAL_V1} of {bytes} exceeds the ruleset's ceiling of 100000)"
        )
    }

    /// **declare-layout halves `h_tile` on a root-claim refusal and halts at the first history tile
    /// that admits** — Gemma-3-1B's shape (spec 04b §10.3: its root claim reads the history's first row
    /// through a whole history tile): refused at 64 for its root claim, admitted at 32, so declared at
    /// 32 — 16 is never asked, the interval is never stepped for it, and the logits tile stays the
    /// widest.
    #[test]
    fn a_root_claim_refusal_halves_h_tile_and_halts_at_the_first_admissible() {
        let mut asked: Vec<(Option<u32>, u32, u32)> = Vec::new();
        let (layout, verdict) = tir_declare_search_v1(
            &mut stub_layout,
            &mut |_| Ok(256),
            &mut |l| {
                asked.push((Some(l.commit_tiles[1]), l.h_tile, l.checkpoint_interval));
                if l.h_tile > 32 { Err(root_claim_refusal(114_604)) } else { Ok(()) }
            },
            &[Some(4096), Some(2048)],
            64,
            true,
        )
        .expect("a layout");
        assert_eq!(verdict, Ok(()));
        assert_eq!((layout.h_tile, layout.checkpoint_interval, layout.commit_tiles[1]), (32, 256, 4096));
        assert_eq!(asked, vec![(Some(4096), 64, 256), (Some(4096), 32, 256)], "halted at the first admissible h_tile");
    }

    /// **A class no history tile admits is refused at the widest**, having asked every history tile
    /// down to 1 once per logits tile (a refusal a narrower tile cannot fix stops the halving: the next
    /// logits tile); a recurrent class's interval is stepped first, a non-recurrent one's never.
    #[test]
    fn the_history_tile_search_stops_at_one_and_reports_the_widest_refusal() {
        let mut asked: Vec<(u32, u32)> = Vec::new();
        let (layout, verdict) = tir_declare_search_v1(
            &mut stub_layout,
            &mut |_| Ok(8),
            &mut |l| {
                asked.push((l.h_tile, l.checkpoint_interval));
                Err(root_claim_refusal(200_000 + u64::from(l.h_tile)))
            },
            &[Some(4096)],
            64,
            false,
        )
        .expect("a verdict");
        assert_eq!(verdict, Err(root_claim_refusal(200_064)), "the widest history tile's refusal");
        assert_eq!(layout.h_tile, 64);
        assert_eq!(asked.iter().map(|a| a.0).collect::<Vec<_>>(), vec![64, 32, 16, 8, 4, 2, 1], "every history tile down to 1, once");
        assert!(asked.iter().all(|a| a.1 == 8), "a non-recurrent class's interval is never stepped");

        // The close sizing's work, at every interval of a recurrent class: the interval first, then h_tile.
        let mut asked: Vec<(u32, u32)> = Vec::new();
        let (layout, verdict) = tir_declare_search_v1(
            &mut stub_layout,
            &mut |_| Ok(4),
            &mut |l| {
                asked.push((l.h_tile, l.checkpoint_interval));
                if l.h_tile > 16 {
                    Err("TIR_EXCEEDS_CEILING (the IR program's IR close sizing work of 67108865 exceeds the ceiling 67108864)".into())
                } else {
                    Ok(())
                }
            },
            &[Some(1024)],
            64,
            true,
        )
        .expect("a layout");
        assert_eq!(verdict, Ok(()));
        assert_eq!((layout.h_tile, layout.checkpoint_interval), (16, 4));
        assert_eq!(asked, vec![(64, 4), (64, 2), (64, 1), (32, 4), (32, 2), (32, 1), (16, 4)]);

        // A refusal no history tile fixes moves to the next logits tile at once.
        let mut asked: Vec<(u32, u32)> = Vec::new();
        let (layout, verdict) = tir_declare_search_v1(
            &mut stub_layout,
            &mut |_| Ok(1),
            &mut |l| {
                asked.push((l.commit_tiles[1], l.h_tile));
                if l.commit_tiles[1] > 1024 {
                    Err("COURT_COST_EXCEEDS_CEILING (the class's IR terminal close bytes as carried of 4000000 exceeds 3200000)"
                        .into())
                } else {
                    Ok(())
                }
            },
            &[Some(4096), Some(2048), Some(1024)],
            64,
            false,
        )
        .expect("a layout");
        assert_eq!((verdict, layout.commit_tiles[1], layout.h_tile), (Ok(()), 1024, 64));
        assert_eq!(asked, vec![(4096, 64), (2048, 64), (1024, 64)]);
    }

    /// The tiny Qwen3.5 hybrid — a convolution window and a gated-delta state in each linear-attention
    /// layer: `Fixed` states whose replay is mostly elementwise work — lowered and written without a
    /// layout; `None` where the fixture is not checked out.
    fn tiny_qwen35(dir: &Path) -> Option<PathBuf> {
        use misaka_palw_tir_lower::float_ref::ParamStore;
        use misaka_palw_tir_lower::float_ref::stream::Resident;
        use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
        use misaka_palw_tir_lower::quant::QuantPolicy;
        use misaka_palw_tir_lower::weights::Checkpoint;
        use misaka_palw_tir_lower::{artifact, fidelity};
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures/hf/qwen3_5");
        if !fixture.join("model.safetensors").exists() {
            return None;
        }
        let config = std::fs::read_to_string(fixture.join("config.json")).unwrap();
        let prep = fidelity::prepare(&config, &LowerOpts { max_window: Some(CONTEXT), ..Default::default() }).expect("lowered");
        let ck = Checkpoint::open(&fixture).unwrap();
        let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap();
        let loader = Resident(Arc::new(params));
        let calib = fidelity::random_sequences(prep.hl.vocab, 2, CONTEXT as usize, 7);
        let quiet = |_: usize, _: usize| {};
        let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).unwrap();
        let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).unwrap();
        let path = dir.join("q35.palwtir");
        let meta = serde_json::json!({ "calibrated_context": CONTEXT, "model_id": "test/qwen3_5-tiny" });
        artifact::write(&path, &prep.lowered.program, &mat.params, [0u8; 64], meta).unwrap();
        Some(path)
    }

    /// `bundle` with the court's terminal MAC ceiling at `macs` — its evaluation limit four times that.
    fn with_terminal_macs(bundle: &PalwConsensusParamsV2, macs: u64) -> PalwConsensusParamsV2 {
        let c = &bundle.court;
        let court = PalwCourtParamsV2::with_cost_ceilings(
            c.max_step_leaf_count(),
            c.turn_deadline_daa(),
            c.terminal_rounds(),
            c.max_close_bytes(),
            macs,
            c.max_operand_count(),
        )
        .and_then(|court| court.with_dissection_arity(c.dissection_arity()))
        .expect("a court");
        PalwConsensusParamsV2 { court, ..bundle.clone() }
    }

    /// **The declared interval is the court's own bound, with a state's elementwise replay counted.**
    /// The tiny Qwen3.5 hybrid, declared on testnet-12 as shipped (the gate at `palw_tir_v1`'s DAA
    /// 2,000), under courts whose evaluation limit shrinks until the interval binds: at the first court
    /// that admits the class at all, the declared interval `C` is admitted and `C + 1` is refused by the
    /// cone-work check — the bound is exact — while `tir_admit_v1`'s `min_j C_j`, which counts a
    /// replay's MACs and transcendentals alone, is refused there by the same check.
    #[test]
    fn the_declared_interval_is_the_court_s_exact_bound_with_elementwise_replay_counted() {
        let dir = std::env::temp_dir().join(format!("palw-sdk-declare-c-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let Some(lowered) = tiny_qwen35(&dir) else {
            eprintln!("the qwen3_5 fixture is missing: skipped");
            return;
        };
        let net = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        let PalwConsensusMode::ConsensusV2(shipped) = &net.palw_consensus_mode else { panic!("testnet-12 is V2") };
        let artifact = misaka_palw_tir_exec::node::TirArtifactV1::open(&lowered).unwrap();
        let program = tir_program_with_scheme_v1(&artifact.container().program, None).unwrap();
        let (root, _) = artifact.inventory_root().unwrap();
        let choice = TirLayoutChoiceV1 { max_context: Some(CONTEXT), ..Default::default() };
        let class_of = |layout: &PalwTirLayoutV1| PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: program.encode(),
            layout: layout.clone(),
            tokenizer_id: Hash64::from_bytes(artifact.container().header.tokenizer_id),
        };
        // A recurrent state whose replay is elementwise: the case `min_j C_j` does not see.
        let run = misaka_palw_tir::admit::tir_admit_v1(
            &program.encode(),
            &tir_admit_inputs_v1(&tir_ceilings_v1(&net).0, choice.tile_len, choice.h_chunk),
        )
        .unwrap();
        assert!(
            run.states.iter().any(|s| s.per_position.elementwise > s.per_position.macs + s.per_position.transcendentals),
            "a Fixed state whose replay is mostly elementwise: {:?}",
            run.states.iter().map(|s| (s.state, s.per_position.macs, s.per_position.elementwise)).collect::<Vec<_>>()
        );
        // As shipped, the class is declared and admitted at the interval the court's bound names.
        let shipped_out = dir.join("q35.shipped.palwtir");
        let declared = tir_declare_layout_v1(&net, shipped, &lowered, &shipped_out, &choice, None).expect("declared");
        assert_eq!(declared.admission, Ok(()), "admitted as shipped");
        assert_eq!(
            declared.layout.checkpoint_interval,
            tir_court_checkpoint_interval_v1(&net, shipped, &program, &declared.layout).unwrap(),
            "declared at the court's bound"
        );
        // Shrink the court until the bound binds: the first court that admits the class at all.
        let mut macs = 1u64;
        let (bundle, layout) = loop {
            assert!(macs < 1 << 40, "some court admits the tiny class");
            let bundle = with_terminal_macs(shipped, macs);
            let out = dir.join(format!("q35.{macs}.palwtir"));
            let d = tir_declare_layout_v1(&net, &bundle, &lowered, &out, &choice, None).expect("declared");
            if d.admission.is_ok() {
                break (bundle, d.layout);
            }
            macs = macs.saturating_mul(2);
        };
        let c = layout.checkpoint_interval;
        assert!(c < TIR_DECLARED_MAX_CHECKPOINT_INTERVAL_V1, "at the tightest admitting court the bound binds: C = {c}");
        assert_eq!(c, tir_court_checkpoint_interval_v1(&net, &bundle, &program, &layout).unwrap());
        let at = |interval: u32| {
            tir_class_admission_offline_v1(
                &net,
                &bundle,
                &class_of(&PalwTirLayoutV1 { checkpoint_interval: interval, ..layout.clone() }),
                root,
            )
        };
        assert_eq!(at(c), Ok(()), "C is admitted");
        let past = at(c + 1).expect_err("C + 1 is refused");
        assert!(past.contains(TIR_CONE_WORK_REFUSAL_V1), "by the cone-work check: {past}");
        // `min_j C_j`, the interval the tool declared before, is past the bound and refused by it.
        let legacy = tir_default_layout_v1(&net, &program, &choice).unwrap().checkpoint_interval;
        eprintln!(
            "terminal MACs {macs}: declared C = {c}, min_j C_j = {legacy}; as shipped C = {}",
            declared.layout.checkpoint_interval
        );
        if legacy > c {
            let refused = at(legacy).expect_err("min_j C_j is refused");
            assert!(refused.contains(TIR_CONE_WORK_REFUSAL_V1), "{refused}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
