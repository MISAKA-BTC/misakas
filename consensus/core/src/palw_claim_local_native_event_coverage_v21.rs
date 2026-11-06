//! Exact native journal owner partition shared by scoped coverage and the
//! existing F1 reader. This is representation evidence, never payment/approval.
//! No F1 suppression is armed here. Current/removal overlay closure remains an
//! independent prerequisite, including strict owner18 checkpoint restoration.
use super::*;
use crate::palw_claim_local_v1::{PalwClaimLocalRowOwnerV1 as Owner, PalwClaimLocalWorkRefV1 as Work};
use crate::palw_native_resource_v1::{
    BTreeNodeEnvelopeV1, NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
fn bad(s: &str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.into())
}
fn decode<T: borsh::BorshDeserialize + borsh::BorshSerialize + NativeWireScanV1>(
    raw: &[u8],
) -> Result<NativeRetainedV1<T>, PalwStateV2Error> {
    crate::palw_claim_local_v1::decode_claim_local_value_v1(raw).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => bad(&e.to_string()),
    })
}
pub(super) fn approval_owner_parts_v21(
    owner: Owner,
    table: u8,
    key: &[u8],
    raw: Option<&[u8]>,
) -> Result<Option<Work>, PalwStateV2Error> {
    use crate::palw_claim_challenge_v2 as paid;
    use crate::palw_claim_local_v1::PalwClaimLocalWorkRefV1 as W;
    use crate::palw_security_certification_v1 as cert;
    if owner != Owner::KernelRoute {
        return Ok(None);
    }
    let Some(raw) = raw else { return Ok(None) };
    if table == cert::PALW_SECURITY_CERTIFICATION_TABLE_V1 && key.len() == 65 {
        let prefix = key[0];
        if matches!(
            prefix,
            cert::CERTIFICATION_CLAIM_PREFIX_V1
                | cert::CERTIFICATION_NATIVE_MINING_PREFIX_V39
                | cert::CERTIFICATION_KERNEL_CLAIM_PREFIX_V1
                | cert::CERTIFICATION_KERNEL_HOLD_PREFIX_V1
        ) {
            let claim = if prefix == cert::CERTIFICATION_KERNEL_HOLD_PREFIX_V1 {
                decode::<cert::PalwKernelCertifiedRewardHoldV1>(raw)?.claim
            } else {
                decode::<cert::PalwClaimCertificationRowV1>(raw)?.claim
            };
            let id = Hash64::from_bytes(claim);
            // All four canonical keys are65bytes. Admit their temporary Vec
            // before constructing it; the decoder retains its own typed lease.
            let _key = crate::palw_native_resource_v1::reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
            let (expected, owner) = match prefix {
                cert::CERTIFICATION_CLAIM_PREFIX_V1 => {
                    (cert::certification_claim_key_v1(&cert::palw_certification_id_v1(&id)), W::ChainClaim { claim: id })
                }
                cert::CERTIFICATION_NATIVE_MINING_PREFIX_V39 => (
                    cert::certification_native_key_v39(&cert::palw_native_certification_id_v39(&id)),
                    W::NativeMiningClaim39 { claim: id },
                ),
                cert::CERTIFICATION_KERNEL_CLAIM_PREFIX_V1 => (
                    cert::certification_kernel_claim_key_v1(&cert::palw_kernel_claim_certification_id_v1(&claim)),
                    W::KernelClaim { claim: id },
                ),
                _ => (
                    cert::certification_kernel_hold_key_v1(&cert::palw_kernel_claim_certification_id_v1(&claim)),
                    W::KernelClaim { claim: id },
                ),
            };
            if key != expected.as_slice() {
                return Err(bad("approval sidecar changed its canonical subject key"));
            }
            return Ok(Some(owner));
        }
    }
    if table == paid::PAID_CLAIM_FINALS_V2 {
        let row = decode::<paid::PalwPaidClaimFinalV2>(raw)?;
        if key != row.enrollment {
            return Err(bad("paid Final context changed enrollment key"));
        }
        return Ok(Some(W::KernelClaim { claim: Hash64::from_bytes(row.base_claim) }));
    }
    if table == paid::PAID_CLAIM_ENROLLMENTS_V2 && key.len() == 65 && key[0] == paid::PAID_CLAIM_BASE_INDEX_PREFIX_V3 {
        let row = decode::<paid::PalwPaidClaimIndexV3>(raw)?;
        if row.version != 3 || row.kind != paid::PalwPaidClaimIndexKindV3::BaseClaim || key[1..] != row.owner {
            return Err(bad("paid approval base-index context differs"));
        }
        return Ok(Some(W::KernelClaim { claim: Hash64::from_bytes(row.owner) }));
    }
    Ok(None)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeHistoricalRoleV21 {
    Work(Work),
    MiningHold,
    MiningAnchor,
    CarrierWinner,
    CurrentOnly,
}
/// Same historical roles as existing native checkpoint/Carrier9 readers. No
/// event/carriage bytes are projected, rewritten or accepted by this function.
pub(super) fn historical_role_v21(
    owner: Owner,
    table: u8,
    key: &[u8],
    old: Option<&[u8]>,
    new: Option<&[u8]>,
) -> Result<NativeHistoricalRoleV21, PalwStateV2Error> {
    let work = match (owner, table) {
        (Owner::ChainClaims, _) => {
            Some(Work::ChainClaim { claim: Hash64::from_bytes(key.try_into().map_err(|_| bad("scoped chain journal key"))?) })
        }
        (Owner::KernelRoute, misaka_palw_kernel::rows::TABLE_CLAIMS_V1) => {
            Some(Work::KernelClaim { claim: Hash64::from_bytes(key.try_into().map_err(|_| bad("scoped kernel journal key"))?) })
        }
        (Owner::ExecV2, PALW_EXEC_V2_TABLE_ROOTS_V1) => {
            Some(Work::ExecSession { root: borsh::from_slice(key).map_err(|_| bad("scoped session journal key"))? })
        }
        (Owner::ExecV2, PALW_EXEC_V2_TABLE_SLICES_V1) => {
            let (root, index) = borsh::from_slice(key).map_err(|_| bad("scoped slice journal key"))?;
            Some(Work::ExecSlice { root, index })
        }
        (Owner::KernelRoute, crate::palw_native_mining_v39::MINING_TABLE_V39)
            if key.len() == 66 && key[..2] == [39, crate::palw_native_mining_v39::MINING_CLAIM_KIND_V39] =>
        {
            Some(Work::NativeMiningClaim39 { claim: Hash64::from_bytes(key[2..].try_into().expect("checked mining identity")) })
        }
        _ => approval_owner_parts_v21(owner, table, key, new.or(old))?,
    };
    if let Some(w) = work {
        return Ok(NativeHistoricalRoleV21::Work(w));
    }
    if owner == Owner::KernelRoute
        && table == crate::palw_native_mining_v39::MINING_TABLE_V39
        && key.starts_with(&[39, crate::palw_native_mining_v39::MINING_JOB_HOLD_KIND_V39])
    {
        return Ok(NativeHistoricalRoleV21::MiningHold);
    }
    if owner == Owner::KernelRoute && table == crate::palw_native39_frontier_v1::NATIVE39_FRONTIER_TABLE_V1 {
        return Ok(NativeHistoricalRoleV21::MiningAnchor);
    }
    if owner == Owner::KernelRoute && table == crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1 {
        return Ok(NativeHistoricalRoleV21::CarrierWinner);
    }
    Ok(NativeHistoricalRoleV21::CurrentOnly)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeCoverageGapV21 {
    ChainCurrentOrRemoval,
    KernelLifecycle,
    ExecLifecycle,
    MiningLifecycle,
    CarrierLineage,
    MiningHold,
}
/// A read-only audit tied to one real builder borrow. It has no skip/accept API:
/// production arming needs complete publication+retirement+undo+cold support,
/// not merely a successful census of currently present bytes.
pub(super) struct NativeCapturedEventAuditV21<'a> {
    _current_material:
        BTreeMap<ClaimBirthV21, super::palw_claim_local_retired_dependency_material_v24::NativeConsumerMaterialCoverageV24<'a>>,
    _retired_claim: &'a BTreeMap<
        super::palw_claim_local_legacy_terminal_v18::RetiredExecEventKeyV24,
        super::palw_claim_local_legacy_terminal_v18::ActualRetiredClaimObservationV25,
    >,
    _retired_exec:
        BTreeMap<ClaimBirthV21, NativeRetainedV1<super::palw_claim_local_legacy_terminal_v18::NativeRetiredExecEventCoverageV24>>,
    _retired_observed: &'a BTreeMap<
        super::palw_claim_local_legacy_terminal_v18::RetiredExecEventKeyV24,
        super::palw_claim_local_legacy_terminal_v18::RetiredExecEventObservationV24,
    >,
    _entries: &'a [PalwDeltaEntryV2],
    _mining: Option<&'a super::palw_claim_local_native_mining_birth_v20::NativeMiningEventCoverageV21>,
    point: PalwBlockContextV2,
    events: u64,
    covered: u64,
    first_gap: Option<(u64, NativeCoverageGapV21)>,
}
impl NativeCapturedEventAuditV21<'_> {
    pub(super) fn point_v21(&self) -> PalwBlockContextV2 {
        self.point
    }
    pub(super) fn counts_v21(&self) -> (u64, u64) {
        (self.events, self.covered)
    }
    pub(super) fn first_gap_v21(&self) -> Option<(u64, NativeCoverageGapV21)> {
        self.first_gap
    }
}
/// Hook AFTER actual Source8/10/20 and owner18 captures, BEFORE deciding on
/// F1. The caller retains unchanged actual inputs/scalars; no caller DTO/count
/// can stand in for native owner readiness. Resource faults propagate.
type ClaimBirthV21 = (Hash64, Hash64, u64, u64);
fn claim_birth_v21(id: Hash64, c: &PalwClaimStateV2) -> ClaimBirthV21 {
    (id, c.accepted_block, c.accepted_daa, c.accepted_blue_score)
}
fn chain_event_covered_v21(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    id: Hash64,
    entry: &PalwDeltaEntryV2,
    entry_index: usize,
    removed: &BTreeMap<ClaimBirthV21, &PalwClaimStateV2>,
    current_material: &BTreeMap<
        ClaimBirthV21,
        super::palw_claim_local_retired_dependency_material_v24::NativeConsumerMaterialCoverageV24<'_>,
    >,
    retired_cert: &BTreeMap<Hash64, bool>,
) -> Result<bool, PalwStateV2Error> {
    use super::palw_claim_local_legacy_terminal_v18 as legacy;
    if let PalwDeltaEntryV2::Claim { old, new, .. } = entry {
        if let (Some(old), None) = (old.as_ref(), new.as_ref())
            && let Some(observed) = b.claim_local_retired_claim_events_v25.get(&claim_birth_v21(id, old))
            && observed.covers_claim_deletion_v25(ctx, entry_index, id, old)?
        {
            return Ok(true);
        }
        if old.is_none() {
            let Some(born) = new else { return Err(bad("native coverage empty Claim mutation")) };
            let Some(admission) = super::palw_claim_local_source_admission_v8::for_accepting_point_v8(&b.state, id, *ctx)? else {
                return Ok(false);
            };
            if admission.original_claim() != born {
                return Ok(false);
            }
        }
        let subject = new.as_ref().or(old.as_ref()).ok_or_else(|| bad("native coverage Claim has no body"))?;
        if let Some(current) = b.state.claims.get(&id).filter(|c| claim_birth_v21(id, c) == claim_birth_v21(id, subject)) {
            return Ok(current_material.contains_key(&claim_birth_v21(id, current)));
        }
        let Some(terminal) = removed.get(&claim_birth_v21(id, subject)) else { return Ok(false) };
        return legacy::removed_checkpoint_overlay_ready_v18(&b.state, b.params, id, terminal, ctx);
    }
    // Canonical certification-only progress has no Claim mutation. The exact
    // selected-instance relay, not the raw slot, identifies any retired owner.
    if let Some(current) = b.state.claims.get(&id) {
        return Ok(current_material.contains_key(&claim_birth_v21(id, current)));
    }
    Ok(retired_cert.get(&id).copied().unwrap_or(false))
}
pub(super) fn audit_captured_native_events_v21<'a>(
    b: &'a TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<NativeRetainedV1<NativeCapturedEventAuditV21<'a>>, PalwStateV2Error> {
    audit_captured_native_events_with_mining_v21(b, ctx, None)
}
pub(super) fn audit_captured_native_events_with_mining_v21<'a>(
    b: &'a TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    mining: Option<&'a super::palw_claim_local_native_mining_birth_v20::NativeMiningEventCoverageV21>,
) -> Result<NativeRetainedV1<NativeCapturedEventAuditV21<'a>>, PalwStateV2Error> {
    if b.claim_local_lock_point_v6 != Some(*ctx) {
        return Err(bad("native event coverage lacks actual executing point"));
    }
    let mut requests = TirNativeRequestsV1::new();
    requests.exact_v1::<NativeCapturedEventAuditV21<'a>>(1).map_err(PalwStateV2Error::NativeResource)?;
    let removed_count = b.entries.iter().filter(|e| matches!(e, PalwDeltaEntryV2::Claim { old: Some(_), new: None, .. })).count();
    requests
        .exact_v1::<BTreeNodeEnvelopeV1<ClaimBirthV21, &PalwClaimStateV2>>(removed_count as u64)
        .map_err(PalwStateV2Error::NativeResource)?;
    let exec_delete_count=b.entries.iter().filter(|e|matches!(e,PalwDeltaEntryV2::ExecV2Row{table,old:Some(_),new:None,..}if matches!(*table,PALW_EXEC_V2_TABLE_ROOTS_V1|PALW_EXEC_V2_TABLE_SLICES_V1))).count();
    requests.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(exec_delete_count as u64).map_err(PalwStateV2Error::NativeResource)?;
    requests
        .exact_v1::<BTreeNodeEnvelopeV1<
            ClaimBirthV21,
            NativeRetainedV1<super::palw_claim_local_legacy_terminal_v18::NativeRetiredExecEventCoverageV24>,
        >>(removed_count as u64)
        .map_err(PalwStateV2Error::NativeResource)?;

    let chain_slots=b.entries.iter().filter(|e|matches!(e,PalwDeltaEntryV2::Claim{..})||matches!(e,PalwDeltaEntryV2::KernelRouteRow{table,key,..}if *table==crate::palw_security_certification_v1::PALW_SECURITY_CERTIFICATION_TABLE_V1&&key.len()==65&&key[0]==crate::palw_security_certification_v1::CERTIFICATION_CLAIM_PREFIX_V1)).count();
    requests.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(chain_slots as u64).map_err(PalwStateV2Error::NativeResource)?;
    requests.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(chain_slots as u64).map_err(PalwStateV2Error::NativeResource)?;
    requests.exact_v1::<BTreeNodeEnvelopeV1<Hash64, bool>>(chain_slots as u64).map_err(PalwStateV2Error::NativeResource)?;
    requests
        .exact_v1::<BTreeNodeEnvelopeV1<
            ClaimBirthV21,
            super::palw_claim_local_retired_dependency_material_v24::NativeConsumerMaterialCoverageV24<'a>,
        >>(chain_slots as u64)
        .map_err(PalwStateV2Error::NativeResource)?;
    let lease = reserve_native_requests_v1(requests.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut removed = BTreeMap::new();
    for entry in &b.entries {
        if let PalwDeltaEntryV2::Claim { key, old: Some(c), new: None } = entry
            && removed.insert(claim_birth_v21(*key, c), c).is_some()
        {
            return Err(bad("native coverage duplicate exact Claim retirement"));
        }
    }
    // Only roots with genuine native deletion entries need a retained EXEC
    // decode. Both indices are admitted above; each exact retired birth is minted
    // and validated ONCE, then every slice uses binary lookup in its own token.
    let mut deleting = BTreeSet::new();
    for e in &b.entries {
        let PalwDeltaEntryV2::ExecV2Row { table, key, old: Some(_), new: None } = e else { continue };
        let root = if *table == PALW_EXEC_V2_TABLE_ROOTS_V1 {
            Some(borsh::from_slice::<Hash64>(key).map_err(|_| bad("retired EXEC root key codec"))?)
        } else if *table == PALW_EXEC_V2_TABLE_SLICES_V1 {
            Some(borsh::from_slice::<(Hash64, u32)>(key).map_err(|_| bad("retired EXEC slice key codec"))?.0)
        } else {
            None
        };
        if let Some(root) = root {
            deleting.insert(root);
        }
    }
    let mut retired_exec = BTreeMap::new();
    for (birth, terminal) in &removed {
        if !deleting.contains(&birth.0) || b.claim_local_retired_exec_events_v24.contains_key(birth) {
            continue;
        }
        if let Some(token) = super::palw_claim_local_legacy_terminal_v18::prepare_retired_exec_event_coverage_v24(
            &b.state, b.params, ctx, terminal, birth.0,
        )? {
            if token.claim_birth_v24() != *birth {
                return Err(bad("retired EXEC event token changed its actual birth"));
            }
            retired_exec.insert(*birth, token);
        }
    }
    // Memoize the strict native ledger and per-consumer historical material
    // traversal once per actual current subject. These private borrowed tokens
    // are not supplied by a producer and grant no payment/credit standing.
    let mut subjects = BTreeSet::new();
    let mut cert_targets = BTreeSet::new();
    for e in &b.entries {
        match e {
            PalwDeltaEntryV2::Claim { key, .. } => {
                subjects.insert(*key);
            }
            PalwDeltaEntryV2::KernelRouteRow { table, key, old, new }
                if *table != crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1 =>
            {
                if let NativeHistoricalRoleV21::Work(Work::ChainClaim { claim }) =
                    historical_role_v21(Owner::KernelRoute, *table, key, old.as_deref(), new.as_deref())?
                {
                    subjects.insert(claim);
                    cert_targets.insert(claim);
                }
            }
            _ => {}
        }
    }
    let mut current_material = BTreeMap::new();
    let mut retired_cert = BTreeMap::new();
    for id in subjects {
        if let Some(current) = b.state.claims.get(&id) {
            if !super::palw_claim_local_legacy_terminal_v18::claim_native_ledger_ownership_ready_in_fold_v21(b, ctx, id, current)? {
                continue;
            }
            if let Some(token) =
                super::palw_claim_local_retired_dependency_material_v24::prepare_consumer_material_coverage_v24(b, ctx, id, current)?
            {
                if !token.covers_claim_v24(ctx, id, current) {
                    return Err(bad("native consumer material token changed actual subject"));
                }
                current_material.insert(claim_birth_v21(id, current), token);
            }
        } else if cert_targets.contains(&id) {
            retired_cert.insert(
                id,
                super::palw_claim_local_legacy_terminal_v18::chain_subject_checkpoint_overlay_ready_in_fold_v21(b, ctx, id)?,
            );
        }
    }
    let (mut events, mut covered, mut first_gap) = (0u64, 0u64, None);
    for (entry_index, entry) in b.entries.iter().enumerate() {
        let role = match entry {
            PalwDeltaEntryV2::Claim { key, .. } => NativeHistoricalRoleV21::Work(Work::ChainClaim { claim: *key }),
            PalwDeltaEntryV2::ExecV2Row { table, key, old, new } => {
                historical_role_v21(Owner::ExecV2, *table, key, old.as_deref(), new.as_deref())?
            }
            PalwDeltaEntryV2::KernelRouteRow { table, key, old, new }
                if *table != crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1 =>
            {
                historical_role_v21(Owner::KernelRoute, *table, key, old.as_deref(), new.as_deref())?
            }
            _ => continue, // Same event numbering as F1/Source10/20, no scalar filtering.
        };
        let ordinal = events;
        events = events.checked_add(1).ok_or_else(|| bad("native event coverage ordinal overflow"))?;
        let gap = match role {
            NativeHistoricalRoleV21::CurrentOnly => None,
            NativeHistoricalRoleV21::Work(Work::ChainClaim { claim }) => {
                if chain_event_covered_v21(b, ctx, claim, entry, entry_index, &removed, &current_material, &retired_cert)? {
                    None
                } else {
                    Some(NativeCoverageGapV21::ChainCurrentOrRemoval)
                }
            }
            NativeHistoricalRoleV21::Work(Work::KernelClaim { claim }) => {
                if matches!(entry,PalwDeltaEntryV2::KernelRouteRow{table,new:None,..}if *table==misaka_palw_kernel::rows::TABLE_CLAIMS_V1)
                {
                    Some(NativeCoverageGapV21::KernelLifecycle)
                } else if super::palw_claim_local_kernel_current_birth_v9::kernel_current_checkpoint_overlay_ready_v18(
                    &b.state, claim,
                )? {
                    None
                } else {
                    Some(NativeCoverageGapV21::KernelLifecycle)
                }
            }
            NativeHistoricalRoleV21::Work(work @ Work::ExecSession { .. })
            | NativeHistoricalRoleV21::Work(work @ Work::ExecSlice { .. }) => {
                let PalwDeltaEntryV2::ExecV2Row { table, key, old, new } = entry else {
                    return Err(bad("EXEC role belongs to another actual journal owner"));
                };
                let live = super::palw_claim_local_exec_archive_v10::covers_native_event_v21(
                    &b.state,
                    *ctx,
                    ordinal,
                    *table,
                    key,
                    old.as_deref(),
                    new.as_deref(),
                )?;
                let retired = if !live && new.is_none() {
                    if let Some(old) = old.as_deref() {
                        let root = match work {
                            Work::ExecSession { root } | Work::ExecSlice { root, .. } => root,
                            _ => unreachable!("actual EXEC role"),
                        };
                        let lo = (root, Hash64::default(), 0, 0);
                        let hi = (root, Hash64::from_bytes([u8::MAX; 64]), u64::MAX, u64::MAX);
                        let mut matches = 0u64;
                        for ((id, ..), token) in retired_exec.range(lo..=hi) {
                            if token.covers_event_v24(ctx, *id, *table, key, old)? {
                                matches = matches.checked_add(1).ok_or_else(|| bad("retired EXEC source multiplicity overflow"))?;
                            }
                        }
                        for ((id, ..), observed) in b.claim_local_retired_exec_events_v24.range(lo..=hi) {
                            if observed.token_v24().covers_event_v24(ctx, *id, *table, key, old)? {
                                matches = matches.checked_add(1).ok_or_else(|| bad("retired EXEC source multiplicity overflow"))?;
                            }
                        }
                        matches == 1 // ambiguity never borrows another source's deletion coverage
                    } else {
                        false
                    }
                } else {
                    false
                };
                if live || retired { None } else { Some(NativeCoverageGapV21::ExecLifecycle) }
            }
            NativeHistoricalRoleV21::Work(Work::NativeMiningClaim39 { claim }) => {
                if mining.map(|m| m.covers_event_v21(*ctx, ordinal, entry)).transpose()?.unwrap_or(false)
                    || super::palw_claim_local_native_mining_birth_v20::covers_native_current_event_v21(
                        &b.state, claim, *ctx, ordinal, entry,
                    )?
                {
                    None
                } else {
                    Some(NativeCoverageGapV21::MiningLifecycle)
                }
            }
            NativeHistoricalRoleV21::Work(_) => return Err(bad("unallocated native event work role")),
            NativeHistoricalRoleV21::CarrierWinner => {
                if super::palw_claim_local_carrier_lineage_v9::covers_native_winner_event_v21(&b.state, *ctx, entry)? {
                    None
                } else {
                    Some(NativeCoverageGapV21::CarrierLineage)
                }
            }
            NativeHistoricalRoleV21::MiningHold | NativeHistoricalRoleV21::MiningAnchor => {
                if mining.map(|m| m.covers_event_v21(*ctx, ordinal, entry)).transpose()?.unwrap_or(false) {
                    None
                } else {
                    Some(NativeCoverageGapV21::MiningHold)
                }
            }
        };
        if let Some(gap) = gap {
            if first_gap.is_none() {
                first_gap = Some((ordinal, gap));
            }
        } else {
            covered += 1;
        }
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        NativeCapturedEventAuditV21 {
            _current_material: current_material,
            _retired_claim: &b.claim_local_retired_claim_events_v25,
            _retired_exec: retired_exec,
            _retired_observed: &b.claim_local_retired_exec_events_v24,
            _entries: &b.entries,
            _mining: mining,
            point: *ctx,
            events,
            covered,
            first_gap,
        },
        lease,
    ))
}

