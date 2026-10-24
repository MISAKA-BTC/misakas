//! **Read-only paid-claim view** (RPC op 247, `getPalwPaidClaim`) for operators and the RFC-0012 acceptance harness
//! (`scripts/drills/rfc12_acceptance_observe.py`). It reads the kernel-route rows the fold already admitted — the enrollment
//! (by its base-claim index), one round row, the Final row and the ledger's conviction flag — and re-runs the same validators
//! the consumers use. It is never an approval, credit, Final or activation authority, and it adds no row, table or fence.
use crate::palw_native_resource_v1::{
    NativeRetainedV1, NativeWireScanV1, preflight_native_value_v1, retain_native_value_decode_classified_v1,
};
use crate::{
    palw_claim_challenge_v2::*,
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_state_v2::PalwBondKeyV2,
};
use misaka_palw_kernel::{
    claim_interactive_v3::round_id_v3,
    claim_paid_v2::PaidClaimReceiptV2,
    hash::{Digest, object_id},
};

// Presence and decoding are distinct: corrupt authenticated rows must never become absence.
fn read<T: NativeWireScanV1 + borsh::BorshDeserialize, const N: usize>(
    route: &PalwKernelRouteStateV1,
    table: u8,
    key: &[u8; N],
) -> Result<Option<NativeRetainedV1<T>>, String> {
    let Some(bytes) =
        crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(route, table, key).map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    preflight_native_value_v1::<T>(bytes, bytes.len() as u64).map_err(|e| e.to_string())?;
    retain_native_value_decode_classified_v1(bytes, || borsh::from_slice(bytes).map_err(|e| e.to_string()))
        .map(Some)
        .map_err(|e| e.to_string())
}

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
    /// The assigned live seats of the round, or the noninteractive round-zero lock (empty before its lock).
    pub seats: Vec<PalwBondKeyV2>,
    pub quorum: u32,
    /// Accepted seats: interactive receipt order, or noninteractive assignment order.
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
    let mut index_key = [0; 65];
    index_key[0] = PAID_CLAIM_BASE_INDEX_PREFIX_V3;
    index_key[1..].copy_from_slice(base);
    let Some(index) = read::<PalwPaidClaimIndexV3, _>(route, PAID_CLAIM_ENROLLMENTS_V2, &index_key)? else { return Ok(None) };
    if index.version != 3 || index.kind != PalwPaidClaimIndexKindV3::BaseClaim || index.owner != *base {
        return Err("paid base index invalid".into());
    }
    let a = route.paid_claim_authority_checked_v6(&index.target).map_err(|e| e.to_string())?;
    if a.enrollment.base_claim != *base || a.context.id() != index.target {
        return Err("paid base index substitutes immutable enrollment".into());
    }
    let enrollment = a.enrollment.id();
    let indexed = route.paid_claim_enrollment_checked_v6(&enrollment).map_err(|e| e.to_string())?;
    if *indexed != *a {
        return Err("paid enrollment indexes disagree".into());
    }
    let ledger = route.ledger()?;
    a.validate(&ledger)?;
    let row = read::<crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3, _>(
        route,
        PAID_CLAIM_ROUNDS_V2,
        &round_id_v3(enrollment, round),
    )?;
    let ordinary = read::<PalwPaidClaimLockV2, _>(route, PAID_CLAIM_LOCKS_V2, &enrollment)?;
    let binding = route.paid_bootstrap_binding_v6(&enrollment).map_err(|e| e.to_string())?;
    let v6_lock = route.paid_bootstrap_lock_v6(&enrollment).map_err(|e| e.to_string())?;
    if let Some(binding) = &binding {
        let expected = crate::palw_paid_bootstrap_source_v6::binding_for_authority_v6(route, &ledger, &a, binding.contract)
            .map_err(|e| e.to_string())?;
        if **binding != expected {
            return Err("paid V6 binding replaces its native authority".into());
        }
    }

    if ordinary.is_some() && (binding.is_some() || v6_lock.is_some()) || binding.is_none() && v6_lock.is_some() {
        return Err("paid claim has conflicting lock namespaces".into());
    }
    let effective;
    let lock;
    let mut receipts = Vec::new();
    let round_found;
    let complete;
    if let Some(plan) = &a.class.interactive_plan {
        if binding.is_some()
            || v6_lock.is_some()
            || ordinary.is_some() && plan.mode == misaka_palw_challenge::InteractiveModeV1::StagedBeacon
        {
            return Err("interactive claim carries an incompatible initial lock".into());
        }
        // FS StageMessage deliberately uses the initial independently locked source.
        // That initial lock is not this round's assignment or completed receipt set.
        if let Some(initial) = &ordinary {
            validate_lock(&a, initial, false, ledger.daa)?;
        }
        if row.is_some() && plan.mode == misaka_palw_challenge::InteractiveModeV1::TranscriptBoundFiatShamir && ordinary.is_none() {
            return Err("FS round lacks its initial source lock".into());
        }
        for c in &a.checker_candidates {
            let id = object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(enrollment, palw_kernel_bond_id_v1(&c.bond)));
            if read::<PaidClaimReceiptV2, _>(route, PAID_CLAIM_RECEIPTS_V2, &id)?.is_some() {
                return Err("interactive claim carries an ordinary receipt".into());
            }
        }
        if let Some(row) = &row {
            if row.enrollment != enrollment || row.round != round {
                return Err("paid round key/identity mismatch".into());
            }
            route.validate_paid_round_v4(row, &a, &ledger)?;
            validate_round_clock(row, ledger.daa)?;
            for receipt in &row.receipts {
                let seat = row
                    .lock
                    .as_ref()
                    .and_then(|l| l.seats.iter().find(|s| palw_kernel_bond_id_v1(s) == receipt.signer))
                    .ok_or("paid round receipt signer is not assigned")?;
                receipts.push(*seat);
            }
        }
        lock = row.as_ref().and_then(|r| r.lock.as_ref());
        round_found = row.is_some();
        complete = row.as_ref().is_some_and(|r| r.complete());
    } else {
        if row.is_some() {
            return Err("noninteractive claim carries an interactive round".into());
        }
        effective = route.paid_claim_effective_lock_checked_v6(&a).map_err(|e| e.to_string())?;
        if effective.is_none() {
            for c in &a.checker_candidates {
                let id = object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(enrollment, palw_kernel_bond_id_v1(&c.bond)));
                if read::<PaidClaimReceiptV2, _>(route, PAID_CLAIM_RECEIPTS_V2, &id)?.is_some() {
                    return Err("unlocked paid claim carries a receipt".into());
                }
            }
        }
        if let Some(l) = &effective {
            validate_lock(&a, l, binding.is_some(), ledger.daa)?;
            for seat in &l.seats {
                let signer = palw_kernel_bond_id_v1(seat);
                let id = object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(enrollment, signer));
                if let Some(receipt) = read::<PaidClaimReceiptV2, _>(route, PAID_CLAIM_RECEIPTS_V2, &id)? {
                    validate_receipt(&a, l, &receipt, signer, ledger.daa)?;
                    if round == 0 {
                        receipts.push(*seat);
                    }
                }
            }
        }
        lock = if round == 0 { effective.as_deref() } else { None };
        round_found = round == 0 && lock.is_some();
        complete = lock.is_some_and(|l| l.source_valid && receipts.len() >= l.quorum as usize);
    }
    let final_row = read::<PalwPaidClaimFinalV2, _>(route, PAID_CLAIM_FINALS_V2, &enrollment)?;
    if let Some(final_row) = &final_row {
        validate_final(route, &a, &ledger, final_row)?;
    }
    let convicted = ledger.claims.get(base).is_some_and(|row| row.convicted);
    let seats = lock.map(|l| l.seats.clone()).unwrap_or_default();
    let state = if final_row.is_some() {
        "final"
    } else if convicted {
        "convicted"
    } else if complete {
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
        round_found,
        quorum: lock.map_or(0, |l| l.quorum),
        seats,
        receipts,
        final_daa: final_row.as_ref().map(|f| f.logical_final_daa),
    }))
}

