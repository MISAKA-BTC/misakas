//! Conservative parameter-coordinate closure for a named native TIR dissection.
//!
//! This derives potential reads, not the reads of one honest execution. Unknown
//! Gather indices cover their legal axis and unknown Select conditions cover
//! both branches. Node-value bounds use dtype bounds, not an honest runtime
//! result or a range derived only for correct reductions. Authenticated claim
//! units remain boundaries (with actual Fixed-state replay tracked): their producers
//! are never required to supply model bytes. A caller must separately reserve
//! acquisition, authentication, retention and money for the returned cover.
use super::*;
use misaka_palw_tir::Ref;
use misaka_palw_tir::demand::history_length_v1;

/// Explicit caller-admitted exploration limit, not a consensus parameter or a
/// model-size policy. Exhaustion returns no partial cover.
#[derive(Clone, Copy, Debug)]
pub struct ParameterCoverBudgetV1 {
    pub max_edges: u64,
    pub max_coordinates: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PotentialParameterCoverV1 {
    /// Canonical ascending inventory coordinates under the original artifact.
    pub leaves: Vec<u32>,
    pub explored_edges: u64,
    pub explored_coordinates: u64,
}

/// Origin classification is attached at the local limit check, never parsed from a string.
#[derive(Debug)]
pub(crate) enum CoverErrorV1 {
    Semantic(String),
    Static(&'static str),
    Local(&'static str),
}
impl From<String> for CoverErrorV1 {
    #[inline]
    fn from(s: String) -> Self {
        Self::Semantic(s)
    }
}
impl From<&'static str> for CoverErrorV1 {
    #[inline]
    fn from(s: &'static str) -> Self {
        Self::Static(s)
    }
}
impl CoverErrorV1 {
    pub(crate) fn legacy(self) -> String {
        match self {
            Self::Semantic(s) => s,
            Self::Static(s) | Self::Local(s) => s.into(),
        }
    }
    pub(crate) fn native(self) -> crate::palw_native_resource_v1::NativeDecodeErrorV1<String> {
        use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1 as E};
        match self {
            Self::Semantic(s) => E::Decode(s),
            Self::Static(s) => E::Decode(s.into()),
            Self::Local(s) => {
                E::Resource(NativeCoverageV1::unsupported::<PotentialParameterCoverV1>("local parameter exploration", s))
            }
        }
    }
}
/// Resolve the original binding and named leaf through the existing native
/// verifier. This does not accept a caller-provided program, root claim, read set
/// or unchecked scope as authority. Callers must first bind `binding` to their
/// retained original with the existing `check_tir_binding_is_the_claims_v1`.
/// Binding decode, range analysis and inventory-index allocation precede the
/// exploration meter and require separate native resource preflight. The meter
/// limits neither their allocation nor the later cost of acquiring this cover.
pub fn derive_parameter_cover_v1(
    binding: &PalwTirStepBindingV1,
    leaf: u64,
    budget: ParameterCoverBudgetV1,
) -> Result<PotentialParameterCoverV1, String> {
    let verified = match check_binding(binding).map_err(|e| e.to_string())? {
        BindingOutcome::Verified(v) => v,
        BindingOutcome::Convicted(v) => return Err(format!("the binding itself convicts ({:?})", v.fault)),
    };
    let prepared = PreparedParameterCoverV1::new(verified, binding.job_context.clone())?;
    prepared.named(leaf, budget)?.ok_or("parameter cover needs a dissected commit leaf".into())
}

/// Exact external boundaries reached by the conservative named-root walk.
/// Coordinates only: does not assert bytes are present, paid or retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RootInputBoundaryV1 {
    PromptPosition(u32),
    GeneratedId(u32),
    CarryStepLeaf(u64),
    CommittedStepLeaf(u64),
    FixedCheckpointStepLeaf(u64),
    HistoryStepLeaf(u64),
}

