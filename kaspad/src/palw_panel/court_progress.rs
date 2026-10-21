//! Serialized court/DA progression. The writable inputs are its outbox,
//! evidence/request books, received material and false-Valid filing state, never
//! the carrier chain or the receipt collector. All original nested loops remain.
use super::*;

pub(super) struct CourtProgressRefsV1<'a> {
    pub(super) pool_books: &'a mut PoolBooksV1,
    pub(super) own_executions: &'a mut HashMap<Hash64, Vec<u8>>,
    pub(super) attn_evidence:
        &'a mut HashMap<(Hash64, bool), Arc<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnSiteEvidenceV1>>,
    pub(super) attn_root_filings:
        &'a mut HashMap<Hash64, (u64, Option<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnAccusedFilingV1>)>,
    pub(super) tir_root_filings: &'a mut HashMap<Hash64, (u64, Option<kaspa_consensus_core::palw_tir_dissect_v1::PalwTirRootClaimV1>)>,
    pub(super) held_court: &'a mut held_court::PalwHeldCourtV1,
    pub(super) shard_books: &'a mut tir_shard::PalwTirShardBooksV1,
    pub(super) requested: &'a mut HashMap<Hash64, u64>,
    pub(super) requested_intervals: &'a mut HashMap<(Hash64, u32), u64>,
    pub(super) court_outbox: &'a mut CourtOutboxV1,
    pub(super) da_flat_sent: &'a mut HashMap<Hash64, u64>,
    pub(super) public_prefix_response_cursor: &'a mut Option<(u64, Hash64, PalwBondKeyV2)>,
    pub(super) false_valid: &'a mut crate::palw_filer_false_valid::PalwFalseValidFilerV1,
}

