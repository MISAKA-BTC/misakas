//! Retained TIR decoding for native consumers. Program codec bytes are not a
//! substitute for the typed program and validation/cone allocation inventory.
use super::*;
use misaka_palw_tir::{
    error::{TirError, TirErrorKind},
    program::TirProgramV1,
};

/// Reserve the actual nested Borsh shape and audited TIR analysis workspace
/// before decode_canonical allocates the program, re-encoding and validation
/// scratch. Keep the ticket while any consumer owns the returned program.
/// This does not reserve unrelated shard/work-shape output allocations.
pub fn decode_native_tir_program_v1(canonical: &[u8]) -> Result<NativeRetainedV1<TirProgramV1>, NativeDecodeErrorV1<TirError>> {
    let lease = if native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1() {
        let requests = super::ledger_expansion::standalone_tir_program_requests_v1(canonical)
            .map_err(|e| NativeDecodeErrorV1::Decode(TirError::new(TirErrorKind::Encoding, e.to_string())))?;
        reserve_native_requests_v1(requests).map_err(NativeDecodeErrorV1::Resource)?
    } else {
        None
    };
    let program = TirProgramV1::decode_canonical(canonical).map_err(NativeDecodeErrorV1::Decode)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(program, lease))
}

/// Element-count envelope used by the existing owner-local growing-Vec policy.
///
/// This pure, nonallocating function returns Some(0) for count 0. Otherwise the
/// count is raised to the existing width-dependent minimum (8 for one-byte
/// elements, 4 for widths at most 1024, 1 for larger elements), then doubled with
/// checked arithmetic. None means only that this element-count calculation
/// overflowed; callers must separately check capacity * size_of::<T>() and all
/// local/coexisting byte limits before allocating.
///
/// This exposes the existing logical allocation-request policy, not an actual
/// Vec capacity promise, physical allocator/RSS bound, host lease, native
/// admission authority, price, wire identifier or computation verdict. ZSTs
/// retain that policy too: nonzero counts use minimum 4 and checked doubling;
/// multiplying a successfully returned count by their width yields zero bytes,
/// but element-count overflow still returns None.
#[must_use]
pub const fn growing_vec_request_capacity_v1<T>(count: u64) -> Option<u64> {
    if count == 0 {
        return Some(0);
    }
    let width = std::mem::size_of::<T>();
    let minimum = if width == 1 {
        8
    } else if width <= 1024 {
        4
    } else {
        1
    };
    let capacity = if count < minimum { minimum } else { count };
    capacity.checked_mul(2)
}

/// Checked typed allocation requests for owner-local analyses. Capacity follows
/// the pinned Vec geometric/minimum-capacity rule; this is neither codec bytes
/// multiplied by a guessed factor nor a physical allocator/RSS estimate.
pub(crate) struct TirNativeRequestsV1(u64);
impl TirNativeRequestsV1 {
    pub(crate) fn new() -> Self {
        Self(0)
    }
    pub(crate) fn exact_v1<T>(&mut self, count: u64) -> Result<(), NativeCoverageV1> {
        let bytes = count
            .checked_mul(std::mem::size_of::<T>() as u64)
            .ok_or_else(|| NativeCoverageV1::overflow::<T>("TIR concrete array request"))?;
        self.add_v1(bytes)
    }
    pub(crate) fn vec_v1<T>(&mut self, count: u64) -> Result<(), NativeCoverageV1> {
        if count == 0 {
            return Ok(());
        }
        let capacity =
            growing_vec_request_capacity_v1::<T>(count).ok_or_else(|| NativeCoverageV1::overflow::<T>("TIR growing Vec capacity"))?;
        self.exact_v1::<T>(capacity)
    }
    pub(crate) fn add_v1(&mut self, bytes: u64) -> Result<(), NativeCoverageV1> {
        self.0 = self.0.checked_add(bytes).ok_or_else(|| NativeCoverageV1::overflow::<Self>("coexisting TIR typed requests"))?;
        Ok(())
    }
    pub(crate) fn product_v1(a: u64, b: u64) -> Result<u64, NativeCoverageV1> {
        a.checked_mul(b).ok_or_else(|| NativeCoverageV1::overflow::<Self>("TIR structural cardinality"))
    }
    pub(crate) fn bytes_v1(self) -> u64 {
        self.0
    }
}

