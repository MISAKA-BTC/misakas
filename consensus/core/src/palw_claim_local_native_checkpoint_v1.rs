//! Native checkpoint vertices use actual retained admission/journal points.
//! Session birth never moves forward to accommodate later slice dependencies.
use super::*;
use crate::palw_native_resource_v1::{NativeDecodeErrorV1 as E, NativeDemandLeaseV1};
use crate::palw_work_slice_v2::{
    PalwWorkPrefixStageV2 as Prefix, PalwWorkRootPhaseV2 as RootPhase, PalwWorkSliceStageV2 as SlicePhase,
};
#[derive(Clone, Copy)]
pub(super) struct Birth<'a> {
    pub point: PalwBlockContextV2,
    pub frame: &'a Vec<u8>,
    pub full_frame: bool,
    pub native_coordinate: bool,
    pub scoped_exec: bool,
}
pub(super) struct Context<'a> {
    pub births: BTreeMap<PalwClaimLocalWorkRefV1, Birth<'a>>,
    settlements: BTreeMap<PalwClaimLocalWorkRefV1, (PalwClaimLocalNativeEventV1, &'a Vec<u8>, bool)>,
    _event_leases: Vec<Option<NativeDemandLeaseV1>>,
    latest_by_work: BTreeMap<PalwClaimLocalWorkRefV1, Birth<'a>>,
    mining_bounds: BTreeMap<Hash64, u64>,
    mining_claims: BTreeMap<
        Hash64,
        (u64, usize, NativeRetainedV1<(misaka_palw_kernel::hash::Digest, crate::palw_native_mining_v39::NativeMiningClaimRecordV39)>),
    >,
}
fn fault(e: E<std::io::Error>) -> PalwStateV2Error {
    match e {
        E::Resource(r) => PalwStateV2Error::NativeResource(r),
        E::Decode(e) => bad(&e.to_string()),
    }
}
fn decode<T: borsh::BorshDeserialize + borsh::BorshSerialize + crate::palw_native_resource_v1::NativeWireScanV1>(
    bytes: &[u8],
) -> Result<NativeRetainedV1<T>, PalwStateV2Error> {
    crate::palw_claim_local_v1::decode_claim_local_value_v1(bytes).map_err(fault)
}
/// Sidecar effects retain their actual subject even when the base row does
/// not change. This selects context only; it never decides approval itself.
pub(super) fn context<'a>(state: &'a PalwChainStateV2) -> Result<Context<'a>, PalwStateV2Error> {
    let mut out = Context {
        births: BTreeMap::new(),
        settlements: BTreeMap::new(),
        _event_leases: Vec::new(),
        latest_by_work: BTreeMap::new(),
        mining_bounds: BTreeMap::new(),
        mining_claims: BTreeMap::new(),
    };
    let anchors = crate::palw_native39_frontier_v1::native39_frontier_anchors_v1(state)?;
    for (frame_key, bytes) in rewards::frames(state) {
        // Preserve the released malformed-frame/read semantics. Until an
        // actual validated-frame cache is installed, complete F integrity is
        // checked here; the borrowed prefix is not a validation substitute.
        let frame = decode::<PalwClaimLiabilityFrameV1>(bytes)?;
        frame.validate_context_v1().map_err(checkpoint_err)?;
        let block = Hash64::from_bytes(frame_key[1..].try_into().map_err(|_| bad("rooted native frame key width"))?);
        let mut legacy_exec = false;
        let found = super::super::palw_claim_local_native_frame_events_v10::visit_rooted_native_events_v10(
            state,
            block,
            |point, ordinal, event| {
                let ordinal = usize::try_from(ordinal).map_err(|_| bad("native event ordinal host width"))?;
                let key = event.key.bytes;
                let old = event.old.map(|x| x.bytes);
                let new = event.new.map(|x| x.bytes);
                use super::super::palw_claim_local_native_event_coverage_v21::{NativeHistoricalRoleV21, historical_role_v21};
                let role = historical_role_v21(event.owner, event.table, key, old, new)?;
                let owner = match role {
                    NativeHistoricalRoleV21::Work(work) => {
                        if matches!(work, PalwClaimLocalWorkRefV1::ExecSession { .. } | PalwClaimLocalWorkRefV1::ExecSlice { .. }) {
                            legacy_exec = true;
                        }
                        Some(work)
                    }
                    _ => None,
                };
                if let Some(owner) = owner
                    && out.latest_by_work.get(&owner).is_none_or(|p| p.point.blue_score < point.blue_score)
                {
                    out.latest_by_work
                        .insert(owner, Birth { point, frame: bytes, full_frame: true, native_coordinate: false, scoped_exec: false });
                }
                use crate::palw_native_mining_v39::{
                    MINING_CLAIM_KIND_V39, MINING_JOB_HOLD_KIND_V39, MINING_TABLE_V39, NativeMiningClaimRecordV39,
                };
                if event.owner == PalwClaimLocalRowOwnerV1::KernelRoute && event.table == MINING_TABLE_V39 {
                    if key.starts_with(&[39, MINING_JOB_HOLD_KIND_V39])
                        && let Some(new) = new
                    {
                        let (claim, until): (misaka_palw_kernel::hash::Digest, u64) =
                            borsh::from_slice(new).map_err(|_| bad("native mining retained hold row"))?;
                        let claim = Hash64::from_bytes(claim);
                        out.mining_bounds.entry(claim).and_modify(|old| *old = (*old).max(until)).or_insert(until);
                    }
                    if key.starts_with(&[39, MINING_CLAIM_KIND_V39]) {
                        let Some(new) = new else { return Ok(()) };
                        let row = decode::<(misaka_palw_kernel::hash::Digest, NativeMiningClaimRecordV39)>(new)?;
                        if row.0 != row.1.root() || key.len() != 66 || key[2..] != row.1.id {
                            return Err(bad("native mining claim key or root"));
                        }
                        let id = Hash64::from_bytes(row.1.id);
                        let _key = crate::palw_native_resource_v1::reserve_native_requests_v1(key.len() as u64)
                            .map_err(PalwStateV2Error::NativeResource)?;
                        let live = state.kernel_route.as_ref().is_some_and(|r| r.aux.contains_key(&(MINING_TABLE_V39, key.to_vec())));
                        if live || anchors.iter().any(|(_, a)| a.claim == id) {
                            if old.is_none() {
                                if row.1.admitted_daa != point.daa_score {
                                    return Err(bad("native mining birth changes admitted clock"));
                                }
                                if out
                                    .births
                                    .insert(
                                        PalwClaimLocalWorkRefV1::NativeMiningClaim39 { claim: id },
                                        Birth { point, frame: bytes, full_frame: true, native_coordinate: false, scoped_exec: false },
                                    )
                                    .is_some()
                                {
                                    return Err(bad("native mining birth duplicated"));
                                }
                            }
                            let newer = out
                                .mining_claims
                                .get(&id)
                                .is_none_or(|(blue, old_ordinal, _)| (*blue, *old_ordinal) < (point.blue_score, ordinal));
                            if newer {
                                out.mining_claims.insert(id, (point.blue_score, ordinal, row));
                            }
                        }
                        return Ok(());
                    }
                }
                if old.is_some() {
                    return Ok(());
                }
                let Some(new) = new else { return Ok(()) };
                let candidate = match (event.owner, event.table) {
                    (PalwClaimLocalRowOwnerV1::KernelRoute, misaka_palw_kernel::rows::TABLE_CLAIMS_V1) => {
                        let id: misaka_palw_kernel::hash::Digest =
                            borsh::from_slice(key).map_err(|_| bad("native kernel birth key"))?;
                        let id = Hash64::from_bytes(id);
                        let _key = crate::palw_native_resource_v1::reserve_native_requests_v1(64)
                            .map_err(PalwStateV2Error::NativeResource)?;
                        let Some(current) = rewards::kernel_claim_row(state, &id) else { return Ok(()) };
                        let original = decode::<misaka_palw_kernel::ledger::ClaimRowV1>(new)?;
                        let current = decode::<misaka_palw_kernel::ledger::ClaimRowV1>(current)?;
                        (current.committed_daa == original.committed_daa && original.committed_daa == point.daa_score)
                            .then_some(PalwClaimLocalWorkRefV1::KernelClaim { claim: id })
                    }
                    (PalwClaimLocalRowOwnerV1::ExecV2, PALW_EXEC_V2_TABLE_ROOTS_V1) => {
                        let id: Hash64 = borsh::from_slice(key).map_err(|_| bad("native session birth key"))?;
                        let Some(current) = state.exec_v2.roots.get(&id) else { return Ok(()) };
                        let original = decode::<crate::palw_work_slice_v2::PalwWorkRootV2>(new)?;
                        if let Some((birth, born_ordinal)) = super::super::palw_claim_local_exec_archive_v10::current_birth_event_v10(
                            state,
                            PalwClaimLocalWorkRefV1::ExecSession { root: id },
                        )? && (point, ordinal as u64) != (birth, born_ordinal)
                        {
                            return Ok(());
                        }
                        (current.opened_daa == original.opened_daa
                            && current.anchor == original.anchor
                            && original.opened_daa == point.daa_score)
                            .then_some(PalwClaimLocalWorkRefV1::ExecSession { root: id })
                    }
                    (PalwClaimLocalRowOwnerV1::ExecV2, PALW_EXEC_V2_TABLE_SLICES_V1) => {
                        let (id, index): (Hash64, u32) = borsh::from_slice(key).map_err(|_| bad("native slice birth key"))?;
                        let Some(current) = state.exec_v2.slices.get(&(id, index)) else { return Ok(()) };
                        let original = decode::<crate::palw_work_slice_v2::PalwWorkSliceRowV2>(new)?;
                        if let Some((birth, born_ordinal)) = super::super::palw_claim_local_exec_archive_v10::current_birth_event_v10(
                            state,
                            PalwClaimLocalWorkRefV1::ExecSlice { root: id, index },
                        )? && (point, ordinal as u64) != (birth, born_ordinal)
                        {
                            return Ok(());
                        }
                        (current.slice_id == original.slice_id
                            && current.accepted_daa == original.accepted_daa
                            && original.accepted_daa == point.daa_score)
                            .then_some(PalwClaimLocalWorkRefV1::ExecSlice { root: id, index })
                    }
                    _ => None,
                };
                if let Some(work) = candidate
                    && out
                        .births
                        .insert(work, Birth { point, frame: bytes, full_frame: true, native_coordinate: false, scoped_exec: false })
                        .is_some()
                {
                    return Err(bad("native admission birth duplicated in retained history"));
                }
                Ok(())
            },
        )?;
        if !found {
            return Err(bad("enumerated rooted native frame disappeared"));
        }
        // Preserve full integrity above, but avoid the SECOND literal prior
        // world/EXEC reconstruction on frames with no native EXEC event.
        if legacy_exec && super::super::palw_claim_local_exec_archive_v10::legacy_exec_frame_needed_v10(state, block)? {
            let events = frame.native_credit_events_v1().map_err(checkpoint_err)?;
            let (events, lease) = events.into_parts_v1();
            out._event_leases.push(lease);
            for event in events {
                let PalwClaimLocalWorkRefV1::ExecSettlement { root, .. } = event.work else { continue };
                if state.exec_v2.roots.get(&root).is_none_or(|r| r.opened_daa > event.point.daa_score) {
                    continue;
                }
                if let Some((birth, born_ordinal)) = super::super::palw_claim_local_exec_archive_v10::current_birth_event_v10(
                    state,
                    PalwClaimLocalWorkRefV1::ExecSession { root },
                )? && (event.point.blue_score < birth.blue_score || (event.point == birth && event.journal_ordinal <= born_ordinal))
                {
                    continue;
                }
                if super::super::palw_claim_local_exec_archive_v10::has_settlement_v10(
                    state,
                    event.work,
                    event.point,
                    event.journal_ordinal,
                )? {
                    continue;
                }
                if out.settlements.insert(event.work, (event, bytes, false)).is_some() {
                    return Err(bad("native settlement event duplicated"));
                }
            }
        }
    }
    // Actual native mining receipt supersedes ONLY its own F1 source. Legacy
    // sources remain byte-compatible and keep the old original-frame path.
    super::super::palw_claim_local_native_mining_birth_v20::visit_receipts_v20(
        state,
        |id, point, until, updated, updated_ordinal, latest, raw| {
            let work = PalwClaimLocalWorkRefV1::NativeMiningClaim39 { claim: id };
            if out.births.get(&work).is_some_and(|old| old.point != point) {
                return Err(bad("scoped mining birth contradicts actual historical point"));
            }
            if out.mining_bounds.get(&id).is_some_and(|old| *old != until) {
                return Err(bad("scoped mining hold differs from actual original native journal"));
            }
            let updated_ordinal =
                usize::try_from(updated_ordinal).map_err(|_| bad("scoped mining updated native ordinal exceeds host"))?;
            if let Some((old_blue, old_ordinal, old)) = out.mining_claims.get(&id) {
                if !super::super::palw_claim_local_native_mining_birth_v20::same_native_source_v20(&old.1, latest)
                    .map_err(super::super::palw_claim_local_paid_archive_v2::state_error)?
                {
                    return Err(bad("scoped mining source changes historical immutable native identity"));
                }
                if old.1 != *latest && (*old_blue, *old_ordinal) >= (updated.blue_score, updated_ordinal) {
                    return Err(bad("scoped mining current contradicts same/newer actual historical native event"));
                }
            }
            use crate::palw_native_resource_v1::{NativeHeapV1, TirNativeRequestsV1, reserve_native_requests_v1};
            let mut q = TirNativeRequestsV1::new();
            let h = latest.native_heap_v1("scoped mining checkpoint native current");
            q.add_v1(h.bytes().ok_or(PalwStateV2Error::NativeResource(h))?).map_err(PalwStateV2Error::NativeResource)?;
            q.exact_v1::<(misaka_palw_kernel::hash::Digest, crate::palw_native_mining_v39::NativeMiningClaimRecordV39)>(1)
                .map_err(PalwStateV2Error::NativeResource)?;
            let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
            let row = NativeRetainedV1::from_owner_reserved_v1((latest.root(), latest.clone()), lease);
            out.births.insert(work, Birth { point, frame: raw, full_frame: false, native_coordinate: true, scoped_exec: false });
            out.mining_bounds.insert(id, until);
            out.mining_claims.insert(id, (updated.blue_score, updated_ordinal, row));
            Ok(())
        },
    )?;
    if let Some(route) = &state.kernel_route {
        use crate::palw_native_mining_v39::{MINING_CLAIM_KIND_V39, MINING_TABLE_V39, NativeMiningClaimRecordV39};
        for ((table, key), raw) in &route.aux {
            if *table != MINING_TABLE_V39 || !key.starts_with(&[39, MINING_CLAIM_KIND_V39]) {
                continue;
            }
            let current = decode::<(misaka_palw_kernel::hash::Digest, NativeMiningClaimRecordV39)>(raw)?;
            let id = Hash64::from_bytes(current.1.id);
            if current.0 != current.1.root() || key.len() != 66 || key[2..] != current.1.id {
                return Err(bad("current native mining claim key or root"));
            }
            if out.mining_claims.get(&id).is_none_or(|(_, _, saved)| saved.0 != current.0)
                || !out.births.contains_key(&PalwClaimLocalWorkRefV1::NativeMiningClaim39 { claim: id })
            {
                return Err(bad("native mining liability lacks its retained authenticated admission frame"));
            }
        }
    }
    // Only actual current physical EXEC sources are visited. These typed B
    // envelopes are explicitly scoped status data, not F1/full-state frames.
    super::super::palw_claim_local_exec_archive_v10::visit_births_v10(state, |view, raw| {
        let work = view.work();
        let point = view.point();
        if out.births.get(&work).is_some_and(|prior| prior.point != point) {
            return Err(bad("scoped EXEC birth contradicts actual legacy birth"));
        }
        out.births.insert(work, Birth { point, frame: raw, full_frame: false, native_coordinate: false, scoped_exec: true });
        Ok(())
    })?;
    super::super::palw_claim_local_exec_archive_v10::visit_settlement_events_v10(state, |event, raw| {
        let (event, lease) = event.into_parts_v1();
        out._event_leases.push(lease);
        if out.settlements.insert(event.work, (event, raw, true)).is_some() {
            return Err(bad("scoped EXEC settlement duplicates an uncovered legacy event"));
        }
        Ok(())
    })?;
    // A real current native admission coordinate replaces fullF ONLY for this
    // Kernel birth. It is explicitly a typed coordinate, not a full frame.
    super::super::palw_claim_local_kernel_current_birth_v9::visit_current_birth_material_v9(state, |id, point, raw| {
        let work = PalwClaimLocalWorkRefV1::KernelClaim { claim: id };
        if out.births.get(&work).is_some_and(|old| old.point != point) {
            return Err(bad("scoped Kernel birth contradicts historical actual point"));
        }
        out.births.insert(work, Birth { point, frame: raw, full_frame: false, native_coordinate: true, scoped_exec: false });
        Ok(())
    })?;
    if let Some(route) = &state.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.len() != 65 || key[0] != super::super::PALW_COMPACT_KERNEL_ORIGIN_PREFIX_V1 {
                continue;
            }
            let id = Hash64::from_bytes(key[1..].try_into().expect("compact kernel key"));
            let origin = super::super::palw_claim_local_kernel_origins_v1::read(state, id)?.expect("existing compact origin");
            let work = PalwClaimLocalWorkRefV1::KernelClaim { claim: id };
            if let Some(birth) = out.births.get(&work) {
                if birth.point != origin.birth {
                    return Err(bad("compact kernel origin differs from retained actual birth"));
                }
            } else {
                out.births.insert(
                    work,
                    Birth { point: origin.birth, frame: raw, full_frame: false, native_coordinate: false, scoped_exec: false },
                );
            }
        }
    }
    Ok(out)
}
fn retain_material(record: &mut PalwClaimLocalRecordV1, bytes: Vec<u8>, material: &mut BTreeMap<Hash64, Vec<u8>>) {
    let id = palw_liability_material_id_v1(&bytes);
    record.material.insert(id);
    material.entry(id).or_insert(bytes);
}
fn retain_birth_material_v10(
    state: &PalwChainStateV2,
    record: &mut PalwClaimLocalRecordV1,
    birth: Birth<'_>,
    material: &mut BTreeMap<Hash64, Vec<u8>>,
) -> Result<(), PalwStateV2Error> {
    if birth.scoped_exec {
        let found = super::super::palw_claim_local_exec_archive_v10::visit_material_v10(state, record.work, |raw| {
            retain_material(record, raw.to_vec(), material);
            Ok(())
        })?;
        if !found {
            return Err(bad("scoped EXEC work lost its actual source material"));
        }
    } else {
        retain_material(record, birth.frame.clone(), material);
    }
    Ok(())
}
fn with_birth(work: PalwClaimLocalWorkRefV1, birth: Birth<'_>, execution_root: Hash64) -> PalwClaimLocalRecordV1 {
    PalwClaimLocalRecordV1 {
        work,
        accepted_daa: birth.point.daa_score,
        accepted_blue_score: birth.point.blue_score,
        accepting_block: birth.point.block,
        anchors: false,
        inputs: BTreeSet::new(),
        execution_root,
        reservations: BTreeSet::new(),
        final_claim: false,
        certified: false,
        voided: false,
        liability_closed: false,
        material: BTreeSet::new(),
    }
}
fn born<'a>(c: &Context<'a>, work: PalwClaimLocalWorkRefV1) -> Result<Birth<'a>, PalwStateV2Error> {
    c.births.get(&work).copied().ok_or_else(|| bad("native liability lacks its retained authenticated admission frame"))
}
pub(super) fn attach(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    context: &Context<'_>,
    records: &mut BTreeMap<PalwClaimLocalWorkRefV1, PalwClaimLocalRecordV1>,
    material: &mut BTreeMap<Hash64, Vec<u8>>,
) -> Result<(), PalwStateV2Error> {
    use PalwClaimLocalWorkRefV1 as W;
    // Kernel birth blue score and accepting block come from the executed journal,
    // never a default zero or the EXEC carrier's unrelated position.
    for (work, record) in records.iter_mut().filter(|(w, _)| matches!(w, W::KernelClaim { .. })) {
        let birth = born(context, *work)?;
        record.accepted_daa = birth.point.daa_score;
        record.accepted_blue_score = birth.point.blue_score;
        record.accepting_block = birth.point.block;
        if !record.liability_closed {
            if birth.native_coordinate {
                let W::KernelClaim { claim } = *work else { unreachable!() };
                let found = super::super::palw_claim_local_kernel_current_birth_v9::visit_current_checkpoint_material_v10(
                    state,
                    claim,
                    |raw| {
                        retain_material(record, raw.to_vec(), material);
                        Ok(())
                    },
                )?;
                if !found {
                    return Err(bad("scoped Kernel birth lost its current native material"));
                }
            } else {
                if !birth.full_frame {
                    return Err(bad("open kernel liability lacks its full admission material"));
                }
                retain_material(record, birth.frame.clone(), material);
            }
        }
    }
    // A chain original does not causally consume evidence first accepted by a
    // later slice. The terminal settlement vertex below consumes those rows.
    for record in records.values_mut().filter(|r| matches!(r.work, W::ChainClaim { .. })) {
        if !record.liability_closed {
            let W::ChainClaim { claim } = record.work else { unreachable!() };
            let current = state.claims.get(&claim).ok_or_else(|| bad("scoped Chain record lost actual claim"))?;
            super::super::palw_claim_local_source_admission_v8::visit_checkpoint_material_v10(state, claim, current, |raw| {
                retain_material(record, raw.to_vec(), material);
                Ok(())
            })?;
            super::super::palw_claim_local_execution_cohort_archive_v15::visit_checkpoint_material_v15(
                state,
                claim,
                current,
                |raw| {
                    retain_material(record, raw.to_vec(), material);
                    Ok(())
                },
            )?;
            super::super::palw_claim_local_legacy_terminal_v18::visit_current_checkpoint_material_v18(
                state,
                params,
                claim,
                current,
                |raw| {
                    retain_material(record, raw.to_vec(), material);
                    Ok(())
                },
            )?;
            // Attach exact retired historical dependencies to THIS actual live
            // consumer only. Retired sources stay off the raw-ID WorkRef graph.
            let _complete = super::super::palw_claim_local_retired_dependency_material_v24::visit_consumer_material_v24(
                state,
                params,
                claim,
                current,
                |raw| {
                    let mid = palw_liability_material_id_v1(raw);
                    record.material.insert(mid);
                    // BTreeMap::entry only looks up here; the already admitted
                    // constructor owns the node/copy demand before lazy insertion.
                    material.entry(mid).or_insert_with(|| raw.to_vec());
                    Ok(())
                },
            )?;
            // Missing remains the native recorded-readiness Hold. The separate
            // no-F1 coverage seal must require complete=true before arming.
        }
        record.inputs.retain(|input| match input {
            W::KernelClaim { .. } => context
                .births
                .get(input)
                .is_some_and(|b| b.point.daa_score <= record.accepted_daa && b.point.blue_score <= record.accepted_blue_score),
            _ => true,
        });
    }
    for (root_id, root) in &state.exec_v2.roots {
        let chain = records.get(&W::ChainClaim { claim: *root_id }).ok_or_else(|| bad("native session lost chain original"))?;
        let execution_root = chain.execution_root;
        let chain_final = chain.final_claim;
        let chain_certified = chain.certified;
        let chain_closed = chain.liability_closed;
        let work = W::ExecSession { root: *root_id };
        let birth = born(context, work)?;
        let mut record = with_birth(work, birth, execution_root);
        record.inputs.insert(W::ChainClaim { claim: *root_id });
        if root.prefix != Prefix::Unbound {
            record.inputs.insert(W::KernelClaim { claim: root.prefix_claim });
        }
        record.voided = matches!(root.phase, RootPhase::Voided { .. });
        record.final_claim = !record.voided && chain_final && matches!(root.phase, RootPhase::Settled { .. });
        record.certified = record.final_claim && chain_certified && root.prefix != Prefix::Unbound;
        record.liability_closed =
            (record.voided && now > root.expiry_daa) || (record.certified && chain_closed && now > root.expiry_daa);
        if !record.liability_closed {
            retain_birth_material_v10(state, &mut record, birth, material)?;
            retain_material(&mut record, borsh::to_vec(&(*root_id, root)).expect("native session row"), material);
        }
        records.insert(work, record);
        for ((_, index), slice) in state.exec_v2.slices.range((*root_id, 0)..=(*root_id, u32::MAX)) {
            let work = W::ExecSlice { root: *root_id, index: *index };
            let birth = born(context, work)?;
            let mut record = with_birth(work, birth, execution_root);
            record.inputs.insert(W::ExecSession { root: *root_id });
            if *index > 0 {
                record.inputs.insert(W::ExecSlice { root: *root_id, index: index - 1 });
            }
            let kernel = records.get(&W::KernelClaim { claim: slice.evidence_root });
            if root.prefix != Prefix::Unbound {
                record.inputs.insert(W::KernelClaim { claim: slice.evidence_root });
            }
            record.voided =
                matches!(slice.stage, SlicePhase::Voided { .. } | SlicePhase::ProvenFalse { .. } | SlicePhase::Defaulted { .. });
            record.final_claim = matches!(slice.stage, SlicePhase::Verified { .. });
            record.certified = record.final_claim && kernel.is_some_and(|r| r.certified);
            record.liability_closed = if record.voided {
                now > root.expiry_daa
            } else {
                record.certified && kernel.is_some_and(|r| r.liability_closed) && now > root.expiry_daa
            };
            if !record.liability_closed {
                retain_birth_material_v10(state, &mut record, birth, material)?;
                retain_material(&mut record, borsh::to_vec(&(*root_id, *index, slice)).expect("native slice row"), material);
            }
            records.insert(work, record);
        }
    }
    for (work, (event, frame, scoped_exec)) in &context.settlements {
        let W::ExecSettlement { root, .. } = work else { continue };
        let Some(session) = records.get(&W::ExecSession { root: *root }) else { continue };
        let mut record = with_birth(
            *work,
            Birth { point: event.point, frame, full_frame: !*scoped_exec, native_coordinate: false, scoped_exec: *scoped_exec },
            session.execution_root,
        );
        record.inputs = event.inputs.clone();
        record.final_claim = session.final_claim;
        record.certified = session.certified;
        record.voided = session.voided;
        record.liability_closed = session.liability_closed;
        if !record.liability_closed {
            retain_birth_material_v10(
                state,
                &mut record,
                Birth { point: event.point, frame, full_frame: !*scoped_exec, native_coordinate: false, scoped_exec: *scoped_exec },
                material,
            )?;
        }
        records.insert(*work, record);
    }
    for (id, (_, _, row)) in &context.mining_claims {
        let work = W::NativeMiningClaim39 { claim: *id };
        let birth = born(context, work)?;
        let mut record = with_birth(work, birth, kaspa_hashes::ZERO_HASH64);
        let claim = &row.1;
        record.voided =
            crate::palw_native_receipt_v39::native_mining_convicted_v39(state, id) || claim.closed.is_some_and(|(_, issued)| !issued);
        record.final_claim = !record.voided && claim.closed.is_some_and(|(_, issued)| issued);
        record.certified = record.final_claim
            && (crate::palw_rfc0018_v1::palw_native_independently_work_certified_v39(params, state, id, claim.admitted_daa)
                || crate::palw_native39_frontier_v1::native39_frontier_anchors_v1(state)?.iter().any(|(_, a)| {
                    a.claim == *id
                        && a.claim_root.as_bytes() == row.0
                        && crate::palw_native39_frontier_v1::native39_anchor_collected_v1(state, a)
                        && crate::palw_native39_frontier_v1::native39_frontier_standing_v1(state, params, a)
                }));
        // The actual source job holds were created in this claim's admission
        // journal. A current policy guess cannot shorten that frozen liability.
        let until = context
            .mining_bounds
            .get(id)
            .copied()
            .ok_or_else(|| bad("native mining admission lost its exact source liability hold"))?;
        record.liability_closed = (record.voided || record.certified)
            && now > until
            && !crate::palw_native_receipt_v39::native_mining_accusation_pending_v39(state, id);
        if !record.liability_closed {
            if birth.native_coordinate {
                let found =
                    super::super::palw_claim_local_native_mining_birth_v20::visit_checkpoint_material_v20(state, *id, |raw| {
                        retain_material(&mut record, raw.to_vec(), material);
                        Ok(())
                    })?;
                if !found {
                    return Err(bad("scoped mining checkpoint lost exact receipt material"));
                }
            } else {
                retain_birth_material_v10(state, &mut record, birth, material)?;
            }
            retain_material(&mut record, borsh::to_vec(&**row).expect("native mining source row"), material);
        }
        records.insert(work, record);
    }
    for (_, anchor) in crate::palw_native39_frontier_v1::native39_frontier_anchors_v1(state)?.iter() {
        let work = W::NativeMining39 { claim: anchor.claim, work: anchor.work, quantum: anchor.quantum };
        let point =
            PalwBlockContextV2 { block: anchor.anchor, daa_score: anchor.anchor_daa, blue_score: anchor.anchor_blue, subsidy: 0 };
        let voided = crate::palw_native_receipt_v39::native_mining_convicted_v39(state, &anchor.claim);
        let certified = !voided && crate::palw_native39_frontier_v1::native39_frontier_standing_v1(state, params, anchor);
        let closed = if voided {
            now >= anchor.matured_daa
        } else {
            crate::palw_native39_frontier_v1::native39_frontier_anchor_ready_v1(
                state,
                params,
                anchor,
                &PalwBlockContextV2 { daa_score: now, ..point },
            )?
        };
        let mut record = PalwClaimLocalRecordV1 {
            work,
            accepted_daa: point.daa_score,
            accepted_blue_score: point.blue_score,
            accepting_block: point.block,
            anchors: true,
            inputs: BTreeSet::from([W::NativeMiningClaim39 { claim: anchor.claim }]),
            execution_root: kaspa_hashes::ZERO_HASH64,
            reservations: BTreeSet::new(),
            final_claim: !voided,
            certified,
            voided,
            liability_closed: closed,
            material: BTreeSet::new(),
        };
        if !closed {
            retain_material(&mut record, borsh::to_vec(anchor).expect("native39 anchor"), material);
            if let Some(route) = &state.kernel_route {
                use crate::palw_native_mining_v39::{MINING_CLAIM_KIND_V39, MINING_RECEIPT_KIND_V39, MINING_TABLE_V39};
                for (kind, id) in [(MINING_CLAIM_KIND_V39, anchor.claim), (MINING_RECEIPT_KIND_V39, anchor.receipt)] {
                    let key = crate::palw_native_receipt_v39::native_mining_row_key_v39(kind, &id.as_bytes());
                    if let Some(bytes) = route.aux.get(&(MINING_TABLE_V39, key)) {
                        retain_material(&mut record, bytes.clone(), material);
                    }
                }
            }
        }
        records.insert(work, record);
    }
    // After finite receipt/source collection, canonical195 retains the owning
    // fold's irreversible decision. It is authenticated as ordinary pruned
    // consensus state under its bound network/approval assumptions, not as a
    // cryptographic proof of model computation. A live193 vertex keeps its
    // original edges; only an already-closed collected lot uses this handoff.
    for (_, sealed) in crate::palw_native_closed_facts_v1::read_native_closed_facts_v1(state)?.iter() {
        if sealed.origin.family != crate::palw_native_closed_facts_v1::ClosedFactFamilyV1::NativeMining39 {
            continue;
        }
        let quantum = sealed.origin.quantum.ok_or_else(|| bad("closed native lot quantum absent"))?;
        let work = W::NativeMining39 { claim: sealed.origin.subject, work: sealed.origin.work, quantum };
        let fact = &sealed.fact;
        if let Some(live) = records.get(&work) {
            if live.accepting_block != fact.anchor
                || live.accepted_blue_score != fact.accepted_blue
                || live.accepted_daa != fact.accepted_daa
            {
                return Err(bad("sealed native lot changes its actual acceptance point"));
            }
            continue;
        }
        records.insert(
            work,
            PalwClaimLocalRecordV1 {
                work,
                accepted_daa: fact.accepted_daa,
                accepted_blue_score: fact.accepted_blue,
                accepting_block: fact.anchor,
                anchors: true,
                inputs: BTreeSet::new(),
                execution_root: kaspa_hashes::ZERO_HASH64,
                reservations: BTreeSet::new(),
                final_claim: true,
                certified: true,
                voided: false,
                liability_closed: true,
                material: BTreeSet::new(),
            },
        );
    }
    // Retain the last executed source event for THIS namespaced work only.
    // Unrelated open records never acquire another claim's latest whole frame.
    // Current family bodies/status/IDs are resolved separately from this same
    // snapshot's scoped inventory, even when no new credit frame was created.
    for record in records.values_mut().filter(|r| !r.liability_closed) {
        // This source's current native inputs were retained above. Its latest
        // unrelated whole F snapshot is neither needed nor relabelled as local.
        if let W::ChainClaim { claim } = record.work {
            let current = state.claims.get(&claim).ok_or_else(|| bad("scoped Chain latest context lost claim"))?;
            // The strict material visitor above already checked every present
            // A8 body and alias. Only genuinely absent A8 uses literal legacyF.
            if super::super::palw_claim_local_source_admission_v8::has_source_admission_v8(state, claim, current)? {
                continue;
            }
            if super::super::palw_claim_local_legacy_terminal_v18::claim_checkpoint_overlay_ready_v18(state, params, claim, current)? {
                continue;
            }
        }
        if context.births.get(&record.work).is_some_and(|birth| birth.native_coordinate || birth.scoped_exec) {
            continue;
        }
        if let Some(latest) = context.latest_by_work.get(&record.work) {
            retain_material(record, latest.frame.clone(), material);
        }
    }
    // Every edge must refer to material that was already admitted. Missing rows
    // and out-of-order native sources cannot turn into optimistic closed credit.
    for record in records.values() {
        for input in &record.inputs {
            let source = records.get(input).ok_or_else(|| bad("native liability checkpoint lost consumed work"))?;
            if source.accepted_daa > record.accepted_daa || source.accepted_blue_score > record.accepted_blue_score {
                return Err(bad("native checkpoint consumed future work"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rfc23s2_native_settlement_and_mining_quantum_wire_namespaces_roundtrip() {
        use crate::palw_native_resource_v1::preflight_native_value_v1;
        use PalwClaimLocalWorkRefV1 as W;
        let id = Hash64::from_u64_word(13);
        let block = Hash64::from_u64_word(14);
        let sources = [
            W::ChainClaim { claim: id },
            W::KernelClaim { claim: id },
            W::ExecSession { root: id },
            W::ExecSlice { root: id, index: 0 },
            W::ExecSettlement { root: id, block },
            W::NativeMining39 { claim: id, work: block, quantum: 0 },
            W::NativeMiningClaim39 { claim: id },
        ];
        assert_eq!(sources.iter().copied().collect::<BTreeSet<_>>().len(), sources.len());
        for (tag, source) in sources.iter().enumerate() {
            let bytes = borsh::to_vec(source).unwrap();
            assert_eq!(bytes[0], tag as u8);
            preflight_native_value_v1::<W>(bytes.as_slice(), bytes.len() as u64).unwrap();
            assert_eq!(*decode::<W>(&bytes).unwrap(), *source);
            assert!(preflight_native_value_v1::<W>(&bytes[..bytes.len() - 1], bytes.len() as u64 - 1).is_err());
        }
        assert_ne!(sources[5], W::NativeMining39 { claim: id, work: block, quantum: 1 });
    }
}

#[cfg(test)]
mod scoped_kernel_checkpoint_tests {
    use super::super::super::palw_claim_local_kernel_current_birth_v9 as scoped;
    use super::*;
    use misaka_palw_kernel::{
        ledger::{ClaimBodyV1, ClaimRowV1},
        lifecycle::{ClaimLifecycleV1, LifecyclePolicyV1},
        spec::{SpecClaimBodyV1, SpecClaimV1, retrieval::RetrievalClaimV1},
    };
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn point(n: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h(100 + n), daa_score: n, blue_score: n, subsidy: 77 }
    }
    fn row(n: u64) -> ClaimRowV1 {
        let body = SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: [7; 64], producer_bond: [8; 64], result: vec![] });
        ClaimRowV1 {
            producer: [8; 64],
            class_binding_id: [9; 64],
            job_id: [7; 64],
            body: ClaimBodyV1::Spec(Box::new(SpecClaimBodyV1 { claim: body, pre_state: vec![], pre_source: None })),
            committed_daa: n,
            life: ClaimLifecycleV1::new(LifecyclePolicyV1 { check_window_daa: 10, challenge_window_daa: 10 }),
            reserved: 0,
            liability_until: None,
            convicted: false,
            rewarded: false,
            sealed_daa: 0,
        }
    }
    fn fixture() -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1) {
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0)
            .unwrap()
            .with_claim_local_settlement_from_daa(Some(0));
        let mut state = PalwChainStateV2::genesis();
        state.last_point = Some(point(1));
        state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h(1), h(2)),
            None,
            misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
        ));
        let extras = PalwTransitionExtrasV1 {
            kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
                network_domain: h(1),
                ruleset_digest: h(2),
                ..Default::default()
            }),
            ..Default::default()
        };
        (state, params, extras)
    }

    #[test]
    fn rfc23s2_kernel_checkpoint_actual_native_birth_without_full_f_cold_and_reorg() {
        let (state, params, extras) = fixture();
        let id = h(300);
        let original = row(2);
        let table = misaka_palw_kernel::rows::TABLE_CLAIMS_V1;
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        b.write_kernel_row_encoded_v1(table, id.as_byte_slice(), Some(&original)).unwrap();
        scoped::capture_native_admissions_v9(&mut b, &point(2)).unwrap();
        b.state.last_point = Some(point(2));
        assert_eq!(rewards::frames(&b.state).count(), 0, "native writer seam has no historical world frame");
        let c = context(&b.state).unwrap();
        let work = PalwClaimLocalWorkRefV1::KernelClaim { claim: id };
        assert!(c.births[&work].native_coordinate);
        assert!(!c.births[&work].full_frame);
        let mut records = BTreeMap::from([(
            work,
            PalwClaimLocalRecordV1 {
                work,
                accepted_daa: 2,
                accepted_blue_score: 0,
                accepting_block: ZERO_HASH64,
                anchors: false,
                inputs: BTreeSet::new(),
                execution_root: ZERO_HASH64,
                reservations: BTreeSet::new(),
                final_claim: false,
                certified: false,
                voided: false,
                liability_closed: false,
                material: BTreeSet::new(),
            },
        )]);
        let mut material = BTreeMap::new();
        attach(&b.state, &params, 2, &c, &mut records, &mut material).unwrap();
        assert_eq!(records[&work].accepting_block, point(2).block);
        assert_eq!(records[&work].accepted_blue_score, 2);
        assert!(!records[&work].certified, "actual birth marker grants no selected approval");
        let proof = material.clone();
        assert!(proof.len() >= 2, "actual native row and typed source-only projection are retained");
        let bytes = borsh::to_vec(b.state.kernel_route().unwrap()).unwrap();
        let mut cold = b.state.clone();
        cold.kernel_route = Some(borsh::from_slice(&bytes).unwrap());
        scoped::validate_current_births_v9(&cold).unwrap();
        let c = context(&cold).unwrap();
        let mut cold_records = records.clone();
        let mut cold_material = BTreeMap::new();
        attach(&cold, &params, 2, &c, &mut cold_records, &mut cold_material).unwrap();
        assert_eq!(cold_material, proof);
        let mut updated = original.clone();
        updated.reserved = 3;
        let prior = cold.clone();
        let mut u = TransitionBuilder::new(&prior, &params, false, false, false, false, &extras);
        u.write_kernel_row_encoded_v1(table, id.as_byte_slice(), Some(&updated)).unwrap();
        scoped::capture_native_admissions_v9(&mut u, &point(3)).unwrap();
        let mut changed = BTreeMap::new();
        scoped::visit_current_checkpoint_material_v10(&u.state, id, |raw| {
            changed.insert(palw_liability_material_id_v1(raw), raw.to_vec());
            Ok(())
        })
        .unwrap();
        assert_ne!(changed, proof, "actual current native mutation changes its scoped material");
        for entry in u.entries.iter().rev() {
            let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry else { panic!("native journal only") };
            super::super::super::apply_kernel_route_row_v1(&mut u.state, *table, key, old, new, true).unwrap();
        }
        let mut restored = BTreeMap::new();
        scoped::visit_current_checkpoint_material_v10(&u.state, id, |raw| {
            restored.insert(palw_liability_material_id_v1(raw), raw.to_vec());
            Ok(())
        })
        .unwrap();
        assert_eq!(restored, proof);
    }
    #[test]
    fn rfc23s2_native_prefix_keeps_actual_f_carriage_integrity_and_birth() {
        // Accounting/codec seam: the common native row writer and real F1
        // canonical carriage factory, not a signed public claim admission.
        let (state, params, extras) = fixture();
        let id = h(300);
        let original = row(2);
        let table = misaka_palw_kernel::rows::TABLE_CLAIMS_V1;
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        b.write_kernel_row_encoded_v1(table, id.as_byte_slice(), Some(&original)).unwrap();
        scoped::capture_native_admissions_v9(&mut b, &point(2)).unwrap();
        let prior = PalwStateCarriageV2::from_state_for_claim_local_frame_v1(&state);
        let mut current = PalwStateCarriageV2::from_state_for_claim_local_frame_v1(&b.state);
        current.last_point = Some(point(2));
        let mut frame = PalwClaimLiabilityFrameV1 {
            point: point(2),
            network_identity: [11; 32],
            fired_forks: [12; 32],
            policy: crate::palw_claim_local_v1::PalwClaimLocalPolicyContextV1::from_rules_v1(&params, 2),
            claims: BTreeSet::new(),
            native_changes: vec![PalwClaimLocalNativeChangeV1 {
                owner: PalwClaimLocalRowOwnerV1::KernelRoute,
                table,
                key: id.as_byte_slice().to_vec(),
                old: None,
                new: Some(borsh::to_vec(&original).unwrap()),
            }],
            prior_state: borsh::to_vec(&prior).unwrap(),
            current_state: borsh::to_vec(&current).unwrap(),
            accepted_objects: vec![],
            origin_headers: BTreeMap::new(),
            inputs: vec![],
        };
        frame.validate_context_v1().unwrap();
        assert!(frame.native_credit_events_v1().unwrap().is_empty());
        let key = crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(point(2).block);
        b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(&frame)).unwrap();
        let view = context(&b.state).unwrap();
        let work = PalwClaimLocalWorkRefV1::KernelClaim { claim: id };
        assert_eq!(view.births[&work].point, point(2));
        assert!(view.births[&work].native_coordinate);
        assert!(view.settlements.is_empty());
        drop(view);
        // A valid prefix with a corrupt carriage tail must NEVER become valid
        // because this frame happens to contain no native EXEC event.
        frame.current_state.push(0xff);
        b.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(&frame)).unwrap();
        assert!(matches!(context(&b.state), Err(PalwStateV2Error::CarriageInconsistent(_))));
    }
    fn exec_root_row() -> crate::palw_work_slice_v2::PalwWorkRootV2 {
        crate::palw_work_slice_v2::PalwWorkRootV2 {
            class_id: h(10),
            canonical_job_id: h(11),
            job_work_id: h(12),
            kernel_version: 1,
            plan_root: h(13),
            total_work: 2,
            boundaries: vec![1, 2],
            initial_state_root: h(14),
            evidence_policy_root: h(15),
            root_bond: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(16), 0)),
            extra_executors: vec![],
            executor_exposure: 0,
            anchor: point(2).block,
            opened_daa: 2,
            expiry_daa: 100,
            next_index: 0,
            last_state_root: h(14),
            accepted_work: 0,
            verified_work: 0,
            pending: 0,
            phase: crate::palw_work_slice_v2::PalwWorkRootPhaseV2::Open,
            prefix_claim: ZERO_HASH64,
            prefix: crate::palw_work_slice_v2::PalwWorkPrefixStageV2::Unbound,
        }
    }
    fn exec_slice_row() -> crate::palw_work_slice_v2::PalwWorkSliceRowV2 {
        crate::palw_work_slice_v2::PalwWorkSliceRowV2 {
            slice_id: h(20),
            range: crate::palw_exec_v2::PalwWorkRangeV1 { start: 1, end: 2 },
            executor: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(16), 0)),
            carrier: h(21),
            accepted_daa: 2,
            predecessor_state_root: h(14),
            result_state_root: h(22),
            evidence_root: h(23),
            da_root: h(24),
            stage: crate::palw_work_slice_v2::PalwWorkSliceStageV2::Verified { verified_daa: 2, leg_cap: 4, route_reward: 0 },
        }
    }

    #[test]
    fn rfc23s2_exec_checkpoint_typed_no_f_multiple_event_leases_cold_and_inverse() {
        // Actual native writer/checkpoint provenance seam. This does not mint
        // a chain original, selected approval or signed EXEC admission.
        let (state, params, extras) = fixture();
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        for n in 0..13u64 {
            let id = h(500 + n);
            let mut root = exec_root_row();
            root.job_work_id = h(1000 + n);
            b.write_exec_job(root.job_work_id, Some(id)).unwrap();
            b.write_exec_root(id, Some(root.clone())).unwrap();
            let mut slice = exec_slice_row();
            slice.slice_id = h(2000 + n);
            b.write_exec_slice((id, 0), Some(slice)).unwrap();
            root.next_index = 1;
            root.accepted_work = 1;
            root.verified_work = 1;
            root.phase = crate::palw_work_slice_v2::PalwWorkRootPhaseV2::Settled { settled_daa: 2 };
            b.write_exec_root(id, Some(root)).unwrap();
        }
        use super::super::super::palw_claim_local_exec_archive_v10 as archive;
        archive::capture_executed_exec_v10(&mut b, &state, &point(2)).unwrap();
        b.state.last_point = Some(point(2));
        assert_eq!(rewards::frames(&b.state).count(), 0);
        let requests = super::super::resources::requests(&b.state).unwrap();
        let _context_lease = crate::palw_native_resource_v1::reserve_native_requests_v1(requests).unwrap();
        let c = context(&b.state).unwrap();
        assert_eq!(c.settlements.len(), 13);
        assert_eq!(c._event_leases.len(), 13);
        assert_eq!(c.births.len(), 26);
        assert!(c.births.values().all(|birth| birth.scoped_exec && !birth.full_frame));
        let first = PalwClaimLocalWorkRefV1::ExecSettlement { root: h(500), block: point(2).block };
        assert_eq!(
            c.settlements[&first].0.inputs,
            BTreeSet::from([
                PalwClaimLocalWorkRefV1::ExecSession { root: h(500) },
                PalwClaimLocalWorkRefV1::ExecSlice { root: h(500), index: 0 }
            ])
        );
        assert!(c.settlements[&first].2);
        drop(c);
        let state_root = b.state.state_root();
        let wire = borsh::to_vec(b.state.kernel_route().unwrap()).unwrap();
        let mut cold = b.state.clone();
        cold.kernel_route = Some(borsh::from_slice(&wire).unwrap());
        archive::validate_exec_archive_v10(&cold).unwrap();
        assert_eq!(context(&cold).unwrap().settlements.len(), 13);
        assert_eq!(cold.state_root(), state_root);
        let prior = cold.clone();
        let mut d = TransitionBuilder::new(&prior, &params, false, false, false, false, &extras);
        d.write_exec_slice((h(500), 0), None).unwrap();
        d.write_exec_job(h(1000), None).unwrap();
        d.write_exec_root(h(500), None).unwrap();
        archive::capture_executed_exec_v10(&mut d, &prior, &point(3)).unwrap();
        assert_eq!(context(&d.state).unwrap().settlements.len(), 12);
        for entry in d.entries.iter().rev() {
            super::super::super::apply_delta_entry(&mut d.state, entry, true).unwrap();
        }
        assert_eq!(d.state.state_root(), state_root);
        assert_eq!(context(&d.state).unwrap().settlements.len(), 13);
    }
}