/// Retain the former top-level allocations until the caller's iteration ends.
/// Fields are in reverse declaration order to keep the old drop order. These are
/// data buffers, not a new cache, resource reservation or acceptance verdict.
pub(super) struct CourtProgressResultV1 {
    _owed: std::collections::BTreeSet<(Hash64, kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1)>,
    pub(super) disclosure_retain: Vec<Hash64>,
    _disclosure: kaspa_consensus_core::palw_producer_v2::PalwDisclosureDutiesV1,
    _da_duties: Vec<kaspa_consensus_core::palw_producer_v2::PalwDaDutyV2>,
    _held_duties: Vec<kaspa_consensus_core::palw_producer_v2::PalwCourtDutyV2>,
    _court_stalls: BTreeMap<&'static str, usize>,
    pub(super) court_duties: Vec<kaspa_consensus_core::palw_producer_v2::PalwCourtDutyV2>,
    _intervals_for_close: Vec<(Hash64, Vec<u32>)>,
    _pull_for_close: Vec<Hash64>,
}
impl PalwPanelService {
    pub(super) async fn court_progress_tick_v1(
        self: &Arc<Self>,
        tick: tick_inputs::PanelTickReadV1<'_>,
        books: CourtProgressRefsV1<'_>,
    ) -> CourtProgressResultV1 {
        let session = tick.session();
        let bond_key = tick.bond();
        let network_domain = tick.domain();
        let current_daa = tick.daa();
        let CourtProgressRefsV1 {
            pool_books,
            own_executions,
            attn_evidence,
            attn_root_filings,
            tir_root_filings,
            mut held_court,
            mut shard_books,
            requested,
            requested_intervals,
            mut court_outbox,
            da_flat_sent,
            mut public_prefix_response_cursor,
            false_valid,
        } = books;
        // --- the court's half: answer the disputes this bond is a party to ---
        //
        // Nothing in this tree used to construct a `CourtDisclosed`. A challenger could open a
        // session and the accused had no software able to answer it, so every dispute ran out
        // on the clock — which is why the opening rung had to stop convicting on silence. This
        // is the missing half: both parties act from the SAME capture, through the same
        // functions, so the bisection converges on a real divergence rather than on whoever
        // stayed awake.
        // **Why a party made no move is the fact an operator needs, and it was unloggable.**
        //
        // Six different gates below `continue` on a session, and five of them said nothing at
        // all — so a fleet where nobody answered looked identical whether the node held no
        // backend, no capture, no midpoint, no prefix state, or no key. Measured on the live
        // drill: 357 sessions opened, 357 total moves (one apiece, all openings), and the log
        // could not distinguish "the responder is broken" from "the responder is not a party".
        // `court_stalls` counts the reasons and the tick prints them, which is bounded (one
        // line per tick, only when something stalled) and turns silence into a measurement.
        // Claims whose ACCUSED capture a court move needs and this node does not hold. Collected
        // rather than awaited inline: the request borrows `self.flow_context` while the loop
        // still holds a borrow of `materials` (audit3 S-01).
        let mut pull_for_close: Vec<Hash64> = Vec::new();
        // ADR-0085 Decision 3: the intervals a close needs and this node does not hold, asked after the loop.
        let mut intervals_for_close: Vec<(Hash64, Vec<u32>)> = Vec::new();
        let court_duties = session.palw_court_duties_v2(vec![bond_key]);
        shard_books.retain_public_court_rows_v1(&court_duties, current_daa);
        let mut court_stalls: BTreeMap<&'static str, usize> = BTreeMap::new();
        attn_evidence.retain(|(session_id, _), _| court_duties.iter().any(|d| d.session_id == *session_id));
        attn_root_filings.retain(|session_id, _| court_duties.iter().any(|d| d.session_id == *session_id));
        tir_root_filings.retain(|session_id, _| court_duties.iter().any(|d| d.session_id == *session_id));
        held_court.begin_tick_v1(&court_duties, current_daa).await;
        let mut held_duties: Vec<kaspa_consensus_core::palw_producer_v2::PalwCourtDutyV2> = Vec::new();
        for duty in &court_duties {
            let move_round = court_move_round_v1(duty);
            if let Some(sent_daa) = court_outbox.submitted_at(&(duty.session_id, move_round, duty.i_am_responder))
                && current_daa < sent_daa.saturating_add(COURT_MOVE_REPLAN_DAA)
            {
                continue;
            }
            // **Already queued, from a tick that could not fund it** (audit M2-15). Moves that
            // do not fit the in-flight budget are carried over to the next tick, and this loop
            // rebuilt the same move every 2 seconds and pushed a duplicate — so the queue grew
            // by a copy per tick, `submitted_this_tick` spent the budget on the oldest copies,
            // and later sessions' moves were dropped unsent. A rung is clocked, so the moves
            // crowded out are exactly the ones whose lapse convicts the responder. The
            // opening-court branch has always had this guard; the responder branch did not.
            if court_outbox
                .pending()
                .iter()
                .any(|(sid, round, responder, _)| *sid == duty.session_id && *round == move_round && *responder == duty.i_am_responder)
            {
                continue;
            }
            // **ADR-0152 §4-ter N3: a held dissection takes the held route** — the windowed
            // builders (N1 for the responder, N2 for a challenger), built off the tick, and the
            // held root claim (tag 57) as move 1 — never the dense builder below, which a held
            // job is past. Below `palw_offence_attribution` no duty takes it.
            let held_host = held_court::PalwPanelHeldHostV1 {
                panel: self,
                session: &session,
                network_domain,
                bond: bond_key,
                materials: pool_books.material.map(),
                open_claims: &HashMap::new(),
            };
            // An IR class's history dissection is F7's (`tir_dissect`, below) — never the attention
            // held route's, whose windowed builders an IR program has none of.
            if session.palw_tir_class_record_v1(duty.class_id).is_none() && held_court.routes_v1(&held_host, duty, current_daa) {
                held_duties.push(duty.clone());
                continue;
            }
            // A partial public challenger continues its local history court before the
            // full-model resolver. This consumer never needs a whole-model backend.
            if !duty.i_am_responder
                && !self.config.tir_shard_shadow
                && (self.config.tir_shard_fetch || self.config.tir_shard_mirror.is_some())
                && self.consensus_config.params.palw_tir_shard_active_at(current_daa)
                && duty.tir_dissection.is_some()
                && let Some(mv) = tir_dissect::palw_tir_dissect_move_of_duty_v1(duty)
            {
                match self
                    .tir_shard_public_court_move_v1(
                        &session,
                        network_domain,
                        current_daa,
                        duty,
                        mv,
                        pool_books.material.map(),
                        pool_books.intervals.resume_mut(),
                        &mut shard_books,
                    )
                    .await
                {
                    Ok(Some(object)) => {
                        court_outbox.set_due((duty.session_id, move_round, false), palw_court_move_due_v1(duty));
                        court_outbox.push_pending((duty.session_id, move_round, false, object));
                    }
                    Ok(None) => {
                        *court_stalls.entry("the partial public bottom does not win").or_default() += 1;
                    }
                    Err(why) => {
                        *court_stalls.entry("the partial public court waits for evidence").or_default() += 1;
                        crate::palw_backends::note_throttled_v1(&format!("tir-partial-court-{}", duty.session_id), || {
                            format!("[{PALW_PANEL}] session {}: partial public court waits ({why})", duty.session_id)
                        });
                    }
                }
                continue;
            }
            // The capture, and the family's backend for it. A party with no material — or a
            // family with no court — cannot answer honestly, and answering dishonestly is what
            // the terminal close exists to punish, so it stays silent and lets the clock decide.
            let mut backend = match self.resolve_backend(&session, duty.class_id, duty.artifact_root) {
                Ok(backend) => backend,
                Err(why) => {
                    // Not rate-limited by session: this one is a NODE-level misconfiguration
                    // (no worker, wrong model, unservable class), identical for every session,
                    // so it is counted per tick like the rest and named once here.
                    *court_stalls.entry("no backend for the class").or_default() += 1;
                    trace!("[{PALW_PANEL}] session {} cannot resolve a backend: {why}", duty.session_id);
                    continue;
                }
            };
            // **This node's own retained copy, when the pool no longer has it.**
            //
            // `retention_dir` was declared, documented and wired into the config — and never
            // read. The in-memory pool drops a claim once it licenses, which is strictly
            // BEFORE a dispute can start: a challenger has to re-execute the job before it
            // knows there is anything to dispute, so by the time the court duty appears the
            // producer's own capture is gone from memory. The producer then depended on
            // hearing itself through `rebroadcast_retained`, whose 60-second burst try_sends
            // hundreds of files into a 256-slot inbox and silently drops the overflow, so
            // which claims it could answer about was decided by directory order.
            //
            // Measured across two drills with the field unread: 143 sessions opened / 4
            // answered, then 357 opened / 3 answered — the same shape both times.
            //
            // The obligation to keep these bytes already exists and is already on disk
            // (`trace_retention_daa`); this is the panel reading it rather than a second copy
            // of the same promise. Read once and put in the pool, because a court duty makes
            // the claim `live` — so the retention rule below keeps it for the session's life
            // and the next tick costs nothing. Still gated by `verify_material`: a file is
            // evidence only if it reproduces the roots the claim committed to.
            // **Selecting evidence, not judging a producer** — so an anchor this node cannot
            // derive degrades to the roots-only check here rather than skipping the duty.
            // These are the responder's OWN captures: refusing to load them because the block
            // header is momentarily unreadable would make the node silent at its own court,
            // and silence at the opening rung now costs it the claim. The judging site (the
            // receipt verdict below) takes the opposite branch, and correctly: there an
            // underivable anchor means "no opinion", because verifying without it is exactly
            // what let borrowed material pass.
            // **A free-prompt claim's anchor is its job id, and its prompt is the user's**
            // (ADR-0073 Decision 1b/1c). Both come off the claim's JOB material — the question
            // the user fixed on chain — never off the block, which asked none. `None` for an
            // attempt, whose anchor the block derives as before.
            let fp_job = if duty.free_prompt {
                let pooled = pool_books.material.map().get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]);
                let Some(job) = self.fp_job_material_for_claim(&duty.claim_id, duty.class_id, &duty.executor_bond, pooled) else {
                    *court_stalls.entry("a free-prompt session whose job material this node has not heard").or_default() += 1;
                    continue;
                };
                if Self::fp_prompt_for_job(
                    kaspa_consensus_core::palw_backend::PalwExecutionOpsV1::new(backend.as_ref()),
                    &job,
                    self.class_prompt_ids_form(job.job.class_id),
                )
                .is_none()
                {
                    *court_stalls.entry("a canonical claim whose material is not its own prompt").or_default() += 1;
                    continue;
                }
                Some(job)
            } else {
                None
            };
            // An attempt's material answers the whole job its block asked for (ADR-0117); a
            // free-prompt claim's job is its own.
            let attempt_draw = if fp_job.is_some() { None } else { self.attempt_draw_for_claim(&session, duty.accepted_block) };
            let anchor = match &fp_job {
                Some(job) => kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(&job.job),
                None => self
                    .job_anchor_for_claim(
                        &session,
                        backend.as_ref(),
                        network_domain,
                        duty.accepted_block,
                        duty.class_id,
                        &duty.executor_bond,
                    )
                    .unwrap_or_default(),
            };
            // `output_root: None`: a court duty names the claim's two roots and no output root
            // (`PalwCourtDutyV2`), and the court selects the step leg's evidence — the answer is
            // not what it adjudicates here, and no family's `verify_material` reads it (SEAT-S2).
            let roots = PalwClaimRootsV1 {
                execution_root: duty.execution_root,
                trace_root: duty.trace_root,
                anchor,
                attempt_draw,
                output_root: None,
                job_pin: None,
            };
            let pool_has_it = pool_books
                .material
                .map()
                .get(&duty.claim_id)
                .map(|pool| {
                    pool.iter().any(|b| {
                        fp_verify_material_v1(
                            kaspa_consensus_core::palw_backend::PalwReplayOpsV1::new(backend.as_ref()),
                            b,
                            self.class_prompt_ids_form(duty.class_id),
                            roots,
                        ) == PalwMaterialVerdictV1::Matches
                    })
                })
                .unwrap_or(false);
            let mut accused_held = pool_has_it;
            if !accused_held
                && let Some(bytes) = self.retained_capture(&duty.claim_id)
                && fp_verify_material_v1(
                    kaspa_consensus_core::palw_backend::PalwReplayOpsV1::new(backend.as_ref()),
                    &bytes,
                    self.class_prompt_ids_form(duty.class_id),
                    roots,
                ) == PalwMaterialVerdictV1::Matches
            {
                info!(
                    "[{PALW_PANEL}] session {} answered from this node's retained capture for claim {}",
                    duty.session_id, duty.claim_id
                );
                pool_books.material.admit(duty.claim_id, bytes);
                accused_held = true;
            }
            // **The responder's own attempt capture, pruned, is re-made — not missed.** The
            // producer keeps an attempt capture for minutes (`retained_capture_prune_due_v1`), and
            // silence at its own court now costs the claim; the capture is a deterministic
            // execution of the job the block asked for, so the responder replays that job — the
            // same one a seat licenses by — and answers only if the bytes reproduce the committed
            // roots. Put in the pool, it stays for the session's life like a file read would.
            if !accused_held && duty.i_am_responder && fp_job.is_none() {
                let work = self
                    .attempt_job_for_claim(
                        &session,
                        backend.as_ref(),
                        network_domain,
                        duty.accepted_block,
                        duty.class_id,
                        &duty.executor_bond,
                    )
                    .map(|(job, prompt)| ReplayWork::Attempt(job, prompt));
                let remade = match work {
                    Some(work) => {
                        let Ok((offloaded, outcome)) = offload(backend, move |b| work.run(b)).await else {
                            *court_stalls.entry("the responder's re-execution task did not finish").or_default() += 1;
                            continue;
                        };
                        backend = offloaded;
                        outcome.map(|outcome| outcome.material)
                    }
                    None => None,
                };
                match remade {
                    Some(bytes)
                        if fp_verify_material_v1(
                            kaspa_consensus_core::palw_backend::PalwReplayOpsV1::new(backend.as_ref()),
                            &bytes,
                            self.class_prompt_ids_form(duty.class_id),
                            roots,
                        ) == PalwMaterialVerdictV1::Matches =>
                    {
                        info!(
                            "[{PALW_PANEL}] session {}: claim {}'s capture was pruned here — re-made by replaying its block's job",
                            duty.session_id, duty.claim_id
                        );
                        pool_books.material.admit(duty.claim_id, bytes);
                    }
                    _ => {
                        warn!(
                            "[{PALW_PANEL}] session {}: claim {}'s capture is not held and a replay of its block's job \
                             did not reproduce the committed roots",
                            duty.session_id, duty.claim_id
                        );
                        *court_stalls.entry("the responder cannot re-make its pruned capture").or_default() += 1;
                    }
                }
            }
            // **The capture is chosen by ROLE** (audit M2-4).
            //
            // The responder speaks for the accused execution, so it answers from the capture
            // that reproduces the claim's committed roots. The CHALLENGER speaks for its own
            // — the execution whose roots differed, which is the entire content of its
            // accusation — so it answers from that. Feeding one capture to both sides made
            // `agree` true at every rung by construction (`bisect_prefix_state` is a pure
            // function of the bytes and the index), the interval walked to the last leaf of a
            // 2^22 space, and no close could be assembled by anyone: fraud escaped, and an
            // attacker who merely agreed 22 times had the honest producer convicted by the
            // backstop for 23 transaction fees.
            //
            // A challenger that has lost its own execution (a restart) re-runs the job here
            // rather than borrowing the accused's — silence costs it the dispute, which is the
            // right price for an accusation it can no longer support.
            if !duty.i_am_responder && !own_executions.contains_key(&duty.claim_id) {
                // The job that was CLAIMED: the user's for a free prompt (ADR-0073 Decision
                // 1d), the block's for an attempt.
                let work = match &fp_job {
                    Some(job) => {
                        Some(ReplayWork::FreePrompt(job.job.clone(), job.prompt_token_ids.iter().map(|t| *t as usize).collect()))
                    }
                    None => self
                        .attempt_job_for_claim(
                            &session,
                            backend.as_ref(),
                            network_domain,
                            duty.accepted_block,
                            duty.class_id,
                            &duty.executor_bond,
                        )
                        .map(|(job, prompt)| ReplayWork::Attempt(job, prompt)),
                };
                let rerun = match work {
                    Some(work) => {
                        let Ok((offloaded, outcome)) = offload(backend, move |b| work.run(b)).await else {
                            *court_stalls.entry("the challenger's re-execution task did not finish").or_default() += 1;
                            continue;
                        };
                        backend = offloaded;
                        outcome
                    }
                    None => None,
                };
                match rerun {
                    Some(outcome) => {
                        own_executions.insert(duty.claim_id, outcome.material);
                    }
                    None => {
                        *court_stalls.entry("the challenger cannot re-execute its own accusation").or_default() += 1;
                        continue;
                    }
                }
            }
            // **The ACCUSED execution — the bytes that reproduce the claim's committed roots.**
            //
            // Role-splitting the capture is right for the rungs and wrong for the terminal move
            // (audit3 S-01). `refutation_for_index` derives the refutation's binding FROM the
            // material it is handed, and `adjudicate_court_close_v2` pins that binding to the
            // accused claim before it reads any evidence:
            //
            //     check_arithmetic_close_binding(claim.trace_root, binding_logits_root_of(..))?;
            //     check_execution_root_binding(claim.execution_root, ..committed_execution_root)?;
            //
            // A challenger only reaches `Terminal` because its roots DIFFER from the claim's —
            // that difference is the accusation. So a close assembled from the challenger's own
            // capture carries the challenger's roots, fails the first binding check, and
            // `palw_court_close_verdict_v2` returns `None`. The challenger files nothing; the
            // responder is the fraudster and will not convict itself; no rung clock runs at
            // `Terminal`; and the `window_court` backstop then slashes the CHALLENGER's
            // collateral and re-arms the fraudulent claim, which finalizes and is paid. Proven
            // fraud became unpunishable, and the only party that can detect it paid to report
            // it.
            //
            // The backend's own contract says which capture the close comes from: "Returned by
            // BOTH sides, and deliberately the same call for both: an honest executor closing
            // its own case and a challenger closing a real fraud assemble the identical
            // object." Identical objects require identical material, and the material both
            // sides can name is the accused's.
            let accused_capture: Option<std::borrow::Cow<'_, [u8]>> = pool_books.material.map().get(&duty.claim_id).and_then(|pool| {
                pool.iter()
                    .map(|b| fp_capture_view(b, self.class_prompt_ids_form(duty.class_id)))
                    .find(|c| backend.verify_material(c, roots) == PalwMaterialVerdictV1::Matches)
            });
            let capture_from_own: Option<std::borrow::Cow<'_, [u8]>> = (!duty.i_am_responder)
                .then(|| own_executions.get(&duty.claim_id).map(|v| std::borrow::Cow::Borrowed(v.as_slice())))
                .flatten();
            let Some(capture) = capture_from_own.or_else(|| accused_capture.as_deref().map(std::borrow::Cow::Borrowed)) else {
                *court_stalls
                    .entry(if pool_books.material.map().contains_key(&duty.claim_id) {
                        // Held material, and none of it reproduces the claim's roots — a
                        // different failure from holding none, and the one that means the pool
                        // is carrying somebody else's bytes for this claim.
                        "capture held but none matches the claim's roots"
                    } else {
                        "no capture for the claim"
                    })
                    .or_default() += 1;
                trace!("[{PALW_PANEL}] session {} needs a move but this node holds no matching capture", duty.session_id);
                continue;
            };
            // ---- RFC-0002 F7: an IR class's history dissection ---------------------------------
            //
            // At a dissected leaf of an IR class the responder owes the IR root claim, then a round
            // per disclosure; the challenger names a child per round; at the bottom a close finishes
            // it (`palw_panel::tir_dissect`). Built off the tick from the party's own capture (the
            // responder's is the claim's; a challenger's its own execution) and, for the bottom, the
            // ACCUSED capture; signed here, and queued as every court move is.
            if let Some(mv) = tir_dissect::palw_tir_dissect_move_of_duty_v1(duty)
                && let Some(tir) = self.resolve_tir_registered_v1(session, duty.class_id, duty.artifact_root)
            {
                let tir = match tir {
                    Ok(tir) => tir,
                    Err(why) => {
                        *court_stalls.entry("the IR backend does not build for the class").or_default() += 1;
                        warn!("[{PALW_PANEL}] session {}: {why}", duty.session_id);
                        continue;
                    }
                };
                let Some((rules, arity)) = self.tir_dissection_rules_v1(&tir, duty.class_id, current_daa) else {
                    *court_stalls.entry("no V2 court parameters for an IR dissection").or_default() += 1;
                    continue;
                };
                let name = tir_dissect::palw_tir_dissect_move_name_v1(mv);
                // **Option D: a challenger's bottom without the accused capture** — the leaf the
                // accused's root claim carries, read off the chain (a large class's capture never
                // reaches a seat). Looked up once, re-looked on the attention route's throttle.
                let accused_root = if mv == tir_dissect::PalwTirDissectMoveV1::Close && accused_capture.is_none() {
                    match tir_root_filings.get(&duty.session_id) {
                        Some((_, Some(root))) => Some(root.clone()),
                        Some((looked, None)) if current_daa < looked.saturating_add(25) => None,
                        _ => {
                            let window = match &self.consensus_config.params.palw_consensus_mode {
                                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                                    bundle.state.window_court()
                                }
                                _ => 0,
                            };
                            let not_before = duty.session_deadline_daa.saturating_sub(window);
                            let span = current_daa.saturating_sub(not_before).saturating_add(64).min(1 << 16) as usize;
                            let sid = duty.session_id;
                            let filed =
                                session.clone().spawn_blocking(move |c| tir_root_claims_from_chain_v1(c, sid, not_before, span)).await;
                            let root = filed.into_iter().find(|root| {
                                root.finalize.binding.committed_execution_root == duty.execution_root
                                    && duty
                                        .tir_dissection
                                        .as_ref()
                                        .is_some_and(|p| p.leaf_index() == root.finalize.output_opening.leaf_index)
                            });
                            tir_root_filings.insert(duty.session_id, (current_daa, root.clone()));
                            if root.is_some() {
                                info!(
                                    "[{PALW_PANEL}] session {}: no accused capture here — the dissection's bottom is built from the \
                                     accused's root claim on chain (RFC-0002 evidence transport D)",
                                    duty.session_id
                                );
                            }
                            root
                        }
                    }
                } else {
                    None
                };
                let (own, accused, task_duty) = (capture.to_vec(), accused_capture.as_deref().map(|c| c.to_vec()), duty.clone());
                let built = tokio::task::spawn_blocking(move || {
                    tir_dissect::palw_tir_dissect_build_v1(
                        &tir,
                        &task_duty,
                        mv,
                        &own,
                        accused.as_deref(),
                        accused_root.as_ref(),
                        &rules,
                    )
                })
                .await;
                let built = match built {
                    Ok(Ok(built)) => built,
                    Ok(Err(why)) => {
                        *court_stalls.entry("an IR dissection move does not build").or_default() += 1;
                        crate::palw_backends::note_throttled_v1(&format!("tir-dissect-{}-{name}", duty.session_id), || {
                            format!("[{PALW_PANEL}] session {}: the IR dissection's {name} does not build: {why}", duty.session_id)
                        });
                        continue;
                    }
                    Err(_) => {
                        *court_stalls.entry("the IR dissection move's task did not finish").or_default() += 1;
                        continue;
                    }
                };
                let object = tir_dissect::palw_tir_dissect_object_v1(
                    built,
                    duty,
                    arity,
                    &|message, context| self.sign(message, context),
                    &|sid, proof| session.palw_court_close_verdict_v2(sid, proof),
                );
                let object = match object {
                    Ok(Some(object)) => object,
                    Ok(None) => {
                        *court_stalls.entry("the IR bottom does not win this party's side").or_default() += 1;
                        continue;
                    }
                    Err(why) => {
                        *court_stalls.entry("an IR dissection move cannot be filed").or_default() += 1;
                        crate::palw_backends::note_throttled_v1(&format!("tir-dissect-{}-{name}", duty.session_id), || {
                            format!("[{PALW_PANEL}] session {}: the IR dissection's {name}: {why}", duty.session_id)
                        });
                        continue;
                    }
                };
                info!("[{PALW_PANEL}] session {}: filing the IR dissection's {name} (round {})", duty.session_id, duty.round);
                court_outbox.set_due((duty.session_id, move_round, duty.i_am_responder), palw_court_move_due_v1(duty));
                court_outbox.push_pending((duty.session_id, move_round, duty.i_am_responder, object));
                continue;
            }
            // ---- ADR-0093 as built: the fused site's dissection -------------------------------
            //
            // At a fused terminal the ladder's close is not the move: the responder owes a ROOT
            // CLAIM (the fold is already clocking it as `AwaitDisclosure`), then one round per
            // disclosure; the challenger names a child per round; at the bottom a close finishes
            // it. Every number is computed from the capture's evidence by the court's own
            // kernels (`palw_attn_responder_v1`) — the responder from its own capture, the
            // challenger's choices from its own execution, the bottom from the ACCUSED capture,
            // whose commitments are what the court opens.
            if duty.dissection.is_some() || (duty.fused_class && duty.terminal_index.is_some()) {
                use kaspa_consensus_core::palw_attn_court_v1::{PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnDissectChoiceV1};
                use kaspa_consensus_core::palw_court_v2::{
                    PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT,
                    palw_attn_root_claim_message_v1, palw_attn_round_message_v1,
                };
                #[derive(Clone, Copy, PartialEq, Eq)]
                enum AttnMove {
                    Root,
                    Round,
                    Choice,
                    Close,
                }
                let Some(narrowed) = duty.terminal_index else {
                    *court_stalls.entry("a dissection whose ladder names no leaf").or_default() += 1;
                    continue;
                };
                let mv = match (duty.dissection.as_ref(), duty.i_am_responder, duty.turn) {
                    (None, true, PalwBisectTurnV1::AwaitDisclosure) => AttnMove::Root,
                    (Some(_), true, PalwBisectTurnV1::AwaitDisclosure) => AttnMove::Round,
                    (Some(_), false, PalwBisectTurnV1::AwaitVerdict) => AttnMove::Choice,
                    (Some(_), _, PalwBisectTurnV1::Terminal) => AttnMove::Close,
                    _ => {
                        *court_stalls.entry("waiting — the dissection's move is the other party's").or_default() += 1;
                        continue;
                    }
                };
                let memory_route =
                    self.consensus_config.params.palw_memory_routing_court_v2_fence().is_some_and(|f| f.is_active(current_daa));
                if !backend.supports_dissection_with_memory_routing_v2(memory_route) {
                    *court_stalls
                        .entry("this class's family cannot take a dissection's turn in this build — the clock will decide")
                        .or_default() += 1;
                    continue;
                }
                // Which capture this move speaks from: the bottom is the accused's, every other
                // move the party's own.
                let from_accused = mv == AttnMove::Close || duty.i_am_responder;
                let key = (duty.session_id, from_accused);
                // **The bottom is opened against the ACCUSED's commitments** (ADR-0093 Decisions 7
                // and 8). What no capture of this node's can give is the accused's own output
                // tile (a fold keeps no rows) and, when its execution followed its lie, the path
                // to its anchor — so the close reads them where the accused was made to file
                // them: its root claim, on chain. Only a filing whose tile proves against THIS
                // claim's root at THIS leaf is kept; the evidence re-checks tile and anchor
                // against the accused's binding before a row is opened.
                let accused_filing = if mv == AttnMove::Close {
                    match attn_root_filings.get(&duty.session_id) {
                        Some((_, Some(filing))) => Some(filing.clone()),
                        Some((looked, None)) if current_daa < looked.saturating_add(25) => None,
                        _ => {
                            let params = &self.consensus_config.params;
                            let (window, cap) = match &params.palw_consensus_mode {
                                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                                    (bundle.state.window_court(), bundle.court.max_step_leaf_count())
                                }
                                _ => (0, 0),
                            };
                            let not_before = duty.session_deadline_daa.saturating_sub(window);
                            let span = current_daa.saturating_sub(not_before).saturating_add(64).min(1 << 16) as usize;
                            let sid = duty.session_id;
                            let filed = session
                                .clone()
                                .spawn_blocking(move |c| attn_root_filings_from_chain_v1(c, sid, not_before, span))
                                .await;
                            let execution_root = duty.execution_root;
                            let filing = filed.into_iter().find(|filing| {
                                filing.binding.committed_execution_root == execution_root
                                    && filing.out_tile.opening.leaf_index == narrowed
                                    && kaspa_consensus_core::palw_step_leg::step_opening_root_capped_v1(
                                        filing.binding.step_leaf_count,
                                        &filing.out_tile.opening,
                                        // The claim's ladder, off the class its binding names
                                        // (ADR-0119 Decision 4): a held filing's path is deeper.
                                        kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(
                                            cap,
                                            &filing.binding.shape_profile,
                                        ),
                                    )
                                    .is_ok_and(|root| root == filing.binding.step_merkle_root)
                            });
                            attn_root_filings.insert(duty.session_id, (current_daa, filing.clone()));
                            filing
                        }
                    }
                } else {
                    None
                };
                // The capture this move reads: the party's own, or — for the close — the accused's
                // when held. A close whose accused capture is not held still asks for it (a fold's
                // own leaves reach further than a filing alone) but proceeds on the filing: an
                // over-cap capture is never served, and waiting on it was the backstop's win for a
                // forger.
                let source: Option<Vec<u8>> = if mv == AttnMove::Close {
                    match accused_capture.as_deref() {
                        Some(accused) => Some(accused.to_vec()),
                        None => {
                            if requested.get(&duty.claim_id).is_none_or(|at| current_daa >= at.saturating_add(25)) {
                                requested.insert(duty.claim_id, current_daa);
                                pull_for_close.push(duty.claim_id);
                            }
                            if accused_filing.is_none() {
                                *court_stalls
                                    .entry("the dissection's bottom needs the accused's capture or its filing — pulling, reading the chain")
                                    .or_default() += 1;
                                continue;
                            }
                            None
                        }
                    }
                } else {
                    Some(capture.to_vec())
                };
                let evidence = match attn_evidence.get(&key) {
                    Some(held) => held.clone(),
                    None => {
                        let carried: Option<Vec<u32>> = fp_job.as_ref().map(|job| job.prompt_token_ids.clone());
                        let Ok((_offloaded, built)) = offload(backend, move |b| {
                            // The capture's own rows first (a fold's with the filed tile); where that
                            // cannot bottom the accused — or there is no capture — the filing alone.
                            let from_capture = source.as_ref().map(|bytes| {
                                b.attn_site_evidence(bytes, narrowed, carried.as_deref(), accused_filing.as_ref().map(|f| &f.out_tile))
                            });
                            match (from_capture, accused_filing.as_ref()) {
                                (Some(Ok(evidence)), _) => Ok(evidence),
                                (Some(Err(why)), Some(filing)) => b
                                    .attn_site_evidence_from_filing(filing, narrowed, carried.as_deref())
                                    .map_err(|e| format!("{why}; and from the accused's filing: {e}")),
                                (Some(Err(why)), None) => Err(why),
                                (None, Some(filing)) => b.attn_site_evidence_from_filing(filing, narrowed, carried.as_deref()),
                                (None, None) => Err("no capture and no filing to read the fused site from".to_string()),
                            }
                        })
                        .await
                        else {
                            *court_stalls.entry("the dissection evidence task did not finish").or_default() += 1;
                            continue;
                        };
                        match built {
                            Ok(evidence) => {
                                let evidence = Arc::new(evidence);
                                attn_evidence.insert(key, evidence.clone());
                                evidence
                            }
                            Err(why) => {
                                *court_stalls.entry("this capture yields no evidence for the fused site").or_default() += 1;
                                warn!("[{PALW_PANEL}] session {}: the fused site's evidence: {why}", duty.session_id);
                                continue;
                            }
                        }
                    }
                };
                let site = match evidence.site_v1(duty.artifact_root, false) {
                    Ok(site) => site,
                    Err(why) => {
                        *court_stalls.entry("the fused site does not derive from the evidence").or_default() += 1;
                        warn!("[{PALW_PANEL}] session {}: {why}", duty.session_id);
                        continue;
                    }
                };
                let object = match mv {
                    AttnMove::Root => {
                        let root = match evidence.root_claim_v1(&site) {
                            Ok(root) => root,
                            Err(why) => {
                                *court_stalls.entry("the root claim does not compute from the evidence").or_default() += 1;
                                warn!("[{PALW_PANEL}] session {}: {why}", duty.session_id);
                                continue;
                            }
                        };
                        let params = &self.consensus_config.params;
                        if !attn_root_claim_is_openable_v1(params, current_daa, evidence.binding.step_leaf_count) {
                            *court_stalls
                                .entry("the court opens a fused site's rows only to 2^22 before the held regime")
                                .or_default() += 1;
                            continue;
                        }
                        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode
                        else {
                            continue;
                        };
                        // The arity the ACCEPTANCE layer derives and refuses any other than: the
                        // processor's `palw_court_params_at` is the held derivation wherever the held
                        // regime is in force (testnet-12 from genesis: 4 where the plain derivation
                        // finds none; testnet-11: 2 either side of its regime's height).
                        let Ok(court) = kaspa_consensus_core::palw_court_v2::palw_court_params_held_at_v2(
                            bundle,
                            params.palw_kary_court_active_at(current_daa),
                            params.palw_held_context_active_at(current_daa),
                        ) else {
                            *court_stalls.entry("this ruleset's court has no shape for a dissection").or_default() += 1;
                            continue;
                        };
                        let message = palw_attn_root_claim_message_v1(&duty.session_id, &root);
                        let Some(signature) = self.sign(&message, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT) else {
                            *court_stalls.entry("no signing key for a root claim").or_default() += 1;
                            continue;
                        };
                        info!(
                            "[{PALW_PANEL}] session {}: answering the fused leaf {narrowed} with a root claim over {} positions (head {}, lanes {}+{})",
                            duty.session_id, root.history_positions, root.head, root.lane_first, root.lane_count
                        );
                        // ADR-0093 Decision 8: past its fence, move 1 carries the anchor its
                        // bottom will read — the one path a challenger cannot rebuild once an
                        // execution followed its lie. Where the site reads no checkpoint the
                        // evidence carries none, and the plain form is the right one.
                        let anchor = evidence
                            .anchor
                            .as_ref()
                            .filter(|_| params.palw_attn_anchored_root_active_at(current_daa))
                            .map(|anchored| Box::new(anchored.anchor.clone()));
                        match anchor {
                            Some(anchor) => PalwConsensusObjectV2::CourtAttnRootClaimedAnchored {
                                session_id: duty.session_id,
                                root,
                                arity: court.dissection_arity(),
                                binding: Box::new(evidence.binding.clone()),
                                out_tile: evidence.out_tile.clone(),
                                anchor,
                                operand_openings: evidence.operand_openings.clone(),
                                signature,
                            },
                            None => PalwConsensusObjectV2::CourtAttnRootClaimed {
                                session_id: duty.session_id,
                                root,
                                arity: court.dissection_arity(),
                                binding: Box::new(evidence.binding.clone()),
                                out_tile: evidence.out_tile.clone(),
                                operand_openings: evidence.operand_openings.clone(),
                                signature,
                            },
                        }
                    }
                    AttnMove::Round => {
                        let Some(phase) = duty.dissection.as_ref() else { continue };
                        let round = match evidence.round_v1(&site, phase) {
                            Ok(round) => round,
                            Err(why) => {
                                *court_stalls.entry("the round does not compute from the evidence").or_default() += 1;
                                warn!("[{PALW_PANEL}] session {}: {why}", duty.session_id);
                                continue;
                            }
                        };
                        let message = palw_attn_round_message_v1(&duty.session_id, phase.round(), &round);
                        let Some(signature) = self.sign(&message, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT) else {
                            *court_stalls.entry("no signing key for a dissection round").or_default() += 1;
                            continue;
                        };
                        PalwConsensusObjectV2::CourtAttnDissected { session_id: duty.session_id, round, signature }
                    }
                    AttnMove::Choice => {
                        let Some(phase) = duty.dissection.as_ref() else { continue };
                        let child = match evidence.divergent_child_v1(&site, phase) {
                            Ok(Some(child)) => child,
                            Ok(None) => {
                                // Every child reproduces: an honest disclosure, and no choice wins.
                                // Silence lets the rung clock end it on this side, which is what an
                                // accusation the dissection does not support deserves.
                                *court_stalls.entry("the responder's disclosure reproduces — nothing to name").or_default() += 1;
                                continue;
                            }
                            Err(why) => {
                                *court_stalls.entry("the challenger cannot recompute the children").or_default() += 1;
                                warn!("[{PALW_PANEL}] session {}: {why}", duty.session_id);
                                continue;
                            }
                        };
                        let choice = PalwAttnDissectChoiceV1 {
                            version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                            session_id: duty.session_id,
                            round: phase.round(),
                            child,
                        };
                        let message = borsh::to_vec(&choice).expect("a dissection choice is borsh-serializable");
                        let Some(signature) = self.sign(&message, PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT) else {
                            *court_stalls.entry("no signing key for a dissection choice").or_default() += 1;
                            continue;
                        };
                        info!(
                            "[{PALW_PANEL}] session {}: naming child {child} of round {} — the responder's claim does not reproduce there",
                            duty.session_id,
                            phase.round()
                        );
                        PalwConsensusObjectV2::CourtAttnChildChosen { session_id: duty.session_id, choice, signature }
                    }
                    AttnMove::Close => {
                        let Some(phase) = duty.dissection.as_ref() else { continue };
                        let bottom =
                            match evidence.site_v1(duty.artifact_root, true).and_then(|anchored| evidence.bottom_v1(&anchored, phase))
                            {
                                Ok(bottom) => bottom,
                                Err(why) => {
                                    *court_stalls.entry("the dissection's bottom does not assemble").or_default() += 1;
                                    warn!("[{PALW_PANEL}] session {}: {why}", duty.session_id);
                                    continue;
                                }
                            };
                        let proof = kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2::AttnDissection {
                            binding: Box::new(evidence.binding.clone()),
                            bottom: Box::new(bottom),
                            operand_openings: evidence.operand_openings.clone(),
                        };
                        let Some(verdict) = session.palw_court_close_verdict_v2(&duty.session_id, &proof) else {
                            *court_stalls.entry("the chain reads no verdict from this bottom").or_default() += 1;
                            continue;
                        };
                        // A party closes only the case it wins: the challenger a conviction, the
                        // responder an acquittal. The other outcome needs no fee to arrive — the
                        // backstop ends an unproven accusation on the challenger's side anyway.
                        let mine = if duty.i_am_responder {
                            verdict == kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ChallengerDefeated
                        } else {
                            verdict == kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty
                        };
                        if !mine {
                            *court_stalls.entry("the bottom's verdict is the other party's").or_default() += 1;
                            continue;
                        }
                        info!(
                            "[{PALW_PANEL}] session {} closes its dissection as {verdict:?} at tile {}",
                            duty.session_id,
                            phase.terminal_tile().unwrap_or(0)
                        );
                        PalwConsensusObjectV2::CourtClosed { session_id: duty.session_id, verdict, proof }
                    }
                };
                court_outbox.set_due((duty.session_id, move_round, duty.i_am_responder), palw_court_move_due_v1(duty));
                court_outbox.push_pending((duty.session_id, move_round, duty.i_am_responder, object));
                continue;
            }
            let object = match (duty.turn, duty.i_am_responder) {
                // Our disclosure: the state of OUR execution at the midpoint the ladder asks
                // about. A prefix commitment, so agreeing at an index means agreeing before it.
                (PalwBisectTurnV1::AwaitDisclosure, true) => {
                    let Some(midpoint) = duty.midpoint else {
                        *court_stalls.entry("the interval has no midpoint to disclose").or_default() += 1;
                        continue;
                    };
                    let Some(mid_state) = backend.bisect_prefix_state(&capture, midpoint) else {
                        // Two different facts wearing one message (audit3 H4). "This capture
                        // has no state at that index" is a bad capture; "this FAMILY has no
                        // rung move at all" is a class whose disputes can never be decided by
                        // anybody, and an operator reading a stalled court needs to know which
                        // one it is looking at.
                        *court_stalls
                            .entry(if backend.supports_court() {
                                "the backend cannot state its prefix at the midpoint"
                            } else {
                                "this class has NO court responder in this build — no party can move, and the dispute cannot be decided"
                            })
                            .or_default() += 1;
                        continue;
                    };
                    let disclosure = PalwBisectDisclosureV1 {
                        version: PALW_BISECT_OBJECT_VERSION_V1,
                        session_id: duty.session_id,
                        round: duty.round,
                        midpoint,
                        mid_state,
                    };
                    let message = borsh::to_vec(&disclosure).expect("a disclosure is borsh-serializable");
                    let Some(signature) = self.sign(&message, PALW_COURT_V2_MLDSA87_DISCLOSURE_CONTEXT) else {
                        *court_stalls.entry("no signing key for a disclosure").or_default() += 1;
                        continue;
                    };
                    Some(PalwConsensusObjectV2::CourtDisclosed { session_id: duty.session_id, disclosure, signature })
                }
                // Our verdict: does the responder's disclosed state match ours at that index?
                // `agree` means the prefix matches, so the divergence is ABOVE the midpoint.
                (PalwBisectTurnV1::AwaitVerdict, false) => {
                    let Some(disclosed) = duty.last_disclosure else {
                        *court_stalls.entry("no disclosure to post a verdict about").or_default() += 1;
                        continue;
                    };
                    let Some(ours) = backend.bisect_prefix_state(&capture, disclosed.0) else {
                        *court_stalls.entry("the backend cannot state its prefix at the disclosed index").or_default() += 1;
                        continue;
                    };
                    let verdict = PalwBisectVerdictV1 {
                        version: PALW_BISECT_OBJECT_VERSION_V1,
                        session_id: duty.session_id,
                        round: duty.round,
                        agree: ours == disclosed.1,
                    };
                    let message = borsh::to_vec(&verdict).expect("a verdict is borsh-serializable");
                    let Some(signature) = self.sign(&message, PALW_COURT_V2_MLDSA87_VERDICT_CONTEXT) else {
                        *court_stalls.entry("no signing key for a verdict").or_default() += 1;
                        continue;
                    };
                    Some(PalwConsensusObjectV2::CourtVerdictPosted { session_id: duty.session_id, verdict, signature })
                }
                // The terminal move. EITHER party may make it, and it is deliberately the SAME
                // call for both: an honest executor closing its own case and a challenger
                // closing a real fraud assemble the identical object, and
                // `adjudicate_court_close_v2` is what decides which way it reads. A prover only
                // one side could run would be a prover that decides the verdict.
                //
                // **STILL NOT COVERED BY A TEST.** The 2026-08-28 audit set the acceptance
                // condition for M2-4 as a live round trip that convicts a real fault AND
                // acquits a real innocent; the 2026-08-29 re-audit recorded that it was unmet
                // and that nothing in the tree tests it; it is still unmet here. The repair
                // below is verified by reading the adjudicator's binding checks, which is the
                // same standing M2-4 was recorded `fixed` at — so it is written down where the
                // code is rather than claimed in a table. The test needs a two-node court
                // harness that does not exist.
                (PalwBisectTurnV1::Terminal, _) => {
                    let Some(index) = duty.terminal_index else {
                        *court_stalls.entry("the ladder has not narrowed to a step").or_default() += 1;
                        continue;
                    };
                    // **RFC-0002 Phase F (F6): an IR class closes with an IR proof**, built by the
                    // IR backend from the ACCUSED capture — `TirCone` for either party, and at a
                    // logits tile the challenger's `TirLogits` and decode-token doors
                    // (`palw_panel::tir_court`). A party files only the close that wins its side.
                    if let Some(tir) = self.resolve_tir_registered_v1(session, duty.class_id, duty.artifact_root) {
                        let tir = match tir {
                            Ok(tir) => tir,
                            Err(why) => {
                                *court_stalls.entry("the IR backend does not build for the class").or_default() += 1;
                                warn!("[{PALW_PANEL}] session {}: {why}", duty.session_id);
                                continue;
                            }
                        };
                        let Some(accused) = accused_capture.as_deref().map(|c| c.to_vec()) else {
                            *court_stalls.entry("the IR close needs the ACCUSED capture and this node holds none").or_default() += 1;
                            continue;
                        };
                        let own = own_executions.get(&duty.claim_id).cloned();
                        let rules = tir.court_rules(&self.config.court);
                        let challenger = !duty.i_am_responder;
                        let Ok(candidates) = tokio::task::spawn_blocking(move || {
                            tir_court::palw_tir_close_candidates_v1(&tir, &accused, own.as_deref(), index, &rules, challenger)
                        })
                        .await
                        else {
                            *court_stalls.entry("the IR close's task did not finish").or_default() += 1;
                            continue;
                        };
                        let mut filed = None;
                        for (label, built) in candidates {
                            let proof = match built {
                                Ok(proof) => proof,
                                Err(why) => {
                                    trace!("[{PALW_PANEL}] session {}: the IR {label} close does not build: {why}", duty.session_id);
                                    continue;
                                }
                            };
                            let Some(verdict) = session.palw_court_close_verdict_v2(&duty.session_id, &proof) else { continue };
                            if tir_court::palw_tir_close_is_mine_v1(verdict, duty.i_am_responder) {
                                let note = tir_court::palw_tir_close_note_v1(&duty.session_id, label, verdict, index);
                                info!("[{PALW_PANEL}] {note}");
                                filed = Some(PalwConsensusObjectV2::CourtClosed { session_id: duty.session_id, verdict, proof });
                                break;
                            }
                        }
                        let Some(object) = filed else {
                            *court_stalls.entry("no IR close wins this party's side").or_default() += 1;
                            continue;
                        };
                        court_outbox.set_due((duty.session_id, move_round, duty.i_am_responder), palw_court_move_due_v1(duty));
                        court_outbox.push_pending((duty.session_id, move_round, duty.i_am_responder, object));
                        continue;
                    }
                    // The rungs speak from each party's OWN execution — that is what makes a
                    // disagreement possible at all. The close does not: it is an assertion
                    // about the accused's step, so it is assembled from the accused's bytes by
                    // whichever party is making it (audit3 S-01).
                    // **The close's source: the accused capture when held, else the served
                    // intervals** (ADR-0085 Decision 3). A free-prompt claim whose capture the
                    // transport refuses (ADR-0084) is closed from the interval carrying the
                    // step — served with its close annex, because the executor reads this
                    // session's terminal index off the chain (§6 item 4) — and the interval
                    // before it, whose tiles a step at an interval's first call reads.
                    enum CloseSource {
                        Capture(Vec<u8>),
                        Served { held: Vec<(u32, Vec<u8>)>, prompt_ids: Vec<u32>, output_ids: Vec<u32>, work_leaves: u64 },
                    }
                    let source = match accused_capture.as_deref() {
                        Some(accused) => CloseSource::Capture(accused.to_vec()),
                        None => {
                            let served = self.close_source_from_served_intervals_v1(
                                backend.as_ref(),
                                &duty.claim_id,
                                fp_job.as_ref(),
                                pool_books.material.map().get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]),
                                pool_books.intervals.map(),
                                index,
                            );
                            match served {
                                Ok((held, prompt_ids, output_ids, work_leaves)) => {
                                    CloseSource::Served { held, prompt_ids, output_ids, work_leaves }
                                }
                                Err(CloseFromServedV1::Missing(wanted)) => {
                                    let fresh: Vec<u32> = wanted
                                        .into_iter()
                                        .filter(|i| {
                                            requested_intervals
                                                .get(&(duty.claim_id, *i))
                                                .is_none_or(|at| current_daa >= at.saturating_add(25))
                                        })
                                        .collect();
                                    if !fresh.is_empty() {
                                        for i in &fresh {
                                            requested_intervals.insert((duty.claim_id, *i), current_daa);
                                        }
                                        intervals_for_close.push((duty.claim_id, fresh));
                                    }
                                    *court_stalls.entry("waiting for the interval's close annex").or_default() += 1;
                                    continue;
                                }
                                Err(CloseFromServedV1::NotThisLane) => {
                                    // **And ASK for it.** The pull lived only in the receipt-duty
                                    // loop, so a challenger that never needed the accused's bytes
                                    // to open its case — it compares roots, not material — had no
                                    // way to obtain them for the close. Same 25-DAA throttle and
                                    // the same solicited-answer registration the receipt path uses.
                                    if requested.get(&duty.claim_id).is_none_or(|at| current_daa >= at.saturating_add(25)) {
                                        requested.insert(duty.claim_id, current_daa);
                                        pull_for_close.push(duty.claim_id);
                                    }
                                    *court_stalls
                                        .entry("the close needs the ACCUSED capture and this node holds none — pulling")
                                        .or_default() += 1;
                                    trace!(
                                        "[{PALW_PANEL}] session {} has narrowed to a step but this node holds no capture matching the claim's roots",
                                        duty.session_id
                                    );
                                    continue;
                                }
                                Err(CloseFromServedV1::Refused(why)) => {
                                    *court_stalls.entry("the served intervals do not assemble a close").or_default() += 1;
                                    warn!("[{PALW_PANEL}] session {}: {why}", duty.session_id);
                                    continue;
                                }
                            }
                        }
                    };
                    // The user's prompt, hash-bound to the binding (ADR-0073 Decision 1c) — carried
                    // into the blocking task with a copy of the accused capture.
                    let carried_prompt: Option<Vec<u32>> = fp_job.as_ref().map(|job| job.prompt_token_ids.clone());
                    let roots_for_close = roots;
                    // **The close's replay bytes are RESERVED** (ADR-0151 follow-up, item 3): a
                    // close that resumes a segment or opens a leaf re-executes the class, priced
                    // here as a full seat — an upper bound over what the resume and the leaf's
                    // replay walk — and a court that cannot reserve waits for the next tick
                    // instead of dying mid-case with a session open against it.
                    //
                    // The executor's kept walk is released first, as the interval seat releases
                    // it: a close from served intervals walks the job on this duty's own
                    // instance, and under the process-wide slot that walk evicted the
                    // executor's idle one, so the two were never resident together.
                    self.release_executor_backend_v1();
                    // At the widest prefill run the ledger grants, on this duty's own instance —
                    // the one the close below runs on (int-10.2 A2).
                    let mut backend = backend;
                    let backends = self.backends();
                    let court_reserved =
                        match self.reserve_replay_at_widest_run_v1("court", backend.as_mut(), duty.class_id, duty.claim_id, |b| {
                            backends.role_memory_need_for_backend_or_chain_v1(
                                b,
                                duty.class_id,
                                duty.artifact_root,
                                None,
                                FULL_SEAT_V1,
                                |id| self.chain_carriage_v1(&session, id),
                            )
                        }) {
                            Ok((reserved, _)) => reserved,
                            Err(why) => {
                                *court_stalls.entry("the memory ledger cannot cover the close's replay").or_default() += 1;
                                crate::palw_backends::note_throttled_v1("panel-court-ledger", || {
                                    format!("[{PALW_PANEL}] session {}: the close waits — {why}", duty.session_id)
                                });
                                continue;
                            }
                        };
                    let Ok((_backend, assembled)) = offload(backend, move |b| {
                        let _held_for_the_close = court_reserved;
                        // **No segment resume before the refutation** (the audit's SEAT-S review,
                        // M1). The close resumed the accused segment (ADR-0133 S1 (4),
                        // `replay_accused_segment_v1`) and discarded what it found: nothing the
                        // chain reads came of it, and past SEAT-S4 a held fold's segment resumes
                        // from the prompt, so the close spent a whole replay before the per-leaf
                        // refutation it files — which recomputes the accused leaf itself.
                        let refutation = match &source {
                            CloseSource::Capture(accused_bytes) => match &carried_prompt {
                                Some(ids) => b.refutation_for_free_prompt_index(accused_bytes, index, ids),
                                None => b.refutation_for_index(accused_bytes, index),
                            },
                            CloseSource::Served { held, prompt_ids, output_ids, work_leaves } => {
                                b.refutation_from_served_intervals(held, roots_for_close, prompt_ids, output_ids, *work_leaves, index)
                            }
                        }
                        .map_err(|e| ("the close does not assemble from this capture", e))?;
                        let openings =
                            b.operand_openings_for(&refutation).map_err(|e| ("the class cannot open the rows this step reads", e))?;
                        Ok::<_, (&'static str, String)>((refutation, openings))
                    })
                    .await
                    else {
                        *court_stalls.entry("the close's task did not finish").or_default() += 1;
                        continue;
                    };
                    let (refutation, operand_openings) = match assembled {
                        Ok(pair) => pair,
                        Err((stall, e)) => {
                            *court_stalls.entry(stall).or_default() += 1;
                            warn!("[{PALW_PANEL}] cannot assemble the close for session {}: {e}", duty.session_id);
                            continue;
                        }
                    };
                    // **The rows the court will need to read, asked of the adjudicator.**
                    // A close whose openings are empty is a close the court cannot recompute
                    // from: it holds no weights, so every operand the disputed step reads has
                    // to arrive with the evidence, proven against the class root.
                    // **How the prompt reaches the court is the network's choice, not ours**
                    // (ADR-0081 Decision 3). Flat: the whole list rides in the refutation.
                    // Merkle: the list is taken OUT and the one tile the disputed step reads
                    // rides as an opening — the court is shown 32 ids and a path, never the
                    // conversation. A step past the prompt (a decode position) reads no prompt
                    // id, so it carries nothing; an opening that does not build is a stall,
                    // not a close under the other form.
                    // One spelling of the carriage, shared with the one-move accusation
                    // (`palw_refutation_prompt_carriage_v1`, ADR-0103 Decision 1).
                    // The NETWORK's form, which is what the chain's close check reads
                    // (`check_close_speaks_the_networks_prompt_form`). A bisection is a
                    // session opened before the held fence, and a held class is registered
                    // only past it, so no class that reaches here commits in another form
                    // (ADR-0118 Decision 3).
                    let proof = match kaspa_consensus_core::palw_step_refute::palw_refutation_prompt_carriage_v1(
                        self.config.prompt_ids_form,
                        refutation,
                    ) {
                        Ok((refutation, None)) => PalwCourtVerdictProofV2::Arithmetic { refutation, operand_openings },
                        Ok((refutation, Some(prompt_ids_opening))) => {
                            PalwCourtVerdictProofV2::ArithmeticOpened { refutation, operand_openings, prompt_ids_opening }
                        }
                        Err(e) => {
                            *court_stalls.entry("the prompt tile the close needs does not open").or_default() += 1;
                            warn!("[{PALW_PANEL}] session {}: the prompt-id opening does not build: {e}", duty.session_id);
                            continue;
                        }
                    };
                    // **The chain says what the evidence means, not us.** A `CourtClosed` must
                    // announce the verdict the proof derives to, and the pipeline refuses one
                    // that names any other — so asking first is both the only way to spend a
                    // fee on an object that will land, and the right ordering: the party that
                    // assembled the evidence is not the party that reads it.
                    let Some(verdict) = session.palw_court_close_verdict_v2(&duty.session_id, &proof) else {
                        *court_stalls.entry("the chain reads no verdict from this close").or_default() += 1;
                        trace!("[{PALW_PANEL}] session {} has no adjudicable close from this capture yet", duty.session_id);
                        continue;
                    };
                    info!("[{PALW_PANEL}] session {} closes as {verdict:?} on step {index}", duty.session_id);
                    // P2-8c (ADR-0152 N10): a close this node's replay built that proves the executor
                    // guilty proves every `Valid` signer the audit's rule reaches false too.
                    if verdict == kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty && !duty.i_am_responder {
                        false_valid.note_court_close_v1(
                            crate::palw_filer_false_valid::palw_false_valid_armed_v1(
                                &self.consensus_config.params,
                                self.config.fee_outpoint.is_some(),
                                current_daa,
                            ),
                            duty.claim_id,
                            &proof,
                            index,
                            current_daa,
                        );
                    }
                    Some(PalwConsensusObjectV2::CourtClosed { session_id: duty.session_id, verdict, proof })
                }
                // Not our move: the other party owes this rung. Counted, because "waiting" and
                // "broken" are the two readings of a quiet court and only one is a problem.
                _ => {
                    *court_stalls.entry("waiting — the rung is the other party's").or_default() += 1;
                    None
                }
            };
            let Some(object) = object else { continue };
            court_outbox.set_due((duty.session_id, move_round, duty.i_am_responder), palw_court_move_due_v1(duty));
            court_outbox.push_pending((duty.session_id, move_round, duty.i_am_responder, object));
        }
        // ADR-0152 §4-ter N3: the held route — the chain reads it asks for, its moves (each with the
        // DAA its session says it is due by), and the builds the ledger admits, off the tick.
        {
            // 4-ter.3 step 6: once a start, the pursuits this bond's held forfeits call for and the
            // demands it holds open, rebuilt from the chain (they live in memory otherwise).
            if !held_court.seeded_v1() {
                let mut seeds = session.clone().spawn_blocking(move |c| c.palw_held_pursuit_seeds_v1(vec![bond_key])).await;
                // A demand sent before the restart and not yet in a block is in the mempool: noted too.
                let (pooled, orphans) = self
                    .flow_context
                    .mining_manager()
                    .clone()
                    .get_all_transactions(kaspa_mining::model::tx_query::TransactionQuery::All)
                    .await;
                seeds.open_demands.extend(held_court::palw_held_pooled_demands_v1(
                    pooled.iter().chain(orphans.iter()).map(|pooled| pooled.tx.as_ref()),
                    bond_key,
                    current_daa,
                ));
                held_court.seed_v1(seeds);
            }
            // 4-ter.3 step 6's pursuits past their sessions: the pursued claims' rows, off the tick.
            let pursued = held_court.pursued_claims_v1();
            let open_claims = if pursued.is_empty() {
                HashMap::new()
            } else {
                let rows = session
                    .clone()
                    .spawn_blocking(move |c| {
                        c.palw_claim_rows_v1(bond_key, kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Seat, false, 4_096)
                    })
                    .await
                    .map(|read| read.rows)
                    .unwrap_or_default();
                // A challenger that is no seat of the claim's panel: the claims it could still dispute.
                let disputable = if pursued.iter().all(|claim| rows.iter().any(|row| row.claim_id == *claim)) {
                    Vec::new()
                } else {
                    session.clone().spawn_blocking(move |c| c.palw_disputable_claims_v2(vec![bond_key])).await
                };
                held_court::palw_held_open_claims_v1(&rows, &disputable, &pursued)
            };
            let held_host = held_court::PalwPanelHeldHostV1 {
                panel: self,
                session: &session,
                network_domain,
                bond: bond_key,
                materials: pool_books.material.map(),
                open_claims: &open_claims,
            };
            let anchors = held_court.history_anchors_v1(&held_duties);
            let branches = session
                .clone()
                .spawn_blocking(move |c| {
                    let tip = c.get_sink();
                    anchors
                        .into_iter()
                        .map(|(sid, claim, anchor)| {
                            let same =
                                if anchor == tip { Ok(true) } else { c.is_chain_ancestor_of(anchor, tip).map_err(|e| e.to_string()) };
                            (sid, claim, same)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let mut history_unavailable = HashSet::new();
            for (sid, claim, same) in branches {
                match same {
                    Ok(true) => {}
                    Ok(false) => {
                        held_court.invalidate_history_v1(sid, claim);
                        court_outbox.retain_pending(|(queued, _, responder, _)| *queued != sid || *responder);
                    }
                    Err(why) => {
                        history_unavailable.insert(sid);
                        court_outbox.retain_pending(|(queued, _, responder, _)| *queued != sid || *responder);
                        warn!("[{PALW_PANEL}] session {sid}: held history branch unavailable: {why}; challenger moves wait");
                    }
                }
            }
            held_duties.retain(|d| !history_unavailable.contains(&d.session_id));
            for read in held_court.chain_reads_v1(&held_host, &held_duties, current_daa) {
                if history_unavailable.contains(&read.session_id) {
                    continue;
                }
                let previous = held_court.history_walk_v1(read.session_id);
                let Some((duty, needed)) = held_court.history_filter_v1(&held_duties, read.session_id) else { continue };
                let opening_cap = held_court::PalwHeldHostV1::opening_cap(&held_host, &duty.class_id, current_daa);
                let page = session
                    .clone()
                    .spawn_blocking(move |c| {
                        attn_held_objects_page_from_chain_v1(c, read, previous, &duty, &needed, opening_cap, 40_000)
                    })
                    .await;
                match page {
                    Ok(page) => held_court.note_history_page_v1(read.session_id, read.claim_id, page, current_daa),
                    Err(why) => {
                        warn!("[{PALW_PANEL}] session {}: held public history unavailable: {why}; cursor unchanged", read.session_id)
                    }
                }
            }
            let busy = |key: &(Hash64, u32, bool)| {
                history_unavailable.contains(&key.0)
                    || court_outbox.pending().iter().any(|(sid, round, responder, _)| (*sid, *round, *responder) == *key)
                    || court_outbox.submitted_at(key).is_some_and(|at| current_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
            };
            let tick = held_court::palw_held_moves_v1(&held_host, &mut held_court, &held_duties, current_daa, busy);
            for why in tick.stalls {
                *court_stalls.entry(why).or_default() += 1;
            }
            for queued in tick.queued {
                court_outbox.set_due(queued.key, queued.due);
                court_outbox.push_pending((queued.key.0, queued.key.1, queued.key.2, queued.object));
            }
            held_court::palw_held_start_builds_v1(&held_host, &mut held_court, current_daa);
        }
        // Ask for every accused capture a close needed and this node did not hold. Outside the
        // logging guard below deliberately: a request that only goes out when a summary line
        // happens to be printed is a request nobody can reason about.
        for claim in pull_for_close.drain(..) {
            // **Signed** since ADR-0077 SA-2: a serving node refuses an unsigned pull, so an
            // unsigned ask here would look to this node exactly like an executor that never
            // answered — and the court arm would lose its close for want of bytes somebody was
            // willing to serve.
            self.request_material_signed(network_domain, claim, current_daa).await;
        }
        for (claim, intervals) in intervals_for_close.drain(..) {
            let asked = self.request_fp_interval_openings(network_domain, claim, &intervals, current_daa).await;
            info!(
                "[{PALW_PANEL}] claim {claim}: asked the executor for interval(s) {intervals:?} for a close ({asked} signed request(s))"
            );
        }
        if !court_stalls.is_empty() {
            let responder_of = court_duties.iter().filter(|d| d.i_am_responder).count();
            info!(
                "[{PALW_PANEL}] court: {} sessions ({responder_of} as responder), {} pending; not moved: {}",
                court_duties.len(),
                court_outbox.pending().len(),
                court_stalls.iter().map(|(why, n)| format!("{n}× {why}")).collect::<Vec<_>>().join(", ")
            );
        }

        // --- the data-availability court's half: open what an accusation says we withheld ---
        //
        // ADR-0062 Decision 3, the responder (mainnet audit, 2026-09-05). Nothing in this tree
        // constructed a `MaterialDisclosed` — the fence's own doc says "DO NOT ARM THIS WITHOUT
        // A DISCLOSURE RESPONDER IN THE FIELD", because a window that convicts on silence is
        // only sound when the silent party could have spoken. This is that party. It opens the
        // accused event out of the capture it already retains — the ids and rows it kept at
        // execution — through the family's `disclose_trace_event`, signs the object's digest
        // under the producing bond, and submits it as a lifecycle carrier through the court
        // queue: a disclosure is clocked exactly as a rung is. Nothing is re-executed — except an
        // attempt capture the producer has pruned, which is re-made by replaying its block's job and
        // used only if it reproduces the committed roots (`remade_attempt_capture_v1`). It never
        // accuses: a seat that received nothing cannot tell withholding from transport loss,
        // and an accusation against an honest producer is a charge to the accuser.
        let da_duties = session.palw_da_duties_v2(vec![bond_key]);
        // One spelling of "is the DA court in force", shared with the producer's refusal to
        // make a claim it could not answer for (mainnet audit 2026-09-06, C-5): the seat that
        // answers and the producer that decides whether answering is possible must not be able
        // to disagree about which chain they are on.
        let da_armed = crate::palw_producer::palw_da_court_in_force_v1(&self.consensus_config, current_daa);
        for duty in &da_duties {
            if !da_armed || current_daa > duty.disclose_deadline_daa {
                continue;
            }
            let key = (duty.claim_id, duty.missing_event_index, true);
            if let Some(sent_daa) = court_outbox.submitted_at(&key)
                && current_daa < sent_daa.saturating_add(COURT_MOVE_REPLAN_DAA)
            {
                continue;
            }
            if court_outbox.pending().iter().any(|(sid, round, responder, _)| *sid == key.0 && *round == key.1 && *responder == key.2)
            {
                continue;
            }
            let held = self
                .retained_capture(&duty.claim_id)
                .or_else(|| std::fs::read(self.config.retention_dir.join("foreign").join(format!("{}.material", duty.claim_id))).ok());
            let bytes = match held {
                Some(bytes) => bytes,
                // **An attempt claim's pruned capture is re-made by replaying its block's job**
                // and checked against the committed roots (`remade_attempt_capture_v1`). A
                // free-prompt capture is kept while the chain can still ask about it, and a held
                // unit is answered from its own retention, so neither comes here.
                None if !duty.free_prompt && duty.held_missing.is_none() => {
                    match self
                        .remade_attempt_capture_v1(
                            &session,
                            network_domain,
                            duty.accepted_block,
                            duty.class_id,
                            duty.artifact_root,
                            &duty.executor_bond,
                            duty.execution_root,
                            duty.trace_root,
                        )
                        .await
                    {
                        Ok(bytes) => {
                            info!(
                                "[{PALW_PANEL}] claim {}: the accused capture was pruned here — re-made by replaying its block's \
                                 job to answer the data-availability accusation",
                                duty.claim_id
                            );
                            bytes
                        }
                        Err(why) => {
                            warn!("[{PALW_PANEL}] claim {}: cannot re-make the pruned capture: {why}", duty.claim_id);
                            *court_stalls.entry("no retained capture, and no replay that reproduces it").or_default() += 1;
                            continue;
                        }
                    }
                }
                None => {
                    *court_stalls.entry("no retained capture to answer a data-availability accusation from").or_default() += 1;
                    continue;
                }
            };
            // **ADR-0103 Decision 4 / ADR-0111 Decision 4: a HELD accusation is answered in its
            // unit** — a leaf's evidence (which the fold then adjudicates), a prompt tile — and an
            // unanswerable unit is said by name, because silence here is the default's slash.
            if let Some(missing) = duty.held_missing {
                match self.held_da_answer_v1(&session, network_domain, duty.claim_id, missing) {
                    Ok(object) => {
                        info!(
                            "[{PALW_PANEL}] claim {}: answering the held accusation of {missing:?} from the retained capture — \
                             deadline DAA {}",
                            duty.claim_id, duty.disclose_deadline_daa
                        );
                        court_outbox.set_due(key, duty.disclose_deadline_daa);
                        court_outbox.push_pending((key.0, key.1, key.2, object));
                    }
                    Err(why) => {
                        warn!("[{PALW_PANEL}] claim {}: cannot answer the held accusation of {missing:?}: {why}", duty.claim_id);
                        *court_stalls.entry("the held unit accused cannot be answered here").or_default() += 1;
                    }
                }
                continue;
            }
            let (row, tile) = kaspa_consensus_core::palw_state_v2::palw_da_event_index_parts_v1(duty.missing_event_index);
            let disclosed = self.disclose_retained_event_v1(&session, duty.class_id, duty.artifact_root, &bytes, row, tile);
            let disclosure = match disclosed {
                Ok(disclosure) => disclosure,
                Err(why) => {
                    warn!(
                        "[{PALW_PANEL}] claim {}: cannot open accused event (row {row}, tile {tile}) from the retained capture: {why}",
                        duty.claim_id
                    );
                    *court_stalls.entry("the family cannot open the accused event").or_default() += 1;
                    continue;
                }
            };
            let digest = kaspa_consensus_core::palw_state_v2::palw_da_disclosure_digest_v1(&disclosure);
            let message = kaspa_consensus_core::palw_state_v2::palw_da_disclosure_message_v3(
                network_domain,
                &duty.claim_id,
                duty.missing_event_index,
                &digest,
            );
            let Some(signature) =
                self.sign(message.as_byte_slice(), kaspa_consensus_core::palw_state_v2::PALW_DA_DISCLOSURE_V2_MLDSA87_CONTEXT)
            else {
                *court_stalls.entry("no signing key for a disclosure").or_default() += 1;
                continue;
            };
            info!(
                "[{PALW_PANEL}] claim {}: answering the data-availability accusation of event (row {row}, tile {tile}) from the \
                 retained capture — deadline DAA {}",
                duty.claim_id, duty.disclose_deadline_daa
            );
            court_outbox.set_due(key, duty.disclose_deadline_daa);
            court_outbox.push_pending((
                key.0,
                key.1,
                key.2,
                PalwConsensusObjectV2::MaterialDisclosed {
                    claim: duty.claim_id,
                    event_index: duty.missing_event_index,
                    disclosure,
                    signature,
                },
            ));
        }

        // --- ADR-0152 X7 / DA-4 (P2-7): R-core+'s data-availability court — every demanded unit ---
        //
        // Past `palw_rcore_plus` an accusation opens a session in `da_sessions` and never touches
        // the claim's phase (DA-1), so the loop above finds nothing there, and the v1 answers it
        // built are refused (`DaV1AnswerRetired`). Here each unit an open session demands of this
        // node is answered with a tag-55 or legacy tag-158 object (`palw_da_built_answer_object_v1`): as the
        // claim's PRODUCER at once, or as a COVERING SIGNER — a live lock whose mask covers the
        // unit, the fold's own predicate, so exactly the units a default would charge it S4 for —
        // once the producer has had its turn (`palw_disclosure_due_v1`). The material is what
        // this node kept for the claim, checked against its roots, or a capture re-made by
        // replaying the claim's job and checked the same way (`palw_da_material_v1`), one claim at
        // a time, off the loop, under the memory ledger (`rcore_da_answers_v1`). One `Flat` a claim
        // answers every in-run event unit (IMPL-16). Below the fence the duty list is empty.
        let disclosure = session.palw_disclosure_duties_v1(vec![bond_key]);
        // What this node must keep: every claim a live lock of its bond still answers for, and
        // every claim an open session demands a unit of now (its own too: a re-made capture is
        // kept under `foreign/`). Pinned HERE, before any seat duty below can write `foreign/`
        // and prune it — on the first tick after a restart the pins set at the end of the last
        // tick are gone (the P2-7 review's LOW), and the age sweep would take a job payload a
        // live lock still answers for; the end of the tick sets them again, with these.
        let disclosure_retain: Vec<Hash64> =
            disclosure.retain.iter().map(|(claim, _)| *claim).chain(disclosure.duties.iter().map(|duty| duty.claim_id)).collect();
        if !disclosure_retain.is_empty() {
            self.foreign_pinned.lock().unwrap().extend(disclosure_retain.iter().copied());
        }
        da_flat_sent.retain(|_, sent| current_daa < sent.saturating_add(COURT_MOVE_REPLAN_DAA));
        // An answer still queued (the fee UTXO was busy) whose unit no open session demands of
        // this node any more — another discloser answered it, or its session closed — is one the
        // fold refuses (`DaUnitAlreadyAnswered`, `DaUnitNotDemanded`): it leaves the queue unsent,
        // rather than paying a carrier the mempool then refuses.
        let owed: std::collections::BTreeSet<(Hash64, kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1)> =
            disclosure.duties.iter().map(|duty| (duty.claim_id, duty.unit)).collect();
        court_outbox.retain_pending(|(_, _, _, object)| match object {
            PalwConsensusObjectV2::MaterialDisclosedV2 { claim, unit, .. } => owed.contains(&(*claim, *unit)),
            PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer } => {
                owed.contains(&(answer.claim, kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::LegacyHeldV2(answer.unit)))
            }
            PalwConsensusObjectV2::LegacyHeldAnswerDeclaredV2 { declaration } => {
                owed.contains(&(
                    declaration.claim,
                    kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::LegacyHeldV2(declaration.unit),
                )) && session.palw_legacy_answer_progress_v2(declaration.key()).map(|p| p.is_none()).unwrap_or(true)
            }
            PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk } => {
                owed.contains(&(chunk.claim, kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::LegacyHeldV2(chunk.unit)))
                    && session
                        .palw_legacy_answer_progress_v2(chunk.key())
                        .map(|p| da_answer_chunks::chunk_is_pending_v2(chunk, p.as_ref()))
                        .unwrap_or(true)
            }
            _ => true,
        });
        if da_armed && !disclosure.duties.is_empty() {
            let mut disclosure_keys = BTreeMap::new();
            // A `Flat` queued (possibly waiting for the fee UTXO) or just sent answers the claim's run.
            let mut flat_queued: HashSet<Hash64> = court_outbox
                .pending()
                .iter()
                .filter_map(|(_, _, _, object)| match object {
                    PalwConsensusObjectV2::MaterialDisclosedV2 {
                        claim,
                        answer:
                            kaspa_consensus_core::palw_da_rcore_v1::PalwDaAnswerV1::Event(
                                kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Flat { .. },
                            ),
                        ..
                    } => Some(*claim),
                    _ => None,
                })
                .collect();
            flat_queued.extend(da_flat_sent.keys().copied());
            // The due units not queued or recently tried, by claim, in the duty list's order —
            // soonest deadline first — so the claim whose session ends first is built first and
            // asks the memory ledger first.
            let mut by_claim: Vec<(Hash64, Vec<&kaspa_consensus_core::palw_producer_v2::PalwDisclosureDutyV1>)> = Vec::new();
            for duty in &disclosure.duties {
                if !palw_disclosure_due_v1(duty, current_daa) {
                    continue;
                }
                // DRILL ONLY (G14 audit G-17 / D-11, `--palw-drill-withhold-all-da`): no unit of any kind is answered.
                if self.config.drill_withhold_all_da {
                    crate::palw_backends::note_throttled_v1("drill-withhold-all-da", || {
                        format!(
                            "[{PALW_PANEL}] PALW DRILL (G14 D-11): withholding DA unit {:?} of claim {} (every demand runs to its deadline)",
                            duty.unit, duty.claim_id
                        )
                    });
                    continue;
                }
                // DRILL ONLY (the G14 collusion drill D-11, `--palw-drill-withhold-held-da`): a legacy held unit is never answered.
                if self.config.drill_withhold_held_da
                    && matches!(duty.unit, kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::LegacyHeldV2(_))
                {
                    crate::palw_backends::note_throttled_v1("drill-withhold-held-da", || {
                        format!(
                            "[{PALW_PANEL}] PALW DRILL (G14 D-11): withholding legacy held unit {:?} of claim {} (the demand runs to its deadline)",
                            duty.unit, duty.claim_id
                        )
                    });
                    continue;
                }
                if flat_queued.contains(&duty.claim_id)
                    && kaspa_consensus_core::palw_da_rcore_v1::palw_da_flat_answers_unit_v1(&duty.unit, duty.in_run_rows)
                {
                    continue;
                }
                let key = if let kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::LegacyHeldV2(unit) = duty.unit {
                    match session.palw_legacy_answer_progress_v2((duty.claim_id, unit, duty.discloser)) {
                        Ok(progress) => da_answer_chunks::queue_key_v2(duty, progress.as_ref()),
                        Err(_) => continue, // An unavailable reader is never treated as an empty assembly.
                    }
                } else {
                    palw_disclosure_queue_key_v1(duty)
                };
                disclosure_keys.insert((duty.claim_id, duty.unit), key);
                if let Some(sent_daa) = court_outbox.submitted_at(&key)
                    && current_daa < sent_daa.saturating_add(COURT_MOVE_REPLAN_DAA)
                {
                    continue;
                }
                if court_outbox.pending().iter().any(|(sid, round, responder, _)| (*sid, *round, *responder) == key) {
                    continue;
                }
                match by_claim.iter_mut().find(|(claim, _)| *claim == duty.claim_id) {
                    Some((_, duties)) => duties.push(duty),
                    None => by_claim.push((duty.claim_id, vec![duty])),
                }
            }
            for (claim, duties) in by_claim {
                let role = duties[0].role;
                match self.rcore_da_answers_v1(&session, network_domain, &duties, flat_queued.contains(&claim)).await {
                    Ok(built) => {
                        for (duty, built) in duties.iter().zip(built) {
                            let key = disclosure_keys[&(duty.claim_id, duty.unit)];
                            match built {
                                // A `Flat` built for an earlier unit answers this one.
                                Ok(None) => {}
                                Ok(Some(object)) => {
                                    let key = da_answer_chunks::object_queue_key_v2(duty, &object);
                                    info!(
                                        "[{PALW_PANEL}] claim {claim}: answering {:?} of an open data-availability session as {role:?} \
                                         — deadline DAA {} (ADR-0152 DA-4, X7)",
                                        duty.unit, duty.deadline_daa
                                    );
                                    if let PalwConsensusObjectV2::MaterialDisclosedV2 {
                                        answer:
                                            kaspa_consensus_core::palw_da_rcore_v1::PalwDaAnswerV1::Event(
                                                kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Flat { .. },
                                            ),
                                        ..
                                    } = &object
                                    {
                                        flat_queued.insert(claim);
                                        da_flat_sent.insert(claim, current_daa);
                                    }
                                    court_outbox.set_due(key, palw_disclosure_answer_due_v1(duty));
                                    court_outbox.push_pending((key.0, key.1, key.2, object));
                                }
                                Err(why) => {
                                    warn!(
                                        "[{PALW_PANEL}] claim {claim}: cannot answer {:?} of an open data-availability session as \
                                         {role:?}: {why}",
                                        duty.unit
                                    );
                                    court_outbox.mark_submitted(key, current_daa);
                                    *court_stalls
                                        .entry("a data-availability unit cannot be answered from what this node holds")
                                        .or_default() += 1;
                                }
                            }
                        }
                    }
                    // Asked again next tick, soonest deadline first again.
                    Err(PalwDaClaimHoldV1::Ledger(why)) => {
                        *court_stalls.entry("the memory ledger cannot cover a data-availability answer").or_default() += 1;
                        crate::palw_backends::note_throttled_v1("panel-da-answer-ledger", || {
                            format!("[{PALW_PANEL}] claim {claim}: the data-availability answer waits — {why}")
                        });
                    }
                    // Tried again a re-plan later, not every tick: building the material may have
                    // replayed the claim's whole job, and one that does not reproduce the roots
                    // will not on the next tick either.
                    Err(PalwDaClaimHoldV1::Material(why)) => {
                        warn!(
                            "[{PALW_PANEL}] claim {claim}: cannot answer {} unit(s) of an open data-availability session as \
                             {role:?}: {why}",
                            duties.len()
                        );
                        for duty in &duties {
                            court_outbox.mark_submitted(disclosure_keys[&(duty.claim_id, duty.unit)], current_daa);
                        }
                        *court_stalls.entry("a data-availability unit cannot be answered from what this node holds").or_default() +=
                            duties.len();
                    }
                }
            }
        }

        if let Err(why) = self
            .public_prefix_answer_tick_v1(
                &session,
                PalwAnswerSlotV1::own(bond_key),
                &mut public_prefix_response_cursor,
                &mut court_outbox,
            )
            .await
        {
            debug!("[{PALW_PANEL}] public prefix response remains pending: {why}");
        }
        CourtProgressResultV1 {
            _owed: owed,
            disclosure_retain,
            _disclosure: disclosure,
            _da_duties: da_duties,
            _held_duties: held_duties,
            _court_stalls: court_stalls,
            court_duties,
            _intervals_for_close: intervals_for_close,
            _pull_for_close: pull_for_close,
        }
    }
}
