//! RFC-0030 private archive verification: native v5 replay of the declared
//! bytes under retained native inputs. It neither changes chain state nor
//! releases money, grants credit, manufactures supply, or authorizes default.
use super::*;
use crate::palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1};
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};

/// G14 r6 D3: at most this many resolved archives are pruned by one block (key order).
pub(super) const PALW_COURT_ARCHIVE_PRUNES_PER_BLOCK_V1: usize = 64;

/// Only this verifier constructs the result. No Borsh/Clone/from-bool path.
/// A later financial transition must also check the bound state root and its
/// own funded contract/once-only effects; this result alone settles nothing.
#[allow(dead_code)]
#[derive(Debug)]
pub(super) struct VerifiedArchivedCloseV1 {
    state_root: Hash64,
    session_id: Hash64,
    side: PalwCourtSideV1,
    verdict: PalwCourtVerdictV2,
    group: Box<PalwCourtCloseGroupV2>,
    close: Box<PalwConsensusObjectV2>,
}
#[allow(dead_code)]
impl VerifiedArchivedCloseV1 {
    pub(super) fn verdict(&self) -> PalwCourtVerdictV2 {
        self.verdict
    }
    pub(super) fn session_id(&self) -> Hash64 {
        self.session_id
    }
    pub(super) fn side(&self) -> PalwCourtSideV1 {
        self.side
    }
    pub(super) fn matches_state(&self, state: &PalwChainStateV2) -> bool {
        self.state_root == state.state_root()
    }
    pub(super) fn group(&self) -> &PalwCourtCloseGroupV2 {
        &self.group
    }
    pub(super) fn close(&self) -> &PalwConsensusObjectV2 {
        &self.close
    }
}
fn same_contract(a: &PalwCourtCloseGroupV2, b: &PalwCourtCloseGroupV2) -> bool {
    a.declarer == b.declarer
        && a.count == b.count
        && a.chunk_digests == b.chunk_digests
        && a.close_digest == b.close_digest
        && a.verdict == b.verdict
        && a.declared_daa == b.declared_daa
        && a.assembly_deadline_daa == b.assembly_deadline_daa
        && a.deposit_outpoint == b.deposit_outpoint
        && a.deposit == b.deposit
}
fn refusal(message: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(message.into())
}

/// Quote the existing message builder's hash-vector clone and Borsh serializer
/// capacity without first constructing either buffer. A borrowed slice has the
/// identical Borsh length prefix and elements as the builder's Vec.
pub(super) fn quote_close_signature_v1(
    q: &mut native_writers::NativeWriteQuoteV1,
    id: &Hash64,
    side: PalwCourtSideV1,
    group: &PalwCourtCloseGroupV2,
) -> Result<(), PalwStateV2Error> {
    q.exact::<Hash64>(group.chunk_digests.len() as u64)?.encoded(&(
        *id,
        side,
        group.count,
        group.chunk_digests.as_slice(),
        group.close_digest,
        group.verdict,
    ))?;
    Ok(())
}

