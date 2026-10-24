//! **G14 ③ / ⑦ on the service tick**: the GDN step filer ([`crate::palw_gdn_step_filer`]) over this node's chain view. Off by
//! default (`--palw-gdn-step-filer`), and dormant below `palw_memory_routing_court_v2` (the fence that owns tag 243; every preset
//! leaves it off). Shares the held-GDN filer's material session ([`super::gdn_filer_tick`]'s `gdn_filer_entries_from_v1`: the claims
//! this bond may dispute, of a Qwen3.6 hybrid class, with a pooled served capture naming the claim). Its objects ride the priority
//! lane (`court_pending`, due now), like the held filer's.
use super::*;
use crate::palw_gdn_step_filer::{
    PALW_GDN_STEP_CLAIMS_PER_TICK_V1, PALW_GDN_STEP_DISCOVER_PER_TICK_V1, PALW_GDN_STEP_FINAL_MARGIN_DAA_V1, PalwGdnStepCaseV1, PALW_GDN_STEP_PROBES_PER_TICK_V1, PalwGdnStepBookV1,
    PalwGdnStepClaimV1, PalwGdnStepReservationAtTipV1, PalwGdnStepResendV1, PalwGdnStepStageV1, PalwGdnStepWindowFactsV1,
    PalwGdnStepWindowV1, palw_gdn_step_convicted_v1, palw_gdn_step_false_valid_proof_v1, palw_gdn_step_probe_case_v1,
    palw_gdn_step_resend_v1, palw_gdn_step_window_v1,
};
use crate::palw_panel::court_outbox::CourtOutboxV1;
use crate::palw_panel::gdn_filer_tick::PALW_GDN_FILER_ROUND_V1;
use misaka_palw_base0::qwen36_backend::Qwen36Backend;

/// The queue round of the step filer's accusation (tag 243), beside the held filer's rounds.
pub(crate) const PALW_GDN_STEP_ROUND_V1: u32 = PALW_GDN_FILER_ROUND_V1 + 0x80;
/// The queue round of the step filer's dispute reservation (tag 154).
pub(crate) const PALW_GDN_STEP_RESERVE_ROUND_V1: u32 = PALW_GDN_FILER_ROUND_V1 + 0x81;

