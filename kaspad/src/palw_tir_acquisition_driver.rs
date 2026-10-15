//! Prospective paid TIR acquisition components. This module does not activate a
//! claim, allocate a native clock/capital budget or acknowledge an unsigned send.
//! Native owned progress must authorize every future owner/responder action.
use crate::palw_memory_ledger::{PalwMemoryReservationKeyV1, PalwMemoryReservationV1, host_ledger_v1};
use kaspa_consensus_core::{
    BlockHash, Hash64,
    palw_backend::{PalwTirPublicCaptureNeedV1, PalwTirPublicCaptureReaderV1},
    palw_state_v2::PalwBondKeyV2,
    palw_step_leg::{PalwStepOpeningV1, step_opening_root_capped_v1},
    palw_tir_public_owned_v1::{
        PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1, PalwTirPublicAcceptedBodyRefV1, PalwTirPublicChunkRecordV1, PalwTirPublicFinishedV1,
        PalwTirPublicOwnedActionV1, PalwTirPublicOwnedV1, PalwTirPublicWorkPhaseV1, PalwTirPublicWorkScopeV1, PalwTirPublicWorkV1,
        palw_tir_public_body_hash_v1, request_body_bound_v1,
    },
    palw_tir_public_source_v1::{
        PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1, PalwTirPublicAuthenticatedIdsV1, PalwTirPublicAuthenticatedUnitV1, PalwTirPublicRequestV1,
        PalwTirPublicResponseV1, PalwTirPublicSourceV1,
    },
    palw_tir_public_state_v1::PalwTirPublicWorkPermitV1,
};
use std::sync::Arc;

/// Local operational accounting only. Dropping/recreating this ledger is NOT a
/// native refund, restart checkpoint or proof of paid authority. Runtime use
/// still requires an accepted native pre-debit for the same literal work scope.
pub(super) type OperationalWorkV1 = PalwTirPublicWorkV1;

pub(super) struct BorrowedPreparationSnapshotV1<'a> {
    native: &'a kaspa_consensus_core::palw_tir_public_state_v1::PalwTirPublicPreparationSnapshotV1,
}
impl<'a> BorrowedPreparationSnapshotV1<'a> {
    pub(super) fn from_native_v1(
        native: &'a kaspa_consensus_core::palw_tir_public_state_v1::PalwTirPublicPreparationSnapshotV1,
    ) -> Self {
        Self { native }
    }
}

/// Preparation-key proposal, never an outside-owner reservation or a DA demand.
pub(super) struct PreparationActionV1 {
    pub snapshot_block: BlockHash,
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub signer: PalwBondKeyV2,
    pub expected_turn: u64,
    pub until_daa: u64,
    pub action: PalwTirPublicOwnedActionV1,
}
pub(super) fn preparation_ready_action_v1(
    snapshot: &BorrowedPreparationSnapshotV1<'_>,
    signer: PalwBondKeyV2,
    descriptor: kaspa_consensus_core::palw_backend::PalwTirPublicPreparedDescriptorV1,
) -> Result<PreparationActionV1, String> {
    let n = snapshot.native;
    let p = &n.row.preparation;
    let c = p.contract_v1();
    if n.now_daa < c.original_accepted_daa
        || n.now_daa >= c.preparation_until_daa
        || !c.custodians.contains(&signer)
        || p.ready_v1(&signer).is_some()
        || p.defaulted_v1(&signer)
        || descriptor.source_id != c.source_id
        || descriptor.material_until_daa != c.material_until_daa
        || descriptor.manifest_digest == Hash64::default()
    {
        return Err("TIR Ready proposal is not a pending original literal preparation duty".into());
    }
    Ok(PreparationActionV1 {
        snapshot_block: n.snapshot_block,
        claim: c.claim,
        allocation_id: c.allocation_id,
        source_id: c.source_id,
        signer,
        expected_turn: 0,
        until_daa: c.preparation_until_daa,
        action: PalwTirPublicOwnedActionV1::PreparationReady { descriptor },
    })
}
pub(super) fn preparation_collect_action_v1(
    snapshot: &BorrowedPreparationSnapshotV1<'_>,
    observer: PalwBondKeyV2,
    actor: PalwBondKeyV2,
) -> Result<PreparationActionV1, String> {
    let n = snapshot.native;
    let p = &n.row.preparation;
    let c = p.contract_v1();
    if observer == c.producer
        || c.drawn_panel.contains(&observer)
        || n.now_daa < c.preparation_until_daa
        || n.now_daa >= c.collect_until_daa
        || !c.custodians.contains(&actor)
        || p.ready_v1(&actor).is_some()
        || p.defaulted_v1(&actor)
    {
        return Err("TIR original preparation default is not independently collectible".into());
    }
    Ok(PreparationActionV1 {
        snapshot_block: n.snapshot_block,
        claim: c.claim,
        allocation_id: c.allocation_id,
        source_id: c.source_id,
        signer: observer,
        expected_turn: 0,
        until_daa: c.collect_until_daa,
        action: PalwTirPublicOwnedActionV1::PreparationCollect { actor },
    })
}

pub(super) struct OperationalLedgerV1 {
    source_id: Hash64,
    maximum: OperationalWorkV1,
    spent: OperationalWorkV1,
    permit: Option<PalwTirPublicWorkPermitV1>,
}

impl OperationalLedgerV1 {
    fn require_preparation_v1(
        &mut self,
        snapshot: &BorrowedPreparationSnapshotV1<'_>,
        quote: &kaspa_consensus_core::palw_backend::PalwTirPublicCaptureSliceV1,
    ) -> Result<(), String> {
        let n = snapshot.native;
        let p = &n.row.preparation;
        let c = p.contract_v1();
        self.debit_v1(c.source_id, OperationalWorkV1 { attempts: 1, hash_bytes: 8192, parser_steps: 1024, ..Default::default() })?;
        let permit = self.permit.as_ref().ok_or("TIR preparation requires a fresh accepted claim-wide work permit")?;
        let s = permit.scope_v1();
        let receipt = p
            .work_v1()
            .actors
            .get(&s.actor)
            .and_then(|a| a.last.as_ref())
            .ok_or("TIR preparation has no literal paid actor receipt")?;
        if !permit.is_preparation_v1()
            || s.phase != PalwTirPublicWorkPhaseV1::Capture
            || s.claim != c.claim
            || s.bundle_id != c.allocation_id
            || s.owner != c.producer
            || s.reserved_daa != c.original_accepted_daa
            || s.turn != 0
            || s.source_id != c.source_id
            || s.input_head != c.source_id
            || s.cursor_id != quote.cursor_id
            || s.slice != quote.sequence
            || quote.source_id != c.source_id
            || receipt.hash_v1()? != permit.receipt_id_v1()
            || n.now_daa < c.original_accepted_daa
            || n.now_daa >= c.preparation_until_daa
            || p.ready_v1(&s.actor).is_some()
            || p.defaulted_v1(&s.actor)
        {
            return Err("TIR preparation work changes original claim clock/actor/index slice or duty already ended".into());
        }
        Ok(())
    }
    #[cfg(test)]
    fn new_v1(source_id: Hash64, maximum: OperationalWorkV1) -> Self {
        Self { source_id, maximum, spent: OperationalWorkV1::default(), permit: None }
    }

    pub(super) fn from_native_permit_v1(
        permit: PalwTirPublicWorkPermitV1,
        expected: PalwTirPublicWorkScopeV1,
    ) -> Result<Self, String> {
        if *permit.scope_v1() != expected {
            return Err("TIR work permit changes exact accepted execution scope".into());
        }
        Ok(Self { source_id: expected.source_id, maximum: permit.work_v1(), spent: Default::default(), permit: Some(permit) })
    }

    pub(super) fn require_current_v1(
        &mut self,
        snapshot: &BorrowedPaidSnapshotV1<'_>,
        source: &PalwTirPublicSourceV1,
        phase: PalwTirPublicWorkPhaseV1,
        request: PalwTirPublicRequestV1,
    ) -> Result<(), String> {
        self.debit_v1(
            source.source_id_v1(),
            OperationalWorkV1 { attempts: 1, hash_bytes: 8192, parser_steps: 1024, ..Default::default() },
        )?;
        let permit = self.permit.as_ref().ok_or("TIR execution requires an accepted fresh native work permit")?;
        if permit.is_preparation_v1() {
            return Err("TIR claim preparation permit cannot fund an owner DA turn".into());
        }
        let scope = permit.scope_v1();
        let c = snapshot.owned.contract_v1();
        if matches!(phase, PalwTirPublicWorkPhaseV1::Capture | PalwTirPublicWorkPhaseV1::Response | PalwTirPublicWorkPhaseV1::Ids) {
            let pending = snapshot.owned.active_v1().ok_or("TIR prepared work has no current native demand")?;
            let logical = if matches!(request, PalwTirPublicRequestV1::Ids { .. }) {
                PalwTirPublicRequestV1::Ids { offset: 0 }
            } else {
                request
            };
            if pending.collected
                || pending.request != logical
                || snapshot.now_daa < pending.opened_daa
                || snapshot.now_daa >= pending.respond_until_daa
            {
                return Err("TIR preparation permit is collected or outside its original response clock".into());
            }
        }
        let work = snapshot.owned.work_v1().ok_or("TIR current snapshot lost native work allocation")?;
        let receipt = work.actors.get(&scope.actor).and_then(|a| a.last.as_ref()).ok_or("TIR work actor receipt disappeared")?;
        if scope.source_id != source.source_id_v1()
            || scope.claim != c.claim
            || scope.owner != c.owner
            || scope.reserved_daa != c.reserved_daa
            || scope.bundle_id != c.bundle_id
            || scope.phase != phase
            || scope.request != request
            || receipt.hash_v1()? != permit.receipt_id_v1()
            || scope.turn != snapshot.owned.active_v1().map_or(snapshot.owned.spent_v1().turns, |p| p.turn)
            || snapshot.now_daa < c.reserved_daa
            || snapshot.now_daa >= c.terms.stream_until_daa
        {
            return Err("TIR work permit is stale on current native source/actor/turn/phase/receipt/clock".into());
        }
        Ok(())
    }

