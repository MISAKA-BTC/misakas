//! **Declaring a generative class** (RFC-0003 Part II; activation step 7) — the twin of
//! [`crate::tir_layout`] and [`crate::tir_registration`] for pipeline classes.
//!
//! A lowered pipeline (the programs, the pipeline, the integer weights) is no class yet: a class is
//! the pipeline under a declared layout per stage, an output header, the offers a job may choose from
//! and a tokenizer, and its id binds all of them (`PalwGenClassV1::class_id`). This module chooses the
//! layouts ([`gen_default_layouts_v1`]: every commit point and state at `tile_len` lanes, the output
//! node's tile its own — the lanes an output tile and a step tile share, PALW-OUT-3 — the history in
//! `h_chunk` rows, the widest checkpoint interval the gate accepts), builds the class and holds it to
//! the registration gate a node would run (`palw_gen_registration_preflight_at_v1`, under
//! [`GenOfflineGateV1`]) — **the declared layout is registrable by construction or the refusal is
//! named** ([`GenDeclaredV1::admission`]) — and writes the `PALWTIR2` container a worker holds
//! ([`gen_write_declared_container_v1`]).
//!
//! The path from a checkpoint to a registered class is
//! `lower (tir-lower) → declare layout → preflight → register → hold the container`.

use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::palw_gen_admission_v1::{
    PalwGenAdmittedV1, palw_gen_post_genesis_registration_v1, palw_gen_registration_preflight_at_v1,
};
use kaspa_consensus_core::palw_gen_artifact_v1::palw_gen_inventory_root_v1;
use kaspa_consensus_core::palw_gen_class_v1::{
    PALW_GEN_CLASS_VERSION_V1, PalwGenClassRecordV1, PalwGenClassV1, PalwGenOffersV1, palw_gen_class_record_v1,
    palw_gen_class_registration_message_v1,
};
use kaspa_consensus_core::palw_gen_v1::{PALW_DRILL_GEN_V1_ENTRY, PalwGenProfileV1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwRegistrationTermsV2};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_hashes::Hash64;
use misaka_palw_gen::OutputSpecV1;
use misaka_palw_tir::pipeline::{PipelineParams, TirPipelineV1};
use misaka_palw_tir::program_v2::TirProgramV2;

use crate::check_architecture::{IR_DEFAULT_H_CHUNK_V1, IR_DEFAULT_TILE_LEN_V1};
use crate::lineage::{PalwGenClassEntryV1, PalwLoadedArtifactV1};
use crate::lineages::generative::GEN_LINEAGE_ID_V1;

/// What an operator chooses; everything else is derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenLayoutChoiceV1 {
    /// Lanes per commit point's tile and per state tile.
    pub tile_len: u32,
    /// The output node's tile (the lanes an output tile shares with its step tile): `tile_len` when
    /// `None`. Inside the output digest's `[4, 2^16]`; a `Rows` output's row is whole tiles of it.
    pub output_tile: Option<u32>,
    /// History rows per tile.
    pub h_chunk: u32,
    /// The widest checkpoint interval to try; halved until the gate admits the class.
    pub checkpoint_interval: u32,
}

impl Default for GenLayoutChoiceV1 {
    fn default() -> Self {
        Self { tile_len: IR_DEFAULT_TILE_LEN_V1, output_tile: None, h_chunk: IR_DEFAULT_H_CHUNK_V1, checkpoint_interval: 64 }
    }
}

/// **A layout per stage**: each stage's context its `max_trip`, every commit point at `tile_len`
/// lanes (the output stage's output node at `output_tile`), every state at `tile_len`, the history in
/// `h_chunk` rows, a checkpoint every `checkpoint_interval` positions (never more than the stage runs).
pub fn gen_default_layouts_v1(
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
    choice: &GenLayoutChoiceV1,
    checkpoint_interval: u32,
) -> Vec<PalwTirLayoutV1> {
    pipeline
        .stages
        .iter()
        .enumerate()
        .map(|(s, st)| {
            let p = &programs[st.program as usize];
            let output_slot = (s == pipeline.output_stage as usize).then(|| {
                p.blocks
                    .iter()
                    .enumerate()
                    .flat_map(|(bi, b)| b.nodes.iter().enumerate().filter(|(_, n)| n.commit).map(move |(ni, _)| (bi, ni)))
                    .position(|(bi, ni)| bi == p.schedule.post as usize && ni == p.output.node() as usize)
            });
            let commits = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
            let mut commit_tiles = vec![choice.tile_len; commits];
            if let (Some(Some(k)), Some(tile)) = (output_slot, choice.output_tile) {
                commit_tiles[k] = tile;
            }
            PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: st.max_trip,
                checkpoint_interval: checkpoint_interval.clamp(1, st.max_trip.max(1)),
                h_tile: choice.h_chunk,
                commit_tiles,
                state_tiles: vec![choice.tile_len; p.states.len()],
            }
        })
        .collect()
}

