//! Explicit strict-wide source authentication and direct i128 evaluation.
//! Input step operands remain narrow: final post I64 is never another node's input.
//! No wire enum/domain, native authority, saved-success capability or store fallback.
//! Limits below are logical bounds. Program decode/range/inventory analysis,
//! Merkle verification, retained material and evaluator/signature heaps need
//! their owner's actual simultaneous reservation before entry. This API does
//! not allocate native resource rights, establish provenance, or emit a verdict.
use super::super::{
    PalwTirCourtRulesV1, PalwTirInventoryIndexV1, PalwTirParamOpeningV1, Requests, TirSource, Units, authenticate_operands,
    authenticate_params, authenticate_prompt, evaluate_leaf_with_work, first_outside, leaf_interval_geometry,
};
use super::{TirSourceOnlyErrorV1, TirSourceOnlyLimitsV1, TirSourceOnlyMaterialV1, TirSourceOnlyQuoteV1, add, mul, n64};
use crate::{
    Hash64,
    palw_native_resource_v1::btree_node_envelope_bytes_v1,
    palw_prompt_ids_v1::{PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1, PalwPromptIdsOpeningV1},
    palw_step_refute::{PalwStepInputRowV1, PalwStepRefuteError},
    palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1,
    palw_tir_step_v1::{
        PalwTirLeafV1, PalwTirStepBindingV1,
        wide_v2::{PalwTirVerifiedBindingV2, verify_tir_binding_v2},
    },
    palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PALW_V2_MAX_NETWORK_ID_BYTES},
};
use misaka_palw_tir::{
    DType,
    demand::{DemandContext, DemandError, DemandRequest, DemandTarget, DemandWork, eval_demanded, history_length_v1},
    interval::analyze_ranges,
    program::{MAX_BLOCKS, MAX_NODE_INPUTS, MAX_NODES_PER_BLOCK, MAX_PROGRAM_BYTES, MAX_STATES, TIR_PROGRAM_VERSION_V1},
    types::MAX_RANK,
};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
/// Local typed data only; outer_root(v2) authenticates IDs when the source factory uses it.
/// No new wire codec/enum or computation assertion is assigned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WideGeneratedIdsPinV2 {
    pub rows_root: Hash64,
    pub generated_token_ids: Vec<u32>,
}
impl WideGeneratedIdsPinV2 {
    fn authenticate(&self, binding: &PalwTirStepBindingV1) -> Result<Vec<u32>, PalwStepRefuteError> {
        let ctx = &binding.job_context;
        if self.generated_token_ids.len() != ctx.exact_decode_tokens as usize
            || crate::palw_logits_q24_v2::outer_root(
                ctx,
                u64::from(ctx.exact_decode_tokens),
                &self.rows_root,
                &self.generated_token_ids,
            ) != binding.full_logits_trace_root
        {
            return Err(PalwStepRefuteError::InputSetNotCanonical("wide generated IDs do not reproduce the bound trace root"));
        }
        Ok(self.generated_token_ids.clone())
    }
}
#[derive(Clone, Copy)]
pub struct TirSourceOnlyMaterialV2<'a> {
    pub operands: &'a PalwStepInputRowV1,
    pub params: &'a PalwTirParamOpeningV1,
    pub prompt_token_ids: &'a [u32],
    pub prompt_ids_openings: &'a [PalwPromptIdsOpeningV1],
    pub decode_tokens: Option<&'a WideGeneratedIdsPinV2>,
}
impl TirSourceOnlyMaterialV2<'_> {
    pub fn preflight(self, limits: TirSourceOnlyLimitsV1) -> Result<TirSourceOnlyQuoteV1, TirSourceOnlyErrorV1> {
        let mut q = TirSourceOnlyMaterialV1 {
            operands: self.operands,
            params: self.params,
            prompt_token_ids: self.prompt_token_ids,
            prompt_ids_openings: self.prompt_ids_openings,
            decode_tokens: None,
        }
        .preflight(limits)?;
        if let Some(pin) = self.decode_tokens {
            let ids = n64(pin.generated_token_ids.len())?;
            q.material_units = add(q.material_units, ids)?;
            // The base None already charges the local Option discriminant; add the
            // v2 pin's root, Vec length, and IDs. This is a local count, not a codec.
            q.material_bytes = add(q.material_bytes, add(68, mul(ids, 4)?)?)?;
            q.logical_auth_storage_bytes = add(q.logical_auth_storage_bytes, mul(ids, 4)?)?;
            if q.material_units > limits.max_material_units
                || q.material_bytes > limits.max_material_bytes
                || q.logical_auth_storage_bytes > limits.max_auth_storage_bytes
            {
                return Err(TirSourceOnlyErrorV1::Budget);
            }
        }
        Ok(q)
    }
}
use super::evaluation_error;
pub struct AuthenticatedTirSourceOnlyV2<'a> {
    binding: &'a PalwTirStepBindingV1,
    material: TirSourceOnlyMaterialV2<'a>,
    frontier: u64,
    verified: PalwTirVerifiedBindingV2,
    inventory: PalwTirInventoryIndexV1,
    operands: Vec<(u64, PalwTirLeafV1)>,
    params: BTreeMap<u32, (u32, Vec<u8>)>,
    prompt: BTreeMap<u32, u32>,
    prompt_tiles: BTreeSet<u32>,
    generated: Option<Vec<u32>>,
    rules: PalwTirCourtRulesV1,
    limits: TirSourceOnlyLimitsV1,
    quote: TirSourceOnlyQuoteV1,
}

