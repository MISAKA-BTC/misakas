//! Bounded own-node accepted facts, never a remote certificate. Native receipt
//! admission, current quorum structure, entropy provenance and actual money are
//! separate observations. Missing historical provenance never becomes success.
use crate::{
    Hash64,
    palw_claim_challenge_v2::*,
    palw_claim_interactive_v3::PalwPaidClaimRoundV3,
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_monetary_material_v1::{PALW_MONETARY_MATERIAL_TABLE_V1, PalwMonetaryMaterialRowV1},
    palw_native_resource_v1::{
        NativeDecodeErrorV1 as E, NativeHeapV1, NativeRetainedV1, NativeWireScanV1, preflight_native_value_v1,
        reserve_native_requests_v1, retain_native_value_decode_classified_v1,
    },
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2},
};
use misaka_palw_kernel::{
    beacon_source_fee::BeaconSourceFeeOutcomeV1,
    claim_paid_v2::PaidClaimReceiptV2,
    hash::{Digest, object_id},
};

pub const MAX_PAID_OBSERVATION_ROUNDS_V5: u32 = 64;
pub const MAX_PAID_OBSERVATION_SOURCES_V5: usize = 64;

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePaidClaimSnapshotV5 {
    pub checkpoint: Hash64,
    pub state_point: Hash64,
    pub state_root: Hash64,
    pub daa: u64,
    pub import_epoch: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativePaidFactV5 {
    Unknown,
    Observed,
    Incomplete,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePaidLockObservationV5 {
    pub round: u32,
    pub context: Hash64,
    pub anchor: Hash64,
    pub sample_seed: Hash64,
    pub assignment: Hash64,
    pub locked_daa: u64,
    pub seats: u32,
    pub quorum: u32,
    pub distinct_complete_receipts: u32,
    pub source_valid_native_flag: bool,
    pub assignment_recomputed: bool,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePaidLatestDrainV5 {
    pub source_claim: Hash64,
    pub source_context: Hash64,
    pub recipient: PalwBondKeyV2,
    pub entitlement: Hash64,
    pub income: Hash64,
    pub queue: Hash64,
    pub amount: u64,
    pub coinbase_queue_amount: u64,
    pub paid_block: Hash64,
    pub paid_daa: u64,
    pub material_receipt: Hash64,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePaidClaimObservationV5 {
    pub version: u16,
    pub snapshot: NativePaidClaimSnapshotV5,
    pub base_claim: Hash64,
    pub authority_present: bool,
    pub enrollment: Option<Hash64>,
    pub context: Option<Hash64>,
    pub checker_snapshot: Option<Hash64>,
    pub expected_rounds: u32,
    pub checked_rounds: u32,
    pub last_lock: Option<NativePaidLockObservationV5>,
    pub native_consumer_complete: Option<bool>,
    /// Exact accepted row facts and recomputed frozen operator assignment;
    /// this does not supply the absent independent entropy provenance adapter.
    pub native_assigned_quorum: NativePaidFactV5,
    pub native_selected_approval: NativePaidFactV5,
    pub source_provenance_revalidated: NativePaidFactV5,
    pub historical_signature_replay: NativePaidFactV5,
    pub final_projection: NativePaidFactV5,
    pub observation_limit_reached: bool,
    pub source_contexts_observed: u32,
    pub source_promises_observed: u32,
    pub escrow_held_sompi: u128,
    pub escrow_paid_sompi: u128,
    pub escrow_refunded_sompi: u128,
    pub latest_matching_native_drain: Option<NativePaidLatestDrainV5>,
    pub coinbase_output_replayed: NativePaidFactV5,
    pub one_time_fee_settlement: NativePaidFactV5,
    pub release_approval: bool,
    pub security_soundness_measured: bool,
}
fn bad(why: impl Into<String>) -> E<String> {
    E::Decode(why.into())
}
fn row<T: NativeWireScanV1 + borsh::BorshDeserialize, const N: usize>(
    route: &PalwKernelRouteStateV1,
    table: u8,
    key: &[u8; N],
) -> Result<Option<NativeRetainedV1<T>>, E<String>> {
    let Some(bytes) =
        crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(route, table, key).map_err(E::Resource)?
    else {
        return Ok(None);
    };
    preflight_native_value_v1::<T>(bytes, bytes.len() as u64).map_err(|why| bad(why.to_string()))?;
    retain_native_value_decode_classified_v1(bytes, || borsh::from_slice(bytes).map_err(|why| why.to_string())).map(Some)
}
fn temporary<T: NativeHeapV1>(value: &T) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, E<String>> {
    let shape = value.native_heap_v1("paid own observation validation overlap");
    let bytes =
        shape.bytes().ok_or_else(|| E::Resource(shape.clone()))?.checked_mul(8).and_then(|n| n.checked_add(65536)).ok_or_else(
            || E::Resource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<T>("paid observation validation overlap")),
        )?;
    reserve_native_requests_v1(bytes).map_err(E::Resource)
}

/// Only the own Consensus API may supply `snapshot`; deserializing equivalent
/// JSON does not grant its provenance. Fixed output has no nested heap.
pub fn native_paid_claim_observation_v5(
    state: &PalwChainStateV2,
    snapshot: NativePaidClaimSnapshotV5,
    base: Hash64,
) -> Result<NativeRetainedV1<NativePaidClaimObservationV5>, E<String>> {
    let owner = reserve_native_requests_v1(std::mem::size_of::<NativePaidClaimObservationV5>() as u64).map_err(E::Resource)?;
    let mut out = NativePaidClaimObservationV5 {
        version: 5,
        snapshot,
        base_claim: base,
        authority_present: false,
        enrollment: None,
        context: None,
        checker_snapshot: None,
        expected_rounds: 0,
        checked_rounds: 0,
        last_lock: None,
        native_consumer_complete: None,
        native_assigned_quorum: NativePaidFactV5::Unknown,
        native_selected_approval: NativePaidFactV5::Unknown,
        source_provenance_revalidated: NativePaidFactV5::Unknown,
        historical_signature_replay: NativePaidFactV5::Unknown,
        final_projection: NativePaidFactV5::Unknown,
        observation_limit_reached: false,
        source_contexts_observed: 0,
        source_promises_observed: 0,
        escrow_held_sompi: 0,
        escrow_paid_sompi: 0,
        escrow_refunded_sompi: 0,
        latest_matching_native_drain: None,
        coinbase_output_replayed: NativePaidFactV5::Unknown,
        one_time_fee_settlement: NativePaidFactV5::Unknown,
        release_approval: false,
        security_soundness_measured: false,
    };
    let Some(route) = state.kernel_route() else { return Ok(NativeRetainedV1::from_owner_reserved_v1(out, owner)) };
    let mut key = [0; 65];
    key[0] = PAID_CLAIM_BASE_INDEX_PREFIX_V3;
    key[1..].copy_from_slice(base.as_byte_slice());
    let Some(index) = row::<PalwPaidClaimIndexV3, 65>(route, PAID_CLAIM_ENROLLMENTS_V2, &key)? else {
        return Ok(NativeRetainedV1::from_owner_reserved_v1(out, owner));
    };
    if index.version != 3 || index.kind != PalwPaidClaimIndexKindV3::BaseClaim || index.owner != base.as_bytes() {
        return Err(bad("paid observation base index differs"));
    }
    let a = row::<PalwPaidClaimAuthorityV2, 64>(route, PAID_CLAIM_ENROLLMENTS_V2, &index.target)?
        .ok_or_else(|| bad("paid observation authority target absent"))?;
    let _validation = temporary(&*a)?;
    let ledger = route.ledger_checked_v1()?;
    a.validate(&ledger).map_err(bad)?;
    if a.context.id() != index.target || a.enrollment.base_claim != base.as_bytes() || a.authorized_daa > snapshot.daa {
        return Err(bad("paid observation authority substitutes base/clock"));
    }
    out.authority_present = true;
    out.enrollment = Some(Hash64::from_bytes(a.enrollment.id()));
    out.context = Some(Hash64::from_bytes(a.context.id()));
    out.checker_snapshot = Some(Hash64::from_bytes(paid_checker_snapshot_root_v2(&a.checker_candidates)));
    out.native_consumer_complete = ledger.paid_claim_consumers_v2.get(&base.as_bytes()).map(|c| c.complete);
    out.expected_rounds = a.class.interactive_plan.as_ref().map_or(1, |plan| plan.rounds);
    if out.expected_rounds > MAX_PAID_OBSERVATION_ROUNDS_V5 {
        out.observation_limit_reached = true;
        return Ok(NativeRetainedV1::from_owner_reserved_v1(out, owner));
    }
    let mut prior = [0; 64];
    let mut contexts = [[0; 64]; MAX_PAID_OBSERVATION_ROUNDS_V5 as usize];
    let mut contexts_len = 0;
    let mut all_complete = true;
    for round in 0..out.expected_rounds {
        let mut proof_round = None;
        let lock;
        let mut counted = 0;
        if a.class.interactive_plan.is_some() {
            let id = misaka_palw_kernel::claim_interactive_v3::round_id_v3(a.enrollment.id(), round);
            let Some(current) = row::<PalwPaidClaimRoundV3, 64>(route, PAID_CLAIM_ROUNDS_V2, &id)? else {
                all_complete = false;
                break;
            };
            let _round_validation = temporary(&*current)?;
            let _compact_validation = compact_overlap(route, &a, &current)?;
            route.validate_paid_round_v4(&current, &a, &ledger).map_err(bad)?;
            if current.id() != id
                || current.prior_transcript != prior
                || current.accepted_daa > snapshot.daa
                || current.receipts.iter().any(|receipt| receipt.signed_daa > snapshot.daa)
            {
                return Err(bad("paid observation round transcript/clock substituted"));
            }
            prior = current.transcript_root;
            counted = u32::try_from(current.receipts.len()).map_err(|_| bad("paid observation receipt count overflow"))?;
            let Some(current_lock) = current.lock.as_ref() else {
                all_complete = false;
                break;
            };
            lock = NativeRetainedV1::try_from_clone_v1(current_lock).map_err(E::Resource)?;
            proof_round = Some(current);
        } else {
            let Some(initial) = row::<PalwPaidClaimLockV2, 64>(route, PAID_CLAIM_LOCKS_V2, &a.enrollment.id())? else {
                all_complete = false;
                break;
            };
            lock = initial;
            if lock.context != a.context.id() {
                return Err(bad("paid observation initial lock context differs"));
            }
            for seat in &lock.seats {
                let signer = palw_kernel_bond_id_v1(seat);
                let id = object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(a.enrollment.id(), signer));
                let Some(receipt) = row::<PaidClaimReceiptV2, 64>(route, PAID_CLAIM_RECEIPTS_V2, &id)? else { continue };
                validate_receipt(&a, &lock, &receipt, signer, snapshot.daa)?;
                counted = counted.checked_add(1).ok_or_else(|| bad("paid observation distinct receipts overflow"))?;
            }
        }
        let assignment = assignment_matches(&a, &lock, proof_round.as_deref().map(|r| r.id()))?;
        if lock.enrollment != a.enrollment.id()
            || lock.locked_daa > snapshot.daa
            || lock.locked_daa < a.authorized_daa
            || !a.enrollment.lifetime.receipt_open(lock.locked_daa)
        {
            return Err(bad("paid observation lock identity/clock differs"));
        }
        let complete = assignment && lock.source_valid && counted >= lock.quorum && lock.quorum > 0;
        all_complete &= complete;
        out.checked_rounds += 1;
        out.last_lock = Some(NativePaidLockObservationV5 {
            round,
            context: Hash64::from_bytes(lock.context),
            anchor: Hash64::from_bytes(lock.anchor),
            sample_seed: Hash64::from_bytes(lock.seed),
            assignment: Hash64::from_bytes(lock.assignment),
            locked_daa: lock.locked_daa,
            seats: lock.seats.len() as u32,
            quorum: lock.quorum,
            distinct_complete_receipts: counted,
            source_valid_native_flag: lock.source_valid,
            assignment_recomputed: assignment,
        });
        if !contexts[..contexts_len].contains(&lock.context) {
            contexts[contexts_len] = lock.context;
            contexts_len += 1;
            observe_source_fees(state, route, &ledger, lock.context, &mut out)?;
        }
    }
    out.native_assigned_quorum =
        if all_complete && out.checked_rounds == out.expected_rounds && out.native_consumer_complete == Some(true) {
            NativePaidFactV5::Observed
        } else {
            NativePaidFactV5::Incomplete
        };
    if let Some(final_row) = row::<PalwPaidClaimFinalV2, 64>(route, PAID_CLAIM_FINALS_V2, &a.enrollment.id())? {
        let last = out.last_lock.ok_or_else(|| bad("paid Final observation has no matching lock"))?;
        let claim = ledger.claims.get(&base.as_bytes()).ok_or_else(|| bad("paid Final base absent"))?;
        if final_row.enrollment != a.enrollment.id()
            || final_row.base_claim != base.as_bytes()
            || final_row.verification_class != a.class.id()
            || final_row.signed_lifetime != a.enrollment.lifetime
            || final_row.sample_seed != last.sample_seed.as_bytes()
            || final_row.source_context != last.context.as_bytes()
            || final_row.completed_rounds != out.expected_rounds
            || final_row.transcript_root != prior
            || final_row.complete_receipts != last.quorum
            || final_row.logical_final_daa > snapshot.daa
            || final_row.logical_final_daa > a.enrollment.lifetime.liability_until
            || final_row.logical_final_daa != a.enrollment.lifetime.dispute_until.max(claim.life.final_hold_until)
            || !matches!(claim.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. })
            || out.native_assigned_quorum != NativePaidFactV5::Observed
        {
            return Err(bad("paid Final projection differs from exact accepted rounds"));
        }
        out.final_projection = NativePaidFactV5::Observed;
    } else {
        out.final_projection = NativePaidFactV5::Incomplete;
    }
    // finals_read_v1 currently emits no authenticated source event for any
    // legacy Final path. The independent bootstrap's separate provenance has
    // no versioned adapter to this paid draw. Never manufacture it here, even
    // when accepted receipt structure and a lock's native flag are present.
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, owner))
}

