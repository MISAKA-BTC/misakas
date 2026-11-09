//! Original-bound numerical parameter footprint, not paid admission, custody,
//! delivery, verification credit or a promise that the footprint is affordable.
use super::PalwTirPublicSourceV1;
use crate::Hash64;
use crate::palw_tir_court_v1::parameter_cover::{CoverErrorV1, ParameterCoverBudgetV1, PreparedParameterCoverV1, RootInputBoundaryV1};
use std::collections::BTreeSet;

/// Explicit caller-admitted exploration and output bounds. No defaults, adopted
/// network constants or model exemptions are introduced by this read-only API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OriginalParameterCoverBudgetV1 {
    pub max_original_leaves: u64,
    pub max_total_edges: u64,
    pub max_total_coordinates: u64,
    /// Counts repeated coordinates separately under each named root. A union
    /// limit alone cannot bound the retained root-to-parameter relation.
    pub max_root_memberships: u64,
}
/// Separate bounds on one root's live walk and the complete traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamingParameterCoverBudgetV1 {
    pub total: OriginalParameterCoverBudgetV1,
    pub per_root_edges: u64,
    pub per_root_coordinates: u64,
    pub per_root_memberships: u64,
    /// Additional construction and walk memory; the supplied Source keeps
    /// its own lease. The sizing callback must separately admit its scratch.
    pub max_workspace_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamingParameterCoverSummaryV1 {
    pub source_id: Hash64,
    pub original_leaves: u64,
    pub numerical_roots: u64,
    pub total_edges: u64,
    pub total_coordinates: u64,
    pub root_memberships: u64,
    /// (leaf, measured bytes); first canonical leaf wins a tie.
    pub largest_root: Option<(u64, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedParameterCoverV1 {
    pub leaf: u64,
    pub parameters: Vec<u32>,
    pub explored_edges: u64,
    pub explored_coordinates: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginalParameterCoverV1 {
    pub source_id: Hash64,
    pub original_leaves: u64,
    /// Ascending original leaf order, including parameter-free numerical roots.
    pub roots: Vec<NamedParameterCoverV1>,
    /// Ascending unique registered inventory coordinates across all roots.
    pub union: Vec<u32>,
    pub total_edges: u64,
    pub total_coordinates: u64,
    pub root_memberships: u64,
}
/// Source-bound, conservative numerical-root input coordinates. Private
/// constructor; this is data dependency geometry, not custody/complete admission.
#[derive(Debug, PartialEq, Eq)]
pub struct OriginalRootInputCoverV1 {
    parameters: OriginalParameterCoverV1,
    roots: Vec<NamedRootInputCoverV1>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct NamedRootInputCoverV1 {
    pub leaf: u64,
    pub inputs: Vec<crate::palw_tir_court_v1::parameter_cover::RootInputBoundaryV1>,
}
impl OriginalRootInputCoverV1 {
    pub fn parameters_v1(&self) -> &OriginalParameterCoverV1 {
        &self.parameters
    }
    pub fn roots_v1(&self) -> &[NamedRootInputCoverV1] {
        &self.roots
    }
}
fn native_input_requests_v1(leaves: u64, edges: u64) -> Result<u64, crate::palw_native_resource_v1::NativeCoverageV1> {
    use crate::palw_native_resource_v1::{NativeCoverageV1, TirNativeRequestsV1, btree_node_envelope_bytes_v1};
    use crate::palw_tir_court_v1::parameter_cover::RootInputBoundaryV1 as I;
    let overflow = || NativeCoverageV1::overflow::<OriginalRootInputCoverV1>("original input dependency requests");
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<NamedRootInputCoverV1>(leaves)?;
    q.vec_v1::<I>(edges)?; // total retained per-root boundary lengths
    let mut minimum = TirNativeRequestsV1::new();
    minimum.vec_v1::<I>(1)?;
    q.add_v1(leaves.checked_mul(minimum.bytes_v1()).ok_or_else(overflow)?)?;
    q.add_v1(edges.checked_mul(btree_node_envelope_bytes_v1::<I, ()>() as u64).ok_or_else(overflow)?)?;
    q.exact_v1::<OriginalRootInputCoverV1>(1)?;
    Ok(q.bytes_v1())
}
fn grow_cover_memory_v1(
    ticket: &mut crate::palw_native_resource_v1::NativeDemandLeaseV1,
    additional: u64,
) -> Result<(), crate::palw_native_resource_v1::NativeCoverageV1> {
    use crate::palw_native_resource_v1::NativeCoverageV1;
    let construction = ticket.memory_v1();
    ticket.grow_pending_memory_v1(additional).map_err(|error| match error {
        NativeCoverageV1::Unsupported { component, native_type, reason } => NativeCoverageV1::Unsupported {
            component,
            native_type,
            reason: format!("{reason} (TIR all-root construction_bytes={construction}, additional_bytes={additional})"),
        },
        other => other,
    })
}
fn add(a: u64, b: u64) -> Result<u64, CoverErrorV1> {
    a.checked_add(b).ok_or(CoverErrorV1::Local("original parameter cover counter overflow"))
}
impl PalwTirPublicSourceV1 {
    /// Reconstruct the actual native original from its retained header and the
    /// registered class/layout. No producer root or model bytes are inputs.
    /// Decode/verification/index/range-analysis allocation occurs once, outside
    /// the exploration meter, and requires separate native resource preflight.
    /// Each leaf_at enumeration also has native CPU work beyond the Walk meter.
    /// The caller must charge that scan, scratch/retained output, authentication,
    /// acquisition, retention and financial obligations before admitting claims.
    pub fn parameter_cover_v1(
        &self,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget: OriginalParameterCoverBudgetV1,
    ) -> Result<OriginalParameterCoverV1, String> {
        if self.header.step_leaf_count > budget.max_original_leaves {
            return Err("original parameter cover leaf budget exhausted".into());
        }
        let binding =
            self.header.resolve_registered_layout_v1(registered, layout, self.header.committed_execution_root, self.maximum_leaves)?;
        let verified = crate::palw_tir_step_v1::verify_tir_binding_v1(&binding, self.maximum_leaves).map_err(|e| e.to_string())?;
        if verified.class_id != self.class_id || verified.context_hash != self.context_hash {
            return Err("original parameter cover substitutes source geometry".into());
        }
        // Binding verification, inventory index and range analysis are shared
        // for all roots. Only the finite potential-coordinate walks repeat.
        let prepared = PreparedParameterCoverV1::new(Box::new(verified), binding.job_context)?;
        self.parameter_cover_prepared_v1(prepared, budget)
    }

    /// Additional native allocation owner for the all-original parameter relation.
    /// Source (and any numerical geometry) keep their own live reservations.
    /// This authenticates original/program coordinates, not material availability,
    /// prompt dependencies, native CPU, fees, custody or complete admission.
    pub fn parameter_cover_native_v1(
        source: &crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget: OriginalParameterCoverBudgetV1,
    ) -> Result<
        crate::palw_native_resource_v1::NativeRetainedV1<OriginalParameterCoverV1>,
        crate::palw_native_resource_v1::NativeDecodeErrorV1<String>,
    > {
        Self::parameter_cover_native_reserved_v1(
            source,
            registered,
            layout,
            budget,
            crate::palw_native_resource_v1::reserve_native_requests_v1,
        )
    }
    fn parameter_cover_native_reserved_v1<R>(
        source: &crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget: OriginalParameterCoverBudgetV1,
        reserve: R,
    ) -> Result<
        crate::palw_native_resource_v1::NativeRetainedV1<OriginalParameterCoverV1>,
        crate::palw_native_resource_v1::NativeDecodeErrorV1<String>,
    >
    where
        R: FnOnce(
            u64,
        ) -> Result<
            Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
            crate::palw_native_resource_v1::NativeCoverageV1,
        >,
    {
        use crate::palw_native_resource_v1::{NativeDecodeErrorV1 as E, NativeRetainedV1};
        if source.header.step_leaf_count > budget.max_original_leaves {
            return Err(CoverErrorV1::Local("original parameter cover leaf budget exhausted").native());
        }
        // Reserve before resolving/decoding/verifying. This is an additional
        // conservative construction peak, not a borrow of Source's ticket.
        let requests = Self::native_construction_requests_v1(registered, layout, &source.header)?;
        let mut lease = reserve(requests).map_err(E::Resource)?;
        let binding = source
            .header
            .resolve_registered_layout_v1(registered, layout, source.header.committed_execution_root, source.maximum_leaves)
            .map_err(E::Decode)?;
        let verified =
            crate::palw_tir_step_v1::verify_tir_binding_v1(&binding, source.maximum_leaves).map_err(|e| E::Decode(e.to_string()))?;
        if verified.class_id != source.class_id || verified.context_hash != source.context_hash {
            return Err(E::Decode("original parameter cover substitutes source geometry".into()));
        }
        let additional = PreparedParameterCoverV1::native_all_roots_requests_v1(
            &verified.space.program,
            &registered.program,
            source.header.step_leaf_count,
            budget,
        )?;
        if let Some(ticket) = lease.as_mut() {
            grow_cover_memory_v1(ticket, additional).map_err(E::Resource)?;
        }
        let prepared = PreparedParameterCoverV1::new(Box::new(verified), binding.job_context).map_err(E::Decode)?;
        let cover = source.parameter_cover_prepared_typed_v1(prepared, budget).map_err(CoverErrorV1::native)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(cover, lease))
    }

    /// Additional coordinate envelope for every numerical root: actual token
    /// positions and previous-occurrence committed carry units. Does not claim
    /// all source/semantic/termination branches or data availability.
    pub fn root_input_cover_native_v1(
        source: &crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget: OriginalParameterCoverBudgetV1,
    ) -> Result<
        crate::palw_native_resource_v1::NativeRetainedV1<OriginalRootInputCoverV1>,
        crate::palw_native_resource_v1::NativeDecodeErrorV1<String>,
    > {
        Self::root_input_cover_native_reserved_v1(
            source,
            registered,
            layout,
            budget,
            crate::palw_native_resource_v1::reserve_native_requests_v1,
        )
    }
    fn root_input_cover_native_reserved_v1<R>(
        source: &crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget: OriginalParameterCoverBudgetV1,
        reserve: R,
    ) -> Result<
        crate::palw_native_resource_v1::NativeRetainedV1<OriginalRootInputCoverV1>,
        crate::palw_native_resource_v1::NativeDecodeErrorV1<String>,
    >
    where
        R: FnOnce(
            u64,
        ) -> Result<
            Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
            crate::palw_native_resource_v1::NativeCoverageV1,
        >,
    {
        use crate::palw_native_resource_v1::{NativeDecodeErrorV1 as E, NativeRetainedV1};
        if source.header.step_leaf_count > budget.max_original_leaves {
            return Err(CoverErrorV1::Local("original parameter cover leaf budget exhausted").native());
        }
        // Reserve before resolving/decoding/verifying. This is an additional
        // conservative construction peak, not a borrow of Source's ticket.
        let requests = Self::native_construction_requests_v1(registered, layout, &source.header)?;
        let mut lease = reserve(requests).map_err(E::Resource)?;
        let binding = source
            .header
            .resolve_registered_layout_v1(registered, layout, source.header.committed_execution_root, source.maximum_leaves)
            .map_err(E::Decode)?;
        let verified =
            crate::palw_tir_step_v1::verify_tir_binding_v1(&binding, source.maximum_leaves).map_err(|e| E::Decode(e.to_string()))?;
        if verified.class_id != source.class_id || verified.context_hash != source.context_hash {
            return Err(E::Decode("original parameter cover substitutes source geometry".into()));
        }
        let additional = PreparedParameterCoverV1::native_all_roots_requests_v1(
            &verified.space.program,
            &registered.program,
            source.header.step_leaf_count,
            budget,
        )?;
        let additional = additional
            .checked_add(native_input_requests_v1(source.header.step_leaf_count, budget.max_total_edges).map_err(E::Resource)?)
            .ok_or_else(|| {
                E::Resource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<OriginalRootInputCoverV1>(
                    "input owner growth",
                ))
            })?;
        if let Some(ticket) = lease.as_mut() {
            grow_cover_memory_v1(ticket, additional).map_err(E::Resource)?;
        }
        let prepared = PreparedParameterCoverV1::new(Box::new(verified), binding.job_context).map_err(E::Decode)?;
        let mut roots = Vec::new();
        let parameters =
            source.parameter_cover_prepared_tracked_v1(prepared, budget, Some(&mut roots)).map_err(CoverErrorV1::native)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(OriginalRootInputCoverV1 { parameters, roots }, lease))
    }

    /// Size one root at a time in canonical order. The callback receives
    /// borrowed coordinates and returns its measured byte count. Its calls
    /// are provisional: only a successful summary certifies full traversal.
    /// No network, consensus, or retained root-to-parameter rows are changed.
    pub fn parameter_cover_streaming_size_v1(
        source: &crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget: StreamingParameterCoverBudgetV1,
        mut size_root: impl FnMut(u64, &[u32]) -> Result<u64, String>,
    ) -> Result<StreamingParameterCoverSummaryV1, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        Self::root_cover_streaming_size_v1(
            source,
            registered,
            layout,
            budget,
            false,
            0..source.leaf_count_v1(),
            |leaf, parameters, _| size_root(leaf, parameters),
        )
    }

    /// Size the numerical roots with their canonical parameter and dynamic
    /// input boundaries together. The callback must separately budget its
    /// own codec/authentication scratch. Neither the input material nor an
    /// original-wide boundary table is loaded or retained by this walk.
    pub fn root_input_cover_streaming_size_v1(
        source: &crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget: StreamingParameterCoverBudgetV1,
        size_root: impl FnMut(u64, &[u32], &BTreeSet<RootInputBoundaryV1>) -> Result<u64, String>,
    ) -> Result<StreamingParameterCoverSummaryV1, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        Self::root_cover_streaming_size_v1(source, registered, layout, budget, true, 0..source.leaf_count_v1(), size_root)
    }

    /// Resolve one authenticated numerical coordinate under the same per-root
    /// and workspace limits as the complete streaming walk.
    pub(crate) fn visit_selected_root_inputs_v1(
        source: &crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget: StreamingParameterCoverBudgetV1,
        leaf: u64,
        mut visit: impl FnMut(&[u32], &BTreeSet<RootInputBoundaryV1>) -> Result<(), String>,
    ) -> Result<(), crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        use crate::palw_native_resource_v1::NativeDecodeErrorV1;
        if leaf >= source.leaf_count_v1() {
            return Err(NativeDecodeErrorV1::Decode("selected numerical leaf is outside the original".into()));
        }
        let summary =
            Self::root_cover_streaming_size_v1(source, registered, layout, budget, true, leaf..leaf + 1, |_, params, inputs| {
                visit(params, inputs)?;
                Ok(0)
            })?;
        if summary.numerical_roots != 1 {
            return Err(NativeDecodeErrorV1::Decode("selected leaf is not a numerical root".into()));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn root_cover_streaming_size_v1(
        source: &crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        budget: StreamingParameterCoverBudgetV1,
        include_inputs: bool,
        leaves: std::ops::Range<u64>,
        mut size_root: impl FnMut(u64, &[u32], &BTreeSet<RootInputBoundaryV1>) -> Result<u64, String>,
    ) -> Result<StreamingParameterCoverSummaryV1, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        use crate::palw_native_resource_v1::{NativeDecodeErrorV1 as E, reserve_native_pending_requests_v1};
        let original_leaves = source.header.step_leaf_count;
        if original_leaves > budget.total.max_original_leaves {
            return Err(CoverErrorV1::Local("original parameter cover leaf budget exhausted").native());
        }
        let initial = Self::native_construction_requests_v1(registered, layout, &source.header)?;
        let workspace_refusal = || CoverErrorV1::Local("streaming parameter cover workspace budget exhausted").native();
        if initial > budget.max_workspace_bytes {
            return Err(workspace_refusal());
        }
        let mut lease = reserve_native_pending_requests_v1(initial).map_err(E::Resource)?;
        let binding = source
            .header
            .resolve_registered_layout_v1(registered, layout, source.header.committed_execution_root, source.maximum_leaves)
            .map_err(E::Decode)?;
        let verified =
            crate::palw_tir_step_v1::verify_tir_binding_v1(&binding, source.maximum_leaves).map_err(|e| E::Decode(e.to_string()))?;
        if verified.class_id != source.class_id || verified.context_hash != source.context_hash {
            return Err(E::Decode("original parameter cover substitutes source geometry".into()));
        }
        let per_root = OriginalParameterCoverBudgetV1 {
            max_original_leaves: 1,
            max_total_edges: budget.per_root_edges.min(budget.total.max_total_edges),
            max_total_coordinates: budget.per_root_coordinates.min(budget.total.max_total_coordinates),
            max_root_memberships: budget.per_root_memberships.min(budget.total.max_root_memberships),
        };
        // The materializer's quote for ONE retained row (including its
        // conservative one-row union allowance) covers our peak. Cumulative
        // exploration never sizes simultaneously owned coordinates here.
        let mut additional =
            PreparedParameterCoverV1::native_all_roots_requests_v1(&verified.space.program, &registered.program, 1, per_root)?;
        if include_inputs {
            let boundaries = native_input_requests_v1(1, per_root.max_total_edges).map_err(E::Resource)?;
            additional = additional.checked_add(boundaries).ok_or_else(workspace_refusal)?;
        }
        if additional > budget.max_workspace_bytes - initial {
            return Err(workspace_refusal());
        }
        if let Some(ticket) = lease.as_mut() {
            grow_cover_memory_v1(ticket, additional).map_err(E::Resource)?;
        }
        let prepared = PreparedParameterCoverV1::new(Box::new(verified), binding.job_context).map_err(E::Decode)?;
        let mut largest_root = None;
        let mut summary = source
            .visit_prepared_roots_v1(prepared, budget.total, per_root, include_inputs, leaves, |root, inputs| {
                let bytes = size_root(root.leaf, &root.parameters, &inputs).map_err(CoverErrorV1::Semantic)?;
                if largest_root.is_none_or(|(_, largest)| bytes > largest) {
                    largest_root = Some((root.leaf, bytes));
                }
                Ok(())
            })
            .map_err(CoverErrorV1::native)?;
        summary.largest_root = largest_root;
        Ok(summary)
    }

    fn parameter_cover_prepared_v1(
        &self,
        prepared: PreparedParameterCoverV1,
        budget: OriginalParameterCoverBudgetV1,
    ) -> Result<OriginalParameterCoverV1, String> {
        self.parameter_cover_prepared_typed_v1(prepared, budget).map_err(CoverErrorV1::legacy)
    }
    fn parameter_cover_prepared_typed_v1(
        &self,
        prepared: PreparedParameterCoverV1,
        budget: OriginalParameterCoverBudgetV1,
    ) -> Result<OriginalParameterCoverV1, CoverErrorV1> {
        self.parameter_cover_prepared_tracked_v1(prepared, budget, None)
    }
    fn parameter_cover_prepared_tracked_v1(
        &self,
        prepared: PreparedParameterCoverV1,
        budget: OriginalParameterCoverBudgetV1,
        mut inputs: Option<&mut Vec<NamedRootInputCoverV1>>,
    ) -> Result<OriginalParameterCoverV1, CoverErrorV1> {
        let mut roots = Vec::new();
        let mut union = BTreeSet::new();
        let summary =
            self.visit_prepared_roots_v1(prepared, budget, budget, inputs.is_some(), 0..self.leaf_count_v1(), |root, boundaries| {
                if let Some(out) = inputs.as_mut() {
                    out.push(NamedRootInputCoverV1 { leaf: root.leaf, inputs: boundaries.into_iter().collect() });
                }
                union.extend(root.parameters.iter().copied());
                roots.push(root);
                Ok(())
            })?;
        Ok(OriginalParameterCoverV1 {
            source_id: summary.source_id,
            original_leaves: summary.original_leaves,
            roots,
            union: union.into_iter().collect(),
            total_edges: summary.total_edges,
            total_coordinates: summary.total_coordinates,
            root_memberships: summary.root_memberships,
        })
    }

    fn visit_prepared_roots_v1(
        &self,
        prepared: PreparedParameterCoverV1,
        budget: OriginalParameterCoverBudgetV1,
        per_root: OriginalParameterCoverBudgetV1,
        include_inputs: bool,
        leaves: std::ops::Range<u64>,
        mut visit: impl FnMut(
            NamedParameterCoverV1,
            BTreeSet<crate::palw_tir_court_v1::parameter_cover::RootInputBoundaryV1>,
        ) -> Result<(), CoverErrorV1>,
    ) -> Result<StreamingParameterCoverSummaryV1, CoverErrorV1> {
        let mut out = StreamingParameterCoverSummaryV1 {
            source_id: self.source_id,
            original_leaves: self.header.step_leaf_count,
            numerical_roots: 0,
            total_edges: 0,
            total_coordinates: 0,
            root_memberships: 0,
            largest_root: None,
        };
        for leaf in leaves {
            let remaining_edges =
                budget.max_total_edges.checked_sub(out.total_edges).ok_or(CoverErrorV1::Local("original cover edge accounting"))?;
            let remaining_coordinates = budget
                .max_total_coordinates
                .checked_sub(out.total_coordinates)
                .ok_or(CoverErrorV1::Local("original cover coordinate accounting"))?;
            let remaining_memberships = budget
                .max_root_memberships
                .checked_sub(out.root_memberships)
                .ok_or(CoverErrorV1::Local("original cover membership accounting"))?
                .min(per_root.max_root_memberships);
            let walk_budget = ParameterCoverBudgetV1 {
                max_edges: remaining_edges.min(per_root.max_total_edges),
                max_coordinates: remaining_coordinates.min(per_root.max_total_coordinates),
            };
            let mut boundaries = BTreeSet::new();
            let cover = if include_inputs {
                prepared.named_with_inputs_v1(leaf, walk_budget, remaining_memberships, &mut boundaries)?
            } else {
                prepared.named_with_leaf_limit_typed(leaf, walk_budget, remaining_memberships)?
            };
            let Some(cover) = cover else { continue };
            let members = u64::try_from(cover.leaves.len()).map_err(|_| "original cover membership count overflow")?;
            if members > remaining_memberships {
                return Err(CoverErrorV1::Local("original parameter cover membership budget exhausted"));
            }
            out.total_edges = add(out.total_edges, cover.explored_edges)?;
            out.total_coordinates = add(out.total_coordinates, cover.explored_coordinates)?;
            out.root_memberships = add(out.root_memberships, members)?;
            out.numerical_roots = add(out.numerical_roots, 1)?;
            visit(
                NamedParameterCoverV1 {
                    leaf,
                    parameters: cover.leaves,
                    explored_edges: cover.explored_edges,
                    explored_coordinates: cover.explored_coordinates,
                },
                boundaries,
            )?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod native_owner_tests {
    use super::*;
    const B: OriginalParameterCoverBudgetV1 = OriginalParameterCoverBudgetV1 {
        max_original_leaves: 1_000_000,
        max_total_edges: 10_000_000,
        max_total_coordinates: 1_000_000,
        max_root_memberships: 1_000_000,
    };
    #[test]
    fn streaming_cover_matches_all_roots_and_withholds_summary_on_any_failure() {
        use crate::palw_native_resource_v1::NativeDecodeErrorV1;
        let f = crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
        let original = f.frame.original_v1();
        let source = PalwTirPublicSourceV1::new_native_v1(
            &f.record,
            &f.layout,
            f.frame.header_v1(),
            original.execution_root,
            original.trace_root,
            f.rules.maximum_leaves,
        )
        .unwrap();
        let expected = source.parameter_cover_v1(&f.record, &f.layout, B).unwrap();
        assert!(!expected.roots.is_empty());
        let budget = StreamingParameterCoverBudgetV1 {
            total: OriginalParameterCoverBudgetV1 {
                max_original_leaves: expected.original_leaves,
                max_total_edges: expected.total_edges,
                max_total_coordinates: expected.total_coordinates,
                max_root_memberships: expected.root_memberships,
            },
            per_root_edges: expected.roots.iter().map(|root| root.explored_edges).max().unwrap(),
            per_root_coordinates: expected.roots.iter().map(|root| root.explored_coordinates).max().unwrap(),
            per_root_memberships: expected.roots.iter().map(|root| root.parameters.len() as u64).max().unwrap(),
            max_workspace_bytes: 128 << 20,
        };
        let mut visited = 0;
        let got =
            PalwTirPublicSourceV1::parameter_cover_streaming_size_v1(&source, &f.record, &f.layout, budget, |leaf, parameters| {
                assert_eq!((leaf, parameters), (expected.roots[visited].leaf, expected.roots[visited].parameters.as_slice()));
                visited += 1;
                // Constant size also checks that ties keep the first leaf.
                Ok(100)
            })
            .unwrap();
        assert_eq!(visited, expected.roots.len());
        assert_eq!(got.source_id, expected.source_id);
        assert_eq!(got.original_leaves, expected.original_leaves);
        assert_eq!(got.numerical_roots, expected.roots.len() as u64);
        assert_eq!(got.total_edges, expected.total_edges);
        assert_eq!(got.total_coordinates, expected.total_coordinates);
        assert_eq!(got.root_memberships, expected.root_memberships);
        assert_eq!(got.largest_root, Some((expected.roots[0].leaf, 100)));

        let total = OriginalParameterCoverBudgetV1 { max_total_edges: expected.total_edges - 1, ..budget.total };
        for too_small in [
            StreamingParameterCoverBudgetV1 { total, ..budget },
            StreamingParameterCoverBudgetV1 { per_root_edges: budget.per_root_edges - 1, ..budget },
            StreamingParameterCoverBudgetV1 { max_workspace_bytes: 0, ..budget },
        ] {
            assert!(matches!(
                PalwTirPublicSourceV1::parameter_cover_streaming_size_v1(&source, &f.record, &f.layout, too_small, |_, _| Ok(100)),
                Err(NativeDecodeErrorV1::Resource(_))
            ));
        }
        assert!(matches!(
            PalwTirPublicSourceV1::parameter_cover_streaming_size_v1(&source, &f.record, &f.layout, budget, |_, _| {
                Err("diagnostic sizing refused".into())
            }),
            Err(NativeDecodeErrorV1::Decode(error)) if error == "diagnostic sizing refused"
        ));
    }

    #[test]
    fn native_original_parameter_cover_preserves_actual_all_root_relation() {
        let f = crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
        let original = f.frame.original_v1();
        let source = PalwTirPublicSourceV1::new_native_v1(
            &f.record,
            &f.layout,
            f.frame.header_v1(),
            original.execution_root,
            original.trace_root,
            f.rules.maximum_leaves,
        )
        .unwrap();
        let legacy = source.parameter_cover_v1(&f.record, &f.layout, B).unwrap();
        let native = PalwTirPublicSourceV1::parameter_cover_native_v1(&source, &f.record, &f.layout, B).unwrap();
        assert!(!legacy.roots.is_empty());
        assert!(!legacy.union.is_empty());
        assert_eq!(*native, legacy);
        assert_eq!(native.root_memberships, native.roots.iter().map(|r| r.parameters.len() as u64).sum::<u64>());
        let low = OriginalParameterCoverBudgetV1 { max_total_edges: 0, ..B };
        assert!(matches!(
            PalwTirPublicSourceV1::parameter_cover_native_v1(&source, &f.record, &f.layout, low),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))
        ));
        for small in [
            OriginalParameterCoverBudgetV1 { max_original_leaves: 0, ..B },
            OriginalParameterCoverBudgetV1 { max_total_coordinates: 0, ..B },
            OriginalParameterCoverBudgetV1 { max_root_memberships: 0, ..B },
        ] {
            assert!(source.parameter_cover_v1(&f.record, &f.layout, small).is_err());
            assert!(matches!(
                PalwTirPublicSourceV1::parameter_cover_native_v1(&source, &f.record, &f.layout, small),
                Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))
            ));
        }
        let binding = source
            .header
            .resolve_registered_layout_v1(&f.record, &f.layout, source.header.committed_execution_root, source.maximum_leaves)
            .unwrap();
        let verified = crate::palw_tir_step_v1::verify_tir_binding_v1(&binding, source.maximum_leaves).unwrap();
        let prelude = PalwTirPublicSourceV1::native_construction_requests_v1(&f.record, &f.layout, &source.header).unwrap();
        let additional = PreparedParameterCoverV1::native_all_roots_requests_v1(
            &verified.space.program,
            &f.record.program,
            source.header.step_leaf_count,
            B,
        )
        .unwrap();
        let total = prelude.checked_add(additional).unwrap();
        for quota in [prelude - 1, total - 1, total] {
            let checker = crate::palw_native_resource_v1::NativeDemandCheckerV1::new(
                crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1 { memory_bytes: quota, database_bytes: 0 },
            );
            let result = PalwTirPublicSourceV1::parameter_cover_native_reserved_v1(&source, &f.record, &f.layout, B, |bytes| {
                checker.reserve_requests_v1(bytes, 0).map(Some)
            });
            if quota == total {
                let owned = result.unwrap();
                assert_eq!(*owned, legacy);
                assert_eq!(checker.outstanding(), (total, 0));
                drop(owned);
            } else if quota == total - 1 {
                use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1};
                assert!(matches!(result, Err(NativeDecodeErrorV1::Resource(NativeCoverageV1::Unsupported { reason, .. }))
                    if reason.contains(&format!("construction_bytes={prelude}, additional_bytes={additional}"))));
            } else {
                assert!(matches!(result, Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
            }
            assert_eq!(checker.outstanding(), (0, 0));
        }
        let unbounded = OriginalParameterCoverBudgetV1 {
            max_original_leaves: u64::MAX,
            max_total_edges: u64::MAX,
            max_total_coordinates: u64::MAX,
            max_root_memberships: u64::MAX,
        };
        assert!(matches!(
            PreparedParameterCoverV1::native_all_roots_requests_v1(
                &verified.space.program,
                &f.record.program,
                source.header.step_leaf_count,
                unbounded
            ),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))
        ));
    }
}