pub fn authenticate_source_only_v2<'a>(
    binding: &'a PalwTirStepBindingV1,
    material: TirSourceOnlyMaterialV2<'a>,
    legacy_frontier: u64,
    rules: &PalwTirCourtRulesV1,
    limits: TirSourceOnlyLimitsV1,
) -> Result<AuthenticatedTirSourceOnlyV2<'a>, TirSourceOnlyErrorV1> {
    let quote = material.preflight(limits)?;
    let verified = binding_geometry(binding, legacy_frontier, rules)?;
    let intervals = analyze_ranges(verified.space().program()).map_err(|_| TirSourceOnlyErrorV1::Binding)?;
    let operands = authenticate_operands(binding, &verified, material.operands, legacy_frontier, rules.max_step_leaf_count)
        .map_err(TirSourceOnlyErrorV1::Material)?;
    let inventory = PalwTirInventoryIndexV1::new(verified.space().program()).ok_or(TirSourceOnlyErrorV1::Binding)?;
    let params = authenticate_params(binding, &verified, &inventory, material.params).map_err(TirSourceOnlyErrorV1::Material)?;
    let (prompt, prompt_tiles) =
        authenticate_prompt(&binding.job_context, rules.prompt_form, material.prompt_token_ids, material.prompt_ids_openings)
            .map_err(TirSourceOnlyErrorV1::Material)?;
    let generated = material.decode_tokens.map(|pin| pin.authenticate(binding)).transpose().map_err(TirSourceOnlyErrorV1::Material)?;
    for ((_, leaf), preimage) in operands.iter().zip(&material.operands.preimages) {
        if !leaf.dtype.committable() {
            return Err(TirSourceOnlyErrorV1::Binding);
        }
        let interval = leaf_interval_geometry(verified.space(), &intervals, leaf).ok_or(TirSourceOnlyErrorV1::Binding)?;
        if first_outside(leaf.dtype, &preimage.values_le, interval).is_some() {
            return Err(TirSourceOnlyErrorV1::Material(PalwStepRefuteError::InputSetNotCanonical(
                "source lane outside registered interval",
            )));
        }
    }
    Ok(AuthenticatedTirSourceOnlyV2 {
        binding,
        material,
        frontier: legacy_frontier,
        verified,
        inventory,
        operands,
        params,
        prompt,
        prompt_tiles,
        generated,
        rules: *rules,
        limits,
        quote,
    })
}

/// Exact local evaluation output only, not a candidate/Miner verdict.
pub struct TirSourceOnlyEvaluationV2 {
    values: Vec<i128>,
    work: DemandWork,
    source_reads: u64,
    dtype: DType,
    shape: Vec<usize>,
    workspace_bytes: u64,
}
impl TirSourceOnlyEvaluationV2 {
    pub fn values(&self) -> &[i128] {
        &self.values
    }
    pub fn work(&self) -> DemandWork {
        self.work
    }
    pub fn source_reads(&self) -> u64 {
        self.source_reads
    }
    pub fn dtype(&self) -> DType {
        self.dtype
    }
    /// Original resolved tensor shape for Node; flattened tile shape for Legacy.
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }
    /// Conservative logical reservation, not measured allocator or process RSS.
    pub fn workspace_bytes(&self) -> u64 {
        self.workspace_bytes
    }
}