    pub(super) fn scope_v1(&self) -> Result<&PalwTirPublicWorkScopeV1, String> {
        self.permit.as_ref().map(|p| p.scope_v1()).ok_or_else(|| "TIR work has no native permit".into())
    }

    pub(super) fn spent_v1(&self) -> OperationalWorkV1 {
        self.spent
    }

    pub(super) fn debit_v1(&mut self, source_id: Hash64, work: OperationalWorkV1) -> Result<(), String> {
        if source_id != self.source_id {
            return Err("TIR operational work changes source".into());
        }
        let next = self.spent.checked_add_v1(work)?;
        if !self.maximum.covers_v1(&next) {
            return Err("TIR cumulative local operational work is exhausted".into());
        }
        self.spent = next;
        Ok(())
    }
}

/// The quoted slice's work as the native debit counts it (G14 R3: the custodian worker debits exactly this plus the fixed
/// per-slice overhead `require_preparation_v1` charges first).
pub(super) fn preparation_slice_work_v1(
    q: &kaspa_consensus_core::palw_backend::PalwTirPublicCaptureSliceV1,
) -> Result<OperationalWorkV1, String> {
    Ok(OperationalWorkV1 {
        attempts: 1,
        io_bytes: q.work.io_bytes,
        hash_bytes: q.work.hash_bytes,
        parser_steps: q.work.io_bytes.checked_mul(16).ok_or("TIR preparation slice parser work overflow")?,
        tree_hashes: q.work.tree_hashes,
        replay_leaves: q.work.replay_leaves,
        replay_positions: q.work.replay_positions,
        replay_id_words: q.work.replay_id_words,
        id_copy_words: q.work.replay_id_words,
        arithmetic_work: q.work.arithmetic_work,
        traffic_bytes: q.work.traffic_bytes,
        ..Default::default()
    })
}
/// Bounded cold preparation only; a returned source response is discarded, not
/// a DA acknowledgement or a computational-correctness credential.
pub(super) fn prepare_claim_capture_slice_v1(
    snapshot: &BorrowedPreparationSnapshotV1<'_>,
    reader: &mut dyn PalwTirPublicCaptureReaderV1,
    quote: &kaspa_consensus_core::palw_backend::PalwTirPublicCaptureSliceV1,
    ledger: &mut OperationalLedgerV1,
) -> Result<bool, String> {
    ledger.require_preparation_v1(snapshot, quote)?;
    let request = PalwTirPublicRequestV1::StepLeaf { index: 0 };
    let bound = snapshot.native.source.unit_bound_v1(&request)?;
    ledger.debit_v1(snapshot.native.source.source_id_v1(), preparation_slice_work_v1(quote)?)?;
    let _holding = host_ledger_v1()
        .reserve(
            PalwMemoryReservationKeyV1 {
                role: "public-tir-cold-slice",
                class_id: snapshot.native.source.header_v1().job_context.shape_profile_id,
                job: snapshot.native.row.preparation.contract_v1().claim,
            },
            bound.resident_ram_bytes,
        )
        .map_err(|e| e.to_string())?;
    Ok(reader.poll_quoted_v1(&request, quote)?.is_some())
}
/// Publication phase has its own request_id=ZERO domain, not a fictional DA
/// request. Caller holds the additional publication RAM/disk lease BEFORE the
/// writer factory; its descriptor remains only a signed retention promise.
pub(super) fn prepare_claim_publication_slice_v1(
    snapshot: &BorrowedPreparationSnapshotV1<'_>,
    writer: &mut dyn kaspa_consensus_core::palw_backend::PalwTirPublicPreparedWriterV1,
    quote: &kaspa_consensus_core::palw_backend::PalwTirPublicCaptureSliceV1,
    ledger: &mut OperationalLedgerV1,
) -> Result<Option<kaspa_consensus_core::palw_backend::PalwTirPublicPreparedDescriptorV1>, String> {
    ledger.require_preparation_v1(snapshot, quote)?;
    if quote.phase != 12 || quote.request_id != Hash64::default() {
        return Err("TIR publication work is not the dedicated prepared-index domain".into());
    }
    ledger.debit_v1(snapshot.native.source.source_id_v1(), preparation_slice_work_v1(quote)?)?;
    writer.poll_quoted_v1(quote)
}

fn metadata_work_v1(source: &PalwTirPublicSourceV1) -> Result<OperationalWorkV1, String> {
    // Includes the existing all-ID body-bound loop and up to 64 history hashes.
    Ok(OperationalWorkV1 {
        attempts: 1,
        hash_bytes: 65_536,
        parser_steps: u64::from(source.decode_count_v1())
            .div_ceil(u64::from(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1))
            .checked_mul(16)
            .and_then(|n| n.checked_add(1024))
            .ok_or("TIR metadata work overflow")?,
        ..Default::default()
    })
}

fn run_fail_closed_v1<T>(failed: &mut bool, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    if *failed {
        return Err("TIR local preparation/recovery is poisoned after a failed attempt".into());
    }
    let result = operation();
    *failed = result.is_err();
    result
}

/// Borrowed adapter only. It is not deserializable and not a native credential.
/// The future rooted reader must supply all three fields from ONE accepted native
/// snapshot. No submitting/signing/ACK actor is connected to this adapter.
pub(super) struct BorrowedPaidSnapshotV1<'a> {
    snapshot_block: BlockHash,
    now_daa: u64,
    owned: &'a PalwTirPublicOwnedV1,
    preparation: Option<&'a kaspa_consensus_core::palw_tir_public_owned_v1::PalwTirPublicPreparationV1>,
}

impl<'a> BorrowedPaidSnapshotV1<'a> {
    pub(super) fn original_clock_v1(&self) -> (u64, u64, u64) {
        (self.owned.contract_v1().reserved_daa, self.owned.contract_v1().terms.stream_until_daa, self.now_daa)
    }
    pub(super) fn material_revision_v1(&self) -> (u64, Hash64) {
        let (count, root) = self.owned.history_v1();
        (count, root.unwrap_or_default())
    }
    pub(super) fn from_native_v1(
        snapshot: &'a kaspa_consensus_core::palw_tir_public_state_v1::PalwTirPublicSnapshotV1,
    ) -> Result<Self, String> {
        let mut view = Self::borrowed_v1(snapshot.snapshot_block, snapshot.now_daa, &snapshot.row.owned);
        view.preparation = snapshot.preparation.as_ref().map(|p| p.preparation.as_ref());
        view.source_v1(&snapshot.source)?;
        Ok(view)
    }

    fn borrowed_v1(snapshot_block: BlockHash, now_daa: u64, owned: &'a PalwTirPublicOwnedV1) -> Self {
        Self { snapshot_block, now_daa, owned, preparation: None }
    }

    fn source_v1(&self, source: &PalwTirPublicSourceV1) -> Result<(), String> {
        let contract = self.owned.contract_v1();
        if contract.source_id != source.source_id_v1() || self.now_daa < contract.reserved_daa {
            return Err("TIR acquisition snapshot changes original source or reservation clock".into());
        }
        if self.owned.preparation_v1().is_some() {
            self.owned.check_preparation_v1(self.preparation.ok_or("TIR view lost current rooted claim-wide Ready state")?)?;
        }
        Ok(())
    }

    fn action_v1(&self, actor: PalwBondKeyV2, until_daa: u64, action: PalwTirPublicOwnedActionV1) -> Result<ScopedActionV1, String> {
        let c = self.owned.contract_v1();
        let expected_turn = action_turn_v1(self.owned.spent_v1().turns, self.owned.active_v1().map(|p| p.turn), &action)?;
        Ok(ScopedActionV1 {
            snapshot_block: self.snapshot_block,
            claim: c.claim,
            owner: c.owner,
            reserved_daa: c.reserved_daa,
            bundle_id: c.bundle_id,
            source_id: c.source_id,
            actor,
            expected_turn,
            until_daa,
            action,
            _reservation: None,
        })
    }
}

fn action_turn_v1(spent_turns: u64, active_turn: Option<u64>, action: &PalwTirPublicOwnedActionV1) -> Result<u64, String> {
    match action {
        PalwTirPublicOwnedActionV1::Demand { .. } | PalwTirPublicOwnedActionV1::Reacquire { .. } => Ok(spent_turns),
        PalwTirPublicOwnedActionV1::WorkDebit(debit) => Ok(debit.scope.turn),
        PalwTirPublicOwnedActionV1::PreparationReady { .. } | PalwTirPublicOwnedActionV1::PreparationCollect { .. } => Ok(0),
        PalwTirPublicOwnedActionV1::Begin { .. }
        | PalwTirPublicOwnedActionV1::Part { .. }
        | PalwTirPublicOwnedActionV1::Finish { .. }
        | PalwTirPublicOwnedActionV1::Collect { .. } => {
            active_turn.ok_or_else(|| "TIR response/collection has no exact native pending turn".into())
        }
    }
}

