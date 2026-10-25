//! Native allocation ownership. A cache entry and every shared Arc retain the
//! same ticket; an independent deep payload obtains a separate ticket first.
use super::*;
use std::{fmt, io, mem::size_of, ops::Deref};

/// Capacity and state/codec refusal retain separate causes across storage and
/// native callers. A local resource limit cannot disqualify a valid block.
#[derive(Debug)]
pub enum NativeDecodeErrorV1<E> {
    Resource(NativeCoverageV1),
    Decode(E),
}
impl<E: fmt::Display> fmt::Display for NativeDecodeErrorV1<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource(e) => write!(f, "{e}"),
            Self::Decode(e) => write!(f, "{e}"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for NativeDecodeErrorV1<E> {}
pub type NativeStateMutationErrorV1 = NativeDecodeErrorV1<crate::palw_state_v2::PalwStateV2Error>;
pub struct NativeRetainedV1<T> {
    value: T,
    _lease: Option<NativeDemandLeaseV1>,
}
impl<T> Deref for NativeRetainedV1<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}
impl<T: fmt::Debug> fmt::Debug for NativeRetainedV1<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.value, f)
    }
}
impl<T: PartialEq> PartialEq for NativeRetainedV1<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.value == rhs.value
    }
}
impl<T: Eq> Eq for NativeRetainedV1<T> {}
impl<T: PartialEq> PartialEq<T> for NativeRetainedV1<T> {
    fn eq(&self, rhs: &T) -> bool {
        self.value == *rhs
    }
}
impl<T: borsh::BorshSerialize> borsh::BorshSerialize for NativeRetainedV1<T> {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        self.value.serialize(writer)
    }
}
impl<T: NativeHeapV1> NativeHeapV1 for NativeRetainedV1<T> {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        self.value.native_heap_v1(path)
    }
}
impl<T> NativeRetainedV1<T> {
    /// Only Core schema owners may hand over a value whose construction was
    /// already reserved before allocation. The ticket is retained by Self.
    pub(crate) fn from_owner_reserved_v1(value: T, lease: Option<NativeDemandLeaseV1>) -> Self {
        Self { value, _lease: lease }
    }

