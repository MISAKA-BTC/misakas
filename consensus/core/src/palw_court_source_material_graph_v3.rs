//! SourceMaterialOnly v3 candidate. Native seven-family query/material closure,
//! immutable allocation edges and runtime guards. Not complete-original, paid
//! admission, a Ready authorization, a material receipt or a default capability.
//! No v2 codec, lifecycle tag, rooted row or production installer is changed.
use crate::{
    Hash64,
    palw_court_service_v1::{ServicePacket, ServiceReservations, causal::CausalServicePacketV1},
    palw_court_tir_requests_v1::TirSharedRequestSetV1,
    palw_tir_public_claim_v1::PalwTirPublicClaimV1,
    palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Request, PalwTirPublicSourceV1},
};
use borsh::BorshSerialize;
use std::collections::{BTreeMap, BTreeSet};
const PART: u64 = crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64;
#[derive(Clone, Copy, Debug)]
pub struct SourceGraphBudgetV3 {
    pub max_queries: u64,
    pub max_roles: u64,
    pub max_vertices: u64,
    pub max_edges: u64,
}
/// Checked metadata bound for local graph expansion only. This is neither a
/// native heap lease nor a cost/profile/admission capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceGraphExtentV3 {
    vertices: u64,
    edges: u64,
}
fn graph_extent_from_transport_v3(
    t: crate::palw_court_tir_requests_v1::TirSharedDataTransportUpperV1,
    full_ids: bool,
) -> Result<SourceGraphExtentV3, String> {
    let overflow = || "source graph metadata extent overflow".to_string();
    let stream_controls = t.streams.checked_mul(2).and_then(|n| n.checked_add(t.parts)).ok_or_else(overflow)?;
    let responses = t.queries.checked_mul(t.maximum_custodians).ok_or_else(overflow)?;
    if t.fresh_responses != responses {
        return Err("source graph metadata response mismatch".into());
    }
    let vertices = t.queries.checked_add(responses).and_then(|n| n.checked_add(stream_controls)).ok_or_else(overflow)?;
    if vertices != t.signed_controls {
        return Err("source graph metadata control mismatch".into());
    }
    // Every unit-stream Begin has its query parent. FullIds Begin has none.
    // Each response depends on ALL R finishes and its own query, not just the
    // response signer's Finish: the exact R-squared OR-frontier edge bound.
    let no_query_begins = if full_ids { t.maximum_custodians } else { 0 };
    let stream_edges = stream_controls.checked_sub(no_query_begins).ok_or_else(overflow)?;
    let edges = responses
        .checked_mul(t.maximum_custodians.checked_add(1).ok_or_else(overflow)?)
        .and_then(|n| n.checked_add(stream_edges))
        .ok_or_else(overflow)?;
    Ok(SourceGraphExtentV3 { vertices, edges })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceGraphCoverageV3 {
    SourceMaterialOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize)]
pub enum SourceMaterialTargetV3 {
    Unit(u64),
    FullIds,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum SourceMaterialNodeKindV3 {
    Request { ordinal: u64, query: Request },
    Begin { target: SourceMaterialTargetV3, role: u64, maximum_body: u64 },
    Part { target: SourceMaterialTargetV3, role: u64, offset: u64, maximum_bytes: u64 },
    Finish { target: SourceMaterialTargetV3, role: u64 },
    Response { ordinal: u64, role: u64 },
}
/// Allocation precedence is intentionally separate from these runtime facts.
/// Native ingress must check Ready/current authority and actual authenticated
/// query/data; a scheduled predecessor is never a completed prerequisite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceRuntimeGuardV3 {
    EligibleFiler,
    ReadyAndQuery { role: u64, query: Option<u64> },
    DeclaredPart { target: SourceMaterialTargetV3, role: u64, offset: u64 },
    ActualCompleteCursor { target: SourceMaterialTargetV3, role: u64 },
    QueryReadyAndAnyPublicMaterial { ordinal: u64, role: u64, target: SourceMaterialTargetV3 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMaterialNodeV3 {
    id: Hash64,
    kind: SourceMaterialNodeKindV3,
    allocation_predecessors: Vec<Hash64>,
    guard: SourceRuntimeGuardV3,
}
impl SourceMaterialNodeV3 {
    pub fn id_v3(&self) -> Hash64 {
        self.id
    }
    pub fn kind_v3(&self) -> &SourceMaterialNodeKindV3 {
        &self.kind
    }
    pub fn allocation_predecessors_v3(&self) -> &[Hash64] {
        &self.allocation_predecessors
    }
    pub fn runtime_guard_v3(&self) -> &SourceRuntimeGuardV3 {
        &self.guard
    }
}
/// No deserializer, public fields or arbitrary graph constructor. Derivation
/// always visits the native complete request universe, never a caller subset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSourceMaterialGraphV3 {
    scope: Hash64,
    absolute_scope: Hash64,
    source: Hash64,
    claim: Hash64,
    accepted_daa: u64,
    hard_daa: u64,
    queries: u64,
    roles: u64,
    nodes: BTreeMap<Hash64, SourceMaterialNodeV3>,
    edges: u64,
}
fn hash<T: BorshSerialize>(scope: Hash64, kind: &T) -> Result<Hash64, String> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/source-material-graph/v3", &(scope, kind))
}
fn source_graph_relative_scope_v3(
    frame: &PalwTirPublicClaimV1,
    source: &PalwTirPublicSourceV1,
    profile: Hash64,
    queries: u64,
    roles: u64,
) -> Result<Hash64, String> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(
        b"misaka-palw/source-material-scope/v3",
        &(
            3u16,
            frame.plan_v1().original_claim,
            source.source_id_v1(),
            frame.plan_v1().resource_plan_id,
            frame.plan_v1().rules_id,
            profile,
            queries,
            roles,
            PART,
        ),
    )
}

