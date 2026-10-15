//! Allocation-request ownership for the native settlement cache and sweep.
//! These are bounds on the actual typed source and algorithm allocations, not
//! an encoded-byte multiplier or a physical RSS / production policy approval.
use crate::{
    Hash64,
    palw_fork_choice_commitment_v1::PalwForkChoiceDeltaV1,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1, native_logical_admission_enabled_v1,
        native_resource_admission_enabled_v1, preflight_native_value_v1, reserve_native_requests_v1, typed::BTreeNodeEnvelopeV1,
    },
    palw_native_settlement_v1::{MatureUsefulWorkV1, NativeDeltaEvidenceV1, NativeEffectV1, SettlementEvidenceV1, SettlementStopV1},
    palw_state_v2::{PalwChainStateV2, PalwClaimSourceV2, PalwClaimStateV2, PalwDeltaEntryV2, PalwStateDeltaV2},
    palw_work_slice_v2::PalwExecV2RewardRowV1,
};
use std::{mem::size_of, sync::Arc};

fn enabled() -> bool {
    native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1()
}
fn slots<T>(count: usize, path: &str) -> NativeCoverageV1 {
    (count as u64)
        .checked_mul(size_of::<T>() as u64)
        .map(|bytes| NativeCoverageV1::Bounded { bytes })
        .unwrap_or_else(|| NativeCoverageV1::overflow::<T>(path))
}
fn vec_slots<T>(count: usize, path: &str) -> NativeCoverageV1 {
    // Pinned Vec push growth, including the first nonempty allocation and the
    // old buffer coexisting with a growing buffer. Exact collects fit as well.
    if count == 0 {
        return NativeCoverageV1::zero();
    }
    match count.checked_mul(4) {
        Some(n) => slots::<T>(n.max(8), path),
        None => NativeCoverageV1::overflow::<Vec<T>>(path),
    }
}
fn reserve(bound: NativeCoverageV1) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
    let bytes = bound.bytes().ok_or(bound)?;
    reserve_native_requests_v1(bytes)
}

/// Reserve before extracting evidence, decoding an opaque EXEC reward row,
/// boxing a fork projection or allocating the cache's Arc/BTree entry. Row is
/// the actual consumer's complete struct (including its retained ticket).
pub fn reserve_native_settlement_row_v1<Row>(
    delta: Option<&PalwStateDeltaV2>,
    fork_projection: bool,
    rule_e_root: bool,
) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
    if !enabled() {
        return Ok(None);
    }
    if cfg!(not(target_pointer_width = "64")) {
        return Err(NativeCoverageV1::unsupported::<Row>("settlement cache", "BTree allocation profile requires 64-bit target"));
    }
    let path = "native settlement cached projection";
    let mut bound = slots::<Row>(1, path)
        .add::<Row>(slots::<[usize; 2]>(1, path), path)
        .add::<Row>(slots::<BTreeNodeEnvelopeV1<Hash64, Arc<Row>>>(1, path), path);
    if fork_projection {
        bound = bound.add::<Row>(slots::<PalwForkChoiceDeltaV1>(1, path), path);
    }
    // RFC-0018 GAP-R4: the boxed committed root a row carries where rule E is in force at its point.
    if rule_e_root {
        bound = bound.add::<Row>(slots::<crate::Hash64>(1, path), path);
    }
    let (mut claims, mut free, mut voided, mut rewards) = (0usize, 0usize, 0usize, 0usize);
    if let Some(delta) = delta {
        for entry in &delta.entries {
            match entry {
                PalwDeltaEntryV2::Claim { new: Some(claim), .. } => {
                    // Counting every possible transition avoids copying to
                    // discover the count; phase filtering can only reduce it.
                    claims = claims.checked_add(1).ok_or_else(|| NativeCoverageV1::overflow::<Row>(path))?;
                    voided = voided.checked_add(1).ok_or_else(|| NativeCoverageV1::overflow::<Row>(path))?;
                    bound = bound.add::<Row>(claim.native_heap_v1(path), path);
                    if let PalwClaimSourceV2::FreePrompt { spent, .. } = &claim.source {
                        free = free.checked_add(1).ok_or_else(|| NativeCoverageV1::overflow::<Row>(path))?;
                        bound = bound.add::<Row>(vec_slots::<u32>(spent.len(), path), path);
                    }
                }
                PalwDeltaEntryV2::ExecV2Row { table, new: Some(bytes), .. }
                    if *table == crate::palw_state_v2::PALW_EXEC_V2_TABLE_REWARDS_V1 =>
                {
                    rewards = rewards.checked_add(1).ok_or_else(|| NativeCoverageV1::overflow::<Row>(path))?;
                    // This row contains a nested payout map. Scan the actual
                    // bytes before native_delta_evidence_v1 decodes that map.
                    let shape = preflight_native_value_v1::<PalwExecV2RewardRowV1>(bytes.as_slice(), bytes.len() as u64)
                        .map_err(|e| NativeCoverageV1::unsupported::<PalwExecV2RewardRowV1>(path, &e.to_string()))?;
                    bound = bound.add::<Row>(NativeCoverageV1::Bounded { bytes: shape.logical_requests_v1() }, path);
                }
                _ => {}
            }
        }
    }
    bound = bound
        .add::<Row>(vec_slots::<(Hash64, PalwClaimStateV2)>(claims, path), path)
        .add::<Row>(vec_slots::<(Hash64, PalwClaimStateV2, Vec<u32>)>(free, path), path)
        .add::<Row>(vec_slots::<Hash64>(voided, path), path)
        .add::<Row>(slots::<BTreeNodeEnvelopeV1<Hash64, u64>>(rewards, path), path);
    reserve(bound)
}