/// A read-only proposal, not a signed carriage or an accepted cursor.
/// A future signed wrapper MUST bind source_id AND expected_turn and check both
/// against native state before applying the action. Transaction deduplication
/// cannot prevent an old Demand from opening a different later turn.
pub(super) struct ScopedActionV1 {
    pub snapshot_block: BlockHash,
    pub claim: Hash64,
    pub owner: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub bundle_id: Hash64,
    pub source_id: Hash64,
    pub actor: PalwBondKeyV2,
    pub expected_turn: u64,
    /// Exclusive original native inclusion cutoff, not `now + retry_window`.
    pub until_daa: u64,
    pub action: PalwTirPublicOwnedActionV1,
    _reservation: Option<Arc<PalwMemoryReservationV1>>,
}

pub(super) enum OwnerIntentV1<'a> {
    Fresh(PalwTirPublicRequestV1),
    Reacquire { previous: &'a PalwTirPublicAcceptedBodyRefV1, proof: &'a PalwStepOpeningV1 },
}

pub(super) enum OwnerPlanV1 {
    Action(ScopedActionV1),
    Wait {
        until_daa: u64,
    },
    /// A public acquisition completed, not arithmetic/proof/conviction completion.
    AcquisitionEof {
        demand_id: Hash64,
    },
    Collected {
        demand_id: Hash64,
    },
    Expired,
}

fn request_cutoff_v1(stream_until: u64, response_span: u64, now: u64) -> Option<u64> {
    stream_until.checked_sub(response_span).filter(|until| now < *until)
}

fn check_body_reference_v1(
    snapshot: &BorrowedPaidSnapshotV1<'_>,
    source: &PalwTirPublicSourceV1,
    reference: &PalwTirPublicAcceptedBodyRefV1,
    history: Option<&PalwStepOpeningV1>,
    ledger: &mut OperationalLedgerV1,
) -> Result<(), String> {
    ledger.debit_v1(source.source_id_v1(), metadata_work_v1(source)?)?;
    snapshot.source_v1(source)?;
    let c = snapshot.owned.contract_v1();
    let (maximum_bytes, _) = request_body_bound_v1(source, reference.request)?;
    if snapshot.now_daa >= c.terms.material_until_daa
        || reference.source_id != c.source_id
        || reference.turn >= snapshot.owned.spent_v1().turns
        || reference.opened_daa < c.reserved_daa
        || reference.respond_until_daa >= c.terms.stream_until_daa
        || reference.opened_daa.checked_add(snapshot.owned.response_span_v1(source, reference.request)?)
            != Some(reference.respond_until_daa)
        || reference.total_bytes == 0
        || reference.total_bytes > maximum_bytes
        || reference.chunk_head == Hash64::default()
        || (reference.answerer != c.owner && !snapshot.owned.response_custodians_v1().contains(&reference.answerer))
        || source.demand_id_v1(
            &kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicDemandScopeV1 {
                claim: c.claim,
                owner: c.owner,
                reserved_daa: c.reserved_daa,
                bundle_id: c.bundle_id,
                turn: reference.turn,
                opened_daa: reference.opened_daa,
                deadline_daa: reference.respond_until_daa,
            },
            &reference.request,
        )? != reference.demand_id
    {
        return Err("TIR accepted body changes literal owner/source/request/signer/turn/original clock".into());
    }
    let current = snapshot.owned.active_v1().is_some_and(|p| {
        p.demand_id == reference.demand_id
            && p.turn == reference.turn
            && p.request == reference.request
            && p.opened_daa == reference.opened_daa
            && p.respond_until_daa == reference.respond_until_daa
            && p.slots.get(&reference.answerer).and_then(|slot| slot.finished.as_ref()) == Some(reference)
    });
    if !current {
        let proof = history.ok_or("TIR historical body has no current native membership proof")?;
        let (count, root) = snapshot.owned.history_v1();
        if proof.siblings.len() > 64
            || proof.leaf_hash != reference.hash_v1()?
            || Some(step_opening_root_capped_v1(count, proof, u64::MAX).map_err(|e| e.to_string())?) != root
        {
            return Err("TIR body is absent from the CURRENT native accepted history".into());
        }
    }
    Ok(())
}

/// Protected mandatory custody must close before the next dependency wave. An
/// owner fallback body never substitutes for a producer/Valid signer's own slot.
pub(super) fn owner_next_v1(
    snapshot: &BorrowedPaidSnapshotV1<'_>,
    source: &PalwTirPublicSourceV1,
    intent: Option<OwnerIntentV1<'_>>,
    ledger: &mut OperationalLedgerV1,
) -> Result<OwnerPlanV1, String> {
    let request = match intent.as_ref() {
        Some(OwnerIntentV1::Fresh(r)) => *r,
        Some(OwnerIntentV1::Reacquire { previous, .. }) => previous.request,
        None => snapshot.owned.active_v1().map_or(ledger.scope_v1()?.request, |p| p.request),
    };
    ledger.require_current_v1(snapshot, source, PalwTirPublicWorkPhaseV1::Planning, request)?;
    if ledger.scope_v1()?.actor != snapshot.owned.contract_v1().owner {
        return Err("TIR planning permit belongs to another actor".into());
    }
    ledger.debit_v1(source.source_id_v1(), metadata_work_v1(source)?)?;
    snapshot.source_v1(source)?;
    let owned = snapshot.owned;
    let c = owned.contract_v1();
    let now = snapshot.now_daa;
    if owned.response_custodians_v1().is_empty() {
        return Err("TIR source has no original Ready responder; use preparation defaults, not vacuous acquisition EOF".into());
    }
    if let Some(p) = owned.active_v1() {
        if p.collected {
            return Ok(OwnerPlanV1::Collected { demand_id: p.demand_id });
        }
        let outstanding = snapshot
            .owned
            .response_custodians_v1()
            .iter()
            .any(|signer| p.slots.get(signer).is_none_or(|slot| slot.finished.is_none()));
        if outstanding {
            if now < p.respond_until_daa {
                return Ok(OwnerPlanV1::Wait { until_daa: p.respond_until_daa });
            }
            if now >= c.terms.collect_until_daa {
                return Ok(OwnerPlanV1::Expired);
            }
            if owned.spent_v1().moves >= c.plan.envelope.maximum_moves {
                return Err("TIR native physical move allocation is exhausted".into());
            }
            return Ok(OwnerPlanV1::Action(snapshot.action_v1(
                c.owner,
                c.terms.collect_until_daa,
                PalwTirPublicOwnedActionV1::Collect { demand_id: p.demand_id },
            )?));
        }
        if intent.is_none() {
            for signer in snapshot.owned.response_custodians_v1() {
                check_body_reference_v1(
                    snapshot,
                    source,
                    p.slots
                        .get(signer)
                        .and_then(|slot| slot.finished.as_ref())
                        .ok_or("TIR mandatory slot lost its accepted body reference")?,
                    None,
                    ledger,
                )?;
            }
            return Ok(OwnerPlanV1::AcquisitionEof { demand_id: p.demand_id });
        }
    }
    let Some(intent) = intent else {
        return Ok(if now >= c.terms.stream_until_daa {
            OwnerPlanV1::Expired
        } else {
            OwnerPlanV1::Wait { until_daa: c.terms.stream_until_daa }
        });
    };
    let spent = owned.spent_v1();
    if now >= c.terms.stream_until_daa || spent.moves >= c.plan.envelope.maximum_moves || spent.turns >= c.plan.maximum_turns {
        return Ok(OwnerPlanV1::Expired);
    }
    let (request, reacquire) = match &intent {
        OwnerIntentV1::Fresh(request) => (*request, false),
        OwnerIntentV1::Reacquire { previous, proof } => {
            check_body_reference_v1(snapshot, source, previous, Some(proof), ledger)?;
            // Reacquire ALWAYS needs the accepted history path, even if current.
            let (count, root) = owned.history_v1();
            if proof.leaf_hash != previous.hash_v1()?
                || proof.siblings.len() > 64
                || Some(step_opening_root_capped_v1(count, proof, u64::MAX).map_err(|e| e.to_string())?) != root
            {
                return Err("TIR reacquisition lacks current accepted history membership".into());
            }
            (previous.request, true)
        }
    };
    let span = owned.response_span_v1(source, request)?;
    let Some(until) = request_cutoff_v1(c.terms.stream_until_daa, span, now) else { return Ok(OwnerPlanV1::Expired) };
    let amount = match request {
        PalwTirPublicRequestV1::StepRun { count, .. } => u64::from(count),
        _ => 1,
    };
    let (used, maximum) = if reacquire {
        (spent.reacquire_logical, c.plan.maximum_reacquire_logical_units)
    } else {
        (spent.fresh_logical, c.plan.maximum_fresh_logical_units)
    };
    if used.checked_add(amount).is_none_or(|next| next > maximum) {
        return Err("TIR request exceeds the immutable native logical acquisition allocation".into());
    }
    let action = match intent {
        OwnerIntentV1::Fresh(request) => PalwTirPublicOwnedActionV1::Demand { request },
        OwnerIntentV1::Reacquire { previous, proof } => {
            spent.reacquire_nonce.checked_add(1).ok_or("TIR reacquisition nonce overflow")?;
            PalwTirPublicOwnedActionV1::Reacquire { previous: previous.clone(), proof: proof.clone(), nonce: spent.reacquire_nonce }
        }
    };
    Ok(OwnerPlanV1::Action(snapshot.action_v1(c.owner, until, action)?))
}