/// Held across weights, partition, cell-work and readiness-range construction.
/// The program owner separately retains its parsed program and validation quote.
pub(crate) struct NativeTirShardWorkspaceV1 {
    _lease: Option<NativeDemandLeaseV1>,
}
pub(crate) fn reserve_native_tir_shard_workspace_v1(
    owner: &NativeRetainedV1<TirProgramV1>,
    s_l: u16,
    s_p: u16,
) -> Result<NativeTirShardWorkspaceV1, NativeCoverageV1> {
    use super::typed::BTreeNodeEnvelopeV1;
    use std::ops::Range;
    if !native_logical_admission_enabled_v1() && !native_resource_admission_enabled_v1() {
        return Ok(NativeTirShardWorkspaceV1 { _lease: None });
    }
    let p: &TirProgramV1 = owner;
    let mut q = TirNativeRequestsV1::new();
    let layers = p.schedule.layers.len() as u64;
    let params = p.params.len() as u64;
    let states = p.states.len() as u64;
    let occurrences = layers.checked_add(2).ok_or_else(|| NativeCoverageV1::overflow::<TirProgramV1>("TIR occurrences"))?;
    let globals = params.checked_add(states).ok_or_else(|| NativeCoverageV1::overflow::<TirProgramV1>("TIR touched declarations"))?;
    // apply_plan holds its first weights while cell_permille constructs a second
    // weights object. Each pass has touched params/states plus globals and the
    // cloned `extra` seen set; no per-artifact tensor value is materialized.
    for _ in 0..2 {
        q.exact_v1::<u128>(layers)?;
        q.vec_v1::<u16>(params)?;
        q.vec_v1::<u16>(states)?;
        q.vec_v1::<(bool, u16)>(globals)?;
        q.vec_v1::<(bool, u16)>(globals)?;
    }
    // Legacy min_shards creates one candidate at a time (at most L ranges).
    // The eventual cell partition (S_L ranges) may coexist with it/outer weights.
    q.exact_v1::<Range<usize>>(layers)?;
    let offered = crate::palw_tir_shard_v1::palw_tir_shard_plan_prelim_v1(s_l, s_p, p.schedule.layers.len()).is_ok();
    // Invalid shapes never reach cell construction. Do not turn an unoffered
    // remote count into a fictitious preallocation request before its refusal.
    let cell_shards = if offered { u64::from(s_l) } else { 0 };
    q.exact_v1::<Range<usize>>(cell_shards)?;
    let cells = TirNativeRequestsV1::product_v1(cell_shards, u64::from(s_p))?;
    q.exact_v1::<u128>(cells)?; // raw cell work
    q.vec_v1::<(usize, u128, u128)>(cells)?; // largest-remainder floors
    q.vec_v1::<usize>(cells)?; // sorting order
    q.exact_v1::<usize>(cells)?; // stable-sort temporary
    q.vec_v1::<u16>(cells)?; // returned permille table
    q.add_v1(crate::palw_tir_work_v1::palw_tir_work_native_workspace_v1(p)?)?;
    // Artifact instance readers are actual (param, block) memberships bounded
    // by actual input references, while wanted/range vectors have at most one
    // membership for each (param, occurrence). Count both separately.
    let references = p
        .blocks
        .iter()
        .try_fold(0u64, |count, b| b.nodes.iter().try_fold(count, |count, n| count.checked_add(n.inputs.len() as u64)))
        .ok_or_else(|| NativeCoverageV1::overflow::<TirProgramV1>("TIR input references"))?;
    let instances = TirNativeRequestsV1::product_v1(params, occurrences)?;
    q.exact_v1::<std::collections::BTreeSet<u8>>(params)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<u8, ()>>(references)?;
    q.vec_v1::<Vec<Option<u16>>>(params)?;
    q.vec_v1::<Option<u16>>(instances)?;
    q.exact_v1::<Option<u16>>(TirNativeRequestsV1::product_v1(params, 8)?)?;
    q.vec_v1::<u16>(params)?;
    q.vec_v1::<(u16, Option<u16>)>(instances)?;
    q.vec_v1::<Range<u32>>(instances)?;
    // The proof keeps its opened bytes elsewhere; only challenge IDs and an
    // exact (index, size) projection are constructed by shard readiness here.
    let draw = crate::palw_model_registry_v1::PALW_READINESS_V2_CHUNKS_V1 as u64;
    q.exact_v1::<BTreeNodeEnvelopeV1<u64, ()>>(draw)?;
    q.vec_v1::<u32>(draw)?;
    q.vec_v1::<(u32, usize)>(draw)?;
    // The existing draw hashes Borsh's 68-byte bond encoding via to_vec;
    // pinned Borsh's serializer requests its 1024-byte initial buffer.
    q.exact_v1::<u8>(1024)?;
    Ok(NativeTirShardWorkspaceV1 { _lease: reserve_native_requests_v1(q.bytes_v1())? })
}

