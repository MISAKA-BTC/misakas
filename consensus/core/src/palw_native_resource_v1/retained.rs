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

/// RFC-0008: why a retained owner's mutation was refused — the typed-RAM reservation, the owner's own verdict on the change
/// (which may itself be a raw-material or metadata cap, or semantic), or the owner bound. Never flattened into one cause.
#[derive(Debug)]
pub enum NativeOwnerMutationErrorV1<E> {
    TypedRam(NativeCoverageV1),
    Rejected(E),
    Bound(String),
}
impl<E: fmt::Display> fmt::Display for NativeOwnerMutationErrorV1<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypedRam(e) => write!(f, "{e}"),
            Self::Rejected(e) => write!(f, "{e}"),
            Self::Bound(e) => write!(f, "{e}"),
        }
    }
}
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
        if let Some(lease) = &lease {
            lease.materialized_v1();
        }
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
        Ok(Self::from_owner_reserved_v1(value, lease))
    }
    /// Reserve a separately owned payload before a fallible construction. The
    /// source's complete typed request is an explicit conservative copy bound.
    fn try_project_v1<U>(&self, f: impl FnOnce(&T) -> U) -> Result<NativeRetainedV1<U>, NativeCoverageV1> {
        let lease = if enabled() || self._lease.is_some() {
            reserve_native_pending_requests_v1(tracked_value_requests_v1(&self.value)?)?
        } else {
            None
        };
        Ok(NativeRetainedV1::from_owner_reserved_v1(f(&self.value), lease))
    }
}
impl<T: Clone + NativeHeapV1> NativeRetainedV1<T> {
    /// Borrowed-source copy construction. Unlike retain_existing_v1, admission
    /// runs before Clone allocates. This authorizes this exact typed deep copy,
    /// not later mutations or a larger derived projection of the input.
    pub fn try_from_clone_v1(source: &T) -> Result<Self, NativeCoverageV1> {
        let requests = value_requests_v1(source)?;
        // Bare-source clones have legacy consumers that flatten errors into
        // semantic strings. Keep their default optional-capability semantics;
        // a reviewed retained source uses try_project_v1 instead.
        if enabled() {
            Self::clone_after_reservation_v1(source, requests, reserve_native_pending_requests_v1)
        } else {
            Self::clone_after_reservation_v1(source, requests, reserve_native_requests_v1)
        }
    }
    fn clone_after_reservation_v1(
        source: &T,
        requests: u64,
        reserve: impl FnOnce(u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1>,
    ) -> Result<Self, NativeCoverageV1> {
        let lease = reserve(requests)?;
        Ok(Self::from_owner_reserved_v1(source.clone(), lease))
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
        self.try_mutate_owner_classified_v1(growth, change).map_err(|e| e.to_string())
    }

    /// RFC-0008: [`Self::try_mutate_owner_v1`] with the refusing cause kept apart — the typed-RAM reservation
    /// (`NativeOwnerMutationErrorV1::TypedRam`), the owner's own refusal of the change (`Rejected`: a material store's raw or
    /// metadata cap, or a semantic verdict), and an owner-bound violation. The string form is byte-identical to the historical one.
    pub(crate) fn try_mutate_owner_classified_v1<E>(
        &self,
        growth: NativeCoverageV1,
        change: impl FnOnce(&mut T) -> Result<(), E>,
    ) -> Result<Self, NativeOwnerMutationErrorV1<E>> {
        use NativeOwnerMutationErrorV1 as M;
        let requests = if enabled() {
            let base = value_requests_v1(&self.value).map_err(M::TypedRam)?;
            let extra = growth.bytes().ok_or_else(|| M::TypedRam(growth.clone()))?;
            base.checked_add(extra).ok_or_else(|| M::Bound("native owner mutation request overflow".into()))?
        } else {
            0
        };
        let lease = reserve_native_pending_requests_v1(requests).map_err(M::TypedRam)?;
        let mut value = self.value.clone();
        change(&mut value).map_err(M::Rejected)?;
        // Diagnostic postcondition catches an incorrect owner-bound derivation;
        // it never retroactively authorizes construction beyond that bound.
        if enabled() && value_requests_v1(&value).map_err(M::TypedRam)? > requests {
            return Err(M::Bound("owner mutation exceeded its audited pre-construction request bound".into()));
        }
        Ok(Self::from_owner_reserved_v1(value, lease))
    }

    pub fn try_clone_v1(&self) -> Result<Self, NativeCoverageV1> {
        self.try_project_v1(Clone::clone)
    }
    /// Prevent Deref method resolution from selecting an unreserved deep Clone.
    #[allow(clippy::should_implement_trait)] // deliberately shadows Clone::clone with a fallible one
    pub fn clone(&self) -> Result<Self, NativeCoverageV1> {
        self.try_clone_v1()
    }
}
impl NativeRetainedV1<crate::palw_state_v2::PalwStateCarriageV2> {
    /// Consume a reserved typed carriage and keep its existing ownership until
    /// restoration and the canonical row-inventory check both finish. `check`
    /// may reuse the restoration root workspace, not allocate an unbounded job.
    pub fn try_restore_checked_v1(
        self,
        point: crate::BlockHash,
        params: &crate::palw_state_v2::PalwStateParamsV2,
        expected_root: Option<crate::Hash64>,
        uncertified_weightless: bool,
        canonical_work_daa: Option<u64>,
        check: impl FnOnce(&crate::palw_state_v2::PalwChainStateV2) -> Result<(), crate::palw_state_v2::PalwStateV2Error>,
    ) -> Result<super::NativeChainStateV1, NativeStateMutationErrorV1> {
        let Self { value, _lease } = self;
        let restored =
            super::restore::restore_checked_v1(point, value, params, expected_root, uncertified_weightless, canonical_work_daa, check);
        drop(_lease);
        restored
    }
}

fn enabled() -> bool {
    native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1()
}
/// Compatibility quote for optional-capability writers. An ordinary raw
/// transition has no preconstruction ticket; changing its zero quote would
/// make its final postcondition demand bytes no writer ever reserved.
pub(crate) fn value_requests_v1<T: NativeHeapV1>(value: &T) -> Result<u64, NativeCoverageV1> {
    if !enabled() {
        return Ok(0);
    }
    typed_value_requests_v1(value)
}
/// Always compute the audited typed request, including when the process-wide
/// checker is dormant. Explicit local checkers use this same sizing path.
pub(crate) fn typed_value_requests_v1<T: NativeHeapV1>(value: &T) -> Result<u64, NativeCoverageV1> {
    tracked_value_requests_v1(value)
}
/// Actual typed quote only for an explicitly tracked owner/construction.
/// This must not replace value_requests_v1 in legacy raw writer paths.
pub(crate) fn tracked_value_requests_v1<T: NativeHeapV1>(value: &T) -> Result<u64, NativeCoverageV1> {
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
    Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
}

/// Classified cold carriage form used by native storage.
pub fn retain_native_carriage_decode_classified_v1<T, E: fmt::Display>(
    point: crate::BlockHash,
    canonical: &[u8],
    decode: impl FnOnce() -> Result<T, E>,
) -> Result<NativeRetainedV1<T>, NativeDecodeErrorV1<E>> {
    // Preserve this existing generic API's contract. A concrete storage boundary
    // can map malformed wire into its own typed error with the API below.
    retain_native_carriage_decode_with_preflight_error_v1(
        point,
        canonical,
        |why| NativeDecodeErrorV1::Resource(NativeCoverageV1::unsupported::<T>("actual cold native carriage shape", &why.to_string())),
        decode,
    )
}

/// One actual preflight/reservation and one decode. The concrete caller owns
/// its codec-versus-resource error mapping; no blanket From<io::Error> bound
/// changes the existing generic callers or detaches the allocation ticket.
pub fn retain_native_carriage_decode_with_preflight_error_v1<T, E: fmt::Display>(
    point: crate::BlockHash,
    canonical: &[u8],
    preflight_error: impl FnOnce(io::Error) -> NativeDecodeErrorV1<E>,
    decode: impl FnOnce() -> Result<T, E>,
) -> Result<NativeRetainedV1<T>, NativeDecodeErrorV1<E>> {
    retain_carriage_decode_with_mode_v1(point, canonical, enabled(), preflight_error, decode)
}
fn retain_carriage_decode_with_mode_v1<T, E: fmt::Display>(
    point: crate::BlockHash,
    canonical: &[u8],
    admission: bool,
    preflight_error: impl FnOnce(io::Error) -> NativeDecodeErrorV1<E>,
    decode: impl FnOnce() -> Result<T, E>,
) -> Result<NativeRetainedV1<T>, NativeDecodeErrorV1<E>> {
    let lease = if admission {
        let preflight = preflight_native_carriage_v1(canonical, canonical.len() as u64).map_err(preflight_error)?;
        reserve_native_wire_v1(point, &preflight).map_err(NativeDecodeErrorV1::Resource)?
    } else {
        match preflight_native_carriage_v1(canonical, canonical.len() as u64) {
            Ok(preflight) => reserve_native_wire_v1(point, &preflight).map_err(NativeDecodeErrorV1::Resource)?,
            Err(_) => None,
        }
    };
    let value = decode().map_err(NativeDecodeErrorV1::Decode)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cold_carriage_preflight_keeps_truncation_before_decode() {
        let carriage = crate::palw_state_v2::PalwStateCarriageV2::from_state(&crate::palw_state_v2::PalwChainStateV2::genesis());
        let mut bytes = borsh::to_vec(&carriage).unwrap();
        bytes.truncate(1); // inside mandatory u16 version; optional-tail EOF is valid legacy carriage
        let called = std::cell::Cell::new(false);
        let result = retain_carriage_decode_with_mode_v1(
            crate::BlockHash::from_u64_word(37),
            &bytes,
            true,
            NativeDecodeErrorV1::Decode,
            || {
                called.set(true);
                Ok::<_, io::Error>(())
            },
        );
        assert!(!called.get(), "malformed framing is refused before typed allocation");
        assert!(
            matches!(result, Err(NativeDecodeErrorV1::Decode(ref why)) if matches!(why.kind(), io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof))
        );
    }
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
    fn default_recognized_carriage_and_its_projection_keep_process_tickets_without_strict_capability() {
        if enabled() {
            return;
        } // other explicit-capability test binaries cover the strict lane
        let state = crate::palw_state_v2::PalwChainStateV2::genesis();
        let carriage = crate::palw_state_v2::PalwStateCarriageV2::from_state(&state);
        let canonical = borsh::to_vec(&carriage).unwrap();
        let retained =
            retain_native_carriage_decode_classified_v1(crate::BlockHash::default(), &canonical, || Ok::<_, std::io::Error>(state))
                .unwrap();
        assert!(retained._lease.as_ref().unwrap().memory_v1() > 0);
        let projected = retained.try_carriage_v1().unwrap();
        assert!(projected._lease.as_ref().unwrap().memory_v1() > 0);
        assert_eq!(borsh::to_vec(&projected).unwrap(), canonical);
        // Unknown shapes preserve legacy decoder behaviour. The absence of a
        // ticket is explicit; this path is covered only by cold-reader limits.
        let legacy =
            retain_native_carriage_decode_classified_v1(crate::BlockHash::default(), &[0xff], || Ok::<_, std::io::Error>(7u8))
                .unwrap();
        assert!(legacy._lease.is_none());
        assert_eq!(*legacy, 7);
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
    /// Borrowed replay retains the original owner on every error. It reserves
    /// its new state, decoded row temporaries and index rebuild before cloning
    /// or applying anything. The old state/ticket remains live on every error.
    /// Reconstruct an original executed event prefix from the ACTUAL selected
    /// parent and stored accepting delta. The complete replay quote is reserved
    /// before cloning even when only a prefix is needed. No owned delta clone
    /// or caller-created replacement history is admitted by this method.
    pub fn try_delta_prefix_v2(
        &self,
        delta: &crate::palw_state_v2::PalwStateDeltaV2,
        end: usize,
        expected_root: crate::Hash64,
        params: &crate::palw_state_v2::PalwStateParamsV2,
    ) -> Result<Self, NativeStateMutationErrorV1> {
        use crate::palw_state_v2::PalwStateV2Error;
        if end > delta.entries.len() {
            return Err(NativeDecodeErrorV1::Decode(PalwStateV2Error::DeltaMismatch("recorded event prefix exceeds actual delta")));
        }
        let requests =
            if enabled() { native_delta_replay_requests_v1(&self.value, delta).map_err(NativeDecodeErrorV1::Resource)? } else { 0 };
        let lease = reserve_native_requests_v1(requests).map_err(NativeDecodeErrorV1::Resource)?;
        let value =
            crate::palw_state_v2::apply_delta_prefix_v2(&self.value, delta, end, params).map_err(NativeDecodeErrorV1::Decode)?;
        if value.state_root() != expected_root {
            return Err(NativeDecodeErrorV1::Decode(PalwStateV2Error::DeltaMismatch(
                "recorded event prefix root differs from actual history",
            )));
        }
        Ok(Self { value, _lease: lease })
    }

    pub fn try_delta_v1(
        &self,
        delta: &crate::palw_state_v2::PalwStateDeltaV2,
        params: &crate::palw_state_v2::PalwStateParamsV2,
        revert: bool,
    ) -> Result<Self, NativeStateMutationErrorV1> {
        let requests = if enabled() {
            native_delta_replay_requests_v1(&self.value, delta).map_err(NativeDecodeErrorV1::Resource)?
        } else if self._lease.is_some() {
            native_delta_replay_requests_v1(&self.value, delta).unwrap_or(0)
        } else {
            0
        };
        let lease = reserve_native_pending_requests_v1(requests).map_err(NativeDecodeErrorV1::Resource)?;
        let value = if revert {
            crate::palw_state_v2::revert_delta_v2(&self.value, delta, params)
        } else {
            crate::palw_state_v2::apply_delta_v2(&self.value, delta, params)
        }
        .map_err(NativeDecodeErrorV1::Decode)?;
        Ok(Self::from_owner_reserved_v1(value, lease))
    }

    /// Replay consumes a private owner: on failure no partially changed value escapes. Keep the
    /// same conservative quote and source/destination ticket overlap as borrowed replay.
    pub fn into_delta_v1(
        self,
        delta: &crate::palw_state_v2::PalwStateDeltaV2,
        params: &crate::palw_state_v2::PalwStateParamsV2,
        revert: bool,
    ) -> Result<Self, NativeStateMutationErrorV1> {
        let requests = if enabled() {
            native_delta_replay_requests_v1(&self.value, delta).map_err(NativeDecodeErrorV1::Resource)?
        } else if self._lease.is_some() {
            native_delta_replay_requests_v1(&self.value, delta).unwrap_or(0)
        } else {
            0
        };
        self.into_delta_after_reservation_v1(delta, params, revert, requests, reserve_native_pending_requests_v1)
    }

    fn into_delta_after_reservation_v1(
        self,
        delta: &crate::palw_state_v2::PalwStateDeltaV2,
        params: &crate::palw_state_v2::PalwStateParamsV2,
        revert: bool,
        requests: u64,
        reserve: impl FnOnce(u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1>,
    ) -> Result<Self, NativeStateMutationErrorV1> {
        // On a replay error the consuming core call drops its private value before unwinding
        // either reservation here. The source's ticket therefore covers every byte until drop.
        let lease = reserve(requests).map_err(NativeDecodeErrorV1::Resource)?;
        let Self { value, _lease: source_lease } = self;
        let value = crate::palw_state_v2::replay_delta_owned_v1(value, delta, params, revert).map_err(NativeDecodeErrorV1::Decode)?;
        drop(source_lease);
        Ok(Self::from_owner_reserved_v1(value, lease))
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
                    if *table == crate::palw_monetary_material_v1::PALW_MONETARY_MATERIAL_TABLE_V1 {
                        shape = shape.merge(s::material_member_replay_shape_v8(bytes).map_err(bad)?).map_err(bad)?;
                    }
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
    let bound = bound
        .add::<s::PalwChainStateV2>(NativeCoverageV1::Bounded { bytes: s::context_gc_replay_requests_v3(state, delta)? }, component);
    let bound = bound.add::<s::PalwChainStateV2>(
        NativeCoverageV1::Bounded { bytes: s::execution_cohort_replay_requests_v15(state, delta)? },
        component,
    );
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
        reserve_native_pending_requests_v1(shape.logical_requests_v1())?
    } else {
        None
    };
    let value = decode().map_err(|e| NativeCoverageV1::unsupported::<T>("cold native typed value decode", &e.to_string()))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
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
        reserve_native_pending_requests_v1(shape.logical_requests_v1()).map_err(NativeDecodeErrorV1::Resource)?
    } else {
        match preflight_native_value_v1::<T>(canonical, canonical.len() as u64) {
            Ok(shape) => reserve_native_pending_requests_v1(shape.logical_requests_v1()).map_err(NativeDecodeErrorV1::Resource)?,
            Err(_) => None,
        }
    };
    let value = decode().map_err(NativeDecodeErrorV1::Decode)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
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

// The native replay quote explicitly requires the pinned 64-bit layout.
#[cfg(all(test, target_pointer_width = "64"))]
mod owned_replay_tests {
    use super::*;
    use crate::palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwDeltaEntryV2, PalwStateDeltaV2, PalwStateParamsV2};

    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, crate::BlockHash::from_u64_word(1), 4, 1000, 100, 1000, 0).unwrap()
    }
    fn delta() -> PalwStateDeltaV2 {
        let point = PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(1), daa_score: 1, blue_score: 1, subsidy: 0 };
        PalwStateDeltaV2 { point, entries: vec![PalwDeltaEntryV2::LastPoint { old: None, new: Some(point) }] }
    }
    fn source_request(state: &PalwChainStateV2) -> u64 {
        let heap = state.native_heap_v1("test source").bytes().unwrap();
        // Same retained-owner envelope as value_requests_v1, independent of global admission.
        let request = heap + (size_of::<NativeChainStateV1>() + size_of::<[usize; 3]>()) as u64;
        assert!(request > 0);
        request
    }
    fn source(checker: &std::sync::Arc<NativeDemandCheckerV1>, request: u64) -> NativeChainStateV1 {
        let lease = checker.reserve_requests_v1(request, 0).unwrap();
        NativeRetainedV1 { value: PalwChainStateV2::genesis(), _lease: Some(lease) }
    }

    #[test]
    fn owned_delta_replay_capacity_admitted_before_change_and_tickets_release() {
        let p = params();
        let d = delta();
        let state = PalwChainStateV2::genesis();
        let source_request = source_request(&state);
        let replay_request = native_delta_replay_requests_v1(&state, &d).unwrap();
        let limit = source_request.checked_add(replay_request).unwrap();
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: limit, database_bytes: 0 });
        let owned = source(&checker, source_request);
        let output = owned
            .into_delta_after_reservation_v1(&d, &p, false, replay_request, |request| {
                assert_eq!(request, replay_request, "the original conservative replay quote");
                assert_eq!(checker.outstanding(), (source_request, 0), "source ticket retained before destination admission");
                let lease = checker.reserve_requests_v1(request, 0)?;
                assert_eq!(checker.outstanding(), (limit, 0));
                Ok(Some(lease))
            })
            .unwrap();
        assert_eq!(output.last_point(), Some(&d.point));
        assert_eq!(checker.outstanding(), (replay_request, 0), "source ticket replaced only after success");
        drop(output);
        assert_eq!(checker.outstanding(), (0, 0));

        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: limit - 1, database_bytes: 0 });
        let owned = source(&checker, source_request);
        let refused = owned.into_delta_after_reservation_v1(&d, &p, false, replay_request, |n| {
            assert_eq!(checker.outstanding(), (source_request, 0));
            checker.reserve_requests_v1(n, 0).map(Some)
        });
        assert!(matches!(refused, Err(NativeDecodeErrorV1::Resource(_))));
        assert_eq!(checker.outstanding(), (0, 0), "consumed source released on pre-mutation refusal");
    }

    #[test]
    fn owned_delta_replay_failed_private_mutation_releases_both_tickets_and_shared_owner_survives() {
        let p = params();
        let mut d = delta();
        d.entries.push(PalwDeltaEntryV2::Weights { old: (1, 1), new: (0, 0) });
        let state = PalwChainStateV2::genesis();
        let source_request = source_request(&state);
        let replay_request = native_delta_replay_requests_v1(&state, &d).unwrap();
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 {
            memory_bytes: source_request * 2 + replay_request,
            database_bytes: 0,
        });
        let shared = std::sync::Arc::new(source(&checker, source_request));
        let reader = std::sync::Arc::clone(&shared);
        let original_root = shared.state_root();
        let owned = source(&checker, source_request);
        let old_error = crate::palw_state_v2::apply_delta_v2(&shared, &d, &p).unwrap_err();
        let error = owned
            .into_delta_after_reservation_v1(&d, &p, false, replay_request, |n| checker.reserve_requests_v1(n, 0).map(Some))
            .unwrap_err();
        assert!(matches!(error, NativeDecodeErrorV1::Decode(e) if e == old_error));
        assert_eq!(checker.outstanding(), (source_request, 0), "failed private owner and destination ticket were released");
        assert_eq!(reader.last_point(), None);
        assert_eq!(reader.state_root(), original_root);
        drop(shared);
        assert_eq!(checker.outstanding(), (source_request, 0), "shared reader retains original ticket");
        drop(reader);
        assert_eq!(checker.outstanding(), (0, 0));
    }
}