impl AuthenticatedTirSourceOnlyV2<'_> {
    /// Artifact identity only: read an already authenticated parameter lane.
    /// This does not evaluate a model or authenticate a Panel intermediate.
    /// Local producer geometry may use it to resolve a dynamic address; Node
    /// still independently recomputes the address and canonical read closure.
    /// Sorted inventory coordinates authenticated by artifact commitments only.
    pub fn authenticated_parameter_leaves(&self) -> impl Iterator<Item = u32> + '_ {
        self.params.keys().copied()
    }
    pub fn authenticated_parameter_lane(&self, param: u16, layer: Option<u16>, index: usize) -> Option<i128> {
        let d = self.verified.space().program().params.get(usize::from(param))?;
        let byte = u64::try_from(index).ok()?.checked_mul(d.dtype.width() as u64)?;
        let leaf = self.inventory.leaf_of(param, layer, byte)?;
        let (start, bytes) = self.params.get(&leaf)?;
        let first = usize::try_from(byte.checked_sub(u64::from(*start))?).ok()?;
        Some(d.dtype.decode_le(bytes.get(first..first.checked_add(d.dtype.width())?)?))
    }
    pub fn quote(&self) -> TirSourceOnlyQuoteV1 {
        self.quote
    }
    pub fn evaluate_node(
        self,
        ctx: DemandContext,
        node: u16,
        first_lane: u64,
        lane_count: u32,
    ) -> Result<TirSourceOnlyEvaluationV2, TirSourceOnlyErrorV1> {
        self.evaluate(Target::Node { ctx, node, first_lane, lane_count })
    }
    pub fn evaluate_legacy(self, leaf: &PalwTirLeafV1) -> Result<TirSourceOnlyEvaluationV2, TirSourceOnlyErrorV1> {
        self.evaluate(Target::Legacy(*leaf))
    }
    fn evaluate(self, target: Target) -> Result<TirSourceOnlyEvaluationV2, TirSourceOnlyErrorV1> {
        let mut geometry = target_geometry(&self.verified, self.binding, self.frontier, target, self.limits)?;
        let workspace_bytes = workspace_quote(&self.verified, geometry.lane_count, &self.rules, self.limits)?;
        geometry.prepare_elements(target)?;
        let reads = Rc::new(Cell::new(0));
        let mut source = TirSource {
            space: self.verified.space(),
            ctx: &self.binding.job_context,
            job: self.verified.job(),
            inventory: &self.inventory,
            units: Units {
                steps: self
                    .operands
                    .iter()
                    .zip(&self.material.operands.preimages)
                    .map(|((index, _), leaf)| (*index, leaf.values_le.clone()))
                    .collect(),
                params: self.params,
                prompt: self.prompt,
                generated: self.generated,
                store: None,
                prompt_form: self.rules.prompt_form,
            },
            before: self.frontier,
            used: Requests::default(),
            leaf_cache: BTreeMap::new(),
            supplied_ctx: None,
            supplied: BTreeMap::new(),
            used_supplied: BTreeSet::new(),
            read_budget: Some((reads.clone(), self.limits.max_source_reads)),
        };
        let (values, work) = evaluate_target(&self.verified, target, &geometry, &mut source, &self.rules)
            .map_err(|error| evaluation_error(error, reads.get(), self.limits.max_source_reads))?;
        canonical(self.material, &self.operands, &self.prompt_tiles, &source.used, self.rules.prompt_form)?;
        Ok(TirSourceOnlyEvaluationV2 {
            values,
            work,
            source_reads: reads.get(),
            dtype: geometry.dtype,
            shape: geometry.shape,
            workspace_bytes,
        })
    }
}

#[derive(Clone, Copy)]
enum Target {
    Node { ctx: DemandContext, node: u16, first_lane: u64, lane_count: u32 },
    Legacy(PalwTirLeafV1),
}
struct TargetGeometry {
    dtype: DType,
    shape: Vec<usize>,
    elements: Vec<usize>,
    first_lane: u64,
    lane_count: u32,
}
impl TargetGeometry {
    fn prepare_elements(&mut self, target: Target) -> Result<(), TirSourceOnlyErrorV1> {
        if matches!(target, Target::Node { .. }) {
            self.elements.try_reserve_exact(self.lane_count as usize).map_err(|_| TirSourceOnlyErrorV1::Budget)?;
            for lane in self.first_lane..self.first_lane + u64::from(self.lane_count) {
                self.elements.push(usize::try_from(lane).map_err(|_| TirSourceOnlyErrorV1::Target)?);
            }
        }
        Ok(())
    }
}

