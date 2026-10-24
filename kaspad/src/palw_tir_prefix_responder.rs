//! Literal paid TIR prefix-state duties (behind `palw_staged_tir_route_v1`). The duty page is the
//! shared streamed-source index; a key whose bundle's active stage is a TIR prefix court is
//! answered here, one authenticated boundary leaf per move, from this node's retained TIR
//! capture through the backend's `disclose_tir_step_leaf_v1` (RFC-0002's TIR executor). Only the
//! accepted slot cursor acknowledges a leaf.
use super::*;
use crate::palw_panel::court_outbox::CourtOutboxV1;
use kaspa_consensus_core::{
    palw_constraint_stream_carriage_v1::{
        PALW_CONSTRAINT_STREAM_CONTEXT_V1, PALW_PREFIX_BATCH_CARRIER_BYTES_V1, PalwConstraintStreamActionV1,
        PalwConstraintStreamMoveV1,
    },
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
    palw_tir_prefix_owned_v1::PalwTirPrefixLeafAnswerV1,
};

/// Its own lane round, below the Float32 responder's (`u32::MAX - 22`).
const ROUND: u32 = u32::MAX - 23;

struct RetainedCaptureV1 {
    bytes: Vec<u8>,
    _memory: crate::palw_memory_ledger::PalwMemoryReservationV1,
}

/// A local responder's original capture is not a verifier credential. Strip only
/// an exact public-job FPC1 envelope, and reserve before any whole-capture allocation.
fn retained_capture_v1(
    path: &std::path::Path,
    job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
    claim: Hash64,
    ledger: &Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1>,
) -> Result<RetainedCaptureV1, String> {
    use kaspa_consensus_core::palw_backend::PalwOriginalCaptureReaderV1;
    use misaka_palw_base0::flat_public_source::{Base0OriginalCaptureFileV1, Base0OriginalCaptureRangeV1};
    let file = Arc::new(Base0OriginalCaptureFileV1::open_v1(path)?);
    let magic = file.read_v1(0, file.len_v1().min(8) as u32)?;
    let source: Arc<dyn PalwOriginalCaptureReaderV1> = if magic.starts_with(b"FPC1") {
        Arc::new(Base0OriginalCaptureRangeV1::fp_capture_v1(file, job)?)
    } else if magic == misaka_palw_sdk::lineages::tir::TirCaptureV1::MAGIC {
        file
    } else {
        return Err("TIR prefix response needs an original capture, not job/answer-only material".into());
    };
    let length = source.len_v1();
    let bytes = length.checked_mul(8).and_then(|n| n.checked_add(65_536)).ok_or("TIR retained capture memory overflow")?;
    let memory = ledger
        .reserve(
            crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                role: "public-tir-prefix-capture",
                class_id: job.class_id,
                job: claim,
            },
            bytes,
        )
        .map_err(|e| e.to_string())?;
    let length = usize::try_from(length).map_err(|_| "TIR capture length exceeds platform address space")?;
    let mut capture = Vec::new();
    capture.try_reserve_exact(length).map_err(|_| "TIR retained capture allocation refused")?;
    while capture.len() < length {
        let count = (length - capture.len()).min(misaka_palw_base0::flat_public_source::FLAT_SOURCE_MAX_READ_V1 as usize);
        let part = source.read_v1(capture.len() as u64, count as u32)?;
        if part.len() != count {
            return Err("TIR retained capture changed its exact original range".into());
        }
        capture.extend_from_slice(&part);
    }
    Ok(RetainedCaptureV1 { bytes: capture, _memory: memory })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1};
    use kaspa_consensus_core::palw_freeprompt_v3::{PalwFreePromptJobV3, palw_fp_capture_encode_v1, palw_fp_material_encode_v1};

    fn job() -> PalwFreePromptJobV3 {
        PalwFreePromptJobV3 {
            version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(1),
            class_id: Hash64::from_u64_word(2),
            executor_bond: kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_u64_word(3), 0),
            executor_pubkey: vec![4; 32],
            operator_id: Hash64::from_u64_word(5),
            anchor_block: Hash64::from_u64_word(6),
            anchor_daa: 10,
            job_nonce: [7; 32],
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::from_u64_word(8),
            prompt_tokens: 2,
            decode_token_limit: 1,
            max_context_tokens: 32,
            privacy_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
            decode: None,
            tail: None,
        }
    }

    #[test]
    fn tir_prefix_retention_strips_only_the_exact_original_fp_envelope_and_holds_its_memory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claim.material");
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(1_048_576), || None);
        let job = job();
        // Codec-selection fixture, not a TIR proof: the backend still has to
        // authenticate the capture against the native accepted header afterward.
        let mut capture = misaka_palw_sdk::lineages::tir::TirCaptureV1::MAGIC.to_vec();
        capture.extend(vec![9; 32_777]);
        for bytes in [capture.clone(), palw_fp_capture_encode_v1(&job, &[1, 2], &capture)] {
            std::fs::write(&path, bytes).unwrap();
            let held = retained_capture_v1(&path, &job, Hash64::from_u64_word(11), &ledger).unwrap();
            assert_eq!(held.bytes, capture);
            assert!(ledger.reserved_bytes() >= capture.len() as u64 * 8 + 65_536);
            drop(held);
            assert_eq!(ledger.reserved_bytes(), 0);
        }
        std::fs::write(&path, palw_fp_capture_encode_v1(&job, &[1, 2], &capture)).unwrap();
        let mut another = job.clone();
        another.job_nonce[0] ^= 1;
        assert!(retained_capture_v1(&path, &another, Hash64::from_u64_word(11), &ledger).is_err());
        assert_eq!(ledger.reserved_bytes(), 0);
        std::fs::write(&path, palw_fp_material_encode_v1(&job, &[1, 2])).unwrap();
        assert!(retained_capture_v1(&path, &job, Hash64::from_u64_word(11), &ledger).is_err());
    }

    #[test]
    fn tir_prefix_capture_reserves_from_original_length_before_reading_large_retained_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oversized.material");
        std::fs::write(&path, misaka_palw_sdk::lineages::tir::TirCaptureV1::MAGIC).unwrap();
        std::fs::OpenOptions::new().write(true).open(&path).unwrap().set_len(1u64 << 32).unwrap();
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(65_536), || None);
        assert!(retained_capture_v1(&path, &job(), Hash64::from_u64_word(12), &ledger).is_err());
        assert_eq!(ledger.reserved_bytes(), 0, "a refused read leaves no source memory or decoded capture held");
    }
}

