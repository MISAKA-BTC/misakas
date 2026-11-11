//! Inverse of a standalone service-bound archive. An enclosing BCT2
//! settlement owns its wider claim and bond cascade separately.
use super::*;
use crate::{
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1},
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row},
};

pub(super) struct CourtArchiveInverseV1 {
    first: usize,
    journal_capacity: usize,
    requests: u64,
    session: Hash64,
    claim: Hash64,
    challenger: PalwBondKeyV2,
    session_due: Option<u64>,
    claim_due: Vec<u64>,
    observations: crate::palw_court_observation_v1::CourtObservationCheckpointV1,
    execution: palw_claim_local_object_spans_v2::ScopedExecutionCheckpointV2,
    gc: (u64, bool),
    _memory: Option<NativeDemandLeaseV1>,
}

fn mismatch(why: &str) -> PalwStateV2Error {
    PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<CourtArchiveInverseV1>("court archive inverse", why))
}

fn restore_row<K: Ord, V: PartialEq>(
    rows: &mut BTreeMap<K, V>,
    key: K,
    old: Option<V>,
    expected: Option<V>,
) -> Result<(), PalwStateV2Error> {
    if rows.get(&key) != expected.as_ref() {
        return Err(mismatch("native writer row differs from its journal"));
    }
    match old {
        Some(row) => {
            rows.insert(key, row);
        }
        None => {
            rows.remove(&key);
        }
    }
    Ok(())
}

impl CourtArchiveInverseV1 {
    pub(super) fn prepare_v1(b: &TransitionBuilder<'_>, session: Hash64) -> Result<Self, PalwStateV2Error> {
        let row = b.state.court_sessions.get(&session).ok_or(PalwStateV2Error::MissingSession(session))?;
        let claim = row.claim;
        if b.state.staged_court_row_v1(&Key::CourtEndArchive(session)).is_some() {
            return Err(PalwStateV2Error::DuplicateSession(session));
        }
        if [PalwCourtSideV1::Challenger, PalwCourtSideV1::Executor]
            .into_iter()
            .any(|side| b.state.staged_court_row_v1(&Key::CourtArchiveProgress(session, side)).is_some())
        {
            return Err(mismatch("archive progress exists without its archive"));
        }
        let count = b.state.claim_deadlines_v2(claim).count();
        let count_and_court = (count as u64).checked_add(1).ok_or_else(|| mismatch("archive deadline count overflow"))?;
        let mut q = super::native_writers::NativeWriteQuoteV1::new("native court archive inverse");
        // Old payloads move back from their forward journal entries. Reserve
        // the touched tree memberships and old deadline list before removal.
        q.exact::<u64>(count as u64)?
            .member::<(u64, Hash64), ()>(count_and_court)?
            .member::<(Hash64, u64), ()>(count as u64)?
            .member::<PalwBondKeyV2, u128>(1)?
            .member::<Hash64, PalwCourtSessionStateV2>(1)?
            .member::<Hash64, u32>(1)?
            .member::<Hash64, u64>(1)?
            .member::<(Hash64, Hash64), ()>(1)?
            .member::<(PalwBondKeyV2, Hash64), ()>(1)?
            .member::<Hash64, Box<crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1>>(1)?
            .member::<(Hash64, PalwCourtSideV1), PalwCourtCloseGroupV2>(2)?
            .member::<(u64, (Hash64, PalwCourtSideV1)), ()>(2)?;
        let memory = q.reserve_scratch_v1()?;
        let mut claim_due = Vec::new();
        claim_due.try_reserve_exact(count).map_err(|_| mismatch("old deadline allocation refused"))?;
        if claim_due.capacity() != count {
            return Err(mismatch("old deadline allocation exceeds its quote"));
        }
        claim_due.extend(b.state.claim_deadlines_v2(claim));
        // Some inverse nodes materialize only on refusal. Keep the scratch
        // owner pending for the whole transaction.
        Ok(Self {
            first: b.entries.len(),
            journal_capacity: b.entries.capacity(),
            requests: b.native_reservation.requests_v1(),
            session,
            claim,
            challenger: row.challenger_bond,
            session_due: b.state.court_deadline_by_session.get(&session).copied(),
            claim_due,
            observations: crate::palw_court_observation_v1::checkpoint_v1(),
            execution: palw_claim_local_object_spans_v2::checkpoint(b),
            gc: palw_claim_local_context_gc_v3::checkpoint_v3(b),
            _memory: memory,
        })
    }

