//! Local one-capture bridge to ORIGINAL legacy data and A1 comparison targets.
//! No runner, public codec/domain/ID, native authority or arithmetic guarantee.
mod budget;
mod legacy;
use super::positive_typed_capture::{PrototypeTypedCapture, TypedRecord, TypedRecordKind};
use crate::{Elem, with_slice};
use budget::{LiveMaterialStage, metadata_backing, quote_with_live, stage_backing};
pub use budget::{StageReservations, TypedAdapterLimits, TypedAdapterQuote, quote_typed_adapter};
use kaspa_consensus_core::{
    Hash64,
    palw_artifact::{PalwArtifactMultiproofV1, PalwArtifactOpeningV1},
    palw_positive_check_v2::{
        expanded_plan::{ExpandedPlan, ExpandedRelationKind},
        full_relation::FullRelationPreparation,
    },
    palw_prompt_ids_v1::PalwPromptIdsOpeningV1,
    palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1},
    palw_step_refute::{PalwDecodeTokenPinV1, PalwTiledDecodePinV1},
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_court_v1::{
        PalwTirCourtRulesV1, PalwTirEvidenceStoreV1, PalwTirParamOpeningV1,
        source_only::{
            OwnedTirSourceOnlyMaterialV1, TirSourceOnlyLimitsV1, TirSourceOnlyMaterialV1, build_source_only_legacy_v1,
            build_source_only_node_v1,
        },
    },
    palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirStepBindingV1, PalwTirStepSpaceV1},
    palw_v2::{PALW_V2_MAX_NETWORK_ID_BYTES, PalwJobContextV2},
};
use misaka_palw_positive_contract::{
    identity::Digest,
    messages::{AdmissionContext, AuthenticatedResult, ClaimForm},
    tir::greedy_selector_id,
};
use misaka_palw_tir::program::{MAX_BLOCKS, MAX_NODES_PER_BLOCK, MAX_PROGRAM_BYTES, MAX_STATES};
use std::{
    cell::{Cell, RefCell},
    mem::size_of,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypedAdapterError {
    Binding,
    Budget,
    Geometry,
    Value,
    Missing,
    Source(String),
}
pub type Error = TypedAdapterError;
/// Independently resolved original inputs. Signed authorship alone is not
/// native admission/funding. Keep metadata alive outside a consuming checker.
pub struct TypedAdapterExpected<'a> {
    pub registered: &'a PalwTirClassV1,
    pub artifact: Hash64,
    pub job: &'a PalwJobContextV2,
    pub result: &'a AuthenticatedResult<'a>,
    pub rooted: &'a AdmissionContext,
    pub step_space: &'a PalwTirStepSpaceV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RecordKey {
    Node(u32, u16, u16),
    Fixed(u32, u16, Option<u16>),
    History(u32, u16, Option<u16>),
    Output(u32),
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum AdapterCharge {
    LookupVisits,
    ProofAttempts,
    ProofLeafReads,
    HashWork,
    ClaimBytes,
    MaterialBytes,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypedAdapterMeter {
    pub lookup_visits: u64,
    pub proof_attempts: u64,
    pub proof_leaf_reads: u64,
    pub hash_work: u64,
    pub claim_bytes: u64,
    pub material_bytes: u64,
}
pub struct TypedCaptureAdapter<'a> {
    pub(crate) capture: &'a PrototypeTypedCapture,
    pub(crate) expected: TypedAdapterExpected<'a>,
    pub(crate) index: Vec<(RecordKey, usize)>,
    pub(crate) limits: TypedAdapterLimits,
    pub(crate) meter: RefCell<TypedAdapterMeter>,
    live_material: Cell<bool>,
    live_material_stage: Cell<LiveMaterialStage>,
    stages: Cell<StageReservations>,
}
fn digest(h: Hash64) -> Digest {
    Digest(h.as_bytes())
}
fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or(Error::Budget)
}
fn mul(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_mul(b).ok_or(Error::Budget)
}
fn key(r: &TypedRecord) -> RecordKey {
    match r.kind {
        TypedRecordKind::Node { position, occurrence, node, .. } => RecordKey::Node(position, occurrence, node),
        TypedRecordKind::FixedAfter { position, state, layer } => RecordKey::Fixed(position, state, layer),
        TypedRecordKind::HistoryAppendRow { position, state, layer } => RecordKey::History(position, state, layer),
        TypedRecordKind::LastOutput { position, .. } => RecordKey::Output(position),
    }
}
impl<'a> TypedCaptureAdapter<'a> {
    /// Persist simultaneous external-owner envelopes for the whole adapter
    /// session. They are additive to the adapter's live material slot; caller
    /// declarations do not verify actual allocations. Later claim exports can
    /// increase these envelopes, but cannot release or replace them.
    pub fn prepare(
        capture: &'a PrototypeTypedCapture,
        expected: TypedAdapterExpected<'a>,
        limits: TypedAdapterLimits,
        stages: StageReservations,
    ) -> Result<Self, Error> {
        let class = expected.registered;
        let job = expected.job;
        let result = expected.result.commit();
        if class.program.len() > MAX_PROGRAM_BYTES
            || class.layout.commit_tiles.len() > MAX_BLOCKS * MAX_NODES_PER_BLOCK
            || class.layout.state_tiles.len() > MAX_STATES
            || job.network_id.is_empty()
            || job.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES
        {
            return Err(Error::Binding);
        }
        // Admit every persistent owner before identity hashing or index allocation.
        let index_bytes = mul(capture.entries().len() as u64, size_of::<(RecordKey, usize)>() as u64)?;
        let base = add(index_bytes, metadata_backing(class, job)?)?;
        if add(base, stage_backing(stages, LiveMaterialStage::default())?)? > limits.max_additional_heap_bytes {
            return Err(Error::Budget);
        }
        expected.rooted.check(ClaimForm::PositiveResultV2, result).map_err(|_| Error::Binding)?;
        if result.temperature_q24 != 0 || result.selector != greedy_selector_id() {
            return Err(Error::Binding);
        }
        if capture.origin().class != job.shape_profile_id
            || capture.origin().artifact != expected.artifact
            || digest(capture.origin().execution) != expected.rooted.expected_execution
            || result.execution != digest(job.context_hash())
            || result.job != digest(job.job_id)
            || result.registered_class != digest(class.class_id(&expected.artifact))
            || class.class_id(&expected.artifact) != capture.origin().class
            || result.artifact != digest(expected.artifact)
            || result.program != digest(class.graph_ir_root())
            || result.input != digest(job.prompt_token_ids_hash)
            || result.tokenizer != digest(job.tokenizer_id)
            || result.execution_rules != digest(job.cu_ruleset_id)
            || result.generation_seed != job.execution_seed
            || result.prefill != job.declared_prefill_tokens
            || result.decode != job.exact_decode_tokens
            || result.max_context != job.max_context_tokens
            || class.layout != expected.step_space.layout
            || class.program != expected.step_space.program.encode()
            || capture.run().generated.len() != job.exact_decode_tokens as usize
        {
            return Err(Error::Binding);
        }
        let count = expected.step_space.leaf_count_capped(job, limits.max_legacy_leaves).map_err(|_| Error::Budget)?;
        if count != capture.run().leaf_count || count != capture.run().leaf_hashes.len() as u64 {
            return Err(Error::Geometry);
        }
        let records = capture.entries().len();
        let bytes = mul(records as u64, size_of::<(RecordKey, usize)>() as u64)?;
        if records == 0
            || records as u64 > limits.max_index_records
            || bytes > limits.max_index_bytes
            || bytes > limits.max_additional_heap_bytes
        {
            return Err(Error::Budget);
        }
        let visits = mul(records as u64, u64::from(usize::BITS - records.leading_zeros()) + 2)?;
        if visits > limits.max_lookup_visits {
            return Err(Error::Budget);
        }
        let mut index = Vec::new();
        index.try_reserve_exact(records).map_err(|_| Error::Budget)?;
        if index.capacity() != records {
            return Err(Error::Budget);
        }
        for (i, r) in capture.entries().iter().enumerate() {
            let lanes = r.shape.iter().try_fold(1usize, |n, d| n.checked_mul(*d).ok_or(Error::Budget))?;
            if r.values.dtype() != r.dtype || r.values.len() != lanes {
                return Err(Error::Value);
            }
            index.push((key(r), i));
        }
        index.sort_unstable_by_key(|e| e.0);
        if index.windows(2).any(|w| w[0].0 == w[1].0) {
            return Err(Error::Geometry);
        }
        Ok(Self {
            capture,
            expected,
            index,
            limits,
            meter: RefCell::new(TypedAdapterMeter { lookup_visits: visits, ..Default::default() }),
            live_material: Cell::new(false),
            live_material_stage: Cell::new(LiveMaterialStage::default()),
            stages: Cell::new(stages),
        })
    }
    pub fn meter(&self) -> TypedAdapterMeter {
        *self.meter.borrow()
    }
    fn ensure_backing(&self, scratch: u64) -> Result<(), Error> {
        self.ensure_backing_with(self.stages.get(), self.live_material_stage.get(), scratch)
    }
    fn ensure_backing_with(&self, stages: StageReservations, material: LiveMaterialStage, scratch: u64) -> Result<(), Error> {
        // Count all exported claims conservatively as still live. A fresh
        // builder move cannot make returned buffers disappear from the quote.
        let index = mul(self.index.capacity() as u64, size_of::<(RecordKey, usize)>() as u64)?;
        let metadata = metadata_backing(self.expected.registered, self.expected.job)?;
        let live = add(add(index, self.meter.borrow().claim_bytes)?, add(metadata, add(stage_backing(stages, material)?, scratch)?)?)?;
        if live > self.limits.max_additional_heap_bytes {
            return Err(Error::Budget);
        }
        Ok(())
    }
    pub(crate) fn charge(&self, operation: AdapterCharge, amount: u64) -> Result<(), Error> {
        let mut m = self.meter.borrow_mut();
        let (value, limit) = match operation {
            AdapterCharge::LookupVisits => (&mut m.lookup_visits, self.limits.max_lookup_visits),
            AdapterCharge::ProofAttempts => (&mut m.proof_attempts, self.limits.max_proof_attempts),
            AdapterCharge::ProofLeafReads => (&mut m.proof_leaf_reads, self.limits.max_proof_leaf_reads),
            AdapterCharge::HashWork => (&mut m.hash_work, self.limits.max_hash_work),
            AdapterCharge::ClaimBytes => (&mut m.claim_bytes, self.limits.max_claims_heap_bytes),
            AdapterCharge::MaterialBytes => (&mut m.material_bytes, self.limits.max_material_bytes),
        };
        let next = add(*value, amount)?;
        if next > limit {
            return Err(Error::Budget);
        }
        *value = next;
        Ok(())
    }
    pub(crate) fn record(&self, key: RecordKey) -> Result<&TypedRecord, Error> {
        self.charge(AdapterCharge::LookupVisits, u64::from(usize::BITS - self.index.len().leading_zeros()) + 1)?;
        let i = self.index.binary_search_by_key(&key, |e| e.0).map_err(|_| Error::Missing)?;
        self.capture.entries().get(self.index[i].1).ok_or(Error::Geometry)
    }
    fn match_plan(&self, plan: &ExpandedPlan) -> Result<(), Error> {
        self.charge(AdapterCharge::LookupVisits, self.expected.registered.program.len() as u64 + 1024)?;
        if plan.job_context() != self.expected.job
            || plan.step_space().layout != self.expected.registered.layout
            || plan.step_space().program != self.expected.step_space.program
            || plan.summary().legacy_relations != self.capture.run().leaf_count
        {
            return Err(Error::Binding);
        }
        Ok(())
    }
    fn match_preparation(&self, prep: &FullRelationPreparation) -> Result<(), Error> {
        self.charge(AdapterCharge::LookupVisits, self.expected.registered.program.len() as u64 + 2048)?;
        if prep.result() != self.expected.result.commit()
            || prep.registered_class() != self.expected.registered
            || prep.job() != self.expected.job
            || prep.artifact() != self.expected.artifact
        {
            return Err(Error::Binding);
        }
        self.match_plan(prep.plan())
    }
    /// One fresh original binding; its active clone backing is part of the
    /// quote. Additional retained exports/copies require caller reservations;
    /// this interface does not track arbitrary caller-owned binding lifetimes.
    pub fn binding(&self) -> Result<PalwTirStepBindingV1, Error> {
        self.ensure_backing(0)?;
        Ok(PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: self.expected.job.clone(),
            class: self.expected.registered.clone(),
            artifact_root: self.expected.artifact,
            full_logits_trace_root: self.capture.run().trace_root,
            step_leaf_count: self.capture.run().leaf_count,
            step_merkle_root: self.capture.run().step_merkle_root,
            committed_execution_root: self.capture.run().execution_root,
        })
    }
    /// Only independently rederived relation indices are accepted. Uncommitted
    /// native values become comparison claims, never authenticated operands.
    pub fn expanded_claim(&self, plan: &ExpandedPlan, index: u64) -> Result<Vec<i128>, Error> {
        self.match_plan(plan)?;
        self.charge(AdapterCharge::LookupVisits, plan.summary().geometry_visit_bound)?;
        let relation = plan.relation_at(index).ok_or(Error::Geometry)?;
        let count = match &relation.kind {
            ExpandedRelationKind::Legacy(l) => u64::from(l.value_count),
            ExpandedRelationKind::RecomputeNode { lane_count, .. } => u64::from(*lane_count),
        };
        if count == 0 || count > self.limits.max_tile_lanes {
            return Err(Error::Budget);
        }
        self.charge(AdapterCharge::ClaimBytes, mul(count, 16)?)?;
        self.ensure_backing(add(mul(count, 4)?, 512)?)?;
        match relation.kind {
            ExpandedRelationKind::Legacy(l) => self.legacy_values(l.index),
            ExpandedRelationKind::RecomputeNode { ctx, block, layer, node, dtype, shape, first_lane, lane_count } => {
                let record = self.record(RecordKey::Node(ctx.pos, ctx.occurrence, node))?;
                let occurrence = self.expected.step_space.occurrences().get(ctx.occurrence as usize).ok_or(Error::Geometry)?;
                self.charge(AdapterCharge::LookupVisits, u64::from(ctx.occurrence) + 1)?;
                let mut slot = u32::from(node);
                for (b, _) in &self.expected.step_space.occurrences()[..ctx.occurrence as usize] {
                    slot = slot
                        .checked_add(self.expected.step_space.program.blocks[*b as usize].nodes.len() as u32)
                        .ok_or(Error::Budget)?;
                }
                if *occurrence != (block, layer)
                    || record.dtype != dtype
                    || record.shape != shape
                    || !matches!(record.kind,TypedRecordKind::Node{position,occurrence,slot:s,block:b,layer:l,node:n,committed:false}
                    if position==ctx.pos && occurrence==ctx.occurrence && s==slot && b==block && l==layer && n==node)
                {
                    return Err(Error::Geometry);
                }
                let start = usize::try_from(first_lane).map_err(|_| Error::Budget)?;
                let end = start.checked_add(lane_count as usize).filter(|e| *e <= record.values.len()).ok_or(Error::Geometry)?;
                let mut values = Vec::new();
                values.try_reserve_exact(lane_count as usize).map_err(|_| Error::Budget)?;
                if values.capacity() != lane_count as usize {
                    return Err(Error::Budget);
                }
                with_slice!(record.values.slice(),src=>{for value in &src[start..end] {values.push(value.to_i128());}});
                Ok(values)
            }
        }
    }
    pub fn all_claims(&self, plan: &ExpandedPlan, stages: StageReservations) -> Result<Vec<Vec<i128>>, Error> {
        self.match_plan(plan)?;
        let stages = self.stages.get().joined(stages);
        self.ensure_backing_with(stages, self.live_material_stage.get(), 0)?;
        self.stages.set(stages);
        let quote = quote_with_live(
            self.capture,
            self.expected.registered,
            self.expected.job,
            plan,
            stages,
            self.limits,
            self.live_material_stage.get(),
        )?;
        if add(quote.additional_peak_bytes, self.meter.borrow().claim_bytes)? > self.limits.max_additional_heap_bytes {
            return Err(Error::Budget);
        }
        let count = usize::try_from(plan.summary().relations).map_err(|_| Error::Budget)?;
        self.charge(AdapterCharge::ClaimBytes, mul(count as u64, size_of::<Vec<i128>>() as u64)?)?;
        self.ensure_backing(0)?;
        let mut claims = Vec::new();
        claims.try_reserve_exact(count).map_err(|_| Error::Budget)?;
        if claims.capacity() != count {
            return Err(Error::Budget);
        }
        for index in 0..count {
            claims.push(self.expanded_claim(plan, index as u64)?);
        }
        Ok(claims)
    }
    pub fn store<'b>(&'b self, source: &'b dyn PalwTirEvidenceStoreV1) -> TypedLegacyStore<'b, 'a> {
        TypedLegacyStore { adapter: self, source }
    }
    /// Frontier is read from a live A1 checker. A mismatch is refused before
    /// source discovery, and builder moves do not reset adapter session costs.
    #[allow(clippy::too_many_arguments)]
    pub fn material<'s>(
        &'s self,
        prep: &FullRelationPreparation,
        index: u64,
        checked_frontier: u64,
        source: &dyn PalwTirEvidenceStoreV1,
        rules: &PalwTirCourtRulesV1,
        limits: TirSourceOnlyLimitsV1,
    ) -> Result<TypedAdapterMaterial<'s, 'a>, Error> {
        self.match_preparation(prep)?;
        if rules.prompt_form != prep.prompt_form() || rules.max_step_leaf_count > self.limits.max_legacy_leaves {
            return Err(Error::Binding);
        }
        self.charge(AdapterCharge::LookupVisits, prep.plan().summary().geometry_visit_bound)?;
        let relation = prep.plan().relation_at(index).ok_or(Error::Geometry)?;
        if relation.legacy_frontier != checked_frontier {
            return Err(Error::Binding);
        }
        if self.live_material.get() {
            return Err(Error::Budget);
        }
        self.charge(AdapterCharge::MaterialBytes, limits.max_material_bytes)?;
        let stage = LiveMaterialStage {
            material_bytes: mul(limits.max_material_bytes, 2)?,
            source_auth_bytes: limits.max_auth_storage_bytes,
            // A1 workspace already includes target indices and results.
            evaluator_bytes: limits.max_workspace_bytes,
        };
        self.ensure_backing_with(self.stages.get(), stage, 0)?;
        let binding = self.binding()?;
        let store = self.store(source);
        self.live_material.set(true);
        self.live_material_stage.set(stage);
        let mut slot = BuildingMaterialSlot { flag: &self.live_material, stage: &self.live_material_stage, transferred: false };
        let material = match relation.kind {
            ExpandedRelationKind::Legacy(leaf) => build_source_only_legacy_v1(&binding, &leaf, &store, rules, limits),
            ExpandedRelationKind::RecomputeNode { ctx, node, first_lane, lane_count, .. } => {
                build_source_only_node_v1(&binding, checked_frontier, ctx, node, first_lane, lane_count, &store, rules, limits)
            }
        }
        .map_err(|e| Error::Source(format!("{e:?}")))?;
        slot.transferred = true;
        Ok(TypedAdapterMaterial { material, adapter: self })
    }
}

