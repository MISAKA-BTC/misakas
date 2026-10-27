//! RFC-0034 S1 private provenance and positive financial discharge. This is
//! neither public paid ingress nor an absence/default or pruning permission.
use crate::{
    Hash64,
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row},
    palw_state_v2::*,
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeMap;

fn semantic(message: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(message.into())
}
/// Local construction refusal is retryable and never an adjudicated offence.
pub(crate) fn archive_inspection_io_error_v1(stage: &str, error: std::io::Error) -> PalwStateV2Error {
    if matches!(error.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::OutOfMemory) {
        PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<PalwConsensusObjectV2>(
            stage,
            &error.to_string(),
        ))
    } else {
        semantic(format!("{stage}: {error}"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ArchivedPositiveFulfillmentV1 {
    pub point: PalwBlockContextV2,
    pub close: Box<PalwConsensusObjectV2>,
}
/// `initial_native_present` is created only with removal of the actual active
/// group. Later unclocked private appends never enlarge this mask. Every other
/// qualified chunk has its actual native point retained separately.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ArchiveDeclarationProgressV1 {
    pub version: u16,
    pub session_id: Hash64,
    pub side: PalwCourtSideV1,
    pub claim: Hash64,
    pub service_profile_id: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub material_until_daa: u64,
    pub captured_at: PalwBlockContextV2,
    pub initial_native_present: u64,
    pub native_chunks: BTreeMap<u8, PalwBlockContextV2>,
    pub completed: Option<ArchivedPositiveFulfillmentV1>,
}
impl ArchiveDeclarationProgressV1 {
    pub fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1
            || *key != Key::CourtArchiveProgress(self.session_id, self.side)
            || self.native_chunks.len() > usize::from(PALW_COURT_CLOSE_MAX_CHUNKS)
            || self.native_chunks.keys().any(|i| *i >= PALW_COURT_CLOSE_MAX_CHUNKS || self.initial_native_present & (1u64 << i) != 0)
        {
            return Err("archive progress key/version/chunk provenance mismatch".into());
        }
        Ok(())
    }
    pub fn qualified_mask(&self) -> u64 {
        self.native_chunks.keys().fold(self.initial_native_present, |mask, i| mask | (1u64 << i))
    }
    /// Light rooted scope validation used by financial readers. Native v5 is
    /// checked by the forward writer and loaded-context verifier, not by every
    /// headroom read. The record contains the actual close and all provenance.
    pub fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        use crate::palw_court_end_archive_v1::CourtEndEvidenceV1;
        self.check(&Key::CourtArchiveProgress(self.session_id, self.side))?;
        let a = state.court_end_archive_v1(self.session_id).ok_or("archive progress has no archive")?;
        let g = a.groups.get(&self.side).ok_or("archive progress has no declared group")?;
        if g.count == 0 || g.count > PALW_COURT_CLOSE_MAX_CHUNKS || g.chunk_digests.len() != usize::from(g.count) {
            return Err("archive progress has invalid declaration chunk bounds".into());
        }
        let r = match state.staged_court_row_v1(&Key::CourtCloseReceipt(self.session_id, self.side)) {
            Some(Row::CourtCloseReceipt(r)) => r.as_ref(),
            _ => return Err("archive progress lost original signature receipt".into()),
        };
        let binding = state
            .court_service_state_v1()
            .and_then(|s| s.claims.get(&self.claim))
            .ok_or("archive progress lost original service binding")?;
        let ended = match a.evidence.as_ref() {
            CourtEndEvidenceV1::Native { point, .. } | CourtEndEvidenceV1::TirConviction { point, .. } => point,
            CourtEndEvidenceV1::Uncaptured | CourtEndEvidenceV1::DeadlineSweep { .. } => {
                return Err("archive progress has no native origin point".into());
            }
        };
        if (self.claim, self.service_profile_id, self.allocation_id, self.source_id, self.material_until_daa)
            != (a.session.claim, a.service_profile_id, a.allocation_id, a.source_id, a.material_until_daa)
            || self.captured_at != *ended
            || r.point.daa_score != g.declared_daa
            || (r.service_profile_id, r.allocation_id, r.source_id, r.session.claim, r.session.deadline_daa, r.declarer)
                != (self.service_profile_id, self.allocation_id, self.source_id, self.claim, a.session.deadline_daa, g.declarer)
            || g.declared_daa.checked_add(palw_close_assembly_daa_v1(g.count)) != Some(g.assembly_deadline_daa)
            || g.assembly_deadline_daa > a.session.deadline_daa
            || g.assembly_deadline_daa > binding.hard_deadline_daa
            || r.rules.is_none()
            || a.fulfilled_groups.contains(&self.side)
            || a.applied_deposit_debits.contains_key(&self.side)
            || self.initial_native_present & !PalwCourtCloseGroupV2::full_mask(g.count) != 0
            || self.qualified_mask() & !g.present != 0
        {
            return Err("archive progress substitutes original scope or native source".into());
        }
        for (i, p) in &self.native_chunks {
            if *i >= g.count
                || p.daa_score < self.captured_at.daa_score
                || p.daa_score < r.point.daa_score
                || p.daa_score > g.assembly_deadline_daa
                || p.daa_score > a.session.deadline_daa
                || p.daa_score > binding.hard_deadline_daa
            {
                return Err("archive progress chunk lies outside its original native window".into());
            }
        }
        if let Some(done) = &self.completed {
            if !g.is_complete()
                || self.qualified_mask() != PalwCourtCloseGroupV2::full_mask(g.count)
                || done.point.daa_score < self.captured_at.daa_score
                || done.point.daa_score > g.assembly_deadline_daa
                || !self.native_chunks.values().any(|p| p == &done.point)
                || self.native_chunks.values().any(|p| p.daa_score > done.point.daa_score)
                || !matches!(done.close.as_ref(),PalwConsensusObjectV2::CourtClosed {session_id,verdict,..}
                    if *session_id==self.session_id && *verdict==g.verdict)
            {
                return Err("archive progress has no complete on-time close provenance".into());
            }
        }
        Ok(())
    }
    pub fn check_loaded_context(&self, state: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
        self.check_context(state).map_err(semantic)?;
        let tip = state.last_point().ok_or_else(|| semantic("archive progress has no loaded accepted point"))?;
        if self.captured_at.daa_score > tip.daa_score || self.native_chunks.values().any(|p| p.daa_score > tip.daa_score) {
            return Err(semantic("archive progress lies beyond the loaded accepted clock"));
        }
        if let Some(done) = &self.completed {
            self.check_v5(state, done)?;
        }
        Ok(())
    }
    pub(crate) fn check_v5(&self, state: &PalwChainStateV2, done: &ArchivedPositiveFulfillmentV1) -> Result<(), PalwStateV2Error> {
        use crate::palw_native_resource_v1::{
            NativeCoverageV1, NativeWireReaderV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1,
        };
        self.check_context(state).map_err(semantic)?;
        let a = state.court_end_archive_v1(self.session_id).ok_or_else(|| semantic("archive missing"))?;
        let g = &a.groups[&self.side];
        let r = match state.staged_court_row_v1(&Key::CourtCloseReceipt(self.session_id, self.side)) {
            Some(Row::CourtCloseReceipt(r)) => r.as_ref(),
            _ => return Err(semantic("archive original receipt missing")),
        };
        let rules = r.rules.ok_or_else(|| semantic("archive original native v5 inputs missing"))?;
        let total = g.chunks.values().try_fold(0usize, |n, b| n.checked_add(b.len())).ok_or_else(|| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwConsensusObjectV2>("archive assembly length"))
        })?;
        // Reserve the signature's actual cloned hash vector and Borsh growing
        // buffer, then the exact assembly. Retain the workspace through replay.
        let message_len =
            borsh::object_length(&(self.session_id, self.side, g.count, g.chunk_digests.as_slice(), g.close_digest, g.verdict))
                .map_err(|e| archive_inspection_io_error_v1("archive signature length", e))? as u64;
        let mut requests = TirNativeRequestsV1::new();
        requests.exact_v1::<Hash64>(g.chunk_digests.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        requests.vec_v1::<u8>(message_len.max(1024)).map_err(PalwStateV2Error::NativeResource)?;
        requests.exact_v1::<u8>(total as u64).map_err(PalwStateV2Error::NativeResource)?;
        let mut lease = reserve_native_requests_v1(requests.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        r.check_context(state).map_err(semantic)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(total).map_err(|e| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PalwConsensusObjectV2>(
                "archive assembly allocation",
                &e.to_string(),
            ))
        })?;
        for b in g.chunks.values() {
            bytes.extend_from_slice(b);
        }
        if palw_court_close_chunk_digest_v1(&bytes) != g.close_digest {
            return Err(semantic("archive complete whole digest mismatch"));
        }
        let mut reader = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
        let shape = PalwConsensusObjectV2::scan_native_wire_v1(&mut reader)
            .map_err(|e| archive_inspection_io_error_v1("archive loaded close shape", e))?;
        if reader.remaining() != 0 {
            return Err(semantic("archive complete wire has trailing bytes"));
        }
        let decode = shape.heap.checked_add(std::mem::size_of::<PalwConsensusObjectV2>() as u64).ok_or_else(|| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwConsensusObjectV2>("archive loaded close decode"))
        })?;
        if let Some(lease) = &mut lease {
            lease.grow_memory_v1(decode).map_err(PalwStateV2Error::NativeResource)?;
        }
        let close: PalwConsensusObjectV2 =
            borsh::from_slice(&bytes).map_err(|e| archive_inspection_io_error_v1("archive loaded close decode", e))?;
        if close != *done.close {
            return Err(semantic("archive qualification substitutes the native close bytes"));
        }
        let PalwConsensusObjectV2::CourtClosed { session_id, verdict, proof } = &close else {
            return Err(semantic("archive qualification is not a close"));
        };
        if *session_id != self.session_id
            || *verdict != g.verdict
            || (proof.is_tir_v1() && !rules.tir_active)
            || (proof.is_gen_v1() && !rules.gen_active)
            || (proof.is_eval_v1() && !rules.improvement_active)
            || (proof.is_constrained_decode_v1() && !rules.constraint_active)
            || (crate::palw_task_heads_v1::palw_object_needs_task_heads_v1(&close) && !rules.task_heads_active)
        {
            return Err(semantic("archive qualification changes verdict or retained native family"));
        }
        let prompt = if rules.prompt_ids_merkle {
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1
        } else {
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
        };
        let actual = crate::palw_court_v2::adjudicate_court_close_borrowed_v5(
            state,
            &self.session_id,
            &r.session,
            r.tir_phase.as_deref(),
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
        .map_err(|e| semantic(e.to_string()))?;
        if actual != *verdict {
            return Err(semantic("archive qualification fails retained native v5"));
        }
        Ok(())
    }
}
impl PalwChainStateV2 {
    pub fn court_archive_progress_v1(&self, id: Hash64, side: PalwCourtSideV1) -> Option<&ArchiveDeclarationProgressV1> {
        match self.staged_court_row_v1(&Key::CourtArchiveProgress(id, side)) {
            Some(Row::CourtArchiveProgress(r)) => Some(r.as_ref()),
            _ => None,
        }
    }
    pub(crate) fn qualified_archive_deposit_v1(&self, id: Hash64, side: PalwCourtSideV1) -> bool {
        self.court_archive_progress_v1(id, side).is_some_and(|r| r.completed.is_some() && r.check_context(self).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_wire_resource_failures_stay_distinct_from_truncated_material() {
        use crate::palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1};
        let overflow = NativeWireShapeV1::default().allocation(u64::MAX, 16).unwrap_err();
        assert!(matches!(archive_inspection_io_error_v1("actual native arithmetic", overflow), PalwStateV2Error::NativeResource(_)));
        let mut reader = NativeWireReaderV1::new(&[][..], 0);
        let timeout = loop {
            if let Err(e) = reader.step() {
                break e;
            }
        };
        assert!(matches!(archive_inspection_io_error_v1("actual native wire CPU", timeout), PalwStateV2Error::NativeResource(_)));
        let mut reader = NativeWireReaderV1::new(&[][..], 0);
        let malformed = PalwConsensusObjectV2::scan_native_wire_v1(&mut reader).unwrap_err();
        assert!(matches!(
            archive_inspection_io_error_v1("actual truncated material", malformed),
            PalwStateV2Error::CarriageInconsistent(_)
        ));
        // A real failed fallible allocation request, rather than a caller's
        // boolean OOM label. No allocation occurs for an impossible capacity.
        let allocation = Vec::<u64>::new().try_reserve_exact(usize::MAX).unwrap_err();
        let refused = PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<
            PalwConsensusObjectV2,
        >("actual reserve overflow", &allocation.to_string()));
        assert!(matches!(refused, PalwStateV2Error::NativeResource(_)));
    }