fn target_geometry(
    v: &PalwTirVerifiedBindingV2,
    binding: &PalwTirStepBindingV1,
    frontier: u64,
    target: Target,
    limits: TirSourceOnlyLimitsV1,
) -> Result<TargetGeometry, TirSourceOnlyErrorV1> {
    let (dtype, shape, first, count) = match target {
        Target::Node { ctx, node, first_lane, lane_count } => {
            if ctx.pos >= v.job().positions
                || (usize::from(ctx.occurrence) + 1 == v.space().occurrences().len() && !v.job().runs_post(ctx.pos))
            {
                return Err(TirSourceOnlyErrorV1::Target);
            }
            let (block, _) = *v.space().occurrences().get(usize::from(ctx.occurrence)).ok_or(TirSourceOnlyErrorV1::Target)?;
            let node =
                v.space().program().blocks[usize::from(block)].nodes.get(usize::from(node)).ok_or(TirSourceOnlyErrorV1::Target)?;
            let h = history_length_v1(v.space().info(), block, ctx.pos).ok_or(TirSourceOnlyErrorV1::Target)?;
            let shape = node.out.resolve(h);
            let lanes = shape.iter().try_fold(1u64, |n, dim| mul(n, *dim as u64))?;
            if first_lane.checked_add(u64::from(lane_count)).is_none_or(|n| n > lanes) {
                return Err(TirSourceOnlyErrorV1::Target);
            }
            (node.out.dtype, shape, first_lane, lane_count)
        }
        Target::Legacy(leaf) => {
            if v.space().leaf_at(&binding.job_context, frontier) != Some(leaf) {
                return Err(TirSourceOnlyErrorV1::Target);
            }
            (leaf.dtype, vec![leaf.value_count as usize], 0, leaf.value_count)
        }
    };
    if count == 0 || u64::from(count) > limits.max_target_lanes || mul(u64::from(count), 16)? > limits.max_target_bytes {
        return Err(TirSourceOnlyErrorV1::Budget);
    }
    Ok(TargetGeometry { dtype, shape, elements: Vec::new(), first_lane: first, lane_count: count })
}

fn evaluate_target(
    v: &PalwTirVerifiedBindingV2,
    target: Target,
    geometry: &TargetGeometry,
    source: &mut TirSource<'_, '_>,
    rules: &PalwTirCourtRulesV1,
) -> Result<(Vec<i128>, DemandWork), DemandError> {
    match target {
        Target::Node { ctx, node, .. } => eval_demanded(
            v.space().program(),
            v.space().info(),
            &DemandRequest { target: DemandTarget::Node { ctx, node }, elements: &geometry.elements },
            source,
            &rules.limits,
        ),
        Target::Legacy(leaf) => evaluate_leaf_with_work(v.space(), &leaf, source, &rules.limits),
    }
}

/// Pinned logical allocation-request envelope for one reference evaluation.
/// These are deliberately conservative per-entry reservations, not a multiplier
/// over encoded bytes. Program decoding, range/inventory analyses and Merkle
/// verification require their separate registered-program/material envelopes.
fn workspace_quote(
    v: &PalwTirVerifiedBindingV2,
    target_lanes: u32,
    rules: &PalwTirCourtRulesV1,
    limits: TirSourceOnlyLimitsV1,
) -> Result<u64, TirSourceOnlyErrorV1> {
    let e = rules.limits.max_elements;
    let t = rules.limits.max_terms;
    let r = limits.max_source_reads;
    let lanes = u64::from(target_lanes);
    // demand::DemandLimits::max_pushes: at most three scans, up to two
    // missing operands per reduction term, plus the primitive input references.
    let pushes = add(mul(t, 6)?, mul(e, 3 * MAX_NODE_INPUTS as u64)?)?;
    let frames = add(add(pushes, lanes)?, 1)?;
    let contexts = mul(u64::from(v.job().positions), n64(v.space().occurrences().len())?)?.min(add(add(add(e, r)?, lanes)?, 1)?);
    let nodes = v.space().program().blocks.iter().map(|block| block.nodes.len()).max().unwrap_or(0) as u64;
    let word = std::mem::size_of::<usize>() as u64;
    // Existing Frame has two usize fields, u16 and bool; four words is a
    // conservative 64-bit envelope. Both pending and active stack Vecs grow.
    let mut bytes = mul(mul(frames.max(4), 4)?, mul(word, 4)?)?;
    let memo_entries = add(add(add(e, t)?, r)?, lanes)?;
    bytes = add(bytes, mul(memo_entries, btree_node_envelope_bytes_v1::<usize, i128>() as u64)?)?;
    bytes = add(bytes, mul(r, btree_node_envelope_bytes_v1::<(u32, u16, Option<u16>, usize), i128>() as u64)?)?;
    bytes = add(bytes, mul(contexts, btree_node_envelope_bytes_v1::<DemandContext, usize>() as u64)?)?;
    // Ctx owns shape Vecs, count Vecs and per-node empty memo-map headers.
    let node_metadata = add(
        add(std::mem::size_of::<Vec<usize>>() as u64, mul(MAX_RANK as u64, word)?)?,
        add(word, std::mem::size_of::<BTreeMap<usize, i128>>() as u64)?,
    )?;
    bytes = add(bytes, mul(mul(mul(contexts, nodes)?, 2)?, node_metadata)?)?;
    bytes = add(bytes, mul(mul(contexts.max(4), 2)?, mul(word, 16)?)?)?;
    // At a reduction: two operand request/value arrays; at TopK additionally
    // order/chosen/sort scratch. This envelope also allows Vec growth slack.
    bytes = add(bytes, mul(add(t, 1)?, 128)?)?;
    // Original target indices plus i128 results; allow minimum Vec capacities
    // and small rank/stride temporaries independently of the target dtype.
    bytes = add(bytes, mul(lanes.max(4), add(word, 16)?)?)?;
    bytes = add(bytes, mul((MAX_RANK as u64 + 1) * 16, word)?)?;
    if bytes > limits.max_workspace_bytes {
        return Err(TirSourceOnlyErrorV1::Budget);
    }
    Ok(bytes)
}

