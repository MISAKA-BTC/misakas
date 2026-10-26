//! Literal paid greedy Float32 source duties (behind `palw_staged_float_constraint_v1`). The
//! duty page is the shared streamed-source index; a key whose bundle holds a Float32 source is
//! answered here, from the claim's public Float32 sidecar or the backend's own projection.
//! Readers survive polls; only native cursors acknowledge bytes.
use super::*;
use kaspa_consensus_core::{
    palw_backend::PalwFloatPublicSourceReaderV1,
    palw_constraint_stream_carriage_v1::{
        PALW_CONSTRAINT_STREAM_CONTEXT_V1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1,
    },
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
};

/// Its own lane round, below the Flat responder's (`u32::MAX - 17`) and the kernel verifier's.
const ROUND: u32 = u32::MAX - 22;
struct ReaderV1 {
    claim: Hash64,
    owner: PalwBondKeyV2,
    reader: Arc<std::sync::Mutex<Box<dyn PalwFloatPublicSourceReaderV1>>>,
    _memory: crate::palw_memory_ledger::PalwMemoryReservationV1,
}
#[derive(Default)]
pub(super) struct FloatSourceBookV1 {
    after: Option<(u64, Hash64, PalwBondKeyV2)>,
    readers: BTreeMap<Hash64, ReaderV1>,
}

