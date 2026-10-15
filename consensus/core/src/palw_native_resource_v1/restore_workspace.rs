//! Additional concrete consistency workspaces. This inventory names the
//! temporary projections in the native chain-state checks; it is independent
//! of decoded primary payload residency and of the stored derived indexes.
use super::*;
use crate::{
    Hash64,
    palw_state_v2::{PalwBondKeyV2, PalwStateCarriageV2},
};
use typed::BTreeNodeEnvelopeV1;

fn add(a: u64, b: u64) -> Result<u64, NativeCoverageV1> {
    a.checked_add(b).ok_or_else(|| NativeCoverageV1::overflow::<PalwStateCarriageV2>("restore consistency source count"))
}
fn tree<K, V>(q: &mut TirNativeRequestsV1, n: u64) -> Result<(), NativeCoverageV1> {
    q.exact_v1::<BTreeNodeEnvelopeV1<K, V>>(n)
}

/// A borrowed actual-carriage quote, before restoring or validating it. Invalid
/// states are not allowed to borrow post-validation invariants: for example an
/// exposure source is counted from every actual audit assignment/slice, rather
/// than assuming that it names a registered bond or fits an approved maximum.
pub(crate) fn native_restore_consistency_requests_v1(c: &PalwStateCarriageV2) -> Result<u64, NativeCoverageV1> {
    if cfg!(not(target_pointer_width = "64")) {
        return Err(NativeCoverageV1::unsupported::<PalwStateCarriageV2>(
            "restore consistency trees",
            "pinned tree profile requires 64-bit",
        ));
    }
    let mut q = TirNativeRequestsV1::new();
    let claims = c.claims.len() as u64;
    let courts = c.court_sessions.len() as u64;
    let audits = c.vertex.mesh.audits.values().try_fold(0u64, |n, row| add(n, row.assignments.len() as u64))?;
    let exposure = add(add(add(claims, courts)?, audits)?, add(c.vertex.mesh.traps.len() as u64, c.exec_v2.slices.len() as u64)?)?;
    tree::<PalwBondKeyV2, u128>(&mut q, exposure)?; // re-derived reservations
    tree::<PalwBondKeyV2, u128>(&mut q, exposure)?; // filtered nonzero comparison
    tree::<PalwBondKeyV2, u128>(&mut q, c.reserved_exposure.len() as u64)?;
    tree::<PalwBondKeyV2, u128>(&mut q, c.classes.len() as u64)?; // registration recount
    tree::<(u64, Hash64), ()>(&mut q, claims)?; // unresolved
    tree::<Hash64, u32>(&mut q, courts)?; // open_courts
    tree::<&Hash64, ()>(&mut q, c.classes.len() as u64)?; // share-bearing classes
    tree::<&Hash64, ()>(&mut q, c.class_shares.len() as u64)?; // compared share keys
    tree::<Hash64, Hash64>(&mut q, claims)?; // expected work IDs
    tree::<(u64, Hash64), ()>(&mut q, claims)?; // internal expected deadlines
    tree::<(u64, Hash64), ()>(&mut q, claims)?; // parameterized deadline check
    tree::<Hash64, usize>(&mut q, c.derived_artifacts.len() as u64)?;
    tree::<Hash64, u128>(&mut q, c.activation_pool_scheduled.len() as u64)?;
    tree::<Hash64, (u8, u8)>(&mut q, c.da_sessions.len() as u64)?;
    let da_units = c.da_sessions.values().try_fold(0u64, |n, row| add(n, row.units.len() as u64))?;
    tree::<&crate::palw_da_rcore_v1::PalwDaUnitV1, ()>(&mut q, da_units)?;
    // One participant set is materialized while iterating a root's exposure.
    // Total actual slices conservatively bounds its distinct executors, without
    // trusting the root's advertised extra-executor cap before consistency.
    tree::<PalwBondKeyV2, ()>(&mut q, c.exec_v2.slices.len() as u64)?;

    // Rebuilt expected indexes coexist with the stored index generation; the
    // reporter and DA tuple comparisons also Clone the stored indexes. Count
    // these two distinct additional generations from the same concrete private
    // index layouts used by the historical restore, with actual source counts.
    macro_rules! count {
        ($field:ident) => {
            (
                stringify!($field),
                c.$field.len() as u64,
                c.$field
                    .native_members_v1()
                    .ok_or_else(|| NativeCoverageV1::overflow::<PalwStateCarriageV2>("restore index memberships"))?,
            )
        };
    }
    let counts = [
        count!(claims),
        count!(court_close_groups),
        count!(court_sessions),
        count!(da_claims),
        count!(da_sessions),
        count!(exec_v2_job_uses),
        count!(exec_v2_rewards),
        count!(improvement_artifacts),
        count!(improvement_cases),
        count!(improvement_epochs),
        count!(improvement_eval_jobs),
        count!(improvement_licences),
        count!(improvement_lines),
        count!(improvement_opt_ins),
        count!(improvement_setter_sets),
        count!(legacy_disputes),
        count!(reporter_commitments),
        count!(staged_courts),
        count!(vesting),
    ];
    let index = crate::palw_state_v2::restore_index_native_shape_v1(&counts)
        .map_err(|e| NativeCoverageV1::unsupported::<PalwStateCarriageV2>("restore consistency index projections", &e.to_string()))?;
    q.add_v1(index.heap)?;
    q.add_v1(index.heap)?;

    // rebuild_vesting_indices currently clones actual rows into a contiguous
    // Vec before writing indexes. Payload heaps and requested Vec frames are
    // separate terms; no size encoded in a payout is used as a native ratio.
    q.vec_v1::<crate::palw_vesting_v1::PalwVestingRowV1>(c.vesting.len() as u64)?;
    for row in c.vesting.values() {
        let heap = row.native_heap_v1("restore cloned vesting row");
        q.add_v1(heap.bytes().ok_or(heap)?)?;
    }
    Ok(q.bytes_v1())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_consistency_tree_arithmetic_rejects_overflow_before_allocating() {
        let mut q = TirNativeRequestsV1::new();
        assert!(tree::<Hash64, u128>(&mut q, u64::MAX).is_err());
        assert_eq!(q.bytes_v1(), 0);
    }
    #[test]
    fn actual_empty_carriage_has_no_dynamic_recount_workspace() {
        let c = PalwStateCarriageV2::from_state(&crate::palw_state_v2::PalwChainStateV2::genesis());
        assert_eq!(native_restore_consistency_requests_v1(&c).unwrap(), 0);
    }
}
