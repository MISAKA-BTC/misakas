//! **Read-only paid-claim view** (RPC op 247, `getPalwPaidClaim`) for operators and the RFC-0012 acceptance harness
//! (`scripts/drills/rfc12_acceptance_observe.py`). It reads the kernel-route rows the fold already admitted — the enrollment
//! (by its base-claim index), one round row, the Final row and the ledger's conviction flag — and re-runs the same validators
//! the consumers use. It is never an approval, credit, Final or activation authority, and it adds no row, table or fence.
use crate::{
    palw_claim_challenge_v2::{PAID_CLAIM_FINALS_V2, PAID_CLAIM_ROUNDS_V2, PalwPaidClaimFinalV2},
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_state_v2::PalwBondKeyV2,
};
use misaka_palw_kernel::{claim_interactive_v3::round_id_v3, hash::Digest};

/// One paid claim as the node's own tip state holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPaidClaimReadV1 {
    pub enrollment: Digest,
    pub class_id: Digest,
    pub job_id: Digest,
    pub producer: PalwBondKeyV2,
    pub material_consent: Digest,
    /// `final`, `convicted`, `round-complete`, `round-locked`, `round-open` or `enrolled` (first that holds).
    pub state: &'static str,
    pub round: u32,
    pub round_found: bool,
    /// The assigned live seats of the round (empty before its lock).
    pub seats: Vec<PalwBondKeyV2>,
    pub quorum: u32,
    /// The seats whose accepted receipts the round row holds, in receipt order.
    pub receipts: Vec<PalwBondKeyV2>,
    /// The logical Final DAA when a Final row exists.
    pub final_daa: Option<u64>,
}

/// The paid claim enrolled for `base` and its `round`; `Ok(None)` when no enrollment indexes that base claim.
pub fn palw_paid_claim_read_v1(
    route: &PalwKernelRouteStateV1,
    base: &Digest,
    round: u32,
) -> Result<Option<PalwPaidClaimReadV1>, String> {
    let Some(a) = route.paid_claim_for_base_v2(base)? else { return Ok(None) };
    let enrollment = a.enrollment.id();
    let round_key = borsh::to_vec(&round_id_v3(enrollment, round)).map_err(|e| e.to_string())?;
    let row = if route.aux.contains_key(&(PAID_CLAIM_ROUNDS_V2, round_key)) {
        Some(route.paid_claim_round_v3(&enrollment, round)?)
    } else {
        None
    };
    let final_row = route.aux_row::<PalwPaidClaimFinalV2>(PAID_CLAIM_FINALS_V2, &enrollment);
    if final_row.as_ref().is_some_and(|f| f.enrollment != enrollment || f.base_claim != *base) {
        return Err("paid Final row key/identity mismatch".into());
    }
    let convicted = route.ledger()?.claims.get(base).is_some_and(|row| row.convicted);
    let lock = row.as_ref().and_then(|r| r.lock.as_ref());
    let seats = lock.map(|l| l.seats.clone()).unwrap_or_default();
    let receipts = row
        .as_ref()
        .map(|r| {
            r.receipts
                .iter()
                .map(|receipt| {
                    seats
                        .iter()
                        .find(|seat| palw_kernel_bond_id_v1(seat) == receipt.signer)
                        .copied()
                        .ok_or("paid round receipt signer is not an assigned seat")
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let state = if final_row.is_some() {
        "final"
    } else if convicted {
        "convicted"
    } else if row.as_ref().is_some_and(|r| r.complete()) {
        "round-complete"
    } else if lock.is_some() {
        "round-locked"
    } else if row.is_some() {
        "round-open"
    } else {
        "enrolled"
    };
    Ok(Some(PalwPaidClaimReadV1 {
        enrollment,
        class_id: a.class.id(),
        job_id: a.job.id(),
        producer: a.producer,
        material_consent: a.enrollment.material_consent,
        state,
        round,
        round_found: row.is_some(),
        quorum: lock.map_or(0, |l| l.quorum),
        seats,
        receipts,
        final_daa: final_row.map(|f| f.logical_final_daa),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_empty_route_has_no_paid_claim() {
        let policy = crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(
            crate::Hash64::from_bytes([1; 64]),
            crate::Hash64::from_bytes([2; 64]),
        );
        let route = PalwKernelRouteStateV1::new(policy, None, Default::default());
        assert_eq!(palw_paid_claim_read_v1(&route, &[1; 64], 0).unwrap(), None);
    }
}