    /// Compatibility extraction is permitted only with all native capacity
    /// capabilities disabled. Strict callers retain this owner instead.
    pub fn into_unreserved_legacy_v1(self) -> Result<T, NativeCoverageV1> {
        if enabled() {
            return Err(NativeCoverageV1::unsupported::<T>(
                "native owner extraction",
                "retaining a bare value would detach its allocation ticket",
            ));
        }
        Ok(self.value)
    }
    /// An explicit owner handoff. The receiving owner must retain the returned
    /// ticket for the value's full lifetime; ordinary callers should keep Self.
    pub fn into_parts_v1(self) -> (T, Option<NativeDemandLeaseV1>) {
        (self.value, self._lease)
    }
}
impl<T: NativeHeapV1> NativeRetainedV1<T> {
    /// Retain a previously existing, actually typed value. This does not cover
    /// its earlier construction; use retain_native_carriage_decode_v1 for disk.
    pub fn retain_existing_v1(value: T) -> Result<Self, NativeCoverageV1> {
        let lease = reserve_native_requests_v1(value_requests_v1(&value)?)?;
        Ok(Self { value, _lease: lease })
    }
    /// Reserve a separately owned payload before a fallible construction. The
    /// source's complete typed request is an explicit conservative copy bound.
    fn try_project_v1<U>(&self, f: impl FnOnce(&T) -> U) -> Result<NativeRetainedV1<U>, NativeCoverageV1> {
        let lease = reserve_native_requests_v1(value_requests_v1(&self.value)?)?;
        Ok(NativeRetainedV1 { value: f(&self.value), _lease: lease })
    }
}
impl<T: Clone + NativeHeapV1> NativeRetainedV1<T> {
    /// Borrowed-source copy construction. Unlike retain_existing_v1, admission
    /// runs before Clone allocates. This authorizes this exact typed deep copy,
    /// not later mutations or a larger derived projection of the input.
    pub fn try_from_clone_v1(source: &T) -> Result<Self, NativeCoverageV1> {
        let requests = value_requests_v1(source)?;
        Self::clone_after_reservation_v1(source, requests, reserve_native_requests_v1)
    }
    fn clone_after_reservation_v1(
        source: &T,
        requests: u64,
        reserve: impl FnOnce(u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1>,
    ) -> Result<Self, NativeCoverageV1> {
        let lease = reserve(requests)?;
        Ok(Self { value: source.clone(), _lease: lease })
    }
    /// Core-owned, reviewed mutation only. The specialized owner API derives
    /// `growth` from its actual typed inputs/codec scans and complete temporary
    /// layouts. A claimed raw byte capacity is not a construction bound. This
    /// primitive is deliberately unavailable to remote/SDK callers.
    pub(crate) fn try_mutate_owner_v1<E: fmt::Display>(
        &self,
        growth: NativeCoverageV1,
        change: impl FnOnce(&mut T) -> Result<(), E>,
    ) -> Result<Self, String> {
        let requests = if enabled() {
            let base = value_requests_v1(&self.value).map_err(|e| e.to_string())?;
            let extra = growth.bytes().ok_or_else(|| growth.to_string())?;
            base.checked_add(extra).ok_or("native owner mutation request overflow")?
        } else {
            0
        };
        let lease = reserve_native_requests_v1(requests).map_err(|e| e.to_string())?;
        let mut value = self.value.clone();
        change(&mut value).map_err(|e| e.to_string())?;
        // Diagnostic postcondition catches an incorrect owner-bound derivation;
        // it never retroactively authorizes construction beyond that bound.
        if enabled() && value_requests_v1(&value).map_err(|e| e.to_string())? > requests {
            return Err("owner mutation exceeded its audited pre-construction request bound".into());
        }
        Ok(Self { value, _lease: lease })
    }

    pub fn try_clone_v1(&self) -> Result<Self, NativeCoverageV1> {
        self.try_project_v1(Clone::clone)
    }
    /// Prevent Deref method resolution from selecting an unreserved deep Clone.
    pub fn clone(&self) -> Result<Self, NativeCoverageV1> {
        self.try_clone_v1()
    }
}
fn enabled() -> bool {
    native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1()
}
pub(crate) fn value_requests_v1<T: NativeHeapV1>(value: &T) -> Result<u64, NativeCoverageV1> {
    if !enabled() {
        return Ok(0);
    }
    let bound = value.native_heap_v1("native retained payload").add::<T>(
        NativeCoverageV1::Bounded { bytes: (size_of::<NativeRetainedV1<T>>() + size_of::<[usize; 3]>()) as u64 },
        "native retained Arc/owner",
    );
    bound.bytes().ok_or(bound)
}

/// Preflight the ACTUAL canonical disk bytes before native carriage allocation,
/// retain its restoration/copy requests through validation and cache lifetime.
/// This quota proof establishes no state/root authority: decode must still run
/// the historical into_state_v3 and committed-root checks over these same bytes.
pub fn retain_native_carriage_decode_v1<T, E: fmt::Display>(
    point: crate::BlockHash,
    canonical: &[u8],
    decode: impl FnOnce() -> Result<T, E>,
) -> Result<NativeRetainedV1<T>, NativeCoverageV1> {
    let lease = if enabled() {
        let preflight = preflight_native_carriage_v1(canonical, canonical.len() as u64)
            .map_err(|why| NativeCoverageV1::unsupported::<T>("actual cold native carriage shape", &why.to_string()))?;
        reserve_native_wire_v1(point, &preflight)?
    } else {
        None
    };
    let value =
        decode().map_err(|why| NativeCoverageV1::unsupported::<T>("cold native carriage decode/root validation", &why.to_string()))?;
    Ok(NativeRetainedV1 { value, _lease: lease })
}

/// Classified cold carriage form used by native storage.
pub fn retain_native_carriage_decode_classified_v1<T, E: fmt::Display>(
    point: crate::BlockHash,
    canonical: &[u8],
    decode: impl FnOnce() -> Result<T, E>,
) -> Result<NativeRetainedV1<T>, NativeDecodeErrorV1<E>> {
    let lease = if enabled() {
        let preflight = preflight_native_carriage_v1(canonical, canonical.len() as u64).map_err(|why| {
            NativeDecodeErrorV1::Resource(NativeCoverageV1::unsupported::<T>("actual cold native carriage shape", &why.to_string()))
        })?;
        reserve_native_wire_v1(point, &preflight).map_err(NativeDecodeErrorV1::Resource)?
    } else {
        None
    };
    let value = decode().map_err(NativeDecodeErrorV1::Decode)?;
    Ok(NativeRetainedV1 { value, _lease: lease })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_replacement_cannot_release_live_shared_payload_ticket() {
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 64, database_bytes: 0 });
        let first =
            std::sync::Arc::new(NativeRetainedV1 { value: vec![7u8; 8], _lease: Some(checker.reserve_requests_v1(32, 0).unwrap()) });
        let reader = std::sync::Arc::clone(&first);
        let mut cache = Some(first);
        let replacement =
            std::sync::Arc::new(NativeRetainedV1 { value: vec![9u8; 8], _lease: Some(checker.reserve_requests_v1(32, 0).unwrap()) });
        cache.replace(replacement);
        assert_eq!(checker.outstanding(), (64, 0));
        assert_eq!(reader.as_slice(), &[7; 8]);
        drop(reader);
        assert_eq!(checker.outstanding(), (32, 0));
        drop(cache);
        assert_eq!(checker.outstanding(), (0, 0));
    }
    #[test]
    fn borrowed_source_clone_is_never_called_before_capacity_admission() {
        #[derive(Debug)]
        struct CountedClone(std::sync::Arc<std::sync::atomic::AtomicUsize>);
        impl Clone for CountedClone {
            fn clone(&self) -> Self {
                self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Self(std::sync::Arc::clone(&self.0))
            }
        }
        impl NativeHeapV1 for CountedClone {
            fn native_heap_v1(&self, _: &str) -> NativeCoverageV1 {
                NativeCoverageV1::Bounded { bytes: 64 }
            }
        }
        let copies = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let input = CountedClone(std::sync::Arc::clone(&copies));
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 64, database_bytes: 0 });
        let failed = NativeRetainedV1::clone_after_reservation_v1(&input, 65, |n| checker.reserve_requests_v1(n, 0).map(Some));
        assert!(failed.is_err());
        assert_eq!(copies.load(std::sync::atomic::Ordering::Relaxed), 0);
        assert_eq!(checker.outstanding(), (0, 0));
        let copy = NativeRetainedV1::clone_after_reservation_v1(&input, 64, |n| checker.reserve_requests_v1(n, 0).map(Some)).unwrap();
        assert_eq!(copies.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(checker.outstanding(), (64, 0));
        drop(copy);
        assert_eq!(checker.outstanding(), (0, 0));
    }
    #[test]
    fn retained_encoding_is_exact_value_encoding() {
        let value = NativeRetainedV1 { value: vec![1u8, 2, 3], _lease: None };
        assert_eq!(borsh::to_vec(&value).unwrap(), borsh::to_vec(&vec![1u8, 2, 3]).unwrap());
    }
}

