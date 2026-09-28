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
}

impl Default for TirLayoutChoiceV1 {
    fn default() -> Self {
        Self { max_context: None, tile_len: IR_DEFAULT_TILE_LEN_V1, h_chunk: IR_DEFAULT_H_CHUNK_V1, logits_scheme: None }
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
                commit_tiles.push(if logits && tiled { PALW_LOGITS_TILE_LANES as u32 } else { choice.tile_len });
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

/// **Write `input`'s program and tensors to `output` as a class** — under the logits scheme `choice`
/// names ([`tir_program_with_scheme_v1`]) and a declared layout: `choice` tiled, its checkpoint
/// interval the widest admission v10 accepts (halved from `min_j C_j` down to 1) — with `model_id`
/// recorded in the container's provenance when given. The inventory root is the input's (the scheme
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
    let program_bytes = program.encode();
    let (artifact_root, _) = artifact.inventory_root()?;
    let mut layout = tir_default_layout_v1(params, program, choice)?;
    let class_of = |layout: &PalwTirLayoutV1| PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program_bytes.clone(),
        layout: layout.clone(),
        tokenizer_id: Hash64::from_bytes(container.header.tokenizer_id),
    };
    let mut admission = tir_class_admission_offline_v1(params, bundle, &class_of(&layout), artifact_root);
    while admission.is_err() && layout.checkpoint_interval > 1 {
        layout.checkpoint_interval /= 2;
        admission = tir_class_admission_offline_v1(params, bundle, &class_of(&layout), artifact_root);
    }
    if admission.is_err() {
        // Every interval refused: keep the widest, whose refusal is the one an operator acts on.
        layout = tir_default_layout_v1(params, program, choice)?;
        admission = tir_class_admission_offline_v1(params, bundle, &class_of(&layout), artifact_root);
    }
    let mut meta = serde_json::from_str::<serde_json::Value>(&container.header.meta).unwrap_or_else(|_| serde_json::json!({}));
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