/// One original's immutable native analysis, shared by every named root. This
/// owns no model bytes, producer roots or caller-provided dependency assertions.
/// Allocation/decoding of this prelude requires separate native admission.
pub(crate) struct PreparedParameterCoverV1 {
    verified: Box<PalwTirVerifiedBindingV1>,
    context: PalwJobContextV2,
    inventory: PalwTirInventoryIndexV1,
    intervals: Vec<Vec<Interval>>,
    param_lens: Vec<Option<usize>>,
    const_lens: Vec<Option<usize>>,
    state_lens: Vec<Option<usize>>,
}
impl PreparedParameterCoverV1 {
    pub(crate) fn new(verified: Box<PalwTirVerifiedBindingV1>, context: PalwJobContextV2) -> Result<Self, String> {
        let inventory = PalwTirInventoryIndexV1::new(&verified.space.program).ok_or("the class has no inventory")?;
        let intervals = analyze_ranges(&verified.space.program).map_err(|e| e.to_string())?;
        let slice_len = |shape: &[u32]| RankVec::from_u32_slice(shape).and_then(|s| count(&s)).ok();
        let param_lens = verified.space.program.params.iter().map(|p| slice_len(&p.shape)).collect();
        let const_lens = verified.space.program.consts.iter().map(|c| slice_len(&c.shape)).collect();
        let state_lens = verified.space.program.states.iter().map(|s| slice_len(&s.shape)).collect();
        Ok(Self { verified, context, inventory, intervals, param_lens, const_lens, state_lens })
    }
    /// Conservative simultaneous allocation peak for the existing all-root
    /// materializer. Counts are checked local limits, never adopted network caps.
    pub(crate) fn native_all_roots_requests_v1(
        program: &TirProgramV1,
        canonical_program: &[u8],
        original_leaves: u64,
        budget: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1,
    ) -> Result<u64, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        use crate::palw_native_resource_v1::{
            NativeCoverageV1, NativeDecodeErrorV1 as E, NativeHeapV1, TirNativeRequestsV1, btree_node_envelope_bytes_v1,
            source_tir_program_shape_v1,
        };
        use crate::palw_tir_public_source_v1::parameter_cover_v1::{NamedParameterCoverV1, OriginalParameterCoverV1};
        use misaka_palw_tir::types::{Dim, TensorType};
        let mut q = TirNativeRequestsV1::new();
        let (shape, normal_form_requests) = source_tir_program_shape_v1(canonical_program)?;
        let n = shape.structural_items_v1();
        let overflow = || E::Resource(NativeCoverageV1::overflow::<OriginalParameterCoverV1>("original parameter native allocation"));
        q.exact_v1::<PalwTirVerifiedBindingV1>(1).map_err(E::Resource)?;
        q.add_v1(TirInventoryOpeningVerifierV1::index_requests(program).map_err(E::Resource)?).map_err(E::Resource)?;
        // Retained range analysis beside the verified StepSpace. Each inner
        // interval vector has its own minimum capacity even when node counts
        // are spread among many blocks.
        q.vec_v1::<Vec<Interval>>(n).map_err(E::Resource)?;
        q.vec_v1::<Interval>(n).map_err(E::Resource)?;
        let mut minimum = TirNativeRequestsV1::new();
        minimum.vec_v1::<Interval>(1).map_err(E::Resource)?;
        q.add_v1(n.checked_mul(minimum.bytes_v1()).ok_or_else(overflow)?).map_err(E::Resource)?;
        // Site/cone live while Walk grows: reductions, folds, bounds, counts,
        // cone computed/seen/derived and repeated-reference traversal stack.
        q.vec_v1::<u16>(n).map_err(E::Resource)?;
        q.vec_v1::<crate::palw_tir_dissect_v1::PalwTirFoldV1>(n).map_err(E::Resource)?;
        q.vec_v1::<Interval>(n).map_err(E::Resource)?;
        q.vec_v1::<u64>(n).map_err(E::Resource)?;
        for _ in 0..3 {
            q.vec_v1::<bool>(n).map_err(E::Resource)?;
        }
        q.vec_v1::<usize>(n.checked_add(1).ok_or_else(overflow)?).map_err(E::Resource)?;
        // The range prelude invokes normal-form validation again while the
        // verified program is retained. Charge the existing linear Source
        // profile for that validator, and the range analyzer's separate
        // per-node operand vectors and largest temporary constant lane vector.
        q.add_v1(normal_form_requests).map_err(E::Resource)?;
        let maximum_inputs =
            program.blocks.iter().flat_map(|block| &block.nodes).map(|node| node.inputs.len() as u64).max().unwrap_or(0);
        q.vec_v1::<Interval>(maximum_inputs).map_err(E::Resource)?;
        q.vec_v1::<TensorType>(maximum_inputs).map_err(E::Resource)?;
        let mut dims = TirNativeRequestsV1::new();
        dims.vec_v1::<Dim>(misaka_palw_tir::types::MAX_RANK as u64).map_err(E::Resource)?;
        q.add_v1(maximum_inputs.checked_mul(dims.bytes_v1()).ok_or_else(overflow)?).map_err(E::Resource)?;
        let maximum_const_lanes = program.consts.iter().map(|c| (c.data.len() / c.dtype.width()) as u64).max().unwrap_or(0);
        q.vec_v1::<i128>(maximum_const_lanes).map_err(E::Resource)?;