/// Data-only boundary for checking the existing post-checkpoint suffix. This
/// does not certify owner/economic scope and cannot select a no-F1 route.
/// Only a complete actual event audit can record this boundary; final route
/// activation still needs its independently verified native ownership seal.
pub(super) struct NativeEventTailBoundaryV21 {
    point: PalwBlockContextV2,
    end: usize,
}
impl NativeCapturedEventAuditV21<'_> {
    pub(super) fn tail_boundary_v21(&self) -> Result<NativeRetainedV1<NativeEventTailBoundaryV21>, PalwStateV2Error> {
        if self.first_gap.is_some() {
            return Err(PalwStateV2Error::NativeContextUnavailable(
                crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
            ));
        }
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<NativeEventTailBoundaryV21>(1).map_err(PalwStateV2Error::NativeResource)?;
        let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(NativeEventTailBoundaryV21 { point: self.point, end: self._entries.len() }, lease))
    }
}
impl NativeEventTailBoundaryV21 {
    pub(super) fn verify_suffix_v21(&self, b: &TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
        if self.point != *ctx || b.claim_local_lock_point_v6 != Some(*ctx) || self.end > b.entries.len() {
            return Err(PalwStateV2Error::NativeContextUnavailable(
                crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
            ));
        }
        for e in &b.entries[self.end..] {
            match e {
                PalwDeltaEntryV2::Claim { .. } | PalwDeltaEntryV2::ExecV2Row { .. } => {
                    return Err(PalwStateV2Error::NativeContextUnavailable(
                        crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
                    ));
                }
                PalwDeltaEntryV2::KernelRouteRow { table, key, old, new }
                    if *table != crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1 =>
                {
                    // Exact existing tail writers: protocol-owned checkpoint and compact
                    // closed/pending summaries. Unknown future tables must be integrated
                    // explicitly rather than silently becoming an unexamined suffix.
                    if !matches!(
                        *table,
                        crate::palw_native39_settlement_checkpoint_v1::NATIVE39_SETTLEMENT_CHECKPOINT_TABLE_V1
                            | crate::palw_native_closed_facts_v1::NATIVE_CLOSED_FACTS_TABLE_V1
                            | super::palw_legacy_pending_facts_v1::LEGACY_PENDING_FACT_TABLE_V1
                    ) || !matches!(
                        historical_role_v21(Owner::KernelRoute, *table, key, old.as_deref(), new.as_deref())?,
                        NativeHistoricalRoleV21::CurrentOnly
                    ) {
                        return Err(PalwStateV2Error::NativeContextUnavailable(
                            crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
                        ));
                    }
                }
                _ => {} // Unchanged scalar journal grammar; no scalar is projected away.
            }
        }
        Ok(())
    }
}

