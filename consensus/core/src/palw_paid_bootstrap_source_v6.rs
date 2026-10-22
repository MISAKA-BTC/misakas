//! Explicit native downstream use of selected bootstrap sources. No old OPV
//! Final/Panel source ID is re-labelled as independent selected approval.
use crate::{Hash64, config::params::{ForkActivation, Params}};
pub use misaka_palw_kernel::claim_paid_bootstrap_source_v6::PaidBootstrapSourcePolicyV6;
use misaka_palw_kernel::claim_paid_bootstrap_source_v6::PaidBootstrapSourceBindingV6;
use borsh::{BorshDeserialize, BorshSerialize};

pub const PAID_BOOTSTRAP_SOURCE_TABLE_V6: u8 = 149;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPaidBootstrapSourceFenceV6 {
    pub activation: ForkActivation,
    pub policy: PaidBootstrapSourcePolicyV6,
}
impl Params {
    pub fn validate_palw_paid_bootstrap_source_v6(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        let mirror = self.palw_paid_bootstrap_source_v6.filter(|f| f.activation != ForkActivation::never())
            .map(|f| (f.activation.daa_score(), f.policy));
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            if bundle.state.paid_bootstrap_source_v6 != mirror {
                return Err(crate::palw_mode_v2::PalwModeV2Error::InvalidOwned("V6 paid bootstrap source mirror is not synchronized".into()));
            }
        }
        if let Some(fence) = self.palw_paid_bootstrap_source_v6 {
            fence.policy.validate().map_err(crate::palw_mode_v2::PalwModeV2Error::InvalidOwned)?;
            if fence.activation != ForkActivation::never() {
                return Err(crate::palw_mode_v2::PalwModeV2Error::InvalidOwned(
                    "paid bootstrap sources remain dormant pending independent review and actual downstream acceptance".into()));
            }
        }
        Ok(())
    }
    pub fn sync_palw_paid_bootstrap_source_v6(&mut self) {
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state.paid_bootstrap_source_v6 = self.palw_paid_bootstrap_source_v6.filter(|f| f.activation != ForkActivation::never())
                .map(|f| (f.activation.daa_score(), f.policy));
        }
    }
}
impl crate::palw_state_v2::PalwStateParamsV2 {
    pub fn paid_bootstrap_source_v6_mirror_matches(&self, activation: u64, policy: &PaidBootstrapSourcePolicyV6) -> bool {
        self.paid_bootstrap_source_v6.as_ref().is_some_and(|(at, p)| *at == activation && p == policy)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidBootstrapLockV6 {
    pub version: u16,
    pub binding: [u8; 64],
    pub source: misaka_palw_challenge::approved_source_v1::ApprovedSourceStateV1,
    pub lock: crate::palw_claim_challenge_v2::PalwPaidClaimLockV2,
}

pub fn binding_key_v6(enrollment: &[u8; 64]) -> [u8; 65] {
    let mut key = [0; 65]; key[0] = 1; key[1..].copy_from_slice(enrollment); key
}
pub fn lock_key_v6(enrollment: &[u8; 64]) -> [u8; 65] {
    let mut key = binding_key_v6(enrollment); key[0] = 2; key
}

use crate::palw_native_resource_v1::{NativeDecodeErrorV1 as E, NativeRetainedV1, NativeWireScanV1,
    BTreeNodeEnvelopeV1, TirNativeRequestsV1,
    preflight_native_value_v1, retain_native_value_decode_classified_v1, reserve_native_requests_v1};
use crate::palw_claim_challenge_v2::{PalwPaidClaimAuthorityV2, PalwPaidClaimLockV2, paid_checker_snapshot_root_v2};
use crate::palw_kernel_route_v1::PalwKernelRouteStateV1;
use misaka_palw_kernel::{hash::{Digest, object_id}, ledger::KernelLedgerV1};
use misaka_palw_challenge::approved_source_v1::{ApprovedSourceCollectorPolicyV1, ApprovedSourceConsumerV1, ApprovedSourceStateV1};

fn bad(why: impl Into<String>) -> E<String> { E::Decode(why.into()) }
pub(crate) fn read_v6<T: NativeWireScanV1 + BorshDeserialize, const N: usize>(
    route: &PalwKernelRouteStateV1, key: &[u8; N],
) -> Result<Option<NativeRetainedV1<T>>, E<String>> {
    read_table_v6(route, PAID_BOOTSTRAP_SOURCE_TABLE_V6, key)
}
fn read_table_v6<T: NativeWireScanV1 + BorshDeserialize, const N: usize>(
    route: &PalwKernelRouteStateV1, table: u8, key: &[u8; N],
) -> Result<Option<NativeRetainedV1<T>>, E<String>> {
    let Some(bytes) = crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(route, table, key)
        .map_err(E::Resource)? else { return Ok(None) };
    preflight_native_value_v1::<T>(bytes, bytes.len() as u64).map_err(|why| bad(why.to_string()))?;
    retain_native_value_decode_classified_v1(bytes, || borsh::from_slice(bytes).map_err(|why| why.to_string())).map(Some)
}
fn epoch_v6(route: &PalwKernelRouteStateV1) -> Result<NativeRetainedV1<crate::palw_source_bootstrap_v1::PalwSourceBootstrapEpochRowV1>, E<String>> {
    read_table_v6(route, crate::palw_source_bootstrap_v1::PALW_SELECTED_SOURCE_BOOTSTRAP_TABLE_V1, &[0])?
        .ok_or_else(|| bad("V6 own bootstrap epoch absent"))
}

/// Constructed during the explicitly signed V6 Commit, before any source in
/// the future window. This fixed row freezes the policy and native checker
/// graph; no later action switches an old enrollment's entropy mode.
pub(crate) fn binding_for_authority_v6(
    route: &PalwKernelRouteStateV1, ledger: &KernelLedgerV1, a: &PalwPaidClaimAuthorityV2,
    policy: PaidBootstrapSourcePolicyV6,
) -> Result<PaidBootstrapSourceBindingV6, E<String>> {
    policy.validate().map_err(bad)?;
    a.validate(ledger).map_err(bad)?;
    let epoch = epoch_v6(route)?;
    let bootstrap = epoch.context.policy;
    let epoch_id = epoch.context.epoch().map_err(bad)?.id().map_err(|_| bad("V6 own epoch identity"))?;
    let until = a.enrollment.lifetime.liability_until;
    if !a.class.supports_approved_bootstrap_source_v6()
        || epoch.draw_refused || epoch.context.anchor.is_none()
        || bootstrap.id().as_bytes() != policy.bootstrap_policy
        || bootstrap.network.as_bytes() != policy.network || route.header.policy.network_domain != policy.network
        || bootstrap.chain_genesis.as_bytes() != a.context.chain_genesis || bootstrap.ruleset.as_bytes() != a.context.ruleset_id
        || bootstrap.anchor_cutoff_daa >= a.authorized_daa
        || a.authorized_daa.checked_add(policy.anchor_delay).is_none_or(|start| start >= bootstrap.ends_at_daa)
        || bootstrap.source_profile.as_bytes() == a.class.base_class
        || policy.maximum_events != bootstrap.selection.maximum_source_works
        || until.checked_add(policy.source_liability_slack).is_none_or(|n| n > bootstrap.stake_release_daa)
        || a.authorized_daa.checked_add(policy.anchor_delay).and_then(|n| n.checked_add(policy.window))
            .and_then(|n| n.checked_add(policy.settlement_depth)).is_none_or(|n| n > a.enrollment.lifetime.checking_until)
    { return Err(bad("V6 bootstrap policy/epoch/independent checker/future window/liability is unavailable")); }
    // Direct bootstrap sources have no paid source edges: their code/material
    // is checked by the frozen bootstrap cohort, not by any downstream draw.
    // The concrete native target identity is still rooted in the graph below.
    let lineage = object_id(b"MISAKA/PALW/PAID-BOOTSTRAP-NATIVE-LINEAGE/V6", &(
        policy.id(), epoch_id, a.enrollment.base_claim, a.class.base_class, a.job.base_job,
        a.subject.commitment_root, paid_checker_snapshot_root_v2(&a.checker_candidates),
        bootstrap.checker_scope.as_bytes(), bootstrap.source_profile.as_bytes(),
    ));
    Ok(PaidBootstrapSourceBindingV6 {
        version: 6, policy: policy.id(), contract: policy, enrollment: a.enrollment.id(), original_context: a.context.id(),
        checker_snapshot: paid_checker_snapshot_root_v2(&a.checker_candidates), liability_until: until,
        collector: ApprovedSourceCollectorPolicyV1 {
            version: 1, network: policy.network, bootstrap_epoch: epoch_id, bootstrap_policy: policy.bootstrap_policy,
            source_profile: bootstrap.source_profile.as_bytes(), checker_scope: bootstrap.checker_scope.as_bytes(),
            required_sources: policy.required_sources, maximum_events: policy.maximum_events,
            anchor_delay: policy.anchor_delay, window: policy.window, settlement_depth: policy.settlement_depth,
        },
        consumer: ApprovedSourceConsumerV1 { commitment: a.subject.commitment_root, commitment_position: a.authorized_daa,
            target_profile: a.class.base_class, lineage },
    })
}

/// Every event is derived from current own native work, cohort approvals and
/// historical settlement. No arbitrary `depends_on = false` source can enter.
pub(crate) fn collect_for_authority_v6(
    route: &PalwKernelRouteStateV1, ledger: &KernelLedgerV1, a: &PalwPaidClaimAuthorityV2,
    binding: &PaidBootstrapSourceBindingV6, now: u64,
) -> Result<ApprovedSourceStateV1, E<String>> {
    if *binding != binding_for_authority_v6(route, ledger, a, binding.contract)? {
        return Err(bad("V6 source binding differs from the own immutable native graph"));
    }
    let epoch = epoch_v6(route)?;
    let mut count = 0usize;
    for ((table, key), _) in &route.aux {
        if *table == crate::palw_source_bootstrap_v1::PALW_SELECTED_SOURCE_BOOTSTRAP_TABLE_V1 && key.len() == 65 && key[0] == 1 {
            count = count.checked_add(1).ok_or_else(|| bad("V6 source count overflow"))?;
            if count > binding.collector.maximum_events as usize {
                return Ok(ApprovedSourceStateV1::Unavailable { found: 0, required: binding.collector.required_sources });
            }
        }
    }
    let bytes = count.checked_mul(std::mem::size_of::<misaka_palw_challenge::approved_source_v1::ApprovedSourceEventV1>())
        .ok_or_else(|| bad("V6 source event workspace overflow"))?;
    let _events = reserve_native_requests_v1(bytes as u64).map_err(E::Resource)?;
    let mut events = Vec::new();
    events.try_reserve_exact(count).map_err(|_| E::Resource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<
        misaka_palw_challenge::approved_source_v1::ApprovedSourceEventV1>("V6 source event vector", "allocator refusal")))?;
    for ((table, key), _) in &route.aux {
        if *table != crate::palw_source_bootstrap_v1::PALW_SELECTED_SOURCE_BOOTSTRAP_TABLE_V1 || key.len() != 65 || key[0] != 1 { continue; }
        let id = Hash64::from_bytes(key[1..].try_into().expect("source key64"));
        let event = crate::palw_state_v2::PalwChainStateV2::paid_source_event_for_route_v6(
            route, ledger, &epoch, id, now, binding.liability_until, binding.contract.source_liability_slack,
        ).map_err(|error| match error { crate::palw_state_v2::PalwStateV2Error::NativeResource(error) => E::Resource(error), other => bad(other.to_string()) })?;
        if let Some(event) = event { events.push(event); }
    }
    misaka_palw_challenge::approved_source_v1::collect_approved_sources_v1(&binding.collector, &binding.consumer, &events, now,
        |source, _| source.claim == a.enrollment.base_claim || source.source_profile == a.class.base_class
            || ledger.paid_claim_consumers_v2.contains_key(&source.claim))
        .map_err(|why| bad(format!("V6 approved source collector refused: {why:?}")))
}

pub(crate) fn anchor_v6(binding: &PaidBootstrapSourceBindingV6, source: &ApprovedSourceStateV1) -> Result<Digest, String> {
    let ApprovedSourceStateV1::Locked { context, .. } = source else { return Err("V6 source anchor is not locked".into()) };
    Ok(object_id(b"MISAKA/PALW/PAID-BOOTSTRAP-DRAW-ANCHOR/V6", &(binding.id(), binding.collector.bootstrap_epoch, context)))
}
pub(crate) fn assignment_v6(
    a: &PalwPaidClaimAuthorityV2, binding: &PaidBootstrapSourceBindingV6, seed: Digest,
) -> Result<(NativeRetainedV1<Vec<crate::palw_state_v2::PalwBondKeyV2>>, u32, Digest), E<String>> {
    type Ranked = (u128, crate::palw_state_v2::PalwBondKeyV2);
    let mut requests = TirNativeRequestsV1::new();
    requests.exact_v1::<BTreeNodeEnvelopeV1<Hash64, Ranked>>(a.checker_candidates.len() as u64).map_err(E::Resource)?;
    requests.exact_v1::<Ranked>(a.checker_candidates.len() as u64).map_err(E::Resource)?;
    let _workspace = reserve_native_requests_v1(requests.bytes_v1()).map_err(E::Resource)?;
    let mut operators = std::collections::BTreeMap::new();
    for candidate in &a.checker_candidates {
        let rank = crate::palw_kernel_route_v1::palw_kernel_interim_key_v1(&Hash64::from_bytes(seed), &candidate.operator, u128::from(candidate.weight));
        let item = operators.entry(candidate.operator).or_insert((rank, candidate.bond));
        if (rank, candidate.bond) < *item { *item = (rank, candidate.bond); }
    }
    let mut ranked: Vec<Ranked> = Vec::new();
    ranked.try_reserve_exact(operators.len()).map_err(|_| E::Resource(
        crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<Ranked>("V6 ranked checker vector", "allocator refusal")))?;
    ranked.extend(operators.into_values());
    // The tuple has a total order, so no additional stable-sort allocation is
    // needed and duplicate operator elimination cannot change tie ordering.
    ranked.sort_unstable();
    let limit = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_SEATS_V1.min(ranked.len());
    let lease = reserve_native_requests_v1((limit * std::mem::size_of::<crate::palw_state_v2::PalwBondKeyV2>()) as u64).map_err(E::Resource)?;
    let mut seats = Vec::new(); seats.try_reserve_exact(limit).map_err(|_| E::Resource(
        crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<crate::palw_state_v2::PalwBondKeyV2>("V6 seat vector", "allocator refusal")))?;
    seats.extend(ranked.into_iter().take(limit).map(|(_,bond)| bond));
    let quorum = u32::from(crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_INTERIM_QUORUM_V1);
    if seats.len() < quorum as usize { return Err(bad("V6 checker snapshot lacks distinct operator quorum")); }
    let assignment = object_id(b"MISAKA/PALW/PAID-BOOTSTRAP-ASSIGNMENT/V6", &(a.enrollment.id(), binding.id(), seed, &seats, quorum));
    Ok((NativeRetainedV1::from_owner_reserved_v1(seats, lease), quorum, assignment))
}

impl PalwKernelRouteStateV1 {
    /// Retained primary-row decode with fixed point-lookup extent. The caller
    /// must still validate the authority against its already retained ledger;
    /// doing that here would recurse while the ledger projects paid inputs.
    pub fn paid_claim_authority_checked_v6(&self, context: &Digest) -> Result<NativeRetainedV1<PalwPaidClaimAuthorityV2>, E<String>> {
        let a: NativeRetainedV1<PalwPaidClaimAuthorityV2> = read_table_v6(
            self, crate::palw_claim_challenge_v2::PAID_CLAIM_ENROLLMENTS_V2, context,
        )?.ok_or_else(|| bad("paid claim authority missing"))?;
        if a.context.id() != *context { return Err(bad("paid claim authority primary key substituted")); }
        Ok(a)
    }
    /// The old enrollment index keeps its version, tag, owner and target
    /// semantics. Both decoding allocations stay owned until the reader ends.
    pub fn paid_claim_enrollment_checked_v6(&self, enrollment: &Digest) -> Result<NativeRetainedV1<PalwPaidClaimAuthorityV2>, E<String>> {
        use crate::palw_claim_challenge_v2::{PalwPaidClaimIndexKindV3, PalwPaidClaimIndexV3, PAID_CLAIM_ID_INDEX_PREFIX_V3};
        let mut key = [0u8; 65]; key[0] = PAID_CLAIM_ID_INDEX_PREFIX_V3; key[1..].copy_from_slice(enrollment);
        let index: NativeRetainedV1<PalwPaidClaimIndexV3> = read_table_v6(
            self, crate::palw_claim_challenge_v2::PAID_CLAIM_ENROLLMENTS_V2, &key,
        )?.ok_or_else(|| bad("paid enrollment index missing"))?;
        if index.version != 3 || index.kind != PalwPaidClaimIndexKindV3::Enrollment || index.owner != *enrollment {
            return Err(bad("paid enrollment index substituted"));
        }
        let a = self.paid_claim_authority_checked_v6(&index.target)?;
        if a.enrollment.id() != *enrollment { return Err(bad("paid enrollment index does not bind its primary authority")); }
        Ok(a)
    }
    pub fn paid_bootstrap_binding_v6(&self, enrollment: &Digest) -> Result<Option<NativeRetainedV1<PaidBootstrapSourceBindingV6>>, E<String>> {
        read_v6(self, &binding_key_v6(enrollment))
    }
    pub fn paid_bootstrap_lock_v6(&self, enrollment: &Digest) -> Result<Option<NativeRetainedV1<PalwPaidBootstrapLockV6>>, E<String>> {
        read_v6(self, &lock_key_v6(enrollment))
    }
    pub fn paid_bootstrap_check_authority_v6(
        &self, a: &PalwPaidClaimAuthorityV2, now: u64,
    ) -> Result<misaka_palw_kernel::claim_paid_v2::PaidClaimCheckAuthorityV2, E<String>> {
        let binding = self.paid_bootstrap_binding_v6(&a.enrollment.id())?.ok_or_else(|| bad("V6 source binding absent"))?;
        let ledger = self.ledger_checked_v1()?;
        let source = collect_for_authority_v6(self, &ledger, a, &binding, now)?;
        let base = ledger.claim_challenge_subject(&a.enrollment.base_claim).ok_or_else(|| bad("V6 native base subject absent"))?;
        misaka_palw_kernel::claim_paid_v2::PaidClaimCheckAuthorityV2::verify_approved_bootstrap_v6(
            &a.class, &a.enrollment, &base, &binding, &source, now, binding.checker_snapshot,
        ).map_err(bad)
    }
    /// This typed selector is only for accepted explicit V6 bindings. V2 rows
    /// retain their original table/codec/context and are never silently moved.
    pub fn paid_claim_effective_lock_v6(&self, a: &PalwPaidClaimAuthorityV2) -> Result<Option<NativeRetainedV1<PalwPaidClaimLockV2>>, String> {
        self.paid_claim_effective_lock_checked_v6(a).map_err(|why| why.to_string())
    }
    pub fn paid_claim_effective_lock_checked_v6(&self, a: &PalwPaidClaimAuthorityV2) -> Result<Option<NativeRetainedV1<PalwPaidClaimLockV2>>, E<String>> {
        if let Some(binding) = self.paid_bootstrap_binding_v6(&a.enrollment.id())? {
            let Some(row) = self.paid_bootstrap_lock_v6(&a.enrollment.id())? else { return Ok(None) };
            if row.version != 6 || row.binding != binding.id() || row.lock.context != binding.id()
                || row.lock.enrollment != a.enrollment.id() || row.lock.seed != binding.seed(&row.source).map_err(bad)?
                || row.lock.anchor != anchor_v6(&binding, &row.source).map_err(bad)?
            { return Err(bad("V6 native lock substitutes its binding/context/seed")); }
            let (seats, quorum, assignment) = assignment_v6(a, &binding, row.lock.seed)?;
            if row.lock.seats != *seats || row.lock.quorum != quorum || row.lock.assignment != assignment {
                return Err(bad("V6 native lock changes the frozen operator assignment"));
            }
            return NativeRetainedV1::try_from_clone_v1(&row.lock).map(Some).map_err(E::Resource);
        }
        read_table_v6(self, crate::palw_claim_challenge_v2::PAID_CLAIM_LOCKS_V2, &a.enrollment.id())
    }
}

impl crate::palw_state_v2::PalwChainStateV2 {
    pub fn assert_paid_bootstrap_source_consistency_v6(
        &self, params: &crate::palw_state_v2::PalwStateParamsV2,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::PalwStateV2Error as S;
        let reject = |why: String| S::CarriageInconsistent(format!("V6 bootstrap source: {why}"));
        let native = |why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, reject);
        let Some(route) = self.kernel_route() else { return Ok(()) };
        if !route.aux.keys().any(|(t,_)| *t == PAID_BOOTSTRAP_SOURCE_TABLE_V6) { return Ok(()); }
        let (at, policy) = params.paid_bootstrap_source_v6.ok_or_else(|| reject("rows lack compiled policy".into()))?;
        policy.validate().map_err(reject)?;
        let now = self.last_point().ok_or_else(|| reject("rows lack native state point".into()))?.daa_score;
        if now < at { return Err(reject("rows precede capability activation".into())); }
        let ledger = route.ledger_checked_v1().map_err(native)?;
        let mut count = 0u32;
        for ((table,key),_) in &route.aux {
            if *table != PAID_BOOTSTRAP_SOURCE_TABLE_V6 { continue; }
            if key.len() != 65 || !matches!(key[0],1|2) { return Err(reject("unknown row family/key extent".into())); }
            let enrollment: Digest = key[1..].try_into().expect("key65");
            let a = route.paid_claim_enrollment_checked_v6(&enrollment).map_err(native)?;
            let binding = route.paid_bootstrap_binding_v6(&enrollment).map_err(native)?
                .ok_or_else(|| reject("native lock lacks a signed-admission binding".into()))?;
            if binding.contract != policy || *binding != binding_for_authority_v6(route, &ledger, &a, policy).map_err(native)?
                || a.authorized_daa < at
                || route.aux.keys().any(|(t,k)| *t == crate::palw_claim_challenge_v2::PAID_CLAIM_LOCKS_V2 && k.as_slice() == enrollment)
            { return Err(reject("binding changes native policy/statement or mixes a legacy lock".into())); }
            if key[0] == 1 {
                count = count.checked_add(1).ok_or_else(|| reject("consumer count overflow".into()))?;
                if count > policy.maximum_consumers { return Err(reject("finite consumer cap exceeded".into())); }
            } else {
                let row = route.paid_bootstrap_lock_v6(&enrollment).map_err(native)?.ok_or_else(|| reject("lock vanished".into()))?;
                if !a.enrollment.lifetime.receipt_open(row.lock.locked_daa) || row.lock.locked_daa < a.authorized_daa
                    || route.paid_claim_effective_lock_checked_v6(&a).map_err(native)?.is_none()
                { return Err(reject("lock changes signed lifetime/frozen assignment".into())); }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod source_controls {
    use super::*;
    #[test]
    fn dedicated_keys_are_fixed_disjoint_and_not_legacy_enrollment_keys() {
        let id = [9;64]; let binding = binding_key_v6(&id); let lock = lock_key_v6(&id);
        assert_eq!(binding.len(), 65); assert_eq!(lock.len(), 65);
        assert_eq!(binding[0], 1); assert_eq!(lock[0], 2); assert_eq!(&binding[1..], &id);
        assert_ne!(binding, lock);
        assert_ne!(PAID_BOOTSTRAP_SOURCE_TABLE_V6, crate::palw_claim_challenge_v2::PAID_CLAIM_LOCKS_V2);
    }
    #[test]
    fn public_presets_do_not_enable_the_new_source_or_invent_policy_values() {
        use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, SIMNET_PARAMS};
        for params in [&MAINNET_PARAMS, &TESTNET_PARAMS, &TESTNET11_PARAMS, &SIMNET_PARAMS] {
            assert!(params.palw_paid_bootstrap_source_v6.is_none());
            params.validate_palw_paid_bootstrap_source_v6().unwrap();
        }
    }
}