impl NativeRetainedV1<crate::palw_state_v2::PalwChainStateV2> {
    /// Carriage contains the state's primary tables and drops derived indexes;
    /// one full typed state copy bounds this specific projection's allocation.
    pub fn try_carriage_v1(&self) -> Result<NativeRetainedV1<crate::palw_state_v2::PalwStateCarriageV2>, NativeCoverageV1> {
        self.try_project_v1(crate::palw_state_v2::PalwStateCarriageV2::from_state)
    }
}

impl NativeRetainedV1<crate::palw_state_v2::PalwChainStateV2> {
    /// The ONLY mutable-state operation exposed by this owner. Replay reserves
    /// its new state, decoded row temporaries and index rebuild before cloning
    /// or applying anything. The old state/ticket remains live on every error.
    pub fn try_delta_v1(
        &self,
        delta: &crate::palw_state_v2::PalwStateDeltaV2,
        params: &crate::palw_state_v2::PalwStateParamsV2,
        revert: bool,
    ) -> Result<Self, NativeStateMutationErrorV1> {
        let requests =
            if enabled() { native_delta_replay_requests_v1(&self.value, delta).map_err(NativeDecodeErrorV1::Resource)? } else { 0 };
        let lease = reserve_native_requests_v1(requests).map_err(NativeDecodeErrorV1::Resource)?;
        let value = if revert {
            crate::palw_state_v2::revert_delta_v2(&self.value, delta, params)
        } else {
            crate::palw_state_v2::apply_delta_v2(&self.value, delta, params)
        }
        .map_err(NativeDecodeErrorV1::Decode)?;
        Ok(Self { value, _lease: lease })
    }
}