        // Walk::run clones exactly one Node per iteration. Its fetches enqueue
        // coordinates and never recurse into run. The Node itself is on the
        // stack; the generated heap visitor includes inputs, output dimensions
        // and primitive-owned vectors such as a transpose permutation.
        let mut node_clone = 0;
        for node in program.blocks.iter().flat_map(|block| &block.nodes) {
            let bound = node.native_heap_v1("parameter cover Node clone");
            node_clone = node_clone.max(bound.bytes().ok_or(E::Resource(bound))?);
        }
        q.add_v1(node_clone).map_err(E::Resource)?;
        // Gather is the largest simultaneous rank workspace: run.shape/c,
        // d/x/xi/q, plus fetch's temporary shape (seven rank Vecs). Other
        // branches (including state replay node.shape) have no larger live set.
        for _ in 0..7 {
            q.vec_v1::<usize>(misaka_palw_tir::types::MAX_RANK as u64).map_err(E::Resource)?;
        }
        let coordinates = budget.max_total_coordinates.min(budget.max_total_edges);
        // No allocating param_instances/index call is made before growth.
        // Row pieces and actual scheduled parameter readers give the same
        // inventory cardinality as the existing index, using borrowed scans.
        let mut inventory = 0u128;
        for (j, param) in program.params.iter().enumerate() {
            let tensor =
                param.shape.iter().try_fold(param.dtype.width() as u128, |a, d| a.checked_mul(u128::from(*d))).ok_or_else(overflow)?;
            let row = if param.shape.len() >= 2 {
                param.shape[1..]
                    .iter()
                    .try_fold(param.dtype.width() as u128, |a, d| a.checked_mul(u128::from(*d)))
                    .ok_or_else(overflow)?
            } else {
                tensor
            };
            let pieces = if row == 0 || tensor == 0 {
                0
            } else {
                (tensor / row).checked_mul(row.div_ceil(u128::from(PALW_TIR_ROW_PIECE_BYTES_V1))).ok_or_else(overflow)?
            };
            let instances = if param.per_layer {
                program
                    .schedule
                    .layers
                    .iter()
                    .filter(|block| {
                        program.blocks.get(usize::from(**block)).is_some_and(|b| {
                            b.nodes.iter().any(|n| n.inputs.iter().any(|r| matches!(r,Ref::Param(k) if usize::from(*k)==j)))
                        })
                    })
                    .count() as u128
            } else {
                1
            };
            inventory = inventory.checked_add(pieces.checked_mul(instances).ok_or_else(overflow)?).ok_or_else(overflow)?;
        }
        let memberships = u128::from(budget.max_root_memberships.min(budget.max_total_edges))
            .min(inventory.checked_mul(u128::from(original_leaves)).ok_or_else(overflow)?) as u64;
        let union_members = memberships.min(inventory.min(u128::from(u64::MAX)) as u64);
        type Key = (u32, u16, u16, usize);
        q.add_v1(coordinates.checked_mul(btree_node_envelope_bytes_v1::<Key, ()>() as u64).ok_or_else(overflow)?)
            .map_err(E::Resource)?;
        q.vec_v1::<Key>(coordinates).map_err(E::Resource)?;
        // Walk.leaves and global union coexist. Conversion to output Vecs
        // occurs while the iterator still owns the remaining tree nodes.
        q.add_v1(
            memberships
                .checked_add(union_members)
                .and_then(|v| v.checked_mul(btree_node_envelope_bytes_v1::<u32, ()>() as u64))
                .ok_or_else(overflow)?,
        )
        .map_err(E::Resource)?;
        q.vec_v1::<NamedParameterCoverV1>(original_leaves).map_err(E::Resource)?;
        q.vec_v1::<u32>(memberships).map_err(E::Resource)?; // all retained root parameters
        q.vec_v1::<u32>(union_members).map_err(E::Resource)?; // final union
        let mut minimum = TirNativeRequestsV1::new();
        minimum.vec_v1::<u32>(1).map_err(E::Resource)?;
        q.add_v1(original_leaves.checked_mul(minimum.bytes_v1()).ok_or_else(overflow)?).map_err(E::Resource)?;
        q.exact_v1::<OriginalParameterCoverV1>(1).map_err(E::Resource)?;
        Ok(q.bytes_v1())
    }
    /// None means an existing canonical leaf has no numerical dissection.
    /// A missing leaf, failed analysis or exhausted exploration is always Err.
    pub(crate) fn named(&self, index: u64, budget: ParameterCoverBudgetV1) -> Result<Option<PotentialParameterCoverV1>, String> {
        self.named_with_leaf_limit(index, budget, budget.max_coordinates)
    }
    pub(crate) fn named_with_leaf_limit(
        &self,
        index: u64,
        budget: ParameterCoverBudgetV1,
        max_leaves: u64,
    ) -> Result<Option<PotentialParameterCoverV1>, String> {
        self.named_with_leaf_limit_typed(index, budget, max_leaves).map_err(CoverErrorV1::legacy)
    }
    pub(crate) fn named_with_leaf_limit_typed(
        &self,
        index: u64,
        budget: ParameterCoverBudgetV1,
        max_leaves: u64,
    ) -> Result<Option<PotentialParameterCoverV1>, CoverErrorV1> {
        self.named_tracked_v1(index, budget, max_leaves, None)
    }
    pub(crate) fn named_with_inputs_v1(
        &self,
        index: u64,
        budget: ParameterCoverBudgetV1,
        max_leaves: u64,
        boundaries: &mut BTreeSet<RootInputBoundaryV1>,
    ) -> Result<Option<PotentialParameterCoverV1>, CoverErrorV1> {
        self.named_tracked_v1(index, budget, max_leaves, Some(boundaries))
    }
    fn named_tracked_v1(
        &self,
        index: u64,
        budget: ParameterCoverBudgetV1,
        max_leaves: u64,
        boundaries: Option<&mut BTreeSet<RootInputBoundaryV1>>,
    ) -> Result<Option<PotentialParameterCoverV1>, CoverErrorV1> {
        let leaf = self.verified.space.leaf_at(&self.context, index).ok_or("parameter cover original leaf missing")?;
        let Some(site) = palw_tir_dissect_site_v1(&self.verified.space, &self.intervals, &leaf) else { return Ok(None) };
        let block = occurrence_block(&self.verified.space, site.ctx.occurrence).ok_or("cover occurrence missing")?;
        let layer = self.verified.space.occurrences().get(usize::from(site.ctx.occurrence)).ok_or("cover occurrence missing")?.1;
        let h = usize::try_from(site.history_positions).map_err(|_| "cover H overflow")?;
        let block_usize = usize::from(block);
        let initial_ctx = SeenContext::new(&self.verified.space.program, block_usize, site.ctx.pos, site.ctx.occurrence, h);
        let mut walk = Walk {
            program: &self.verified.space.program,
            block: block_usize,
            layer,
            h,
            space: &self.verified.space,
            context: &self.context,
            original_leaf: index,
            boundaries,
            pos: site.ctx.pos,
            occurrence: site.ctx.occurrence,
            inventory: &self.inventory,
            param_lens: &self.param_lens,
            const_lens: &self.const_lens,
            state_lens: &self.state_lens,
            budget,
            leaf_limit: max_leaves,
            edges: 0,
            seen_count: 0,
            contexts: vec![initial_ctx],
            active_ctx: 0,
            pending: Vec::new(),
            leaves: BTreeSet::new(),
            leaves_bits: vec![0u64; (self.inventory.leaf_count() as usize).div_ceil(64)],
            last_node_tile: None,
            last_coord: None,
            recorded_coords: Vec::new(),
            last_hist_node: None,
        };
        if let PalwTirLeafKindV1::Commit { first_element, .. } = leaf.kind {
            for element in first_element..first_element + u64::from(leaf.value_count) {
                walk.node(site.node, element as usize)?;
            }
        }
        // Every legal root element/closure and every legal bottom range is covered.
        // This intentionally includes elements not reached by one particular root;
        // it is a finite coordinate envelope, not a canonical carriage for a move.
        for (node, count) in site.reductions.iter().zip(&site.counts) {
            for element in 0..usize::try_from(*count).map_err(|_| "cover reduction count overflow")? {
                walk.node(*node, element)?;
            }
        }
        walk.run()?;
        Ok(Some(PotentialParameterCoverV1 {
            leaves: walk.leaves.into_iter().collect(),
            explored_edges: walk.edges,
            explored_coordinates: walk.seen_count,
        }))
    }
}

