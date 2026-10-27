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
    Local(&'static str),
}
impl From<String> for CoverErrorV1 {
    fn from(s: String) -> Self {
        Self::Semantic(s)
    }
}
impl From<&str> for CoverErrorV1 {
    fn from(s: &str) -> Self {
        Self::Semantic(s.into())
    }
}
impl CoverErrorV1 {
    pub(crate) fn legacy(self) -> String {
        match self {
            Self::Semantic(s) => s,
            Self::Local(s) => s.into(),
        }
    }
    pub(crate) fn native(self) -> crate::palw_native_resource_v1::NativeDecodeErrorV1<String> {
        use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1 as E};
        match self {
            Self::Semantic(s) => E::Decode(s),
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
}
impl PreparedParameterCoverV1 {
    pub(crate) fn new(verified: Box<PalwTirVerifiedBindingV1>, context: PalwJobContextV2) -> Result<Self, String> {
        let inventory = PalwTirInventoryIndexV1::new(&verified.space.program).ok_or("the class has no inventory")?;
        let intervals = analyze_ranges(&verified.space.program).map_err(|e| e.to_string())?;
        Ok(Self { verified, context, inventory, intervals })
    }
    /// Conservative simultaneous allocation peak for the existing all-root
    /// materializer. Counts are checked local limits, never adopted network caps.
    pub(crate) fn native_all_roots_requests_v1(
        program: &TirProgramV1,
        shape: &crate::palw_native_resource_v1::NativeTirProgramShapeV1,
        original_leaves: u64,
        budget: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1,
    ) -> Result<u64, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        use crate::palw_native_resource_v1::{
            NativeCoverageV1, NativeDecodeErrorV1 as E, TirNativeRequestsV1, btree_node_envelope_bytes_v1,
        };
        use crate::palw_tir_public_source_v1::parameter_cover_v1::{NamedParameterCoverV1, OriginalParameterCoverV1};
        let mut q = TirNativeRequestsV1::new();
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
        // One cloned Node is bounded by the entire actual scanned program
        // ownership. No peer-provided rank/count or sizeof(Node)-only estimate.
        q.add_v1(shape.program_requests_v1()).map_err(E::Resource)?;
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
        let mut walk = Walk {
            program: &self.verified.space.program,
            block: usize::from(block),
            layer,
            h,
            space: &self.verified.space,
            context: &self.context,
            original_leaf: index,
            boundaries,
            pos: site.ctx.pos,
            occurrence: site.ctx.occurrence,
            inventory: &self.inventory,
            budget,
            leaf_limit: max_leaves,
            edges: 0,
            seen: BTreeSet::new(),
            pending: Vec::new(),
            leaves: BTreeSet::new(),
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
            explored_coordinates: walk.seen.len() as u64,
        }))
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
    budget: ParameterCoverBudgetV1,
    leaf_limit: u64,
    edges: u64,
    seen: BTreeSet<(u32, u16, u16, usize)>,
    pending: Vec<(u32, u16, u16, usize)>,
    leaves: BTreeSet<u32>,
}
fn count(shape: &[usize]) -> Result<usize, CoverErrorV1> {
    shape.iter().try_fold(1usize, |a, b| a.checked_mul(*b).ok_or("cover shape overflow".into()))
}
fn coords(mut index: usize, shape: &[usize]) -> Result<Vec<usize>, CoverErrorV1> {
    if index >= count(shape)? {
        return Err("cover index outside shape".into());
    }
    let mut out = vec![0; shape.len()];
    for i in (0..shape.len()).rev() {
        out[i] = index % shape[i];
        index /= shape[i];
    }
    Ok(out)
}
fn index(c: &[usize], shape: &[usize]) -> Result<usize, CoverErrorV1> {
    if c.len() != shape.len() {
        return Err("cover coordinate rank".into());
    }
    c.iter().zip(shape).try_fold(0usize, |a, (v, n)| {
        if v >= n {
            return Err("cover coordinate outside shape".into());
        }
        a.checked_mul(*n).and_then(|x| x.checked_add(*v)).ok_or("cover coordinate overflow".into())
    })
}
fn broadcast(c: &[usize], shape: &[usize]) -> Result<usize, CoverErrorV1> {
    if shape.len() > c.len() {
        return Err("cover broadcast rank".into());
    }
    let mapped: Vec<_> = c[c.len() - shape.len()..].iter().zip(shape).map(|(v, n)| if *n == 1 { 0 } else { *v }).collect();
    index(&mapped, shape)
}
impl Walk<'_, '_> {
    fn edge(&mut self) -> Result<(), CoverErrorV1> {
        self.edges = self.edges.checked_add(1).ok_or(CoverErrorV1::Local("cover edge overflow"))?;
        if self.edges > self.budget.max_edges {
            return Err(CoverErrorV1::Local("parameter cover edge budget exhausted"));
        }
        Ok(())
    }
    fn node(&mut self, n: u16, i: usize) -> Result<(), CoverErrorV1> {
        self.edge()?;
        let shape = self.program.blocks[self.block].nodes.get(n as usize).ok_or("cover node missing")?.out.resolve(self.h);
        if i >= count(&shape)? {
            return Err("cover node element outside shape".into());
        }
        let key = (self.pos, self.occurrence, n, i);
        if !self.seen.contains(&key) {
            if self.seen.len() as u64 >= self.budget.max_coordinates {
                return Err(CoverErrorV1::Local("parameter cover coordinate budget exhausted"));
            }
            self.seen.insert(key);
            self.pending.push(key);
        }
        Ok(())
    }
    fn shape(&self, r: Ref) -> Result<Vec<usize>, CoverErrorV1> {
        let p = self.program;
        let b = &p.blocks[self.block];
        Ok(match r {
            Ref::Node(j) => b.nodes.get(j as usize).ok_or("cover node missing")?.out.resolve(self.h),
            Ref::CarryIn(j) => b.carry_in.get(j as usize).ok_or("cover carry missing")?.resolve(self.h),
            Ref::Param(j) => p.params.get(j as usize).ok_or("cover param missing")?.shape.iter().map(|d| *d as usize).collect(),
            Ref::Const(j) => p.consts.get(j as usize).ok_or("cover const missing")?.shape.iter().map(|d| *d as usize).collect(),
            Ref::State(j) => p.states.get(j as usize).ok_or("cover state missing")?.shape.iter().map(|d| *d as usize).collect(),
            Ref::Input(_) => vec![],
        })
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
        if i >= count(&self.shape(r)?)? {
            return Err("cover operand index outside shape".into());
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
                if !self.leaves.contains(&leaf) && self.leaves.len() as u64 >= self.leaf_limit {
                    return Err(CoverErrorV1::Local("parameter cover leaf budget exhausted"));
                }
                self.leaves.insert(leaf);
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
                    self.boundaries.as_mut().unwrap().insert(input);
                }
            }
            // Input(1) is the position scalar; constants need no supplied bytes.
            Ref::Const(_) | Ref::Input(_) => {}
        }
        Ok(())
    }
    fn record_coordinate(&mut self, coord: PalwStepCoordinateV1, kind: u8) -> Result<(), CoverErrorV1> {
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
        let job = self.space.job_shape(self.context).map_err(|e| e.to_string())?;
        let coord = source_geometry::commit(self.space, job, ctx, node, index as u64 / u64::from(width)).map_err(|e| e.to_string())?;
        self.record_coordinate(coord, if carry { 0 } else { 1 })
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
        let job = self.space.job_shape(self.context).map_err(|e| e.to_string())?;
        match source_geometry::history(self.space, job, self.pos, state, layer, row_pos, index).map_err(|e| e.to_string())? {
            source_geometry::SupplyGeometryV1::Step { coord, .. } => self.record_coordinate(coord, 3),
            source_geometry::SupplyGeometryV1::Node { ctx, node, index } => self.record_node(ctx, node, index, false),
            _ => Err("cover history source geometry".into()),
        }
    }
    fn set_context(&mut self, pos: u32, occ: u16) -> Result<(), CoverErrorV1> {
        let (block, layer) = *self.space.occurrences().get(usize::from(occ)).ok_or("cover replay occurrence missing")?;
        self.block = usize::from(block);
        self.layer = layer;
        self.pos = pos;
        self.occurrence = occ;
        self.h = usize::try_from(history_length_v1(&self.space.info, block, pos).ok_or("cover replay H missing")?)
            .map_err(|_| "cover replay H overflow")?;
        Ok(())
    }
    fn run(&mut self) -> Result<(), CoverErrorV1> {
        while let Some((pos, occ, n, i)) = self.pending.pop() {
            self.set_context(pos, occ)?;
            let node = self.program.blocks[self.block].nodes[n as usize].clone();
            let shape = node.out.resolve(self.h);
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
                Prim::Transpose { perm } => {
                    let s = self.shape(inputs[0])?;
                    let mut q = vec![0; s.len()];
                    for (k, p) in perm.iter().enumerate() {
                        q[*p as usize] = c[k];
                    }
                    self.fetch(inputs[0], index(&q, &s)?)?;
                }
                Prim::Slice { axis, start } => {
                    let s = self.shape(inputs[0])?;
                    let mut q = c.clone();
                    q[axis as usize] = q[axis as usize].checked_add(start as usize).ok_or("cover Slice overflow")?;
                    self.fetch(inputs[0], index(&q, &s)?)?;
                }
                Prim::Concat { axis } => {
                    let mut q = c.clone();
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
                    let mut xi = c[..bd].to_vec();
                    xi.extend_from_slice(&c[a..a + m]);
                    let ix = index(&xi, &x)?;
                    self.fetch(inputs[1], ix)?;
                    let iv = self.bounds(inputs[1], ix)?;
                    let lo = iv.lo.max(0);
                    let hi = iv.hi.min(d[a] as i128 - 1);
                    if lo <= hi {
                        for v in usize::try_from(lo).map_err(|_| "cover Gather lower overflow")?
                            ..=usize::try_from(hi).map_err(|_| "cover Gather upper overflow")?
                        {
                            let mut q = c[..a].to_vec();
                            q.push(v);
                            q.extend_from_slice(&c[a + m..]);
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
                        let mut q = c.clone();
                        q[axis as usize] = t;
                        self.fetch(inputs[0], index(&q, &s)?)?;
                    }
                }
                Prim::HistAppend { state } => {
                    let row = count(&self.program.states[state as usize].shape.iter().map(|d| *d as usize).collect::<Vec<_>>())?;
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
