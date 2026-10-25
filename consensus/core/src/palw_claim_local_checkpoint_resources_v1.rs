//! Borrowed RFC-0023 checkpoint construction inventory. Codec lengths fund byte
//! vectors only; maps, nested clones and growing arrays are counted by type.
use super::*;
use crate::palw_native_resource_v1::{BTreeNodeEnvelopeV1, NativeCoverageV1, NativeHeapV1, TirNativeRequestsV1};

fn overflow() -> NativeCoverageV1 {
    NativeCoverageV1::overflow::<PalwClaimLocalCheckpointV1>("checkpoint construction")
}
fn add(a: u64, b: u64) -> Result<u64, NativeCoverageV1> {
    a.checked_add(b).ok_or_else(overflow)
}
fn encoded<T: borsh::BorshSerialize>(value: &T) -> Result<u64, NativeCoverageV1> {
    borsh::object_length(value).map(|n| n as u64).map_err(|_| overflow())
}
fn bytes(q: &mut TirNativeRequestsV1, n: u64) -> Result<(), NativeCoverageV1> {
    // borsh::to_vec starts with 1024 bytes and grows geometrically. Include
    // both old and new allocations at a growth boundary, even if realloc moves.
    q.exact_v1::<u8>(n.max(1024).checked_mul(3).ok_or_else(overflow)?)
}

fn growing<T>(q: &mut TirNativeRequestsV1, n: u64) -> Result<(), NativeCoverageV1> {
    if n == 0 {
        return Ok(());
    }
    let width = std::mem::size_of::<T>();
    let minimum = if width == 1 {
        8
    } else if width <= 1024 {
        4
    } else {
        1
    };
    q.exact_v1::<T>(n.max(minimum).checked_mul(3).ok_or_else(overflow)?)
}

pub(super) fn requests(state: &PalwChainStateV2) -> Result<u64, NativeCoverageV1> {
    if cfg!(not(target_pointer_width = "64")) {
        return Err(NativeCoverageV1::unsupported::<PalwClaimLocalCheckpointV1>(
            "checkpoint construction trees",
            "pinned BTree layout requires 64-bit",
        ));
    }
    let mut q = TirNativeRequestsV1::new();
    let claims = state.claims.len() as u64;
    let courts = state.court_sessions.len() as u64;
    let frames = rewards::frames(state).count() as u64;
    q.exact_v1::<BTreeNodeEnvelopeV1<(u64, u64), Hash64>>(frames)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, Vec<(Hash64, PalwCourtSessionStateV2)>>>(courts)?;
    // Initial grouping uses growing Vecs (minimum 4 slots); subsequent per-claim
    // clones have exact length. Include old+new arrays at a growth boundary:
    // twelve slots per session bounds grouping, plus one exact clone slot.
    q.exact_v1::<(Hash64, PalwCourtSessionStateV2)>(courts.checked_mul(13).ok_or_else(overflow)?)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1, PalwClaimLocalRecordV1>>(claims)?;
    // Close groups are borrowed for serialization only (never cloned): one material entry,
    // one manifest entry and one encoding buffer each, at most two per session.
    let groups = state.court_close_groups.len() as u64;
    let rows = add(add(claims, courts)?, groups)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, Vec<u8>>>(rows)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(rows)?;
    for ((sid, side), group) in &state.court_close_groups {
        bytes(&mut q, encoded(&(sid, side, group))?)?;
    }
    growing::<(Hash64, PalwClaimStateV2, Vec<(Hash64, PalwCourtSessionStateV2)>)>(&mut q, claims)?;
    growing::<Hash64>(&mut q, claims)?;
    // Two Vec length prefixes. Count every claim/court as open, independently
    // of readiness; a failed/partial state cannot borrow a semantic invariant.
    let mut liability_bytes = 8u64;
    for claim in state.claims.values() {
        let heap = claim.native_heap_v1("checkpoint liability claim copy");
        q.add_v1(heap.bytes().ok_or(heap)?)?;
        let wire = encoded(claim)?;
        bytes(&mut q, wire)?;
        // id + claim + court Vec prefix + one birth frame id.
        liability_bytes = add(liability_bytes, add(wire, 64 + 4 + 64)?)?;
    }
    for session in state.court_sessions.values() {
        // The grouping and retained liability copy coexist. Serialization
        // borrows the session and does not create a third deep clone.
        let heap = session.native_heap_v1("checkpoint court copies").scale::<PalwCourtSessionStateV2>(2, "court copies");
        q.add_v1(heap.bytes().ok_or(heap)?)?;
        let wire = add(64, encoded(session)?)?;
        bytes(&mut q, wire)?;
        liability_bytes = add(liability_bytes, wire)?;
    }
    bytes(&mut q, liability_bytes)?;
    Ok(q.bytes_v1())
}