pub fn native_delta_replay_requests_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    delta: &crate::palw_state_v2::PalwStateDeltaV2,
) -> Result<u64, NativeCoverageV1> {
    use crate::palw_state_v2::{self as s, PalwDeltaEntryV2 as E};
    if cfg!(not(target_pointer_width = "64")) {
        return Err(NativeCoverageV1::unsupported::<s::PalwStateDeltaV2>(
            "native delta replay",
            "pinned tree profile requires 64-bit",
        ));
    }
    let component = "native delta replay allocation requests";
    let bad = |why: io::Error| NativeCoverageV1::unsupported::<s::PalwStateDeltaV2>(component, &why.to_string());
    let mut decoded = NativeWireShapeV1::default();
    for entry in &delta.entries {
        let row = match entry {
            E::VertexRow { table, key, old, new } => s::scan_vertex_delta_row_v1(state, *table, key, old, new),
            E::ImprovementRow { table, key, old, new } => s::scan_improvement_delta_row_v1(state, *table, key, old, new),
            E::ExecV2Row { table, key, old, new } => s::scan_exec_delta_row_v1(state, *table, key, old, new),
            E::BondBudgetRow { table, key, old, new } => s::scan_budget_delta_row_v1(*table, key, old, new),
            E::KernelRouteRow { table, key, old, new } => {
                let mut shape = NativeWireShapeV1::default();
                for bytes in [old, new].into_iter().flatten() {
                    shape = shape.merge(preflight_kernel_served_row_v1(*table, key, bytes).map_err(bad)?).map_err(bad)?;
                }
                Ok(shape)
            }
            _ => Ok(NativeWireShapeV1::default()),
        }
        .map_err(bad)?;
        decoded = decoded.merge(row).map_err(bad)?;
    }
    // Deadline/consistency consumers also decode rooted aux and ledger rows;
    // raw byte residency is not authority for those native projections.
    let old_ledger = if let Some(route) = state.kernel_route() {
        inventory_ledger_expansion_v1(&route.rows, false)?
            .checked_add(super::ledger_expansion::inventory_ledger_aux_inputs_v1(route)?)
            .ok_or_else(|| NativeCoverageV1::overflow::<s::PalwChainStateV2>(component))?
    } else {
        0
    };
    if decoded.composite_components != 0 {
        let pool = old_ledger.checked_add(decoded.heap).ok_or_else(|| NativeCoverageV1::overflow::<s::PalwChainStateV2>(component))?;
        decoded = decoded
            .allocation(
                decoded.composite_components,
                usize::try_from(pool).map_err(|_| NativeCoverageV1::overflow::<s::PalwChainStateV2>(component))?,
            )
            .map_err(bad)?;
    }
    let members = delta
        .native_members_v1()
        .and_then(|n| n.checked_add(decoded.collection_entries))
        .ok_or_else(|| NativeCoverageV1::overflow::<s::PalwStateDeltaV2>(component))?;
    // Every rebuilt membership belongs to one of the concrete primary/index
    // layouts in transition_node_unit_v1. Count two generations: the cloned old
    // indexes and the newly rebuilt indexes can coexist until replacement.
    let existing = s::transition_existing_index_shape_v1(state).map_err(bad)?;
    let trees = members
        .checked_mul(s::transition_node_unit_v1())
        .and_then(|n| n.checked_add(existing.heap))
        .and_then(|n| n.checked_mul(2))
        .ok_or_else(|| NativeCoverageV1::overflow::<s::PalwChainStateV2>(component))?;
    let bound = state.native_heap_v1(component)
        // clone destination plus a previous/installed row temporary: a full
        // source payload is conservative for the latter one-row projection.
        .scale::<s::PalwChainStateV2>(2,component)
        .add::<s::PalwStateDeltaV2>(delta.native_heap_v1(component).scale::<s::PalwStateDeltaV2>(2,component),component)
        .add::<s::PalwStateDeltaV2>(NativeCoverageV1::Bounded {bytes:decoded.heap}.scale::<s::PalwStateDeltaV2>(2,component),component)
        .add::<s::PalwChainStateV2>(NativeCoverageV1::Bounded {bytes:trees},component)
        .add::<s::PalwChainStateV2>(NativeCoverageV1::Bounded {bytes:old_ledger},component)
        .add::<s::PalwChainStateV2>(NativeCoverageV1::Bounded {bytes:(size_of::<NativeRetainedV1<s::PalwChainStateV2>>() + size_of::<[usize;3]>()) as u64},component);
    bound.bytes().ok_or(bound)
}