#[cfg(test)]
mod root_input_tests {
    use super::*;
    use crate::palw_tir_court_v1::parameter_cover::RootInputBoundaryV1 as I;
    #[test]
    fn actual_root_input_coordinates_keep_parameter_relation_and_authenticate_carry_units() {
        let f = crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
        let original = f.frame.original_v1();
        let source = PalwTirPublicSourceV1::new_native_v1(
            &f.record,
            &f.layout,
            f.frame.header_v1(),
            original.execution_root,
            original.trace_root,
            f.rules.maximum_leaves,
        )
        .unwrap();
        let budget = OriginalParameterCoverBudgetV1 {
            max_original_leaves: 1_000_000,
            max_total_edges: 10_000_000,
            max_total_coordinates: 1_000_000,
            max_root_memberships: 1_000_000,
        };
        let old = source.parameter_cover_v1(&f.record, &f.layout, budget).unwrap();
        let input = PalwTirPublicSourceV1::root_input_cover_native_v1(&source, &f.record, &f.layout, budget).unwrap();
        assert_eq!(input.parameters, old);
        assert_eq!(input.roots.iter().map(|r| r.leaf).collect::<Vec<_>>(), old.roots.iter().map(|r| r.leaf).collect::<Vec<_>>());
        let mut carry = 0;
        let mut tokens = 0;
        use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
        for root in &input.roots {
            for boundary in &root.inputs {
                match *boundary {
                    I::CarryStepLeaf(index)
                    | I::CommittedStepLeaf(index)
                    | I::FixedCheckpointStepLeaf(index)
                    | I::HistoryStepLeaf(index) => {
                        if matches!(boundary, I::CarryStepLeaf(_)) {
                            carry += 1;
                        }
                        assert!(index < root.leaf);
                        let query = super::super::PalwTirPublicRequestV1::StepLeaf { index };
                        let response = super::super::PalwTirPublicResponseV1::StepLeaf {
                            preimage: f.execution.step_leaf(index).unwrap(),
                            opening: f.execution.step_opening(index).unwrap(),
                        };
                        let unit = source.authenticate_v1(&query, &response, None).unwrap();
                        assert_eq!(unit.source_id_v1(), source.source_id_v1());
                        assert_eq!(unit.request_v1(), query);
                    }
                    I::PromptPosition(pos) => {
                        tokens += 1;
                        assert!(pos < source.header.job_context.declared_prefill_tokens);
                    }
                    I::GeneratedId(index) => {
                        tokens += 1;
                        assert!(index < source.header.job_context.exact_decode_tokens - 1);
                    }
                }
            }
        }
        assert!(carry > 0, "fixture must exercise actual previous-occurrence carry material");
        // Token reads can be hidden behind authenticated committed carries;
        // the complete parameter cover must not invent prompt dependencies.
        let _ = tokens;
        let binding = source
            .header
            .resolve_registered_layout_v1(&f.record, &f.layout, source.header.committed_execution_root, source.maximum_leaves)
            .unwrap();
        let verified = crate::palw_tir_step_v1::verify_tir_binding_v1(&binding, source.maximum_leaves).unwrap();
        let prelude = PalwTirPublicSourceV1::native_construction_requests_v1(&f.record, &f.layout, &source.header).unwrap();
        let additional = PreparedParameterCoverV1::native_all_roots_requests_v1(
            &verified.space.program,
            &f.record.program,
            source.header.step_leaf_count,
            budget,
        )
        .unwrap()
            + native_input_requests_v1(source.header.step_leaf_count, budget.max_total_edges).unwrap();
        let total = prelude.checked_add(additional).unwrap();
        for quota in [prelude - 1, total - 1, total] {
            let checker = crate::palw_native_resource_v1::NativeDemandCheckerV1::new(
                crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1 { memory_bytes: quota, database_bytes: 0 },
            );
            let result = PalwTirPublicSourceV1::root_input_cover_native_reserved_v1(&source, &f.record, &f.layout, budget, |bytes| {
                checker.reserve_requests_v1(bytes, 0).map(Some)
            });
            if quota == total {
                let owned = result.unwrap();
                assert_eq!(*owned, *input);
                assert_eq!(checker.outstanding(), (total, 0));
                drop(owned);
            } else {
                assert!(matches!(result, Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
            }
            assert_eq!(checker.outstanding(), (0, 0));
        }
        assert!(native_input_requests_v1(u64::MAX, u64::MAX).is_err());
    }
    #[test]
    fn actual_input0_numerical_roots_name_prompt_and_generated_coordinates() {
        use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1 as F;
        for form in [F::Flat, F::MerkleV1] {
            let f = crate::palw_tir_public_claim_v1::palw_tir_public_input_history_frame_fixture_v1(form);
            let original = f.frame.original_v1();
            assert_eq!(f.frame.plan_v1().owner_work.rules.prompt_form, form);
            let source = PalwTirPublicSourceV1::new_native_v1(
                &f.record,
                &f.layout,
                f.frame.header_v1(),
                original.execution_root,
                original.trace_root,
                f.rules.maximum_leaves,
            )
            .unwrap();
            let budget = OriginalParameterCoverBudgetV1 {
                max_original_leaves: 1_000_000,
                max_total_edges: 10_000_000,
                max_total_coordinates: 1_000_000,
                max_root_memberships: 1_000_000,
            };
            let cover = PalwTirPublicSourceV1::root_input_cover_native_v1(&source, &f.record, &f.layout, budget).unwrap();
            assert_eq!(cover.parameters, source.parameter_cover_v1(&f.record, &f.layout, budget).unwrap());
            let streaming = StreamingParameterCoverBudgetV1 {
                total: budget,
                per_root_edges: cover.parameters.roots.iter().map(|root| root.explored_edges).max().unwrap(),
                per_root_coordinates: cover.parameters.roots.iter().map(|root| root.explored_coordinates).max().unwrap(),
                per_root_memberships: cover.parameters.roots.iter().map(|root| root.parameters.len() as u64).max().unwrap(),
                max_workspace_bytes: 128 << 20,
            };
            let mut visited = 0;
            let summary = PalwTirPublicSourceV1::root_input_cover_streaming_size_v1(
                &source,
                &f.record,
                &f.layout,
                streaming,
                |leaf, parameters, inputs| {
                    let expected = &cover.roots[visited];
                    assert_eq!(leaf, expected.leaf);
                    assert_eq!(parameters, cover.parameters.roots[visited].parameters.as_slice());
                    assert!(inputs.iter().eq(expected.inputs.iter()));
                    visited += 1;
                    Ok(inputs.len() as u64)
                },
            )
            .unwrap();
            assert_eq!(visited, cover.roots.len());
            assert_eq!(summary.total_edges, cover.parameters.total_edges);
            assert_eq!(summary.total_coordinates, cover.parameters.total_coordinates);
            assert_eq!(summary.root_memberships, cover.parameters.root_memberships);
            let largest = cover.roots.iter().rev().max_by_key(|root| root.inputs.len()).unwrap();
            assert_eq!(summary.largest_root, Some((largest.leaf, largest.inputs.len() as u64)));
            let selected = cover.roots.len() - 1;
            let mut selected_calls = 0;
            PalwTirPublicSourceV1::visit_selected_root_inputs_v1(
                &source,
                &f.record,
                &f.layout,
                streaming,
                cover.roots[selected].leaf,
                |parameters, inputs| {
                    assert_eq!(parameters, cover.parameters.roots[selected].parameters.as_slice());
                    assert!(inputs.iter().eq(cover.roots[selected].inputs.iter()));
                    selected_calls += 1;
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(selected_calls, 1);
            assert!(
                PalwTirPublicSourceV1::visit_selected_root_inputs_v1(
                    &source,
                    &f.record,
                    &f.layout,
                    streaming,
                    source.leaf_count_v1(),
                    |_, _| panic!("out-of-range leaf must not visit a root"),
                )
                .is_err()
            );
            assert!(matches!(
                PalwTirPublicSourceV1::root_input_cover_streaming_size_v1(
                    &source,
                    &f.record,
                    &f.layout,
                    StreamingParameterCoverBudgetV1 { max_workspace_bytes: 0, ..streaming },
                    |_, _, _| panic!("refused workspace must not visit roots"),
                ),
                Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))
            ));
            assert!(cover.roots.iter().any(|r| r.inputs.iter().any(|i| matches!(i, I::PromptPosition(_)))));
            assert!(cover.roots.iter().any(|r| r.inputs.iter().any(|i| matches!(i, I::GeneratedId(_)))));
        }
    }
    #[test]
    fn token_source_helper_is_identical_to_native_court_split_at_boundaries() {
        use crate::palw_tir_step_v1::{PalwTirJobShapeV1, PalwTirTokenSourceV1 as T};
        let job = PalwTirJobShapeV1 { prefill: 3, positions: 6 };
        assert_eq!(job.token_source_v1(0), T::Prompt(0));
        assert_eq!(job.token_source_v1(2), T::Prompt(2));
        assert_eq!(job.token_source_v1(3), T::Generated(0));
        assert_eq!(job.token_source_v1(5), T::Generated(2));
    }
}

#[cfg(test)]
mod step_material_dependency_tests {
    use super::*;
    #[test]
    fn actual_numerical_roots_include_committed_and_history_step_boundaries() {
        use crate::palw_tir_court_v1::parameter_cover::RootInputBoundaryV1 as I;
        let f = crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
        let o = f.frame.original_v1();
        let source = PalwTirPublicSourceV1::new_native_v1(
            &f.record,
            &f.layout,
            f.frame.header_v1(),
            o.execution_root,
            o.trace_root,
            f.rules.maximum_leaves,
        )
        .unwrap();
        let budget = OriginalParameterCoverBudgetV1 {
            max_original_leaves: 1_000_000,
            max_total_edges: 10_000_000,
            max_total_coordinates: 1_000_000,
            max_root_memberships: 1_000_000,
        };
        let cover = PalwTirPublicSourceV1::root_input_cover_native_v1(&source, &f.record, &f.layout, budget).unwrap();
        assert_eq!(*cover.parameters_v1(), source.parameter_cover_v1(&f.record, &f.layout, budget).unwrap());
        assert!(cover.roots_v1().iter().flat_map(|r| &r.inputs).any(|i| matches!(i, I::CommittedStepLeaf(_))));
        assert!(cover.roots_v1().iter().flat_map(|r| &r.inputs).any(|i| matches!(i, I::HistoryStepLeaf(_))));
    }
}

#[cfg(test)]
mod fixed_root_material_tests {
    use super::*;
    #[test]
    fn actual_fixed_state_numerical_roots_require_checkpoint_and_prior_writer_material() {
        use crate::palw_tir_court_v1::{PalwTirEvidenceStoreV1, parameter_cover::RootInputBoundaryV1 as I};
        use crate::palw_tir_step_v1::PalwTirLeafKindV1 as K;
        use misaka_palw_tir::demand::{DemandContext, carry_in_node_v1, state_writer_v1};
        for committed in [false, true] {
            let f = crate::palw_tir_public_claim_v1::palw_tir_public_fixed_history_frame_fixture_v1(
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                committed,
            );
            let o = f.frame.original_v1();
            let source = PalwTirPublicSourceV1::new_native_v1(
                &f.record,
                &f.layout,
                f.frame.header_v1(),
                o.execution_root,
                o.trace_root,
                f.rules.maximum_leaves,
            )
            .unwrap();
            let budget = OriginalParameterCoverBudgetV1 {
                max_original_leaves: 1_000_000,
                max_total_edges: 10_000_000,
                max_total_coordinates: 1_000_000,
                max_root_memberships: 1_000_000,
            };
            let old = source.parameter_cover_v1(&f.record, &f.layout, budget).unwrap();
            let cover = PalwTirPublicSourceV1::root_input_cover_native_v1(&source, &f.record, &f.layout, budget).unwrap();
            assert_eq!(cover.parameters_v1(), &old, "None legacy parameter walk retains its exact relation/counters");
            let binding = &f.execution.binding;
            let v = crate::palw_tir_step_v1::verify_tir_binding_v1(binding, 1 << 40).unwrap();
            let state: u16 = v.space.program.states.iter().position(|s| s.name == "boundary_scalar").unwrap().try_into().unwrap();
            let (mut checkpoints, mut prior_writers) = (0, 0);
            for root in cover.roots_v1() {
                let leaf = v.space.leaf_at(&binding.job_context, root.leaf).unwrap();
                let K::Commit { occurrence, .. } = leaf.kind else { continue };
                let (_, layer) = v.space.occurrences()[usize::try_from(occurrence).unwrap()];
                for dep in &root.inputs {
                    let index = match *dep {
                        I::FixedCheckpointStepLeaf(i) | I::CommittedStepLeaf(i) | I::CarryStepLeaf(i) => i,
                        _ => continue,
                    };
                    let actual = v.space.leaf_at(&binding.job_context, index).unwrap();
                    if matches!(dep, I::FixedCheckpointStepLeaf(_)) {
                        if matches!(actual.kind,K::State{state:j,..} if j==state) {
                            checkpoints += 1;
                        }
                    } else if leaf.position > 0 && !leaf.position.is_multiple_of(f.layout.checkpoint_interval) {
                        if let Some((writer_occ, writer_node)) = state_writer_v1(&v.space.program, state, layer) {
                            if committed {
                                if matches!(actual.kind,K::Commit{occurrence:o,node:n,..} if o==u32::from(writer_occ)&&n==writer_node)
                                    && actual.position == leaf.position - 1
                                {
                                    prior_writers += 1;
                                }
                            } else {
                                let (prior, node) = carry_in_node_v1(
                                    &v.space.program,
                                    DemandContext { pos: leaf.position - 1, occurrence: writer_occ },
                                    0,
                                )
                                .unwrap();
                                if matches!(actual.kind,K::Commit{occurrence:o,node:n,..} if o==u32::from(prior.occurrence)&&n==node)
                                    && actual.position == prior.pos
                                    && matches!(dep, I::CarryStepLeaf(_))
                                {
                                    prior_writers += 1;
                                }
                            }
                        }
                    }
                    let q = super::super::PalwTirPublicRequestV1::StepLeaf { index };
                    let r = super::super::PalwTirPublicResponseV1::StepLeaf {
                        preimage: f.execution.step_leaf(index).unwrap(),
                        opening: f.execution.step_opening(index).unwrap(),
                    };
                    source.authenticate_v1(&q, &r, None).unwrap();
                }
            }
            assert!(checkpoints > 0, "actual numerical root must require new Fixed checkpoint body");
            assert!(prior_writers > 0, "actual noncheckpoint root must require its committed writer or replay carry body");
        }
    }
}
