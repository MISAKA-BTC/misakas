//! Ordered seat verification and publication. Read-only duty/chain inputs are
//! separated from mutable replay, service, receipt and filing books. The old
//! per-duty control flow, ledger ownership and task creation order are retained.
use super::*;

pub(super) struct SeatProgressReadV1<'a> {
    pub(super) receipt_facts: &'a crate::palw_receipt_pool::ReceiptChainFactsV1,
    pub(super) vertex_state_path: &'a PathBuf,
    pub(super) mesh_node: &'a mesh::PalwMeshNodeV1,
    pub(super) trap_book: &'a mesh::PalwTrapBookV1,
    pub(super) duties: &'a [kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2],
    pub(super) seat_r_duties: &'a [PalwSeatRDutyV1],
    pub(super) seat_r: bool,
    pub(super) vertex_fence: Option<u64>,
}

pub(super) struct SeatProgressRefsV1<'a> {
    pub(super) pool_books: &'a mut PoolBooksV1,
    pub(super) receipt_pool_v2: &'a mut crate::palw_receipt_pool::PalwReceiptPoolV1<PalwSeatReceiptV2>,
    pub(super) receipt_pool_v3: &'a mut crate::palw_receipt_pool::PalwReceiptPoolV1<PalwSeatReceiptV3>,
    pub(super) shard_shadow: &'a mut tir_shard::PalwTirShardShadowV1,
    pub(super) own_filed: &'a mut HashMap<SeatDutyPanelKeyV1, crate::palw_receipt_pool::OwnFiledV1>,
    pub(super) own_filed_whole: &'a mut HashMap<SeatDutyPanelKeyV1, crate::palw_receipt_pool::OwnFiledV1>,
    pub(super) answered: &'a mut HashSet<SeatDutyPanelKeyV1>,
    pub(super) first_seen: &'a mut HashMap<Hash64, u64>,
    pub(super) replayed: &'a mut HashSet<SeatDutyPanelKeyV1>,
    pub(super) seat_replays: &'a mut PalwSeatReplaysV1,
    pub(super) receipts_filed_at: &'a mut std::collections::VecDeque<std::time::Instant>,
    pub(super) seat_resumes: &'a mut PalwSeatResumesV1,
    pub(super) service: &'a mut PalwSeatServiceV1,
    pub(super) replay_refuted: &'a mut HashSet<Hash64>,
    pub(super) evidence_kept: &'a mut HashSet<Hash64>,
    pub(super) capture_sampled: &'a mut HashSet<Hash64>,
    pub(super) s3_samplings: &'a mut HashMap<SeatDutyPanelKeyV1, tokio::task::JoinHandle<PalwSeatS3SampleV1>>,
    pub(super) s3_sampled: &'a mut HashSet<SeatDutyPanelKeyV1>,
    pub(super) requested: &'a mut HashMap<Hash64, u64>,
    pub(super) requested_intervals: &'a mut HashMap<(Hash64, u32), u64>,
    pub(super) own_receipts: &'a mut HashMap<Hash64, (PalwSeatReceiptV2, u64)>,
    pub(super) vertex_book: &'a mut vertex::PalwVertexBookV1,
    pub(super) vertex_equivocated: &'a mut bool,
    pub(super) court_outbox: &'a mut CourtOutboxV1,
    pub(super) reporter_filer: &'a mut reporter_filer::PalwReporterFilerV1,
    pub(super) accused: &'a mut HashSet<Hash64>,
    pub(super) own_receipt_class: &'a mut HashMap<Hash64, Hash64>,
    pub(super) accusations: &'a mut PalwSeatAccusationsV1,
    pub(super) false_valid: &'a mut crate::palw_filer_false_valid::PalwFalseValidFilerV1,
    pub(super) improve: &'a mut improve::PalwImproveLoopV1,
}
impl PalwPanelService {
    pub(super) async fn seat_progress_tick_v1(
        self: &Arc<Self>,
        tick: tick_inputs::PanelTickReadV1<'_>,
        order: Vec<usize>,
        read: SeatProgressReadV1<'_>,
        books: SeatProgressRefsV1<'_>,
    ) {
        let session = tick.session();
        let bond_key = tick.bond();
        let network_domain = tick.domain();
        let current_daa = tick.daa();
        let SeatProgressReadV1 { receipt_facts, vertex_state_path, mesh_node, trap_book, duties, seat_r_duties, seat_r, vertex_fence } =
            read;
        let SeatProgressRefsV1 {
            pool_books,
            receipt_pool_v2,
            receipt_pool_v3,
            shard_shadow,
            own_filed,
            own_filed_whole,
            answered,
            first_seen,
            replayed,
            mut seat_replays,
            receipts_filed_at,
            mut seat_resumes,
            service,
            replay_refuted,
            evidence_kept,
            capture_sampled,
            s3_samplings,
            s3_sampled,
            requested,
            mut requested_intervals,
            own_receipts,
            vertex_book,
            vertex_equivocated,
            mut court_outbox,
            mut reporter_filer,
            mut accused,
            own_receipt_class,
            accusations,
            false_valid,
            mut improve,
        } = books;

        for index in order {
            let (duty, seat_r_duty) = (&duties[index], seat_r_duties[index]);
            let deadline = seat_r_duty.deadline;
            if !seat_duty_is_due_until_v1(duty, &answered, current_daa, deadline) {
                continue;
            }
            first_seen.entry(duty.claim_id).or_insert(current_daa.max(duty.bound_daa));
            // **N-5 / P2-6: service is recorded, not only its first absence** (the P2-6 review,
            // HIGH). A free-prompt claim noted unserved is served the moment this seat holds a job
            // of its class and executor — the pool's or this node's retention, the tail's own
            // `holds_fp_job` — whatever arm runs this tick, or none (a replay deferred for memory
            // below): the arms note what they see besides (`PalwSeatServiceV1`).
            if seat_r && duty.free_prompt && service.is_unserved(&duty.claim_id) {
                let pooled = pool_books.material.map().get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]);
                if self.fp_job_material_for_claim(&duty.claim_id, duty.class_id, &duty.executor_bond, pooled).is_some() {
                    service.note_served(duty.claim_id);
                }
            }
            let mut segments_attested: Option<kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2> = None;
            // **ADR-0152 Q-7: `Sampled`, the answer of a partial seat whose replay is not coming**
            // (`palw_seat_sampled_due_v1`: past SEAT-R, R-core+ and S3, within the last sixty DAA of
            // the window). Before the memory check below, because the case it is for is the host that
            // fits a segment but not the whole replay. The sampling runs off the loop, once per duty,
            // on a capture SEAT-S3 verified first (`palw_seat_s3_sample_v1`); while it runs the duty
            // waits. It ends `Sampled` (filed like any V3 receipt, over the seat's assigned mask, and
            // carried by a licence set as a rider or by SR-10's door), `Faulted` (the court's: noted,
            // and this seat files nothing), or nothing at all. Never `Valid`.
            let duty_key = seat_duty_panel_key_v1(duty);
            let mut sampled_now = false;
            // DRILL ONLY (the G14 collusion drill): a seat colluding with this claim's producer signs Valid without judging it.
            let colluding = self.config.drill_collude_with.contains(&duty.executor_bond);
            if let Some(running) = s3_samplings.get(&duty_key) {
                if !running.is_finished() {
                    continue;
                }
                let finished = s3_samplings.remove(&duty_key).expect("the handle just read");
                s3_sampled.insert(duty_key);
                match finished.await {
                    Ok(PalwSeatS3SampleV1::Sampled) => sampled_now = true,
                    Ok(PalwSeatS3SampleV1::Faulted) => self.note_seat_fault_v1(duty.claim_id, 0, 0),
                    Ok(PalwSeatS3SampleV1::Unchecked) | Err(_) => {}
                }
            } else if !colluding
                && !s3_sampled.contains(&duty_key)
                && palw_seat_sampled_due_v1(
                    seat_r,
                    self.consensus_config.params.palw_rcore_plus_active_at(current_daa),
                    self.consensus_config.params.palw_verification_s3_at(current_daa),
                    seat_r_duty.role,
                    deadline,
                    current_daa,
                )
                && !seat_replays.runs_for_claim(&duty.claim_id)
                && !replay_refuted.contains(&duty.claim_id)
                && !self.seat_found_fault_v1(&duty.claim_id)
            {
                let pooled = pool_books.material.map().get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]);
                match self.seat_s3_sampling_start_v1(&session, duty, network_domain, pooled) {
                    Ok(Some(running)) => {
                        s3_samplings.insert(duty_key, running);
                        continue;
                    }
                    // No capture of the claim's to sample: nothing to try again.
                    Ok(None) => {
                        s3_sampled.insert(duty_key);
                    }
                    // The ledger refused: a later tick inside the lead may still fit.
                    Err(()) => {}
                }
            }
            // **Host memory, node-local and never consensus**: a replay that would push this host
            // past its usable memory waits for a later tick instead of starting — swap is not
            // capacity, and a host in swap finishes no replay at all. The deadline still runs;
            // a seat that never fits answers nothing, which the quorum prices as silence. A SEAT-R
            // replay that stands holds its own reservation, so polling it needs no second check;
            // and a SEAT-S4 resume reserves its own figure a segment at a time — the partial seat's
            // streamed fold, never this check's whole-job full seat, which a host that fits the
            // segment but not the whole replay could never pass (the second pass, finding 4).
            if !(seat_r && (seat_replays.holds_claim(&duty.claim_id) || seat_r_duty.role == PalwSeatRRoleV1::PartialResumes))
                && !sampled_now
                && !colluding
                && let Err(why) = self.replay_memory_budget_v1(&session, Some((duty.class_id, duty.artifact_root)))
            {
                crate::palw_backends::note_throttled_v1("panel-replay-budget", || {
                    format!("[{PALW_PANEL}] replay of claim {} deferred: {why}", duty.claim_id)
                });
                continue;
            }
            // **SEAT-R's guard at run time** (the SEAT-R review, LOW): set by the two off-loop
            // replays' `Licensed` arms and nowhere else, and read before anything is signed — past
            // SEAT-R a `Valid` without it files nothing, in a release build as in a test.
            let mut licensed_by_replay = false;
            // Its SEAT-S4 twin: set by the verified resume's `Licensed` arms alone, for a
            // `PartialResumes` seat's own-mask V3 (`PalwSeatArmV1::VerifiedSegmentResume`).
            let mut licensed_by_verified_resume = false;
            // A replay of the claim's own job that did not reproduce it: no replay runs again,
            // nothing licenses it here, and the full seat's fault finders run on.
            let refuted = seat_r && replay_refuted.contains(&duty.claim_id);
            let verdict = 'verdict: {
                // Q-7: a finished S3 sampling is this duty's answer (above).
                if sampled_now {
                    break 'verdict Some(PalwReceiptVerdictV2::Sampled);
                }
                // DRILL ONLY (the G14 collusion drill, `--palw-drill-collude-with`): every seat of this Panel is the producer's, so
                // the claim is signed Valid as a reproduced replay would sign it — the whole job (V2) and the seat's own mask (V3) —
                // and nothing about it is judged or filed. What the drill then shows is that one bond OUTSIDE the Panel still
                // convicts it from public material, and that each of these Valid signatures is charged.
                if colluding {
                    // As the seat's own role would license it: a full seat by its replay, a partial seat (SEAT-S4) by a verified
                    // resume over exactly its assigned mask.
                    if seat_r_duty.role == PalwSeatRRoleV1::PartialResumes {
                        licensed_by_verified_resume = true;
                        segments_attested = Some(
                            kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2(
                                duty.panel_anchor,
                                duty.claim_id,
                                duty.panel_seat_count.max(1),
                            )
                            .mask_of(duty.seat_index as u16),
                        );
                    } else {
                        licensed_by_replay = true;
                    }
                    info!(
                        "[{PALW_PANEL}] PALW DRILL (G14 collusion): claim {} of colluding producer {}:{} — signed Valid without judging it",
                        duty.claim_id, duty.executor_bond.0.transaction_id, duty.executor_bond.0.index
                    );
                    break 'verdict Some(PalwReceiptVerdictV2::Valid);
                }
                // **Class capability is decided BEFORE looking at deliveries.** This resolve
                // lived inside the per-material loop, so a seat that received NOTHING never
                // reached its own `Incapable` answer and fell through to `Unavailable` — a
                // signed accusation of withholding from a seat that could not have judged the
                // data had it arrived. Zero deliveries and zero capability must answer the
                // same thing.
                if self.resolve_backend(&session, duty.class_id, duty.artifact_root).is_err() {
                    break 'verdict Some(PalwReceiptVerdictV2::Incapable);
                }
                // **RFC-0004 (A10): an evaluation claim** — a free-prompt claim with no quanta — is replayed
                // through the pipeline executor from the chain's own state (its job, the item's prompt
                // and reference, the policy's parameters), never from material an executor serves: it has
                // none to withhold, so no arm below may file `Unavailable` against it. A matching replay
                // licenses `Valid`; a difference is the court's question and files nothing.
                match self.improve_seat_step_v1(&mut improve, duty).await {
                    improve::PalwImproveSeatStepV1::NotEvaluation => {}
                    improve::PalwImproveSeatStepV1::Valid => {
                        debug_assert!(palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::EvaluationReplay));
                        licensed_by_replay = true;
                        break 'verdict Some(PalwReceiptVerdictV2::Valid);
                    }
                    improve::PalwImproveSeatStepV1::Waiting
                    | improve::PalwImproveSeatStepV1::Differs
                    | improve::PalwImproveSeatStepV1::Unjudgeable => break 'verdict None,
                }
                // **ADR-0098 Decision 2: a seat that found a fault in this claim files NOTHING
                // about it** — in this round or any later one. Not a `Valid` from an arm whose
                // samples missed the lie, and not the half-window `Unavailable`, which accuses a
                // producer that served the data (and, without ADR-0065 D4, is charged when the
                // rest of the panel licenses). Silence is never charged; the fault goes to the
                // court through the challenger's half, and nothing here replays the claim again.
                if self.seat_found_fault_v1(&duty.claim_id) {
                    // A fault is found in material this seat held: it was served (N-5).
                    service.note_served(duty.claim_id);
                    // **A fault still addressed at a BLOCK is not finished with** (ADR-0086
                    // Decision 6, found by ADR-0111's drill): only the interval arm below names
                    // the leaf from the block's served leaves, and this gate used to stop it for
                    // good the moment the block was noted — the leaves arrived and were never
                    // read, so no leaf was ever named. A free-prompt claim whose fault is a block
                    // runs the interval arm again, which files nothing while the fault stands
                    // (`FaultInRange` answers `None`), and the gate after it stops the capture arm.
                    let fault_is_a_block = duty.free_prompt
                        && self.seat_faults.lock().unwrap().address(&duty.claim_id).is_some_and(|(_, count)| count > 1);
                    if !fault_is_a_block {
                        // **ADR-0111 Decision 6: a NAMED leaf this seat could not accuse from a
                        // capture is pursued, not dropped** — the executor's evidence if it has
                        // answered, the fast-path ask if not yet, the demand on chain past the
                        // patience. What files is the court's object, never a receipt.
                        if duty.free_prompt
                            && let Some((key, object, accusation)) = self
                                .pursue_named_leaf_v1(
                                    duty,
                                    network_domain,
                                    current_daa,
                                    pool_books.intervals.map(),
                                    accused.contains(&duty.claim_id),
                                )
                                .await
                        {
                            if accusation {
                                accused.insert(duty.claim_id);
                            }
                            if !court_outbox.pending().iter().any(|(sid, _, _, _)| *sid == key) {
                                // Dated (the second review's MEDIUM): an undated item waits behind
                                // every dated one, and a lie it would have convicted reaches `Final`.
                                // Before the claim's earliest `Final` too (the fourth review's MEDIUM).
                                let earliest_final = palw_seat_claim_earliest_final_v1(&self.consensus_config.params, duty.bound_daa);
                                court_outbox
                                    .set_due((key, 0, false), palw_seat_court_filing_due_v1(deadline, earliest_final, current_daa));
                                court_outbox.push_pending((key, 0, false, object));
                            }
                        }
                        break 'verdict None;
                    }
                }
                // **SEAT-R: a refuting replay is terminal for `Valid`, not for the fault finders**
                // (the SEAT-R review, MEDIUM). Making it terminal for everything silenced the one
                // seat that had caught the claim: no interval arm, no capture sampler, no
                // accusation — while three colluding seats could still license it through the
                // V1 door. So a partial seat stops here (its replay is the licence's fallback, not
                // a second fault finder), and the full seat goes on to the arms below, none of
                // which signs `Valid` past the fence; the tail's evidence gate still keeps it from
                // accusing a producer whose material it verified of withholding it.
                if refuted && seat_r_duty.role != PalwSeatRRoleV1::FullSeat {
                    break 'verdict None;
                }
                // **The free-prompt lane: the seat REPLAYS the job** (FP-R6). An attempt
                // claim's job is derived from its anchor, so the arm below re-hashes the
                // material under that derivation — which for a free-prompt claim derives a
                // job the caller never asked, mismatches every honest material, and files
                // `Unavailable` into a quorum that DEFAULTS the producer. This lane's
                // material is the job itself ([`palw_fp_material_decode_v1`]), and the only
                // verifier that checks what the claim asserts is a fresh execution: every
                // root the claim committed, recomputed on this seat's own artifact.
                //
                // The execution is minutes, not milliseconds, and the pool admits at most
                // four payloads per claim — so a forged payload costs this seat bounded
                // compute and an honest one costs the same run the executor already priced.
                // The cheap fields are pre-checked not as security (an attacker copies
                // them off the duty) but so the common garbage fails before the expensive
                // step.
                if duty.free_prompt {
                    // **RFC-0003 (carriage node half): a TENSOR claim is judged by its own seat pass.** A claim of
                    // an image or an embedding class this node holds is a `PalwGenJobV1` on the free-prompt lane,
                    // its material the producer's capture (`FPG1`); the seat replays the whole job over its held
                    // class (no partial seat, no interval seat, no capture sampler — a tensor claim has none) and
                    // files `Valid` when the replay reproduces the claim's roots and canonical output. A class this
                    // node does not hold answered `Incapable` above.
                    let tensor_class = self.backends().resolve_gen_v1(duty.class_id, duty.artifact_root);
                    if let Some(Ok(tensor)) = tensor_class
                        && gen_court::gen_class_is_tensor_v1(&tensor)
                    {
                        if !(seat_r && seat_r_duty.role == PalwSeatRRoleV1::FullSeat) || refuted {
                            break 'verdict None;
                        }
                        let pooled = pool_books.material.map().get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]);
                        let pass = self
                            .gen_tensor_seat_pass_v1(
                                &session,
                                std::sync::Arc::new(tensor),
                                duty,
                                seat_r_duty,
                                current_daa,
                                pooled,
                                &mut seat_replays,
                            )
                            .await;
                        if pass.served {
                            service.note_served(duty.claim_id);
                        }
                        match pass.step {
                            PalwSeatReplayStepV1::Licensed => {
                                // X7: past the fence, a capture this seat could not keep is a Valid it cannot answer for.
                                if !self.licensed_job_kept_v1(&duty.claim_id, pass.kept, current_daa) {
                                    break 'verdict None;
                                }
                                debug_assert!(palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::FreePromptReplay));
                                licensed_by_replay = true;
                                break 'verdict Some(PalwReceiptVerdictV2::Valid);
                            }
                            // Terminal for `Valid`; the court's pass accuses (`gen_one_move_pass_v1`).
                            PalwSeatReplayStepV1::Refuted => {
                                replay_refuted.insert(duty.claim_id);
                                break 'verdict None;
                            }
                            PalwSeatReplayStepV1::Waiting => break 'verdict None,
                            // Nothing replays and nothing is refuted: ask for the claim's capture at the tail's
                            // pace, and — served none — file `Unavailable` at the material wait (N-5).
                            PalwSeatReplayStepV1::NoVerdict => {
                                let reask_daa = seat_reask_daa_v1(deadline.saturating_sub(duty.bound_daa));
                                if requested.get(&duty.claim_id).is_none_or(|at| current_daa >= at.saturating_add(reask_daa)) {
                                    requested.insert(duty.claim_id, current_daa);
                                    self.request_material_signed(network_domain, duty.claim_id, current_daa).await;
                                }
                                if !pass.served {
                                    service.note_unserved(duty.claim_id, current_daa);
                                    if palw_seat_material_wait_ends_v1(seat_r, duty.bound_daa, deadline, current_daa, || {
                                        Self::palw_licence_stands_v1(
                                            &session,
                                            duty.claim_id,
                                            &receipt_pool_v2,
                                            &receipt_pool_v3,
                                            &receipt_facts,
                                        )
                                    }) {
                                        break 'verdict Some(PalwReceiptVerdictV2::Unavailable {
                                            chunk_index: 0,
                                            requested_daa: first_seen[&duty.claim_id].max(duty.bound_daa),
                                        });
                                    }
                                }
                                break 'verdict None;
                            }
                        }
                    }
                    // **SEAT-S4 past SEAT-R: a C7 partial seat resumes its own mask** (the review's
                    // re-enable, `PalwSeatRRoleV1::PartialResumes`), off the loop, from served
                    // openings authenticated against the claim — under the job and context the
                    // claim's answer authenticates (L2, finding 1), never the job's ceiling nor a
                    // held job nothing bound to the claim. Holding no answer yet it asks for the
                    // claim's material (the answer envelope carries the ids); holding nothing to
                    // try, it waits out the material wait (N-5).
                    if seat_r && seat_r_duty.role == PalwSeatRRoleV1::PartialResumes {
                        let held = pool_books.material.map().get(&duty.claim_id).map(|v| v.to_vec()).unwrap_or_default();
                        let job = self.resolve_backend_for_claim_v1(&session, duty.class_id, duty.artifact_root, Some(duty.claim_id)).ok().and_then(|resolved| {
                            let (material, ctx) = self.fp_seat_job_v1(resolved.as_ref(), duty, &held)?;
                            let form = self.class_prompt_ids_form(material.job.class_id);
                            let prompt = Self::fp_prompt_for_job(
                                kaspa_consensus_core::palw_backend::PalwExecutionOpsV1::new(resolved.as_ref()),
                                &material,
                                form,
                            )?;
                            Some((ctx, prompt.iter().map(|t| *t as usize).collect::<Vec<usize>>()))
                        });
                        // N-5 / P2-6: the step's word on service is noted as it is read
                        // (`PalwSeatServiceV1::resumed`) — the `Unavailable` below still files at the
                        // wait, and accuses only for a seat that holds nothing (the P2-6 review, HIGH
                        // and LOW).
                        let holds_job = job.is_some();
                        let step = match job {
                            Some((ctx, prompt)) => {
                                self.seat_s4_resume_v1(
                                    &session,
                                    duty,
                                    seat_r_duty,
                                    current_daa,
                                    network_domain,
                                    &ctx,
                                    &prompt,
                                    pool_books.intervals.resume_mut(),
                                    &mut requested_intervals,
                                    &mut seat_replays,
                                    &mut seat_resumes,
                                )
                                .await
                            }
                            // No job the answer authenticated, so no context to authenticate an
                            // opening under (finding 1). Served a job of the claim's class and executor
                            // — the claim's or a stranger's, which only the answer tells apart — the
                            // seat accuses nobody until one is authenticated; served none, it waits out
                            // the material wait (N-5).
                            None => {
                                let reask_daa = seat_reask_daa_v1(deadline.saturating_sub(duty.bound_daa));
                                if requested.get(&duty.claim_id).is_none_or(|at| current_daa >= at.saturating_add(reask_daa)) {
                                    requested.insert(duty.claim_id, current_daa);
                                    self.request_material_signed(network_domain, duty.claim_id, current_daa).await;
                                }
                                let served = self
                                    .fp_job_material_for_claim(&duty.claim_id, duty.class_id, &duty.executor_bond, &held)
                                    .is_some();
                                PalwSeatResumeStepV1::Starved { missing: Vec::new(), served }
                            }
                        };
                        match service.resumed(duty.claim_id, current_daa, holds_job, step) {
                            PalwSeatResumeStepV1::Licensed(mask) => {
                                debug_assert!(palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::VerifiedSegmentResume));
                                segments_attested = Some(mask);
                                licensed_by_verified_resume = true;
                                break 'verdict Some(PalwReceiptVerdictV2::Valid);
                            }
                            PalwSeatResumeStepV1::Waiting => break 'verdict None,
                            // Served, and nothing left to try: silent, never an accusation (N-5).
                            PalwSeatResumeStepV1::Starved { served: true, .. } => break 'verdict None,
                            PalwSeatResumeStepV1::Starved { served: false, .. } => {
                                if palw_seat_material_wait_ends_v1(seat_r, duty.bound_daa, deadline, current_daa, || {
                                    Self::palw_licence_stands_v1(
                                        &session,
                                        duty.claim_id,
                                        &receipt_pool_v2,
                                        &receipt_pool_v3,
                                        &receipt_facts,
                                    )
                                }) {
                                    break 'verdict Some(PalwReceiptVerdictV2::Unavailable {
                                        chunk_index: 0,
                                        requested_daa: first_seen[&duty.claim_id].max(duty.bound_daa),
                                    });
                                }
                                break 'verdict None;
                            }
                        }
                    }
                    // **SEAT-R: a seat licenses by its own replay of the claim's job, run first and
                    // off the loop** (`fp_seat_replay_pass_v1`) — the full seat, and a replaying
                    // partial seat (`PalwSeatRRoleV1`). While it runs the seat files nothing;
                    // when no replay licenses, the full seat's arms below still run — the interval
                    // seat and the capture sampler find faults and accuse — and none of them signs
                    // `Valid`. A partial seat's path ends at its resume's `Abstain`.
                    if seat_r && matches!(seat_r_duty.role, PalwSeatRRoleV1::FullSeat | PalwSeatRRoleV1::PartialReplays) && !refuted {
                        let pooled = pool_books.material.map().get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]);
                        // A job of the claim's replays, ran, or refuted it: served (N-5, the P2-6
                        // review's HIGH); `NoVerdict` is read below.
                        let pass =
                            self.fp_seat_replay_pass_v1(&session, duty, seat_r_duty, current_daa, pooled, &mut seat_replays).await;
                        if palw_seat_replay_served_v1(&pass.0) {
                            service.note_served(duty.claim_id);
                        }
                        match pass {
                            (PalwSeatReplayStepV1::Licensed, bytes) => {
                                // X7: past the fence, a job this seat could not keep is a Valid it cannot answer for.
                                if !self.licensed_job_kept_v1(&duty.claim_id, bytes, current_daa) {
                                    break 'verdict None;
                                }
                                debug_assert!(palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::FreePromptReplay));
                                licensed_by_replay = true;
                                break 'verdict Some(PalwReceiptVerdictV2::Valid);
                            }
                            // Terminal for `Valid`; the full seat's fault finders run on.
                            (PalwSeatReplayStepV1::Refuted, _) => {
                                replay_refuted.insert(duty.claim_id);
                                if seat_r_duty.role != PalwSeatRRoleV1::FullSeat {
                                    break 'verdict None;
                                }
                            }
                            (PalwSeatReplayStepV1::Waiting, _) => break 'verdict None,
                            // A partial seat that replays is the licence's fallback, not a fault
                            // finder: holding no job that replays, it asks for the claim's — at the
                            // tail's pace, since nothing it holds verified (audit M2-1) — and files
                            // nothing.
                            (PalwSeatReplayStepV1::NoVerdict, _) if seat_r_duty.role == PalwSeatRRoleV1::PartialReplays => {
                                let reask_daa = seat_reask_daa_v1(deadline.saturating_sub(duty.bound_daa));
                                if requested.get(&duty.claim_id).is_none_or(|at| current_daa >= at.saturating_add(reask_daa)) {
                                    requested.insert(duty.claim_id, current_daa);
                                    self.request_material_signed(network_domain, duty.claim_id, current_daa).await;
                                }
                                // N-5: served no job at all — nothing in the pool or this node's
                                // retention decodes as the claim's class and executor's job — it
                                // files its `Unavailable` at the material wait. One that holds a
                                // job it could not replay in time was served, and accuses nobody.
                                let pooled = pool_books.material.map().get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]);
                                if self.fp_job_material_for_claim(&duty.claim_id, duty.class_id, &duty.executor_bond, pooled).is_some()
                                {
                                    service.note_served(duty.claim_id);
                                } else {
                                    service.note_unserved(duty.claim_id, current_daa);
                                    if palw_seat_material_wait_ends_v1(seat_r, duty.bound_daa, deadline, current_daa, || {
                                        Self::palw_licence_stands_v1(
                                            &session,
                                            duty.claim_id,
                                            &receipt_pool_v2,
                                            &receipt_pool_v3,
                                            &receipt_facts,
                                        )
                                    }) {
                                        break 'verdict Some(PalwReceiptVerdictV2::Unavailable {
                                            chunk_index: 0,
                                            requested_daa: first_seen[&duty.claim_id].max(duty.bound_daa),
                                        });
                                    }
                                }
                                break 'verdict None;
                            }
                            (PalwSeatReplayStepV1::NoVerdict, _) => {}
                        }
                    }
                    // **Past Verification V2 a partial seat resumes assigned segments first**
                    // (ADR-0133 S1). The interval seat (ADR-0077 Decision 8, ADR-0082 Decision 9)
                    // stays the fallback for the full-replay seat and for a seat that cannot
                    // open a checkpoint; the whole-capture arms below remain what a seat does
                    // when the class, the executor or this build cannot do either.
                    let held = pool_books.material.map().get(&duty.claim_id).map(|v| v.to_vec()).unwrap_or_default();
                    if let Some(material) = self.fp_job_material_for_claim(&duty.claim_id, duty.class_id, &duty.executor_bond, &held)
                        && let Ok(resolved) = self.resolve_backend_for_claim_v1(&session, duty.class_id, duty.artifact_root, Some(duty.claim_id))
                        && let Some(prompt_ids) = Self::fp_prompt_for_job(
                            kaspa_consensus_core::palw_backend::PalwExecutionOpsV1::new(resolved.as_ref()),
                            &material,
                            self.class_prompt_ids_form(material.job.class_id),
                        )
                    {
                        let output_ids = self.fp_committed_output_ids_v1(resolved.as_ref(), duty, &held);
                        if self.consensus_config.params.palw_verification_v2_at(current_daa) {
                            let prompt: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
                            let ctx = output_ids
                                .as_ref()
                                .and_then(|ids| {
                                    resolved.fp_job_context_for_executed_v1(&material.job, ids.len().min(u32::MAX as usize) as u32)
                                })
                                .or_else(|| resolved.fp_job_context_v1(&material.job));
                            if let Some(ctx) = ctx {
                                match self
                                    .palw_v2_try_partial_resume_v1(
                                        duty,
                                        resolved.as_ref(),
                                        &ctx,
                                        &prompt,
                                        current_daa,
                                        network_domain,
                                        &held,
                                        pool_books.intervals.map(),
                                        &mut requested_intervals,
                                    )
                                    .await
                                {
                                    PalwV2SeatPathV1::Waiting => break 'verdict None,
                                    PalwV2SeatPathV1::Faulted => break 'verdict None,
                                    PalwV2SeatPathV1::Abstain => break 'verdict None,
                                    PalwV2SeatPathV1::Attested(mask) => {
                                        if !palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::SegmentResume) {
                                            break 'verdict None;
                                        }
                                        segments_attested = Some(mask);
                                        info!(
                                            "[{PALW_PANEL}] claim {}: licensed by V2 segment resume — mask {:#x} (ADR-0133 S1)",
                                            duty.claim_id, mask.0
                                        );
                                        break 'verdict Some(PalwReceiptVerdictV2::Valid);
                                    }
                                    PalwV2SeatPathV1::FullReplay => {}
                                }
                            }
                        }
                        if self.consensus_config.params.palw_verification_v2_at(current_daa) {
                            let assignment = kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2(
                                duty.panel_anchor,
                                duty.claim_id,
                                duty.panel_seat_count.max(1),
                            );
                            if duty.seat_index as u16 != assignment.full_seat {
                                break 'verdict None;
                            }
                        }
                        // **The seat's ONE context per claim, built from what RAN** (ADR-0084
                        // Decision 4, ADR-0074 Decision 7). The executed count is the answer's
                        // length, and the line above has already made the chain's
                        // `output_root` agree with it — so this is the context the claim was
                        // committed under, whether the run reached its ceiling or stopped
                        // early. Built at the CEILING it excluded every `EndOfGeneration`
                        // claim from this lane by making its surplus interval indices
                        // unopenable by construction.
                        // **RFC-0001 §A.3 step 7, before any interval is replayed**: a V4 answer
                        // must end exactly where its stop rule ends it (or run its whole budget)
                        // and commit no banned id. Pure in the job and the answer, which the chain
                        // already bound; an answer that fails it is not one this seat vouches for.
                        if let Some(output_ids) = output_ids.as_ref()
                            && let Some(decode) = material.job.decode.as_ref().filter(|_| material.job.decodes_under_v4_rules())
                            && let Err(why) = kaspa_consensus_core::palw_decode_pipeline_v4::decode_answer_stop_v4(
                                decode,
                                material.job.decode_token_limit,
                                u32::MAX,
                                output_ids,
                            )
                        {
                            warn!(
                                "[{PALW_PANEL}] claim {}: the V4 answer does not stop where its job's stop rule ends it ({why}) — \
                                 filing nothing",
                                duty.claim_id
                            );
                            break 'verdict None;
                        }
                        if let Some(output_ids) = output_ids
                            && let Some(ctx) = resolved
                                .fp_job_context_for_executed_v1(&material.job, output_ids.len().min(u32::MAX as usize) as u32)
                            && let Some(verdict) = self
                                .interval_seat_outcome_v1(
                                    &session,
                                    duty,
                                    network_domain,
                                    current_daa,
                                    &ctx,
                                    kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(&material.job),
                                    &prompt_ids,
                                    &output_ids,
                                    pool_books.intervals.map(),
                                    Some(&material.job),
                                )
                                .await
                            // SEAT-R: the sampled intervals find faults; they license nothing.
                            && (!matches!(verdict, PalwReceiptVerdictV2::Valid)
                                || palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::FreePromptInterval))
                        {
                            break 'verdict Some(verdict);
                        }
                    }
                    if self.consensus_config.params.palw_verification_v2_at(current_daa) {
                        let assignment = kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2(
                            duty.panel_anchor,
                            duty.claim_id,
                            duty.panel_seat_count.max(1),
                        );
                        if duty.seat_index as u16 != assignment.full_seat {
                            // Partial seats wait on SC01 resume. Genesis full replay is the
                            // designated full-replay seat's duty only.
                            break 'verdict None;
                        }
                    }
                    // ADR-0098 Decision 2, the same round: the interval arm may just have found
                    // one. The capture arm below would otherwise certify a claim this seat has
                    // just watched fail to replay.
                    if self.seat_found_fault_v1(&duty.claim_id) {
                        break 'verdict None;
                    }
                    // The same pooled payloads the interval path was offered, handed on rather
                    // than cloned a second time: a claim's pool is megabytes.
                    let pooled = held;
                    let disk = [
                        self.config.retention_dir.join(format!(
                            "{}{}",
                            duty.claim_id,
                            crate::palw_producer::PALW_RETAINED_MATERIAL_SUFFIX
                        )),
                        self.config.retention_dir.join("foreign").join(format!("{}.material", duty.claim_id)),
                    ]
                    .into_iter()
                    .filter_map(|path| std::fs::read(path).ok());
                    for bytes in pooled.into_iter().chain(disk) {
                        // **The answer beside the question** (ADR-0073 Decision 1a/1e). An
                        // `FPC1` payload carries the job, the user's prompt and the executor's
                        // family capture, and a seat CHECKS it — the roots under the job-id
                        // anchor, the shape the binding commits to (priced from the capture,
                        // not from what the commitment declared), and sampled leaves through
                        // the court's own adjudicator — and never re-runs it.
                        if let Some(payload) = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_decode_v1(
                            &bytes,
                            self.class_prompt_ids_form(duty.class_id),
                        ) {
                            let job = &payload.material.job;
                            if job.class_id != duty.class_id
                                || job.executor_bond != duty.executor_bond.0
                                || !self.fp_privacy_mode_judgeable(job.privacy_mode)
                            {
                                continue;
                            }
                            // **The ids, before anything is read out of the capture**
                            // (ADR-0077 Decision 16, W8). Under `PanelDa` the commitment
                            // carries none, so these ARE the question — and a seat that
                            // cannot bind them to the job has not seen the question and may
                            // not certify the answer. Named, never printed: the error says
                            // which of "nothing served" and "not this claim's" happened, and
                            // no id or text goes to the log (SA-5, ADR-0079 SA-7).
                            if let Err(why) = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_seat_prompt_admit_v1(
                                job,
                                Some(&payload.material.prompt_token_ids),
                                self.class_prompt_ids_form(duty.class_id),
                            ) {
                                warn!("[{PALW_PANEL}] claim {}: the served prompt is not this claim's ({why})", duty.claim_id);
                                continue;
                            }
                            let Ok(backend) = self.resolve_backend_for_claim_v1(&session, duty.class_id, duty.artifact_root, Some(duty.claim_id)) else {
                                break 'verdict Some(PalwReceiptVerdictV2::Incapable);
                            };
                            let roots = PalwClaimRootsV1 {
                                execution_root: duty.execution_root,
                                trace_root: duty.trace_root,
                                anchor: kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(job),
                                attempt_draw: None,
                                output_root: Some(duty.output_root),
                                // ADR-0152 v3.1 J-1 (the 3a review's L-b): the pin the claim recorded.
                                job_pin: duty.fp_job_pin_v1(),
                            };
                            // RFC-0001 §A: under the claim's decode rule (its job's, for a V4 claim).
                            let fp_verdict = fp_verify_capture_under_job_v1(
                                kaspa_consensus_core::palw_backend::PalwReplayOpsV1::new(backend.as_ref()),
                                &payload.material.job,
                                &payload.capture,
                                roots,
                            );
                            if fp_verdict != PalwMaterialVerdictV1::Matches {
                                // P2-8, J1 auto: the claim's committed execution under another job?
                                self.j1_auto_probe_v1(
                                    &session,
                                    &mut reporter_filer,
                                    duty,
                                    deadline,
                                    current_daa,
                                    bond_key,
                                    &network_domain,
                                    &payload.capture,
                                    roots,
                                )
                                .await;
                                continue;
                            }
                            let Some(shape) = backend.capture_shape(&payload.capture) else { continue };
                            // **Was this the work the claim was paid for?** The claim was
                            // priced from the leaf count its commitment declared; the capture
                            // the execution root binds has a leaf count of its own, and the
                            // two must be one number (ADR-0074 Decision 5).
                            if !kaspa_consensus_core::palw_backend::palw_opening_is_at_the_claims_price_v1(
                                shape.step_leaf_count,
                                duty.work_leaves,
                            ) {
                                warn!(
                                    "[{PALW_PANEL}] claim {}: the capture has {} leaves and the claim was priced at {} — the roots \
                                     match but the WORK does not, so this is not the claim's capture",
                                    duty.claim_id, shape.step_leaf_count, duty.work_leaves
                                );
                                continue;
                            }
                            // SEAT-R: the claim's own capture is held, so the tail below may not
                            // accuse its producer of withholding it; and a capture already sampled
                            // to an end is not sampled again on every tick the replay runs — past
                            // the fence the sampler finds faults, it licenses nothing.
                            if seat_r {
                                evidence_kept.insert(duty.claim_id);
                                if capture_sampled.contains(&duty.claim_id) {
                                    continue;
                                }
                            }
                            // The prompt the samples are opened with: the user's, or the
                            // network's own for a canonical claim (ADR-0074 Decision 1).
                            let Some(prompt_ids) = Self::fp_prompt_for_job(
                                kaspa_consensus_core::palw_backend::PalwExecutionOpsV1::new(backend.as_ref()),
                                &payload.material,
                                self.class_prompt_ids_form(payload.material.job.class_id),
                            ) else {
                                warn!(
                                    "[{PALW_PANEL}] claim {}: a canonical claim whose material is not its own prompt",
                                    duty.claim_id
                                );
                                continue;
                            };
                            // **Off the panel's loop, like every other forward pass on this
                            // lane.** Each of these draws re-executes the whole job on a fold
                            // class (`refutation_for_free_prompt_index` →
                            // `tiles_from_material_v1` → `dense_capture_from_fold_v1`, no
                            // cache), and running them inline blocked this node's every other
                            // duty for the duration. The FPM1 arm below has always used
                            // `offload`; this one did not.
                            let (capture_owned, prompt_owned, claim_owned) =
                                (payload.capture.clone(), prompt_ids.clone(), duty.claim_id);
                            let (leaves, artifact_root, ladder) =
                                (shape.step_leaf_count, duty.artifact_root, self.class_step_ladder(duty.class_id));
                            let prompt_ids_form = self.class_prompt_ids_form(duty.class_id);
                            // **The capture's bytes are RESERVED before a leaf is opened** (DoS
                            // audit 2026-09-24, #4). The sampler lays the capture out whole — a
                            // fold is re-executed into dense tiles — and it took no reservation:
                            // one held free-prompt claim made a full seat allocate 62 GiB beside
                            // whatever the producer and the court in this process held. Now the
                            // capture is first refused by name past the materialization cap (it
                            // is the streamed routes' to judge), then priced as a full seat whose
                            // capture is the DENSE one, then reserved; a refusal defers the duty
                            // and files nothing, as the full-seat replay's and the court's do.
                            let backends = self.backends();
                            let need = match backends.whole_capture_memory_need_v1(
                                backend.as_ref(),
                                duty.class_id,
                                duty.artifact_root,
                                &payload.capture,
                                ladder,
                                |id| self.chain_carriage_v1(&session, id),
                            ) {
                                Ok(need) => need,
                                Err(why) => {
                                    if self.first_sample_refusal_v1(duty.claim_id) {
                                        warn!(
                                            "[{PALW_PANEL}] claim {}: the served capture is not laid out whole on this seat ({why}) — \
                                             nothing filed from this arm",
                                            duty.claim_id
                                        );
                                    }
                                    continue;
                                }
                            };
                            // At the widest prefill run the ledger grants, on this duty's own instance
                            // — the re-execution that lays the capture out runs on it (int-10.2 A2).
                            // The refusal above does not move with the width, so a width it could
                            // refuse at falls back to the cap's figure.
                            let mut backend = backend;
                            let reserved = match self.reserve_replay_at_widest_run_v1(
                                "full-seat capture",
                                backend.as_mut(),
                                duty.class_id,
                                duty.claim_id,
                                |b| {
                                    backends
                                        .whole_capture_memory_need_v1(
                                            b,
                                            duty.class_id,
                                            duty.artifact_root,
                                            &payload.capture,
                                            ladder,
                                            |id| self.chain_carriage_v1(&session, id),
                                        )
                                        .unwrap_or_else(|_| need.clone())
                                },
                            ) {
                                Ok((reserved, _)) => reserved,
                                Err(why) => {
                                    crate::palw_backends::note_throttled_v1("panel-capture-ledger", || {
                                        format!("[{PALW_PANEL}] capture sample of claim {} deferred: {why}", duty.claim_id)
                                    });
                                    break 'verdict None;
                                }
                            };
                            let Ok((_backend, samples)) = offload(backend, move |b| {
                                let _held_for_the_samples = reserved;
                                Self::fp_capture_samples_clear(
                                    b,
                                    &capture_owned,
                                    &prompt_owned,
                                    leaves,
                                    artifact_root,
                                    ladder,
                                    &claim_owned,
                                    prompt_ids_form,
                                )
                            })
                            .await
                            else {
                                continue;
                            };
                            if seat_r && !matches!(samples, CaptureSamplesV1::FaultAt { .. }) {
                                capture_sampled.insert(duty.claim_id);
                            }
                            match samples {
                                CaptureSamplesV1::Cleared => {}
                                CaptureSamplesV1::NotCleared { refusal } => {
                                    // Once per claim: the arm re-runs every tick until a verdict.
                                    if let Some(why) = refusal
                                        && self.first_sample_refusal_v1(duty.claim_id)
                                    {
                                        warn!(
                                            "[{PALW_PANEL}] claim {}: no sample of the served capture adjudicates on this seat (first: \
                                             {why}) — nothing filed from this arm",
                                            duty.claim_id
                                        );
                                    }
                                    continue;
                                }
                                // ADR-0098 Decision 2: a leaf of the claim's OWN capture — its roots
                                // matched, its price matched — that does not recompute is a fault
                                // this seat found, and it is recorded rather than dropped.
                                CaptureSamplesV1::FaultAt { leaf, refutation, openings, prompt_opening } => {
                                    self.note_seat_fault_v1(duty.claim_id, leaf, 1);
                                    // P2-8c (ADR-0152 N10): the same refutation is a proof against every
                                    // `Valid` signer of the claim, filed once a licence names them.
                                    false_valid.note_step_refutation_v1(
                                        crate::palw_filer_false_valid::palw_false_valid_armed_v1(
                                            &self.consensus_config.params,
                                            self.config.fee_outpoint.is_some(),
                                            current_daa,
                                        ),
                                        duty.claim_id,
                                        &refutation,
                                        &openings,
                                        prompt_opening.as_ref(),
                                        crate::palw_filer_false_valid::PalwFalseValidSourceV1::CaptureSample { leaf },
                                        Some(duty.bound_daa),
                                        current_daa,
                                    );
                                    // **P2-8 (SR-8): past `palw_rcore_plus`, `ExecutorRefuted` over
                                    // the refutation in hand** — built before the accusation below
                                    // takes the refutation by value; `None` below the fence.
                                    let refuted = self.capture_arm_filing_v1(
                                        duty,
                                        deadline,
                                        current_daa,
                                        bond_key,
                                        &refutation,
                                        &openings,
                                        &prompt_opening,
                                    );
                                    // **The one thing a seat that found a lie files** (ADR-0098
                                    // Decision 2, ADR-0099 Decision 5): on a network whose
                                    // acceptance layer takes it, the accusation — the leaf, the
                                    // refutation it just ran, the openings it just proved —
                                    // signed under this seat's bond key and queued on the same
                                    // carrier path every court move rides. Once per claim.
                                    let court = 'court: {
                                        if !crate::palw_producer::palw_shard_court_in_force_v1(&self.consensus_config, current_daa)
                                            || accused.contains(&duty.claim_id)
                                        {
                                            break 'court None;
                                        }
                                        use kaspa_consensus_core::palw_shard_court_v1::{
                                            PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT, PALW_SHARD_COURT_VERSION_V1,
                                            PalwShardCourtAccusationV1, palw_shard_court_session_id_v1,
                                        };
                                        let ladder = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
                                            &self.config.court,
                                            self.consensus_config.params.palw_court_ladder.is_some_and(|f| f.is_active(current_daa)),
                                        );
                                        // ADR-0152 §4-ter: the CLASS's ladder past the fence.
                                        let ladder = self.seat_refutation_ladder_v1(duty.class_id, ladder, current_daa);
                                        // The chain adjudicates at ITS refutation ladder (2^22
                                        // where `palw_court_ladder` is dormant), and this seat
                                        // sampled at the bundle's; a claim above the chain's
                                        // ladder is a fault the one-move court cannot try, so
                                        // it is recorded (above) and not filed into a drop.
                                        if refutation.binding.step_leaf_count > ladder {
                                            warn!(
                                                "[{PALW_PANEL}] claim {}: {} leaves is above the refutation ladder the chain tries this class at ({ladder}); the fault is recorded, not accused",
                                                duty.claim_id, refutation.binding.step_leaf_count
                                            );
                                            break 'court None;
                                        }
                                        let mut accusation = PalwShardCourtAccusationV1 {
                                            version: PALW_SHARD_COURT_VERSION_V1,
                                            claim: duty.claim_id,
                                            execution_root: duty.execution_root,
                                            trace_root: duty.trace_root,
                                            executor_bond: duty.executor_bond,
                                            accuser_bond: duty.seat_bond,
                                            leaf_index: leaf,
                                            // t12: a fused site's history is the dissection's, and
                                            // the bound verdict refuses it in the accusation.
                                            refutation: if self.consensus_config.params.palw_audit_2026_09_23_active_at(current_daa) {
                                                kaspa_consensus_core::palw_shard_court_v1::palw_one_move_refutation_v2(*refutation)
                                            } else {
                                                *refutation
                                            },
                                            artifact_openings: openings,
                                            prompt_ids_opening: prompt_opening,
                                            signature: Vec::new(),
                                        };
                                        match accusation.validate_shape(ladder) {
                                            Ok(()) => {
                                                let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                                                    self.consensus_config.params.net.to_string().as_bytes(),
                                                    Some(self.consensus_config.genesis.hash),
                                                );
                                                let session_id = palw_shard_court_session_id_v1(domain.as_byte_slice(), &accusation);
                                                if let Some(signature) =
                                                    self.sign(session_id.as_byte_slice(), PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT)
                                                {
                                                    accusation.signature = signature;
                                                    let object =
                                                        PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(accusation) };
                                                    match kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object) {
                                                        Ok(()) => break 'court Some((session_id, object)),
                                                        Err(why) => warn!(
                                                            "[{PALW_PANEL}] claim {}: the accusation cannot ride a carrier ({why}); recorded, not filed",
                                                            duty.claim_id
                                                        ),
                                                    }
                                                }
                                            }
                                            Err(e) => warn!(
                                                "[{PALW_PANEL}] claim {}: the accusation's shape is refused locally ({e}); recorded, not filed",
                                                duty.claim_id
                                            ),
                                        }
                                        None
                                    };
                                    // P2-8: kind 4 through the reporter filer first, the accusation
                                    // kept as its fallback; else the accusation, as below the fence.
                                    let court_key = court.as_ref().map(|(session_id, _)| (*session_id, 0u32, false));
                                    if court_key.is_some() {
                                        use kaspa_consensus_core::palw_detection_policy_v1::PalwDetectionStageV1 as St;
                                        // G14 funnel: this seat localised a leaf and queued its accusation.
                                        for stage in [St::Eligible, St::Checked, St::Detected, St::Localized, St::Filed] {
                                            self.config.telemetry.detection_stage_once(duty.class_id, duty.claim_id, stage);
                                        }
                                        self.config.telemetry.detection_watch(duty.class_id, duty.claim_id);
                                    }
                                    self.capture_arm_files_v1(
                                        &session,
                                        &mut reporter_filer,
                                        &mut accused,
                                        &mut court_outbox,
                                        duty,
                                        leaf,
                                        bond_key,
                                        &network_domain,
                                        refuted,
                                        court,
                                    );
                                    // Dated (the second review's MEDIUM), and the one-move
                                    // accusation due NOW (the fourth's): the held dissection it
                                    // opens holds `Final` off only once it lands, and `Final` can
                                    // come a challenge window after the bound (`bound + 121` on the
                                    // 8k row), long before the duty's deadline. The filer's own
                                    // items (a kind 4 filed first) carry the filer's dates, read
                                    // after its pass.
                                    if let Some(key) = court_key
                                        && court_outbox
                                            .pending()
                                            .iter()
                                            .any(|(sid, round, responder, _)| (*sid, *round, *responder) == key)
                                    {
                                        court_outbox.date_if_absent(key, current_daa);
                                    }
                                    break 'verdict None;
                                }
                            }
                            self.persist_foreign_material(&duty.claim_id, &bytes);
                            // SEAT-R: cleared samples are no replay — the bytes are kept, and
                            // the next payload is judged.
                            if palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::FreePromptCapture) {
                                break 'verdict Some(PalwReceiptVerdictV2::Valid);
                            }
                            continue;
                        }
                        // `FPM1` alone — the question without the answer. Re-execution is the
                        // verifier this lane had before ADR-0073, kept for clients that do not
                        // yet ship the capture: a seat's last resort, never its duty, and
                        // bounded exactly as it always was. Past SEAT-R it is the full seat's
                        // duty, and the replay pass at the top of this lane has run this job
                        // off the loop already.
                        if seat_r {
                            continue;
                        }
                        let Some(material) = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_material_decode_v1(
                            &bytes,
                            self.class_prompt_ids_form(duty.class_id),
                        ) else {
                            continue;
                        };
                        if material.job.class_id != duty.class_id
                            || material.job.executor_bond != duty.executor_bond.0
                            || !self.fp_privacy_mode_judgeable(material.job.privacy_mode)
                        {
                            continue;
                        }
                        // The same gate as the capture arm, on the arm that RE-EXECUTES: this
                        // one derives a prompt to run, so ids that do not bind to the job
                        // would have it run a different job and disagree with an honest
                        // producer (ADR-0077 Decision 16, W8).
                        if let Err(why) = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_seat_prompt_admit_v1(
                            &material.job,
                            Some(&material.prompt_token_ids),
                            self.class_prompt_ids_form(duty.class_id),
                        ) {
                            warn!("[{PALW_PANEL}] claim {}: the served prompt is not this claim's ({why})", duty.claim_id);
                            continue;
                        }
                        let Ok(backend) = self.resolve_backend_for_claim_v1(&session, duty.class_id, duty.artifact_root, Some(duty.claim_id)) else {
                            break 'verdict Some(PalwReceiptVerdictV2::Incapable);
                        };
                        let Some(prompt_ids) = Self::fp_prompt_for_job(
                            kaspa_consensus_core::palw_backend::PalwExecutionOpsV1::new(backend.as_ref()),
                            &material,
                            self.class_prompt_ids_form(material.job.class_id),
                        ) else {
                            warn!("[{PALW_PANEL}] claim {}: a canonical claim whose material is not its own prompt", duty.claim_id);
                            continue;
                        };
                        let prompt: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
                        let material_job = material.job.clone();
                        let Ok((_backend, run)) = offload(backend, move |b| b.execute_free_prompt(&material_job, &prompt)).await
                        else {
                            continue;
                        };
                        let run = match run {
                            Ok(run) => run,
                            Err(e) => {
                                // An execution the class refuses is not evidence either way —
                                // logged so an operator can see WHY a lane stays unverified.
                                warn!("[{PALW_PANEL}] free-prompt replay for claim {} refused: {e}", duty.claim_id);
                                continue;
                            }
                        };
                        if run.outcome.execution_root != duty.execution_root || run.facts.full_logits_trace_root != duty.trace_root {
                            // A replay that disagrees is the court's business, not a receipt's:
                            // this seat simply has not verified the claim, and the shared tail
                            // below (pull, then the half-window accusation) applies unchanged.
                            continue;
                        }
                        // **The roots are not the whole question: WAS THIS THE WORK THE CLAIM
                        // WAS PAID FOR?**
                        //
                        // A commitment's `execution_root` rides its payload verbatim and the
                        // chain relates it to nothing (it holds no leg roots to recompute
                        // from), while the claim's `pwu`/`quanta` come from the job shape the
                        // payload DECLARES. So a producer may declare a hundred-thousand-token
                        // job, serve a one-token material whose roots are honestly that
                        // material's, and a seat comparing only roots certifies it — after
                        // which the claim's quanta are spendable as block work bought with
                        // recycled collateral instead of inference. That is the one property
                        // this lane exists to establish, so the seat re-prices what it
                        // actually ran and refuses anything that is not the claim's price.
                        if !kaspa_consensus_core::palw_backend::palw_opening_is_at_the_claims_price_v1(
                            run.facts.step_leaf_count,
                            duty.work_leaves,
                        ) {
                            warn!(
                                "[{PALW_PANEL}] claim {}: the replay has {} leaves and the claim was priced at {} — the roots \
                                 match but the WORK does not, so this is not the claim's material",
                                duty.claim_id, run.facts.step_leaf_count, duty.work_leaves
                            );
                            continue;
                        }
                        self.persist_foreign_material(&duty.claim_id, &bytes);
                        debug_assert!(palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::FreePromptReplay));
                        break 'verdict Some(PalwReceiptVerdictV2::Valid);
                    }
                }
                // SEAT-R: a material this seat has verified and kept is not verified again while
                // its replay runs — it licenses nothing past the fence either way.
                let kept_as_evidence = seat_r && evidence_kept.contains(&duty.claim_id);
                if !duty.free_prompt && !kept_as_evidence {
                    for bytes in pool_books.material.map().get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]) {
                        // Through the backend seam, which recomputes the leg root exactly.
                        // `Mismatch` is deliberately NOT an accusation here: it gathers no quorum
                        // and the claim voids. Convicting is the court's move, on evidence, and a
                        // seat that cannot reproduce a claim has not yet produced any.
                        //
                        // Resolved per duty from what the CHAIN says the claim's class is. A seat
                        // holding no material for that class cannot judge it — and it now SAYS so,
                        // instead of filing nothing.
                        //
                        // Filing nothing was read by the chain as a no-show and charged. Sortition
                        // does not ask which classes a node can execute, so that charge landed on
                        // seats whose only fault was being picked, and no answer avoided it:
                        // `Valid` would be a lie and `Unavailable` a signed accusation against an
                        // honest producer. `Incapable` is the missing answer — free, and counting
                        // toward neither side of the quorum. The chain refuses it for the liveness
                        // floor, where no node can truthfully claim it, so filing it there would
                        // only waste a fee.
                        let Ok(backend) = self.resolve_backend_for_claim_v1(&session, duty.class_id, duty.artifact_root, Some(duty.claim_id)) else {
                            break 'verdict Some(PalwReceiptVerdictV2::Incapable);
                        };
                        let Some(anchor) = self.job_anchor_for_claim(
                            &session,
                            backend.as_ref(),
                            network_domain,
                            duty.accepted_block,
                            duty.class_id,
                            &duty.executor_bond,
                        ) else {
                            break 'verdict None;
                        };
                        let roots = PalwClaimRootsV1 {
                            execution_root: duty.execution_root,
                            trace_root: duty.trace_root,
                            anchor,
                            attempt_draw: self.attempt_draw_for_claim(&session, duty.accepted_block),
                            output_root: Some(duty.output_root),
                            job_pin: duty.fp_job_pin_v1(),
                        };
                        let arm = palw_attempt_material_arm_v1(
                            seat_r,
                            PalwSeatArmV1::AttemptMaterial,
                            kaspa_consensus_core::palw_backend::PalwReplayOpsV1::new(backend.as_ref()),
                            bytes,
                            roots,
                        );
                        if arm == PalwMaterialArmV1::Nothing {
                            // P2-8, J1 auto: the claim's committed execution under another job?
                            self.j1_auto_probe_v1(
                                &session,
                                &mut reporter_filer,
                                duty,
                                deadline,
                                current_daa,
                                bond_key,
                                &network_domain,
                                bytes,
                                roots,
                            )
                            .await;
                        }
                        if arm != PalwMaterialArmV1::Nothing {
                            // **Retained here, and only here**: the chain carries this claim, this
                            // seat is on its panel, and these exact bytes reproduce its committed
                            // roots. Everything weaker was what let a stranger fill the disk
                            // (audit M2-2). Past SEAT-R they are kept all the same — the claim's
                            // material to serve, and what the producer committed if the replay
                            // refutes it — and the verdict is the replay's.
                            self.persist_foreign_material(&duty.claim_id, bytes);
                            if arm == PalwMaterialArmV1::Licenses {
                                break 'verdict Some(PalwReceiptVerdictV2::Valid);
                            }
                            evidence_kept.insert(duty.claim_id);
                            break;
                        }
                    }
                    // **This node's own disk, before the network and before any accusation**
                    // (audit M2-21). A seat that restarts loses its pool but keeps its retention,
                    // and the court arm already reads it — the verdict arm did not, so a restarted
                    // seat signed `Unavailable` against a producer whose material was sitting in
                    // its own directory. Verified like anything else: a file is evidence only if it
                    // reproduces the roots the claim committed to.
                    if !(seat_r && evidence_kept.contains(&duty.claim_id))
                        && let Some(bytes) = self.retained_capture(&duty.claim_id).or_else(|| {
                            std::fs::read(self.config.retention_dir.join("foreign").join(format!("{}.material", duty.claim_id))).ok()
                        })
                        && let Ok(backend) = self.resolve_backend_for_claim_v1(&session, duty.class_id, duty.artifact_root, Some(duty.claim_id))
                        && let Some(anchor) = self.job_anchor_for_claim(
                            &session,
                            backend.as_ref(),
                            network_domain,
                            duty.accepted_block,
                            duty.class_id,
                            &duty.executor_bond,
                        )
                    {
                        let arm = palw_attempt_material_arm_v1(
                            seat_r,
                            PalwSeatArmV1::RetainedMaterial,
                            kaspa_consensus_core::palw_backend::PalwReplayOpsV1::new(backend.as_ref()),
                            &bytes,
                            PalwClaimRootsV1 {
                                execution_root: duty.execution_root,
                                trace_root: duty.trace_root,
                                anchor,
                                attempt_draw: self.attempt_draw_for_claim(&session, duty.accepted_block),
                                output_root: Some(duty.output_root),
                                job_pin: duty.fp_job_pin_v1(),
                            },
                        );
                        if arm != PalwMaterialArmV1::Nothing {
                            pool_books.material.admit(duty.claim_id, bytes);
                            if arm == PalwMaterialArmV1::Licenses {
                                break 'verdict Some(PalwReceiptVerdictV2::Valid);
                            }
                            evidence_kept.insert(duty.claim_id);
                        }
                    }
                }
                // **The interval arm on the attempt lane** (ADR-0084 Decision 4 — ADR-0077
                // Decision 8's "at which point the same arm serves both lanes"). Reached only
                // when no whole material verified: a class whose capture fits the transport
                // (the floor, the graph-v2 class) has concluded above. The job and the
                // prompt come from the ANCHOR, exactly as the whole-capture arm derives them;
                // the answer's ids from the served attempt envelope (`ATA1`) or a capture this
                // node holds, bound to the claim's `output_root` under the anchor-derived
                // context; the state is recomputed and `k` intervals replayed, as on the
                // free-prompt lane. `None` falls through to the pull and the half-window
                // exactly as before.
                if !duty.free_prompt
                    && let Ok(resolved) = self.resolve_backend_for_claim_v1(&session, duty.class_id, duty.artifact_root, Some(duty.claim_id))
                    && let Some((ctx, prompt)) = self.attempt_job_for_claim(
                        &session,
                        resolved.as_ref(),
                        network_domain,
                        duty.accepted_block,
                        duty.class_id,
                        &duty.executor_bond,
                    )
                {
                    // **ADR-0084 Decision 7 — the verdict by execution.** No material of this
                    // claim's is held and, past the grace a push needs, none is coming: the seat
                    // re-runs the canonical job the anchor implies and compares the roots. A
                    // graph-v5 attempt is 784 MB of tiles the seat never fetches and one
                    // execution it can afford — the producer's own cost.
                    let resolved = if seat_r {
                        // **SEAT-R: the replay is the seat's one route to `Valid`, so it starts at
                        // the bind.** The grace waited for a material that would license first,
                        // and past the fence none does. A partial seat on a class this seat has
                        // not read files nothing, and one on a C7 class that is not the claim's
                        // outsider resumes its own mask (SEAT-S4); the full seat — and a replaying
                        // partial seat — replays off the loop (`PalwSeatRRoleV1`). The replay is not
                        // spent by a tick that could not start it: `replayed` spent the one replay
                        // on a ledger refusal, which a material arm used to cover. A claim it has
                        // refuted is not replayed again: the full seat's fault finders run on.
                        if seat_r_duty.role == PalwSeatRRoleV1::PartialAbstains {
                            break 'verdict None;
                        }
                        // SEAT-S4: a C7 partial seat resumes its own mask from the anchor's job
                        // (`seat_s4_resume_v1`), and files that mask's V3 alone.
                        if seat_r_duty.role == PalwSeatRRoleV1::PartialResumes {
                            // The attempt lane holds its job by the anchor, which no producer serves:
                            // whatever the resume comes to, this seat accuses nobody of withholding.
                            // An opening it lacks files the `Unavailable` at the wait and no
                            // accusation — DA-4's `Flat` answer to an accusation of row 0 does not
                            // reach an opening; P2-8d names the opening's own unit (the P2-6 review, LOW).
                            service.note_served(duty.claim_id);
                            match self
                                .seat_s4_resume_v1(
                                    &session,
                                    duty,
                                    seat_r_duty,
                                    current_daa,
                                    network_domain,
                                    &ctx,
                                    &prompt,
                                    pool_books.intervals.resume_mut(),
                                    &mut requested_intervals,
                                    &mut seat_replays,
                                    &mut seat_resumes,
                                )
                                .await
                            {
                                PalwSeatResumeStepV1::Licensed(mask) => {
                                    debug_assert!(palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::VerifiedSegmentResume));
                                    segments_attested = Some(mask);
                                    licensed_by_verified_resume = true;
                                    break 'verdict Some(PalwReceiptVerdictV2::Valid);
                                }
                                PalwSeatResumeStepV1::Waiting => break 'verdict None,
                                // Served, and nothing left to try: silent, never an accusation (N-5).
                                PalwSeatResumeStepV1::Starved { served: true, .. } => break 'verdict None,
                                PalwSeatResumeStepV1::Starved { served: false, .. } => {
                                    if palw_seat_material_wait_ends_v1(seat_r, duty.bound_daa, deadline, current_daa, || {
                                        Self::palw_licence_stands_v1(
                                            &session,
                                            duty.claim_id,
                                            &receipt_pool_v2,
                                            &receipt_pool_v3,
                                            &receipt_facts,
                                        )
                                    }) {
                                        break 'verdict Some(PalwReceiptVerdictV2::Unavailable {
                                            chunk_index: 0,
                                            requested_daa: first_seen[&duty.claim_id].max(duty.bound_daa),
                                        });
                                    }
                                    break 'verdict None;
                                }
                            }
                        }
                        if refuted {
                            resolved
                        } else {
                            match self
                                .attempt_seat_replay_v1(
                                    &session,
                                    duty,
                                    seat_r_duty,
                                    current_daa,
                                    resolved,
                                    &ctx,
                                    &prompt,
                                    &mut seat_replays,
                                )
                                .await
                            {
                                (PalwSeatReplayStepV1::Licensed, _) => {
                                    debug_assert!(palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::AttemptReplay));
                                    licensed_by_replay = true;
                                    break 'verdict Some(PalwReceiptVerdictV2::Valid);
                                }
                                (PalwSeatReplayStepV1::Waiting, _) => break 'verdict None,
                                // Terminal for `Valid` (`Refuted`), or no verdict: the full seat's
                                // fault finders run on; a partial seat that replays is the fallback
                                // for a licence, not a second full seat.
                                (step, backend) => {
                                    if step == PalwSeatReplayStepV1::Refuted {
                                        replay_refuted.insert(duty.claim_id);
                                    }
                                    match backend {
                                        Some(backend) if seat_r_duty.role == PalwSeatRRoleV1::FullSeat => backend,
                                        _ => break 'verdict None,
                                    }
                                }
                            }
                        }
                    } else if current_daa >= duty.bound_daa.saturating_add(PALW_ATTEMPT_REPLAY_GRACE_DAA)
                        && replayed.insert(seat_duty_panel_key_v1(duty))
                    {
                        if self.consensus_config.params.palw_verification_v2_at(current_daa) {
                            match self
                                .palw_v2_try_partial_resume_v1(
                                    duty,
                                    resolved.as_ref(),
                                    &ctx,
                                    &prompt,
                                    current_daa,
                                    network_domain,
                                    pool_books.material.map().get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]),
                                    pool_books.intervals.map(),
                                    &mut requested_intervals,
                                )
                                .await
                            {
                                PalwV2SeatPathV1::Waiting => break 'verdict None,
                                PalwV2SeatPathV1::Faulted => break 'verdict None,
                                PalwV2SeatPathV1::Abstain => break 'verdict None,
                                PalwV2SeatPathV1::Attested(mask) => {
                                    if !palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::SegmentResume) {
                                        break 'verdict None;
                                    }
                                    segments_attested = Some(mask);
                                    info!(
                                        "[{PALW_PANEL}] claim {}: licensed by V2 segment resume — mask {:#x} (ADR-0133 S1)",
                                        duty.claim_id, mask.0
                                    );
                                    break 'verdict Some(PalwReceiptVerdictV2::Valid);
                                }
                                PalwV2SeatPathV1::FullReplay => {}
                            }
                        }
                        let assignment = kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2(
                            duty.panel_anchor,
                            duty.claim_id,
                            duty.panel_seat_count.max(1),
                        );
                        if self.consensus_config.params.palw_verification_v2_at(current_daa)
                            && duty.seat_index as u16 != assignment.full_seat
                        {
                            break 'verdict None;
                        }
                        let (ctx_for_blocking, prompt_for_blocking) = (ctx.clone(), prompt.clone());
                        let started = std::time::Instant::now();
                        info!(
                            "[{PALW_PANEL}] claim {}: no material held after {} DAA — replaying the anchor's job for a verdict (ADR-0084 D7)",
                            duty.claim_id,
                            current_daa.saturating_sub(duty.bound_daa)
                        );
                        // **The replay's bytes are RESERVED for its life** (ADR-0151 follow-up,
                        // item 3): the full-seat need of THIS job, taken from the ledger before
                        // the blocking task starts and released when it returns, so a producer's
                        // attempt in this process cannot start beside it. A refusal defers the
                        // duty to a later tick, as the pre-check's did.
                        // At the widest prefill run the ledger grants, on this duty's own instance —
                        // the one offloaded below (int-10.2 A2).
                        let mut resolved = resolved;
                        let routing_owner =
                            match self.bind_native_routing_replay_v2(&session, resolved.as_mut(), duty.claim_id, &ctx) {
                                Ok(owner) => owner,
                                Err(why) => {
                                    warn!("[{PALW_PANEL}] claim {}: native routed replay held: {why}", duty.claim_id);
                                    break 'verdict None;
                                }
                            };
                        let backends = self.backends();
                        let reserved = match self.reserve_replay_at_widest_run_v1(
                            "full-seat",
                            resolved.as_mut(),
                            duty.class_id,
                            duty.claim_id,
                            |b| {
                                backends.role_memory_need_for_backend_or_chain_v1(
                                    b,
                                    duty.class_id,
                                    duty.artifact_root,
                                    Some(&ctx),
                                    FULL_SEAT_V1,
                                    |id| self.chain_carriage_v1(&session, id),
                                )
                            },
                        ) {
                            Ok((reserved, _)) => reserved,
                            Err(why) => {
                                crate::palw_backends::note_throttled_v1("panel-replay-ledger", || {
                                    format!("[{PALW_PANEL}] replay of claim {} deferred: {why}", duty.claim_id)
                                });
                                break 'verdict None;
                            }
                        };
                        // ADR-0112 Decision 8, for a seat: what the replay read from storage
                        // and what each held residency read of it — a storage line per replay.
                        let storage_before = crate::palw_backends::storage_snapshot_v1(&self.class_holdings);
                        let replayed = offload(resolved, move |b| {
                            let _held_for_the_replay = reserved;
                            b.execute_for_verdict(&ctx_for_blocking, &prompt_for_blocking)
                        })
                        .await;
                        if let Err(why) = self.refresh_native_routing_replay_v2(&session, routing_owner.as_ref()) {
                            warn!("[{PALW_PANEL}] claim {}: replay context changed: {why}", duty.claim_id);
                            break 'verdict None;
                        }
                        crate::palw_backends::log_storage_v1(PALW_PANEL, "replay", &storage_before, &self.class_holdings);
                        match replayed {
                            Ok((backend, Ok(roots))) => {
                                if replay_licenses_v1(&roots, duty.execution_root, duty.trace_root, duty.work_leaves) {
                                    self.config.telemetry.panel_replay(
                                        duty.class_id,
                                        started.elapsed().as_millis() as u64,
                                        roots.work_leaves.unwrap_or(duty.work_leaves),
                                    );
                                    info!(
                                        "[{PALW_PANEL}] claim {}: licensed by replay — the anchor's job reproduces the claim's roots \
                                         ({} leaves replayed, priced {}, {:.0?}); no material moved (ADR-0084 D7)",
                                        duty.claim_id,
                                        roots.work_leaves.map(|w| w.to_string()).unwrap_or_else(|| "unpriced".into()),
                                        duty.work_leaves,
                                        started.elapsed()
                                    );
                                    debug_assert!(palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::AttemptReplay));
                                    break 'verdict Some(PalwReceiptVerdictV2::Valid);
                                }
                                warn!(
                                    "[{PALW_PANEL}] claim {}: the anchor's job does NOT reproduce the claim's roots (execution {} vs claimed {}, \
                                     trace {} vs {}, work {:?} vs priced {}) — no verdict from the replay; the opening lane decides",
                                    duty.claim_id,
                                    roots.execution_root,
                                    duty.execution_root,
                                    roots.trace_root,
                                    duty.trace_root,
                                    roots.work_leaves,
                                    duty.work_leaves
                                );
                                backend
                            }
                            Ok((backend, Err(e))) => {
                                warn!("[{PALW_PANEL}] claim {}: the replay refused: {e}", duty.claim_id);
                                backend
                            }
                            Err(e) => {
                                warn!("[{PALW_PANEL}] claim {}: the replay did not run: {e}", duty.claim_id);
                                break 'verdict None;
                            }
                        }
                    } else {
                        resolved
                    };
                    let prompt_ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
                    let held = pool_books.material.map().get(&duty.claim_id).map(|v| v.to_vec()).unwrap_or_default();
                    // The job's id IS the block's anchor: `job_for_anchor` names it so.
                    let anchor = ctx.job_id;
                    if let Some(output_ids) =
                        self.attempt_committed_output_ids_v1(kaspa_consensus_core::palw_backend::PalwMaterialOpsV1::new(resolved.as_ref()), duty, &ctx, anchor, &prompt_ids, &held)
                        && let Some(verdict) = self
                            .interval_seat_outcome_v1(
                                &session,
                                duty,
                                network_domain,
                                current_daa,
                                &ctx,
                                anchor,
                                &prompt_ids,
                                &output_ids,
                                pool_books.intervals.map(),
                                None,
                            )
                            .await
                        // SEAT-R: the sampled intervals find faults; they license nothing.
                        && (!matches!(verdict, PalwReceiptVerdictV2::Valid)
                            || palw_seat_arm_licenses_v1(seat_r, PalwSeatArmV1::AttemptInterval))
                    {
                        break 'verdict Some(verdict);
                    }
                }
                // ADR-0098 Decision 2, before the tail: a fault found anywhere above in this round
                // (the attempt lane's interval arm runs last) is not "no verifying material".
                if self.seat_found_fault_v1(&duty.claim_id) {
                    break 'verdict None;
                }
                // **SEAT-R: a seat holding the claim's verified material accuses nobody of
                // withholding it.** Below the fence nothing verified reaches this line; past it a
                // verified material licenses nothing, so a full seat whose replay refused, or a
                // partial seat before its resume, reaches it holding the producer's own bytes.
                if seat_r && evidence_kept.contains(&duty.claim_id) {
                    break 'verdict None;
                }
                // No verifying material yet. Ask the network before accusing: the producer may
                // be gone, but any peer that heard the broadcast can re-serve it.
                //
                // **The condition is "nothing VERIFIED", which is what the loop above just
                // established — not "the pool is empty"** (audit M2-1). Gating on emptiness
                // meant one unverifiable byte-string suppressed the pull permanently: an
                // attacker sends four ~70-byte payloads for a claim id it reads off the header,
                // the pool is non-empty and matches nothing, the seat never asks, and at half
                // the window it signs `Unavailable` against an honest producer. Reaching this
                // line already means every pooled payload failed to verify.
                //
                // **At least twice before the accusation below** (the 2026-09-20 Studio drill).
                // The re-ask was a flat 25 DAA, and the accusation comes at half the receipt
                // window: on the devnet's 40-DAA window that is 20, so a seat whose first pull
                // went unanswered — the executor's throttle refused four of five seats asking
                // in the same second — signed `Unavailable` without ever asking again. A
                // quarter of the window puts two asks inside the half; 25 stays the ceiling, so
                // a long window (testnet-11's) re-asks exactly as before.
                // The window is the one the loop reads for the duty (`palw_seat_receipt_deadline_v1`):
                // the duty's own below SEAT-R, the chain's per-claim one past it.
                let window = deadline.saturating_sub(duty.bound_daa);
                let reask_daa = seat_reask_daa_v1(window);
                if requested.get(&duty.claim_id).is_none_or(|at| current_daa >= at.saturating_add(reask_daa)) {
                    requested.insert(duty.claim_id, current_daa);
                    // Registered with the gossip center first, and SIGNED (ADR-0077 SA-2): the
                    // answer must be exempt from the per-claim relay budget an attacker may
                    // already have spent, and a serving node refuses an unsigned pull — an
                    // unsigned ask would produce the silence this seat is about to accuse
                    // somebody of.
                    self.request_material_signed(network_domain, duty.claim_id, current_daa).await;
                }
                // Wait out half the window before accusing — gossip is not instant and an
                // early `Unavailable` is a false accusation with a signature on it. Past SEAT-R no
                // later than `X_ASK` after the bind, and at once when a licence already stands
                // (N-5, `palw_seat_material_wait_until_v1`); below it, half the window as always.
                // Past SEAT-R only a producer that served this seat no job waits to be accused
                // (`palw_seat_tail_v1`, finding 2): a seat whose own replay did not run in time, or
                // refused twice, is silent. A claim this seat's replay refuted — this tick or an
                // earlier one — keeps its path, and is the challenger's half's, not an unserved one.
                let refuted_here = refuted || replay_refuted.contains(&duty.claim_id);
                match palw_seat_tail_v1(seat_r, duty.free_prompt, refuted_here, || {
                    let pooled = pool_books.material.map().get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]);
                    self.fp_job_material_for_claim(&duty.claim_id, duty.class_id, &duty.executor_bond, pooled).is_some()
                }) {
                    // Holding the claim's job, or on the attempt lane (whose job no producer
                    // serves): served, for the rest of the duty (N-5, the P2-6 review's HIGH).
                    PalwSeatTailV1::Silent => {
                        service.note_served(duty.claim_id);
                        break 'verdict None;
                    }
                    PalwSeatTailV1::Waits { unserved: true } => service.note_unserved(duty.claim_id, current_daa),
                    // A claim this seat's replay or fault finder refuted was served (false material).
                    PalwSeatTailV1::Waits { unserved: false } => {
                        if refuted_here {
                            service.note_served(duty.claim_id);
                        }
                    }
                }
                if palw_seat_material_wait_ends_v1(seat_r, duty.bound_daa, deadline, current_daa, || {
                    Self::palw_licence_stands_v1(&session, duty.claim_id, &receipt_pool_v2, &receipt_pool_v3, &receipt_facts)
                }) {
                    break 'verdict Some(PalwReceiptVerdictV2::Unavailable {
                        chunk_index: 0,
                        // Floored at THIS panel's `bound_daa`: `first_seen` dates the claim,
                        // not the panel, and a redrawn claim's second panel was inheriting a
                        // timestamp from before it existed — which the acceptance layer
                        // refuses as "the request predates the panel that was owed the data",
                        // taking the whole receipt set with it.
                        requested_daa: first_seen[&duty.claim_id].max(duty.bound_daa),
                    });
                }
                None
            };
            let Some(verdict) = verdict else { continue };
            // RFC38: every approving form, including Sampled and the vertex path, checks the actual accepted transaction.
            // A legacy served answer/replay is not a substitute for this carriage binding.
            if matches!(verdict, PalwReceiptVerdictV2::Valid | PalwReceiptVerdictV2::Sampled) {
                if let Err(why) = self.fp_accounting_answer_for_seat_v1(&session, duty) {
                    match why {
                        kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1::Resource(why) => {
                            crate::palw_backends::note_throttled_v1("panel-accounting-capacity", || format!("[{PALW_PANEL}] claim {} accounting check deferred by local capacity: {why}", duty.claim_id));
                        }
                        kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1::Decode(why) => {
                            crate::palw_backends::note_throttled_v1("panel-accounting-unbound", || format!("[{PALW_PANEL}] claim {} accounting answer unbound ({why}); no approving receipt/vertex filed", duty.claim_id));
                        }
                    }
                    continue;
                }
            }

            // **SEAT-R, enforced where the receipt is signed**: past the fence a `Valid` that no
            // off-loop replay licensed files nothing, whatever arm produced it — the source pin
            // (`seat_r_tests`) and the `debug_assert!`s name the arms; this holds in a release build.
            if seat_r && matches!(verdict, PalwReceiptVerdictV2::Valid) && !(licensed_by_replay || licensed_by_verified_resume) {
                crate::palw_backends::note_throttled_v1("panel-seat-r-guard", || {
                    format!("[{PALW_PANEL}] claim {}: a Valid no replay licensed — nothing filed (SEAT-R)", duty.claim_id)
                });
                continue;
            }
            // **SEAT-S4's licence is its own mask's V3, by a resuming seat, and nothing wider**: a
            // verified resume that is not a `PartialResumes` seat's, or whose mask is not the one the
            // assignment gave this seat, files nothing.
            if licensed_by_verified_resume
                && (seat_r_duty.role != PalwSeatRRoleV1::PartialResumes
                    || segments_attested
                        != Some(
                            kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2(
                                duty.panel_anchor,
                                duty.claim_id,
                                duty.panel_seat_count.max(1),
                            )
                            .mask_of(duty.seat_index as u16),
                        ))
            {
                warn!("[{PALW_PANEL}] claim {}: a verified resume outside its own mask — nothing filed (SEAT-S4)", duty.claim_id);
                continue;
            }
            // Inside what acceptance takes: at or after the bind, and at or before the deadline the
            // loop reads for the duty — which the due check above has already held it to.
            let signed_daa = current_daa.clamp(duty.bound_daa, deadline);
            let k = kaspa_consensus_core::palw_verification_v2::palw_segment_count_v2(duty.panel_seat_count.max(1));
            let v2_at = self.consensus_config.params.palw_verification_v2_at(current_daa);
            let mask = if v2_at {
                let assignment = kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2(
                    duty.panel_anchor,
                    duty.claim_id,
                    duty.panel_seat_count.max(1),
                );
                segments_attested.unwrap_or_else(|| assignment.mask_of(duty.seat_index as u16))
            } else {
                kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2::full(k)
            };
            let valid = matches!(verdict, PalwReceiptVerdictV2::Valid);
            // **Which receipts this verdict files** (`palw_seat_receipt_forms_v1`): the one it always
            // filed — except a `Valid` past SEAT-R, a replay's, which files the whole-job V2 receipt
            // beside its V3 over the seat's assigned mask (the whole job for the full seat).
            let forms = palw_seat_receipt_forms_v1(v2_at, seat_r, valid, seat_r_duty.role, mask);
            if forms.segments.is_none() && !forms.whole {
                warn!(
                    "[{PALW_PANEL}] claim {}: a Valid in a seat role that files nothing ({:?}) — nothing filed (SEAT-R)",
                    duty.claim_id, seat_r_duty.role
                );
                continue;
            }
            // **RFC-0007: past `palw_verification_vertex_v1`, a verdict on a claim whose panel bound at or after the fence is a LEAF of
            // this seat's round vertex, and no receipt is signed** (the chain's path rule: such a claim licenses by tally; one bound
            // before the fence keeps the receipt path to its licence or its void). A role that files no V3 form files no leaf either,
            // and a `Valid` leaf attests exactly the seat's assigned mask, which is all the chain takes of it.
            if let Some(fence) = vertex_fence
                && duty.bound_daa >= fence
            {
                if forms.segments.is_none() {
                    warn!(
                        "[{PALW_PANEL}] claim {}: a verdict in a seat role that files no leaf ({:?}) — nothing filed (SEAT-R)",
                        duty.claim_id, seat_r_duty.role
                    );
                    continue;
                }
                vertex_book.record(vertex::palw_vertex_verdict_leaf_v1(
                    &duty.claim_id,
                    duty.bound_daa,
                    verdict,
                    self.config.vertex_full_refs,
                ));
                // **A seat that verified a claim's material and kept it says so** — a `Held` leaf beside its verdict (RFC-0007 §I.7;
                // voluntary): three equal ones are a DA certificate, and an attester of a claim whose data the court concludes was
                // not served is charged its exposure.
                if valid
                    && let Some(bytes) = self.retained_capture(&duty.claim_id).or_else(|| {
                        std::fs::read(self.config.retention_dir.join("foreign").join(format!("{}.material", duty.claim_id))).ok()
                    })
                    && let Some(held) =
                        vertex::palw_vertex_held_capture_leaf_v1(&duty.claim_id, duty.bound_daa, &bytes, self.config.vertex_full_refs)
                {
                    vertex_book.record(held);
                }
                self.config.telemetry.panel_receipt(duty.class_id, verdict_name(&verdict));
                receipts_filed_at.push_back(std::time::Instant::now());
                info!(
                    "[{PALW_PANEL}] recorded a {:?} leaf for claim {} in this round's vertex (RFC-0007)",
                    verdict_name(&verdict),
                    duty.claim_id
                );
                if valid {
                    service.note_served(duty.claim_id);
                }
                if matches!(verdict, PalwReceiptVerdictV2::Unavailable { .. })
                    && service.is_unserved(&duty.claim_id)
                    && let Some(accuse_by) = palw_seat_da_accuse_by_v1(
                        self.consensus_config.params.palw_rcore_plus_active_at(current_daa),
                        replay_refuted.contains(&duty.claim_id),
                        current_daa,
                        deadline,
                    )
                {
                    accusations.want(duty.claim_id, accuse_by);
                }
                answered.insert(seat_duty_panel_key_v1(duty));
                continue;
            }
            let kp = self.keypair.as_ref().expect("checked at start");
            let sign = |message: Hash64, context: &'static [u8], form: &str| -> Option<Vec<u8>> {
                match libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, message.as_byte_slice(), context, [0u8; 32]) {
                    Ok(sig) => Some(sig.as_ref().to_vec()),
                    Err(e) => {
                        warn!("[{PALW_PANEL}] ML-DSA-87 {form}sign failed for claim {}: {e:?}", duty.claim_id);
                        None
                    }
                }
            };
            // Every form signed before any is filed: a signature that fails files nothing this tick,
            // and the duty is answered on the next.
            let segmented = match forms.segments {
                None => None,
                Some(mask) => {
                    let message = palw_receipt_message_v3(network_domain, duty.claim_id, verdict, signed_daa, mask);
                    let Some(signature) = sign(message, PALW_RECEIPT_V3_MLDSA87_CONTEXT, "V3 ") else { continue };
                    let inner = PalwSeatReceiptV2 { claim: duty.claim_id, verdict, seat_bond: bond_key, signed_daa, signature };
                    Some(PalwSeatReceiptV3 { receipt: inner, segments: mask })
                }
            };
            let whole = if forms.whole {
                let message = palw_receipt_message_v2(network_domain, duty.claim_id, verdict, signed_daa);
                let Some(signature) = sign(message, PALW_RECEIPT_V2_MLDSA87_CONTEXT, "") else { continue };
                Some(PalwSeatReceiptV2 { claim: duty.claim_id, verdict, seat_bond: bond_key, signed_daa, signature })
            } else {
                None
            };
            if self.config.tir_shard_shadow && licensed_by_replay && valid {
                shard_shadow.note(duty, true);
            }
            self.config.telemetry.panel_receipt(duty.class_id, verdict_name(&verdict));
            receipts_filed_at.push_back(std::time::Instant::now());
            let key = seat_duty_panel_key_v1(duty);
            let now = std::time::Instant::now();
            let filed = |segments| crate::palw_receipt_pool::OwnFiledV1 {
                claim: duty.claim_id,
                verdict,
                signed_daa,
                segments,
                schedule: crate::palw_receipt_pool::OwnRebroadcastV1::filed(now),
            };
            // ADR-0124 Decision 2: a `Valid` receipt is the one this seat may have to carry itself —
            // the whole-job one where it filed one (the V1 and supplementary validators check the V2
            // message), and otherwise the V3 receipt's inner half, as it always was. Kept until the
            // deadline the chain takes it by.
            if valid && let Some(carried) = whole.clone().or_else(|| segmented.as_ref().map(|r| r.receipt.clone())) {
                own_receipts.insert(duty.claim_id, (carried, deadline));
                own_receipt_class.insert(duty.claim_id, duty.class_id);
            }
            let beside = segmented.is_some();
            if let Some(receipt) = segmented {
                info!(
                    "[{PALW_PANEL}] filed a {:?} V3 receipt for claim {} (mask {:#x})",
                    verdict_name(&verdict),
                    duty.claim_id,
                    receipt.segments.0
                );
                let bytes = borsh::to_vec(&receipt).expect("a V3 receipt serializes");
                // **This node's own receipt is never in the evictable pool** (fix (1)). The V2
                // pool keeps the inner half beside it, as it always held it, where no whole-job
                // receipt is filed: the V1 assembler refuses it (it is signed over the V3
                // message), and it is what puts the claim on the collector's list.
                if whole.is_none() {
                    receipt_pool_v2.insert_own(receipt.receipt.clone(), current_daa);
                }
                own_filed.insert(key, filed(Some(receipt.segments)));
                receipt_pool_v3.insert_own(receipt, current_daa);
                self.flow_context.broadcast_palw_seat_receipt(bytes).await;
            }
            if let Some(receipt) = whole {
                info!(
                    "[{PALW_PANEL}] filed a {:?} receipt for claim {}{}",
                    verdict_name(&verdict),
                    duty.claim_id,
                    match (v2_at, beside) {
                        (false, _) => "",
                        (true, true) => " (the whole-job V2 beside the V3: this seat's replay, SEAT-R)",
                        (true, false) => "",
                    }
                );
                let bytes = borsh::to_vec(&receipt).expect("a receipt serializes");
                // The V1 door's candidate, and this node's own: never evictable (fix (1)).
                receipt_pool_v2.insert_own(receipt, current_daa);
                if beside {
                    own_filed_whole.insert(key, filed(None));
                } else {
                    own_filed.insert(key, filed(None));
                }
                self.flow_context.broadcast_palw_seat_receipt(bytes).await;
            }
            if valid {
                service.note_served(duty.claim_id);
            }
            // **P2-6: an `Unavailable` files a `DefaultAccused` too** (ADR-0152 §3.8, DA-1: every
            // class) — only a seat that was never served the claim (`PalwSeatServiceV1`: a seat
            // holding the job and waiting on an opening files the receipt and abstains, the P2-6
            // review's LOW), inside its landing margin and never for a claim this seat's own replay
            // refuted (`palw_seat_da_accuse_by_v1`); built, asked of the chain and queued below.
            if matches!(verdict, PalwReceiptVerdictV2::Unavailable { .. })
                && service.is_unserved(&duty.claim_id)
                && let Some(by) = palw_seat_da_accuse_by_v1(
                    self.consensus_config.params.palw_rcore_plus_active_at(current_daa),
                    replay_refuted.contains(&duty.claim_id),
                    current_daa,
                    deadline,
                )
            {
                accusations.want(duty.claim_id, by);
            }
            answered.insert(key);
        }

        // --- RFC-0007: this seat's vertex for the round ---
        //
        // Past the fence only. The vertices this seat has sealed are reconciled with the chain's rows (a landed one is forgotten, one that
        // waited out the carry window returns its leaves to the next round), then the pending leaves are sealed into this round's one
        // vertex — the round persisted FIRST, so a restart can never sign it twice — and queued for the carrier site below.
        if let Some(fence) = vertex_fence {
            let kp = self.keypair.as_ref().expect("checked at start");
            let from_round = (current_daa / kaspa_consensus_core::palw_vertex_v1::PALW_VERTEX_ROUND_DAA_V1)
                .saturating_sub(kaspa_consensus_core::palw_vertex_v1::PALW_VERTEX_MAX_CARRY_DAA_V1 + 2);
            let status = session.palw_v2_vertex_status_v1(bond_key, from_round);
            let rows: Vec<(u64, Hash64)> = status.own.iter().map(|(round, root, _)| (*round, *root)).collect();
            for round in vertex_book.reconcile(&rows, current_daa) {
                info!("[{PALW_PANEL}] this seat's vertex of round {round} is on the chain (RFC-0007)");
            }
            let sealed = vertex_book.seal(
                network_domain,
                bond_key,
                current_daa,
                &|message, context| Self::sign_hedged(&kp.signing_key, message, context),
                &|round| vertex::palw_vertex_state_write_v1(&vertex_state_path, round),
            );
            if let Some(round) = sealed {
                info!(
                    "[{PALW_PANEL}] sealed this seat's vertex of round {round} ({} leaves pending after it, {} sealed waiting)",
                    vertex_book.pending_leaves(),
                    vertex_book.sealed_waiting()
                );
            }
            // **DRILL ONLY: sign the round's second vertex** — the first vertex's leaves with every verdict changed — once, from the
            // configured DAA. The persisted round is not consulted: this is the one thing a seat must never do, done on purpose in a
            // salted drill so the equivocation's catch, slash and ejection can be shown.
            if let (Some(round), Some(at)) = (sealed, self.config.vertex_equivocate_at)
                && current_daa >= at
                && !*vertex_equivocated
                && let Some(first) = vertex_book.sealed_vertex(round)
            {
                let leaves: Vec<_> = first
                    .leaves
                    .iter()
                    .filter_map(|leaf| match leaf {
                        kaspa_consensus_core::palw_vertex_v1::PalwVertexLeafV1::Verdict { claim, .. } => {
                            Some(kaspa_consensus_core::palw_vertex_v1::PalwVertexLeafV1::Verdict {
                                claim: *claim,
                                verdict: PalwReceiptVerdictV2::Sampled,
                            })
                        }
                        _ => None,
                    })
                    .collect();
                if let Some(second) = kaspa_consensus_core::palw_vertex_v1::PalwVerificationVertexV1::sign_v1(
                    network_domain,
                    bond_key,
                    first.signed_daa,
                    leaves,
                    |message, context| Self::sign_hedged(&kp.signing_key, message, context),
                ) && second.leaves_root != first.leaves_root
                {
                    warn!("[{PALW_PANEL}] DRILL: signing a SECOND vertex for round {round} (equivocation)");
                    *vertex_equivocated = true;
                    vertex_book.drill_extra(second);
                }
            }
            let mut status_line = vertex_book.status_v1(Some(fence), status.rounds, status.tallies, status.held_claims);
            // RFC-0007 Parts II and IV: the mesh at the tip, this seat's audits and trap plans, and the sketch mirror's totals.
            status_line.push(' ');
            status_line.push_str(&mesh_node.status_v1(&session.palw_v2_mesh_status_v1(bond_key)));
            status_line.push(' ');
            status_line.push_str(&trap_book.status_v1());
            if let Some(sketch) = &self.sketch {
                status_line.push(' ');
                status_line.push_str(&sketch.status_v1());
            }
            self.flow_context.update_palw_runtime(|r| r.verification_vertex = status_line);
        }
    }
}
