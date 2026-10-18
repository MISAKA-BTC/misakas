//! **The state parameters' rules (impl PalwStateParamsV2)** — moved out of `palw_state_v2.rs` (2026-10-12, a behaviour-preserving split: the items are the same; members the parent calls are `pub(super)`).

use super::*;

impl PalwStateParamsV2 {
    /// The challenge window for a claim licensed at `daa_score`: `window_challenge`, or
    /// [`PALW_SHORT_CHALLENGE_WINDOW_DAA_V1`] once `Params::palw_short_challenge_window` is in force
    /// (ADR-0132 §7.6 — the rule `6fdf6ba7` kept as a bare constant outside every fingerprint, made
    /// a fence). `None` never shortens.
    pub fn window_challenge_at(&self, daa_score: u64) -> u64 {
        match self.short_challenge_window_from_daa {
            Some(from) if daa_score >= from => PALW_SHORT_CHALLENGE_WINDOW_DAA_V1,
            _ => self.window_challenge,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        beta_permille: u16,
        window_bind: u64,
        window_receipt: u64,
        window_challenge: u64,
        window_court: u64,
        epoch_length: u64,
        base_class_id: Hash64,
        class_daa_max_factor: u32,
        budget_tolerance_permille: u32,
        min_collateral_sompi: u64,
        fp_attempt_share_permille: u16,
        fp_abandon_hold_daa: u64,
    ) -> Result<Self, PalwStateV2Error> {
        if beta_permille > 1000 {
            return Err(PalwStateV2Error::InvalidParams("beta_permille exceeds 1000 (β ≤ 1)"));
        }
        if window_bind == 0 || window_receipt == 0 || window_challenge == 0 || window_court == 0 {
            return Err(PalwStateV2Error::InvalidParams("every lattice window must be at least one DAA unit"));
        }
        if epoch_length == 0 {
            return Err(PalwStateV2Error::InvalidParams("epoch_length must be at least one DAA unit"));
        }
        if base_class_id == Hash64::default() {
            return Err(PalwStateV2Error::InvalidParams("a zero base class id names no liveness floor"));
        }
        if class_daa_max_factor < 2 {
            return Err(PalwStateV2Error::InvalidParams("max_factor below 2 freezes the retarget"));
        }
        if budget_tolerance_permille < 1000 {
            return Err(PalwStateV2Error::InvalidParams("a budget tolerance below unity starves every class of its own cadence"));
        }
        if budget_tolerance_permille > crate::palw_class_daa::PALW_CLASS_BUDGET_MAX_TOLERANCE_PERMILLE {
            return Err(PalwStateV2Error::InvalidParams("a budget tolerance above the ceiling is a cap no epoch can approach"));
        }
        if min_collateral_sompi == 0 {
            return Err(PalwStateV2Error::InvalidParams(
                "a zero minimum collateral bonds nothing — panel dedup would be free to defeat",
            ));
        }
        if fp_attempt_share_permille == 0 || fp_attempt_share_permille > 1000 {
            return Err(PalwStateV2Error::InvalidParams("the attempt share must be 1..=1000 permille — a zero floor has no beacons"));
        }
        Ok(Self {
            beta_permille,
            window_bind,
            window_receipt,
            window_challenge,
            window_court,
            epoch_length,
            base_class_id,
            class_daa_max_factor,
            budget_tolerance_permille,
            min_collateral_sompi,
            fp_attempt_share_permille,
            fp_abandon_hold_daa,
            fp_quanta_per_canonical_job: 0,
            fp_max_quanta_per_receipt: 0,
            fp_certified_classes: None,
            fp_max_exposure_ratio_permille: 1000,
            // Zero: `new` builds a network that pays no PALW reward, which is what every caller
            // predating the reward wiring meant and still means. See
            // `with_worker_carve_permille`.
            worker_carve_permille: 0,
            // OFF, for the reason on the field: a non-zero value is a rule about what a class's
            // registered slash value must be, and no existing preset's satisfies one.
            min_slash_permille_of_escrow: 0,
            // ADR-0056 Decisions 3 and 5 are OFF unless a network decides them: an all-zero economy is
            // exactly the behaviour every preset had before this ADR, so adding the fields moves
            // the fingerprint and moves nothing else.
            registration_exposure_sompi: 0,
            reclaim_epochs: 0,
            // **The floor's reserve, and there is exactly ONE of it.** Three lines of work reached
            // this same guard independently — an audit finding (999 individually-legal grants take
            // the floor to 1‰), ADR-0056's withdrawn Decision 4 walk, and the share-growth rule's own
            // reserve — and three fields would have been three places for the bound to differ.
            // This name is the audit's, because its version landed first and closes a measured
            // critical; every path that can move a permille now checks it.
            min_base_class_share_permille: 100,
            short_challenge_window_from_daa: None,
            // The whole session as one rung: the identity that leaves the backstop as the only
            // clock. See the field doc.
            turn_deadline_daa: window_court,
            claim_retirement_daa: 0,
            // Zero: `new` builds a network whose share table moves only at an activation grant,
            // which is what every caller predating ADR-0054 meant. See
            // `with_class_share_growth_v1`.
            class_growth_permille: 0,
            fp_decode_rules_daa: None,
            fp_decode_rules_from_daa: None,
            class_receipt_window: None,
            escrow_backed_exposure_from_daa: None,
            capacity_weight_cap_from_daa: None,
            rcore_plus_from_daa: None,
            capacity_liability: None,
            withdrawal_delay_daa: 0,
            capacity_escrow_from_daa: None,
            rcore_conservative_classes: Vec::new(),
            final_lock_full_collateral_from_daa: None,
            capacity_room_from_daa: None,
            capacity_batch_from_daa: None,
            capacity_audit_from_daa: None,
            capacity_audit_operators: Vec::new(),
            capacity_slots_from_daa: None,
            capacity_network_from_daa: None,
            capacity_network_anchor_delay: 0,
            capacity_network_verify_from_daa: None,
            panel_unavailable_expiry_from_daa: None,
            panel_fast_switch_from_daa: None,
            seat_availability_from_daa: None,
            capacity_emission_from_daa: None,
            capacity_riders_from_daa: None,
            capacity_breaker_from_daa: None,
            class_verify_deadline_from_daa: None,
            class_verify_rows: Vec::new(),
            held_unanswerable_classes: Vec::new(),
            readiness_v2_max_age_spans: None,
            final_lock_life_retro_from_daa: None,
            registry_resilience_from_daa: None,
            final_lock_life_from_daa: None,
            floor_refusal_retry_from_daa: None,
            tir_from_daa: None,
            gen_from_daa: None,
            fp_job_v5_from_daa: None,
            gen_max_inflight_claims: [0; 4],
            tir_fence2_from_daa: None,
            improve_from_daa: None,
            improve_ceilings: None,
            improve_lifecycle_base_daa: None,
            held_close_chunks_from_daa: None,
            gen_range_twin_from_daa: None,
            task_heads_from_daa: None,
            task_heads_max_inflight_claims: 0,
            vertex_from_daa: None,
            exec_v2_from_daa: None,
            witness_manifest_from_daa: None,
            audit_mesh_from_daa: None,
            capped_from_daa: None,
            tir_shard_from_daa: None,
            tir_shard_segment_from_daa: None,
            adapter_class_from_daa: None,
            fp_decode_constraint_from_daa: None,
            class_seating: None,
            tir_only_from_daa: None,
            floor_reserve_from_daa: None,
            anchor_window_from_daa: None,
            panel_v3: None,
            panel_lifetime_v2: None,
            palw_claim_challenge_v2: None,
            palw_claim_compact_v4: None,
            bond_budget: None,
            bond_formula_v2: None,
            reporter_income_from_daa: None,
            material_income_from_daa: None,
            monetary_material: None,
            material_release_recovery_v2: None,
            econ_contracts_v1: Default::default(),
            selected_source_bootstrap_v1: None,
            paid_consumer_lifecycle: None,
            audit_1004_from_daa: None,
            reporter_share_v2_from_daa: None,
            legacy_public_filer_from_daa: None,
            court_model_scope_from_daa: None,
            public_evidence_reward_from_daa: None,
            claim_hard_deadline_v1: None,
            cell_coverage_closure_from_daa: None,
            model_eligibility_v1: None,
        })
    }

    /// Set ADR-0133 §11.3's class-derived receipt deadline. Refuses a zero span, which would make
    /// every class's derived window zero and quietly hand the whole network back the global one.
    pub fn with_class_receipt_window(mut self, activation_daa: u64, span_daa: u64) -> Result<Self, PalwStateV2Error> {
        if span_daa == 0 {
            return Err(PalwStateV2Error::InvalidParams("a zero schedule span derives a zero window for every class"));
        }
        self.class_receipt_window = Some(PalwClassReceiptWindowV1 { activation_daa, span_daa });
        Ok(self)
    }

    /// **ADR-0133 §11.3: the receipt window a claim of `class_id`, bound at `bound_daa`, is
    /// judged against, in DAA.**
    ///
    /// Below the fence — and for a class the registry holds no row for — this is the network's
    /// `window_receipt` unchanged, so every claim already on the chain keeps the deadline it was
    /// bound under. Past it a rowed class is judged against the LARGER of that and the window its
    /// own graph derives, `verification_window_spans × span_daa`.
    ///
    /// **Larger, never smaller.** A class that already fits the global deadline (§11.3's
    /// `window_fits_receipt`) derives a window at or under `window_receipt`, so the `max` leaves
    /// it exactly where it was. The fence cannot tighten anybody's window; it widens only the
    /// class whose replay the one global number was never sized for, and it widens that class's
    /// claims ALONE — the global deadline is untouched, which is the whole reason §11.3 refused
    /// to stretch it.
    ///
    /// **The fence is asked against `bound_daa`, not the block being folded**, because this
    /// answer has to be the same one every time it is computed: at bind, at each receipt, at the
    /// sweep, and at the rebuild after a restart, where no block DAA exists. For the same reason
    /// the only state it reads is `verification_window_spans`, which a class's registered work
    /// fixes for its whole life — a row's `state`, `ready_seats` and utilization all move, and a
    /// deadline derived from those would move under a claim that had already been bound.
    ///
    /// **ADR-0152 §4-quater V3: past `Params::palw_class_verify_deadline` (at `bound_daa`) this is
    /// `W_r(c) = max(window_receipt, D(c))`** with `D(c)` = [`Self::claim_verify_daa_v1`] — the class's
    /// window counted in reference spans of 5 DAA, not in the lane's span (1 DAA on testnet-12, which
    /// gave the 2M row 2,799 DAA of a 13,995-DAA derivation). `shape` is the claim's
    /// ([`crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim`]); a class-level
    /// reader asks for the canonical job (`Attempt`). Below the fence `shape` is unread and this is
    /// §11.3's rule byte for byte.
    pub fn receipt_window_for_claim_v1(
        &self,
        state: &PalwChainStateV2,
        class_id: &Hash64,
        shape: crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1,
        bound_daa: u64,
    ) -> u64 {
        if self.class_verify_deadline_active_at(bound_daa) {
            return self.window_receipt.max(self.claim_verify_daa_v1(state, class_id, shape, bound_daa));
        }
        let Some(rule) = self.class_receipt_window else {
            return self.window_receipt;
        };
        if bound_daa < rule.activation_daa {
            return self.window_receipt;
        }
        let Some(row) = state.model_lifecycle(class_id) else {
            return self.window_receipt;
        };
        self.window_receipt.max((row.profile.verification_window_spans as u64).saturating_mul(rule.span_daa.max(1)))
    }

    /// **ADR-0152 §4-quater V1: `D(c)`, the compute-bearing verification deadline of a claim of
    /// `class_id` and `shape` bound at `bound_daa`, in DAA.** A pure function of the claim's class
    /// row (its registered `verification_ccu`, fixed for the class's life), the class's published
    /// free-prompt work profile (write-once, and past `palw_fp_derived_work` present before the first
    /// free-prompt claim), the measured rows and `bound_daa` — so it is the same at the bind, at every
    /// receipt, at the sweep and at the rebuild.
    ///
    /// Past the fence at `bound_daa`:
    /// * a class with an active measured row at `bound_daa` (ADR-0153's flag day) —
    ///   `min(D_cap, ⌈(m·T_R(N) + t_fixed) / τ⌉)` at the claim's `N` ([`PalwClassVerifyRowV1::daa`]);
    /// * otherwise the derived branch: an attempt is `5 × verification_window_spans`
    ///   ([`crate::palw_class_verify_deadline_v1::palw_derived_verify_daa_v1`]; floor 10, Qwen3.6 10,
    ///   8k 15, 2M 13,995); a free-prompt claim is the larger of that and the class's largest run
    ///   ([`palw_class_fp_verify_daa_v1`]); a class without a registry row has no canonical term (0).
    ///
    /// Below the fence it is §11.3's product verbatim — `verification_window_spans × span_daa` where
    /// §11.3 is in force at `bound_daa` for a rowed class (spans × 5 on testnet-11), 0 otherwise — and
    /// nothing below the fence reads it for a rule.
    ///
    /// [`PalwClassVerifyRowV1::daa`]: crate::palw_class_verify_deadline_v1::PalwClassVerifyRowV1::daa
    pub fn claim_verify_daa_v1(
        &self,
        state: &PalwChainStateV2,
        class_id: &Hash64,
        shape: crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1,
        bound_daa: u64,
    ) -> u64 {
        use crate::palw_class_verify_deadline_v1::{PalwClaimVerifyShapeV1, palw_derived_verify_daa_v1};
        if !self.class_verify_deadline_active_at(bound_daa) {
            return match (self.class_receipt_window, state.model_lifecycle(class_id)) {
                (Some(rule), Some(row)) if bound_daa >= rule.activation_daa => {
                    (row.profile.verification_window_spans as u64).saturating_mul(rule.span_daa.max(1))
                }
                _ => 0,
            };
        }
        if let Some(row) = self.class_verify_row_at(class_id, bound_daa) {
            return row.daa(row.positions_of(shape));
        }
        let canonical = state
            .model_lifecycle(class_id)
            .map_or(0, |row| palw_derived_verify_daa_v1(palw_class_verify_ccu_v1(state, class_id, row.work.verification_ccu)));
        match shape {
            PalwClaimVerifyShapeV1::Attempt => canonical,
            PalwClaimVerifyShapeV1::FreePrompt { .. } => canonical.max(palw_class_fp_verify_daa_v1(state, class_id).unwrap_or(0)),
        }
    }

    /// **ADR-0152 §4-quater: sets the bundle's copies of `Params::palw_class_verify_deadline` and
    /// `Params::palw_class_verify_rows`** — written by `Params::sync_palw_class_verify_deadline` and by
    /// nothing else, so the two cannot drift.
    pub fn with_class_verify_deadline(
        mut self,
        from_daa: Option<u64>,
        rows: Vec<crate::palw_class_verify_deadline_v1::PalwClassVerifyRowV1>,
    ) -> Self {
        self.class_verify_deadline_from_daa = from_daa;
        self.class_verify_rows = rows;
        self
    }

    /// **The readiness-V2 horizon's mirror** (user decision 2026-09-25, readiness capacity option
    /// (a)): the bundle's copy of `Params::palw_readiness_v2_max_age_spans`, written by
    /// `Params::sync_palw_readiness_v2_max_age_spans` and by nothing else; `None` where the fence is
    /// not armed.
    pub fn with_readiness_v2_max_age_spans(mut self, spans: Option<u32>) -> Self {
        self.readiness_v2_max_age_spans = spans;
        self
    }

    /// The mirror as it is (`None` on every network but testnet-12).
    pub fn readiness_v2_max_age_spans(&self) -> Option<u32> {
        self.readiness_v2_max_age_spans
    }

    /// **Lane F1: the registry-resilience mirror** — written by `Params::sync_palw_registry_resilience`
    /// and by nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_registry_resilience_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.registry_resilience_from_daa = from_daa;
        self
    }