fn canonical(
    material: TirSourceOnlyMaterialV2<'_>,
    operands: &[(u64, PalwTirLeafV1)],
    prompt_tiles: &BTreeSet<u32>,
    used: &Requests,
    form: PalwPromptIdsFormV1,
) -> Result<(), TirSourceOnlyErrorV1> {
    let bad = |why| TirSourceOnlyErrorV1::Material(PalwStepRefuteError::InputSetNotCanonical(why));
    if used.steps.len() != operands.len() || !operands.iter().all(|(index, _)| used.steps.contains(index)) {
        return Err(bad("source operands are not exactly the read set"));
    }
    if used.params.len() != material.params.opened_count() {
        return Err(bad("source params are not exactly the read set"));
    }
    match form {
        PalwPromptIdsFormV1::Flat if used.prompt.is_empty() != material.prompt_token_ids.is_empty() => {
            return Err(bad("source flat prompt presence is not canonical"));
        }
        PalwPromptIdsFormV1::MerkleV1 => {
            let read: BTreeSet<u32> = used.prompt.iter().map(|p| p / PALW_PROMPT_IDS_TILE_LEN).collect();
            if &read != prompt_tiles {
                return Err(bad("source prompt tiles are not exactly the read set"));
            }
        }
        _ => {}
    }
    if used.decode != material.decode_tokens.is_some() {
        return Err(bad("source decode pin presence is not canonical"));
    }
    Ok(())
}

fn binding_geometry(
    binding: &PalwTirStepBindingV1,
    frontier: u64,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwTirVerifiedBindingV2, TirSourceOnlyErrorV1> {
    let class = &binding.class;
    if class.version != PALW_TIR_CLASS_VERSION_V1
        || class.program.len() > MAX_PROGRAM_BYTES
        || class.layout.commit_tiles.len() > MAX_BLOCKS * MAX_NODES_PER_BLOCK
        || class.layout.state_tiles.len() > MAX_STATES
        || binding.job_context.version != PALW_TRACE_COMMITMENT_VERSION_V2
        || binding.job_context.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES
        || frontier > binding.step_leaf_count
    {
        return Err(TirSourceOnlyErrorV1::Binding);
    }
    let verified = verify_tir_binding_v2(binding, rules.max_step_leaf_count).map_err(|_| TirSourceOnlyErrorV1::Binding)?;
    if verified.space().program().version != TIR_PROGRAM_VERSION_V1
        || Hash64::from_bytes(verified.space().program().logits_scheme_id) != crate::palw_logits_q24_v2::scheme_id()
        || binding.job_context.trace_scheme_id != crate::palw_logits_q24_v2::scheme_id()
    {
        return Err(TirSourceOnlyErrorV1::Binding);
    }
    Ok(verified)
}

// A finite test-data constructor only. It discovers the canonical read set over
// truthful reference values; it performs no authentication and is never a Node
// source, producer capability, native admission, or production store fallback.
#[cfg(test)]
pub(crate) mod test_support;
