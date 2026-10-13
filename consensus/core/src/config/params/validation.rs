//! **`Params`: validation methods** — moved out of `params.rs` (2026-10-13, a behaviour-preserving split: the methods are the same; each keeps the reach it had).

use super::*;

impl Params {
    /// **The three fences of the 2026-10-12 rulings** (register §3): `palw_court_model_scope_v1` (D2), `palw_public_evidence_reward_v1`
    /// (N4) and `palw_claim_hard_deadline_v1` (X3). Each is refused when armed at a real height until its rule is built and verified;
    /// `None` and `Some(never())` pass.
    pub fn validate_palw_2026_10_12_ruling_fences_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        for (fence, why) in [
            (
                self.palw_court_model_scope_v1,
                "palw_court_model_scope_v1 cannot be armed: its rule (register D2, ruling 2026-10-12) is not built and verified yet",
            ),
            (
                self.palw_public_evidence_reward_v1,
                "palw_public_evidence_reward_v1 cannot be armed: its rule (register N4, ruling 2026-10-12) is not built and verified yet",
            ),
            (
                self.palw_claim_hard_deadline_v1,
                "palw_claim_hard_deadline_v1 cannot be armed: its rule (register X3, ruling 2026-10-12) is not built and verified yet",
            ),
        ] {
            if fence.is_some_and(|f| f != ForkActivation::never()) {
                return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(why));
            }
        }
        // D2's first half is LG14-B's StepLeaf rule (a StepLeaf answer owes no model rows; the accuser supplies them), so the scope
        // fence needs `palw_legacy_held_da_v2` in force at or below it. Checked even though arming is refused above, so the ordering
        // is already pinned when the refusal is lifted.
        if let Some(scope) = self.palw_court_model_scope_v1.filter(|f| *f != ForkActivation::never())
            && !self
                .palw_legacy_held_da_v2
                .is_some_and(|held| held != ForkActivation::never() && held.daa_score() <= scope.daa_score())
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "palw_court_model_scope_v1 needs palw_legacy_held_da_v2 in force at or below it (its StepLeaf half)",
            ));
        }
        Ok(())
    }

    /// **The model-specific court window's prerequisite** (`palw_model_court_window`, the DAA-3,600 flag day): the
    /// k-ary court in force at or below it — a model-specific window has no executable court until the court
    /// that dissects is active. `Ok` while the fence is dormant (`None` or `never()`). Public so the rule is
    /// tested on its own: on a ruleset whose genesis registers a fused row, the k-ary court's own refusals answer first.
    pub fn validate_palw_model_court_window(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        if let Some(model_window_at) = self.palw_model_court_window.filter(|f| *f != ForkActivation::never())
            && !self.palw_kary_court.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= model_window_at.daa_score())
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "palw_model_court_window is armed before palw_kary_court: a model-specific window has no executable court until the k-ary court is active",
            ));
        }
        Ok(())
    }

    pub fn validate_palw_v2(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        self.validate_palw_v2_inner(false)
    }

    /// Feature-gated private factory exception for release heights only. All
    /// value, dependency and retirement checks run on the actual armed bundle.
    #[cfg(feature = "private-integration-config")]
    pub fn validate_palw_v2_private_rule_e_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        if self.palw_fork_choice_rule_e_v1 != Some(ForkActivation::new(1))
            || self.palw_fork_choice_commitment_v1 != Some(ForkActivation::new(1))
            || self.palw_dns_retirement.is_none_or(|r| r.activation != ForkActivation::new(1))
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid("private Rule E factory requires all three exact heights"));
        }
        self.validate_palw_v2_inner(true)
    }

    fn validate_palw_v2_inner(&self, private_rule_e: bool) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        // The overlay's bond collateral has one gate left: the acceptance-time skip, in force from
        // `bond_spend_gate_mergeset_activation_daa_score`. The own-body gate that covered the blocks
        // below that fence could not run on any network (every overlay ships the fence at the
        // overlay's own activation) and is gone, so an overlay whose fence sits above its
        // activation would leave collateral spendable in between. Refused rather than shipped.
        // Hard mandatory attestation inclusion is removed: no shipped overlay ever armed it, and a
        // network that names a height would run a rule no code enforces.
        if let Some(dns) = self.dns_params.as_ref()
            && dns.mandatory_attestation_inclusion_daa_score != u64::MAX
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "mandatory attestation inclusion is removed — the overlay's inclusion fence must stay unset (u64::MAX)",
            ));
        }
        if let Some(dns) = self.dns_params.as_ref()
            && dns.bond_spend_gate_mergeset_activation_daa_score > dns.dns_activation_daa_score
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "the overlay's bond spend gate must be in force from the overlay's activation (the own-body gate is gone)",
            ));
        }
        // **The token overlay is removed** — no ledger fold, emission settlement or token store is
        // left — so a network that arms one of its fences schedules rules no code enforces. The
        // fields stay (`DnsParams` is hashed whole into `consensus_params_id`); arming them is
        // refused here, before a peer is dialed.
        if let Some(dns) = self.dns_params.as_ref()
            && (dns.tkn.tkn_shadow_activation_daa_score != u64::MAX
                || dns.tkn.tkn_activation_daa_score != u64::MAX
                || dns.tkn.emission_activation_epoch != u64::MAX)
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "the token overlay is removed: dns_params.tkn's tkn_shadow_activation_daa_score, tkn_activation_daa_score and \
                 emission_activation_epoch must stay u64::MAX, because no code enforces an armed token program",
            ));
        }
        // **VLT voting weight is removed** — the compute-weighted tally, the frozen voting
        // snapshots, the precommit round, the finality certificates and the activation state
        // machine are gone, so a weight fence that names a height schedules a switch of voting
        // power no code performs. The field stays (`DnsParams` is hashed whole into
        // `consensus_params_id`); arming it is refused here. This subsumes the old ordering check
        // (the shadow fence at or below the weight fence): no shadow height exceeds `u64::MAX`.
        if let Some(dns) = self.dns_params.as_ref()
            && dns.vlt.vlt_activation_daa_score != u64::MAX
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "VLT voting weight is removed: dns_params.vlt.vlt_activation_daa_score must stay u64::MAX, because no code \
                 moves voting power onto verified compute",
            ));
        }
        // **The 2026-09-23 audit fence is armed at genesis or not at all** (2026-09-24 DoS audit
        // review). Past it the second clock ticks at the licence and every tick enters the anchor
        // ring; below it `Final` ticks the counter and nothing enters the ring. A crossing above
        // genesis therefore starts with a counter the ring does not hold, and the D1 floor reads
        // that surplus as pruned anchors (`palw_settled_anchor_floor_daa_v1` answers `Some(0)`:
        // only genesis bonds mature) until `depth` licences refill the ring, while the second
        // clock's escape reads the empty ring as a stretch with no anchor. Both are sound only on a
        // chain whose ring and counter grew together from block one — which is how the one network
        // that arms the fence (testnet-12) arms it. A later crossing needs its own migration of the
        // ring; until one exists it is refused here, before a peer is dialed.
        if let Some(fence) = self.palw_audit_2026_09_23
            && fence.daa_score() != 0
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "palw_audit_2026_09_23 may only be armed at genesis (DAA 0): a later crossing starts the anchor ring empty \
                 under a counter that pre-fence Finals already moved",
            ));
        }
        // **RFC-0002 Phase F: the IR fence's own refusals** (`crate::palw_tir_v1`).
        self.validate_palw_tir_v1()?;
        // **RFC-0003: the generative fence's own refusals** (`crate::palw_gen_v1`).
        self.validate_palw_gen_v1()?;
        // **FP Job V5** (`crate::palw_fp_job_v5`): over the generative fence and FP Job V4's.
        self.validate_palw_fp_job_v5_v1()?;
        // **The generative range twin** (`crate::palw_gen_range_twin_v1`): over the generative fence.
        self.validate_palw_gen_range_twin_v1()?;
        // **The task-head profile** (`crate::palw_task_heads_v1`): dormant, refused when armed.
        self.validate_palw_task_heads_v1()?;
        // **RFC-0011 §15.7's dormant fence** (`crate::palw_probabilistic_constraints_v1`): refused when armed.
        self.validate_palw_probabilistic_constraints_v1()?;
        // **RFC-0004 Part II's dormant fence** (`crate::palw_typed_roots_v1`): refused when armed.
        self.validate_palw_typed_roots_v1()?;
        // **RFC-0009 L2's dormant fence** (`crate::palw_fork_choice_commitment_v1`): refused when armed.
        if !private_rule_e {
            self.validate_palw_fork_choice_commitment_v1()?;
        }
        // **M*-49's dormant fence** (`crate::palw_verifier_pay_v1`): refused when armed.
        self.validate_palw_verifier_pay_v1()?;
        // **RFC-0002 P0's dormant fence** (`crate::palw_model_refutation_seal_v4`): refused when armed.
        self.validate_palw_model_refutation_seal_v1()?;
        self.validate_palw_beacon_source_opt_in_v1()?;
        self.validate_palw_beacon_source_fee_v1()?;
        // **RFC-0015's dormant fence** (`crate::palw_panel_free_v1`): refused when armed.
        self.validate_palw_panel_free_v1()?;
        self.validate_palw_signed_registration_v1()?;
        // **RFC-0006 per-segment pricing** (`crate::palw_tir_shard_segment_v2`): refused when armed.
        self.validate_palw_tir_shard_segment_v2()?;
        self.validate_palw_provider_court_v1()?;
        self.validate_palw_legacy_held_da_v2()?;
        // **ADR-0176 / ADR-0177's dormant fences** (`crate::palw_bond_budget_v1`): refused when armed.
        self.validate_palw_bond_budget_v1()?;
        self.validate_palw_reporter_income_v2()?;
        self.validate_palw_material_income_v3()?;
        self.validate_palw_paid_consumer_lifecycle_v1()?;
        self.validate_palw_model_bond_allocation_v1()?;
        // **ADR-0032's amended reporter share** (`crate::palw_reporter_share_v2`): refused when armed.
        self.validate_palw_reporter_share_v2()?;
        // **Lane LG14-A's public filer** (`crate::palw_legacy_public_filer_v1`): refused when armed.
        self.validate_palw_legacy_public_filer_v1()?;
        // **The three fences of the 2026-10-12 rulings** (register §3): refused when armed until their rules are built.
        self.validate_palw_2026_10_12_ruling_fences_v1()?;
        self.validate_palw_permissionless_panel_v1()?;
        // **A-2 uniformity** (`crate::palw_lifecycle_objects_v2`): every lifecycle kind's owning fence needs the audit fence declared.
        self.validate_palw_lifecycle_kind_fences_v1()?;
        // **palw_fp_prefix_inherit** (`crate::palw_fp_prefix_v1`).
        self.validate_palw_fp_prefix_inherit_v1()?;
        // **palw_fp_prefix_state** (`crate::palw_fp_prefix_v1`).
        self.validate_palw_fp_prefix_state_v1()?;
        // **palw_fp_tokenizer_match** (`crate::palw_fp_tokenizer_v1`).
        self.validate_palw_fp_tokenizer_match_v1()?;
        // **palw_fp_constraint_v2** (`crate::palw_fp_constraint_v2`).
        self.validate_palw_fp_constraint_v2_v1()?;
        // **palw_adapter_class_v1** (`crate::palw_adapter_class_v1`).
        self.validate_palw_adapter_class_v1_v1()?;
        // **RFC-0002 Phase F, the second IR fence's refusals** (`crate::palw_tir_fence2_v1`).
        self.validate_palw_tir_fence2()?;
        // **RFC-0006, layer-sharded panels' refusals** (`crate::palw_tir_shard_v1`).
        self.validate_palw_tir_shard_v1()?;
        // **ADR-0165: the Useful Work Transition's refusals** (`crate::palw_real_share_v1`).
        self.validate_palw_useful_work_v1()?;
        // **RFC-0004: the improvement fence's own refusals** (`crate::palw_improve_v1`).
        self.validate_palw_improvement_v1()?;
        self.validate_palw_model_immutable_v1()?;
        self.validate_palw_class_semantics_v1()?;
        self.validate_palw_court_commitment_scope_v1()?;
        self.validate_palw_staged_public_court_v1()?;
        // **RFC-0003 decision 22: the held leaf challenge's refusals** (`crate::palw_held_close_v1`).
        self.validate_palw_held_close_chunks_v1()?;
        // **RFC-0007 Part I: the verification vertex's refusals** (`crate::palw_vertex_v1`).
        self.validate_palw_verification_vertex_v1()?;
        // **RFC-0008 v2: the EXEC payload's refusals** (`crate::palw_exec_v2`): armable on no ruleset until the section 9 gates pass.
        self.validate_palw_exec_payload_v2()?;
        self.validate_palw_witness_manifest_v1()?;
        self.validate_palw_audit_mesh_v1()?;
        self.validate_palw_capped_onboarding_v1()?;
        // **RFC-0002 Part II §II.7.5: the class-seating fence's refusals** (`crate::palw_class_seating_fence_v1`).
        self.validate_palw_class_seating()?;
        // **RFC-0002 Phase H: the IR-only fence's refusals** (`crate::palw_tir_only_v1`).
        self.validate_palw_tir_only_v1()?;
        // **ADR-0152 v2 F2: the offence-attribution fence is armed at genesis or not at all.** Past
        // it the V1 `PanelFalseValid` is refused and a kind the chain never consumed before is; a
        // later crossing would leave pre-fence V1 rows beside V2 rows keyed differently for the
        // same (seat, claim), and one Valid could then be paid for twice. And it is a conviction
        // route of machinery four other fences define — the ledger (`palw_objective_offence`), the
        // lock unit and liability pruning (`palw_audit_2026_09_23`), the segmented receipt it
        // accepts (`palw_verification_v2`) and the forfeiture it records
        // (`palw_economic_safety`) — so each must be armed at or below it.
        if let Some(fence) = self.palw_offence_attribution
            && fence != ForkActivation::never()
        {
            if fence.daa_score() != 0 {
                return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                    "palw_offence_attribution may only be armed at genesis (DAA 0): a later crossing keys one seat's false \
                     Valid on one claim under two ledgers",
                ));
            }
            let armed_below =
                |f: Option<ForkActivation>| f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= fence.daa_score());
            if !(armed_below(self.palw_objective_offence)
                && armed_below(self.palw_audit_2026_09_23)
                && armed_below(self.palw_verification_v2)
                && armed_below(self.palw_economic_safety))
            {
                return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                    "palw_offence_attribution is armed without palw_objective_offence, palw_audit_2026_09_23, \
                     palw_verification_v2 and palw_economic_safety all armed at or below it: the rule convicts through the \
                     ledger, the locks, the segmented receipts and the forfeiture those fences define",
                ));
            }
            // **ADR-0152 v3.1 J-3 (F1, SPEC §4.3): the prefill draw at genesis too.** The identity
            // rule's J5 derives an attempt's whole context as the drawn one — the canonical job at
            // one decode step (`palw_attempt_job_v1(·, true)`) — so a network whose attempts ran the
            // whole canonical job at any height would have every honest claim of that height fail
            // J5 and be convicted of answering another job.
            if !self.palw_prefill_draw.is_some_and(|f| f != ForkActivation::never() && f.daa_score() == 0) {
                return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                    "palw_offence_attribution is armed without palw_prefill_draw at genesis: its identity rule (J5) derives \
                     every attempt's context at the prefill draw's one decode step",
                ));
            }
        }
        self.validate_palw_class_verify_deadline_v1()?;
        // ADR-0089 Decision 9's two preconditions: an EVM face of a market that does not exist,
        // or on a lane that is inert, is a design that has not been armed.
        if let Some(evm) = self.palw_model_evm {
            match self.palw_model_market {
                Some(market) if market.daa_score() <= evm.daa_score() => {}
                _ => return Err(crate::palw_mode_v2::PalwModeV2Error::ModelEvmBeforeMarket),
            }
            if self.evm_activation_daa_score > evm.daa_score() {
                return Err(crate::palw_mode_v2::PalwModeV2Error::ModelEvmOnInertLane);
            }
        }
        // Lane sink (the model sink binding, post-launch): over the market and the P-B1 refund route.
        self.validate_palw_model_sink_bound_v1()?;
        // ADR-0160 lane verify (F-B, F-R): over R-core+'s funnel, the audit fence and their mirrors.
        self.validate_palw_capacity_verify_v1()?;
        // ADR-0160 stage 2 (F-Q, F-S): over F-L, F-B, lane A (Q) and F-W, F-L, F-E (S), and a credited step
        // only past the audit door.
        self.validate_palw_capacity_stage2_v1()?;
        self.validate_palw_capacity_network_verify_v1()?;
        self.validate_palw_panel_unavailable_expiry_v1()?;
        self.validate_palw_panel_fast_switch_v1()?;
        self.validate_palw_seat_availability_v1()?;
        // ADR-0164 (stages 5–7): F-EM, F-M1, F-K and the ρ ≥ 250 steps' prerequisites.
        self.validate_palw_capacity_s567_v1()?;
        use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
        // **ADR-0093 Decision 8 needs the court it is a move of.** A root claim is a dissection's
        // first move; a fence armed where the k-ary court is not has no dissection to anchor. Asked
        // first: it reads two fences and nothing of the bundle.
        if let Some(anchored) = self.palw_attn_anchored_root
            && anchored != ForkActivation::never()
            && !self.palw_kary_court.is_some_and(|court| court != ForkActivation::never() && court.daa_score() <= anchored.daa_score())
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_attn_anchored_root is armed without palw_kary_court armed at or below the same height: the anchored root \
                 claim is a move of a dissection the network does not run (ADR-0093 Decision 8)",
            ));
        }
        // ADR-0135: the registry reads seat exposure (the panel economy) and steps at the execution
        // lane's span boundaries, so both must be in force by its height.
        if let Some(registry) = self.palw_model_registry
            && registry != ForkActivation::never()
        {
            let economy_below =
                self.palw_panel_economy.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= registry.daa_score());
            let lane_below = self
                .palw_execution_lane
                .is_some_and(|lane| lane.activation != ForkActivation::never() && lane.activation.daa_score() <= registry.daa_score());
            if !economy_below || !lane_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_model_registry is armed without palw_panel_economy and palw_execution_lane armed at or below its height: \
                     the registry reads seat exposure and steps at span boundaries",
                ));
            }
        }
        // ADR-0132 Upgrade C: the payout prices a claim on the registry's work for its class and
        // splits it with a panel the economy put on duty, so both must be in force by its height;
        // and its numbers must be a price (a zero rate pays nothing to everyone) and shares.
        if let Some(payout) = self.palw_economic_payout
            && payout.activation != ForkActivation::never()
        {
            let registry_below = self
                .palw_model_registry
                .is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= payout.activation.daa_score());
            let economy_below = self
                .palw_panel_economy
                .is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= payout.activation.daa_score());
            if !registry_below || !economy_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_economic_payout is armed without palw_model_registry and palw_panel_economy armed at or below its height: \
                     the payout prices the registry's work and splits with the economy's panel",
                ));
            }
            if payout.rate_sompi_per_giga == 0
                || payout.panel_share_min_permille > payout.panel_share_max_permille
                || payout.panel_share_max_permille > 1_000
                || payout.cap_utilization_max_permille > 1_000
            {
                return Err(PalwModeV2Error::Invalid(
                    "palw_economic_payout carries a zero rate, an inverted or over-unity panel share, or an over-unity cap ceiling",
                ));
            }
        }
        // ADR-0137: the work target reads the registry's rows (a class's CCU) and the payout's
        // rate (W₀); both must be armed at or below it.
        if let Some(work) = self.palw_work_target
            && work != ForkActivation::never()
        {
            let registry_below =
                self.palw_model_registry.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= work.daa_score());
            let payout_below = self
                .palw_economic_payout
                .is_some_and(|p| p.activation != ForkActivation::never() && p.activation.daa_score() <= work.daa_score());
            if !registry_below || !payout_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_work_target is armed without palw_model_registry and palw_economic_payout armed at or below its height: \
                     the work target prices the registry's classes at the payout's rate",
                ));
            }
        }
        // ADR-0132 S: with the network draw gone, the work target is what holds the cadence.
        if let Some(single) = self.palw_single_lottery
            && single != ForkActivation::never()
        {
            let work_below =
                self.palw_work_target.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= single.daa_score());
            if !work_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_single_lottery is armed without palw_work_target armed at or below its height: with the network draw \
                     gone the work target is the only rule left holding the cadence",
                ));
            }
        }
        // **ADR-0145: the derived basis needs the registry UNDER it, and the reason is a drift that
        // nothing later can reconcile.**
        //
        // A claim's derived work is read from its class's `model_lifecycles` row, which is written
        // when the registry first steps (`step_model_registry` gives every class in state a row,
        // real or inert) and never rewritten afterwards. A claim accepted past the canonical-work
        // fence but BEFORE the registry fence would find no row at its `Final` — priced on the
        // declared basis — and would find one at its retirement, priced on the derived one. The
        // difference lands in `retired_safe_weight`, the claim is gone, and no fence position
        // reconciles it: the node refuses its own tip on the next restart and every peer importing
        // that snapshot is refused with it. Requiring the registry at or below the height makes the
        // row exist before any re-priced claim can, so the three readings cannot disagree.
        if let Some(canonical) = self.palw_canonical_work.filter(|f| *f != ForkActivation::never()) {
            let registry_below =
                self.palw_model_registry.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= canonical.daa_score());
            if !registry_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_canonical_work is armed without palw_model_registry armed at or below its height: the derived work is \
                     read from a class's registry row, and a claim that finalizes before the row exists is re-priced when it \
                     retires — a drift nothing later can reconcile",
                ));
            }
        }
        // **The economic activation bundle arms at ONE height, or not at all.**
        //
        // ADR-0145's three fences are not independent policies that happen to have landed
        // together. Accounting made new while admission stays old leaves a registrant able to admit
        // the very class whose work the new accounting prices; admission made new while the
        // free-prompt lane still takes the executor's word leaves that lane outside the rule; and
        // either of those composed with the other is an arbitrage path neither fence opens alone.
        // The 2026-09-19 re-audit found three such compositions, of which item 7 is the clearest —
        // a `Final` bought before the registry opened, carried through an ARMED independence fence.
        // Each fence was correct and the pair was not.
        //
        // The registry and the work target join the rule, and at one height rather than in order:
        // the registry's share redistribution writes `class_shares` in proportion to
        // `admission_milli`, which carries `admission_permille` (Probation 50, ActiveLimited 100,
        // Active 1000), and `class_shares` is the pwu side of `attempt_target_seed_v1` — a class's
        // DIFFICULTY, hence its weight per claim. That path is dead only while the work target is
        // active, so with the registry armed strictly first a class's lifecycle stage moves its
        // PRICE. The design says the lifecycle expands the AMOUNT of eligible work and nothing else,
        // and a property that holds because two numbers in a release card happen to match is not a
        // property.
        //
        // A build that arms none of the three is refused nothing here, which is every shipped
        // preset today.
        if let Some(height) = [self.palw_canonical_work, self.palw_admission_independence, self.palw_fp_derived_work]
            .iter()
            .find_map(|f| f.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score()))
        {
            let at_height = |f: Option<ForkActivation>| f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() == height);
            if !at_height(self.palw_canonical_work)
                || !at_height(self.palw_admission_independence)
                || !at_height(self.palw_fp_derived_work)
            {
                return Err(PalwModeV2Error::Invalid(
                    "the ADR-0145 economic fences are not all armed at one height: canonical work, independent admission and \
                     the free-prompt lane's derived work COMPOSE, and each is safe in a way the pairs are not — they arm \
                     together or not at all",
                ));
            }
            let one_height_below = match (
                self.palw_model_registry.filter(|f| *f != ForkActivation::never()),
                self.palw_work_target.filter(|f| *f != ForkActivation::never()),
            ) {
                (Some(registry), Some(target)) => registry.daa_score() == target.daa_score() && registry.daa_score() <= height,
                _ => false,
            };
            // **And one owner per artifact root** (ADR-0143), which is the bundle's other
            // precondition and the one that makes "a relabelled copy is not a new model"
            // structural rather than a hope.
            //
            // `shape_profile_id` includes representation choices the canonical descriptor strips —
            // `tile_len`, a thread count — so a registrant can mint a DIFFERENT class id for the
            // SAME model by re-tiling it, and each copy takes its own registration, its own row and
            // its own share grant. Past ADR-0143 it cannot: a re-tiling does not change the
            // weights, so the copy must name the artifact root it is a tiling OF, and a root has
            // exactly one owner on the chain. A copy that names a root it does not own is refused
            // at the entrance; one that invents a root has no artifact behind it and no seat can
            // prove possession of it.
            //
            // Without this the economic bundle would derive one canonical work per model and still
            // let one model hold N classes' worth of grants — which is the same finding the
            // denominator fix closed from the other side.
            let one_owner_per_root =
                self.palw_artifact_root_ownership.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= height);
            if !one_owner_per_root {
                return Err(PalwModeV2Error::Invalid(
                    "the ADR-0145 economic bundle is armed without palw_artifact_root_ownership at or below it: without one \
                     owner per artifact root, a registrant mints a fresh class id for the same model by re-tiling it, and \
                     each copy takes its own registration, its own lifecycle row and its own share grant",
                ));
            }
            if !one_height_below {
                return Err(PalwModeV2Error::Invalid(
                    "the ADR-0145 economic bundle is armed without palw_model_registry and palw_work_target armed at ONE height \
                     at or below it: armed apart, the registry redistributes class_shares in proportion to admission_permille, \
                     so a class's lifecycle stage moves its difficulty and therefore its weight per claim — the lifecycle may \
                     expand the amount of eligible work, never its price",
                ));
            }
        }
        // ADR-0142 §6a: the clock's reference is derived from the DAA window — the block at the
        // parent's score with the lowest blue score — which is what lets a pruned node and one that
        // joined by pruning proof answer exactly as an archival one. A SAMPLED window can omit that
        // block, and the reference would then be a later one at the same score: every node would
        // still agree, but slots would open late and the clock would run slow for no stated reason.
        // So the rule names its own precondition rather than assuming it.
        if let Some(cursor) = self.palw_clock_cursor
            && cursor != ForkActivation::never()
            && self.difficulty_sample_rate != 1
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_clock_cursor is armed on a network whose difficulty window is sampled: the clock's reference is the \
                 window block that advanced the score, and a sampled window can omit it",
            ));
        }
        // ADR-0142: the anchor clock leaves the heartbeat as the only lane that can advance the
        // score. Under ADR-0066 Decision 2's rule an attempt block postponed the next beat, so a
        // chain producing faster than the interval suppressed the lane and the score stopped —
        // measured on the registry drill as a DAA frozen at its own flag day, zero beats minted.
        // The two fences are therefore not independent, and this is where that is enforced.
        if let Some(anchor) = self.palw_anchor_clock
            && anchor != ForkActivation::never()
        {
            let cursor_below =
                self.palw_clock_cursor.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= anchor.daa_score());
            if !cursor_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_anchor_clock is armed without palw_clock_cursor armed at or below its height: past the anchor clock \
                     only a heartbeat can advance the score, and without the cursor an attempt block postpones the next one",
                ));
            }
        }
        // The clock floor (H3/H5) is a rule ABOUT the cursor — the slot a beat is stamped for, the
        // stamp a step must carry, the reference among tied steps — so it cannot stand without one.
        if let Some(floor) = self.palw_clock_floor
            && floor != ForkActivation::never()
        {
            let cursor_below =
                self.palw_clock_cursor.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= floor.daa_score());
            if !cursor_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_clock_floor is armed without palw_clock_cursor armed at or below its height: the floor bounds the \
                     cursor's slots, and without the cursor there is no slot to bound",
                ));
            }
        }
        // **ADR-0147: an outsider-judged claim cannot be licensed by parts.** The stratified draw
        // (`palw_shard_licensing`, ADR-0100 Decision 4) seats each shard from the class's own
        // population and has no outsider seat, so a bought class drawn per shard would be judged
        // by the population its registrant fills — the defect this bundle exists to close, through
        // a door it never looked at. Refused together until the stratified draw carries an
        // outsider of its own.
        if let Some(independence) = self.palw_admission_independence
            && independence != ForkActivation::never()
            && self.palw_shard_licensing.is_some_and(|f| f != ForkActivation::never())
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_admission_independence is armed beside palw_shard_licensing: a stratified panel seats no outsider, so \
                 a bought class licensed by parts would be judged by its own population alone (ADR-0147)",
            ));
        }
        // **The 2026-09-19 audit's F3: admission independence is enforced through the registry's
        // rows, so it may not be armed without the registry.** Two of its four rules read a class's
        // lifecycle row — the `Candidate` state a registration opens in, and ADR-0145 I4's rule
        // that a class which admits no claims is not in the work-price unit — and rows exist only
        // past `palw_model_registry`. Armed alone it would carry the panel and quorum halves and
        // silently carry neither of the others: a registration would still reprice every
        // incumbent, which is the finding it is here to close. The rule names its own precondition
        // rather than leaving it to a deployment note.
        if let Some(independence) = self.palw_admission_independence
            && independence != ForkActivation::never()
        {
            let registry_below =
                self.palw_model_registry.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= independence.daa_score());
            if !registry_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_admission_independence is armed without palw_model_registry armed at or below its height: a class's \
                     admission and the work-price unit are both read off registry rows, and without the registry there are none",
                ));
            }
        }
        // Option A (2026-09-23): the audit fence and the V2 bundle's copy of its height are one value.
        // The ledger's reserve, its release and its re-derivation at load read the COPY (the load path
        // has no transition extras), while the fork id hashes the fence — a bundle that disagreed
        // would reserve escrow the fingerprint never announced, or restart into a ledger it cannot
        // re-derive.
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            let fence = self.palw_audit_2026_09_23.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
            if bundle.state.escrow_backed_exposure_from_daa() != fence {
                return Err(PalwModeV2Error::Invalid(
                    "palw_audit_2026_09_23 and the V2 bundle's escrow_backed_exposure_from_daa disagree: mirror the fence \
                     with Params::sync_palw_escrow_backed_exposure after the bundle is assembled",
                ));
            }
        }
        // ADR-0132 §7.6: the short challenge window's fence and the V2 bundle's copy of its height
        // are one value. A bundle that disagrees would price challenge deadlines the fork id never
        // advertised — the silent divergence the fence exists to end.
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            let fence = self.palw_short_challenge_window.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
            if bundle.state.short_challenge_window_from_daa() != fence {
                return Err(PalwModeV2Error::Invalid(
                    "palw_short_challenge_window and the V2 bundle's short_challenge_window_from_daa disagree: arm the fence \
                     through Params::set_palw_short_challenge_window, which writes both",
                ));
            }
        }
        // ADR-0133 §11.3: the class receipt-window fence and the V2 bundle's copy are one value,
        // for the reason the short challenge window's are — a bundle that disagreed would arm
        // receipt deadlines the fork id never advertised, and the deadline index is rebuilt from
        // the bundle at every restart, so the disagreement would surface as one node sweeping a
        // claim another still holds live.
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            let fence = self.palw_class_receipt_window.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
            if bundle.state.class_receipt_window_daa() != fence {
                return Err(PalwModeV2Error::Invalid(
                    "palw_class_receipt_window and the V2 bundle's class receipt window disagree: arm the fence through \
                     Params::set_palw_class_receipt_window, which writes both",
                ));
            }
        }
        // ADR-0133 §11.3 needs a span to measure a class's window in, and the lane is what states
        // one. Arming the deadline rule without the lane would multiply every class's
        // `verification_window_spans` by the fallback span, which is a different number from the
        // one the `window_fits_receipt` observation uses — the two would disagree about the same
        // class.
        if let Some(receipt_window) = self.palw_class_receipt_window.filter(|f| *f != ForkActivation::never()) {
            let lane_below = self.palw_execution_lane.is_some_and(|lane| {
                lane.activation != ForkActivation::never() && lane.activation.daa_score() <= receipt_window.daa_score()
            });
            if !lane_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_class_receipt_window is armed without palw_execution_lane armed at or below its height: a class's \
                     derived window is counted in spans, and without the lane there is no span length to measure it in",
                ));
            }
        }
        // ADR-0133 Verification V2 never precedes the registry: segment-scoped licensing is the
        // model economy's capacity rule, and it has nothing to count before the registry seats a
        // panel by evidence.
        if let Some(v2) = self.palw_verification_v2.filter(|f| *f != ForkActivation::never()) {
            let registry_below =
                self.palw_model_registry.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= v2.daa_score());
            if !registry_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_verification_v2 is armed without palw_model_registry armed at or below its height: coverage licensing \
                     counts seats the registry seated",
                ));
            }
        }
        if let Some(s3) = self.palw_verification_s3.filter(|f| *f != ForkActivation::never()) {
            let v2_below = self.palw_verification_v2.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= s3.daa_score());
            if !v2_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_verification_s3 is armed without palw_verification_v2 armed at or below its height: sampling attests \
                     a V2 mask",
                ));
            }
        }
        if let Some(s2) = self.palw_verification_s2.filter(|f| *f != ForkActivation::never()) {
            let v2_below = self.palw_verification_v2.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= s2.daa_score());
            if !v2_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_verification_s2 is armed without palw_verification_v2 armed at or below its height: the optimistic \
                     door names the S1 full-replay seat",
                ));
            }
            if let Some(s3) = self.palw_verification_s3.filter(|f| *f != ForkActivation::never())
                && s3.daa_score() > s2.daa_score()
            {
                return Err(PalwModeV2Error::Invalid(
                    "palw_verification_s2 is armed below palw_verification_s3: the operator's order is S1 then S3 then S2",
                ));
            }
        }
        // ADR-0138: the anchor clock and the single lottery are one decision at one height. An
        // unpriced attempt lane that still advances the DAA score runs every window ten times fast;
        // an anchor clock without the single lottery would exempt nothing new. Both or neither, same day.
        // The clock alone is a rule with something to exempt (receipts and heartbeats have been
        // unpriced since ADR-0083), so it may be armed alone; but where the single lottery is armed
        // too, the two share one day — an unpriced attempt lane that still ticks the clock is the
        // defect the fence exists to close, and no test-only clearing may spell them apart.
        if let (Some(clock), Some(lottery)) = (
            self.palw_anchor_clock.filter(|f| *f != ForkActivation::never()),
            self.palw_single_lottery.filter(|f| *f != ForkActivation::never()),
        ) && clock.daa_score() != lottery.daa_score()
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_anchor_clock and palw_single_lottery are armed at different heights: the lane leaves `bits` and the DAA \
                 clock on one day",
            ));
        }
        // ADR-0133 §11.2: readiness V2 replaces the registry's own possession proof, so it never
        // precedes the registry that seats one.
        if let Some(v2) = self.palw_readiness_v2.filter(|f| *f != ForkActivation::never()) {
            let registry_below =
                self.palw_model_registry.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= v2.daa_score());
            if !registry_below {
                return Err(PalwModeV2Error::Invalid(
                    "palw_readiness_v2 is armed without palw_model_registry armed at or below its height: a possession proof \
                     proves possession of a class the registry holds",
                ));
            }
        }
        // **ADR-0128 Decision 8: the BFT gate's refusals**, ahead of the V2 gate below because the
        // overlay is any lineage's. Every `Some` is judged, a `never()` height included: the values
        // reach the fingerprint whether or not the height does.
        if let Some(gate) = self.dns_bft_gate {
            let Some(dns) = self.dns_params.as_ref() else {
                return Err(PalwModeV2Error::Invalid(
                    "dns_bft_gate is set on a network that runs no DNS overlay: there are no validators to vote and no \
                     stake reorg gate to decide (ADR-0128 Decision 8)",
                ));
            };
            if gate.t_leak_daa == 0 {
                return Err(PalwModeV2Error::Invalid(
                    "dns_bft_gate.t_leak_daa is zero: every bond would be leaked the moment its epoch closed, and no counted \
                     set could keep a validator (ADR-0128 Decision 8)",
                ));
            }
            if gate.reentry_final_depth_daa >= gate.t_leak_daa {
                return Err(PalwModeV2Error::Invalid(
                    "dns_bft_gate.reentry_final_depth_daa is not below t_leak_daa: the re-entry window is empty, so a leaked \
                     validator could never be counted again (ADR-0066 SA-2, ADR-0128 Decision 8)",
                ));
            }
            if gate.min_retained_validators < crate::dns_bft_v1::DNS_BFT_MIN_RETAINED_VALIDATORS_FLOOR_V1 {
                return Err(PalwModeV2Error::Invalid(
                    "dns_bft_gate.min_retained_validators is below four: a counted set smaller than that tolerates no fault, so \
                     leaking down to it would let a handful of validators finalize for the network (ADR-0128 Decision 3)",
                ));
            }
            let walk = crate::dns_bft_v1::dns_bft_walk_blue_score_v1(&gate, dns);
            if walk.is_none_or(|walk| self.blockrate.pruning_depth < walk) {
                return Err(PalwModeV2Error::InvalidOwned(format!(
                    "dns_bft_gate reads a walk of {} blue score from the sink (the StakeScore window plus the leak's evidence \
                     window) and the pruning depth is {}: the pruning point would pass the evidence a sink's leak reads, and \
                     two synced nodes could count different sets (ADR-0128 Decision 3, SA-3)",
                    walk.map_or_else(|| "more than u64::MAX".to_owned(), |w| w.to_string()),
                    self.blockrate.pruning_depth
                )));
            }
        }
        // **The inactivity leak is removed** (ADR-0060 Decision 4 / ADR-0066 Decision 4): the
        // window-bound leak this field armed is gone, and the leak that exists is `dns_bft_gate`'s
        // (ADR-0128 Decision 3), with its own numbers and its own evidence window.
        // `palw_inactivity_leak` stays a field — hashed only when set, so an unset preset's ids are
        // what they were — and a network that sets it is refused before a peer is dialed. Checked
        // ahead of the V2 gate below, because the leak was a DNS-overlay rule any lineage could set.
        if self.palw_inactivity_leak.is_some() {
            return Err(PalwModeV2Error::Invalid(
                "the inactivity leak is removed: palw_inactivity_leak must stay unset, because no code implements its rule — \
                 the counted set's leak is dns_bft_gate's (ADR-0128)",
            ));
        }
        // **ADR-0069 Decision 7 is armable AT GENESIS or not at all, and a scheduled height is
        // refused here — on every lineage, before the mode is even looked at.**
        //
        // The fence carries a REPAIR as well as a rule (see the field's doc): past it,
        // `retire_claim` books a retiring free-prompt claim's spent quanta into
        // `retired_safe_weight` instead of stranding them in `safe_weight`. Arming it closes that
        // hole going forward. It does not, and cannot, repair a chain that already stranded weight
        // while the fence was dormant, and that asymmetry is the difference between a fence and a
        // recovery path:
        //
        //  * the stranded weight is in `safe_weight` and the claim that would re-derive it has
        //    been dropped, so there is nothing left at the activation height to compute a
        //    correction FROM — `retired_safe_weight` was never credited, so `safe_ceiling` sits
        //    BELOW `safe_weight` and the armed `<=` bound refuses for the same reason the dormant
        //    equality does;
        //  * crossing the activation does not heal it: the fold carries both scalars forward
        //    untouched, so every later state inherits the refusal;
        //  * and a node whose durable tip is such a state cannot start at all (`load_tip` →
        //    `CarriageInconsistent`), so it never reaches the activation height to be healed by
        //    it. The same is true of every peer importing that pruning-point snapshot.
        //
        // All three are measured, in all three positions, by
        // `arming_the_fence_does_not_repair_a_chain_that_already_stranded_weight` (consensus/core/
        // tests/palw_adr0069_d7_fold.rs). Repairing such a chain means writing a state BELOW the
        // fence differently, which moves roots that are already committed — a re-mint, not an
        // activation.
        //
        // So the only safe arming is one where no dormant history exists: genesis. ADR-0069 says
        // that in prose; a sentence in an ADR is not a gate — this is the gate, in the same place
        // and for the same reason as ADR-0065 D1's window check below. An operator who follows
        // the ADR by scheduling a height gets a node that refuses to start, not a mesh that
        // wedges one retirement later.
        //
        // `never()` is exempt because `Some(never())` is absence (it collapses to `None` in
        // `normalize_values_a_scheduled_fence_drags_with_it`), and the comparison is against
        // `genesis.daa_score` rather than 0 so a preset whose genesis does not start at zero is
        // judged against its own first block.
        //
        // **What this gate does NOT have to catch**: an operator arming `always()` on a chain that
        // is already running. Params carry no chain view, so it could not be detected here — and
        // it does not need to be, because in force from block 1 is a RULE difference: it moves
        // `consensus_identity_id` (pinned in
        // `the_uncertified_weightless_fence_separates_networks_only_when_it_is_in_force`), so such
        // a node is refused at the handshake by every peer on the running network instead of
        // joining it and wedging one retirement later. A scheduled height is the dangerous one
        // precisely because it does NOT move the identity — that is what the top-level placement
        // buys, and it is why the refusal has to live here rather than in the fingerprint.
        if let Some(activation) = self.palw_uncertified_weightless {
            let armed = activation.daa_score();
            if armed != ForkActivation::never().daa_score() && armed > self.genesis.daa_score {
                return Err(PalwModeV2Error::Invalid(
                    "palw_uncertified_weightless is armed above genesis: ADR-0069 Decision 7 carries the free-prompt \
                     retirement repair, and arming it on a chain that may already have stranded weight while it was \
                     dormant does not repair that chain — it is genesis-only, or dormant",
                ));
            }
        }
        // **ADR-0083's second fence needs its first** (mainnet audit, 2026-09-05). The receipt rows
        // leave the count only on a window that already counts by lane; armed alone, or armed
        // before `palw_difficulty_priced_rows`, it would name a rule the window is not running.
        if let Some(receipts) = self.palw_receipt_rows_unpriced
            && receipts != ForkActivation::never()
        {
            let first_in_time = self
                .palw_difficulty_priced_rows
                .filter(|priced| *priced != ForkActivation::never())
                .is_some_and(|priced| priced.daa_score() <= receipts.daa_score());
            if !first_in_time {
                return Err(PalwModeV2Error::Invalid(
                    "palw_receipt_rows_unpriced is armed while palw_difficulty_priced_rows is dormant or scheduled later: the \
                     receipt lane can only leave a count that is already taken by lane (ADR-0083 Decision 1)",
                ));
            }
        }
        // **ADR-0072 D8's header-stage pin is a V2-lane rule, and it must not be armable without a
        // lane** (mainnet audit, 2026-09-06 — C-1). The pins it enforces are fields of a V2 attempt
        // envelope and their derivation is the bundle's own `PalwStateParamsV2`; armed on a network
        // with no bundle the fence names a rule with nothing to apply it to, and the header
        // processor would resolve it to `None` while `consensus_params_id` said the network had
        // armed something. A fence that hashes and does not fire is the silent-fork shape this file
        // exists to refuse.
        if let Some(pins) = self.palw_attempt_header_pins
            && pins != ForkActivation::never()
            && !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_))
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_attempt_header_pins is armed on a network that is not ConsensusV2: the DA pins it enforces are fields \
                 of a V2 attempt envelope and their derivation is the bundle's state params (ADR-0072 Decision 8)",
            ));
        }
        // **The free-prompt DA pin is a V2-lane rule too, and it must not be armable without a
        // lane** (mainnet audit, 2026-09-06 — M-3). The obligation it derives is written onto a
        // `PalwClaimStateV2` record by the V2 transition, and its length comes from the bundle's own
        // state params; armed on a network with no bundle it names a rule with nothing to apply it
        // to while `consensus_params_id` says the network armed something.
        if let Some(pins) = self.palw_fp_da_pins
            && pins != ForkActivation::never()
            && !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_))
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_fp_da_pins is armed on a network that is not ConsensusV2: the retention obligation it derives is \
                 written onto a V2 claim record and its length is the bundle's state params (ADR-0072 Decision 8)",
            ));
        }
        // **ADR-0105's coloring rule is about heartbeats, and it must not be armable where there
        // are none.** Armed without the lane (or on a network that is not ConsensusV2, where the
        // lane cannot exist) the fence would enter `consensus_params_id` and fire on nothing — a
        // fence that hashes and does not fire, the shape the two refusals above exist to keep out.
        if let Some(transparent) = self.palw_heartbeat_transparent
            && transparent != ForkActivation::never()
            && !self.palw_heartbeat_lane_fence().is_some_and(|lane| lane != ForkActivation::never())
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_heartbeat_transparent is armed on a network whose heartbeat lane is not: the rule decides how a \
                 heartbeat counts in a bonded block's anticone, and with no heartbeat lane (or no ConsensusV2 ruleset) \
                 there is nothing for it to decide (ADR-0105)",
            ));
        }
        // ---- F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
        // The same-chain restriction narrows ADR-0105's transparency; armed where that rule is not in
        // force at or below its height it would enter the fingerprint and restrict nothing.
        if let Some(same_chain) = self.palw_heartbeat_transparent_same_chain
            && same_chain != ForkActivation::never()
            && !self
                .palw_heartbeat_transparent_fence()
                .is_some_and(|transparent| transparent != ForkActivation::never() && transparent.daa_score() <= same_chain.daa_score())
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_heartbeat_transparent_same_chain is armed where palw_heartbeat_transparent is not in force at or below its \
                 height: it restricts which candidates that rule exempts, and without the rule there is nothing to restrict \
                 (ADR-0105 §11)",
            ));
        }
        // ---- end F1 heartbeat transparency (post-launch flag day, rcore/f1-hb-transparency) ----
        // ADR-0160 lane escrow (post-launch): over option A's slot, R-core+'s ledger and F2's void, mirrored.
        self.validate_palw_capacity_escrow_v1()?;
        // **ADR-0107 reads the closed epoch's finals off the claim records, so it must not be
        // armable where those records are gone before the boundary reads them** — or on a network
        // with no V2 lane at all, where there are no claims and the fence would hash and not fire.
        if let Some(fence) = self.palw_share_growth_final
            && fence != ForkActivation::never()
        {
            let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else {
                return Err(PalwModeV2Error::Invalid(
                    "palw_share_growth_final is armed on a network that is not ConsensusV2: it counts V2 attempt claims \
                     that reached Final, and there are none (ADR-0107)",
                ));
            };
            let retirement = bundle.state.claim_retirement_daa();
            if retirement != 0 && retirement <= bundle.state.epoch_length() {
                return Err(PalwModeV2Error::Invalid(
                    "palw_share_growth_final is armed where claims retire within an epoch (claim_retirement_daa <= \
                     epoch_length): a claim finalized early in the closed epoch would be gone before the boundary counts \
                     it, and the class would be refused growth it earned (ADR-0107)",
                ));
            }
        }
        if let Some(retirement) = self.palw_dns_retirement {
            if self.dns_params.as_ref().is_some_and(|dns| {
                retirement.activation.daa_score() < dns.full_reward_split_daa_score
                    || retirement.activation.daa_score() < dns.dns_activation_daa_score
            }) {
                return Err(PalwModeV2Error::Invalid("RFC-0012 retirement must follow DNS activation and its full subsidy split"));
            }
            if self.palw_overlay_carve.is_some_and(|c| c.worker_carve_permille > 920)
                || matches!(&self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(b) if b.state.worker_carve_permille() > 920)
            {
                return Err(PalwModeV2Error::Invalid("RFC-0012 recovery escrow must fit inside the 92% worker base"));
            }
            if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_))
                || self.dns_params.is_none()
                || !retirement.settlement.valid()
                || retirement.legacy_evidence_horizon_daa == 0
            {
                return Err(PalwModeV2Error::Invalid(
                    "RFC-0012 requires ConsensusV2, historical DNS rules, a complete settlement policy and a finite evidence horizon",
                ));
            }
        }
        // **ADR-0126 Decision 4: the overlay carve lowers a split this network runs, and the escrow it
        // grows must fit the worker base that lower split leaves** — the invariant the bundle's own
        // carve is refused on below, at the fence's numbers. A fence that hashes and cannot fire, or
        // fires into an escrow the block that earned it cannot fund, is refused before a peer is
        // dialed.
        if let Some(carve) = self.palw_overlay_carve
            && carve.activation != ForkActivation::never()
        {
            let Some(dns) = self.dns_params.as_ref() else {
                return Err(PalwModeV2Error::Invalid(
                    "palw_overlay_carve is armed on a network that runs no validator overlay: the split it lowers is the \
                     overlay's (ADR-0126 Decision 4)",
                ));
            };
            if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_overlay_carve is armed on a network that is not ConsensusV2: the escrow it grows is a V2 claim's \
                     (ADR-0126 Decision 4)",
                ));
            }
            if carve.activation.daa_score() < dns.full_reward_split_daa_score {
                return Err(PalwModeV2Error::Invalid(
                    "palw_overlay_carve activates below the overlay's full reward split: it lowers the full split, and below \
                     full_reward_split_daa_score the bootstrap split is in force (ADR-0126 Decision 4)",
                ));
            }
            let full = &dns.reward_params.fee_split;
            if carve.subsidy_validator_bps > full.subsidy_validator_bps {
                return Err(PalwModeV2Error::Invalid(
                    "palw_overlay_carve raises the validator share above the network's full split: the fence only lowers it \
                     (ADR-0126 Decision 4)",
                ));
            }
            if carve.worker_carve_permille > 1000 {
                return Err(PalwModeV2Error::Invalid("palw_overlay_carve escrows more than the whole subsidy (ADR-0126 Decision 4)"));
            }
            let worker_base_bps = 10_000u32
                .saturating_sub(u32::from(full.subsidy_worker_inclusion_bps))
                .saturating_sub(u32::from(carve.subsidy_validator_bps))
                .saturating_sub(u32::from(full.subsidy_service_bps));
            if u32::from(carve.worker_carve_permille) * 10 > worker_base_bps {
                return Err(PalwModeV2Error::Invalid(
                    "palw_overlay_carve's worker carve exceeds the worker base its lowered split leaves: the escrow could not \
                     be funded from the block it is carved from (ADR-0126 Decision 4)",
                ));
            }
            // Admission's collateral gate (`min_slash_permille_of_escrow`) sizes a claim's escrow at
            // the bundle's carve and does not read this fence. It is zero on every bundle, where it
            // checks nothing; armed beside the fence it would back a grown escrow as if it had not
            // grown, so the pair is refused until admission reads the carve as well.
            if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
                && bundle.state.min_slash_permille_of_escrow() != 0
            {
                return Err(PalwModeV2Error::Invalid(
                    "palw_overlay_carve is armed beside a non-zero min_slash_permille_of_escrow: admission sizes the \
                     escrow it backs at the bundle's carve, not the fence's (ADR-0126 Decision 4)",
                ));
            }
        }
        // **ADR-0130: the exposure floor raises a seat's reservation, so it must have a seat economy
        // to raise and a number that means something.** Armed without the panel economy (or below
        // its height) the floor would hash into the fingerprint and reserve nothing — no duty row is
        // written there — and a zero or runaway multiple is a fence that either does nothing or
        // prices every bond out of every panel. Refused before a peer is dialed.
        if let Some(floor) = self.palw_panel_exposure_floor
            && floor.activation != ForkActivation::never()
        {
            if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_panel_exposure_floor is armed on a network that is not ConsensusV2: the reservation it floors is a \
                     V2 seat's (ADR-0130)",
                ));
            }
            let Some(economy) = self.palw_panel_economy.filter(|economy| *economy != ForkActivation::never()) else {
                return Err(PalwModeV2Error::Invalid(
                    "palw_panel_exposure_floor is armed without palw_panel_economy: no seat holds exposure to floor, so the \
                     fence would hash and reserve nothing (ADR-0130)",
                ));
            };
            if floor.activation.daa_score() < economy.daa_score() {
                return Err(PalwModeV2Error::Invalid(
                    "palw_panel_exposure_floor activates below palw_panel_economy: between the two heights no seat is on duty, \
                     so the floor would be in force and floor nothing (ADR-0130)",
                ));
            }
            if floor.reward_multiple_permille == 0 {
                return Err(PalwModeV2Error::Invalid(
                    "palw_panel_exposure_floor's reward_multiple_permille is zero: that is no floor, and a fence that changes \
                     nothing must not move the fingerprint (ADR-0130)",
                ));
            }
            if floor.reward_multiple_permille > crate::palw_panel_economy_v1::PALW_PANEL_REWARD_MULTIPLE_MAX_PERMILLE_V1 {
                return Err(PalwModeV2Error::Invalid(
                    "palw_panel_exposure_floor's reward_multiple_permille is past 100,000 ‰: a seat reserving more than a \
                     hundred times its most prices every real bond out of every panel (ADR-0130)",
                ));
            }
        }
        // **ADR-0152-adjacent: the Activation Pool, R1 and R2 (user decision 2026-09-25) are armed at
        // genesis or not at all.** R1 and R2 change how every existing class is reclaimed and stepped,
        // and the pool's rows open at registration: a later crossing would reclaim or keep a class by
        // two rules either side of one height, with no drill that crosses it. They read the registry
        // rows (`palw_model_registry`), ADR-0147's jury and its `Candidate` state
        // (`palw_admission_independence`) and the P-B1 refund route and payout cap
        // (`palw_audit_2026_09_23`), so all three must be armed at genesis beside it.
        if let Some(pool) = self.palw_activation_pool
            && pool.activation != ForkActivation::never()
        {
            if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_activation_pool is armed on a network that is not ConsensusV2: its rows and rules are the V2 \
                     registry's (ADR-0152-adjacent: Activation Pool)",
                ));
            }
            if pool.activation.daa_score() != 0 {
                return Err(PalwModeV2Error::Invalid(
                    "palw_activation_pool may only be armed at genesis (DAA 0): R1 and R2 change how every class is reclaimed \
                     and stepped, and a crossing would reclaim one class by two rules (ADR-0152-adjacent: Activation Pool)",
                ));
            }
            let at_genesis = |f: Option<ForkActivation>| f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() == 0);
            if !(at_genesis(self.palw_model_registry)
                && at_genesis(self.palw_admission_independence)
                && at_genesis(self.palw_audit_2026_09_23))
            {
                return Err(PalwModeV2Error::Invalid(
                    "palw_activation_pool is armed without palw_model_registry, palw_admission_independence and \
                     palw_audit_2026_09_23 all armed at genesis: the pool reads the registry's rows, ADR-0147's jury and \
                     the P-B1 refund route (ADR-0152-adjacent: Activation Pool)",
                ));
            }
            // The fix round's L5: the rest of what the rules read, at genesis too — the execution lane
            // (the span R2 staggers and (a) dates by, and the seed anchor the jury is drawn from),
            // readiness V2 (the possession proof (a) pays and F4 dates), the panel economy (the panel
            // floor (a) asks and the credited seats (b) pays) and one-bond-per-operator (the unit (a)
            // and (b) pay once each).
            if !(at_genesis(self.palw_execution_lane.map(|lane| lane.activation))
                && at_genesis(self.palw_readiness_v2)
                && at_genesis(self.palw_panel_economy)
                && at_genesis(self.palw_operator_id_unique))
            {
                return Err(PalwModeV2Error::Invalid(
                    "palw_activation_pool is armed without palw_execution_lane, palw_readiness_v2, palw_panel_economy and \
                     palw_operator_id_unique all armed at genesis: the pool dates by the lane's spans, pays verified \
                     possession, asks the panel floor and pays each operator once (ADR-0152-adjacent: Activation Pool)",
                ));
            }
            if let Some(why) = pool.terms.refusal() {
                return Err(PalwModeV2Error::Invalid(why));
            }
        }
        // The readiness-V2 horizon (user decision 2026-09-25, readiness capacity option (a)): genesis
        // only, over the registry and readiness V2 at genesis, inside [8, 30] spans, mirrored.
        self.validate_palw_readiness_v2_max_age_v1()?;
        // Lane F1 (registry resilience, V03/V05): over R-core+ and the registry, mirrored.
        self.validate_palw_registry_resilience_v1()?;
        // Lane F2 (post-launch): a draw the eligible population refuses re-anchors — over R-core+, mirrored.
        self.validate_palw_floor_refusal_retry_v1()?;
        // Lane maturity (post-launch, 2026-09-26): ADR-0065 D1 brought forward, on both paths.
        self.validate_palw_bond_maturity_early_v1()?;
        // Lane bind-deadlock (post-launch): ConsensusV2 with R-core+ at or below it, on both paths.
        self.validate_palw_anchor_at_ceiling_v1()?;
        // Lane accept-order (post-launch): ConsensusV2 with the execution lane at or below it, on both paths.
        self.validate_palw_lane_accept_parents_first_v1()?;
        // Lane cap-weight (ADR-0160 F-W): over R-core+ and the strict-win reorg rule, mirrored.
        self.validate_palw_capacity_weight_cap_v1()?;
        // FP Job V4 (RFC-0001 §A): ConsensusV2 over the derivation, mirrored — on both paths.
        self.validate_palw_fp_decode_rules_v1()?;
        let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else {
            // ADR-0152 R-core+ is asked LAST on both paths, so every older fence's own refusal —
            // the prerequisites' rules — names itself before R-core+ names the missing prerequisite.
            // ADR-0151's stated maturity after it (user decision 2026-09-25): a value riding a fence
            // the prerequisites name first.
            self.validate_palw_rcore_plus_v1()?;
            self.validate_palw_exec_quantum_maturity_v1()?;
            // Lane F1 (the panel seed, post-launch): over R-core+, which names its own refusals first.
            self.validate_palw_panel_seed_execution_v1()?;
            // Lane V02 (post-launch): over R-core+, mirrored.
            self.validate_palw_final_lock_full_collateral_v1()?;
            // Lane V02's lock life (post-launch): over R-core+, mirrored.
            self.validate_palw_final_lock_life_v1()?;
            // Lane F2-lock (post-launch): the lock life applied retroactively — over lane V02's, mirrored.
            self.validate_palw_final_lock_life_retro_v1()?;
            // Lane A (the operator anchor, post-launch): over R-core+, likewise.
            self.validate_palw_operator_anchor_v1()?;
            // ADR-0160 lane liab (F-L): over R-core+ and the attribution fence, likewise.
            self.validate_palw_capacity_liability_v1()?;
            // ADR-0170's window: last on this path too (see the ConsensusV2 tail below); a ruleset with no V2 bundle refuses it by name.
            self.validate_palw_anchor_window_v1()?;
            self.validate_palw_receipt_spend_v4()?;
            self.validate_palw_evidence_court_v1()?;
            self.validate_palw_audit_1004_v1()?;
            // ADR-0178 on this path too: rule E's fence and the DNS retirement's order.
            return if private_rule_e {
                self.validate_palw_fork_choice_rule_e_order_v1()
            } else {
                self.validate_palw_fork_choice_rule_e_v1()
            };
        };
        bundle.validate()?;
        // **ADR-0103 Decision 1: a ladder past the bisection's clock is a network on which no
        // bisection is ever played.** `bundle.validate` admits one only over the COMPLETE_V4 set and
        // only where the held clock fits; what it cannot see is whether the regime covers every
        // height. A fence armed later leaves the heights before it playing a bisection this window
        // cannot hold — so the ladder may pass the clock only on a network minted with the fence.
        let bisection_fits = bundle.court.worst_case_duration_daa().is_some_and(|worst| worst < bundle.state.window_court());
        let held_from_genesis = self.palw_held_context.is_some_and(|f| f != ForkActivation::never() && f.is_active(0));
        if !bisection_fits && !held_from_genesis {
            return Err(PalwModeV2Error::Invalid(
                "the court's ladder is past the bisection's clock and palw_held_context is not armed from genesis: a bisection \
                 at any height could not finish inside window_court (ADR-0103 Decision 1)",
            ));
        }
        // The same for the advertised prompt cap: past the wire frame's, a job's ids cannot ride,
        // and before a later fence they would have to.
        if bundle.freeprompt.prompt_cap_is_past_the_frame_v1() && !held_from_genesis {
            return Err(PalwModeV2Error::Invalid(
                "the free-prompt cap is past the wire frame's and palw_held_context is not armed from genesis: the ids of a \
                 prompt that long cannot ride a commitment (ADR-0103 Decision 4)",
            ));
        }
        // **A V2 network's genesis is minted at the ambient target, because the class lottery is
        // its throttle** (mainnet audit, 2026-09-05).
        //
        // ADR-0066 states the doctrine — "a V2 network runs at MAX because the class lottery, not
        // the hash target, is its throttle" — and ADR-0083 makes the retarget answer
        // `max_difficulty_target` for a window with no bits-priced row. Both PALW presets mint
        // their genesis at that value (`PALW_RC_GENESIS`, `DEVNET_GENESIS`: `bits = 0x207fffff`).
        // `MAINNET_GENESIS` carries `0x1f7fffff` — one compact exponent, 256x harder — inherited
        // from the hash lineage, where a launch-hashrate ramp is the right thing to want.
        //
        // Carding mainnet is a re-mint (the bonds' collateral enters the premine, which moves the
        // genesis), so its `bits` is a value the ceremony chooses and nothing told it to choose
        // this one. Left at the hash lineage's, the chain is born needing 256x the inferences per
        // block for the whole fixed-difficulty launch window `min_difficulty_window_size` buys
        // (150 blocks, ~5 h) before a single retarget can answer — the same shape as the wedge
        // ADR-0083 was written for, arrived at from the other side and on the one network where
        // "wait for the DAA to fix it" costs real money.
        //
        // A rule rather than a runbook note, for the reason the fence checks below are: it is
        // exactly the kind of constant a mint gets right by remembering.
        {
            let ambient = self.max_difficulty_target.compact_target_bits();
            if self.genesis.bits != ambient {
                return Err(PalwModeV2Error::Invalid(
                    "a ConsensusV2 genesis is minted at a target other than this network's ambient maximum: a V2                      network is throttled by the class lottery, not by the hash target, so a harder genesis prices                      the attempt lanes off the chain for the whole fixed-difficulty launch window",
                ));
            }
        }
        // **ADR-0082 Decision 3: an armed dissection court must be able to derive its arity from
        // the ruleset it judges under, and it is checked HERE, at assembly.** The derivation
        // (`palw_court_v2::palw_court_params_at_v2`) answers `None` when no legal arity fits the
        // court window — a window that cannot hold its own dispute — and `None` is a refusal,
        // never a fallback to the binary court (two is a value the derivation can legitimately
        // return, so a fallback would be indistinguishable from a window that really fits). A
        // node that refused only when it came to JUDGE would run, accept claims, and discover at
        // the first dispute that its court has no shape; refusing to assemble is the only way
        // this is a rule rather than a runbook note. Dormant, the ruleset is untouched.
        if self.palw_kary_court_fence().is_some() {
            // ADR-0103 Decision 5: on a network minted with the held regime no session plays the
            // leaf ladder, so the court it judges under from genesis derives its arity with the
            // ladder's rounds at zero — the same derivation `palw_court_params_at` runs there.
            let held_from_genesis = self.palw_held_context.is_some_and(|f| f != ForkActivation::never() && f.is_active(0));
            let Ok(derived) = crate::palw_court_v2::palw_court_params_held_at_v2(bundle, true, held_from_genesis) else {
                return Err(PalwModeV2Error::Invalid(
                    "palw_kary_court is armed and no dissection arity fits this ruleset's court window: the window \
                     cannot hold its own dispute, so the court the fence names has no shape",
                ));
            };
            // A held regime armed LATER changes the derivation at its height; the frozen field is
            // one number, so the court past the fence must derive the same one (or the gate prices a
            // class for an arity the ruleset id does not name).
            if self.palw_held_context.is_some_and(|f| f != ForkActivation::never()) && !held_from_genesis {
                let held = crate::palw_court_v2::palw_court_params_held_at_v2(bundle, true, true).map_err(|_| {
                    PalwModeV2Error::Invalid(
                        "palw_held_context is armed and no dissection arity fits this ruleset's court window with the ladder at \
                         zero (ADR-0103 Decision 5)",
                    )
                })?;
                if held.dissection_arity() != bundle.court.dissection_arity() {
                    return Err(PalwModeV2Error::Invalid(
                        "palw_held_context arms after genesis and the arity the held court derives is not the ruleset's frozen \
                         dissection_arity: mint the regime from genesis, or at an arity both clocks derive (ADR-0103 Decision 5)",
                    ));
                }
            }
            // **ONE spelling of the arity, checked where the ruleset is assembled** (audit D H-1).
            //
            // `palw_court_params_at_v2` OVERWRITES `court.dissection_arity` with the derived value,
            // so past this fence the court that plays a dispute and the field the ruleset froze are
            // two statements about one rule — and nothing compared them. The admission gate reads
            // the caller's ruleset reading (`PalwKaryCourtV1::dissection_arity`, this derivation's
            // answer) and the fold reads the arity off the object; the stored field is what a
            // genesis tool writes and what `palw_ruleset_id_v2` hashes. If the two differ, a class
            // is admitted priced for one court and prosecuted in another — under-priced by a factor
            // of `k/2` in disclosure bytes on a close ceiling it was admitted under.
            //
            // No shipped genesis builder writes the derived value in (every preset literal is
            // `PALW_COURT_BINARY_ARITY_V1`), so arming this fence on a ruleset whose derivation
            // answers anything but 2 is a re-mint: the bundle must carry the arity it derives.
            // Refusing to ASSEMBLE is the only way that is a rule rather than a runbook note.
            if derived.dissection_arity() != bundle.court.dissection_arity() {
                return Err(PalwModeV2Error::Invalid(
                    "palw_kary_court is armed and this ruleset's frozen dissection_arity is not the arity it \
                     derives: the gate that admits a class would price it for one court and the court that plays \
                     its dispute would deal another — re-mint the bundle carrying the derived arity",
                ));
            }
            // **ADR-0082 Decision 1 at the OTHER door** (audit D H-3). A fused row can also arrive
            // by being minted at genesis, and genesis registrations are verified against the
            // committed catalog rather than through `verify_class_admission_*` — so
            // `FusedAttentionNeedsTheKaryCourt`, the refusal that keeps a class the court cannot
            // try out of the catalog, has no door on that route. The walk below is the same one
            // the arity derivation uses, so "does this ruleset register a fused row" is answered
            // once. Here the fence is armed, so the row has a court and nothing is refused; the
            // dormant half is the check after this block.
        } else if crate::palw_court_v2::palw_attn_widest_registered_site_v2(bundle).0 > 0 {
            return Err(PalwModeV2Error::Invalid(
                "this ruleset's genesis set registers a fused-attention (graph-v5) class and palw_kary_court is \
                 dormant: a court with no dissection cannot try that leaf, so every dispute of that class would \
                 end unprosecuted at the price it was admitted for (ADR-0082 Decision 1, ADR-0049 Decision C)",
            ));
        }
        self.validate_palw_model_court_window()?;
        // **ADR-0082 Decisions 10 and 11 — the audit D M-1 / audit C M-5 refusal is LIFTED** (RFC-0001
        // §A, lane fp-sampler, 2026-09-27). It stood because the transition answered
        // `FreePromptDecodeLeavesUnavailable` past the fence and no engine implemented
        // `decode_token_select_v2`. Both now exist, with FP Job V4: the transition credits decode
        // work over the derived graph, every free-prompt engine and seat selects through
        // `PalwFpDecoderV1`, and the doors admit one job version per height. What arming still
        // requires is `validate_palw_fp_decode_rules_v1`'s, asked on both paths above.
        // **ADR-0096 Decisions 6–8: the refusal is LIFTED** (RFC-0001 §2.5, lane U, 2026-10-03). It stood because no job could
        // carry a constraint, no engine masked a decode and no seat replayed one. This build carries the constrained job
        // (FP job version 6, `crate::palw_fp_constraint_job_v1`), the mask in every producer's driver and every seat's
        // replay rule, and the court's third arm as a pure check (`check_tiled_decode_token_refutation_v3`). What arming
        // still requires is the fence's own prerequisites below. (The court arm is not yet a proof kind on the wire: a
        // lying constrained claim is stopped by the seat quorum, and convicted only where a challenger files the existing
        // decode-token refutation — see ADR-0096 and the lane report.)
        if let Some(activation) = self.palw_fp_decode_constraint
            && activation != ForkActivation::never()
            && !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_))
        {
            return Err(PalwModeV2Error::Invalid("palw_fp_decode_constraint is armed on a network that is not ConsensusV2"));
        }
        {
            let mirror = match &self.palw_consensus_mode {
                PalwConsensusMode::ConsensusV2(bundle) => bundle.state.fp_decode_constraint_from_daa(),
                _ => None,
            };
            let armed = self.palw_fp_decode_constraint.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
            if mirror != armed && matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_fp_decode_constraint disagrees with the V2 bundle's mirror: mirror it with \
                     Params::sync_palw_fp_decode_constraint_v1 after the bundle is assembled",
                ));
            }
        }
        // **ADR-0081 Decision 3 / ADR-0082 Decision 5: the prompt-commitment form is a property
        // of the GENESIS, never of a height** (private-prompts design, 2026-09-05).
        //
        // Every writer of a `prompt_token_ids_hash` — the worker, the panel's canonical job, the
        // raw lane's `from_envelope`, the three backends — and every reader — the payload decoders,
        // the seat's admission, the tx door, the extraction walk, the court's two arithmetic arms —
        // takes the form from `palw_prompt_ids_form_v1`, which is `palw_prompt_ids_form_at(0)`. A
        // fence that flipped mid-chain would make the form a function of each job's anchor height,
        // which none of those readers holds (a decoder sees bytes, not a block), so the only
        // coherent arming is from block one. The bundle's `trace_format_version` says the same
        // thing inside `palw_ruleset_id_v2` (3 flat, 4 Merkle): two networks cannot share a ruleset
        // id and hash their prompts differently, and a ruleset cannot claim one form while its
        // fence says the other.
        if let Some(activation) = self.palw_prompt_ids_merkle
            && activation != ForkActivation::never()
        {
            if activation != ForkActivation::always() {
                return Err(PalwModeV2Error::Invalid(
                    "palw_prompt_ids_merkle may only be armed at genesis: the prompt-commitment form is decided once per \
                     network, never per height, because no reader of a prompt_token_ids_hash holds the job's anchor height",
                ));
            }
            if bundle.trace_format_version != crate::palw_mode_v2::PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS {
                return Err(PalwModeV2Error::Invalid(
                    "palw_prompt_ids_merkle is armed but the bundle's trace_format_version is not the Merkle-ids format (4): \
                     the ruleset id would not say what every job on the network is called",
                ));
            }
        } else if bundle.trace_format_version == crate::palw_mode_v2::PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS {
            return Err(PalwModeV2Error::Invalid(
                "the bundle's trace_format_version is the Merkle-ids format (4) but palw_prompt_ids_merkle is dormant: the \
                 ruleset id claims a prompt commitment the fence does not arm",
            ));
        }
        // **ADR-0042 Decision 11: the fence and the committed set are written from one fact, and
        // neither half may stand alone** (mainnet audit 2026-09-06, M-8). Same shape and same
        // reasons as the block above it. Genesis-only for the stronger of those reasons: a signing
        // context is what a signature MEANS, every verifier of one holds bytes rather than a block
        // height, and a set that changed mid-chain would make the meaning of a signature a
        // function of a height none of them can see.
        if let Some(activation) = self.palw_signature_contexts_v2
            && activation != ForkActivation::never()
        {
            if activation != ForkActivation::always() {
                return Err(PalwModeV2Error::Invalid(
                    "palw_signature_contexts_v2 may only be armed at genesis: the set of contexts a ruleset commits to is \
                     decided once per network, never per height, because no verifier of a signature holds the object's height",
                ));
            }
            if bundle.signature_contexts_root != crate::palw_mode_v2::palw_v2_signature_contexts_root_v2()
                && !crate::palw_mode_v2::palw_v2_signature_contexts_root_covers_v3(&bundle.signature_contexts_root)
            {
                return Err(PalwModeV2Error::Invalid(
                    "palw_signature_contexts_v2 is armed but the bundle's signature_contexts_root is not a complete set (V2 or \
                     V3): the ruleset id would not name every context a signature on this network is verified under",
                ));
            }
        } else if bundle.signature_contexts_root != crate::palw_mode_v2::palw_v2_signature_contexts_root() {
            return Err(PalwModeV2Error::Invalid(
                "the bundle's signature_contexts_root is the complete set but palw_signature_contexts_v2 is dormant: the \
                 ruleset id claims a context set the fence does not arm",
            ));
        }
        // **ADR-0099 Decision 5, built (ADR-0100): the one-move court is a consensus object, and
        // what still gates arming it is the ruleset move its signing context needs.** The
        // acceptance layer verifies `ShardCourtAccused` under the shard court's context, which is
        // in the COMPLETE_V3 set and in no set a live network committed to. A fence armed over a
        // bundle whose committed set lacks the context would verify signatures under a context the
        // ruleset id does not name — audit M-8's defect, re-created on purpose. `never()` is
        // absence and is exempt.
        if let Some(activation) = self.palw_shard_court
            && activation != ForkActivation::never()
            && !crate::palw_mode_v2::palw_v2_signature_contexts_root_covers_v3(&bundle.signature_contexts_root)
            // ADR-0118 Decision 2: a held fence armed past genesis at or below the court's height
            // commits V4 ⊇ V3 there (Decision 1), so the accusation's context is named from the
            // court's first block. (The held regime requires the court at or below ITS height, so
            // the two land on one height.)
            && !self.palw_held_context.is_some_and(|held| {
                held != ForkActivation::never() && !held.is_active(0) && held.daa_score() <= activation.daa_score()
            })
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_shard_court is armed but the bundle's signature_contexts_root is not the COMPLETE_V3 set: the accusation's \
                 signing context (misaka-palw/shard-court/accuse/mldsa87/v1) is not one this network's ruleset id commits to — \
                 a network arms the one-move court by stating the V3 set at genesis (palw_signature_contexts_v2 over the V3 root)",
            ));
        }
        // **ADR-0100 Decision 4: per-shard licensing needs the same ruleset move, and the court.**
        if let Some(activation) = self.palw_shard_licensing
            && activation != ForkActivation::never()
        {
            if !crate::palw_mode_v2::palw_v2_signature_contexts_root_covers_v3(&bundle.signature_contexts_root) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_shard_licensing is armed but the bundle's signature_contexts_root is not the COMPLETE_V3 set: the \
                     shard plan and a bond's shard list are signed under contexts only V3 commits to",
                ));
            }
            let court_by_then = self
                .palw_shard_court
                .is_some_and(|court| court != ForkActivation::never() && court.daa_score() <= activation.daa_score());
            if !court_by_then {
                return Err(PalwModeV2Error::Invalid(
                    "palw_shard_licensing is armed without palw_shard_court armed at or below the same height: a panel whose \
                     seats each hold a shard could license a lie no seat of it could convict (ADR-0098 Decision 5)",
                ));
            }
        }
        // **ADR-0103: the held regime arms over the set that names its contexts, with the one-move
        // court and the k-ary court under it, on a network whose prompt ids open a tile at a time.**
        // Each condition is a thing the regime stands on rather than a preference: its three
        // objects are verified under V4's contexts (audit M-8's rule), it refuses the bisection so
        // the one-move court must already convict, its dissection is the k-ary court's, and a held
        // class's ids never ride — a flat digest would make every seat and every close carry them.
        if let Some(activation) = self.palw_held_context
            && activation != ForkActivation::never()
        {
            // **ADR-0118 Decision 1: a held fence armed past genesis commits the V4 contexts itself.**
            // From genesis the bundle is being minted and states V4 (`palw_held_context_mint_v1`).
            // At a height on a network minted before the regime existed, the objects the regime
            // verifies under V4's contexts did not exist before the fence, so no signature's meaning
            // changes at it; and the fingerprint writes the V4 root beside the fence's height, so
            // two builds that spell a V4 context differently cannot share an identity — the M-8 rule,
            // kept by the fence rather than by the bundle.
            // ADR-0152 IMPL-7: V5 is V4 plus R-core+'s contexts, so it satisfies this gate as a superset.
            if !crate::palw_mode_v2::palw_v2_signature_contexts_root_covers_v4(&bundle.signature_contexts_root)
                && activation.is_active(0)
            {
                return Err(PalwModeV2Error::Invalid(
                    "palw_held_context is armed from genesis but the bundle's signature_contexts_root is not the COMPLETE_V4 \
                     set: the checkpoint court and the held DA court sign under contexts only V4 commits to (ADR-0103)",
                ));
            }
            // **ADR-0119 Decision 1: no deeper than the regime's own ladder.** Under the regime every
            // fused leaf of every class is tried by a dissection whose session is a bisection ladder
            // over the claim's ladder, and no bisection ladder opens past `2^40`; a held class's is
            // exactly that, and a network ladder past it would leave every other fused class's
            // dissection unopenable.
            if bundle.court.max_step_leaf_count() > crate::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1 {
                return Err(PalwModeV2Error::Invalid(
                    "palw_held_context is armed over a court ladder past 2^40: a dissection's session is a bisection ladder over \
                     the claim's ladder, and no bisection ladder opens past that (ADR-0119 Decision 1)",
                ));
            }
            let by_then = |fence: Option<ForkActivation>| {
                fence.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= activation.daa_score())
            };
            if !by_then(self.palw_kary_court) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_held_context is armed without palw_kary_court at or below its height: the held regime's \
                     dissection is the k-ary court's (ADR-0103 Decision 5)",
                ));
            }
            if !by_then(self.palw_shard_court) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_held_context is armed without palw_shard_court at or below its height: the held regime refuses the \
                     bisection, so the one-move court must already convict (ADR-0103 Decision 1)",
                ));
            }
            // ADR-0118 Decision 3: past genesis, on a network minted flat, a held class carries its
            // own Merkle form (`palw_prompt_ids_form_of_class_v1`) and every other class keeps the
            // network's. From genesis the mint states Merkle for the whole network, as before.
            if self.palw_prompt_ids_form_v1() != crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1 && activation.is_active(0) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_held_context is armed from genesis on a network whose prompt ids are flat: a genesis that takes the \
                     regime mints its prompt ids Merkle, since a held class's ids never ride and a flat digest cannot be opened \
                     a tile at a time (ADR-0103 Decision 4; trace format 4) — armed past genesis, a held class commits its own \
                     Merkle form (ADR-0118 Decision 3)",
                ));
            }
            // The ids never ride above one standard transaction, so a held class's widest job must
            // be able to commit with none — `PanelDa`, the only mode whose commitment carries the
            // digest alone. Without it every held class reads its payload wall `Linear` and the
            // gate refuses them all by that name: a fence that could admit nothing.
            if !by_then(self.palw_panel_da) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_held_context is armed without palw_panel_da at or below its height: a held class's widest job commits \
                     with no ids on the chain, which only PanelDa allows (ADR-0103 Decision 4)",
                ));
            }
            // **ADR-0111 Decision 5: the held regime needs the data-availability court.** A seat
            // that holds no capture names a leaf and must be able to demand its evidence; without the
            // court that demand does not exist, and an executor that withholds one leaf is named and
            // never convicted. The pins make a free-prompt claim's retention a chain fact — without
            // them the producer writes its own, and a producer that writes `0` is immune.
            if !by_then(self.palw_da_court) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_held_context is armed without palw_da_court at or below its height: a seat that holds no capture \
                     demands a leaf's evidence through it, and without it a withheld leaf is unprosecutable (ADR-0111 Decision 5)",
                ));
            }
            if !by_then(self.palw_fp_da_pins) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_held_context is armed without palw_fp_da_pins at or below its height: a free-prompt claim's retention \
                     would be the producer's own number, and a demand could never fit inside it (ADR-0111 Decision 5)",
                ));
            }
        }
        // **ADR-0102 at the genesis door.** A genesis row is verified against the committed
        // catalog, not through `verify_class_admission_*`, so `TokenLiftNeedsItsFence` has no door
        // there — the ADR-0082 fused-row precedent above.
        // **ADR-0103 at the genesis door**, for the same reason: a held class in the genesis set
        // is admitted by nothing but this.
        if crate::palw_class_admission_v2::palw_genesis_registers_held_class_v1(bundle)
            && !self.palw_held_context.is_some_and(|f| f != ForkActivation::never() && f.is_active(0))
        {
            return Err(PalwModeV2Error::Invalid(
                "this ruleset's genesis set registers a class under the held map and palw_held_context is not armed from \
                 genesis: the map lifts the context ceiling only where the regime is (ADR-0103)",
            ));
        }
        if crate::palw_class_admission_v2::palw_genesis_reaches_fenced_kernel_v1(bundle)
            && !self.palw_token_lift.is_some_and(|f| f != ForkActivation::never() && f.is_active(0))
        {
            return Err(PalwModeV2Error::Invalid(
                "this ruleset's genesis set registers a class reaching the per-token lift kernel and palw_token_lift is \
                 not armed from genesis: that kernel is adjudicated only where its fence is (ADR-0102)",
            ));
        }
        // **RFC-0002 Phase F: no IR class at genesis before Phase H.** A genesis row is checked
        // against the committed catalog, not by admission v10, and the catalog has no IR entry form.
        if crate::palw_tir_class_v1::palw_genesis_registers_tir_class_v1(bundle) {
            return Err(PalwModeV2Error::Invalid(
                "this ruleset's genesis set registers an IR class: IR genesis rows arrive with RFC-0002 Phase H",
            ));
        }
        // **RFC-0003: no generative class at genesis.** A genesis row is checked against the
        // committed catalog, which has no pipeline entry form; a generative class registers past
        // `palw_gen_v1`, through the pipeline admission.
        if crate::palw_gen_class_v1::palw_genesis_registers_gen_class_v1(bundle) {
            return Err(PalwModeV2Error::Invalid(
                "this ruleset's genesis set registers a generative class: RFC-0003 v1 has no generative genesis rows",
            ));
        }
        if crate::palw_class_admission_v2::palw_genesis_reaches_kimi_kernel_v1(bundle)
            && !self.palw_kimi_k3.is_some_and(|f| f != ForkActivation::never() && f.is_active(0))
        {
            return Err(PalwModeV2Error::Invalid(
                "this ruleset's genesis set registers a class reaching a Kimi K3 kernel and palw_kimi_k3 is not armed from \
                 genesis",
            ));
        }
        // **P0a at the genesis door**, the ADR-0102 precedent: genesis rows are verified against the
        // committed catalog rather than through the admission gate, so a version-3 row needs the
        // fence armed from genesis or it would be the one class the fence exists to admit, admitted
        // without it.
        if crate::palw_class_admission_v2::palw_genesis_registers_key_head_class_v1(bundle)
            && !self.palw_gdn_key_heads.is_some_and(|f| f != ForkActivation::never() && f.is_active(0))
        {
            return Err(PalwModeV2Error::Invalid(
                "this ruleset's genesis set registers a version-3 profile (the derived GDN key-head count, P0a) and \
                 palw_gdn_key_heads is not armed from genesis",
            ));
        }
        // **ADR-0093 Decision 6 at the genesis door**, the same reason: a fence armed from genesis
        // judges the genesis rows too, or its first class would be the one it exists to refuse.
        if self.palw_fused_dissectable.is_some_and(|f| f != ForkActivation::never() && f.is_active(0))
            && crate::palw_class_admission_v2::palw_genesis_holds_undissectable_fused_class_v1(bundle)
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_fused_dissectable is armed from genesis and this ruleset's genesis set registers a fused class whose \
                 output tile no dissection can try (ADR-0093 Decision 6)",
            ));
        }
        if self.palw_credit.is_some()
            || self.palw_fork_choice.is_some()
            || self.palw_schedule.is_some()
            || self.palw_ramp.is_some()
            || self.palw_block_commitment.is_some()
        {
            return Err(PalwModeV2Error::Invalid("a ConsensusV2 network may not set any V1 PALW fence"));
        }
        // **ADR-0065 D1: the arming height must be past its own window.** The draw is fail-closed
        // — a short panel is `InsufficientEligibleBonds`, so the claim never binds and voids at
        // `BindTimeout` — and the genesis registers exactly `seat_count + 1` bonds while the draw
        // excludes the executor, leaving zero slack. Genesis bonds carry `registered_daa =
        // genesis.daa_score`, so a fence armed at a height below the window would make every one of
        // them immature at once: no claim could bind, `safe_frontier` would never leave 0, and
        // pruning would never start. Refusing the configuration is the only way this is a rule
        // rather than a note in a runbook somebody has to remember.
        // **ADR-0066 Decision 1: the fence may only declare a price this binary can compute.**
        //
        // `work_log2` is in `consensus_params_id`, so it is what operators coordinate on; the
        // target actually applied lives in `StateLayer0::new` as a constant, because the PoW path
        // has no params. Two sources for one number is exactly how a value gets announced rather
        // than enforced, so they are compared here and a mismatch refuses to start — the same
        // stance `check_algo_id_known` takes for an algorithm the binary cannot derive.
        if let Some(heartbeat) = self.palw_heartbeat
            && heartbeat.activation != ForkActivation::never()
            && heartbeat.work_log2 != crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_heartbeat declares a work_log2 this binary does not implement: the lane's target is a \
                 network constant and the fence must name the same one",
            ));
        }
        // **ADR-0068 Phase 1: the same lock for the same reason, twice over.** F3a's mergeset
        // bound and F2's attempt work are both values beside a fence with a second source in the
        // enforcing code, and two sources for one number is how a value gets announced rather
        // than enforced.
        if let Some(heartbeat) = self.palw_heartbeat
            && heartbeat.activation != ForkActivation::never()
            && heartbeat.max_per_mergeset != crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_heartbeat declares a max_per_mergeset this binary does not enforce: the width bound is a \
                 network constant and the fence must name the same one",
            ));
        }
        // ADR-0125: the lane's shape must be one this binary can run. Every bound is a consensus
        // number the header, the fold and the coinbase all read, so an unrunnable value refuses to
        // start rather than splitting the first block that meets it.
        if let Some(lane) = self.palw_execution_lane
            && lane.activation != ForkActivation::never()
        {
            use crate::palw_execution_lane_v1::{PALW_EXEC_MAX_PER_MERGESET_BOUND_V1, PALW_EXEC_MAX_PERMITS_PER_ROUND_V1};
            if lane.permits_per_round == 0 || lane.permits_per_round > PALW_EXEC_MAX_PERMITS_PER_ROUND_V1 {
                return Err(PalwModeV2Error::Invalid(
                    "palw_execution_lane declares a permits_per_round outside 1..=10: a round block's permit index is \
                     bounded by the widest round this binary implements",
                ));
            }
            // §7.2: the stage table. Used slots first, heights strictly after the opening and after
            // each other, each width in range and wider than the stage before it — a lane only
            // widens, which is what lets one per-round bound (the widest stage in force) cover every
            // span a mergeset can reach back to.
            let mut previous = (lane.activation.daa_score(), lane.permits_per_round);
            let mut seen_unused = false;
            for stage in lane.widenings.iter() {
                if !stage.is_used() {
                    seen_unused = true;
                    continue;
                }
                if seen_unused {
                    return Err(PalwModeV2Error::Invalid(
                        "palw_execution_lane lists a widening after an unused slot: the stage table is a prefix",
                    ));
                }
                if stage.activation.daa_score() <= previous.0 {
                    return Err(PalwModeV2Error::Invalid(
                        "palw_execution_lane's widenings must be at heights after the lane opens and after each other",
                    ));
                }
                if stage.permits_per_round <= previous.1 || stage.permits_per_round > PALW_EXEC_MAX_PERMITS_PER_ROUND_V1 {
                    return Err(PalwModeV2Error::Invalid(
                        "palw_execution_lane's widenings must each widen the round, and no wider than 10 permits",
                    ));
                }
                previous = (stage.activation.daa_score(), stage.permits_per_round);
            }
            if lane.max_per_mergeset < lane.widest() as u64 || lane.max_per_mergeset > PALW_EXEC_MAX_PER_MERGESET_BOUND_V1 {
                return Err(PalwModeV2Error::Invalid(
                    "palw_execution_lane declares a max_per_mergeset below one round or above the bound the coinbase \
                     carries: a merging block could never hold a full round, or could owe more outputs than it may carry",
                ));
            }
            if lane.schedule_span_daa == 0 {
                return Err(PalwModeV2Error::Invalid("palw_execution_lane declares a zero schedule span"));
            }
            if lane.short_span.is_used() {
                if lane.short_span.schedule_span_daa == 0 {
                    return Err(PalwModeV2Error::Invalid("palw_execution_lane's short span is zero"));
                }
                if lane.short_span.schedule_span_daa >= lane.schedule_span_daa {
                    return Err(PalwModeV2Error::Invalid("palw_execution_lane's short span must be shorter than the opening span"));
                }
                if lane.short_span.activation.daa_score() <= lane.activation.daa_score() {
                    return Err(PalwModeV2Error::Invalid("palw_execution_lane's short span must fire after the lane opens"));
                }
                if lane.widenings.iter().any(|stage| stage.is_used() && stage.activation == lane.short_span.activation) {
                    return Err(PalwModeV2Error::Invalid(
                        "palw_execution_lane's short span must have a height of its own, not a widening's",
                    ));
                }
            }
        }
        if let Some(attempt_work) = self.palw_attempt_work
            && attempt_work.activation != ForkActivation::never()
            && attempt_work.work_log2 != crate::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_attempt_work declares a work_log2 this binary does not implement: the attempt lane's work is a \
                 network constant and the fence must name the same one",
            ));
        }
        // …and the third price, for the third time and the same reason (ADR-0071 Decision 2).
        if let Some(attempt_work) = self.palw_attempt_work
            && attempt_work.activation != ForkActivation::never()
            && attempt_work.ticket_bucket_log2 != crate::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_attempt_work declares a ticket_bucket_log2 this binary does not bucket by: how many nonces one \
                 execution buys is a network constant and the fence must name the same one",
            ));
        }
        // **ADR-0073 SA-1: an armed fold must be at least the amendment's floor.** `k` is a value
        // beside a fence, so the identity does not separate two builds that disagree about it
        // while it is merely scheduled — the same coordination hazard `palw_bond_maturity` carries.
        // What CAN be enforced locally is that a node never runs a fold too narrow to bound the
        // withholding bias the amendment exists to bound: at k = 1 the rule is the pre-SA-1 rule
        // wearing a fence, and at k = 2 the bias is `p²`, which SA-1 explicitly declines.
        if let Some(fold) = self.palw_beacon_fold
            && fold.activation != ForkActivation::never()
            && fold.k < crate::palw_fp_beacon_v3::PALW_BEACON_FOLD_MIN_K_V1
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_beacon_fold is armed with a k below ADR-0073 SA-1's floor: a fold narrower than three attempt \
                 blocks does not bound the beacon's withholding bias, and a fence that names one is the old rule \
                 wearing a new name",
            ));
        }
        if let Some(maturity) = self.palw_bond_maturity {
            if maturity.window_daa == 0 {
                return Err(PalwModeV2Error::Invalid("a zero bond maturity window is the rule switched off wearing a fence"));
            }
            let armed = maturity.activation.daa_score();
            if armed != u64::MAX && armed < self.genesis.daa_score.saturating_add(maturity.window_daa) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_bond_maturity is armed before its own window has elapsed since genesis — every genesis bond                      would be immature at once and no panel could be drawn",
                ));
            }
            // **And the registry has to have somewhere to fall back to.** The guard above proves
            // the genesis bonds are mature when the fence fires; it can say nothing about a
            // REPLACEMENT. On a registry with no spare, one bond leaving eligibility — retired, or
            // slashed under `min_collateral_sompi` — makes every panel undrawable for a whole
            // maturity window, because the replacement is itself immature. A short draw is not a
            // smaller panel, it is no panel, so that is an outage and not a degradation.
            //
            // ADR-0065 records this as "do not arm D1 on a network running at `seat_count + 1`
            // bonds". A sentence in an ADR is not a gate, so this is the gate.
            if armed != u64::MAX {
                let registered = bundle
                    .genesis_objects
                    .iter()
                    .filter(|o| matches!(o, crate::palw_state_v2::PalwConsensusObjectV2::BondRegistered { .. }))
                    .count();
                if registered < crate::palw_fp_devnet_v3::palw_v2_maturity_armable_bonds_v1() {
                    return Err(PalwModeV2Error::Invalid(
                        "palw_bond_maturity is armed on a genesis whose bond registry has no spare seat: D1 makes a \
                         replacement bond unseatable for a full window, so one departure would leave every claim unable \
                         to bind. Grow the registry to PALW_V2_PANEL_SEATS + 3 before arming it.",
                    ));
                }
            }
        }
        if self.pow_palw_activation.is_active(u64::MAX - 1) || self.pow_palw_ollama_activation.is_active(u64::MAX - 1) {
            return Err(PalwModeV2Error::Invalid("a ConsensusV2 network may not activate any V1 PALW proof-of-work"));
        }
        // **ADR-0075 SA-3: two rows of the card may not be one operator.**
        //
        // `derive_panel_v2` seats ONE bond per operator id, so two genesis rows sharing an operator
        // — the shape a card assembled from copied rows takes, and the shape a single host holding
        // two seats' keys takes — is not two seats. It is one seat and one row of dead collateral,
        // and the registry silently shrinks by one below every count this file already gates on
        // (`palw_v2_min_genesis_bonds_v1`, `palw_v2_maturity_armable_bonds_v1`), which is how an
        // apparently armable card produces a network whose claims cannot bind. The card is written
        // by hand; this is the only place that reads it before a chain exists.
        {
            let mut operators = std::collections::BTreeSet::new();
            for object in &bundle.genesis_objects {
                if let crate::palw_state_v2::PalwConsensusObjectV2::BondRegistered { operator_pubkey, .. } = object
                    && !operators.insert(crate::palw_state_v2::palw_operator_id_v2(operator_pubkey))
                {
                    return Err(PalwModeV2Error::Invalid(
                        "two genesis bonds share an operator id: the panel draw seats one bond per operator, so the \
                         registry is smaller than it looks and the seat counts this gate checks are wrong",
                    ));
                }
            }
        }
        // **The chain must still hold what the lattice will ask it for** (audit M2-20).
        //
        // Every judgement in the claim lattice is anchored on the claim block's own header, and the
        // pruning processor deletes headers outside the pruning horizon — so a lattice longer than
        // that horizon produces courts nobody can adjudicate and seats that accuse because they
        // cannot derive an anchor. The finality/anticone bound is the same class of defect one
        // level down: it was silently clamped to `pruning_depth` under a comment about test
        // networks. Both are asserted at construction, so a preset that violates either cannot be
        // built rather than failing days into a dispute.
        // TWO bind+receipt pairs: a claim whose first panel concludes nothing is revived once and
        // binds a second (`sweep_deadlines`' `PanelBound` arm), so the longest path a judgement
        // can be anchored on is the redrawn one — and it is the length the horizon has to cover.
        // ADR-0062 SA-6: `+ accuse + disclose`, zero while the fence is `None`. ADR-0152 §4-quater
        // P-1 / E-11: past `palw_class_verify_deadline` the receipt window is `max(window_receipt,
        // D_cap)` and the DA term the `R_eff` span at D_cap — the ONE lattice the depth is derived
        // from (`palw_v2_claim_lattice_daa_v1`), so a measured class (whose claims reach D_cap) can
        // never sit on a horizon that prunes what its judgements read.
        let lattice = palw_v2_claim_lattice_daa_v1(bundle, self.palw_da_court, self.palw_class_verify_deadline);
        if lattice > self.blockrate.pruning_depth {
            return Err(PalwModeV2Error::Invalid(
                "the claim lattice outlives the pruning horizon — the headers its judgements are anchored on would be deleted first",
            ));
        }
        // **ADR-0062 SA-3, proved at construction rather than assumed.**
        //
        // `palw_da_disclose_window_daa_v1` is `window_challenge`, and SA-3 requires the disclose
        // window to be at least twice the finality window so a reorg across the deadline cannot
        // flip the verdict without a finality violation. Every V2 preset is built with
        // `finality_depth = window_challenge / 2` (`with_palw_v2_depths`), so the relation holds
        // by construction — but "by construction" is a sentence, and a bundle assembled by hand
        // could break it silently. This is the gate. Only past the fence: a dormant rule has no
        // window to size.
        if self.palw_da_court.is_some() {
            let disclose = crate::palw_state_v2::palw_da_disclose_window_daa_v1(&bundle.state);
            if disclose < 2 * self.blockrate.finality_depth {
                return Err(PalwModeV2Error::Invalid(
                    "the data-availability disclose window is narrower than twice the finality window — a reorg across \
                     the deadline could flip the verdict without a finality violation (ADR-0062 SA-3)",
                ));
            }
            // **RESPONDER COVERAGE: a court may not be armed over a family that cannot answer it**
            // (mainnet audit 2026-09-06, C-5; ADR-0062 SA-2).
            //
            // SA-2 says it in the ADR's own words: "a class whose event cannot be disclosed inside
            // the budget is refused at admission rather than being undefendable afterwards." There
            // was no door on the GENESIS route, and the fence's own doc says what that costs:
            // armed with no responder, every accusation succeeds and every producer is slashable
            // for the price of one bond.
            //
            // The fact asked here is CONSENSUS-VISIBLE and is the one
            // `check_trace_event_disclosure_v1` itself refuses on ("this class commits under a
            // scheme no disclosure form names"): the class's registered `logits_scheme_id`. It is
            // deliberately NOT `PalwExecutionBackendV1::supports_court()` — that is node-local, it
            // is a bool a family writes about itself, and `palw_e2e_adjudicability`'s own module
            // doc records it as exactly the wrong shape for precisely this job.
            //
            // **A registered class this call cannot DESCRIBE is passed over, and that is a limit
            // of this site rather than a verdict about the class.** The designed patch refused it,
            // on the reasoning that a class nothing here can name is a class this rule has no
            // opinion about while the court has a verdict about it. That reasoning does not survive
            // contact with the assembly: a GENESIS class carries `admission: None` and its profile
            // lives in the CATALOG, of which the bundle holds only `class_catalog_root` — a
            // commitment, not a preimage. `palw_registered_class_profiles_v1` therefore answers for
            // a genesis row only when the typed catalog in `palw_e2e_adjudicability` happens to
            // carry it, which is true of the three rows this tree ships and false of every network
            // `palw_rc_params` assembles from a catalog of its own. Refusing here would have
            // stopped `the_shipped_palw_rc_network_boots_and_runs_its_genesis` from booting a
            // ruleset that is not undefendable at all — only undescribable from this seam.
            //
            // So the residue is named instead of hidden: past this gate, "every registered class
            // commits under a disclosable scheme" is proven for the rows the ruleset itself can
            // resolve, and asserted for the rest by whoever supplied the catalog.
            // `a_catalog_class_this_ruleset_cannot_describe_is_not_refused_here` pins that limit,
            // so it can neither widen nor close silently.
            //
            // Assembly-time only: no fold reads this, no fence is added, no fingerprint field
            // moves. Every shipped preset's registered classes declare one of the two named
            // schemes (BASE-0 flat, both model tiers tiled), so nothing that assembles today stops
            // assembling.
            for (class_id, profile) in crate::palw_e2e_adjudicability::palw_registered_class_profiles_v1(bundle) {
                let Some(profile) = profile else { continue };
                let scheme = profile.logits_scheme_id;
                if scheme != crate::palw_step_refute::flat_logits_scheme_id_v1()
                    && scheme != crate::palw_step_refute::tiled_logits_scheme_id_v1()
                {
                    return Err(PalwModeV2Error::InvalidOwned(format!(
                        "palw_da_court is armed and the genesis set registers class {class_id} under a logits commitment \
                         scheme no disclosure form names: `PalwTraceEventDisclosureV1` has a Flat arm and a Tiled arm, \
                         `check_trace_event_disclosure_v1` refuses anything else, so no `MaterialDisclosed` could ever \
                         refute an accusation against this class and every accusation would win by silence (ADR-0062 SA-2)"
                    )));
                }
            }
            // **The delay in force past the fence outlasts the DA court BY CONSTRUCTION** (mainnet
            // audit 2026-09-05, "should fix" `palw_mode_v2.rs:938`).
            //
            // There is deliberately no check here. `palw_v2_bond_withdrawal_delay_at_v1` adds
            // exactly `accuse + disclose` past the fence, so "the resolved delay outlasts
            // `liability + accuse + disclose`" reduces to "the bundle's delay outlasts the
            // liability" — which `PalwConsensusParamsV2::validate_ruleset_shape` already refuses a
            // ruleset for. A second copy of it here would be a condition no configuration can
            // fail: a check shaped to agree with its own resolver. The property is pinned where it
            // can be observed instead, in
            // `the_withdrawal_delay_in_force_outlasts_the_data_availability_court`.
        }
        let k = self.blockrate.ghostdag_k as u64;
        let anticone_bound =
            self.blockrate.finality_depth + self.blockrate.merge_depth + 4 * self.blockrate.mergeset_size_limit * k + 2 * k + 2;
        if anticone_bound > self.blockrate.pruning_depth {
            return Err(PalwModeV2Error::Invalid(
                "anticone finalization depth exceeds the pruning depth — the clamp that hides this is for test networks",
            ));
        }
        // ADR-0038 Decision H, enforced where the decision says it must be — at construction.
        //
        // Audit H2: the cadence is in neither `PalwConsensusParamsV2` nor therefore
        // `palw_ruleset_id_v2`, while every window inside the bundle is denominated in DAA score.
        // Two networks could share a ruleset id and run different rules in wall-clock terms, which
        // defeats the one property Decision 11 exists to give — that "the RC and mainnet ship the
        // same ruleset" is a hash a node checks rather than a release note it trusts. The bundle's
        // id still does not cover the cadence; this refuses the only configurations where that
        // silence would matter.
        if self.blockrate.target_time_per_block != crate::palw_mode_v2::PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS {
            return Err(PalwModeV2Error::Invalid(
                "a ConsensusV2 network must run the frozen 120 s cadence (ADR-0038 Decision H) — every window in the bundle is DAA-denominated",
            ));
        }
        // **…and `target_time_per_block` is one field of a SET, so checking it alone was the gate
        // agreeing with the bug it was meant to catch** (mainnet audit item 6).
        //
        // `ghostdag_k`, `mergeset_size_limit`, `merge_depth` and `coinbase_maturity` are all
        // derived from the same lambda, and a 10 bps base that had only its `target_time_per_block`
        // overwritten passed the check above while running a 10 bps DAG at a 120 s clock. Compared
        // against the shared constructor rather than against literals, so the two cannot drift.
        //
        // Two fields are deliberately not compared. `max_block_parents` may be WIDER (a topology
        // choice a base is allowed to make), and `finality_depth` / `pruning_depth` are raised by
        // the assembly to cover the claim lattice — both already carry their own bounds above.
        {
            let canonical = BlockrateParams::new_two_minute_bps();
            let mismatch = (self.blockrate.ghostdag_k != canonical.ghostdag_k)
                || (self.blockrate.mergeset_size_limit != canonical.mergeset_size_limit)
                || (self.blockrate.merge_depth != canonical.merge_depth)
                || (self.blockrate.coinbase_maturity != canonical.coinbase_maturity)
                || (self.blockrate.past_median_time_sample_rate != canonical.past_median_time_sample_rate)
                || (self.blockrate.difficulty_sample_rate != canonical.difficulty_sample_rate)
                || (self.blockrate.max_block_parents < canonical.max_block_parents);
            if mismatch {
                return Err(PalwModeV2Error::Invalid(
                    "a ConsensusV2 network runs the 120 s DAG parameters, not just the 120 s clock: k, mergeset, \
                     merge depth and maturity are derived from the same rate and must come from new_two_minute_bps",
                ));
            }
        }
        // **The fork that selects that cadence must already be behind us.** Mainnet activates
        // Crescendo at a finite height and runs `pre_crescendo_target_time_per_block` below it, so
        // a V2 network with a future Crescendo would produce 10 bps headers under 120 s-denominated
        // consensus windows for its entire pre-fork life.
        if self.crescendo_activation != ForkActivation::always() {
            return Err(PalwModeV2Error::Invalid(
                "a ConsensusV2 network must have Crescendo active from genesis — its cadence is the frozen one, not a fork away",
            ));
        }
        // **And the DNS windows are block counts, so they are cadence too.** Inherited from a
        // 10 bps base the "14 day" unbonding period states about 46 years at 120 s, and
        // `bond_spend_gate` enforces it in consensus — validator collateral confiscated by a unit
        // mismatch. Stated as a fixed point of `at_two_minute_cadence` so the gate and the
        // assembly cannot disagree about what the right table is.
        if let Some(dns) = self.dns_params.as_ref()
            && *dns != dns.clone().at_two_minute_cadence()
        {
            return Err(PalwModeV2Error::Invalid(
                "the network's DNS windows are counted for a different block rate — at 120 s these state \
                 durations they do not mean, and the unbonding period is enforced in consensus",
            ));
        }
        // …and the cadence the bundle COMMITS to must be the cadence the network RUNS. The
        // bundle's copy is what enters `palw_ruleset_id_v2`; this is what stops the id from
        // describing a cadence the node does not keep.
        if self.blockrate.target_time_per_block != bundle.cadence_target_time_per_block_ms {
            return Err(PalwModeV2Error::Invalid("the network's cadence is not the one its ruleset id commits to"));
        }
        // **ADR-0042 Decision 10: the carve must FIT inside the share it is carved from.**
        //
        // "PALW reward is a carve of the fixed subsidy ... never an addition to it — the schedule
        // is never exceeded (I6/I15)." An attempt-lane block's escrow is withheld from that
        // block's §F worker BASE share, so a bundle whose `worker_carve_permille` exceeds
        // `subsidy_worker_base_bps` would escrow more than the block earns. The coinbase's
        // `saturating_sub` would floor the withholding at the worker share while the release still
        // paid the whole escrow, and the difference would be minted above the schedule — the same
        // over-issuance this Decision forbids, arriving through a parameter instead of a missing
        // line.
        //
        // Checked here rather than in `PalwConsensusParamsV2::validate` because the two numbers
        // live in different objects: the carve is the bundle's, the share is the network's
        // `dns_params`, and only `Params` holds both. A network with no `dns_params` performs no
        // §F carve at all, so the whole subsidy is the worker's and any permille fits.
        if let Some(dns) = self.dns_params.as_ref() {
            let carve_bps = u32::from(bundle.state.worker_carve_permille()) * 10;
            let worker_base_bps = u32::from(dns.reward_params.fee_split.subsidy_worker_base_bps);
            if carve_bps > worker_base_bps {
                return Err(PalwModeV2Error::Invalid(
                    "the PALW worker carve exceeds the network's own worker share — the escrow could not be funded from the block it is carved from",
                ));
            }
        }
        // **Court-window bound invariant (mainnet audit 2026-09-11 deep fence).** Past
        // `palw_audit_2026_09_11_deep`, `CourtOpened` refuses a court whose `window_court` would run
        // past `claim.trace_retention_daa` (= accepted_daa + `min_trace_retention_daa`, pinned at
        // admission) — the DA path's SA-1/SA-6 mirrored onto the interactive court. If
        // `min_trace_retention_daa` were shorter than `window_court`, NO court could ever open — even
        // one filed at acceptance would exceed the obligation — so fraud adjudication would be dead.
        // Refuse such a bundle at construction, and only when the fence is scheduled: below it the
        // court is unbounded by retention exactly as before, so any obligation fits.
        if self.palw_audit_2026_09_11_deep.is_some() {
            let min_retention = crate::palw_producer_v2::palw_min_trace_retention_daa_v1(&bundle.state);
            if min_retention < bundle.state.window_court() {
                return Err(PalwModeV2Error::Invalid(
                    "the retention obligation is shorter than a court window — past palw_audit_2026_09_11_deep no court could open",
                ));
            }
        }
        // ADR-0152 R-core+ (see the non-V2 return above), and then — after every fence's own
        // refusal, so a missing prerequisite names itself first — the §4-ter answerability mirror.
        self.validate_palw_rcore_plus_v1()?;
        // Lane V02 (post-launch): over R-core+, mirrored.
        self.validate_palw_final_lock_full_collateral_v1()?;
        self.validate_palw_final_lock_life_v1()?;
        // Lane F2-lock (post-launch): over lane V02's lock life, which names its own refusals first.
        self.validate_palw_final_lock_life_retro_v1()?;
        self.validate_palw_held_answerability_v1()?;
        // ADR-0151's stated maturity (user decision 2026-09-25), after every fence's own refusal: it
        // rides `palw_economic_safety`, so a missing bundle is named by the rules that need it first.
        // Over its bundle's fence, inside the liability horizon, and never the `None` rule twice.
        self.validate_palw_exec_quantum_maturity_v1()?;
        // Lane F1 (the panel seed, post-launch): over R-core+, which names its own refusals first.
        self.validate_palw_panel_seed_execution_v1()?;
        // Lane A (the operator anchor, post-launch): over R-core+, likewise.
        self.validate_palw_operator_anchor_v1()?;
        // ADR-0160 lane liab (F-L): over R-core+ and the attribution fence, likewise.
        self.validate_palw_capacity_liability_v1()?;
        // **ADR-0170: the seed anchor window's refusals** (`crate::palw_anchor_window_v1`) — last, after every fence's own refusal, so a
        // prerequisite it shares with another fence (the execution lane, the economic-safety bundle, ADR-0147's jury, the registry) is
        // named by that fence's own refusal first.
        self.validate_palw_anchor_window_v1()?;
        // **RFC-0009: the receipt redemption's refusals** (`crate::palw_receipt_v4`) — after the audit fences it names.
        self.validate_palw_receipt_spend_v4()?;
        // **RFC-0009 stage B: the provider court's refusals** — after the fences it names.
        self.validate_palw_evidence_court_v1()?;
        // Lane PA's audit-1004 fence: last, likewise.
        self.validate_palw_audit_1004_v1()?;
        // **ADR-0178: rule E's fence and the DNS retirement's order** — last, after every other refusal, so a rule-E refusal
        // is reached only by a ruleset that passes everything else (`rfc12_zero_dns_matrix` relies on that).
        if private_rule_e { self.validate_palw_fork_choice_rule_e_order_v1() } else { self.validate_palw_fork_choice_rule_e_v1() }
    }

    pub fn validate_palw_v1(&self) -> Result<(), crate::palw_registry::PalwRegistryError> {
        // The V1 lineage's fork choice (ADR-0039 W4′) and its credit gate (ADR-0033) weighed and
        // paid blocks against the DNS overlay's bond view, and both are removed: PALW does not read
        // validators. A network that still names either fence would run a rule no code enforces,
        // so it is refused at startup. The schedule and the ramp only ever described the class that
        // credit registered, and are refused with it.
        use crate::palw_registry::PalwRegistryError::NotCanonical;
        if self.palw_fork_choice.is_some() {
            return Err(NotCanonical("palw_fork_choice is removed — the V1 fork choice read the validator overlay's bonds"));
        }
        if self.palw_credit.is_some() {
            return Err(NotCanonical("palw_credit is removed — the V1 credit gate paid through the validator overlay's bonds"));
        }
        if self.palw_schedule.is_some() {
            return Err(NotCanonical(
                "palw_schedule is set without palw_credit — there is no registered class for these windows to describe",
            ));
        }
        if self.palw_ramp.is_some() {
            return Err(NotCanonical("palw_ramp is set without palw_credit — there is no registered class whose work it would ramp"));
        }
        Ok(())
    }

    /// **What F-E refuses** (ADR-0160 §6.1). Called by `validate_palw_v2`, public so a test can name each
    /// refusal. Below the fence it checks only that the bundle's mirror is `None`. Past it:
    /// * ConsensusV2 only (there is no V2 claim ledger elsewhere);
    /// * `palw_audit_2026_09_23` at or below it — the escrow slot the fence re-prices is option A's,
    ///   which that fence opens (below it the slot is 0, and `m_c` would ADD a reservation);
    /// * `palw_rcore_plus` at or below it — SR-1's committed ledger, its licence release and C7's list;
    /// * `palw_offence_attribution` at or below it — a void closes the claim's courts neutrally, so an
    ///   obligation hold is never pinned open by a session (C3);
    /// * the bundle's mirror equal to the height;
    /// * on a network that retires claims, `claim_retirement_daa > window_receipt` — a claim's E-4 hold
    ///   (`voided + window_receipt`) must end before its retirement is due.
    ///
    /// **Wired on `rcore/cap-s1` (ADR-0160 v3 §10.1):** F-W (`palw_capacity_weight_cap`) and F-L
    /// (`palw_capacity_aggregate_liability`) at or below it. Without a credit `m_c = E`, and F-E then
    /// changes only E-4.
    pub fn validate_palw_capacity_escrow_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let mirror = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.state.capacity_escrow_from_daa()),
            _ => None,
        };
        let Some(fence) = self.palw_capacity_escrow_at_licence.filter(|fence| *fence != ForkActivation::never()) else {
            if mirror.flatten().is_some() {
                return Err(Invalid(
                    "the V2 bundle carries a capacity-escrow height without palw_capacity_escrow_at_licence armed: mirror the \
                     fence with Params::sync_palw_capacity_escrow after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else {
            return Err(Invalid(
                "palw_capacity_escrow_at_licence is armed on a network that is not ConsensusV2: there is no V2 claim ledger whose \
                 escrow slot it re-prices (ADR-0160)",
            ));
        };
        let at_or_below =
            |f: Option<ForkActivation>| f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= fence.daa_score());
        if !(at_or_below(self.palw_audit_2026_09_23)
            && at_or_below(self.palw_rcore_plus)
            && at_or_below(self.palw_offence_attribution))
        {
            return Err(Invalid(
                "palw_capacity_escrow_at_licence is armed without palw_audit_2026_09_23, palw_rcore_plus and \
                 palw_offence_attribution all at or below it: the escrow slot it re-prices is option A's, released by SR-1's \
                 ledger, and a void must close its courts before an obligation hold may outlive it (ADR-0160 §6.1)",
            ));
        }
        // rcore/cap-s1 (ADR-0160 v3 §10.1): F-W at or below F-E — the escrow slot is priced beside the
        // capped weight reservation (`m_c + min(w, R_budget − held)`, §5.1); a claim accepted between the
        // two would hold an uncapped `w` next to a re-priced slot.
        if !at_or_below(self.palw_capacity_weight_cap) {
            return Err(Invalid(
                "palw_capacity_escrow_at_licence is armed without palw_capacity_weight_cap (F-W) at or below it: the escrow \
                 slot is priced beside the capped weight reservation (ADR-0160 v3 §5.1, §10.1)",
            ));
        }
        // rcore/cap-s1 (ADR-0160 v3 §10.1): F-L at or below F-E — the credit the slot reads is F-L's step
        // (`capacity_escrow_credit_at_v1`), and a claim accepted between the two must be bound as one past
        // both (`palw_escrow_bind_reserves_the_lock_v1`).
        if !at_or_below(self.palw_capacity_aggregate_liability.as_ref().map(|value| value.activation)) {
            return Err(Invalid(
                "palw_capacity_escrow_at_licence is armed without palw_capacity_aggregate_liability (F-L) at or below it: the \
                 escrow slot's credit is F-L's step (ADR-0160 v3 §10.1)",
            ));
        }
        if mirror.flatten() != Some(fence.daa_score()) {
            return Err(Invalid(
                "palw_capacity_escrow_at_licence disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_capacity_escrow after the bundle is assembled",
            ));
        }
        let retirement = bundle.state.claim_retirement_daa();
        if retirement > 0 && retirement <= bundle.state.window_receipt() {
            return Err(Invalid(
                "palw_capacity_escrow_at_licence is armed where claims retire no later than window_receipt after their void: \
                 an E-4 obligation hold (voided + window_receipt) must end before the claim's retirement (ADR-0160 E-4)",
            ));
        }
        Ok(())
    }

    /// **What the FP Job V4 fence refuses** (RFC-0001 §A; audit D M-1 lifted by what this build now
    /// carries). Called by `validate_palw_v2` on both paths, public so a test can name each refusal.
    ///
    /// The M-1 refusal stood because neither half of ADR-0082 D10/D11 existed on the path that
    /// applies it. This build carries both, and V4 with them: the transition credits a free-prompt
    /// claim its DECODE work (`palw_fp_commitment_price_from_state_v1`, over the class's published
    /// graph), every free-prompt engine selects through `palw_decode_pipeline_v4::PalwFpDecoderV1`
    /// (the V3 rule for a V3 job, the §A.3 pipeline with Decision 11's key for a V4 job), and every
    /// admission door admits exactly one job version per height. What remains is what the rules
    /// NEED, refused by name:
    ///
    /// * **ConsensusV2** — the free-prompt lane exists nowhere else;
    /// * **`palw_fp_derived_work` at or below it** — D10 enumerates the decode leaves from the class's
    ///   published graph (ADR-0145 §5); below the derivation the transition has nothing to split the
    ///   capture with and would refuse every claim `FreePromptDecodeLeavesUnavailable`;
    /// * **the bundle's mirror equal to the fence**, and the retired serialized height `None` — a
    ///   height inside the bundle's borsh would move `palw_ruleset_id_v2`, which the identity id reads
    ///   unnormalised, at deploy instead of at the fence.
    pub fn validate_palw_fp_decode_rules_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let (mirror, serialized) = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                (bundle.state.fp_decode_rules_from_daa(), bundle.state.fp_decode_rules_serialized_daa())
            }
            _ => (None, None),
        };
        if serialized.is_some() {
            return Err(Invalid(
                "the V2 bundle serializes a decode-rules height: it moves palw_ruleset_id_v2 — and with it the identity every \
                 peer compares — at deploy; the height rides the borsh-skipped mirror Params::sync_palw_fp_decode_rules writes",
            ));
        }
        let Some(fence) = self.palw_fp_decode_rules.filter(|fence| *fence != ForkActivation::never()) else {
            if mirror.is_some() {
                return Err(Invalid(
                    "the V2 bundle carries a decode-rules height without palw_fp_decode_rules armed: mirror the fence with \
                     Params::sync_palw_fp_decode_rules after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        if !matches!(self.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)) {
            return Err(Invalid(
                "palw_fp_decode_rules is armed on a network that is not ConsensusV2: the free-prompt lane it governs exists only there",
            ));
        }
        match self.palw_fp_derived_work.filter(|derived| *derived != ForkActivation::never()) {
            Some(derived) if derived.daa_score() <= fence.daa_score() => {}
            _ => {
                return Err(Invalid(
                    "palw_fp_decode_rules is armed without palw_fp_derived_work at or below its height: ADR-0082 Decision 10 \
                     credits a claim's decode leaves, which the transition enumerates from the class's published graph \
                     (ADR-0145 §5) — below the derivation every free-prompt claim would be refused \
                     FreePromptDecodeLeavesUnavailable",
                ));
            }
        }
        if mirror != Some(fence.daa_score()) {
            return Err(Invalid(
                "palw_fp_decode_rules is armed and the V2 bundle's mirror disagrees: mirror the fence with \
                 Params::sync_palw_fp_decode_rules after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What lane V02's fence refuses.** Called by `validate_palw_v2` after R-core+'s own refusals,
    /// public so a test can name each. Below the fence it checks only that the bundle's mirror is
    /// `None`. Past it: ConsensusV2 only; `palw_rcore_plus` (the one ledger this splits) armed at or
    /// below its height; the mirror equal to the height. Any height is admissible — genesis included,
    /// and on a live testnet-12 the common post-launch height.
    pub fn validate_palw_final_lock_full_collateral_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let mirror = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.state.final_lock_full_collateral_from_daa()),
            _ => None,
        };
        let Some(fence) = self.palw_final_lock_full_collateral.filter(|fence| *fence != ForkActivation::never()) else {
            if mirror.flatten().is_some() {
                return Err(Invalid(
                    "the V2 bundle carries a final-lock height without palw_final_lock_full_collateral armed: mirror the fence \
                     with Params::sync_palw_final_lock_full_collateral after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let Some(mirror) = mirror else {
            return Err(Invalid(
                "palw_final_lock_full_collateral is armed on a network that is not ConsensusV2: the ledger it splits is \
                 R-core+'s",
            ));
        };
        if !self.palw_rcore_plus.is_some_and(|rcore| rcore != ForkActivation::never() && rcore.daa_score() <= fence.daa_score()) {
            return Err(Invalid(
                "palw_final_lock_full_collateral is armed without palw_rcore_plus at or below it: it splits R-core+'s one \
                 committed ledger, which does not exist below that fence",
            ));
        }
        if mirror != Some(fence.daa_score()) {
            return Err(Invalid(
                "palw_final_lock_full_collateral disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_final_lock_full_collateral after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What lane V02's fence refuses.** Called by `validate_palw_v2` after R-core+'s own refusals,
    /// public so a test can name each. Below the fence it checks only that the bundle's mirror is
    /// `None`. Past it: ConsensusV2 only; `palw_rcore_plus` (whose vesting rows cover `E` while a lock
    /// is released early) armed at or below its height; the mirror equal to the height. Any height is
    /// admissible — genesis included, and on a live testnet-12 the common post-launch height.
    pub fn validate_palw_final_lock_life_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let mirror = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.state.final_lock_life_from_daa()),
            _ => None,
        };
        let Some(fence) = self.palw_final_lock_life.filter(|fence| *fence != ForkActivation::never()) else {
            if mirror.flatten().is_some() {
                return Err(Invalid(
                    "the V2 bundle carries a final-lock-life height without palw_final_lock_life armed: mirror the fence \
                     with Params::sync_palw_final_lock_life after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let Some(mirror) = mirror else {
            return Err(Invalid(
                "palw_final_lock_life is armed on a network that is not ConsensusV2: the locks it dates are R-core+'s",
            ));
        };
        if !self.palw_rcore_plus.is_some_and(|rcore| rcore != ForkActivation::never() && rcore.daa_score() <= fence.daa_score()) {
            return Err(Invalid(
                "palw_final_lock_life is armed without palw_rcore_plus at or below it: its vesting rows are what still \
                 cover E once a Valid seat's lock is released early, and they do not exist below that fence",
            ));
        }
        if mirror != Some(fence.daa_score()) {
            return Err(Invalid(
                "palw_final_lock_life disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_final_lock_life after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What lane F2-lock's fence refuses.** Called by `validate_palw_v2` after lane V02's own
    /// refusals, public so a test can name each. Below the fence it checks only that the bundle's
    /// mirror is `None`. Past it: ConsensusV2 only; `palw_final_lock_life` armed at or below its height
    /// (the locks it re-dates take that fence's life, and past that fence a lock stops being slashable
    /// when it stops being committed — without it a lock this fence releases to new work would still
    /// be taken by a conviction, one unit of collateral answering two `Valid`s); the mirror equal to
    /// the height. Any height is admissible, genesis included (a chain born with it has no long lock
    /// to re-date).
    pub fn validate_palw_final_lock_life_retro_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let mirror = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.state.final_lock_life_retro_from_daa()),
            _ => None,
        };
        let Some(fence) = self.palw_final_lock_life_retro.filter(|fence| *fence != ForkActivation::never()) else {
            if mirror.flatten().is_some() {
                return Err(Invalid(
                    "the V2 bundle carries a retroactive lock-life height without palw_final_lock_life_retro armed: mirror \
                     the fence with Params::sync_palw_final_lock_life_retro after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let Some(mirror) = mirror else {
            return Err(Invalid(
                "palw_final_lock_life_retro is armed on a network that is not ConsensusV2: the locks it re-dates are \
                 R-core+'s",
            ));
        };
        if !self.palw_final_lock_life.is_some_and(|life| life != ForkActivation::never() && life.daa_score() <= fence.daa_score()) {
            return Err(Invalid(
                "palw_final_lock_life_retro is armed without palw_final_lock_life at or below it: the locks it re-dates \
                 take that fence's life, and only past it does a lock stop being slashable when it stops being committed",
            ));
        }
        if mirror != Some(fence.daa_score()) {
            return Err(Invalid(
                "palw_final_lock_life_retro disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_final_lock_life_retro after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **ADR-0152 §4-ter (A-held): the answerability mirror is the derivation, byte for byte.**
    pub fn validate_palw_held_answerability_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && bundle.state.held_unanswerable_classes() != self.palw_held_unanswerable_classes_v1().as_slice()
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "the V2 bundle's held_unanswerable_classes disagrees with its genesis held rows under palw_offence_attribution: \
                 mirror it with Params::sync_palw_held_answerability after the bundle is assembled (ADR-0152 §4-ter)",
            ));
        }
        // ADR-0152 §4-ter (the review's F5): a network that arms the attribution fence over the held
        // regime must hold a whole held dissection at the compute cap inside its court window.
        let armed = self.palw_offence_attribution.is_some_and(|f| f != ForkActivation::never())
            && self.palw_held_context.is_some_and(|f| f != ForkActivation::never());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && armed
            && !crate::palw_class_admission_v2::palw_held_dissection_fits_court_v1(
                bundle.court.turn_deadline_daa(),
                bundle.state.window_court(),
                crate::palw_state_v2::palw_close_assembly_daa_v1(crate::palw_state_v2::PALW_COURT_CLOSE_MAX_CHUNKS),
            )
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "a held dissection at the compute-turn cap does not fit this ruleset's court window (ADR-0152 §4-ter, \
                 palw_held_dissection_fits_court_v1)",
            ));
        }
        Ok(())
    }

    /// **ADR-0152 v3.1 §6: what `palw_rcore_plus` refuses** (T24's fence part). Called by
    /// `validate_palw_v2` before any later rule, and public so T24 can name each refusal alone.
    /// Below the fence it checks only that nothing of R-core+ is set: a C7 list without the fence,
    /// or a bundle whose mirrors are not the dormant values.
    pub fn validate_palw_rcore_plus_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let Some(fence) = self.palw_rcore_plus.filter(|f| *f != ForkActivation::never()) else {
            if !self.palw_rcore_conservative_classes.is_empty() {
                return Err(Invalid(
                    "palw_rcore_conservative_classes is non-empty without palw_rcore_plus armed: C7 is R-core+'s hold, and \
                     nothing reads it below the fence",
                ));
            }
            if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
                && (bundle.state.rcore_plus_from_daa().is_some()
                    || bundle.state.withdrawal_delay_daa() != 0
                    || !bundle.state.rcore_conservative_classes().is_empty())
            {
                return Err(Invalid(
                    "the V2 bundle carries R-core+ mirrors without palw_rcore_plus armed: mirror the fence with \
                     Params::sync_palw_rcore_plus after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        // Genesis only, the audit fence's rule: R-core+ re-prices every live claim's reservation, and
        // a crossing would leave claims reserved under one ledger and released under another.
        if fence.daa_score() != 0 {
            return Err(Invalid(
                "palw_rcore_plus may only be armed at genesis (DAA 0): a crossing would reserve a live claim under one \
                 ledger and release it under another",
            ));
        }
        let armed_below =
            |f: Option<ForkActivation>| f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= fence.daa_score());
        // ADR §6 "Prerequisites (X15, F20)", one refusal each so T24 can name the one it removed.
        let prerequisites: [(bool, &'static str); 14] = [
            (
                armed_below(self.palw_offence_attribution),
                "palw_rcore_plus is armed without palw_offence_attribution at or below it: R-core+'s tiers read F1/F2's records",
            ),
            (
                armed_below(self.palw_admission_independence),
                "palw_rcore_plus is armed without palw_admission_independence at or below it: the stake draw relies on its cut",
            ),
            (
                armed_below(self.palw_audit_2026_09_23),
                "palw_rcore_plus is armed without palw_audit_2026_09_23 at or below it: the one ledger extends its rate room",
            ),
            // ADR-0152 SW-8 (M4 review finding 4): the stake draw binds a panel only in its anchor
            // block, on the pre-object base the acceptance walk validates against — a state the walk
            // folds object by object only past the 2026-09-11 audit's A-1. Below it the walk
            // rehearses each object through a whole-block transition while the derivation reads
            // another state, and a binding it dropped could never be retried.
            (
                armed_below(self.palw_audit_2026_09_11),
                "palw_rcore_plus is armed without palw_audit_2026_09_11 at or below it: SW-8's one state is the walk's \
                 object-by-object pre-object base",
            ),
            (armed_below(self.palw_economic_safety), "palw_rcore_plus is armed without palw_economic_safety at or below it"),
            (
                armed_below(self.palw_objective_offence),
                "palw_rcore_plus is armed without palw_objective_offence at or below it: ExecutorRefuted rides its ledger",
            ),
            (armed_below(self.palw_panel_economy), "palw_rcore_plus is armed without palw_panel_economy at or below it"),
            (
                armed_below(self.palw_panel_exposure_floor.map(|floor| floor.activation)),
                "palw_rcore_plus is armed without palw_panel_exposure_floor at or below it",
            ),
            (
                armed_below(self.palw_unavailable_abstains),
                "palw_rcore_plus is armed without palw_unavailable_abstains at or below it: A-4's premise",
            ),
            (armed_below(self.palw_clock_floor), "palw_rcore_plus is armed without palw_clock_floor at or below it"),
            (armed_below(self.palw_clock_cursor), "palw_rcore_plus is armed without palw_clock_cursor at or below it"),
            (
                armed_below(self.palw_verification_v2),
                "palw_rcore_plus is armed without palw_verification_v2 at or below it: Q-1's V3 receipts",
            ),
            (armed_below(self.palw_da_court), "palw_rcore_plus is armed without palw_da_court at or below it"),
            (
                armed_below(self.palw_operator_id_unique),
                "palw_rcore_plus is armed without palw_operator_id_unique at or below it: the stake draw weighs one operator's bond",
            ),
        ];
        if let Some((_, why)) = prerequisites.iter().find(|(armed, _)| !armed) {
            return Err(Invalid(why));
        }
        if self.palw_settled_anchor_depth.is_none() {
            return Err(Invalid(
                "palw_rcore_plus is armed without palw_settled_anchor_depth: vesting and the locks mature on the second clock",
            ));
        }
        if self.palw_shard_licensing.is_some_and(|f| f != ForkActivation::never()) {
            return Err(Invalid(
                "palw_rcore_plus is armed beside palw_shard_licensing: Q-3's basis recount has no rule for a licence by parts",
            ));
        }
        // C7 is a subset of testnet-12's genesis held rows: the hold names classes the chain
        // registered at genesis, never a class a later registration could bring.
        if !self.palw_rcore_conservative_classes.is_empty() {
            let held = palw_t12_genesis_held_class_ids_v1();
            if self.palw_rcore_conservative_classes.iter().any(|class| !held.contains(class)) {
                return Err(Invalid(
                    "palw_rcore_conservative_classes names a class that is not one of testnet-12's genesis held rows \
                     (PALW_T12_GENESIS_HELD_ROWS)",
                ));
            }
        }
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            // IMPL-7: the reporter-commit (and, with M3, the DA-disclosure-v4) context is only in V5.
            if bundle.signature_contexts_root != crate::palw_mode_v2::palw_v2_signature_contexts_root_v5() {
                return Err(Invalid(
                    "palw_rcore_plus is armed but the bundle's signature_contexts_root is not the COMPLETE_V5 set: R-core+'s \
                     reporter commitments sign under a context only V5 commits to (ADR-0152 IMPL-7)",
                ));
            }
            // S-3 (SR-6): the fold re-checks a licence set's backed subset against the door's quorum, and
            // the fold holds no panel params — it reads the colluding quorum the lock is priced for.
            // The two must be one number, or a backed subset could license under a quorum the
            // acceptance layer never asked for.
            if bundle.panel.quorum() as u64 != crate::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1 {
                return Err(Invalid(
                    "palw_rcore_plus is armed with a panel quorum other than PALW_PANEL_COLLUDING_QUORUM_V1: the fold checks a \
                     licence's backed subset (SR-6) against that quorum",
                ));
            }
            // IMPL-6: the three mirrors are the params, byte for byte (the escrow mirror's rule).
            let delay = palw_v2_bond_withdrawal_delay_at_v1(bundle, self.palw_da_court, 0);
            if bundle.state.rcore_plus_from_daa() != Some(fence.daa_score())
                || bundle.state.withdrawal_delay_daa() != delay
                || bundle.state.rcore_conservative_classes() != self.palw_rcore_conservative_classes
            {
                return Err(Invalid(
                    "palw_rcore_plus, the processor's withdrawal delay or palw_rcore_conservative_classes disagree with the V2 \
                     bundle's mirrors: mirror them with Params::sync_palw_rcore_plus after the bundle is assembled",
                ));
            }
        }
        Ok(())
    }

    /// **ADR-0152 §4-quater: what `palw_class_verify_deadline` refuses** (T-D7's fence part). Called
    /// by `validate_palw_v2`, and public so a test can name each refusal alone. Below the fence it
    /// checks only that nothing of it is set: rows without the fence, or a bundle whose mirrors are
    /// not the dormant values.
    pub fn validate_palw_class_verify_deadline_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let Some(fence) = self.palw_class_verify_deadline.filter(|f| *f != ForkActivation::never()) else {
            if !self.palw_class_verify_rows.is_empty() {
                return Err(Invalid(
                    "palw_class_verify_rows is non-empty without palw_class_verify_deadline armed: a measured row prices a \
                     class-derived deadline, and nothing reads one below the fence",
                ));
            }
            if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
                && (bundle.state.class_verify_deadline_from_daa().is_some() || !bundle.state.class_verify_rows().is_empty())
            {
                return Err(Invalid(
                    "the V2 bundle carries class-verify-deadline mirrors without palw_class_verify_deadline armed: mirror the \
                     fence with Params::sync_palw_class_verify_deadline after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        // Genesis only: the pruning depth that fits a class-derived deadline is fixed at genesis
        // (P-1 — a pruning point cannot move backward, so the depth can never be raised later), and a
        // crossing would leave claims licensed below it finalizing at `L + wc` beside claims past it
        // held to `H`, with no pruning depth sized for the second.
        if fence.daa_score() != 0 {
            return Err(Invalid(
                "palw_class_verify_deadline may only be armed at genesis (DAA 0): the pruning depth its deadlines need \
                 is a genesis fact",
            ));
        }
        let armed_below =
            |f: Option<ForkActivation>| f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= fence.daa_score());
        // The rule replaces §11.3's product: a class-derived receipt window must already be the rule.
        if !armed_below(self.palw_class_receipt_window) {
            return Err(Invalid(
                "palw_class_verify_deadline is armed without palw_class_receipt_window at or below it: the class-derived \
                 deadline replaces §11.3's receipt window and cannot precede it",
            ));
        }
        // Long-D, the room's hold, V6 and the `R_eff` span in the pruning horizon are all measured
        // against the SHORT challenge window (120): a licence that could still get the long one
        // (1,200) would make every one of them the wrong bound.
        if !armed_below(self.palw_short_challenge_window) {
            return Err(Invalid(
                "palw_class_verify_deadline is armed without palw_short_challenge_window at or below it: long-D, the room's \
                 hold and the pruning horizon's R_eff span are all measured against the short challenge window",
            ));
        }
        // A free-prompt claim's deadline reads its class's published work profile; past the derived-work
        // fence no free-prompt claim exists without one, so the deadline a claim is bound under never
        // changes when a profile is published later.
        if !armed_below(self.palw_fp_derived_work) {
            return Err(Invalid(
                "palw_class_verify_deadline is armed without palw_fp_derived_work at or below it: a free-prompt claim's \
                 deadline reads its class's work profile, which only that fence makes exist before the claim",
            ));
        }
        let mut seen: Vec<crate::Hash64> = Vec::new();
        for row in self.palw_class_verify_rows {
            row.validate().map_err(Invalid)?;
            if seen.contains(&row.class_id) {
                return Err(Invalid("palw_class_verify_rows names one class twice: a class has one measured deadline"));
            }
            seen.push(row.class_id);
            // U-D8: a class opens only when its lies are attributable.
            if !self.palw_offence_attribution.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= row.activation_daa) {
                return Err(Invalid(
                    "a palw_class_verify_rows row activates before palw_offence_attribution: a class that needs a measured \
                     deadline opens only when a false Valid on it is attributable (U-D8)",
                ));
            }
        }
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && (bundle.state.class_verify_deadline_from_daa() != Some(fence.daa_score())
                || bundle.state.class_verify_rows() != self.palw_class_verify_rows)
        {
            return Err(Invalid(
                "palw_class_verify_deadline or palw_class_verify_rows disagree with the V2 bundle's mirrors: mirror them \
                 with Params::sync_palw_class_verify_deadline after the bundle is assembled",
            ));
        }
        // **E-11 / T-D8b**: a measured row opens deadlines up to D_cap, which only a pruning horizon
        // sized for D_cap holds. Past this fence `validate_palw_v2`'s claim-lattice check (K18) reads
        // `palw_v2_claim_lattice_daa_v1` — the D_cap lattice, the same number `with_palw_v2_depths`
        // derives the depth from — so no row, and no fence, can stand on a shorter horizon.
        Ok(())
    }

    /// **What `palw_readiness_v2_max_age_spans` refuses** (user decision 2026-09-25, readiness capacity
    /// option (a)). Called by `validate_palw_v2`, and public so a test can name each refusal alone.
    /// Below the fence it checks only that the bundle's mirror is the dormant `None`.
    pub fn validate_palw_readiness_v2_max_age_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        use crate::palw_model_registry_v1::{PALW_READINESS_V2_MAX_AGE_SPANS_MAX_V1, PALW_READINESS_V2_MAX_AGE_SPANS_V1};
        let mirror = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.state.readiness_v2_max_age_spans()),
            _ => None,
        };
        let Some(horizon) = self.palw_readiness_v2_max_age_spans.filter(|horizon| horizon.activation != ForkActivation::never())
        else {
            if mirror.flatten().is_some() {
                return Err(Invalid(
                    "the V2 bundle carries a readiness-V2 horizon without palw_readiness_v2_max_age_spans armed: mirror the \
                     fence with Params::sync_palw_readiness_v2_max_age_spans after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let Some(mirror) = mirror else {
            return Err(Invalid(
                "palw_readiness_v2_max_age_spans is armed on a network that is not ConsensusV2: the horizon is the V2 \
                 registry's readiness rule",
            ));
        };
        // Genesis only: a crossing would count one row ready and not ready either side of one height —
        // the registry's count, ADR-0147's jury and the draw all move at once — with no drill that
        // crosses it.
        if horizon.activation.daa_score() != 0 {
            return Err(Invalid(
                "palw_readiness_v2_max_age_spans may only be armed at genesis (DAA 0): the horizon decides which seats \
                 are ready, and a crossing would count one row by two horizons",
            ));
        }
        // The horizon is readiness V2's and the registry's: both from genesis beside it, so no block
        // judges a V2 row by any other horizon.
        let at_genesis = |f: Option<ForkActivation>| f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() == 0);
        if !(at_genesis(self.palw_model_registry) && at_genesis(self.palw_readiness_v2)) {
            return Err(Invalid(
                "palw_readiness_v2_max_age_spans is armed without palw_model_registry and palw_readiness_v2 both armed \
                 at genesis: the horizon is the V2 possession row's, which only the registry past readiness V2 counts",
            ));
        }
        // Never below the default (at eight the half-age duty, due past `max / 2` — age 5 — still
        // comes before the M1 escalation from `max − 2` — age 6; at six they would meet), never past
        // the V1 age (a V2 row exists to make possession bite harder than the one-leaf row it
        // replaced).
        if !(PALW_READINESS_V2_MAX_AGE_SPANS_V1..=PALW_READINESS_V2_MAX_AGE_SPANS_MAX_V1).contains(&horizon.max_age_spans) {
            return Err(Invalid(
                "palw_readiness_v2_max_age_spans is outside [8, 30] spans: never shorter than the eight a V2 row stands \
                 by default, never longer than the V1 age (thirty), which would make the V2 row the laxer one",
            ));
        }
        if mirror != Some(horizon.max_age_spans) {
            return Err(Invalid(
                "palw_readiness_v2_max_age_spans disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_readiness_v2_max_age_spans after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What `palw_registry_resilience` refuses** (lane F1). Called by `validate_palw_v2`, public so a
    /// test can name each refusal. Below the fence it checks only that the bundle's mirror is `None`.
    /// Past it: ConsensusV2 only; `palw_rcore_plus` (SW-8's anchor-block void, which V03(1) narrows)
    /// and `palw_model_registry` (the lifecycle V03(2) and V05 step) armed at or below its height; the
    /// mirror equal to the height. Any height is admissible, genesis included — the rules are written
    /// to be crossed on a live chain.
    pub fn validate_palw_registry_resilience_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let mirror = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.state.registry_resilience_from_daa()),
            _ => None,
        };
        let Some(fence) = self.palw_registry_resilience.filter(|fence| *fence != ForkActivation::never()) else {
            if mirror.flatten().is_some() {
                return Err(Invalid(
                    "the V2 bundle carries a registry-resilience height without palw_registry_resilience armed: mirror the \
                     fence with Params::sync_palw_registry_resilience after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let Some(mirror) = mirror else {
            return Err(Invalid(
                "palw_registry_resilience is armed on a network that is not ConsensusV2: the rules it changes are the V2 \
                 registry's and SW-8's",
            ));
        };
        let at_or_below =
            |f: Option<ForkActivation>| f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= fence.daa_score());
        if !(at_or_below(self.palw_rcore_plus) && at_or_below(self.palw_model_registry)) {
            return Err(Invalid(
                "palw_registry_resilience is armed without palw_rcore_plus and palw_model_registry both armed at or below \
                 it: the re-anchor narrows SW-8's anchor-block void, and the probation rules step the registry's lifecycle",
            ));
        }
        if mirror != Some(fence.daa_score()) {
            return Err(Invalid(
                "palw_registry_resilience disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_registry_resilience after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What `palw_floor_refusal_retry` refuses** (lane F2). Called by `validate_palw_v2`, public so a
    /// test can name each refusal. Below the fence it checks only that the bundle's mirror is `None`.
    /// Past it: ConsensusV2 only; `palw_rcore_plus` — the stake draw whose refusals it retries and
    /// SW-8's anchor-block void it narrows — armed at or below its height; the mirror equal to the
    /// height. Any height is admissible, genesis included — the rule is written to be crossed on a live
    /// chain.
    pub fn validate_palw_floor_refusal_retry_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let mirror = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.state.floor_refusal_retry_from_daa()),
            _ => None,
        };
        let Some(fence) = self.palw_floor_refusal_retry.filter(|fence| *fence != ForkActivation::never()) else {
            if mirror.flatten().is_some() {
                return Err(Invalid(
                    "the V2 bundle carries a floor-refusal-retry height without palw_floor_refusal_retry armed: mirror the \
                     fence with Params::sync_palw_floor_refusal_retry after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let Some(mirror) = mirror else {
            return Err(Invalid(
                "palw_floor_refusal_retry is armed on a network that is not ConsensusV2: the draw it retries is the V2 \
                 stake draw and the void it narrows is SW-8's",
            ));
        };
        if !self.palw_rcore_plus.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= fence.daa_score()) {
            return Err(Invalid(
                "palw_floor_refusal_retry is armed without palw_rcore_plus armed at or below it: the refusals it retries are \
                 the stake draw's, and the void it narrows is SW-8's anchor-block void",
            ));
        }
        if mirror != Some(fence.daa_score()) {
            return Err(Invalid(
                "palw_floor_refusal_retry disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_floor_refusal_retry after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What `palw_exec_quantum_maturity_daa` refuses** (user decision 2026-09-25). Called by
    /// `validate_palw_v2`, and public so a test can name each refusal alone. `None` is always valid.
    ///
    /// * **Without `palw_economic_safety`** — a maturity nothing reads (the value rides that fence's
    ///   height; it is not a fence of its own).
    /// * **On a network with no V2 bundle** — there is no liability horizon to measure it against.
    /// * **Past `window_court`** — the gap it leaves is already zero at the horizon, and a longer
    ///   maturity only freezes an honest Final's rights after nothing can convict it any more.
    /// * **At the lattice challenge window** — that is the `None` rule; stated a second way it would
    ///   fingerprint apart from itself and split two builds that run one rule.
    ///
    /// Zero is a maturity, not absence: every right spendable the span after next, and the whole
    /// mint priced (ADR-0151: any maturity is safe because the residual is priced).
    pub fn validate_palw_exec_quantum_maturity_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let Some(maturity) = self.palw_exec_quantum_maturity_daa else {
            return Ok(());
        };
        if self.palw_economic_safety.is_none_or(|f| f == ForkActivation::never()) {
            return Err(Invalid(
                "palw_exec_quantum_maturity_daa is stated without palw_economic_safety: the maturity is that bundle's, \
                 and below its fence nothing reads it",
            ));
        }
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else {
            return Err(Invalid(
                "palw_exec_quantum_maturity_daa is stated on a network that is not ConsensusV2: there is no liability \
                 horizon to measure it against",
            ));
        };
        if maturity > bundle.state.window_court() {
            return Err(Invalid(
                "palw_exec_quantum_maturity_daa is past window_court: a right that matures after its liability expires \
                 is frozen for nothing — at window_court the priced gap is already zero",
            ));
        }
        if maturity == bundle.state.window_challenge() {
            return Err(Invalid(
                "palw_exec_quantum_maturity_daa states the lattice challenge window, which is the None rule: leave it \
                 None, or the one rule fingerprints two ways",
            ));
        }
        Ok(())
    }

    /// **What lane maturity's fence refuses.** Called by `validate_palw_v2` on both paths, and public
    /// so a test can name each refusal alone. Dormant (`None` or `never()`) is always valid.
    ///
    /// * **On a network that is not ConsensusV2** — there is no V2 panel to draw.
    /// * **Without `palw_bond_maturity` scheduled** — the window is `palw_bond_maturity`'s and the
    ///   fence has none of its own: one window, so the two can never disagree about a bond.
    /// * **At or past `palw_bond_maturity`'s own height** — it would bring nothing forward, and at the
    ///   SAME height (testnet-12's 1,000) a second fence is invisible to the fork id.
    /// * **Inside the window on a genesis above DAA 0** — the one case in which D1 armed early starves
    ///   the draw: genesis bonds register at `genesis.daa_score`, and `anchor_daa - window` saturating
    ///   at 0 stays below it until `genesis.daa_score + window`. At a DAA-0 genesis (testnet-12, and
    ///   every shipped genesis) the floor never falls below a genesis bond, which is the whole reason
    ///   this fence may sit where `palw_bond_maturity`'s own guard would refuse.
    pub fn validate_palw_bond_maturity_early_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let Some(fence) = self.palw_bond_maturity_early.filter(|f| *f != ForkActivation::never()) else {
            return Ok(());
        };
        if !matches!(self.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)) {
            return Err(Invalid(
                "palw_bond_maturity_early is armed on a network that is not ConsensusV2: there is no V2 panel to draw",
            ));
        }
        let Some(maturity) = self.palw_bond_maturity.filter(|m| m.activation != ForkActivation::never()) else {
            return Err(Invalid(
                "palw_bond_maturity_early is armed without palw_bond_maturity scheduled: the window it brings forward \
                 is palw_bond_maturity's, and it has none of its own",
            ));
        };
        if fence.daa_score() >= maturity.activation.daa_score() {
            return Err(Invalid(
                "palw_bond_maturity_early is armed at or past palw_bond_maturity's own height: it brings nothing \
                 forward, and at the same height a second fence is invisible to the fork id",
            ));
        }
        let genesis = self.genesis.daa_score;
        if genesis != 0 && fence.daa_score() < genesis.saturating_add(maturity.window_daa) {
            return Err(Invalid(
                "palw_bond_maturity_early is armed inside its window on a genesis above DAA 0: the genesis bonds \
                 register at the genesis DAA, the saturating floor stays below it, and no panel could be drawn",
            ));
        }
        Ok(())
    }

    pub fn validate_palw_class_semantics_v1(&self) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        if self.palw_class_semantics_v1.is_some_and(|f| f != ForkActivation::never()) && self.palw_class_semantics_v1_fence().is_none()
        {
            return Err(crate::palw_state_v2::PalwStateV2Error::InvalidParams("palw_class_semantics_v1 requires ConsensusV2"));
        }
        Ok(())
    }

    pub fn validate_palw_court_commitment_scope_v1(&self) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        if self.palw_court_commitment_scope_v1.is_some_and(|f| f != ForkActivation::never())
            && self.palw_court_commitment_scope_v1_fence().is_none()
        {
            return Err(crate::palw_state_v2::PalwStateV2Error::InvalidParams("palw_court_commitment_scope_v1 requires ConsensusV2"));
        }
        Ok(())
    }

    pub fn validate_palw_staged_public_court_v1(&self) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        if self.palw_staged_public_court_v1.is_some_and(|f| f != ForkActivation::never())
            && self.palw_staged_public_court_v1_fence().is_none()
        {
            return Err(crate::palw_state_v2::PalwStateV2Error::InvalidParams("palw_staged_public_court_v1 requires ConsensusV2"));
        }
        Ok(())
    }

    /// Registration IDs and root attribution must already exist when definitions become fixed.
    pub fn validate_palw_model_immutable_v1(&self) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::PalwStateV2Error::InvalidParams;
        let Some(fence) = self.palw_model_immutable_v1.filter(|f| *f != ForkActivation::never()) else { return Ok(()) };
        if self.palw_model_immutable_v1_fence().is_none() {
            return Err(InvalidParams("palw_model_immutable_v1 requires ConsensusV2"));
        }
        for dependency in [self.palw_model_lines_fence(), self.palw_artifact_root_ownership] {
            if !dependency.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= fence.daa_score()) {
                return Err(InvalidParams("palw_model_immutable_v1 requires model lines and artifact root ownership at or below it"));
            }
        }
        Ok(())
    }

    /// **What F-W refuses.** Called by `validate_palw_v2`, public so a test can name each refusal.
    /// Dormant: the bundle's mirror must be `None`. Armed: ConsensusV2 only; `palw_rcore_plus` at or
    /// below it (the staged weight reads R-core+'s licence record, and J-1 caps what R-core+'s
    /// reservation prices); `palw_reorg_strict_economic_win` at or below it (staged weight lowers every
    /// candidate's live total, so all-economic ties become common, and without strict-win a tie is
    /// decided by the hash — the grinding hole that fence closes); `palw_operator_anchor` (lane A) at or
    /// below it (a private branch that anchors its own panels self-licenses up to its bonds' caps, a
    /// staging the public branch cannot match inside the lead — ADR-0160 §8 A1); the mirror equal to the
    /// height. Any height is legal otherwise — the rule is keyed on each claim's `accepted_daa`, written
    /// to be crossed on a live chain.
    pub fn validate_palw_capacity_weight_cap_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let mirror = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.state.capacity_weight_cap_from_daa()),
            _ => None,
        };
        let Some(fence) = self.palw_capacity_weight_cap.filter(|fence| *fence != ForkActivation::never()) else {
            if mirror.flatten().is_some() {
                return Err(Invalid(
                    "the V2 bundle carries a capacity weight-cap height without palw_capacity_weight_cap armed: mirror the \
                     fence with Params::sync_palw_capacity_weight_cap after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let Some(mirror) = mirror else {
            return Err(Invalid(
                "palw_capacity_weight_cap is armed on a network that is not ConsensusV2: the weight it caps is the V2 fold's \
                 bounded_immature",
            ));
        };
        let at_or_below =
            |f: Option<ForkActivation>| f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= fence.daa_score());
        if !at_or_below(self.palw_rcore_plus) {
            return Err(Invalid(
                "palw_capacity_weight_cap is armed without palw_rcore_plus at or below it: the staged weight reads R-core+'s \
                 licence record and the cap bounds the reservation R-core+ prices (ADR-0160 §6.1)",
            ));
        }
        if !at_or_below(self.palw_reorg_strict_economic_win) {
            return Err(Invalid(
                "palw_capacity_weight_cap is armed without palw_reorg_strict_economic_win at or below it: staged weight makes \
                 all-economic ties common, and without strict-win a deep reorg on a tie is decided by the hash (ADR-0160 §4.2)",
            ));
        }
        // rcore/cap-s1 — ONE shallow-tie rule, and F-W never without its no-DAA-lowering check. Past F-W the
        // deep-reorg gate is strict-win's own rule (the DAA-750 release's), whose shallow tie never moves the
        // sink to a lower DAA; staged weight makes all-economic ties common, and without that check chained
        // tie releases walk the sink back past the published two ticks (worse under F-W: an attempt-only
        // branch ties at zero ticks). A build whose strict-win rule dropped the check cannot arm F-W.
        if !crate::palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_NEVER_LOWERS_SINK_DAA_V1 {
            return Err(Invalid(
                "palw_capacity_weight_cap is armed on a build whose strict-win shallow-tie rule may LOWER the sink's DAA: \
                 F-W makes all-economic ties common and is never armable without the no-DAA-lowering check (rcore/cap-s1)",
            ));
        }
        // ADR-0160 §8 A1 (the lane's verify finding 3): past F-W a private branch gains live weight only by
        // STAGING claims (bind, licence) the public branch has not staged, and the public branch's own
        // post-fork claims weigh 0 until bound (≥ anchor_delay later) while its licences of pre-fork claims
        // can be imported — so the honest chain's un-importable gain inside a short lead is ~0, and any
        // positive private staging wins strict-win. The operator anchor is what stops a private branch
        // anchoring (and grinding) its own panels. Required until the structural P0-10 fix replaces it.
        if !at_or_below(self.palw_operator_anchor.as_ref().map(|rule| rule.activation)) {
            return Err(Invalid(
                "palw_capacity_weight_cap is armed without palw_operator_anchor at or below it: staged weight lets a private \
                 branch that anchors its own panels self-license up to its bonds' caps, which the public branch cannot \
                 match inside the lead, so strict-win would allow the deep reorg (ADR-0160 §8 A1)",
            ));
        }
        if mirror != Some(fence.daa_score()) {
            return Err(Invalid(
                "palw_capacity_weight_cap disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_capacity_weight_cap after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What lane sink's fence refuses.** Called by `validate_palw_v2`, public so a test can name
    /// each refusal. The binding is a rule about the market's `MSKMDL01` sink on a V2 lifecycle
    /// carrier, so it needs a ConsensusV2 ruleset that declares `palw_model_market` (at any height:
    /// below the market the header-context door refuses the form outright, so refusing an unbound
    /// one there too changes nothing). Its payee rule exists for the P-B1 refund of a refused buy or
    /// seed, which `palw_audit_2026_09_23` arms, so that fence must be armed at or below it. Any
    /// height is legal otherwise — genesis included, and on a live testnet-12 the common
    /// post-launch height the operator picks away from every scheduled one.
    pub fn validate_palw_model_sink_bound_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let Some(fence) = self.palw_model_sink_bound.filter(|fence| *fence != ForkActivation::never()) else {
            return Ok(());
        };
        if !matches!(self.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)) {
            return Err(Invalid(
                "palw_model_sink_bound is armed on a network that is not ConsensusV2: the model market's sink is a V2 \
                 lifecycle carrier's output",
            ));
        }
        if !self.palw_model_market.is_some_and(|market| market != ForkActivation::never()) {
            return Err(Invalid(
                "palw_model_sink_bound is armed without palw_model_market declared: there is no model sink form to bind",
            ));
        }
        if !self.palw_audit_2026_09_23.is_some_and(|audit| audit != ForkActivation::never() && audit.daa_score() <= fence.daa_score())
        {
            return Err(Invalid(
                "palw_model_sink_bound is armed without palw_audit_2026_09_23 at or below it: its payee rule exists for the \
                 P-B1 refund of a refused carrier buy or seed, which that fence arms",
            ));
        }
        Ok(())
    }

    /// **What F-EM, F-M1, F-K and the ρ ≥ 250 steps refuse** (ADR-0164 §6; called by `validate_palw_v2`, public so a test can name
    /// each refusal): any of the three armed off ConsensusV2 (the fold that reads it is not running); F-EM without F-L and F-E at or
    /// below it; F-M1 without F-EM, F-B, F-N, F-S, F-W and F-Q at or below it (a rider is a claim: it needs the budget it is paid
    /// from, the batch licence and audit door every claim leaves through, the network share and slots that bound it, and J-1); F-K
    /// without F-L, F-S and F-Q at or below it (its counters are the ramp's own observables); **an F-L step at ρ ≥ 250 without all
    /// three at or below the step's height** (×250 and ×1000 are not armable without the budget, the riders and the breaker); and a
    /// mirror that disagrees with a field.
    pub fn validate_palw_capacity_s567_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let armed = |fence: Option<ForkActivation>| fence.filter(|fence| *fence != ForkActivation::never());
        let (emission, riders, breaker) =
            (armed(self.palw_capacity_emission_budget), armed(self.palw_capacity_multi_claim), armed(self.palw_capacity_rho_breaker));
        let bundle = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle),
            _ => None,
        };
        let mirrored = bundle.map(|bundle| {
            (
                bundle.state.capacity_emission_from_daa(),
                bundle.state.capacity_riders_from_daa(),
                bundle.state.capacity_breaker_from_daa(),
            )
        });
        let want = (emission.map(|f| f.daa_score()), riders.map(|f| f.daa_score()), breaker.map(|f| f.daa_score()));
        if want == (None, None, None) {
            if mirrored.is_some_and(|mirrored| mirrored != (None, None, None)) {
                return Err(Invalid(
                    "the V2 bundle mirrors palw_capacity_emission_budget / palw_capacity_multi_claim / palw_capacity_rho_breaker \
                     without the fence armed: mirror them with Params::sync_palw_capacity_s567 after the bundle is assembled",
                ));
            }
            return Ok(());
        }
        let Some(mirrored) = mirrored else {
            return Err(Invalid(
                "palw_capacity_emission_budget / palw_capacity_multi_claim / palw_capacity_rho_breaker is armed off ConsensusV2: the \
                 fold that reads it is not running",
            ));
        };
        let at_or_below = |fence: Option<ForkActivation>, h: u64| {
            fence.filter(|f| *f != ForkActivation::never()).is_some_and(|f| f.daa_score() <= h)
        };
        let liability = self.palw_capacity_aggregate_liability.as_ref().filter(|value| value.activation != ForkActivation::never());
        let liability_below = |h: u64| liability.is_some_and(|value| value.activation.daa_score() <= h);
        if let Some(h) = emission.map(|f| f.daa_score())
            && !(liability_below(h) && at_or_below(self.palw_capacity_escrow_at_licence, h))
        {
            return Err(Invalid(
                "palw_capacity_emission_budget is armed without palw_capacity_aggregate_liability (F-L) and \
                 palw_capacity_escrow_at_licence (F-E) at or below it: the budget charges the carve a claim escrows at acceptance",
            ));
        }
        if let Some(h) = riders.map(|f| f.daa_score())
            && !(at_or_below(self.palw_capacity_emission_budget, h)
                && at_or_below(self.palw_capacity_batch_licence, h)
                && at_or_below(self.palw_capacity_network_room, h)
                && at_or_below(self.palw_capacity_issuance_slots, h)
                && at_or_below(self.palw_capacity_weight_cap, h)
                && at_or_below(self.palw_capacity_audit_door, h))
        {
            return Err(Invalid(
                "palw_capacity_multi_claim is armed without palw_capacity_emission_budget, palw_capacity_batch_licence, \
                 palw_capacity_network_room, palw_capacity_issuance_slots, palw_capacity_weight_cap and palw_capacity_audit_door all \
                 armed at or below it: a rider is a claim, and the budget it is paid from, the licence and audit it leaves through, \
                 the share and slots that bound it and J-1 must already hold",
            ));
        }
        if let Some(h) = breaker.map(|f| f.daa_score())
            && !(liability_below(h)
                && at_or_below(self.palw_capacity_issuance_slots, h)
                && at_or_below(self.palw_capacity_audit_door, h))
        {
            return Err(Invalid(
                "palw_capacity_rho_breaker is armed without palw_capacity_aggregate_liability, palw_capacity_issuance_slots and \
                 palw_capacity_audit_door at or below it: its counters are the ramp's own observables and its tier is the slots'",
            ));
        }
        if let Some(value) = liability {
            for step in value.steps.iter().filter(|step| step.rho >= crate::palw_audit_door_v1::PALW_CAPACITY_AUDIT_K2_RHO_V1) {
                let h = step.from_daa;
                if !(at_or_below(self.palw_capacity_emission_budget, h)
                    && at_or_below(self.palw_capacity_multi_claim, h)
                    && at_or_below(self.palw_capacity_rho_breaker, h))
                {
                    return Err(Invalid(
                        "palw_capacity_aggregate_liability has a step at rho >= 250 without palw_capacity_emission_budget, \
                         palw_capacity_multi_claim and palw_capacity_rho_breaker all armed at or below its height: x250 and x1000 \
                         need the per-DAA budget, the riders and the breaker",
                    ));
                }
            }
        }
        if mirrored != want {
            return Err(Invalid(
                "palw_capacity_emission_budget / palw_capacity_multi_claim / palw_capacity_rho_breaker disagrees with the V2 bundle's \
                 mirror: mirror them with Params::sync_palw_capacity_s567 after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What the verification term refuses** (int-11). Called by `validate_palw_v2`, public so a test can name each
    /// refusal: armed off ConsensusV2 (the fold that reads it is not running); armed without F-N
    /// (`palw_capacity_network_room`) at or below it (the term caps F-N's level); the mirror unequal. Below the fence it
    /// checks only that the bundle's mirror is empty.
    pub fn validate_palw_capacity_network_verify_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let fence = self.palw_capacity_network_verify.filter(|fence| *fence != ForkActivation::never());
        let bundle = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle),
            _ => None,
        };
        let Some(fence) = fence else {
            if let Some(bundle) = bundle
                && bundle.state.capacity_network_verify_from_daa().is_some()
            {
                return Err(Invalid(
                    "the V2 bundle mirrors palw_capacity_network_verify without the fence armed: mirror it with \
                     Params::sync_palw_capacity_network_verify after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let Some(bundle) = bundle else {
            return Err(Invalid("palw_capacity_network_verify is armed off ConsensusV2: the fold that reads it is not running"));
        };
        let h = fence.daa_score();
        let network_below =
            self.palw_capacity_network_room.filter(|f| *f != ForkActivation::never()).is_some_and(|f| f.daa_score() <= h);
        if !network_below {
            return Err(Invalid(
                "palw_capacity_network_verify is armed without palw_capacity_network_room at or below it: the term caps F-N's \
                 level, and there is no level to cap before F-N",
            ));
        }
        if bundle.state.capacity_network_verify_from_daa() != Some(h) {
            return Err(Invalid(
                "palw_capacity_network_verify disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_capacity_network_verify after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What [`Self::palw_panel_unavailable_expiry`] refuses** (lane PL). Called by `validate_palw_v2`, public so a test can name each refusal:
    /// armed off ConsensusV2; armed without a prerequisite at or below it; the mirror unequal. Below the fence it checks only
    /// that the bundle's mirror is empty.
    pub fn validate_palw_panel_unavailable_expiry_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let fence = self.palw_panel_unavailable_expiry.filter(|fence| *fence != ForkActivation::never());
        let bundle = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle),
            _ => None,
        };
        let Some(fence) = fence else {
            if let Some(bundle) = bundle
                && bundle.state.panel_unavailable_expiry_from_daa().is_some()
            {
                return Err(Invalid(
                    "the V2 bundle mirrors palw_panel_unavailable_expiry without the fence armed: mirror it with \
                     Params::sync_palw_panel_unavailable_expiry after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let Some(bundle) = bundle else {
            return Err(Invalid("palw_panel_unavailable_expiry is armed off ConsensusV2: the fold that reads it is not running"));
        };
        let h = fence.daa_score();
        let palw_audit_2026_09_23_below =
            self.palw_audit_2026_09_23.filter(|f| *f != ForkActivation::never()).is_some_and(|f| f.daa_score() <= h);
        if !palw_audit_2026_09_23_below {
            return Err(Invalid(
                "palw_panel_unavailable_expiry is armed without palw_audit_2026_09_23 at or below it: the second-timeout charge it removes exists only past that fence",
            ));
        }
        if bundle.state.panel_unavailable_expiry_from_daa() != Some(h) {
            return Err(Invalid(
                "palw_panel_unavailable_expiry disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_panel_unavailable_expiry after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What [`Self::palw_panel_fast_switch`] refuses** (lane PL). Called by `validate_palw_v2`, public so a test can name each refusal:
    /// armed off ConsensusV2; armed without a prerequisite at or below it; the mirror unequal. Below the fence it checks only
    /// that the bundle's mirror is empty.
    pub fn validate_palw_panel_fast_switch_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let fence = self.palw_panel_fast_switch.filter(|fence| *fence != ForkActivation::never());
        let bundle = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle),
            _ => None,
        };
        let Some(fence) = fence else {
            if let Some(bundle) = bundle
                && bundle.state.panel_fast_switch_from_daa().is_some()
            {
                return Err(Invalid(
                    "the V2 bundle mirrors palw_panel_fast_switch without the fence armed: mirror it with \
                     Params::sync_palw_panel_fast_switch after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let Some(bundle) = bundle else {
            return Err(Invalid("palw_panel_fast_switch is armed off ConsensusV2: the fold that reads it is not running"));
        };
        let h = fence.daa_score();
        let palw_panel_unavailable_expiry_below =
            self.palw_panel_unavailable_expiry.filter(|f| *f != ForkActivation::never()).is_some_and(|f| f.daa_score() <= h);
        if !palw_panel_unavailable_expiry_below {
            return Err(Invalid(
                "palw_panel_fast_switch is armed without palw_panel_unavailable_expiry at or below it: the redraw's hard window must end in the uncharged expiry",
            ));
        }
        let palw_rcore_plus_below = self.palw_rcore_plus.filter(|f| *f != ForkActivation::never()).is_some_and(|f| f.daa_score() <= h);
        if !palw_rcore_plus_below {
            return Err(Invalid(
                "palw_panel_fast_switch is armed without palw_rcore_plus at or below it: the fast switch rides the redraw of the stake-weighted draw (SW-8)",
            ));
        }
        if bundle.state.panel_fast_switch_from_daa() != Some(h) {
            return Err(Invalid(
                "palw_panel_fast_switch disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_panel_fast_switch after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What [`Self::palw_seat_availability`] refuses** (lane PL). Called by `validate_palw_v2`, public so a test can name each refusal:
    /// armed off ConsensusV2; armed without a prerequisite at or below it; the mirror unequal. Below the fence it checks only
    /// that the bundle's mirror is empty.
    pub fn validate_palw_seat_availability_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let fence = self.palw_seat_availability.filter(|fence| *fence != ForkActivation::never());
        let bundle = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle),
            _ => None,
        };
        let Some(fence) = fence else {
            if let Some(bundle) = bundle
                && bundle.state.seat_availability_from_daa().is_some()
            {
                return Err(Invalid(
                    "the V2 bundle mirrors palw_seat_availability without the fence armed: mirror it with \
                     Params::sync_palw_seat_availability after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let Some(bundle) = bundle else {
            return Err(Invalid("palw_seat_availability is armed off ConsensusV2: the fold that reads it is not running"));
        };
        let h = fence.daa_score();
        let palw_rcore_plus_below = self.palw_rcore_plus.filter(|f| *f != ForkActivation::never()).is_some_and(|f| f.daa_score() <= h);
        if !palw_rcore_plus_below {
            return Err(Invalid(
                "palw_seat_availability is armed without palw_rcore_plus at or below it: the factor weights the stake-weighted race",
            ));
        }
        if bundle.state.seat_availability_from_daa() != Some(h) {
            return Err(Invalid(
                "palw_seat_availability disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_seat_availability after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What F-Q and F-S refuse** (ADR-0160 v3 §10.1). Called by `validate_palw_v2`, public so a test
    /// can name each refusal. Below both fences it checks only that the bundle's mirrors are empty.
    ///
    /// * F-Q: ConsensusV2; `palw_capacity_aggregate_liability` (the steps whose credit it gates),
    ///   `palw_capacity_batch_licence` (a receipt batch rides beside the licence lists) and
    ///   `palw_operator_anchor` (the pool is its operator bonds) at or below it; the mirror equal.
    /// * F-S: ConsensusV2; `palw_capacity_weight_cap`, `palw_capacity_aggregate_liability` and
    ///   `palw_capacity_escrow_at_licence` at or below it (the slot's life is the stage commitment's);
    ///   the mirror equal.
    /// * **A credited step needs the audit door** (D-23, v3 D.3): every step of F-L's schedule whose
    ///   credit applies (`palw_escrow_credit_applies_v1`: the step's credit reaches `q_seat`) starts at
    ///   or past F-Q — a credit without the door would pay a fraud that commits less than its reward.
    pub fn validate_palw_capacity_stage2_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let armed = |fence: Option<ForkActivation>| fence.filter(|fence| *fence != ForkActivation::never());
        let (audit, slots) = (armed(self.palw_capacity_audit_door), armed(self.palw_capacity_issuance_slots));
        let network = armed(self.palw_capacity_network_room);
        // A malformed value is refused by its own validator by name; the door's rule reads a well-formed one.
        // A value armed at `never()` is no fence (the collapse's reading): it credits nothing.
        let credited_from = self
            .palw_capacity_aggregate_liability
            .as_ref()
            .filter(|value| value.activation != ForkActivation::never() && value.refusal_v1().is_none())
            .and_then(|value| {
                value
                    .steps
                    .iter()
                    .filter(|step| {
                        crate::palw_escrow_funding_v2::palw_escrow_credit_applies_v1(
                            &crate::palw_escrow_funding_v2::PalwEscrowCreditV1 {
                                rho: step.rho,
                                q_credit_permille: step.q_credit_permille,
                            },
                        )
                    })
                    .map(|step| step.from_daa)
                    .min()
            });
        if let Some(from) = credited_from
            && audit.is_none_or(|fence| fence.daa_score() > from)
        {
            return Err(Invalid(
                "palw_capacity_aggregate_liability credits a step without palw_capacity_audit_door (F-Q) at or below it: a \
                 credited claim commits less than its reward, so it must reach Final only through its audit (ADR-0160 v3 D-23)",
            ));
        }
        let bundle = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle),
            _ => None,
        };
        if audit.is_none() && slots.is_none() && network.is_none() {
            if let Some(bundle) = bundle
                && (bundle.state.capacity_audit_from_daa().is_some()
                    || bundle.state.capacity_slots_from_daa().is_some()
                    || bundle.state.capacity_network_from_daa().is_some())
            {
                return Err(Invalid(
                    "the V2 bundle mirrors palw_capacity_audit_door or palw_capacity_issuance_slots without the fence armed: \
                     mirror them with Params::sync_palw_capacity_stage2 after the bundle is assembled",
                ));
            }
            return Ok(());
        }
        let Some(bundle) = bundle else {
            return Err(Invalid(
                "palw_capacity_audit_door or palw_capacity_issuance_slots is armed on a network that is not ConsensusV2: the \
                 claims they gate are the V2 fold's",
            ));
        };
        let at_or_below = |fence: Option<ForkActivation>, height: u64| armed(fence).is_some_and(|f| f.daa_score() <= height);
        if let Some(audit) = audit {
            let h = audit.daa_score();
            let liability_at = self.palw_capacity_aggregate_liability.as_ref().map(|value| value.activation);
            if !(at_or_below(liability_at, h)
                && at_or_below(self.palw_capacity_batch_licence, h)
                && at_or_below(self.palw_operator_anchor.as_ref().map(|anchor| anchor.activation), h))
            {
                return Err(Invalid(
                    "palw_capacity_audit_door is armed without palw_capacity_aggregate_liability, palw_capacity_batch_licence and \
                     palw_operator_anchor at or below it: the door gates F-L's credited steps, its receipts ride beside the batch \
                     licences, and its pool is lane A's operator bonds (ADR-0160 v3 §10.1)",
                ));
            }
        }
        if let Some(network) = network {
            let h = network.daa_score();
            if !(at_or_below(self.palw_capacity_verify_room, h)
                && at_or_below(self.palw_capacity_issuance_slots, h)
                && at_or_below(self.palw_operator_anchor.as_ref().map(|anchor| anchor.activation), h))
            {
                return Err(Invalid(
                    "palw_capacity_network_room is armed without palw_capacity_verify_room, palw_capacity_issuance_slots and \
                     palw_operator_anchor at or below it: the level reads F-R's seat room, the share rides lane S's slots, and \
                     the carriage and anchor terms count lane A's operator attempts (ADR-0160 v3 §10.1)",
                ));
            }
        }
        if let Some(slots) = slots {
            let h = slots.daa_score();
            let liability_at = self.palw_capacity_aggregate_liability.as_ref().map(|value| value.activation);
            if !(at_or_below(self.palw_capacity_weight_cap, h)
                && at_or_below(liability_at, h)
                && at_or_below(self.palw_capacity_escrow_at_licence, h))
            {
                return Err(Invalid(
                    "palw_capacity_issuance_slots is armed without palw_capacity_weight_cap, palw_capacity_aggregate_liability and \
                     palw_capacity_escrow_at_licence at or below it: the slot lives as long as the stage commitment and is priced \
                     at the step's ρ (ADR-0160 v3 §10.1)",
                ));
            }
        }
        let operators = match (audit.is_some() || network.is_some(), &self.palw_operator_anchor) {
            (true, Some(anchor)) => anchor.operators.clone(),
            _ => Vec::new(),
        };
        if bundle.state.capacity_network_from_daa() != network.map(|f| f.daa_score()) {
            return Err(Invalid(
                "palw_capacity_network_room disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_capacity_stage2 \
                 after the bundle is assembled",
            ));
        }
        if bundle.state.capacity_audit_from_daa() != audit.map(|f| f.daa_score())
            || bundle.state.capacity_audit_operators() != operators.as_slice()
            || bundle.state.capacity_slots_from_daa() != slots.map(|f| f.daa_score())
        {
            return Err(Invalid(
                "palw_capacity_audit_door or palw_capacity_issuance_slots disagrees with the V2 bundle's mirror: mirror them \
                 with Params::sync_palw_capacity_stage2 after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What F-B and F-R refuse** (ADR-0160 §6.1). Called by `validate_palw_v2`, public so a test can
    /// name each refusal. Below both fences it checks only that the bundle's mirrors are `None`.
    ///
    /// * F-B: ConsensusV2; `palw_rcore_plus` and `palw_verification_v2` at or below it (a batch entry
    ///   is fed to R-core+'s coverage funnel); the mirror equal to the height.
    /// * F-R: ConsensusV2; `palw_rcore_plus` (T-2(a), which the share replaces, is R-core+'s),
    ///   `palw_audit_2026_09_23` (the rate room it re-prices) and `palw_offence_attribution` (the
    ///   convictions whose freeze bounds the junk-DoS the share opens) and — rcore/cap-s1, ADR-0160 v3
    ///   §10.1 — lane liab's `palw_capacity_aggregate_liability` (the freeze the share reads) at or below
    ///   it; the mirror equal to the height.
    ///
    /// Any height is admissible otherwise — genesis included, and on the live testnet-12 the one common
    /// capacity height `H_cap` the operator picks away from every scheduled one.
    pub fn validate_palw_capacity_verify_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let mirrors = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                Some((bundle.state.capacity_batch_from_daa(), bundle.state.capacity_room_from_daa()))
            }
            _ => None,
        };
        let armed = |fence: Option<ForkActivation>| fence.filter(|fence| *fence != ForkActivation::never());
        let (batch, room) = (armed(self.palw_capacity_batch_licence), armed(self.palw_capacity_verify_room));
        if batch.is_none() && room.is_none() {
            if mirrors.is_some_and(|(b, r)| b.is_some() || r.is_some()) {
                return Err(Invalid(
                    "the V2 bundle carries an ADR-0160 verify height without palw_capacity_batch_licence or \
                     palw_capacity_verify_room armed: mirror them with Params::sync_palw_capacity_verify after the bundle is \
                     assembled",
                ));
            }
            return Ok(());
        }
        let Some((batch_mirror, room_mirror)) = mirrors else {
            return Err(Invalid(
                "palw_capacity_batch_licence or palw_capacity_verify_room is armed on a network that is not ConsensusV2: the \
                 licences and the room they change are the V2 panel's",
            ));
        };
        let at_or_below = |f: Option<ForkActivation>, h: ForkActivation| {
            f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= h.daa_score())
        };
        if let Some(fence) = batch
            && !(at_or_below(self.palw_rcore_plus, fence) && at_or_below(self.palw_verification_v2, fence))
        {
            return Err(Invalid(
                "palw_capacity_batch_licence is armed without palw_rcore_plus and palw_verification_v2 both at or below it: \
                 a batch entry is R-core+'s coverage licence, fed to the same funnel",
            ));
        }
        if let Some(fence) = room {
            if !(at_or_below(self.palw_rcore_plus, fence)
                && at_or_below(self.palw_audit_2026_09_23, fence)
                && at_or_below(self.palw_offence_attribution, fence))
            {
                return Err(Invalid(
                    "palw_capacity_verify_room is armed without palw_rcore_plus, palw_audit_2026_09_23 and \
                     palw_offence_attribution all at or below it: the share replaces R-core+'s T-2(a), the capacity re-prices \
                     the audit's rate room, and the junk-DoS bound needs the convictions that freeze a flooding bond",
                ));
            }
            // rcore/cap-s1 (ADR-0160 v3 §10.1): F-L at or below F-R — a frozen bond holds no share (AG-3).
            if !at_or_below(self.palw_capacity_aggregate_liability.as_ref().map(|value| value.activation), fence) {
                return Err(Invalid(
                    "palw_capacity_verify_room is armed without palw_capacity_aggregate_liability (F-L) at or below it: the \
                     share leaves a frozen bond nothing, and the freeze is F-L's (ADR-0160 v3 §10.1)",
                ));
            }
        }
        if batch_mirror != batch.map(|f| f.daa_score()) || room_mirror != room.map(|f| f.daa_score()) {
            return Err(Invalid(
                "palw_capacity_batch_licence or palw_capacity_verify_room disagrees with the V2 bundle's mirror: mirror them \
                 with Params::sync_palw_capacity_verify after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What lane F1's fence refuses.** Only one thing: arming it where an anchor may be a block
    /// with no execution commitment. Past `palw_rcore_plus` only attempt blocks anchor a panel
    /// (`palw_block_may_anchor_a_panel_v1`), so every anchor carries the commitment the seed is
    /// keyed on; below it a heartbeat may anchor and the seed would name nothing. So the fence needs
    /// `palw_rcore_plus` armed at or below it, on a ConsensusV2 ruleset. Any height is legal
    /// otherwise — genesis included (a new network may be born with the rule), and on a live
    /// testnet-12 a post-launch height the operator picks away from every scheduled one (1,000 is
    /// `palw_bond_maturity`'s, and a fence sharing a height is invisible to the fork id).
    pub fn validate_palw_panel_seed_execution_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let Some(fence) = self.palw_panel_seed_execution.filter(|f| *f != ForkActivation::never()) else {
            return Ok(());
        };
        if !matches!(self.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)) {
            return Err(Invalid(
                "palw_panel_seed_execution is armed on a network that is not ConsensusV2: there is no V2 panel to seed",
            ));
        }
        if !self.palw_rcore_plus.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= fence.daa_score()) {
            return Err(Invalid(
                "palw_panel_seed_execution is armed without palw_rcore_plus at or below it: below R-core+ a heartbeat may \
                 anchor a panel, and a heartbeat has no execution commitment to key the seed on",
            ));
        }
        Ok(())
    }

    /// **What lane A's fence refuses**: arming it off ConsensusV2 (there is no V2 panel to anchor);
    /// without `palw_rcore_plus` at or below it (below R-core+ a heartbeat may anchor a panel, and the
    /// rule is written over the attempt-only lane rule R-core+ installs); without lane F1
    /// (`palw_panel_seed_execution`) at or below it (the displacement-proof seed is an execution
    /// commitment read off the earliest operator attempt, which only F1's seed rule carries); and a value
    /// [`crate::palw_operator_anchor_v1::PalwOperatorAnchorV1::refusal_v1`] refuses — an empty operator
    /// list, one not sorted and distinct, or one naming a bond the genesis does not register. Any
    /// height is legal otherwise; on a live testnet-12 the operator picks one post-launch height for
    /// every post-launch fence, away from every scheduled one (never 1,000, `palw_bond_maturity`'s).
    pub fn validate_palw_operator_anchor_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let Some(value) = self.palw_operator_anchor.as_ref().filter(|rule| rule.activation != ForkActivation::never()) else {
            return Ok(());
        };
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else {
            return Err(Invalid("palw_operator_anchor is armed on a network that is not ConsensusV2: there is no V2 panel to anchor"));
        };
        if !self.palw_rcore_plus.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= value.activation.daa_score()) {
            return Err(Invalid(
                "palw_operator_anchor is armed without palw_rcore_plus at or below it: below R-core+ a heartbeat may anchor a \
                 panel, and the operator rule narrows R-core+'s attempt-only anchor lane",
            ));
        }
        if !self
            .palw_panel_seed_execution
            .is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= value.activation.daa_score())
        {
            return Err(Invalid(
                "palw_operator_anchor is armed without palw_panel_seed_execution (lane F1) at or below it: past lane A the seed is \
                 read off the earliest operator attempt at or past the slot as an execution commitment, so a displaced operator \
                 attempt still keys the draw — without F1 the seed would be the binding block's identity, and displacing the \
                 anchor would re-roll the panel",
            ));
        }
        if let Some(why) = value.refusal_v1(&bundle.genesis_objects) {
            return Err(Invalid(why));
        }
        Ok(())
    }

    /// **What F-L refuses** (ADR-0160 §6.1, the contract's validation): arming it off ConsensusV2 (there
    /// is no V2 fold to forfeit in); without `palw_rcore_plus` (the conviction funnel, the one ledger
    /// and the seat prices it re-prices are R-core+'s) and `palw_offence_attribution` (the intent class
    /// is read off the attribution route's contradictions) armed at or below it; a value
    /// [`crate::palw_aggregate_liability_v1::PalwCapacityLiabilityV1::refusal_v1`] refuses (no step, a
    /// first step off the fence's height, heights not strictly increasing, ρ = 0, q > 1000‰); and a
    /// bundle mirror that disagrees with the field. Below the fence it checks only that the mirror is
    /// `None`. Any height is admissible otherwise; on a live testnet-12 the capacity release arms it at
    /// the one common height `H_cap` (§6.4).
    pub fn validate_palw_capacity_liability_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let mirror = match &self.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.state.capacity_liability().cloned()),
            _ => None,
        };
        let Some(value) = self.palw_capacity_aggregate_liability.as_ref().filter(|value| value.activation != ForkActivation::never())
        else {
            if mirror.flatten().is_some() {
                return Err(Invalid(
                    "the V2 bundle carries an aggregate-liability value without palw_capacity_aggregate_liability armed: mirror \
                     the fence with Params::sync_palw_capacity_liability after the bundle is assembled",
                ));
            }
            return Ok(());
        };
        let Some(mirror) = mirror else {
            return Err(Invalid(
                "palw_capacity_aggregate_liability is armed on a network that is not ConsensusV2: there is no V2 fold whose \
                 convictions it could aggregate",
            ));
        };
        let at_or_below = |f: Option<ForkActivation>| {
            f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= value.activation.daa_score())
        };
        if !(at_or_below(self.palw_rcore_plus) && at_or_below(self.palw_offence_attribution)) {
            return Err(Invalid(
                "palw_capacity_aggregate_liability is armed without palw_rcore_plus and palw_offence_attribution both armed at or \
                 below it: the conviction funnel, the one ledger and the seat prices it re-prices are R-core+'s, and its intent \
                 class is read off the attribution route's contradictions",
            ));
        }
        if let Some(why) = value.refusal_v1() {
            return Err(Invalid(why));
        }
        if mirror.as_ref() != Some(value) {
            return Err(Invalid(
                "palw_capacity_aggregate_liability disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_capacity_liability after the bundle is assembled",
            ));
        }
        Ok(())
    }

    /// **What lane bind-deadlock's fence refuses**: arming it off ConsensusV2; without
    /// `palw_rcore_plus` at or below it — below R-core+ every lane but the receipt and round lanes
    /// anchors (a heartbeat among them), so a claim never waits on an attempt and there is no deadlock
    /// to break; the rule is written for SW-8's one-anchor-block world; and without lane A
    /// (`palw_operator_anchor`) at or below it (the int-4 audit's LOW) — the binder's second condition
    /// is lane A's operator predicate (`palw_v2_anchor_at_ceiling_binder_v1`), so below lane A every
    /// bonded attempt at its ceiling, not only an operator's own, would bind the claims due at it. Any other height
    /// is legal; on a live testnet-12 the post-launch release arms it with the other post-launch fences
    /// at one independent height (never 1,000, `palw_bond_maturity`'s).
    pub fn validate_palw_anchor_at_ceiling_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let Some(fence) = self.palw_anchor_at_ceiling.filter(|f| *f != ForkActivation::never()) else {
            return Ok(());
        };
        if !matches!(self.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)) {
            return Err(Invalid(
                "palw_anchor_at_ceiling is armed on a network that is not ConsensusV2: there is no V2 anchor to bind",
            ));
        }
        if !self.palw_rcore_plus.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= fence.daa_score()) {
            return Err(Invalid(
                "palw_anchor_at_ceiling is armed without palw_rcore_plus at or below it: below R-core+ a heartbeat anchors a panel, \
                 so no claim waits on an attempt block and there is no ceiling deadlock to break",
            ));
        }
        if !self
            .palw_operator_anchor
            .as_ref()
            .is_some_and(|rule| rule.activation != ForkActivation::never() && rule.activation.daa_score() <= fence.daa_score())
        {
            return Err(Invalid(
                "palw_anchor_at_ceiling is armed without palw_operator_anchor (lane A) at or below it: the binder is an operator's \
                 own attempt at its bond's exposure ceiling, judged by lane A's operator predicate — below lane A any bonded \
                 attempt at its ceiling would bind the claims due at it",
            ));
        }
        Ok(())
    }

    /// **What lane accept-order's fence refuses**: arming it off ConsensusV2, or without the execution
    /// lane opening at or below it — without round blocks no two blocks of a mergeset tie on blue work
    /// while one is in the other's past, so there is nothing to reorder and no round block to refuse
    /// an EVM payload. Any other height is legal; on a live testnet-12 (lane open from genesis) the
    /// post-launch release arms it with the other post-launch fences at one independent height.
    pub fn validate_palw_lane_accept_parents_first_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::palw_mode_v2::PalwModeV2Error::Invalid;
        let Some(fence) = self.palw_lane_accept_parents_first.filter(|f| *f != ForkActivation::never()) else {
            return Ok(());
        };
        if !matches!(self.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)) {
            return Err(Invalid(
                "palw_lane_accept_parents_first is armed on a network that is not ConsensusV2: there is no round lane to order",
            ));
        }
        if !self
            .palw_execution_lane
            .is_some_and(|lane| lane.activation != ForkActivation::never() && lane.activation.daa_score() <= fence.daa_score())
        {
            return Err(Invalid(
                "palw_lane_accept_parents_first is armed without palw_execution_lane opening at or below it: without round blocks \
                 no mergeset holds a blue-work tie between a block and its parent, so there is nothing to order",
            ));
        }
        Ok(())
    }
}