/// Exact standalone Borsh preflight and ticket ownership for delta/history
/// payloads. This proof admits allocation only; caller validates point/root.
pub fn retain_native_value_decode_v1<T: NativeWireScanV1, E: fmt::Display>(
    canonical: &[u8],
    decode: impl FnOnce() -> Result<T, E>,
) -> Result<NativeRetainedV1<T>, NativeCoverageV1> {
    let lease = if enabled() {
        let shape = preflight_native_value_v1::<T>(canonical, canonical.len() as u64)
            .map_err(|e| NativeCoverageV1::unsupported::<T>("cold native typed value", &e.to_string()))?;
        reserve_native_requests_v1(shape.logical_requests_v1())?
    } else {
        None
    };
    let value = decode().map_err(|e| NativeCoverageV1::unsupported::<T>("cold native typed value decode", &e.to_string()))?;
    Ok(NativeRetainedV1 { value, _lease: lease })
}

/// Classified standalone cold value form used by native storage.
pub fn retain_native_value_decode_classified_v1<T: NativeWireScanV1, E: fmt::Display>(
    canonical: &[u8],
    decode: impl FnOnce() -> Result<T, E>,
) -> Result<NativeRetainedV1<T>, NativeDecodeErrorV1<E>> {
    let lease = if enabled() {
        let shape = preflight_native_value_v1::<T>(canonical, canonical.len() as u64).map_err(|e| {
            NativeDecodeErrorV1::Resource(NativeCoverageV1::unsupported::<T>("cold native typed value", &e.to_string()))
        })?;
        reserve_native_requests_v1(shape.logical_requests_v1()).map_err(NativeDecodeErrorV1::Resource)?
    } else {
        None
    };
    let value = decode().map_err(NativeDecodeErrorV1::Decode)?;
    Ok(NativeRetainedV1 { value, _lease: lease })
}

#[cfg(test)]
mod replay_tests {
    use super::*;
    use crate::palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwDeltaEntryV2, PalwStateDeltaV2};
    fn delta(entries: Vec<PalwDeltaEntryV2>) -> PalwStateDeltaV2 {
        PalwStateDeltaV2 {
            point: PalwBlockContextV2 { block: crate::BlockHash::default(), daa_score: 0, blue_score: 0, subsidy: 0 },
            entries,
        }
    }
    #[test]
    fn replay_quote_checks_actual_opaque_row_shapes_before_mutation() {
        let state = PalwChainStateV2::genesis();
        let root = state.state_root();
        let valid = delta(vec![]);
        assert!(native_delta_replay_requests_v1(&state, &valid).unwrap() > 0);
        let malformed = delta(vec![PalwDeltaEntryV2::VertexRow { table: 1, key: vec![], old: None, new: Some(vec![0xff; 4]) }]);
        assert!(native_delta_replay_requests_v1(&state, &malformed).is_err());
        assert_eq!(state.state_root(), root);
    }
    #[test]
    fn opaque_restore_membership_overflow_is_not_saturated() {
        assert!(crate::palw_state_v2::restore_index_native_shape_v1(&[("vesting", u64::MAX, 1)]).is_err());
    }
}
