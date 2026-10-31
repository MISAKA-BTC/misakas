//! Pure G14 source geometry for an already resolved TIR job/class.
//!
//! No signature, selected-prefix, AcceptedMaterial, fee, response reservation,
//! native duty or credit authority is created here. The native A4 resolver must
//! supply actual registered rows and the accepted job/network prompt form, and
//! independently bind scope/service/history to the current selected view.
use misaka_palw_positive_contract::{
    ContractError,
    identity::Digest,
    messages::{MinerResultCommit, SourceKind, SourceRequest},
};

use crate::{
    Hash64,
    palw_artifact::{palw_artifact_opening_path_len_v1, palw_artifact_operand_borsh_len_v1, verify_artifact_opening_v1},
    palw_prompt_ids_v1::{
        PALW_PROMPT_IDS_OPENING_HEADER_BYTES, PalwPromptIdsFormV1, prompt_ids_tile_count_v1, prompt_ids_tile_len_v1,
        prompt_token_ids_match_v1, verify_prompt_ids_opening_v1,
    },
    palw_step_leg::step_leg_max_opening_siblings_v1,
    palw_tir_admission_v1::{PALW_TIR_CLASS_RECORD_VERSION_V1, PalwTirClassRecordV1},
    palw_tir_artifact_v1::PALW_TIR_ROW_PIECE_BYTES_V1,
    palw_tir_attempt_v1::PalwTirJobFactsV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1},
    palw_tir_court_v1::PalwTirInventoryIndexV1,
    palw_v2::{PALW_V2_MAX_NETWORK_ID_BYTES, PalwJobContextV2},
};
use misaka_palw_positive_contract::identity::{CaseId, ResultId};
use misaka_palw_tir::TirProgramV1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PositiveSourceGeometryErrorV2 {
    Contract(ContractError),
    Binding,
    Budget,
}
impl From<ContractError> for PositiveSourceGeometryErrorV2 {
    fn from(value: ContractError) -> Self {
        Self::Contract(value)
    }
}
use crate::{
    palw_artifact::PalwArtifactOpeningV1,
    palw_prompt_ids_v1::{PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsOpeningV1},
};
/// Existing G14 data types; this enum is a process DTO, without a wire codec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoredUnitV2 {
    Artifact(PalwArtifactOpeningV1),
    Input(PalwPromptIdsOpeningV1),
    /// Existing whole-prompt G14 input under the legacy flat commitment.
    FlatInput(Vec<u32>),
}
impl StoredUnitV2 {
    pub fn bounded(&self, max: u64) -> bool {
        match self {
            Self::Artifact(unit) => {
                unit.operand.bytes.len() as u64 <= max && unit.operand.tensor_name.len() as u64 <= max && unit.path.len() <= 32
            }
            Self::Input(unit) => unit.tile_ids.len() <= PALW_PROMPT_IDS_TILE_LEN as usize && unit.opening.siblings.len() <= 32,
            Self::FlatInput(ids) => (ids.len() as u64).checked_mul(4).is_some_and(|n| n <= max),
        }
    }
    pub fn sizes(&self) -> Result<(u64, u64), PositiveSourceGeometryErrorV2> {
        match self {
            Self::FlatInput(ids) => Ok((ids.len() as u64 * 4, 4 + ids.len() as u64 * 4)),
            Self::Artifact(unit) => Ok((
                unit.operand.bytes.len() as u64,
                borsh::object_length(unit).map_err(|_| PositiveSourceGeometryErrorV2::Binding)? as u64,
            )),
            Self::Input(unit) => Ok((
                unit.tile_ids.len() as u64 * 4,
                borsh::object_length(unit).map_err(|_| PositiveSourceGeometryErrorV2::Binding)? as u64,
            )),
        }
    }
    pub fn authenticates(&self, result: &MinerResultCommit, request: &SourceRequest, index: u64, units: u64) -> bool {
        match (self, request.kind) {
            (Self::Artifact(unit), SourceKind::ArtifactInventory) => {
                request.source == result.artifact
                    && u64::from(unit.leaf_index) == index
                    && u64::from(unit.leaf_count) == units
                    && verify_artifact_opening_v1(unit, Hash64::from_bytes(request.source.0)).is_ok()
            }
            (Self::Input(unit), SourceKind::Input) => {
                request.source == result.input
                    && u64::from(unit.tile_index) == index
                    && prompt_ids_tile_count_v1(u64::from(result.prefill)) == Some(units)
                    && verify_prompt_ids_opening_v1(&Hash64::from_bytes(request.source.0), result.prefill, unit).is_ok()
            }
            (Self::FlatInput(ids), SourceKind::Input) => {
                request.source == result.input
                    && units == 1
                    && index == 0
                    && ids.len() == result.prefill as usize
                    && prompt_token_ids_match_v1(PalwPromptIdsFormV1::Flat, ids, &Hash64::from_bytes(request.source.0))
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GeometryLimitsV2 {
    pub max_program_bytes: u64,
    pub max_parameter_instances: u64,
    pub max_inventory_leaves: u32,
    pub max_units_per_range: u64,
}

/// The existing raw G14 body only: artifact opening / prompt opening / flat
/// Vec<u32>. No outer operation, carrier framing, fee, work-unit or RSS quote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct G14UnitBoundV2 {
    pub source_units: u64,
    pub max_unit_bytes: u64,
    pub max_data_bytes: u64,
}

pub struct TirSourceGeometryV2 {
    result: ResultId,
    case: CaseId,
    service: Digest,
    artifact: Digest,
    input: Digest,
    prefill: u32,
    input_form: PalwPromptIdsFormV1,
    program: TirProgramV1,
    inventory: PalwTirInventoryIndexV1,
    max_units_per_range: u64,
}
impl TirSourceGeometryV2 {
    /// `registered_artifact` must come from the actual PalwClassStateV2 row;
    /// `registered` from state.tir_class_v1, never copied from a candidate.
    /// `network_prompt_form` is the accepted job's native network policy. The
    /// existing held-class override is applied by PalwTirJobFactsV1 itself.
    #[allow(clippy::too_many_arguments)]
    pub fn derive(
        class: &PalwTirClassV1,
        registered: &PalwTirClassRecordV1,
        registered_artifact: Hash64,
        job: &PalwJobContextV2,
        result: &MinerResultCommit,
        network_prompt_form: PalwPromptIdsFormV1,
        limits: GeometryLimitsV2,
    ) -> Result<Self, PositiveSourceGeometryErrorV2> {
        let cap = limits.max_program_bytes.min(misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64);
        if cap == 0
            || limits.max_units_per_range == 0
            || class.program.len() as u64 > cap
            || registered.program.len() as u64 > cap
            || class.layout.commit_tiles.len() as u64 > cap / 4
            || class.layout.state_tiles.len() as u64 > cap / 4
            || job.network_id.is_empty()
            || job.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES
        {
            return Err(PositiveSourceGeometryErrorV2::Budget);
        }
        result.validate_shape()?;
        if class.version != PALW_TIR_CLASS_VERSION_V1
            || class.layout.version != PALW_TIR_LAYOUT_VERSION_V1
            || registered.version != PALW_TIR_CLASS_RECORD_VERSION_V1
            || job.version != 2
            || registered.program.as_slice() != class.program.as_slice()
            || result.artifact.0 != registered_artifact.as_bytes()
            || result.registered_class.0 != registered.facts.class_id.as_bytes()
            || result.program.0 != registered.graph_ir_root.as_bytes()
            || result.tokenizer.0 != registered.tokenizer_id.as_bytes()
        {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        // Strictly bounded normal-form program before any inventory expansion.
        let program = class.decode_program().map_err(|_| PositiveSourceGeometryErrorV2::Binding)?;
        preflight_inventory_v2(&program, limits)?;
        let committed = program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).count();
        if class.layout.commit_tiles.len() != committed
            || class.layout.state_tiles.len() != program.states.len()
            || class.layout.max_context == 0
            || class.layout.max_context > program.history_bound
            || class.layout.checkpoint_interval == 0
            || class.layout.h_tile == 0
        {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        registered.check_program_v1().map_err(|_| PositiveSourceGeometryErrorV2::Binding)?;
        let class_id = class.class_id(&registered_artifact);
        if class_id != registered.facts.class_id
            || registered.facts != PalwTirJobFactsV1::of(class, &program, class_id)
            || class.graph_ir_root() != registered.graph_ir_root
            || class.layout_digest() != registered.layout_digest
            || class.tokenizer_id != registered.tokenizer_id
            || registered.prim_set_id != Hash64::from_bytes(program.prim_set_id)
            || registered.logits_vocab
                != u32::try_from(program.blocks[program.schedule.post as usize].nodes[program.logits as usize].out.elements_at(1))
                    .unwrap_or(u32::MAX)
            || job.shape_profile_id != class_id
            || job.tokenizer_id != class.tokenizer_id
            || result.job.0 != job.job_id.as_bytes()
            || result.execution.0 != job.context_hash().as_bytes()
            || result.input.0 != job.prompt_token_ids_hash.as_bytes()
            || result.execution_rules.0 != job.cu_ruleset_id.as_bytes()
            || result.generation_seed != job.execution_seed
            || result.prefill != job.declared_prefill_tokens
            || result.decode != job.exact_decode_tokens
            || result.max_context != job.max_context_tokens
            || Hash64::from_bytes(program.logits_scheme_id) != job.trace_scheme_id
        {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        let positions = result
            .prefill
            .checked_add(result.decode.checked_sub(1).ok_or(PositiveSourceGeometryErrorV2::Binding)?)
            .ok_or(PositiveSourceGeometryErrorV2::Budget)?;
        if positions > class.layout.max_context || positions >= job.max_context_tokens {
            return Err(PositiveSourceGeometryErrorV2::Budget);
        }
        let inventory = PalwTirInventoryIndexV1::new(&program).ok_or(PositiveSourceGeometryErrorV2::Binding)?;
        if inventory.leaf_count() > limits.max_inventory_leaves {
            return Err(PositiveSourceGeometryErrorV2::Budget);
        }
        Ok(Self {
            result: result.id(),
            case: result.case_id(),
            service: result.service_contract,
            artifact: result.artifact,
            input: result.input,
            prefill: result.prefill,
            input_form: registered.facts.prompt_ids_form(network_prompt_form),
            program,
            inventory,
            max_units_per_range: limits.max_units_per_range,
        })
    }
    pub fn result_id(&self) -> ResultId {
        self.result
    }
    pub fn input_form(&self) -> PalwPromptIdsFormV1 {
        self.input_form
    }
    pub fn source_root(&self, kind: SourceKind) -> Digest {
        match kind {
            SourceKind::ArtifactInventory => self.artifact,
            SourceKind::Input => self.input,
        }
    }
    pub fn source_units(&self, kind: SourceKind) -> u64 {
        match kind {
            SourceKind::ArtifactInventory => u64::from(self.inventory.leaf_count()),
            SourceKind::Input => match self.input_form {
                PalwPromptIdsFormV1::Flat => 1,
                PalwPromptIdsFormV1::MerkleV1 => prompt_ids_tile_count_v1(u64::from(self.prefill)).unwrap_or(0),
            },
        }
    }
    pub fn validate_request(&self, request: &SourceRequest) -> Result<(), PositiveSourceGeometryErrorV2> {
        if request.case != self.case || request.contract != self.service || request.source != self.source_root(request.kind) {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        let units = self.source_units(request.kind);
        if units == 0 {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        request.validate(units)?;
        if request.end - request.first > self.max_units_per_range {
            return Err(PositiveSourceGeometryErrorV2::Budget);
        }
        Ok(())
    }
    pub fn unit_bound(&self, kind: SourceKind, index: u64) -> Result<G14UnitBoundV2, PositiveSourceGeometryErrorV2> {
        let source_units = self.source_units(kind);
        if index >= source_units {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        let (data, body) = match kind {
            SourceKind::ArtifactInventory => {
                let leaf = u32::try_from(index).map_err(|_| PositiveSourceGeometryErrorV2::Binding)?;
                let (param, layer, _, len) = self.inventory.piece_of(leaf).ok_or(PositiveSourceGeometryErrorV2::Binding)?;
                let name = &self.program.params[usize::from(param)].name;
                let path = palw_artifact_opening_path_len_v1(self.inventory.leaf_count(), leaf)
                    .ok_or(PositiveSourceGeometryErrorV2::Binding)?;
                let body = palw_artifact_operand_borsh_len_v1(name.len(), layer.is_some(), len as usize)
                    .checked_add(12)
                    .and_then(|n| path.checked_mul(64).and_then(|p| n.checked_add(p)))
                    .ok_or(PositiveSourceGeometryErrorV2::Budget)?;
                (u64::from(len), body)
            }
            SourceKind::Input => match self.input_form {
                PalwPromptIdsFormV1::Flat => {
                    let bytes = u64::from(self.prefill).checked_mul(4).ok_or(PositiveSourceGeometryErrorV2::Budget)?;
                    (bytes, bytes.checked_add(4).ok_or(PositiveSourceGeometryErrorV2::Budget)?)
                }
                PalwPromptIdsFormV1::MerkleV1 => {
                    let lanes =
                        prompt_ids_tile_len_v1(u64::from(self.prefill), index).ok_or(PositiveSourceGeometryErrorV2::Binding)? as u64;
                    let bytes = lanes.checked_mul(4).ok_or(PositiveSourceGeometryErrorV2::Budget)?;
                    let path = step_leg_max_opening_siblings_v1(source_units) as u64;
                    (
                        bytes,
                        bytes
                            .checked_add(PALW_PROMPT_IDS_OPENING_HEADER_BYTES)
                            .and_then(|n| path.checked_mul(64).and_then(|p| n.checked_add(p)))
                            .ok_or(PositiveSourceGeometryErrorV2::Budget)?,
                    )
                }
            },
        };
        Ok(G14UnitBoundV2 { source_units, max_unit_bytes: body, max_data_bytes: data })
    }
    pub fn request_bound(&self, request: &SourceRequest) -> Result<G14UnitBoundV2, PositiveSourceGeometryErrorV2> {
        self.validate_request(request)?;
        let mut upper = G14UnitBoundV2 { source_units: self.source_units(request.kind), max_unit_bytes: 0, max_data_bytes: 0 };
        for index in request.first..request.end {
            let b = self.unit_bound(request.kind, index)?;
            upper.max_unit_bytes = upper.max_unit_bytes.max(b.max_unit_bytes);
            upper.max_data_bytes = upper.max_data_bytes.max(b.max_data_bytes);
        }
        Ok(upper)
    }
    /// Geometry and data authentication only, not a literal ServiceReceipt.
    pub fn check_unit(
        &self,
        kind: SourceKind,
        index: u64,
        unit: &StoredUnitV2,
    ) -> Result<G14UnitBoundV2, PositiveSourceGeometryErrorV2> {
        let bound = self.unit_bound(kind, index)?;
        match (kind, unit) {
            (SourceKind::ArtifactInventory, StoredUnitV2::Artifact(opening)) => {
                let leaf = u32::try_from(index).map_err(|_| PositiveSourceGeometryErrorV2::Binding)?;
                let (param, layer, start, len) = self.inventory.piece_of(leaf).ok_or(PositiveSourceGeometryErrorV2::Binding)?;
                if opening.leaf_index != leaf
                    || opening.leaf_count != self.inventory.leaf_count()
                    || opening.operand.tensor_name != self.program.params[usize::from(param)].name
                    || opening.operand.layer != layer
                    || opening.operand.row_start != start
                    || opening.operand.bytes.len() != len as usize
                    || opening.path.len() > 32
                {
                    return Err(PositiveSourceGeometryErrorV2::Binding);
                }
                verify_artifact_opening_v1(opening, Hash64::from_bytes(self.artifact.0))
                    .map_err(|_| PositiveSourceGeometryErrorV2::Binding)?;
            }
            (SourceKind::Input, StoredUnitV2::Input(opening)) if self.input_form == PalwPromptIdsFormV1::MerkleV1 => {
                if u64::from(opening.tile_index) != index
                    || opening.opening.siblings.len() > 32
                    || opening.tile_ids.len()
                        != prompt_ids_tile_len_v1(u64::from(self.prefill), index).ok_or(PositiveSourceGeometryErrorV2::Binding)?
                {
                    return Err(PositiveSourceGeometryErrorV2::Binding);
                }
                verify_prompt_ids_opening_v1(&Hash64::from_bytes(self.input.0), self.prefill, opening)
                    .map_err(|_| PositiveSourceGeometryErrorV2::Binding)?;
            }
            (SourceKind::Input, StoredUnitV2::FlatInput(ids)) if self.input_form == PalwPromptIdsFormV1::Flat => {
                if ids.len() != self.prefill as usize
                    || !prompt_token_ids_match_v1(self.input_form, ids, &Hash64::from_bytes(self.input.0))
                {
                    return Err(PositiveSourceGeometryErrorV2::Binding);
                }
            }
            _ => return Err(PositiveSourceGeometryErrorV2::Binding),
        }
        Ok(bound)
    }
}

/// Upper bounds deliberately overcount a per-layer parameter as present in
/// every scheduled layer. This prevents the existing index's instance vectors
/// from being expanded before finite allocation bounds have been checked.
pub fn preflight_inventory_v2(program: &TirProgramV1, limits: GeometryLimitsV2) -> Result<(), PositiveSourceGeometryErrorV2> {
    let mut instances = 0u64;
    let mut leaves = 0u64;
    for param in &program.params {
        let n = if param.per_layer { program.schedule.layers.len() as u64 } else { 1 };
        instances =
            instances.checked_add(n).filter(|n| *n <= limits.max_parameter_instances).ok_or(PositiveSourceGeometryErrorV2::Budget)?;
        let width = param.dtype.width() as u64;
        let bytes =
            param.shape.iter().try_fold(width, |n, d| n.checked_mul(u64::from(*d))).ok_or(PositiveSourceGeometryErrorV2::Budget)?;
        if bytes > u32::MAX as u64 {
            return Err(PositiveSourceGeometryErrorV2::Budget);
        }
        let row = if param.shape.len() >= 2 {
            param.shape[1..]
                .iter()
                .try_fold(width, |n, d| n.checked_mul(u64::from(*d)))
                .ok_or(PositiveSourceGeometryErrorV2::Budget)?
        } else {
            bytes
        };
        let per_instance = if row == 0 || bytes == 0 {
            0
        } else {
            (bytes / row).checked_mul(row.div_ceil(PALW_TIR_ROW_PIECE_BYTES_V1)).ok_or(PositiveSourceGeometryErrorV2::Budget)?
        };
        leaves = leaves
            .checked_add(per_instance.checked_mul(n).ok_or(PositiveSourceGeometryErrorV2::Budget)?)
            .filter(|n| *n <= u64::from(limits.max_inventory_leaves))
            .ok_or(PositiveSourceGeometryErrorV2::Budget)?;
    }
    Ok(())
}

#[path = "palw_positive_source_geometry_v2/codec.rs"]
mod codec;