/// Targeted equivalent of the retained-state Final invariant; never let the requested
/// display round select which lock authenticates the global paid Final.
fn validate_final(
    route: &PalwKernelRouteStateV1,
    a: &PalwPaidClaimAuthorityV2,
    ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
    f: &PalwPaidClaimFinalV2,
) -> Result<(), String> {
    let base = a.enrollment.base_claim;
    let claim = ledger.claims.get(&base).ok_or("paid Final base claim missing")?;
    if !ledger.paid_claim_consumers_v2.get(&base).is_some_and(|c| c.complete)
        || f.enrollment != a.enrollment.id()
        || f.base_claim != base
        || f.verification_class != a.class.id()
        || f.signed_lifetime != a.enrollment.lifetime
        || f.logical_final_daa > ledger.daa
        || f.logical_final_daa > a.enrollment.lifetime.liability_until
        || f.logical_final_daa != a.enrollment.lifetime.dispute_until.max(claim.life.final_hold_until)
        || !matches!(claim.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. })
    {
        return Err("paid Final canonical payment/approvals/statement/clock mismatch".into());
    }
    let (lock, rounds, transcript) = if let Some(plan) = &a.class.interactive_plan {
        if !route.paid_claim_interactive_complete_v3(a, ledger)? {
            return Err("paid Final has incomplete interactive plan".into());
        }
        // A global Final depends on every signed round, including rounds other than
        // the one requested by the operator. Restore checks each of these timelines.
        for previous in 0..plan.rounds - 1 {
            let row = read::<crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3, _>(
                route,
                PAID_CLAIM_ROUNDS_V2,
                &round_id_v3(a.enrollment.id(), previous),
            )?
            .ok_or("paid Final previous round absent")?;
            if row.enrollment != a.enrollment.id() || row.round != previous {
                return Err("paid Final previous round identity mismatch".into());
            }
            validate_round_clock(&row, ledger.daa)?;
        }
        let last = read::<crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3, _>(
            route,
            PAID_CLAIM_ROUNDS_V2,
            &round_id_v3(a.enrollment.id(), plan.rounds - 1),
        )?
        .ok_or("paid Final last round absent")?;
        if last.enrollment != a.enrollment.id() || last.round != plan.rounds - 1 {
            return Err("paid Final last round identity mismatch".into());
        }
        route.validate_paid_round_v4(&last, a, ledger)?;
        validate_round_clock(&last, ledger.daa)?;
        (
            NativeRetainedV1::try_from_clone_v1(last.lock.as_ref().ok_or("paid Final last round lock absent")?)
                .map_err(|e| e.to_string())?,
            plan.rounds,
            last.transcript_root,
        )
    } else {
        (
            route.paid_claim_effective_lock_checked_v6(a).map_err(|e| e.to_string())?.ok_or("paid Final initial lock absent")?,
            1,
            [0; 64],
        )
    };
    if f.completed_rounds != rounds
        || f.transcript_root != transcript
        || f.sample_seed != lock.seed
        || f.source_context != lock.context
        || f.complete_receipts != lock.quorum
        || !lock.source_valid
    {
        return Err("paid Final seed/transcript/full-plan coverage mismatch".into());
    }
    Ok(())
}

