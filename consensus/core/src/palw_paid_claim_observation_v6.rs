//! Readonly observations for explicit approved-bootstrap paid draws. These
//! never reinterpret a legacy lock or turn current row consistency into a
//! historical-signature replay, a security estimate or a payment receipt.
use crate::{
    Hash64,
    palw_claim_challenge_v2::{
        PAID_CLAIM_BASE_INDEX_PREFIX_V3, PAID_CLAIM_ENROLLMENTS_V2, PAID_CLAIM_FINALS_V2, PAID_CLAIM_RECEIPTS_V2,
        PalwPaidClaimFinalV2, PalwPaidClaimIndexKindV3, PalwPaidClaimIndexV3,
    },
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_native_resource_v1::{
        BTreeNodeEnvelopeV1, NativeDecodeErrorV1 as E, NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1,
        preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_classified_v1,
    },
    palw_paid_bootstrap_source_v6::{binding_for_authority_v6, collect_for_authority_v6},
    palw_paid_claim_observation_v5::{NativePaidClaimSnapshotV5, NativePaidFactV5, NativePaidLockObservationV5, validate_receipt},
    palw_state_v2::PalwChainStateV2,
};
use misaka_palw_challenge::approved_source_v1::ApprovedSourceStateV1;
use misaka_palw_kernel::{
    claim_paid_bootstrap_source_v6::PaidBootstrapSourcePolicyV6, claim_paid_v2::PaidClaimReceiptV2, hash::object_id,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeApprovedSourceStageV6 {
    Missing,
    Pending,
    Unavailable,
    Locked,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePaidBootstrapObservationV6 {
    pub version: u16,
    pub snapshot: NativePaidClaimSnapshotV5,
    pub base_claim: Hash64,
    pub authority_present: bool,
    pub explicit_v6_binding: bool,
    pub enrollment: Option<Hash64>,
    pub context: Option<Hash64>,
    pub checker_snapshot: Option<Hash64>,
    pub policy: Option<Hash64>,
    pub binding: Option<Hash64>,
    pub bootstrap_epoch: Option<Hash64>,
    pub required_sources: u32,
    pub current_sources: u32,
    pub source_stage: NativeApprovedSourceStageV6,
    pub source_context: Option<Hash64>,
    pub source_root: Option<Hash64>,
    pub source_output: Option<Hash64>,
    pub source_locked_daa: Option<u64>,
    pub lock: Option<NativePaidLockObservationV5>,
    pub native_consumer_complete: Option<bool>,
    pub native_assigned_quorum: NativePaidFactV5,
    /// Current own cohort/work/material/settlement graph was reconstructed.
    /// This is not a second replay of every historical carrier signature.
    pub source_graph_revalidated: NativePaidFactV5,
    pub final_projection: NativePaidFactV5,
    pub native_selected_approval: NativePaidFactV5,
    pub historical_signature_replay: NativePaidFactV5,
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

pub fn native_paid_bootstrap_observation_v6(
    state: &PalwChainStateV2,
    snapshot: NativePaidClaimSnapshotV5,
    base: Hash64,
    policy: PaidBootstrapSourcePolicyV6,
) -> Result<NativeRetainedV1<NativePaidBootstrapObservationV6>, E<String>> {
    policy.validate().map_err(bad)?;
    let owner = reserve_native_requests_v1(std::mem::size_of::<NativePaidBootstrapObservationV6>() as u64).map_err(E::Resource)?;
    let mut out = NativePaidBootstrapObservationV6 {
        version: 6,
        snapshot,
        base_claim: base,
        authority_present: false,
        explicit_v6_binding: false,
        enrollment: None,
        context: None,
        checker_snapshot: None,
        policy: None,
        binding: None,
        bootstrap_epoch: None,
        required_sources: 0,
        current_sources: 0,
        source_stage: NativeApprovedSourceStageV6::Missing,
        source_context: None,
        source_root: None,
        source_output: None,
        source_locked_daa: None,
        lock: None,
        native_consumer_complete: None,
        native_assigned_quorum: NativePaidFactV5::Unknown,
        source_graph_revalidated: NativePaidFactV5::Unknown,
        final_projection: NativePaidFactV5::Unknown,
        native_selected_approval: NativePaidFactV5::Unknown,
        historical_signature_replay: NativePaidFactV5::Unknown,
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
        return Err(bad("V6 observation base index differs"));
    }
    let a = route.paid_claim_authority_checked_v6(&index.target)?;
    // Authority validation reconstructs a context's two BTree sets and hashes
    // its serialized contracts. Account for those allocations independently
    // of the retained decode; a read must not escape the native quota.
    let nodes = a
        .context
        .eligible_profiles
        .len()
        .checked_add(a.context.excluded_profiles.len())
        .ok_or_else(|| bad("V6 observation context extent overflow"))?;
    let mut requests = TirNativeRequestsV1::new();
    requests.exact_v1::<BTreeNodeEnvelopeV1<[u8; 64], ()>>(nodes as u64).map_err(E::Resource)?;
    let encoded = borsh::object_length(&*a).map_err(|why| bad(why.to_string()))? as u64;
    let overlap = requests
        .bytes_v1()
        .checked_add(encoded.checked_mul(2).ok_or_else(|| bad("V6 validation encoding overflow"))?)
        .ok_or_else(|| bad("V6 validation extent overflow"))?;
    let _validation = reserve_native_requests_v1(overlap).map_err(E::Resource)?;
    let ledger = route.ledger_checked_v1()?;
    a.validate(&ledger).map_err(bad)?;
    if a.enrollment.base_claim != base.as_bytes() || a.authorized_daa > snapshot.daa {
        return Err(bad("V6 observation substitutes native base/clock"));
    }
    out.authority_present = true;
    out.enrollment = Some(Hash64::from_bytes(a.enrollment.id()));
    out.context = Some(Hash64::from_bytes(a.context.id()));
    let Some(binding) = route.paid_bootstrap_binding_v6(&a.enrollment.id())? else {
        // An old V2 enrollment is observable as present, never as a V6 draw.
        return Ok(NativeRetainedV1::from_owner_reserved_v1(out, owner));
    };
    if binding.contract != policy || *binding != binding_for_authority_v6(route, &ledger, &a, policy)? {
        return Err(bad("V6 observation changes its own native policy/binding"));
    }
    out.explicit_v6_binding = true;
    out.checker_snapshot = Some(Hash64::from_bytes(binding.checker_snapshot));
    out.policy = Some(Hash64::from_bytes(binding.policy));
    out.binding = Some(Hash64::from_bytes(binding.id()));
    out.bootstrap_epoch = Some(Hash64::from_bytes(binding.collector.bootstrap_epoch));
    out.required_sources = binding.collector.required_sources;
    out.native_consumer_complete = ledger.paid_claim_consumers_v2.get(&base.as_bytes()).map(|c| c.complete);
    let current = collect_for_authority_v6(route, &ledger, &a, &binding, snapshot.daa)?;
    match current {
        ApprovedSourceStateV1::Pending { found, required } | ApprovedSourceStateV1::Unavailable { found, required } => {
            if required != out.required_sources {
                return Err(bad("V6 observation source requirement differs"));
            }
            out.current_sources = found;
            out.source_stage = if matches!(current, ApprovedSourceStateV1::Pending { .. }) {
                NativeApprovedSourceStageV6::Pending
            } else {
                NativeApprovedSourceStageV6::Unavailable
            };
        }
        ApprovedSourceStateV1::Locked { context, root, output, sources, locked_at } => {
            if sources != out.required_sources || locked_at > snapshot.daa {
                return Err(bad("V6 observation source count/clock differs"));
            }
            out.source_stage = NativeApprovedSourceStageV6::Locked;
            out.current_sources = sources;
            out.source_context = Some(Hash64::from_bytes(context));
            out.source_root = Some(Hash64::from_bytes(root));
            out.source_output = Some(Hash64::from_bytes(output));
            out.source_locked_daa = Some(locked_at);
        }
    }
    out.native_assigned_quorum = NativePaidFactV5::Incomplete;
    out.source_graph_revalidated = NativePaidFactV5::Incomplete;
    out.final_projection = NativePaidFactV5::Incomplete;
    let Some(native_lock) = route.paid_bootstrap_lock_v6(&a.enrollment.id())? else {
        return Ok(NativeRetainedV1::from_owner_reserved_v1(out, owner));
    };
    // This separately recomputes the V6 seed/anchor/frozen assignment. It
    // cannot fall back to the legacy table while a V6 binding exists.
    let lock = route.paid_claim_effective_lock_checked_v6(&a)?.ok_or_else(|| bad("V6 lock disappeared"))?;
    if lock.locked_daa > snapshot.daa || lock.locked_daa < a.authorized_daa || !a.enrollment.lifetime.receipt_open(lock.locked_daa) {
        return Err(bad("V6 observation lock clock differs"));
    }
    let graph_matches = native_lock.source == current && binding.seed(&current).ok() == Some(lock.seed);
    if graph_matches {
        out.source_graph_revalidated = NativePaidFactV5::Observed;
    }
    let mut counted = 0u32;
    for (position, seat) in lock.seats.iter().enumerate() {
        if lock.seats[..position].contains(seat) {
            return Err(bad("V6 observation duplicated checker seat"));
        }
        let signer = palw_kernel_bond_id_v1(seat);
        let id = object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(a.enrollment.id(), signer));
        let Some(receipt) = row::<PaidClaimReceiptV2, 64>(route, PAID_CLAIM_RECEIPTS_V2, &id)? else { continue };
        validate_receipt(&a, &lock, &receipt, signer, snapshot.daa)?;
        counted = counted.checked_add(1).ok_or_else(|| bad("V6 observation receipt count overflow"))?;
    }
    out.lock = Some(NativePaidLockObservationV5 {
        round: 0,
        context: Hash64::from_bytes(lock.context),
        anchor: Hash64::from_bytes(lock.anchor),
        sample_seed: Hash64::from_bytes(lock.seed),
        assignment: Hash64::from_bytes(lock.assignment),
        locked_daa: lock.locked_daa,
        seats: u32::try_from(lock.seats.len()).map_err(|_| bad("V6 seats overflow"))?,
        quorum: lock.quorum,
        distinct_complete_receipts: counted,
        source_valid_native_flag: lock.source_valid,
        assignment_recomputed: true,
    });
    if graph_matches && lock.source_valid && lock.quorum > 0 && counted >= lock.quorum && out.native_consumer_complete == Some(true) {
        out.native_assigned_quorum = NativePaidFactV5::Observed;
    }
    if let Some(final_row) = row::<PalwPaidClaimFinalV2, 64>(route, PAID_CLAIM_FINALS_V2, &a.enrollment.id())? {
        let claim = ledger.claims.get(&base.as_bytes()).ok_or_else(|| bad("V6 Final native base absent"))?;
        if final_row.enrollment != a.enrollment.id()
            || final_row.base_claim != base.as_bytes()
            || final_row.verification_class != a.class.id()
            || final_row.signed_lifetime != a.enrollment.lifetime
            || final_row.sample_seed != lock.seed
            || final_row.source_context != lock.context
            || final_row.completed_rounds != 1
            || final_row.transcript_root != [0; 64]
            || final_row.complete_receipts != lock.quorum
            || final_row.logical_final_daa > snapshot.daa
            || final_row.logical_final_daa > a.enrollment.lifetime.liability_until
            || final_row.logical_final_daa != a.enrollment.lifetime.dispute_until.max(claim.life.final_hold_until)
            || !matches!(claim.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. })
            || out.native_assigned_quorum != NativePaidFactV5::Observed
        {
            return Err(bad("V6 Final observation does not match exact native source/receipt projection"));
        }
        out.final_projection = NativePaidFactV5::Observed;
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, owner))
}