#[cfg(test)]
mod tests {
    use super::super::ledger_expansion::standalone_tir_program_requests_v1;
    #[test]
    fn structural_tir_array_and_cardinality_overflow_refuses() {
        let mut q = super::TirNativeRequestsV1::new();
        assert!(q.exact_v1::<u128>(u64::MAX).is_err());
        assert!(q.vec_v1::<u8>(u64::MAX).is_err());
        assert!(super::TirNativeRequestsV1::product_v1(u64::MAX, 2).is_err());
        assert_eq!(q.bytes_v1(), 0);
    }
    #[test]
    fn public_vec_capacity_envelope_keeps_literal_minimum_and_overflow_boundaries() {
        use super::super::growing_vec_request_capacity_v1 as capacity;
        // Const evaluation and the public re-export, not a runtime allocation.
        const ZERO: Option<u64> = super::super::growing_vec_request_capacity_v1::<u8>(0);
        const SMALL: Option<u64> = super::super::growing_vec_request_capacity_v1::<u64>(1);
        assert_eq!(ZERO, Some(0));
        assert_eq!(SMALL, Some(8));
        for (count, expected) in [(0, 0), (1, 16), (7, 16), (8, 16), (9, 18)] {
            assert_eq!(capacity::<u8>(count), Some(expected));
        }
        for (count, expected) in [(0, 0), (1, 8), (3, 8), (4, 8), (5, 10)] {
            assert_eq!(capacity::<u64>(count), Some(expected));
            assert_eq!(capacity::<[u8; 1024]>(count), Some(expected));
            assert_eq!(capacity::<()>(count), Some(expected)); // ZST keeps the old minimum 4 policy.
        }
        for (count, expected) in [(0, 0), (1, 2), (2, 4), (4, 8)] {
            assert_eq!(capacity::<[u8; 1025]>(count), Some(expected));
        }
        let half = u64::MAX / 2;
        assert_eq!(capacity::<u8>(half), Some(u64::MAX - 1));
        assert_eq!(capacity::<[u8; 1025]>(half), Some(u64::MAX - 1));
        assert_eq!(capacity::<()>(half), Some(u64::MAX - 1));
        assert_eq!(capacity::<u8>(half + 1), None);
        assert_eq!(capacity::<u8>(u64::MAX), None);
        assert_eq!(capacity::<[u8; 1025]>(half + 1), None);
        assert_eq!(capacity::<()>(half + 1), None);
    }
    #[test]
    fn private_vec_request_preserves_capacity_byte_and_total_overflow_order() {
        use super::super::NativeCoverageV1;
        let mut q = super::TirNativeRequestsV1::new();
        q.add_v1(7).unwrap();
        assert_eq!(q.vec_v1::<u8>(u64::MAX).unwrap_err(), NativeCoverageV1::overflow::<u8>("TIR growing Vec capacity"));
        assert_eq!(q.0, 7);
        // Capacity doubling succeeds first; native byte multiplication then fails.
        assert_eq!(
            q.vec_v1::<[u8; 1025]>(u64::MAX / 2).unwrap_err(),
            NativeCoverageV1::overflow::<[u8; 1025]>("TIR concrete array request")
        );
        assert_eq!(q.0, 7);
        let mut full = super::TirNativeRequestsV1::new();
        full.add_v1(u64::MAX).unwrap();
        full.vec_v1::<u8>(0).unwrap(); // Old zero early return, even with a full sum.
        assert_eq!(
            full.vec_v1::<u8>(1).unwrap_err(),
            NativeCoverageV1::overflow::<super::TirNativeRequestsV1>("coexisting TIR typed requests")
        );
        assert_eq!(full.bytes_v1(), u64::MAX);
        let mut zst = super::TirNativeRequestsV1::new();
        zst.vec_v1::<()>(1).unwrap();
        assert_eq!(zst.0, 0);
        assert_eq!(zst.vec_v1::<()>(u64::MAX).unwrap_err(), NativeCoverageV1::overflow::<()>("TIR growing Vec capacity"));
        assert_eq!(zst.0, 0);
    }
    #[test]
    fn retained_tir_decode_preserves_the_exact_canonical_program() {
        use misaka_palw_tir::{DType, Ref, TensorType, builder::ProgramBuilder, program::HISTORY_BOUND_V1_HELD};
        let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
        let scores = b.param("scores", DType::I32, &[16], false);
        let mut pre = b.block("topk", vec![]);
        let x = pre.topk(scores, 0, 2);
        let x = pre.cast(x, DType::I32);
        let pre = pre.finish(&[x]);
        let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[2])]);
        let y = post.cast(Ref::CarryIn(0), DType::I32);
        post.commit(y);
        let post = post.finish(&[]);
        let original = b.finish(pre, vec![], post, 0);
        let bytes = original.encode();
        let retained = super::decode_native_tir_program_v1(&bytes).unwrap();
        assert_eq!(retained.encode(), bytes);
        assert_eq!(*retained, original);
        assert!(standalone_tir_program_requests_v1(&bytes).unwrap() > bytes.len() as u64);
    }
    #[test]
    fn retained_tir_program_shape_refuses_truncation_and_hostile_lengths() {
        assert!(standalone_tir_program_requests_v1(&[]).is_err());
        // Program version/prim-set/token/history prefix, then an impossible
        // params vector. The borrowed preflight cannot allocate this count.
        let mut bytes = vec![0u8; 2 + 64 + 4 + 4];
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(standalone_tir_program_requests_v1(&bytes).is_err());
    }
}
