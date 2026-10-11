//! Deterministic verifier-owned tokenizer court continuation. This worker reads only the
//! D7 holding and authenticated binding/job/table. One guarded move is kept until its exact
//! accepted cursor is observed; a submitted carrier is never an acknowledgement.
use crate::{
    palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryReservationKeyV1, PalwMemoryReservationV1},
    palw_tokenizer_witness::{TokenizerWitnessMoveV1, TokenizerWitnessPacketV1, TokenizerWitnessV1},
};
use kaspa_consensus_core::{
    Hash64,
    palw_constraint_stream_carriage_v1::{PalwAuxiliaryPublicCourtSeedV1, PalwConstraintStreamActionV1, PalwConstraintStreamSeedV1},
    palw_fp_constraint_job_v1::{PALW_TOKEN_TABLE_MAX_LEAVES_V1, PalwTokenTableV1, table_eog_leaf_hash_v1, table_leaf_hash_v1},
    palw_freeprompt_v3::PalwFreePromptJobV3,
    palw_offence_attribution_v1::{PalwOffenceTargetV1, palw_constrained_job_target_v1},
    palw_staged_court_state_v1::{PalwStagedCertificateRefV1, PalwStagedOwnedCursorV1, PalwStagedOwnedProgressV1},
    palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1, step_opening_from_leaf_source_capped_v1, step_opening_root_capped_v1},
    palw_step_refute::PalwTiledDecodePinV1,
    palw_tokenizer_rendering_v1::{PalwQwenRenderingMapV1, palw_tokenizer_raw_commitment_v1},
    palw_tokenizer_table_stream_v1::PalwTokenizerTableStreamV1,
};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TokenizerCursorV1 {
    pub offset: u64,
    pub pending_key: Option<(u64, Hash64)>,
    pub key_root: Hash64,
    pub key_events: u64,
    pub sealed: bool,
    pub rendering_offset: u32,
}
struct Candidate {
    path: PathBuf,
    table: Arc<PalwTokenTableV1>,
    id: u32,
    actual_length: u64,
    seed: PalwConstraintStreamSeedV1,
    _reservation: Arc<PalwMemoryReservationV1>,
}
impl Candidate {
    fn select(
        path: &Path,
        target: &PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        table: Arc<PalwTokenTableV1>,
        progress: &PalwStagedOwnedProgressV1,
        ledger: &Arc<PalwMemoryLedgerV1>,
    ) -> Result<Self, String> {
        palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
        if progress.claim != target.claim_id
            || progress.now_daa < progress.terms.accepted_daa
            || progress.reserved_daa > progress.now_daa
        {
            return Err("staged snapshot does not identify the admitted public claim/reservation".into());
        }
        let row = target.registered_semantics.as_ref().ok_or("missing immutable rendering semantics")?;
        let raw = row.raw_tokenizer_bytes.ok_or("historical tokenizer lacks signed length")?;
        let bound = row.tokenizer_key_events.ok_or("historical tokenizer lacks signed work")?;
        let root = row.token_table_root.ok_or("missing immutable table root")?;
        if row.rendering_rule.is_none()
            || !row.matches_context(&binding.job_context)
            || table.tokenizer_id != row.tokenizer_id
            || table.entries.is_empty()
            || table.entries.len() != binding.shape_profile.vocab_size as usize
        {
            return Err("held table is not the registered rendering/profile statement".into());
        }
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let actual_length = file.metadata().map_err(|e| e.to_string())?.len();
        // All JSON value/string copies, map nodes and temporary renderings precede allocation.
        // Capped Merkle traversal has O(log vocab) live hashes; table bytes are the registry's
        // existing immutable holding, not a second table copy in this worker.
        let scratch = actual_length.checked_mul(128).and_then(|n| n.checked_add(1_048_576)).ok_or("selection RAM overflow")?;
        let _scratch = ledger
            .reserve(
                PalwMemoryReservationKeyV1 { role: "public-tokenizer-selection", class_id: target.class_id, job: target.claim_id },
                scratch,
            )
            .map_err(|e| e.to_string())?;
        let capacity = usize::try_from(actual_length).map_err(|_| "held tokenizer exceeds platform address space")?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(capacity).map_err(|_| "held tokenizer allocation refused")?;
        file.take(actual_length.checked_add(1).ok_or("held tokenizer length overflow")?)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 != actual_length || palw_tokenizer_raw_commitment_v1(&bytes) != row.tokenizer_id {
            return Err("own tokenizer operand does not authenticate to the registered file".into());
        }
        let map = PalwQwenRenderingMapV1::parse(&bytes);
        let id = match &map {
            Ok(map) => {
                table.entries.iter().enumerate().position(|(i, e)| map.rendering(i as u32).as_ref() != e.as_ref()).unwrap_or(0) as u32
            }
            Err(_) => 0,
        };
        let count = table.entries.len() as u64 + 1;
        let leaf = |j: u64| {
            if j == table.entries.len() as u64 {
                table_eog_leaf_hash_v1(&table.eog_token_ids)
            } else {
                table_leaf_hash_v1(j as u32, table.entries[j as usize].as_deref())
            }
        };
        let opening = step_opening_from_leaf_source_capped_v1(count, u64::from(id), PALW_TOKEN_TABLE_MAX_LEAVES_V1, leaf)
            .map_err(|e| e.to_string())?;
        if step_opening_root_capped_v1(count, &opening, PALW_TOKEN_TABLE_MAX_LEAVES_V1).map_err(|e| e.to_string())? != root {
            return Err("held table is not authenticated by the immutable root".into());
        }
        let eog_opening = step_opening_from_leaf_source_capped_v1(count, count - 1, PALW_TOKEN_TABLE_MAX_LEAVES_V1, leaf)
            .map_err(|e| e.to_string())?;
        let length = table.entries[id as usize].as_ref().map_or(0, Vec::len);
        let retained = Arc::new(
            ledger
                .reserve(
                    PalwMemoryReservationKeyV1 {
                        role: "public-tokenizer-staged-carrier",
                        class_id: target.class_id,
                        job: target.claim_id,
                    },
                    1_048_576,
                )
                .map_err(|e| e.to_string())?,
        );
        // Selection fields are unused by typed auxiliary admission. Their canonical empty
        // representation makes the seed deterministic without inventing a public row witness.
        let empty = || PalwStepOpeningV1 { leaf_index: 0, leaf_hash: Hash64::default(), siblings: Vec::new() };
        let seed = PalwConstraintStreamSeedV1 {
            version: 1,
            claim: target.claim_id,
            reserver: progress.reserver,
            reserved_daa: progress.reserved_daa,
            binding: binding.clone(),
            job: None,
            pin: PalwTiledDecodePinV1 {
                position: 0,
                generated_token_ids: Vec::new(),
                row_root: Hash64::default(),
                row_opening: empty(),
                committed_tile_lanes: Vec::new(),
                committed_opening: empty(),
                beat_tile_lanes: Vec::new(),
                beat_opening: empty(),
                beat_lane: 0,
            },
            witness: None,
            eog_count: u32::try_from(table.eog_token_ids.len()).map_err(|_| "eog count overflow")?,
            eog_opening,
            auxiliary: Some(PalwAuxiliaryPublicCourtSeedV1::Tokenizer {
                id,
                present: table.entries[id as usize].is_some(),
                length: u32::try_from(length).map_err(|_| "table rendering length overflow")?,
                opening,
                raw_tokenizer_bytes: raw,
                tokenizer_key_events: bound,
                operand_tokenizer_bytes: actual_length,
            }),
            signature: Vec::new(),
        };
        Ok(Self { path: path.to_owned(), table, id, actual_length, seed, _reservation: retained })
    }
}
struct Pending {
    before: TokenizerCursorV1,
    after: TokenizerCursorV1,
    action: PalwConstraintStreamActionV1,
    _packet: Option<TokenizerWitnessPacketV1>,
}
pub(crate) enum TokenizerScheduleV1 {
    Seed(PalwConstraintStreamSeedV1),
    Move(kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1),
    Finished(Option<PalwStagedCertificateRefV1>),
}
pub(crate) struct TokenizerSchedulerV1 {
    candidate: Candidate,
    witness: TokenizerWitnessV1,
    court: PalwTokenizerTableStreamV1,
    sealed: bool,
    pending: Option<Pending>,
    moves: u64,
    context: Hash64,
}
impl TokenizerSchedulerV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        path: &Path,
        target: &PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        table: Arc<PalwTokenTableV1>,
        progress: &PalwStagedOwnedProgressV1,
        ledger: &Arc<PalwMemoryLedgerV1>,
    ) -> Result<Self, String> {
        let candidate = Candidate::select(path, target, binding, job, table, progress, ledger)?;
        let context = Hash64::from_bytes(candidate.seed.signing_message().try_into().map_err(|_| "seed digest width")?);
        if progress.seed_id.is_some_and(|id| id != context) {
            return Err("accepted owner court has another immutable seed; no replacement is permitted".into());
        }
        let row = target.registered_semantics.as_ref().ok_or("missing semantics")?;
        let witness = TokenizerWitnessV1::open(
            &candidate.path,
            context,
            row.tokenizer_id,
            candidate.actual_length,
            row.tokenizer_key_events.unwrap(),
            candidate.id,
            target.class_id,
            target.claim_id,
            ledger,
        )?;
        let Some(PalwAuxiliaryPublicCourtSeedV1::Tokenizer { id, present, length, opening, .. }) = &candidate.seed.auxiliary else {
            unreachable!()
        };
        let court = PalwTokenizerTableStreamV1::new_with_operand_length(
            context,
            target,
            binding,
            job,
            *id,
            *present,
            *length,
            opening,
            candidate.actual_length,
        )?;
        Ok(Self { candidate, witness, court, sealed: false, pending: None, moves: 1, context })
    }
    /// Each decision is derived from one authenticated accepted snapshot. Signing/fee
    /// funding use the ordinary service carrier path after this blocking worker returns.
    pub(crate) fn poll(&mut self, progress: &PalwStagedOwnedProgressV1) -> Result<TokenizerScheduleV1, String> {
        if (progress.claim, progress.reserver, progress.reserved_daa)
            != (self.candidate.seed.claim, self.candidate.seed.reserver, self.candidate.seed.reserved_daa)
            || progress.now_daa < progress.terms.accepted_daa
            || progress.terms.accepted_daa > progress.terms.open_until_daa
            || progress.terms.open_until_daa > progress.terms.stream_until_daa
            || progress.terms.stream_until_daa > progress.terms.collect_until_daa
        {
            return Err("staged snapshot changes claim/reservation or its admitted clocks".into());
        }
        let Some(cursor) = &progress.cursor else {
            if progress.seed_id.is_some() || progress.now_daa >= progress.terms.open_until_daa {
                return Err("prospective tokenizer opening deadline is closed".into());
            }
            return Ok(TokenizerScheduleV1::Seed(self.candidate.seed.clone()));
        };
        if progress.seed_id != Some(self.context) {
            return Err("snapshot owns a different immutable staged seed".into());
        }
        let PalwStagedOwnedCursorV1::Tokenizer {
            offset,
            tokenizer_sealed,
            pending_key,
            key_root,
            key_events,
            rendering_offset,
            completed,
        } = cursor
        else {
            return Err("this reservation owns another typed public court".into());
        };
        let accepted = TokenizerCursorV1 {
            offset: *offset,
            sealed: *tokenizer_sealed,
            pending_key: *pending_key,
            key_root: *key_root,
            key_events: *key_events,
            rendering_offset: *rendering_offset,
        };
        if self.pending.is_none() {
            self.resume(&accepted, progress.terms.plan.maximum_moves)?;
        }
        if let Some(fault) = completed {
            if progress.now_daa >= progress.terms.collect_until_daa || self.cursor() != accepted || self.court.finish()? != *fault {
                return Err("accepted completion does not match authenticated local court or collection clock".into());
            }
            self.pending = None;
            return Ok(TokenizerScheduleV1::Finished(fault.then_some(PalwStagedCertificateRefV1 {
                claim: progress.claim,
                reserver: progress.reserver,
                reserved_daa: progress.reserved_daa,
                seed_id: self.context,
            })));
        }
        if progress.now_daa >= progress.terms.stream_until_daa {
            return Err("paid tokenizer streaming deadline is closed".into());
        }
        let action = self.next(&accepted, progress.terms.plan.maximum_moves)?;
        Ok(TokenizerScheduleV1::Move(kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1 {
            version: 1,
            claim: progress.claim,
            reserver: progress.reserver,
            reserved_daa: progress.reserved_daa,
            seed_id: self.context,
            phase: kaspa_consensus_core::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::Tokenizer,
            action,
            signature: Vec::new(),
        }))
    }
    pub(crate) fn seed(&self) -> &PalwConstraintStreamSeedV1 {
        &self.candidate.seed
    }
    pub(crate) fn seed_id(&self) -> Hash64 {
        self.context
    }
    fn cursor(&self) -> TokenizerCursorV1 {
        TokenizerCursorV1 {
            offset: self.court.tokenizer_offset(),
            pending_key: self.court.pending_key(),
            key_root: self.court.key_set_root(),
            key_events: self.court.key_events(),
            sealed: self.sealed,
            rendering_offset: self.court.rendering_offset(),
        }
    }
    fn generate(&mut self) -> Result<Pending, String> {
        let before = self.cursor();
        let (action, packet) = if !self.sealed {
            match self.witness.next()? {
                TokenizerWitnessMoveV1::Packet(packet) => {
                    self.court.packet(self.context, &packet.packet)?;
                    (PalwConstraintStreamActionV1::TokenizerPacket(packet.packet.clone()), Some(packet))
                }
                TokenizerWitnessMoveV1::Seal => {
                    self.court.seal_tokenizer(self.context)?;
                    self.sealed = true;
                    (PalwConstraintStreamActionV1::TokenizerSeal, None)
                }
            }
        } else {
            let offset = self.court.rendering_offset();
            let bytes = self.candidate.table.entries[self.candidate.id as usize].as_deref().unwrap_or_default();
            let part = bytes.get(offset as usize..).ok_or("accepted rendering offset exceeds held original leaf")?;
            if part.is_empty() {
                return Err("completed tokenizer court requires accepted certificate, not another move".into());
            }
            let part = part[..part.len().min(65536)].to_vec();
            self.court.rendering_part(self.context, offset, &part)?;
            (PalwConstraintStreamActionV1::TokenizerRenderingPart { offset, bytes: part }, None)
        };
        self.moves = self.moves.checked_add(1).ok_or("tokenizer replay counter overflow")?;
        Ok(Pending { before, after: self.cursor(), action, _packet: packet })
    }
    /// Restore by deterministic bounded own-file replay, comparing every parser-derived key
    /// root/count and the sealed bit, not trusting a caller-supplied parser checkpoint.
    pub(crate) fn resume(&mut self, accepted: &TokenizerCursorV1, maximum_moves: u64) -> Result<(), String> {
        while self.cursor() != *accepted {
            if self.moves >= maximum_moves {
                return Err("accepted tokenizer cursor exceeds paid bounded replay envelope".into());
            }
            let before = self.cursor();
            let next = self.generate()?;
            if self.cursor() == before
                || self.cursor().offset > accepted.offset
                || self.cursor().key_events > accepted.key_events
                || self.cursor().rendering_offset > accepted.rendering_offset
                || (self.sealed && !accepted.sealed)
            {
                return Err("accepted tokenizer cursor is not a deterministic complete packet boundary".into());
            }
            drop(next);
        }
        Ok(())
    }
    /// Pending packet owns its memory guard through queue/retry/acceptance. A stale snapshot
    /// repeats the same move. Reorg behind the pending boundary is reported for reconstruction.
    pub(crate) fn next(&mut self, accepted: &TokenizerCursorV1, maximum_moves: u64) -> Result<PalwConstraintStreamActionV1, String> {
        if let Some(p) = &self.pending {
            if p.before == *accepted {
                return Ok(p.action.clone());
            }
            if p.after != *accepted {
                return Err("tokenizer cursor changed branch; rebuild from authenticated holding".into());
            }
            self.pending = None;
        } else if self.cursor() != *accepted {
            return Err("tokenizer worker needs bounded replay to accepted cursor".into());
        }
        if self.moves >= maximum_moves {
            return Err("tokenizer court reached its paid move envelope".into());
        }
        self.pending = Some(self.generate()?);
        Ok(self.pending.as_ref().unwrap().action.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tokenizer::tests::{RAW, prospective_table};
    use super::*;
    use crate::palw_memory_ledger::PalwMemoryPoolV1;
    use kaspa_consensus_core::{
        palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
        palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
        palw_rendering_resources_v1::{PalwRenderingClaimTermsV1, PalwStagedPublicCourtConfigV1, PalwTokenizerResourcePlanV1},
        palw_tokenizer_owned_court_v1::PalwOwnedTokenizerCourtV1,
    };
    fn progress(target: &PalwOffenceTargetV1, bound: u64) -> PalwStagedOwnedProgressV1 {
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let plan = PalwTokenizerResourcePlanV1::derive_with_key_bound(RAW.len() as u64, bound, config.prices)
            .unwrap()
            .court_envelope(4096, config.prices)
            .unwrap();
        PalwStagedOwnedProgressV1 {
            snapshot_block: kaspa_consensus_core::BlockHash::from_u64_word(17),
            now_daa: 100,
            claim: target.claim_id,
            reserver: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
                transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(711),
                index: 0,
            }),
            reserved_daa: 100,
            terms: PalwRenderingClaimTermsV1::derive(100, plan, 100, 100, 10).unwrap(),
            seed_id: None,
            cursor: None,
        }
    }
    #[test]
    fn guarded_tokenizer_scheduler_restarts_retries_and_closes_only_the_real_core_court() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tokenizer.json");
        std::fs::write(&path, RAW).unwrap();
        for (table_fault, bound) in [(false, 1000), (true, 1000), (false, 1)] {
            let (mut target, binding, job, table) = prospective_table(table_fault);
            target.registered_semantics.as_mut().unwrap().tokenizer_key_events = Some(bound);
            let table = Arc::new(table);
            let mut progress = progress(&target, bound);
            let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(8_000_000), || None);
            let mut worker = TokenizerSchedulerV1::open(&path, &target, &binding, &job, table.clone(), &progress, &ledger).unwrap();
            let seed_id = worker.seed_id();
            let mut seed = worker.seed().clone();
            let Some(PalwAuxiliaryPublicCourtSeedV1::Tokenizer { id, .. }) = &seed.auxiliary else { panic!() };
            assert_eq!(
                *id,
                if table_fault { binding.shape_profile.vocab_size - 1 } else { 0 },
                "all registered entries are checked before nomination"
            );
            seed.signature = vec![0; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
            let config = PalwStagedPublicCourtConfigV1::network_v1();
            let mut row = PalwOwnedTokenizerCourtV1::admit(
                &seed,
                &job,
                &target,
                progress.terms,
                config.prices,
                100,
                u64::MAX,
                100000,
                progress.terms.material_until_daa,
                progress.terms.bond_exit_after_daa,
                &[progress.terms.bond_exit_after_daa; 3],
            )
            .unwrap();
            progress.seed_id = Some(seed_id);
            let mut cursor = worker.cursor();
            let mut sealed = false;
            for turn in 0..100 {
                let action = worker.next(&cursor, progress.terms.plan.maximum_moves).unwrap();
                assert_eq!(
                    worker.next(&cursor, progress.terms.plan.maximum_moves).unwrap(),
                    action,
                    "mempool delivery is not acknowledgement"
                );
                let movement = PalwConstraintStreamMoveV1 {
                    version: 1,
                    claim: target.claim_id,
                    reserver: progress.reserver,
                    reserved_daa: 100,
                    seed_id,
                    phase: PalwConstraintStreamPhaseV1::Tokenizer,
                    action: action.clone(),
                    signature: seed.signature.clone(),
                };
                row.movement(100, &movement).unwrap();
                sealed |= matches!(action, PalwConstraintStreamActionV1::TokenizerSeal);
                cursor = TokenizerCursorV1 {
                    offset: row.court().tokenizer_offset(),
                    pending_key: row.court().pending_key(),
                    key_root: row.court().key_set_root(),
                    key_events: row.court().key_events(),
                    sealed,
                    rendering_offset: row.court().rendering_offset(),
                };
                assert_eq!(cursor, worker.cursor(), "local generated packet must exactly reproduce accepted core state");
                if let Ok(certificate) = row.certificate() {
                    assert_eq!(certificate.is_some(), table_fault || bound == 1);
                    break;
                }
                // Stop immediately after the seal as well: EOF alone is insufficient to choose the next move.
                if turn == 0 || matches!(action, PalwConstraintStreamActionV1::TokenizerSeal) {
                    let id = worker.seed_id();
                    drop(worker);
                    worker = TokenizerSchedulerV1::open(&path, &target, &binding, &job, table.clone(), &progress, &ledger).unwrap();
                    assert_eq!(worker.seed_id(), id, "accepted-clock changes cannot rename the immutable seed");
                    worker.resume(&cursor, progress.terms.plan.maximum_moves).unwrap();
                }
                assert!(turn < 99, "the real core court must complete");
            }
            let wrong = TokenizerCursorV1 { key_root: Hash64::from_u64_word(77), ..cursor.clone() };
            assert!(
                worker.next(&wrong, progress.terms.plan.maximum_moves).is_err(),
                "a caller root cannot replace accepted parser work"
            );
            drop(worker);
            assert_eq!(ledger.reserved_bytes(), 0);
        }
    }
    #[test]
    fn tokenizer_scheduler_refuses_wrong_operands_changed_job_and_unfunded_heap_before_seed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tokenizer.json");
        std::fs::write(&path, RAW).unwrap();
        let (mut target, binding, job, table) = prospective_table(false);
        target.registered_semantics.as_mut().unwrap().tokenizer_key_events = Some(1000);
        let progress = progress(&target, 1000);
        let table = Arc::new(table);
        let tiny = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(1), || None);
        assert!(TokenizerSchedulerV1::open(&path, &target, &binding, &job, table.clone(), &progress, &tiny).is_err());
        assert_eq!(tiny.reserved_bytes(), 0);
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(8_000_000), || None);
        let mut changed = job.clone();
        changed.job_nonce[0] ^= 1;
        assert!(TokenizerSchedulerV1::open(&path, &target, &binding, &changed, table.clone(), &progress, &ledger).is_err());
        std::fs::write(&path, b"wrong held model file").unwrap();
        assert!(TokenizerSchedulerV1::open(&path, &target, &binding, &job, table, &progress, &ledger).is_err());
        assert_eq!(ledger.reserved_bytes(), 0);
    }
}