fn validate_receipt(
    a: &PalwPaidClaimAuthorityV2,
    lock: &PalwPaidClaimLockV2,
    receipt: &PaidClaimReceiptV2,
    signer: Digest,
    now: u64,
) -> Result<(), E<String>> {
    if receipt.version != 2
        || !receipt.complete
        || receipt.enrollment != a.enrollment.id()
        || receipt.signer != signer
        || receipt.round != 0
        || receipt.message_root != [0; 64]
        || receipt.assignment != lock.assignment
        || receipt.sample_seed != lock.seed
        || receipt.source_context != lock.context
        || receipt.verification_class != a.class.id()
        || receipt.evidence_root != a.enrollment.commit_root
        || receipt.transcript_root != a.enrollment.prior_transcript
        || receipt.signed_daa < lock.locked_daa
        || receipt.signed_daa > now
        || !a.enrollment.lifetime.receipt_open(receipt.signed_daa)
    {
        return Err(bad("paid observation receipt substitutes the exact assigned signed statement"));
    }
    Ok(())
}
fn assignment_matches(a: &PalwPaidClaimAuthorityV2, lock: &PalwPaidClaimLockV2, round: Option<Digest>) -> Result<bool, E<String>> {
    let mut operators = std::collections::BTreeMap::new();
    for c in &a.checker_candidates {
        let rank =
            crate::palw_kernel_route_v1::palw_kernel_interim_key_v1(&Hash64::from_bytes(lock.seed), &c.operator, u128::from(c.weight));
        let best = operators.entry(c.operator).or_insert((rank, c.bond));
        if (rank, c.bond) < *best {
            *best = (rank, c.bond);
        }
    }
    let mut ranked: Vec<_> = operators.into_values().collect();
    ranked.sort();
    let seats: Vec<_> =
        ranked.into_iter().take(crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_SEATS_V1).map(|(_, bond)| bond).collect();
    let quorum = u32::from(crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_QUORUM_V1);
    let id = if let Some(round) = round {
        object_id(b"MISAKA/PALW/CLAIM-PROOF-ROUND-ASSIGNMENT/V3", &(round, lock.seed, &seats, quorum))
    } else {
        object_id(b"MISAKA/PALW/PAID-CLAIM-ASSIGNMENT/V2", &(a.enrollment.id(), lock.seed, &seats, quorum))
    };
    Ok(lock.seats == seats && lock.quorum == quorum && lock.assignment == id && seats.len() >= quorum as usize)
}

