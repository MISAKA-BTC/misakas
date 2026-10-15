use super::*;
use kaspa_consensus_core::{
    palw_constraint_stream_carriage_v1::{
        PALW_CONSTRAINT_STREAM_CONTEXT_V1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1,
    },
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
    palw_selected_event_source_v1::{PalwSelectedEventAnswerV1, PalwSelectedEventBindingV1},
};

const ROUND: u32 = u32::MAX - 15;
impl PalwPanelService {
    pub(super) async fn public_selected_answer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        slot: super::PalwAnswerSlotV1<'_>,
        after: &mut Option<(u64, Hash64, PalwBondKeyV2)>,
        pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        due: &mut HashMap<(Hash64, u32, bool), u64>,
        moved: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(), String> {
        if self.consensus_config.params.palw_staged_public_court_v1.is_none() {
            return Ok(());
        }
        let signer = slot.slot;
        let lane_round = ROUND - slot.round_offset;
        let _metadata = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-selected-duty-page",
                    class_id: Hash64::default(),
                    job: Hash64::default(),
                },
                1_048_576,
            )
            .map_err(|e| e.to_string())?;
        let (keys, next) = session.palw_staged_selected_duties_v1(signer, *after)?;
        if keys.len() > 64 {
            return Err("selected duties exceed native bounded page".into());
        }
        *after = next;
        // Direct accepted reads keep another page's pending source alive while refusing
        // stale fork identities and already fulfilled duties before spending another fee.
        pending.retain(|(_, round, responder, object)| {
            if (*round, *responder) != (lane_round, true) {
                return true;
            }
            let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = object else { return false };
            let PalwConstraintStreamActionV1::SelectedEventAnswer(answer) = &movement.action else { return false };
            if answer.answerer != signer {
                // Another literal slot's packet (a delegated producer slot shares the lane).
                return true;
            }
            let paid = match session.palw_staged_paid_authority_v1((movement.claim, movement.reserver)) {
                Err(_) => return true,
                Ok(None) => return false,
                Ok(Some(paid)) => paid,
            };
            let Some(bytes) = paid.terms.plan.resident_ram_bytes.checked_mul(2) else { return true };
            let Ok(_read) = crate::palw_memory_ledger::host_ledger_v1().reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-selected-queued-snapshot",
                    class_id: Hash64::default(),
                    job: movement.claim,
                },
                bytes,
            ) else {
                return true;
            };
            match session.palw_staged_selected_events_v1((movement.claim, movement.reserver)) {
                Err(_) => true,
                Ok(None) => false,
                Ok(Some(p)) => {
                    p.now_daa < p.session.respond_until_daa
                        && (p.session.claim, p.session.owner, p.session.reserved_daa, p.session.bundle_id)
                            == (movement.claim, movement.reserver, movement.reserved_daa, movement.seed_id)
                        && answer.answerer == signer
                        && p.session.custodians.contains(&signer)
                        && p.session
                            .requests
                            .get(&answer.coordinate)
                            .is_some_and(|r| r.demand_id == answer.demand_id && !r.collected && !r.answered.contains(&signer))
                }
            }
        });
        for (claim, owner) in keys {
            if slot.only.is_some_and(|only| !only.contains(&claim)) {
                continue;
            }
            if pending.iter().filter(|(_, r, b, _)| (*r, *b) == (lane_round, true)).count() >= 64 {
                break;
            }
            let mut attempt = None;
            let result = async {
                let paid = session.palw_staged_paid_authority_v1((claim, owner))?.ok_or("selected response lost prepaid owner")?;
                let bytes = paid.terms.plan.resident_ram_bytes.checked_mul(2).ok_or("selected response read memory overflow")?;
                let read = crate::palw_memory_ledger::host_ledger_v1()
                    .reserve(
                        crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                            role: "public-selected-response-context",
                            class_id: Hash64::default(),
                            job: claim,
                        },
                        bytes,
                    )
                    .map_err(|e| e.to_string())?;
                let progress =
                    session.palw_staged_selected_events_v1((claim, owner))?.ok_or("selected duty lost accepted snapshot")?;
                let Some(request) = progress.session.requests.values().find(|r| !r.collected && !r.answered.contains(&signer)) else {
                    return Ok(());
                };
                if !progress.session.custodians.contains(&signer) || progress.now_daa >= progress.session.respond_until_daa {
                    return Ok(());
                }
                let key = (request.demand_id, lane_round, true);
                if pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == key)
                    || moved.get(&key).is_some_and(|at| progress.now_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
                {
                    return Ok(());
                }
                attempt = Some((key, progress.now_daa));
                let context = progress.context.as_ref().ok_or("selected duty lacks accepted original binding/job")?;
                let class = context.job.class_id;
                let facts =
                    session.palw_producer_facts_v2(class, None).ok_or("selected response has no original registered artifact")?;
                let need = self
                    .backends()
                    .role_memory_need_or_chain_v1(class, facts.artifact_root, FULL_SEAT_V1, |id| self.chain_carriage_v1(session, id))
                    .ok_or("selected responder resource profile unavailable")?;
                let execution = self.reserve_replay_v1("public-selected-answer", &need, class, claim)?;
                let backend: Arc<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1> =
                    if PalwBondKeyV2(context.job.executor_bond) == signer && slot.only.is_none() {
                        self.executor_backend_v1(session, class, facts.artifact_root)?
                    } else {
                        Arc::from(self.resolve_backend(session, class, facts.artifact_root)?)
                    };
                let (_, _, work_leaves) = session.palw_claim_roots_v2(claim).ok_or("selected response claim roots unavailable")?;
                let claim_facts = PalwDaClaimFactsV1 {
                    claim_id: claim,
                    class_id: class,
                    executor_bond: PalwBondKeyV2(context.job.executor_bond),
                    execution_root: context.binding.committed_execution_root,
                    trace_root: context.binding.full_logits_trace_root,
                    work_leaves,
                    form: self.class_prompt_ids_form(class),
                    lane: PalwDaLaneV1::FreePrompt { panel_da_admissible: self.consensus_config.params.palw_panel_da_admissible() },
                    job_pin: Some(kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(
                        &context.binding.job_context,
                    )),
                };
                let coordinate = request.coordinate;
                let demand_id = request.demand_id;
                let original_binding = context.binding.clone();
                let paths = self.fp_retained_payload_paths(&claim);
                let foreign = self.config.retention_dir.join("foreign");
                let maximum = self.class_step_ladder(class);
                let keypair = self.keypair.clone();
                let clock = progress.now_daa;
                let deadline = progress.session.respond_until_daa;
                let reserved = progress.session.reserved_daa;
                let bundle = progress.session.bundle_id;
                self.foreign_pinned.lock().unwrap().insert(claim);
                let object = tokio::task::spawn_blocking(move || -> Result<PalwConsensusObjectV2, String> {
                    let _read = read;
                    let _execution = execution;
                    let (material, _) = palw_da_material_v1(
                        backend.as_ref(),
                        &claim_facts,
                        paths.into_iter().filter_map(|p| std::fs::read(p).ok()),
                        |bytes| palw_da_keep_remade_v1(&foreign, &claim, bytes),
                    )?;
                    let capture = match &material {
                        PalwDaCaptureV1::FreePrompt(payload) => &payload.capture,
                        PalwDaCaptureV1::Attempt(capture) => capture,
                    };
                    let (binding, event) = backend.disclose_selected_event_v1(capture, coordinate.row, coordinate.tile, maximum)?;
                    if binding != *original_binding {
                        return Err("selected responder capture changes original accepted binding".into());
                    }
                    let answer = PalwSelectedEventAnswerV1 {
                        answerer: signer,
                        demand_id,
                        coordinate,
                        binding: PalwSelectedEventBindingV1::from_binding_v1(&binding)?,
                        event,
                    };
                    answer.check_v1()?;
                    let mut movement = PalwConstraintStreamMoveV1 {
                        version: 1,
                        claim,
                        reserver: owner,
                        reserved_daa: reserved,
                        seed_id: bundle,
                        phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                        action: PalwConstraintStreamActionV1::SelectedEventAnswer(answer),
                        signature: Vec::new(),
                    };
                    movement.signature = keypair
                        .as_ref()
                        .and_then(|kp| {
                            libcrux_ml_dsa::ml_dsa_87::sign(
                                &kp.signing_key,
                                &movement.signing_message(),
                                PALW_CONSTRAINT_STREAM_CONTEXT_V1,
                                [0u8; 32],
                            )
                            .ok()
                            .map(|s| s.as_ref().to_vec())
                        })
                        .ok_or("selected responder signing key unavailable")?;
                    movement.check()?;
                    Ok(PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) })
                })
                .await
                .map_err(|e| format!("selected response worker failed: {e}"))??;
                pending.push((key.0, key.1, key.2, object));
                due.insert(key, deadline.saturating_sub(1));
                moved.insert(key, clock);
                Ok::<(), String>(())
            }
            .await;
            if let Err(why) = result {
                if let Some((key, clock)) = attempt {
                    moved.insert(key, clock);
                }
                debug!("[{PALW_PANEL}] claim {claim}: original selected response remains pending: {why}")
            }
        }
        Ok(())
    }
}