/// Structural material-graph coverage for the EXACT Constraint tail only.
/// This is not admission of a constraint form at a native activation point.
fn native_constraint_graph_tail_requests_v3(
    frame: &PalwTirPublicClaimV1,
) -> Result<u64, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
    use crate::palw_freeprompt_v3::PalwFpJobTailV1;
    use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1 as E, TirNativeRequestsV1, preflight_native_value_v1};
    let job = &frame.original_v1().job;
    let bytes = match &job.tail {
        None => return Ok(0),
        Some(PalwFpJobTailV1::Prefix(_)) if job.version == crate::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION => {
            // Fixed Copy tail; existing normative shape check is allocation-free.
            // Prefix StepSpace/template ownership is the separate family owner.
            crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job).map_err(|e| E::Decode(e.to_string()))?;
            return Ok(0);
        }
        Some(PalwFpJobTailV1::Constraint(tail)) if job.is_constraint() => &tail.constraint,
        _ => {
            return Err(E::Resource(NativeCoverageV1::unsupported::<NativeSourceMaterialGraphV3>(
                "native source graph frame parser",
                "this carried job-tail parser inventory is unavailable",
            )));
        }
    };
    let max = match bytes.get(..2).map(|x| u16::from_le_bytes([x[0], x[1]])) {
        Some(crate::palw_decode_constraint_v1::PALW_DECODE_CONSTRAINT_VERSION_V1) => {
            crate::palw_decode_constraint_v1::PALW_CONSTRAINT_MAX_BYTES_V1
        }
        Some(crate::palw_fp_constraint_v2::PALW_DECODE_CONSTRAINT_VERSION_V2) => {
            crate::palw_fp_constraint_v2::PALW_CONSTRAINT_MAX_BYTES_V2
        }
        _ => {
            return Err(E::Decode("source graph Constraint form is unknown/truncated".into()));
        }
    };
    if bytes.len() > max {
        return Err(E::Decode("source graph Constraint exceeds its existing form byte limit".into()));
    }
    let parsed =
        preflight_native_value_v1::<crate::palw_decode_constraint_v1::PalwDecodeConstraintV1>(bytes.as_slice(), bytes.len() as u64)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::OutOfMemory | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Unsupported => {
                    E::Resource(NativeCoverageV1::unsupported::<NativeSourceMaterialGraphV3>(
                        "native source graph Constraint scanner",
                        &e.to_string(),
                    ))
                }
                _ => E::Decode(e.to_string()),
            })?;
    let mut q = TirNativeRequestsV1::new();
    q.add_v1(parsed.logical_requests_v1()).map_err(E::Resource)?;
    // Parsed frames/nodes/edges coexist with ONE canonical serialization from
    // validate_form/from_bytes_form. The next writer replaces the previous;
    // *2 covers Borsh growth from the existing 1024-byte starting buffer.
    let writer = (bytes.len() as u64)
        .max(1024)
        .checked_mul(2)
        .ok_or_else(|| E::Resource(NativeCoverageV1::overflow::<NativeSourceMaterialGraphV3>("Constraint canonical writer")))?;
    q.exact_v1::<u8>(writer).map_err(E::Resource)?;
    Ok(q.bytes_v1())
}
fn native_check_constraint_graph_tail_v3(
    frame: &PalwTirPublicClaimV1,
) -> Result<(), crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
    use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1 as E};
    let job = &frame.original_v1().job;
    if let Some(bytes) = crate::palw_fp_constraint_job_v1::palw_fp_constraint_tail_v1(job) {
        if job.decode.is_some() {
            return Err(E::Decode("Constraint job has decode configuration".into()));
        }
        // Same typed decoder + existing normative validate_form/canonical check
        // as from_bytes_form. IO resource errors keep their cause, rather than
        // crossing the legacy enum's string-only Malformed boundary.
        let parsed: crate::palw_decode_constraint_v1::PalwDecodeConstraintV1 =
            borsh::from_slice(bytes).map_err(|e| match e.kind() {
                std::io::ErrorKind::OutOfMemory | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Unsupported => E::Resource(
                    NativeCoverageV1::unsupported::<NativeSourceMaterialGraphV3>("native Constraint decode", &e.to_string()),
                ),
                _ => E::Decode(e.to_string()),
            })?;
        let (version, nodes, max) = match parsed.version {
            crate::palw_decode_constraint_v1::PALW_DECODE_CONSTRAINT_VERSION_V1 => (
                crate::palw_decode_constraint_v1::PALW_DECODE_CONSTRAINT_VERSION_V1,
                crate::palw_decode_constraint_v1::PALW_CONSTRAINT_MAX_NODES_V1,
                crate::palw_decode_constraint_v1::PALW_CONSTRAINT_MAX_BYTES_V1,
            ),
            crate::palw_fp_constraint_v2::PALW_DECODE_CONSTRAINT_VERSION_V2 => (
                crate::palw_fp_constraint_v2::PALW_DECODE_CONSTRAINT_VERSION_V2,
                crate::palw_fp_constraint_v2::PALW_CONSTRAINT_MAX_NODES_V2,
                crate::palw_fp_constraint_v2::PALW_CONSTRAINT_MAX_BYTES_V2,
            ),
            _ => return Err(E::Decode("unknown Constraint form".into())),
        };
        parsed.validate_form(version, nodes, max).map_err(|e| E::Decode(e.to_string()))?;
        if parsed.to_bytes() != *bytes {
            return Err(E::Decode("noncanonical Constraint form".into()));
        }
    }
    Ok(())
}

/// Frame validation has its own temporary plan clone and Borsh/FP hash
/// writers. No Source/StepSpace reconstruction is performed by this owner.
fn native_source_graph_prelude_v3(
    frame: &PalwTirPublicClaimV1,
    source: &PalwTirPublicSourceV1,
    profile: Hash64,
) -> Result<u64, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
    use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1 as E, NativeHeapV1, TirNativeRequestsV1};
    let mut q = TirNativeRequestsV1::new();
    q.add_v1(native_constraint_graph_tail_requests_v3(frame)?).map_err(E::Resource)?;
    // Full frame heap is a conservative bound on the actual plan-only clone
    // in accepted_id_v1, plus frame subobject clones during structural checks.
    let clone = frame.native_heap_v1("source graph frame validation clone");
    q.add_v1(clone.bytes().ok_or(E::Resource(clone))?).map_err(E::Resource)?;
    let f = borsh::object_length(frame).map_err(|e| E::Decode(e.to_string()))? as u64;
    // Exact field widths of set.scope_id. Values are borrowed actual scope
    // facts; no invented source/profile is authenticated by a size exemplar.
    let p = frame.plan_v1();
    let scope = borsh::object_length(&(1u16, p.original_claim, p.allocation_id, p.rules_id, profile, source.source_id_v1(), 0u64))
        .map_err(|e| E::Decode(e.to_string()))? as u64;
    let writer = f
        .max(scope)
        .max(1024)
        .checked_mul(2)
        .ok_or_else(|| E::Resource(NativeCoverageV1::overflow::<NativeSourceMaterialGraphV3>("source graph Borsh hash writer")))?;
    // Claim/job/header/pledge/receipt/plan writers execute sequentially; each
    // serializes a frame subobject. scope_id is the other fixed tuple above.
    // node/relative/absolute hashes stream directly and allocate no Vec writer.
    q.exact_v1::<u8>(writer).map_err(E::Resource)?;
    Ok(q.bytes_v1())
}
fn native_source_graph_output_v3(
    extent: SourceGraphExtentV3,
    queries: u64,
    roles: u64,
) -> Result<u64, crate::palw_native_resource_v1::NativeCoverageV1> {
    use crate::palw_native_resource_v1::{NativeCoverageV1, NativeRetainedV1, TirNativeRequestsV1, btree_node_envelope_bytes_v1};
    if std::mem::size_of::<usize>() != 8 {
        return Err(NativeCoverageV1::unsupported::<NativeSourceMaterialGraphV3>(
            "source graph BTree inventory",
            "64-bit native BTree profile required",
        ));
    }
    let mut q = TirNativeRequestsV1::new();
    let overflow = || NativeCoverageV1::overflow::<NativeSourceMaterialGraphV3>("source graph native counts");
    let mut tree = |entries: u64, width: usize| -> Result<(), NativeCoverageV1> {
        q.add_v1(entries.checked_mul(width as u64).ok_or_else(overflow)?)
    };
    tree(extent.vertices, btree_node_envelope_bytes_v1::<Hash64, SourceMaterialNodeV3>())?;
    tree(queries, btree_node_envelope_bytes_v1::<u64, Hash64>())?;
    tree(extent.vertices, btree_node_envelope_bytes_v1::<Hash64, usize>())?; // check indegree
    tree(extent.vertices, btree_node_envelope_bytes_v1::<Hash64, Vec<Hash64>>())?; // check children
    // Variable parent/children Vecs each have independent minimum capacity.
    // This counts the aggregate E elements AND a one-item growth envelope per
    // possible nonempty Vec, not E as one monolithic flat array.
    let mut minimum = TirNativeRequestsV1::new();
    minimum.vec_v1::<Hash64>(1)?;
    let minimum_bytes = minimum.bytes_v1();
    for _ in 0..2 {
        q.vec_v1::<Hash64>(extent.edges)?;
        q.add_v1(extent.vertices.checked_mul(minimum_bytes).ok_or_else(overflow)?)?;
    }
    q.vec_v1::<u64>(queries)?; // FullIds ordinal list, conservative <= Q
    q.vec_v1::<Hash64>(roles)?; // one live streams() finishes list
    q.vec_v1::<Hash64>(extent.vertices)?; // check ready list, may grow by push
    q.exact_v1::<NativeRetainedV1<NativeSourceMaterialGraphV3>>(1)?;
    Ok(q.bytes_v1())
}

