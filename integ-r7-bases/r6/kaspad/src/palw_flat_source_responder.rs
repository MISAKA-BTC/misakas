//! Literal paid Flat-source duties. Readers survive polls; only native cursors acknowledge bytes.
use super::*;
use crate::palw_panel::court_outbox::CourtOutboxV1;
use kaspa_consensus_core::{
    palw_backend::PalwFlatPublicSourceReaderV1,
    palw_constraint_stream_carriage_v1::{
        PALW_CONSTRAINT_STREAM_CONTEXT_V1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1,
    },
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
};

const ROUND: u32 = u32::MAX - 17;
struct ReaderV1 {
    claim: Hash64,
    owner: PalwBondKeyV2,
    reader: Arc<std::sync::Mutex<Box<dyn PalwFlatPublicSourceReaderV1>>>,
    _memory: crate::palw_memory_ledger::PalwMemoryReservationV1,
}
#[derive(Default)]
pub(super) struct FlatSourceBookV1 {
    after: Option<(u64, Hash64, PalwBondKeyV2)>,
    readers: BTreeMap<Hash64, ReaderV1>,
}

impl PalwPanelService {
    pub(super) async fn public_flat_answer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        slot: super::PalwAnswerSlotV1<'_>,
        book: &mut FlatSourceBookV1,
        court_outbox: &mut CourtOutboxV1,
    ) -> Result<(), String> {
        if self.consensus_config.params.palw_staged_public_court_v1.is_none() {
            return Ok(());
        }
        let signer = slot.slot;
        let lane_round = ROUND - slot.round_offset;
        let _metadata = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-flat-duty-page",
                    class_id: Hash64::default(),
                    job: Hash64::default(),
                },
                1_048_576,
            )
            .map_err(|e| e.to_string())?;
        let (mut keys, next) = session.palw_staged_flat_duties_v1(signer, book.after)?;
        if keys.len() > 64 {
            return Err("Flat duties exceed native bounded page".into());
        }
        book.after = next;
        for slot in book.readers.values() {
            if !keys.contains(&(slot.claim, slot.owner)) {
                keys.push((slot.claim, slot.owner));
            }
        }
        // Refresh pending packets directly, independently of the paginated discovery cursor.
        court_outbox.retain_pending(|(_, round, responder, object)| {
            if (*round, *responder) != (lane_round, true) {
                return true;
            }
            let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = object else { return false };
            let PalwConstraintStreamActionV1::FlatSourceAnswer(answer) = &movement.action else { return false };
            if answer.answerer != signer {
                return true;
            }
            let Ok(Some(paid)) = session.palw_staged_paid_authority_v1((movement.claim, movement.reserver)) else { return true };
            let Some(bytes) = paid.terms.plan.resident_ram_bytes.checked_mul(2) else { return true };
            let Ok(_read) = crate::palw_memory_ledger::host_ledger_v1().reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-flat-queued-snapshot",
                    class_id: Hash64::default(),
                    job: movement.claim,
                },
                bytes,
            ) else {
                return true;
            };
            match session.palw_staged_flat_source_v1((movement.claim, movement.reserver)) {
                Err(_) => true,
                Ok(None) => false,
                Ok(Some(p)) => {
                    p.session.source_id == answer.source_id
                        && (p.session.claim, p.session.owner, p.session.reserved_daa, p.session.bundle_id)
                            == (movement.claim, movement.reserver, movement.reserved_daa, movement.seed_id)
                        && answer.answerer == signer
                        && p.session.has_response_duty_v1(&signer)
                        && p.session.response_deadline_v1(&signer).is_some_and(|until| p.now_daa < until)
                        && p.session.slot_cursor_v1(&signer).is_ok_and(|cursor| cursor == answer.cursor)
                }
            }
        });
        let mut live = HashSet::new();
        for (claim, owner) in keys {
            if slot.only.is_some_and(|only| !only.contains(&claim)) {
                continue;
            }
            let mut attempt = None;
            let result = async {
                let Some(paid) = session.palw_staged_paid_authority_v1((claim, owner))? else { return Ok(()) };
                let bytes = paid.terms.plan.resident_ram_bytes.checked_mul(2).ok_or("Flat response read memory overflow")?;
                let read = crate::palw_memory_ledger::host_ledger_v1()
                    .reserve(
                        crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                            role: "public-flat-response-context",
                            class_id: Hash64::default(),
                            job: claim,
                        },
                        bytes,
                    )
                    .map_err(|e| e.to_string())?;
                let Some(progress) = session.palw_staged_flat_source_v1((claim, owner))? else { return Ok(()) };
                let source = &progress.session;
                let id = source.source_id;
                if progress.now_daa < source.collect_until_daa && !source.collected_v1() {
                    live.insert(id);
                }
                if !source.has_response_duty_v1(&signer)
                    || source.response_deadline_v1(&signer).is_none_or(|until| progress.now_daa >= until)
                {
                    return Ok(());
                }
                let key = (id, lane_round, true);
                if court_outbox.pending().iter().filter(|(_, round, responder, _)| (*round, *responder) == (lane_round, true)).count()
                    >= 64
                {
                    return Ok(());
                }
                if court_outbox.pending().iter().any(|(a, b, c, _)| (*a, *b, *c) == key)
                    || court_outbox.submitted_at(&key).is_some_and(|at| progress.now_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
                {
                    return Ok(());
                }
                attempt = Some((key, progress.now_daa));
                if !book.readers.contains_key(&id) {
                    if book.readers.len() >= 64 {
                        return Err("Flat persistent source slots are occupied".into());
                    }
                    let context = &progress.context;
                    let class = context.job.class_id;
                    let facts = session.palw_producer_facts_v2(class, None).ok_or("Flat registered artifact unavailable")?;
                    let need = self
                        .backends()
                        .role_memory_need_or_chain_v1(class, facts.artifact_root, FULL_SEAT_V1, |id| {
                            self.chain_carriage_v1(session, id)
                        })
                        .ok_or("Flat source resource profile unavailable")?;
                    let memory = self.reserve_replay_v1("public-flat-answer", &need, class, claim)?;
                    let backend = self.resolve_backend(session, class, facts.artifact_root)?;
                    let sidecar = crate::palw_flat_public_source::path_v1(&self.config.retention_dir, claim);
                    let payloads = self.fp_retained_payload_paths(&claim);
                    let binding = context.binding.clone();
                    let job = context.job.clone();
                    let reader = tokio::task::spawn_blocking(move || {
                        use misaka_palw_base0::flat_public_source::{Base0OriginalCaptureFileV1, Base0OriginalCaptureRangeV1};
                        let reader = if sidecar.is_file() {
                            let source = Arc::new(Base0OriginalCaptureFileV1::open_v1(&sidecar)?);
                            backend.open_flat_public_source_v1(source, &binding)?
                        } else {
                            let mut found = None;
                            for path in payloads {
                                let Ok(source) = Base0OriginalCaptureFileV1::open_v1(&path) else { continue };
                                let Ok(source) = Base0OriginalCaptureRangeV1::fp_capture_v1(Arc::new(source), &job) else { continue };
                                if let Ok(reader) = backend.open_flat_public_source_v1(Arc::new(source), &binding) {
                                    found = Some(reader);
                                    break;
                                }
                            }
                            found.ok_or("Flat source has no original retained public capture")?
                        };
                        Ok::<_, String>((reader, memory))
                    })
                    .await
                    .map_err(|e| format!("Flat reader factory failed: {e}"))??;
                    book.readers
                        .insert(id, ReaderV1 { claim, owner, reader: Arc::new(std::sync::Mutex::new(reader.0)), _memory: reader.1 });
                }
                let slot = book.readers.get(&id).ok_or("Flat source reader disappeared")?;
                if (slot.claim, slot.owner) != (claim, owner) {
                    return Err("Flat reader substitutes literal native source".into());
                }
                let reader = slot.reader.clone();
                let clock = progress.now_daa;
                let deadline = source.response_deadline_v1(&signer).ok_or("Flat source has no response deadline")?;
                let bundle_id = source.bundle_id;
                let answer = tokio::task::spawn_blocking(move || {
                    let _read = read;
                    let mut reader = reader.lock().map_err(|_| "Flat reader lock poisoned")?;
                    crate::palw_flat_public_source::answer_v1(&progress.session, signer, clock, reader.as_mut())
                })
                .await
                .map_err(|e| format!("Flat source poll failed: {e}"))??;
                let Some(answer) = answer else { return Ok(()) };
                let mut movement = PalwConstraintStreamMoveV1 {
                    version: 1,
                    claim,
                    reserver: owner,
                    reserved_daa: paid.reserved_daa,
                    seed_id: bundle_id,
                    phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                    action: PalwConstraintStreamActionV1::FlatSourceAnswer(answer),
                    signature: Vec::new(),
                };
                movement.signature = self
                    .sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1)
                    .ok_or("Flat custodian signing key unavailable")?;
                movement.check()?;
                court_outbox.push_pending((
                    key.0,
                    key.1,
                    key.2,
                    PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) },
                ));
                court_outbox.set_due(key, deadline.saturating_sub(1));
                court_outbox.mark_submitted(key, clock);
                Ok::<(), String>(())
            }
            .await;
            if let Err(why) = result {
                if let Some((key, clock)) = attempt {
                    court_outbox.mark_submitted(key, clock);
                }
                // A refused read/guard never manufactures a source response or a conviction.
                debug!("[{PALW_PANEL}] claim {claim}: original Flat response remains pending: {why}");
                live.extend(book.readers.iter().filter_map(|(id, slot)| (slot.claim == claim && slot.owner == owner).then_some(*id)));
            }
        }
        book.readers.retain(|id, _| live.contains(id));
        Ok(())
    }
}