    #[test]
    fn borrowed_signature_quote_fields_preserve_actual_message_bytes_at_boundaries() {
        use crate::palw_court_v2::palw_court_close_declaration_message_v1;
        let id = Hash64::from_bytes([7; 64]);
        let whole = Hash64::from_bytes([9; 64]);
        for count in [1u8, 2, PALW_COURT_CLOSE_MAX_CHUNKS] {
            let hashes = (0..count).map(|i| Hash64::from_bytes([i; 64])).collect::<Vec<_>>();
            for side in [PalwCourtSideV1::Executor, PalwCourtSideV1::Challenger] {
                for verdict in [PalwCourtVerdictV2::ExecutorGuilty, PalwCourtVerdictV2::ChallengerDefeated] {
                    let actual = palw_court_close_declaration_message_v1(&id, side, count, &hashes, &whole, verdict);
                    let borrowed = (id, side, count, hashes.as_slice(), whole, verdict);
                    assert_eq!(borsh::to_vec(&borrowed).unwrap(), actual);
                    assert_eq!(borsh::object_length(&borrowed).unwrap(), actual.len());
                    let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
                    q.exact_v1::<Hash64>(u64::from(count)).unwrap();
                    q.vec_v1::<u8>((actual.len() as u64).max(1024)).unwrap();
                    assert!(
                        q.bytes_v1() >= u64::from(count) * 64 + actual.capacity() as u64,
                        "the real cloned vector and serializer capacity must fit"
                    );
                }
            }
        }
    }
}