fn compact_overlap(
    route: &PalwKernelRouteStateV1,
    a: &PalwPaidClaimAuthorityV2,
    current: &PalwPaidClaimRoundV3,
) -> Result<
    (Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>),
    E<String>,
> {
    use crate::palw_claim_compact_v4::{
        COMPACT_PROOF_BINDING_PREFIX_V4, PAID_CLAIM_COMPACT_V4, PalwPaidCompactContractV4, PalwPaidCompactProofBindingV4,
    };
    let mut key = [0; 65];
    key[0] = COMPACT_PROOF_BINDING_PREFIX_V4;
    key[1..].copy_from_slice(&current.id());
    let binding = row::<PalwPaidCompactProofBindingV4, 65>(route, PAID_CLAIM_COMPACT_V4, &key)?;
    let binding_overlap = binding.as_ref().map(|value| temporary(&**value)).transpose()?.flatten();
    let contract_overlap = if let Some(plan) = a.class.interactive_plan.as_ref().filter(|plan| plan.version == 4) {
        let contract = row::<PalwPaidCompactContractV4, 64>(route, PAID_CLAIM_COMPACT_V4, &plan.id())?
            .ok_or_else(|| bad("paid observation compact contract absent"))?;
        temporary(&*contract)?
    } else {
        None
    };
    Ok((binding_overlap, contract_overlap))
}

