//! CHILD of the actual private carrier-lineage owner. Observation ONLY.
//! No Borsh codec/plan/hash, prospective permit, fee, price or invented finite H.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::palw_state_v2) enum CarrierRetentionReasonV9 {
    /// Actual native winning-output/pruning authority must release this owner.
    /// E.H expiry alone cannot release a source summary or reverse entry.
    NativeWinningOutputOrPruningAuthority,
    /// Existing real source-duty collection calls release_consumer_facts_v9.
    ActualConsumerSourceDutyCollection,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::palw_state_v2) struct ReferencedRowsGeometryV9 {
    /// REFERENCED sealed rows, not every row in the global component.
    pub rows: u64,
    pub encoded_value_bytes: u64,
    pub component_rows_after: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::palw_state_v2) struct ActualJournalBytesV9 {
    pub writes: u64,
    pub inserts: u64,
    pub replaces: u64,
    pub deletes: u64,
    pub key_bytes: u64,
    pub old_present: u64,
    pub new_present: u64,
    pub old_encoded_value_bytes: u64,
    pub new_encoded_value_bytes: u64,
    /// Actual Borsh KernelRouteRow delta entry incl tag/table/key/Option framing.
    pub encoded_delta_bytes: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::palw_state_v2) struct ActualMetadataWritesV9 {
    pub nbia: [ActualJournalBytesV9; 4],
    /// Existing literal winner index is separate, not folded into new A3 fees.
    pub literal_winner167: ActualJournalBytesV9,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::palw_state_v2) struct ActualCarrierScopeV9 {
    pub consumer: Hash64,
    pub accepting_point: PalwBlockContextV2,
    pub stored_provenance: (Hash64, u64, u64),
    pub source_manifest: RowValueV2,
    pub sealed_facts: RowValueV2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::palw_state_v2) struct ActualInputCardinalityV9 {
    pub funded_input_occurrences: u64,
    pub accepted_input_occurrences: u64,
    pub distinct_spent_txids: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::palw_state_v2) struct LocalObservationDemandV9 {
    /// Local ABI-sized report wrapper only. NOT capture/reader total peak.
    /// Existing typed capture/readers retain and admit their independent leases.
    pub report_wrapper_requests: u64,
    pub input_cardinality_scratch_requests: u64,
}
/// Private construction, no Serialize. Canonical scalar evidence and local
/// NativeDemand are separate, never a paid-plan/admission capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::palw_state_v2) struct ActualCarrierGeometryV9 {
    scope: ActualCarrierScopeV9,
    /// SUMMARY, REVERSE, CONSUMER referenced sealed row quantities respectively.
    referenced: [ReferencedRowsGeometryV9; 3],
    /// N, B, I, A actual writes respectively, only when captured in this call.
    /// None means historical journal NOT OBSERVED, never zero writes inferred.
    journal: Option<ActualMetadataWritesV9>,
    retention: [CarrierRetentionReasonV9; 3],
    local: LocalObservationDemandV9,
    input_cardinality: Option<ActualInputCardinalityV9>,
}
impl ActualCarrierGeometryV9 {
    pub(in crate::palw_state_v2) fn actual_input_cardinality(&self) -> Option<ActualInputCardinalityV9> {
        self.input_cardinality
    }
    pub(in crate::palw_state_v2) fn scope(&self) -> ActualCarrierScopeV9 {
        self.scope
    }
    pub(in crate::palw_state_v2) fn referenced_rows(&self) -> &[ReferencedRowsGeometryV9; 3] {
        &self.referenced
    }
    pub(in crate::palw_state_v2) fn actual_journal(&self) -> Option<&ActualMetadataWritesV9> {
        self.journal.as_ref()
    }
    pub(in crate::palw_state_v2) fn retention_reasons(&self) -> &[CarrierRetentionReasonV9; 3] {
        &self.retention
    }
    pub(in crate::palw_state_v2) fn local_observation_demand(&self) -> LocalObservationDemandV9 {
        self.local
    }
}
// Fault injection only: a fresh exact subprocess arms the REAL native demand
// checker at zero AFTER the successful writer, before observer-local requests.
// No fake Resource value, paid authority or production control is introduced.
#[cfg(test)]
thread_local! {static OBSERVER_FAILURE_STAGE_V9:std::cell::Cell<u8>=const{std::cell::Cell::new(0)};
static OBSERVER_FAILURE_HIT_V9:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
#[cfg(test)]
pub(in crate::palw_state_v2) fn arm_geometry_observer_failure_v9(stage: u8) {
    assert!(matches!(stage, 1 | 2));
    OBSERVER_FAILURE_STAGE_V9.with(|v| v.set(stage));
    OBSERVER_FAILURE_HIT_V9.with(|v| v.set(false));
}
#[cfg(test)]
pub(in crate::palw_state_v2) fn geometry_observer_failure_hit_v9() -> bool {
    OBSERVER_FAILURE_HIT_V9.with(|v| v.get())
}
#[cfg(test)]
fn inject_observer_failure_v9(stage: u8) {
    if OBSERVER_FAILURE_STAGE_V9.with(|v| {
        if v.get() == stage {
            v.set(0);
            true
        } else {
            false
        }
    }) {
        use crate::palw_native_resource_v1::{
            NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, native_declared_logical_limits_v1,
        };
        assert!(native_declared_logical_limits_v1().is_none(), "fault child must use a fresh checker");
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
        assert_eq!(native_declared_logical_limits_v1().unwrap().memory_bytes, 0);
        OBSERVER_FAILURE_HIT_V9.with(|v| v.set(true));
    }
}
fn plus(a: &mut u64, b: u64) -> Result<(), ContextErrorV2> {
    *a = a.checked_add(b).ok_or_else(|| bad("carrier actual geometry overflow"))?;
    Ok(())
}
fn encoded<T: BorshSerialize>(value: &T) -> Result<u64, ContextErrorV2> {
    u64::try_from(borsh::object_length(value).map_err(|_| bad("carrier actual encoded length"))?)
        .map_err(|_| bad("carrier actual encoded length overflow"))
}
/// Cold/retained read. Authenticates actual consumer manifest/facts/native views;
/// cannot infer an old capture journal from endpoint state or producer counters.
pub(in crate::palw_state_v2) fn observe_sealed_consumer_geometry_v9(
    state: &PalwChainStateV2,
    consumer: Hash64,
    point: PalwBlockContextV2,
) -> Result<NativeRetainedV1<ActualCarrierGeometryV9>, PalwStateV2Error> {
    observe_inner(state, consumer, point, None, None).map_err(err)
}
fn observe_inner(
    state: &PalwChainStateV2,
    consumer: Hash64,
    point: PalwBlockContextV2,
    journal: Option<&[PalwDeltaEntryV2]>,
    inputs: Option<(ActualInputCardinalityV9, u64)>,
) -> Result<NativeRetainedV1<ActualCarrierGeometryV9>, ContextErrorV2> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<NativeRetainedV1<ActualCarrierGeometryV9>>(1).map_err(ContextErrorV2::Resource)?;
    let report_requests = q.bytes_v1();
    let lease = reserve_native_requests_v1(report_requests).map_err(ContextErrorV2::Resource)?;
    let row = lookup::<FactsV9>(state, CONSUMER, None, consumer_key(consumer, point)?)?
        .ok_or(ContextErrorV2::Unavailable("successful native carrier capture facts absent"))?;
    let manifest = super::super::palw_claim_local_source_admission_v8::manifest_reference_v9(state, consumer, point)
        .map_err(native_err)?
        .ok_or(ContextErrorV2::Unavailable("carrier capture actual source manifest absent"))?;
    let admission = super::super::palw_claim_local_source_admission_v8::for_birth_point_v8(
        state,
        consumer,
        point.block,
        point.daa_score,
        point.blue_score,
    )
    .map_err(native_err)?
    .ok_or(ContextErrorV2::Unavailable("carrier observation exact source admission absent"))?;
    if admission.accepting_point() != point || row.stored != BirthV9::stored(admission.original_claim()) {
        return Err(bad("carrier observation actual/stored consumer birth differs"));
    }
    if row.version != 9
        || row.consumer != consumer
        || row.point != point
        || row.source_manifest != manifest
        || row.facts.windows(2).any(|w| w[0].spent_txid >= w[1].spent_txid)
    {
        return Err(bad("carrier actual geometry scope/order differs"));
    }
    let mut logical = [ReferencedRowsGeometryV9::default(); 3];
    for (i, table) in [SUMMARY, REVERSE, CONSUMER].into_iter().enumerate() {
        logical[i].component_rows_after = root(state, table, None)?.rows;
    }
    logical[2].rows = 1;
    logical[2].encoded_value_bytes = encoded(&*row)?;
    // Canonical reference seal is the ACTUAL N directory RowValue, not a new
    // hash implementation. lookup above already validated content-addressed B
    // identity and canonical typed body; its actual byte span must agree.
    let reader = StateNodesV2(state);
    let actual = nodes::lookup(&reader, root(state, CONSUMER, None)?, consumer_key(consumer, point)?.as_byte_slice())?
        .ok_or(ContextErrorV2::Unavailable("carrier consumer directory lost facts"))?;
    if actual.bytes != logical[2].encoded_value_bytes {
        return Err(bad("carrier facts actual encoded reference differs"));
    }
    visit_consumer_facts_v9(state, consumer, point, |fact, owner| {
        let summary = summary(state, owner.id()).map_err(err)?.ok_or_else(|| err(bad("actual source summary absent")))?;
        let reverse = lookup::<ReverseV9>(state, REVERSE, None, fact.spent_txid())
            .map_err(err)?
            .ok_or_else(|| err(bad("actual spent reverse absent")))?;
        if reverse.version != 9
            || reverse.txid != fact.spent_txid()
            || !reverse.owners.contains(&owner.id())
            || reverse.owners.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(err(bad("actual reverse row changed source membership")));
        }
        plus(&mut logical[0].rows, 1).map_err(err)?;
        plus(&mut logical[0].encoded_value_bytes, encoded(&*summary).map_err(err)?).map_err(err)?;
        plus(&mut logical[1].rows, 1).map_err(err)?;
        plus(&mut logical[1].encoded_value_bytes, encoded(&*reverse).map_err(err)?).map_err(err)?;
        Ok(())
    })
    .map_err(native_err)?;
    let writes = journal.map(journal_geometry).transpose()?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        ActualCarrierGeometryV9 {
            scope: ActualCarrierScopeV9 {
                consumer,
                accepting_point: point,
                stored_provenance: (row.stored.block, row.stored.daa, row.stored.blue),
                source_manifest: manifest,
                sealed_facts: actual,
            },
            referenced: logical,
            journal: writes,
            retention: [
                CarrierRetentionReasonV9::NativeWinningOutputOrPruningAuthority,
                CarrierRetentionReasonV9::NativeWinningOutputOrPruningAuthority,
                CarrierRetentionReasonV9::ActualConsumerSourceDutyCollection,
            ],
            local: LocalObservationDemandV9 {
                report_wrapper_requests: report_requests,
                input_cardinality_scratch_requests: inputs.map_or(0, |(_, n)| n),
            },
            input_cardinality: inputs.map(|(counts, _)| counts),
        },
        lease,
    ))
}
fn journal_geometry(journal: &[PalwDeltaEntryV2]) -> Result<ActualMetadataWritesV9, ContextErrorV2> {
    let mut out = ActualMetadataWritesV9::default();
    for entry in journal {
        let PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } = entry else {
            return Err(bad("carrier capture journal crossed non-aux write"));
        };
        let g = if *table == crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1 {
            if key.len() != 64 {
                return Err(bad("actual winner167 key differs"));
            }
            &mut out.literal_winner167
        } else {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 {
                return Err(bad("carrier capture journal crossed table"));
            }
            let i = match key.first() {
                Some(b'N') => 0,
                Some(b'B') => 1,
                Some(b'I') => 2,
                Some(b'A') => 3,
                _ => return Err(bad("carrier capture journal crossed namespace")),
            };
            &mut out.nbia[i]
        };
        plus(&mut g.writes, 1)?;
        plus(&mut g.key_bytes, key.len() as u64)?;
        match (old.is_some(), new.is_some()) {
            (false, true) => plus(&mut g.inserts, 1)?,
            (true, true) => plus(&mut g.replaces, 1)?,
            (true, false) => plus(&mut g.deletes, 1)?,
            _ => return Err(bad("empty carrier capture write")),
        }
        if let Some(raw) = old {
            plus(&mut g.old_present, 1)?;
            plus(&mut g.old_encoded_value_bytes, raw.len() as u64)?;
        }
        if let Some(raw) = new {
            plus(&mut g.new_present, 1)?;
            plus(&mut g.new_encoded_value_bytes, raw.len() as u64)?;
        }
        plus(&mut g.encoded_delta_bytes, encoded(entry)?)?;
    }
    Ok(out)
}
fn actual_input_cardinality(
    extras: &PalwTransitionExtrasV1,
    consumer: Hash64,
) -> Result<(ActualInputCardinalityV9, u64), ContextErrorV2> {
    let funded = extras.tir_funded_originals.get(&consumer).map_or(&[][..], |p| p.carrier().inputs.as_slice());
    let accepted = extras.carrier_input_spends.get(&consumer).map_or(&[][..], Vec::as_slice);
    let total = funded.len().checked_add(accepted.len()).ok_or_else(|| bad("actual input occurrence count overflow"))?;
    let count = u64::try_from(total).map_err(|_| bad("actual input occurrence count conversion"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<Hash64, ()>>(count).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<BTreeSet<Hash64>>(1).map_err(ContextErrorV2::Resource)?;
    let requests = q.bytes_v1();
    let _lease = reserve_native_requests_v1(requests).map_err(ContextErrorV2::Resource)?;
    let distinct: BTreeSet<Hash64> =
        funded.iter().map(|i| i.previous_outpoint.transaction_id).chain(accepted.iter().copied()).collect();
    Ok((
        ActualInputCardinalityV9 {
            funded_input_occurrences: funded.len() as u64,
            accepted_input_occurrences: accepted.len() as u64,
            distinct_spent_txids: distinct.len() as u64,
        },
        requests,
    ))
}
/// OPTIONAL replacement call for actual native factory, never in addition to
/// capture_consumer_facts_v9. Produces the SAME writes; observation failure rolls
/// them back atomically. No caller-provided journal/spend-count Vec accepted.
pub(in crate::palw_state_v2) fn capture_consumer_facts_observed_v9(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    point: PalwBlockContextV2,
    consumer: Hash64,
    original: &PalwClaimStateV2,
    source_manifest: RowValueV2,
) -> Result<NativeRetainedV1<ActualCarrierGeometryV9>, PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(point.daa_score) {
        return Err(err(ContextErrorV2::Unavailable("carrier observation fence dormant")));
    }
    atomic(b, |b| {
        let start = b.entries.len();
        capture_consumer_facts_inner_v9(b, parent, point, consumer, original, source_manifest).map_err(native_err)?;
        #[cfg(test)]
        inject_observer_failure_v9(1);
        let inputs = actual_input_cardinality(b.extras, consumer)?;
        #[cfg(test)]
        inject_observer_failure_v9(2);
        observe_inner(&b.state, consumer, point, Some(&b.entries[start..]), Some(inputs))
    })
    .map_err(err)
}
/// Per-source borrowed/leased native views; no new source Vec/partial world.
pub(in crate::palw_state_v2) fn visit_observed_sources_v9(
    state: &PalwChainStateV2,
    report: &ActualCarrierGeometryV9,
    visit: impl FnMut(&CarrierFactV9, CarrierSourceViewV9) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    let manifest = super::super::palw_claim_local_source_admission_v8::manifest_reference_v9(
        state,
        report.scope.consumer,
        report.scope.accepting_point,
    )?;
    if manifest != Some(report.scope.source_manifest) {
        return Err(err(ContextErrorV2::Unavailable("observed consumer actual source manifest released or changed")));
    }
    let row =
        lookup::<FactsV9>(state, CONSUMER, None, consumer_key(report.scope.consumer, report.scope.accepting_point).map_err(err)?)
            .map_err(err)?
            .ok_or_else(|| err(ContextErrorV2::Unavailable("observed consumer source duty already collected")))?;
    let reader = StateNodesV2(state);
    let actual = nodes::lookup(
        &reader,
        root(state, CONSUMER, None).map_err(err)?,
        consumer_key(report.scope.consumer, report.scope.accepting_point).map_err(err)?.as_byte_slice(),
    )
    .map_err(err)?;
    if actual != Some(report.scope.sealed_facts) || row.source_manifest != report.scope.source_manifest {
        return Err(err(bad("observation no longer matches actual consumer scope")));
    }
    visit_consumer_facts_v9(state, report.scope.consumer, report.scope.accepting_point, visit)?;
    Ok(())
}

/// Separate successful SOURCE writer evidence. Consumer capture does not own
/// this original SUMMARY/REVERSE mint; A3 must not pretend it was included there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::palw_state_v2) struct ActualWinnerGeometryV9 {
    source: Hash64,
    point: PalwBlockContextV2,
    winner: Hash64,
    sealed_summary: RowValueV2,
    referenced: [ReferencedRowsGeometryV9; 3],
    journal: Option<ActualMetadataWritesV9>,
    retention: CarrierRetentionReasonV9,
    local: LocalObservationDemandV9,
}
impl ActualWinnerGeometryV9 {
    pub(in crate::palw_state_v2) fn source_scope(&self) -> (Hash64, PalwBlockContextV2, Hash64) {
        (self.source, self.point, self.winner)
    }
    pub(in crate::palw_state_v2) fn sealed_summary(&self) -> RowValueV2 {
        self.sealed_summary
    }
    pub(in crate::palw_state_v2) fn referenced_rows(&self) -> &[ReferencedRowsGeometryV9; 3] {
        &self.referenced
    }
    pub(in crate::palw_state_v2) fn actual_journal(&self) -> Option<&ActualMetadataWritesV9> {
        self.journal.as_ref()
    }
    pub(in crate::palw_state_v2) fn retention_reason(&self) -> CarrierRetentionReasonV9 {
        self.retention
    }
    pub(in crate::palw_state_v2) fn local_observation_demand(&self) -> LocalObservationDemandV9 {
        self.local
    }
}
pub(in crate::palw_state_v2) fn observe_sealed_winner_geometry_v9(
    state: &PalwChainStateV2,
    source: Hash64,
    point: PalwBlockContextV2,
    winner: Hash64,
) -> Result<NativeRetainedV1<ActualWinnerGeometryV9>, PalwStateV2Error> {
    observe_winner_inner(state, source, point, winner, None).map_err(err)
}
fn observe_winner_inner(
    state: &PalwChainStateV2,
    source: Hash64,
    point: PalwBlockContextV2,
    winner: Hash64,
    journal: Option<&[PalwDeltaEntryV2]>,
) -> Result<NativeRetainedV1<ActualWinnerGeometryV9>, ContextErrorV2> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<NativeRetainedV1<ActualWinnerGeometryV9>>(1).map_err(ContextErrorV2::Resource)?;
    let report_requests = q.bytes_v1();
    let lease = reserve_native_requests_v1(report_requests).map_err(ContextErrorV2::Resource)?;
    let id = summary_id(winner, source, point)?;
    let owner = view(state, id)?;
    if owner.source() != source || owner.accepting_point() != point || owner.winning_txid() != winner {
        return Err(bad("observed source winner scope differs"));
    }
    let summary = summary(state, id)?.ok_or(ContextErrorV2::Unavailable("observed source summary absent"))?;
    let reverse = lookup::<ReverseV9>(state, REVERSE, None, winner)?.ok_or(ContextErrorV2::Unavailable("observed reverse absent"))?;
    if reverse.version != 9
        || reverse.txid != winner
        || !reverse.owners.contains(&id)
        || reverse.owners.windows(2).any(|w| w[0] >= w[1])
    {
        return Err(bad("observed reverse source membership differs"));
    }
    let mut logical = [ReferencedRowsGeometryV9::default(); 3];
    for (i, table) in [SUMMARY, REVERSE, CONSUMER].into_iter().enumerate() {
        logical[i].component_rows_after = root(state, table, None)?.rows;
    }
    logical[0].rows = 1;
    logical[0].encoded_value_bytes = encoded(&*summary)?;
    logical[1].rows = 1;
    logical[1].encoded_value_bytes = encoded(&*reverse)?;
    let reader = StateNodesV2(state);
    let actual = nodes::lookup(&reader, root(state, SUMMARY, None)?, id.as_byte_slice())?
        .ok_or(ContextErrorV2::Unavailable("actual summary directory absent"))?;
    if actual.bytes != logical[0].encoded_value_bytes {
        return Err(bad("actual encoded summary span differs"));
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        ActualWinnerGeometryV9 {
            source,
            point,
            winner,
            sealed_summary: actual,
            referenced: logical,
            journal: journal.map(journal_geometry).transpose()?,
            retention: CarrierRetentionReasonV9::NativeWinningOutputOrPruningAuthority,
            local: LocalObservationDemandV9 { report_wrapper_requests: report_requests, input_cardinality_scratch_requests: 0 },
        },
        lease,
    ))
}
/// Optional native call replacement, NOT a preview and not an extra invocation.
/// Requires the successful None->Some actual Claim witness already enforced by
/// write_actual_winner_v9; does not accept a producer count or paid boolean.
pub(in crate::palw_state_v2) fn write_actual_winner_observed_v9(
    b: &mut TransitionBuilder<'_>,
    point: &PalwBlockContextV2,
    source: Hash64,
    winner: Hash64,
) -> Result<NativeRetainedV1<ActualWinnerGeometryV9>, PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(point.daa_score) {
        return Err(err(ContextErrorV2::Unavailable("carrier observation fence dormant")));
    }
    atomic(b, |b| {
        let start = b.entries.len();
        write_actual_winner_v9(b, point, source, winner).map_err(native_err)?;
        #[cfg(test)]
        inject_observer_failure_v9(2);
        observe_winner_inner(&b.state, source, *point, winner, Some(&b.entries[start..]))
    })
    .map_err(err)
}
