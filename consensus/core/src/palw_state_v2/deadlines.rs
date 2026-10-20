//! **The per-block deadline sweeps (court deadlines, then every other end)** — moved out of `palw_state_v2.rs` (2026-10-12, a behaviour-preserving split: the items are the same; members the parent calls are `pub(super)`).

use super::*;

pub(super) fn sweep_court_deadlines(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    while let Some(&(deadline, session_id)) = builder.state.court_deadlines.iter().next() {
        if deadline >= ctx.daa_score {
            break;
        }
        let mut session = builder.state.court_sessions.get(&session_id).ok_or(PalwStateV2Error::MissingSession(session_id))?.clone();
        // **A missing claim drops the session; it does not fail the block.** `retire_claim` closes
        // a claim's sessions with it, so this should be unreachable — and it is written to be
        // survivable anyway, because the alternative failure mode is a chain that stops forever on
        // a state nothing can repair. Dropping is also the correct answer on its own terms: a
        // session about a claim the chain no longer holds can decide nothing.
        let Some(claim) = builder.state.claims.get(&session.claim).cloned() else {
            builder.write_court(session_id, None)?;
            continue;
        };

        // **P0-9: silence at a rung decides the dispute, and no object says so.**
        //
        // `declare_no_show` reads whose turn it was from the machine's own state, which is this
        // chain's state — so every node reaches the same verdict from the same absence, and there
        // is nothing for an attacker to forge. That is the whole reason the ladder had to be
        // carried before defaults could be honoured.
        //
        // A silent RESPONDER loses: it announced a root and would not stand behind it at the
        // index it was asked about. A silent CHALLENGER loses: prosecution is its burden, which
        // is the same rule the backstop below applies. Either way the session ends here.
        // A rung clock counts only if it runs out STRICTLY inside the session budget. When the
        // two coincide — which is what `turn_deadline_daa`'s default makes them — the rung can
        // decide nothing the backstop would not already have decided, and letting it try would
        // silently invert the outcome: the backstop closes challenger-side, a first-rung silence
        // closes against the responder. So a network without a tighter rung window keeps exactly
        // the pre-ladder behavior, and one with a real ladder gets the rung verdict it configured.
        //
        // **And the rung clock does not run at `Terminal`** (audit M2-4). `court_next_deadline_v2`
        // already excludes that turn — "`Terminal` has no move the responder can make … the
        // backstop still ends it, on the challenger's side, which is what an unproven accusation
        // deserves" — but that function only decides WHEN a session is visited. WHO is charged was
        // re-derived here from two raw numbers with no reference to the turn, and at `Terminal` the
        // last completed rung's deadline is below both, so `rung_fired` was true, `declare_no_show`
        // returned `Responder`, and the sweep convicted the accused for a close the challenger
        // never filed. On the RC ruleset (rungs 60 DAA apart, backstop 2,400) that was every
        // terminal session. The two comments stated opposite rules; this makes the arithmetic obey
        // the one that is documented.
        //
        // **And the turn is the PHASE's once one is open** (ADR-0082 Decision 2). The dissection
        // is a phase of this session and not a second one, so the sweep asks the same two
        // questions of it — who owes a move, and by when — through the same helper the deadline
        // index uses. Asking the LADDER here would read `Terminal` on every session that had
        // reached its dissection and hand the whole exchange to the backstop, which closes
        // challenger-side: a responder could simply stop answering rounds and win.
        let class_is_fused = court_session_class_is_fused_v2(&builder.state, &session);
        let (turn, rung_deadline) = court_session_turn_and_rung_deadline_v2(&builder.state, &session);
        // RFC-0002 F7: an IR class's phase, the table's row beside the session. Every rule below that
        // asks "is a phase open" asks it of either phase.
        let mut tir_phase = builder.state.tir_dissections.get(&session_id).map(|phase| phase.as_ref().clone());
        let no_phase = session.dissection.is_none() && tir_phase.is_none();
        // **The fused terminal's OPENING move, named before the ladder is abandoned** (mainnet
        // audit 2026-09-06, C-2/H-5).
        //
        // `declare_no_show` below takes `&mut` and stamps the ladder `Abandoned`, so this question
        // cannot be asked after it; and the remap above has already rewritten the turn to
        // `AwaitDisclosure`, so by then a fused terminal is indistinguishable from an ordinary rung.
        // It is asked once, here, off the same two facts the remap used.
        let owes_the_dissection_opening =
            no_phase && class_is_fused && session.ladder.turn() == crate::palw_bisect::PalwBisectTurnV1::Terminal;
        let turn_can_still_move =
            !matches!(turn, crate::palw_bisect::PalwBisectTurnV1::Terminal | crate::palw_bisect::PalwBisectTurnV1::Abandoned);
        let rung_fired = turn_can_still_move && rung_deadline < ctx.daa_score && rung_deadline < session.deadline_daa;
        if rung_fired {
            // The phase's own `declare_no_show` when a phase is open, the ladder's otherwise —
            // and both mint the same `PalwBisectNoShowV1` with the same rule at `Terminal`, which
            // is why the arms below need no second spelling.
            let declared = match (tir_phase.as_mut(), session.dissection.as_mut()) {
                (Some(phase), _) => phase.declare_no_show(ctx.daa_score).map_err(|e| e.to_string()),
                (None, Some(phase)) => phase.declare_no_show(ctx.daa_score).map_err(|e| e.to_string()),
                (None, None) => session.ladder.declare_no_show(ctx.daa_score).map_err(|e| e.to_string()),
            };
            if let Ok(no_show) = declared {
                // **Whether the accused could have moved at all, as a fact the chain already
                // holds** (ADR-0069 Decision 3-3).
                //
                // The opening-rung exemption below exists for one reason: a family with no
                // `bisect_prefix_state` cannot make the first move, so convicting its silence
                // convicts an honest producer for its build's gap. ADR-0069 turns "can move" into
                // an on-chain fact rather than a property of this tree — a class holds cadence only
                // if its family is certified end to end, and that certificate IS the drill that
                // proves the family plays a court through to a conviction. So weight is the
                // question, and the state answers it.
                //
                // Weight-bearing: the family certified, the producer CAN stand behind its root, and
                // a silence at round 0 is a default like any other. Weightless: the exemption
                // stands, because the reason for it stands. It is also self-correcting — a family
                // that certifies later takes weight by activation, and its claims become
                // convictable at that same moment with nothing here to change.
                //
                // A class that certified and then decayed out of the table is exempted too, which
                // is generous by one case and deliberately so: the arm decides whether to DESTROY
                // collateral, and the direction to be wrong in is the one that does not.
                //
                // Through the SAME helper the fold and the re-derivation use, with the predicate
                // asked unconditionally: this exemption is not fenced and never was — it decides
                // whether to destroy collateral, and it answered `share > 0` before Decision 7
                // existed. Spelling it a third time by hand is what let the ADR claim "one helper"
                // while three call sites each held their own copy of the question.
                let class_holds_weight = palw_class_bears_weight_v2(&builder.state.class_shares, &claim.class_id, true);
                builder.write_court(session_id, None)?;
                match no_show.silent_party {
                    // **A silence at the OPENING rung is not a conviction** (audit M2-5).
                    //
                    // The rung clock convicts a responder that will not stand behind its own root
                    // at the index it was asked about. That is only a meaningful accusation once
                    // the responder has shown it can play at all: two of this chain's three genesis
                    // classes (40 % of cadence between them) have no `bisect_prefix_state` in this
                    // tree, so their honest producers CANNOT make the first move — and an attacker
                    // holding any minimum bond could open one court per licensed claim and take the
                    // producer's reservation 60 DAA later, recovering its own stake in full. The
                    // 2026-08-22 audit closed exactly this and it was re-opened on the strength of a
                    // responder that exists for one family out of three.
                    //
                    // Round 0 therefore falls through to the session backstop, which closes on the
                    // challenger's side — the right answer for an accusation that was never
                    // supported by an exchange. A responder that has answered once and then goes
                    // silent still loses at the next rung, which is the case the ladder is for.
                    //
                    // **And it does not reach into a DISSECTION** (ADR-0082 Decision 2). The
                    // exemption's whole premise is "this responder may not be able to move at
                    // all"; a session with an open phase has a responder that filed a root claim,
                    // which is the move the exemption doubts it can make. Letting round 0 of the
                    // PHASE inherit it would give an executor one free silence per dispute after
                    // it had already shown it could play — the exemption becoming a strategy
                    // rather than a mercy.
                    //
                    // **And the same mercy at a FUSED TERMINAL, past the fence** (mainnet audit
                    // 2026-09-06, C-2/H-5). The exemption above is spelled `ladder.round() == 0`,
                    // which is "the responder has not shown it can play"; the exemption's own
                    // sentence for why it stops at a dissection is that "a session with an open
                    // phase has a responder that filed a root claim, which is the move the
                    // exemption doubts it can make". At a fused terminal `dissection.is_none()` —
                    // the root claim is precisely the move that has NOT been made, and no binary in
                    // the field constructs one. A converged ladder has `round() > 0` by
                    // construction, so the `round() == 0` clause — written before ADR-0082 §10.6
                    // added the fused remap — could never reach this state, and every fused
                    // terminal that ran its clock out convicted the accused regardless of guilt.
                    //
                    // Fenced, and deliberately not merged into the clause above: that one is
                    // unfenced and always was, and it decides a different question.
                    // **C-08 (mainnet audit 2026-09-11 deep fence): the fused-terminal mercy is
                    // LIFTED past the fence.** The mercy above exists because, before the deep fence,
                    // no binary in the field could construct the fused ROOT CLAIM for a dense-v5 class
                    // — its canonical leaf count exceeds the `PALW_STEP_MAX_LEAVES` (2^22) opening cap,
                    // so `step_opening_root_capped_v1` refused the very move the responder owed. So a
                    // guilty fused responder could reach the terminal, file nothing, and have its
                    // forged claim final unconvicted — the withholding the finding names. e8's
                    // ADR-0119, on this SAME 6,000 fence, raises that cap to the class's own ladder
                    // (2^26 for dense v5) and ships the responder that builds the root claim, so past
                    // the fence the move IS constructible and withholding it is a genuine default.
                    // The mercy therefore stops applying to the fused terminal past the fence, and the
                    // silence falls through to the conviction arm below (`void_and_slash` + the C-03
                    // court-time charge). **This is safe ONLY with ADR-0119's cap-raise and responder
                    // in the same build — fd requires deep+held to ship as one build, which is exactly
                    // that guarantee.** The unfenced `round() == 0 && weightless` clause is untouched:
                    // it is the opening-rung / weightless-family mercy (M2-5), a different question
                    // that the cap raise does not bear on.
                    // **ADR-0152 §4-ter C1: an answerable held class's opening silence is a default,
                    // past `palw_offence_attribution`, whichever mercy it would otherwise have read.**
                    // Both mercies below doubt that the responder CAN move; for such a class the move
                    // is the held root claim, which its windowed builder makes (and which a producer
                    // that cannot make it does not underwrite — `supports_dissection` is held-aware).
                    crate::palw_bisect::PalwBisectPartyV1::Responder
                        if !(owes_the_dissection_opening
                            && palw_held_claim_is_answerable_v2(&builder.state, builder.params, builder.extras, &claim))
                            && ((no_phase && session.ladder.round() == 0 && !class_holds_weight)
                                || (builder.extras.court_responder_coverage_active
                                    && owes_the_dissection_opening
                                    // The deep fence lifts the fused-terminal mercy — but ONLY where an
                                    // honest responder could build the root-claim evidence. Below
                                    // `palw_offence_attribution` a held class cannot (ADR-0121 §7), so its
                                    // mercy is kept even past the deep fence; past it, an answerable held
                                    // class can (C1, `court_root_evidence_is_buildable_v2`).
                                    && !(builder.extras.audit_2026_09_11_deep_active
                                        && builder.state.court_root_evidence_is_buildable_v2(
                                            &claim,
                                            builder.extras.offence_attribution_active,
                                            builder.params,
                                        )))) =>
                    {
                        // Ends the session, convicts nobody, and FINES nobody — see
                        // `rearm_after_unanswered_opening` (audit3 H4). Routing this to the
                        // challenger-side close made the accuser pay for the accused's silence,
                        // which on a class with no responder is a certainty rather than a risk.
                        rearm_after_unanswered_opening(builder, ctx, session.claim, &claim)?;
                    }
                    crate::palw_bisect::PalwBisectPartyV1::Responder => {
                        // Same treatment a proven fault gets, and for the same reason: an
                        // executor that will not stand behind its own announced root at the
                        // index it was asked about has not been convicted of arithmetic, but it
                        // HAS defaulted on the only defence available to it. A late default
                        // against an already-terminal claim closes the session and changes
                        // nothing else, exactly as a late verdict does. Past
                        // `palw_offence_attribution` it is recorded as the default it is
                        // ([`palw_court_default_void_reason_v1`]) — charged the same, but never a
                        // proof a `Valid` signer can be convicted by.
                        if !claim.phase.is_terminal() {
                            let reason = palw_court_default_void_reason_v1(builder.extras.offence_attribution_active);
                            builder.void_and_slash(session.claim, &claim, ctx.daa_score, reason)?;
                            // C-03 (deep fence): the defaulting responder also pays for the time it
                            // kept the session open (a no-op below the fence).
                            builder.charge_court_time_v1(claim.bond, claim.reserved, session.opened_daa, ctx.daa_score)?;
                        }
                    }
                    crate::palw_bisect::PalwBisectPartyV1::Challenger => {
                        rearm_after_challenger_side_close(
                            builder,
                            ctx,
                            session.claim,
                            &claim,
                            session.challenger_bond,
                            session.opened_daa,
                        )?;
                    }
                }
                continue;
            }
            // The machine that owns the turn — the ladder, or the dissection phase once one is
            // open — is already `Abandoned`: its rung clock is spent and only the backstop can
            // still act. Rewriting the record re-indexes it under the backstop, so the queue
            // moves forward instead of spinning on a deadline nothing consumes.
            if session.deadline_daa >= ctx.daa_score {
                let refreshed = session.clone();
                builder.write_court(session_id, Some(refreshed))?;
                continue;
            }
        }
        // **ADR-0080 design A: a failed declaration loses on its OWN side.**
        //
        // A close is legal only at `Terminal`, where the rung clock is off and the backstop is the
        // only thing that fires — and the backstop closes CHALLENGER-side. So an EXECUTOR that
        // declares a close and then does not assemble it would end the session against its own
        // prosecutor: it could suspend nothing, spend nothing, and win. That is not a residual, it
        // is the mechanism inverted into a weapon, and it is the one thing the declaration must
        // not make possible.
        //
        // So a live, INCOMPLETE group decides the direction: the declarer defaulted on a close it
        // announced and pinned every byte of, which is the `Responder` treatment the rung sweep
        // already gives an executor that will not stand behind its own root. A CHALLENGER that
        // fails the same way lands on the challenger-side close it would have landed on anyway.
        //
        // **A COMPLETE group cannot still be here.** W7 assembles and adjudicates in the block that
        // carries the last chunk — the group is gone by the time the session ends, whichever way it
        // went — so "incomplete" is the only state a group can be in when the backstop fires, and
        // it is exactly the offence this arm exists for.
        //
        // **A BELT since audit H-2c**, not the strap: `sweep_court_close_deadlines` runs first and
        // convicts at the group's own `assembly_deadline_daa`, which is never above this backstop.
        // The one case that still reaches here is the two being the same DAA. Kept rather than
        // deleted because deleting it would make the direction of this ending depend on which of
        // two sweeps happened to fire, and L-9 is closed either way: `write_court` charges every
        // group standing at the removal, not just the side this conviction routes to.
        let failed_declarer_side = [PalwCourtSideV1::Challenger, PalwCourtSideV1::Executor]
            .into_iter()
            .find(|side| builder.state.court_close_groups.get(&(session_id, *side)).is_some_and(|group| !group.is_complete()));
        // **W10: the assembly reserve is collected, and the direction is W5's.** One spelling with
        // the completing-chunk path, so the two cannot disagree about which bond a failed
        // declaration costs — see [`convict_close_declarer_v1`].
        if let Some(side) = failed_declarer_side {
            convict_close_declarer_v1(builder, ctx, session_id, side)?;
            continue;
        }
        builder.write_court(session_id, None)?;
        // The backstop's own direction, unchanged for a session that declared no close at all.
        rearm_after_challenger_side_close(builder, ctx, session.claim, &claim, session.challenger_bond, session.opened_daa)?;
    }
    Ok(())
}