/// Origin classification is attached at the local limit check, never parsed from a string.
#[derive(Debug)]
pub(crate) enum SourceGraphDeriveErrorV3 {
    Semantic(String),
    Local(&'static str),
}
impl From<String> for SourceGraphDeriveErrorV3 {
    fn from(s: String) -> Self {
        Self::Semantic(s)
    }
}
impl From<&str> for SourceGraphDeriveErrorV3 {
    fn from(s: &str) -> Self {
        Self::Semantic(s.into())
    }
}
impl SourceGraphDeriveErrorV3 {
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
                E::Resource(NativeCoverageV1::unsupported::<NativeSourceMaterialGraphV3>("local source graph exploration", s))
            }
        }
    }
}
impl NativeSourceMaterialGraphV3 {
    /// Explicit allocation owner only. Retains construction/index scratch
    /// conservatively through graph lifetime; no paid/native profile or complete
    /// original is certified. Old budgets, graph IDs and old derive remain.
    pub fn derive_native_v3(
        frame: &PalwTirPublicClaimV1,
        source: &crate::palw_native_resource_v1::NativeRetainedV1<PalwTirPublicSourceV1>,
        profile: Hash64,
        budget: SourceGraphBudgetV3,
    ) -> Result<crate::palw_native_resource_v1::NativeRetainedV1<Self>, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>>
    {
        Self::derive_native_reserved_v3(frame, source, profile, budget, crate::palw_native_resource_v1::reserve_native_requests_v1)
    }
    fn derive_native_reserved_v3<R>(
        frame: &PalwTirPublicClaimV1,
        source: &crate::palw_native_resource_v1::NativeRetainedV1<PalwTirPublicSourceV1>,
        profile: Hash64,
        budget: SourceGraphBudgetV3,
        reserve: R,
    ) -> Result<crate::palw_native_resource_v1::NativeRetainedV1<Self>, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>>
    where
        R: FnOnce(
            u64,
        ) -> Result<
            Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
            crate::palw_native_resource_v1::NativeCoverageV1,
        >,
    {
        use crate::palw_native_resource_v1::{NativeDecodeErrorV1 as E, NativeRetainedV1};
        let prelude = native_source_graph_prelude_v3(frame, source, profile)?;
        let mut lease = reserve(prelude).map_err(E::Resource)?;
        native_check_constraint_graph_tail_v3(frame)?;
        frame.check_structure_v1(&frame.key_v1()).map_err(E::Decode)?;
        let set = TirSharedRequestSetV1::new(frame, source, profile).map_err(E::Decode)?;
        let roles = frame.custody_v1().pledge_v1().maximum_custodians;
        if roles == 0 {
            return Err(E::Decode("source graph query/role budget exhausted".into()));
        }
        if set.len() > budget.max_queries || roles > budget.max_roles {
            return Err(SourceGraphDeriveErrorV3::Local("source graph query/role budget exhausted").native());
        }
        let upper = set.signed_data_transport_upper().map_err(E::Decode)?;
        let extent = graph_extent_from_transport_v3(upper, set.has_full_ids_stream_v3()).map_err(E::Decode)?;
        if extent.vertices > budget.max_vertices || extent.edges > budget.max_edges {
            return Err(
                SourceGraphDeriveErrorV3::Local("source graph metadata vertex/edge budget exhausted before expansion").native()
            );
        }
        let output = native_source_graph_output_v3(extent, set.len(), roles).map_err(E::Resource)?;
        if let Some(ticket) = lease.as_mut() {
            ticket.grow_memory_v1(output).map_err(E::Resource)?;
        }
        // All shape/query maps and predecessor/check indices are now covered.
        // Stream/control hashes use fixed request descriptors and streaming sink.
        let graph = Self::derive_typed_v3(frame, source, profile, budget).map_err(SourceGraphDeriveErrorV3::native)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(graph, lease))
    }

    /// `frame` and `source` must be resolved from the native original by the
    /// caller. Structure/geometry are rechecked here; custody signatures and
    /// role-slot authority are not certified by a capacity-bound role count.
    /// Native construction/shape/hash/index costs need preflight separately.
    pub fn derive_v3(
        frame: &PalwTirPublicClaimV1,
        source: &PalwTirPublicSourceV1,
        profile: Hash64,
        budget: SourceGraphBudgetV3,
    ) -> Result<Self, String> {
        Self::derive_typed_v3(frame, source, profile, budget).map_err(SourceGraphDeriveErrorV3::legacy)
    }
    fn derive_typed_v3(
        frame: &PalwTirPublicClaimV1,
        source: &PalwTirPublicSourceV1,
        profile: Hash64,
        budget: SourceGraphBudgetV3,
    ) -> Result<Self, SourceGraphDeriveErrorV3> {
        frame.check_structure_v1(&frame.key_v1())?;
        let set = TirSharedRequestSetV1::new(frame, source, profile)?;
        let roles = frame.custody_v1().pledge_v1().maximum_custodians;
        if roles == 0 {
            return Err("source graph query/role budget exhausted".into());
        }
        if set.len() > budget.max_queries || roles > budget.max_roles {
            return Err(SourceGraphDeriveErrorV3::Local("source graph query/role budget exhausted"));
        }
        // Independent native upper covers every distinct signed DATA control.
        // It cheaply rejects huge geometry before enumerating/allocating nodes.
        let upper = set.signed_data_transport_upper()?;
        let extent = graph_extent_from_transport_v3(upper, set.has_full_ids_stream_v3())?;
        // Reject BEFORE allocating nodes/query maps/role predecessor Vecs.
        // Existing caller local budgets remain unchanged; not native RAM leases.
        if extent.vertices > budget.max_vertices || extent.edges > budget.max_edges {
            return Err(SourceGraphDeriveErrorV3::Local("source graph metadata vertex/edge budget exhausted before expansion"));
        }
        let relative_scope = source_graph_relative_scope_v3(frame, source, profile, set.len(), roles)?;
        let accepted_daa = frame.plan_v1().terms.accepted_daa;
        let hard_daa =
            accepted_daa.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA).ok_or("source graph original hard overflow")?;
        let mut out = Self {
            scope: relative_scope,
            absolute_scope: set.scope_id()?,
            source: source.source_id_v1(),
            claim: frame.plan_v1().original_claim,
            accepted_daa,
            hard_daa,
            queries: set.len(),
            roles,
            nodes: BTreeMap::new(),
            edges: 0,
        };
        let mut query_ids = BTreeMap::new();
        let mut full_ids_queries = Vec::new();
        for ordinal in 0..set.len() {
            let request = set.request_at(ordinal)?;
            let id = out.insert(
                SourceMaterialNodeKindV3::Request { ordinal, query: request },
                vec![],
                SourceRuntimeGuardV3::EligibleFiler,
                budget,
            )?;
            query_ids.insert(ordinal, id);
            if matches!(request, Request::Ids { .. }) {
                full_ids_queries.push(ordinal);
                continue;
            }
            let (body, _) = crate::palw_tir_public_owned_v1::request_body_bound_v1(source, request)?;
            let finishes = out.streams(SourceMaterialTargetV3::Unit(ordinal), body, Some(id), Some(ordinal), budget)?;
            out.responses(ordinal, id, SourceMaterialTargetV3::Unit(ordinal), &finishes, budget)?;
        }
        if !full_ids_queries.is_empty() {
            let (body, _) = crate::palw_tir_public_owned_v1::request_body_bound_v1(source, Request::Ids { offset: 0 })?;
            // FullIds Begin has no native query prerequisite; each Ids response
            // still requires its own query plus the one shared sequence.
            let finishes = out.streams(SourceMaterialTargetV3::FullIds, body, None, None, budget)?;
            for ordinal in full_ids_queries {
                out.responses(ordinal, query_ids[&ordinal], SourceMaterialTargetV3::FullIds, &finishes, budget)?;
            }
        }
        if out.nodes.len() as u64 > upper.signed_controls {
            return Err("source graph exceeds native transport upper".into());
        }
        if out.nodes.len() as u64 > extent.vertices || out.edges > extent.edges {
            return Err("source graph exceeded checked metadata extent".into());
        }
        out.check_v3()?;
        Ok(out)
    }
    fn insert(
        &mut self,
        kind: SourceMaterialNodeKindV3,
        mut parents: Vec<Hash64>,
        guard: SourceRuntimeGuardV3,
        b: SourceGraphBudgetV3,
    ) -> Result<Hash64, SourceGraphDeriveErrorV3> {
        if self.nodes.len() as u64 >= b.max_vertices {
            return Err(SourceGraphDeriveErrorV3::Local("source graph vertex budget exhausted"));
        }
        parents.sort_unstable();
        if parents.windows(2).any(|w| w[0] == w[1]) {
            return Err("duplicate source allocation parent".into());
        }
        let edges =
            self.edges.checked_add(parents.len() as u64).ok_or(SourceGraphDeriveErrorV3::Local("source graph edge overflow"))?;
        if edges > b.max_edges {
            return Err(SourceGraphDeriveErrorV3::Local("source graph edge budget exhausted"));
        }
        if parents.iter().any(|p| !self.nodes.contains_key(p)) {
            return Err("source graph missing allocation parent".into());
        }
        let id = hash(self.scope, &kind)?;
        if self.nodes.insert(id, SourceMaterialNodeV3 { id, kind, allocation_predecessors: parents, guard }).is_some() {
            return Err("source graph identity collision".into());
        }
        self.edges = edges;
        Ok(id)
    }
    fn streams(
        &mut self,
        target: SourceMaterialTargetV3,
        body: u64,
        query_id: Option<Hash64>,
        query: Option<u64>,
        b: SourceGraphBudgetV3,
    ) -> Result<Vec<Hash64>, SourceGraphDeriveErrorV3> {
        if body == 0 {
            return Err("source graph empty native stream".into());
        }
        let mut finishes = Vec::new();
        for role in 0..self.roles {
            let mut last = self.insert(
                SourceMaterialNodeKindV3::Begin { target, role, maximum_body: body },
                query_id.into_iter().collect(),
                SourceRuntimeGuardV3::ReadyAndQuery { role, query },
                b,
            )?;
            let mut offset = 0;
            while offset < body {
                let bytes = (body - offset).min(PART);
                last = self.insert(
                    SourceMaterialNodeKindV3::Part { target, role, offset, maximum_bytes: bytes },
                    vec![last],
                    SourceRuntimeGuardV3::DeclaredPart { target, role, offset },
                    b,
                )?;
                offset = offset.checked_add(bytes).ok_or("source part cursor overflow")?;
            }
            finishes.push(self.insert(
                SourceMaterialNodeKindV3::Finish { target, role },
                vec![last],
                SourceRuntimeGuardV3::ActualCompleteCursor { target, role },
                b,
            )?);
        }
        Ok(finishes)
    }
    fn responses(
        &mut self,
        ordinal: u64,
        query: Hash64,
        target: SourceMaterialTargetV3,
        finishes: &[Hash64],
        b: SourceGraphBudgetV3,
    ) -> Result<(), SourceGraphDeriveErrorV3> {
        for role in 0..self.roles {
            if self.edges.checked_add(finishes.len() as u64).and_then(|n| n.checked_add(1)).is_none_or(|n| n > b.max_edges) {
                return Err(SourceGraphDeriveErrorV3::Local("source graph edge budget exhausted"));
            }
            let mut parents = finishes.to_vec();
            parents.push(query);
            self.insert(
                SourceMaterialNodeKindV3::Response { ordinal, role },
                parents,
                SourceRuntimeGuardV3::QueryReadyAndAnyPublicMaterial { ordinal, role, target },
                b,
            )?;
        }
        Ok(())
    }
    /// Rebind an immutable native-derived graph to the exact original frame and
    /// source without treating its namespace as an adopted service policy body.
    /// Caller admits frame identity hashing scratch separately.
    pub(crate) fn check_frame_binding_v3(
        &self,
        frame: &PalwTirPublicClaimV1,
        source: &PalwTirPublicSourceV1,
        profile: Hash64,
    ) -> Result<(), String> {
        let set = TirSharedRequestSetV1::new(frame, source, profile)?;
        let p = frame.plan_v1();
        let roles = frame.custody_v1().pledge_v1().maximum_custodians;
        let scope = source_graph_relative_scope_v3(frame, source, profile, set.len(), roles)?;
        let hard = p
            .terms
            .accepted_daa
            .checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
            .ok_or("source graph original hard overflow")?;
        if (self.scope, self.absolute_scope, self.source, self.claim, self.accepted_daa, self.hard_daa, self.queries, self.roles)
            != (scope, set.scope_id()?, source.source_id_v1(), p.original_claim, p.terms.accepted_daa, hard, set.len(), roles)
        {
            return Err("source graph belongs to another frame/source/profile/allocation".into());
        }
        Ok(())
    }
    pub fn coverage_v3(&self) -> SourceGraphCoverageV3 {
        SourceGraphCoverageV3::SourceMaterialOnly
    }
    /// Clock-free native grammar/source/resource/profile identity. Absolute
    /// accepted allocation scope is retained separately and never signed as this.
    pub fn scope_v3(&self) -> Hash64 {
        self.scope
    }
    pub fn absolute_scope_v3(&self) -> Hash64 {
        self.absolute_scope
    }
    pub fn source_v3(&self) -> Hash64 {
        self.source
    }
    pub fn query_count_v3(&self) -> u64 {
        self.queries
    }
    pub fn role_slots_v3(&self) -> u64 {
        self.roles
    }
    pub fn edge_count_v3(&self) -> u64 {
        self.edges
    }
    pub fn nodes_v3(&self) -> impl Iterator<Item = &SourceMaterialNodeV3> {
        self.nodes.values()
    }
    /// Read the SAME existing logical Finish identity generated by insert().
    /// Does not allocate a new index or issue a material-completion receipt.
    pub fn finish_node_v3(&self, target: SourceMaterialTargetV3, role: u64) -> Result<Option<&SourceMaterialNodeV3>, String> {
        if role >= self.roles || matches!(target,SourceMaterialTargetV3::Unit(n) if n>=self.queries) {
            return Err("source Finish target/role outside native graph".into());
        }
        let kind = SourceMaterialNodeKindV3::Finish { target, role };
        let id = hash(self.scope, &kind)?;
        match self.nodes.get(&id) {
            Some(node) if node.kind != kind => Err("source Finish identity/kind mismatch".into()),
            node => Ok(node),
        }
    }
    /// Absolute allocation identity cannot be borrowed by another accepting
    /// instance even when its source and relative grammar are identical.
    pub fn absolute_packet_id_v3(&self, logical_id: Hash64) -> [u8; 64] {
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/source-allocation-packet/v3",
            &(self.absolute_scope, logical_id),
        )
        .expect("fixed hash identities serialize into the infallible hash sink")
        .as_bytes()
    }
    pub fn packet_ids_v3(&self) -> BTreeSet<[u8; 64]> {
        self.nodes.keys().map(|h| self.absolute_packet_id_v3(*h)).collect()
    }
    pub fn check_v3(&self) -> Result<(), String> {
        let mut edges = 0u64;
        let mut indegree = BTreeMap::new();
        let mut children: BTreeMap<Hash64, Vec<Hash64>> = BTreeMap::new();
        for (id, n) in &self.nodes {
            if *id != n.id || hash(self.scope, &n.kind)? != *id || n.allocation_predecessors.windows(2).any(|w| w[0] >= w[1]) {
                return Err("source graph identity/order changed".into());
            }
            edges = edges.checked_add(n.allocation_predecessors.len() as u64).ok_or("source graph edge overflow")?;
            indegree.insert(*id, n.allocation_predecessors.len());
            for p in &n.allocation_predecessors {
                if !self.nodes.contains_key(p) {
                    return Err("source graph parent absent".into());
                }
                children.entry(*p).or_default().push(*id);
            }
        }
        let mut ready: Vec<_> = indegree.iter().filter_map(|(id, n)| (*n == 0).then_some(*id)).collect();
        let mut done = 0;
        while let Some(id) = ready.pop() {
            done += 1;
            for child in children.get(&id).into_iter().flatten() {
                let n = indegree.get_mut(child).ok_or("source child missing")?;
                *n = n.checked_sub(1).ok_or("source edge accounting")?;
                if *n == 0 {
                    ready.push(*child);
                }
            }
        }
        if done != self.nodes.len() || edges != self.edges || self.nodes.is_empty() {
            return Err("source graph lost closure or is cyclic".into());
        }
        Ok(())
    }
    /// Scheduling projection only. Packet costs/windows must be derived and
    /// authenticated by the future native cost coordinator. Graph/index/map/
    /// clone ownership and slot-search work still require native preflight. This binds their
    /// immutable spelling and full source DAG, not their economic sufficiency.
    pub fn bind_projection_v3(&self, packets: Vec<ServicePacket>) -> Result<SourceServiceProjectionV3<'_>, String> {
        self.check_v3()?;
        let mut by_id = BTreeMap::new();
        for p in packets {
            if p.release_daa <= self.accepted_daa || p.release_daa >= p.service_before || p.service_before > self.hard_daa {
                return Err("source projection outside original acceptance/hard interval".into());
            }
            if by_id.insert(p.id, p).is_some() {
                return Err("source projection duplicate packet".into());
            }
        }
        if by_id.keys().copied().collect::<BTreeSet<_>>() != self.packet_ids_v3() {
            return Err("source projection omitted/added packet".into());
        }
        let packets = self
            .nodes
            .values()
            .map(|n| CausalServicePacketV1 {
                packet: by_id.remove(&self.absolute_packet_id_v3(n.id)).expect("exact set checked"),
                predecessors: n.allocation_predecessors.iter().map(|h| self.absolute_packet_id_v3(*h)).collect(),
            })
            .collect();
        Ok(SourceServiceProjectionV3 { graph: self, packets })
    }
}
/// Native-owned graph borrow, no arbitrary edge override, wire decoder, paid
/// flag or original installer. Reload must rederive the same full native graph.
pub struct SourceServiceProjectionV3<'a> {
    graph: &'a NativeSourceMaterialGraphV3,
    packets: Vec<CausalServicePacketV1>,
}
impl SourceServiceProjectionV3<'_> {
    /// **G14 lane r6-d1-release: the projection's causal predecessors** (child -> strictly increasing predecessor ids; nodes with
    /// none omitted), as the court service row records them so the canonical recovery sweep keeps causal order.
    pub fn causal_edges_v3(&self) -> std::collections::BTreeMap<[u8; 64], Vec<[u8; 64]>> {
        self.packets
            .iter()
            .filter(|n| !n.predecessors.is_empty())
            .map(|n| {
                let mut preds = n.predecessors.clone();
                preds.sort_unstable();
                preds.dedup();
                (n.packet.id, preds)
            })
            .collect()
    }
    pub fn check_loaded_v3(&self, book: &ServiceReservations, claim_ids: &BTreeSet<[u8; 64]>) -> Result<(), String> {
        self.graph.check_v3()?;
        if *claim_ids != self.graph.packet_ids_v3() {
            return Err("source projection claim packet universe omitted/added".into());
        }
        book.check_causal_v1(&self.packets).map_err(|e| format!("{e:?}"))
    }
    pub fn reserve_v3(&self, book: &mut ServiceReservations, hard: u64) -> Result<(), String> {
        if hard != self.graph.hard_daa {
            return Err("source projection substitutes original hard deadline".into());
        }
        let mut candidate = book.clone();
        candidate.reserve_causal_v1(&self.packets, hard).map_err(|e| format!("{e:?}"))?;
        self.check_loaded_v3(&candidate, &self.graph.packet_ids_v3())?;
        *book = candidate;
        Ok(())
    }
    pub fn repair_v3(&self, book: &mut ServiceReservations, claim_ids: &BTreeSet<[u8; 64]>, hard: u64) -> Result<usize, String> {
        self.check_loaded_v3(book, claim_ids)?;
        if hard != self.graph.hard_daa {
            return Err("source repair substitutes original hard deadline".into());
        }
        let mut candidate = book.clone();
        let n = candidate.repair_missing_causal_v1(&self.packets, hard).map_err(|e| format!("{e:?}"))?;
        self.check_loaded_v3(&candidate, claim_ids)?;
        *book = candidate;
        Ok(n)
    }
}
/// Actual Begin total controls whether a maximum reservation part is used.
/// None is a legal unused tail slot, never an accepted material/response fact.
pub fn declared_part_length_v3(maximum: u64, actual: u64, offset: u64) -> Result<Option<u64>, String> {
    if actual == 0 || actual > maximum || offset >= maximum || !offset.is_multiple_of(PART) {
        return Err("source graph part declaration geometry".into());
    }
    Ok((offset < actual).then(|| (actual - offset).min(PART)))
}