/// Operational per-attempt limits. A native paid permit is a SEPARATE prerequisite;
/// neither the backend's Need nor a host-memory ticket proves native admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ResponsePreparationLimitsV1 {
    pub maximum_resident_ram_bytes: u64,
    pub maximum_prepare_io_bytes: u64,
    pub maximum_replay_leaves: u64,
    pub maximum_replay_positions: u64,
    pub maximum_replay_id_words: u64,
    pub maximum_response_io_bytes: u64,
    pub maximum_response_tree_hashes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ResponseAttemptWorkV1 {
    pub preparation_io_upper: u64,
    pub replay_leaves_upper: u64,
    pub replay_positions_upper: u64,
    pub replay_id_words_upper: u64,
    pub response_io_upper: u64,
    pub response_tree_hashes_upper: u64,
}

pub(super) enum UnitAuthenticationV1 {
    OriginalRoot(PalwTirPublicAuthenticatedUnitV1),
    /// Owned Ids(offset=0) is the complete canonical chunk sequence. One chunk
    /// alone cannot acknowledge that native request or authorize rows/tiles.
    ProvisionalIds {
        offset: u32,
    },
}

pub(super) struct PreparedResponseUnitV1 {
    source_id: Hash64,
    request: PalwTirPublicRequestV1,
    encoded_unit: Vec<u8>,
    authentication: UnitAuthenticationV1,
    pub work: ResponseAttemptWorkV1,
    _reservation: PalwMemoryReservationV1,
}

impl PreparedResponseUnitV1 {
    pub(super) fn source_id_v1(&self) -> Hash64 {
        self.source_id
    }
    pub(super) fn request_v1(&self) -> PalwTirPublicRequestV1 {
        self.request
    }
    pub(super) fn encoded_unit_v1(&self) -> &[u8] {
        &self.encoded_unit
    }
    pub(super) fn authentication_v1(&self) -> &UnitAuthenticationV1 {
        &self.authentication
    }

    pub(super) fn into_body_v1(
        self,
        snapshot: &BorrowedPaidSnapshotV1<'_>,
        source: &PalwTirPublicSourceV1,
        ledger: &mut OperationalLedgerV1,
    ) -> Result<PreparedLogicalBodyV1, String> {
        ledger.require_current_v1(snapshot, source, PalwTirPublicWorkPhaseV1::Response, self.request)?;
        let UnitAuthenticationV1::OriginalRoot(authenticated) = self.authentication else {
            return Err("a provisional ID chunk is not a completed owned logical body".into());
        };
        ledger.debit_v1(
            self.source_id,
            OperationalWorkV1 { attempts: 1, hash_bytes: self.encoded_unit.len() as u64, ..Default::default() },
        )?;
        Ok(PreparedLogicalBodyV1 {
            source_id: self.source_id,
            request: self.request,
            body_hash: palw_tir_public_body_hash_v1(&self.encoded_unit),
            bytes: self.encoded_unit,
            authenticated: PalwTirPublicFinishedV1::Unit(authenticated),
            _reservation: Arc::new(self._reservation),
        })
    }
}

pub(super) struct PreparedLogicalBodyV1 {
    source_id: Hash64,
    request: PalwTirPublicRequestV1,
    body_hash: Hash64,
    bytes: Vec<u8>,
    authenticated: PalwTirPublicFinishedV1,
    _reservation: Arc<PalwMemoryReservationV1>,
}

impl PreparedLogicalBodyV1 {
    pub(super) fn authenticated_v1(&self) -> &PalwTirPublicFinishedV1 {
        &self.authenticated
    }
}

/// Process one original canonical ID chunk at a time. This avoids retaining one
/// full-decode unit RAM ticket for every chunk of the same logical ID body.
pub(super) struct IdsBodyPreparationV1 {
    source_id: Hash64,
    offset: u32,
    maximum_bytes: usize,
    bytes: Vec<u8>,
    assembler: kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicIdsAssemblerV1,
    sealed: Option<PalwTirPublicAuthenticatedIdsV1>,
    failed: bool,
    _reservation: Arc<PalwMemoryReservationV1>,
}

impl IdsBodyPreparationV1 {
    pub(super) fn start_v1(
        snapshot: &BorrowedPaidSnapshotV1<'_>,
        source: &PalwTirPublicSourceV1,
        claim: Hash64,
        maximum_body_ram_bytes: u64,
        ledger: &mut OperationalLedgerV1,
    ) -> Result<Self, String> {
        ledger.require_current_v1(snapshot, source, PalwTirPublicWorkPhaseV1::Ids, PalwTirPublicRequestV1::Ids { offset: 0 })?;
        ledger.debit_v1(source.source_id_v1(), metadata_work_v1(source)?)?;
        source.check_request_v1(&PalwTirPublicRequestV1::Ids { offset: 0 })?;
        let (maximum_bytes, ram) = request_body_bound_v1(source, PalwTirPublicRequestV1::Ids { offset: 0 })?;
        if ram > maximum_body_ram_bytes {
            return Err("complete TIR ID body exceeds its held operational allocation".into());
        }
        let reservation = host_ledger_v1()
            .reserve(
                PalwMemoryReservationKeyV1 {
                    role: "public-tir-ids-body",
                    class_id: source.header_v1().job_context.shape_profile_id,
                    job: claim,
                },
                ram,
            )
            .map_err(|e| e.to_string())?;
        let maximum_bytes = usize::try_from(maximum_bytes).map_err(|_| "TIR ID body exceeds native usize")?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(maximum_bytes).map_err(|e| e.to_string())?;
        Ok(Self {
            source_id: source.source_id_v1(),
            offset: 0,
            maximum_bytes,
            bytes,
            assembler: Default::default(),
            sealed: None,
            failed: false,
            _reservation: Arc::new(reservation),
        })
    }

    pub(super) fn next_chunk_v1(&self, source: &PalwTirPublicSourceV1) -> Result<Option<PalwTirPublicRequestV1>, String> {
        if self.failed {
            return Err("TIR ID preparation is poisoned after a failed attempt".into());
        }
        if source.source_id_v1() != self.source_id {
            return Err("TIR ID preparation changes its original source".into());
        }
        Ok((self.offset < source.decode_count_v1()).then_some(PalwTirPublicRequestV1::Ids { offset: self.offset }))
    }

    pub(super) fn push_v1(
        &mut self,
        snapshot: &BorrowedPaidSnapshotV1<'_>,
        source: &PalwTirPublicSourceV1,
        chunk: PreparedResponseUnitV1,
        ledger: &mut OperationalLedgerV1,
    ) -> Result<(), String> {
        let mut failed = self.failed;
        let result = run_fail_closed_v1(&mut failed, || {
            ledger.require_current_v1(snapshot, source, PalwTirPublicWorkPhaseV1::Ids, chunk.request)?;
            if ledger.scope_v1()?.input_offset != u64::from(self.offset) {
                return Err("TIR ID work permit changes exact assembly offset".into());
            }
            self.push_inner_v1(source, chunk, ledger)
        });
        self.failed = failed;
        result
    }

    fn push_inner_v1(
        &mut self,
        source: &PalwTirPublicSourceV1,
        chunk: PreparedResponseUnitV1,
        ledger: &mut OperationalLedgerV1,
    ) -> Result<(), String> {
        let request = self.next_chunk_v1(source)?.ok_or("TIR ID body already reached local EOF")?;
        let bound = source.unit_bound_v1(&request)?;
        let words = u64::from((source.decode_count_v1() - self.offset).min(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1));
        let complete = u64::from(self.offset).checked_add(words) == Some(u64::from(source.decode_count_v1()));
        // The in-place core assembler appends once and clones only at EOF.
        ledger.debit_v1(
            source.source_id_v1(),
            OperationalWorkV1 {
                attempts: 1,
                io_bytes: chunk.encoded_unit.len() as u64,
                parser_steps: bound.maximum_parser_steps,
                hash_bytes: bound
                    .maximum_hash_input_bytes
                    .checked_add(if complete { u64::from(source.decode_count_v1()) * 4 } else { 0 })
                    .ok_or("TIR ID hash work overflow")?,
                id_copy_words: words
                    .checked_mul(4)
                    .and_then(|n| n.checked_add(if complete { u64::from(source.decode_count_v1()) } else { 0 }))
                    .ok_or("TIR ID copy work overflow")?,
                ..Default::default()
            },
        )?;
        if chunk.source_id != self.source_id
            || chunk.request != request
            || !matches!(chunk.authentication, UnitAuthenticationV1::ProvisionalIds { offset } if offset == self.offset)
            || self.bytes.len().checked_add(chunk.encoded_unit.len()).is_none_or(|end| end > self.maximum_bytes)
        {
            return Err("TIR ID chunk changes source/order or logical-body allocation".into());
        }
        let response = source.decode_response_v1(&request, &chunk.encoded_unit)?;
        let sealed = self.assembler.push_v1(source, &request, &response)?;
        let next = self
            .offset
            .checked_add((source.decode_count_v1() - self.offset).min(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1))
            .ok_or("TIR ID offset overflow")?;
        self.bytes.extend_from_slice(&chunk.encoded_unit);
        self.offset = next;
        self.sealed = sealed;
        Ok(())
    }