pub(super) fn sweep_deadlines(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    // One at a time, smallest (deadline, claim) first: resolving a claim mutates the set, and
    // the (deadline, claim) order is what makes the sweep identical on every node.
    while let Some(&(deadline, claim_id)) = builder.state.deadlines.iter().next() {
        if deadline >= ctx.daa_score {
            break;
        }
        builder.state.deadlines.remove(&(deadline, claim_id));
        let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
        // Lane PL part C × G14 (SHARD): no uncharged end of a held lane-A claim while an accusation is pending — DL-1 arms none
        // (unreachable by the derivation); skipped consistently with it, the end of the last accusation re-arms.
        if !claim.phase.is_terminal() && builder.state.palw_v2_expiry_held_v1(builder.params, &claim_id, &claim) {
            continue;
        }
        if builder.state.court_service_holds_claim_v1(&claim_id) { continue; }
        match claim.phase {
            PalwClaimPhaseV2::Provisional => {
                let reason = builder.bind_timeout_reason(&claim, ctx);
                builder.void_claim(claim_id, &claim, ctx.daa_score, reason)?;
            }
            PalwClaimPhaseV2::PanelBound { .. } => {
                // The receipt window closed with no concluding object. Nothing here can tell WHY:
                // the chain cannot observe silence (`slash_silent_seats`), and `Incapable` is a
                // legitimate answer that is never charged and counts toward neither quorum. So a
                // panel that says nothing and a panel that honestly cannot judge are the same
                // event from the transition's side, and charging on that ambiguity is what the
                // no-show slash was removed for.
                //
                // **But voiding is not neutral either — it lands the whole loss on the producer**,
                // who did the work, served the material and was assigned this panel by sortition.
                // That made silence the seats' best play and left an attacker a free move: take
                // enough seats to deny every quorum, say nothing, and burn other people's escrow
                // at no cost. Neither slashing nor voiding answers it, because both pick a side
                // of a question the chain cannot decide.
                //
                // A REDRAW does not have to decide it. The claim returns to `Provisional` and
                // binds a second panel anchored on this sweep, so the seats are different ones
                // (see `rebound_daa`): honest incapacity gets a panel that can judge, and silence
                // stops being effective because it no longer decides anything. It costs one more
                // `window_bind + window_receipt`, which `palw_v2_lattice_fits_pruning_horizon`
                // now holds against the pruning horizon.
                //
                // Once. A claim that times out again has had two independent panels fail to
                // conclude, and a rule that redrew forever would be a claim that never resolves —
                // the reservation held, the escrow neither paid nor burned, indefinitely.
                if claim.rebound_daa.is_none() {
                    // ADR-0124 Decision 3: the first panel's seats leave duty with their exposure
                    // — the redraw deals different seats, who go on duty when it binds.
                    builder.cancel_panel_recipients_v1(claim_id)?;
                    builder.release_seat_duties(&claim_id)?;
                    let mut revived = claim.clone();
                    revived.phase = PalwClaimPhaseV2::Provisional;
                    revived.rebound_daa = Some(ctx.daa_score);
                    let deadline = revived
                        .bind_base_daa()
                        .checked_add(builder.params.window_bind)
                        .ok_or(PalwStateV2Error::Overflow("redraw bind deadline"))?;
                    builder.write_claim(claim_id, Some(revived))?;
                    builder.arm_deadline(deadline, claim_id)?;
                } else if builder.params.panel_unavailable_expiry_active_at(ctx.daa_score) {
                    // **Lane PL part C (ADR-0166): two panels failing to conclude is verifier unavailability, not producer
                    // fraud.** The chain cannot tell a silent Sybil seat or an honest outage from a withholding producer by
                    // this event alone (ADR-0064), and charging the producer for it let ANY seat holder burn an honest
                    // producer's reservation by saying nothing — so heavier models, whose panels are silent more often, were
                    // the more dangerous to run. The claim expires: no reward, the reservation and bond lock returned at once
                    // (`PanelUnavailable` keeps no E-4 hold), no slash, no strike; the fee is spent and the seats get nothing.
                    // Withholding is NOT made free by this: an unanswered data-availability demand still convicts through the
                    // DA court (`ProducerWithholding`), and a false execution through the court (`CourtFraud`) — the
                    // producer-fault paths do not run through this sweep.
                    builder.void_claim(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::PanelUnavailable)?;
                } else {
                    builder.slash_silent_seats(&claim_id, &claim, &[])?;
                    // **Past the 2026-09-23 audit fence the second timeout charges the producer**
                    // (the audit's #10): two independent panels failing to conclude is the one
                    // observable a withholding producer cannot avoid, and before the fence it left the
                    // producer's whole reservation to come back untouched. `void_and_slash` takes the
                    // escrow-inclusive reservation on this reason past the fence — and, for a
                    // compute-priced free-prompt claim, its receipt rights (`rights_reserved`, the
                    // audit's #5), which are ~500× the claim's weight reservation on testnet-12.
                    //
                    // **The silent seats are NOT charged** (the route-matrix re-audit's #8 corrects
                    // this comment, which said they were "charged above either way"):
                    // `slash_silent_seats` has an empty body, because the chain cannot observe
                    // silence, and a seat that signed nothing locked nothing and left duty with its
                    // exposure released at the first redraw. So the whole cost of two panels failing
                    // to conclude falls on the producer, whoever caused the silence — a permissionless
                    // seat Sybil pays nothing to cause it. Whether the receipt rights belong in this
                    // availability charge, or only in `CourtFraud` and DA-confirmed
                    // `ProducerWithholding`, is an open decision (see the re-audit report); until it
                    // is taken this is the charge. A BindTimeout is not charged: the chain
                    // binds a panel itself the moment the anchor is reached, so a bind that never
                    // happened is the network's capacity failing, never a producer's choice — and
                    // charging it would charge the victims of a claim that occupied the seats.
                    if builder.extras.audit_2026_09_23_active {
                        builder.void_and_slash(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::ReceiptTimeout)?;
                    } else {
                        builder.void_claim(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::ReceiptTimeout)?;
                    }
                }
            }
            // **ADR-0152 Q-5's gate (M4): an S2 licence is a fast path, never the basis for `Final`.**
            // DL-1 armed this deadline past both supplementary doors, and no set raised the recount
            // to 2 by then, so the claim holds one seat's replay of some segment and nothing more.
            //
            // * **The first panel redraws** (V3S-01), exactly as a first `ReceiptTimeout` would: one
            //   silent, `Sampled` or late partial seat cannot sink an honest S2 claim into S0′ — with
            //   every non-full seat able to upgrade it by a full-replay V2 `Valid`, all four must
            //   withhold, and then a fresh panel judges it.
            // * **The second panel voids `NotReplayBacked`**, S0′ at launch (SR-5): two independent
            //   panels failed to back the claim by replay. The charge is `void_and_slash`'s — the S-4
            //   funnel's S0′ arm, `w + esc + rr` and nothing else — not restated here.
            PalwClaimPhaseV2::ReceiptLicensed { .. } if palw_rcore_licence_awaits_replay_v1(&claim) => {
                debug_assert!(
                    !builder.state.open_courts_by_claim.contains_key(&claim_id),
                    "a claim under court holds no final deadline"
                );
                if builder.state.panel_v3.as_ref().is_some_and(|engine| engine.claim_rows().contains_key(&claim_id)) {
                    // **RFC-0010: a claim the permissionless Panel bound is not redrawn by V2's binder** (there is none for it): an
                    // S2 licence no supplementary set raised to a replay-backed one EXPIRES, uncharged, like a second panel that
                    // did not show up (`PanelUnavailable`: no slash, no strike, the reservation returned).
                    //
                    // **G14 (SHARD): never while an accusation is pending** — the expiry closes every session neutrally. DL-1 arms
                    // no deadline then (unreachable by the derivation); skipped consistently with it, the close re-arms.
                    if builder.state.palw_accusation_pending_v1(&claim_id) {
                        continue;
                    }
                    builder.void_claim(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::PanelUnavailable)?;
                } else if claim.rebound_daa.is_none() {
                    builder.redraw_unreplayed_licence_v1(claim_id, &claim, ctx.daa_score)?;
                } else if builder.params.panel_unavailable_expiry_active_at(ctx.daa_score) {
                    // Lane PL part C: two panels that did not back an S2 licence by replay are verifiers that did not show up —
                    // the licence expires uncharged, as a second silent receipt window does (see above).
                    builder.void_claim(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::PanelUnavailable)?;
                } else {
                    builder.void_and_slash(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::NotReplayBacked)?;
                }
            }
            PalwClaimPhaseV2::ReceiptLicensed { .. } => {
                debug_assert!(
                    !builder.state.open_courts_by_claim.contains_key(&claim_id),
                    "a claim under court holds no final deadline"
                );
                builder.finalize_claim(claim_id, &claim, ctx.daa_score)?;
            }
            // **ADR-0062 SA-3: the disclose window closed with no disclosure on THIS chain.**
            //
            // That is the whole of "silence", and it is checkable precisely because it is the
            // absence of an object from a branch's own fold rather than an observation about the
            // network (contrast ADR-0064 Fact A). Every node recomputing this branch reaches the
            // same verdict at the same DAA, whatever wall-clock time the disclosure did or did not
            // arrive at — DA-5 — because no node-local timer takes part.
            //
            // The producer could not open one committed event of its own trace inside a window
            // wider than twice the finality depth, on a chain where ANY producer could have
            // carried the answer. So the default is confirmed and the claim voids taking
            // `claim.reserved` — never the whole bond (SA-5) — and the accuser, who was right,
            // pays nothing: its reservation comes back untouched, released by `write_claim` when
            // the void takes the claim out of the disputed phase.
            PalwClaimPhaseV2::DefaultDisputed { .. } => {
                builder.void_and_slash(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::ProducerWithholding)?;
            }
            // The ONE terminal claim that legitimately owns a deadline: a free-prompt commitment
            // abandoned at `BindTimeout`, whose collateral hold expires here (audit C5). The
            // deadline fired, so the hold is over by construction — the predicate is asserted
            // rather than re-tested, because a deadline armed by `void_claim` and a hold computed
            // from the record are the same arithmetic.
            // **Guarded on WHICH deadline fired, not only on the phase.** An abandoned free-prompt
            // claim owns TWO deadlines over its life — the hold's, then its retirement's — and this
            // arm sits above the generic terminal-retirement arm, so without the equality below the
            // second one re-entered here. `release_abandon_hold` then read the entry it had already
            // removed as `unwrap_or(0)` and `0.checked_sub(reserved)` returned `None`: a transition
            // that is a pure function of (parent state, daa) fails identically on every node, so the
            // block is disqualified everywhere and the chain never advances again. The shipped
            // bundle satisfies both preconditions (`FP_ABANDON_HOLD` 600, `CLAIM_RETIREMENT` 2400)
            // and no test covered the pair: the hold test runs with retirement off and the three
            // retirement tests run with the hold at zero.
            //
            // The `debug_assert!` below could not catch it — it is true on both entries — which is
            // the shape this codebase keeps finding: a gate whose predicate cannot distinguish the
            // case it exists to distinguish.
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, voided_daa }
                if matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. })
                    && builder.params.fp_abandon_hold_daa > 0
                    && deadline == voided_daa.saturating_add(builder.params.fp_abandon_hold_daa) =>
            {
                debug_assert!(
                    !palw_claim_is_on_abandon_hold_v2(&claim, builder.params, ctx.daa_score),
                    "the hold's own deadline fired while the record still says it is held"
                );
                builder.release_abandon_hold(&claim)?;
                // The hold is over; the record now owes its retirement entry, which is strictly
                // later (`with_claim_retirement_daa` enforces the ordering) so this loop still
                // terminates. A no-op on a network that does not retire.
                builder.arm_retirement(claim_id, &claim)?;
            }
            // **ADR-0160 E-4: an attempt's obligation hold expires here**, guarded on WHICH deadline
            // fired exactly as the free-prompt arm above is (the retirement entry follows it and must
            // not re-enter): the terminal release is the same one, then the retirement is armed
            // (`validate_palw_v2` keeps `claim_retirement_daa` past `h_obl`).
            PalwClaimPhaseV2::Voided { .. }
                if crate::palw_escrow_funding_v2::palw_claim_obligation_release_at_v1(&claim, builder.params) == Some(deadline) =>
            {
                debug_assert!(
                    !crate::palw_escrow_funding_v2::palw_claim_obligation_hold_v1(&claim, builder.params, ctx.daa_score),
                    "the obligation hold's own deadline fired while the record still says it is held"
                );
                builder.release_abandon_hold(&claim)?;
                builder.arm_retirement(claim_id, &claim)?;
            }
            PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. } if builder.params.claim_retirement_daa > 0 => {
                // Launch blockers §8: the retirement. Nothing reads a terminal claim, and keeping
                // it costs a re-hash and a re-serialization of the whole registry on every block.
                let effective = palw_exec_v2_rewards_v1::retirement(
                    &builder.state,
                    builder.params,
                    &claim_id,
                    terminal_deadline_at_v1(&claim, builder.params, Some(ctx.daa_score))?,
                    &claim,
                )?;
                // G-15: an unpaid Final is not retired (`palw_unpaid_reward_retirement_floor_v1`, the same floor DL-1 derives).
                let effective = super::palw_unpaid_reward_retirement_floor_v1(
                    &builder.state,
                    builder.params,
                    &claim_id,
                    &claim,
                    Some(ctx.daa_score),
                    effective,
                    true,
                );
                if let Some(at) = effective.filter(|at| *at >= ctx.daa_score) {
                    builder.arm_deadline(at, claim_id)?;
                    continue;
                }
                // RFC23: retirement must not discard a certified consumer's
                // held amount while another actual liability is still open.
                if let Some(at) = super::palw_security_certification_fold_v1::try_release_claim_local_reward_v1(
                    builder,
                    ctx.daa_score,
                    &claim_id,
                    &claim,
                    true,
                )? {
                    builder.arm_deadline(at, claim_id)?;
                    continue;
                }
                // G-15 (`palw_reward_liability_horizon_v1`, dormant): a certified reward still deferred to the liability horizon is
                // named now — a retired claim can no longer be convicted, so retirement closes its liability window too. A no-op below
                // the fence, for a voided claim, and once the Gate C sweep has named it.
                super::palw_security_certification_fold_v1::release_deferred_certified_reward_at_retirement_v1(
                    builder,
                    ctx.daa_score,
                    &claim_id,
                    &claim,
                )?;
                builder.retire_claim(claim_id, &claim)?;
            }
            PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. } => {
                // Any other terminal claim owns no deadline; finding one is index corruption.
                return Err(PalwStateV2Error::CarriageInconsistent(format!("terminal claim {claim_id} held a deadline")));
            }
        }
    }
    Ok(())
}