struct BuildingMaterialSlot<'a> {
    flag: &'a Cell<bool>,
    stage: &'a Cell<LiveMaterialStage>,
    transferred: bool,
}
impl Drop for BuildingMaterialSlot<'_> {
    fn drop(&mut self) {
        if !self.transferred {
            self.flag.set(false);
            self.stage.set(LiveMaterialStage::default());
        }
    }
}

/// One owned staging slot. Drop releases backing, never spent work counters.
/// No Clone/owned-buffer escape; explicit copies are the caller's reservation.
pub struct TypedAdapterMaterial<'s, 'a> {
    material: OwnedTirSourceOnlyMaterialV1,
    adapter: &'s TypedCaptureAdapter<'a>,
}
impl TypedAdapterMaterial<'_, '_> {
    pub fn as_borrowed(&self) -> TirSourceOnlyMaterialV1<'_> {
        self.material.as_borrowed()
    }
}
impl Drop for TypedAdapterMaterial<'_, '_> {
    fn drop(&mut self) {
        self.adapter.live_material.set(false);
        self.adapter.live_material_stage.set(LiveMaterialStage::default());
    }
}

/// Original authenticated side material supplies params/prompt/decode pins.
/// Step leaves/paths never delegate to a replaying store or uncommitted values.
pub struct TypedLegacyStore<'s, 'a> {
    adapter: &'s TypedCaptureAdapter<'a>,
    source: &'s dyn PalwTirEvidenceStoreV1,
}
impl PalwTirEvidenceStoreV1 for TypedLegacyStore<'_, '_> {
    fn begin_builder_move_v1(&self) {
        // No delegation: this hook may fetch/replay/reset hidden provider work.
    }
    fn step_leaf(&self, index: u64) -> Option<PalwStepTileLeafV1> {
        self.adapter.legacy_preimage(index).ok()
    }
    fn step_opening(&self, index: u64) -> Option<PalwStepOpeningV1> {
        self.adapter.legacy_opening(index).ok()
    }
    fn step_range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>> {
        self.adapter.legacy_range_siblings(first, count).ok()
    }
    fn param_opening(&self, leaf: u32) -> Option<PalwArtifactOpeningV1> {
        self.source.param_opening(leaf)
    }
    fn param_multiproof(&self, leaves: &[u32]) -> Option<PalwArtifactMultiproofV1> {
        self.source.param_multiproof(leaves)
    }
    fn param_carriage(&self, leaves: &[u32]) -> Option<PalwTirParamOpeningV1> {
        self.source.param_carriage(leaves)
    }
    fn prompt_token_ids(&self) -> Option<Vec<u32>> {
        self.source.prompt_token_ids()
    }
    fn prompt_ids_opening(&self, tile: u32) -> Option<PalwPromptIdsOpeningV1> {
        self.source.prompt_ids_opening(tile)
    }
    fn decode_pin(&self) -> Option<PalwDecodeTokenPinV1> {
        self.source.decode_pin()
    }
    fn row_pin(&self, row: u32, lane: u32) -> Option<PalwTiledDecodePinV1> {
        self.source.row_pin(row, lane)
    }
}