    pub(super) fn finish_v1(
        self,
        snapshot: &BorrowedPaidSnapshotV1<'_>,
        source: &PalwTirPublicSourceV1,
        ledger: &mut OperationalLedgerV1,
    ) -> Result<PreparedLogicalBodyV1, String> {
        ledger.require_current_v1(snapshot, source, PalwTirPublicWorkPhaseV1::Response, PalwTirPublicRequestV1::Ids { offset: 0 })?;
        if self.failed {
            return Err("TIR ID preparation is poisoned after a failed attempt".into());
        }
        ledger.debit_v1(
            source.source_id_v1(),
            OperationalWorkV1 { attempts: 1, hash_bytes: self.bytes.len() as u64, ..Default::default() },
        )?;
        if source.source_id_v1() != self.source_id || self.offset != source.decode_count_v1() || self.bytes.len() != self.maximum_bytes
        {
            return Err("TIR ID logical body has not reached exact original EOF".into());
        }
        let authenticated = self.sealed.ok_or("TIR ID logical body never authenticated the original outer trace hash")?;
        Ok(PreparedLogicalBodyV1 {
            source_id: self.source_id,
            request: PalwTirPublicRequestV1::Ids { offset: 0 },
            body_hash: palw_tir_public_body_hash_v1(&self.bytes),
            bytes: self.bytes,
            authenticated: PalwTirPublicFinishedV1::Ids(authenticated),
            _reservation: self._reservation,
        })
    }
}

pub(super) enum ResponderPlanV1 {
    Action(ScopedActionV1),
    Finished,
    Expired,
    ImmutableBodyConflict,
}

fn body_part_range_v1(total: usize, accepted_offset: u64) -> Result<Option<std::ops::Range<usize>>, String> {
    let offset = usize::try_from(accepted_offset).map_err(|_| "TIR accepted offset exceeds usize")?;
    if offset > total || (offset != total && !offset.is_multiple_of(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1)) {
        return Err("TIR accepted cursor is outside its canonical original body".into());
    }
    Ok((offset < total).then(|| offset..offset + (total - offset).min(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1)))
}

pub(super) fn responder_next_v1(
    snapshot: &BorrowedPaidSnapshotV1<'_>,
    source: &PalwTirPublicSourceV1,
    signer: PalwBondKeyV2,
    body: &PreparedLogicalBodyV1,
    ledger: &mut OperationalLedgerV1,
) -> Result<ResponderPlanV1, String> {
    ledger.require_current_v1(snapshot, source, PalwTirPublicWorkPhaseV1::Response, body.request)?;
    if ledger.scope_v1()?.actor != signer {
        return Err("TIR response work permit changes literal signer".into());
    }
    ledger.debit_v1(source.source_id_v1(), metadata_work_v1(source)?)?;
    snapshot.source_v1(source)?;
    let c = snapshot.owned.contract_v1();
    let p = snapshot.owned.active_v1().ok_or("TIR response has no native pending request")?;
    if signer != c.owner && !snapshot.owned.response_custodians_v1().contains(&signer) {
        return Err("TIR response signer has no literal protected slot".into());
    }
    if body.source_id != c.source_id || body.request != p.request {
        return Err("TIR prepared body answers another original source/request".into());
    }
    if p.collected
        || snapshot.now_daa < p.opened_daa
        || snapshot.now_daa >= p.respond_until_daa
        || snapshot.now_daa >= c.terms.stream_until_daa
    {
        return Ok(ResponderPlanV1::Expired);
    }
    if let Some(slot) = p.slots.get(&signer) {
        if slot.body_hash != body.body_hash || slot.total_bytes != body.bytes.len() as u64 {
            return Ok(ResponderPlanV1::ImmutableBodyConflict);
        }
        if let Some(reference) = slot.finished.as_ref() {
            check_body_reference_v1(snapshot, source, reference, None, ledger)?;
            return Ok(ResponderPlanV1::Finished);
        }
    }
    if snapshot.owned.spent_v1().moves >= c.plan.envelope.maximum_moves {
        return Err("TIR native physical move allocation is exhausted".into());
    }
    let action = match p.slots.get(&signer) {
        None => PalwTirPublicOwnedActionV1::Begin {
            demand_id: p.demand_id,
            answerer: signer,
            total_bytes: body.bytes.len() as u64,
            body_hash: body.body_hash,
        },
        Some(slot) => match body_part_range_v1(body.bytes.len(), slot.accepted_offset)? {
            None => PalwTirPublicOwnedActionV1::Finish { demand_id: p.demand_id, answerer: signer },
            Some(range) => {
                ledger.debit_v1(
                    source.source_id_v1(),
                    OperationalWorkV1 { attempts: 1, io_bytes: range.len() as u64, ..Default::default() },
                )?;
                PalwTirPublicOwnedActionV1::Part {
                    demand_id: p.demand_id,
                    answerer: signer,
                    offset: slot.accepted_offset,
                    bytes: body.bytes[range].to_vec(),
                }
            }
        },
    };
    let mut action = snapshot.action_v1(signer, p.respond_until_daa, action)?;
    action._reservation = Some(body._reservation.clone());
    Ok(ResponderPlanV1::Action(action))
}

/// Lookup context from the CURRENT native snapshot. Returned bytes may be an
/// untrusted local/P2P copy; only the accepted immutable head authenticates them.
pub(super) struct AcceptedChunkReadV1<'a> {
    pub snapshot_block: BlockHash,
    pub contract: &'a kaspa_consensus_core::palw_tir_public_owned_v1::PalwTirPublicContractV1,
    pub reference: &'a PalwTirPublicAcceptedBodyRefV1,
    pub head: Hash64,
    pub ending_offset: u64,
}

pub(super) trait AcceptedChunkReaderV1 {
    /// One bounded record or None while public history is unavailable. None is
    /// not a new DA default, and never advances an accepted slot or local cache.
    fn poll_chunk_v1(&mut self, request: AcceptedChunkReadV1<'_>) -> Result<Option<PalwTirPublicChunkRecordV1>, String>;
}

pub(super) struct RecoveredBodyV1 {
    reference: PalwTirPublicAcceptedBodyRefV1,
    body: PreparedLogicalBodyV1,
}

impl RecoveredBodyV1 {
    /// Required again before consuming a cached body after a native snapshot
    /// change. A local file/tip/old membership path is not accepted authority.
    pub(super) fn current_v1(
        &self,
        snapshot: &BorrowedPaidSnapshotV1<'_>,
        source: &PalwTirPublicSourceV1,
        history: Option<&PalwStepOpeningV1>,
        ledger: &mut OperationalLedgerV1,
    ) -> Result<&PreparedLogicalBodyV1, String> {
        let scope = *ledger.scope_v1()?;
        ledger.require_current_v1(snapshot, source, scope.phase, scope.request)?;
        check_body_reference_v1(snapshot, source, &self.reference, history, ledger)?;
        Ok(&self.body)
    }
}

pub(super) enum BodyRecoveryPollV1 {
    Pending,
    /// Root-authenticated acquisition EOF only; not proof Ready or conviction.
    Complete(RecoveredBodyV1),
}

pub(super) struct AcceptedBodyRecoveryV1 {
    reference: PalwTirPublicAcceptedBodyRefV1,
    bytes: Vec<u8>,
    head: Hash64,
    remaining: u64,
    records: u64,
    reservation: Option<Arc<PalwMemoryReservationV1>>,
    failed: bool,
}

fn checked_chunk_range_v1(
    reference: &PalwTirPublicAcceptedBodyRefV1,
    head: Hash64,
    remaining: u64,
    records: u64,
    record: &PalwTirPublicChunkRecordV1,
) -> Result<std::ops::Range<usize>, String> {
    let expected_records = reference.total_bytes.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64);
    if records >= expected_records
        || record.demand_id != reference.demand_id
        || record.answerer != reference.answerer
        || record.head_v1()? != head
        || !record.offset.is_multiple_of(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64)
        || record.offset.checked_add(record.bytes.len() as u64) != Some(remaining)
        || record.offset >= reference.total_bytes
        || record.bytes.len() as u64 != (reference.total_bytes - record.offset).min(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64)
    {
        return Err("untrusted TIR chunk copy changes accepted scope/signer/head/canonical offset".into());
    }
    let first = usize::try_from(record.offset).map_err(|_| "TIR chunk offset exceeds usize")?;
    let end = usize::try_from(remaining).map_err(|_| "TIR chunk end exceeds usize")?;
    Ok(first..end)
}