impl PalwPanelService {
    /// **One tick of the GDN step filer**: discover, probe one claim, then file / track / re-submit every filed case.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn gdn_step_filer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwGdnStepBookV1,
        false_valid: &mut crate::palw_filer_false_valid::PalwFalseValidFilerV1,
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        court_outbox: &mut CourtOutboxV1,
    ) {
        use kaspa_consensus_core::config::params::ForkActivation;
        let params = &self.consensus_config.params;
        let armed = params.palw_memory_routing_court_v2_fence().is_some_and(|f| f != ForkActivation::never() && f.is_active(current_daa));
        if !self.config.gdn_step_filer {
            return;
        }
        if !armed {
            crate::palw_backends::note_throttled_v1("panel-gdn-step-filer-dormant", || {
                format!("[{PALW_PANEL}] the GDN step filer is enabled but palw_memory_routing_court_v2 is not in force at DAA {current_daa}: idle")
            });
            return;
        }
        let Some(kp) = self.keypair.clone() else { return };
        let sign = move |message: &[u8], context: &[u8]| Self::sign_hedged(&kp.signing_key, message, context);

        // --- discover (round 2): every claim inside tag 243's window, one bounded page a tick ---
        // Owned keys only cross the await; the book is not borrowed across it.
        let after = book.discover_after;
        let page = session
            .clone()
            .spawn_blocking(move |c| c.palw_gdn_step_candidates_v1(bond_key, after, PALW_GDN_STEP_DISCOVER_PER_TICK_V1))
            .await;
        let keys: Vec<(u64, Hash64)> = page.iter().map(|cand| (cand.accepted_daa, cand.claim.claim_id)).collect();
        book.advance_discovery(&keys, PALW_GDN_STEP_DISCOVER_PER_TICK_V1);
        let targets: Vec<_> = page.into_iter().filter(|cand| !book.is_known(&cand.claim.claim_id)).map(|cand| cand.claim).collect();
        // --- probe (the held filer's material session; synchronous) ---
        let entries = self.gdn_filer_entries_from_v1(session, targets, materials, network_domain, bond_key);
        // The fold's limits per class, read once before any probing (no borrow of the book across this await).
        let classes: Vec<Hash64> = {
            let mut v: Vec<Hash64> = entries.iter().map(|e| e.d.class_id).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let limits: HashMap<Hash64, kaspa_consensus_core::palw_gdn_court_v2::PalwGdnCourtLimitsV2> = session
            .clone()
            .spawn_blocking(move |c| {
                classes.into_iter().filter_map(|class| c.palw_gdn_court_limits_v2(class).map(|l| (class, l))).collect::<HashMap<_, _>>()
            })
            .await;
        let mut probed = 0usize;
        for e in entries {
            let claim = PalwGdnStepClaimV1 {
                claim: e.d.claim_id,
                executor: e.d.executor_bond,
                execution_root: e.d.execution_root,
                trace_root: e.d.trace_root,
                class_id: e.d.class_id,
                artifact_root: e.d.artifact_root,
            };
            let claim_id = claim.claim;
            book.open(claim, current_daa);
            if probed >= PALW_GDN_STEP_CLAIMS_PER_TICK_V1 {
                continue;
            }
            let Some(class_limits) = limits.get(&e.d.class_id).copied() else {
                debug!("[{PALW_PANEL}] claim {claim_id}: the GDN step filer has no court limits for class {} at the tip", e.d.class_id);
                continue;
            };
            let Some(q36) = e.backend.as_any_v1().and_then(|a| a.downcast_ref::<Qwen36Backend>()) else { continue };
            let mut owned = match q36.gdn_accuser_view_v2(&e.capture, Some(e.prompt.as_slice())) {
                Ok(owned) => owned,
                Err(why) => {
                    debug!("[{PALW_PANEL}] claim {claim_id}: the GDN step filer has no accuser view yet: {why}");
                    continue;
                }
            };
            // Round 2 (1): openings are cut, and the verdict judged, at the FOLD's ladder for this class — never a local guess.
            if owned.max_leaves != class_limits.ladder {
                info!(
                    "[{PALW_PANEL}] claim {claim_id}: the GDN step filer judges at the fold's class ladder {} (the material's ladder was \
                     {})",
                    class_limits.ladder, owned.max_leaves
                );
                owned.max_leaves = class_limits.ladder;
            }
            let (openings, _root) = match q36.gdn_inventory_openings_v2() {
                Ok(o) => o,
                Err(why) => {
                    debug!("[{PALW_PANEL}] claim {claim_id}: the GDN step filer has no class openings: {why}");
                    continue;
                }
            };
            if q36.gdn_recompute_kernels_v2().is_err() {
                continue;
            }
            probed += 1;
            let view = owned.view();
            let notes = palw_gdn_step_probe_case_v1(
                book,
                &claim_id,
                &view,
                || q36.gdn_recompute_kernels_v2().expect("resolved above"),
                bond_key,
                network_domain,
                &openings,
                class_limits,
                &sign,
                PALW_GDN_STEP_PROBES_PER_TICK_V1,
            );
            for note in notes {
                info!("[{PALW_PANEL}] claim {claim_id}: GDN step filer — {note} (G14 ③, RFC-0006)");
            }
            if let Some(why) = book.settled.get(&claim_id) {
                info!("[{PALW_PANEL}] claim {claim_id}: GDN step filer settled — {why}");
            }
        }

        // --- file, track the verdict, re-submit what a reorg dropped ---
        let filed: Vec<Hash64> =
            book.cases.iter().filter(|(_, c)| matches!(c.stage, PalwGdnStepStageV1::Filed { .. })).map(|(k, _)| *k).collect();
        if filed.is_empty() {
            return;
        }
        let views = session
            .clone()
            .spawn_blocking(move |c| {
                filed
                    .into_iter()
                    .map(|claim| (claim, c.palw_legacy_dispute_v1(claim), c.palw_gdn_step_candidate_v1(bond_key, claim)))
                    .collect::<Vec<_>>()
            })
            .await;
        let reservation_fence = params.palw_legacy_public_filer_active_at(current_daa);
        for (claim_id, view, candidate) in views {
            // Copy what this claim's decisions need out of the book; no borrow of it is held below.
            let Some((object, leaf, mut sent)) = book.cases.get(&claim_id).and_then(|case| match &case.stage {
                PalwGdnStepStageV1::Filed { object, leaf, sent } => Some((std::sync::Arc::clone(object), *leaf, *sent)),
                PalwGdnStepStageV1::Probing { .. } => None,
            }) else {
                continue;
            };
            let Some(view) = view else {
                book.settle(&claim_id, "the tip no longer holds the claim");
                info!("[{PALW_PANEL}] claim {claim_id}: GDN step filer — the tip no longer holds the claim; case closed");
                continue;
            };
            // The verdict: a conviction hands every liable Valid signer to the false-Valid filer.
            if let Some(voided_daa) = palw_gdn_step_convicted_v1(&view.phase) {
                let fv_armed = crate::palw_filer_false_valid::palw_false_valid_armed_v1(params, self.config.fee_outpoint.is_some(), current_daa);
                let mut noted = false_valid.holds(&claim_id);
                if !noted && fv_armed {
                    noted = false_valid.note_proof_v1(
                        palw_gdn_step_false_valid_proof_v1(claim_id, voided_daa),
                        crate::palw_filer_false_valid::PalwFalseValidSourceV1::CourtClose { leaf },
                        Some(view.accepted_daa),
                        current_daa,
                    );
                }
                if noted && fv_armed {
                    false_valid.wake_after_court_fraud_v1(&claim_id, current_daa);
                }
                info!(
                    "[{PALW_PANEL}] claim {claim_id}: convicted (CourtFraud at DAA {voided_daa}) — every liable Valid signer is handed to \
                     the false-Valid filer: {noted} (G14 ⑦)"
                );
                book.settle(&claim_id, format!("convicted at DAA {voided_daa}"));
                continue;
            }
            let record = view.record.as_ref();
            let live = record.is_some_and(|r| r.live.contains_key(&bond_key));
            let closed = record.is_some_and(|r| r.closed.contains(&bond_key));
            let facts = PalwGdnStepWindowFactsV1 {
                phase: view.phase.clone(),
                final_deadline_daa: view.deadline_daa,
                hard_deadline_daa: view.hard_deadline_daa,
                own_reservation_live: live,
                own_reservation_closed: closed,
                reservation_fence,
                final_row_open: candidate.as_ref().is_some_and(|c| c.final_row_open),
            };
            let at_tip = if live {
                PalwGdnStepReservationAtTipV1::Live
            } else if closed {
                PalwGdnStepReservationAtTipV1::Closed
            } else {
                PalwGdnStepReservationAtTipV1::Absent
            };
            let accuse_key = (claim_id, PALW_GDN_STEP_ROUND_V1, false);
            let reserve_key = (claim_id, PALW_GDN_STEP_RESERVE_ROUND_V1, false);
            let reservation_object = || {
                let mut signed = true;
                let object = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_reserved_object_v1(
                    network_domain,
                    kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_reservation_v1(&view, bond_key),
                    |m| {
                        sign(m, kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1).unwrap_or_else(
                            || {
                                signed = false;
                                Vec::new()
                            },
                        )
                    },
                );
                signed.then_some(object)
            };
            // What happens to the case after this claim's decisions: keep it (with `sent` written back) or settle it.
            let mut settle_why: Option<String> = None;
            if sent.accusation.is_none() {
                match palw_gdn_step_window_v1(&facts, current_daa, PALW_GDN_STEP_FINAL_MARGIN_DAA_V1) {
                    PalwGdnStepWindowV1::Open { reserve } => {
                        if reserve && sent.reservation.is_none() {
                            match reservation_object() {
                                Some(r) => {
                                    info!(
                                        "[{PALW_PANEL}] claim {claim_id}: GDN step filer reserves the claim first (phase {:?}, Final deadline \
                                         {:?}, now {current_daa}) — DisputeReservedV1",
                                        view.phase, view.deadline_daa
                                    );
                                    court_outbox.push_pending((claim_id, PALW_GDN_STEP_RESERVE_ROUND_V1, false, r));
                                    court_outbox.set_due(reserve_key, current_daa);
                                    sent.reservation = Some((current_daa, 1));
                                }
                                None => warn!("[{PALW_PANEL}] claim {claim_id}: GDN step filer cannot sign a reservation"),
                            }
                        }
                        info!(
                            "[{PALW_PANEL}] claim {claim_id}: GDN step filer queues GdnStepAccusedV2 (phase {:?}, Final deadline {:?}, \
                             now {current_daa}; G14 ③)",
                            view.phase, view.deadline_daa
                        );
                        match court_outbox.try_push_clone(accuse_key, &object) {
                            Ok(()) => {
                                court_outbox.set_due(accuse_key, current_daa);
                                sent.accusation = Some((current_daa, 1));
                            }
                            Err(why) => warn!("[{PALW_PANEL}] claim {claim_id}: GDN accusation admission refused; retained for retry: {why}"),
                        }
                    }
                    PalwGdnStepWindowV1::Closed(why) => {
                        warn!("[{PALW_PANEL}] claim {claim_id}: GDN step filer does NOT file — {why}");
                        settle_why = Some(format!("window closed: {why}"));
                    }
                }
            } else if let PalwGdnStepWindowV1::Closed(why) = palw_gdn_step_window_v1(&facts, current_daa, 0) {
                // Filed already: a closed window ends the case.
                warn!("[{PALW_PANEL}] claim {claim_id}: GDN step filer's accusation did not land before the window closed — {why}");
                settle_why = Some(format!("window closed after filing: {why}"));
            } else {
                // Filed already: a reorg (or a lost carrier) re-submits.
                for resend in palw_gdn_step_resend_v1(&sent, at_tip, false, current_daa) {
                    match resend {
                        PalwGdnStepResendV1::Reservation => {
                            if let Some(r) = reservation_object() {
                                info!(
                                    "[{PALW_PANEL}] claim {claim_id}: GDN step filer's reservation is not on the selected chain (reorg or \
                                     lost carrier) — re-submitting"
                                );
                                court_outbox.forget_submitted(&reserve_key);
                                court_outbox.push_pending((claim_id, PALW_GDN_STEP_RESERVE_ROUND_V1, false, r));
                                court_outbox.set_due(reserve_key, current_daa);
                                let n = sent.reservation.map_or(0, |(_, n)| n);
                                sent.reservation = Some((current_daa, n.saturating_add(1)));
                            }
                        }
                        PalwGdnStepResendV1::Accusation => {
                            info!(
                                "[{PALW_PANEL}] claim {claim_id}: GDN step filer's accusation has no verdict on the selected chain (reorg \
                                 or lost carrier) — re-submitting"
                            );
                            match court_outbox.try_push_clone(accuse_key, &object) {
                                Ok(()) => {
                                    court_outbox.forget_submitted(&accuse_key);
                                    court_outbox.set_due(accuse_key, current_daa);
                                    let n = sent.accusation.map_or(0, |(_, n)| n);
                                    sent.accusation = Some((current_daa, n.saturating_add(1)));
                                }
                                Err(why) => warn!("[{PALW_PANEL}] claim {claim_id}: GDN resend admission refused; retained for retry: {why}"),
                            }
                        }
                        PalwGdnStepResendV1::GiveUp(why) => {
                            warn!("[{PALW_PANEL}] claim {claim_id}: GDN step filer gives up — {why}");
                            settle_why = Some(format!("gave up: {why}"));
                            break;
                        }
                    }
                }
            }
            // Re-look the case up (nothing borrowed it above) and write the outcome back.
            match settle_why {
                Some(why) => book.settle(&claim_id, why),
                None => {
                    if let Some(PalwGdnStepCaseV1 { stage: PalwGdnStepStageV1::Filed { sent: slot, .. }, .. }) = book.cases.get_mut(&claim_id) {
                        *slot = sent;
                    }
                }
            }
        }
    }
}