/// **The ruleset an offline admission is asked under**: the network's own where `palw_gen_v1` is armed
/// on it (at the fence's height), else the same ruleset with the IR fence and the generative fence armed
/// at DAA 1 under the drill's ceilings (the generative fence is dormant on every network), so a layout is
/// judged by the gate it will meet rather than refused for the fence alone.
pub struct GenOfflineGateV1 {
    pub params: Params,
    pub daa: u64,
    pub hypothetical: bool,
}

impl GenOfflineGateV1 {
    pub fn of(params: &Params) -> Self {
        if let Some(fence) = params.palw_gen_v1_fence() {
            return Self { params: params.clone(), daa: fence.activation.daa_score(), hypothetical: false };
        }
        let mut armed = params.clone();
        if armed.palw_tir_v1_fence().is_none() {
            (kaspa_consensus_core::palw_tir_v1::PALW_T12_TIR_V1_ENTRY.set)(&mut armed, Some(ForkActivation::new(1)));
        }
        (PALW_DRILL_GEN_V1_ENTRY.set)(&mut armed, Some(ForkActivation::new(1)));
        Self { params: armed, daa: 1, hypothetical: true }
    }

    pub fn note(&self) -> String {
        if self.hypothetical {
            "as if palw_gen_v1 were armed at DAA 1 under the drill's ceilings (it is dormant on this network)".to_string()
        } else {
            format!("at DAA {} (palw_gen_v1 on this network)", self.daa)
        }
    }
}

/// The registrant bond an offline gate is asked under: the gate reads no bond (the processor and the
/// fold do), so any key will do.
fn gate_bond() -> PalwBondKeyV2 {
    PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_bytes([0; 64]), 0))
}

/// **The registration gate a node would run, asked of `class` offline** — the pipeline admission under
/// [`GenOfflineGateV1`], at a nominal pricing, weightless. The admitted class's entry, row and report,
/// or the refusal by code.
pub fn gen_class_admission_offline_v1(
    gate: &GenOfflineGateV1,
    bundle: &PalwConsensusParamsV2,
    class: &PalwGenClassV1,
    artifact_root: Hash64,
) -> Result<PalwGenAdmittedV1, String> {
    let object =
        palw_gen_post_genesis_registration_v1(class.clone(), artifact_root, 0, 1 << 100, 1, gate.daa, gate_bond(), Vec::new())
            .map_err(|e| format!("{} ({e})", e.code()))?;
    palw_gen_registration_preflight_at_v1(&gate.params, bundle, &object, gate.daa).map_err(|e| format!("{} ({e})", e.code()))
}

/// What a class is made of, before its layouts are chosen.
#[derive(Clone, Debug)]
pub struct GenClassSpecV1 {
    pub profile: PalwGenProfileV1,
    pub pipeline: TirPipelineV1,
    pub programs: Vec<TirProgramV2>,
    pub tokenizer_id: Hash64,
    /// The class's output header (the kind, the shape and its meta).
    pub output: OutputSpecV1,
    pub offers: PalwGenOffersV1,
}

impl GenClassSpecV1 {
    /// The class under `layouts`.
    pub fn class(&self, layouts: Vec<PalwTirLayoutV1>) -> PalwGenClassV1 {
        PalwGenClassV1 {
            version: PALW_GEN_CLASS_VERSION_V1,
            profile: self.profile as u8,
            pipeline: self.pipeline.encode(),
            programs: self.programs.iter().map(|p| p.encode()).collect(),
            layouts,
            output: self.output.clone(),
            offers: self.offers.clone(),
            tokenizer_id: self.tokenizer_id,
        }
    }
}