/// Construction inventory only. Possession does not select a checkpoint route,
/// authorize credit, or replace the complete native-event ownership check.
/// Every target originates in an actual signed OAR2 receipt, not an extras flag.
pub(super) struct NativeMixedInitialPublicationV21 {
    point: PalwBlockContextV2,
    claims: Vec<Hash64>,
}
impl NativeMixedInitialPublicationV21 {
    pub(super) fn has_initial_originals_v24(&self) -> bool {
        !self.claims.is_empty()
    }
}
pub(super) fn capture_initial_mixed_sources_v21(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
) -> Result<NativeRetainedV1<NativeMixedInitialPublicationV21>, PalwStateV2Error> {
    use super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1 as finance;
    if b.claim_local_lock_point_v6 != Some(*ctx) {
        return Err(bad("mixed publication lacks executing point"));
    }
    // Hold each independently checked token through Source15 publication. Their
    // own tickets remain live; this ticket covers only the two outer vectors.
    let n = objects.iter().filter(|o| matches!(o, PalwConsensusObjectV2::FreePromptCommitted { .. })).count();
    let fresh = b.entries.iter().filter(|e| matches!(e, PalwDeltaEntryV2::Claim { old: None, new: Some(_), .. })).count();
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Hash64>(n as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(n as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<Hash64>(fresh as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(fresh as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<NativeRetainedV1<finance::NativeMixedArchiveCapacityV21>>(n as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<NativeMixedInitialPublicationV21>(1).map_err(PalwStateV2Error::NativeResource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut claims = Vec::with_capacity(n);
    let mut capacities = Vec::with_capacity(n);
    let mut seen = BTreeSet::new();
    for o in objects {
        let PalwConsensusObjectV2::FreePromptCommitted { claim, .. } = o else { continue };
        let Some(cap) = finance::verified_mixed_archive_capacity_v21(&b.state, *claim, *ctx)? else { continue };
        // Repeated outer objects cannot enlarge the signed native contribution.
        if !seen.insert(*claim) {
            return Err(bad("mixed publication repeats actual original"));
        }
        claims.push(*claim);
        capacities.push(cap);
    }
    // Every genuinely fresh scoped native source needs its immutable birth
    // cohort, not only payer-profiled FP: ordinary/unwrapped FP and Attempt
    // cannot rely on a later census that has already advanced past this birth.
    let mut targets = Vec::with_capacity(fresh);
    let mut targeted = BTreeSet::new();
    for e in &b.entries {
        let PalwDeltaEntryV2::Claim { key, old: None, new: Some(c) } = e else { continue };
        if c.execution_root == Hash64::default() || !targeted.insert(*key) {
            continue;
        }
        if super::palw_claim_local_source_admission_v8::for_accepting_point_v8(&b.state, *key, *ctx)?.is_some() {
            targets.push(*key);
        }
    }
    if !targets.is_empty() {
        // Whole actual batch was observed once. These are real native birth IDs;
        // no projected accepted membership or expected scalar mints a snapshot.
        let report = super::palw_claim_local_execution_cohort_archive_v15::capture_postblock_cohorts_v15(b, *ctx, &targets)?;
        // An unsupported ordinary historical source may retain its old F1 path;
        // it must not turn somebody else's valid cohort into a fabricated complete
        // snapshot. The strict native owner getter independently checks each target.
        // New Mixed sources cannot proceed with missing own context: the native
        // initializer/preflight must establish it before any financial writes.
        if report.missing_legacy_census != 0 && !claims.is_empty() {
            for cap in &capacities {
                let claim = cap.claim_v21();
                if cap.owner18_envelope_v21().expects_modern_alias_v18() {
                    let source = super::palw_claim_local_source_admission_v8::for_accepting_point_v8(&b.state, claim, *ctx)?
                        .ok_or_else(|| bad("mixed actual source vanished before birth snapshot"))?;
                    if super::palw_claim_local_execution_cohort_archive_v15::alias_original_snapshot_v15(
                        &b.state,
                        claim,
                        source.original_claim(),
                    )?
                    .is_none()
                    {
                        return Err(PalwStateV2Error::NativeContextUnavailable(
                            crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
                        ));
                    }
                }
            }
        }
    }
    for cap in &capacities {
        super::palw_claim_local_legacy_terminal_v18::capture_initial_mixed_ledger_v21(b, ctx, cap)?;
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(NativeMixedInitialPublicationV21 { point: *ctx, claims }, lease))
}
/// Call only after the actual Current/Held/cert writers and complete scoped
/// event readiness check. Finance independently rechecks all21 and15/18 bodies;
/// this inventory conveys no source-ready or no-F1 authority by itself.
pub(super) fn finalize_initial_mixed_sources_v21(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    publication: &NativeMixedInitialPublicationV21,
) -> Result<(), PalwStateV2Error> {
    if publication.point != *ctx || b.claim_local_lock_point_v6 != Some(*ctx) {
        return Err(bad("mixed finalization point differs"));
    }
    for claim in &publication.claims {
        super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::mark_original_archive_constructed_v1(
            b, ctx, *claim,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_native_role_partition_preserves_owner_namespace_and_malformed_refusal() {
        let id = Hash64::from_u64_word(21);
        let raw = id.as_byte_slice();
        assert_eq!(
            historical_role_v21(Owner::ChainClaims, 0, raw, None, None).unwrap(),
            NativeHistoricalRoleV21::Work(Work::ChainClaim { claim: id })
        );
        assert_eq!(
            historical_role_v21(Owner::ExecV2, PALW_EXEC_V2_TABLE_ROOTS_V1, raw, None, None).unwrap(),
            NativeHistoricalRoleV21::Work(Work::ExecSession { root: id })
        );
        assert_eq!(
            historical_role_v21(Owner::KernelRoute, misaka_palw_kernel::rows::TABLE_CLAIMS_V1, raw, None, None).unwrap(),
            NativeHistoricalRoleV21::Work(Work::KernelClaim { claim: id })
        );
        let mut mining = [0u8; 66];
        mining[..2].copy_from_slice(&[39, crate::palw_native_mining_v39::MINING_CLAIM_KIND_V39]);
        mining[2..].copy_from_slice(raw);
        assert_eq!(
            historical_role_v21(Owner::KernelRoute, crate::palw_native_mining_v39::MINING_TABLE_V39, &mining, None, None).unwrap(),
            NativeHistoricalRoleV21::Work(Work::NativeMiningClaim39 { claim: id })
        );
        assert!(historical_role_v21(Owner::ChainClaims, 0, &raw[..63], None, None).is_err());
        assert!(historical_role_v21(Owner::ExecV2, PALW_EXEC_V2_TABLE_SLICES_V1, raw, None, None).is_err());
        // Same bytes in an ordinary current native table acquire no historical work
        // identity from coincidence. This classifier grants no credit/approval.
        assert_eq!(historical_role_v21(Owner::KernelRoute, 250, raw, None, None).unwrap(), NativeHistoricalRoleV21::CurrentOnly);
    }
}
