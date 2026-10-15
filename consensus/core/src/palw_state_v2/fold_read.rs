//! **The fold's read-only view (impl PalwFoldReadV1)** — moved out of `palw_state_v2.rs` (2026-10-12, a behaviour-preserving split: the items are the same; members the parent calls are `pub(super)`).

use super::*;

impl PalwFoldReadV1<'_> {
    /// **RFC-0008 v2 (X8R), a TEST SEAM: is `class_id` one whose REAL attempts skip the registry, seating and share gates?** Only
    /// where the processor's `cfg(test)` hook named it (`PalwKernelRouteExtrasV1::test_admitted_classes`, empty in every build that is
    /// not a test, and absent wherever the kernel route is not in force) — the composed EXEC-slice × G14 run's stand-in for the legacy
    /// channel's Panel admission of the session's V2 REAL root (the Lead's GAP-81 decision: an onboarded class's V2 claims earn only
    /// there; the harness cannot prove seat readiness).
    pub(super) fn test_admitted_class_v1(&self, class_id: &Hash64) -> bool {
        self.extras.kernel_route.as_ref().is_some_and(|route| route.test_admitted_classes.contains(class_id))
    }

    /// **ADR-0152 S-SPEC §2 `claim_g_v1`: a claim's gain terms for the action tiers, FROZEN** (the
    /// S-4 review) — [`palw_claim_g_v1`]: the liability row's copy, else the claim record's
    /// licence-time `G_res`, else (unlicensed) its acceptance records' `w + R`. `G = g_res +
    /// escrowed_reward`. The builder's `claim_g_v1` is this read; read here so the accuser gates
    /// outside the builder price a held session with the same `G` (ADR-0152 §4-ter, the review's F7).
    pub(super) fn claim_g_v1(&self, claim_id: &Hash64) -> Option<PalwClaimGV1> {
        palw_claim_g_v1(self.state, &crate::palw_weight_cap_v1::PalwCapacityGainScaleV1::of(self.params), claim_id)
    }

    /// **ADR-0152 §4-ter C4: what a held dissection's losing challenger is charged, before the
    /// floor's cap** — `max(claim.reserved, G)`, `G = g_res + escrowed_reward` ([`Self::claim_g_v1`]).
    /// The one spelling for the charge (`rearm_after_challenger_side_close`), the reservation that
    /// makes it certain (`open_held_dissection_v1`) and every gate that counts a held session
    /// ([`Self::held_accuser_surplus_v1`]).
    pub(super) fn held_dissection_charge_v1(&self, claim_id: &Hash64, claim: &PalwClaimStateV2) -> u128 {
        let g = self.claim_g_v1(claim_id).map(|gains| gains.g()).unwrap_or(0);
        claim.reserved.max(g)
    }

    /// **Does this block charge a held class's session at `max(reserved, G)`?** Past
    /// `palw_offence_attribution` on a held network — with the session's claim on a held class
    /// (`class_is_held_v1`, read per session by [`palw_held_accuser_surplus_of_v1`]), exactly where
    /// `rearm_after_challenger_side_close` charges it so. Lane V02: the processor resolves the same
    /// pair for the readers outside the fold (`palw_held_accuser_charge_at`).
    pub(super) fn held_charge_block_active_v1(&self) -> bool {
        self.extras.offence_attribution_active && self.extras.held_context_ladder.is_some()
    }

    /// **ADR-0152 §4-ter C4 (the review's F7): what `bond`'s open held dissections hold beyond the
    /// court index's count.** [`palw_accuser_exposure_v1`] counts every session its bond challenges
    /// at the claim's `reserved` (what `write_court` releases); a held one costs its losing
    /// challenger `min(max(reserved, G), floor)` (C4) — on an 8k claim ≈ 3,250 MSK more. Past
    /// `palw_rcore_plus` every accuser gate adds this surplus to the accuser ledger, so no DA
    /// accusation, court opening, work gate or declaration spends room a held session's charge
    /// already needs. Zero below either fence.
    pub(super) fn held_accuser_surplus_v1(&self, bond: &PalwBondKeyV2, now_daa: u64) -> u128 {
        if !self.params.rcore_plus_active_at(now_daa) {
            return 0;
        }
        // Lane V02: the block's half here, the claim's half (`class_is_held_v1`) in the one pure
        // expression the readers outside the fold (the draw's filter, admission item 8, the
        // producer's facts) call too.
        if !self.held_charge_block_active_v1() {
            return 0;
        }
        palw_held_accuser_surplus_of_v1(
            self.state,
            &crate::palw_weight_cap_v1::PalwCapacityGainScaleV1::of(self.params),
            bond,
            self.params.min_collateral_sompi(),
        )
    }

    /// **The accuser ledger every gate reads past `palw_rcore_plus`** — [`palw_accuser_exposure_v1`]
    /// with each open held dissection counted at its charge ([`Self::held_accuser_surplus_v1`]).
    pub(super) fn accuser_ledger_v1(&self, bond: &PalwBondKeyV2, now_daa: u64) -> u128 {
        palw_accuser_exposure_v1(self.state, bond).saturating_add(self.held_accuser_surplus_v1(bond, now_daa))
    }

    /// **ADR-0152 DA-1 / DA-6 / DA-8 (M3): may `accuser` open a session on `claim_id` at `now_daa`,
    /// and at what stage and price?** The one gate [`palw_da_accusation_admissible_v2`] exports and the
    /// fold's opening reads, so the filer's fee-safe de-duplication and the fold cannot disagree.
    ///
    /// * **Accusable (DA-8):** a panel is bound — `PanelBound`, `ReceiptLicensed`, or a redrawn
    ///   `Provisional` — or the claim has an open public post-Final stage (unmatured vesting, or FP liability past the filer fence); never a
    ///   voided claim. `now + W_disclose ≤ trace_retention_daa`.
    /// * **The accuser (DA-8):** not the producer; Active and at or above the floor; with no session
    ///   of its own open on the claim; a seat of the claim's CURRENT panel within its four sessions
    ///   (and exempt from the lifetime cap), anyone else within three open and sixteen ever.
    /// * **The price (DA-6):** `min(⌈r · S_P(stage)⌉, min_collateral)`, checked on the accuser's free
    ///   half: `max(committed, 500‰ · C) + accuser exposure + new ≤ C` (A-6, review M1) — the ONE
    ///   accuser gate every accusation reads, [`palw_rcore_gate_room_of_v1`] with
    ///   [`PalwRcoreGateV1::Accuser`] (the court's `check_accuser_room` reads the same).
    pub(super) fn da_admission_v1(
        &self,
        claim_id: &Hash64,
        accuser: &PalwBondKeyV2,
        now_daa: u64,
    ) -> Result<PalwDaAdmissionV1, PalwStateV2Error> {
        self.da_admission_mode_v1(claim_id, accuser, now_daa, false)
    }

    /// **Lane G14-L: may `accuser` join one more LANE to its open reserved session on `claim_id`, and at what price?** Exactly
    /// [`Self::da_admission_v1`]'s gates and DA-6 price on the free half, except the two a lane replaces: the open session is the one
    /// it joins (DA-1), and the lane counts on the reservation's lane budget, not its session count
    /// ([`crate::palw_legacy_parallel_v1::palw_legacy_lane_join_check_v1`]).
    pub(super) fn da_lane_admission_v1(
        &self,
        claim_id: &Hash64,
        accuser: &PalwBondKeyV2,
        now_daa: u64,
    ) -> Result<PalwDaAdmissionV1, PalwStateV2Error> {
        self.da_admission_mode_v1(claim_id, accuser, now_daa, true)
    }

    pub(super) fn da_admission_mode_v1(
        &self,
        claim_id: &Hash64,
        accuser: &PalwBondKeyV2,
        now_daa: u64,
        lane: bool,
    ) -> Result<PalwDaAdmissionV1, PalwStateV2Error> {
        use crate::palw_da_rcore_v1::{
            PALW_DA_OPEN_NON_SEAT_PER_CLAIM_V1, PALW_DA_SESSIONS_PER_CLAIM_TOTAL_V1, PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1,
            PalwDaStageV1, palw_da_session_exposure_at_bps_v1, palw_da_stage_reward_base_v1,
        };
        let state = self.state;
        let claim = state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?;
        let panel = state.panels.get(claim_id);
        let stage = match claim.phase {
            PalwClaimPhaseV2::Provisional if claim.rebound_daa.is_some() && panel.is_some() => PalwDaStageV1::Live,
            PalwClaimPhaseV2::PanelBound { .. } => PalwDaStageV1::Live,
            PalwClaimPhaseV2::ReceiptLicensed { .. } => PalwDaStageV1::Licensed,
            PalwClaimPhaseV2::Final { .. }
                if palw_legacy_public_filer_fold_v1::palw_final_public_dispute_open_v1(
                    state,
                    self.params,
                    claim_id,
                    claim,
                    now_daa,
                ) =>
            {
                PalwDaStageV1::FinalRow
            }
            PalwClaimPhaseV2::Voided { .. } | PalwClaimPhaseV2::DefaultDisputed { .. } => {
                return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "DefaultAccused" });
            }
            PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::Final { .. } => {
                return Err(PalwStateV2Error::DaClaimNotAccusable(*claim_id));
            }
        };
        let disclose = palw_da_disclose_window_daa_v1(self.params);
        let deadline_daa = now_daa.checked_add(disclose).ok_or(PalwStateV2Error::Overflow("da disclose deadline"))?;
        palw_exec_v2_rewards_v1::require_native_da_window(state, self.params, claim_id, deadline_daa, now_daa)?;
        if now_daa < claim.accepted_daa || deadline_daa > claim.trace_retention_daa {
            return Err(PalwStateV2Error::DaOutsideRetention {
                claim: *claim_id,
                at: now_daa,
                window: disclose,
                retention_daa: claim.trace_retention_daa,
            });
        }
        let panel = panel.ok_or(PalwStateV2Error::DaClaimNotAccusable(*claim_id))?;
        if panel.seats.is_empty() {
            return Err(PalwStateV2Error::EmptyPanel);
        }
        if *accuser == claim.bond {
            return Err(PalwStateV2Error::DaAccuserIsTheProducer(*accuser));
        }
        let record = state.bonds.get(accuser).ok_or(PalwStateV2Error::MissingBond(*accuser))?;
        if !matches!(record.status, PalwBondStatusV2::Active) {
            return Err(PalwStateV2Error::BondNotActive(*accuser));
        }
        let floor = self.params.min_collateral_sompi();
        if record.collateral < floor {
            return Err(PalwStateV2Error::BondBelowFloor { bond: *accuser, collateral: record.collateral, floor });
        }
        if state.da_sessions.contains_key(&(*claim_id, *accuser)) != lane {
            return Err(if lane {
                PalwStateV2Error::DaSessionBudgetExhausted {
                    claim: *claim_id,
                    accuser: *accuser,
                    why: "a lane joins the accuser's open reserved session, and none is open",
                }
            } else {
                PalwStateV2Error::DaSessionAlreadyOpen { claim: *claim_id, accuser: *accuser }
            });
        }
        let accuser_is_seat = panel.seats.iter().any(|seat| seat.bond == *accuser);
        let da = state.da_claims.get(claim_id);
        let exhausted = |why| Err(PalwStateV2Error::DaSessionBudgetExhausted { claim: *claim_id, accuser: *accuser, why });
        // Lane LG14-A: a non-seat whose dispute reservation on the claim is live opens RESERVED sessions, on the reservation's own
        // budget — never DA-8's shared non-seat budget, which a producer's Sybils could exhaust. Empty below the fence.
        let reserved = (!accuser_is_seat).then(|| state.legacy_dispute_reservation_v1(claim_id, accuser)).flatten();
        if accuser_is_seat {
            if lane {
                return exhausted("a seat's sessions carry no lanes: lanes are a non-seat reservation's");
            }
            if da.and_then(|record| record.opened_by_seat.get(accuser)).copied().unwrap_or(0) >= PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1
            {
                return exhausted("a seat opens at most four sessions on a claim over its life");
            }
        } else if let Some(row) = reserved {
            if !lane && row.sessions_opened >= crate::palw_legacy_public_filer_v1::PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1 {
                return exhausted("the dispute reservation exhausted its own session budget on this claim");
            }
        } else if lane {
            return exhausted("a lane joins only a reserved session of a non-seat bond");
        } else {
            if da.map(|record| record.open_other_sessions).unwrap_or(0) >= PALW_DA_OPEN_NON_SEAT_PER_CLAIM_V1 {
                return exhausted("at most three non-seat sessions are open on a claim at once");
            }
            if da.map(|record| record.opened_non_seat_total).unwrap_or(0) >= PALW_DA_SESSIONS_PER_CLAIM_TOTAL_V1 {
                return exhausted("at most sixteen non-seat sessions open on a claim over its life");
            }
        }
        let full = palw_claim_bond_reservation_v1(self.params, claim).ok_or(PalwStateV2Error::Overflow("da reward base"))?;
        let producer_collateral = state.bonds.get(&claim.bond).map(|bond| bond.collateral).unwrap_or(0);
        // DA-6's base reads the frozen `G` the tiers read (the S-4 review), never the live gain.
        let g = palw_claim_g_v1(state, &crate::palw_weight_cap_v1::PalwCapacityGainScaleV1::of(self.params), claim_id)
            .map(|gains| gains.g())
            .unwrap_or(0);
        // The share is fixed by the session's open (`now_daa`, the accusation block): 10% below
        // `Params::palw_reporter_share_v2`, 49% at or past it (lane INTF). The session stores it.
        let exposure = palw_da_session_exposure_at_bps_v1(
            palw_da_stage_reward_base_v1(stage, full, producer_collateral, g),
            floor,
            self.params.reporter_reward_bps_at(now_daa),
        );
        // A-6: the accuser's free half — a seat whose 500‰ is full of locks can still accuse, and no
        // bond accuses past its collateral (review M1: courts and the one ledger counted). Through
        // the one accuser gate ([`PalwRcoreGateV1::Accuser`]), the room the court opening and the held
        // dissection read (`TransitionBuilder::check_accuser_room`), refused the way it refuses.
        let committed = palw_bond_committed_v1(state, accuser, now_daa, self.second_clock_depth(now_daa), self.params.window_court);
        let room = palw_rcore_gate_room_of_v1(
            record.collateral,
            self.params.fp_max_exposure_ratio_permille,
            committed,
            self.accuser_ledger_v1(accuser, now_daa),
            PalwRcoreGateV1::Accuser,
        );
        if exposure > room {
            let collateral = u128::from(record.collateral);
            return Err(PalwStateV2Error::AccusationExposureCeiling {
                bond: *accuser,
                edge: "data-availability session",
                backed: collateral.saturating_sub(room),
                accusation: exposure,
                ceiling: collateral,
            });
        }
        Ok(PalwDaAdmissionV1 { stage, accuser_is_seat, exposure, deadline_daa })
    }

    /// **ADR-0152 X7 / DA-4 / DA-7 (M3): is `lock` live for the data-availability court at
    /// `now_daa`?** Both clocks, the second bounded per obligation (`is_live_v3`) at the ESCAPED
    /// depth ([`Self::second_clock_depth`]) — the one reading behind who may answer (X7) and who a
    /// default charges (C7), so the fold and Phase 2's responder (P2-7) cannot disagree about a
    /// signer's standing.
    pub(super) fn da_lock_live_v1(&self, lock: &crate::palw_panel_var_v1::PalwSlashableLockV1, now_daa: u64) -> bool {
        lock.is_live_v3(now_daa, self.state.settled_attempt_finals, self.second_clock_depth(now_daa), self.params.window_court)
    }

    /// **X7 (DA-4): may `discloser` answer a session on `claim_id`?** The claim's producer, or a bond
    /// still liable on it — a live lock ([`Self::da_lock_live_v1`]).
    pub(super) fn da_discloser_liable_v1(
        &self,
        claim_id: &Hash64,
        claim: &PalwClaimStateV2,
        discloser: &PalwBondKeyV2,
        now_daa: u64,
    ) -> bool {
        *discloser == claim.bond
            || self.state.slashable_locks.get(&(*discloser, *claim_id)).is_some_and(|lock| self.da_lock_live_v1(lock, now_daa))
    }

    /// **C7 (DA-7): is `lock` a covering signer's for any of `units`?** Live
    /// ([`Self::da_lock_live_v1`]) and its recorded mask covers at least one of them
    /// ([`crate::palw_da_rcore_v1::palw_da_unit_covered_by_v1`]) — what a default charges S4 on, and
    /// so exactly what a signer owes an answer to (P2-7).
    pub(super) fn da_lock_covers_v1(
        &self,
        lock: &crate::palw_panel_var_v1::PalwSlashableLockV1,
        units: &[crate::palw_da_rcore_v1::PalwDaUnitV1],
        now_daa: u64,
    ) -> bool {
        self.da_lock_live_v1(lock, now_daa)
            && units.iter().any(|unit| crate::palw_da_rcore_v1::palw_da_unit_covered_by_v1(unit, lock.attested, lock.segments))
    }

    pub(super) fn canonical_per_draw(&self, class_id: &Hash64, accepted_daa: u64) -> Option<u64> {
        self.state
            .palw_attempt_per_draw_v1(
                &self.params.base_class_id,
                class_id,
                accepted_daa,
                self.extras.canonical_work_daa,
                self.base_known_draw(),
            )
            .map(|work| work.min(u64::MAX as u128) as u64)
    }

    pub(super) fn base_known_draw(&self) -> Option<u128> {
        self.extras
            .model_registry
            .as_ref()
            .and_then(|fold| fold.genesis_works.get(&self.params.base_class_id))
            .map(|work| work.economic_ccu_per_claim)
    }

    pub(super) fn exposure_basis(&self, accepted_daa: u64) -> Option<PalwExposureBasisV1> {
        self.state.palw_exposure_basis_v2(
            &self.params.base_class_id,
            accepted_daa,
            self.extras.canonical_work_daa,
            self.base_known_draw(),
        )
    }

    pub(super) fn settled_anchor_depth(&self) -> Option<u64> {
        if self.extras.audit_2026_09_23_active { self.extras.settled_anchor_depth } else { None }
    }

    /// The work target's floor at this block (ADR-0132/0137) — the value the class target is read
    /// against — `None` where the work target is dormant. (The builder's `work_target_floor`.)
    pub(super) fn work_target_floor(&self, ctx: &PalwBlockContextV2) -> Option<u128> {
        if !self.extras.work_target_active {
            return None;
        }
        let rate = self
            .extras
            .economic_payout
            .map(|fold| fold.rate_sompi_per_giga)
            .or_else(|| self.extras.work_target.as_ref().map(|fold| fold.rate_sompi_per_giga))
            .unwrap_or(crate::palw_work_target_v1::PALW_WORK_TARGET_SHADOW_RATE_SOMPI_PER_GIGA_V1);
        let floor = palw_work_floor_for_block_v1(self.params, ctx.subsidy, self.extras.escrow_carve, rate);
        // ADR-0132 S: with the network draw gone the stepped `W` is what holds the cadence, and it
        // is never below the block's `W₀`.
        if self.extras.single_lottery_active {
            return Some(floor.max(self.state.work_target.map(|target| target.work).unwrap_or(0)));
        }
        Some(floor)
    }

    /// **§3.6 `G_eq` (D6, S-4): what a FRESH attempt of `class_id` would put at stake at this block**
    /// — `E + w + R`: option A's escrow term at this block's subsidy (the GAIN,
    /// [`PalwStateParamsV2::claim_escrow_reservation_v1`] — past `palw_capacity_escrow_at_licence` the
    /// admission ceiling reads `m_c` instead, and Eq keeps its cap on the gain, ADR-0160 AG-2), the
    /// weight reservation `apply_attempt` would write for it (at the pwu the producer's
    /// facts derive: `palw_attempt_derived_pwu_v1` past the canonical-work height, the class's rule
    /// before it; `× attempts` past the 2026-09-23 audit), and that claim's realizable rights. A
    /// `class_id` that is not a registered class prices the base class. A class the chain cannot price
    /// an attempt of (no derived draw) contributes its escrow alone.
    pub(super) fn eq_cap_basis_v1(&self, ctx: &PalwBlockContextV2, class_id: &Hash64) -> u128 {
        let class_id = if self.state.classes.contains_key(class_id) { *class_id } else { self.params.base_class_id() };
        let escrow = self
            .params
            .claim_escrow_reservation_v1(ctx.daa_score, palw_claim_escrow_v1(self.params, ctx.subsidy, self.extras.escrow_carve));
        let Some(class) = self.state.classes.get(&class_id) else { return escrow };
        let canonical = self.canonical_per_draw(&class_id, ctx.daa_score);
        let target =
            crate::palw_admission_v2::palw_effective_class_target_v1(self.state, self.params, &class_id, self.work_target_floor(ctx))
                .ok();
        let past_the_unit = self.extras.canonical_work_daa.is_some_and(|height| ctx.daa_score >= height);
        let pwu = if past_the_unit {
            let draw = self.state.palw_attempt_per_draw_v1(
                &self.params.base_class_id,
                &class_id,
                ctx.daa_score,
                self.extras.canonical_work_daa,
                self.base_known_draw(),
            );
            match (target, draw) {
                (Some(target), Some(draw)) => crate::palw_admission_v2::palw_attempt_derived_pwu_v1(target, draw),
                _ => return escrow,
            }
        } else {
            match (&class.pwu_rule, target) {
                (PalwPwuRuleV2::DerivedV1 { pwu_per_inference }, Some(target)) => {
                    crate::palw_pwu::palw_pwu_v1(target, *pwu_per_inference)
                }
                (PalwPwuRuleV2::MaxPerAttempt(cap), _) => *cap,
                (PalwPwuRuleV2::DerivedV1 { .. }, None) => return escrow,
            }
        };
        let attempts = if self.extras.audit_2026_09_23_active { crate::palw_pwu::palw_claim_attempts_v1(pwu, canonical) } else { 1 };
        let w = u128::from(palw_exposure_pwu_v3(class, pwu, canonical, self.exposure_basis(ctx.daa_score)))
            .saturating_mul(u128::from(class.slash_value_per_pwu))
            .saturating_mul(u128::from(attempts));
        let rights = match self.extras.economic_safety {
            Some(safety) => self.realizable_rights_of_v1(&class_id, pwu, ctx.daa_score, &safety),
            None => 0,
        };
        escrow.saturating_add(w).saturating_add(rights)
    }

    /// The second clock's depth at `now_daa` AFTER the liveness escape
    /// ([`palw_second_clock_depth_v1`], 2026-09-24 DoS audit): `None` below the fence, and `None`
    /// once no anchor has settled for `2 × window_court`. Every lock, liability and retirement
    /// reads its clocks through this, and so does the draw ([`palw_second_clock_depth_of_v1`]).
    pub(super) fn second_clock_depth(&self, now_daa: u64) -> Option<u64> {
        palw_second_clock_depth_v1(self.settled_anchor_depth(), &self.state.recent_anchor_daas, now_daa, self.params.window_court)
    }

    /// **What one Valid signature on this claim must lock** (ADR-0144 §9, ADR-0151 D1/D4).
    ///
    /// Three colluding seats must out-value the most the lie can earn
    /// (`palw_colluding_quorum_covers_v1`), and past `Params::palw_economic_safety` that gain
    /// includes the execution rights the Final mints — which
    /// `palw_claim_extra_economic_rights_v1` valued at zero, on a claim whose Final mints 270,029
    /// permit candidates. The margin also stops being one sompi.
    pub(super) fn panel_valid_lock_required(&self, claim: &PalwClaimStateV2) -> u128 {
        self.panel_valid_lock_required_for(claim, crate::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1)
    }

    /// [`Self::panel_valid_lock_required`] for a colluding set of `colluding` `Valid` signers:
    /// `palw_seat_lock_required_v2(G, colluding)`. Every caller below `palw_audit_2026_09_23` asks
    /// for [`crate::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1`], and at that count this is the
    /// function it replaced, byte for byte — the dormant branch still calls
    /// `palw_panel_seat_required_v1`, not an arithmetic twin of it.
    pub(super) fn panel_valid_lock_required_for(&self, claim: &PalwClaimStateV2, colluding: u64) -> u128 {
        let slash = self.state.classes.get(&claim.class_id).map(|c| c.slash_value_per_pwu).unwrap_or(0);
        // **2026-09-23 audit C-3: the weight term in the unit the reservation was written in.**
        // Below the fence the raw derived pwu met the collateral-unit price — 2,810x the unit — and
        // the 2M row's seat lock came to 119.19x the collateral any genesis bond posts, so no panel
        // could ever bind that class. Above it the term is what `reserved` already is.
        let mut facts = if self.extras.audit_2026_09_23_active {
            // ADR-0160 stage 3 (F1): at the claim's full weight (`reserved × ρ`; today's at ρ = 1).
            crate::palw_panel_var_v1::PalwClaimFraudFactsV1::from_claim(claim, slash)
                .at_full_weight_v1(crate::palw_weight_cap_v1::PalwCapacityGainScaleV1::of(self.params).scale_of(claim))
        } else {
            crate::palw_panel_var_v1::PalwClaimFraudFactsV1::from_claim_pre_2026_09_23(claim, slash)
        };
        let Some(safety) = self.extras.economic_safety else {
            // Dormant: byte-identical to the rule every existing lock was written under.
            if colluding == crate::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1 {
                return crate::palw_panel_var_v1::palw_panel_seat_required_v1(&facts);
            }
            return crate::palw_offence_v1::palw_min_slashable_per_colluding_seat_v1(
                crate::palw_panel_var_v1::palw_max_fraud_gain_v1(&facts),
                colluding,
            );
        };
        // The audit's #5: a free-prompt claim's reserved receipt rights are rights its Final realizes
        // too, so the seat that licenses it is priced for them as for an attempt's execution rights.
        facts.extra_economic_rights_sompi = self.claim_realizable_rights_v1(claim, &safety).max(claim.rights_reserved);
        crate::palw_economic_safety_v1::palw_seat_lock_required_v2(crate::palw_panel_var_v1::palw_max_fraud_gain_v1(&facts), colluding)
    }

    /// **ADR-0152 L-1's facts** (S-SPEC §2 `claim_g_v1`): the claim's fraud facts as the seat lock
    /// prices them — the weight in its reservation's unit and, past `palw_economic_safety`, the
    /// execution rights its Final could realize (or its reserved receipt rights, whichever is more).
    /// Both fences are R-core+ prerequisites; without the safety fold the reserved rights stand in.
    pub(super) fn rcore_fraud_facts(&self, claim: &PalwClaimStateV2) -> crate::palw_panel_var_v1::PalwClaimFraudFactsV1 {
        let slash = self.state.classes.get(&claim.class_id).map(|c| c.slash_value_per_pwu).unwrap_or(0);
        // ADR-0160 stage 3 (F1): at the claim's full weight (`reserved × ρ`; today's at ρ = 1).
        let mut facts = crate::palw_panel_var_v1::PalwClaimFraudFactsV1::from_claim(claim, slash)
            .at_full_weight_v1(crate::palw_weight_cap_v1::PalwCapacityGainScaleV1::of(self.params).scale_of(claim));
        facts.extra_economic_rights_sompi = match self.extras.economic_safety {
            Some(safety) => self.claim_realizable_rights_v1(claim, &safety).max(claim.rights_reserved),
            None => claim.rights_reserved,
        };
        facts
    }

    /// **ADR-0152 IMPL-15's buyback bound `s`**: the slice of the escrow a Final buys from the pair of
    /// the claim's line (`palw_model_buyback_slice_v1`, 5% of `E`) where that pair is open to the buy
    /// now — the `model_buyback_at_final` path, bounded by the whole escrow — else 0 (no line, no pair,
    /// a closed pair: the miner is paid the whole escrow and nothing is bought).
    pub(super) fn rcore_buyback_bound(&self, claim_id: &Hash64, claim: &PalwClaimStateV2) -> u64 {
        let slice = crate::palw_model_market_v1::palw_model_buyback_slice_v1(claim.escrowed_reward);
        let open = self
            .state
            .claim_roots
            .get(claim_id)
            .and_then(|root| self.state.artifact_line_of_root(&claim.class_id, root, self.extras.artifact_root_ownership_active))
            // ADR-0162: the pair `model_buyback_at_final` buys from, by the same function.
            .and_then(|line_id| self.model_buyback_market_v2(&line_id))
            .is_some_and(|market| crate::palw_model_market_v1::palw_model_buyback_quote_v1(&market, slice).is_some());
        if open { slice } else { 0 }
    }

    /// **ADR-0152 L-1: the residual fraud gain** `G_res = w + R + s` — `palw_max_fraud_gain_v1(facts)
    /// − escrowed_reward` (which omits `s`) plus the buyback bound (IMPL-15; the S review's M2).
    pub(super) fn rcore_g_res(&self, claim_id: &Hash64, claim: &PalwClaimStateV2) -> u128 {
        crate::palw_panel_var_v1::palw_max_fraud_gain_v1(&self.rcore_fraud_facts(claim))
            .saturating_sub(claim.escrowed_reward as u128)
            .saturating_add(u128::from(self.rcore_buyback_bound(claim_id, claim)))
    }

    /// **ADR-0152 L-1: what one counted `Valid` signer of a set recounted to `basis_k` locks** —
    /// [`palw_rcore_lock_v1`] on this claim's `G_res`, escrow and buyback bound: on the whole gain
    /// while [`PALW_RCORE_VESTING_ROWS_LANDED_V1`] is `false`.
    ///
    /// **ADR-0160 AS-2:** past `Params::palw_capacity_aggregate_liability` at the claim's
    /// `accepted_daa`, [`crate::palw_aggregate_liability_v1::palw_seat_lock_v2`] of that price.
    pub(super) fn rcore_lock(&self, claim_id: &Hash64, claim: &PalwClaimStateV2, basis_k: u8) -> u128 {
        crate::palw_aggregate_liability_v1::palw_seat_lock_v2(
            self.rcore_lock_unreduced(claim_id, claim, basis_k),
            crate::palw_audit_door_v1::palw_capacity_seat_step_v1(self.params, claim),
        )
    }

    /// [`Self::rcore_lock`] before ADR-0160's AS-2 — today's L-1 price, which AS-2 re-prices (and
    /// AS-1's duty reads through AS-2, `palw_seat_duty_v2`).
    pub(super) fn rcore_lock_unreduced(&self, claim_id: &Hash64, claim: &PalwClaimStateV2, basis_k: u8) -> u128 {
        palw_rcore_lock_v1(
            self.rcore_g_res(claim_id, claim),
            claim.escrowed_reward,
            self.rcore_buyback_bound(claim_id, claim),
            basis_k,
        )
    }

    /// **ADR-0152 L-4 / L-4b: a claim's bind-time prices** for a panel of `seat_count` seats at
    /// `now_daa` — the λ-term (ECON's floor half at this block's multiple), `lock_2`, the commitment
    /// the duty is capped by, `duty_bind` and the eligibility `max(duty_bind, lock_2)`.
    pub(super) fn rcore_bind_prices(
        &self,
        claim_id: &Hash64,
        claim: &PalwClaimStateV2,
        seat_count: usize,
        now_daa: u64,
    ) -> PalwRcoreBindPricesV1 {
        // ADR-0160 AS-1/AS-2 (lane liab), keyed on the claim's `accepted_daa`: the lock a counted
        // `Valid` posts is AS-2's; the duty divides λ by ρ and keeps AS-2's lock as its lock term
        // (`palw_seat_duty_v2`, lane liab review 2, finding 3), so a bound seat is backed at the
        // licence by construction whatever the credit. With no step (every shipped preset, every
        // claim accepted below the fence) both are today's.
        // Stage 2 (v3 AS-1′): the step prices a CREDITED claim only; every other claim is priced as today.
        let step = crate::palw_audit_door_v1::palw_capacity_seat_step_v1(self.params, claim);
        let lock_2_unreduced = self.rcore_lock_unreduced(claim_id, claim, PALW_RCORE_FINAL_BASIS_K_V1);
        let lock_2 = crate::palw_aggregate_liability_v1::palw_seat_lock_v2(lock_2_unreduced, step);
        let lambda_term = crate::palw_panel_economy_v1::palw_panel_seat_reward_floor_v1(
            claim.escrowed_reward,
            seat_count,
            self.extras.panel_reward_multiple_permille,
        );
        let commitment = palw_claim_commitment_v1(self.params, claim, now_daa).unwrap_or(u128::MAX);
        let duty_bind =
            crate::palw_aggregate_liability_v1::palw_seat_duty_v2(lambda_term, lock_2_unreduced, commitment, seat_count, step);
        PalwRcoreBindPricesV1 {
            g_res: self.rcore_g_res(claim_id, claim),
            lock_2,
            lambda_term,
            commitment,
            duty_bind,
            eligibility: palw_rcore_seat_eligibility_v1(duty_bind, lock_2),
        }
    }

    /// **2026-09-23 audit #6: what each `Valid` signer of a set arriving through `door` must lock.**
    ///
    /// The lock was priced for three colluders on every door, and the doors do not all need three.
    /// The inequality ADR-0151 D1 rests on is "the smallest set of `Valid` signatures that LICENSES
    /// out-values the gain", and the smallest such set is a property of the door:
    ///
    /// * **V1 `ReceiptLicensed`** — a quorum (`validate_receipt_quorum_v2`, three of five on every
    ///   ConsensusV2 preset). Priced at three, as before.
    /// * **V2 `ReceiptLicensedV2`** — the V1 quorum AND coverage (`validate_receipt_coverage_v2`).
    ///   The coverage half needs the whole panel on testnet-12's `K = seats − 1` assignment
    ///   (five, brute-forced in `dos_l3_quorum_and_rights`), but the fold does not re-derive
    ///   coverage, and "five" is a property of one assignment shape, not of the door. The quorum
    ///   half is the lower bound the door GUARANTEES, so it is priced there — at three. Pricing
    ///   it at five would cut every coverage lock to three-fifths on the strength of a geometry
    ///   the rule never checks.
    /// * **S2 `OptimisticLicensed`** — the full-replay seat's `Valid` is necessary and SUFFICIENT
    ///   (`palw_optimistic_licence_v2`; the processor tolerates `NoQuorum`). The colluding set is
    ///   that one seat (two on an outsider-judged claim, and one is the conservative count), so
    ///   the full seat locks `palw_seat_lock_required_v2(G, 1)` — the whole gain plus the margin
    ///   on one bond. Priced at three it locked 0.367·G: a Final whose escrow the colluders kept
    ///   with 1,515 MSK to spare after the conviction. The other seats of an S2 set are auditors
    ///   the door does not need; their `Valid` locks at the quorum price, as it did, because
    ///   the inequality does not rest on them and pricing them at `G` would cut every auditor's
    ///   concurrency by three for nothing.
    /// * **A shard part** (`ShardReceiptLicensed`) — `quorum_per_shard` seats of ONE shard. A lie
    ///   lives in one shard's work, the honest shards license honestly, so the colluders need
    ///   only the lying shard's quorum: each of its `Valid` signers locks `G / quorum_per_shard`
    ///   (never priced below the whole-object count's share: `min(quorum_per_shard, 3)`).
    ///
    /// Below the fence every door is the quorum price, which is what every lock on every other
    /// network was written at.
    pub(super) fn door_lock_prices(
        &self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        door: PalwLicenceDoorV1,
    ) -> PalwDoorLockPricesV1 {
        let quorum = crate::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1;
        let every = self.panel_valid_lock_required_for(claim, quorum);
        if !self.extras.audit_2026_09_23_active {
            return PalwDoorLockPricesV1 { every, load_bearing: None };
        }
        let colluding = crate::palw_economic_safety_v1::palw_door_colluding_signers_v1(door);
        match door {
            PalwLicenceDoorV1::Quorum | PalwLicenceDoorV1::Coverage => PalwDoorLockPricesV1 { every, load_bearing: None },
            PalwLicenceDoorV1::Optimistic => {
                let full_price = self.panel_valid_lock_required_for(claim, colluding);
                // The seat the acceptance layer licensed from, derived the way it derived it
                // (`palw_optimistic_receipts_license_v2`: the panel's anchor, the claim, the seat
                // count). A panel that names no full seat cannot have passed that check; if one
                // ever did, every `Valid` of the set carries the one-seat price rather than none.
                match self.optimistic_full_seat(claim_id) {
                    Some(full) => PalwDoorLockPricesV1 { every, load_bearing: Some((full, full_price)) },
                    None => PalwDoorLockPricesV1 { every: full_price, load_bearing: None },
                }
            }
            PalwLicenceDoorV1::ShardPart { .. } => {
                PalwDoorLockPricesV1 { every: self.panel_valid_lock_required_for(claim, colluding), load_bearing: None }
            }
        }
    }

    /// The full-replay seat S2 licenses from: `palw_optimistic_full_seat_bond_v2` over this
    /// claim's bound panel, exactly as the processor's `OptimisticLicensed` acceptance names it.
    pub(super) fn optimistic_full_seat(&self, claim_id: Hash64) -> Option<PalwBondKeyV2> {
        let panel = self.state.panels.get(&claim_id)?;
        let seats: Vec<PalwBondKeyV2> = panel.seats.iter().map(|seat| seat.bond).collect();
        let assignment = crate::palw_verification_v2::palw_segment_assignment_v2(panel.anchor, claim_id, seats.len() as u16);
        crate::palw_optimistic_licence_v2::palw_optimistic_full_seat_bond_v2(&assignment, &seats)
    }

    /// **ADR-0152 L-1: what one `Valid` counted into a set of recounted `basis_k` locks** —
    /// [`palw_rcore_lock_v1`] over the claim's residual gain `G_res = G − escrowed_reward`, with `G`
    /// from the facts the licence's lock is priced on ([`Self::panel_valid_lock_required_for`] past
    /// its fences: the claim's reservation-unit weight and, under economic safety, its realizable
    /// rights), and the buyback bound `s = 0` (every testnet-12 genesis row, ADR §2; the vesting row's
    /// `buyback_bound` is the peer's). SR-10's price for the signers a supplementary set counts;
    /// S-SPEC §2's `claim_g_v1` is the shared form S-3 lands for the licence arms.
    pub(super) fn rcore_lock_required_v1(&self, claim: &PalwClaimStateV2, basis_k: u8) -> u128 {
        let slash = self.state.classes.get(&claim.class_id).map(|c| c.slash_value_per_pwu).unwrap_or(0);
        // ADR-0160 stage 3 (F1): at the claim's full weight (`reserved × ρ`; today's at ρ = 1).
        let mut facts = crate::palw_panel_var_v1::PalwClaimFraudFactsV1::from_claim(claim, slash)
            .at_full_weight_v1(crate::palw_weight_cap_v1::PalwCapacityGainScaleV1::of(self.params).scale_of(claim));
        if let Some(safety) = self.extras.economic_safety {
            facts.extra_economic_rights_sompi = self.claim_realizable_rights_v1(claim, &safety).max(claim.rights_reserved);
        }
        let g_res = crate::palw_panel_var_v1::palw_max_fraud_gain_v1(&facts).saturating_sub(u128::from(claim.escrowed_reward));
        // ADR-0160 AS-2 (lane liab): the supplementary set's price, re-priced as the licence's is.
        crate::palw_aggregate_liability_v1::palw_seat_lock_v2(
            palw_rcore_lock_v1(g_res, claim.escrowed_reward, 0, basis_k),
            crate::palw_audit_door_v1::palw_capacity_seat_step_v1(self.params, claim),
        )
    }

    /// **The execution rights this claim's Final could realize before a conviction could take them.**
    ///
    /// The quanta it mints are `credit / execution_quantum`, and the credit is the UNCLAMPED
    /// CanonicalWork scalar — the same value `record_round_final` credits, read here the same way so
    /// the lock and the mint cannot disagree about how many rights are at stake. Zero where the lane
    /// mints no quanta at all, which is every network that leaves `palw_execution_quanta` dormant.
    pub(super) fn claim_realizable_rights_v1(&self, claim: &PalwClaimStateV2, safety: &PalwEconomicSafetyFoldV1) -> u128 {
        self.realizable_rights_of_v1(&claim.class_id, claim.pwu, claim.accepted_daa, safety)
    }

    /// [`Self::claim_realizable_rights_v1`] of a claim of `class_id` at `pwu`, accepted at
    /// `accepted_daa` — the three fields it reads, so a claim that does not exist yet (Eq's fresh
    /// attempt, [`Self::eq_cap_basis_v1`]) is priced by the same expression.
    pub(super) fn realizable_rights_of_v1(
        &self,
        class_id: &Hash64,
        pwu: u64,
        accepted_daa: u64,
        safety: &PalwEconomicSafetyFoldV1,
    ) -> u128 {
        let Some(lane) = self.extras.round_lane.filter(|lane| lane.execution_quantum > 0) else { return 0 };
        let Some(class) = self.state.classes.get(class_id) else { return 0 };
        // The same accessor `record_round_final` uses, so the lock and the mint cannot disagree.
        let canonical = self.canonical_per_draw(class_id, accepted_daa);
        // 2026-09-23 audit C-2: the same unit `record_round_final` credits in, past the same fence.
        let exposure = if self.extras.audit_2026_09_23_active {
            palw_exposure_pwu_v3(class, pwu, canonical, self.exposure_basis(accepted_daa))
        } else {
            palw_exposure_pwu_v2(class, pwu, canonical)
        };
        let counted = crate::palw_execution_quanta_v1::palw_execution_quantum_count_v1(
            u128::from(exposure),
            u128::from(lane.execution_quantum),
            // The count is what the mint will produce; its fractional tie-break is seeded by a span
            // the lock cannot see, so the ceiling (`whole + 1`) is the honest bound to price.
            crate::Hash64::default(),
            crate::Hash64::default(),
        );
        // And the same ceiling the mint stops at, so the lock never prices tickets no span issues.
        let counted = if self.extras.audit_2026_09_23_active {
            counted.min(crate::palw_execution_quanta_v1::PALW_EXEC_MAX_QUANTA_PER_SPAN_V1 as u32)
        } else {
            counted
        };
        let quanta = counted.saturating_add(1);
        // The maturity the snapshot is delayed by (`rotate_round_lane`), from the same field.
        crate::palw_economic_safety_v1::palw_realizable_before_maturity_v1(
            quanta,
            safety.maturity_daa,
            self.params.window_court,
            safety.target_time_per_block_ms,
            crate::palw_economic_safety_v1::palw_permit_value_sompi_v1(safety.permit_value_sompi),
        )
    }

    /// **The lifecycle half of the claim gate, as one predicate: the state `class_id`'s registry row
    /// holds it in, where that state admits nothing.** `None` where the registry is dormant at this
    /// block, for the base class (never gated), for a class without a row below the work target, and
    /// for a state `PalwModelLifecycleV1::admits_claims` lists — a positive list, so a state added
    /// later admits nothing until someone writes it there. The claim gate and the market gate
    /// ([`Self::check_model_market_admits`]) both ask it, so a claim and a position cannot disagree
    /// about whether the chain serves a model (the 2026-09-23 Position route matrix, P-B3: they did —
    /// a buy filled on a class whose every claim the registry refused).
    /// **G14-for-rewards at this read** (`palw_opv_bootstrap_v1::palw_reward_gate_v1`): `Unarmed` below the OPV fence.
    pub(crate) fn reward_gate_v1(&self, class_id: &Hash64, daa: u64) -> crate::palw_opv_bootstrap_v1::PalwRewardGateV1 {
        crate::palw_opv_bootstrap_v1::palw_reward_gate_v1(
            self.state,
            &self.params.base_class_id(),
            self.extras.kernel_route.as_ref(),
            class_id,
            daa,
        )
    }

    pub(super) fn class_lifecycle_refusal(&self, class_id: &Hash64) -> Option<String> {
        self.extras.model_registry.as_ref()?;
        if *class_id == self.params.base_class_id() {
            return None;
        }
        match self.state.model_lifecycles.get(class_id) {
            // ADR-0137: past the work target every model class is priced by its row; a class
            // without one (registered before the fence without a carriage) is refused, not free.
            None => self.extras.work_target_active.then(|| "no row".to_string()),
            Some(row) => (!row.state.admits_claims()).then(|| format!("{:?}", row.state)),
        }
    }

    /// **A line's market follows its class's registry lifecycle** (the 2026-09-23 Position route
    /// matrix, P-B3). A seed — and so an instalment toward one (ADR-0094) — and a buy are taken only
    /// where [`Self::class_lifecycle_refusal`] finds nothing: the question the claim gate asks first,
    /// through the same expression. Measured before the fix: a buy of 449 units filled on a class the
    /// registry held at `Prefetching`, and two live 1 MSK pledges landed on a private testnet-12's
    /// `Prefetching` 74c67e63 — a tradable pair on a model the chain was refusing every claim of, and
    /// for a class that never leaves `Candidate` a pledge locked for good in a pair nobody can trade.
    /// ADR-0090 Decision 2's pre-activation seed was written for the class-status CLOCK, which always
    /// arrives; a lifecycle is not a clock. A SELL never asks: a holder is never trapped in a model the
    /// registry has since held. Below `Params::palw_audit_2026_09_23` nothing is asked, so
    /// testnet-11 — which arms the market — folds every seed and buy exactly as it did.
    pub(super) fn check_model_market_admits(&self, class_id: &Hash64) -> Result<(), PalwStateV2Error> {
        if !self.extras.audit_2026_09_23_active {
            return Ok(());
        }
        match self.class_lifecycle_refusal(class_id) {
            Some(state) => Err(PalwStateV2Error::ModelClassNotEligible { class: *class_id, state }),
            None => Ok(()),
        }
    }

    /// **ADR-0162 Decision 5: a line's market trades only once the chain has APPROVED its class** —
    /// past `Params::palw_model_virtual_v1`, the gate every buy answers (and the reward's buyback,
    /// Decision 6): the class's status is `Active`, and where the registry is in force its lifecycle
    /// is exactly `Active`. `Probation` and `ActiveLimited` admit claims and still admit no buy: the
    /// market opens at the model's addition, its trading at its approval, and a position is never
    /// sold in a model the chain is still trying out. A class without a registry row is asked
    /// [`Self::class_lifecycle_refusal`]'s question, as the claim gate asks it. The refusals are
    /// `ModelClassNotActive` and `ModelClassNotTrading`, both the EVM's reason 3 (`NOT_ACTIVE`). A sell never asks this: no position exists before the class's
    /// first `Active`, and a holder can always leave a class that has since been held, frozen or
    /// retired. Not consulted below the fence.
    pub(super) fn check_model_trading_v2(&self, class_id: &Hash64) -> Result<(), PalwStateV2Error> {
        let class = self.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?;
        if !matches!(class.status, PalwClassStatusV2::Active) {
            return Err(PalwStateV2Error::ModelClassNotActive(*class_id));
        }
        if self.extras.model_registry.is_none() || *class_id == self.params.base_class_id() {
            return Ok(());
        }
        let refusal = match self.state.model_lifecycles.get(class_id) {
            Some(row) => {
                (!matches!(row.state, crate::palw_model_registry_v1::PalwModelLifecycleV1::Active)).then(|| format!("{:?}", row.state))
            }
            None => self.class_lifecycle_refusal(class_id),
        };
        match refusal {
            Some(state) => Err(PalwStateV2Error::ModelClassNotTrading { class: *class_id, state }),
            None => Ok(()),
        }
    }

    /// **The pair the reward's slice buys from at a claim's `Final`** (ADR-0091 Decision 2), as the
    /// fold buys it and as ADR-0152's lock prices it (`rcore_buyback_bound`) — one answer for both.
    /// Below ADR-0162's fence the line's row as written (`None` without one; a closed or unopened row
    /// quotes nothing). Past it the line's market in force, and only where a buy would be admitted
    /// now (ADR-0162 Decision 6): before the class is approved the miner is paid the whole escrow —
    /// ADR-0091 Decision 3's "a market closed to buys" — so a class rejected before `Active` has
    /// locked no miner's MSK in a pair that never trades, and trading opens exactly at
    /// `(V + seed) / supply`. The row's `closed_to_buys` is a record of its last move past the fence,
    /// not the gate: the gate is asked live (the 2026-09-25 Position review's #4).
    pub(super) fn model_buyback_market_v2(&self, line_id: &Hash64) -> Option<crate::palw_model_market_v1::PalwModelMarketV1> {
        let Some(fence_daa) = self.extras.model_virtual_v1 else {
            return self.state.model_markets.get(line_id).copied();
        };
        let line = self.state.model_line_or_founding(line_id)?;
        if !line.is_active() || self.check_model_trading_v2(&line.class_id).is_err() {
            return None;
        }
        let market = self.state.model_market_in_force_v2(line_id, fence_daa, &self.params.base_class_id())?;
        Some(crate::palw_model_market_v1::PalwModelMarketV1 { closed_to_buys: false, ..market })
    }

    /// **ADR-0152 §4-quater V2: may a claim of `class_id` and `shape` be taken at `now_daa`, as far as
    /// its verification deadline decides?** Past `palw_class_verify_deadline` (at `now_daa`; below it
    /// nothing is asked):
    ///
    /// * (a) an NM class ([`palw_class_needs_measured_row_v1`]) without a measured row active at
    ///   `now_daa` is refused `ClassDeadlineUnmeasured` on every lane — the 2M row at testnet-12's
    ///   launch (U-D1), attempt and free prompt alike;
    /// * (b) without an active row, a free-prompt claim of a class whose largest run derives a deadline
    ///   past `window_receipt` is refused `FreePromptDeadlineUnmeasured` (a class-level answer);
    /// * (c) with an active row, a free-prompt claim whose measured `D` exceeds D_cap is refused
    ///   `FreePromptDeadlineOverCap` — asked only where the claim's `work_leaves` are known
    ///   (`per_claim`, the free-prompt arm), since the class gate carries none;
    /// * (d) (the review's L3) a claim whose own `D` is long (past 120, so its `H` can outlive
    ///   `L + 120`) on a class the panel room does NOT hold to Final ([`palw_panel_holds_to_final_v1`]:
    ///   K-1 keys on the class's canonical job, which a short class's long free-prompt run does not
    ///   move) is refused `ClaimDeadlineOutlivesRoomHold` — the licence would release the room while
    ///   seats still replay. Refused here rather than making K-1 read the free-prompt profile: the
    ///   room's hold stays a function of the registry row. Asked wherever the claim's `D` is known
    ///   (an attempt, the free-prompt arm, or a class with no row).
    ///
    /// Every other testnet-12 genesis class passes and behaves as it did: the floor (no registry row,
    /// no class-derived term) and the 8k row (`D` 15, its largest free-prompt run 75).
    ///
    /// **TODO(ADR-0153, before any MoE registration — the review's L1): V2c, the paging predicate**
    /// (§4-quater.4 NM (iii), refused at registration beside `verify_class_admission_v9` as V2b), is
    /// not here. When it lands it must key on `artifact_bytes` — the bytes a seat pages — and NOT on
    /// `working_set_bytes`, which for a mixture counts only the resident experts and would call a
    /// paging MoE class resident. **And node N-8** (readiness self-selection) must compare a host's
    /// `m·T` with the claim's `receipt_window_daa` (`W_r = max(window_receipt, D)`), never with the
    /// raw `verify_daa`: for a short class `D` (15 for the 8k row) is far below the 600 DAA the chain
    /// actually gives a seat, and a node reading `D` would deselect itself from classes it can serve.
    pub(super) fn check_class_verify_admits_v1(
        &self,
        class_id: &Hash64,
        now_daa: u64,
        shape: crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1,
        per_claim: bool,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_class_verify_deadline_v1::{PALW_CLASS_VERIFY_CAP_DAA_V1, PalwClaimVerifyShapeV1, palw_derived_verify_daa_v1};
        if !self.params.class_verify_deadline_active_at(now_daa) {
            return Ok(());
        }
        let open_daa = self.params.window_receipt();
        match self.params.class_verify_row_at(class_id, now_daa) {
            Some(row) => {
                if per_claim && let PalwClaimVerifyShapeV1::FreePrompt { .. } = shape {
                    let raw = row.raw_daa(row.positions_of(shape));
                    if raw > u128::from(PALW_CLASS_VERIFY_CAP_DAA_V1) {
                        return Err(PalwStateV2Error::FreePromptDeadlineOverCap {
                            class: *class_id,
                            measured_daa: u64::try_from(raw).unwrap_or(u64::MAX),
                            cap_daa: PALW_CLASS_VERIFY_CAP_DAA_V1,
                        });
                    }
                }
            }
            None => {
                if palw_class_needs_measured_row_v1(self.params, self.state, class_id) {
                    let derived_daa = self
                        .state
                        .model_lifecycle(class_id)
                        .map(|row| {
                            palw_derived_verify_daa_v1(palw_class_verify_ccu_v1(self.state, class_id, row.work.verification_ccu))
                        })
                        .unwrap_or(0);
                    return Err(PalwStateV2Error::ClassDeadlineUnmeasured { class: *class_id, derived_daa, open_daa });
                }
                if let PalwClaimVerifyShapeV1::FreePrompt { .. } = shape
                    && let Some(derived_daa) = palw_class_fp_verify_daa_v1(self.state, class_id)
                    && derived_daa > open_daa
                {
                    return Err(PalwStateV2Error::FreePromptDeadlineUnmeasured { class: *class_id, derived_daa, open_daa });
                }
            }
        }
        // (d) The room must hold what the claim's horizon can outlive. `D` is known for an attempt, in
        // the free-prompt arm (`per_claim`), and on the derived branch (a class-level free-prompt `D`);
        // the class gate's free-prompt shape under a measured row carries no size, and its arm asks.
        let d_known = matches!(shape, PalwClaimVerifyShapeV1::Attempt)
            || per_claim
            || self.params.class_verify_row_at(class_id, now_daa).is_none();
        if d_known {
            let verify_daa = self.params.claim_verify_daa_v1(self.state, class_id, shape, now_daa);
            if verify_daa > crate::palw_class_verify_deadline_v1::PALW_CLASS_VERIFY_LONG_D_DAA_V1
                && !palw_panel_holds_to_final_v1(self.params, self.state, class_id)
            {
                return Err(PalwStateV2Error::ClaimDeadlineOutlivesRoomHold {
                    class: *class_id,
                    verify_daa,
                    hold_daa: crate::palw_class_verify_deadline_v1::PALW_CLASS_VERIFY_LONG_D_DAA_V1,
                });
            }
        }
        Ok(())
    }

    /// `incoming` is the claim asked about ([`PalwGatedClaimV1`]): an attempt — every caller below
    /// the fence — or, past it, a free-prompt commitment of so many quanta (2026-09-24 DoS audit
    /// #11). Past the fence the rate room counts it POOLED with its class's claims in flight; the
    /// static cap of a class held to Final
    /// ([`crate::palw_work_target_v1::palw_panel_held_to_final_v1`]), the rule below the fence, and
    /// the cap before the registry governs count it as its whole claims.
    pub(super) fn check_class_admits_claim(
        &self,
        class_id: &Hash64,
        now_daa: u64,
        incoming: PalwGatedClaimV1,
    ) -> Result<(), PalwStateV2Error> {
        // RFC-0008 v2 (X8R): the composed run's test seam — empty in production (`test_admitted_class_v1`).
        if self.test_admitted_class_v1(class_id) {
            return Ok(());
        }
        // **ADR-0165 A″: the base floor is the idle-only bonded fallback.** First, before any lifecycle or room question:
        // the floor state advanced to `now_daa` must be Idle. (The fold advances it at the block's start and after every REAL
        // attempt it accepts, so a floor attempt reads the state at its place in the fold's order.)
        if incoming == PalwGatedClaimV1::Attempt
            && self.params.floor_reserve_active_at(now_daa)
            && *class_id == self.params.base_class_id()
        {
            let state = self.state.floor_state_v1();
            if !state.admits_floor_at(now_daa) {
                let state = crate::palw_real_share_v1::palw_floor_step_v1(state, now_daa, None);
                return Err(PalwStateV2Error::FloorNotIdle { class: *class_id, daa: now_daa, state });
            }
        }
        // ADR-0160 X-I5: past `palw_capacity_escrow_at_licence` no attempt of a class C7 by its window
        // but absent from C7's list — its escrow term would be priced as attributable.
        crate::palw_escrow_funding_v2::palw_escrow_class_gate_v1(
            self.params,
            self.state,
            class_id,
            now_daa,
            incoming == PalwGatedClaimV1::Attempt,
        )?;
        // **G14-for-rewards** (`palw_opv_bootstrap_v1::palw_reward_gate_v1`, past the OPV fence): a post-fence class that never passed
        // the onboarding/G14 path takes no claim on any lane. Every claim this gate sees is a V2-root claim, and a V2 root is not
        // convictable by the kernel route, so an ONBOARDED class's claims get nothing beyond the old rules below — the registry
        // lifecycle, the Panel verify deadline and the Panel room are all asked (the Lead's GAP-81 decision: the new rewards are per
        // CLAIM verification route; only kernel-route claims earn them). Unarmed, exempt or onboarded: the rules below, byte for byte.
        if let crate::palw_opv_bootstrap_v1::PalwRewardGateV1::Refused { code, why } = self.reward_gate_v1(class_id, now_daa) {
            return Err(PalwStateV2Error::ClassNotRewardable { class: *class_id, code, why });
        }
        // **G-01 option (b) (`palw_g14_route_required_v1`)**: past the fence a V2 Attempt — the legacy Panel route, whatever its class
        // (floor, model, TIR v1, gen v1, shard) — is taken only while that route's G14 public-conviction route (LG14-A + LG14-B) is
        // armed; otherwise it earns no reward, weight or consensus credit (a refused own attempt is skipped, the block stands, and the
        // producer's pre-check answers this before an inference). Read from the fence state: the route's arming restores the reward.
        // `None` below the fence (every shipped network). The free-prompt lane asks its own question with the job in hand
        // (`palw_g14_route_fp_refusal_v1`, the commitment arm).
        if incoming == PalwGatedClaimV1::Attempt
            && let Some(why) = crate::palw_g14_gates_v1::palw_g14_route_attempt_refusal_v1(self.params, now_daa)
        {
            return Err(PalwStateV2Error::ClassNotRewardable {
                class: *class_id,
                code: crate::palw_g14_gates_v1::PALW_G14_ROUTE_ABSENT_CODE_V1,
                why,
            });
        }
        // **RFC-0006 R6-CELL round 4 (`palw_cell_coverage_closure_v1`, REQ-CELL-1b)**: past the fence, a class whose graph the
        // fold holds and whose cell coverage has a gap takes no claim on any lane — explicitly uncovered and unrewarded.
        // Where the fold does not hold the graph, the court close convicts instead (REQ-CELL-2). Below the fence `None`.
        // RFC-0006 consumer v2 (`palw_memory_routing_court_v2`): past its fence the same gate reads what that consumer reaches, with
        // the class's router binding (aux 139) in hand; below it, the round-4 answer byte for byte.
        let router_binding = super::palw_router_binding_row_of_v2(self.state, class_id);
        if let Some(refusal) = crate::palw_cell_coverage_closure_v1::palw_cell_closure_claim_refusal_v2(
            self.params,
            now_daa,
            self.state.fp_work_profile_of(class_id),
            router_binding.as_ref(),
        ) {
            return Err(PalwStateV2Error::ClassNotRewardable {
                class: *class_id,
                code: crate::palw_cell_coverage_closure_v1::PALW_CELL_UNCOVERABLE_CODE_V1,
                why: refusal.why().to_string(),
            });
        }
        // **RFC-0017 (`palw_model_eligibility_v1`, REQ-ELIG-1)**: past the fence, a class whose eligibility stage carries no
        // reward-bearing block right (`Registered`, `Lapsed`) takes no attempt and no free-prompt commitment — both lanes carry a
        // subsidy-carved block. Its user-paid kernel / funded jobs never meet this gate. Below the fence, and for a genesis class,
        // `None`: the rules below, byte for byte. GAP-E4 (2026-10-15): the RFC-0004 evaluation lane is gated alike — it earns no
        // coinbase, block or Final weight, but its executor and credited seats are paid and its score decides a line's promotion, so an
        // unexamined class is not one an evaluation may run on.
        // U-9 (user decision 2026-10-15): the gated lanes are RFC-0017's one list; user-paid kernel / funded jobs stay open.
        let lane = match incoming {
            PalwGatedClaimV1::Attempt => crate::palw_model_eligibility_v1::PalwEligibilityLaneV1::Attempt,
            PalwGatedClaimV1::FreePrompt { .. } => crate::palw_model_eligibility_v1::PalwEligibilityLaneV1::FreePrompt,
            PalwGatedClaimV1::Evaluation => crate::palw_model_eligibility_v1::PalwEligibilityLaneV1::Evaluation,
        };
        if crate::palw_model_eligibility_v1::palw_eligibility_lane_gated_v1(lane)
            && let Some((_, why)) =
                super::palw_model_eligibility_fold_v1::eligibility_claim_refusal_v1(self.state, self.params, class_id, now_daa)
        {
            return Err(PalwStateV2Error::ClassNotRewardable {
                class: *class_id,
                code: super::palw_model_eligibility_fold_v1::PALW_ELIGIBILITY_NOT_ELIGIBLE_CODE_V1,
                why,
            });
        }
        let Some(fold) = self.extras.model_registry.as_ref() else { return Ok(()) };
        let whole = incoming.whole_claims(self.params.fp_quanta_per_canonical_job as u64);
        if let Some(state) = self.class_lifecycle_refusal(class_id) {
            return Err(PalwStateV2Error::ClassNotAdmitting { class: *class_id, state });
        }
        // §4-quater V2: after the lifecycle, before the room — a class whose deadline the chain
        // cannot state takes no claim on any lane, whatever room it has. The gate carries no
        // free-prompt claim's size, so the per-claim measured cap (V2(c)) is the free-prompt arm's.
        let shape = match incoming {
            PalwGatedClaimV1::Attempt => crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::Attempt,
            PalwGatedClaimV1::FreePrompt { .. } | PalwGatedClaimV1::Evaluation => {
                crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::FreePrompt { work_leaves: 0 }
            }
        };
        self.check_class_verify_admits_v1(class_id, now_daa, shape, false)?;
        // **RFC-0016 V+ (`palw_model_block_rights_v1`, dormant)**: a model-class Attempt needs one of its model's pre-issued block
        // rights. Asked here, so the producer's pre-check (`palw_class_admits_claim_v1`) answers it before the forward. `Ok` below
        // the fence and for the floor class.
        if incoming == PalwGatedClaimV1::Attempt {
            self.rfc0016_rights_admit_v1(class_id, now_daa)?;
        }
        // What the lifecycle let through without a row to read — the base class, and a class
        // without a row below the work target — has no room to check either.
        if *class_id == self.params.base_class_id() {
            return Ok(());
        }
        let Some(row) = self.state.model_lifecycles.get(class_id) else { return Ok(()) };
        // **RFC-0007 Part IV.2: a `Capped` class has no panel room to read** (it has fewer than a panel's holders by definition): its gate
        // is the capped weight cap and the capped table's bound, and the lifecycle's own refusal above.
        if palw_mesh_fold_v1::palw_lifecycle_row_is_capped_v1(row) {
            return palw_mesh_fold_v1::capped_class_admits_v1(self.state);
        }
        // **H-2 of the 2026-09-18 audit: the room check waits for the grace the registry already
        // computes.** The work target and the registry arm at the same height, and a readiness proof
        // is refused below that height — so on the flag day itself every class has zero ready seats,
        // the room is zero, and every non-base class would refuse claims until seven seats prove
        // possession, while their producers mined blocks their own chain rejected.
        // `grace_until_daa` (activation + `readiness_probe_max_age_spans` spans) exists for exactly
        // this window; inside it the class is bounded by the inflight cap below, as before the fence.
        if self.extras.work_target_active && fold.governs_at(now_daa) {
            let audited = self.extras.audit_2026_09_23_active;
            // **The block's own attempt, judged where step 3 began** (2026-09-24 audit #4 review,
            // item 2). Step 3 folds objects that change the demand without asking this gate — an
            // accusation re-opening a licensed claim, a court on one, a retirement that takes a
            // ready seat — so a room read at step 4 could refuse the attempt the producer's
            // pre-check admitted and disqualify the whole block. Past the fence the attempt's room
            // is read once, before step 3's first object (`TransitionBuilder::own_attempt_fit`), and
            // every gated object in step 3 is refused rather than taking it (below). Step 4 then
            // asks only the lifecycle for it. The re-charges take effect for the next block.
            let exempt = audited && incoming == PalwGatedClaimV1::Attempt && self.room_exempt_class == Some(*class_id);
            // **RFC-0016 elastic Final capacity (`palw_elastic_final_capacity_v1`, dormant)**: past it the per-verifier water-filled
            // room (each verifier counted once, no cross-model subtraction; capacity 0 admits nothing; C7 concurrency derived from
            // capacity) decides instead of the rate room and the static C7 cap below. `None` below the fence.
            if !exempt && let Some(verdict) = self.rfc0016_elastic_room_v1(class_id, row, fold, now_daa, whole, incoming) {
                return verdict;
            }
            // **A class held to Final is held to its static cap past the fence too** (the review's
            // item 1, ADR-0152 T-2(b)): c_2M = 1 until ADR-0153. The rate room alone lets such a
            // class run as many claims as the panel's replay holds, which is exactly what the hold
            // forbids. Held is ADR-0152's C7 (`palw_panel_held_to_final_v1`: a window of at least
            // 1,000 spans, testnet-12's 2M row), not ADR-0119's held regime: the short-window row
            // records a held ladder too, and T-2(a) releases it at licence and judges it by the room
            // alone. The claim asked about counts whole here, not pooled as the room counts it: it
            // draws a panel of its own, and c_2M = 1 is one claim, so no part-job slack admits a
            // second.
            // C7 is read through `palw_rcore_class_is_c7_v1` — the window rule united with the
            // conservative list, one predicate with the escrow's release (the S review's L2; the
            // list is empty below `palw_rcore_plus`, so this is the window rule there) — and past
            // `palw_class_verify_deadline` a long-D class is held beside it (§4-quater K-1,
            // `palw_panel_holds_to_final_v1`; no genesis class is long-D but the closed 2M row).
            if audited && !exempt && palw_panel_holds_to_final_v1(self.params, self.state, class_id) {
                let inflight = self.model_registry_inflight(class_id);
                if (inflight as u64).saturating_add(whole) > row.profile.max_inflight_claims as u64 {
                    return Err(PalwStateV2Error::ClassInflightCapped {
                        class: *class_id,
                        inflight,
                        cap: row.profile.max_inflight_claims,
                    });
                }
            }
            // ADR-0137 D5: one network-wide replay budget in place of the per-class cap. **Past the
            // fence the claim is pooled before it is counted** (the audit #4 review's item 4, and its
            // verification): what the class would owe with it ([`palw_panel_owed_v1`] with `extra`)
            // against what the class holds — the budget is replay, and a free-prompt commitment that
            // fills its class's last part-job adds no job of it. An attempt adds one, as before.
            if !exempt {
                let pooled_fits = audited && self.panel_rate_v1(class_id, row, fold, now_daa, Some((*class_id, incoming))).fits();
                if !pooled_fits {
                    let (room, inflight_replay, budget, horizon_spans) = self.panel_room_v1(class_id, row, fold, now_daa);
                    if audited || room < whole {
                        return Err(PalwStateV2Error::PanelRoomExhausted { class: *class_id, inflight_replay, budget, horizon_spans });
                    }
                }
            }
            // **And a gated object of ANOTHER class may not take the own attempt's room** (the
            // review's item 2): the budget a span is each class's own ready seats', so an object that
            // fits its class can still leave the attempt's class short. Refused only where the
            // attempt fitted before this object and would not after it.
            if audited
                && let Some(own) = self.own_attempt_class.filter(|own| own != class_id && *own != self.params.base_class_id())
                && let Some(own_row) = self.state.model_lifecycles.get(&own)
            {
                let before = self.panel_rate_v1(&own, own_row, fold, now_daa, None);
                let after = self.panel_rate_v1(&own, own_row, fold, now_daa, Some((*class_id, incoming)));
                if before.fits() && !after.fits() {
                    return Err(PalwStateV2Error::PanelRoomExhausted {
                        class: own,
                        inflight_replay: after.inflight_replay(),
                        budget: after.per_span.saturating_mul(after.window as u128),
                        horizon_spans: after.window,
                    });
                }
            }
            return Ok(());
        }
        let inflight = self.model_registry_inflight(class_id);
        if (inflight as u64).saturating_add(whole) > row.profile.max_inflight_claims as u64 {
            return Err(PalwStateV2Error::ClassInflightCapped { class: *class_id, inflight, cap: row.profile.max_inflight_claims });
        }
        Ok(())
    }

    /// **RFC-0004 A6 (spec 17 §17.8.4, decided 2026-09-30): the evaluation claims' share of a class's replay
    /// room.** Evaluation claims are counted in the room as whole claims ([`PalwGatedClaimV1::Evaluation`]) and
    /// are bounded to `permille` of the class's capacity (the fence's `max_eval_budget_permille`), so evaluation
    /// cannot crowd out the class's attempts and free-prompt claims — at least one is always admitted. Read where
    /// the rate room is read (the registry governing, the work target in force), for a class with a registry row
    /// whose in-flight evaluation claims (replay still owed, or all of them for a class held to Final) plus this
    /// one are past the share; `None` otherwise.
    pub(super) fn eval_room_share_refusal_v1(&self, class_id: &Hash64, now_daa: u64, permille: u16) -> Option<&'static str> {
        let fold = self.extras.model_registry.as_ref()?;
        if !(self.extras.work_target_active && fold.governs_at(now_daa)) || *class_id == self.params.base_class_id() {
            return None;
        }
        let row = self.state.model_lifecycles.get(class_id)?;
        let rate = self.panel_rate_v1(class_id, row, fold, now_daa, Some((*class_id, PalwGatedClaimV1::Evaluation)));
        let held = palw_panel_holds_to_final_v1(self.params, self.state, class_id);
        let pending = self.with_inflight_index(|index| {
            index
                .get(class_id)
                .map(|tally| if held { tally.eval_claims } else { tally.eval_claims.saturating_sub(tally.licensed_eval_claims) })
                .unwrap_or(0)
        });
        palw_eval_room_share_exceeded_v1(pending, rate.capacity, permille)
            .then_some("an evaluation claim past its share of its class's replay room (max_eval_budget_permille)")
    }

    pub(super) fn panel_room_v1(
        &self,
        class_id: &Hash64,
        row: &crate::palw_model_registry_v1::PalwModelLifecycleRowV1,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
        now_daa: u64,
    ) -> (u64, u128, u128, u64) {
        // **2026-09-24 audit #4: past `palw_audit_2026_09_23` the room is a rate** — each class's
        // owed replay over its own window ([`Self::panel_room_by_rate_v1`]). Below it (testnet-11
        // has run the registry since 6,001) the common-horizon rule stands verbatim.
        if self.extras.audit_2026_09_23_active {
            return self.panel_room_by_rate_v1(class_id, row, fold, now_daa);
        }
        let base = self.params.base_class_id();
        let g = &fold.globals;
        let seat_count = g.seat_count as u128;
        let horizon = match self.panel_horizon.and_then(|cache| cache.get()) {
            Some(horizon) => horizon,
            None => {
                let horizon = self
                    .state
                    .model_lifecycles
                    .iter()
                    .filter(|(id, r)| **id != base && r.state.admission_permille() > 0)
                    .map(|(_, r)| r.profile.verification_window_spans as u64)
                    .min()
                    .unwrap_or(1)
                    .max(1);
                if let Some(cache) = self.panel_horizon {
                    cache.set(Some(horizon));
                }
                horizon
            }
        };
        // **Audit #13b: summed over the classes WITH claims in flight, not over every row.** A row
        // with none contributed `0 × ccu × seats` to the walk's sum, and a saturating sum of
        // non-negative terms does not depend on its order or on its zeros — so iterating the
        // index and looking each class's row up is the walk's number. A class in flight with no
        // row contributed nothing to the walk (it iterated rows) and contributes nothing here.
        let audit = self.extras.audit_2026_09_23_active;
        let per_job = self.params.fp_quanta_per_canonical_job;
        let inflight_replay: u128 = self.with_inflight_index(|index| {
            index
                .iter()
                .filter(|(id, _)| **id != base)
                .filter_map(|(id, tally)| self.state.model_lifecycles.get(id).map(|r| (tally.counted(audit, per_job), r)))
                .map(|(inflight, r)| (inflight as u128).saturating_mul(r.work.economic_ccu_per_claim).saturating_mul(seat_count))
                .fold(0u128, u128::saturating_add)
        });
        // Step 3's reservation for this block's own attempt (`own_attempt_class`): one claim of its
        // class on the budget, as if already in flight.
        let inflight_replay = match self.own_attempt_class.filter(|id| *id != base) {
            Some(id) => match self.state.model_lifecycles.get(&id) {
                Some(r) => inflight_replay.saturating_add(r.work.economic_ccu_per_claim.saturating_mul(seat_count)),
                None => inflight_replay,
            },
            None => inflight_replay,
        };
        // ADR-0152 SW-9: past `palw_rcore_plus`, a class outside C7 counts effective ready operators.
        let ready = self.model_registry_room_ready_v1(class_id, row, now_daa, fold) as u128;
        let per_span =
            ready.saturating_mul(g.reference_work_per_span).saturating_mul(g.utilization_permille.min(1_000) as u128) / 1_000;
        let budget = per_span.saturating_mul(horizon as u128);
        let cost = row.work.economic_ccu_per_claim.saturating_mul(seat_count);
        (crate::palw_work_target_v1::palw_panel_room_v1(per_span, horizon, inflight_replay, cost), inflight_replay, budget, horizon)
    }

    /// **The room by rate** (2026-09-24 audit #4): this class's ready seats' replay a span, less
    /// the per-span terms of every OTHER class's owed replay — each over its own window — is the
    /// class's capacity in its claims ([`PalwPanelRateV1`]); the room is that less what the class
    /// owes. A class held to Final ([`crate::palw_work_target_v1::palw_panel_held_to_final_v1`],
    /// ADR-0152's C7) is also held to its static cap (T-2(b)).
    ///
    /// Returns what [`Self::panel_room_v1`] returns, read over this class's window: the whole
    /// demand as replay over `window` spans, the budget as `per_span × window`, and `window` as the
    /// horizon — so `inflight / budget` is the demand a span over the budget a span, the
    /// utilization the row reports, and no other class's window enters any of the four.
    pub(super) fn panel_room_by_rate_v1(
        &self,
        class_id: &Hash64,
        row: &crate::palw_model_registry_v1::PalwModelLifecycleRowV1,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
        now_daa: u64,
    ) -> (u64, u128, u128, u64) {
        let rate = self.panel_rate_v1(class_id, row, fold, now_daa, None);
        let mut room = rate.room();
        // C7 through the one predicate (the S review's L2), with §4-quater K-1's long-D classes.
        if palw_panel_holds_to_final_v1(self.params, self.state, class_id) {
            room = room.min((row.profile.max_inflight_claims as u128).saturating_sub(rate.owed).min(u64::MAX as u128) as u64);
        }
        (room, rate.inflight_replay(), rate.per_span.saturating_mul(rate.window as u128), rate.window)
    }

    /// One class's [`PalwPanelRateV1`] reading, with an `extra` claim of some class pooled into what
    /// that class owes.
    pub(super) fn panel_rate_v1(
        &self,
        class_id: &Hash64,
        row: &crate::palw_model_registry_v1::PalwModelLifecycleRowV1,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
        now_daa: u64,
        extra: Option<(Hash64, PalwGatedClaimV1)>,
    ) -> PalwPanelRateV1 {
        let g = &fold.globals;
        // **ADR-0160 F-R: past `palw_capacity_verify_room` the panel's replay is priced as
        // Verification V2 costs it** — `k = 2` whole replays a claim at the measured speed
        // ([`crate::palw_verify_capacity_v1::palw_replay_pricing_v1`]) — for this class's capacity
        // and every other class's term alike. Below it: `seat_count` full replays at the reference,
        // the expression below byte for byte.
        let room_v2 = self.params.capacity_room_active_at(now_daa);
        let shipped_replicas = g.seat_count as u128;
        let replicas =
            if room_v2 { crate::palw_verify_capacity_v1::palw_replay_pricing_v1(true, g).replicas } else { shipped_replicas };
        let (owed, terms, shipped_terms) = self.with_inflight_index(|index| {
            let owed = palw_panel_owed_v1(self.state, self.params, index, self.own_attempt_class, extra);
            let terms = palw_panel_terms_v1(self.state, replicas, &owed);
            let shipped_terms = room_v2.then(|| palw_panel_terms_v1(self.state, shipped_replicas, &owed));
            (owed, terms, shipped_terms)
        });
        // ADR-0152 SW-9 (T-2(a) amended): past `palw_rcore_plus`, a class outside C7 counts
        // effective ready operators over capped weights — the capacity the stake draw leaves it.
        let ready = self.model_registry_room_ready_v1(class_id, row, now_daa, fold) as u128;
        let shipped_per_span = palw_panel_shipped_per_span_v1(ready, g);
        let per_span = if room_v2 {
            ready.saturating_mul(crate::palw_verify_capacity_v1::palw_replay_pricing_v1(true, g).per_seat_per_span)
        } else {
            shipped_per_span
        };
        let window = row.profile.verification_window_spans as u64;
        let mut rate =
            PalwPanelRateV1::read(class_id, per_span, window, row.work.economic_ccu_per_claim.saturating_mul(replicas), &owed, &terms);
        // **Stage 1 (rcore/cap-s1): F-R's room is never above ρ × the shipped room.** The measured
        // pricing (`k = 2` at the measured speed) holds ≈ 6× the claims of the shipped one at the same
        // ready seats (the stage-1 state diff: 8k at 1,000,000 MSK, 3 → 29 claims at ρ = 1), and the
        // user's plan admits more claims only through ρ. So past the fence the class's capacity is the
        // lesser of the measured reading and the shipped reading of a panel whose replay budget is ρ
        // times today's (`per_span × ρ`, every class's load priced as today) — at ρ = 1 today's
        // capacity exactly. One clamp for the gate, the pooled check, the own attempt's room and the
        // producer's pre-check alike (they all read this function).
        if let Some(shipped_terms) = shipped_terms {
            let rho = u128::from(crate::palw_weight_cap_v1::palw_capacity_rho_at_v1(self.params, now_daa));
            let shipped = PalwPanelRateV1::read(
                class_id,
                shipped_per_span.saturating_mul(rho),
                window,
                row.work.economic_ccu_per_claim.saturating_mul(shipped_replicas),
                &owed,
                &shipped_terms,
            );
            rate.capacity = rate.capacity.min(shipped.capacity);
        }
        rate
    }

    pub(super) fn model_registry_ready_seats(
        &self,
        class_id: &Hash64,
        now_daa: u64,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
    ) -> u32 {
        let ready = self
            .state
            .bonds
            .iter()
            .filter(|(bond_key, bond)| self.model_registry_seat_is_ready(bond_key, bond, class_id, now_daa, fold))
            .count();
        ready.min(u32::MAX as usize) as u32
    }

    /// **ADR-0152 SW-9: the ready count the panel room reads** — [`Self::model_registry_ready_seats`]'s
    /// bonds, handed to [`palw_panel_room_ready_count_v1`], which counts them as bonds (below
    /// `palw_rcore_plus`, and for a C7 class past it) or as `ready_eff` over their operators' capped
    /// weights (past it, outside C7). The readiness predicate is the fold's own, so the room and the
    /// `NoCapablePanel` test ([`Self::model_registry_ready_seats`], which stays a bond count: it asks
    /// whether a panel can be FILLED, and `ready_eff >= seat_count` iff the operators can) read one
    /// set of ready seats.
    pub(super) fn model_registry_room_ready_v1(
        &self,
        class_id: &Hash64,
        row: &crate::palw_model_registry_v1::PalwModelLifecycleRowV1,
        now_daa: u64,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
    ) -> u32 {
        let ready = self
            .state
            .bonds
            .iter()
            .filter(|(bond_key, bond)| self.model_registry_seat_is_ready(bond_key, bond, class_id, now_daa, fold))
            .map(|(_, bond)| bond);
        palw_panel_room_ready_count_v1(self.params, row, now_daa, fold.globals.seat_count as u64, ready)
    }

    pub(super) fn model_registry_seat_is_ready(
        &self,
        bond_key: &PalwBondKeyV2,
        // The bond the caller iterated; the predicate reads it by key from the same state.
        _bond: &PalwBondStateV2,
        class_id: &Hash64,
        now_daa: u64,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
    ) -> bool {
        // ADR-0133 §11.2: past readiness V2 a row stands for the globals' V2 horizon (eight spans;
        // twenty-four on testnet-12, user decision 2026-09-25), not thirty, and a V1 row does not
        // stand at all — the challenge rotates every span and the rotation has to bite.
        // **The one predicate** (the 2026-09-24 readiness-age sweep): the same five clauses the
        // registry read serves as `readySeatsNow` — `palw_seat_not_ready_reason_under_v1`, under
        // THIS block's `readiness_v2_active` — so the count and the RPC cannot disagree. Clause for
        // clause what this function tested before: active, above the floor, a row, fresh (a V1 row
        // never past V2), free collateral for the readiness multiple.
        // Past the fence `collateral` is read as the net figure it is (`palw_bond_free_collateral_v1`,
        // review of the 2026-09-24 DoS audit #12) — in the fold by THIS block's fence, as before.
        #[cfg(test)]
        tests::PALW_READY_PREDICATE_EVALS_FOR_TESTS.with(|count| count.set(count.get() + 1));
        // **Lane MU (ADR-0173)**: past `palw_audit_1004_v1` a seat holds the class if it holds a root in force (below it, the one row).
        let Some(row) = self.state.seat_readiness_class_v1(self.params, bond_key, class_id, now_daa) else { return false };
        // **RFC-0004 §6.3/§6.7 (spec 17 §17.7.1): a composite class adds the parent clause** — the seat must be
        // ready for the composite's parent too. The class-blind predicate for every class without a record.
        crate::palw_model_registry_v1::palw_seat_class_not_ready_reason_net_v1(
            self.state,
            self.params,
            bond_key,
            class_id,
            row,
            now_daa,
            fold,
            self.extras.readiness_v2_active,
            self.extras.audit_2026_09_23_active,
        )
        .is_none()
    }

    /// **ADR-0152 v3.1 T-2(a)'s per-bond share of a class** — `⌈c_class / 2⌉` claims, where
    /// `c_class` is the class's `max_inflight_claims` for a C7 class ([`palw_rcore_class_is_c7_v1`],
    /// the one union of the window rule and the conservative list: c_2M = 1 until ADR-0153, so one)
    /// and otherwise `palw_panel_capacity_by_rate_v1(per_span, 0, window, cost)`: the claims of this
    /// class the panel's replay a span holds over the class's own window with no other class beside
    /// it. `per_span` and `window` are read from the very [`PalwPanelRateV1`] the room reads
    /// (`panel_rate_v1`), so M4's `ready_eff` (SW-9), which changes that reading, changes this one
    /// with it; `cost` is the same expression `panel_rate_v1` prices a claim's replay with.
    ///
    /// `None` wherever the share is not a rule: below `Params::palw_rcore_plus`, without the
    /// registry, on the base class, on a class without a row, and where the room does not govern
    /// (`work_target_active && fold.governs_at(now)`, the gate's own condition — inside the
    /// registry's grace the static cap bounds the class and no share is read).
    ///
    /// **TODO(ADR-0152 §4-quater K-1 — must fix before any long-D class is accepted):** `c_class`
    /// keys on C7 ([`palw_rcore_class_is_c7_v1`]) and not on K-1's hold,
    /// [`palw_panel_holds_to_final_v1`] (C7 ∪ long-D past `palw_class_verify_deadline`), so a long-D
    /// class outside C7 would get the rate-derived share while its room holds it to Final by its
    /// static cap. No testnet-12 class is long-D and open today (the 2M row is C7 and closed until
    /// its measured row, U-D1), so nothing reads the difference yet; no behaviour change here.
    pub(super) fn bond_class_share_v1(&self, class_id: &Hash64, now_daa: u64) -> Option<u64> {
        if !self.params.rcore_plus_active_at(now_daa) {
            return None;
        }
        let fold = self.extras.model_registry.as_ref()?;
        if *class_id == self.params.base_class_id() || !(self.extras.work_target_active && fold.governs_at(now_daa)) {
            return None;
        }
        Some(self.class_capacity_v1(class_id, now_daa)?.div_ceil(2))
    }

    /// **`c_class`, the room T-2(a) and ADR-0160's share divide** — the class's static cap for a C7
    /// class, otherwise the claims its ready seats' replay holds over its window with no other class
    /// beside it (`palw_panel_capacity_by_rate_v1(per_span, 0, window, cost)`, read from the very
    /// [`PalwPanelRateV1`] the room reads). Past `Params::palw_capacity_verify_room` the claim's cost
    /// is `k = 2` replays (`panel_rate_v1` prices `per_span` at the measured speed there), so this is
    /// [`crate::palw_verify_capacity_v1::palw_panel_capacity_v2`]. `None` where the share is no rule
    /// ([`Self::bond_class_share_v1`]'s conditions).
    pub(super) fn class_capacity_v1(&self, class_id: &Hash64, now_daa: u64) -> Option<u64> {
        if !self.params.rcore_plus_active_at(now_daa) {
            return None;
        }
        let fold = self.extras.model_registry.as_ref()?;
        if *class_id == self.params.base_class_id() || !(self.extras.work_target_active && fold.governs_at(now_daa)) {
            return None;
        }
        let row = self.state.model_lifecycles.get(class_id)?;
        // **RFC-0006 §6.2, decision 7: an IR class with a layer-shard plan has the BINDING SHARD's room.** A class's verification
        // supply is the ready seats of each shard, not of the whole model: `min over shards of ⌊ ready_eff(c, i) × per_seat ×
        // window / (2 × eccu × w_i / w) ⌋`, a shard's ready seats being the bonds whose possession proof of that shard's rows is
        // fresh. The measured-speed constant is unchanged (decision 9).
        if self.params.tir_shard_active_at(now_daa)
            && !palw_rcore_class_is_c7_v1(self.params, self.state, class_id)
            && let Some(plan) = self.state.tir_shard_plans.get(class_id)
        {
            let pricing =
                crate::palw_verify_capacity_v1::palw_replay_pricing_v1(self.params.capacity_room_active_at(now_daa), &fold.globals);
            let ready: Vec<u128> = (0..plan.s_l)
                .map(|shard| {
                    let ready_class = crate::palw_tir_shard_v1::palw_tir_shard_ready_class_v1(class_id, plan.s_l, shard);
                    u128::from(self.model_registry_ready_seats(&ready_class, now_daa, fold))
                })
                .collect();
            let permille: Vec<u32> = if self.params.tir_shard_segment_active_at(now_daa) {
                // Future admission room must cover the same resident-priced masks as
                // bind/pay/locks. Existing claims retain their acceptance-time prices.
                let Some(tir) = self.state.tir_classes.get(class_id) else { return Some(0) };
                let Ok(program) = misaka_palw_tir::TirProgramV1::decode_canonical(&tir.program) else { return Some(0) };
                let Ok(resident) = crate::palw_tir_shard_segment_v2::palw_tir_shard_cell_resident_permille_v2(
                    &program,
                    tir.facts.max_context,
                    plan.s_l,
                    plan.s_p,
                ) else {
                    return Some(0);
                };
                let Ok(shares) = crate::palw_tir_shard_segment_v2::palw_tir_shard_room_permille_v2(
                    &plan.cell_permille,
                    &resident,
                    plan.s_l,
                    plan.s_p,
                ) else {
                    return Some(0);
                };
                shares
            } else {
                (0..usize::from(plan.s_l))
                    .map(|shard| {
                        plan.cell_permille
                            .iter()
                            .skip(shard * usize::from(plan.s_p))
                            .take(usize::from(plan.s_p))
                            .map(|p| u32::from(*p))
                            .sum()
                    })
                    .collect()
            };
            let window = (row.profile.verification_window_spans as u64).max(1);
            return Some(crate::palw_tir_shard_v1::palw_tir_shard_room_v1(
                &ready,
                &permille,
                pricing.per_seat_per_span,
                window,
                row.work.economic_ccu_per_claim,
            ));
        }
        Some(if palw_rcore_class_is_c7_v1(self.params, self.state, class_id) {
            row.profile.max_inflight_claims as u64
        } else {
            let rate = self.panel_rate_v1(class_id, row, fold, now_daa, None);
            let replicas = if self.params.capacity_room_active_at(now_daa) {
                crate::palw_verify_capacity_v1::palw_replay_pricing_v1(true, &fold.globals).replicas
            } else {
                fold.globals.seat_count as u128
            };
            let cost = row.work.economic_ccu_per_claim.saturating_mul(replicas);
            crate::palw_work_target_v1::palw_panel_capacity_by_rate_v1(rate.per_span, 0, rate.window, cost)
        })
    }

    /// **Stage 1 (rcore/cap-s1): `c_class` as the SHIPPED pricing reads it** — [`Self::class_capacity_v1`]
    /// below `Params::palw_capacity_verify_room`, byte for byte (`seat_count` full replays at the
    /// reference work a span), at any height: the unit of the per-bond share's ρ clamp.
    pub(super) fn class_capacity_shipped_v1(&self, class_id: &Hash64, now_daa: u64) -> Option<u64> {
        if !self.params.rcore_plus_active_at(now_daa) {
            return None;
        }
        let fold = self.extras.model_registry.as_ref()?;
        if *class_id == self.params.base_class_id() || !(self.extras.work_target_active && fold.governs_at(now_daa)) {
            return None;
        }
        let row = self.state.model_lifecycles.get(class_id)?;
        Some(if palw_rcore_class_is_c7_v1(self.params, self.state, class_id) {
            row.profile.max_inflight_claims as u64
        } else {
            let ready = self.model_registry_room_ready_v1(class_id, row, now_daa, fold) as u128;
            let per_span = palw_panel_shipped_per_span_v1(ready, &fold.globals);
            let cost = row.work.economic_ccu_per_claim.saturating_mul(fold.globals.seat_count as u128);
            crate::palw_work_target_v1::palw_panel_capacity_by_rate_v1(
                per_span,
                0,
                (row.profile.verification_window_spans as u64).max(1),
                cost,
            )
        })
    }

    /// **The share `bond` may hold of `class_id`'s unlicensed claims**: T-2(a)'s `⌈c/2⌉` below
    /// `Params::palw_capacity_verify_room`, and past it ADR-0160's cap over the same `c_class`
    /// ([`crate::palw_verify_capacity_v1::palw_bond_room_cap_v1`]): the bond's stake-proportional share
    /// of the room among the active holders (every bond holding an unlicensed claim of the class, and
    /// this one) — the whole room when it holds alone — guaranteed against the others, plus whatever
    /// rounding slack no holder has taken; none for a frozen bond. C7 keeps its static cap as the room
    /// (V-I6: c_2M = 1).
    ///
    /// **Stage 1 (rcore/cap-s1): the room the shares divide is never above ρ × T-2(a)'s shipped share**
    /// (`ρ · ⌈c_ship / 2⌉`, [`Self::class_capacity_shipped_v1`]). The stake share lets a bond holding
    /// alone take the whole room, which the measured pricing makes ≈ 6× today's `c_class` — at ρ = 1 a
    /// lone 100,000 MSK bond went from T-2(a)'s 3 claims of 8k to 15. Dividing `ρ · ⌈c_ship / 2⌉` instead
    /// keeps a lone bond at today's share at ρ = 1, and keeps the rule split-neutral (the shares of k
    /// holders divide the same room one holder of their total stake would take whole) — where T-2(a)
    /// itself was not: under it two bonds held up to the class's whole `c` and one bond of their total
    /// stake `⌈c / 2⌉`. At ρ = 1 the class's holders together therefore hold at most today's per-bond
    /// share, which is less than today's `c_class`: stage 1's "equal or less".
    pub(super) fn bond_class_share_for_v1(&self, class_id: &Hash64, bond: &PalwBondKeyV2, now_daa: u64) -> Option<u64> {
        if !self.params.capacity_room_active_at(now_daa) {
            return self.bond_class_share_v1(class_id, now_daa);
        }
        let room = self.class_share_room_v1(class_id, now_daa)?;
        let held: BTreeMap<PalwBondKeyV2, u64> = self.with_inflight_index(|index| {
            index
                .get(class_id)
                .map(|tally| tally.unlicensed_by_bond.iter().map(|(holder, n)| (*holder, *n as u64)).collect())
                .unwrap_or_default()
        });
        // A model class's room keeps no racing headroom (the floor's, `check_floor_room_v1`).
        Some(self.room_cap_v1(room, 0, bond, &held))
    }

    /// [`crate::palw_verify_capacity_v1::palw_bond_room_cap_v1`] for `bond` over the holders `held`
    /// (bond → claims held against `room`), `bond` counted with what it holds (none if absent), the
    /// shares dividing `room − headroom`.
    pub(super) fn room_cap_v1(&self, room: u64, headroom: u64, bond: &PalwBondKeyV2, held: &BTreeMap<PalwBondKeyV2, u64>) -> u64 {
        let holder = |key: &PalwBondKeyV2, n: u64| crate::palw_verify_capacity_v1::PalwRoomHolderV1 {
            collateral: self.state.bonds.get(key).map(|record| record.collateral).unwrap_or(0),
            held: n,
            frozen: crate::palw_verify_capacity_v1::palw_verify_bond_is_frozen_v1(self.state, key),
        };
        let mut holders = vec![holder(bond, held.get(bond).copied().unwrap_or(0))];
        holders.extend(held.iter().filter(|(key, _)| *key != bond).map(|(key, n)| holder(key, *n)));
        crate::palw_verify_capacity_v1::palw_bond_room_cap_v1(room, headroom, 0, &holders)
    }

    /// **ADR-0160 F-R (J-6): may one more floor claim — `claim`, about to be accepted on `bond` —
    /// enter?** `Ok` below `Params::palw_capacity_verify_room` and for every other class. Past it:
    ///
    /// 1. the floor room ([`crate::palw_verify_capacity_v1::palw_floor_room_v1`]): the eligible
    ///    seats' free work room, in whole seat-slots of this claim's bind eligibility
    ///    (`max(duty_bind, lock_2)`, what a seat must have room for to be drawn), over the panel's
    ///    seat count, must exceed the floor claims still waiting for a bind — `FloorRoomExhausted`;
    /// 2. the bond's cap on that pipeline ([`crate::palw_verify_capacity_v1::palw_bond_room_cap_v1`]
    ///    over the bonds with a floor claim waiting): its waiting floor claims plus this one within
    ///    its stake share and the free rounding slack — `BondClassShareExceeded` on the base class.
    ///    The shares divide the pipeline less its racing headroom
    ///    ([`crate::palw_verify_capacity_v1::palw_floor_room_race_headroom_v1`]), which stays open to
    ///    single units past a share: producers racing one tip are skipped (their carve burned) only
    ///    when more of them land first than the units free at that tip.
    ///
    /// The eligible seats are the draw's structural population for the floor: `Active` at the
    /// panel's floor and able to judge the base class. Both refusals are skips for the block's own
    /// attempt (step 4), so the block stands and still anchors.
    pub(super) fn check_floor_room_v1(
        &self,
        bond: &PalwBondKeyV2,
        claim_id: &Hash64,
        claim: &PalwClaimStateV2,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        let Some((room, waiting)) = self.floor_room_reading_v1(claim_id, claim, now_daa) else { return Ok(()) };
        let base = self.params.base_class_id();
        let pending: u64 = waiting.values().sum();
        if !room.admits_one() {
            return Err(PalwStateV2Error::FloorRoomExhausted { pending, slots: room.slots });
        }
        let own_waiting = waiting.get(bond).copied().unwrap_or(0);
        // The floor keeps its racing headroom out of the shares (the lane-verify review's racing
        // finding): small bonds racing the same tip draw single units from it, so a producer whose
        // pre-check passed at its tip is skipped — its withheld carve burned — only when more racers
        // land ahead of it than its tip showed free units.
        let capacity = room.capacity();
        let headroom = crate::palw_verify_capacity_v1::palw_floor_room_race_headroom_v1(capacity);
        // **ADR-0160 stage 4 (F-N): past F-N the pipeline is divided by lane N's rule** over the floor's
        // registered demand (`class_share_n_v1`): split-neutral, work-conserving, no per-bond slack unit.
        if self.params.capacity_network_active_at(now_daa) {
            return self.class_share_n_v1(bond, &base, capacity, headroom, &waiting, now_daa);
        }
        let share = self.room_cap_v1(capacity, headroom, bond, &waiting);
        if own_waiting.saturating_add(1) > share {
            return Err(PalwStateV2Error::BondClassShareExceeded {
                bond: *bond,
                class: base,
                unlicensed: own_waiting.min(u32::MAX as u64) as u32,
                share,
            });
        }
        Ok(())
    }

    /// **The floor room's capacity for `claim`** (J-6: the seats' free capital over its bind eligibility),
    /// lane N's `L_seat` for the base class; `None` where J-6 is no rule.
    pub(super) fn floor_room_capacity_v1(&self, claim: &PalwClaimStateV2, now_daa: u64) -> Option<u64> {
        self.floor_room_reading_v1(&Hash64::default(), claim, now_daa).map(|(room, _)| room.capacity())
    }

    /// **J-6's reading for a floor `claim`**: the room over the seats' capital and the floor claims waiting
    /// for a bind, by bond. `None` below `Params::palw_capacity_verify_room` and off the base class.
    pub(super) fn floor_room_reading_v1(
        &self,
        claim_id: &Hash64,
        claim: &PalwClaimStateV2,
        now_daa: u64,
    ) -> Option<(crate::palw_verify_capacity_v1::PalwFloorRoomV1, BTreeMap<PalwBondKeyV2, u64>)> {
        let base = self.params.base_class_id();
        if !self.params.capacity_room_active_at(now_daa) || claim.class_id != base {
            return None;
        }
        let seat_count = self
            .extras
            .model_registry
            .as_ref()
            .map(|fold| fold.globals.seat_count as u64)
            .unwrap_or(crate::palw_verification_profile_v1::PALW_SEAT_COUNT_V1 as u64);
        let eligibility = self.rcore_bind_prices(claim_id, claim, seat_count as usize, now_daa).eligibility;
        // Every seat keeps room for its own next claims before any is counted as a slot: the seats
        // are the network's anchors (past `palw_operator_anchor`, its only ones), and a seat whose
        // work room the flood's duties filled cannot make the attempt that binds the flood — its own
        // attempt fails the ceiling, no anchor comes, and the claims the room admitted void unbound.
        let anchor_reserve = palw_claim_commitment_v1(self.params, claim, now_daa)
            .unwrap_or(u128::MAX)
            .saturating_mul(crate::palw_verify_capacity_v1::PALW_FLOOR_ROOM_ANCHOR_RESERVE_V1 as u128);
        let floor = if self.extras.panel_economy_active {
            crate::palw_panel_economy_v1::palw_panel_collateral_floor_v1(self.params.min_collateral_sompi)
        } else {
            self.params.min_collateral_sompi
        };
        let escaped = self.second_clock_depth(now_daa);
        let rooms = self
            .state
            .bonds
            .iter()
            .filter(|(key, record)| {
                palw_bond_may_take_work_v2(record, floor) && palw_bond_may_judge_class_v4(self.state, key, record, &base, false, None)
            })
            .map(|(key, record)| {
                palw_rcore_gate_room_of_v1(
                    record.collateral,
                    self.params.fp_max_exposure_ratio_permille,
                    palw_bond_committed_v1(self.state, key, now_daa, escaped, self.params.window_court),
                    self.accuser_ledger_v1(key, now_daa),
                    PalwRcoreGateV1::Work,
                )
                .saturating_sub(anchor_reserve)
            });
        let mut waiting: BTreeMap<PalwBondKeyV2, u64> = BTreeMap::new();
        for (_, key) in self.state.unresolved.iter() {
            if let Some(live) = self.state.claims.get(key)
                && live.class_id == base
                && matches!(live.phase, PalwClaimPhaseV2::Provisional)
            {
                *waiting.entry(live.bond).or_default() += 1;
            }
        }
        let pending: u64 = waiting.values().sum();
        let room = crate::palw_verify_capacity_v1::palw_floor_room_v1(rooms, eligibility, seat_count, pending);
        Some((room, waiting))
    }

    /// **ADR-0160 F-N (stage 4): a class room divided by lane N's rule** — F-R's per-bond share past F-N.
    /// F-R's own share (`room_cap_v1`: the stake share among the room's holders plus ONE slack unit a
    /// bond, the floor's racing headroom drawn one unit a bond) lets ten 13k bonds take ten units where
    /// their 130k whole takes one past its share — the split the user's stage-4 property forbids ("13k ×
    /// 10 bonds never beats 130k × 1"). Past F-N the room is divided as lane N divides the network
    /// ([`crate::palw_network_room_v1::palw_network_share_admits_v1`]): the shares (by 13k units) divide
    /// `room − headroom` among the class's holders, the bonds whose demand for THIS class is registered and
    /// the asker; a bond takes a unit when what the registered bonds are owed leaves one, and the most-owed
    /// registered bond takes one whenever a unit is free — the headroom and the rounding slack are first
    /// come, with no per-bond count. `held` is the class's count by bond on F-R's own basis (the floor's claims
    /// waiting for a bind; a model class's unlicensed claims). A frozen bond takes nothing and is owed
    /// nothing; its claims still occupy the room.
    pub(super) fn class_share_n_v1(
        &self,
        bond: &PalwBondKeyV2,
        class_id: &Hash64,
        room: u64,
        headroom: u64,
        held: &BTreeMap<PalwBondKeyV2, u64>,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_network_room_v1 as n;
        let held_b = held.get(bond).copied().unwrap_or(0);
        let refused = |share: u64| PalwStateV2Error::BondClassShareExceeded {
            bond: *bond,
            class: *class_id,
            unlicensed: held_b.min(u32::MAX as u64) as u32,
            share,
        };
        let frozen = |key: &PalwBondKeyV2| crate::palw_verify_capacity_v1::palw_verify_bond_is_frozen_v1(self.state, key);
        if frozen(bond) {
            return Err(refused(0));
        }
        let units = |key: &PalwBondKeyV2| n::palw_network_units_v1(self.state.bonds.get(key).map_or(0, |record| record.collateral));
        let unlicensed: u64 = held.values().fold(0u64, |sum, n| sum.saturating_add(*n));
        // The class's holders divide the room; the bonds registered for THIS class (not frozen) are owed.
        let mut others: BTreeMap<PalwBondKeyV2, n::PalwNetworkBondV1> = held
            .iter()
            .filter(|(_, n)| **n > 0)
            .map(|(key, n)| (*key, n::PalwNetworkBondV1 { units: units(key), held: *n, registered: false }))
            .collect();
        for ((key, class), until) in self.state.network_demand.iter() {
            if class == class_id && *until >= now_daa && !frozen(key) {
                others.entry(*key).or_insert(n::PalwNetworkBondV1 { units: units(key), held: 0, registered: true }).registered = true;
            }
        }
        let b_registered = others.get(bond).is_some_and(|entry| entry.registered);
        n::palw_network_share_admits_v1(
            room,
            room.saturating_sub(headroom),
            unlicensed,
            bond,
            units(bond),
            held_b,
            b_registered,
            &others,
        )
        .map_err(|refusal| refused(refusal.share))
    }

    /// **The room a model class's per-bond shares divide past F-R**: `min(c_v2, ρ·⌈c_ship/2⌉)` (stage 1's
    /// clamp, [`Self::bond_class_share_for_v1`]); `None` where the share is no rule.
    pub(super) fn class_share_room_v1(&self, class_id: &Hash64, now_daa: u64) -> Option<u64> {
        let room = self.class_capacity_v1(class_id, now_daa)?;
        let rho = u64::from(crate::palw_weight_cap_v1::palw_capacity_rho_at_v1(self.params, now_daa));
        Some(match self.class_capacity_shipped_v1(class_id, now_daa) {
            Some(shipped) => room.min(rho.saturating_mul(shipped.div_ceil(2))),
            None => room,
        })
    }

    /// **ADR-0160 F-N (stage 4): lane N's admission** — the network level `L_net` ([`Self::network_level_v1`]),
    /// the network's unlicensed claims `U`, and the work-conserving share among the bonds whose demand is
    /// registered at `now_daa` ([`crate::palw_network_room_v1::palw_network_admits_v1`]). `Ok` below
    /// `Params::palw_capacity_network_room`.
    pub(super) fn check_network_room_v1(
        &self,
        bond: &PalwBondKeyV2,
        claim: &PalwClaimStateV2,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_network_room_v1 as n;
        if !self.params.capacity_network_active_at(now_daa) {
            return Ok(());
        }
        let level = self.network_level_v1(claim, now_daa);
        let mut held: BTreeMap<PalwBondKeyV2, u64> = BTreeMap::new();
        let mut unlicensed = 0u64;
        for live in self.state.claims.values() {
            if crate::palw_network_room_v1::palw_network_is_unlicensed_v1(live) {
                unlicensed += 1;
                *held.entry(live.bond).or_default() += 1;
            }
        }
        let units = |key: &PalwBondKeyV2| n::palw_network_units_v1(self.state.bonds.get(key).map_or(0, |record| record.collateral));
        // `Act(t)`: every bond holding an unlicensed claim, and every bond whose demand (for any class) is
        // registered at `now_daa` — `Reg(t)`, the ones owed their deficit.
        let mut others: BTreeMap<PalwBondKeyV2, n::PalwNetworkBondV1> =
            held.iter().map(|(key, n)| (*key, n::PalwNetworkBondV1 { units: units(key), held: *n, registered: false })).collect();
        for ((key, _), until) in self.state.network_demand.iter() {
            if *until >= now_daa {
                others.entry(*key).or_insert(n::PalwNetworkBondV1 { units: units(key), held: 0, registered: true }).registered = true;
            }
        }
        let b_registered = others.get(bond).is_some_and(|entry| entry.registered);
        n::palw_network_admits_v1(level, unlicensed, bond, units(bond), held.get(bond).copied().unwrap_or(0), b_registered, &others)
            .map_err(|refusal| PalwStateV2Error::NetworkRoomExhausted {
                bond: *bond,
                level: refusal.level,
                unlicensed: refusal.unlicensed,
                held: refusal.held,
                share: refusal.share,
                deficit_others: refusal.deficit_others,
            })
    }

    /// **`L_net = min(L_seat, L_carry, L_anchor)`** at `now_daa` (lane N, ADR-0160 v3 §7.1): the seats'
    /// room for unlicensed claims ([`Self::network_seat_level_v1`]), the licences the carriage lands over
    /// `H_L` and the binds the operator anchors make over an anchor delay, both over the operator ring's
    /// `ā_op` (floored at one block a DAA).
    pub(super) fn network_level_v1(&self, claim: &PalwClaimStateV2, now_daa: u64) -> u64 {
        use crate::palw_network_room_v1 as n;
        let lpb =
            if self.params.capacity_batch_active_at(now_daa) { n::PALW_NETWORK_LPB_BATCH_V1 } else { n::PALW_NETWORK_LPB_SINGLE_V1 };
        // **int-11: past `palw_capacity_network_verify` the level is also capped by `L_ver`** — what the seats can verify
        // inside the receipt window at the floor supply (lane P's finding on the live backlog); `None` below it: today's
        // level, byte for byte.
        // **ADR-0167: and `L_ver` steps with the issuance tier** — the verification supply the release adds (RFC-0006's sharding) counts from
        // ρ = 250, and the breaker (F-K) takes it back by lowering the tier, so a backlog or an overrun lowers the network level too.
        let tier = {
            let step = crate::palw_weight_cap_v1::palw_capacity_rho_at_v1(self.params, now_daa);
            if self.params.capacity_breaker_active_at(now_daa) {
                crate::palw_capacity_s567_v1::palw_breaker_tier_v1(step, self.state.rho_breaker_v1())
            } else {
                step
            }
        };
        let l_ver = self
            .params
            .capacity_network_verify_level_at(now_daa)
            .map(|base| base.saturating_mul(crate::palw_capacity_s567_v1::palw_verify_supply_milli_v1(tier)) / 1_000);
        n::palw_network_level_with_verify_v1(
            self.network_seat_level_v1(claim, now_daa),
            n::palw_network_a_op_milli_v1(&self.state.operator_ring, now_daa),
            lpb,
            self.params.capacity_network_anchor_delay(),
            l_ver,
        )
    }

    /// **`L_seat`: the unlicensed claims the network's seats hold room for** — on `U`'s basis, summed over
    /// the classes F-R prices: the floor's J-6 pipeline (the claims waiting for a bind and the ones still
    /// bindable, priced at `claim`'s own bind eligibility when it is a floor claim and at a floor template
    /// of its bond, DAA and escrow otherwise) plus the floor claims bound and not yet licensed (their seats'
    /// capital already holds them); and each model class's room as its per-bond shares divide it
    /// (`min(c_v2, ρ·⌈c_ship/2⌉)`). F-R keeps its per-class rooms and shares; this is the cross-class
    /// level lane N's share divides.
    pub(super) fn network_seat_level_v1(&self, claim: &PalwClaimStateV2, now_daa: u64) -> u64 {
        let base = self.params.base_class_id();
        let template;
        let floor_claim = if claim.class_id == base {
            claim
        } else {
            template = palw_claim_template_v1(base, claim.bond, now_daa, 0, claim.escrowed_reward);
            &template
        };
        let floor = self.floor_room_capacity_v1(floor_claim, now_daa).unwrap_or(0);
        let bound_floor = self
            .state
            .claims
            .values()
            .filter(|live| {
                live.class_id == base
                    && crate::palw_network_room_v1::palw_network_is_unlicensed_v1(live)
                    && !matches!(live.phase, PalwClaimPhaseV2::Provisional)
            })
            .count() as u64;
        let models: u64 = self
            .state
            .model_lifecycles
            .keys()
            .filter(|class_id| **class_id != base)
            .filter_map(|class_id| self.class_share_room_v1(class_id, now_daa))
            .fold(0u64, u64::saturating_add);
        floor.saturating_add(bound_floor).saturating_add(models)
    }

    /// **ADR-0152 v3.1 T-2(a): no bond holds more than `⌈c_class / 2⌉` unlicensed claims of a
    /// non-base class** (S-6; capital pricing, not identity — a second bond is a second
    /// `min_collateral` of stake, which is what T-3's ceiling already prices). Asked after
    /// `check_class_admits_claim` at the two doors a claim enters by — `apply_attempt` (the block's
    /// own attempt and every merged one) and the free-prompt commitment arm — for one more claim
    /// of `class_id` on `bond`, against S-1's per-bond count (`bond_class_unlicensed`, on T-2(c)'s
    /// licence predicate): refused with [`PalwStateV2Error::BondClassShareExceeded`] where
    /// `unlicensed + 1 > share`.
    ///
    /// Why it never disqualifies a block: the count can move under a producer between its
    /// pre-check and the fold — this block's own objects, an accusation re-opening a licensed
    /// claim — so the refusal joins `AttemptExposureCeiling` in step 4's own-attempt skip arm (the
    /// attempt is skipped, the block stands); step 4b skips merged work on any refusal; and a
    /// free-prompt commitment it refuses is dropped by the acceptance rehearsal, which folds the
    /// same arm. Below `Params::palw_rcore_plus` (every network but testnet-12, and testnet-12
    /// with the fence off) [`Self::bond_class_share_v1`] is `None` and this is `Ok` without
    /// reading anything.
    pub(super) fn check_bond_class_share(
        &self,
        bond: &PalwBondKeyV2,
        class_id: &Hash64,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        // RFC-0008 v2 (X8R): the composed run's test seam — empty in production (`test_admitted_class_v1`).
        if self.test_admitted_class_v1(class_id) {
            return Ok(());
        }
        // ADR-0160 F-R: past `palw_capacity_verify_room` the stake-proportional share replaces
        // T-2(a)'s `⌈c/2⌉` (`bond_class_share_for_v1`); below it this is T-2(a) byte for byte.
        // ADR-0160 stage 4 (F-N): past F-N a model class's room is divided by lane N's rule over the class's
        // registered demand (`class_share_n_v1`) — split-neutral, no per-bond slack unit.
        if self.params.capacity_network_active_at(now_daa) && self.params.capacity_room_active_at(now_daa) {
            let Some(room) = self.class_share_room_v1(class_id, now_daa) else { return Ok(()) };
            let held: BTreeMap<PalwBondKeyV2, u64> = self.with_inflight_index(|index| {
                index
                    .get(class_id)
                    .map(|tally| tally.unlicensed_by_bond.iter().map(|(holder, n)| (*holder, *n as u64)).collect())
                    .unwrap_or_default()
            });
            return self.class_share_n_v1(bond, class_id, room, 0, &held, now_daa);
        }
        let Some(share) = self.bond_class_share_for_v1(class_id, bond, now_daa) else { return Ok(()) };
        let unlicensed = self.bond_class_unlicensed(class_id, bond);
        if (unlicensed as u64).saturating_add(1) > share {
            return Err(PalwStateV2Error::BondClassShareExceeded { bond: *bond, class: *class_id, unlicensed, share });
        }
        Ok(())
    }

    pub(super) fn model_registry_inflight(&self, class_id: &Hash64) -> u32 {
        let audit = self.extras.audit_2026_09_23_active;
        let per_job = self.params.fp_quanta_per_canonical_job;
        let counted = self.with_inflight_index(|index| index.get(class_id).map(|tally| tally.counted(audit, per_job)).unwrap_or(0));
        // Step 3's reservation for this block's own attempt (`own_attempt_class`).
        counted.saturating_add(u32::from(self.own_attempt_class == Some(*class_id)))
    }

    /// **ADR-0152 T-2(a)'s reading (S-SPEC 1g): `bond`'s claims of `class_id` in flight that do not
    /// count as licensed** — the tally's `unlicensed_by_bond`, through the same cache as the room.
    #[allow(dead_code)]
    pub(super) fn bond_class_unlicensed(&self, class_id: &Hash64, bond: &PalwBondKeyV2) -> u32 {
        self.with_inflight_index(|index| {
            index.get(class_id).and_then(|tally| tally.unlicensed_by_bond.get(bond).copied()).unwrap_or(0)
        })
    }

    /// Read the in-flight index (audit #13b), building it from the live claims on first use.
    /// Outside the fold (no cache) the index is built for this one read.
    pub(super) fn with_inflight_index<R>(&self, read: impl FnOnce(&BTreeMap<Hash64, PalwInflightTallyV1>) -> R) -> R {
        let build = || palw_inflight_index_build_v1(self.state);
        match self.inflight_index {
            Some(cache) => {
                let mut slot = cache.borrow_mut();
                let index = slot.get_or_insert_with(build);
                read(index)
            }
            None => read(&build()),
        }
    }
}