fn observe_source_fees(
    state: &PalwChainStateV2,
    route: &PalwKernelRouteStateV1,
    ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
    context: Digest,
    out: &mut NativePaidClaimObservationV5,
) -> Result<(), E<String>> {
    let Some(funded) = ledger.beacon_source_fees.get(&context) else { return Ok(()) };
    out.source_contexts_observed += 1;
    out.escrow_held_sompi =
        out.escrow_held_sompi.checked_add(u128::from(funded.held)).ok_or_else(|| bad("paid escrow held overflow"))?;
    out.escrow_paid_sompi =
        out.escrow_paid_sompi.checked_add(u128::from(funded.paid)).ok_or_else(|| bad("paid escrow paid overflow"))?;
    out.escrow_refunded_sompi =
        out.escrow_refunded_sompi.checked_add(u128::from(funded.refunded)).ok_or_else(|| bad("paid escrow refunded overflow"))?;
    let mut count = 0;
    for ((_, producer), promise) in ledger.beacon_source_fee_promises.range((context, [0; 64])..=(context, [255; 64])) {
        count += 1;
        if count > MAX_PAID_OBSERVATION_SOURCES_V5 {
            out.observation_limit_reached = true;
            break;
        }
        out.source_promises_observed += 1;
        if promise.terms_id != funded.terms.id() {
            return Err(bad("paid observation source fee terms differ"));
        }
        let Some(source) = ledger.beacon_sources.get(&(context, *producer)) else {
            return Err(bad("paid source promise has no native source"));
        };
        let Some(BeaconSourceFeeOutcomeV1::Paid { daa, claim: paid_claim, .. }) = promise.outcome else { continue };
        let Some((claim, _)) = source.revealed else { continue };
        if claim != paid_claim {
            return Err(bad("paid source fee outcome substitutes revealed claim"));
        }
        let Some(bond) = route.bond_key_of(producer) else { continue };
        let Some(native) = state.bond(&bond) else { continue };
        let Some(material) = row::<PalwMonetaryMaterialRowV1, 64>(route, PALW_MONETARY_MATERIAL_TABLE_V1, &claim)? else { continue };
        if material.terms.root.as_bytes() != claim {
            return Err(bad("paid observation source material key differs"));
        }
        let Some(payment) = &material.last_payment else { continue };
        // Exact frozen domains from native source/queued-income identities.
        let mut subject = misaka_palw_kernel::hash::keyed(b"misaka-palw/bond-budget/source-subject/v1");
        subject.update(&source.job);
        subject.update(&source.seal);
        let subject = Hash64::from_bytes(misaka_palw_kernel::hash::finish(subject));
        let mut entitlement = misaka_palw_kernel::hash::keyed(b"misaka-palw/bond-budget/source-income/v1");
        entitlement.update(&context);
        entitlement.update(subject.as_byte_slice());
        entitlement.update(&borsh::to_vec(&bond).map_err(|why| bad(why.to_string()))?);
        let entitlement = Hash64::from_bytes(misaka_palw_kernel::hash::finish(entitlement));
        let queue = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&native.payout_payload);
        let mut income = misaka_palw_kernel::hash::keyed(b"misaka-palw/bond-budget/native-queued-income/v1");
        income.update(queue.as_byte_slice());
        income.update(&borsh::to_vec(&bond).map_err(|why| bad(why.to_string()))?);
        let income = Hash64::from_bytes(misaka_palw_kernel::hash::finish(income));
        if payment.root.as_bytes() != claim
            || payment.entitlement != Some(entitlement)
            || payment.income != income
            || payment.queue != queue
            || payment.amount != promise.fee
            || payment.amount > payment.coinbase_queue_amount
            || payment.paid_block == Hash64::default()
            || payment.paid_daa > out.snapshot.daa
            || payment.material_receipt_id != crate::palw_bond_budget_v1::material_queue_receipt_id_v1(entitlement, income, daa)
        {
            continue;
        }
        let observed = NativePaidLatestDrainV5 {
            source_claim: Hash64::from_bytes(claim),
            source_context: Hash64::from_bytes(context),
            recipient: bond,
            entitlement,
            income,
            queue,
            amount: payment.amount,
            coinbase_queue_amount: payment.coinbase_queue_amount,
            paid_block: payment.paid_block,
            paid_daa: payment.paid_daa,
            material_receipt: payment.material_receipt_id,
        };
        if out.latest_matching_native_drain.is_none_or(|old| observed.paid_daa > old.paid_daa) {
            out.latest_matching_native_drain = Some(observed);
        }
    }
    Ok(())
}

