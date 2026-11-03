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

/// Result-free native G14 inventory. It has no Case, service receipt or
/// source-duty authority. Assignment intake can quote it before a Result exists.
pub struct TirSourceCatalogV2 {
    artifact: Digest,
    input: Digest,
    prefill: u32,
    input_form: PalwPromptIdsFormV1,
    program: TirProgramV1,
    inventory: PalwTirInventoryIndexV1,
    max_units_per_range: u64,
}
pub struct TirSourceGeometryV2 {
    result: ResultId,
    case: CaseId,
    service: Digest,
    catalog: TirSourceCatalogV2,
}
impl std::ops::Deref for TirSourceGeometryV2 {
    type Target = TirSourceCatalogV2;
    fn deref(&self) -> &Self::Target {
        &self.catalog
    }
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
        result.validate_shape()?;
        if result.artifact.0 != registered_artifact.as_bytes()
            || result.registered_class.0 != registered.facts.class_id.as_bytes()
            || result.program.0 != registered.graph_ir_root.as_bytes()
            || result.tokenizer.0 != job.tokenizer_id.as_bytes()
            || result.job.0 != job.job_id.as_bytes()
            || result.execution.0 != job.context_hash().as_bytes()
            || result.input.0 != job.prompt_token_ids_hash.as_bytes()
            || result.execution_rules.0 != job.cu_ruleset_id.as_bytes()
            || result.generation_seed != job.execution_seed
            || result.prefill != job.declared_prefill_tokens
            || result.decode != job.exact_decode_tokens
            || result.max_context != job.max_context_tokens
        {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        let catalog = TirSourceCatalogV2::derive(class, registered, registered_artifact, job, network_prompt_form, limits)?;
        Self::bind_catalog_v2(result, catalog)
    }

    /// Bind a catalog already derived from the independently authenticated
    /// Assignment job. The native caller must have checked the full Result.
    pub(crate) fn bind_catalog_v2(
        result: &MinerResultCommit,
        catalog: TirSourceCatalogV2,
    ) -> Result<Self, PositiveSourceGeometryErrorV2> {
        if result.artifact != catalog.artifact || result.input != catalog.input || result.prefill != catalog.prefill {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        Ok(Self { result: result.id(), case: result.case_id(), service: result.service_contract, catalog })
    }
    pub fn result_id(&self) -> ResultId {
        self.result
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

/// Exact result-free inventory summary, not an accepted service or lease.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceCatalogQuoteV2 {
    pub digest: Digest,
    pub units: u64,
    pub data_bytes: u64,
    pub opening_bytes: u64,
    pub max_unit_bytes: u64,
}
struct CatalogHasherV2(blake2b_simd::State);
impl std::io::Write for CatalogHasherV2 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl TirSourceCatalogV2 {
    /// Stream the canonical full source inventory without allocating a Vec of
    /// units. The cap is checked before enumeration. Every ordinal and exact
    /// G14 opening bound is bound; independent roles do not reset this catalog.
    pub fn quote(
        &self,
        assignment: misaka_palw_positive_contract::assignment::AssignmentId,
        context: Digest,
        source_policy: Digest,
        max_entries: u32,
    ) -> Result<SourceCatalogQuoteV2, PositiveSourceGeometryErrorV2> {
        let kinds = [SourceKind::ArtifactInventory, SourceKind::Input];
        let units = self
            .source_units(kinds[0])
            .checked_add(self.source_units(kinds[1]))
            .filter(|n| *n > 0 && *n <= u64::from(max_entries))
            .ok_or(PositiveSourceGeometryErrorV2::Budget)?;
        let mut writer = CatalogHasherV2(
            blake2b_simd::Params::new().hash_length(64).key(b"misaka/positive/assignment-source-catalog/v2/r1").to_state(),
        );
        borsh::to_writer(&mut writer, &(2u16, 1u16, assignment, context, source_policy, units))
            .map_err(|_| PositiveSourceGeometryErrorV2::Binding)?;
        let mut quote = SourceCatalogQuoteV2 { digest: Digest([0; 64]), units, data_bytes: 0, opening_bytes: 0, max_unit_bytes: 0 };
        for kind in kinds {
            let count = self.source_units(kind);
            borsh::to_writer(&mut writer, &(kind, self.source_root(kind), count))
                .map_err(|_| PositiveSourceGeometryErrorV2::Binding)?;
            for index in 0..count {
                let bound = self.unit_bound(kind, index)?;
                borsh::to_writer(&mut writer, &(index, bound.max_data_bytes, bound.max_unit_bytes))
                    .map_err(|_| PositiveSourceGeometryErrorV2::Binding)?;
                quote.data_bytes = quote.data_bytes.checked_add(bound.max_data_bytes).ok_or(PositiveSourceGeometryErrorV2::Budget)?;
                quote.opening_bytes =
                    quote.opening_bytes.checked_add(bound.max_unit_bytes).ok_or(PositiveSourceGeometryErrorV2::Budget)?;
                quote.max_unit_bytes = quote.max_unit_bytes.max(bound.max_unit_bytes);
            }
        }
        quote.digest = Digest(writer.0.finalize().as_bytes().try_into().expect("64-byte digest"));
        Ok(quote)
    }
    /// Checks complete native registry/program/layout/job inputs. No synthetic
    /// MinerResultCommit, zero CaseId or placeholder service is constructed.
    #[allow(clippy::too_many_arguments)]
    pub fn derive(
        class: &PalwTirClassV1,
        registered: &PalwTirClassRecordV1,
        registered_artifact: Hash64,
        job: &PalwJobContextV2,
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
        // Registration authenticates its own tokenizer via class identity.
        // Canonical jobs may use a zero tokenizer independently of this
        // registered identity. This reader creates no job authority.
        if class.version != PALW_TIR_CLASS_VERSION_V1
            || class.layout.version != PALW_TIR_LAYOUT_VERSION_V1
            || registered.version != PALW_TIR_CLASS_RECORD_VERSION_V1
            || job.version != 2
            || registered.program.as_slice() != class.program.as_slice()
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
            || Hash64::from_bytes(program.logits_scheme_id) != job.trace_scheme_id
        {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        let positions = job
            .declared_prefill_tokens
            .checked_add(job.exact_decode_tokens.checked_sub(1).ok_or(PositiveSourceGeometryErrorV2::Binding)?)
            .ok_or(PositiveSourceGeometryErrorV2::Budget)?;
        if positions > class.layout.max_context || positions >= job.max_context_tokens {
            return Err(PositiveSourceGeometryErrorV2::Budget);
        }
        let inventory = PalwTirInventoryIndexV1::new(&program).ok_or(PositiveSourceGeometryErrorV2::Binding)?;
        if inventory.leaf_count() > limits.max_inventory_leaves {
            return Err(PositiveSourceGeometryErrorV2::Budget);
        }
        Ok(Self {
            artifact: Digest(registered_artifact.as_bytes()),
            input: Digest(job.prompt_token_ids_hash.as_bytes()),
            prefill: job.declared_prefill_tokens,
            input_form: registered.facts.prompt_ids_form(network_prompt_form),
            program,
            inventory,
            max_units_per_range: limits.max_units_per_range,
        })
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
