use super::trace::discover;
use super::*;
use crate::node::{inventory::TirParamOpenerV1, memory::tir_full_tree_storage_need_v1, tree::TirStepTreeV1};
use kaspa_consensus_core::{
    palw_logits_q24_v2::{self as wide, WideLogitsEventV2},
    palw_positive_check_v2::{
        expanded_plan::{ExpandedRelation, ExpandedRelationKind},
        full_relation::wide_v2::{FullRelationPreparationV2, StepTargetV2},
    },
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1},
    palw_step_refute::{PALW_LOGITS_TILE_LANES, PalwStepInputRowV1},
    palw_tir_court_v1::{
        PalwTirCourtRulesV1, PalwTirInventoryIndexV1, PalwTirParamOpeningV1,
        source_only::{
            TirSourceOnlyLimitsV1,
            wide_v2::{TirSourceOnlyMaterialV2, WideGeneratedIdsPinV2, authenticate_source_only_v2},
        },
    },
    palw_tir_step_v1::{PalwTirLeafKindV1, palw_tir_leaf_values_v2},
};
use misaka_palw_positive_contract::{
    messages::{AdmissionContext, AuthenticatedResult, ClaimForm},
    tir::greedy_selector_id,
};
use misaka_palw_tir::{
    Ref,
    demand::{DemandContext, DemandLimits, carry_in_node_v1},
};
use std::cell::Cell;
/// Required owner bounds for the actual provider and coexisting stages. They
/// are not consensus authority. Unknown provider scratch remains a hard refusal.
#[derive(Clone, Copy)]
pub struct WideProviderEnvelopeV2 {
    pub per_call_io: u64,
    pub per_call_work: u64,
    pub max_result_bytes: u64,
}
#[derive(Clone, Copy)]
pub struct WidePanelOwnersV2 {
    pub metadata_auth_bytes: u64,
    pub metadata_auth_work: u64,
    pub source_auth_bytes: u64,
    pub signature_bytes: u64,
    pub retained_record_bytes: u64,
}
type BuiltMaterialV2 = (PalwStepInputRowV1, PalwTirParamOpeningV1, Vec<u32>, Option<WideGeneratedIdsPinV2>, Option<WidePanelTargetV2>);
pub struct WidePanelBuilderV2<'c, 'a> {
    pub(super) capture: &'c WidePanelCaptureV2<'a>,
    prep: &'c FullRelationPreparationV2,
    opener: &'c dyn TirParamOpenerV1,
    envelope: WideProviderEnvelopeV2,
    owners: WidePanelOwnersV2,
    pub(super) inventory: PalwTirInventoryIndexV1,
    binding: PalwTirStepBindingV1,
    tree: TirStepTreeV1,
    row_tree: TirStepTreeV1,
    tile_trees: Vec<TirStepTreeV1>,
    live: Cell<bool>,
    _lease: TirMemoryLeaseV1,
}
impl<'c, 'a> WidePanelBuilderV2<'c, 'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        capture: &'c WidePanelCaptureV2<'a>,
        prep: &'c FullRelationPreparationV2,
        result: &AuthenticatedResult<'_>,
        rooted: &AdmissionContext,
        opener: &'c dyn TirParamOpenerV1,
        envelope: WideProviderEnvelopeV2,
        owners: WidePanelOwnersV2,
        reserver: &TirMemoryReserverV1,
    ) -> Result<Self, Error> {
        if capture.builder_live.get() {
            return Err("wide capture already has a prepared builder".into());
        }
        let l = capture.session.limits;
        let e = &capture.expected;
        capture.session.charge(
            Charge::Visit,
            add(
                add(count(e.class.program.len())?, count(e.job.network_id.len())?)?,
                add(mul(count(e.class.layout.commit_tiles.len() + e.class.layout.state_tiles.len())?, 4)?, 1024)?,
            )?,
        )?;
        if prep.registered_class() != e.class
            || prep.job() != e.job
            || prep.artifact() != e.artifact
            || prep.result() != result.commit()
            || prep.prompt_form() != PalwPromptIdsFormV1::Flat
            || prep.result().selector != greedy_selector_id()
        {
            return Err("wide Panel independent preparation mismatch".into());
        }
        rooted.check(ClaimForm::PositiveResultV2, result.commit()).map_err(|e| format!("wide Panel rooted join: {e:?}"))?;
        if [
            owners.metadata_auth_bytes,
            owners.metadata_auth_work,
            owners.source_auth_bytes,
            owners.signature_bytes,
            owners.retained_record_bytes,
            envelope.per_call_io,
            envelope.per_call_work,
            envelope.max_result_bytes,
        ]
        .contains(&0)
            || envelope.max_result_bytes > l.max_material_bytes
        {
            return Err("wide Panel owner/provider envelope required".into());
        }
        // Conservative local request floor for repeated original metadata
        // decode/validation/ranges/StepSpace and the inventory's per-layer
        // instances. Caller owner declarations cannot silently choose zero or
        // ignore the param/state x occurrence expansion. This is not RSS or a
        // native resource admission; the upstream metadata owner remains required.
        let p = e.space.program();
        let layers = add(count(p.schedule.layers.len())?, 1)?;
        let metadata_floor = [
            mul(count(e.class.program.len())?, 512)?,
            mul(count(p.params.len())?, add(1024, mul(layers, 32)?)?)?,
            mul(count(p.states.len())?, add(1024, mul(layers, 256)?)?)?,
            mul(count(e.class.layout.commit_tiles.len() + e.class.layout.state_tiles.len())?, 32)?,
            65536,
        ]
        .into_iter()
        .try_fold(0, add)?;
        let work_floor =
            add(mul(count(e.class.program.len())?, 128)?, mul(mul(count(p.params.len() + p.states.len())?, layers)?, 64)?)?;
        if owners.metadata_auth_bytes < metadata_floor || owners.metadata_auth_work < work_floor {
            return Err("wide metadata owner below derived original-program request floor".into());
        }
        let trees =
            capture.run.logits_rows().iter().try_fold(tir_full_tree_storage_need_v1(capture.run.leaf_count())?, |a, row| {
                add(a, tir_full_tree_storage_need_v1(row.len().div_ceil(PALW_LOGITS_TILE_LANES) as u64)?)
            })?;
        let heap = [
            l.external_retained_bytes,
            trees,
            tir_full_tree_storage_need_v1(capture.run.generated().len() as u64)?,
            mul(l.max_trace_frames, 1024)?,
            mul(l.max_material_bytes, 4)?,
            mul(l.max_claim_bytes, 2)?,
            l.max_provider_scratch,
            owners.metadata_auth_bytes,
            owners.source_auth_bytes,
            owners.signature_bytes,
            owners.retained_record_bytes,
        ]
        .into_iter()
        .try_fold(0, add)?;
        if add(add(heap, capture.retained_heap)?, capture.run.reserved_bytes())? > l.max_live_heap_bytes {
            return Err("wide Panel builder simultaneous owners cap".into());
        }
        let lease = reserver("positive-wide-panel-builder-v2", e.job.shape_profile_id, capture.context, heap)?;
        capture.session.charge(Charge::Hash, mul(capture.run.leaf_count(), 4)?)?;
        capture.session.charge(Charge::Io, mul(capture.run.leaf_count(), 64)?)?;
        let tree = TirStepTreeV1::full(capture.run.leaf_hashes());
        if tree.root() != Some(capture.run.step_merkle_root()) {
            return Err("wide Panel original tree root".into());
        }
        let mut tile_trees = vec_exact(capture.run.logits_rows().len())?;
        let mut roots = vec_exact(capture.run.logits_rows().len())?;
        for (row, lanes) in capture.run.logits_rows().iter().enumerate() {
            capture.session.charge(
                Charge::Hash,
                add(mul(lanes.len() as u64, 8)?, mul(lanes.len().div_ceil(PALW_LOGITS_TILE_LANES) as u64, 4)?)?,
            )?;
            let hashes = lanes
                .chunks(PALW_LOGITS_TILE_LANES)
                .enumerate()
                .map(|(tile, values)| wide::tile_leaf(&capture.context, row as u32, tile as u32, values))
                .collect::<Vec<_>>();
            let tree = TirStepTreeV1::full(&hashes);
            roots.push(tree.root().ok_or("wide empty row tree")?);
            tile_trees.push(tree)
        }
        capture.session.charge(Charge::Hash, mul(roots.len() as u64, 4)?)?;
        let row_tree = TirStepTreeV1::full(&roots);
        if wide::outer_root(e.job, roots.len() as u64, &row_tree.root().ok_or("wide empty rows")?, capture.run.generated())
            != capture.run.trace_root()
        {
            return Err("wide Panel trace tree root".into());
        }
        capture.session.charge(Charge::Visit, owners.metadata_auth_work)?;
        let inventory = PalwTirInventoryIndexV1::new(e.space.program()).ok_or("wide Panel inventory")?;
        let binding = capture.binding()?;
        capture.builder_live.set(true);
        Ok(Self {
            capture,
            prep,
            opener,
            envelope,
            owners,
            inventory,
            binding,
            tree,
            row_tree,
            tile_trees,
            live: Cell::new(false),
            _lease: lease,
        })
    }
    pub fn meter(&self) -> WidePanelMeterV2 {
        self.capture.meter()
    }
    pub fn plan(&self) -> &kaspa_consensus_core::palw_positive_check_v2::expanded_plan::wide_v2::ExpandedPlanV2 {
        self.prep.plan()
    }
    pub(super) fn session(&self) -> &Session {
        &self.capture.session
    }
    fn relation(&self, index: u64, frontier: u64) -> Result<ExpandedRelation, Error> {
        self.capture.session.charge(Charge::Visit, self.prep.plan().summary().geometry_visit_bound)?;
        let r = self.prep.plan().relation_at(index).ok_or("wide relation index")?;
        if r.legacy_frontier != frontier {
            return Err("wide relation frontier mismatch".into());
        }
        Ok(r)
    }
    fn rules(&self, limits: TirSourceOnlyLimitsV1) -> PalwTirCourtRulesV1 {
        PalwTirCourtRulesV1 {
            max_step_leaf_count: self.capture.session.limits.max_step_leaves,
            prompt_form: self.prep.prompt_form(),
            limits: DemandLimits { max_elements: limits.max_target_lanes, max_terms: self.capture.session.limits.max_visits },
        }
    }
    fn parameters(&self, leaves: &[u32], limits: TirSourceOnlyLimitsV1) -> Result<PalwTirParamOpeningV1, Error> {
        if leaves.is_empty() {
            return Ok(PalwTirParamOpeningV1::None);
        }
        self.capture.session.charge(Charge::Provider, self.envelope.per_call_io)?;
        self.capture.session.charge(Charge::Io, self.envelope.per_call_io)?;
        self.capture.session.charge(Charge::Visit, self.envelope.per_call_work)?;
        let scratch = self.opener.param_scratch_storage_v1(leaves).ok_or("wide provider scratch unknown")?;
        if scratch > self.capture.session.limits.max_provider_scratch {
            return Err("wide provider scratch cap".into());
        }
        let proof = self.opener.param_carriage(leaves).ok_or("wide provider missing inventory proof")?;
        let empty = PalwStepInputRowV1 { preimages: vec![], run_siblings: vec![] };
        let material = TirSourceOnlyMaterialV2 {
            operands: &empty,
            params: &proof,
            prompt_token_ids: &[],
            prompt_ids_openings: &[],
            decode_tokens: None,
        };
        let q = material.preflight(limits).map_err(|e| format!("wide provider response bounds: {e:?}"))?;
        if q.material_bytes > self.envelope.max_result_bytes {
            return Err("wide provider exceeded quote".into());
        }
        self.capture.session.charge(Charge::Hash, add(self.owners.metadata_auth_work, mul(q.material_bytes, 8)?)?)?;
        let authenticated =
            authenticate_source_only_v2(&self.binding, material, self.binding.step_leaf_count, &self.rules(limits), limits)
                .map_err(|e| format!("wide provider provenance: {e:?}"))?;
        if !authenticated.authenticated_parameter_leaves().eq(leaves.iter().copied()) {
            return Err("wide provider returned a different requested leaf set".into());
        }
        drop(authenticated);
        Ok(proof)
    }
    pub(super) fn ref_shape(&self, ctx: DemandContext, r: Ref) -> Result<Vec<usize>, Error> {
        self.capture.session.charge(Charge::Visit, 1)?;
        let space = self.capture.expected.space;
        let (block, _) = space.occurrences()[ctx.occurrence as usize];
        let p = space.program();
        Ok(match r {
            Ref::Node(n) => {
                let h = misaka_palw_tir::demand::history_length_v1(space.info(), block, ctx.pos).ok_or("wide shape history")?;
                p.blocks[block as usize].nodes[n as usize].out.resolve(h)
            }
            Ref::CarryIn(k) => p.blocks[block as usize].carry_in[k as usize].resolve(1),
            Ref::Param(j) => p.params[j as usize].shape.iter().map(|n| *n as usize).collect(),
            Ref::Const(j) => p.consts[j as usize].shape.iter().map(|n| *n as usize).collect(),
            Ref::State(j) => p.states[j as usize].shape.iter().map(|n| *n as usize).collect(),
            Ref::Input(_) => vec![],
        })
    }
    pub(super) fn ref_value(&self, ctx: DemandContext, r: Ref, lane: usize) -> Result<i128, Error> {
        let space = self.capture.expected.space;
        let (_, layer) = space.occurrences()[ctx.occurrence as usize];
        let p = space.program();
        match r {
            Ref::Node(n) => self.capture.value(Key::Node(ctx.pos, ctx.occurrence, n), lane),
            Ref::CarryIn(k) => {
                let (c, n) = carry_in_node_v1(p, ctx, k).ok_or("wide dynamic carry")?;
                self.capture.value(Key::Node(c.pos, c.occurrence, n), lane)
            }
            Ref::State(s) => {
                if ctx.pos == 0 {
                    return Ok(0);
                }
                let layer = p.states[s as usize].per_layer.then_some(layer).flatten();
                self.capture.value(Key::Fixed(ctx.pos - 1, s, layer), lane)
            }
            Ref::Input(j) => {
                if j == 0 {
                    let job = space.job_shape(self.capture.expected.job).map_err(|e| e.to_string())?;
                    let id = if ctx.pos < job.prefill {
                        *self.capture.expected.prompt.get(ctx.pos as usize).ok_or("wide prompt missing")?
                    } else {
                        *self.capture.run.generated().get((ctx.pos - job.prefill) as usize).ok_or("wide generated missing")?
                    };
                    Ok(i128::from(id))
                } else {
                    Ok(i128::from(ctx.pos))
                }
            }
            Ref::Const(j) => {
                let c = &p.consts[j as usize];
                let width = c.dtype.width();
                let from = lane.checked_mul(width).ok_or("wide const byte overflow")?;
                Ok(c.dtype.decode_le(c.data.get(from..from + width).ok_or("wide const lane")?))
            }
            Ref::Param(j) => {
                let d = &p.params[j as usize];
                let layer = d.per_layer.then_some(layer).flatten();
                let leaf =
                    self.inventory.leaf_of(j, layer, mul(count(lane)?, d.dtype.width() as u64)?).ok_or("wide dynamic param leaf")?;
                let limits = self.source_limits();
                let proof = self.parameters(&[leaf], limits)?;
                let empty = PalwStepInputRowV1 { preimages: vec![], run_siblings: vec![] };
                let material = TirSourceOnlyMaterialV2 {
                    operands: &empty,
                    params: &proof,
                    prompt_token_ids: &[],
                    prompt_ids_openings: &[],
                    decode_tokens: None,
                };
                self.capture.session.charge(Charge::Hash, self.owners.metadata_auth_work)?;
                let auth =
                    authenticate_source_only_v2(&self.binding, material, self.binding.step_leaf_count, &self.rules(limits), limits)
                        .map_err(|e| format!("wide dynamic artifact auth: {e:?}"))?;
                auth.authenticated_parameter_lane(j, layer, lane).ok_or("wide dynamic artifact lane".into())
            }
        }
    }
    fn source_limits(&self) -> TirSourceOnlyLimitsV1 {
        let l = self.capture.session.limits;
        TirSourceOnlyLimitsV1 {
            max_source_reads: l.max_visits,
            max_target_lanes: l.max_trace_frames,
            max_target_bytes: l.max_material_bytes,
            max_material_bytes: l.max_material_bytes,
            max_material_units: l.max_trace_frames,
            max_auth_storage_bytes: self.owners.source_auth_bytes,
            max_workspace_bytes: self.owners.metadata_auth_bytes,
        }
    }
    /// Extract only comparison claims from the same run, preserving original i128
    /// integers. These values never become Node source operands or computation credit.
    pub fn claim(&self, index: u64) -> Result<Vec<i128>, Error> {
        self.capture.session.charge(Charge::Visit, self.prep.plan().summary().geometry_visit_bound)?;
        let r = self.prep.plan().relation_at(index).ok_or("wide claim index")?;
        let n = match r.kind {
            ExpandedRelationKind::Legacy(l) => l.value_count,
            ExpandedRelationKind::RecomputeNode { lane_count, .. } => lane_count,
        };
        let bytes = mul(u64::from(n), 16)?;
        if bytes > self.capture.session.limits.max_claim_bytes {
            return Err("wide claim tile cap".into());
        }
        self.capture.session.charge(Charge::Copy, bytes)?;
        let out = match r.kind {
            ExpandedRelationKind::Legacy(l) => {
                let preimage = self.capture.preimage(l.index)?;
                // Canonical leaf decoding performs its own original-geometry lookup.
                self.capture.session.charge(Charge::Visit, self.prep.plan().summary().geometry_visit_bound)?;
                palw_tir_leaf_values_v2(self.capture.expected.space, self.capture.expected.job, &l, &preimage)
                    .map_err(|e| e.to_string())?
            }
            ExpandedRelationKind::RecomputeNode { ctx, node, first_lane, lane_count, .. } => {
                self.capture.values(Key::Node(ctx.pos, ctx.occurrence, node), first_lane, lane_count)?
            }
        };
        Ok(out)
    }
    pub fn material(&self, index: u64, frontier: u64) -> Result<WidePanelMaterialV2<'_, 'c, 'a>, Error> {
        if self.live.replace(true) {
            return Err("wide Panel material slot already live".into());
        }
        let result = self.material_inner(index, frontier);
        match result {
            Ok((operands, params, prompt, pin, target)) => {
                Ok(WidePanelMaterialV2 { builder: self, operands, params, prompt, pin, target })
            }
            Err(e) => {
                self.live.set(false);
                Err(e)
            }
        }
    }
    fn material_inner(&self, index: u64, frontier: u64) -> Result<BuiltMaterialV2, Error> {
        self.capture.session.charge(Charge::Copy, mul(self.capture.session.limits.max_material_bytes, 2)?)?;
        let r = self.relation(index, frontier)?;
        let reads = discover(self, &r)?;
        let steps = reads.steps.iter().copied().collect::<Vec<_>>();
        // Bound all source copies/proofs BEFORE reading a leaf or calling a provider.
        // Range proofs have at most two siblings per tree level (u64 => <=128).
        let mut runs = 0u64;
        let mut previous = None;
        let mut bound = 256u64;
        for index in &steps {
            bound = add(bound, add(64, self.capture.preimage_bytes(*index)?)?)?;
            if previous.is_none_or(|p: u64| p.checked_add(1) != Some(*index)) {
                runs += 1;
            }
            previous = Some(*index);
        }
        bound = add(bound, mul(runs, add(4, 128 * 64)?)?)?;
        if !reads.params.is_empty() {
            bound = add(bound, self.envelope.max_result_bytes)?;
        }
        if reads.prompt {
            bound = add(bound, mul(count(self.capture.expected.prompt.len())?, 4)?)?;
        }
        if reads.decode {
            bound = add(bound, add(68, mul(count(self.capture.run.generated().len())?, 4)?)?)?;
        }
        if bound > self.capture.session.limits.max_material_bytes {
            return Err("wide material pre-copy cap".into());
        }
        let mut preimages = vec_exact(steps.len())?;
        for i in &steps {
            preimages.push(self.capture.preimage(*i)?)
        }
        let mut siblings = vec![];
        let mut k = 0;
        while k < steps.len() {
            let mut n = 1;
            while k + n < steps.len() && steps[k + n] == steps[k] + n as u64 {
                n += 1
            }
            self.capture.session.charge(Charge::Proof, 1)?;
            self.capture.session.charge(Charge::Visit, 128)?;
            self.capture.session.charge(Charge::Io, 128 * 64)?;
            siblings.push(self.tree.range_siblings(steps[k], n as u64).ok_or("wide source range proof")?);
            k += n;
        }
        let operands = PalwStepInputRowV1 { preimages, run_siblings: siblings };
        let params = self.parameters(&reads.params.iter().copied().collect::<Vec<_>>(), self.source_limits())?;
        let prompt = if reads.prompt { self.capture.expected.prompt.to_vec() } else { vec![] };
        let pin = reads.decode.then(|| WideGeneratedIdsPinV2 {
            rows_root: self.row_tree.root().expect("prepared rows"),
            generated_token_ids: self.capture.run.generated().to_vec(),
        });
        let target = self.target(&r)?;
        let material = TirSourceOnlyMaterialV2 {
            operands: &operands,
            params: &params,
            prompt_token_ids: &prompt,
            prompt_ids_openings: &[],
            decode_tokens: pin.as_ref(),
        };
        material.preflight(self.source_limits()).map_err(|e| format!("wide assembled material bound: {e:?}"))?;
        Ok((operands, params, prompt, pin, target))
    }
    fn target(&self, r: &ExpandedRelation) -> Result<Option<WidePanelTargetV2>, Error> {
        let ExpandedRelationKind::Legacy(leaf) = r.kind else { return Ok(None) };
        self.capture.session.charge(Charge::Proof, 1)?;
        self.capture.session.charge(Charge::Visit, 256)?;
        self.capture.session.charge(Charge::Io, 128 * 64 * 3)?;
        let mut bound = add(self.capture.preimage_bytes(leaf.index)?, add(512, mul(count(self.capture.run.generated().len())?, 4)?)?)?;
        if leaf.dtype == DType::I64 {
            bound = add(bound, add((PALW_LOGITS_TILE_LANES as u64) * 8, 128 * 64 * 3)?)?;
        }
        if bound > self.capture.session.limits.max_material_bytes {
            return Err("wide target pre-copy cap".into());
        }
        let opening = self.tree.opening(leaf.index).ok_or("wide target opening")?;
        let preimage = self.capture.preimage(leaf.index)?;
        let trace = if leaf.dtype == DType::I64 {
            let PalwTirLeafKindV1::Commit { first_element, .. } = leaf.kind else { return Err("wide non-final target".into()) };
            let row = leaf.position + 1 - self.capture.expected.job.declared_prefill_tokens;
            let tile = first_element / PALW_LOGITS_TILE_LANES as u64;
            self.capture.session.charge(Charge::Proof, 2)?;
            let all = &self.capture.run.logits_rows()[row as usize];
            let from = tile as usize * PALW_LOGITS_TILE_LANES;
            let lanes = all[from..all.len().min(from + PALW_LOGITS_TILE_LANES)].to_vec();
            Some(WideLogitsEventV2 {
                row,
                generated: self.capture.run.generated().to_vec(),
                row_root: self.tile_trees[row as usize].root().ok_or("wide row root")?,
                row_opening: self.row_tree.opening(u64::from(row)).ok_or("wide row proof")?,
                lanes,
                tile_opening: self.tile_trees[row as usize].opening(tile).ok_or("wide tile proof")?,
            })
        } else {
            None
        };
        Ok(Some(WidePanelTargetV2 { opening, preimage, trace }))
    }
}
pub struct WidePanelTargetV2 {
    pub opening: PalwStepOpeningV1,
    pub preimage: PalwStepTileLeafV1,
    pub trace: Option<WideLogitsEventV2>,
}
impl WidePanelTargetV2 {
    pub fn as_borrowed(&self) -> StepTargetV2<'_> {
        StepTargetV2 { opening: &self.opening, preimage: &self.preimage, trace: self.trace.as_ref() }
    }
}
pub struct WidePanelMaterialV2<'b, 'c, 'a> {
    builder: &'b WidePanelBuilderV2<'c, 'a>,
    operands: PalwStepInputRowV1,
    params: PalwTirParamOpeningV1,
    prompt: Vec<u32>,
    pin: Option<WideGeneratedIdsPinV2>,
    target: Option<WidePanelTargetV2>,
}
impl WidePanelMaterialV2<'_, '_, '_> {
    pub fn as_borrowed(&self) -> TirSourceOnlyMaterialV2<'_> {
        TirSourceOnlyMaterialV2 {
            operands: &self.operands,
            params: &self.params,
            prompt_token_ids: &self.prompt,
            prompt_ids_openings: &[],
            decode_tokens: self.pin.as_ref(),
        }
    }
    pub fn target(&self) -> Option<StepTargetV2<'_>> {
        self.target.as_ref().map(WidePanelTargetV2::as_borrowed)
    }
}
impl Drop for WidePanelMaterialV2<'_, '_, '_> {
    fn drop(&mut self) {
        self.builder.live.set(false)
    }
}

impl Drop for WidePanelBuilderV2<'_, '_> {
    fn drop(&mut self) {
        self.capture.builder_live.set(false)
    }
}