impl AcceptedBodyRecoveryV1 {
    pub(super) fn start_v1(
        snapshot: &BorrowedPaidSnapshotV1<'_>,
        source: &PalwTirPublicSourceV1,
        reference: &PalwTirPublicAcceptedBodyRefV1,
        history: Option<&PalwStepOpeningV1>,
        maximum_body_ram_bytes: u64,
        ledger: &mut OperationalLedgerV1,
    ) -> Result<Self, String> {
        ledger.require_current_v1(snapshot, source, PalwTirPublicWorkPhaseV1::BodyRead, reference.request)?;
        let scope = ledger.scope_v1()?;
        if scope.input_head != reference.hash_v1()?
            || scope.cursor_id != reference.chunk_head
            || scope.input_offset != reference.total_bytes
        {
            return Err("TIR recovery start changes exact native accepted body cursor".into());
        }
        check_body_reference_v1(snapshot, source, reference, history, ledger)?;
        let (_, ram) = request_body_bound_v1(source, reference.request)?;
        if ram > maximum_body_ram_bytes {
            return Err("TIR accepted-body recovery exceeds its held operational allocation".into());
        }
        let reservation = host_ledger_v1()
            .reserve(
                PalwMemoryReservationKeyV1 {
                    role: "public-tir-accepted-body",
                    class_id: source.header_v1().job_context.shape_profile_id,
                    job: snapshot.owned.contract_v1().claim,
                },
                ram,
            )
            .map_err(|e| e.to_string())?;
        let size = usize::try_from(reference.total_bytes).map_err(|_| "TIR accepted body exceeds native usize")?;
        ledger.debit_v1(
            source.source_id_v1(),
            OperationalWorkV1 { attempts: 1, io_bytes: reference.total_bytes, ..Default::default() },
        )?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(size).map_err(|e| e.to_string())?;
        bytes.resize(size, 0);
        Ok(Self {
            reference: reference.clone(),
            bytes,
            head: reference.chunk_head,
            remaining: reference.total_bytes,
            records: 0,
            reservation: Some(Arc::new(reservation)),
            failed: false,
        })
    }

    /// Local offsets cache copies only. Every chunk and final emission rechecks
    /// the current native reference; no cache progress authorizes a native move.
    pub(super) fn poll_v1(
        &mut self,
        snapshot: &BorrowedPaidSnapshotV1<'_>,
        source: &PalwTirPublicSourceV1,
        history: Option<&PalwStepOpeningV1>,
        ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
        reader: &mut dyn AcceptedChunkReaderV1,
        ledger: &mut OperationalLedgerV1,
    ) -> Result<BodyRecoveryPollV1, String> {
        let mut failed = self.failed;
        let result = run_fail_closed_v1(&mut failed, || {
            let phase =
                if self.remaining == 0 { PalwTirPublicWorkPhaseV1::BodyAuthenticate } else { PalwTirPublicWorkPhaseV1::BodyRead };
            ledger.require_current_v1(snapshot, source, phase, self.reference.request)?;
            let scope = ledger.scope_v1()?;
            let (cursor, offset) = if self.remaining == 0 {
                (self.reference.chunk_head, self.reference.total_bytes)
            } else {
                (self.head, self.remaining)
            };
            if scope.input_head != self.reference.hash_v1()? || scope.cursor_id != cursor || scope.input_offset != offset {
                return Err("TIR recovery permit changes exact accepted body/chunk/native cursor".into());
            }
            self.poll_inner_v1(snapshot, source, history, ids, reader, ledger)
        });
        self.failed = failed;
        result
    }

    fn poll_inner_v1(
        &mut self,
        snapshot: &BorrowedPaidSnapshotV1<'_>,
        source: &PalwTirPublicSourceV1,
        history: Option<&PalwStepOpeningV1>,
        ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
        reader: &mut dyn AcceptedChunkReaderV1,
        ledger: &mut OperationalLedgerV1,
    ) -> Result<BodyRecoveryPollV1, String> {
        check_body_reference_v1(snapshot, source, &self.reference, history, ledger)?;
        if self.reservation.is_none() {
            return Err("TIR accepted-body recovery has already emitted its bytes".into());
        }
        let expected_records = self.reference.total_bytes.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64);
        if self.remaining != 0 {
            // Charge a bounded read/hash attempt BEFORE untrusted history I/O.
            // None and rejected copies consume this allocation too.
            let record_bytes = PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64 + 1024;
            ledger.debit_v1(
                source.source_id_v1(),
                OperationalWorkV1 {
                    attempts: 1,
                    io_bytes: record_bytes * 2,
                    hash_bytes: record_bytes,
                    parser_steps: record_bytes * 16,
                    ..Default::default()
                },
            )?;
            let Some(record) = reader.poll_chunk_v1(AcceptedChunkReadV1 {
                snapshot_block: snapshot.snapshot_block,
                contract: snapshot.owned.contract_v1(),
                reference: &self.reference,
                head: self.head,
                ending_offset: self.remaining,
            })?
            else {
                return Ok(BodyRecoveryPollV1::Pending);
            };
            let range = checked_chunk_range_v1(&self.reference, self.head, self.remaining, self.records, &record)?;
            self.bytes.get_mut(range).ok_or("TIR chunk is outside the accepted body")?.copy_from_slice(&record.bytes);
            self.head = record.previous;
            self.remaining = record.offset;
            self.records += 1;
            // EOF copy is not authentication. A separate accepted final-auth
            // debit is required on the next invocation, even for the last part.
            return Ok(BodyRecoveryPollV1::Pending);
        }
        ledger.debit_v1(
            source.source_id_v1(),
            OperationalWorkV1 { attempts: 1, hash_bytes: self.bytes.len() as u64, ..Default::default() },
        )?;
        if self.head != Hash64::default()
            || self.records != expected_records
            || palw_tir_public_body_hash_v1(&self.bytes) != self.reference.body_hash
        {
            return Err("TIR public body never reached exact accepted head/checksum EOF".into());
        }
        let authenticated = if self.reference.request == (PalwTirPublicRequestV1::Ids { offset: 0 }) {
            let mut assembler = kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicIdsAssemblerV1::default();
            let mut at = 0usize;
            let mut sealed = None;
            for offset in (0..source.decode_count_v1()).step_by(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize) {
                let request = PalwTirPublicRequestV1::Ids { offset };
                let bound = source.unit_bound_v1(&request)?;
                let length = usize::try_from(bound.maximum_response_bytes).map_err(|_| "TIR ID chunk exceeds usize")?;
                let words = u64::from((source.decode_count_v1() - offset).min(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1));
                let complete = u64::from(offset) + words == u64::from(source.decode_count_v1());
                ledger.debit_v1(
                    source.source_id_v1(),
                    OperationalWorkV1 {
                        attempts: 1,
                        parser_steps: bound.maximum_parser_steps,
                        hash_bytes: bound
                            .maximum_hash_input_bytes
                            .checked_add(if complete { u64::from(source.decode_count_v1()) * 4 } else { 0 })
                            .ok_or("TIR ID recovery hash work overflow")?,
                        id_copy_words: words
                            .checked_mul(4)
                            .and_then(|n| n.checked_add(if complete { u64::from(source.decode_count_v1()) } else { 0 }))
                            .ok_or("TIR ID recovery copy work overflow")?,
                        ..Default::default()
                    },
                )?;
                let end = at.checked_add(length).ok_or("TIR ID body offset overflow")?;
                let raw = self.bytes.get(at..end).ok_or("TIR accepted ID body is truncated")?;
                let response = source.decode_response_v1(&request, raw)?;
                sealed = assembler.push_v1(source, &request, &response)?;
                at = end;
            }
            if at != self.bytes.len() {
                return Err("TIR accepted ID body has trailing bytes".into());
            }
            PalwTirPublicFinishedV1::Ids(sealed.ok_or("TIR accepted IDs never sealed the original trace hash")?)
        } else {
            let bound = source.unit_bound_v1(&self.reference.request)?;
            ledger.debit_v1(
                source.source_id_v1(),
                OperationalWorkV1 {
                    attempts: 1,
                    parser_steps: bound.maximum_parser_steps,
                    hash_bytes: bound.maximum_hash_input_bytes,
                    ..Default::default()
                },
            )?;
            let response = source.decode_response_v1(&self.reference.request, &self.bytes)?;
            PalwTirPublicFinishedV1::Unit(source.authenticate_v1(&self.reference.request, &response, ids)?)
        };
        // Reconfirm before returning the marker; completed bytes are never an
        // authority to increment native turn/offset/nonce on a different fork.
        check_body_reference_v1(snapshot, source, &self.reference, history, ledger)?;
        let body = PreparedLogicalBodyV1 {
            source_id: source.source_id_v1(),
            request: self.reference.request,
            body_hash: self.reference.body_hash,
            bytes: std::mem::take(&mut self.bytes),
            authenticated,
            _reservation: self.reservation.take().ok_or("TIR recovered body lost its host allocation")?,
        };
        Ok(BodyRecoveryPollV1::Complete(RecoveredBodyV1 { reference: self.reference.clone(), body }))
    }
}

pub(super) enum ResponsePreparationV1 {
    /// Bounded source preparation, not a response, EOF, missing-duty proof or ack.
    AwaitSource {
        work: ResponseAttemptWorkV1,
    },
    Prepared(PreparedResponseUnitV1),
}

