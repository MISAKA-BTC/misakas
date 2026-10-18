//! Actual EXEC input references projected from an executed frame. These are
//! provenance edges, not certification or a complete-material checkpoint.
use super::{PalwClaimCheckpointErrorV1, PalwClaimLiabilityFrameV1};
use crate::Hash64;
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeHeapV1, NativeRetainedV1, TirNativeRequestsV1, native_enum, native_struct, reserve_native_requests_v1,
};
use crate::palw_state_v2::PalwBlockContextV2;
use crate::palw_work_slice_v2::PalwWorkPrefixStageV2;
use kaspa_hashes::ZERO_HASH64;

/// Namespace is part of the reference: a chain claim, a kernel claim and a
/// work session may deliberately use identical raw bytes. Birth/retirement
/// resolution still needs their original admission context; raw IDs alone
/// are not globally unique credit-lot identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwClaimLocalWorkRefV1 {
    ChainClaim { claim: Hash64 },
    KernelClaim { claim: Hash64 },
    ExecSession { root: Hash64 },
    ExecSlice { root: Hash64, index: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClaimLocalExecDependencyV1 {
    pub consumer: PalwClaimLocalWorkRefV1,
    pub input: PalwClaimLocalWorkRefV1,
}
/// A projection at one executed point, without a credit capability. Unbound
/// legacy sessions explicitly remain incomplete; their declared prefix and
/// evidence hashes do not become authenticated kernel inputs retrospectively.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClaimLocalExecOriginsV1 {
    pub point: PalwBlockContextV2,
    pub edges: Vec<PalwClaimLocalExecDependencyV1>,
    pub unbound_roots: Vec<Hash64>,
}

impl PalwClaimLiabilityFrameV1 {
    /// Derive from the admitted rows, never from a producer dependency list,
    /// model IDs, payee bonds or coincidentally equal input/output hashes.
    /// The caller must obtain this frame through authenticated executed state.
    /// This does not establish that the referenced material can be recovered.
    pub fn exec_dependency_origins_v1(&self) -> Result<NativeRetainedV1<PalwClaimLocalExecOriginsV1>, PalwClaimCheckpointErrorV1> {
        self.with_context_carriages_v1(|_, current| {
            use PalwClaimLocalWorkRefV1::*;
            let exec = &current.exec_v2;
            let overflow = || PalwClaimCheckpointErrorV1::Resource(NativeCoverageV1::overflow::<Self>("EXEC input origins"));
            let roots = exec.roots.len() as u64;
            let edges_bound =
                roots.checked_mul(2).and_then(|n| n.checked_add((exec.slices.len() as u64).checked_mul(3)?)).ok_or_else(overflow)?;
            let mut q = TirNativeRequestsV1::new();
            q.exact_v1::<PalwClaimLocalExecDependencyV1>(edges_bound).map_err(PalwClaimCheckpointErrorV1::Resource)?;
            q.exact_v1::<Hash64>(roots).map_err(PalwClaimCheckpointErrorV1::Resource)?;
            let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwClaimCheckpointErrorV1::Resource)?;
            let capacity = usize::try_from(edges_bound).map_err(|_| overflow())?;
            let mut edges = Vec::with_capacity(capacity);
            let mut unbound_roots = Vec::with_capacity(exec.roots.len());
            for (id, root) in &exec.roots {
                edges.push(PalwClaimLocalExecDependencyV1 { consumer: ExecSession { root: *id }, input: ChainClaim { claim: *id } });
                if matches!(root.prefix, PalwWorkPrefixStageV2::Unbound) {
                    unbound_roots.push(*id);
                } else {
                    if root.prefix_claim == ZERO_HASH64 {
                        return Err("bound EXEC prefix lost its kernel claim".into());
                    }
                    edges.push(PalwClaimLocalExecDependencyV1 {
                        consumer: ExecSession { root: *id },
                        input: KernelClaim { claim: root.prefix_claim },
                    });
                }
            }
            for ((id, index), slice) in &exec.slices {
                let root = exec.roots.get(id).ok_or(PalwClaimCheckpointErrorV1::Semantic("EXEC slice lost its session"))?;
                let consumer = ExecSlice { root: *id, index: *index };
                // The session's aggregate work depends on each admitted slice.
                // This never adds a backwards edge into the base chain claim.
                edges.push(PalwClaimLocalExecDependencyV1 { consumer: ExecSession { root: *id }, input: consumer });
                if !matches!(root.prefix, PalwWorkPrefixStageV2::Unbound) {
                    if slice.evidence_root == ZERO_HASH64 {
                        return Err("bound EXEC slice lost its kernel claim".into());
                    }
                    edges.push(PalwClaimLocalExecDependencyV1 { consumer, input: KernelClaim { claim: slice.evidence_root } });
                }
                if *index == 0 {
                    if slice.predecessor_state_root != root.initial_state_root {
                        return Err("EXEC first slice changed its admitted input boundary".into());
                    }
                    if !matches!(root.prefix, PalwWorkPrefixStageV2::Unbound) {
                        edges.push(PalwClaimLocalExecDependencyV1 { consumer, input: KernelClaim { claim: root.prefix_claim } });
                    }
                } else {
                    let previous = exec
                        .slices
                        .get(&(*id, index - 1))
                        .ok_or(PalwClaimCheckpointErrorV1::Semantic("EXEC slice lost its admitted predecessor"))?;
                    if slice.predecessor_state_root != previous.result_state_root {
                        return Err("EXEC slice changed its admitted input boundary".into());
                    }
                    edges.push(PalwClaimLocalExecDependencyV1 { consumer, input: ExecSlice { root: *id, index: index - 1 } });
                }
            }
            Ok(NativeRetainedV1::from_owner_reserved_v1(
                PalwClaimLocalExecOriginsV1 { point: self.point, edges, unbound_roots },
                lease,
            ))
        })
    }
}

native_enum!(PalwClaimLocalWorkRefV1; ChainClaim {claim}, KernelClaim {claim}, ExecSession {root}, ExecSlice {root,index});
native_struct!(PalwClaimLocalExecDependencyV1; consumer, input);
native_struct!(PalwClaimLocalExecOriginsV1; point, edges, unbound_roots);