impl PalwPanelService {
    pub(super) async fn public_float_answer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        slot: super::PalwAnswerSlotV1<'_>,
        book: &mut FloatSourceBookV1,
        pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        due: &mut HashMap<(Hash64, u32, bool), u64>,
        moved: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(), String> {
        if self.consensus_config.params.palw_staged_public_court_v1.is_none()
            || self.consensus_config.params.palw_staged_float_constraint_v1_fence().is_none()
        {
            return Ok(());
        }
        let signer = slot.slot;
        let lane_round = ROUND - slot.round_offset;
        let _metadata = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-float-duty-page",
                    class_id: Hash64::default(),
                    job: Hash64::default(),
                },
                1_048_576,
            )
            .map_err(|e| e.to_string())?;
        let (mut keys, next) = session.palw_staged_flat_duties_v1(signer, book.after)?;
        if keys.len() > 64 {
            return Err("Float32 duties exceed native bounded page".into());
        }
        book.after = next;
        for slot in book.readers.values() {
            if !keys.contains(&(slot.claim, slot.owner)) {
                keys.push((slot.claim, slot.owner));
            }
        }
        pending.retain(|(_, round, responder, object)| {
            if (*round, *responder) != (lane_round, true) {
                return true;
            }
            let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = object else { return false };
            let PalwConstraintStreamActionV1::FloatSourceAnswer(answer) = &movement.action else { return false };
            if answer.answerer != signer {
                return true;
            }
            match session.palw_staged_float_source_v1((movement.claim, movement.reserver)) {
                Err(_) => true,
                Ok(None) => false,
                Ok(Some(p)) => {
                    p.session.source_id == answer.source_id
                        && (p.session.claim, p.session.owner, p.session.reserved_daa, p.session.bundle_id)
                            == (movement.claim, movement.reserver, movement.reserved_daa, movement.seed_id)
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
                let bytes = paid.terms.plan.resident_ram_bytes.checked_mul(2).ok_or("Float32 response read memory overflow")?;
                let read = crate::palw_memory_ledger::host_ledger_v1()
                    .reserve(
                        crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                            role: "public-float-response-context",
                            class_id: Hash64::default(),
                            job: claim,
                        },
                        bytes,
                    )
                    .map_err(|e| e.to_string())?;
                // A Flat bundle on the shared page is the Flat responder's.
                let Some(progress) = session.palw_staged_float_source_v1((claim, owner))? else { return Ok(()) };
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
                if pending.iter().filter(|(_, round, responder, _)| (*round, *responder) == (lane_round, true)).count() >= 64 {
                    return Ok(());
                }
                if pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == key)
                    || moved.get(&key).is_some_and(|at| progress.now_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
                {
                    return Ok(());
                }
                attempt = Some((key, progress.now_daa));
                if !book.readers.contains_key(&id) {
                    if book.readers.len() >= 64 {
                        return Err("Float32 persistent source slots are occupied".into());
                    }
                    let context = &progress.context;
                    let class = context.job.class_id;
                    let sidecar = crate::palw_flat_public_source::float_path_v1(&self.config.retention_dir, claim);
                    let memory = crate::palw_memory_ledger::host_ledger_v1()
                        .reserve(
                            crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                                role: "public-float-answer",
                                class_id: class,
                                job: claim,
                            },
                            // One header (ids + events) and one 64 KiB part, never a resident row set.
                            8 << 20,
                        )
                        .map_err(|e| e.to_string())?;
                    let reader: Box<dyn PalwFloatPublicSourceReaderV1> = if sidecar.is_file() {
                        tokio::task::spawn_blocking(move || {
                            let source =
                                Arc::new(misaka_palw_base0::flat_public_source::Base0OriginalCaptureFileV1::open_v1(&sidecar)?);
                            kaspa_consensus_core::palw_float_constraint_source_v1::PalwFloatPublicSidecarReaderV1::open_v1(source)
                                .map(|reader| Box::new(reader) as Box<dyn PalwFloatPublicSourceReaderV1>)
                        })
                        .await
                        .map_err(|e| format!("Float32 sidecar reader failed: {e}"))??
                    } else {
                        // No sidecar: only a backend that retains f32 rows can answer (none in-node today).
                        let facts = session.palw_producer_facts_v2(class, None).ok_or("Float32 registered artifact unavailable")?;
                        let backend = self.resolve_backend(session, class, facts.artifact_root)?;
                        let payloads = self.fp_retained_payload_paths(&claim);
                        let binding = context.binding.clone();
                        let job = context.job.clone();
                        tokio::task::spawn_blocking(move || {
                            use misaka_palw_base0::flat_public_source::{Base0OriginalCaptureFileV1, Base0OriginalCaptureRangeV1};
                            for path in payloads {
                                let Ok(source) = Base0OriginalCaptureFileV1::open_v1(&path) else { continue };
                                let Ok(source) = Base0OriginalCaptureRangeV1::fp_capture_v1(Arc::new(source), &job) else { continue };
                                if let Ok(reader) = backend.open_float_public_source_v1(Arc::new(source), &binding) {
                                    return Ok(reader);
                                }
                            }
                            Err::<Box<dyn PalwFloatPublicSourceReaderV1>, String>(
                                "Float32 source has no original public sidecar or retaining backend".into(),
                            )
                        })
                        .await
                        .map_err(|e| format!("Float32 reader factory failed: {e}"))??
                    };
                    book.readers
                        .insert(id, ReaderV1 { claim, owner, reader: Arc::new(std::sync::Mutex::new(reader)), _memory: memory });
                }
                let slot = book.readers.get(&id).ok_or("Float32 source reader disappeared")?;
                if (slot.claim, slot.owner) != (claim, owner) {
                    return Err("Float32 reader substitutes literal native source".into());
                }
                let reader = slot.reader.clone();
                let clock = progress.now_daa;
                let deadline = source.response_deadline_v1(&signer).ok_or("Float32 source has no response deadline")?;
                let bundle_id = source.bundle_id;
                let answer = tokio::task::spawn_blocking(move || {
                    let _read = read;
                    let mut reader = reader.lock().map_err(|_| "Float32 reader lock poisoned")?;
                    crate::palw_flat_public_source::float_answer_v1(&progress.session, signer, clock, reader.as_mut())
                })
                .await
                .map_err(|e| format!("Float32 source poll failed: {e}"))??;
                let Some(answer) = answer else { return Ok(()) };
                let mut movement = PalwConstraintStreamMoveV1 {
                    version: 1,
                    claim,
                    reserver: owner,
                    reserved_daa: paid.reserved_daa,
                    seed_id: bundle_id,
                    phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                    action: PalwConstraintStreamActionV1::FloatSourceAnswer(answer),
                    signature: Vec::new(),
                };
                movement.signature = self
                    .sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1)
                    .ok_or("Float32 custodian signing key unavailable")?;
                movement.check()?;
                pending.push((key.0, key.1, key.2, PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) }));
                due.insert(key, deadline.saturating_sub(1));
                moved.insert(key, clock);
                Ok::<(), String>(())
            }
            .await;
            if let Err(why) = result {
                if let Some((key, clock)) = attempt {
                    moved.insert(key, clock);
                }
                // A refused read/guard never manufactures a source response or a conviction.
                debug!("[{PALW_PANEL}] claim {claim}: original Float32 response remains pending: {why}");
                live.extend(book.readers.iter().filter_map(|(id, slot)| (slot.claim == claim && slot.owner == owner).then_some(*id)));
            }
        }
        book.readers.retain(|id, _| live.contains(id));
        Ok(())
    }
}