fn validate_round_clock(row: &crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3, now: u64) -> Result<(), String> {
    if row.accepted_daa > now
        || row.lock.as_ref().is_some_and(|l| l.locked_daa > now)
        || row.receipts.iter().any(|r| r.signed_daa > now)
    {
        return Err("paid round native timeline mismatch".into());
    }
    Ok(())
}

fn validate_lock(a: &PalwPaidClaimAuthorityV2, l: &PalwPaidClaimLockV2, v6: bool, now: u64) -> Result<(), String> {
    let unique: std::collections::BTreeSet<_> = l.seats.iter().collect();
    if !v6 {
        use crate::palw_native_resource_v1::{BTreeNodeEnvelopeV1, TirNativeRequestsV1, reserve_native_requests_v1};
        type Ranked = (u128, PalwBondKeyV2);
        let mut requests = TirNativeRequestsV1::new();
        requests
            .exact_v1::<BTreeNodeEnvelopeV1<crate::Hash64, Ranked>>(a.checker_candidates.len() as u64)
            .map_err(|e| e.to_string())?;
        requests.exact_v1::<Ranked>(a.checker_candidates.len() as u64).map_err(|e| e.to_string())?;
        let _workspace = reserve_native_requests_v1(requests.bytes_v1()).map_err(|e| e.to_string())?;
        // Match the native lock fold's one seat per operator, including its tie-break and ordering.
        let mut operators = std::collections::BTreeMap::new();
        for c in &a.checker_candidates {
            let rank = crate::palw_kernel_route_v1::palw_kernel_interim_key_v1(
                &crate::Hash64::from_bytes(l.seed),
                &c.operator,
                u128::from(c.weight),
            );
            let entry = operators.entry(c.operator).or_insert((rank, c.bond));
            if (rank, c.bond) < *entry {
                *entry = (rank, c.bond);
            }
        }
        let mut ranked = Vec::new();
        ranked.try_reserve_exact(operators.len()).map_err(|_| "paid checker ranking allocation refused")?;
        ranked.extend(operators.into_values());
        ranked.sort_unstable();
        if ranked.into_iter().take(l.seats.len()).map(|(_, bond)| bond).collect::<Vec<_>>() != l.seats {
            return Err("paid lock changes the selected operator seats".into());
        }
    }

    if l.enrollment != a.enrollment.id()
        || !v6
            && (l.context != a.context.id()
                || l.assignment != object_id(b"MISAKA/PALW/PAID-CLAIM-ASSIGNMENT/V2", &(l.enrollment, l.seed, &l.seats, l.quorum)))
        || l.quorum == 0
        || l.seats.len() < l.quorum as usize
        || l.seats.len() > crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_SEATS_V1
        || unique.len() != l.seats.len()
        || l.locked_daa > now
        || l.locked_daa < a.authorized_daa
        || !a.enrollment.lifetime.receipt_open(l.locked_daa)
        || l.seats.iter().any(|s| !a.checker_candidates.iter().any(|c| c.bond == *s))
    {
        return Err("paid lock enrollment/context/assignment/seats/clock mismatch".into());
    }
    Ok(())
}
fn validate_receipt(
    a: &PalwPaidClaimAuthorityV2,
    l: &PalwPaidClaimLockV2,
    r: &PaidClaimReceiptV2,
    signer: Digest,
    now: u64,
) -> Result<(), String> {
    if r.enrollment != a.enrollment.id()
        || r.signer != signer
        || r.version != 2
        || !r.complete
        || r.round != 0
        || r.message_root != [0; 64]
        || r.assignment != l.assignment
        || r.sample_seed != l.seed
        || r.source_context != l.context
        || r.verification_class != a.class.id()
        || r.evidence_root != a.enrollment.commit_root
        || r.transcript_root != a.enrollment.prior_transcript
        || r.signed_daa > now
        || r.signed_daa < l.locked_daa
        || !a.enrollment.lifetime.receipt_open(r.signed_daa)
    {
        return Err("paid claim receipt projection is malformed/replayed".into());
    }
    Ok(())
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