    pub(super) fn restore_v1(self, b: &mut TransitionBuilder<'_>) -> Result<(), PalwStateV2Error> {
        if palw_claim_local_context_gc_v3::checkpoint_v3(b) != self.gc {
            // Archive/progress keys do not write the claim-local GC tables.
            // A changed writer must extend the inverse before using it.
            return Err(mismatch("archive unexpectedly changed claim-local GC"));
        }
        while b.entries.len() > self.first {
            let entry = b.entries.pop().expect("checked archive journal length");
            match entry {
                PalwDeltaEntryV2::Exposure { key, old, new } if key == self.challenger => {
                    restore_row(&mut b.state.reserved_exposure, key, old, new)?;
                }
                PalwDeltaEntryV2::Court { key, old: Some(old), new: None } if key == self.session && old.claim == self.claim => {
                    let challenger = old.challenger_bond;
                    restore_row(&mut b.state.court_sessions, key, Some(old), None)?;
                    let count = b.state.open_courts_by_claim.entry(self.claim).or_default();
                    *count = count.checked_add(1).ok_or_else(|| mismatch("court count overflow"))?;
                    b.state.court_sessions_by_claim.insert((self.claim, key));
                    b.state.courts_by_challenger.insert((challenger, key));
                }
                PalwDeltaEntryV2::TirDissection { key, old, new } if key == self.session => {
                    restore_row(&mut b.state.tir_dissections, key, old, new)?;
                }
                PalwDeltaEntryV2::CourtCloseGroup { key, old: Some(old), new: None } if key.0 == self.session => {
                    let due = old.assembly_deadline_daa;
                    restore_row(&mut b.state.court_close_groups, key, Some(old), None)?;
                    b.state.court_close_deadlines.insert((due, key));
                }
                PalwDeltaEntryV2::StagedCourtSeed { key, old: None, new: Some(new) } if matches!(key, Key::CourtEndArchive(id) | Key::CourtArchiveProgress(id, _) if id == self.session) =>
                {
                    if b.state.staged_court_row_v1(&key) != Some(&new)
                        || !matches!(new, Row::CourtEndArchive(_) | Row::CourtArchiveProgress(_))
                    {
                        return Err(mismatch("archive row differs from its journal"));
                    }
                    b.state.staged_courts.remove(&key);
                    super::palw_staged_court_fold_v1::update_indices(&mut b.state, key, Some(&new), None);
                }
                _ => return Err(mismatch("archive inverse encountered an unrelated write")),
            }
        }
        b.state.replace_court_deadline_v2(self.session, self.session_due);
        loop {
            let first = b.state.claim_deadlines_v2(self.claim).next();
            let Some(daa) = first else { break };
            b.state.remove_claim_deadline_v2(daa, self.claim);
        }
        for daa in self.claim_due {
            b.state.insert_claim_deadline_v2(daa, self.claim);
        }
        palw_claim_local_object_spans_v2::restore(b, self.execution);
        crate::palw_court_observation_v1::restore_v1(self.observations);
        *b.inflight_index.get_mut() = None;
        b.registration_index = None;
        b.panel_horizon.set(None);
        if b.native_writers_tracking_v1() {
            // The rollback moved every old payload back and discarded the
            // new rows. The journal Vec may still own its enlarged buffer.
            let journal = if b.entries.capacity() > self.journal_capacity {
                (b.entries.capacity() as u64)
                    .checked_mul(std::mem::size_of::<PalwDeltaEntryV2>() as u64)
                    .ok_or_else(|| mismatch("retained archive journal capacity overflow"))?
            } else {
                0
            };
            let retained = self.requests.checked_add(journal).ok_or_else(|| mismatch("archive reservation overflow"))?;
            b.native_reservation.restore_requests_v1(retained).map_err(PalwStateV2Error::NativeResource)?;
        }
        Ok(())
    }
}