fn response_work_v1(
    need: &PalwTirPublicCaptureNeedV1,
    limits: &ResponsePreparationLimitsV1,
    unit_ram: u64,
) -> Result<ResponseAttemptWorkV1, String> {
    if unit_ram > limits.maximum_resident_ram_bytes
        || need.maximum_prepare_io_bytes > limits.maximum_prepare_io_bytes
        || need.maximum_replay_leaves > limits.maximum_replay_leaves
        || need.maximum_replay_positions > limits.maximum_replay_positions
        || need.maximum_replay_id_words > limits.maximum_replay_id_words
        || need.maximum_response_io_bytes > limits.maximum_response_io_bytes
        || need.maximum_response_tree_hashes > limits.maximum_response_tree_hashes
    {
        return Err("TIR public response exceeds its explicit operational allocation".into());
    }
    Ok(ResponseAttemptWorkV1 {
        preparation_io_upper: need.maximum_prepare_io_bytes,
        replay_leaves_upper: need.maximum_replay_leaves,
        replay_positions_upper: need.maximum_replay_positions,
        replay_id_words_upper: need.maximum_replay_id_words,
        response_io_upper: need.maximum_response_io_bytes,
        response_tree_hashes_upper: need.maximum_response_tree_hashes,
    })
}

fn check_ids_chunk_v1(source: &PalwTirPublicSourceV1, offset: u32, response: &PalwTirPublicResponseV1) -> Result<(), String> {
    let PalwTirPublicResponseV1::Ids { ids, .. } = response else {
        return Err("TIR ID chunk reader returned another response family".into());
    };
    let length = source
        .decode_count_v1()
        .checked_sub(offset)
        .ok_or("TIR ID chunk starts past the original decode")?
        .min(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1);
    if length == 0 || ids.len() != length as usize {
        return Err("TIR ID response is not its canonical original chunk".into());
    }
    Ok(())
}