/// What [`gen_declare_layout_v1`] wrote.
#[derive(Debug)]
pub struct GenDeclaredV1 {
    pub class: PalwGenClassV1,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    /// Leaves of the weights' pipeline inventory.
    pub inventory_leaves: u32,
    /// The chain row a registration would write (`None` only when the class does not derive one).
    pub row: Option<PalwGenClassRecordV1>,
    /// The gate's verdict: the admitted class's entry, row and report, or the refusal at the widest
    /// checkpoint interval the layout allows.
    pub admission: Result<PalwGenAdmittedV1, String>,
    /// Where the gate was asked ([`GenOfflineGateV1::note`]).
    pub admission_at: String,
}

/// **Declare a class's layouts**: the weights' inventory root, the layouts of `choice` at the widest
/// checkpoint interval the gate admits (halved from `choice.checkpoint_interval` down to 1), the class
/// and its row. A refusal the interval cannot fix — a close too wide to carry, anything the layout or
/// the program decides — is returned as it is (the widest interval's), which is the refusal an operator
/// acts on.
pub fn gen_declare_layout_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    spec: &GenClassSpecV1,
    weights: &dyn PipelineParams,
    choice: &GenLayoutChoiceV1,
) -> Result<GenDeclaredV1, String> {
    let (artifact_root, inventory_leaves) =
        palw_gen_inventory_root_v1(&spec.programs, weights).map_err(|e| format!("the weights' inventory: {e}"))?;
    let gate = GenOfflineGateV1::of(params);
    let mut checkpoint = choice.checkpoint_interval.max(1);
    // The first refusal, at the widest interval: the one an operator acts on when none is admitted.
    let mut widest: Option<(PalwGenClassV1, String)> = None;
    let (class, admission) = loop {
        let class = spec.class(gen_default_layouts_v1(&spec.pipeline, &spec.programs, choice, checkpoint));
        match gen_class_admission_offline_v1(&gate, bundle, &class, artifact_root) {
            Ok(admitted) => break (class, Ok(admitted)),
            Err(why) => {
                if widest.is_none() {
                    widest = Some((class, why.clone()));
                }
                // A close too wide to carry is the tile's to fix, not the interval's.
                if checkpoint <= 1 || why.contains("close bytes") {
                    let (class, why) = widest.take().expect("set above");
                    break (class, Err(why));
                }
                checkpoint /= 2;
            }
        }
    };
    let class_id = class.class_id(&artifact_root);
    let row = palw_gen_class_record_v1(&class, &artifact_root).ok();
    Ok(GenDeclaredV1 { class, class_id, artifact_root, inventory_leaves, row, admission, admission_at: gate.note() })
}

/// **The registration** of a declared class under the chain's `terms`: the network's own pricing (the
/// base class's slash value and initial target), weightless, at the yardstick job's leaf count — gated
/// before it is returned. Call once with an empty signature to learn the message to sign
/// ([`gen_registration_message_v1`]), then again with the signature.
#[allow(clippy::too_many_arguments)]
pub fn build_gen_registration_v1(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    class: &PalwGenClassV1,
    artifact_root: Hash64,
    terms: &PalwRegistrationTermsV2,
    activation_daa: u64,
    registrant_bond: PalwBondKeyV2,
    signature: Vec<u8>,
    daa: u64,
) -> Result<PalwConsensusObjectV2, String> {
    let object = palw_gen_post_genesis_registration_v1(
        class.clone(),
        artifact_root,
        0,
        terms.initial_target,
        terms.slash_value_per_pwu,
        activation_daa,
        registrant_bond,
        signature,
    )
    .map_err(|e| format!("{} ({e})", e.code()))?;
    palw_gen_registration_preflight_at_v1(params, bundle, &object, daa)
        .map_err(|e| format!("the admission gate refuses the registration: {} ({e})", e.code()))?;
    Ok(object)
}

/// **The message the registrant bond signs** — `palw_gen_class_registration_message_v1` over every field
/// `object` carries (never over a field assembled beside it). `None` for any other object.
pub fn gen_registration_message_v1(network_domain: Hash64, object: &PalwConsensusObjectV2) -> Option<Hash64> {
    let PalwConsensusObjectV2::ClassRegisteredGenV1 {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule,
        initial_target,
        share_permille,
        activation_daa,
        admission,
    } = object
    else {
        return None;
    };
    Some(palw_gen_class_registration_message_v1(
        network_domain,
        *class_id,
        *share_permille,
        *activation_daa,
        &admission.registrant_bond,
        *artifact_root,
        *slash_value_per_pwu,
        *initial_target,
        pwu_rule,
        &admission.class,
    ))
}

