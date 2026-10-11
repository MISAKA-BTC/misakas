//! Local admission for retained jobs, executor work and full proof trees. The daemon
//! supplies its existing shared host ledger; standalone SDK tools may supply a probe.
//! Requested-storage envelopes are operational policy, not measured RSS/fee proofs.

use std::sync::{Arc, OnceLock};

use kaspa_consensus_core::{
    Hash64, palw_tir_class_v1::PalwTirClassV1, palw_tir_step_v1::PalwTirStepSpaceV1, palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::program::StateKind;

use super::{evidence::TirRetainedJobV1, run::TirResumePointV1};
use crate::TirPlan;

pub type TirMemoryLeaseV1 = Box<dyn Send + Sync>;
pub type TirMemoryReserverV1 = Arc<dyn Fn(&'static str, Hash64, Hash64, u64) -> Result<TirMemoryLeaseV1, String> + Send + Sync>;
static RESERVER: OnceLock<TirMemoryReserverV1> = OnceLock::new();

/// Installed once by the daemon. The callback must reserve before returning and its
/// owned guard must release on Drop. No consensus flag, object or price is changed.
pub fn set_tir_memory_reserver_v1(reserver: TirMemoryReserverV1) -> bool {
    RESERVER.set(reserver).is_ok()
}
pub(crate) fn memory_reserver_v1() -> Option<TirMemoryReserverV1> {
    RESERVER.get().cloned()
}
pub(crate) fn reserver_id_v1(reserver: &Option<TirMemoryReserverV1>) -> usize {
    reserver.as_ref().map_or(0, |r| Arc::as_ptr(r) as *const () as usize)
}
pub(crate) fn reserve_v1(
    reserver: &Option<TirMemoryReserverV1>,
    role: &'static str,
    class: Hash64,
    ctx: &PalwJobContextV2,
    bytes: u64,
) -> Result<Option<TirMemoryLeaseV1>, String> {
    reserver.as_ref().map(|r| r(role, class, ctx.context_hash(), bytes)).transpose()
}
fn bytes(n: u128) -> Result<u64, String> {
    u64::try_from(n).map_err(|_| "the TIR requested storage exceeds u64".into())
}
fn add(a: u128, b: u128) -> Result<u128, String> {
    a.checked_add(b).ok_or_else(|| "the TIR requested storage overflows".into())
}
fn mul(a: u128, b: u128) -> Result<u128, String> {
    a.checked_mul(b).ok_or_else(|| "the TIR requested storage overflows".into())
}
fn triangle(n: u128) -> Result<u128, String> {
    Ok(mul(n, add(n, 1)?)? / 2)
}

/// Sum min(k * checkpoint_interval, limit), k in 1..=count, without walking context.
fn checkpoint_rows(count: u128, interval: u128, limit: u128) -> Result<u128, String> {
    let rising = count.min(limit / interval);
    add(mul(interval, triangle(rising)?)?, mul(count - rising, limit)?)
}

/// Native full tree leaf entries + all levels + per-level Vec headers, with no N-leaf
/// temporary allocation. Includes an extra hash vector for dense construction.
pub fn tir_full_tree_storage_need_v1(count: u64) -> Result<u64, String> {
    let mut width = u128::from(count);
    let (mut nodes, mut levels) = (width, 0u128); // tree's independent leaves vector
    while width > 0 {
        nodes = add(nodes, width)?;
        levels += 1;
        if width == 1 {
            break;
        }
        width = width.div_ceil(2);
    }
    bytes(add(
        add(
            mul(nodes, std::mem::size_of::<Option<Hash64>>() as u128)?,
            mul(u128::from(count), std::mem::size_of::<Hash64>() as u128)?,
        )?,
        add(mul(levels, 2 * std::mem::size_of::<Vec<Option<Hash64>>>() as u128)?, 16 << 10)?,
    )?)
}

/// Persistent fold hashes and root-fold scratch, including vector growth. This
/// reservation stays with the Arc that owns the hashes after executor work ends.
pub fn tir_fold_hash_storage_need_v1(count: u64) -> Result<u64, String> {
    bytes(add(mul(u128::from(count), 4 * std::mem::size_of::<Hash64>() as u128)?, 16 << 10)?)
}

pub(crate) struct TirFoldHashesLeaseV1 {
    pub(crate) hashes: Vec<Hash64>,
    pub(crate) admission_id: usize,
    pub(crate) _lease: Option<TirMemoryLeaseV1>,
    pub(crate) _reserver: Option<TirMemoryReserverV1>,
}
impl std::ops::Deref for TirFoldHashesLeaseV1 {
    type Target = Vec<Hash64>;
    fn deref(&self) -> &Self::Target {
        &self.hashes
    }
}

/// Retained hashes/trace/prompt and checkpoint copies. Accounts native vector headers,
/// possible vector capacity growth, global/per-layer Fixed and Hist, and token prefixes.
/// No position/leaf enumeration and no parameter acquisition is performed here.
pub fn tir_retained_storage_need_v1(
    space: &PalwTirStepSpaceV1,
    class: &PalwTirClassV1,
    ctx: &PalwJobContextV2,
    cap: u64,
) -> Result<u64, String> {
    let job = space.job_shape(ctx).map_err(|e| e.to_string())?;
    let count = u128::from(space.leaf_count_capped(ctx, cap).map_err(|e| e.to_string())?);
    let interval = u128::from(space.layout.checkpoint_interval);
    let checkpoints = u128::from(job.positions) / interval;
    let mut points = mul(checkpoints, 2 * std::mem::size_of::<TirResumePointV1>() as u128)?;
    for fixed in space.fixed_instances() {
        points = add(points, mul(checkpoints, add(mul(u128::from(fixed.elements), 16)?, 24)?)?)?;
    }
    for hist in space.hist_instances() {
        let StateKind::Hist { window } = space.program.states[hist.state as usize].kind else {
            return Err("a non-Hist instance".into());
        };
        // max(window suffix, tile tail) <= sum of their lengths; deliberately conservative.
        let rows = add(
            checkpoint_rows(checkpoints, interval, u128::from(window))?,
            checkpoint_rows(checkpoints, interval, u128::from(space.layout.h_tile))?,
        )?;
        points = add(points, add(mul(rows, add(mul(u128::from(hist.elements), 4)?, 24)?)?, mul(checkpoints, 24)?)?)?;
    }
    let prefix = u128::from(job.prefill - 1);
    let first = (prefix / interval).min(checkpoints);
    let token_prefixes = mul(interval, triangle(checkpoints)? - triangle(first)?)?
        .checked_sub(mul(checkpoints - first, prefix)?)
        .ok_or("the checkpoint token count runs backwards")?;
    points = add(points, mul(token_prefixes, 4)?)?;
    let post = space.program.schedule.post as usize;
    let logits = space.program.blocks[post].nodes[space.program.logits as usize].out.elements_at(1) as u128;
    let decode = u128::from(ctx.exact_decode_tokens);
    let trace = mul(decode, add(mul(logits, 4)?, 24 + 4)?)?;
    let data = add(add(mul(count, 256)?, mul(points, 2)?)?, add(trace, mul(u128::from(job.prefill), 4)?)?)?;
    bytes(add(data, add(mul(class.program.len() as u128, 64)?, 16 << 20)?)?)
}

/// A conservative reservation for full executor working buffers and checkpoint
/// export/conversion scratch. Weights already held by the artifact are not counted;
/// the largest parameter's transient materialization is counted. This unmeasured
/// envelope is separate from retained data and full-tree reservations.
pub fn tir_executor_storage_need_v1(space: &PalwTirStepSpaceV1, plan: &TirPlan, ctx: &PalwJobContextV2) -> Result<u64, String> {
    let job = space.job_shape(ctx).map_err(|e| e.to_string())?;
    let mut values = 0u128;
    for block in &plan.blocks {
        let h = u64::from(block.window.unwrap_or(1).min(job.positions));
        for node in &block.nodes {
            values = add(values, mul(u128::from(node.out.elements_at(h)), 16)?)?;
        }
        for carry in &block.carry_in {
            values = add(values, mul(u128::from(carry.elements_at(1)), 32)?)?;
        }
    }
    for instance in &plan.instances {
        let elements = instance.shape.iter().try_fold(1u128, |n, d| mul(n, *d as u128))?;
        let rows = match instance.kind {
            StateKind::Fixed { .. } => 2,
            StateKind::Hist { window } => {
                u128::from(window.min(job.positions)).checked_add(u128::from(space.layout.h_tile)).ok_or("state rows overflow")?
            }
        };
        values = add(values, mul(mul(elements, rows)?, 16)?)?;
    }
    let largest_param = plan.program.params.iter().try_fold(0u128, |largest, p| {
        let elements = p.shape.iter().try_fold(1u128, |n, d| mul(n, u128::from(*d)))?;
        Ok::<_, String>(largest.max(mul(elements, p.dtype.width() as u128)?))
    })?;
    values = add(values, largest_param)?;
    let metadata = mul(mul(plan.program.encode().len() as u128, 64)?, plan.occurrences.len().max(1) as u128)?;
    bytes(add(add(mul(values, 32)?, metadata)?, 16 << 20)?)
}

/// Retained data and its host reservation share ONE Arc owner. Memo eviction releases
/// neither while another request still owns this object; last Arc drop releases both.
/// The wrapper intentionally has no Clone that would copy payload without a new lease.
pub struct TirRetainedJobLeaseV1 {
    inner: TirRetainedJobV1,
    pub(crate) admission_id: usize,
    pub(crate) _lease: Option<TirMemoryLeaseV1>,
    // Keep provider identity alive so a scoped callback address cannot be reused
    // while an old memo still carries admission under that identity.
    _reserver: Option<TirMemoryReserverV1>,
    pub reserved_bytes: u64,
}
impl TirRetainedJobLeaseV1 {
    pub(crate) fn new(
        inner: TirRetainedJobV1,
        reserver: Option<TirMemoryReserverV1>,
        lease: Option<TirMemoryLeaseV1>,
        reserved_bytes: u64,
    ) -> Self {
        Self { inner, admission_id: reserver_id_v1(&reserver), _lease: lease, _reserver: reserver, reserved_bytes }
    }
}
impl std::ops::Deref for TirRetainedJobLeaseV1 {
    type Target = TirRetainedJobV1;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl std::fmt::Debug for TirRetainedJobLeaseV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TirRetainedJobLeaseV1")
            .field("binding", &self.inner.binding)
            .field("reserved_bytes", &self.reserved_bytes)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_storage_sums_match_walk_and_refuse_overflow() {
        for c in [1, 2, 7, 4096] {
            for count in 0..64 {
                for limit in [0, 1, 3, 8, 1024] {
                    assert_eq!(checkpoint_rows(count, c, limit).unwrap(), (1..=count).map(|k| (k * c).min(limit)).sum());
                }
            }
        }
        assert!(checkpoint_rows(u128::MAX, 1, u128::MAX).is_err());
        assert!(tir_full_tree_storage_need_v1(u64::MAX).is_err());
    }
}