    /// Lane F1: `Params::palw_registry_resilience`'s height, if the network arms it (the mirror).
    pub fn registry_resilience_from_daa(&self) -> Option<u64> {
        self.registry_resilience_from_daa
    }

    /// **Lane F2: the floor-refusal-retry mirror** — written by `Params::sync_palw_floor_refusal_retry`
    /// and by nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_floor_refusal_retry_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.floor_refusal_retry_from_daa = from_daa;
        self
    }

    /// Lane F2: `Params::palw_floor_refusal_retry`'s height, if the network arms it (the mirror).
    pub fn floor_refusal_retry_from_daa(&self) -> Option<u64> {
        self.floor_refusal_retry_from_daa
    }

    /// **RFC-0002 Phase F: the IR fence's mirror** — written by `Params::sync_palw_tir_v1` and by
    /// nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_tir_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.tir_from_daa = from_daa;
        self
    }

    /// `Params::palw_tir_v1`'s height, if the network arms it (the mirror).
    pub fn tir_from_daa(&self) -> Option<u64> {
        self.tir_from_daa
    }

    /// **Is `palw_tir_v1` in force at `daa_score`?** `false` on every shipped preset.
    pub fn tir_active_at(&self, daa_score: u64) -> bool {
        self.tir_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **RFC-0003: the generative fence's mirror** — written by `Params::sync_palw_gen_v1` and by
    /// nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_gen_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.gen_from_daa = from_daa;
        self
    }

    /// `Params::palw_gen_v1`'s height, if the network arms it (the mirror).
    pub fn gen_from_daa(&self) -> Option<u64> {
        self.gen_from_daa
    }

    /// **Is `palw_gen_v1` in force at `daa_score`?** `false` on every shipped preset.
    pub fn gen_active_at(&self, daa_score: u64) -> bool {
        self.gen_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **RFC-0003 §I.4: the tensor lane's mirror of `Params::palw_fp_job_v5`'s height** — written by
    /// `Params::sync_palw_gen_v1` and by nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_fp_job_v5_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.fp_job_v5_from_daa = from_daa;
        self
    }

    /// `Params::palw_fp_job_v5`'s height, if the network arms it (the mirror).
    pub fn fp_job_v5_from_daa(&self) -> Option<u64> {
        self.fp_job_v5_from_daa
    }

    /// **Is `palw_fp_job_v5` in force at `daa_score`?** `false` on every shipped preset.
    pub fn fp_job_v5_active_at(&self, daa_score: u64) -> bool {
        self.fp_job_v5_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **RFC-0003 §I.4.8: the generative fence's per-profile cap on claims in flight** — written by
    /// `Params::sync_palw_gen_v1` and by nothing else (and by fixtures); zeros where the fence is not armed.
    pub fn with_gen_max_inflight_claims(mut self, caps: [u32; 4]) -> Self {
        self.gen_max_inflight_claims = caps;
        self
    }

    /// The cap on one class's claims in flight for `profile` (`Image`, `Embedding`, `Audio`, `Video`; 0
    /// for any other, and where the fence is not armed).
    pub fn gen_max_inflight_claims(&self, profile: crate::palw_gen_v1::PalwGenProfileV1) -> u32 {
        // The `Head` profile's cap is its own fence's (`palw_task_heads_v1`), not one of `palw_gen_v1`'s four.
        if profile == crate::palw_gen_v1::PalwGenProfileV1::Head {
            return self.task_heads_max_inflight_claims;
        }
        (profile as usize).checked_sub(1).and_then(|i| self.gen_max_inflight_claims.get(i)).copied().unwrap_or(0)
    }

    /// Every profile's cap, in `Image`, `Embedding`, `Audio`, `Video` order (the mirror as written).
    pub fn gen_max_inflight_claims_all(&self) -> [u32; 4] {
        self.gen_max_inflight_claims
    }

    /// **RFC-0002 Phase F: the second IR fence's mirror** — written by `Params::sync_palw_tir_fence2`
    /// and by nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_tir_fence2_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.tir_fence2_from_daa = from_daa;
        self
    }

    /// **RFC-0006: the layer-sharded panels' mirror** — written by `Params::sync_palw_tir_shard_v1` and by nothing else
    /// (and by fixtures); `None` where the fence is not armed.
    pub fn with_tir_shard_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.tir_shard_from_daa = from_daa;
        self
    }

    /// `Params::palw_tir_shard_v1`'s height, if the network arms it (the mirror).
    pub fn tir_shard_from_daa(&self) -> Option<u64> {
        self.tir_shard_from_daa
    }

    /// **Are layer-sharded panels in force at `daa_score`?** `false` on every shipped preset.
    pub fn tir_shard_active_at(&self, daa_score: u64) -> bool {
        self.tir_shard_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **RFC-0006 per-segment pricing's mirror** — written by `Params::sync_palw_tir_shard_segment_v2` and by nothing else (and
    /// by fixtures); `None` where the fence is not armed.
    pub fn with_tir_shard_segment_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.tir_shard_segment_from_daa = from_daa;
        self
    }

    /// `Params::palw_tir_shard_segment_v2`'s height, if the network arms it (the mirror).
    pub fn tir_shard_segment_from_daa(&self) -> Option<u64> {
        self.tir_shard_segment_from_daa
    }

    /// **Is per-segment pricing in force for a panel bound at `bound_daa`?** `false` on every shipped preset.
    pub fn tir_shard_segment_active_at(&self, bound_daa: u64) -> bool {
        self.tir_shard_segment_from_daa.is_some_and(|from| bound_daa >= from)
    }

    /// `Params::palw_tir_fence2`'s height, if the network arms it (the mirror).
    pub fn tir_fence2_from_daa(&self) -> Option<u64> {
        self.tir_fence2_from_daa
    }

    /// **Is the second IR fence in force at `daa_score`?** `false` on every shipped preset.
    pub fn tir_fence2_active_at(&self, daa_score: u64) -> bool {
        self.tir_fence2_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **RFC-0004: the improvement fence's mirror** — written by `Params::sync_palw_improvement_v1` and
    /// by nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_improve_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.improve_from_daa = from_daa;
        self
    }

    /// RFC-0004: the fence's ceilings' mirror, written with its height by
    /// `Params::sync_palw_improvement_v1`.
    pub fn with_improve_ceilings(mut self, ceilings: Option<crate::palw_improve_v1::PalwImprovementCeilingsV1>) -> Self {
        self.improve_ceilings = ceilings;
        self
    }

    /// The fence's ceilings, if the network arms it (the mirror).
    pub fn improve_ceilings(&self) -> Option<crate::palw_improve_v1::PalwImprovementCeilingsV1> {
        self.improve_ceilings
    }

    /// RFC-0004 A6: the claim lifecycle bound's DAA-independent mirror, written with the fence's height by
    /// `Params::sync_palw_improvement_v1`.
    pub fn with_improve_lifecycle_base(mut self, base_daa: Option<u64>) -> Self {
        self.improve_lifecycle_base_daa = base_daa;
        self
    }

    /// The mirror as it is: the anchor slot and the licence allowance, if the network arms the fence.
    pub fn improve_lifecycle_base_daa(&self) -> Option<u64> {
        self.improve_lifecycle_base_daa
    }

    /// **`Λ` at `daa`** (spec 17 §17.4.3 rows 19–20): the mirror plus the challenge window in force for a claim licensed at
    /// `daa` ([`Self::window_challenge_at`]), never less than the challenge floor — what an epoch's windows are held to when a
    /// policy is applied at `daa`. A fence that only shortens the window (ADR-0132 §7.6) can only shorten `Λ`, so a policy that
    /// held it when applied holds it ever after. `None` where the fence is not armed (or a fixture sets no mirror).
    pub fn improve_lifecycle_at(&self, daa: u64) -> Option<u64> {
        self.improve_lifecycle_base_daa
            .map(|base| crate::palw_improve_v1::palw_improvement_claim_lifecycle_v1(base, self.window_challenge_at(daa)))
    }

    /// `Params::palw_improvement_v1`'s height, if the network arms it (the mirror).
    pub fn improve_from_daa(&self) -> Option<u64> {
        self.improve_from_daa
    }

    /// **Is `palw_improvement_v1` in force at `daa_score`?** `false` on every shipped preset.
    pub fn improve_active_at(&self, daa_score: u64) -> bool {
        self.improve_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **Is pipeline-claim data availability in force at `daa_score`?** (spec 17 §17.14) `palw_improvement_v1`'s
    /// height — the one place the choice of fence is made, so the fold, the acceptance walk and the gate
    /// cannot disagree. `false` on every shipped preset.
    pub fn pipeline_da_active_at(&self, daa_score: u64) -> bool {
        self.improve_active_at(daa_score)
    }

    /// **RFC-0003 decision 22: the held leaf challenge's mirror** — written by
    /// `Params::sync_palw_held_close_chunks_v1` and by nothing else (and by fixtures); `None` where the
    /// fence is not armed.
    /// **ADR-0165: the floor reserve's mirror** — written by `Params::sync_palw_floor_reserve_v1` and by
    /// nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_floor_reserve_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.floor_reserve_from_daa = from_daa;
        self
    }

    /// `Params::palw_floor_reserve_v1`'s height, if the network arms it (the mirror).
    pub fn floor_reserve_from_daa(&self) -> Option<u64> {
        self.floor_reserve_from_daa
    }

    /// **Is the base floor a reserve at `daa_score`?** `false` on every shipped preset.
    pub fn floor_reserve_active_at(&self, daa_score: u64) -> bool {
        self.floor_reserve_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **ADR-0170: the seed anchor window's mirror** — written by `Params::sync_palw_anchor_window_v1` and by nothing else (and by
    /// fixtures); `None` where the fence is not armed.
    pub fn with_anchor_window_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.anchor_window_from_daa = from_daa;
        self
    }

    /// **Lane PA: the audit-1004 fence's mirror** — written by `Params::sync_palw_audit_1004_v1` and by nothing else (and by fixtures).
    pub fn with_audit_1004_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.audit_1004_from_daa = from_daa;
        self
    }

    /// `Params::palw_audit_1004_v1`'s height, if the network arms it (the mirror).
    pub fn audit_1004_from_daa(&self) -> Option<u64> {
        self.audit_1004_from_daa
    }

    /// **Are the audit-1004 rules in force at `daa_score`?** `false` on every shipped preset.
    pub fn audit_1004_active_at(&self, daa_score: u64) -> bool {
        self.audit_1004_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **Lane INTF: the reporter share's mirror** — written by `Params::sync_palw_reporter_share_v2` and by nothing else (and by
    /// fixtures, which is how a test arms the 49% share); `None` where the fence is not armed.
    pub fn with_reporter_share_v2_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.reporter_share_v2_from_daa = from_daa;
        self
    }

    /// `Params::palw_reporter_share_v2`'s height, if the network arms it (the mirror).
    pub fn reporter_share_v2_from_daa(&self) -> Option<u64> {
        self.reporter_share_v2_from_daa
    }

    /// **Is ADR-0032's amended reporter share in force at `daa_score`?** `false` on every shipped preset.
    pub fn reporter_share_v2_active_at(&self, daa_score: u64) -> bool {
        self.reporter_share_v2_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **The PALW reporter share, in basis points, in force at `daa_score`** — [`PALW_RCORE_REPORTER_REWARD_BPS_V1`] (1,000)
    /// below `Params::palw_reporter_share_v2` (every block of every shipped network), [`PALW_RCORE_REPORTER_REWARD_BPS_V2`]
    /// (4,900) at or past it. **Which DAA fixes an amount:** R-1 reads it at the conviction's close (the block that consumes
    /// the offence and opens the reward — `accepted_daa` of the consumed record), DA-6 at the session's open (the accusation
    /// block — the exposure is stored on the session and never re-priced). So a reward or an exposure written below the fence
    /// keeps its 10% amount through every later block, sweep, refund, burn, reorg replay and reload.
    pub fn reporter_reward_bps_at(&self, daa_score: u64) -> u16 {
        if self.reporter_share_v2_active_at(daa_score) { PALW_RCORE_REPORTER_REWARD_BPS_V2 } else { PALW_RCORE_REPORTER_REWARD_BPS_V1 }
    }

    /// **Lane LG14-A: the public filer's mirror** — written by `Params::sync_palw_legacy_public_filer_v1` and by nothing else (and by
    /// fixtures, which is how a test arms it); `None` where the fence is not armed.
    pub fn with_legacy_public_filer_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.legacy_public_filer_from_daa = from_daa;
        self
    }

    /// `Params::palw_legacy_public_filer_v1`'s height, if the network arms it (the mirror).
    pub fn legacy_public_filer_from_daa(&self) -> Option<u64> {
        self.legacy_public_filer_from_daa
    }

    /// **Is the legacy route's public filer in force at `daa_score`?** `false` on every shipped preset.
    pub fn legacy_public_filer_active_at(&self, daa_score: u64) -> bool {
        self.legacy_public_filer_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **The 2026-10-12 rulings' mirrors** (`crate::palw_ruling_fences_v1`) — written by
    /// `Params::sync_palw_2026_10_12_ruling_fences_v1` and by nothing else (and by fixtures, which is how a test arms them);
    /// `None` where a fence is not armed.
    pub fn with_court_model_scope_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.court_model_scope_from_daa = from_daa;
        self
    }

    /// `Params::palw_court_model_scope_v1`'s height, if the network arms it (the mirror; register D2).
    pub fn court_model_scope_from_daa(&self) -> Option<u64> {
        self.court_model_scope_from_daa
    }

    /// N4's mirror (see [`Self::with_court_model_scope_from_daa`]).
    pub fn with_public_evidence_reward_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.public_evidence_reward_from_daa = from_daa;
        self
    }

    /// `Params::palw_public_evidence_reward_v1`'s height, if the network arms it (the mirror; register N4).
    pub fn public_evidence_reward_from_daa(&self) -> Option<u64> {
        self.public_evidence_reward_from_daa
    }

    /// **Is N4 in force for a claim accepted at `daa_score`?** `false` on every shipped preset.
    pub fn public_evidence_reward_active_at(&self, daa_score: u64) -> bool {
        self.public_evidence_reward_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// X3's mirror (see [`Self::with_court_model_scope_from_daa`]).
    pub fn with_claim_hard_deadline_v1(mut self, mirror: Option<crate::palw_ruling_fences_v1::PalwClaimHardDeadlineMirrorV1>) -> Self {
        self.claim_hard_deadline_v1 = mirror;
        self
    }

    /// `Params::palw_claim_hard_deadline_v1`'s height and policy, if the network arms it (the mirror; register X3).
    pub fn claim_hard_deadline_v1(&self) -> Option<crate::palw_ruling_fences_v1::PalwClaimHardDeadlineMirrorV1> {
        self.claim_hard_deadline_v1
    }

    /// **R6-CELL round 4's mirror** (`crate::palw_cell_coverage_closure_v1`) — written by `Params::sync_palw_cell_coverage_closure_v1`
    /// and by nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_cell_coverage_closure_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.cell_coverage_closure_from_daa = from_daa;
        self
    }

    /// `Params::palw_cell_coverage_closure_v1`'s height, if the network arms it (the mirror).
    pub fn cell_coverage_closure_from_daa(&self) -> Option<u64> {
        self.cell_coverage_closure_from_daa
    }

    /// **RFC-0017's mirror** (`crate::palw_model_eligibility_v1`) — written by `Params::sync_palw_model_eligibility_v1` and by
    /// nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_model_eligibility_v1(
        mut self,
        mirror: Option<crate::palw_model_eligibility_v1::PalwModelEligibilityMirrorV1>,
    ) -> Self {
        self.model_eligibility_v1 = mirror;
        self
    }

    /// `Params::palw_model_eligibility_v1`'s height and policy, if the network arms it (the mirror).
    pub fn model_eligibility_v1(&self) -> Option<&crate::palw_model_eligibility_v1::PalwModelEligibilityMirrorV1> {
        self.model_eligibility_v1.as_ref()
    }

    /// The mirror if it governs `daa` (A-2: `None` below the fence and on every shipped network).
    pub fn model_eligibility_v1_at(&self, daa: u64) -> Option<&crate::palw_model_eligibility_v1::PalwModelEligibilityMirrorV1> {
        self.model_eligibility_v1.as_ref().filter(|m| m.governs(daa))
    }

    /// **RFC-0010: the permissionless Panel's mirror** — written by `Params::sync_palw_permissionless_panel_v1` and by nothing else
    /// (and by fixtures); `None` where the fence is not armed.
    pub fn with_panel_v3(mut self, mirror: Option<crate::palw_permissionless_panel_v1::PalwPanelV3ParamsV1>) -> Self {
        self.panel_v3 = mirror;
        self
    }

    /// The permissionless Panel's mirror, if the network arms it.
    pub fn panel_v3(&self) -> Option<&crate::palw_permissionless_panel_v1::PalwPanelV3ParamsV1> {
        self.panel_v3.as_ref()
    }

    /// **Lane BUDGET: the bond budget's mirror** — written by `Params::sync_palw_bond_budget_v1` and by nothing else (and by fixtures);
    /// `None` where the fence is not armed.
    pub fn with_bond_budget(mut self, mirror: Option<crate::palw_bond_budget_v1::PalwBondBudgetMirrorV1>) -> Self {
        self.bond_budget = mirror;
        self
    }

    /// The bond budget's mirror, if the network arms the fence.
    pub fn with_reporter_income_from(mut self, from: Option<u64>) -> Self {
        self.reporter_income_from_daa = from;
        self
    }

    pub fn reporter_income_from(&self) -> Option<u64> {
        self.reporter_income_from_daa
    }

    pub fn reporter_income_active_at(&self, daa: u64) -> bool {
        self.bond_budget_active_at(daa) && self.reporter_income_from_daa.is_some_and(|from| daa >= from)
    }

    pub fn with_material_income_from(mut self, from: Option<u64>) -> Self {
        self.material_income_from_daa = from;
        self
    }
    pub fn material_income_from(&self) -> Option<u64> {
        self.material_income_from_daa
    }
    pub fn material_income_active_at(&self, daa: u64) -> bool {
        self.reporter_income_active_at(daa) && self.material_income_from_daa.is_some_and(|from| daa >= from)
    }

    pub fn with_paid_consumer_lifecycle(
        mut self,
        mirror: Option<crate::palw_bond_budget_v1::PalwPaidConsumerLifecycleMirrorV1>,
    ) -> Self {
        self.paid_consumer_lifecycle = mirror;
        self
    }
    pub fn paid_consumer_lifecycle(&self) -> Option<&crate::palw_bond_budget_v1::PalwPaidConsumerLifecycleMirrorV1> {
        self.paid_consumer_lifecycle.as_ref()
    }
    pub fn paid_consumer_lifecycle_active_at(&self, daa: u64) -> bool {
        self.material_income_active_at(daa) && self.paid_consumer_lifecycle.as_ref().is_some_and(|f| daa >= f.from_daa)
    }

    pub fn bond_budget(&self) -> Option<&crate::palw_bond_budget_v1::PalwBondBudgetMirrorV1> {
        self.bond_budget.as_ref()
    }

    /// **ADR-0176 amendment: `palw_bond_formula_v2`'s mirror** — written by `Params::sync_palw_bond_formula_v2` (and fixtures).
    pub fn with_bond_formula_v2(mut self, mirror: Option<crate::palw_bond_formula_v2::PalwBondFormulaMirrorV2>) -> Self {
        self.bond_formula_v2 = mirror;
        self
    }

    /// The bond formula's mirror, if the network arms the fence (`None` on every shipped preset).
    pub fn bond_formula_v2(&self) -> Option<&crate::palw_bond_formula_v2::PalwBondFormulaMirrorV2> {
        self.bond_formula_v2.as_ref()
    }

    /// **Is the bond budget in force at `daa_score`?** `false` on every shipped preset.
    pub fn bond_budget_active_at(&self, daa_score: u64) -> bool {
        self.bond_budget.as_ref().is_some_and(|mirror| mirror.active_at(daa_score))
    }

    /// **Which rule binds a claim accepted at `accepted_daa`?** `true`: the permissionless Panel (V3) — the fence is in force at
    /// the claim's ACCEPTANCE (never at a later binder or retry block) and R-core+ is (the V3 fold prices its seats on the one
    /// R-core+ ledger). `false`: the historical lane-A lattice, byte for byte. `false` on every shipped preset.
    pub fn panel_v3_rule_at(&self, accepted_daa: u64) -> bool {
        self.panel_v3.is_some_and(|mirror| accepted_daa >= mirror.from_daa) && self.rcore_plus_active_at(accepted_daa)
    }

    /// `Params::palw_anchor_window_v1`'s height, if the network arms it (the mirror).
    pub fn anchor_window_from_daa(&self) -> Option<u64> {
        self.anchor_window_from_daa
    }

    /// **Is the seed anchor a window at `daa_score`?** `false` on every shipped preset.
    pub fn anchor_window_active_at(&self, daa_score: u64) -> bool {
        self.anchor_window_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **The window the jury and the schedule seeding read at `daa_score`, in spans** — [`crate::palw_anchor_window_v1::PALW_ANCHOR_WINDOW_SPANS_V1`]
    /// past the fence, `None` below it (where the anchor is the span before's, as it was).
    pub fn anchor_window_spans_at(&self, daa_score: u64) -> Option<u64> {
        self.anchor_window_active_at(daa_score).then_some(crate::palw_anchor_window_v1::PALW_ANCHOR_WINDOW_SPANS_V1)
    }

    /// **M1 at `daa_score`: does an admitted attempt a block MERGES record the seed anchor?** The fence is in force and the variant
    /// keeps M1 ([`crate::palw_anchor_window_v1::PALW_ANCHOR_WINDOW_MERGED_V1`]).
    pub fn anchor_window_records_merged_at(&self, daa_score: u64) -> bool {
        self.anchor_window_active_at(daa_score) && crate::palw_anchor_window_v1::PALW_ANCHOR_WINDOW_MERGED_V1
    }

    /// **The generative range twin's mirror** — written by `Params::sync_palw_gen_range_twin_v1` and by nothing else (and by
    /// fixtures); `None` where the fence is not armed.
    pub fn with_gen_range_twin_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.gen_range_twin_from_daa = from_daa;
        self
    }

    /// `Params::palw_gen_range_twin_v1`'s height, if the network arms it (the mirror).
    pub fn gen_range_twin_from_daa(&self) -> Option<u64> {
        self.gen_range_twin_from_daa
    }

    /// **The task-head fence's mirror** — written by `Params::sync_palw_task_heads_v1` and by nothing else (and by fixtures).
    pub fn with_task_heads_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.task_heads_from_daa = from_daa;
        self
    }

    /// `Params::palw_task_heads_v1`'s height, if the network arms it (the mirror).
    pub fn task_heads_from_daa(&self) -> Option<u64> {
        self.task_heads_from_daa
    }

    /// **Is the `Head` profile in force at `daa_score`?** `false` on every shipped preset.
    pub fn task_heads_active_at(&self, daa_score: u64) -> bool {
        self.task_heads_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// The `Head` profile's in-flight cap's mirror (written by `Params::sync_palw_task_heads_v1`).
    pub fn with_task_heads_max_inflight_claims(mut self, cap: u32) -> Self {
        self.task_heads_max_inflight_claims = cap;
        self
    }

    /// The `Head` profile's cap on claims one class holds in flight (0 where the fence is not armed).
    pub fn task_heads_max_inflight_claims(&self) -> u32 {
        self.task_heads_max_inflight_claims
    }

    /// **Does the generative range twin size closes at `daa_score`?** `false` on every shipped preset.
    pub fn gen_range_twin_active_at(&self, daa_score: u64) -> bool {
        self.gen_range_twin_from_daa.is_some_and(|from| daa_score >= from)
    }

    pub fn with_held_close_chunks_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.held_close_chunks_from_daa = from_daa;
        self
    }

    /// **RFC-0007 Part I: the verification vertex fence's mirror** — written by
    /// `Params::sync_palw_verification_vertex_v1` and by nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_vertex_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.vertex_from_daa = from_daa;
        self
    }

    /// `Params::palw_verification_vertex_v1`'s height, if the network arms it (the mirror).
    pub fn vertex_from_daa(&self) -> Option<u64> {
        self.vertex_from_daa
    }

    /// **Is the verification vertex in force at `daa_score`?** `false` on every shipped preset.
    pub fn vertex_active_at(&self, daa_score: u64) -> bool {
        self.vertex_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **RFC-0008 v2: the EXEC payload fence's mirror** — written by `Params::sync_palw_exec_payload_v2` and by nothing else (and by
    /// fixtures); `None` where the fence is not armed.
    pub fn with_exec_v2_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.exec_v2_from_daa = from_daa;
        self
    }

    /// `Params::palw_exec_payload_v2`'s height, if the network arms it (the mirror).
    pub fn exec_v2_from_daa(&self) -> Option<u64> {
        self.exec_v2_from_daa
    }

    /// **Is the unified EXEC payload (`PXE2`, the work-slice ledgers) in force at `daa_score`?** `false` on every shipped preset.
    pub fn exec_v2_active_at(&self, daa_score: u64) -> bool {
        self.exec_v2_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// RFC-0007: the `witness_manifest` fence's mirror — written by its `Params::sync_*` and nothing else (and fixtures).
    pub fn with_witness_manifest_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.witness_manifest_from_daa = from_daa;
        self
    }

    /// RFC-0007: the `witness_manifest` fence's height, if the network arms it (the mirror).
    pub fn witness_manifest_from_daa(&self) -> Option<u64> {
        self.witness_manifest_from_daa
    }

    /// RFC-0007: is the `witness_manifest` fence in force at `daa_score`? `false` on every shipped preset.
    pub fn witness_manifest_active_at(&self, daa_score: u64) -> bool {
        self.witness_manifest_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// RFC-0007: the `audit_mesh` fence's mirror — written by its `Params::sync_*` and nothing else (and fixtures).
    pub fn with_audit_mesh_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.audit_mesh_from_daa = from_daa;
        self
    }

    /// RFC-0007: the `audit_mesh` fence's height, if the network arms it (the mirror).
    pub fn audit_mesh_from_daa(&self) -> Option<u64> {
        self.audit_mesh_from_daa
    }

    /// RFC-0007: is the `audit_mesh` fence in force at `daa_score`? `false` on every shipped preset.
    pub fn audit_mesh_active_at(&self, daa_score: u64) -> bool {
        self.audit_mesh_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// RFC-0007: the `capped` fence's mirror — written by its `Params::sync_*` and nothing else (and fixtures).
    pub fn with_capped_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.capped_from_daa = from_daa;
        self
    }

    /// RFC-0007: the `capped` fence's height, if the network arms it (the mirror).
    pub fn capped_from_daa(&self) -> Option<u64> {
        self.capped_from_daa
    }

    /// RFC-0007: is the `capped` fence in force at `daa_score`? `false` on every shipped preset.
    pub fn capped_active_at(&self, daa_score: u64) -> bool {
        self.capped_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// `Params::palw_held_close_chunks_v1`'s height, if the network arms it (the mirror).
    pub fn held_close_chunks_from_daa(&self) -> Option<u64> {
        self.held_close_chunks_from_daa
    }

    /// **Is the held leaf challenge in force at `daa_score`?** `false` on every shipped preset.
    pub fn held_close_chunks_active_at(&self, daa_score: u64) -> bool {
        self.held_close_chunks_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **ADR-0096 Decision 8: the decode constraint's mirror** — written by `Params::sync_palw_fp_decode_constraint_v1` and by
    /// nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_fp_decode_constraint_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.fp_decode_constraint_from_daa = from_daa;
        self
    }

    pub fn fp_decode_constraint_from_daa(&self) -> Option<u64> {
        self.fp_decode_constraint_from_daa
    }

    /// **Is the decode constraint in force at `daa_score`?** `false` on every shipped preset.
    pub fn fp_decode_constraint_active_at(&self, daa_score: u64) -> bool {
        self.fp_decode_constraint_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **RFC-0001 §2.10: the adapter class listing's mirror** — written by `Params::sync_palw_adapter_class_v1` and
    /// by nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_adapter_class_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.adapter_class_from_daa = from_daa;
        self
    }

    /// `Params::palw_adapter_class_v1`'s height, if the network arms it (the mirror).
    pub fn adapter_class_from_daa(&self) -> Option<u64> {
        self.adapter_class_from_daa
    }

    /// **Is the adapter class listing in force at `daa_score`?** `false` on every shipped preset.
    pub fn adapter_class_active_at(&self, daa_score: u64) -> bool {
        self.adapter_class_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **RFC-0002 Part II Proposal A: the class-seating fence's mirror** — the height, the floor and the raise, written by
    /// `Params::sync_palw_class_seating` and by nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_class_seating(mut self, seating: Option<crate::palw_class_seating_fence_v1::PalwClassSeatingMirrorV1>) -> Self {
        self.class_seating = seating;
        self
    }

    /// **RFC-0002 Phase H: the IR-only fence's mirror** — written by `Params::sync_palw_tir_only_v1` and by nothing else (and by fixtures).
    pub fn with_tir_only_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.tir_only_from_daa = from_daa;
        self
    }

    /// `Params::palw_tir_only_v1`'s height, if the network arms it (the mirror).
    pub fn tir_only_from_daa(&self) -> Option<u64> {
        self.tir_only_from_daa
    }

    /// **Is a new class admitted only as an IR program (or pipeline) at `daa_score`?** `false` on every shipped preset.
    pub fn tir_only_active_at(&self, daa_score: u64) -> bool {
        self.tir_only_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// `Params::palw_class_seating`'s height, floor and raise, if the network arms it (the mirror).
    pub fn class_seating(&self) -> Option<crate::palw_class_seating_fence_v1::PalwClassSeatingMirrorV1> {
        self.class_seating
    }

    /// **The seating rule's terms in force at `daa_score`** — `None` below the fence, on every shipped preset.
    pub fn class_seating_terms_at(&self, daa_score: u64) -> Option<crate::palw_class_seating_fence_v1::PalwClassSeatingTermsV1> {
        self.class_seating.and_then(|mirror| mirror.terms_at(daa_score))
    }

    /// **Lane F2: is the floor-refusal retry in force at `daa_score`?** `false` on every shipped preset.
    /// Past it a claim its anchor block did not bind, whose stake draw refused for eligibility at every
    /// seed on the anchor block's pre-object base, is re-based on the anchor block and retried at its
    /// next anchor slot until the bind window's backstop ([`palw_floor_retry_is_due_v1`]). Asked at the
    /// ANCHOR BLOCK's DAA, and — for a retried claim — at its own `rebound_daa`
    /// ([`palw_claim_awaits_ncp_retry_v1`]).
    pub fn floor_refusal_retry_active_at(&self, daa_score: u64) -> bool {
        self.floor_refusal_retry_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **Lane F1: are the registry-resilience rules in force at `daa_score`?** `false` on every shipped
    /// preset. Past the fence:
    /// * **V03(1)** — a claim its anchor block did not bind while its class could not seat a panel on
    ///   the state the draw read is not voided there: it is re-based on the anchor block and retried
    ///   at its next anchor slot, and only the bind window's backstop, `accepted_daa + window_bind`,
    ///   voids it ([`palw_ncp_retry_is_due_v1`]). Asked at the ANCHOR BLOCK's DAA.
    /// * **V03(2)** — a class `Held` out of `Probation` for readiness returns to the probation progress
    ///   it had ([`crate::palw_model_registry_v1::palw_lifecycle_step_resilient_v1`]);
    /// * **V05** — a probation resets only on failed probes of at least two distinct producer bonds
    ///   ([`crate::palw_model_registry_v1::PALW_PROBATION_RESET_DISTINCT_BONDS_V1`]). V03(2) and V05
    ///   are asked per SPAN ([`Self::registry_resilience_governs_span_v1`]).
    pub fn registry_resilience_active_at(&self, daa_score: u64) -> bool {
        self.registry_resilience_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **Lane F1: does the fence govern execution span `span`** — is it in force at the span's FIRST
    /// DAA (`span × span_daa`)? The one key V03(2) and V05 share: a failed probe names its bond iff
    /// the span it is counted in is governed, and the span step judges a span's probes by the
    /// resilient rules iff that span is governed — so a span that straddles the height is judged,
    /// whole, by the rules it began under, and no span is judged by one rule with probes noted under
    /// the other. (On testnet-12 a span is one DAA, so the span's first DAA is the DAA itself.)
    pub fn registry_resilience_governs_span_v1(&self, span: u64, span_daa: u64) -> bool {
        self.registry_resilience_active_at(span.saturating_mul(span_daa.max(1)))
    }

    /// **How long a readiness-V2 row stands on this network, in spans** — the mirror, or the default
    /// [`crate::palw_model_registry_v1::PALW_READINESS_V2_MAX_AGE_SPANS_V1`] (eight) where the fence is
    /// not armed. The fence is genesis-only, so "armed" is "in force at every DAA". What the registry
    /// fold's globals carry ([`crate::palw_model_registry_v1::palw_registry_globals_of_bundle_v1`]).
    pub fn readiness_v2_max_age_spans_v1(&self) -> u32 {
        self.readiness_v2_max_age_spans.unwrap_or(crate::palw_model_registry_v1::PALW_READINESS_V2_MAX_AGE_SPANS_V1)
    }

    /// ADR-0152 §4-quater: the fence's height, if the network arms it (the mirror).
    pub fn class_verify_deadline_from_daa(&self) -> Option<u64> {
        self.class_verify_deadline_from_daa
    }

    /// ADR-0152 §4-quater: whether class-derived verification deadlines are in force at `daa_score`.
    /// `false` on every network but testnet-12, where it is armed at genesis.
    pub fn class_verify_deadline_active_at(&self, daa_score: u64) -> bool {
        self.class_verify_deadline_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// ADR-0152 §4-quater: the measured rows (the mirror; empty on every network at launch).
    pub fn class_verify_rows(&self) -> &[crate::palw_class_verify_deadline_v1::PalwClassVerifyRowV1] {
        &self.class_verify_rows
    }

    /// The measured row that prices `class_id` at `daa_score`, if one is active there.
    pub fn class_verify_row_at(
        &self,
        class_id: &Hash64,
        daa_score: u64,
    ) -> Option<&crate::palw_class_verify_deadline_v1::PalwClassVerifyRowV1> {
        self.class_verify_rows.iter().find(|row| row.class_id == *class_id && row.is_active_at(daa_score))
    }

    /// Disarm ADR-0133 §11.3's rule — every class back on the network's `window_receipt`.
    pub fn without_class_receipt_window(mut self) -> Self {
        self.class_receipt_window = None;
        self
    }

    /// ADR-0133 §11.3's fence height, if the network arms it.
    pub fn class_receipt_window_daa(&self) -> Option<u64> {
        self.class_receipt_window.map(|rule| rule.activation_daa)
    }

    /// Set the rung window (P0-9). Refuses zero for the reason `PalwCourtParamsV2::new` does, and
    /// refuses a window longer than the session it must fit inside — a rung that outlives the
    /// backstop is a rung whose deadline can never be reached.
    pub fn with_turn_deadline_daa(mut self, turn_deadline_daa: u64) -> Result<Self, PalwStateV2Error> {
        if turn_deadline_daa == 0 {
            return Err(PalwStateV2Error::InvalidParams("a zero rung window defaults whichever party the block order reaches first"));
        }
        if turn_deadline_daa > self.window_court {
            return Err(PalwStateV2Error::InvalidParams("a rung window longer than the court window can never come due"));
        }
        self.turn_deadline_daa = turn_deadline_daa;
        Ok(self)
    }

    pub fn turn_deadline_daa(&self) -> u64 {
        self.turn_deadline_daa
    }

    /// Set the retirement span. Must sit strictly above the abandon hold when both are on: the two
    /// are the only deadlines a terminal claim can own, and ordering them is what makes the index
    /// recomputable from the record — the hold entry first, the retirement entry after it.
    /// ADR-0074 Decision 5: price the free-prompt lane. Both numbers must be non-zero — a zero
    /// quantum divides nothing, a zero cap certifies receipts that can never draw.
    pub fn with_fp_quanta(mut self, quanta_per_canonical_job: u32, max_quanta_per_receipt: u32) -> Result<Self, PalwStateV2Error> {
        if quanta_per_canonical_job == 0 || max_quanta_per_receipt == 0 {
            return Err(PalwStateV2Error::InvalidParams("the free-prompt price needs a non-zero quanta-per-job and a non-zero cap"));
        }
        self.fp_quanta_per_canonical_job = quanta_per_canonical_job;
        self.fp_max_quanta_per_receipt = max_quanta_per_receipt;
        Ok(self)
    }

    pub fn fp_quanta_per_canonical_job(&self) -> u32 {
        self.fp_quanta_per_canonical_job
    }

    pub fn fp_max_quanta_per_receipt(&self) -> u32 {
        self.fp_max_quanta_per_receipt
    }

    /// ADR-0074 Decision 6: gate the free-prompt lane on certification. The set may be empty —
    /// that is "no class's free-prompt lane bears weight", not "ungated".
    pub fn with_fp_certified_classes(mut self, certified: BTreeSet<Hash64>) -> Self {
        self.fp_certified_classes = Some(certified);
        self
    }

    pub fn fp_certified_classes(&self) -> Option<&BTreeSet<Hash64>> {
        self.fp_certified_classes.as_ref()
    }

    /// The free-prompt lane's exposure ceiling (admission item 8's ratio, applied at the
    /// transition). `1..=1000` permille of the bond's collateral.
    pub fn with_fp_exposure_ceiling(mut self, ratio_permille: u32) -> Result<Self, PalwStateV2Error> {
        if ratio_permille == 0 || ratio_permille > 1000 {
            return Err(PalwStateV2Error::InvalidParams("the free-prompt exposure ceiling must be 1..=1000 permille"));
        }
        self.fp_max_exposure_ratio_permille = ratio_permille;
        Ok(self)
    }

    pub fn fp_max_exposure_ratio_permille(&self) -> u32 {
        self.fp_max_exposure_ratio_permille
    }

    pub fn with_claim_retirement_daa(mut self, claim_retirement_daa: u64) -> Result<Self, PalwStateV2Error> {
        if claim_retirement_daa > 0 && self.fp_abandon_hold_daa > 0 && claim_retirement_daa <= self.fp_abandon_hold_daa {
            return Err(PalwStateV2Error::InvalidParams("the claim retirement span must outlast the abandon hold"));
        }
        self.claim_retirement_daa = claim_retirement_daa;
        Ok(self)
    }

    pub fn claim_retirement_daa(&self) -> u64 {
        self.claim_retirement_daa
    }

    /// **ADR-0056 Decisions 3 and 5, as one call**: a registry price and a reclamation clock.
    ///
    /// The share WALK is not here. Two branches wrote one, and the one that shipped is
    /// [`Self::with_class_share_growth_v1`]: it measures a class's FILLED BUDGET against the same
    /// closed epoch the retarget reads, where this line's version counted consecutive epochs above
    /// and below two fill thresholds. Filled budget is the better signal for a reason the other
    /// line measured rather than argued — a minimum-share class has `expected == observed == 1`
    /// in the only two states its budget allows, so a streak rule keyed to those states has no
    /// reachable input on exactly the class that most needs to grow.
    ///
    /// A consuming builder rather than two more `new` arguments for the reason every other late
    /// field here is one: a positional list of same-typed integers is a place to transpose two of
    /// them silently.
    pub fn with_class_economy_v1(mut self, registration_exposure_sompi: u64, reclaim_epochs: u32) -> Result<Self, PalwStateV2Error> {
        // The floor's reserve is NOT an argument here: it is `min_base_class_share_permille`, set
        // by its own builder because the registration path needs it whether or not the walk is on.
        // A second copy taken here would be a second bound to drift.
        self.registration_exposure_sompi = registration_exposure_sompi;
        self.reclaim_epochs = reclaim_epochs;
        Ok(self)
    }

    pub fn registration_exposure_sompi(&self) -> u64 {
        self.registration_exposure_sompi
    }
    pub fn reclaim_epochs(&self) -> u32 {
        self.reclaim_epochs
    }

    /// Set the producer carve (ADR-0042 Decision 10). Consuming-builder rather than a `new`
    /// argument so that adding reward to this machine could not silently change what an existing
    /// caller's params mean — a thirteenth positional `u16` next to `fp_attempt_share_permille`
    /// and `beta_permille` is exactly the argument a reader mis-binds.
    pub fn with_worker_carve_permille(mut self, worker_carve_permille: u16) -> Result<Self, PalwStateV2Error> {
        if worker_carve_permille > 1000 {
            return Err(PalwStateV2Error::InvalidParams("the worker carve cannot exceed the whole subsidy"));
        }
        self.worker_carve_permille = worker_carve_permille;
        Ok(self)
    }

    pub fn worker_carve_permille(&self) -> u16 {
        self.worker_carve_permille
    }

    /// **The worker carve of one subsidy — what a claim escrows, and what the coinbase withholds.**
    /// `⌊subsidy · worker_carve_permille / 1000⌋`, the exact number [`apply_attempt`] snapshots into
    /// `escrowed_reward`. The processor calls this to size the merged-escrow withhold (B-1) from each
    /// merged block's own subsidy, so the carve the fold records and the carve the coinbase withholds
    /// are one value computed by one function — the matched pair the coinbase-hash rule requires.
    pub fn worker_carve(&self, subsidy: u64) -> u64 {
        worker_carve_v2(self, subsidy, None)
    }

    /// **The worker carve of one subsidy at the carve its attempt block resolved** (ADR-0126
    /// Decision 3) — [`Self::worker_carve`] where `escrow_carve` is `None`, the fence's carve where it
    /// is `Some`. The one function both sides of the matched pair call: the fold escrows with it and
    /// the processor sizes the merged withhold with it, from the same record.
    pub fn worker_carve_at(&self, subsidy: u64, escrow_carve: Option<crate::palw_reward_v2::PalwRewardParamsV2>) -> u64 {
        worker_carve_v2(self, subsidy, escrow_carve)
    }

    /// **Require a claim's collateral to be a fraction of the reward it escrows** (2026-08-30).
    ///
    /// The downside of producing is `pwu_per_inference x slash_value_per_pwu`; the upside is the block's worker
    /// carve. Nothing tied them together, and on the live network they sat four orders of
    /// magnitude apart — so abandoning a claim, which costs the panel five seats' verification and
    /// destroys the escrow, cost its producer 0.00005 % of what the claim was worth. This is the
    /// inequality that makes the two one economy.
    ///
    /// It binds the CLASS's registered slash value, not the producer: every class must already
    /// carry the floor's `slash_value_per_pwu` (`SlashValueNotTheNetworks`), so this is a single
    /// chain-wide statement about what that number has to be worth. Attempts that fail it are
    /// refused by admission — the block stays valid and carries no claim, so a slash value chosen
    /// too low costs its own producer rather than stopping the chain.
    ///
    /// 1000‰ would mean a claim risks exactly what it can earn. The derivation that produced the
    /// 100‰ this network mints with is in `docs/palw-bond-economics-2026-08-30.md`.
    pub fn with_min_slash_permille_of_escrow(mut self, permille: u16) -> Result<Self, PalwStateV2Error> {
        if permille > 1000 {
            return Err(PalwStateV2Error::InvalidParams("a claim cannot be required to risk more than it can earn"));
        }
        self.min_slash_permille_of_escrow = permille;
        Ok(self)
    }

    pub fn min_slash_permille_of_escrow(&self) -> u16 {
        self.min_slash_permille_of_escrow
    }

    /// **Turn on the share-raise path** (ADR-0054): the per-epoch growth step, in permille of a
    /// class's own share, and the permille the liveness floor keeps.
    ///
    /// A consuming builder for the same reason the carve is one: adding a fourteenth positional
    /// `u16` beside `fp_attempt_share_permille` and `budget_tolerance_permille` is the argument a
    /// reader mis-binds, and a share rule bound to the wrong slot moves the cadence table.
    ///
    /// Refused: a growth step above the whole denominator (a class that could more than double
    /// every epoch is not following its production, it is racing it), and a reserve above the
    /// denominator. A reserve BELOW the grant floor is not refused — it is raised to it, because
    /// the floor is a bound the table already enforces everywhere else.
    /// **The reserve is not an argument.** ADR-0054 Decision 2 asks for a quantity the floor keeps,
    /// and the table already has one: `min_base_class_share_permille`, which the audit added after
    /// showing 999 individually-legal grants could walk the floor to 1‰. A second stored reserve
    /// would be a second place for the same bound to be set, and the two would differ the first
    /// time someone set one of them. So this takes the growth step alone and reads the bound from
    /// the field that owns it — see `base_class_reserve_permille`.
    pub fn with_class_share_growth_v1(mut self, class_growth_permille: u16) -> Result<Self, PalwStateV2Error> {
        if class_growth_permille > 1000 {
            return Err(PalwStateV2Error::InvalidParams("a growth step above 1000‰ would more than double a share in one epoch"));
        }
        if class_growth_permille > 0 && self.min_base_class_share_permille == 0 {
            return Err(PalwStateV2Error::InvalidParams(
                "a growth rule with no floor reserve can walk the liveness floor to the grant floor (ADR-0039 W6′)",
            ));
        }
        self.class_growth_permille = class_growth_permille;
        Ok(self)
    }

    /// ADR-0054's growth step, in permille of a class's own share per closed epoch. `0` = the rule
    /// is off and no share moves outside an activation grant.
    /// **ADR-0082 Decisions 10/11: arm the lane's decode rules at this DAA score.**
    /// `Params::palw_fp_decode_rules_fence().map(|f| f.daa_score())` is the one value a caller
    /// should pass; no preset passes anything.
    ///
    /// Writes the borsh-skipped mirror (`Params::sync_palw_fp_decode_rules` is the one caller a
    /// node has), never the retired serialized field.
    pub fn with_fp_decode_rules_v1(mut self, activation_daa: Option<u64>) -> Self {
        self.fp_decode_rules_from_daa = activation_daa;
        self
    }

    /// The mirrored height, as `validate_palw_v2` compares it with the fence.
    pub fn fp_decode_rules_from_daa(&self) -> Option<u64> {
        self.fp_decode_rules_from_daa
    }

    /// The retired serialized height — `None` on every bundle `validate_palw_v2` admits.
    pub fn fp_decode_rules_serialized_daa(&self) -> Option<u64> {
        self.fp_decode_rules_daa
    }

    /// Are ADR-0082's decode rules in force at `daa_score`? `false` on every shipped preset, and
    /// on every caller that has not armed it.
    pub fn fp_decode_rules_at(&self, daa_score: u64) -> bool {
        self.fp_decode_rules_from_daa.is_some_and(|at| daa_score >= at)
    }

    pub fn class_growth_permille(&self) -> u16 {
        self.class_growth_permille
    }

    /// **The permille the liveness floor keeps** — the reserve a growing class may not draw past
    /// (ADR-0054 Decision 2, ADR-0039 W6′).
    ///
    /// Derived, not stored. The floor is the reservoir every growing class draws from and the one
    /// class every node can run without an artifact; W6′ said its share "may never be zero", and a
    /// rule that moves permille every epoch makes that bound worthless, because a table walked down
    /// to one permille has ended liveness while satisfying the letter of it.
    ///
    /// The quantity is `min_base_class_share_permille`, the field the audit added when it showed
    /// the same walk could be performed by 999 individually-legal GRANTS. Growth and granting are
    /// two ways to move a permille and there is one bound between them, raised to the grant floor
    /// because a reserve under the smallest grantable share is not a reserve.
    pub fn base_class_reserve_permille(&self) -> u16 {
        self.min_base_class_share_permille.max(self.min_grantable_share_permille())
    }

    pub fn min_collateral_sompi(&self) -> u64 {
        self.min_collateral_sompi
    }

    pub fn beta_permille(&self) -> u16 {
        self.beta_permille
    }

    /// The epoch arithmetic admission's budget predicate must share with the state's counters —
    /// two epoch definitions would let a claim be inside one and outside the other.
    pub fn epoch_length(&self) -> u64 {
        self.epoch_length
    }

    /// The bind window, exposed for the acceptance layers that must agree with the sweep about
    /// when a claim's panel may still legally bind.
    pub fn window_bind(&self) -> u64 {
        self.window_bind
    }

    /// The receipt window, exposed for the startup gate's liability-period arithmetic.
    pub fn window_receipt(&self) -> u64 {
        self.window_receipt
    }

    /// The challenge window, exposed so court acceptance can agree with the Final sweep about
    /// when a claim is still challengeable.
    pub fn window_challenge(&self) -> u64 {
        self.window_challenge
    }

    /// ADR-0132 §7.6: the short challenge window's fence height, if the network schedules one.
    pub fn short_challenge_window_from_daa(&self) -> Option<u64> {
        self.short_challenge_window_from_daa
    }

    /// ADR-0132 §7.6: sets the short challenge window's fence height — the V2 bundle's copy of
    /// `Params::palw_short_challenge_window`, written by `Params::set_palw_short_challenge_window`.
    pub fn with_short_challenge_window_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.short_challenge_window_from_daa = from_daa;
        self
    }

    /// Option A: the height from which a claim's escrow is reserved on its bond, if the network arms it.
    pub fn escrow_backed_exposure_from_daa(&self) -> Option<u64> {
        self.escrow_backed_exposure_from_daa
    }

    /// Option A: sets that height — the V2 bundle's copy of `Params::palw_audit_2026_09_23`, written by
    /// `Params::sync_palw_escrow_backed_exposure`.
    pub fn with_escrow_backed_exposure_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.escrow_backed_exposure_from_daa = from_daa;
        self
    }

    /// ADR-0160 F-W: `Params::palw_capacity_weight_cap`'s height, if the network arms it (the mirror).
    pub fn capacity_weight_cap_from_daa(&self) -> Option<u64> {
        self.capacity_weight_cap_from_daa
    }

    /// ADR-0160 F-W: sets that height — the V2 bundle's copy of `Params::palw_capacity_weight_cap`,
    /// written by `Params::sync_palw_capacity_weight_cap`.
    pub fn with_capacity_weight_cap_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.capacity_weight_cap_from_daa = from_daa;
        self
    }

    /// ADR-0160 F-W: is a claim ACCEPTED at `accepted_daa` accounted under the weight cap? `false` on
    /// every network but a testnet-12 that armed the fence, and for every claim accepted below it.
    pub fn capacity_weight_cap_applies_at(&self, accepted_daa: u64) -> bool {
        self.capacity_weight_cap_from_daa.is_some_and(|from| accepted_daa >= from)
    }

    /// ADR-0152 R-core+: `Params::palw_rcore_plus`'s height, if the network arms it (the mirror).
    pub fn rcore_plus_from_daa(&self) -> Option<u64> {
        self.rcore_plus_from_daa
    }

    /// ADR-0152 R-core+: whether the fence is in force at `daa_score`. `false` on every network but
    /// testnet-12. Declared by the v22 skeleton for S's writers; nothing reads it yet.
    pub fn rcore_plus_active_at(&self, daa_score: u64) -> bool {
        self.rcore_plus_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **ADR-0160 lane liab (F-L): the mirror of `Params::palw_capacity_aggregate_liability`**, written
    /// by `Params::sync_palw_capacity_liability` and by nothing else.
    pub fn with_capacity_liability(mut self, value: Option<crate::palw_aggregate_liability_v1::PalwCapacityLiabilityV1>) -> Self {
        self.capacity_liability = value;
        self
    }

    /// ADR-0160 lane liab (F-L): the fence's value (the mirror); `None` where it is not armed.
    pub fn capacity_liability(&self) -> Option<&crate::palw_aggregate_liability_v1::PalwCapacityLiabilityV1> {
        self.capacity_liability.as_ref()
    }

    /// **ADR-0160 (F-L): the ramp's step in force at `daa`** — the seat side reads it at the claim's
    /// `accepted_daa` (AS-1, AS-2), the escrow lane its `m_c` there. `None` where the fence is not
    /// armed or not yet active at `daa` (every shipped preset: today's prices).
    pub fn capacity_step_at(&self, daa: u64) -> Option<crate::palw_aggregate_liability_v1::PalwCapacityStepV1> {
        self.capacity_liability.as_ref().and_then(|value| crate::palw_aggregate_liability_v1::palw_capacity_step_at_v1(value, daa))
    }

    /// ADR-0160 (F-L): whether the aggregate liability is in force at `daa` (a conviction's DAA for
    /// AG-2/AG-3). `false` on every shipped preset.
    pub fn capacity_liability_active_at(&self, daa: u64) -> bool {
        self.capacity_step_at(daa).is_some()
    }

    /// ADR-0152 R-core+: the withdrawal delay the processor enforces, DA lattice included (the
    /// mirror; 0 where the fence is not armed).
    pub fn withdrawal_delay_daa(&self) -> u64 {
        self.withdrawal_delay_daa
    }

    /// **ADR-0160 lane escrow (F-E): the mirror's setter**, written by `Params::sync_palw_capacity_escrow`
    /// and by nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_capacity_escrow_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.capacity_escrow_from_daa = from_daa;
        self
    }

    /// ADR-0160 lane escrow: `Params::palw_capacity_escrow_at_licence`'s height, if armed (the mirror).
    pub fn capacity_escrow_from_daa(&self) -> Option<u64> {
        self.capacity_escrow_from_daa
    }

    /// **ADR-0160 lane escrow: is a claim accepted at `accepted_daa` under F-E?** Keyed on the CLAIM's
    /// acceptance (the ADR-0145 precedent), never on the current block's: a claim accepted below the
    /// height keeps `w + E` for its whole life, and a re-derivation at any height returns the same
    /// number. `false` on every shipped preset.
    pub fn capacity_escrow_active_at(&self, accepted_daa: u64) -> bool {
        self.capacity_escrow_from_daa.is_some_and(|from| accepted_daa >= from)
    }

    /// **ADR-0160 §7.2's one cross-lane line: the credit step in force at a claim's `accepted_daa`** —
    /// lane liab's F-L step (`Self::capacity_step_at`, the fence's mirror), rewired on rcore/cap-s1: the
    /// escrow lane's stand-in schedule is gone, so the escrow slot and the seat side (AS-1/AS-2) read ONE
    /// schedule. `None` where F-L is not armed or not yet active (every shipped preset): no credit.
    pub fn capacity_escrow_credit_at_v1(&self, accepted_daa: u64) -> Option<crate::palw_escrow_funding_v2::PalwEscrowCreditV1> {
        self.capacity_step_at(accepted_daa)
            .map(|step| crate::palw_escrow_funding_v2::PalwEscrowCreditV1 { rho: step.rho, q_credit_permille: step.q_credit_permille })
    }

    /// **The conviction floor a credit is priced against, for a claim accepted at `accepted_daa` with
    /// escrow `e_sompi`** (finding 2 of this lane's verification; ADR-0160 §4.5's `L`).
    ///
    /// On `rcore/cap-escrow` alone there is no aggregate funnel (F-L), so no conviction burns any other
    /// reward of the bond, and the cheapest producer route charges the claim's forfeit alone: S1
    /// (`ProducerWithholding`, the DA default a root with no material behind it ends in) adds its action
    /// tier only on X11's third strike, and strikes are counted one per aligned 1,000-DAA epoch, so a
    /// campaign inside one epoch never escalates. The floor is therefore `Tier(0)`: `L = m`, and the
    /// credit needs `q ≥ 819‰` for `ρ = 10` ([`crate::palw_escrow_funding_v2::palw_escrow_m_star_v2`]).
    ///
    /// **rcore/cap-s1 — the user's decision 2 (2026-09-26 17:50): the credit is priced on the loss a
    /// conviction ACTUALLY collects, per offence class.** The floor is the SMALLEST extra collection
    /// (beyond the claim's own stage commitment `w + m_c`) over every producer route that can convict a
    /// fraud of an attempt claim — the fraudster picks its route (a claim with no material behind its
    /// roots ends in a DA default whatever else it is) — read off lane liab's route table
    /// (`palw_aggregate_liability_v1::palw_credit_conviction_floor_v1`): a DA default's S1 adds its tier
    /// only on X11's third strike (0 on the first; decision 1 keeps S1 in the tier class), kinds 5 / 12
    /// (and 6 / 7 / 8, a held verdict) add S2's tier `min(10%·C_min, 3E)` — kind 12's `w + m_c + 1,300`
    /// ≈ 1,620 MSK at a 13k bond and ρ 10 —, the intent class the whole bond. Keyed on nothing but the
    /// params and `e_sompi` (the record's), so the load re-derivation returns the same slot.
    pub fn capacity_escrow_conviction_floor_at_v1(
        &self,
        accepted_daa: u64,
        e_sompi: u128,
    ) -> crate::palw_escrow_funding_v2::PalwEscrowConvictionFloorV1 {
        let _ = accepted_daa;
        crate::palw_escrow_funding_v2::PalwEscrowConvictionFloorV1::Tier(
            crate::palw_aggregate_liability_v1::palw_credit_conviction_floor_v1(self.min_collateral_sompi(), e_sompi),
        )
    }

    /// ADR-0152 R-core+: C7's list (the mirror; empty where the fence is not armed).
    pub fn rcore_conservative_classes(&self) -> &[Hash64] {
        &self.rcore_conservative_classes
    }

    /// ADR-0152 R-core+: sets the three mirrors at once — the V2 bundle's copy of
    /// `Params::palw_rcore_plus`, of the withdrawal delay the processor enforces, and of
    /// `Params::palw_rcore_conservative_classes`, written by `Params::sync_palw_rcore_plus` and by
    /// nothing else, so the three cannot drift apart.
    pub fn with_rcore_plus_mirrors(
        mut self,
        from_daa: Option<u64>,
        withdrawal_delay_daa: u64,
        conservative_classes: Vec<Hash64>,
    ) -> Self {
        self.rcore_plus_from_daa = from_daa;
        self.withdrawal_delay_daa = withdrawal_delay_daa;
        self.rcore_conservative_classes = conservative_classes;
        self
    }

    // ---- lane V02 (post-launch, 2026-09-26): a resolved claim's lock off the work ceiling ----

    /// **Lane V02: the mirror's setter** — written by `Params::sync_palw_final_lock_full_collateral`
    /// and by nothing else (and by fixtures); `None` where the fence is not armed.
    pub fn with_final_lock_full_collateral_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.final_lock_full_collateral_from_daa = from_daa;
        self
    }

    /// Lane V02: `Params::palw_final_lock_full_collateral`'s height, if the network arms it.
    pub fn final_lock_full_collateral_from_daa(&self) -> Option<u64> {
        self.final_lock_full_collateral_from_daa
    }

    /// **Lane V02: does a gate evaluated at `daa_score` count a resolved claim's lock against the whole
    /// collateral instead of the work ceiling?** `false` on every shipped preset. Asked at the gate's
    /// own DAA ([`palw_bond_off_ceiling_v1`]).
    pub fn final_lock_full_collateral_active_at(&self, daa_score: u64) -> bool {
        self.final_lock_full_collateral_from_daa.is_some_and(|from| daa_score >= from)
    }
    // ---- end lane V02 ----
    /// **ADR-0160 lane verify: F-R's and F-B's mirrors**, written by `Params::sync_palw_capacity_verify`
    /// and by nothing else (and by fixtures); `None` where a fence is not armed.
    pub fn with_capacity_verify_mirrors(mut self, room_from_daa: Option<u64>, batch_from_daa: Option<u64>) -> Self {
        self.capacity_room_from_daa = room_from_daa;
        self.capacity_batch_from_daa = batch_from_daa;
        self
    }

    /// ADR-0160 F-R: `Params::palw_capacity_verify_room`'s height, if armed (the mirror).
    pub fn capacity_room_from_daa(&self) -> Option<u64> {
        self.capacity_room_from_daa
    }

    /// ADR-0160 F-B: `Params::palw_capacity_batch_licence`'s height, if armed (the mirror).
    pub fn capacity_batch_from_daa(&self) -> Option<u64> {
        self.capacity_batch_from_daa
    }

    /// **ADR-0160 F-R: does room v2 judge a block at `daa_score`?** — the measured `k = 2` capacity,
    /// the stake-proportional share in place of T-2(a), and the floor room
    /// ([`crate::palw_verify_capacity_v1`]). `false` on every shipped preset.
    pub fn capacity_room_active_at(&self, daa_score: u64) -> bool {
        self.capacity_room_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **ADR-0160 F-B: may a block at `daa_score` carry a batch licence** (and a kind-3 filing a
    /// `Windowed` receipt, [`crate::palw_batch_licence_v1`])? `false` on every shipped preset.
    pub fn capacity_batch_active_at(&self, daa_score: u64) -> bool {
        self.capacity_batch_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **ADR-0160 stage 2: F-Q's and F-S's mirrors**, written by `Params::sync_palw_capacity_stage2` and
    /// by nothing else (and by fixtures); `None` / empty where a fence is not armed.
    pub fn with_capacity_stage2_mirrors(
        mut self,
        audit_from_daa: Option<u64>,
        audit_operators: Vec<PalwBondKeyV2>,
        slots_from_daa: Option<u64>,
    ) -> Self {
        self.capacity_audit_from_daa = audit_from_daa;
        self.capacity_audit_operators = audit_operators;
        self.capacity_slots_from_daa = slots_from_daa;
        self
    }

    /// ADR-0160 F-Q: `Params::palw_capacity_audit_door`'s height, if armed (the mirror).
    pub fn capacity_audit_from_daa(&self) -> Option<u64> {
        self.capacity_audit_from_daa
    }

    /// ADR-0160 F-Q: the operator bonds an audit pool draws from (the mirror; empty below the fence).
    pub fn capacity_audit_operators(&self) -> &[PalwBondKeyV2] {
        &self.capacity_audit_operators
    }

    /// **ADR-0160 F-Q: is the audit door in force at `daa_score`?** — a block may carry audit receipts,
    /// and a claim ACCEPTED at or past it is credited under the door ([`crate::palw_audit_door_v1`]).
    /// `false` on every shipped preset.
    pub fn capacity_audit_active_at(&self, daa_score: u64) -> bool {
        self.capacity_audit_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **ADR-0160 F-N (stage 4): the mirror's setter** — the height and the panel's anchor delay.
    pub fn with_capacity_network_mirror(mut self, from_daa: Option<u64>, anchor_delay: u64) -> Self {
        self.capacity_network_from_daa = from_daa;
        self.capacity_network_anchor_delay = if from_daa.is_some() { anchor_delay } else { 0 };
        self
    }

    /// ADR-0160 F-N: `Params::palw_capacity_network_room`'s height, if armed (the mirror).
    pub fn capacity_network_from_daa(&self) -> Option<u64> {
        self.capacity_network_from_daa
    }

    /// **ADR-0160 F-N: does lane N judge an admission at `daa_score`?** `false` on every shipped preset.
    pub fn capacity_network_active_at(&self, daa_score: u64) -> bool {
        self.capacity_network_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// ADR-0160 F-N: the panel's anchor delay `L_anchor` reads (0 where F-N is not armed).
    pub fn capacity_network_anchor_delay(&self) -> u64 {
        self.capacity_network_anchor_delay
    }

    /// **int-11: the verification term's mirror's setter** (F-N's static `L_ver`).
    pub fn with_capacity_network_verify_mirror(mut self, from_daa: Option<u64>) -> Self {
        self.capacity_network_verify_from_daa = from_daa;
        self
    }

    /// int-11: `Params::palw_capacity_network_verify`'s height, if armed (the mirror).
    pub fn capacity_network_verify_from_daa(&self) -> Option<u64> {
        self.capacity_network_verify_from_daa
    }

    /// **int-11: does `L_net` carry the verification term at `daa_score`?** `false` on every shipped preset.
    pub fn capacity_network_verify_active_at(&self, daa_score: u64) -> bool {
        self.capacity_network_verify_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **Lane PL part C: the mirror's setter** (`Params::palw_panel_unavailable_expiry`).
    pub fn with_panel_unavailable_expiry_mirror(mut self, from_daa: Option<u64>) -> Self {
        self.panel_unavailable_expiry_from_daa = from_daa;
        self
    }

    /// Lane PL part C: `Params::palw_panel_unavailable_expiry`'s height, if armed (the mirror).
    pub fn panel_unavailable_expiry_from_daa(&self) -> Option<u64> {
        self.panel_unavailable_expiry_from_daa
    }

    /// **Lane PL part C: is `Params::palw_panel_unavailable_expiry` in force at `daa_score`?** `false` on every shipped preset.
    pub fn panel_unavailable_expiry_active_at(&self, daa_score: u64) -> bool {
        self.panel_unavailable_expiry_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **Lane PL part D: the mirror's setter** (`Params::palw_panel_fast_switch`).
    pub fn with_panel_fast_switch_mirror(mut self, from_daa: Option<u64>) -> Self {
        self.panel_fast_switch_from_daa = from_daa;
        self
    }

    /// Lane PL part D: `Params::palw_panel_fast_switch`'s height, if armed (the mirror).
    pub fn panel_fast_switch_from_daa(&self) -> Option<u64> {
        self.panel_fast_switch_from_daa
    }

    /// **Lane PL part D: is `Params::palw_panel_fast_switch` in force at `daa_score`?** `false` on every shipped preset.
    pub fn panel_fast_switch_active_at(&self, daa_score: u64) -> bool {
        self.panel_fast_switch_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **Lane PL part D: `T_fast`, the first panel's fast window** for a receipt window `window` (`W_r(c)`): `W_r / 20`, at
    /// least 30 DAA, at most half of `W_r` — 30 DAA at the shipped 600-DAA window, ~700 at the 2M class's 13,995. Healthy
    /// panels conclude in 2–6 DAA at the floor (the 10-02 measurement), so `T_fast` is 5x that at the floor and scales with the
    /// class's own verification window above it.
    pub fn panel_fast_switch_window_v1(window: u64) -> u64 {
        (window / 20).max(PALW_PANEL_FAST_SWITCH_MIN_DAA_V1).min(window.div_ceil(2)).max(1)
    }

    /// **Lane PL part D: how long after its bind the sweep waits on a `PanelBound` claim** — `window` (`W_r(c)`), except that a
    /// claim still holding its one redraw (`rebound_daa == None`) bound past `palw_panel_fast_switch` waits
    /// [`Self::panel_fast_switch_window_v1`]. The redraw's panel gets the whole window (`T_hard`). The one spelling the arm
    /// sites, DL-1 and the load check read, so the index cannot disagree with itself.
    pub fn panel_bound_sweep_window_v1(&self, claim: &PalwClaimStateV2, bound_daa: u64, window: u64) -> u64 {
        if claim.rebound_daa.is_none() && self.panel_fast_switch_active_at(bound_daa) {
            Self::panel_fast_switch_window_v1(window)
        } else {
            window
        }
    }

    /// **Lane PL part E: the mirror's setter** (`Params::palw_seat_availability`).
    pub fn with_seat_availability_mirror(mut self, from_daa: Option<u64>) -> Self {
        self.seat_availability_from_daa = from_daa;
        self
    }

    /// Lane PL part E: `Params::palw_seat_availability`'s height, if armed (the mirror).
    pub fn seat_availability_from_daa(&self) -> Option<u64> {
        self.seat_availability_from_daa
    }

    /// **Lane PL part E: is `Params::palw_seat_availability` in force at `daa_score`?** `false` on every shipped preset.
    pub fn seat_availability_active_at(&self, daa_score: u64) -> bool {
        self.seat_availability_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **int-11: the verification term's level at `daa_score`** — `Some(L_ver)` (`⌊1.5 × (window_receipt − anchor_delay) / 2⌋`,
    /// 435 at the shipped 600 / 20) once `palw_capacity_network_verify` is in force, `None` below it (and on every shipped
    /// preset): what `TransitionBuilder::network_level_v1` caps `L_net` with.
    pub fn capacity_network_verify_level_at(&self, daa_score: u64) -> Option<u64> {
        self.capacity_network_verify_active_at(daa_score).then(|| {
            crate::palw_network_room_v1::palw_network_verify_level_v1(self.window_receipt(), self.capacity_network_anchor_delay())
        })
    }

    /// **ADR-0164: the three stage-5/6 fences' mirrors' setter** — F-EM, F-M1 and F-K heights, written by
    /// `Params::sync_palw_capacity_s567` and by nothing else.
    pub fn with_capacity_s567_mirror(mut self, emission: Option<u64>, riders: Option<u64>, breaker: Option<u64>) -> Self {
        self.capacity_emission_from_daa = emission;
        self.capacity_riders_from_daa = riders;
        self.capacity_breaker_from_daa = breaker;
        self
    }

    /// ADR-0164 F-EM's height, if armed (the mirror).
    pub fn capacity_emission_from_daa(&self) -> Option<u64> {
        self.capacity_emission_from_daa
    }

    /// **ADR-0164 F-EM: does the per-DAA reward budget judge an attempt claim accepted at `daa_score`?** `false` on every shipped preset.
    pub fn capacity_emission_active_at(&self, daa_score: u64) -> bool {
        self.capacity_emission_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// ADR-0164 F-M1's height, if armed (the mirror).
    pub fn capacity_riders_from_daa(&self) -> Option<u64> {
        self.capacity_riders_from_daa
    }

    /// **ADR-0164 F-M1: may riders attach at `daa_score`?** `false` on every shipped preset.
    pub fn capacity_riders_active_at(&self, daa_score: u64) -> bool {
        self.capacity_riders_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// ADR-0164 F-K's height, if armed (the mirror).
    pub fn capacity_breaker_from_daa(&self) -> Option<u64> {
        self.capacity_breaker_from_daa
    }

    /// **ADR-0164 F-K: does the breaker count and judge at `daa_score`?** `false` on every shipped preset.
    pub fn capacity_breaker_active_at(&self, daa_score: u64) -> bool {
        self.capacity_breaker_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// ADR-0160 F-S: `Params::palw_capacity_issuance_slots`'s height, if armed (the mirror).
    pub fn capacity_slots_from_daa(&self) -> Option<u64> {
        self.capacity_slots_from_daa
    }

    /// **ADR-0160 F-S: does lane S judge an admission at `daa_score`?** `false` on every shipped preset.
    pub fn capacity_slots_active_at(&self, daa_score: u64) -> bool {
        self.capacity_slots_from_daa.is_some_and(|from| daa_score >= from)
    }

    /// **ADR-0160 F-S: does a claim ACCEPTED at `accepted_daa` hold an issuance slot?** (S.1: past the
    /// fence's height, a claim keeps one accounting for its whole life, ADR-0145's keying.)
    pub fn capacity_slots_applies_at(&self, accepted_daa: u64) -> bool {
        self.capacity_slots_active_at(accepted_daa)
    }

    /// ADR-0152 §4-ter (A-held): the held classes no honest party can dissect inside a turn (the
    /// mirror; empty where `palw_offence_attribution` is not armed).
    pub fn held_unanswerable_classes(&self) -> &[Hash64] {
        &self.held_unanswerable_classes
    }

    /// ADR-0152 §4-ter (A-held): is `class_id` one no honest party can dissect inside a turn? Asked
    /// only past `palw_offence_attribution` — below it every held class keeps the mercy and the
    /// one-move court's `NeedsDissection` is what it was.
    pub fn held_class_is_unanswerable_v1(&self, class_id: &Hash64) -> bool {
        self.held_unanswerable_classes.binary_search(class_id).is_ok()
    }

    /// ADR-0152 §4-ter (A-held): the mirror's setter, written by
    /// `Params::sync_palw_held_answerability` and by nothing else. Sorted here, so the lookup above
    /// is a search whatever order the caller derived the list in.
    pub fn with_held_unanswerable_classes(mut self, mut classes: Vec<Hash64>) -> Self {
        classes.sort();
        classes.dedup();
        self.held_unanswerable_classes = classes;
        self
    }

    /// **Does a view read a bond's `collateral` as already net of its `slashed`?**
    /// ([`palw_bond_free_collateral_v1`], review of the 2026-09-24 DoS audit #12.) The views hold no
    /// transition extras, so they read the bundle's copy of `palw_audit_2026_09_23` — the one value
    /// `Params::validate_palw_v2` makes equal to the fence — and, because that fence may only be
    /// armed at genesis (DAA 0, refused otherwise at startup), "armed" is "active at every DAA".
    pub fn bond_collateral_is_net_v1(&self) -> bool {
        self.escrow_backed_exposure_from_daa.is_some()
    }

    /// **Option A's escrow term: the claim's `escrowed_reward` past the height, nothing before it** — the
    /// escrow GAIN a claim reaches for. Below `palw_capacity_escrow_at_licence` it is also what the bond
    /// holds, and the ledger reads it through [`Self::claim_escrow_term_v2`]; past that fence the bond
    /// holds `m_c` instead and this remains only the gain (the Eq cap's `G_eq`).
    pub fn claim_escrow_reservation_v1(&self, accepted_daa: u64, escrowed_reward: u64) -> u128 {
        match self.escrow_backed_exposure_from_daa {
            Some(from) if accepted_daa >= from => escrowed_reward as u128,
            _ => 0,
        }
    }

    /// **The escrow slot a claim of `class_id` accepted at `accepted_daa` holds on its producer's
    /// bond** — option A's term ([`Self::claim_escrow_reservation_v1`]) below
    /// `palw_capacity_escrow_at_licence`, and past it (keyed on `accepted_daa`) `m_c`
    /// ([`crate::palw_escrow_funding_v2::palw_escrow_term_v2`], ADR-0160 E-1). The ONE reading the
    /// ledger's reserve and release, its re-derivation at load, SR-1's licence release, the seat duty at
    /// bind, the forfeit, the admission ceiling and the producer's headroom share, so the gate can never
    /// admit a claim the ledger cannot record, or release what it never held.
    ///
    /// `class_id` is the signature's one addition (ADR-0160 §7.2 left it to the lane): C7 keeps
    /// `m = E` (X-I5), and every caller holds the claim's class — the record or the attempt.
    pub fn claim_escrow_term_v2(&self, accepted_daa: u64, escrowed_reward: u64, class_id: &Hash64) -> u128 {
        // ADR-0176 amendment (ユーザー判断 2026-10-15): past `palw_bond_formula_v2` (keyed on acceptance) the escrow term is the
        // formula's economic right + collectable-loss top-up + court/DA/retention part; below it, exactly the old reading.
        if let Some(term) = self.bond_formula_escrow_term_v2(accepted_daa, escrowed_reward, class_id) {
            return term;
        }
        if self.capacity_escrow_active_at(accepted_daa) {
            return crate::palw_escrow_funding_v2::palw_escrow_term_v2(self, accepted_daa, escrowed_reward, class_id);
        }
        self.claim_escrow_reservation_v1(accepted_daa, escrowed_reward)
    }

    /// The per-session court budget (see the field's doc).
    pub fn window_court(&self) -> u64 {
        self.window_court
    }

    /// **Lane V02 (post-launch): the mirror's setter** — written by `Params::sync_palw_final_lock_life`
    /// and by nothing else, so the copy cannot drift from `Params::palw_final_lock_life`. `None` where
    /// the fence is not armed.
    pub fn with_final_lock_life_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.final_lock_life_from_daa = from_daa;
        self
    }

    /// Lane V02 (post-launch): `Params::palw_final_lock_life`'s height, if the network arms it (the mirror).
    pub fn final_lock_life_from_daa(&self) -> Option<u64> {
        self.final_lock_life_from_daa
    }

    /// **Lane V02 (post-launch): the DAA life a `Valid` seat's slashable lock gets when it is dated at
    /// `now_daa`.** [`PALW_FINAL_LOCK_LIFE_DAA_V1`] (1,000, never longer than `window_court`) once
    /// `Params::palw_final_lock_life` is in force at `now_daa`, else `window_court` — byte for byte the
    /// pre-fence life. Keyed on the DATING block's DAA, so a lock stamped below the height keeps the
    /// long life when read past it (the stored `expiry_daa` is what `is_live` reads), and one stamped
    /// past it gets the short life; deterministic across the fence.
    pub fn final_lock_life_at(&self, now_daa: u64) -> u64 {
        if self.final_lock_life_active_at(now_daa) { PALW_FINAL_LOCK_LIFE_DAA_V1.min(self.window_court) } else { self.window_court }
    }

    /// **Lane V02 (post-launch): is `Params::palw_final_lock_life` in force at `now_daa`?** The one
    /// key both halves of the fence read, each at its own block's DAA: the block that DATES a `Valid`
    /// lock ([`Self::final_lock_life_at`]) and the block that CONVICTS on one (a lock the fold no
    /// longer counts as committed is not slashable past the height — the user's 2026-09-26 decision).
    /// `false` wherever the fence is not armed, so both read exactly as before it.
    pub fn final_lock_life_active_at(&self, now_daa: u64) -> bool {
        self.final_lock_life_from_daa.is_some_and(|from| now_daa >= from)
    }

    /// **Lane F2-lock (post-launch): the mirror's setter** — written by
    /// `Params::sync_palw_final_lock_life_retro` and by nothing else (fixtures aside), so the copy
    /// cannot drift from `Params::palw_final_lock_life_retro`. `None` where the fence is not armed.
    pub fn with_final_lock_life_retro_from_daa(mut self, from_daa: Option<u64>) -> Self {
        self.final_lock_life_retro_from_daa = from_daa;
        self
    }

    /// Lane F2-lock (post-launch): `Params::palw_final_lock_life_retro`'s height `H`, if the network
    /// arms it (the mirror).
    pub fn final_lock_life_retro_from_daa(&self) -> Option<u64> {
        self.final_lock_life_retro_from_daa
    }

    /// **Lane F2-lock (post-launch): is `Params::palw_final_lock_life_retro` in force at `now_daa`?**
    /// Read by a `Final` at `now_daa` (past `H` its seat locks are dated to exactly `F + 1,000`, never
    /// kept longer) — the crossing block's one-time re-date is keyed on the crossing itself
    /// ([`palw_final_lock_life_retro_crossing_v1`]). `false` wherever the fence is not armed.
    pub fn final_lock_life_retro_active_at(&self, now_daa: u64) -> bool {
        self.final_lock_life_retro_from_daa.is_some_and(|from| now_daa >= from)
    }

    pub fn base_class_id(&self) -> Hash64 {
        self.base_class_id
    }

    pub fn class_daa_max_factor(&self) -> u32 {
        self.class_daa_max_factor
    }

    pub fn budget_tolerance_permille(&self) -> u32 {
        self.budget_tolerance_permille
    }

    /// The attempt lane's permille of combined production (see the field's doc).
    pub fn fp_attempt_share_permille(&self) -> u16 {
        self.fp_attempt_share_permille
    }

    /// How long an abandoned free-prompt claim's collateral stays reserved (see the field's doc).
    pub fn fp_abandon_hold_daa(&self) -> u64 {
        self.fp_abandon_hold_daa
    }

    /// ADR-0045's grant floor: the smallest share whose WORST-CASE epoch budget
    /// (denominator = the whole 1000‰) is still at least one block — `⌈10⁶ / (tol · E)⌉`.
    /// Enforced at every grant, which is what makes a mid-flight zero budget unrepresentable
    /// instead of an epoch-time cliff (the V1 derivation's `ZeroBudget`, moved to where the
    /// share is chosen).
    pub fn min_base_class_share_permille(&self) -> u16 {
        self.min_base_class_share_permille
    }

    /// Override the liveness floor's share floor. Refused above 1000‰ (it would make the base the
    /// only class a chain could ever carry) and below the grant floor (it would mean nothing).
    pub fn with_min_base_class_share_permille(mut self, permille: u16) -> Result<Self, PalwStateV2Error> {
        if permille > 1000 || permille < self.min_grantable_share_permille() {
            return Err(PalwStateV2Error::ShareOutOfRange { got: permille });
        }
        self.min_base_class_share_permille = permille;
        Ok(self)
    }

    pub fn min_grantable_share_permille(&self) -> u16 {
        let denom = (self.budget_tolerance_permille as u128) * (self.epoch_length as u128);
        // denom ≥ 1000 (tolerance floor × epoch ≥ 1), so the ceiling division is ≤ 1000 and a
        // whole-table base grant is always fundable.
        (1_000_000u128.div_ceil(denom)).max(1) as u16
    }
}

impl PalwStateParamsV2 {
    pub fn material_release_recovery_from_v2(&self) -> Option<u64> { self.material_release_recovery_v2.as_ref().map(|m|m.from_daa) }
    pub fn with_material_release_recovery_v2(mut self, mirror: Option<crate::palw_material_release_recovery_v2::PalwMaterialReleaseRecoveryMirrorV2>) -> Self { self.material_release_recovery_v2 = mirror; self }
    pub fn material_release_recovery_at_v2(&self, daa: u64) -> Option<&crate::palw_material_release_recovery_v2::PalwMaterialReleaseRecoveryPolicyV2> {
        self.material_release_recovery_v2.as_ref().filter(|m|daa>=m.from_daa && self.monetary_material_at_v1(daa).is_some()).map(|m|&m.policy)
    }
    pub fn monetary_material_from_v1(&self) -> Option<u64> {
        self.monetary_material.as_ref().map(|m| m.from_daa)
    }
    pub fn with_monetary_material(mut self, mirror: Option<crate::palw_monetary_material_v1::PalwMonetaryMaterialMirrorV1>) -> Self {
        self.monetary_material = mirror;
        self
    }
    pub fn monetary_material_at_v1(&self, daa: u64) -> Option<&crate::palw_monetary_material_v1::PalwMonetaryMaterialPolicyV1> {
        self.monetary_material.as_ref().filter(|m| daa >= m.from_daa && self.material_income_active_at(daa)).map(|m| &m.policy)
    }
}

impl crate::palw_state_v2::PalwStateParamsV2 {
    pub fn with_panel_lifetime_v2(mut self, mirror: Option<crate::palw_panel_lifetime_v2::PalwPanelLifetimeMirrorV2>) -> Self {
        self.panel_lifetime_v2 = mirror;
        self
    }
    pub fn panel_lifetime_v2_at(&self, daa: u64) -> Option<crate::palw_panel_lifetime_v2::PalwPanelLifetimeMirrorV2> {
        self.panel_lifetime_v2.filter(|m| daa >= m.from_daa)
    }
}

impl PalwStateParamsV2 {
    pub fn with_claim_challenge_v2(mut self, fence: Option<crate::palw_claim_challenge_v2::PalwClaimChallengeFenceV2>) -> Self {
        self.palw_claim_challenge_v2 = fence;
        self
    }
    pub fn claim_challenge_v2_at(&self, daa: u64) -> Option<crate::palw_claim_challenge_v2::PalwClaimChallengeFenceV2> {
        self.palw_claim_challenge_v2
            .filter(|f| f.activation != crate::config::params::ForkActivation::never() && f.activation.is_active(daa))
    }
}

impl PalwStateParamsV2 {
    pub fn with_claim_compact_v4(mut self, fence: Option<crate::palw_claim_compact_v4::PalwClaimCompactFenceV4>) -> Self {
        self.palw_claim_compact_v4 = fence.filter(|f| f.activation != crate::config::params::ForkActivation::never());
        self
    }
    pub fn claim_compact_v4(&self) -> Option<&crate::palw_claim_compact_v4::PalwClaimCompactFenceV4> {
        self.palw_claim_compact_v4.as_ref()
    }
    pub fn claim_compact_v4_at(&self, daa: u64) -> Option<&crate::palw_claim_compact_v4::PalwClaimCompactPolicyV4> {
        self.palw_claim_compact_v4.as_ref()
            .filter(|f| f.activation.is_active(daa) && self.claim_challenge_v2_at(daa).is_some())
            .map(|f| &f.policy)
    }
}