struct SeenContext {
    pos: u32,
    occ: u16,
    nodes: Vec<Option<Vec<u64>>>,
    node_lens: Vec<Option<usize>>,
    carry_lens: Vec<Option<usize>>,
}
impl SeenContext {
    fn new(program: &TirProgramV1, block: usize, pos: u32, occ: u16, h: usize) -> Self {
        let Some(b) = program.blocks.get(block) else {
            return Self { pos, occ, nodes: Vec::new(), node_lens: Vec::new(), carry_lens: Vec::new() };
        };
        let node_lens = b.nodes.iter().map(|n| RankVec::from_tensor(&n.out, h).and_then(|s| count(&s)).ok()).collect();
        let carry_lens = b.carry_in.iter().map(|c| RankVec::from_tensor(c, h).and_then(|s| count(&s)).ok()).collect();
        Self { pos, occ, nodes: vec![None; b.nodes.len()], node_lens, carry_lens }
    }
}

#[derive(Clone, Copy)]
struct RankVec {
    buf: [usize; 8],
    len: u8,
}
impl RankVec {
    #[inline]
    fn empty() -> Self {
        Self { buf: [0; 8], len: 0 }
    }
    #[inline]
    fn zeros(len: usize) -> Result<Self, CoverErrorV1> {
        if len > 8 {
            return Err("cover coordinate rank".into());
        }
        Ok(Self { buf: [0; 8], len: len as u8 })
    }
    #[inline]
    fn from_tensor(t: &misaka_palw_tir::types::TensorType, h: usize) -> Result<Self, CoverErrorV1> {
        let mut out = Self::zeros(t.shape.len())?;
        for (slot, dim) in out.buf[..t.shape.len()].iter_mut().zip(&t.shape) {
            *slot = dim.at(h);
        }
        Ok(out)
    }
    #[inline]
    fn from_u32_slice(shape: &[u32]) -> Result<Self, CoverErrorV1> {
        let mut out = Self::zeros(shape.len())?;
        for (slot, dim) in out.buf[..shape.len()].iter_mut().zip(shape) {
            *slot = *dim as usize;
        }
        Ok(out)
    }
    #[inline]
    fn push(&mut self, v: usize) -> Result<(), CoverErrorV1> {
        let idx = self.len as usize;
        if idx >= 8 {
            return Err("cover coordinate rank".into());
        }
        self.buf[idx] = v;
        self.len += 1;
        Ok(())
    }
    #[inline]
    fn extend_from_slice(&mut self, slice: &[usize]) -> Result<(), CoverErrorV1> {
        let start = self.len as usize;
        if start + slice.len() > 8 {
            return Err("cover coordinate rank".into());
        }
        self.buf[start..start + slice.len()].copy_from_slice(slice);
        self.len += slice.len() as u8;
        Ok(())
    }
}
impl std::ops::Deref for RankVec {
    type Target = [usize];
    #[inline]
    fn deref(&self) -> &[usize] {
        &self.buf[..self.len as usize]
    }
}
impl std::ops::DerefMut for RankVec {
    #[inline]
    fn deref_mut(&mut self) -> &mut [usize] {
        &mut self.buf[..self.len as usize]
    }
}