/// Caller already holds the original-capture preparation RAM/disk lease and a
/// native paid response permit. This function adds the exact unit host ticket
/// BEFORE polling/serializing. Returned bytes are not a native body declaration:
/// the owned module's full-body hash/codec must build Begin/Part/Finish from them.
/// In particular, provisional ID chunks require the complete all-D owned codec.
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_response_v1(
    snapshot: &BorrowedPaidSnapshotV1<'_>,
    source: &PalwTirPublicSourceV1,
    request: PalwTirPublicRequestV1,
    ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
    reader: &mut dyn PalwTirPublicCaptureReaderV1,
    quote: &kaspa_consensus_core::palw_backend::PalwTirPublicCaptureSliceV1,
    capture_need: &PalwTirPublicCaptureNeedV1,
    limits: &ResponsePreparationLimitsV1,
    claim: Hash64,
    ledger: &mut OperationalLedgerV1,
) -> Result<ResponsePreparationV1, String> {
    let phase = ledger.scope_v1()?.phase;
    if !matches!(phase, PalwTirPublicWorkPhaseV1::Capture | PalwTirPublicWorkPhaseV1::Response) {
        return Err("TIR capture slice has another native work phase".into());
    }
    if let Some(preparation) = snapshot.preparation {
        if phase == PalwTirPublicWorkPhaseV1::Capture {
            return Err("TIR short DA response cannot perform claim-wide cold preparation".into());
        }
        let actor = ledger.scope_v1()?.actor;
        if actor != snapshot.owned.contract_v1().owner {
            let ready = preparation.ready_v1(&actor).ok_or("TIR literal responder has no current original Ready promise")?;
            if reader.prepared_descriptor_v1() != Some(&ready.descriptor) {
                return Err("TIR response reader is not the literal native Ready descriptor's warm index".into());
            }
        }
    }
    ledger.require_current_v1(snapshot, source, phase, request)?;
    let scope = ledger.scope_v1()?;
    if scope.claim != claim
        || scope.cursor_id != quote.cursor_id
        || scope.slice != quote.sequence
        || quote.source_id != source.source_id_v1()
        || scope.input_head != source.source_id_v1()
        || quote.resident_ram_bytes > capture_need.resident_ram_bytes
    {
        return Err("TIR capture quote changes exact native paid source/sequence/cursor/holding".into());
    }
    let bound = source.unit_bound_v1(&request)?;
    response_work_v1(capture_need, limits, bound.resident_ram_bytes)?;
    let work = ResponseAttemptWorkV1 {
        preparation_io_upper: quote.work.io_bytes,
        replay_leaves_upper: quote.work.replay_leaves,
        replay_positions_upper: quote.work.replay_positions,
        replay_id_words_upper: quote.work.replay_id_words,
        response_io_upper: quote.work.io_bytes,
        response_tree_hashes_upper: quote.work.tree_hashes,
    };
    // Exact bounded slice + local codec/authentication work, before poll. The
    // FULL grant was already consumed in native state; None/Err never refund it.
    ledger.debit_v1(
        source.source_id_v1(),
        OperationalWorkV1 {
            attempts: 1,
            io_bytes: quote
                .work
                .io_bytes
                .checked_add(bound.maximum_response_bytes.checked_mul(2).ok_or("TIR response I/O work overflow")?)
                .ok_or("TIR response I/O work overflow")?,
            hash_bytes: quote
                .work
                .hash_bytes
                .checked_add(quote.work.tree_hashes.checked_mul(128).ok_or("TIR response hash work overflow")?)
                .and_then(|n| n.checked_add(bound.maximum_hash_input_bytes))
                .ok_or("TIR response hash work overflow")?,
            parser_steps: quote
                .work
                .io_bytes
                .checked_mul(16)
                .and_then(|n| n.checked_add(bound.maximum_parser_steps))
                .ok_or("TIR response parser work overflow")?,
            tree_hashes: quote.work.tree_hashes,
            replay_leaves: quote.work.replay_leaves,
            replay_positions: quote.work.replay_positions,
            replay_id_words: quote.work.replay_id_words,
            arithmetic_work: quote.work.arithmetic_work,
            traffic_bytes: quote.work.traffic_bytes,
            id_copy_words: quote.work.replay_id_words,
            copy_bytes: bound.maximum_response_bytes.checked_mul(4).ok_or("TIR response copy work overflow")?,
            ..Default::default()
        },
    )?;
    let reservation = host_ledger_v1()
        .reserve(
            PalwMemoryReservationKeyV1 {
                role: "public-tir-response-unit",
                class_id: source.header_v1().job_context.shape_profile_id,
                job: claim,
            },
            bound.resident_ram_bytes,
        )
        .map_err(|e| e.to_string())?;
    let Some(response) = reader.poll_quoted_v1(&request, quote)? else {
        return Ok(ResponsePreparationV1::AwaitSource { work });
    };
    if borsh::object_length(&response).map_err(|e| e.to_string())? as u64 > bound.maximum_response_bytes {
        return Err("TIR capture response exceeds its exact native logical-unit bound".into());
    }
    let authentication = match request {
        PalwTirPublicRequestV1::Ids { offset } => {
            check_ids_chunk_v1(source, offset, &response)?;
            UnitAuthenticationV1::ProvisionalIds { offset }
        }
        _ => UnitAuthenticationV1::OriginalRoot(source.authenticate_v1(&request, &response, ids)?),
    };
    let encoded_unit = borsh::to_vec(&response).map_err(|e| e.to_string())?;
    Ok(ResponsePreparationV1::Prepared(PreparedResponseUnitV1 {
        source_id: source.source_id_v1(),
        request,
        encoded_unit,
        authentication,
        work,
        _reservation: reservation,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cumulative_operational_debit_counts_pending_and_error_without_refund() {
        let unit = OperationalWorkV1 { attempts: 1, io_bytes: 7, hash_bytes: 11, parser_steps: 13, ..Default::default() };
        let mut ledger = OperationalLedgerV1::new_v1(
            hash(1),
            OperationalWorkV1 { attempts: 2, io_bytes: 14, hash_bytes: 22, parser_steps: 26, ..Default::default() },
        );
        ledger.debit_v1(hash(1), unit).unwrap();
        let pending: Result<Option<()>, String> = Ok(None);
        assert!(pending.unwrap().is_none());
        ledger.debit_v1(hash(1), unit).unwrap();
        let failed: Result<(), String> = Err("untrusted copy rejected".into());
        assert!(failed.is_err());
        let spent = ledger.spent_v1();
        assert_eq!(spent.attempts, 2);
        assert!(ledger.debit_v1(hash(1), unit).is_err());
        assert_eq!(ledger.spent_v1(), spent, "failed admission is atomic, completed attempts are never refunded");
        assert!(ledger.debit_v1(hash(2), OperationalWorkV1::default()).is_err());
    }

    #[test]
    fn operational_work_dimensions_and_overflow_never_partially_advance() {
        for work in [
            OperationalWorkV1 { io_bytes: 1, ..Default::default() },
            OperationalWorkV1 { hash_bytes: 1, ..Default::default() },
            OperationalWorkV1 { parser_steps: 1, ..Default::default() },
            OperationalWorkV1 { id_copy_words: 1, ..Default::default() },
            OperationalWorkV1 { replay_leaves: 1, ..Default::default() },
            OperationalWorkV1 { replay_positions: 1, ..Default::default() },
            OperationalWorkV1 { replay_id_words: 1, ..Default::default() },
        ] {
            let mut ledger = OperationalLedgerV1::new_v1(hash(1), OperationalWorkV1 { attempts: 2, ..Default::default() });
            assert!(ledger.debit_v1(hash(1), OperationalWorkV1 { attempts: 1, ..work }).is_err());
            assert_eq!(ledger.spent_v1(), OperationalWorkV1::default());
        }
        let mut ledger = OperationalLedgerV1::new_v1(hash(1), OperationalWorkV1 { attempts: u64::MAX, ..Default::default() });
        ledger.debit_v1(hash(1), OperationalWorkV1 { attempts: u64::MAX, ..Default::default() }).unwrap();
        assert!(ledger.debit_v1(hash(1), OperationalWorkV1 { attempts: 1, ..Default::default() }).is_err());
        assert_eq!(ledger.spent_v1().attempts, u64::MAX);
    }

    #[test]
    fn failed_recovery_or_id_assembly_cannot_repeat_retained_buffer_work() {
        let mut poisoned = false;
        let mut calls = 0;
        let mut ledger =
            OperationalLedgerV1::new_v1(hash(1), OperationalWorkV1 { attempts: 2, hash_bytes: 4096, ..Default::default() });
        let first: Result<(), String> = run_fail_closed_v1(&mut poisoned, || {
            ledger.debit_v1(hash(1), OperationalWorkV1 { attempts: 1, hash_bytes: 2048, ..Default::default() })?;
            calls += 1;
            Err("full-body hash / source decode / original EOF authentication failed".into())
        });
        assert!(first.is_err());
        let spent = ledger.spent_v1();
        for _ in 0..3 {
            assert!(
                run_fail_closed_v1(&mut poisoned, || {
                    calls += 1;
                    ledger.debit_v1(hash(1), OperationalWorkV1 { attempts: 1, hash_bytes: 2048, ..Default::default() })
                })
                .is_err()
            );
        }
        assert_eq!(calls, 1);
        assert_eq!(ledger.spent_v1(), spent);
    }

    fn hash(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn signer(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
            transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(n),
            index: 0,
        })
    }

    fn chunk_fixture() -> (PalwTirPublicAcceptedBodyRefV1, PalwTirPublicChunkRecordV1, PalwTirPublicChunkRecordV1) {
        // Chunk-copy policy fixture only, not a native accepted body credential.
        let first = PalwTirPublicChunkRecordV1 {
            demand_id: hash(1),
            answerer: signer(2),
            previous: Hash64::default(),
            offset: 0,
            bytes: vec![3; PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1],
        };
        let last = PalwTirPublicChunkRecordV1 {
            demand_id: hash(1),
            answerer: signer(2),
            previous: first.head_v1().unwrap(),
            offset: PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64,
            bytes: vec![4; 7],
        };
        let mut body = first.bytes.clone();
        body.extend_from_slice(&last.bytes);
        let reference = PalwTirPublicAcceptedBodyRefV1 {
            source_id: hash(5),
            demand_id: hash(1),
            turn: 6,
            request: PalwTirPublicRequestV1::StepLeaf { index: 7 },
            answerer: signer(2),
            body_hash: palw_tir_public_body_hash_v1(&body),
            total_bytes: body.len() as u64,
            chunk_head: last.head_v1().unwrap(),
            opened_daa: 10,
            respond_until_daa: 100,
        };
        (reference, first, last)
    }

    #[test]
    fn body_cursor_uses_only_canonical_accepted_offsets_and_exact_eof() {
        let part = PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1;
        assert_eq!(body_part_range_v1(part + 7, 0).unwrap(), Some(0..part));
        assert_eq!(body_part_range_v1(part + 7, part as u64).unwrap(), Some(part..part + 7));
        assert_eq!(body_part_range_v1(part + 7, (part + 7) as u64).unwrap(), None);
        assert!(body_part_range_v1(part + 7, 1).is_err());
        assert!(body_part_range_v1(part + 7, (part + 8) as u64).is_err());
    }

    #[test]
    fn proposal_turn_binds_fresh_requests_and_exact_pending_responses() {
        let demand = PalwTirPublicOwnedActionV1::Demand { request: PalwTirPublicRequestV1::StepLeaf { index: 7 } };
        assert_eq!(action_turn_v1(9, Some(8), &demand).unwrap(), 9);
        assert_eq!(
            action_turn_v1(10, Some(9), &demand).unwrap(),
            10,
            "a future signed wrapper must reject replay of the previous expected turn"
        );
        for action in [
            PalwTirPublicOwnedActionV1::Begin { demand_id: hash(1), answerer: signer(2), total_bytes: 7, body_hash: hash(3) },
            PalwTirPublicOwnedActionV1::Part { demand_id: hash(1), answerer: signer(2), offset: 0, bytes: vec![3] },
            PalwTirPublicOwnedActionV1::Finish { demand_id: hash(1), answerer: signer(2) },
            PalwTirPublicOwnedActionV1::Collect { demand_id: hash(1) },
        ] {
            assert_eq!(action_turn_v1(9, Some(8), &action).unwrap(), 8);
            assert!(action_turn_v1(9, None, &action).is_err());
        }
        let (previous, _, _) = chunk_fixture();
        let reacquire = PalwTirPublicOwnedActionV1::Reacquire {
            previous,
            proof: PalwStepOpeningV1 { leaf_index: 0, leaf_hash: hash(1), siblings: vec![] },
            nonce: 0,
        };
        assert_eq!(action_turn_v1(9, Some(8), &reacquire).unwrap(), 9);
    }

    #[test]
    fn public_chunk_copy_checks_full_head_scope_signer_offset_before_progress() {
        let (reference, first, last) = chunk_fixture();
        assert_eq!(
            checked_chunk_range_v1(&reference, reference.chunk_head, reference.total_bytes, 0, &last).unwrap(),
            PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1..PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 + 7
        );
        assert_eq!(
            checked_chunk_range_v1(&reference, last.previous, last.offset, 1, &first).unwrap(),
            0..PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1
        );
        for altered in [
            PalwTirPublicChunkRecordV1 { demand_id: hash(99), ..last.clone() },
            PalwTirPublicChunkRecordV1 { answerer: signer(99), ..last.clone() },
            PalwTirPublicChunkRecordV1 { previous: hash(99), ..last.clone() },
            PalwTirPublicChunkRecordV1 { offset: 1, ..last.clone() },
            PalwTirPublicChunkRecordV1 { bytes: vec![9; 7], ..last.clone() },
        ] {
            assert!(checked_chunk_range_v1(&reference, reference.chunk_head, reference.total_bytes, 0, &altered).is_err());
        }
        assert!(checked_chunk_range_v1(&reference, last.previous, reference.total_bytes, 0, &first).is_err());
        assert!(checked_chunk_range_v1(&reference, reference.chunk_head, reference.total_bytes, 2, &last).is_err());
        let mut altered_body = first.bytes;
        altered_body.extend_from_slice(&[4; 6]);
        assert_ne!(palw_tir_public_body_hash_v1(&altered_body), reference.body_hash, "partial EOF is not full body authentication");
    }

    #[test]
    fn demand_inclusion_cutoff_cannot_reset_or_shorten_the_original_response_span() {
        assert_eq!(request_cutoff_v1(100, 20, 79), Some(80));
        assert_eq!(request_cutoff_v1(100, 20, 80), None);
        assert_eq!(request_cutoff_v1(100, 101, 0), None);
        assert_eq!(request_cutoff_v1(u64::MAX, 20, u64::MAX - 20), None);
    }

    fn need() -> PalwTirPublicCaptureNeedV1 {
        PalwTirPublicCaptureNeedV1 {
            resident_ram_bytes: 1_024,
            disk_bytes: 4_096,
            maximum_prepare_io_bytes: 8_192,
            maximum_replay_leaves: 64,
            maximum_replay_positions: 32,
            maximum_replay_id_words: 256,
            maximum_response_io_bytes: 16_384,
            maximum_response_tree_hashes: 128,
        }
    }

    fn limits() -> ResponsePreparationLimitsV1 {
        ResponsePreparationLimitsV1 {
            maximum_resident_ram_bytes: 2_048,
            maximum_prepare_io_bytes: 8_192,
            maximum_replay_leaves: 64,
            maximum_replay_positions: 32,
            maximum_replay_id_words: 256,
            maximum_response_io_bytes: 16_384,
            maximum_response_tree_hashes: 128,
        }
    }

    #[test]
    fn response_work_is_checked_before_a_reader_or_serializer_can_run() {
        let work = response_work_v1(&need(), &limits(), 2_048).unwrap();
        assert_eq!(work.response_io_upper, 16_384);
        assert_eq!(work.response_tree_hashes_upper, 128);
        assert!(response_work_v1(&need(), &limits(), 2_049).is_err());
        let mut larger = need();
        larger.maximum_response_io_bytes += 1;
        assert!(response_work_v1(&larger, &limits(), 1).is_err());
        larger = need();
        larger.maximum_replay_leaves += 1;
        assert!(response_work_v1(&larger, &limits(), 1).is_err());
        larger = need();
        larger.maximum_response_tree_hashes += 1;
        assert!(response_work_v1(&larger, &limits(), 1).is_err());
        larger = need();
        larger.maximum_replay_positions += 1;
        assert!(response_work_v1(&larger, &limits(), 1).is_err());
        larger = need();
        larger.maximum_replay_id_words += 1;
        assert!(response_work_v1(&larger, &limits(), 1).is_err());
    }
}
