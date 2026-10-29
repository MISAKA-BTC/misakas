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
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, Vec<u8>>>(add(claims, courts)?)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(add(claims, courts)?)?;
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
    // One retained copy of every historical frame and both of its carriages.
    // Nested target/source/retirement readers reserve their own copies; the
    // test quota allows four of those sets plus 32MiB of construction workspace.
    // Malformed frame bytes are skipped here so capacity admission still happens
    // before the fold reports the original inconsistency.
    q.add_v1(retained_history_v1(state)?)?;
    quote_kernel_dependency_rows_v1(state, &mut q)?;
    Ok(q.bytes_v1())
}

fn io_resource(error: std::io::Error) -> Option<NativeCoverageV1> {
    match error.kind() {
        std::io::ErrorKind::OutOfMemory => Some(overflow()),
        std::io::ErrorKind::Unsupported | std::io::ErrorKind::TimedOut => Some(NativeCoverageV1::unsupported::<
            crate::palw_claim_local_v1::PalwClaimLiabilityFrameV1,
        >("checkpoint historical scan", &error.to_string())),
        _ => None,
    }
}

fn retained_history_v1(state: &PalwChainStateV2) -> Result<u64, NativeCoverageV1> {
    use crate::palw_claim_local_v1::{PalwClaimLiabilityFrameV1, PalwClaimLocalInputV1, decode_claim_local_value_v1};
    use crate::palw_native_resource_v1::{NativeDecodeErrorV1, preflight_native_carriage_v1, preflight_native_value_v1};
    let mut total = 0u64;
    let mut prefix_edges = 0u64;
    for (_, bytes) in rewards::frames(state) {
        let shape = match preflight_native_value_v1::<PalwClaimLiabilityFrameV1>(bytes.as_slice(), bytes.len() as u64) {
            Ok(shape) => shape,
            Err(error) => {
                if let Some(fault) = io_resource(error) {
                    return Err(fault);
                }
                continue;
            }
        };
        total = add(total, shape.logical_requests_v1())?;
        let frame = match decode_claim_local_value_v1::<PalwClaimLiabilityFrameV1>(bytes) {
            Ok(frame) => frame,
            Err(NativeDecodeErrorV1::Resource(fault)) => return Err(fault),
            Err(NativeDecodeErrorV1::Decode(_)) => continue,
        };
        prefix_edges = add(
            prefix_edges,
            frame.inputs.iter().filter(|input| matches!(input, PalwClaimLocalInputV1::PrefixDependency { .. })).count() as u64,
        )?;
        for carriage in [&frame.prior_state, &frame.current_state] {
            match preflight_native_carriage_v1(carriage.as_slice(), carriage.len() as u64) {
                Ok(scan) => {
                    let memory = match scan.memory_requests_v1() {
                        Ok(memory) => memory,
                        Err(error) => {
                            if let Some(fault) = io_resource(error) {
                                return Err(fault);
                            }
                            continue;
                        }
                    };
                    total = add(total, memory)?;
                }
                Err(error) => {
                    if let Some(fault) = io_resource(error) {
                        return Err(fault);
                    }
                }
            }
        }
    }
    // Input-set nodes for prefix edges the checkpoint keeps when the source is live.
    let node = std::mem::size_of::<BTreeNodeEnvelopeV1<crate::palw_claim_local_v1::PalwClaimLocalWorkRefV1, ()>>() as u64;
    total = add(total, prefix_edges.checked_mul(node).ok_or_else(overflow)?)?;
    Ok(total)
}

fn quote_kernel_dependency_rows_v1(state: &PalwChainStateV2, q: &mut TirNativeRequestsV1) -> Result<(), NativeCoverageV1> {
    use crate::palw_claim_local_v1::PalwClaimLocalWorkRefV1;
    use crate::palw_native_resource_v1::preflight_native_value_v1;
    use std::collections::BTreeSet;
    let Some(route) = state.kernel_route.as_ref() else { return Ok(()) };
    let mut ids = BTreeSet::new();
    for (id, root) in &state.exec_v2.roots {
        if matches!(root.prefix, crate::palw_work_slice_v2::PalwWorkPrefixStageV2::Unbound) {
            continue;
        }
        if root.prefix_claim != kaspa_hashes::ZERO_HASH64 {
            ids.insert(root.prefix_claim);
        }
        for (_, slice) in state.exec_v2.slices.range((*id, 0)..=(*id, u32::MAX)) {
            if slice.evidence_root != kaspa_hashes::ZERO_HASH64 {
                ids.insert(slice.evidence_root);
            }
        }
    }
    if ids.is_empty() {
        return Ok(());
    }
    q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1, crate::palw_claim_local_v1::PalwClaimLocalRecordV1>>(ids.len() as u64)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1, ()>>(ids.len() as u64)?;
    for id in ids {
        let Some(row) = route.rows.get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, id.as_byte_slice().to_vec())) else {
            continue;
        };
        let shape = match preflight_native_value_v1::<misaka_palw_kernel::ledger::ClaimRowV1>(row.as_slice(), row.len() as u64) {
            Ok(shape) => shape,
            Err(error) => {
                if let Some(fault) = io_resource(error) {
                    return Err(fault);
                }
                continue;
            }
        };
        q.add_v1(shape.logical_requests_v1())?;
        bytes(q, row.len() as u64)?;
    }
    Ok(())
}