/// **Write a declared class's `PALWTIR2` container** from weights held in any form (its row's pipeline,
/// programs, class and tokenizer, and every tensor in the inventory's order). Returns the file digest.
pub fn gen_write_declared_container_v1(
    path: &std::path::Path,
    declared: &GenDeclaredV1,
    weights: &dyn PipelineParams,
    meta: String,
) -> Result<[u8; 64], String> {
    let row = declared.row.as_ref().ok_or("the class derives no registry row")?;
    misaka_palw_base0::gen_worker::gen_write_container_v1(path, row, &PipelineParamsRef(weights), meta)
}

/// `&dyn PipelineParams` as a sized `PipelineParams`.
struct PipelineParamsRef<'a>(&'a dyn PipelineParams);

impl PipelineParams for PipelineParamsRef<'_> {
    fn params(&self, program: u16) -> &dyn misaka_palw_tir::interp::ParamSource {
        self.0.params(program)
    }
}

// ---------------------------------------------------------------------------------------------
// The node's half: which held generative class registers (the IR selection's twin)
// ---------------------------------------------------------------------------------------------

/// **The generative classes of `holdings`** — every artifact the generative lineage loaded, one entry per
/// class id (first holding wins). Read off the holdings themselves, so any SDK instance over them agrees.
pub fn gen_entries_of_v1(holdings: &[PalwLoadedArtifactV1]) -> Vec<PalwGenClassEntryV1> {
    let mut out: Vec<PalwGenClassEntryV1> = Vec::new();
    for h in holdings.iter().filter(|h| h.lineage_id == GEN_LINEAGE_ID_V1) {
        if let Some(entry) = h.payload().downcast_ref::<PalwGenClassEntryV1>()
            && !out.iter().any(|e| e.class_id() == entry.class_id())
        {
            out.push(entry.clone());
        }
    }
    out
}

/// Does `wanted` name `entry` — its model id, or its class id in hex?
fn names(entry: &PalwGenClassEntryV1, wanted: &str) -> bool {
    let w = wanted.trim_start_matches("0x");
    entry.model_id == wanted || entry.class_id().to_string() == w
}

/// **The one generative registration this node should attempt, or why there is none** — the IR
/// selection's sentences, over generative holdings: nothing held, everything already registered (by class
/// id, or by artifact root: known weights are never re-registered under a fresh id), the operator's
/// `--palw-register-class` matching nothing, or more than one left and nothing picked.
pub fn gen_registration_candidate_v1(
    holdings: &[PalwLoadedArtifactV1],
    terms: &PalwRegistrationTermsV2,
    wanted: Option<&str>,
) -> Result<PalwGenClassEntryV1, String> {
    let wanted = wanted.filter(|s| !s.is_empty());
    let named: Vec<PalwGenClassEntryV1> =
        gen_entries_of_v1(holdings).into_iter().filter(|e| wanted.is_none_or(|w| names(e, w))).collect();
    if named.is_empty() {
        return Err(match wanted {
            Some(w) => format!("--palw-register-class {w} names no generative class this node's artifacts declare"),
            None => {
                "no --palw-class-artifact is a generative (PALWTIR2) artifact, so there is no generative class to register".to_string()
            }
        });
    }
    let fresh: Vec<PalwGenClassEntryV1> = named
        .into_iter()
        .filter(|e| !terms.registered_class_ids.contains(&e.class_id()) && !terms.registered_artifact_roots.contains(&e.artifact_root))
        .collect();
    match fresh.len() {
        0 => Err("every generative class this node's artifacts declare is already registered on this chain (or its weights are)"
            .to_string()),
        1 => Ok(fresh.into_iter().next().expect("one")),
        n => Err(format!(
            "this node's artifacts declare {n} unregistered generative classes ({}) — name one with --palw-register-class <model-id>",
            fresh.iter().map(|e| e.model_id.as_str()).collect::<Vec<_>>().join(", ")
        )),
    }
}