#[cfg(test)]
mod tests {
    include!("palw_court_canonical_finish_lookup_tests.rs");
    include!("palw_court_constraint_graph_native_tests.rs");
    use super::*;
    use crate::palw_court_service_v1::ServiceCost;
    const B: SourceGraphBudgetV3 =
        SourceGraphBudgetV3 { max_queries: 100_000, max_roles: 8, max_vertices: 2_000_000, max_edges: 8_000_000 };
    fn fixture() -> crate::palw_tir_public_claim_v1::PalwTirPublicFrameFixtureV1 {
        crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 2)
    }
    fn source(f: &crate::palw_tir_public_claim_v1::PalwTirPublicFrameFixtureV1) -> PalwTirPublicSourceV1 {
        f.frame.source_identity_v1(&f.record, &f.layout, f.rules.maximum_leaves).unwrap()
    }
    fn graph(f: &crate::palw_tir_public_claim_v1::PalwTirPublicFrameFixtureV1) -> NativeSourceMaterialGraphV3 {
        NativeSourceMaterialGraphV3::derive_v3(&f.frame, &source(f), Hash64::from_u64_word(71), B).unwrap()
    }
    #[test]
    fn native_source_graph_owner_keeps_exact_nodes_and_releases_growth_on_refusal() {
        use crate::palw_native_resource_v1::{NativeDecodeErrorV1 as E, NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1};
        let profile = Hash64::from_u64_word(71);
        for f in [fixture(), crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_two_tile_v1()] {
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
            let old = NativeSourceMaterialGraphV3::derive_v3(&f.frame, &source, profile, B).unwrap();
            let set = TirSharedRequestSetV1::new(&f.frame, &source, profile).unwrap();
            let extent =
                graph_extent_from_transport_v3(set.signed_data_transport_upper().unwrap(), set.has_full_ids_stream_v3()).unwrap();
            let prelude = native_source_graph_prelude_v3(&f.frame, &source, profile).unwrap();
            let output = native_source_graph_output_v3(extent, set.len(), old.roles).unwrap();
            let total = prelude.checked_add(output).unwrap();
            for quota in [prelude - 1, total - 1, total] {
                let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: quota, database_bytes: 0 });
                let result = NativeSourceMaterialGraphV3::derive_native_reserved_v3(&f.frame, &source, profile, B, |bytes| {
                    checker.reserve_requests_v1(bytes, 0).map(Some)
                });
                if quota == total {
                    let graph = result.unwrap();
                    assert_eq!(graph.nodes, old.nodes);
                    assert_eq!(graph.scope, old.scope);
                    assert_eq!(graph.absolute_scope, old.absolute_scope);
                    assert_eq!(graph.edges, old.edges);
                    assert_eq!(checker.outstanding(), (total, 0));
                    drop(graph);
                } else {
                    assert!(matches!(result, Err(E::Resource(_))));
                }
                assert_eq!(checker.outstanding(), (0, 0));
            }
            assert!(matches!(
                NativeSourceMaterialGraphV3::derive_native_v3(
                    &f.frame,
                    &source,
                    profile,
                    SourceGraphBudgetV3 { max_edges: extent.edges - 1, ..B }
                ),
                Err(E::Resource(_))
            ));
            for small in [
                SourceGraphBudgetV3 { max_queries: 0, ..B },
                SourceGraphBudgetV3 { max_roles: old.roles - 1, ..B },
                SourceGraphBudgetV3 { max_vertices: 0, ..B },
                SourceGraphBudgetV3 { max_edges: 0, ..B },
            ] {
                assert!(NativeSourceMaterialGraphV3::derive_v3(&f.frame, &source, profile, small).is_err());
                assert!(matches!(
                    NativeSourceMaterialGraphV3::derive_native_v3(&f.frame, &source, profile, small),
                    Err(E::Resource(_))
                ));
            }
        }
        assert!(native_source_graph_output_v3(SourceGraphExtentV3 { vertices: u64::MAX, edges: 1 }, 1, 1).is_err());
        assert!(native_source_graph_output_v3(SourceGraphExtentV3 { vertices: 1, edges: u64::MAX }, 1, 1).is_err());
    }
    #[test]
    fn grouped_graph_extent_rejects_local_budget_before_expansion() {
        for f in [fixture(), crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_two_tile_v1()] {
            let source = source(&f);
            let set = TirSharedRequestSetV1::new(&f.frame, &source, Hash64::from_u64_word(71)).unwrap();
            let extent =
                graph_extent_from_transport_v3(set.signed_data_transport_upper().unwrap(), set.has_full_ids_stream_v3()).unwrap();
            let exact_budget = SourceGraphBudgetV3 { max_vertices: extent.vertices, max_edges: extent.edges, ..B };
            let g = NativeSourceMaterialGraphV3::derive_v3(&f.frame, &source, Hash64::from_u64_word(71), exact_budget).unwrap();
            assert!(g.nodes.len() as u64 <= extent.vertices && g.edges <= extent.edges);
            assert!(extent.vertices > 0 && extent.edges > 0);
            for small in [
                SourceGraphBudgetV3 { max_vertices: extent.vertices - 1, ..exact_budget },
                SourceGraphBudgetV3 { max_edges: extent.edges - 1, ..exact_budget },
            ] {
                let error = NativeSourceMaterialGraphV3::derive_v3(&f.frame, &source, Hash64::from_u64_word(71), small).unwrap_err();
                assert_eq!(error, "source graph metadata vertex/edge budget exhausted before expansion");
            }
        }
    }
    #[test]
    fn grouped_graph_extent_counts_all_role_or_edges_and_checks_overflow() {
        use crate::palw_court_tir_requests_v1::TirSharedDataTransportUpperV1 as T;
        // Arithmetic-only unit fixture, not an authenticated original/profile.
        // Q=2, R=3, one shared FullIds stream per role: V=2+6+9.
        let t = T {
            queries: 2,
            maximum_custodians: 3,
            streams: 3,
            parts: 3,
            fresh_responses: 6,
            signed_controls: 17,
            ..Default::default()
        };
        assert_eq!(graph_extent_from_transport_v3(t, true).unwrap(), SourceGraphExtentV3 { vertices: 17, edges: 30 });
        assert_eq!(graph_extent_from_transport_v3(t, false).unwrap().edges, 33);
        assert!(graph_extent_from_transport_v3(T { maximum_custodians: u64::MAX, ..t }, true).is_err());
        assert!(graph_extent_from_transport_v3(T { streams: u64::MAX, ..t }, true).is_err());
        assert!(graph_extent_from_transport_v3(T { fresh_responses: 5, ..t }, true).is_err());
    }
    #[test]
    fn native_source_universe_and_all_role_streams_are_closed_without_owner_slots() {
        let f = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_two_tile_v1();
        let source = source(&f);
        let g = graph(&f);
        g.check_v3().unwrap();
        let set = TirSharedRequestSetV1::new(&f.frame, &source, Hash64::from_u64_word(71)).unwrap();
        let upper = set.signed_data_transport_upper().unwrap();
        assert_eq!(g.coverage_v3(), SourceGraphCoverageV3::SourceMaterialOnly);
        assert_eq!(g.query_count_v3(), set.len());
        assert_eq!(g.role_slots_v3(), f.frame.custody_v1().pledge_v1().maximum_custodians);
        assert!(g.nodes.len() as u64 <= upper.signed_controls);
        let queries = g
            .nodes_v3()
            .filter_map(|n| match n.kind_v3() {
                SourceMaterialNodeKindV3::Request { ordinal, query } => Some((*ordinal, *query)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        for ordinal in 0..set.len() {
            assert_eq!(queries[&ordinal], set.request_at(ordinal).unwrap());
        }
        let mut exact = set.len().checked_mul(1 + g.roles).unwrap();
        let mut has_ids = false;
        for ordinal in 0..set.len() {
            let q = set.request_at(ordinal).unwrap();
            if matches!(q, Request::Ids { .. }) {
                has_ids = true;
                continue;
            }
            let (body, _) = crate::palw_tir_public_owned_v1::request_body_bound_v1(&source, q).unwrap();
            exact += (2 + body.div_ceil(PART)) * g.roles;
        }
        if has_ids {
            let (body, _) = crate::palw_tir_public_owned_v1::request_body_bound_v1(&source, Request::Ids { offset: 0 }).unwrap();
            exact += (2 + body.div_ceil(PART)) * g.roles;
        }
        assert_eq!(
            g.nodes.len() as u64,
            exact,
            "every individual native bound is expanded exactly; family maxima are upper bounds only"
        );
        let families = queries
            .values()
            .map(|q| match q {
                Request::StepNode { .. } => 0,
                Request::StepLeaf { .. } => 1,
                Request::StepRun { .. } => 2,
                Request::RowsNode { .. } => 3,
                Request::TileNode { .. } => 4,
                Request::TraceTile { .. } => 5,
                Request::Ids { .. } => 6,
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(families.len(), 7, "native two-tile original exercises all seven source families");
        let mut family_bounds: BTreeMap<u8, BTreeSet<u64>> = BTreeMap::new();
        for q in queries.values() {
            let family = match q {
                Request::StepNode { .. } => 0,
                Request::StepLeaf { .. } => 1,
                Request::StepRun { .. } => 2,
                Request::RowsNode { .. } => 3,
                Request::TileNode { .. } => 4,
                Request::TraceTile { .. } => 5,
                Request::Ids { .. } => 6,
            };
            family_bounds.entry(family).or_default().insert(source.unit_bound_v1(q).unwrap().maximum_response_bytes);
        }
        assert!(family_bounds[&5].len() > 1, "full and ragged one-lane TraceTile bounds differ");
        assert!(family_bounds.values().filter(|v| v.len() > 1).count() >= 2, "multiple native families have varying per-query bounds");
        let ids = queries.values().filter(|q| matches!(q, Request::Ids { .. })).count();
        assert!(ids > 0);
        let begins = g
            .nodes_v3()
            .filter(|n| matches!(n.kind_v3(), SourceMaterialNodeKindV3::Begin { target: SourceMaterialTargetV3::FullIds, .. }))
            .collect::<Vec<_>>();
        assert_eq!(begins.len() as u64, g.roles, "one FullIds sequence per role, never one per query chunk");
        assert!(begins.iter().all(|n| n.allocation_predecessors_v3().is_empty()), "native FullIds Begin does not require a query");
        let response = g.nodes_v3().find(|n| matches!(n.kind_v3(), SourceMaterialNodeKindV3::Response { .. })).unwrap();
        assert_eq!(response.allocation_predecessors_v3().len() as u64, g.roles + 1);
        assert!(
            matches!(response.runtime_guard_v3(), SourceRuntimeGuardV3::QueryReadyAndAnyPublicMaterial { .. }),
            "all allocated signer finishes are not runtime AND duties"
        );
        for budget in [
            SourceGraphBudgetV3 { max_queries: 0, ..B },
            SourceGraphBudgetV3 { max_roles: g.roles - 1, ..B },
            SourceGraphBudgetV3 { max_vertices: 0, ..B },
            SourceGraphBudgetV3 { max_edges: 0, ..B },
        ] {
            assert!(NativeSourceMaterialGraphV3::derive_v3(&f.frame, &source, Hash64::from_u64_word(71), budget).is_err());
        }
    }
    #[test]
    fn short_body_tail_slots_are_unused_and_used_parts_match_the_native_cursor_rule() {
        for maximum in [1, PART, PART + 1, 2 * PART + 3] {
            for actual in [1, maximum] {
                let mut accepted = 0;
                for offset in (0..maximum).step_by(PART as usize) {
                    match declared_part_length_v3(maximum, actual, offset).unwrap() {
                        None => assert!(offset >= actual),
                        Some(n) => {
                            accepted = crate::palw_court_shared_tir_stream_v1::check_shared_stream_part_v1(
                                actual, accepted, offset, n as usize,
                            )
                            .unwrap();
                        }
                    }
                }
                assert_eq!(accepted, actual);
            }
        }
        assert!(declared_part_length_v3(PART + 1, 0, 0).is_err());
        assert!(declared_part_length_v3(PART + 1, PART + 2, 0).is_err());
        assert!(declared_part_length_v3(PART + 1, 1, 1).is_err());
    }
    // Unit costs below test scheduling/immutability only; they are not calibrated prices.
    fn packets(g: &NativeSourceMaterialGraphV3) -> Vec<ServicePacket> {
        g.nodes_v3()
            .map(|n| ServicePacket {
                id: g.absolute_packet_id_v3(n.id_v3()),
                release_daa: g.accepted_daa + 1,
                service_before: g.hard_daa,
                cost: ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 },
            })
            .collect()
    }
    fn book() -> ServiceReservations {
        ServiceReservations::new(ServiceCost { bytes: 1_000_000, work: 1_000_000, io: 1_000_000, controls: 1_000_000 }).unwrap()
    }
    #[test]
    fn projection_reload_and_repair_refuse_a_partial_packet_universe_atomically() {
        let f = fixture();
        let g = graph(&f);
        let packets = packets(&g);
        let projection = g.bind_projection_v3(packets.clone()).unwrap();
        let mut b = book();
        projection.reserve_v3(&mut b, g.hard_daa).unwrap();
        let bytes = borsh::to_vec(&b).unwrap();
        let loaded: ServiceReservations = borsh::from_slice(&bytes).unwrap();
        let all = g.packet_ids_v3();
        projection.check_loaded_v3(&loaded, &all).unwrap();
        let mut partial = all.clone();
        partial.pop_first();
        assert!(projection.check_loaded_v3(&loaded, &partial).is_err());
        let saved = b.clone();
        assert!(projection.repair_v3(&mut b, &partial, g.hard_daa).is_err());
        assert_eq!(b, saved);
        let mut missing = packets.clone();
        missing.pop();
        assert!(g.bind_projection_v3(missing).is_err());
        let mut changed = packets;
        changed[0].cost.bytes += 1;
        let other = g.bind_projection_v3(changed).unwrap();
        assert!(
            other.check_loaded_v3(&loaded, &all).is_err(),
            "retained packet cost spelling is immutable even though its pricing is separate"
        );
        b.observe_block(g.accepted_daa + 2, [9; 64], ServiceCost::default()).unwrap();
        let before = b.clone();
        // Native causal repair may reject when descendants retain earlier slots;
        // either result must preserve the complete graph and be atomic.
        match projection.repair_v3(&mut b, &all, g.hard_daa) {
            Ok(_) => projection.check_loaded_v3(&b, &all).unwrap(),
            Err(_) => assert_eq!(b, before),
        }
    }
    #[test]
    fn relative_identity_excludes_actual_acceptance_but_binds_source_and_profile() {
        use crate::palw_tir_public_allocation_v1::{PalwTirPublicAllocationQuoteV1, derive_tir_public_allocation_v1};
        use crate::palw_tir_public_custody_v1::{PalwTirPublicCustodyContractV1, PalwTirPublicCustodyPledgeV1};
        let mut f = fixture();
        let g = graph(&f);
        let old_scope = g.absolute_scope_v3();
        let original = f.frame.original_v1().clone();
        let header = f.frame.header_v1().clone();
        let geometry = crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {
            class_id: original.job.class_id,
            producer: f.frame.custody_v1().pledge_v1().producer,
            execution_root: original.execution_root,
            job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&original),
            accepted_daa: 117,
        };
        let PalwTirPublicAllocationQuoteV1::Complete(plan) =
            derive_tir_public_allocation_v1(&geometry, &original, &header, &f.record, &f.layout, None, None, f.rules).unwrap()
        else {
            panic!("native arithmetic quote")
        };
        let mut pledge = PalwTirPublicCustodyPledgeV1::for_plan_v1(&original, &plan, 118).unwrap();
        pledge.signature = vec![2; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
        let verify = |_: &[u8], _: &[u8], _: &[u8], _: &[u8]| true;
        let custody = PalwTirPublicCustodyContractV1::new_v1(
            pledge,
            plan.terms,
            BTreeSet::new(),
            plan.source_id,
            plan.resource_plan_id,
            f.rules.maximum_custodians,
            &vec![1; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN],
            verify,
        )
        .unwrap();
        f.frame = PalwTirPublicClaimV1::prepare_native_v1(
            original,
            header,
            *plan,
            custody,
            &geometry,
            &f.record,
            &f.layout,
            None,
            None,
            f.rules,
            &BTreeSet::new(),
            &f.object,
            verify,
        )
        .unwrap();
        let later = graph(&f);
        assert_ne!(later.absolute_scope_v3(), old_scope);
        assert_eq!(later.scope_v3(), g.scope_v3());
        assert_eq!(later.nodes_v3().map(|n| n.id_v3()).collect::<Vec<_>>(), g.nodes_v3().map(|n| n.id_v3()).collect::<Vec<_>>());
        assert_ne!(later.packet_ids_v3(), g.packet_ids_v3());
        let old_projection = g.bind_projection_v3(packets(&g)).unwrap();
        let mut old_book = book();
        old_projection.reserve_v3(&mut old_book, g.hard_daa).unwrap();
        let new_projection = later.bind_projection_v3(packets(&later)).unwrap();
        assert!(
            new_projection.check_loaded_v3(&old_book, &later.packet_ids_v3()).is_err(),
            "same logical graph cannot borrow another acceptance's service book"
        );
        for n in 0..2 {
            let mut invalid = packets(&later);
            if n == 0 {
                invalid[0].release_daa = later.accepted_daa;
            } else {
                invalid[0].service_before = later.hard_daa + 1;
            }
            assert!(later.bind_projection_v3(invalid).is_err());
        }
        let other = NativeSourceMaterialGraphV3::derive_v3(&f.frame, &source(&f), Hash64::from_u64_word(72), B).unwrap();
        assert_ne!(other.scope_v3(), g.scope_v3());
        assert_ne!(other.packet_ids_v3(), g.packet_ids_v3());
        let changed = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(Some((0, 0)), 2);
        assert_ne!(graph(&changed).scope_v3(), g.scope_v3());
    }
}