/// Inspection of the current native chunk event, not a peer-supplied point or
/// a financial capability. The native dispatcher has not yet installed public
/// archive intake. Completion, v5 adjudication and funded qualification remain
/// separate requirements. No serialization/Clone/from-bool construction path.
#[allow(dead_code)]
#[derive(Debug)]
pub(super) struct InspectedArchivedNativeChunkV1 {
    state_root: Hash64,
    session_id: Hash64,
    side: PalwCourtSideV1,
    index: u8,
    chunk_digest: Hash64,
    point: PalwBlockContextV2,
    assembly_deadline_daa: u64,
}
#[allow(dead_code)]
impl InspectedArchivedNativeChunkV1 {
    pub(super) fn matches_state(&self, state: &PalwChainStateV2) -> bool {
        self.state_root == state.state_root()
    }
    pub(super) fn point(&self) -> PalwBlockContextV2 {
        self.point
    }
    pub(super) fn assembly_deadline_daa(&self) -> u64 {
        self.assembly_deadline_daa
    }
    pub(super) fn matches_chunk(&self, id: Hash64, side: PalwCourtSideV1, index: u8, bytes: &[u8]) -> bool {
        self.session_id == id
            && self.side == side
            && self.index == index
            && self.chunk_digest == palw_court_close_chunk_digest_v1(bytes)
    }
}
/// Local construction limits must never turn a valid proof into misconduct.
/// Syntax/truncation remains a semantic refusal; budget/OOM is retryable local
/// NativeResource, including the wire reader's actual CPU/arithmetic limits.
pub(super) fn inspection_io_error(stage: &str, error: std::io::Error) -> PalwStateV2Error {
    crate::palw_court_archive_discharge_v1::archive_inspection_io_error_v1(stage, error)
}
impl TransitionBuilder<'_> {
    /// Capture only the active native prefix removed in THIS checkpoint. Old
    /// archives or unclocked private appends cannot acquire retroactive origin.
    pub(super) fn capture_archive_native_origins_v1(&mut self, id: Hash64) -> Result<(), PalwStateV2Error> {
        use crate::palw_court_archive_discharge_v1::ArchiveDeclarationProgressV1 as Progress;
        use crate::palw_court_end_archive_v1::CourtEndEvidenceV1;
        for side in [PalwCourtSideV1::Executor, PalwCourtSideV1::Challenger] {
            let a = self.state.court_end_archive_v1(id).ok_or_else(|| refusal("new archive missing"))?;
            let Some(g) = a.groups.get(&side) else {
                continue;
            };
            if a.fulfilled_groups.contains(&side) || a.applied_deposit_debits.contains_key(&side) {
                continue;
            }
            let captured_at = match a.evidence.as_ref() {
                CourtEndEvidenceV1::Native { point, .. } | CourtEndEvidenceV1::TirConviction { point, .. } => *point,
                CourtEndEvidenceV1::Uncaptured | CourtEndEvidenceV1::DeadlineSweep { .. } => continue,
            };
            let Some(Row::CourtCloseReceipt(r)) = self.state.staged_court_row_v1(&Key::CourtCloseReceipt(id, side)) else {
                continue;
            };
            if r.rules.is_none() {
                continue;
            }
            let row = Progress {
                version: 1,
                session_id: id,
                side,
                claim: a.session.claim,
                service_profile_id: a.service_profile_id,
                allocation_id: a.allocation_id,
                source_id: a.source_id,
                material_until_daa: a.material_until_daa,
                captured_at,
                initial_native_present: g.present,
                native_chunks: BTreeMap::new(),
                completed: None,
            };
            if self.native_writers_tracking_v1() {
                let mut q = native_writers::NativeWriteQuoteV1::new("archive native prefix record before ownership");
                q.exact::<Progress>(2)?.exact::<usize>(2)?;
                self.admit_native_write_v1(q)?;
            }
            self.write_staged_row_v1(Key::CourtArchiveProgress(id, side), Some(Row::CourtArchiveProgress(Box::new(row).into())))?;
        }
        Ok(())
    }

    /// Private S1 transaction. It is deliberately absent from public dispatch
    /// until funded ingress exists. The event point comes only from the native
    /// wrapper. Correct bytes are not a refund or a claim-credit authorization.
    #[allow(dead_code)]
    pub(super) fn apply_native_archived_chunk_v1(&mut self) -> Result<bool, PalwStateV2Error> {
        use crate::palw_court_archive_discharge_v1::ArchivedPositiveFulfillmentV1;
        use crate::palw_court_end_archive_v1::CourtEndEvidenceV1;
        let saved = self.try_checkpoint()?;
        let result = (|| {
            let inspected = self.inspect_archived_native_chunk_v1()?;
            let id = inspected.session_id;
            let side = inspected.side;
            let index = inspected.index;
            let progress =
                self.state.court_archive_progress_v1(id, side).ok_or_else(|| refusal("archive prefix has no native provenance"))?;
            progress.check_context(&self.state).map_err(refusal)?;
            if progress.completed.is_some() {
                return Err(refusal("archived declaration already discharged"));
            }
            let group = &self.state.court_end_archive_v1(id).unwrap().groups[&side];
            let Some(CourtEndEvidenceV1::Native { object, .. }) = self.court_end_evidence.as_deref() else { unreachable!() };
            let PalwConsensusObjectV2::CourtCloseChunk { bytes, .. } = object.as_ref() else { unreachable!() };
            if self.native_writers_tracking_v1() {
                let mut q = native_writers::NativeWriteQuoteV1::new("archive native completion before candidate and progress copies");
                q.copy(progress)?
                    .copy(group)?
                    .exact::<u8>(bytes.len() as u64)?
                    .member::<u8, Vec<u8>>(1)?
                    .member::<u8, PalwBlockContextV2>(1)?;
                self.admit_native_write_v1(q)?;
            }
            let mut next = self.state.court_archive_progress_v1(id, side).unwrap().clone();
            let mut candidate = self.state.court_end_archive_v1(id).unwrap().groups[&side].clone();
            let Some(CourtEndEvidenceV1::Native { object, .. }) = self.court_end_evidence.as_deref() else { unreachable!() };
            let PalwConsensusObjectV2::CourtCloseChunk { bytes, .. } = object.as_ref() else { unreachable!() };
            candidate.chunks.insert(index, bytes.clone());
            candidate.present |= 1u64 << index;
            next.native_chunks.insert(index, inspected.point);
            let qualifies = candidate.is_complete() && next.qualified_mask() == PalwCourtCloseGroupV2::full_mask(candidate.count);
            if qualifies {
                let verified = self.verify_archived_close_v1(id, side, &candidate)?;
                if !verified.matches_state(&self.state)
                    || !inspected.matches_state(&self.state)
                    || verified.session_id() != id
                    || verified.side() != side
                    || verified.group() != &candidate
                {
                    return Err(refusal("archive completion changed its verified native root or contract"));
                }
                if self.native_writers_tracking_v1() {
                    let mut q = native_writers::NativeWriteQuoteV1::new("retained qualified native close before copy");
                    q.copy(verified.close())?.exact::<PalwConsensusObjectV2>(1)?;
                    self.admit_native_write_v1(q)?;
                }
                next.completed =
                    Some(ArchivedPositiveFulfillmentV1 { point: inspected.point, close: Box::new(verified.close().clone()) });
            }
            let bytes = &candidate.chunks[&index];
            if !inspected.matches_state(&self.state) || !inspected.matches_chunk(id, side, index, bytes) {
                return Err(refusal("native chunk inspection became stale"));
            }
            if !self.state.court_end_archive_v1(id).unwrap().groups[&side].has(index) {
                self.append_archived_close_chunk_v1(id, side, index, bytes)?;
            }
            self.write_staged_row_v1(Key::CourtArchiveProgress(id, side), Some(Row::CourtArchiveProgress(Box::new(next).into())))?;
            Ok(qualifies)
        })();
        if result.is_err() {
            self.restore(saved);
        }
        result
    }

    /// True only for a removal of one of the pruned session's own archive
    /// rows (archive, its two signed receipts, its two provenance records)
    /// while [`Self::prune_resolved_court_archive_v1`] is running.
    pub(super) fn archive_prune_removes_v1(&self, key: &Key, new: Option<&Row>) -> bool {
        let Some(id) = self.court_archive_prune else { return false };
        new.is_none()
            && match key {
                Key::CourtEndArchive(k) | Key::CourtCloseReceipt(k, _) | Key::CourtArchiveProgress(k, _) => *k == id,
                _ => false,
            }
    }

    /// RFC-0030 / RFC-0034 S1 / ADR-0201: once every pending liability of an
    /// archived court is resolved (applied native debit, fulfilled native
    /// close, or rooted on-time positive fulfillment) and no nominal hold
    /// remains for either actor, AND (user decision D3) the archived session's
    /// `material_until_daa` has passed at `now`, the archive and its per-side
    /// receipt and provenance rows may be removed. Nothing here moves money: deposits are
    /// nominal and were never escrow. Gated by the archive's own existence
    /// condition (the court service book that created it). The removal is
    /// journaled, so undoing the block restores every row exactly.
    pub(super) fn prune_resolved_court_archive_v1(&mut self, id: Hash64, now: u64) -> Result<(), PalwStateV2Error> {
        if self.state.court_service_state_v1().is_none() {
            return Err(refusal("archive pruning requires the court service book"));
        }
        let archive = self.state.court_end_archive_v1(id).ok_or_else(|| refusal("court archive missing"))?;
        archive.check_context(&self.state).map_err(refusal)?;
        for side in [PalwCourtSideV1::Executor, PalwCourtSideV1::Challenger] {
            if let Some(p) = self.state.court_archive_progress_v1(id, side) {
                p.check(&Key::CourtArchiveProgress(id, side)).map_err(refusal)?;
            }
        }
        if !self.state.court_archive_fully_resolved_v1(id, now) {
            return Err(refusal("archived court still carries an unresolved liability, a nominal hold or retained material"));
        }
        if self.court_archive_prune.is_some() {
            return Err(refusal("archive pruning is not re-entrant"));
        }
        let saved = self.try_checkpoint()?;
        self.court_archive_prune = Some(id);
        let result = (|| {
            for side in [PalwCourtSideV1::Executor, PalwCourtSideV1::Challenger] {
                if self.state.staged_court_row_v1(&Key::CourtArchiveProgress(id, side)).is_some() {
                    self.write_staged_row_v1(Key::CourtArchiveProgress(id, side), None)?;
                }
            }
            for side in [PalwCourtSideV1::Executor, PalwCourtSideV1::Challenger] {
                if self.state.staged_court_row_v1(&Key::CourtCloseReceipt(id, side)).is_some() {
                    self.write_staged_row_v1(Key::CourtCloseReceipt(id, side), None)?;
                }
            }
            self.write_staged_row_v1(Key::CourtEndArchive(id), None)
        })();
        self.court_archive_prune = None;
        if result.is_err() {
            self.restore(saved);
        }
        result
    }

    /// **G14 r6 D3: the per-block archive pruner.** Called once per block from
    /// the PALW fold after the court service observation. Gated by the archive's
    /// own existence condition (the court service book; absent on every network
    /// today, so a no-op). Removes, in key order, at most
    /// [`PALW_COURT_ARCHIVE_PRUNES_PER_BLOCK_V1`] archives that are fully
    /// resolved and past their material horizon. An archive the pruner refuses
    /// (context lost, re-entrancy) is left in place: the pruner restores its own
    /// checkpoint, so a refusal never invalidates the block. Journaled through
    /// the staged-court writer; reorg restores every row.
    pub(super) fn sweep_resolved_court_archives_v1(&mut self, now: u64) -> Result<(), PalwStateV2Error> {
        if self.state.court_service_state_v1().is_none() {
            return Ok(());
        }
        let due: Vec<Hash64> = self
            .state
            .staged_cache_lease_rows_v1()
            .filter_map(|(k, _)| match k {
                Key::CourtEndArchive(id) => Some(id),
                _ => None,
            })
            .filter(|id| self.state.court_archive_fully_resolved_v1(*id, now))
            .take(PALW_COURT_ARCHIVE_PRUNES_PER_BLOCK_V1)
            .collect();
        for id in due {
            let _ = self.prune_resolved_court_archive_v1(id, now);
        }
        Ok(())
    }

    /// All forward writes require the actual removal/native event and retained
    /// v5 inputs; a deserialized progress record is not a mutation capability.
    pub(super) fn check_archive_progress_write_v1(&mut self, key: &Key, new: Option<&Row>) -> Result<(), PalwStateV2Error> {
        use crate::palw_court_end_archive_v1::CourtEndEvidenceV1;
        if self.archive_prune_removes_v1(key, new) {
            return Ok(());
        }
        let is_key = matches!(key, Key::CourtArchiveProgress(..));
        if !is_key && !matches!(new, Some(Row::CourtArchiveProgress(_))) {
            return Ok(());
        }
        let Some(Row::CourtArchiveProgress(next)) = new else {
            return Err(refusal("archive provenance cannot be removed or retyped"));
        };
        next.check(key).and_then(|()| next.check_context(&self.state)).map_err(refusal)?;
        let a = self.state.court_end_archive_v1(next.session_id).ok_or_else(|| refusal("archive missing"))?;
        let group = &a.groups[&next.side];
        let old = match self.state.staged_court_row_v1(key) {
            Some(Row::CourtArchiveProgress(r)) => Some(r.as_ref()),
            None => None,
            _ => return Err(refusal("archive provenance foreign row")),
        };
        if let Some(old) = old {
            if next.as_ref() == old {
                return Ok(());
            }
            if old.completed.is_some()
                || (
                    next.version,
                    next.session_id,
                    next.side,
                    next.claim,
                    next.service_profile_id,
                    next.allocation_id,
                    next.source_id,
                    next.material_until_daa,
                    next.captured_at,
                    next.initial_native_present,
                ) != (
                    old.version,
                    old.session_id,
                    old.side,
                    old.claim,
                    old.service_profile_id,
                    old.allocation_id,
                    old.source_id,
                    old.material_until_daa,
                    old.captured_at,
                    old.initial_native_present,
                )
                || next.native_chunks.len() != old.native_chunks.len() + 1
                || old.native_chunks.iter().any(|(i, p)| next.native_chunks.get(i) != Some(p))
            {
                return Err(refusal("archive provenance changes accepted history or discharged responsibility"));
            }
            let Some(CourtEndEvidenceV1::Native { point, object }) = self.court_end_evidence.as_deref() else {
                return Err(refusal("archive progress lacks native event"));
            };
            let PalwConsensusObjectV2::CourtCloseChunk { session_id, side, index, bytes } = object.as_ref() else {
                return Err(refusal("archive progress event is not a chunk"));
            };
            // A precommitted chunk retained earlier without a clock can be
            // observed again in this actual native event. Its new point is a
            // current positive delivery, never a retroactive timestamp. Already
            // qualified identities cannot be charged/reused a second time.
            let newly_appended=self.entries.iter().rev().any(|e|matches!(e,PalwDeltaEntryV2::StagedCourtSeed {key:Key::CourtEndArchive(id),old:Some(Row::CourtEndArchive(before)),new:Some(Row::CourtEndArchive(after))}
                if *id==*session_id && before.groups.get(side).is_some_and(|g|*index<g.count&&!g.has(*index))
                && after.groups.get(side).is_some_and(|g|g.chunks.get(index)==Some(bytes))));
            let retained_reobservation =
                *index < group.count && old.qualified_mask() & (1u64 << index) == 0 && group.chunks.get(index) == Some(bytes);
            if *session_id != next.session_id
                || *side != next.side
                || old.native_chunks.contains_key(index)
                || next.native_chunks.get(index) != Some(point)
                || group.chunks.get(index) != Some(bytes)
                || old.native_chunks.values().any(|prior| prior.daa_score > point.daa_score)
                || (!newly_appended && !retained_reobservation)
            {
                return Err(refusal("archive progress has no matching actual chunk append"));
            }
        } else {
            if !next.native_chunks.is_empty()||next.completed.is_some()||next.initial_native_present!=group.present
                || !self.entries.iter().any(|e|matches!(e,PalwDeltaEntryV2::Court {key,old:Some(s),new:None} if *key==next.session_id && s==&a.session))
                || !self.entries.iter().any(|e|matches!(e,PalwDeltaEntryV2::CourtCloseGroup {key,old:Some(g),new:None}
                    if *key==(next.session_id,next.side) && g==group))
                || !self.entries.iter().any(|e|matches!(e,PalwDeltaEntryV2::StagedCourtSeed {key:Key::CourtEndArchive(id),old:None,new:Some(Row::CourtEndArchive(saved))}
                    if *id==next.session_id && saved.as_ref()==a))
                || self.court_end_evidence.as_deref()!=Some(a.evidence.as_ref())
            {return Err(refusal("archive provenance was not captured with its active native prefix"));}
        }
        if self.native_writers_tracking_v1() {
            let mut q = native_writers::NativeWriteQuoteV1::new("archive progress receipt and v5 inspection before buffers");
            quote_close_signature_v1(&mut q, &next.session_id, next.side, group)?;
            if next.completed.is_some() {
                q.copy(group)?;
            }
            self.admit_native_write_v1(q)?;
        }
        let r = match self.state.staged_court_row_v1(&Key::CourtCloseReceipt(next.session_id, next.side)) {
            Some(Row::CourtCloseReceipt(r)) => r.as_ref(),
            _ => unreachable!(),
        };
        r.check_context(&self.state).map_err(refusal)?;
        if let Some(done) = &next.completed {
            // Reuse the native verifier's actual-wire shape admission and typed
            // resource errors. A retained clone's capacities do not bound a new
            // Borsh decoder's allocations.
            let candidate = self.state.court_end_archive_v1(next.session_id).unwrap().groups[&next.side].clone();
            let verified = self.verify_archived_close_v1(next.session_id, next.side, &candidate)?;
            if !verified.matches_state(&self.state) || verified.close() != done.close.as_ref() {
                return Err(refusal("archive qualified close changed its native replay root or bytes"));
            }
        }
        Ok(())
    }

    /// Inspect the point/object pair captured by the actual native wrapper.
    /// Never infer an accepting point from last_point (still the parent while
    /// a block is being built), a supplied timestamp or an old material append.
    /// This neither appends bytes nor marks an obligation fulfilled.
    #[allow(dead_code)]
    pub(super) fn inspect_archived_native_chunk_v1(&mut self) -> Result<InspectedArchivedNativeChunkV1, PalwStateV2Error> {
        use crate::palw_court_end_archive_v1::CourtEndEvidenceV1;
        let Some(CourtEndEvidenceV1::Native { point, object }) = self.court_end_evidence.as_deref() else {
            return Err(refusal("archived chunk lacks the current native event"));
        };
        let PalwConsensusObjectV2::CourtCloseChunk { session_id, side, index, bytes } = object.as_ref() else {
            return Err(refusal("current native event is not an archived close chunk"));
        };
        let archive = self.state.court_end_archive_v1(*session_id).ok_or_else(|| refusal("court archive missing"))?;
        archive.check_context(&self.state).map_err(refusal)?;
        let group = archive.groups.get(side).ok_or_else(|| refusal("archived declaration missing"))?;
        let retained_reobservation = group.chunks.get(index) == Some(bytes)
            && self.state.court_archive_progress_v1(*session_id, *side).is_some_and(|p| {
                p.completed.is_none()
                    && *index < group.count
                    && p.check_context(&self.state).is_ok()
                    && p.qualified_mask() & (1u64 << index) == 0
            });
        if archive.fulfilled_groups.contains(side)
            || archive.applied_deposit_debits.contains_key(side)
            || *index >= group.count
            || (group.has(*index) && !retained_reobservation)
            || bytes.is_empty()
            || bytes.len() > PALW_COURT_CLOSE_CHUNK_MAX_BYTES
            || palw_court_close_chunk_digest_v1(bytes) != group.chunk_digests[usize::from(*index)]
        {
            return Err(refusal("native archived chunk is not missing declared material"));
        }
        let receipt = match self.state.staged_court_row_v1(&Key::CourtCloseReceipt(*session_id, *side)) {
            Some(Row::CourtCloseReceipt(r)) => r.as_ref(),
            _ => return Err(refusal("original signed close receipt missing")),
        };
        let deadline = receipt
            .point
            .daa_score
            .checked_add(palw_close_assembly_daa_v1(group.count))
            .ok_or_else(|| refusal("original signed assembly deadline overflow"))?;
        let binding = self
            .state
            .court_service_state_v1()
            .and_then(|s| s.claims.get(&archive.session.claim))
            .ok_or_else(|| refusal("archive original binding missing"))?;
        let ended_at = match archive.evidence.as_ref() {
            CourtEndEvidenceV1::Native { point, .. } | CourtEndEvidenceV1::TirConviction { point, .. } => Some(point.daa_score),
            CourtEndEvidenceV1::Uncaptured | CourtEndEvidenceV1::DeadlineSweep { .. } => None,
        };
        if deadline != group.assembly_deadline_daa
            || point.daa_score < receipt.point.daa_score
            || point.daa_score > deadline
            || point.daa_score > receipt.session.deadline_daa
            || point.daa_score > binding.hard_deadline_daa
            || ended_at.is_some_and(|ended| point.daa_score < ended)
            || self.state.last_point().is_some_and(|parent| point.daa_score < parent.daa_score)
        {
            return Err(refusal("native archived chunk is outside its original signed window"));
        }
        let inspected = InspectedArchivedNativeChunkV1 {
            state_root: self.state.state_root(),
            session_id: *session_id,
            side: *side,
            index: *index,
            chunk_digest: group.chunk_digests[usize::from(*index)],
            point: *point,
            assembly_deadline_daa: deadline,
        };
        if self.native_writers_tracking_v1() {
            let mut q = native_writers::NativeWriteQuoteV1::new("archived native chunk original signature before buffers");
            quote_close_signature_v1(&mut q, &inspected.session_id, inspected.side, group)?;
            self.admit_native_write_v1(q)?;
        }
        let receipt = match self.state.staged_court_row_v1(&Key::CourtCloseReceipt(inspected.session_id, inspected.side)) {
            Some(Row::CourtCloseReceipt(r)) => r.as_ref(),
            _ => unreachable!(),
        };
        receipt.check_context(&self.state).map_err(refusal)?;
        Ok(inspected)
    }

    /// Retain only a missing precommitted chunk after active court termination.
    /// This private material path admits no new declaration, clock, fulfilment,
    /// penalty or financial release. A completed malformed object is evidence to
    /// retain, not permission to punish its signer or to discard its last bytes.
    #[allow(dead_code)]
    pub(super) fn append_archived_close_chunk_v1(
        &mut self,
        id: Hash64,
        side: PalwCourtSideV1,
        index: u8,
        bytes: &[u8],
    ) -> Result<(), PalwStateV2Error> {
        let archive = self.state.court_end_archive_v1(id).ok_or_else(|| refusal("court archive missing"))?;
        archive.check_context(&self.state).map_err(refusal)?;
        let group = archive.groups.get(&side).ok_or_else(|| refusal("archived declaration missing"))?;
        if archive.fulfilled_groups.contains(&side)
            || archive.applied_deposit_debits.contains_key(&side)
            || index >= group.count
            || group.has(index)
            || bytes.is_empty()
            || bytes.len() > PALW_COURT_CLOSE_CHUNK_MAX_BYTES
            || palw_court_close_chunk_digest_v1(bytes) != group.chunk_digests[usize::from(index)]
        {
            return Err(refusal("archive chunk is not missing declared material"));
        }
        if self.native_writers_tracking_v1() {
            let mut q = native_writers::NativeWriteQuoteV1::new("archive material append before copy and signature message");
            q.copy(archive)?
                .exact::<crate::palw_court_end_archive_v1::PendingCourtArchiveV1>(2)?
                .exact::<u8>(
                    (2 * std::mem::size_of::<usize>()
                        + std::mem::align_of::<crate::palw_court_end_archive_v1::PendingCourtArchiveV1>()) as u64,
                )?
                .exact::<u8>(bytes.len() as u64)?
                .member::<u8, Vec<u8>>(1)?;
            quote_close_signature_v1(&mut q, &id, side, group)?;
            self.admit_native_write_v1(q)?;
        }
        let receipt = match self.state.staged_court_row_v1(&Key::CourtCloseReceipt(id, side)) {
            Some(Row::CourtCloseReceipt(r)) => r.as_ref(),
            _ => return Err(refusal("original signed close receipt missing")),
        };
        receipt.check_context(&self.state).map_err(refusal)?;
        let mut next = self.state.court_end_archive_v1(id).expect("validated archive").clone();
        let group = next.groups.get_mut(&side).expect("validated side");
        group.chunks.insert(index, bytes.to_vec());
        group.present |= 1u64 << index;
        self.write_staged_row_v1(Key::CourtEndArchive(id), Some(Row::CourtEndArchive(Box::new(next).into())))
    }

    /// A private verifier, not a public ingestion or paid-admission endpoint.
    /// The candidate can add only the precommitted missing bytes. No clock,
    /// policy, signature, verdict, contract or previously accepted byte changes.
    #[allow(dead_code)]
    pub(super) fn verify_archived_close_v1(
        &mut self,
        id: Hash64,
        side: PalwCourtSideV1,
        candidate: &PalwCourtCloseGroupV2,
    ) -> Result<VerifiedArchivedCloseV1, PalwStateV2Error> {
        let archive = self.state.court_end_archive_v1(id).ok_or_else(|| refusal("court archive missing"))?;
        archive.check_context(&self.state).map_err(refusal)?;
        let original = archive.groups.get(&side).ok_or_else(|| refusal("archived declaration missing"))?;
        if archive.fulfilled_groups.contains(&side) || archive.applied_deposit_debits.contains_key(&side) {
            return Err(refusal("archived declaration already discharged"));
        }
        if !same_contract(original, candidate)
            || !candidate.is_complete()
            || candidate.chunks.len() != usize::from(candidate.count)
            || candidate.present != PalwCourtCloseGroupV2::full_mask(candidate.count)
            || original.chunks.iter().any(|(i, bytes)| candidate.chunks.get(i) != Some(bytes))
        {
            return Err(refusal("completed archive group changes its accepted contract/prefix"));
        }
        let mut total = 0u64;
        for i in 0..candidate.count {
            let bytes = candidate.chunks.get(&i).ok_or_else(|| refusal("archive completion has a chunk hole"))?;
            if bytes.is_empty()
                || bytes.len() > PALW_COURT_CLOSE_CHUNK_MAX_BYTES
                || palw_court_close_chunk_digest_v1(bytes) != candidate.chunk_digests[usize::from(i)]
            {
                return Err(refusal("archive completion chunk digest/size mismatch"));
            }
            total = total.checked_add(bytes.len() as u64).ok_or_else(|| refusal("archive completion length overflow"))?;
        }
        if !matches!(self.state.staged_court_row_v1(&Key::CourtCloseReceipt(id, side)), Some(Row::CourtCloseReceipt(_))) {
            return Err(refusal("original signed native close receipt missing"));
        }
        // Admit retained output and scratch before signature-message
        // construction, assembly, and decoding. Session/phase are borrowed.
        if self.native_writers_tracking_v1() {
            let mut q = native_writers::NativeWriteQuoteV1::new("archive complete signed v5 replay before buffers");
            q.copy(candidate)?.exact::<PalwCourtCloseGroupV2>(1)?.exact::<u8>(total)?;
            quote_close_signature_v1(&mut q, &id, side, candidate)?;
            self.admit_native_write_v1(q)?;
        }
        let receipt = match self.state.staged_court_row_v1(&Key::CourtCloseReceipt(id, side)) {
            Some(Row::CourtCloseReceipt(r)) => r.as_ref(),
            _ => unreachable!(),
        };
        receipt.check_context(&self.state).map_err(refusal)?;
        let rules = receipt.rules.ok_or_else(|| refusal("original native v5 inputs missing; no default fallback"))?;
        crate::palw_court_v2::check_close_declared_chunk_count_v2(&id, candidate.count, &rules.court)
            .map_err(|e| refusal(e.to_string()))?;
        let total = usize::try_from(total).map_err(|_| refusal("archive completion cannot fit native address space"))?;
        let mut assembled = Vec::new();
        assembled.try_reserve_exact(total).map_err(|e| {
            PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<PalwConsensusObjectV2>(
                "archive v5 assembly allocation",
                &e.to_string(),
            ))
        })?;
        for i in 0..candidate.count {
            assembled.extend_from_slice(&candidate.chunks[&i]);
        }
        if palw_court_close_chunk_digest_v1(&assembled) != candidate.close_digest {
            return Err(refusal("archive completion whole digest mismatch"));
        }
        let mut reader = NativeWireReaderV1::new(assembled.as_slice(), assembled.len() as u64);
        let shape =
            PalwConsensusObjectV2::scan_native_wire_v1(&mut reader).map_err(|e| inspection_io_error("archive close wire shape", e))?;
        if reader.remaining() != 0 {
            return Err(refusal("archive close has trailing material"));
        }
        if self.native_writers_tracking_v1() {
            let mut q = native_writers::NativeWriteQuoteV1::new("archive v5 proof decode before allocation");
            q.exact::<u8>(shape.heap)?.exact::<PalwConsensusObjectV2>(1)?;
            self.admit_native_write_v1(q)?;
        }
        let close: PalwConsensusObjectV2 =
            borsh::from_slice(&assembled).map_err(|e| inspection_io_error("archive close decode", e))?;
        let PalwConsensusObjectV2::CourtClosed { session_id, verdict, proof } = &close else {
            return Err(refusal("archive bytes are not a court close"));
        };
        if *session_id != id || *verdict != candidate.verdict {
            return Err(refusal("archive close changes declared session/verdict"));
        }
        if (proof.is_tir_v1() && !rules.tir_active)
            || (proof.is_gen_v1() && !rules.gen_active)
            || (proof.is_eval_v1() && !rules.improvement_active)
            || (proof.is_constrained_decode_v1() && !rules.constraint_active)
            || (crate::palw_task_heads_v1::palw_object_needs_task_heads_v1(&close) && !rules.task_heads_active)
        {
            return Err(refusal("archive proof family was dormant in its retained native context"));
        }
        // Borrow authenticated retained records directly. No global state copy,
        // temporary active row, derived-index changes, or inspection-state writes.
        let receipt = match self.state.staged_court_row_v1(&Key::CourtCloseReceipt(id, side)) {
            Some(Row::CourtCloseReceipt(r)) => r.as_ref(),
            _ => unreachable!(),
        };
        let prompt = if rules.prompt_ids_merkle {
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1
        } else {
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
        };
        let derived = crate::palw_court_v2::adjudicate_court_close_borrowed_v5(
            &self.state,
            &id,
            &receipt.session,
            receipt.tir_phase.as_deref(),
            proof,
            &rules.court,
            rules.step_ladder,
            prompt,
            rules.held_regime,
            rules.decode_close_convicts_only,
            rules.fp_decode_from_daa,
            rules.cell_closure_from_daa,
            rules.memory_routing_from_daa,
        )
        .map_err(|e| refusal(format!("archived v5 close does not adjudicate: {e}")))?;
        if derived != *verdict {
            return Err(refusal("archived v5 result differs from the signed declaration"));
        }
        Ok(VerifiedArchivedCloseV1 {
            state_root: self.state.state_root(),
            session_id: id,
            side,
            verdict: derived,
            group: Box::new(candidate.clone()),
            close: Box::new(close),
        })
    }
}