#[cfg(test)]
mod source_controls {
    use super::*;
    #[test]
    fn absent_own_authority_keeps_approval_signature_and_all_settlement_facts_unknown() {
        // Native metadata construction only. No signature, source work, node,
        // payment, security acceptance or release experiment is performed.
        let state = PalwChainStateV2::genesis();
        let snapshot = NativePaidClaimSnapshotV5 {
            checkpoint: Hash64::from_bytes([1; 64]),
            state_point: Hash64::from_bytes([2; 64]),
            state_root: Hash64::from_bytes([3; 64]),
            daa: 100,
            import_epoch: 4,
        };
        let result = native_paid_claim_observation_v5(&state, snapshot, Hash64::from_bytes([5; 64])).unwrap();
        assert!(!result.authority_present);
        assert_eq!(result.native_assigned_quorum, NativePaidFactV5::Unknown);
        assert_eq!(result.native_selected_approval, NativePaidFactV5::Unknown);
        assert_eq!(result.source_provenance_revalidated, NativePaidFactV5::Unknown);
        assert_eq!(result.historical_signature_replay, NativePaidFactV5::Unknown);
        assert_eq!(result.one_time_fee_settlement, NativePaidFactV5::Unknown);
        assert_eq!(result.coinbase_output_replayed, NativePaidFactV5::Unknown);
        assert!(result.latest_matching_native_drain.is_none());
        assert!(!result.release_approval && !result.security_soundness_measured);
        assert!(std::mem::size_of::<NativePaidClaimObservationV5>() < 4096, "fixed metadata output stays bounded");
    }
}