struct Walk<'a, 'sink> {
    program: &'a TirProgramV1,
    block: usize,
    layer: Option<u16>,
    h: usize,
    space: &'a PalwTirStepSpaceV1,
    context: &'a PalwJobContextV2,
    original_leaf: u64,
    boundaries: Option<&'sink mut BTreeSet<RootInputBoundaryV1>>,
    pos: u32,
    occurrence: u16,
    inventory: &'a PalwTirInventoryIndexV1,
    param_lens: &'a [Option<usize>],
    const_lens: &'a [Option<usize>],
    state_lens: &'a [Option<usize>],
    budget: ParameterCoverBudgetV1,
    leaf_limit: u64,
    edges: u64,
    seen_count: u64,
    contexts: Vec<SeenContext>,
    active_ctx: usize,
    pending: Vec<(u32, u16, u16, usize)>,
    leaves: BTreeSet<u32>,
    leaves_bits: Vec<u64>,
    last_node_tile: Option<(u32, u16, u16, u64, bool)>,
    last_coord: Option<(PalwStepCoordinateV1, u8)>,
    recorded_coords: Vec<(PalwStepCoordinateV1, u8)>,
    last_hist_node: Option<((u16, Option<u16>, u32), (DemandContext, u16))>,
}
#[inline]
fn count(shape: &[usize]) -> Result<usize, CoverErrorV1> {
    shape.iter().try_fold(1usize, |a, b| a.checked_mul(*b).ok_or(CoverErrorV1::Static("cover shape overflow")))
}
#[inline]
fn coords(mut index: usize, shape: &[usize]) -> Result<RankVec, CoverErrorV1> {
    if index >= count(shape)? {
        return Err(CoverErrorV1::Static("cover index outside shape"));
    }
    let mut out = RankVec::zeros(shape.len())?;
    for i in (0..shape.len()).rev() {
        out[i] = index % shape[i];
        index /= shape[i];
    }
    Ok(out)
}
#[inline]
fn index(c: &[usize], shape: &[usize]) -> Result<usize, CoverErrorV1> {
    if c.len() != shape.len() {
        return Err(CoverErrorV1::Static("cover coordinate rank"));
    }
    c.iter().zip(shape).try_fold(0usize, |a, (v, n)| {
        if v >= n {
            return Err(CoverErrorV1::Static("cover coordinate outside shape"));
        }
        a.checked_mul(*n).and_then(|x| x.checked_add(*v)).ok_or(CoverErrorV1::Static("cover coordinate overflow"))
    })
}
#[inline]
fn broadcast(c: &[usize], shape: &[usize]) -> Result<usize, CoverErrorV1> {
    if shape.len() > c.len() {
        return Err(CoverErrorV1::Static("cover broadcast rank"));
    }
    c[c.len() - shape.len()..].iter().zip(shape).try_fold(0usize, |a, (v, n)| {
        let mv = if *n == 1 { 0 } else { *v };
        if mv >= *n {
            return Err(CoverErrorV1::Static("cover coordinate outside shape"));
        }
        a.checked_mul(*n).and_then(|x| x.checked_add(mv)).ok_or(CoverErrorV1::Static("cover coordinate overflow"))
    })
}
impl Walk<'_, '_> {
    #[inline]
    fn edge(&mut self) -> Result<(), CoverErrorV1> {
        self.edges = self.edges.checked_add(1).ok_or(CoverErrorV1::Local("cover edge overflow"))?;
        if self.edges > self.budget.max_edges {
            return Err(CoverErrorV1::Local("parameter cover edge budget exhausted"));
        }
        Ok(())
    }
    #[inline]
    fn node(&mut self, n: u16, i: usize) -> Result<(), CoverErrorV1> {
        self.edge()?;
        let ctx = &mut self.contexts[self.active_ctx];
        let node_len = match ctx.node_lens.get(n as usize).copied().flatten() {
            Some(len) => len,
            None => {
                let out = &self.program.blocks[self.block].nodes.get(n as usize).ok_or("cover node missing")?.out;
                count(&RankVec::from_tensor(out, self.h)?)?
            }
        };
        if i >= node_len {
            return Err(CoverErrorV1::Static("cover node element outside shape"));
        }
        let bits = ctx.nodes[n as usize].get_or_insert_with(|| vec![0u64; node_len.div_ceil(64)]);
        let word = &mut bits[i >> 6];
        let mask = 1u64 << (i & 63);
        if (*word & mask) == 0 {
            if self.seen_count >= self.budget.max_coordinates {
                return Err(CoverErrorV1::Local("parameter cover coordinate budget exhausted"));
            }
            *word |= mask;
            self.seen_count += 1;
            self.pending.push((self.pos, self.occurrence, n, i));
        }
        Ok(())
    }
    #[inline]
    fn operand_len(&self, r: Ref) -> Result<usize, CoverErrorV1> {
        let ctx = &self.contexts[self.active_ctx];
        let cached = match r {
            Ref::Node(j) => ctx.node_lens.get(j as usize).copied().flatten(),
            Ref::CarryIn(j) => ctx.carry_lens.get(j as usize).copied().flatten(),
            Ref::Param(j) => self.param_lens.get(j as usize).copied().flatten(),
            Ref::Const(j) => self.const_lens.get(j as usize).copied().flatten(),
            Ref::State(j) => self.state_lens.get(j as usize).copied().flatten(),
            Ref::Input(_) => Some(1),
        };
        match cached {
            Some(len) => Ok(len),
            None => count(&self.shape(r)?),
        }
    }
    #[inline]
    fn shape(&self, r: Ref) -> Result<RankVec, CoverErrorV1> {
        let p = self.program;
        let b = &p.blocks[self.block];
        match r {
            Ref::Node(j) => RankVec::from_tensor(&b.nodes.get(j as usize).ok_or("cover node missing")?.out, self.h),
            Ref::CarryIn(j) => RankVec::from_tensor(b.carry_in.get(j as usize).ok_or("cover carry missing")?, self.h),
            Ref::Param(j) => RankVec::from_u32_slice(&p.params.get(j as usize).ok_or("cover param missing")?.shape),
            Ref::Const(j) => RankVec::from_u32_slice(&p.consts.get(j as usize).ok_or("cover const missing")?.shape),
            Ref::State(j) => RankVec::from_u32_slice(&p.states.get(j as usize).ok_or("cover state missing")?.shape),
            Ref::Input(_) => Ok(RankVec::empty()),
        }
    }
    fn bounds(&self, r: Ref, i: usize) -> Result<Interval, CoverErrorV1> {
        Ok(match r {
            Ref::Node(j) => Interval::of(self.program.blocks[self.block].nodes.get(j as usize).ok_or("cover node missing")?.out.dtype),
            Ref::Const(j) => {
                let c = self.program.consts.get(j as usize).ok_or("cover const missing")?;
                let w = c.dtype.width();
                let off = i.checked_mul(w).ok_or("cover const offset overflow")?;
                Interval::point(c.dtype.decode_le(
                    c.data.get(off..off.checked_add(w).ok_or("cover const end overflow")?).ok_or("cover const element missing")?,
                ))
            }
            Ref::Input(0) => Interval::new(0, self.program.token_bound.saturating_sub(1) as i128),
            Ref::Input(1) => Interval::point(self.pos as i128),
            Ref::Param(j) => {
                let d = self.program.params.get(j as usize).ok_or("cover param missing")?.dtype;
                Interval::new(d.min_value(), d.max_value())
            }
            _ => Interval::new(i128::MIN, i128::MAX),
        })
    }
    fn fetch(&mut self, r: Ref, i: usize) -> Result<(), CoverErrorV1> {
        self.edge()?;
        if i >= self.operand_len(r)? {
            return Err(CoverErrorV1::Static("cover operand index outside shape"));
        }
        match r {
            Ref::Node(j) => {
                if !self.program.blocks[self.block].nodes[j as usize].commit {
                    self.node(j, i)?;
                } else if self.boundaries.is_some() {
                    self.record_node(DemandContext { pos: self.pos, occurrence: self.occurrence }, j, i, false)?;
                }
            }
            Ref::Param(j) => {
                let p = &self.program.params[j as usize];
                let byte = (i as u64).checked_mul(p.dtype.width() as u64).ok_or("cover param byte overflow")?;
                let leaf = self
                    .inventory
                    .leaf_of(j, if p.per_layer { self.layer } else { None }, byte)
                    .ok_or("cover parameter coordinate missing")?;
                let word_idx = (leaf as usize) >> 6;
                let mask = 1u64 << (leaf & 63);
                if (self.leaves_bits[word_idx] & mask) == 0 {
                    if self.leaves.len() as u64 >= self.leaf_limit {
                        return Err(CoverErrorV1::Local("parameter cover leaf budget exhausted"));
                    }
                    self.leaves_bits[word_idx] |= mask;
                    self.leaves.insert(leaf);
                }
            }
            Ref::State(j) => {
                if self.boundaries.is_some() {
                    self.record_fixed(j, i)?;
                } else if self.pos > 0 && !self.pos.is_multiple_of(self.space.layout.checkpoint_interval) {
                    let state = &self.program.states[j as usize];
                    let layer = if state.per_layer { self.layer } else { None };
                    if let Some((occ, writer)) = state_writer_v1(self.program, j, layer) {
                        let block = usize::from(self.space.occurrences()[usize::from(occ)].0);
                        if !self.program.blocks[block].nodes[usize::from(writer)].commit {
                            let old = (self.pos, self.occurrence);
                            self.set_context(self.pos - 1, occ)?;
                            self.node(writer, i)?;
                            self.set_context(old.0, old.1)?;
                        }
                    }
                }
            }
            Ref::CarryIn(k) => {
                if self.boundaries.is_some() {
                    // Existing verified TIR-14 requires every carry_out committed.
                    // Reuse the exact demand mapping; refuse rather than invent
                    // a StepLeaf if a future program violates that precondition.
                    let ctx = misaka_palw_tir::demand::DemandContext { pos: self.pos, occurrence: self.occurrence };
                    let (prev, node) =
                        misaka_palw_tir::demand::carry_in_node_v1(self.program, ctx, k).ok_or("cover carry-out missing")?;
                    self.record_node(prev, node, i, true)?;
                }
            }
            Ref::Input(0) => {
                if self.boundaries.is_some() {
                    let job = self.space.job_shape(self.context).map_err(|e| e.to_string())?;
                    if self.pos >= job.positions {
                        return Err("cover token position outside original".into());
                    }
                    let input = match job.token_source_v1(self.pos) {
                        crate::palw_tir_step_v1::PalwTirTokenSourceV1::Prompt(p) => RootInputBoundaryV1::PromptPosition(p),
                        crate::palw_tir_step_v1::PalwTirTokenSourceV1::Generated(p) => RootInputBoundaryV1::GeneratedId(p),
                    };
                    if let Some(boundaries) = self.boundaries.as_mut() {
                        boundaries.insert(input);
                    }
                }
            }
            // Input(1) is the position scalar; constants need no supplied bytes.
            Ref::Const(_) | Ref::Input(_) => {}
        }
        Ok(())
    }
    fn record_coordinate(&mut self, coord: PalwStepCoordinateV1, kind: u8) -> Result<(), CoverErrorV1> {
        if self.last_coord == Some((coord, kind)) || self.recorded_coords.contains(&(coord, kind)) {
            return Ok(());
        }
        let leaf = self.space.leaf_index(self.context, &coord).ok_or("cover source coordinate has no original leaf")?;
        if leaf >= self.original_leaf {
            return Err("cover source leaf does not precede original root".into());
        }
        let item = match kind {
            0 => RootInputBoundaryV1::CarryStepLeaf(leaf),
            1 => RootInputBoundaryV1::CommittedStepLeaf(leaf),
            2 => RootInputBoundaryV1::FixedCheckpointStepLeaf(leaf),
            _ => RootInputBoundaryV1::HistoryStepLeaf(leaf),
        };
        self.boundaries.as_mut().ok_or("cover boundary sink missing")?.insert(item);
        self.last_coord = Some((coord, kind));
        self.recorded_coords.push((coord, kind));
        Ok(())
    }
    fn record_node(&mut self, ctx: DemandContext, node: u16, index: usize, carry: bool) -> Result<(), CoverErrorV1> {
        let block = occurrence_block(self.space, ctx.occurrence).ok_or("cover source occurrence missing")?;
        if !self.program.blocks[usize::from(block)].nodes.get(usize::from(node)).ok_or("cover source node absent")?.commit {
            return Err("cover source node is not committed".into());
        }
        let width = self.space.commit_tile_len(block, node).ok_or("cover source tile absent")?;
        if width == 0 {
            return Err("cover source zero tile width".into());
        }
        let tile = index as u64 / u64::from(width);
        let key = (ctx.pos, ctx.occurrence, node, tile, carry);
        if self.last_node_tile == Some(key) {
            return Ok(());
        }
        let job = self.space.job_shape(self.context).map_err(|e| e.to_string())?;
        let coord = source_geometry::commit(self.space, job, ctx, node, tile).map_err(|e| e.to_string())?;
        self.record_coordinate(coord, if carry { 0 } else { 1 })?;
        self.last_node_tile = Some(key);
        Ok(())
    }
    fn record_fixed(&mut self, state: u16, index: usize) -> Result<(), CoverErrorV1> {
        let decl = self.program.states.get(usize::from(state)).ok_or("cover source state absent")?;
        if !matches!(decl.kind, misaka_palw_tir::program::StateKind::Fixed { .. }) {
            return Err("cover State ref is not Fixed".into());
        }
        let layer = if decl.per_layer { self.layer } else { None };
        let job = self.space.job_shape(self.context).map_err(|e| e.to_string())?;
        let supply = source_geometry::fixed(self.space, job, self.pos, state, layer, index).map_err(|e| e.to_string())?;
        match supply {
            source_geometry::SupplyGeometryV1::Zero => Ok(()),
            source_geometry::SupplyGeometryV1::Step { coord, .. } => self.record_coordinate(coord, 2),
            source_geometry::SupplyGeometryV1::Replay => {
                if let Some((occ, writer)) = state_writer_v1(self.program, state, layer) {
                    let old = (self.pos, self.occurrence);
                    let prior = self.pos.checked_sub(1).ok_or("cover replay precedes zero")?;
                    let block = occurrence_block(self.space, occ).ok_or("cover writer occurrence absent")?;
                    if self.program.blocks[usize::from(block)].nodes[usize::from(writer)].commit {
                        self.record_node(DemandContext { pos: prior, occurrence: occ }, writer, index, false)
                    } else {
                        self.set_context(prior, occ)?;
                        self.node(writer, index)?;
                        self.set_context(old.0, old.1)
                    }
                } else {
                    // No writer anywhere for this actual instance. Native demand
                    // keeps walking back until its next source.state boundary;
                    // derive exactly that boundary directly, not an assumed value.
                    let at = self.pos - self.pos % self.space.layout.checkpoint_interval;
                    match source_geometry::fixed(self.space, job, at, state, layer, index).map_err(|e| e.to_string())? {
                        source_geometry::SupplyGeometryV1::Zero => Ok(()),
                        source_geometry::SupplyGeometryV1::Step { coord, .. } => self.record_coordinate(coord, 2),
                        _ => Err("cover unwritten state boundary unresolved".into()),
                    }
                }
            }
            _ => Err("cover Fixed source geometry".into()),
        }
    }
    fn record_history(&mut self, state: u16, row_pos: u32, index: usize) -> Result<(), CoverErrorV1> {
        let decl = self.program.states.get(usize::from(state)).ok_or("cover history state absent")?;
        let layer = if decl.per_layer { self.layer } else { None };
        let h_tile = self.space.layout.h_tile;
        let tile_start = row_pos - row_pos % h_tile;
        let tile_end = tile_start + (h_tile - 1);
        if tile_end >= self.pos {
            let key = (state, layer, row_pos);
            let (ctx, node) = match self.last_hist_node {
                Some((k, val)) if k == key => val,
                _ => {
                    let val = hist_row_node_v1(&self.space.program, state, layer, row_pos)
                        .ok_or("cover history state appends nothing")?;
                    self.last_hist_node = Some((key, val));
                    val
                }
            };
            return self.record_node(ctx, node, index, false);
        }
        let job = self.space.job_shape(self.context).map_err(|e| e.to_string())?;
        match source_geometry::history(self.space, job, self.pos, state, layer, row_pos, index).map_err(|e| e.to_string())? {
            source_geometry::SupplyGeometryV1::Step { coord, .. } => self.record_coordinate(coord, 3),
            source_geometry::SupplyGeometryV1::Node { ctx, node, index } => self.record_node(ctx, node, index, false),
            _ => Err("cover history source geometry".into()),
        }
    }
    #[inline]
    fn set_context(&mut self, pos: u32, occ: u16) -> Result<(), CoverErrorV1> {
        if self.pos == pos && self.occurrence == occ {
            return Ok(());
        }
        let (block, layer) = *self.space.occurrences().get(usize::from(occ)).ok_or("cover replay occurrence missing")?;
        let block_usize = usize::from(block);
        self.block = block_usize;
        self.layer = layer;
        self.pos = pos;
        self.occurrence = occ;
        self.h = history_length_v1(&self.space.info, block, pos).ok_or("cover replay H missing")?;
        if let Some(idx) = self.contexts.iter().position(|c| c.pos == pos && c.occ == occ) {
            self.active_ctx = idx;
        } else {
            let ctx = SeenContext::new(self.program, block_usize, pos, occ, self.h);
            self.active_ctx = self.contexts.len();
            self.contexts.push(ctx);
        }
        Ok(())
    }
    fn run(&mut self) -> Result<(), CoverErrorV1> {
        let program = self.program;
        while let Some((pos, occ, n, i)) = self.pending.pop() {
            self.set_context(pos, occ)?;
            let node = &program.blocks[self.block].nodes[n as usize];
            let shape = RankVec::from_tensor(&node.out, self.h)?;
            let c = coords(i, &shape)?;
            let inputs = &node.inputs;
            match node.prim {
                Prim::Iota { .. } => {}
                Prim::Reshape
                | Prim::Cast
                | Prim::Clamp { .. }
                | Prim::Log2Floor
                | Prim::IntExp
                | Prim::IntRsqrt
                | Prim::IntLn
                | Prim::StateWrite { .. } => self.fetch(inputs[0], i)?,
                Prim::Transpose { ref perm } => {
                    let s = self.shape(inputs[0])?;
                    let mut q = RankVec::zeros(s.len())?;
                    for (k, p) in perm.iter().enumerate() {
                        q[*p as usize] = c[k];
                    }
                    self.fetch(inputs[0], index(&q, &s)?)?;
                }
                Prim::Slice { axis, start } => {
                    let s = self.shape(inputs[0])?;
                    let mut q = c;
                    q[axis as usize] = q[axis as usize].checked_add(start as usize).ok_or("cover Slice overflow")?;
                    self.fetch(inputs[0], index(&q, &s)?)?;
                }
                Prim::Concat { axis } => {
                    let mut q = c;
                    for r in inputs {
                        let s = self.shape(*r)?;
                        if q[axis as usize] < s[axis as usize] {
                            self.fetch(*r, index(&q, &s)?)?;
                            break;
                        }
                        q[axis as usize] -= s[axis as usize];
                    }
                }
                Prim::Broadcast => {
                    let s = self.shape(inputs[0])?;
                    self.fetch(inputs[0], broadcast(&c, &s)?)?;
                }
                Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => {
                    for r in inputs {
                        let s = self.shape(*r)?;
                        self.fetch(*r, broadcast(&c, &s)?)?;
                    }
                }
                Prim::Select => {
                    let s = self.shape(inputs[0])?;
                    let ci = broadcast(&c, &s)?;
                    self.fetch(inputs[0], ci)?;
                    let iv = self.bounds(inputs[0], ci)?;
                    for (take, r) in [(iv.lo != 0 || iv.hi != 0, inputs[1]), (iv.lo <= 0 && iv.hi >= 0, inputs[2])] {
                        if take {
                            let s = self.shape(r)?;
                            self.fetch(r, broadcast(&c, &s)?)?;
                        }
                    }
                }
                Prim::Gather { axis, batch_dims } => {
                    let (a, bd) = (axis as usize, batch_dims as usize);
                    let d = self.shape(inputs[0])?;
                    let x = self.shape(inputs[1])?;
                    let m = x.len().checked_sub(bd).ok_or("cover Gather batch rank")?;
                    let mut xi = RankVec::empty();
                    xi.extend_from_slice(&c[..bd])?;
                    xi.extend_from_slice(&c[a..a + m])?;
                    let ix = index(&xi, &x)?;
                    self.fetch(inputs[1], ix)?;
                    let iv = self.bounds(inputs[1], ix)?;
                    let lo = iv.lo.max(0);
                    let hi = iv.hi.min(d[a] as i128 - 1);
                    if lo <= hi {
                        for v in usize::try_from(lo).map_err(|_| "cover Gather lower overflow")?
                            ..=usize::try_from(hi).map_err(|_| "cover Gather upper overflow")?
                        {
                            let mut q = RankVec::empty();
                            q.extend_from_slice(&c[..a])?;
                            q.push(v)?;
                            q.extend_from_slice(&c[a + m..])?;
                            self.fetch(inputs[0], index(&q, &d)?)?;
                        }
                    }
                }
                Prim::MatMul => {
                    let x = self.shape(inputs[0])?;
                    let y = self.shape(inputs[1])?;
                    let (xr, yr, cr) = (x.len(), y.len(), c.len());
                    let batch = &c[..cr - 2];
                    let xb = broadcast(batch, &x[..xr - 2])?;
                    let yb = broadcast(batch, &y[..yr - 2])?;
                    let (m, k, nn) = (x[xr - 2], x[xr - 1], y[yr - 1]);
                    for t in 0..k {
                        self.fetch(inputs[0], xb * m * k + c[cr - 2] * k + t)?;
                        self.fetch(inputs[1], yb * k * nn + t * nn + c[cr - 1])?;
                    }
                }
                Prim::ReduceSum { axis } | Prim::ReduceMax { axis } | Prim::TopK { axis, .. } => {
                    let s = self.shape(inputs[0])?;
                    for t in 0..s[axis as usize] {
                        let mut q = c;
                        q[axis as usize] = t;
                        self.fetch(inputs[0], index(&q, &s)?)?;
                    }
                }
                Prim::HistAppend { state } => {
                    let row = count(&RankVec::from_u32_slice(&program.states[state as usize].shape)?)?;
                    if i / row + 1 == self.h {
                        self.fetch(inputs[0], i % row)?;
                    } else if self.boundaries.is_some() {
                        // Actual native Demand HistAppend selects this absolute row.
                        let row_pos = (self.pos as usize)
                            .checked_add(1)
                            .and_then(|v| v.checked_sub(self.h))
                            .and_then(|v| v.checked_add(i / row))
                            .ok_or("cover history row position overflow")?;
                        self.record_history(state, u32::try_from(row_pos).map_err(|_| "cover history row exceeds u32")?, i % row)?;
                    }
                }
            }
        }
        Ok(())
    }
}