#[derive(Default)]
pub(super) struct TirPrefixBookV1 {
    after: Option<(u64, Hash64, PalwBondKeyV2)>,
}

impl PalwPanelService {
    pub(super) async fn public_tir_prefix_answer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        slot: super::PalwAnswerSlotV1<'_>,
        book: &mut TirPrefixBookV1,
        court_outbox: &mut CourtOutboxV1,
    ) -> Result<(), String> {
        if self.consensus_config.params.palw_staged_public_court_v1.is_none()
            || self.consensus_config.params.palw_staged_tir_route_v1_fence().is_none()
        {
            return Ok(());
        }
        let signer = slot.slot;
        let lane_round = ROUND - slot.round_offset;
        let _metadata = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-tir-prefix-duty-page",
                    class_id: Hash64::default(),
                    job: Hash64::default(),
                },
                1_048_576,
            )
            .map_err(|e| e.to_string())?;
        let (keys, next) = session.palw_staged_flat_duties_v1(signer, book.after)?;
        if keys.len() > 64 {
            return Err("TIR prefix duties exceed native bounded page".into());
        }
        book.after = next;
        // A queued leaf whose slot cursor moved on (or whose court closed) is stale.
        court_outbox.retain_pending(|(_, round, responder, object)| {
            if (*round, *responder) != (lane_round, true) {
                return true;
            }
            let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = object else { return false };
            let PalwConstraintStreamActionV1::NativeTirPrefixAnswer(answer) = &movement.action else { return false };
            if answer.answerer != signer {
                return true;
            }
            let Ok(Some(paid)) = session.palw_staged_paid_authority_v1((movement.claim, movement.reserver)) else { return true };
            let Some(bytes) = paid.terms.plan.resident_ram_bytes.checked_mul(2) else { return true };
            let Ok(_snapshot) = crate::palw_memory_ledger::host_ledger_v1().reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-tir-prefix-queued-snapshot",
                    class_id: Hash64::default(),
                    job: movement.claim,
                },
                bytes,
            ) else {
                return true;
            };
            match session.palw_staged_tir_prefix_v1((movement.claim, movement.reserver)) {
                Err(_) => true,
                Ok(None) => false,
                Ok(Some(p)) => {
                    p.row.seed_id() == answer.seed_id
                        && p.row.has_response_duty_v1(&signer)
                        && p.now_daa < p.row.respond_until_daa
                        && p.row.slot_ordinal_v1(&signer) == Some(answer.ordinal)
                }
            }
        });
        for (claim, owner) in keys {
            if slot.only.is_some_and(|only| !only.contains(&claim)) {
                continue;
            }
            if court_outbox.pending().iter().filter(|(_, r, b, _)| (*r, *b) == (lane_round, true)).count() >= 64 {
                break;
            }
            let mut attempt = None;
            let result = async {
                let Some(paid) = session.palw_staged_paid_authority_v1((claim, owner))? else { return Ok(()) };
                let bytes = paid.terms.plan.resident_ram_bytes.checked_mul(2).ok_or("TIR prefix snapshot memory overflow")?;
                let snapshot = crate::palw_memory_ledger::host_ledger_v1()
                    .reserve(
                        crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                            role: "public-tir-prefix-snapshot",
                            class_id: Hash64::default(),
                            job: claim,
                        },
                        bytes,
                    )
                    .map_err(|e| e.to_string())?;
                // A Flat or Float32 bundle on the shared page is answered by its own responder.
                let Some(progress) = session.palw_staged_tir_prefix_v1((claim, owner))? else { return Ok(()) };
                let row = &progress.row;
                if !row.has_response_duty_v1(&signer) || progress.now_daa >= row.respond_until_daa {
                    return Ok(());
                }
                let ordinal = row.slot_ordinal_v1(&signer).ok_or("TIR prefix signer has no literal slot")?;
                let key = (row.seed_id(), lane_round, true);
                if court_outbox.pending().iter().any(|(a, b, c, _)| (*a, *b, *c) == key)
                    || court_outbox.submitted_at(&key).is_some_and(|at| progress.now_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
                {
                    return Ok(());
                }
                attempt = Some((key, progress.now_daa));
                let packet_memory = crate::palw_memory_ledger::host_ledger_v1()
                    .reserve(
                        crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                            role: "public-tir-prefix-carrier",
                            class_id: progress.header.job_context.shape_profile_id,
                            job: claim,
                        },
                        PALW_PREFIX_BATCH_CARRIER_BYTES_V1 as u64 * 8 + 65_536,
                    )
                    .map_err(|e| e.to_string())?;
                let class = progress.header.job_context.shape_profile_id;
                let maximum = self.class_step_ladder(class);
                let leaf_index = session
                    .palw_staged_tir_prefix_leaf_index_v1((claim, owner), ordinal, maximum)?
                    .ok_or("TIR prefix slot names no boundary leaf")?;
                let facts = session.palw_producer_facts_v2(class, None).ok_or("TIR prefix class artifact unavailable")?;
                let need = self
                    .backends()
                    .role_memory_need_or_chain_v1(class, facts.artifact_root, FULL_SEAT_V1, |id| self.chain_carriage_v1(session, id))
                    .ok_or("TIR prefix source resource profile unavailable")?;
                let memory = self.reserve_replay_v1("public-tir-prefix-answer", &need, class, claim)?;
                let commitment =
                    session.palw_staged_original_commitment_v1(claim)?.ok_or("TIR prefix original public job unavailable")?;
                if kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&commitment) != claim
                    || kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(&commitment)
                        != kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&progress.header.job_context)
                {
                    return Err("TIR prefix response original public job changed".into());
                }
                let backend: Arc<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1> =
                    if row.producer == signer && slot.only.is_none() {
                        self.executor_backend_v1(session, class, facts.artifact_root)?
                    } else {
                        Arc::from(self.resolve_backend(session, class, facts.artifact_root)?)
                    };
                let paths = self.fp_retained_payload_paths(&claim);
                let header = progress.header.clone();
                let leaf = tokio::task::spawn_blocking(move || {
                    let _memory = memory;
                    for path in paths {
                        let Ok(material) =
                            retained_capture_v1(&path, &commitment.job, claim, &crate::palw_memory_ledger::host_ledger_v1())
                        else {
                            continue;
                        };
                        if let Ok(leaf) = backend.disclose_tir_step_leaf_v1(&material.bytes, &header, leaf_index, maximum) {
                            return Ok(leaf);
                        }
                    }
                    Err::<_, String>("TIR prefix responder has no retained TIR capture for this claim".into())
                })
                .await
                .map_err(|e| format!("TIR prefix responder worker failed: {e}"))??;
                let answer = PalwTirPrefixLeafAnswerV1 {
                    answerer: signer,
                    seed_id: row.seed_id(),
                    ordinal,
                    values_le: leaf.0,
                    opening: leaf.1,
                };
                answer.check_wire_v1()?;
                let mut movement = PalwConstraintStreamMoveV1 {
                    version: 1,
                    claim,
                    reserver: owner,
                    reserved_daa: row.reserved_daa,
                    seed_id: row.seed_id(),
                    phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                    action: PalwConstraintStreamActionV1::NativeTirPrefixAnswer(answer),
                    signature: Vec::new(),
                };
                // Last native read after disclosure, immediately before signing.
                // Existing snapshot/packet reservations remain live here.
                let Some(signature)=super::response_presign::sign_current_v1(
                    session.palw_staged_tir_prefix_v1((claim,owner)),
                    |fresh|super::response_presign::tir_current_v1(&progress,fresh,&movement,signer),
                    ||self.sign(&movement.signing_message(),PALW_CONSTRAINT_STREAM_CONTEXT_V1),
                )? else {return Ok(())};
                movement.signature=signature;
                movement.check()?;
                let deadline = row.respond_until_daa;
                let clock=progress.now_daa;
                let object=PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) };
                court_outbox.try_push_source_response(
                    key,&object,deadline.saturating_sub(1),
                    super::court_outbox::SourceResponseSnapshotV1::Tir(progress),snapshot,Some(packet_memory),
                ).map_err(|why|why.to_string())?;
                court_outbox.mark_submitted(key, clock);
                Ok::<(), String>(())
            }
            .await;
            if let Err(why) = result {
                if let Some((key, clock)) = attempt {
                    court_outbox.mark_submitted(key, clock);
                }
                debug!("[{PALW_PANEL}] claim {claim}: TIR prefix-state response remains pending: {why}");
            }
        }
        Ok(())
    }
}