/// One owner for the chain-walk Vec, grown BEFORE each push and dropped only
/// after the Vec. Sharing a cached Arc does not deep-copy its evidence.
#[derive(Default)]
pub struct NativeSettlementWalkLeaseV1(Option<NativeDemandLeaseV1>);
impl NativeSettlementWalkLeaseV1 {
    pub fn before_push_v1<Row>(&mut self) -> Result<(), NativeCoverageV1> {
        if !enabled() {
            return Ok(());
        }
        // Eight pointer slots per appended row cover the Vec's first capacity,
        // geometric growth and the prior buffer during reallocation.
        let bytes = (size_of::<Arc<Row>>() as u64)
            .checked_mul(8)
            .ok_or_else(|| NativeCoverageV1::overflow::<Row>("native settlement walk"))?;
        match &mut self.0 {
            Some(lease) => lease.grow_memory_v1(bytes),
            None => {
                self.0 = reserve_native_requests_v1(bytes)?;
                Ok(())
            }
        }
    }
}

/// Reserve the sweep's complete local collections before building any of them.
/// The consumer retains this ticket alongside its chain/facts readiness memo.
/// `extra_facts` is the actual test-only injected count (zero in production).
pub fn reserve_native_settlement_sweep_v1<'a, Memo>(
    evidence: impl Iterator<Item = &'a NativeDeltaEvidenceV1>,
    state: &PalwChainStateV2,
    rows: usize,
    extra_facts: usize,
) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
    if !enabled() {
        return Ok(None);
    }
    let path = "native settlement sweep";
    let (mut facts, mut voided) = (extra_facts, 0usize);
    for row in evidence {
        facts = facts
            .checked_add(row.finalized_attempts.len())
            .ok_or_else(|| NativeCoverageV1::overflow::<NativeDeltaEvidenceV1>(path))?;
        for (_, _, fresh) in &row.fp_spends {
            facts = facts.checked_add(fresh.len()).ok_or_else(|| NativeCoverageV1::overflow::<NativeDeltaEvidenceV1>(path))?;
        }
        voided = voided.checked_add(row.voided.len()).ok_or_else(|| NativeCoverageV1::overflow::<NativeDeltaEvidenceV1>(path))?;
    }
    let sessions = state
        .da_sessions_iter()
        .count()
        .checked_add(state.court_sessions_iter().count())
        .ok_or_else(|| NativeCoverageV1::overflow::<PalwChainStateV2>(path))?;
    let bound = slots::<Memo>(1, path)
        .add::<PalwChainStateV2>(slots::<u8>(40, path), path)
        .add::<PalwChainStateV2>(slots::<BTreeNodeEnvelopeV1<Hash64, ()>>(voided, path), path)
        .add::<PalwChainStateV2>(vec_slots::<(Hash64, u64)>(sessions, path), path)
        .add::<PalwChainStateV2>(slots::<BTreeNodeEnvelopeV1<Hash64, ()>>(sessions, path), path)
        // Output facts and a block's temporary fact Vec coexist during extend.
        .add::<PalwChainStateV2>(vec_slots::<MatureUsefulWorkV1>(facts, path).scale::<PalwChainStateV2>(2, path), path)
        .add::<PalwChainStateV2>(vec_slots::<NativeEffectV1>(rows, path), path)
        .add::<PalwChainStateV2>(vec_slots::<(Hash64, NativeEffectV1)>(rows, path), path)
        .add::<PalwChainStateV2>(slots::<BTreeNodeEnvelopeV1<Hash64, ()>>(rows, path), path)
        // Prefix sweep: results, fact buckets, live references, anchor indexes,
        // and the three accumulator trees. The reference fallback also fits.
        .add::<PalwChainStateV2>(vec_slots::<Result<SettlementEvidenceV1, SettlementStopV1>>(rows, path), path)
        .add::<PalwChainStateV2>(vec_slots::<Vec<usize>>(rows, path), path)
        .add::<PalwChainStateV2>(slots::<usize>(facts, path).scale::<PalwChainStateV2>(8, path), path)
        .add::<PalwChainStateV2>(vec_slots::<&MatureUsefulWorkV1>(facts, path), path)
        .add::<PalwChainStateV2>(vec_slots::<u64>(rows, path), path)
        .add::<PalwChainStateV2>(slots::<BTreeNodeEnvelopeV1<Hash64, usize>>(facts, path), path)
        .add::<PalwChainStateV2>(slots::<BTreeNodeEnvelopeV1<Hash64, ()>>(facts, path).scale::<PalwChainStateV2>(2, path), path)
        .add::<PalwChainStateV2>(slots::<BTreeNodeEnvelopeV1<Hash64, u128>>(facts, path).scale::<PalwChainStateV2>(2, path), path);
    reserve(bound)
}
