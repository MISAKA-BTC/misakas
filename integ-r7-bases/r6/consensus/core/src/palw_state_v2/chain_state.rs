//! **The chain state's readers and helpers (impl PalwChainStateV2)** — moved out of `palw_state_v2.rs` (2026-10-12, a behaviour-preserving split: the items are the same; members the parent calls are `pub(super)`).

use super::*;

impl PalwChainStateV2 {
    /// The empty state a V2 network boots from. No bonds, no classes, no weight, frontier at the
    /// zero point.
    pub fn genesis() -> Self {
        Self {
            bonds: BTreeMap::new(),
            reserved_exposure: BTreeMap::new(),
            classes: BTreeMap::new(),
            class_targets: BTreeMap::new(),
            class_shares: BTreeMap::new(),
            epoch_budgets: None,
            receipt_targets: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            pending_payouts: BTreeMap::new(),
            claims: BTreeMap::new(),
            panels: BTreeMap::new(),
            court_sessions: BTreeMap::new(),
            epoch_counters: BTreeMap::new(),
            receipt_epoch_counters: BTreeMap::new(),
            model_markets: BTreeMap::new(),
            model_positions: BTreeMap::new(),
            model_lines: BTreeMap::new(),
            model_versions: BTreeMap::new(),
            model_benefits: BTreeMap::new(),
            model_position_since: BTreeMap::new(),
            model_proposals: BTreeMap::new(),
            model_evaluations: BTreeMap::new(),
            claim_roots: BTreeMap::new(),
            evm_settlements: BTreeMap::new(),
            class_shard_plans: BTreeMap::new(),
            bond_shards: BTreeMap::new(),
            shard_licensing: BTreeMap::new(),
            held_da_missing: BTreeMap::new(),
            held_leaf_demands: BTreeMap::new(),
            class_step_ladders: BTreeMap::new(),
            class_court_windows: BTreeMap::new(),
            vertex: crate::palw_vertex_v1::PalwVertexStateV1::default(),
            exec_v2: crate::palw_work_slice_v2::PalwExecV2StateV1::default(),
            exec_v2_rewards: BTreeMap::new(),
            exec_v2_reward_holds: BTreeMap::new(),
            exec_v2_origins: BTreeMap::new(),
            exec_v2_job_uses: BTreeMap::new(),
            exec_v2_job_use_deadlines: BTreeSet::new(),
            exec_v2_reward_payees: BTreeMap::new(),
            tir_shard_plans: BTreeMap::new(),
            tir_shard_claims: BTreeMap::new(),
            floor_state: None,
            seat_availability: BTreeMap::new(),
            seat_root_readiness: BTreeMap::new(),
            kernel_route: None,
            bond_budget: None,
            panel_duties: BTreeMap::new(),
            panel_reserve_sompi: 0,
            round_span: 0,
            round_finals: BTreeMap::new(),
            round_schedules: BTreeMap::new(),
            round_pending: BTreeMap::new(),
            round_permits_used: BTreeMap::new(),
            round_equivocations: BTreeMap::new(),
            round_seed_anchor: None,
            model_lifecycles: BTreeMap::new(),
            seat_readiness: BTreeMap::new(),
            claim_economics: BTreeMap::new(),
            work_target_shadow: None,
            final_work: BTreeMap::new(),
            work_target: None,
            artifact_owners: BTreeMap::new(),
            fp_work_profiles: BTreeMap::new(),
            fp_claim_prompts: BTreeMap::new(),
            consumed_offences: BTreeMap::new(),
            slashable_locks: BTreeMap::new(),
            panel_liabilities: BTreeMap::new(),
            registration_exposure: BTreeMap::new(),
            class_walks: BTreeMap::new(),
            certified_families: BTreeMap::new(),
            fp_certified_families: BTreeMap::new(),
            fp_certified_classes: BTreeMap::new(),
            pending_chunks: BTreeMap::new(),
            court_close_groups: BTreeMap::new(),
            derived_artifacts: BTreeMap::new(),
            safe_weight: 0,
            bounded_immature: 0,
            safe_frontier_blue_score: 0,
            safe_frontier: ZERO_HASH64,
            last_point: None,
            retired_safe_weight: 0,
            settled_attempt_finals: 0,
            recent_anchor_daas: Vec::new(),
            withholding_strikes: BTreeMap::new(),
            reward_pending: BTreeMap::new(),
            reporter_commitments: BTreeMap::new(),
            reporter_rewards: BTreeMap::new(),
            reporter_counters: PalwReporterCountersV1::default(),
            vesting: BTreeMap::new(),
            vesting_counters: crate::palw_vesting_v1::PalwVestingCountersV1::default(),
            da_sessions: BTreeMap::new(),
            da_claims: BTreeMap::new(),
            activation_pools: BTreeMap::new(),
            activation_pool_counters: Default::default(),
            activation_pool_scheduled: BTreeMap::new(),
            activation_readiness_landed: BTreeMap::new(),
            held_forfeits: BTreeMap::new(),
            probation_memory: BTreeMap::new(),
            bond_freezes: BTreeMap::new(),
            audit_receipts: BTreeMap::new(),
            excluded_auditors: BTreeMap::new(),
            issuance_buckets: BTreeMap::new(),
            network_demand: BTreeMap::new(),
            operator_ring: BTreeMap::new(),
            capacity_ledger: BTreeMap::new(),
            tir_classes: BTreeMap::new(),
            tir_dissections: BTreeMap::new(),
            gen_classes: BTreeMap::new(),
            improvement_lines: BTreeMap::new(),
            improvement_epochs: BTreeMap::new(),
            improvement_policies: BTreeMap::new(),
            improvement_usage: BTreeMap::new(),
            improvement_heads: BTreeMap::new(),
            improvement_pools: BTreeMap::new(),
            improvement_material: BTreeMap::new(),
            improvement_candidates: BTreeMap::new(),
            improvement_pool_entries: BTreeMap::new(),
            improvement_items: BTreeMap::new(),
            improvement_results: BTreeMap::new(),
            improvement_grants: BTreeMap::new(),
            improvement_earnings: BTreeMap::new(),
            improvement_cases: BTreeMap::new(),
            improvement_opt_ins: BTreeMap::new(),
            improvement_setter_sets: BTreeMap::new(),
            improvement_datasets: BTreeMap::new(),
            improvement_artifacts: BTreeMap::new(),
            improvement_licences: BTreeMap::new(),
            improvement_composite_classes: BTreeMap::new(),
            improvement_eval_jobs: BTreeMap::new(),
            panel_v3: None,
            legacy_disputes: BTreeMap::new(),
            tir_designated: Default::default(),
            funded_escrows: BTreeMap::new(),
            d2_readiness: BTreeMap::new(),
            fp_public_jobs: BTreeMap::new(),
            class_semantics: BTreeMap::new(),
            claim_semantics: BTreeMap::new(),
            legacy_answers: BTreeMap::new(),
            terminal_evidence: BTreeMap::new(),
            staged_courts: BTreeMap::new(),
            staged_holds: BTreeSet::new(),
            staged_due: BTreeSet::new(),
            staged_by_owner: BTreeSet::new(),
            staged_inventory_by_owner: BTreeSet::new(),
            staged_tir_by_owner: BTreeSet::new(),
            staged_tir_by_claim: BTreeMap::new(),
            staged_court_service_by_claim: BTreeMap::new(),
            staged_tir_claims_by_producer: BTreeSet::new(),
            staged_tir_responders: BTreeSet::new(),
            staged_tir_preparers: BTreeSet::new(),
            staged_prefix_responders: BTreeSet::new(),
            staged_selected_responders: BTreeSet::new(),
            staged_flat_responders: BTreeSet::new(),
            staged_history_responders: BTreeSet::new(),
            improvement_due: BTreeSet::new(),
            improvement_heads_of: BTreeMap::new(),
            improvement_retiring: BTreeSet::new(),
            improvement_open_epochs: 0,
            improvement_live_results: 0,
            improvement_artifact_outputs: BTreeSet::new(),
            improvement_artifact_answers: BTreeSet::new(),
            improvement_artifact_deadlines: BTreeSet::new(),
            improvement_material_epochs: BTreeSet::new(),
            improvement_opt_in_expiries: BTreeSet::new(),
            improvement_licence_expiries: BTreeSet::new(),
            improvement_eval_claims: BTreeMap::new(),
            deadlines: BTreeSet::new(),
            unresolved: BTreeSet::new(),
            work_ids: BTreeMap::new(),
            open_courts_by_claim: BTreeMap::new(),
            court_deadlines: BTreeSet::new(),
            court_close_deadlines: BTreeSet::new(),
            courts_by_challenger: BTreeSet::new(),
            commitments_by_reporter: BTreeMap::new(),
            commitments_by_daa: BTreeSet::new(),
            vesting_order: BTreeSet::new(),
            vesting_unlatched: BTreeSet::new(),
            vesting_unlatched_by_settled: BTreeSet::new(),
            vesting_payees: BTreeSet::new(),
            da_deadlines: BTreeSet::new(),
            da_by_accuser: BTreeSet::new(),
            legacy_dispute_deadlines: BTreeSet::new(),
            legacy_disputes_by_reserver: BTreeSet::new(),
            capacity_weight_index: Default::default(),
        }
    }

    // ---- read access (no mutators are public) ----

    pub fn bond(&self, key: &PalwBondKeyV2) -> Option<&PalwBondStateV2> {
        self.bonds.get(key)
    }

    /// Every registered class id, whatever its status — the transition refuses a duplicate for
    /// any non-Dormant status, so a registration builder filters against the whole set.
    pub fn class_ids(&self) -> Vec<Hash64> {
        self.classes.keys().copied().collect()
    }

    /// Every LIVE registration's pinned artifact root — the companion of [`Self::class_ids`], for
    /// the registration builder's known-weights filter.
    ///
    /// Filtered to `Active` and `Registered` (audit M2-17). A class reclaimed to `Dormant` keeps
    /// its row, and ADR-0056 Decision 5 makes that row's id the one path back — "a fresh
    /// signature, a fresh exposure reservation, a fresh soak, and the grant floor again". With
    /// reclaimed roots in this list the shipped node refused to re-register the very weights that
    /// re-entry is for, and the only alternative — a changed profile — is a different class and
    /// another permanent registry row.
    pub fn class_artifact_roots(&self) -> Vec<Hash64> {
        self.classes
            .values()
            .filter(|record| matches!(record.status, PalwClassStatusV2::Active | PalwClassStatusV2::Registered { .. }))
            .map(|record| record.artifact_root)
            .collect()
    }

    pub fn class(&self, id: &Hash64) -> Option<&PalwClassStateV2> {
        self.classes.get(id)
    }

    /// **C-08 (mainnet audit 2026-09-11 deep fence): can an honest responder build the fused
    /// ROOT-CLAIM evidence for `class_id` within a node's memory?** C-08 lifts the fused-terminal
    /// mercy past the deep fence and convicts a responder that withholds the root claim — sound only
    /// where the responder could actually have filed it. A dense-v5 class can (its rerun is ~13K
    /// leaves ≈ 3 MB per position, ~1.7 GB at 512 positions). A HELD v7 class cannot: its evidence
    /// builder re-executes the fold retention densely and holds every tile (~6 MB per position at
    /// ~103K leaves), which at a few hundred positions exceeds 24 GB — so an honest held-class
    /// responder would be convicted of `CourtFraud` for a move it has no memory to make, the exact
    /// case ADR-0093's mercy exists for. A windowed builder — the rows the site reads, replayed from
    /// the interval's anchor — is ADR-0121 §7's, and the evidence's own size (the whole K/V history
    /// the site reduces over) is a carrier question besides; neither is decided.
    ///
    /// **So a held class keeps the mercy: `!self.class_is_held_v1(class_id)`** (ADR-0119's
    /// registry, merged into the same 7,000 build). Every class that is not held is convicted past
    /// the deep fence exactly as C-08 wrote it; a held class's honest responder is never convicted
    /// for a move no node can make. Kept as a named seam, not a comment, so C-08's condition is
    /// structurally complete and a later flag day that can build the evidence edits one body.
    pub(crate) fn court_root_evidence_is_buildable_v1(&self, class_id: &Hash64) -> bool {
        !self.class_is_held_v1(class_id)
    }

    /// **ADR-0152 §4-ter C1: the mercy, per class.** [`Self::court_root_evidence_is_buildable_v1`]
    /// past `palw_offence_attribution` for a held class an honest responder CAN answer: the windowed
    /// builder (the responder's `attn_site_evidence_held_v1`) rebuilds the site's evidence from one
    /// plain forward to the anchor, so at `n_ctx ≤ PALW_HELD_ANSWERABLE_N_CTX_V1` the root claim is a
    /// move the responder can make, and withholding it is a default like any other. A held class the
    /// bundle lists as unanswerable (the 2M row) keeps the mercy, and below the fence this is v1.
    pub(crate) fn court_root_evidence_is_buildable_v2(
        &self,
        claim: &PalwClaimStateV2,
        offence_attribution_active: bool,
        params: &PalwStateParamsV2,
    ) -> bool {
        self.court_root_evidence_is_buildable_v1(&claim.class_id)
            || (offence_attribution_active && !self.held_claim_is_unanswerable_v2(params, claim))
    }

    /// Keep pre-fence claims on their original mercy reading. Only a claim
    /// accepted under the memory/routing court reads the new held route from its
    /// immutable registered profile. Missing held profile data fails closed.
    pub fn held_claim_is_unanswerable_v2(&self, params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {
        if !params.memory_routing_court_from_daa().is_some_and(|at| claim.accepted_daa >= at) {
            return params.held_class_is_unanswerable_v1(&claim.class_id);
        }
        match self.fp_work_profile_of(&claim.class_id) {
            Some(profile) if profile.shape_profile_id() == claim.class_id => {
                crate::palw_class_admission_v2::palw_held_class_unanswerable_v2(profile, true).is_some()
            }
            Some(_) => true,
            None => params.held_class_is_unanswerable_v1(&claim.class_id) || self.class_is_held_v1(&claim.class_id),
        }
    }

    /// ADR-0071 SA-3: the production fact recorded under `key`, if the chain holds one.
    /// Keyed by [`palw_capability_fact_key_v1`]; read through
    /// [`palw_bond_produced_on_class_v1`] so the draw and any RPC that reports eligibility ask
    /// one question.
    pub fn capability_fact(&self, key: &Hash64) -> Option<&PalwCapabilityStateV2> {
        self.capabilities.get(key)
    }

    /// ADR-0075: the families this chain certified for `lane`, in digest order — the set the
    /// gates read alongside the genesis one (`family_certified_for_weight_v2`).
    pub fn chain_certified_families(&self, lane: PalwCertifiedLaneV1) -> Vec<crate::palw_e2e_adjudicability::PalwE2eFamilyV1> {
        self.certified_family_records(lane).values().map(|record| record.family.clone()).collect()
    }

    pub fn certified_family(&self, lane: PalwCertifiedLaneV1, digest: &Hash64) -> Option<&PalwCertifiedFamilyStateV2> {
        self.certified_family_records(lane).get(digest)
    }

    /// Every family the chain certified for `lane`, by digest — what `getPalwRegistrationTerms`
    /// serves (ADR-0122 §8.2), so a registrant and a lane binding can be priced and checked the way
    /// the transition will check them.
    pub fn certified_family_entries(&self, lane: PalwCertifiedLaneV1) -> Vec<(Hash64, PalwCertifiedFamilyStateV2)> {
        self.certified_family_records(lane).iter().map(|(digest, record)| (*digest, record.clone())).collect()
    }

    pub(super) fn certified_family_records(&self, lane: PalwCertifiedLaneV1) -> &BTreeMap<Hash64, PalwCertifiedFamilyStateV2> {
        match lane {
            PalwCertifiedLaneV1::Attempt => &self.certified_families,
            PalwCertifiedLaneV1::FreePrompt => &self.fp_certified_families,
        }
    }

    /// ADR-0075 Decision 5: the chain's own free-prompt certification of `class`, if any. The
    /// genesis-frozen set (`PalwStateParamsV2::fp_certified_classes`) is the other half of the
    /// gate; the free-prompt arm reads both.
    pub fn fp_lane_certification(&self, class: &Hash64) -> Option<&PalwClassLaneCertificationV2> {
        self.fp_certified_classes.get(class)
    }

    /// ADR-0075 Decision 14: the parts of a chunked object that have arrived so far.
    pub fn pending_chunk_group(&self, group: &Hash64) -> Option<&PalwPendingChunksV2> {
        self.pending_chunks.get(group)
    }

    /// ADR-0080 design A: the declared close on one side of one session, if there is one.
    pub fn court_close_group(&self, session: &Hash64, side: PalwCourtSideV1) -> Option<&PalwCourtCloseGroupV2> {
        self.court_close_groups.get(&(*session, side))
    }

    pub fn court_close_groups_iter(&self) -> impl Iterator<Item = (&(Hash64, PalwCourtSideV1), &PalwCourtCloseGroupV2)> {
        self.court_close_groups.iter()
    }

    /// ADR-0078 Decision 4: the derivation of `claim` under `transformer`, if the chain holds one.
    pub fn derived_artifact(&self, claim: &Hash64, transformer: &Hash64) -> Option<&crate::palw_derived_v1::PalwDerivedRowV1> {
        self.derived_artifacts.get(&crate::palw_derived_v1::PalwDerivedKeyV1 { claim: *claim, transformer: *transformer })
    }

    /// ADR-0078 Decision 4: every derivation the chain holds for `claim`, in transformer-id order.
    pub fn derived_artifacts_of(
        &self,
        claim: Hash64,
    ) -> impl Iterator<Item = (&crate::palw_derived_v1::PalwDerivedKeyV1, &crate::palw_derived_v1::PalwDerivedRowV1)> {
        self.derived_artifacts.range(crate::palw_derived_v1::PalwDerivedKeyV1::claim_range(claim))
    }

    /// The whole derivation table, for readers that walk it (the panel's log line, a census).
    pub fn derived_artifacts_iter(
        &self,
    ) -> impl Iterator<Item = (&crate::palw_derived_v1::PalwDerivedKeyV1, &crate::palw_derived_v1::PalwDerivedRowV1)> {
        self.derived_artifacts.iter()
    }

    pub fn class_target(&self, id: &Hash64) -> Option<&PalwClassTargetV2> {
        self.class_targets.get(id)
    }

    /// The class's share of cadence, in permille (ADR-0045 Decision 3). `None` for a class this
    /// chain never registered — and only for those: registration grants, nothing revokes.
    pub fn class_share_permille(&self, id: &Hash64) -> Option<u16> {
        self.class_shares.get(id).copied()
    }

    /// Every class this chain holds, in canonical order. Distinct from `class_shares_iter`: a
    /// class can be registered and hold no share, and telling those apart is the whole point of
    /// looking.
    pub fn classes_iter(&self) -> impl Iterator<Item = (&Hash64, &PalwClassStateV2)> {
        self.classes.iter()
    }

    /// The whole share table, in canonical order — what the boundary derivations fold over.
    pub fn class_shares_iter(&self) -> impl Iterator<Item = (&Hash64, &u16)> {
        self.class_shares.iter()
    }

    /// The current epoch's frozen budgets, if a boundary has written them (ADR-0045 Decision 2).
    pub fn epoch_budgets(&self) -> Option<&PalwEpochBudgetsV2> {
        self.epoch_budgets.as_ref()
    }

    /// The receipt lane's per-class target — what a quantum ticket is admitted against (FP-04).
    pub fn receipt_target(&self, id: &Hash64) -> Option<&PalwClassTargetV2> {
        self.receipt_targets.get(id)
    }

    pub fn receipt_epoch_counter(&self, class_id: &Hash64) -> Option<&PalwEpochCounterV2> {
        self.receipt_epoch_counters.get(class_id)
    }

    /// ADR-0160 F-W: the per-bond weight index (derived; empty below the fence).
    pub fn capacity_weight_index(&self) -> &crate::palw_weight_cap_v1::PalwBondWeightIndexV1 {
        &self.capacity_weight_index
    }

    /// Every claim this state holds, in canonical order — what a fold over the lattice reads
    /// (ADR-0042 Decision 10's maturity walk, and any consumer that must ask a question of the
    /// whole claim set rather than of one id).
    pub fn claims_iter(&self) -> impl Iterator<Item = (&Hash64, &PalwClaimStateV2)> {
        self.claims.iter()
    }

    /// ADR-0087: a class's market row, `None` before its first move.
    pub fn model_market(&self, class_id: &Hash64) -> Option<&crate::palw_model_market_v1::PalwModelMarketV1> {
        self.model_markets.get(class_id)
    }

    pub fn model_markets_iter(&self) -> impl Iterator<Item = (&Hash64, &crate::palw_model_market_v1::PalwModelMarketV1)> {
        self.model_markets.iter()
    }

    /// **ADR-0162 Decision 1: a line's market past `Params::palw_model_virtual_v1`** (in force from
    /// `fence_daa`), as the fold moves it and every reader shows it — the one place a line is opened.
    ///
    /// Every line the registry resolves has a market from its creation: the row where one was
    /// written, else the opening `palw_model_market_in_force_v2` gives it — `X = V`, the whole supply
    /// in the curve, opened at the line's founding height or the fence's, whichever is later (a class's
    /// founding line was created by the class's registration, a founded line by `ModelLineFounded`).
    /// A pledge still collecting ADR-0094's floor opens with the pledge as its seed; a market opened
    /// before the fence is returned as it is, `V = 0` and all. The floor class has no line
    /// (`ModelLineOnFloor`), so it is opened by nothing here: its row, if a pre-fence seed made one,
    /// and `None` otherwise. `None` for a line the registry does not know.
    pub fn model_market_in_force_v2(
        &self,
        line_id: &Hash64,
        fence_daa: u64,
        base_class_id: &Hash64,
    ) -> Option<crate::palw_model_market_v1::PalwModelMarketV1> {
        let row = self.model_markets.get(line_id);
        if let Some(market) = row
            && market.is_open()
        {
            return Some(*market);
        }
        let line = self.model_line_or_founding(line_id)?;
        if line.class_id == *base_class_id {
            return row.copied();
        }
        Some(crate::palw_model_market_v1::palw_model_market_in_force_v2(row, line.founded_daa.max(fence_daa)))
    }

    /// ADR-0087: units `holder` holds in `class_id`, zero when no row exists.
    /// ADR-0095 §4.6: the height of the line's most recent publication, which is what the cadence
    /// is measured from. A line that has never published beyond its founding is measured from the
    /// founding, so a new line is not born already lapsed.
    pub fn model_line_last_version_daa(&self, line_id: &Hash64) -> u64 {
        let founded = self.model_line_or_founding(line_id).map(|l| l.founded_daa).unwrap_or(0);
        self.model_versions
            .range((*line_id, 0)..=(*line_id, u32::MAX))
            .map(|((_, _), v)| v.published_daa)
            .max()
            .unwrap_or(founded)
            .max(founded)
    }

    /// ADR-0095 §4.1: the line's declaration, as stored (including any pending weakening).
    pub fn model_benefits(&self, line_id: &Hash64) -> Option<&crate::palw_model_benefits_v1::PalwModelBenefitsV1> {
        self.model_benefits.get(line_id)
    }

    /// ADR-0095 §4.6: the tiers GOVERNING at `daa` — after the notice has matured and after the
    /// lapse rules. This is the reader every rule and every card must use; the raw `tiers` field is
    /// only what was last written.
    pub fn model_benefit_tiers_in_effect(
        &self,
        line_id: &Hash64,
        daa: u64,
    ) -> Vec<crate::palw_model_benefits_v1::PalwModelBenefitTierV1> {
        let Some(row) = self.model_benefits.get(line_id) else { return Vec::new() };
        let last = self.model_line_last_version_daa(line_id);
        crate::palw_model_benefits_v1::palw_model_benefits_in_effect_v1(row, last, daa).to_vec()
    }

    /// ADR-0095 §4.6: why the declaration is granting nothing, for the card.
    pub fn model_benefit_lapse(&self, line_id: &Hash64, daa: u64) -> Option<crate::palw_model_benefits_v1::PalwModelBenefitLapseV1> {
        let row = self.model_benefits.get(line_id)?;
        let last = self.model_line_last_version_daa(line_id);
        crate::palw_model_benefits_v1::palw_model_benefits_lapse_v1(row, last, daa)
    }

    /// ADR-0095 §4.4: the lead the fold enforces on this line's version paths at `daa`.
    pub fn model_benefit_enforced_lead(&self, line_id: &Hash64, daa: u64) -> u64 {
        crate::palw_model_benefits_v1::palw_model_benefits_enforced_lead_v1(&self.model_benefit_tiers_in_effect(line_id, daa))
    }

    /// ADR-0095 §4.5: how long this holder has held without selling, at `daa`.
    pub fn model_position_tenure(&self, line_id: &Hash64, holder: &Hash64, daa: u64) -> u64 {
        match self.model_position_since.get(&(*line_id, *holder)) {
            Some(since) => daa.saturating_sub(*since),
            None => 0,
        }
    }

    /// **ADR-0095 §4.5: the height this holder's tenure clock last started — `None` where the chain
    /// keeps no clock for it.** A holding bought before the membership fence, or on a network that
    /// never armed it, has none, and [`Self::model_position_tenure`] reads it as 0. A reader states
    /// this beside the tenure so "held since H" is the chain's number rather than `tip − tenure`
    /// rebuilt by a caller who cannot tell "started this block" from "never started" (the
    /// 2026-09-23 Position route matrix, P-B2). A read; no rule calls it.
    pub fn model_position_since(&self, line_id: &Hash64, holder: &Hash64) -> Option<u64> {
        self.model_position_since.get(&(*line_id, *holder)).copied()
    }

    /// **ADR-0095 §4.3 — the tier, and the one place the whole network computes it.**
    ///
    /// A gateway, a wallet and the explorer must agree, so this takes the height it is asked about
    /// and answers from the tiers in effect there. Both lanes count: ADR-0089 keeps carrier-held
    /// and EVM-held positions in separate namespaces, and a person is not two people.
    pub fn model_benefit_tier_across(
        &self,
        line_id: &Hash64,
        holders: &[Hash64],
        daa: u64,
    ) -> Option<(usize, crate::palw_model_benefits_v1::PalwModelBenefitTierV1)> {
        let tiers = self.model_benefit_tiers_in_effect(line_id, daa);
        if tiers.is_empty() {
            return None;
        }
        let (units, tenure) = self.model_position_across(line_id, holders, daa);
        let chosen = crate::palw_model_benefits_v1::tier_for_units(&tiers, units, tenure)?.clone();
        let idx = tiers.iter().position(|t| t.min_units == chosen.min_units)?;
        Some((idx, chosen))
    }

    pub fn model_benefit_tier(
        &self,
        line_id: &Hash64,
        holder: &Hash64,
        daa: u64,
    ) -> Option<(usize, crate::palw_model_benefits_v1::PalwModelBenefitTierV1)> {
        let tiers = self.model_benefit_tiers_in_effect(line_id, daa);
        if tiers.is_empty() {
            return None;
        }
        let (units, tenure) = self.model_position_across(line_id, std::slice::from_ref(holder), daa);
        let chosen = crate::palw_model_benefits_v1::tier_for_units(&tiers, units, tenure)?.clone();
        let idx = tiers.iter().position(|t| t.min_units == chosen.min_units)?;
        Some((idx, chosen))
    }

    pub fn model_position(&self, class_id: &Hash64, holder: &Hash64) -> u64 {
        self.model_positions.get(&(*class_id, *holder)).copied().unwrap_or(0)
    }

    /// ADR-0087 Decision 8: every position a holder has, by class.
    ///
    /// **And its cost** (mainnet audit 2026-09-06, M-13). This is a full scan of `model_positions`,
    /// paid per RPC call with no per-caller budget. The table is bounded — see
    /// [`Self::model_position_rows_of_line`] for the argument — but bounded is not small: at
    /// `PALW_MODEL_SUPPLY_UNITS_V1` rows a line it is 500,000 entries a line, and this walks every
    /// line's. The key is `(line, holder)`, so no range answers "this holder's rows" without a
    /// second index; adding one would be a new consensus table, which is the defect this finding is
    /// about. The scan stays, the cost is stated, and the caller is bounded instead.
    /// **ADR-0095 §4.3, N3 — what a PERSON holds of a line, across the ids they proved.**
    ///
    /// ADR-0089 gives the EVM lane its own holder ids (`evm_holder_v1` of the account), unrelated
    /// to a carrier-lane payout key, and the chain cannot tell that two ids are one person. So the
    /// PERSON is whatever set of ids they can sign for (§4.8), and this sums exactly that set —
    /// never more, because an unproven id is somebody else's.
    ///
    /// The tenure is the MOST RECENT of the ids' clocks: a person who sold from one of their own
    /// addresses last block has sold, and taking the oldest clock would let a holder keep a long
    /// tenure in one hand while trading with the other.
    ///
    /// **Each id counts once, however often the list names it** (the 2026-09-23 Position route
    /// matrix, P11). The list is the caller's — a gateway's, a wallet's, the holder's own — and
    /// summing it as given let one holding be counted twice: the same id listed twice turned tier 0
    /// into tier 1. A set of ids is a set. No consensus path reads this sum (its only readers are
    /// the two tier functions above, which nothing in the fold, the processor or the EVM window
    /// calls), so the repair changes no state root on any network and takes no fence.
    pub fn model_position_across(&self, line_id: &Hash64, holders: &[Hash64], daa: u64) -> (u64, u64) {
        let mut units = 0u64;
        let mut tenure = u64::MAX;
        let mut counted = BTreeSet::new();
        for h in holders {
            if !counted.insert(*h) {
                continue;
            }
            let held = self.model_position(line_id, h);
            if held == 0 {
                continue;
            }
            units = units.saturating_add(held);
            tenure = tenure.min(self.model_position_tenure(line_id, h, daa));
        }
        (units, if tenure == u64::MAX { 0 } else { tenure })
    }

    pub fn model_positions_of(&self, holder: &Hash64) -> Vec<(Hash64, u64)> {
        self.model_positions.iter().filter(|((_, h), _)| h == holder).map(|((c, _), units)| (*c, *units)).collect()
    }

    /// **How many position rows one line holds** (audit M-13). Bounded by
    /// `PALW_MODEL_SUPPLY_UNITS_V1`: `write_model_position` removes a row at zero units, ADR-0090
    /// Decision 1 makes a position a whole unit, and ADR-0087 M1 makes the sum of holders' units at
    /// most the supply — so a line cannot have more holders than it has units, and each of those
    /// units was bought from a curve that is strictly more expensive as the table grows, on a line
    /// that cost `PALW_MODEL_SEED_MIN_SOMPI_V1` to open. The bound exists; nothing said it, and
    /// nothing would notice if a later change to the supply constant or to the zero-row removal
    /// took it away. Read by the test that pins it.
    pub fn model_position_rows_of_line(&self, line_id: &Hash64) -> usize {
        self.model_positions.range((*line_id, Hash64::default())..).take_while(|((l, _), _)| l == line_id).count()
    }

    /// ADR-0087 M1: units held in a class across every holder.
    pub fn model_units_held(&self, class_id: &Hash64) -> u128 {
        self.model_positions.iter().filter(|((c, _), _)| c == class_id).map(|(_, u)| *u as u128).sum()
    }

    // ---- ADR-0088: lines, versions, roots in force -------------------------------------------

    /// ADR-0088 Decision 1: a line's row as written, `None` for a founding line nothing touched.
    pub fn model_line(&self, line_id: &Hash64) -> Option<&crate::palw_model_lines_v1::PalwModelLineV1> {
        self.model_lines.get(line_id)
    }

    /// ADR-0088 Decision 1: a line as the fold sees it — the row, or the class's founding line
    /// synthesised from the class row (its id IS the class id). `None` when neither exists.
    pub fn model_line_or_founding(&self, line_id: &Hash64) -> Option<crate::palw_model_lines_v1::PalwModelLineV1> {
        if let Some(row) = self.model_lines.get(line_id) {
            return Some(row.clone());
        }
        let class = self.classes.get(line_id)?;
        Some(crate::palw_model_lines_v1::founding_line_v1(*line_id, class.registrant_bond, Vec::new(), class.registered_daa))
    }

    /// ADR-0088 Decision 2: a version row; version 1 of a founding line with no rows is the
    /// class's registration, synthesised.
    pub fn model_version(&self, line_id: &Hash64, version: u32) -> Option<crate::palw_model_lines_v1::PalwModelVersionV1> {
        if let Some(row) = self.model_versions.get(&(*line_id, version)) {
            return Some(row.clone());
        }
        if version != 1 || self.model_lines.contains_key(line_id) {
            return None;
        }
        let class = self.classes.get(line_id)?;
        Some(crate::palw_model_lines_v1::PalwModelVersionV1 {
            root: class.artifact_root,
            parent: None,
            adopted_from: None,
            runtime_hash: None,
            dataset_commitment: None,
            training_config_hash: None,
            notes_hash: None,
            published_daa: class.registered_daa,
            published_by: class.registrant_bond,
            status: crate::palw_model_lines_v1::PalwVersionStatusV1::Current,
            usage: Default::default(),
        })
    }

    pub fn model_lines_iter(&self) -> impl Iterator<Item = (&Hash64, &crate::palw_model_lines_v1::PalwModelLineV1)> {
        self.model_lines.iter()
    }

    pub fn model_versions_of(&self, line_id: &Hash64) -> Vec<(u32, &crate::palw_model_lines_v1::PalwModelVersionV1)> {
        self.model_versions.range((*line_id, 0)..=(*line_id, u32::MAX)).map(|((_, n), v)| (*n, v)).collect()
    }

    /// ADR-0191 step B: the designated-custodian record of `key`, written only by a tag-245 event once its fence is in force.
    pub fn tir_designated_record(
        &self,
        key: &crate::palw_tir_designated_answers_v1::PalwTirRequestKeyV1,
    ) -> Option<&crate::palw_tir_designated_answers_v1::PalwTirDesignatedRecordV1> {
        self.tir_designated.records.get(key)
    }

    /// **G14 lane escrow-rfc08 (RFC-0035 A): the rooted funded-escrow rows**, by contract id — what the UTXO spend filter locks
    /// (the unsettled rows' outpoints, `crate::palw_funded_escrow_v1::palw_escrow_rows_by_outpoint_v1`). Empty on every network.
    pub fn funded_escrows_v1(&self) -> &BTreeMap<Hash64, crate::palw_funded_escrow_v1::PalwFundedEscrowRowV1> {
        &self.funded_escrows
    }

    /// G14 item 6 (ADR-0200 C1): the D2 readiness registry (recorded tag-230 roots and their disclosure cases). Empty on every network.
    pub fn d2_readiness_v1(&self) -> &crate::palw_court_model_scope_routes_v1::readiness_disclosure::PalwD2ReadinessRegistryV1 {
        &self.d2_readiness
    }

    /// ADR-0191 step B: every designated-custodian record in the registry (empty below the fence).
    pub fn tir_designated_records(
        &self,
    ) -> &std::collections::BTreeMap<
        crate::palw_tir_designated_answers_v1::PalwTirRequestKeyV1,
        crate::palw_tir_designated_answers_v1::PalwTirDesignatedRecordV1,
    > {
        &self.tir_designated.records
    }

    pub fn model_proposals_of(&self, line_id: &Hash64) -> Vec<(&Hash64, &crate::palw_model_lines_v1::PalwModelProposalV1)> {
        self.model_proposals.iter().filter(|(_, p)| p.line_id == *line_id).collect()
    }

    pub fn model_evaluations_of(
        &self,
        line_id: &Hash64,
        version: u32,
    ) -> Vec<(&PalwBondKeyV2, &crate::palw_model_lines_v1::PalwModelEvaluationV1)> {
        self.model_evaluations.iter().filter(|((l, v, _), _)| l == line_id && *v == version).map(|((_, _, b), e)| (b, e)).collect()
    }

    /// The line rows of a class (the founding line included only when it has a row).
    pub fn class_line_rows(&self, class_id: &Hash64) -> Vec<(&Hash64, &crate::palw_model_lines_v1::PalwModelLineV1)> {
        self.model_lines.iter().filter(|(_, l)| l.class_id == *class_id).collect()
    }

    /// **ADR-0088 Decision 3: the roots in force for a class at `daa`.** The founding root when
    /// the class has no line rows; otherwise, over every Active line of the class, the versions
    /// in force (current, previews, superseded inside their grace), plus the founding root while
    /// the founding line has no row of its own.
    /// **RFC-0004: does any improvement table hold a row?** The root block's (and the carriage's)
    /// one guard: all empty on every network below the fence.
    pub fn has_improvement_rows_v1(&self) -> bool {
        !self.improvement_lines.is_empty()
            || !self.improvement_epochs.is_empty()
            || !self.improvement_policies.is_empty()
            || !self.improvement_usage.is_empty()
            || !self.improvement_heads.is_empty()
            || !self.improvement_pools.is_empty()
            || !self.improvement_material.is_empty()
            || !self.improvement_candidates.is_empty()
            || !self.improvement_pool_entries.is_empty()
            || !self.improvement_items.is_empty()
            || !self.improvement_results.is_empty()
            || !self.improvement_grants.is_empty()
            || !self.improvement_earnings.is_empty()
    }

    /// Test support: one governed line's rows and one epoch's detail rows, inserted directly (the
    /// fold writes them only through its writers).
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_improvement_rows_for_test_v1(
        &mut self,
        line: crate::palw_improve_state_v1::PalwImprovementLineV1,
        policy: crate::palw_improve_state_v1::PalwImprovementPolicyRecordV1,
        usage: crate::palw_improve_state_v1::PalwImprovementUsageV1,
        head: crate::palw_improve_state_v1::PalwLineageHeadEntryV1,
        pool: crate::palw_improve_state_v1::PalwImprovementPoolV1,
        material: crate::palw_improve_state_v1::PalwMaterialFrontierV1,
        epoch: crate::palw_improve_state_v1::PalwImprovementEpochV1,
        candidate: crate::palw_improve_state_v1::PalwEpochCandidateV1,
        pool_entry: crate::palw_improve_state_v1::PalwPoolEntryV2,
        item: crate::palw_improve_state_v1::PalwEvalItemV1,
        result: crate::palw_improve_state_v1::PalwEvalResultV1,
        grant: crate::palw_improve_state_v1::PalwRewardGrantV1,
    ) {
        let id = line.line_id;
        let n = epoch.epoch;
        self.improvement_lines.insert(id, line);
        self.improvement_policies.insert(id, policy);
        self.improvement_usage.insert(id, usage);
        self.improvement_heads.insert((id, 0), head);
        self.improvement_pools.insert(id, pool);
        self.improvement_material.insert((id, n), material);
        self.improvement_epochs.insert((id, n), epoch);
        self.improvement_candidates.insert((id, n, 0), candidate);
        let entry_key = match pool_entry {
            crate::palw_improve_state_v1::PalwPoolEntryV2::HoldOut { id: case, .. } => (id, n, 1u8, case),
            crate::palw_improve_state_v1::PalwPoolEntryV2::SetterSet { set_id, .. } => (id, n, 2u8, set_id),
        };
        self.improvement_pool_entries.insert(entry_key, pool_entry);
        self.improvement_items.insert((id, n, item.item), item);
        self.improvement_results.insert((id, n, result.item, result.subject), result);
        self.improvement_grants.insert((id, n, 0), grant);
        rebuild_improvement_indices_v1(self);
    }

    pub fn class_roots_in_force(&self, class_id: &Hash64, daa: u64) -> Vec<Hash64> {
        let Some(class) = self.classes.get(class_id) else { return Vec::new() };
        let mut roots: Vec<Hash64> = Vec::new();
        if !self.model_lines.contains_key(class_id) {
            roots.push(class.artifact_root);
        }
        for (line_id, line) in self.model_lines.iter().filter(|(_, l)| l.class_id == *class_id) {
            let retired_out = line.retired_daa.is_some_and(|d| daa >= d);
            if retired_out {
                continue;
            }
            for ((_, _), version) in self.model_versions.range((*line_id, 0)..=(*line_id, u32::MAX)) {
                if version.in_force_at(daa) && !roots.contains(&version.root) {
                    roots.push(version.root);
                }
            }
        }
        roots
    }

    /// **ADR-0101: everything a client needs to check a provider's service descriptor**, assembled
    /// from one state at one height. Until this existed, `PalwLineServiceFactsV1` was built only by
    /// test fixtures — the check ADR-0101 specifies had no node that could answer it, so "any
    /// provider may serve, and a client can check rather than trust" was true of the design and not
    /// of the software.
    ///
    /// `ownership_active` is `Params::palw_artifact_root_ownership` at `daa`: past it the roots are
    /// the ones this line OWNS (ADR-0143), which is what stops a copy line offering the class's own
    /// founding artifact as its own.
    pub fn line_service_facts_v1(
        &self,
        class_id: &Hash64,
        line_id: &Hash64,
        daa: u64,
        ownership_active: bool,
    ) -> crate::palw_service_descriptor_v1::PalwLineServiceFactsV1 {
        let declared_grants = self.model_benefit_tiers_in_effect(line_id, daa).iter().fold(0u32, |bits, tier| bits | tier.grants);
        crate::palw_service_descriptor_v1::PalwLineServiceFactsV1 {
            line_id: *line_id,
            declared_grants,
            roots: self.line_service_roots_v1(class_id, line_id, daa, ownership_active),
            origin_pubkeys: self.line_origin_pubkeys_v1(line_id),
            now_daa: daa,
        }
    }

    /// **ADR-0101: the keys the line's own people sign with** — the `origin_pubkeys` half of
    /// [`crate::palw_service_descriptor_v1::PalwLineServiceFactsV1`]. The registered pubkeys of the
    /// line's owner, developer and maintainer bonds, deduplicated and sorted so two nodes answer
    /// one list. A grant only the line's origin may provide (its developer room, its support desk)
    /// is checked against this, so a descriptor claiming one under a stranger's key is refused.
    pub fn line_origin_pubkeys_v1(&self, line_id: &Hash64) -> Vec<Vec<u8>> {
        let Some(line) = self.model_line_or_founding(line_id) else { return Vec::new() };
        let mut keys: Vec<Vec<u8>> = [line.owner, line.developer, line.maintainer]
            .into_iter()
            .flatten()
            .filter_map(|bond| self.bonds.get(&bond).map(|state| state.pubkey.clone()))
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// **ADR-0101: the roots a provider may serve FOR ONE LINE** — the `roots` half of
    /// [`crate::palw_service_descriptor_v1::PalwLineServiceFactsV1`], which until now nothing
    /// outside a test fixture produced.
    ///
    /// ADR-0101 wrote this as "the class's registered artifact root and every version root the
    /// line published", and the first clause is a hole ADR-0143 closes: the class's founding root
    /// is the FOUNDING line's, so a stranger's line offering it is offering somebody else's
    /// artifact. This takes the class's roots in force and keeps the ones this line owns, so past
    /// the fence a copy line is left with exactly the roots it published itself.
    pub fn line_service_roots_v1(&self, class_id: &Hash64, line_id: &Hash64, daa: u64, ownership_active: bool) -> Vec<Hash64> {
        self.class_roots_in_force(class_id, daa)
            .into_iter()
            .filter(|root| self.artifact_line_of_root(class_id, root, ownership_active) == Some(*line_id))
            .collect()
    }

    /// **ADR-0143 Decisions 2 and 3: the ONE answer to "which line owns this artifact root".**
    ///
    /// Past `Params::palw_artifact_root_ownership` this is the rooted index and nothing else, so
    /// the answer cannot depend on which `line_id` sorts first in a `BTreeMap`. Below the fence it
    /// is the walk every existing row was written under, kept byte for byte.
    ///
    /// The class is still asked because every caller has one and attribution has always been
    /// class-scoped: a root owned by another class's line is not this class's to attribute, and
    /// saying so is what keeps a cross-class duplicate from paying the wrong pair.
    pub fn artifact_line_of_root(&self, class_id: &Hash64, root: &Hash64, ownership_active: bool) -> Option<Hash64> {
        if ownership_active {
            return self.artifact_owners.get(&(*class_id, *root)).map(|owner| owner.line_id);
        }
        self.model_line_of_root_positional(class_id, root)
    }

    /// ADR-0088 Decision 4: the `(line, version)` a root belongs to, for attribution.
    ///
    /// Past ADR-0143's fence the LINE comes from the index; the version is then the lowest in-force
    /// version of that one line carrying the root — a scan inside a single line, which no id order
    /// can reorder — and the index's own recorded version when the line has no rows at all, which
    /// is how a class's synthesised founding line answers.
    pub fn model_version_of_root(&self, class_id: &Hash64, root: &Hash64, daa: u64, ownership_active: bool) -> Option<(Hash64, u32)> {
        if ownership_active {
            let owner = self.artifact_owners.get(&(*class_id, *root))?;
            if self.model_lines.get(&owner.line_id).is_some_and(|line| line.retired_daa.is_some_and(|d| daa >= d)) {
                return None;
            }
            let in_force = self
                .model_versions
                .range((owner.line_id, 0)..=(owner.line_id, u32::MAX))
                .find(|(_, version)| version.root == *root && version.in_force_at(daa))
                .map(|((_, n), _)| *n);
            // A line with no version rows is a founding line the fold synthesises, and the index
            // recorded its version at the reservation. Anything else in force wins over it.
            return in_force
                .or_else(|| {
                    self.model_versions.range((owner.line_id, 0)..=(owner.line_id, u32::MAX)).next().is_none().then_some(owner.version)
                })
                .map(|n| (owner.line_id, n));
        }
        for (line_id, line) in self.model_lines.iter().filter(|(_, l)| l.class_id == *class_id) {
            if line.retired_daa.is_some_and(|d| daa >= d) {
                continue;
            }
            for ((_, n), version) in self.model_versions.range((*line_id, 0)..=(*line_id, u32::MAX)) {
                if version.root == *root && version.in_force_at(daa) {
                    return Some((*line_id, *n));
                }
            }
        }
        let class = self.classes.get(class_id)?;
        (!self.model_lines.contains_key(class_id) && class.artifact_root == *root).then_some((*class_id, 1))
    }

    /// ADR-0091 Decision 2: the line a root belongs to, for the reward's buyback — every version
    /// of every line of the class, in force or not, in the registry's order (attribution was a
    /// fact of the accepting block and does not lapse with a version); the class itself, as its
    /// founding line, when no line row exists and the root is the class's.
    ///
    /// **Superseded past ADR-0143's fence** by [`Self::artifact_line_of_root`], which is the only
    /// caller that may reach it: it is kept so a chain below the fence answers exactly as it did,
    /// and it is private so nothing new can grow a second way to ask the question.
    pub(super) fn model_line_of_root_positional(&self, class_id: &Hash64, root: &Hash64) -> Option<Hash64> {
        for (line_id, _) in self.model_lines.iter().filter(|(_, l)| l.class_id == *class_id) {
            if self.model_versions.range((*line_id, 0)..=(*line_id, u32::MAX)).any(|(_, v)| v.root == *root) {
                return Some(*line_id);
            }
        }
        let class = self.classes.get(class_id)?;
        (!self.model_lines.contains_key(class_id) && class.artifact_root == *root).then_some(*class_id)
    }

    /// ADR-0088 Decision 3: the root a claim was admitted against, when the fold recorded one.
    pub fn claim_root(&self, claim_id: &Hash64) -> Option<Hash64> {
        self.claim_roots.get(claim_id).copied()
    }

    /// ADR-0089 Decision 6: the settlements this state's block decided, in sequence order — what
    /// the selected child must carry as `MarketSettle` ops.
    pub fn evm_settlements(&self) -> Vec<crate::evm::model_market::PalwEvmSettlementV1> {
        self.evm_settlements.values().copied().collect()
    }

    /// ADR-0100 Decision 4: a class's shard plan, if its registrant declared one.
    pub fn class_shard_plan(&self, class_id: &Hash64) -> Option<crate::palw_shard_licensing_v1::PalwClassShardPlanV1> {
        self.class_shard_plans.get(class_id).copied()
    }

    /// ADR-0100 Decision 4: the shards of `class_id` this bond declared it holds.
    pub fn shards_of_bond(&self, bond: &PalwBondKeyV2, class_id: &Hash64) -> Option<&[u32]> {
        self.bond_shards.get(&(*bond, *class_id)).map(Vec::as_slice)
    }

    /// ADR-0100 Decision 4: which shards of a `PanelBound` claim have licensed so far.
    pub fn shard_licensing_of(&self, claim_id: &Hash64) -> Option<&crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1> {
        self.shard_licensing.get(claim_id)
    }

    /// ADR-0103 Decision 4: the unit a held accusation named, while the claim's session is open.
    pub fn held_da_missing_of(&self, claim_id: &Hash64) -> Option<crate::palw_held_da_v1::PalwHeldMissingV1> {
        self.held_da_missing.get(claim_id).copied()
    }

    /// ADR-0111 Decision 3: which seats have demanded which leaf of a live claim.
    pub fn held_leaf_demands_of(&self, claim_id: &Hash64) -> Option<&BTreeMap<PalwBondKeyV2, u64>> {
        self.held_leaf_demands.get(claim_id)
    }

    /// ADR-0124: the seats on duty for a live claim bound past the panel-economy fence, each with
    /// the DAA the chain credited its `Valid` receipt at (`0` = not yet). `None` for a claim whose
    /// panel was bound below the fence, or that has gone terminal.
    pub fn panel_duties_of(&self, claim_id: &Hash64) -> Option<&BTreeMap<PalwBondKeyV2, u64>> {
        self.panel_duties.get(claim_id).map(|row| &row.seats)
    }

    /// ADR-0130: a live claim's whole duty row — the seats and what each reserved when it bound.
    pub fn panel_duty_row_of(&self, claim_id: &Hash64) -> Option<&PalwPanelDutyRowV1> {
        self.panel_duties.get(claim_id)
    }

    /// ADR-0130: every live duty row, in claim-id order — the ledger a shadow reader rebuilds
    /// seat exposure from (`palw_panel_economy_v1::palw_shadow_seat_exposure_ledger_v1`).
    pub fn panel_duty_rows_iter(&self) -> impl Iterator<Item = (&Hash64, &PalwPanelDutyRowV1)> {
        self.panel_duties.iter()
    }

    /// ADR-0124 Decision 2: the panel reserve, cumulative — pool shares no seat was credited for.
    pub fn panel_reserve_sompi(&self) -> u64 {
        self.panel_reserve_sompi
    }

    /// ADR-0125: the schedule a round block anchored in `span` is judged by, if this state keeps one.
    pub fn round_schedule(&self, span: u64) -> Option<&crate::palw_execution_lane_v1::PalwExecScheduleV1> {
        self.round_schedules.get(&span)
    }

    /// ADR-0125: every schedule this state keeps, by span.
    pub fn round_schedules(&self) -> &BTreeMap<u64, crate::palw_execution_lane_v1::PalwExecScheduleV1> {
        &self.round_schedules
    }

    /// ADR-0125 §7.4: how many permits of `span` this chain has accepted.
    pub fn round_permits_accepted(&self, span: u64) -> u64 {
        self.round_permits_used.range((span, 0)..=(span, u64::MAX)).map(|(_, bits)| u64::from(bits.count_ones())).sum()
    }

    /// ADR-0125 §7.3: was the permit `(span, round, index)` proven signed twice on this chain?
    pub fn round_equivocated(&self, span: u64, round: u64, index: u16) -> bool {
        index < 16 && self.round_equivocations.get(&(span, round)).is_some_and(|bits| bits & (1u16 << index) != 0)
    }

    /// ADR-0125: has the permit `(span, round, index)` already been accepted on this chain?
    pub fn round_permit_used(&self, span: u64, round: u64, index: u16) -> bool {
        index < 16 && self.round_permits_used.get(&(span, round)).is_some_and(|bits| bits & (1u16 << index) != 0)
    }

    /// ADR-0125: the attempts finalized so far in the open span, and that span.
    pub fn round_finals(&self) -> (u64, &BTreeMap<Hash64, crate::palw_execution_lane_v1::PalwExecFinalV1>) {
        (self.round_span, &self.round_finals)
    }

    /// ADR-0130: the snapshot waiting for its seed, if any — participants, not a schedule: no round
    /// block is judged by it.
    pub fn round_pending_snapshot(&self) -> Option<&crate::palw_execution_lane_v1::PalwExecSnapshotV1> {
        self.round_pending.values().next()
    }

    /// ADR-0130 / ADR-0151: every snapshot waiting for its span, by target — past the economic-safety
    /// bundle one per span of maturity.
    pub fn round_pending_snapshots(&self) -> &BTreeMap<u64, crate::palw_execution_lane_v1::PalwExecSnapshotV1> {
        &self.round_pending
    }

    /// ADR-0130: the latest attempt-carrying chain block of the open span, if one has been recorded.
    pub fn round_seed_anchor(&self) -> Option<&crate::palw_execution_lane_v1::PalwExecSeedAnchorV1> {
        self.round_seed_anchor.as_ref()
    }

    /// Whether the execution lane has written anything: the gate the root and the carriage share.
    pub(super) fn round_lane_is_written(&self) -> bool {
        self.round_span != 0
            || !self.round_finals.is_empty()
            || !self.round_schedules.is_empty()
            || !self.round_permits_used.is_empty()
            || !self.round_equivocations.is_empty()
            || self.round_scheduler_is_written()
    }

    /// ADR-0130's rows: the gate their own sub-block of the root and their carriage tail share.
    pub(super) fn round_scheduler_is_written(&self) -> bool {
        !self.round_pending.is_empty() || self.round_seed_anchor.is_some()
    }

    /// ADR-0135's rows: the gate their own sub-block of the root and their carriage tail share.
    pub(super) fn model_registry_is_written(&self) -> bool {
        !self.model_lifecycles.is_empty() || !self.seat_readiness.is_empty()
    }

    /// ADR-0135: a class's lifecycle row, once the registry has stepped it.
    pub fn model_lifecycle(&self, class_id: &Hash64) -> Option<&crate::palw_model_registry_v1::PalwModelLifecycleRowV1> {
        self.model_lifecycles.get(class_id)
    }

    pub fn model_lifecycles_iter(&self) -> impl Iterator<Item = (&Hash64, &crate::palw_model_registry_v1::PalwModelLifecycleRowV1)> {
        self.model_lifecycles.iter()
    }

    // ---- ADR-0145: the derived work, read from rooted state ----------------------------------

    /// **What one draw of `class_id` really costs, derived — `None` while the fence is dormant.**
    ///
    /// `canonical_work_daa` is `Params::palw_canonical_work_daa()`, the fence's HEIGHT, and
    /// `accepted_daa` is the claim's own. Keying on the claim rather than on the block is not a
    /// style choice: `safe_weight` is priced once at a claim's `Final` and then re-derived at two
    /// later chain points (`retire_claim` and [`Self::assert_internal_consistency_v3`]), so a
    /// block-keyed fence would price one claim under two bases and the state would refuse itself
    /// on the next restart.
    ///
    /// **The source is `model_lifecycles`, which is rooted state.** The row's
    /// `economic_ccu_per_claim` is `palw_model_work_from_carriage_v1`'s walk of the class's own
    /// registration carriage under ADR-0131's cost table — the compute of the PREFILL-DRAW job,
    /// i.e. of the job an attempt actually runs — and
    /// `palw_canonical_work_v1::the_provisional_scalar_is_adr_0131_of_the_executed_job` pins that
    /// it is the same number [`crate::palw_canonical_work_v1::PalwCanonicalWorkVectorV1::provisional_scalar_v1`]
    /// returns. So the chain already derives this quantity; what changes past the fence is only
    /// that the accounting reads it.
    ///
    /// It is read from the row and not recomputed because the row is frozen when the registry
    /// opens the class, so every re-derivation of a given claim returns the same value forever —
    /// which is the property that makes the fence armable. Past `Params::palw_model_registry`
    /// every class in state holds a row (`step_model_registry` writes an inert zero row for one
    /// the build cannot describe), and a class with no row or zero work keeps the declared basis
    /// rather than falling to zero weight: an accounting change must not become a liveness failure.
    pub fn palw_canonical_per_draw_v1(&self, class_id: &Hash64, accepted_daa: u64, canonical_work_daa: Option<u64>) -> Option<u128> {
        if accepted_daa < canonical_work_daa? {
            return None;
        }
        let derived = self.model_lifecycles.get(class_id)?.work.economic_ccu_per_claim;
        (derived > 0).then_some(derived)
    }

    /// **ADR-0149 §5: the derived draw an ATTEMPT is priced on** — the class's row, or, for the
    /// liveness floor before the registry has written any row, the draw that row will carry.
    ///
    /// The registry writes a class's row at its first span boundary (`step_model_registry`), and a
    /// chain block's attempt is admitted against its PARENT's state. So with the canonical-work
    /// fence at the registry's own height — which `validate_palw_v2` permits, and which is what
    /// arming the economy at a network's genesis means — the first blocks past the fence are
    /// admitted against a state that holds no row at all. Read from the row alone, every attempt
    /// there is `PwuUnderivable`, the floor's included; the producers hold; and a network whose
    /// only block type is the attempt stops at the fence it was built to cross.
    ///
    /// The floor is the one class whose production cannot wait for its row — every other class is
    /// refused its row-less window already, by the work target, which arms at the registry's
    /// height — and the one class a caller can price without one. `base_known_draw` is the floor's
    /// entry in the table the registry reads (`PalwModelRegistryFoldV1::genesis_works`), which
    /// `step_model_registry` copies into the row VERBATIM: the claim priced on it is priced on
    /// exactly the number its row will carry, so every later reading of that claim agrees with this
    /// one. Where the floor has a row, the row answers and `base_known_draw` is not read.
    pub fn palw_attempt_per_draw_v1(
        &self,
        base_class_id: &Hash64,
        class_id: &Hash64,
        accepted_daa: u64,
        canonical_work_daa: Option<u64>,
        base_known_draw: Option<u128>,
    ) -> Option<u128> {
        if let Some(draw) = self.palw_canonical_per_draw_v1(class_id, accepted_daa, canonical_work_daa) {
            return Some(draw);
        }
        let past_the_unit = canonical_work_daa.is_some_and(|height| accepted_daa >= height);
        if !past_the_unit || class_id != base_class_id || self.model_lifecycles.contains_key(class_id) {
            return None;
        }
        base_known_draw.filter(|draw| *draw > 0)
    }

    /// **The floor class's two measures of one draw** — see [`PalwExposureBasisV1`] for why
    /// exposure needs a unit when reward and weight do not.
    ///
    /// `None` wherever the floor has nothing to normalise against, and every caller then keeps the
    /// declared basis. Deriving it here, from the base class the params name, is what lets the
    /// admission ceiling and the fold's ledger reach the same number without either looking the
    /// other up: they are handed the same two inputs and run the same expression.
    pub fn palw_exposure_basis_v1(
        &self,
        base_class_id: &Hash64,
        accepted_daa: u64,
        canonical_work_daa: Option<u64>,
    ) -> Option<PalwExposureBasisV1> {
        self.palw_exposure_basis_v2(base_class_id, accepted_daa, canonical_work_daa, None)
    }

    /// [`Self::palw_exposure_basis_v1`] with the floor's draw read through
    /// [`Self::palw_attempt_per_draw_v1`] — ADR-0149 §5. A basis that needed the floor's row would
    /// leave the floor's first row-less attempt reserved on its claimed pwu, which past the fence
    /// is the derived work of a WIN rather than of one inference: the gate's ceiling and the fold's
    /// ledger would part at the very block the fence is crossed.
    pub fn palw_exposure_basis_v2(
        &self,
        base_class_id: &Hash64,
        accepted_daa: u64,
        canonical_work_daa: Option<u64>,
        base_known_draw: Option<u128>,
    ) -> Option<PalwExposureBasisV1> {
        let base_canonical =
            self.palw_attempt_per_draw_v1(base_class_id, base_class_id, accepted_daa, canonical_work_daa, base_known_draw)?;
        let base_declared = palw_max_exposure_pwu_of_rule_v1(&self.classes.get(base_class_id)?.pwu_rule);
        let base_canonical = base_canonical.min(u64::MAX as u128) as u64;
        (base_declared > 0 && base_canonical > 0).then_some(PalwExposureBasisV1 { base_declared, base_canonical })
    }

    /// **A claim's `safe_weight` contribution on the derived basis** — `None` while the fence is
    /// dormant, or where the class offers nothing to derive from.
    ///
    /// See [`crate::palw_canonical_work_v1::palw_claim_canonical_pwu_v1`] for why this is a
    /// re-pricing of `claim.pwu` rather than a fresh product: the lottery factor is a fact about
    /// the chain the claim really paid and must survive; the per-inference factor is the
    /// registrant's number and must not.
    ///
    /// **ADR-0149: past the fence the ratio is the identity.** The admission now demands that an
    /// attempt's `pwu` EQUAL `palw_pwu_v1(target, derived draw)`, so a claim accepted past the fence
    /// carries the derivation itself and re-pricing it would divide a derived quantity by a declared
    /// one. Its weight is its pwu. A claim accepted below the fence keeps the declared basis
    /// (`None`), exactly as before.
    pub fn palw_claim_canonical_weight_v1(&self, claim: &PalwClaimStateV2, canonical_work_daa: Option<u64>) -> Option<u128> {
        canonical_work_daa?;
        if !matches!(claim.source, PalwClaimSourceV2::Attempt) || claim.accepted_daa < canonical_work_daa? {
            return None;
        }
        Some(claim.pwu as u128)
    }

    /// ADR-0135: a seat's last possession proof for a class.
    pub fn seat_readiness(
        &self,
        bond: &PalwBondKeyV2,
        class_id: &Hash64,
    ) -> Option<&crate::palw_model_registry_v1::PalwSeatReadinessRowV1> {
        self.seat_readiness.get(&(*bond, *class_id))
    }

    /// **Lane MU (ADR-0173): a seat's possession proof of one ROOT of a class.** The class's registered (founding) root is the
    /// legacy `(bond, class)` table's — which is what makes the fence need no migration — and any other root is the
    /// `(bond, class, root)` table's. `None` for the root means the founding root.
    pub fn seat_readiness_for_root(
        &self,
        bond: &PalwBondKeyV2,
        class_id: &Hash64,
        root: Option<&Hash64>,
    ) -> Option<&crate::palw_model_registry_v1::PalwSeatReadinessRowV1> {
        match root {
            Some(root) if self.classes.get(class_id).is_some_and(|class| class.artifact_root != *root) => {
                self.seat_root_readiness.get(&(*bond, *class_id, *root))
            }
            _ => self.seat_readiness.get(&(*bond, *class_id)),
        }
    }

    /// **Lane MU (ADR-0173): the row a CLASS-level reader (the lifecycle's ready count, the jury, the room, the RPC) judges a seat
    /// by** — below `palw_audit_1004_v1` the legacy row exactly; past it the freshest of the legacy row and the seat's rows for the
    /// roots in force ("holds the class" means holds a version in force, so a class whose founding root left force after its grace
    /// does not read as unseated while its current root is held).
    pub fn seat_readiness_class_v1(
        &self,
        params: &PalwStateParamsV2,
        bond: &PalwBondKeyV2,
        class_id: &Hash64,
        daa: u64,
    ) -> Option<&crate::palw_model_registry_v1::PalwSeatReadinessRowV1> {
        let legacy = self.seat_readiness.get(&(*bond, *class_id));
        if self.seat_root_readiness.is_empty() || !params.audit_1004_active_at(daa) {
            return legacy;
        }
        let lo = (*bond, *class_id, Hash64::from_bytes([0u8; 64]));
        let hi = (*bond, *class_id, Hash64::from_bytes([0xffu8; 64]));
        let in_force = self.class_roots_in_force(class_id, daa);
        let mut best = legacy;
        for ((_, _, root), row) in self.seat_root_readiness.range(lo..=hi) {
            if in_force.contains(root) && best.is_none_or(|held| row.proved_daa > held.proved_daa) {
                best = Some(row);
            }
        }
        best
    }

    pub fn seat_root_readiness_iter(
        &self,
    ) -> impl Iterator<Item = (&(PalwBondKeyV2, Hash64, Hash64), &crate::palw_model_registry_v1::PalwSeatReadinessRowV1)> {
        self.seat_root_readiness.iter()
    }

    pub fn seat_readiness_iter(
        &self,
    ) -> impl Iterator<Item = (&(PalwBondKeyV2, Hash64), &crate::palw_model_registry_v1::PalwSeatReadinessRowV1)> {
        self.seat_readiness.iter()
    }

    /// ADR-0132 Upgrade C: a live claim's economics snapshot, where the payout fence wrote one.
    pub fn claim_economics_of(&self, claim_id: &Hash64) -> Option<&crate::palw_economic_payout_v1::PalwClaimEconomicsV1> {
        self.claim_economics.get(claim_id)
    }

    pub fn claim_economics_iter(&self) -> impl Iterator<Item = (&Hash64, &crate::palw_economic_payout_v1::PalwClaimEconomicsV1)> {
        self.claim_economics.iter()
    }

    /// ADR-0137 (shadow): the work target as of the last epoch boundary this state folded, or
    /// `None` before the first boundary the shadow saw.
    pub fn work_target_shadow(&self) -> Option<&crate::palw_work_target_v1::PalwWorkTargetV2> {
        self.work_target_shadow.as_ref()
    }

    /// ADR-0137 / ADR-0132 S: the work target the chain holds (past `Params::palw_work_target`).
    /// ADR-0143 Decision 1: the one owner of an artifact root, or `None` where the index has not
    /// been built (the fence is dormant) or the root is unclaimed. A caller that must distinguish
    /// "no owner" from "no index" asks [`Self::artifact_owners_are_written`].
    pub fn artifact_owner_of(&self, class_id: &Hash64, root: &Hash64) -> Option<&crate::palw_model_lines_v1::PalwArtifactOwnerV1> {
        self.artifact_owners.get(&(*class_id, *root))
    }

    /// Whether the ownership index exists at all. Empty means the fence has not fired — past it the
    /// migration has written at least the base class's founding root.
    pub fn artifact_owners_are_written(&self) -> bool {
        !self.artifact_owners.is_empty()
    }

    pub fn artifact_owners_iter(&self) -> impl Iterator<Item = (&(Hash64, Hash64), &crate::palw_model_lines_v1::PalwArtifactOwnerV1)> {
        self.artifact_owners.iter()
    }

    /// **ADR-0145 §5: the class's graph, if this chain holds one.** `None` below
    /// `Params::palw_fp_derived_work` (nothing writes the index) and for a class that has not
    /// published one past it — and the two are the same answer on purpose: the transition refuses a
    /// free-prompt commitment of a class it cannot derive work for, whichever of the two is why.
    pub fn fp_work_profile_of(&self, class_id: &Hash64) -> Option<&crate::palw_step::PalwShapeProfileV3> {
        self.fp_work_profiles.get(class_id).map(|profile| profile.as_ref())
    }

    /// Whether any class has published a graph at all — how a reader tells "the fence is dormant"
    /// from "this class has not published".
    pub fn fp_work_profiles_are_written(&self) -> bool {
        !self.fp_work_profiles.is_empty()
    }

    /// **ADR-0145 §6: what this bond has already been paid for on this class**, as
    /// `(prompt_tokens, prefix_root)` pairs for
    /// [`crate::palw_freeprompt_v3::fp_accounted_prefix_tokens_v1`]. Empty below the fence.
    pub fn fp_claimed_prompts_of(&self, class_id: &Hash64, bond: &PalwBondKeyV2) -> Vec<(u32, Hash64)> {
        self.fp_claim_prompts
            .range((*class_id, *bond, Hash64::default())..)
            .take_while(|((class, key, _), _)| class == class_id && key == bond)
            .map(|(_, row)| (row.prompt_tokens, row.prefix_root))
            .collect()
    }

    /// **Every prompt this CLASS has been paid for, whoever committed it**, for the symmetric cache
    /// reading ([`crate::palw_freeprompt_v3::fp_accounted_prefix_tokens_v2`]). Empty below the
    /// fence.
    ///
    /// **The bond left the question** (re-audit 2026-09-19). Read per bond, the discount was a
    /// function of WHO was asking, so a producer holding one conversation's KV cache split its
    /// bond and sold the same prefill again at full price — and bonds are cheap and permanent.
    /// Whether a prefix's compute has been paid for is a fact about the prefix, not about the
    /// payee, so the reading is over the class. Together with the row now outliving its claim, the
    /// credit is a pure function of (this prompt, the prompts committed in this candidate chain):
    /// no bond, no claim id, no commitment order, and therefore the same total however a
    /// conversation is cut up, in whatever order, across however many bonds.
    ///
    /// **What it costs, stated rather than hidden.** A producer that genuinely re-runs a committed
    /// prompt COLD did the prefill and is now credited only its extension. The chain cannot tell a
    /// cold re-run from a cached one — that is what ADR-0145 §6's prefix-STATE commitment is for,
    /// and it is a change to the commitment's wire. Until it exists the conservative direction is
    /// the one that never pays for compute that may not have happened, because the other direction
    /// is the 62× finding.
    ///
    /// `now_daa` is the block asking: a row whose retention has run out does not answer, whether or
    /// not the boundary that sweeps it has been folded yet.
    pub fn fp_claimed_prompt_ids_of(&self, class_id: &Hash64, now_daa: u64) -> Vec<&[u32]> {
        self.fp_claim_prompts
            .range((*class_id, PALW_BOND_KEY_V2_MIN, Hash64::default())..)
            .take_while(|((class, _, _), _)| class == class_id)
            .filter(|(_, row)| row.expires_daa > now_daa)
            .map(|(_, row)| row.prompt_token_ids.as_slice())
            .collect()
    }

    /// **G14 R4 (node replay input): the prompt ids `claim` was accepted with**, when its acceptance kept them (the ADR-0145 row,
    /// written for compute-priced claims and swept at its retention). `None` otherwise: the caller then has no public input to
    /// replay, never a guessed one.
    pub fn fp_claim_prompt_ids_v1(&self, class_id: &Hash64, bond: &PalwBondKeyV2, claim: &Hash64) -> Option<Vec<u32>> {
        self.fp_claim_prompts.get(&(*class_id, *bond, *claim)).map(|row| row.prompt_token_ids.clone())
    }

    pub fn work_target(&self) -> Option<&crate::palw_work_target_v1::PalwWorkTargetV2> {
        self.work_target.as_ref()
    }

    /// ADR-0137 (shadow): finalized economic work by closed epoch and class.
    pub fn final_work_iter(&self) -> impl Iterator<Item = (&u64, &BTreeMap<Hash64, u128>)> {
        self.final_work.iter()
    }

    /// ADR-0137 (shadow): the reader's share — each class's finalized work over the last
    /// `window_epochs` epochs, in permille, class-id order.
    pub fn final_work_shares_v1(&self, window_epochs: u64) -> Vec<(Hash64, u16)> {
        crate::palw_work_target_v1::palw_final_work_shares_v1(&self.final_work, window_epochs)
    }

    /// **ADR-0119 Decision 2: the step ladder of a class, given the network's** — the ladder this
    /// state recorded when the class registered (a held class's, the regime's `2^40`), or the
    /// network's for every class it recorded none for. The one question every consensus reader of
    /// a claim's ladder asks, so the one-move court, the checkpoint court, the held DA court, the
    /// dissection and the extraction walk cannot answer it differently.
    pub fn class_step_ladder_v1(&self, class_id: &Hash64, network_ladder: u64) -> u64 {
        self.class_step_ladders.get(class_id).copied().unwrap_or(network_ladder)
    }

    /// **ADR-0119 Decision 5: is this a class under the held regime** — one that recorded its own
    /// ladder when it registered, which only a held class does.
    pub fn class_is_held_v1(&self, class_id: &Hash64) -> bool {
        self.class_step_ladders.contains_key(class_id)
    }

    /// **RFC-0006: an IR class's layer-shard plan**, if its registrant declared one.
    pub fn tir_shard_plan(&self, class_id: &Hash64) -> Option<&crate::palw_tir_shard_v1::PalwTirShardPlanV1> {
        self.tir_shard_plans.get(class_id)
    }

    /// **RFC-0006: what the chain keeps of a claim drawn per shard**, while the claim is live.
    pub fn tir_shard_claim(&self, claim_id: &Hash64) -> Option<&crate::palw_tir_shard_v1::PalwTirShardClaimV1> {
        self.tir_shard_claims.get(claim_id)
    }

    /// Every IR class with a layer-shard plan.
    pub fn tir_shard_plans_iter(&self) -> impl Iterator<Item = (&Hash64, &crate::palw_tir_shard_v1::PalwTirShardPlanV1)> {
        self.tir_shard_plans.iter()
    }

    /// Every live claim drawn per shard, with its per-shard record (the non-seat cell watcher's targets read it, agent SHARD).
    pub fn tir_shard_claims_iter(&self) -> impl Iterator<Item = (&Hash64, &crate::palw_tir_shard_v1::PalwTirShardClaimV1)> {
        self.tir_shard_claims.iter()
    }

    /// **The finite court window a class committed when it registered past `palw_model_court_window`**,
    /// `None` for every other class — every class registered below the fence, and every class past it that
    /// the rule does not reach (a graph class with no fused attention site, an IR class with no dissected
    /// cone). The table is empty on every chain below the fence, so a court opened for a class with no row
    /// takes the rule exactly as the release before this one folded it.
    pub fn class_model_court_window_v1(&self, class_id: &Hash64) -> Option<u64> {
        self.class_court_windows.get(class_id).copied()
    }

    /// **RFC-0007 Part II: a class's witness profile**, if it recorded one past `palw_witness_manifest_v1`.
    pub fn mesh_witness_profile_v1(&self, class_id: &Hash64) -> Option<&crate::palw_mesh_v1::PalwWitnessProfileRowV1> {
        self.vertex.mesh.witness.get(class_id)
    }

    /// **RFC-0007 Part IV.1: a claim's audit row**, while it stands.
    pub fn mesh_audit_row_v1(&self, claim: &Hash64) -> Option<&crate::palw_mesh_v1::PalwAuditRowV1> {
        self.vertex.mesh.audits.get(claim)
    }

    /// **RFC-0007 Part IV.1: the trap row of a commitment**, while it is open.
    pub fn mesh_trap_row_v1(&self, commitment: &Hash64) -> Option<&crate::palw_mesh_v1::PalwTrapRowV1> {
        self.vertex.mesh.traps.get(commitment)
    }

    /// **RFC-0007 Part IV.2: a claim's capped row**, while it stands.
    pub fn mesh_capped_row_v1(&self, claim: &Hash64) -> Option<&crate::palw_mesh_v1::PalwCappedClaimRowV1> {
        self.vertex.mesh.capped.get(claim)
    }

    /// **RFC-0007 Parts II and IV: what a node reads of the mesh** at the tip — the table sizes, the fences, and `seat`'s open audits.
    pub fn mesh_status_v1(&self, params: &PalwStateParamsV2, seat: &PalwBondKeyV2) -> crate::palw_mesh_v1::PalwMeshStatusV1 {
        crate::palw_mesh_v1::PalwMeshStatusV1 {
            witness_fence_daa: params.witness_manifest_from_daa(),
            audit_fence_daa: params.audit_mesh_from_daa(),
            capped_fence_daa: params.capped_from_daa(),
            witness_profiles: self.vertex.mesh.witness.len() as u64,
            audit_rows: self.vertex.mesh.audits.len() as u64,
            traps_open: self.vertex.mesh.traps.len() as u64,
            capped_claims: self.vertex.mesh.capped.len() as u64,
            own_audits: self
                .vertex
                .mesh
                .audits
                .iter()
                .filter_map(|(claim, row)| row.assignment_of(seat).map(|a| (*claim, a.ticket, a.outcome.is_some())))
                .collect(),
        }
    }

    /// **RFC-0007: the row the chain holds for `(round, seat)`** — the first vertex it accepted, or the conviction.
    pub fn vertex_round_row_v1(&self, round: u64, seat: &PalwBondKeyV2) -> Option<&crate::palw_vertex_v1::PalwVertexRoundRowV1> {
        self.vertex.rounds.get(&(round, *seat))
    }

    /// **RFC-0007: a `PanelBound` claim's tally of counted verdict leaves**, if any leaf counted.
    pub fn vertex_tally_of_v1(&self, claim: &Hash64) -> Option<&crate::palw_vertex_v1::PalwVertexTallyV1> {
        self.vertex.tallies.get(claim)
    }

    /// **RFC-0007: a live claim's `Held` attestations**, if any.
    pub fn vertex_held_of_v1(&self, claim: &Hash64) -> Option<&Vec<crate::palw_vertex_v1::PalwVertexHeldRowV1>> {
        self.vertex.held.get(claim)
    }

    /// **RFC-0007: what a node reads of the vertex tables** — the counts, and `seat`'s own rows from `from_round`.
    pub fn vertex_status_v1(
        &self,
        params: &PalwStateParamsV2,
        seat: &PalwBondKeyV2,
        from_round: u64,
    ) -> crate::palw_vertex_v1::PalwVertexStatusV1 {
        crate::palw_vertex_v1::PalwVertexStatusV1 {
            fence_from_daa: params.vertex_from_daa(),
            rounds: self.vertex.rounds.len() as u64,
            tallies: self.vertex.tallies.len() as u64,
            held_claims: self.vertex.held.len() as u64,
            own: self
                .vertex
                .rounds
                .iter()
                .filter(|((round, bond), _)| *round >= from_round && bond == seat)
                .map(|((round, _), row)| (*round, row.leaves_root, row.convicted))
                .collect(),
        }
    }

    /// RFC-0007: the number of `(round, seat)` rows, tallies and claims with `Held` rows (telemetry and the status the kit reads).
    /// **RFC-0010: the permissionless Panel's engine**, if the fence has created it on this chain.
    pub fn panel_v3(&self) -> Option<&misaka_palw_panel::PermissionlessPanelStateV1> {
        self.panel_v3.as_ref()
    }

    /// **Does the permissionless Panel's engine clock this claim?** `true` for a claim the engine tracks and has not ended: its
    /// seal, entropy, assignment and receipt-window timers are the engine's, so V2's bind and receipt deadlines do not apply to it
    /// while it is `Provisional` or `PanelBound`. **And for a claim whose engine end is DEFERRED** (the G14 guard,
    /// [`Self::palw_accusation_pending_v1`]): the engine decided a non-fraud end while an accusation was pending, the V2 claim
    /// still waits, and it is still the engine's — no V2 bind or receipt deadline, never offered to the lane-A binder — until the
    /// deferred end is applied or a conviction ends it first. `false` on every chain with no engine.
    pub fn panel_v3_clocks_claim_v1(&self, claim_id: &Hash64) -> bool {
        self.panel_v3.as_ref().and_then(|engine| engine.claim_rows().get(claim_id)).is_some_and(|record| match record.phase {
            misaka_palw_panel::ClaimPhaseV3::Voided { .. } => matches!(
                self.claims.get(claim_id).map(|claim| &claim.phase),
                Some(PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::DefaultDisputed { .. })
            ),
            ref phase => !phase.terminal(),
        })
    }

    /// **RFC-0010 × G14 — is an accusation pending on this claim?** (agent SHARD, `docs/design/palw/verification/shard-rfc6-10.md` §2.) A
    /// data-availability session (a seat's or a non-seat's) is open on it, a court session (bisection or dissection) is open on
    /// it, or it is `DefaultDisputed`. THE one predicate every V3 non-fraud end reads: while it holds, no expiry, redraw or
    /// seal/beacon absence closes the claim — the engine's receipt clock pauses, the engine's own end is deferred (the V2 void
    /// waits), and a V3 S2 licence holds no deadline — so an accusation always reaches its verdict or its own backstop and is
    /// never closed neutrally by a non-fraud void. Bounded: three non-seat DA sessions at once and sixteen over a claim's life,
    /// four per seat, each at most `W_disclose`; court sessions by the court's own capacity and backstop.
    pub fn palw_accusation_pending_v1(&self, claim_id: &Hash64) -> bool {
        // Lane LG14-A: a live dispute reservation is a pending accusation (empty below `palw_legacy_public_filer_v1`).
        // G14 lane r6-d1-release (D1): a court service binding is not an accusation — it must not pause or defer the claim's Final
        // (open courts and shared demands on the claim still count through their own rows below).
        self.palw_legacy_dispute_holds_v1(claim_id)
            || self.da_claims.get(claim_id).is_some_and(|record| record.open_sessions() > 0)
            || self.open_courts_by_claim.get(claim_id).is_some_and(|open| *open > 0)
            || matches!(self.claims.get(claim_id).map(|claim| &claim.phase), Some(PalwClaimPhaseV2::DefaultDisputed { .. }))
            // G14 R5/R6: an outside owner's open demand on an ORIGINAL TIR frame (empty below `palw_tir_public_conviction_v1`).
            || self.staged_tir_public_demand_open_v1(claim_id)
    }

    /// **A V3 claim's S2 licence held by a pending accusation** (DL-1's G14 row): the engine bound it (its record exists, released
    /// at the licence), it is an S2 licence not yet raised to a replay-backed one, and an accusation is pending — it owes no
    /// deadline, so the S2 expiry (`PanelUnavailable`, uncharged, which closes every session neutrally) cannot pre-empt it; the
    /// close of the last accusation re-derives the deadline. `false` on every chain with no engine.
    pub fn palw_v3_s2_licence_held_v1(&self, claim_id: &Hash64, claim: &PalwClaimStateV2) -> bool {
        matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. })
            && palw_rcore_licence_awaits_replay_v1(claim)
            && self.panel_v3.as_ref().is_some_and(|engine| engine.claim_rows().contains_key(claim_id))
            && self.palw_accusation_pending_v1(claim_id)
    }

    /// **Lane PL part C × G14: does the hold reach this lane-A claim?** (Agent SHARD, the Lead's ruling of 2026-10-09 under
    /// PRINCIPLES §3/§6.7: nothing escapes to an uncharged void while a valid accusation is open.) Past
    /// `palw_panel_unavailable_expiry` a lane-A claim's SECOND Panel ends uncharged when it fails — the second receipt timeout and
    /// an S2 licence's second gate expire `PanelUnavailable`, a redrawn claim's bind window `BindTimeout` — and an uncharged void
    /// closes every session on the claim neutrally. So the hold reaches a V2 claim (not the permissionless engine's, which has its
    /// own guard) whose redraw (`rebound_daa`) is at or past the fence, while it is `Provisional`, `PanelBound`, or an S2 licence
    /// awaiting replay. Keyed on the redraw — immutable — so DL-1 agrees at rest and in the fold. `false` on every shipped preset.
    pub fn palw_v2_expiry_hold_applies_v1(&self, params: &PalwStateParamsV2, claim_id: &Hash64, claim: &PalwClaimStateV2) -> bool {
        claim.rebound_daa.is_some_and(|at| params.panel_unavailable_expiry_active_at(at))
            && !self.panel_v3.as_ref().is_some_and(|engine| engine.claim_rows().contains_key(claim_id))
            && match claim.phase {
                PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. } => true,
                PalwClaimPhaseV2::ReceiptLicensed { .. } => palw_rcore_licence_awaits_replay_v1(claim),
                _ => false,
            }
    }

    /// **The hold itself** (DL-1's part-C G14 row): the hold reaches the claim and an accusation is pending on it
    /// ([`Self::palw_accusation_pending_v1`]) — it owes no deadline, so no uncharged expiry can pre-empt the accusation; the end of
    /// the last accusation re-derives the deadline, and an expiry already due then lands at the next sweep.
    pub fn palw_v2_expiry_held_v1(&self, params: &PalwStateParamsV2, claim_id: &Hash64, claim: &PalwClaimStateV2) -> bool {
        self.palw_v2_expiry_hold_applies_v1(params, claim_id, claim) && self.palw_accusation_pending_v1(claim_id)
    }

    pub fn vertex_counts_v1(&self) -> (usize, usize, usize) {
        (self.vertex.rounds.len(), self.vertex.tallies.len(), self.vertex.held.len())
    }

    /// **RFC-0007: the claims whose panel bound at `bound_daa`**, by a walk of the panels (the compact reference's
    /// resolution; the fold keeps a per-block index so a vertex of many compact leaves walks once).
    pub fn claims_bound_at_v1(&self, bound_daa: u64) -> Vec<Hash64> {
        self.panels.iter().filter(|(_, panel)| panel.bound_daa == bound_daa).map(|(claim, _)| *claim).collect()
    }

    /// **ADR-0165: the floor state as stored** — the default (Idle, no probe ended) where nothing was ever written. Not
    /// advanced to any DAA: ask [`crate::palw_real_share_v1::PalwFloorStateV1::admits_floor_at`] for a gate.
    pub fn floor_state_v1(&self) -> crate::palw_real_share_v1::PalwFloorStateV1 {
        self.floor_state.unwrap_or_default()
    }

    /// Lane PL part E (tests): install an availability row directly.
    #[cfg(test)]
    pub(crate) fn set_seat_availability_row_for_tests(
        &mut self,
        bond: PalwBondKeyV2,
        row: crate::palw_seat_availability_v1::PalwSeatAvailabilityV1,
    ) {
        self.seat_availability.insert(bond, row);
    }

    /// Lane PL part E: a bond's availability row, if it has one.
    pub fn seat_availability_of_v1(&self, bond: &PalwBondKeyV2) -> Option<&crate::palw_seat_availability_v1::PalwSeatAvailabilityV1> {
        self.seat_availability.get(bond)
    }

    /// The registered class's finite dispute backstop, or the network default.
    pub fn class_court_window_v1(&self, class_id: &Hash64, network_window: u64) -> u64 {
        self.class_model_court_window_v1(class_id).unwrap_or(network_window)
    }

    /// **ADR-0089 Decision 2: the window** — every row a read precompile may serve, flattened,
    /// at this state's own point (the EVM block's selected parent). Bond keys are resolved to
    /// their payout payloads, statuses to codes, and every line of every class is present
    /// (founding lines synthesised), so the executor needs no PALW type but the view's.
    pub fn evm_view_v1(&self, chain_id: u64, base_class_id: Hash64) -> crate::evm::model_market::PalwEvmViewV1 {
        use crate::evm::model_market::*;
        use crate::palw_model_lines_v1::{PalwModelLineStatusV1, PalwVersionStatusV1};
        let daa = self.last_point().map(|p| p.daa_score).unwrap_or(0);
        let payload_of = |bond: Option<PalwBondKeyV2>| bond.and_then(|b| self.bonds.get(&b)).map(|b| b.payout_payload);
        let classes: Vec<(Hash64, PalwEvmClassRowV1)> = self
            .classes
            .iter()
            .map(|(id, class)| {
                let status = match class.status {
                    PalwClassStatusV2::Active => 0,
                    PalwClassStatusV2::Frozen { .. } => 1,
                    PalwClassStatusV2::Registered { .. } => 2,
                    PalwClassStatusV2::Dormant { .. } => 3,
                };
                let row = PalwEvmClassRowV1 {
                    status,
                    share_permille: self.class_shares.get(id).copied().unwrap_or(0),
                    budget_blocks: self.epoch_budgets.as_ref().and_then(|b| b.budget_blocks.get(id).copied()).unwrap_or(0),
                    canonical_leaves: class.pwu_rule.canonical_leaves_v1(),
                    is_base: *id == base_class_id,
                    registrant_payload: payload_of(class.registrant_bond),
                    registered_daa: class.registered_daa,
                    certified_attempt: self.class_shares.contains_key(id),
                    certified_fp: self.fp_certified_classes.contains_key(id),
                    roots_in_force: self.class_roots_in_force(id, daa),
                };
                (*id, row)
            })
            .collect();
        let mut lines: BTreeMap<Hash64, PalwEvmLineRowV1> = BTreeMap::new();
        let mut versions: BTreeMap<(Hash64, u32), PalwEvmVersionRowV1> = BTreeMap::new();
        let line_row = |line: &crate::palw_model_lines_v1::PalwModelLineV1| PalwEvmLineRowV1 {
            class_id: line.class_id,
            owner_payload: payload_of(line.owner),
            developer_payload: payload_of(line.developer_bond()),
            maintainer_payload: payload_of(line.maintainer_bond()),
            name: line.name.clone(),
            founded_daa: line.founded_daa,
            current: line.current,
            versions_published: line.versions_published,
            preview_count: line.previews.len() as u32,
            contributor_permille_of_leg: line.contributor_permille_of_leg,
            status: match line.status {
                PalwModelLineStatusV1::Active => 0,
                PalwModelLineStatusV1::Retired => 1,
            },
        };
        let version_row = |v: &crate::palw_model_lines_v1::PalwModelVersionV1| {
            let (status, until_daa) = match v.status {
                PalwVersionStatusV1::Current => (0, 0),
                PalwVersionStatusV1::Preview => (1, 0),
                PalwVersionStatusV1::Superseded { until_daa } => (2, until_daa),
                PalwVersionStatusV1::Withdrawn => (3, 0),
            };
            PalwEvmVersionRowV1 {
                root: v.root,
                parent: v.parent,
                adopted_from: v.adopted_from,
                runtime_hash: v.runtime_hash,
                dataset_commitment: v.dataset_commitment,
                training_config_hash: v.training_config_hash,
                notes_hash: v.notes_hash,
                published_daa: v.published_daa,
                published_by_payload: payload_of(v.published_by),
                status,
                until_daa,
                attempt_claims: v.usage.attempt_claims,
                fp_claims: v.usage.fp_claims,
                work_leaves: v.usage.work_leaves,
                first_used_daa: v.usage.first_used_daa,
                last_used_daa: v.usage.last_used_daa,
            }
        };
        for id in self.classes.keys() {
            if *id == base_class_id {
                continue;
            }
            if !self.model_lines.contains_key(id)
                && let Some(line) = self.model_line_or_founding(id)
                && let Some(v1) = self.model_version(id, 1)
            {
                lines.insert(*id, line_row(&line));
                versions.insert((*id, 1), version_row(&v1));
            }
        }
        for (id, line) in self.model_lines.iter() {
            lines.insert(*id, line_row(line));
            for ((_, n), v) in self.model_versions.range((*id, 0)..=(*id, u32::MAX)) {
                versions.insert((*id, *n), version_row(v));
            }
        }
        let mut proposals: BTreeMap<Hash64, Vec<PalwEvmProposalRowV1>> = BTreeMap::new();
        for (pid, p) in self.model_proposals.iter() {
            proposals.entry(p.line_id).or_default().push(PalwEvmProposalRowV1 {
                proposal_id: *pid,
                root: p.root,
                note_hash: p.note_hash,
                by_payload: payload_of(Some(p.by)),
                posted_daa: p.posted_daa,
                adopted_in: p.adopted_in,
            });
        }
        let mut evaluations: BTreeMap<(Hash64, u32), Vec<PalwEvmEvaluationRowV1>> = BTreeMap::new();
        for ((l, n, by), e) in self.model_evaluations.iter() {
            evaluations.entry((*l, *n)).or_default().push(PalwEvmEvaluationRowV1 {
                evaluator_id: e.evaluator_id,
                score_permille: e.score_permille,
                report_hash: e.report_hash,
                by_payload: payload_of(Some(*by)),
                posted_daa: e.posted_daa,
                is_lines_own: e.is_lines_own,
            });
        }
        PalwEvmViewV1 {
            chain_daa: daa,
            chain_id,
            classes,
            lines: lines.into_iter().collect(),
            versions,
            proposals,
            evaluations,
            markets: self.model_markets.clone(),
            positions: self.model_positions.clone(),
            // P-B3: filled by the caller that holds the block's extras
            // (`palw_evm_market_refused_classes_v1`), and only past the audit fence.
            market_refused_classes: Default::default(),
        }
    }

    pub fn claim(&self, id: &Hash64) -> Option<&PalwClaimStateV2> {
        self.claims.get(id)
    }

    pub fn consumed_offence(&self, id: &Hash64) -> Option<&crate::palw_offence_v1::PalwConsumedOffenceV1> {
        self.consumed_offences.get(id)
    }

    /// **Every conviction the chain remembers, by offence key** — a read for ADR-0160's shadow
    /// accounting (`palw_capacity_shadow_v1`: attribution counters and the would-be AG-3 freeze).
    /// No rule reads it.
    pub fn consumed_offences_iter(&self) -> impl Iterator<Item = (&Hash64, &crate::palw_offence_v1::PalwConsumedOffenceV1)> + '_ {
        self.consumed_offences.iter()
    }

    pub fn slashable_lock(&self, seat: PalwBondKeyV2, claim: Hash64) -> Option<&crate::palw_panel_var_v1::PalwSlashableLockV1> {
        self.slashable_locks.get(&(seat, claim))
    }

    /// **ADR-0151: the executions whose downstream rights are forfeit.**
    ///
    /// Derived from `consumed_offences` rather than kept beside it: a conviction is written once, and
    /// a second ledger of "what that conviction forfeited" is a second thing to keep in step. Every
    /// `PanelFalseValid` that named an execution contributes its root; a conviction that named none
    /// (a key signing two roots is about the key) contributes nothing.
    ///
    /// Scanned per span boundary, which is O(convictions) on a map that only grows by a conviction —
    /// a whole objective offence, its evidence and its slash behind each row.
    pub fn palw_forfeited_execution_roots_v1(&self) -> std::collections::BTreeSet<Hash64> {
        self.consumed_offences.values().filter(|row| row.execution_root != Hash64::default()).map(|row| row.execution_root).collect()
    }

    /// **Whether one execution's rights are forfeit** — [`Self::palw_forfeited_execution_roots_v1`]
    /// asked about a single root, without building the set (2026-09-24 DoS audit #5).
    ///
    /// The zero root is never forfeit: a conviction below ADR-0151's bundle records it for "no
    /// execution named", and a claim that carries it names none either, so reading it as a match
    /// would forfeit every unrooted claim on the first conviction of the chain's life.
    pub fn palw_execution_root_is_forfeited_v1(&self, execution_root: &Hash64) -> bool {
        *execution_root != Hash64::default() && self.consumed_offences.values().any(|row| row.execution_root == *execution_root)
    }

    pub fn panel_liability(&self, claim: &Hash64) -> Option<&crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1> {
        self.panel_liabilities.get(claim)
    }

    /// **Whether a void the chain wrote binds this claim** — the live claim `Voided` for `reason` at
    /// `voided_daa`, or its liability row recording that same void once the claim has retired. The
    /// one reading of a `ProducerWithholding` / `CourtFraud` contradiction against a `Valid` signer,
    /// shared by the V1 fold (`bind_panel_false_valid`) and ADR-0152 v2's adjudicator
    /// ([`crate::palw_offence_attribution_v1::palw_check_panel_false_valid_v2`]), so the two routes
    /// cannot bind a void differently.
    pub fn palw_void_binds_claim_v1(&self, claim_id: &Hash64, reason: PalwVoidReasonV2, voided_daa: u64) -> bool {
        let live = self.claims.get(claim_id).is_some_and(
            |c| matches!(c.phase, PalwClaimPhaseV2::Voided { voided_daa: d, reason: r } if d == voided_daa && r == reason),
        );
        let liability = self
            .panel_liabilities
            .get(claim_id)
            .is_some_and(|row| crate::palw_panel_var_v1::palw_liability_matches_void_v1(row, reason, voided_daa));
        live || liability
    }

    /// How many court sessions are open on `claim_id` — the `open_courts_by_claim` index, which a
    /// session's open and close keep in step. `0` for a claim with none, or no longer live.
    pub fn open_courts_of(&self, claim_id: &Hash64) -> u32 {
        self.open_courts_by_claim.get(claim_id).copied().unwrap_or(0)
    }

    /// **Every Valid lock `bond` holds, in claim order** — a range over the bond's own keys of the
    /// `(bond, claim)`-keyed map, never a scan of the whole map filtered by bond (the 2026-09-23
    /// route-matrix re-audit's #3). The route-matrix #3 draw asks for each eligible bond of every
    /// pending claim in every chain block, in the assembler and the acceptance alike, and the map is
    /// never pruned; filtering the whole map made that O(bonds × all locks) per claim. The set read is
    /// the same set, so every figure summed over it is unchanged on every network.
    pub fn slashable_locks_of(
        &self,
        bond: &PalwBondKeyV2,
    ) -> impl Iterator<Item = (&(PalwBondKeyV2, Hash64), &crate::palw_panel_var_v1::PalwSlashableLockV1)> + '_ {
        let bond = *bond;
        self.slashable_locks.range((bond, Hash64::default())..).take_while(move |((b, _), _)| *b == bond)
    }

    /// [`Self::slashable_available`] on both clocks — see `PalwSlashableLockV1::is_live_v2`.
    pub fn slashable_available_v2(&self, bond: &PalwBondKeyV2, now_daa: u64, depth: Option<u64>) -> u128 {
        let posted = self.bonds.get(bond).map(|b| b.collateral as u128).unwrap_or(0);
        let locked = self
            .slashable_locks_of(bond)
            .filter(|(_, lock)| lock.is_live_v2(now_daa, self.settled_attempt_finals, depth))
            .map(|(_, lock)| lock.amount)
            .fold(0u128, u128::saturating_add);
        crate::palw_panel_var_v1::palw_available_slashable_v1(posted, locked)
    }

    pub fn slashable_available(&self, bond: &PalwBondKeyV2, now_daa: u64) -> u128 {
        let posted = self.bonds.get(bond).map(|b| b.collateral as u128).unwrap_or(0);
        let locked = self
            .slashable_locks_of(bond)
            .filter(|(_, lock)| lock.is_live(now_daa))
            .map(|(_, lock)| lock.amount)
            .fold(0u128, u128::saturating_add);
        crate::palw_panel_var_v1::palw_available_slashable_v1(posted, locked)
    }

    /// The escrows this state released — what the NEXT block's coinbase must pay, in claim-id
    /// order. Empty on every state whose last block finalized nothing, which is most of them.
    pub fn pending_payouts_iter(&self) -> impl Iterator<Item = (&Hash64, &PalwPayoutV2)> {
        self.pending_payouts.iter()
    }

    /// **Who may be served a claim's PRIVATE material** (ADR-0077 Decision 16's transport half;
    /// private-prompts design, 2026-09-05): the executor's own bond, every seat of the panel
    /// bound to the claim, and the challenger of every court session open on it. Each of those
    /// has a duty that needs the bytes; nobody else does, and under `PanelDa` the prompt inside
    /// them is exactly what the commitment withheld from the chain. Read by the executor's node
    /// before it answers a pull for a mode-2 claim — a stranger's pull, however well signed, is
    /// refused by name.
    pub fn claim_readers_v2(&self, claim: &Hash64) -> Vec<PalwBondKeyV2> {
        let mut readers: Vec<PalwBondKeyV2> = Vec::new();
        let mut admit = |bond: &PalwBondKeyV2| {
            if !readers.contains(bond) {
                readers.push(*bond);
            }
        };
        if let Some(record) = self.claims.get(claim) {
            admit(&record.bond);
        }
        if let Some(panel) = self.panels.get(claim) {
            for seat in &panel.seats {
                admit(&seat.bond);
            }
        }
        for session in self.court_sessions.values().filter(|s| s.claim == *claim) {
            admit(&session.challenger_bond);
        }
        readers
    }

    pub fn panel(&self, claim: &Hash64) -> Option<&PalwPanelStateV2> {
        self.panels.get(claim)
    }

    pub fn court_session(&self, id: &Hash64) -> Option<&PalwCourtSessionStateV2> {
        self.court_sessions.get(id)
    }

    pub fn epoch_counter(&self, class_id: &Hash64) -> Option<&PalwEpochCounterV2> {
        self.epoch_counters.get(class_id)
    }

    /// Iterate the bond registry in canonical key order — what the panel sortition tickets.
    pub fn bonds_iter(&self) -> impl Iterator<Item = (&PalwBondKeyV2, &PalwBondStateV2)> {
        self.bonds.iter()
    }

    /// **Has this public key ever registered a bond here — and is that bond still Active?**
    ///
    /// **Deliberately UNFILTERED, and the status rides along rather than narrowing the answer.**
    /// The registry is append-only: `BondRetireRequested` rewrites `status` in place and nothing
    /// else writes a bond row away (`write_bond(_, None)` is reachable only from a delta undo), and
    /// `BondRegistered` refuses any key that already appears in it —
    /// [`PalwStateV2Error::DuplicateBondKey`], "one key, one bond". So a key that has held a bond
    /// can never register another, retired or not.
    ///
    /// That is why filtering `Retiring` out here would be a defect and not a fix. A node whose
    /// "am I already bonded?" question skipped retiring bonds would not gain the ability to
    /// register a replacement — it would build and submit a carrier the transition must reject,
    /// spend the fee, and leave the operator watching a ten-minute "no bond appeared" timeout
    /// instead of reading the one line that tells them the truth: this key is spent, mint a new
    /// ML-DSA-87 seed. The status is returned so the caller can say WHICH situation it is in
    /// without asking a second question; `palw_bond_may_take_work_v2` is the predicate for
    /// "may this bond be given work", which is a different question again.
    pub fn bond_of_pubkey_v2(&self, pubkey: &[u8]) -> Option<(PalwBondKeyV2, PalwBondStatusV2)> {
        self.bonds.iter().find(|(_, bond)| bond.pubkey == pubkey).map(|(key, bond)| (*key, bond.status.clone()))
    }

    /// Decision 6's `reserved_exposure(bond)` — what the admission ceiling is checked against.
    /// Tests only: pin a class's receipt-lane target directly. The transition seeds every class
    /// at [`PALW_RECEIPT_TARGET_SEED_V1`] and only the receipt retarget moves it, so a test that
    /// needs a generous or a stingy receipt target sets it here rather than through a registration
    /// field that no longer reaches this slot.
    /// Tests only: mark `open` court sessions on `claim_id` (0 removes the entry) — what
    /// [`Self::palw_accusation_pending_v1`] reads.
    #[cfg(test)]
    pub(crate) fn set_open_courts_for_tests(&mut self, claim_id: Hash64, open: u32) {
        if open == 0 {
            self.open_courts_by_claim.remove(&claim_id);
        } else {
            self.open_courts_by_claim.insert(claim_id, open);
        }
    }

    #[cfg(test)]
    pub(crate) fn set_receipt_target_for_tests(&mut self, class_id: Hash64, target: u128) {
        self.receipt_targets.insert(class_id, PalwClassTargetV2 { target });
    }

    /// Tests only: write a class's registry row directly, for the modules outside this one that
    /// need a class the canonical-work fence can price without folding the registry to get it.
    #[cfg(test)]
    pub(crate) fn set_model_lifecycle_for_tests(
        &mut self,
        class_id: Hash64,
        row: crate::palw_model_registry_v1::PalwModelLifecycleRowV1,
    ) {
        self.model_lifecycles.insert(class_id, row);
    }

    /// RFC-0023 tests: put a claim into / take it out of the unresolved index (the fold keeps it in lockstep with every claim write).
    #[cfg(test)]
    pub(crate) fn set_unresolved_for_tests(&mut self, blue_score: u64, claim_id: Hash64, open: bool) {
        if open {
            self.unresolved.insert((blue_score, claim_id));
        } else {
            self.unresolved.remove(&(blue_score, claim_id));
        }
    }
    /// Tests only: the rows ADR-0152 v2's adjudicator resolves a false-Valid target from — a
    /// claim, its bound panel, its liability row — and the session indices that make it wait, for
    /// `palw_offence_attribution_v1`'s tests, which judge evidence against state without folding a
    /// claim's whole lifecycle to reach it.
    #[cfg(test)]
    pub(crate) fn set_false_valid_rows_for_tests(
        &mut self,
        claim_id: Hash64,
        claim: Option<PalwClaimStateV2>,
        panel: Option<PalwPanelStateV2>,
        liability: Option<crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1>,
        open_courts: u32,
    ) {
        match claim {
            Some(claim) => self.claims.insert(claim_id, claim),
            None => self.claims.remove(&claim_id),
        };
        match panel {
            Some(panel) => self.panels.insert(claim_id, panel),
            None => self.panels.remove(&claim_id),
        };
        match liability {
            Some(row) => self.panel_liabilities.insert(claim_id, row),
            None => self.panel_liabilities.remove(&claim_id),
        };
        if open_courts == 0 {
            self.open_courts_by_claim.remove(&claim_id);
        } else {
            self.open_courts_by_claim.insert(claim_id, open_courts);
        }
    }

    /// Tests only: a class row, for the same tests — the artifact root an operand opening proves
    /// against is read from it.
    #[cfg(test)]
    pub(crate) fn set_class_for_tests(&mut self, class_id: Hash64, class: PalwClassStateV2) {
        self.classes.insert(class_id, class);
    }

    /// Tests only (G14 r6): replace a claim row in place (e.g. voided by a conviction the test does not fold).
    #[cfg(test)]
    pub(crate) fn set_claim_for_tests(&mut self, claim_id: Hash64, claim: PalwClaimStateV2) {
        self.claims.insert(claim_id, claim);
    }

    /// **What `bond` can still lock for a `Valid` signature at `now_daa`**: its posted collateral
    /// less every lock still live on the two clocks. The fold's bind and licence checks and the
    /// panel draw read this one expression (the 2026-09-23 route-matrix audit's #3).
    ///
    /// **Per lock, after the liveness escape** (2026-09-24 DoS audit): `settled_anchor_depth` is
    /// the second clock's depth (`None` is the DAA-only rule, byte for byte), taken through
    /// [`palw_second_clock_depth_v1`] on this state's ring at `now_daa` — idempotent, so a caller
    /// may pass the depth already escaped — and each lock is live by
    /// `PalwSlashableLockV1::is_live_v3`, which bounds the second clock per obligation at
    /// `2 × window_court` past its DAA expiry.
    pub fn palw_slashable_available_v1(
        &self,
        bond: &PalwBondKeyV2,
        now_daa: u64,
        settled_anchor_depth: Option<u64>,
        window_court: u64,
    ) -> u128 {
        let posted = self.bonds.get(bond).map(|b| b.collateral as u128).unwrap_or(0);
        let settled_now = self.settled_attempt_finals;
        let depth = palw_second_clock_depth_v1(settled_anchor_depth, &self.recent_anchor_daas, now_daa, window_court);
        let locked = self
            .slashable_locks_of(bond)
            .filter(|(_, lock)| lock.is_live_v3(now_daa, settled_now, depth, window_court))
            .map(|(_, lock)| lock.amount)
            .fold(0u128, u128::saturating_add);
        crate::palw_panel_var_v1::palw_available_slashable_v1(posted, locked)
    }

    /// **The `Valid` locks on `bond` the fold counts as live at `now_daa`, by claim** — the filter
    /// [`Self::palw_slashable_available_v1`] sums and `BondRetireRequested`'s refusal
    /// (`slashable_live_locked`, "still holds … sompi of slashable panel locks") sums: the lock's
    /// DAA clock, or the second clock at the depth the liveness escape leaves
    /// ([`palw_second_clock_depth_v1`] over `settled_anchor_depth`, the RAW depth at `now_daa`).
    /// Read-only, for node policy and RPC (testnet-12 lifecycle audit T12-058: a seat's retirement
    /// was dropped for locks no read showed); nothing that folds calls it.
    pub fn palw_live_slashable_locks_v1(
        &self,
        bond: &PalwBondKeyV2,
        now_daa: u64,
        settled_anchor_depth: Option<u64>,
        window_court: u64,
    ) -> impl Iterator<Item = (Hash64, &crate::palw_panel_var_v1::PalwSlashableLockV1)> + '_ {
        let settled_now = self.settled_attempt_finals;
        let depth = palw_second_clock_depth_v1(settled_anchor_depth, &self.recent_anchor_daas, now_daa, window_court);
        self.slashable_locks_of(bond)
            .filter(move |(_, lock)| lock.is_live_v3(now_daa, settled_now, depth, window_court))
            .map(|((_, claim), lock)| (*claim, lock))
    }

    /// The second clock's depth at `now_daa` after the liveness escape, from the RAW depth — what
    /// [`Self::palw_live_slashable_locks_v1`] judges with, for a reader that must say whether the
    /// DAA clock or the anchor count is what still holds a lock.
    pub fn palw_escaped_second_clock_depth_v1(
        &self,
        settled_anchor_depth: Option<u64>,
        now_daa: u64,
        window_court: u64,
    ) -> Option<u64> {
        palw_second_clock_depth_v1(settled_anchor_depth, &self.recent_anchor_daas, now_daa, window_court)
    }

    pub fn reserved_exposure(&self, key: &PalwBondKeyV2) -> u128 {
        self.reserved_exposure.get(key).copied().unwrap_or(0)
    }

    /// ADR-0056 Decision 3: what this bond has reserved by REGISTERING classes, as opposed to by
    /// making claims. The ceiling is applied to the sum; they are kept apart so each stays
    /// derivable from the collection it summarizes.
    pub fn registration_exposure(&self, key: &PalwBondKeyV2) -> u128 {
        self.registration_exposure.get(key).copied().unwrap_or(0)
    }

    /// The walk counters for a class, if any boundary has folded one yet.
    pub fn class_walk(&self, key: &Hash64) -> Option<PalwClassWalkV2> {
        self.class_walks.get(key).copied()
    }

    /// Iterate the open court sessions in canonical id order — what a party's own court duty list
    /// is built from.
    pub fn court_sessions_iter(&self) -> impl Iterator<Item = (&Hash64, &PalwCourtSessionStateV2)> {
        self.court_sessions.iter()
    }

    /// How many court sessions this state holds. A retired claim must leave none of its own.
    pub fn court_sessions_len(&self) -> usize {
        self.court_sessions.len()
    }

    /// Sessions still armed against `claim`. Non-zero after the claim has been retired is the
    /// state that halts a chain — see `a_court_session_cannot_outlive_its_claim_and_stop_the_chain`.
    pub fn court_sessions_for_claim(&self, claim: &Hash64) -> usize {
        self.court_sessions.values().filter(|s| s.claim == *claim).count()
    }

    pub fn safe_weight(&self) -> u128 {
        self.safe_weight
    }

    pub fn bounded_immature(&self) -> u128 {
        self.bounded_immature
    }

    /// The `safe_weight` carried by claims this state no longer holds.
    ///
    /// Hashed into [`Self::state_root`], so what `retire_claim` books here is a consensus choice
    /// and not bookkeeping. Public so a test can pin that choice directly: the accumulator is the
    /// only place a retired claim's contribution still exists, and asserting on `safe_weight`
    /// alone cannot tell "priced at `pwu`" from "priced through Decision 7" for a claim whose
    /// class was weightless the whole time — the reviewer's mutation of exactly that line left
    /// every test green.
    pub fn retired_safe_weight(&self) -> u128 {
        self.retired_safe_weight
    }

    /// The second clock's reading: anchors ever settled (rooted, monotone) — attempt licences past
    /// `palw_audit_2026_09_23`, attempt `Final`s below it. See the field.
    pub fn settled_attempt_finals(&self) -> u64 {
        self.settled_attempt_finals
    }

    /// The DAAs of the most recent anchors, sorted ascending — see the field. Empty below
    /// `palw_audit_2026_09_23`.
    pub fn recent_anchor_daas(&self) -> &[u64] {
        &self.recent_anchor_daas
    }

    // ---- ADR-0152 R-core+ (the v22 skeleton): read accessors. Every map is empty and every
    // counter zero on every network until its writer lands. ----

    /// Row 16: a bond's live S1 strikes (the DAA of the first S1 in each aligned epoch).
    pub fn withholding_strikes(&self, bond: &PalwBondKeyV2) -> Option<&[u64]> {
        self.withholding_strikes.get(bond).map(|v| v.as_slice())
    }

    /// Row 12: the reporter reward pending on an offence key.
    pub fn reward_pending(&self, offence_key: &Hash64) -> Option<&PalwPendingRewardV1> {
        self.reward_pending.get(offence_key)
    }

    /// Row 13: an open reporter commitment, by its row key — the slot
    /// [`palw_reporter_commit_slot_v1`]`(commitment, reporter)` (S-7); [`Self::reporter_commitment_of`]
    /// reads it by the pair.
    pub fn reporter_commitment(&self, commitment: &Hash64) -> Option<&PalwReporterCommitV1> {
        self.reporter_commitments.get(commitment)
    }

    /// R-3's cap: how many reporter commitments `reporter` holds open (the derived index, S-SPEC 1g).
    pub fn reporter_open_commitments(&self, reporter: &PalwBondKeyV2) -> u32 {
        self.commitments_by_reporter.get(reporter).copied().unwrap_or(0)
    }

    /// Row 11: the awarded reporter rewards step 3d has yet to move, in key order.
    pub fn reporter_rewards_iter(&self) -> impl Iterator<Item = (&Hash64, &PalwPayoutV2)> {
        self.reporter_rewards.iter()
    }

    /// Row 12, whole: every reporter reward still in its reveal window, in offence-key order (S-7).
    pub fn reward_pending_iter(&self) -> impl Iterator<Item = (&Hash64, &PalwPendingRewardV1)> {
        self.reward_pending.iter()
    }

    /// Row 11, one key: the award step 3d has yet to move for this offence (S-7).
    pub fn reporter_reward(&self, offence_key: &Hash64) -> Option<&PalwPayoutV2> {
        self.reporter_rewards.get(offence_key)
    }

    /// Row 13 by the pair (S-7): `reporter`'s open commitment to `commitment`, if it rooted one —
    /// another reporter's copy of the same hash is another row.
    pub fn reporter_commitment_of(&self, commitment: &Hash64, reporter: &PalwBondKeyV2) -> Option<&PalwReporterCommitV1> {
        self.reporter_commitments.get(&palw_reporter_commit_slot_v1(commitment, reporter))
    }

    /// Row 17, the reporter half.
    pub fn reporter_counters(&self) -> PalwReporterCountersV1 {
        self.reporter_counters
    }

    /// Row 10: a Final claim's vesting row (S-SPEC P5's `vesting_row`).
    pub fn vesting_row(&self, claim_id: &Hash64) -> Option<&crate::palw_vesting_v1::PalwVestingRowV1> {
        self.vesting.get(claim_id)
    }

    /// Row 17, the vesting half.
    pub fn vesting_counters(&self) -> crate::palw_vesting_v1::PalwVestingCountersV1 {
        self.vesting_counters
    }

    // ---- ADR-0152 V-3 / phase2-plan §2.3 (the vesting work): the rows through their derived
    // indexes. ----

    /// Every vesting row in V-7's order, `(expiry_daa, claim_id)` (phase2-plan §2.3).
    pub fn vesting_iter_by_expiry(&self) -> impl Iterator<Item = &crate::palw_vesting_v1::PalwVestingRowV1> + '_ {
        self.vesting_order.iter().map(move |(_, claim_id)| self.vesting.get(claim_id).expect("the vesting order indexes live rows"))
    }

    /// How many vesting rows the state holds.
    pub fn vesting_len(&self) -> usize {
        self.vesting.len()
    }

    /// The rows `bond` is payee of (producer or credited seat), latched or not, unlatched first.
    pub fn vesting_rows_of_payee<'s>(
        &'s self,
        bond: &PalwBondKeyV2,
    ) -> impl Iterator<Item = &'s crate::palw_vesting_v1::PalwVestingRowV1> + use<'s> {
        let bond = *bond;
        self.vesting_payees
            .range((bond, false, 0, ZERO_HASH64)..)
            .take_while(move |(payee, ..)| *payee == bond)
            .map(move |(_, _, _, claim_id)| self.vesting.get(claim_id).expect("the payee index names live rows"))
    }

    /// **Every leg `bond` is payee of** (phase2-plan §2.3's `vesting_legs_of_payee`, for RPC):
    /// through the payee index, each of its rows (latched or not) with the legs that pay it — a
    /// producer that also sat on its own claim's panel is named twice, once per leg.
    pub fn vesting_legs_of_payee<'s>(
        &'s self,
        bond: &PalwBondKeyV2,
    ) -> impl Iterator<Item = (&'s crate::palw_vesting_v1::PalwVestingRowV1, crate::palw_vesting_v1::PalwVestingLegV1)> + use<'s> {
        let bond = *bond;
        self.vesting_rows_of_payee(&bond)
            .flat_map(move |row| row.legs().filter(|leg| leg.payee_bond == Some(bond)).map(|leg| (row, leg)).collect::<Vec<_>>())
    }

    /// `(expiry_daa, claim_id)` of every UNLATCHED row `bond` is payee of, ascending — B-3's walk
    /// (`palw_bond_is_payee_of_unmatured_row_v1`) reads it from the top.
    pub fn vesting_unlatched_of_payee<'s>(&'s self, bond: &PalwBondKeyV2) -> impl DoubleEndedIterator<Item = (u64, Hash64)> + use<'s> {
        self.vesting_payees
            .range((*bond, false, 0, ZERO_HASH64)..=(*bond, false, u64::MAX, Hash64::from_bytes([0xFF; 64])))
            .map(|(_, _, expiry_daa, claim_id)| (*expiry_daa, *claim_id))
    }

    /// One queued payout, by key (V-7's "new key" question).
    pub fn pending_payout(&self, key: &Hash64) -> Option<&PalwPayoutV2> {
        self.pending_payouts.get(key)
    }

    /// Enter one row into the four vesting indexes (ADR-0152 V-3).
    pub(super) fn vesting_index_insert(&mut self, row: &crate::palw_vesting_v1::PalwVestingRowV1) {
        self.vesting_order.insert((row.expiry_daa, row.claim_id));
        let latched = row.matured_at.is_some();
        if !latched {
            self.vesting_unlatched.insert((row.expiry_daa, row.claim_id));
            self.vesting_unlatched_by_settled.insert((row.settled_at_final, row.claim_id));
        }
        for payee in row.payee_bonds() {
            self.vesting_payees.insert((payee, latched, row.expiry_daa, row.claim_id));
        }
    }

    /// Take one row out of the four vesting indexes — the exact entries `vesting_index_insert`
    /// made for it, so a row's rewrite (its latch, M3's DA-5 re-key) is remove-old-then-insert-new.
    pub(super) fn vesting_index_remove(&mut self, row: &crate::palw_vesting_v1::PalwVestingRowV1) {
        self.vesting_order.remove(&(row.expiry_daa, row.claim_id));
        let latched = row.matured_at.is_some();
        self.vesting_unlatched.remove(&(row.expiry_daa, row.claim_id));
        self.vesting_unlatched_by_settled.remove(&(row.settled_at_final, row.claim_id));
        for payee in row.payee_bonds() {
            self.vesting_payees.remove(&(payee, latched, row.expiry_daa, row.claim_id));
        }
    }

    /// Rebuild the four native vesting indexes and the additional EXEC payee index (import; phase2-plan I-7).
    pub(super) fn rebuild_vesting_indices(&mut self) {
        self.vesting_order.clear();
        self.vesting_unlatched.clear();
        self.vesting_unlatched_by_settled.clear();
        self.vesting_payees.clear();
        self.exec_v2_reward_payees.clear();
        for (id, row) in &self.exec_v2_rewards {
            for (bond, _) in row.legs.keys() {
                self.exec_v2_reward_payees.entry(*bond).or_default().insert(*id);
            }
        }
        let rows: Vec<crate::palw_vesting_v1::PalwVestingRowV1> = self.vesting.values().cloned().collect();
        for row in &rows {
            self.vesting_index_insert(row);
        }
    }

    /// Do the vesting indexes describe exactly the rows? (`assert_internal_consistency_v3`.)
    pub(super) fn vesting_indices_are_consistent(&self) -> bool {
        let mut rebuilt = PalwChainStateV2::genesis();
        for row in self.vesting.values() {
            rebuilt.vesting_index_insert(row);
        }
        rebuilt.vesting_order == self.vesting_order
            && rebuilt.vesting_unlatched == self.vesting_unlatched
            && rebuilt.vesting_unlatched_by_settled == self.vesting_unlatched_by_settled
            && rebuilt.vesting_payees == self.vesting_payees
    }

    /// Row 14 (DA-2): the open session `accuser` holds on `claim_id`.
    pub fn da_session(&self, claim_id: &Hash64, accuser: &PalwBondKeyV2) -> Option<&crate::palw_da_rcore_v1::PalwDaSessionV1> {
        self.da_sessions.get(&(*claim_id, *accuser))
    }

    /// Row 14 (DA-2): every open session on `claim_id`, in accuser order.
    pub fn da_sessions_of(
        &self,
        claim_id: &Hash64,
    ) -> impl Iterator<Item = (&PalwBondKeyV2, &crate::palw_da_rcore_v1::PalwDaSessionV1)> + '_ {
        let claim_id = *claim_id;
        self.da_sessions
            .range((claim_id, PALW_BOND_KEY_V2_MIN)..)
            .take_while(move |((claim, _), _)| *claim == claim_id)
            .map(|((_, accuser), session)| (accuser, session))
    }

    /// Row 15 (DA-2): a claim's court record, from its first session until the claim retires.
    pub fn da_claim(&self, claim_id: &Hash64) -> Option<&crate::palw_da_rcore_v1::PalwDaClaimV1> {
        self.da_claims.get(claim_id)
    }

    /// **ADR-0152-adjacent: a class's Activation Pool**, if it has one.
    pub fn activation_pool(&self, class_id: &Hash64) -> Option<&crate::palw_activation_pool_v1::PalwActivationPoolV1> {
        self.activation_pools.get(class_id)
    }

    /// Every Activation Pool, by class id.
    pub fn activation_pools_iter(
        &self,
    ) -> impl Iterator<Item = (&Hash64, &crate::palw_activation_pool_v1::PalwActivationPoolV1)> + '_ {
        self.activation_pools.iter()
    }

    /// The Activation Pool counters (the sums of every row, I2).
    pub fn activation_pool_counters(&self) -> crate::palw_activation_pool_v1::PalwActivationPoolCountersV1 {
        self.activation_pool_counters
    }

    /// The payouts a class's pool owes and has not flushed yet, by payee payload (F5).
    pub fn activation_pool_scheduled_iter(&self) -> impl Iterator<Item = (&(Hash64, Hash64), &u64)> + '_ {
        self.activation_pool_scheduled.iter()
    }

    /// The span a Candidate's latest readiness proof by `bond` landed in (F4), if recorded.
    pub fn activation_readiness_landed(&self, class_id: &Hash64, bond: &PalwBondKeyV2) -> Option<u64> {
        self.activation_readiness_landed.get(&(*class_id, *bond)).copied()
    }

    /// **Does this state hold any Activation Pool data?** The one guard of the `activation_pool/v1`
    /// root block and the `0xB5` carriage tail.
    pub(super) fn has_activation_pool_data(&self) -> bool {
        !self.activation_pools.is_empty()
            || !self.activation_pool_counters.is_zero()
            || !self.activation_pool_scheduled.is_empty()
            || !self.activation_readiness_landed.is_empty()
    }

    /// **The 2026-09-25 sweep's V03(2)/V05: a class's probation memory**, if it holds one.
    pub fn probation_memory_of_v1(&self, class_id: &Hash64) -> Option<&crate::palw_model_registry_v1::PalwProbationMemoryV1> {
        self.probation_memory.get(class_id)
    }

    /// **ADR-0160 AG-3: `bond`'s freeze**, if it has one (see
    /// [`crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1`]).
    pub fn bond_freeze_of_v1(&self, bond: &PalwBondKeyV2) -> Option<&crate::palw_aggregate_liability_v1::PalwBondFreezeV1> {
        self.bond_freezes.get(bond)
    }

    /// ADR-0160 AG-3: every freeze, in bond order.
    pub fn bond_freezes_iter_v1(
        &self,
    ) -> impl Iterator<Item = (&PalwBondKeyV2, &crate::palw_aggregate_liability_v1::PalwBondFreezeV1)> {
        self.bond_freezes.iter()
    }

    /// **ADR-0160 F-Q: `claim`'s audit receipts**, if it holds any.
    pub fn audit_status_of_v1(&self, claim: &Hash64) -> Option<&crate::palw_audit_door_v1::PalwAuditStatusV1> {
        self.audit_receipts.get(claim)
    }

    /// ADR-0160 F-Q: every claim's receipts, in claim order.
    pub fn audit_receipts_iter_v1(&self) -> impl Iterator<Item = (&Hash64, &crate::palw_audit_door_v1::PalwAuditStatusV1)> {
        self.audit_receipts.iter()
    }

    /// **ADR-0160 F-Q (§5.9 (g)): is `bond` excluded from every audit pool?**
    pub fn auditor_excluded_v1(&self, bond: &PalwBondKeyV2) -> bool {
        self.excluded_auditors.contains_key(bond)
    }

    /// ADR-0160 F-Q: every excluded auditor with the DAA of its exclusion.
    pub fn excluded_auditors_iter_v1(&self) -> impl Iterator<Item = (&PalwBondKeyV2, &u64)> {
        self.excluded_auditors.iter()
    }

    /// **ADR-0160 F-N: the DAA through which `bond`'s unmet demand for `class_id` is registered**, if it is.
    pub fn network_demand_of_v1(&self, bond: &PalwBondKeyV2, class_id: &Hash64) -> Option<u64> {
        self.network_demand.get(&(*bond, *class_id)).copied()
    }

    /// **ADR-0160 F-N: is `bond` registered at `now_daa`** (for any class) — lane N's `Reg(t)`.
    pub fn network_registered_v1(&self, bond: &PalwBondKeyV2, now_daa: u64) -> bool {
        self.network_demand
            .range((*bond, Hash64::default())..)
            .take_while(|((key, _), _)| key == bond)
            .any(|(_, until)| *until >= now_daa)
    }

    /// ADR-0160 F-N: every registered demand ((bond, class) → until DAA).
    pub fn network_demand_iter_v1(&self) -> impl Iterator<Item = (&(PalwBondKeyV2, Hash64), &u64)> {
        self.network_demand.iter()
    }

    /// ADR-0160 F-N: the operator-attempt ring (DAA → chain blocks).
    pub fn operator_ring_v1(&self) -> &BTreeMap<u64, u32> {
        &self.operator_ring
    }

    /// **ADR-0164 F-EM: the milli-carves charged in `daa`** (0 where the ledger holds no row for it).
    pub fn emission_spent_milli_v1(&self, daa: u64) -> u64 {
        match self.capacity_ledger.get(&crate::palw_capacity_s567_v1::PalwCapacityLedgerKeyV1::Emission(daa)) {
            Some(crate::palw_capacity_s567_v1::PalwCapacityLedgerRowV1::Emission { spent_milli }) => *spent_milli,
            _ => 0,
        }
    }

    /// **ADR-0164 F-M1: has `lead` taken its riders already?** (Its mark stands for the riders' window.)
    pub fn rider_mark_of_v1(&self, lead: &Hash64) -> Option<(u16, u64)> {
        match self.capacity_ledger.get(&crate::palw_capacity_s567_v1::PalwCapacityLedgerKeyV1::RiderLead(*lead)) {
            Some(crate::palw_capacity_s567_v1::PalwCapacityLedgerRowV1::Riders { riders, daa }) => Some((*riders, *daa)),
            _ => None,
        }
    }

    /// **ADR-0164 F-K: the breaker's row**, if the breaker has counted or judged (absent: the top level, nothing counted).
    pub fn rho_breaker_v1(&self) -> Option<&crate::palw_capacity_s567_v1::PalwRhoBreakerV1> {
        match self.capacity_ledger.get(&crate::palw_capacity_s567_v1::PalwCapacityLedgerKeyV1::Breaker) {
            Some(crate::palw_capacity_s567_v1::PalwCapacityLedgerRowV1::Breaker(row)) => Some(row),
            _ => None,
        }
    }

    /// **ADR-0164 F-K: a bond's own breaker row**, if it holds one.
    pub fn bond_breaker_v1(&self, bond: &PalwBondKeyV2) -> Option<&crate::palw_capacity_s567_v1::PalwBondBreakerV1> {
        match self.capacity_ledger.get(&crate::palw_capacity_s567_v1::PalwCapacityLedgerKeyV1::BondBreaker(*bond)) {
            Some(crate::palw_capacity_s567_v1::PalwCapacityLedgerRowV1::Bond(row)) => Some(row),
            _ => None,
        }
    }

    /// The ledger itself (read-only; tests and the node's views).
    pub fn capacity_ledger_v1(
        &self,
    ) -> &BTreeMap<crate::palw_capacity_s567_v1::PalwCapacityLedgerKeyV1, crate::palw_capacity_s567_v1::PalwCapacityLedgerRowV1> {
        &self.capacity_ledger
    }

    /// **RFC-0002 Phase F: an IR class's record**, or `None` for a class that is not an IR program.
    pub fn tir_class_v1(&self, class_id: &Hash64) -> Option<&crate::palw_tir_admission_v1::PalwTirClassRecordV1> {
        self.tir_classes.get(class_id)
    }

    /// **G14: one staged public court row** by its key (read-only).
    pub fn staged_court_row_v1(
        &self,
        key: &crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1,
    ) -> Option<&crate::palw_staged_court_state_v1::PalwStagedCourtRowV1> {
        self.staged_courts.get(key)
    }

    /// **G14 lane court-f02: every native duty clock of `claim`** (RFC-0028 history read by the F02 attribution). A scan of the
    /// staged rows: duty clocks are keyed by duty id, not claim.
    /// G14 lane r5-f02-wire: read through the per-claim index (`staged_court_service_by_claim`, kind 1), O(clocks of this claim).
    pub fn court_duty_clocks_for_claim_v1(&self, claim: Hash64) -> Vec<&crate::palw_court_duty_clock_v1::CourtDutyClockV1> {
        let Some(rows) = self.staged_court_service_by_claim.get(&claim) else { return Vec::new() };
        rows.iter()
            .filter(|(kind, _, _)| *kind == crate::palw_staged_court_state_v1::COURT_INDEX_DUTY_CLOCK_V1)
            .filter_map(|(_, _, key)| match self.staged_courts.get(key) {
                Some(crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::CourtDutyClock(clock)) if clock.claim == claim => {
                    Some(clock.as_ref())
                }
                _ => None,
            })
            .collect()
    }

    /// The full-scan reading of [`Self::court_duty_clocks_for_claim_v1`] and [`Self::court_shared_initial_notice_daa_v1`] — the
    /// index's specification; tests assert the index agrees with it.
    pub fn court_service_claim_scan_v1(&self, claim: Hash64) -> (usize, Option<u64>) {
        use crate::palw_staged_court_state_v1::PalwStagedCourtRowV1 as R;
        let clocks = self.staged_courts.values().filter(|r| matches!(r, R::CourtDutyClock(c) if c.claim == claim)).count();
        let notice = self
            .staged_courts
            .values()
            .filter_map(|r| match r {
                R::CourtSharedQuery(q) if q.claim == claim => Some(q.opened_daa),
                _ => None,
            })
            .min();
        (clocks, notice)
    }

    /// **G14 lane court-f02 (RFC-0031): the DAA of `claim`'s initial shared notice** — the earliest native shared request row
    /// (`CourtSharedQuery`) recorded for it, if any. Rooted: the rows come and go with the staged-court delta on reorg.
    /// G14 lane r5-f02-wire: the index's first kind-0 entry is the earliest notice (entries sort by `(kind, daa, key)`).
    pub fn court_shared_initial_notice_daa_v1(&self, claim: Hash64) -> Option<u64> {
        self.staged_court_service_by_claim
            .get(&claim)?
            .iter()
            .find(|(kind, _, _)| *kind == crate::palw_staged_court_state_v1::COURT_INDEX_SHARED_NOTICE_V1)
            .map(|(_, daa, _)| *daa)
    }

    /// Ordered read of staged rows in `[from, to]` (RFC-0032 A material intake).
    pub fn staged_court_range_v1<'a>(
        &'a self,
        from: &crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1,
        to: &crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1,
    ) -> impl Iterator<Item = (&'a crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1, &'a crate::palw_staged_court_state_v1::PalwStagedCourtRowV1)> + 'a {
        self.staged_courts.range(*from..=*to)
    }

    /// Derived lifecycle consistency only, not proof of a new paid acquisition
    /// or raw-source grant. Admission remains disabled until the full native
    /// allocation and distinct custody pledge/grants are wired.
    pub(super) fn assert_tir_acquisition_indices_v1(&self) -> Result<(), PalwStateV2Error> {
        use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
        let mut owners = BTreeSet::new();
        let mut claims = BTreeMap::<Hash64, BTreeSet<(u64, Key)>>::new();
        let mut responders = BTreeSet::new();
        let mut preparers = BTreeSet::new();
        let mut producers = BTreeSet::new();
        let mut due = BTreeSet::new();
        let mut holds = BTreeSet::new();
        let now = self.last_point().map(|p| p.daa_score).unwrap_or(0);
        for (key, row) in &self.staged_courts {
            if matches!(key, Key::TirAcquisition(..) | Key::TirRawChunk(_) | Key::TirAcceptedBody(_) | Key::TirPreparation(..) | Key::TirClaim(_) | Key::TirWitness(..)) {
                let (at, row_holds) = palw_staged_court_fold_v1::index_rows(*key, row);
                due.insert((at, *key));
                holds.extend(row_holds);
            }
            if let Some((owner, claim, allocation, _)) = palw_staged_court_fold_v1::tir_index_metadata_v1(*key, row) {
                owners.insert((owner, claim, allocation));
                responders.extend(palw_staged_court_fold_v1::tir_response_index_rows_v1(*key, row));
            }
            preparers.extend(palw_staged_court_fold_v1::tir_preparation_index_rows_v1(*key, row));
            producers.extend(palw_staged_court_fold_v1::tir_claim_producer_index_v1(*key, row));
            if let Some((claim, until)) = palw_staged_court_fold_v1::tir_source_index_metadata_v1(*key, row) {
                claims.entry(claim).or_default().insert((until, *key));
                let retained_until = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(self, claim)
                    .map(|frame| frame.plan_v1().terms.material_until_daa)
                    .or_else(|| self.staged_claim_contract_v1(&claim).map(|contract| contract.terms.material_until_daa));
                if now < until && retained_until.is_none_or(|retained_until| retained_until < until) {
                    return Err(PalwStateV2Error::CarriageInconsistent(
                        "live TIR acquisition lost its original claim/public-source retention hold".into(),
                    ));
                }
            }
            let material = match row {
                Row::TirRawChunk(row) => Some((row.acquisition, row.source_id, row.material_until_daa)),
                Row::TirAcceptedBody(row) => Some((row.acquisition, row.source_id, row.material_until_daa)),
                _ => None,
            };
            if let Some((acquisition, source, until)) = material.filter(|(_, _, until)| now < *until) {
                let Some(Row::TirAcquisition(owner)) = self.staged_courts.get(&acquisition) else {
                    return Err(PalwStateV2Error::CarriageInconsistent("live TIR public material lost its acquisition scope".into()));
                };
                let c = owner.owned.contract_v1();
                if c.source_id != source || c.terms.material_until_daa != until {
                    return Err(PalwStateV2Error::CarriageInconsistent("TIR public material changed original source/TTL".into()));
                }
            }
        }
        let actual_due: BTreeSet<_> = self.staged_due.iter().filter(|(_, key)| {
            matches!(key, Key::TirAcquisition(..) | Key::TirRawChunk(_) | Key::TirAcceptedBody(_) | Key::TirPreparation(..) | Key::TirClaim(_) | Key::TirWitness(..))
        }).copied().collect();
        let actual_holds: BTreeSet<_> = self.staged_holds.iter().filter(|(_, _, key)| {
            matches!(key, Key::TirAcquisition(..) | Key::TirRawChunk(_) | Key::TirAcceptedBody(_) | Key::TirPreparation(..) | Key::TirClaim(_) | Key::TirWitness(..))
        }).copied().collect();
        if owners != self.staged_tir_by_owner || claims != self.staged_tir_by_claim || responders != self.staged_tir_responders
            || preparers != self.staged_tir_preparers
            || producers != self.staged_tir_claims_by_producer
            || due != actual_due || holds != actual_holds
        {
            return Err(PalwStateV2Error::CarriageInconsistent("TIR acquisition lifecycle indexes differ from rooted rows".into()));
        }
        Ok(())
    }

    /// **RFC-0002 F7: a court session's IR history dissection**, if its root claim was admitted.
    pub fn tir_dissection_v1(&self, session_id: &Hash64) -> Option<&crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1> {
        self.tir_dissections.get(session_id).map(|phase| phase.as_ref())
    }

    /// **RFC-0003: a generative class's record**, or `None` for a class that is not a pipeline.
    pub fn gen_class_v1(&self, class_id: &Hash64) -> Option<&crate::palw_gen_class_v1::PalwGenClassRecordV1> {
        self.gen_classes.get(class_id)
    }

    /// **Does the class commit its logits in Q24** (RFC-0001 §A.3's unit; RFC-0003 §I.3)? Every legacy
    /// class does: K = 24 integer kernels. An IR class (`tir_classes`) or a generative class
    /// (`gen_classes`) commits them in the unit its lowerer calibrated, which is declared nowhere —
    /// until the fence that makes Q24 a guarantee of the lowering, when a class registered past it
    /// answers `true` here (the registration height is in its class row). A class the registry does
    /// not hold is a legacy question for the transition to refuse by name, and answers `true`.
    pub fn class_commits_q24_logits_v1(&self, class_id: &Hash64) -> bool {
        !self.tir_classes.contains_key(class_id) && !self.gen_classes.contains_key(class_id)
    }

    /// **RFC-0001 stage 2b: may this class carry inherited prefix leaves** (FP job version 12)? See
    /// [`crate::palw_fp_prefix_v1::palw_fp_prefix_inherit_class_safe_v1`].
    pub fn class_prefix_inherit_safe_v1(&self, class_id: &Hash64) -> Result<(), &'static str> {
        crate::palw_fp_prefix_v1::palw_fp_prefix_inherit_class_safe_v1(
            self.fp_work_profile_of(class_id),
            self.class_is_held_v1(class_id),
            !self.class_commits_q24_logits_v1(class_id),
            self.class_model_court_window_v1(class_id).is_some(),
        )
    }

    /// **RFC-0001 §2.9: the tokenizer commitment a class's registry row lists** — an IR class's record's `tokenizer_id`, a
    /// generative class's row's — or `None` for a class whose row lists none (a dense legacy class: its tokenizer
    /// commitment is inside an artifact digest the chain cannot read). The one read `palw_fp_tokenizer_match` compares a
    /// job against.
    pub fn claim_semantics_v1(&self, claim_id: &Hash64) -> Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1> {
        self.claim_semantics.get(claim_id)
    }

    pub fn class_semantics_v1(&self, class_id: &Hash64) -> Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1> {
        self.class_semantics.get(class_id)
    }

    pub fn class_tokenizer_listing_v1(&self, class_id: &Hash64) -> Option<Hash64> {
        self.tir_classes
            .get(class_id)
            .map(|record| record.tokenizer_id)
            .or_else(|| self.gen_classes.get(class_id).map(|record| record.tokenizer_id))
            .or_else(|| self.class_semantics.get(class_id).map(|row| row.tokenizer_id))
    }

    /// **RFC-0003 §II.2.1: a V5 claim's class, resolved against the generative registry** — the row
    /// its job names, holding the job to it (`palw_fp_v5_resolve_class_v1`). `armed` is
    /// `Params::palw_fp_job_v5` in force at the judged height.
    pub fn fp_v5_class_v1(
        &self,
        job: &crate::palw_fp_job_v5::PalwFreePromptJobV5,
        armed: bool,
    ) -> Result<&crate::palw_gen_class_v1::PalwGenClassRecordV1, crate::palw_fp_job_v5::PalwFpV5Error> {
        crate::palw_fp_job_v5::palw_fp_v5_resolve_class_v1(job, self.gen_classes.get(&job.v4.class_id), armed)
    }

    /// A test's way to replace an open court session (its ladder walked to `Terminal` off-fold) — R6-CELL round 4's court-v4 fixture.
    #[cfg(test)]
    pub(crate) fn test_replace_court_session_v2(&mut self, session_id: Hash64, session: PalwCourtSessionStateV2) {
        self.court_sessions.insert(session_id, session);
    }

    /// A test's way to stand a `tir_classes` row in (the fold writes rows only through a registration).
    #[cfg(test)]
    pub(crate) fn test_insert_tir_class_v1(&mut self, class_id: Hash64, record: crate::palw_tir_admission_v1::PalwTirClassRecordV1) {
        self.tir_classes.insert(class_id, record);
    }

    /// **ADR-0160 F-S: `bond`'s issuance bucket**, if it has one (a bond with none holds a full bucket).
    pub fn issuance_bucket_of_v1(&self, bond: &PalwBondKeyV2) -> Option<&crate::palw_issuance_slots_v1::PalwIssuanceBucketV1> {
        self.issuance_buckets.get(bond)
    }

    /// **4-ter.3 step 6: every held forfeit**, by `(claim, session)`.
    pub fn held_forfeits_iter(&self) -> impl Iterator<Item = (&(Hash64, Hash64), &PalwHeldForfeitV1)> + '_ {
        self.held_forfeits.iter()
    }

    /// Row 14 (DA-2): every open data-availability session, by `(claim, accuser)`.
    pub fn da_sessions_iter(
        &self,
    ) -> impl Iterator<Item = (&(Hash64, PalwBondKeyV2), &crate::palw_da_rcore_v1::PalwDaSessionV1)> + '_ {
        self.da_sessions.iter()
    }

    /// **4-ter.3 step 6: the held forfeits kept for `claim_id`**, by session ([`PalwHeldForfeitV1`]).
    pub fn held_forfeits_of_claim(&self, claim_id: &Hash64) -> impl Iterator<Item = (&Hash64, &PalwHeldForfeitV1)> + '_ {
        let claim = *claim_id;
        self.held_forfeits
            .range((claim, ZERO_HASH64)..)
            .take_while(move |((of, _), _)| *of == claim)
            .map(|((_, session), r)| (session, r))
    }

    /// The one deadline `claim_id` holds in the sweep queue, if any (DL-1: none while a seat session
    /// pauses it). A scan of the index — a read for tests and RPCs, never a consensus path.
    pub fn deadline_of(&self, claim_id: &Hash64) -> Option<u64> {
        self.deadlines.iter().find(|(_, id)| id == claim_id).map(|(at, _)| *at)
    }

    /// The DA sessions by deadline, `(deadline_daa, claim, accuser)` — the sweep's order (DA-7).
    pub fn da_deadlines_iter(&self) -> impl Iterator<Item = &(u64, Hash64, PalwBondKeyV2)> + '_ {
        self.da_deadlines.iter()
    }

    /// **Does this state hold any R-core+ data?** The one predicate the root block and the
    /// carriage tail are both guarded by, so the two cannot disagree about when the block exists
    /// (the skeleton review's F6: both guards are [`palw_rcore_plus_has_data_v1`]).
    pub(super) fn has_rcore_plus_data(&self) -> bool {
        palw_rcore_plus_has_data_v1(PalwRcorePlusItemsV1 {
            withholding_strikes: &self.withholding_strikes,
            reward_pending: &self.reward_pending,
            reporter_commitments: &self.reporter_commitments,
            reporter_rewards: &self.reporter_rewards,
            reporter_counters: &self.reporter_counters,
            vesting: &self.vesting,
            vesting_counters: &self.vesting_counters,
            da_sessions: &self.da_sessions,
            da_claims: &self.da_claims,
        })
    }

    pub fn safe_frontier(&self) -> (u64, BlockHash) {
        (self.safe_frontier_blue_score, self.safe_frontier)
    }

    /// **RFC-0018, tests only**: set the state's point (the fold's last chain block), as a committed post-state carries it.
    #[cfg(test)]
    pub(crate) fn set_last_point_for_tests(&mut self, point: Option<PalwBlockContextV2>) {
        self.last_point = point;
    }

    /// **RFC-0018, tests only**: set or remove one bond row (the attack acceptance fixtures' registries).
    #[cfg(test)]
    pub(crate) fn set_bond_for_tests(&mut self, key: PalwBondKeyV2, bond: Option<PalwBondStateV2>) {
        match bond {
            Some(bond) => self.bonds.insert(key, bond),
            None => self.bonds.remove(&key),
        };
    }

    /// **RFC-0018, tests only**: write one kernel-route aux row (creating an interim-policy route when there is none).
    #[cfg(test)]
    pub(crate) fn set_kernel_aux_row_for_tests(&mut self, table: u8, key: Vec<u8>, bytes: Option<Vec<u8>>) {
        let route = self.kernel_route.get_or_insert_with(|| {
            crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new_typed(
                crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::default(), Hash64::default()),
                None,
                None,
                misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 0, burned: 0 },
            )
        });
        match bytes {
            Some(bytes) => {
                route.aux.insert((table, key), bytes);
            }
            None => {
                route.aux.remove(&(table, key));
            }
        }
    }

    /// **RFC-0018**: the kernel route, for the security certification's aux-table reads (`crate::palw_security_certification_v1`).
    pub fn kernel_route_state_v1(&self) -> Option<&crate::palw_kernel_route_v1::PalwKernelRouteStateV1> {
        self.kernel_route.as_ref()
    }

    /// **RFC-0018 §7.7**: the accepted blue score of the oldest unresolved claim (`None`: nothing unresolved) — the pruning-progress
    /// rule's first bound.
    /// The open claims in acceptance order (`(accepted blue score, claim)`), for RFC-0023 stage 1's resolved prefix.
    pub fn unresolved_iter(&self) -> impl Iterator<Item = (u64, &Hash64)> + '_ {
        self.unresolved.iter().map(|(blue_score, id)| (*blue_score, id))
    }
    pub fn unresolved_first_blue_score_v1(&self) -> Option<u64> {
        self.unresolved.iter().next().map(|(blue_score, _)| *blue_score)
    }

    pub fn last_point(&self) -> Option<&PalwBlockContextV2> {
        self.last_point.as_ref()
    }

    /// This state's standing in fork choice, through the one comparator (Decision 9).
    /// `live_total` is constructed by [`PalwCandidateOrderV1::new`], never stored here, so a
    /// maturing claim cannot lower it.
    pub fn candidate_order(&self, candidate: Hash64) -> PalwCandidateOrderV1 {
        PalwCandidateOrderV1::new(self.safe_frontier_blue_score, self.safe_weight, self.bounded_immature, candidate)
    }

    // ---- the root ----

    /// The state root: version, then every collection root in the struct's declared order, then
    /// the scalars. The exact ordering is frozen in ADR-0043 §2 **as amended 2026-08-22 (M-02)**,
    /// which lists the version-5 preimage in full — the ADR text and this body are held equal by
    /// `the_state_root_preimage_is_exactly_the_adr_0043_list` (a from-spec second
    /// implementation), and every field's membership is enforced by
    /// `every_primary_datum_moves_the_root`. Changing this body — or what any collection's entry
    /// encoding covers — is a consensus change and needs a new version constant, a matching
    /// ADR-0043 amendment, and new golden vectors; the tests refuse anything less.
    pub fn state_root(&self) -> Hash64 {
        // **One spelling: the root is the keyed hash of the preimage, and the preimage is what `state_root_preimage` writes.** (RFC-0009 stage D:
        // a light client is shown the preimage — every collection's ROOT, never its rows — to prove one collection's root belongs to a header's
        // committed state root, then opens only that collection. Splitting the function in two changed no byte: BLAKE2b streaming is hashing the
        // concatenation, and the golden vectors hold it.)
        palw_state_root_of_preimage_v1(&self.state_root_preimage())
    }

    /// The exact byte string [`Self::state_root`] hashes, in the order ADR-0043 §2 freezes. Collections appear as their 64-byte
    /// [`collection_root`]; scalars and the small borsh blocks appear in full.
    pub fn state_root_preimage(&self) -> Vec<u8> {
        self.state_root_preimage_located_v1().0
    }

    /// [`Self::state_root_preimage`] (byte for byte: it IS that function's body) and the offset at which the writer itself began the
    /// Some-only `kernel-route/v1` block — `None` when the state has no kernel route. The serving side of op 242 carries this offset
    /// (`kernel_route_offset`); a client never trusts it, but compares it with what
    /// [`Self::kernel_route_offset_of_preimage_v1`] derives from the authenticated preimage alone.
    pub fn state_root_preimage_located_v1(&self) -> (Vec<u8>, Option<usize>) {
        let mut kernel_route_offset = None;
        let mut state = RootPreimageV1::default();
        state.update(&PALW_STATE_V2_VERSION.to_le_bytes());
        state.update(collection_root(b"bonds", &self.bonds).as_byte_slice());
        state.update(collection_root(b"reserved_exposure", &self.reserved_exposure).as_byte_slice());
        state.update(collection_root(b"classes", &self.classes).as_byte_slice());
        state.update(collection_root(b"class_targets", &self.class_targets).as_byte_slice());
        state.update(collection_root(b"class_shares", &self.class_shares).as_byte_slice());
        // ADR-0056 Decisions 3 and 5. Rooted here, after the share table they are the economics of.
        state.update(collection_root(b"registration_exposure", &self.registration_exposure).as_byte_slice());
        state.update(collection_root(b"class_walks", &self.class_walks).as_byte_slice());
        match &self.epoch_budgets {
            None => {
                state.update(&[0u8]);
            }
            Some(budgets) => {
                state.update(&[1u8]);
                state.update(&borsh::to_vec(budgets).expect("PalwEpochBudgetsV2 is borsh-serializable"));
            }
        }
        state.update(collection_root(b"receipt_targets", &self.receipt_targets).as_byte_slice());
        state.update(collection_root(b"capabilities", &self.capabilities).as_byte_slice());
        state.update(collection_root(b"claims", &self.claims).as_byte_slice());
        state.update(collection_root(b"pending_payouts", &self.pending_payouts).as_byte_slice());
        state.update(collection_root(b"certified_families", &self.certified_families).as_byte_slice());
        state.update(collection_root(b"fp_certified_families", &self.fp_certified_families).as_byte_slice());
        state.update(collection_root(b"fp_certified_classes", &self.fp_certified_classes).as_byte_slice());
        state.update(collection_root(b"pending_chunks", &self.pending_chunks).as_byte_slice());
        // ADR-0080 design A, rooted immediately after the chunk table it deliberately is not.
        state.update(collection_root(b"court_close_groups", &self.court_close_groups).as_byte_slice());
        // ADR-0078 Decision 4, rooted after the certification tables it is the layer above.
        state.update(collection_root(b"derived_artifacts", &self.derived_artifacts).as_byte_slice());
        state.update(collection_root(b"panels", &self.panels).as_byte_slice());
        state.update(collection_root(b"court_sessions", &self.court_sessions).as_byte_slice());
        state.update(collection_root(b"epoch_counters", &self.epoch_counters).as_byte_slice());
        state.update(collection_root(b"receipt_epoch_counters", &self.receipt_epoch_counters).as_byte_slice());
        // ADR-0087 M7: the market collections enter the root only once a move has been folded,
        // which cannot happen below `Params::palw_model_market`; before that the root is the one a
        // build without the fields computes, header for header.
        if !self.model_markets.is_empty() || !self.model_positions.is_empty() {
            state.update(collection_root(b"model_markets", &self.model_markets).as_byte_slice());
            state.update(collection_root(b"model_positions", &self.model_positions).as_byte_slice());
        }
        // ADR-0088 Decision 10: the registry's collections enter the root only once a line row,
        // a version, a proposal, an evaluation or a claim root has been written, which cannot
        // happen below `Params::palw_model_lines`.
        if !self.model_lines.is_empty()
            || !self.model_versions.is_empty()
            || !self.model_proposals.is_empty()
            || !self.model_evaluations.is_empty()
            || !self.claim_roots.is_empty()
        {
            state.update(collection_root(b"model_lines", &self.model_lines).as_byte_slice());
            state.update(collection_root(b"model_versions", &self.model_versions).as_byte_slice());
            state.update(collection_root(b"model_proposals", &self.model_proposals).as_byte_slice());
            state.update(collection_root(b"model_evaluations", &self.model_evaluations).as_byte_slice());
            state.update(collection_root(b"claim_roots", &self.claim_roots).as_byte_slice());
        }
        // **ADR-0095 §4.11 as corrected.** Its own block, NOT folded into the registry's above:
        // on a chain where the registry is already live, adding these to that block would move the
        // root the moment the code shipped. Empty until something is declared, and nothing can be
        // declared below `Params::palw_model_benefits`.
        if !self.model_benefits.is_empty() || !self.model_position_since.is_empty() {
            state.update(collection_root(b"model_benefits", &self.model_benefits).as_byte_slice());
            state.update(collection_root(b"model_position_since", &self.model_position_since).as_byte_slice());
        }
        // ADR-0089 Decision 6: the settlement list is in the root while it waits for the child,
        // and only then.
        if !self.evm_settlements.is_empty() {
            state.update(collection_root(b"evm_settlements", &self.evm_settlements).as_byte_slice());
        }
        // **ADR-0100 Decision 4.** Its own block, for ADR-0095's reason: on a chain where the
        // blocks above are live, joining one of them would move the root the moment the code
        // shipped. Empty until a plan is declared, and nothing can be declared below
        // `Params::palw_shard_licensing`.
        if !self.class_shard_plans.is_empty() || !self.bond_shards.is_empty() || !self.shard_licensing.is_empty() {
            state.update(collection_root(b"class_shard_plans", &self.class_shard_plans).as_byte_slice());
            state.update(collection_root(b"bond_shards", &self.bond_shards).as_byte_slice());
            state.update(collection_root(b"shard_licensing", &self.shard_licensing).as_byte_slice());
        }
        // **ADR-0103 Decision 4.** Its own block, for ADR-0095's reason: empty until a held
        // accusation is accepted, which nothing below `Params::palw_held_context` can do.
        if !self.held_da_missing.is_empty() {
            state.update(collection_root(b"held_da_missing", &self.held_da_missing).as_byte_slice());
        }
        // **ADR-0111 Decision 3.** Its own block, for the same reason: empty until a seat demands a
        // leaf's evidence, which nothing below `Params::palw_held_context` can do.
        if !self.held_leaf_demands.is_empty() {
            state.update(collection_root(b"held_leaf_demands", &self.held_leaf_demands).as_byte_slice());
        }
        // **ADR-0119 Decision 2.** Its own block, for the same reason: empty until a held class
        // registers, which nothing below `Params::palw_held_context` can do.
        if !self.class_step_ladders.is_empty() {
            state.update(collection_root(b"class_step_ladders", &self.class_step_ladders).as_byte_slice());
        }
        if !self.class_court_windows.is_empty() {
            state.update(collection_root(b"class_court_windows", &self.class_court_windows).as_byte_slice());
        }
        // **RFC-0006.** Its own block, for ADR-0095's reason: empty until a plan is declared, and nothing can be declared
        // below `Params::palw_tir_shard_v1`, so a network that never armed it roots as before.
        if !self.tir_shard_plans.is_empty() || !self.tir_shard_claims.is_empty() {
            state.update(b"tir_shard/v1");
            state.update(collection_root(b"tir_shard_plans", &self.tir_shard_plans).as_byte_slice());
            state.update(collection_root(b"tir_shard_claims", &self.tir_shard_claims).as_byte_slice());
        }
        // **ADR-0165.** Its own block, for the same reason: `None` until a REAL attempt is accepted past
        // `Params::palw_floor_reserve_v1`, which nothing below the fence can do.
        if let Some(floor) = &self.floor_state {
            state.update(b"floor_state_v1");
            state.update(&borsh::to_vec(floor).expect("a floor state is borsh-serializable"));
        }
        if !self.seat_availability.is_empty() {
            state.update(collection_root(b"seat_availability", &self.seat_availability).as_byte_slice());
        }
        if !self.seat_root_readiness.is_empty() {
            state.update(collection_root(b"seat_root_readiness", &self.seat_root_readiness).as_byte_slice());
        }
        // **G14 lane D.** Its own block, Some-only: `None` until the first block at or past the fence nobody can arm.
        if let Some(kernel) = &self.kernel_route {
            kernel_route_offset = Some(state.0.len());
            state.update(b"kernel-route/v1");
            state.update(kernel.ledger_root().as_byte_slice());
            state.update(kernel.aux_root().as_byte_slice());
        }
        // **Lane LG14-A.** Its own block, Some-only: empty until the first reservation past `palw_legacy_public_filer_v1`, which no
        // network can arm, so a dormant network roots exactly as before.
        if !self.staged_courts.is_empty() {
            state.update(b"staged_public_courts/v1");
            state.update(collection_root(b"staged_public_courts", &self.staged_courts).as_byte_slice());
        }
        if !self.terminal_evidence.is_empty() {
            state.update(b"terminal_evidence/v1");
            state.update(collection_root(b"terminal_evidence", &self.terminal_evidence).as_byte_slice());
        }
        if !self.legacy_answers.is_empty() {
            state.update(b"legacy_answers/v2");
            state.update(collection_root(b"legacy_answers", &self.legacy_answers).as_byte_slice());
        }
        if !self.fp_public_jobs.is_empty() {
            state.update(b"public_fp_jobs/v1");
            state.update(collection_root(b"public_fp_jobs", &self.fp_public_jobs).as_byte_slice());
        }
        if !self.class_semantics.is_empty() {
            state.update(b"class_semantics/v1");
            state.update(collection_root(b"class_semantics", &self.class_semantics).as_byte_slice());
            state.update(collection_root(b"claim_semantics", &self.claim_semantics).as_byte_slice());
        }
        if !self.legacy_disputes.is_empty() {
            state.update(b"legacy_disputes/v1");
            state.update(collection_root(b"legacy_disputes", &self.legacy_disputes).as_byte_slice());
        }
        // **ADR-0191 step B.** Its own block: empty until a designated event is folded past its fence, which nothing below it can do.
        if !self.tir_designated.records.is_empty() {
            state.update(b"tir_designated/v1");
            state.update(collection_root(b"tir_designated", &self.tir_designated.records).as_byte_slice());
        }
        // **G14 lane escrow-rfc08 (RFC-0035 A).** Its own block: empty until a funded original installs a row, which nothing can yet.
        if !self.funded_escrows.is_empty() {
            state.update(b"funded_escrow/v1");
            state.update(collection_root(b"funded_escrow", &self.funded_escrows).as_byte_slice());
        }
        // **G14 item 6 (ADR-0200 C1).** Its own block: empty until a root is recorded with an adopted draw cap, which nothing can yet.
        if !self.d2_readiness.is_empty() {
            state.update(b"d2_readiness/v1");
            state.update(collection_root(b"d2_readiness", &self.d2_readiness).as_byte_slice());
        }
        // **ADR-0124 Decisions 2 and 3.** Its own block, for the same reason: empty until a panel
        // is bound past `Params::palw_panel_economy`, and the reserve is zero until a pool leaves a
        // share unpaid, which only such a panel can do. Below the fence the root is the one a
        // build without the fields computes, header for header.
        if !self.panel_duties.is_empty() || self.panel_reserve_sompi != 0 {
            state.update(collection_root(b"panel_duties", &self.panel_duties).as_byte_slice());
            state.update(&self.panel_reserve_sompi.to_le_bytes());
        }
        // **ADR-0125.** Its own block, for the same reason: empty until an attempt is finalized past
        // `Params::palw_execution_lane`, which nothing below the fence can do.
        if self.round_lane_is_written() {
            state.update(b"round_lane/v1");
            state.update(&self.round_span.to_le_bytes());
            state.update(collection_root(b"round_finals", &self.round_finals).as_byte_slice());
            state.update(collection_root(b"round_schedules", &self.round_schedules).as_byte_slice());
            state.update(collection_root(b"round_permits_used", &self.round_permits_used).as_byte_slice());
            // §7.3: only once a permit was burned, so a lane without an equivocation roots as before.
            if !self.round_equivocations.is_empty() {
                state.update(collection_root(b"round_equivocations", &self.round_equivocations).as_byte_slice());
            }
            // ADR-0130: the snapshot waiting for its seed and the seed anchor, named, once either exists.
            if self.round_scheduler_is_written() {
                state.update(b"round_scheduler/v2");
                state.update(collection_root(b"round_pending", &self.round_pending).as_byte_slice());
                match &self.round_seed_anchor {
                    None => state.update(&[0u8]),
                    Some(anchor) => {
                        state.update(&[1u8]);
                        state.update(&borsh::to_vec(anchor).expect("PalwExecSeedAnchorV1 is borsh-serializable"))
                    }
                };
            }
        }
        // ADR-0135: the registry's rows, named, once either exists — a state without them is
        // byte-identical to a state before the field.
        if self.model_registry_is_written() {
            state.update(b"model_registry/v1");
            state.update(collection_root(b"model_lifecycles", &self.model_lifecycles).as_byte_slice());
            state.update(collection_root(b"seat_readiness", &self.seat_readiness).as_byte_slice());
        }
        // ADR-0132 Upgrade C: the claims' economics rows, named, once any exists.
        if !self.claim_economics.is_empty() {
            state.update(b"claim_economics/v1");
            state.update(collection_root(b"claim_economics", &self.claim_economics).as_byte_slice());
        }
        // ADR-0137 / ADR-0132 S: the work target, named, once it exists (past its fence).
        if let Some(target) = &self.work_target {
            state.update(b"work_target/v1");
            state.update(&borsh::to_vec(target).expect("PalwWorkTargetV2 is borsh-serializable"));
        }
        // ADR-0143: the artifact-root owners, named, once the index exists (past its fence). Empty
        // hashes as nothing, so a chain with the fence dormant commits the root it always did.
        if !self.artifact_owners.is_empty() {
            state.update(b"artifact_owners/v1");
            state.update(collection_root(b"artifact_owners", &self.artifact_owners).as_byte_slice());
        }
        // ADR-0145 §5/§6: the class graphs the derivation reads and the live claims' prompts the
        // cache rule reads, named, once they exist (past `Params::palw_fp_derived_work`). Empty
        // hashes as nothing, so a chain with the fence dormant commits the root it always did —
        // which is the property `a_dormant_derived_work_fence_moves_no_state_root` asserts.
        if !self.fp_work_profiles.is_empty() {
            state.update(b"fp_work_profiles/v1");
            state.update(collection_root(b"fp_work_profiles", &self.fp_work_profiles).as_byte_slice());
        }
        if !self.fp_claim_prompts.is_empty() {
            state.update(b"fp_claim_prompts/v1");
            state.update(collection_root(b"fp_claim_prompts", &self.fp_claim_prompts).as_byte_slice());
        }
        // ADR-0144 §9: consumed offences, live locks and post-Final liability. Empty hashes as
        // nothing, so a chain with the fence dormant commits the root it always did.
        if !self.consumed_offences.is_empty() {
            state.update(b"consumed_offences/v1");
            state.update(collection_root(b"consumed_offences", &self.consumed_offences).as_byte_slice());
        }
        if !self.slashable_locks.is_empty() {
            state.update(b"slashable_locks/v1");
            state.update(collection_root(b"slashable_locks", &self.slashable_locks).as_byte_slice());
        }
        if !self.panel_liabilities.is_empty() {
            state.update(b"panel_liabilities/v1");
            state.update(collection_root(b"panel_liabilities", &self.panel_liabilities).as_byte_slice());
        }
        // **ADR-0152 v3.1 R-core+ (the v22 skeleton): ONE block, after the last collection block,
        // hashed only when any of its items is non-empty or non-zero** — so a state holding none of
        // it (every state on every network until the writers land) roots exactly as the version
        // bump alone gives. Once present, every item is hashed, empty ones included, in landing
        // order: S's five, then the vesting map and counters; M3 appends its two here.
        if self.has_rcore_plus_data() {
            state.update(b"rcore_plus/v1");
            state.update(collection_root(b"withholding_strikes", &self.withholding_strikes).as_byte_slice());
            state.update(collection_root(b"reward_pending", &self.reward_pending).as_byte_slice());
            state.update(collection_root(b"reporter_commitments", &self.reporter_commitments).as_byte_slice());
            state.update(collection_root(b"reporter_rewards", &self.reporter_rewards).as_byte_slice());
            state.update(&borsh::to_vec(&self.reporter_counters).expect("PalwReporterCountersV1 is borsh-serializable"));
            state.update(collection_root(b"vesting", &self.vesting).as_byte_slice());
            state.update(&borsh::to_vec(&self.vesting_counters).expect("PalwVestingCountersV1 is borsh-serializable"));
            // M3 (DA-2, §6 rows 14–15), appended in landing order.
            state.update(collection_root(b"da_sessions", &self.da_sessions).as_byte_slice());
            state.update(collection_root(b"da_claims", &self.da_claims).as_byte_slice());
        }
        // **ADR-0152-adjacent: the Activation Pool, ONE Some-only block after R-core+'s** — hashed only
        // when a row exists or a counter is non-zero, so every state without a pool (every network
        // but testnet-12, and testnet-12's genesis) roots exactly as before; once present, both items.
        if self.has_activation_pool_data() {
            state.update(b"activation_pool/v1");
            state.update(collection_root(b"activation_pools", &self.activation_pools).as_byte_slice());
            state.update(&borsh::to_vec(&self.activation_pool_counters).expect("PalwActivationPoolCountersV1 is borsh-serializable"));
            state.update(collection_root(b"activation_pool_scheduled", &self.activation_pool_scheduled).as_byte_slice());
            state.update(collection_root(b"activation_readiness_landed", &self.activation_readiness_landed).as_byte_slice());
        }
        // **ADR-0152 §4-ter.3 step 6: the held forfeits, ONE Some-only block** (after the Activation
        // Pool's `activation_pool/v1` where both branches meet) — hashed only when a record exists,
        // so every state without one roots exactly as before.
        if !self.held_forfeits.is_empty() {
            state.update(b"held_forfeits/v1");
            state.update(collection_root(b"held_forfeits", &self.held_forfeits).as_byte_slice());
        }
        // **The 2026-09-25 sweep's V03(2)/V05: the probation memory, ONE Some-only block** (after the
        // held forfeits') — hashed only when a row exists, so every state without one roots exactly as
        // before.
        if !self.probation_memory.is_empty() {
            state.update(b"registry_probation/v1");
            state.update(collection_root(b"probation_memory", &self.probation_memory).as_byte_slice());
        }
        state.update(&self.safe_weight.to_le_bytes());
        state.update(&self.retired_safe_weight.to_le_bytes());
        state.update(&self.settled_attempt_finals.to_le_bytes());
        // Empty below `palw_audit_2026_09_23`, and then absent, so a dormant network's root is
        // byte-identical to one computed before the ring existed.
        if !self.recent_anchor_daas.is_empty() {
            state.update(b"recent_anchor_daas/v1");
            state.update(&(self.recent_anchor_daas.len() as u64).to_le_bytes());
            for daa in &self.recent_anchor_daas {
                state.update(&daa.to_le_bytes());
            }
        }
        // **ADR-0160 lane liab (F-L): the bond freezes, ONE Some-only block** after the anchor ring's —
        // hashed only when a freeze exists, so every state without one (every state below the fence)
        // roots exactly as before.
        if !self.bond_freezes.is_empty() {
            state.update(b"bond_freezes/v1");
            state.update(collection_root(b"bond_freezes", &self.bond_freezes).as_byte_slice());
        }
        // **ADR-0160 stage 2 (F-Q, F-S): the receipts, the exclusions and the buckets, ONE Some-only
        // block** after the freezes' — hashed only when one of them holds a row, so every state below
        // both fences roots exactly as before.
        if !self.audit_receipts.is_empty() || !self.excluded_auditors.is_empty() || !self.issuance_buckets.is_empty() {
            state.update(b"capacity_qs/v1");
            state.update(collection_root(b"audit_receipts", &self.audit_receipts).as_byte_slice());
            state.update(collection_root(b"excluded_auditors", &self.excluded_auditors).as_byte_slice());
            state.update(collection_root(b"issuance_buckets", &self.issuance_buckets).as_byte_slice());
        }
        // **ADR-0160 stage 4 (F-N): the registered demand and the operator ring, ONE Some-only block.**
        if !self.network_demand.is_empty() || !self.operator_ring.is_empty() {
            state.update(b"capacity_n/v1");
            state.update(collection_root(b"network_demand", &self.network_demand).as_byte_slice());
            state.update(collection_root(b"operator_ring", &self.operator_ring).as_byte_slice());
        }
        // **ADR-0164 (F-EM, F-M1, F-K): the capacity ledger, ONE Some-only block** — hashed only when it holds a row, so every state
        // below the three fences roots exactly as before.
        if !self.capacity_ledger.is_empty() {
            state.update(b"capacity_s567/v1");
            state.update(collection_root(b"capacity_ledger", &self.capacity_ledger).as_byte_slice());
        }
        // **RFC-0002 Phase F (F6): the IR classes, ONE Some-only block** — empty until an IR class
        // registers, which nothing below `palw_tir_v1` can do.
        if !self.tir_classes.is_empty() {
            state.update(b"tir_classes/v1");
            state.update(palw_tir_classes_root_v1(&self.tir_classes).as_byte_slice());
        }
        // **RFC-0002 Phase F (F7): the open IR dissections, ONE Some-only block** — empty until an IR
        // class's root claim is admitted.
        if !self.tir_dissections.is_empty() {
            state.update(b"tir_dissections/v1");
            state.update(collection_root(b"tir_dissections", &self.tir_dissections).as_byte_slice());
        }
        // **RFC-0003: the generative classes, ONE Some-only block** — empty until a generative class
        // registers, which nothing below `palw_gen_v1` can do.
        if !self.gen_classes.is_empty() {
            state.update(b"gen_classes/v1");
            state.update(palw_gen_classes_root_v1(&self.gen_classes).as_byte_slice());
        }
        // **RFC-0004: the governed lines and their epochs, ONE Some-only block** — empty until a line
        // opts in, which nothing below `palw_improvement_v1` can do.
        if self.has_improvement_rows_v1() {
            state.update(b"improvement/v1");
            state.update(collection_root(b"improvement_lines", &self.improvement_lines).as_byte_slice());
            state.update(collection_root(b"improvement_epochs", &self.improvement_epochs).as_byte_slice());
            state.update(collection_root(b"improvement_policies", &self.improvement_policies).as_byte_slice());
            state.update(collection_root(b"improvement_usage", &self.improvement_usage).as_byte_slice());
            state.update(collection_root(b"improvement_heads", &self.improvement_heads).as_byte_slice());
            state.update(collection_root(b"improvement_pools", &self.improvement_pools).as_byte_slice());
            state.update(collection_root(b"improvement_material", &self.improvement_material).as_byte_slice());
            state.update(collection_root(b"improvement_candidates", &self.improvement_candidates).as_byte_slice());
            state.update(collection_root(b"improvement_pool_entries", &self.improvement_pool_entries).as_byte_slice());
            state.update(collection_root(b"improvement_items", &self.improvement_items).as_byte_slice());
            state.update(collection_root(b"improvement_results", &self.improvement_results).as_byte_slice());
            state.update(collection_root(b"improvement_grants", &self.improvement_grants).as_byte_slice());
            state.update(collection_root(b"improvement_earnings", &self.improvement_earnings).as_byte_slice());
        }
        // **RFC-0004 A4/A5: the material and candidates lane's tables, ONE Some-only block** after
        // `improvement/v1` (spec 17 §17.0) — empty until a governed line takes material, a licence
        // registers or a composite candidate is admitted, which nothing below `palw_improvement_v1` can do.
        if self.has_improvement_material_rows_v1() {
            state.update(b"improvement-material/v1");
            state.update(collection_root(b"improvement_cases", &self.improvement_cases).as_byte_slice());
            state.update(collection_root(b"improvement_opt_ins", &self.improvement_opt_ins).as_byte_slice());
            state.update(collection_root(b"improvement_setter_sets", &self.improvement_setter_sets).as_byte_slice());
            state.update(collection_root(b"improvement_datasets", &self.improvement_datasets).as_byte_slice());
            state.update(collection_root(b"improvement_artifacts", &self.improvement_artifacts).as_byte_slice());
            state.update(collection_root(b"improvement_licences", &self.improvement_licences).as_byte_slice());
            state.update(collection_root(b"improvement_composite_classes", &self.improvement_composite_classes).as_byte_slice());
        }
        // **RFC-0004 A6: the evaluation jobs, ONE Some-only block** (after the material lane's
        // `improvement-material/v1`) — empty until a job's first claim, which nothing below
        // `palw_improvement_v1` can make.
        if !self.improvement_eval_jobs.is_empty() {
            state.update(b"improvement-eval/v1");
            state.update(collection_root(b"improvement_eval_jobs", &self.improvement_eval_jobs).as_byte_slice());
        }
        // **RFC-0007 Part I: the verification vertex's tables, ONE Some-only block** after `improvement-eval/v1` — empty until
        // a vertex is accepted, which nothing below `palw_verification_vertex_v1` can do.
        if !self.vertex.is_empty() {
            state.update(b"vertex/v1");
            state.update(collection_root(b"vertex_rounds", &self.vertex.rounds).as_byte_slice());
            state.update(collection_root(b"vertex_tallies", &self.vertex.tallies).as_byte_slice());
            state.update(collection_root(b"vertex_held", &self.vertex.held).as_byte_slice());
        }
        // **RFC-0007 Parts II and IV: the mesh's tables, ONE more Some-only block** — empty until a fence's first write.
        if !self.vertex.mesh.is_empty() {
            state.update(b"mesh/v1");
            state.update(collection_root(b"mesh_witness", &self.vertex.mesh.witness).as_byte_slice());
            state.update(collection_root(b"mesh_audits", &self.vertex.mesh.audits).as_byte_slice());
            state.update(collection_root(b"mesh_traps", &self.vertex.mesh.traps).as_byte_slice());
            state.update(collection_root(b"mesh_capped", &self.vertex.mesh.capped).as_byte_slice());
        }
        // **RFC-0010: the permissionless Panel's engine, ONE Some-only block** after the mesh's — the engine exists only from the
        // first block at or past `palw_permissionless_panel_v1`, which no network can arm today, so a network that never arms it
        // roots as before. The engine commits its own state (policy, cursor, claims, retained work identities, certified outputs,
        // reservations, frozen epoch source sets) under one root.
        if let Some(panel) = &self.panel_v3 {
            state.update(b"panel_v3/v1");
            state.update(panel.root().as_byte_slice());
        }
        // **Lane BUDGET (ADR-0176 / ADR-0177): the bond budget, ONE Some-only block** after the permissionless Panel's — the engine
        // exists only from the first block at or past `palw_bond_budget_v1`, which no network can arm, so a network that never arms it
        // roots as before. The header's digest, then its five tables' roots.
        if let Some(budget) = &self.bond_budget {
            state.update(b"bond_budget/v1");
            let (header, tables) = budget.root_parts();
            state.update(header.as_byte_slice());
            for table in tables {
                state.update(table.as_byte_slice());
            }
        }
        // **RFC-0008 v2: the work-slice ledgers, ONE Some-only block** — empty until a root is declared, which nothing below
        // `palw_exec_payload_v2` can do.
        if !self.exec_v2.is_empty() {
            state.update(b"exec_v2/v1");
            state.update(collection_root(b"exec_v2_roots", &self.exec_v2.roots).as_byte_slice());
            state.update(collection_root(b"exec_v2_slices", &self.exec_v2.slices).as_byte_slice());
            state.update(collection_root(b"exec_v2_jobs", &self.exec_v2.jobs).as_byte_slice());
            state.update(collection_root(b"exec_v2_anchored", &self.exec_v2.anchored).as_byte_slice());
        }
        if !self.exec_v2_rewards.is_empty() {
            state.update(b"exec_v2_reward_vesting/v1");
            state.update(collection_root(b"exec_v2_reward_vesting", &self.exec_v2_rewards).as_byte_slice());
        }
        if !self.exec_v2_reward_holds.is_empty() {
            state.update(b"exec_v2_reward_holds/v1");
            state.update(collection_root(b"exec_v2_reward_holds", &self.exec_v2_reward_holds).as_byte_slice());
        }
        if !self.exec_v2_origins.is_empty() {
            state.update(b"exec_v2_origins/v1");
            state.update(collection_root(b"exec_v2_origins", &self.exec_v2_origins).as_byte_slice());
        }
        if !self.exec_v2_job_uses.is_empty() {
            state.update(b"exec_v2_job_uses/v1");
            state.update(collection_root(b"exec_v2_job_uses", &self.exec_v2_job_uses).as_byte_slice());
        }
        state.update(&self.bounded_immature.to_le_bytes());
        state.update(&self.safe_frontier_blue_score.to_le_bytes());
        state.update(self.safe_frontier.as_byte_slice());
        match &self.last_point {
            None => {
                state.update(&[0u8]);
            }
            Some(p) => {
                state.update(&[1u8]);
                state.update(&borsh::to_vec(p).expect("PalwBlockContextV2 is borsh-serializable"));
            }
        }
        (state.0, kernel_route_offset)
    }

    /// **Where the `kernel-route/v1` block begins in an AUTHENTICATED state-root preimage — derived from the preimage alone, by walking
    /// the fixed grammar [`Self::state_root_preimage_located_v1`] writes before that block.** `Ok(None)`: the grammar reaches no
    /// kernel-route block (the state has none). `Err`: the bytes are not a preimage of this build's layout (another version, a
    /// truncated or malformed prefix) — the caller refuses, it never falls back to searching.
    ///
    /// **Why a walk and not a search.** Searching the preimage for the label is ambiguous: the label could recur, and when the state has
    /// no kernel route a search would accept ANY later occurrence. The walk only ever looks where the writer can begin a block:
    ///
    /// 1. the version (2 bytes, this build's [`PALW_STATE_V2_VERSION`] only), seven collection roots, the ADR-0045 epoch-budget block
    ///    (tag `0`, or tag `1` and ONE typed borsh `PalwEpochBudgetsV2`, whose decoding is self-delimiting), fourteen collection roots;
    /// 2. at most 17 UNLABELED roots (ADR-0087 markets 2, ADR-0088 registry 5, ADR-0095 benefits 2, ADR-0089 settlements 1, ADR-0100
    ///    shards 3, ADR-0103 1, ADR-0111 1, ADR-0119 ladders 1 and windows 1), then optionally `tir_shard/v1` + two roots, then
    ///    optionally `floor_state_v1` + ONE typed borsh `PalwFloorStateV1`, then at most two unlabeled roots (lane PL-E seat
    ///    availability, seat root readiness), then `kernel-route/v1` + the ledger root + the aux root.
    ///
    /// Every position the walk inspects is therefore a position where an honest writer starts a block or a root. The result is a pure
    /// function of the preimage, and the preimage is authenticated by the header's committed root before this is called, so **a
    /// serving node cannot steer which window is chosen**: it can neither supply nor alter the bytes, and the offset it carries is only
    /// compared with this one. The walk is wrong only if a 64-byte keyed BLAKE2b collection root (a hash of chain state, not a value
    /// any party chooses) happens to BEGIN with one of the three labels — a ≥96-bit prefix match no state can be ground into. When the
    /// state has no kernel route, the inspected positions fall inside later blocks, which hold fixed labels (none contains this label),
    /// collection roots and consensus-computed integers, never free bytes; and a monetary consumer additionally requires the block.
    ///
    /// A new Some-only block inserted BEFORE `kernel-route/v1` must extend this walk in the same change (the
    /// `kernel_route_locator_*` tests compare it with the writer's own offset over every optional block before it).
    pub fn kernel_route_offset_of_preimage_v1(preimage: &[u8]) -> Result<Option<usize>, String> {
        const ROOT: usize = 64;
        const KERNEL: &[u8] = b"kernel-route/v1";
        const TIR_SHARD: &[u8] = b"tir_shard/v1";
        const FLOOR: &[u8] = b"floor_state_v1";
        const UNLABELED_BEFORE_TIR_SHARD: usize = 17;
        const UNLABELED_BEFORE_KERNEL: usize = 2;
        let fits = |at: usize, n: usize| at.checked_add(n).is_some_and(|end| end <= preimage.len());
        let starts = |at: usize, label: &[u8]| preimage.get(at..).is_some_and(|rest| rest.starts_with(label));
        if preimage.get(..2) != Some(PALW_STATE_V2_VERSION.to_le_bytes().as_slice()) {
            return Err(format!("the preimage is not a version-{PALW_STATE_V2_VERSION} PALW state preimage"));
        }
        let mut at = 2 + 7 * ROOT;
        match preimage.get(at) {
            Some(0) => at += 1,
            Some(1) => {
                let mut rest = &preimage[at + 1..];
                let before = rest.len();
                <PalwEpochBudgetsV2 as borsh::BorshDeserialize>::deserialize(&mut rest)
                    .map_err(|e| format!("the preimage's epoch-budget block does not decode: {e}"))?;
                at += 1 + (before - rest.len());
            }
            _ => return Err("the preimage's epoch-budget tag is neither 0 nor 1".into()),
        }
        at += 14 * ROOT;
        if !fits(at, 0) {
            return Err("the preimage is shorter than its fixed prefix".into());
        }
        // `phase`: 0 = in the unlabeled run before `tir_shard/v1`; 1 = right after `tir_shard/v1`; 2 = after `floor_state_v1`.
        let (mut phase, mut run) = (0u8, 0usize);
        loop {
            if starts(at, KERNEL) {
                return if fits(at, KERNEL.len() + 2 * ROOT) {
                    Ok(Some(at))
                } else {
                    Err("the preimage's kernel-route block is truncated".into())
                };
            }
            if phase == 0 && run <= UNLABELED_BEFORE_TIR_SHARD && starts(at, TIR_SHARD) {
                if !fits(at, TIR_SHARD.len() + 2 * ROOT) {
                    return Err("the preimage's tir-shard block is truncated".into());
                }
                (at, phase, run) = (at + TIR_SHARD.len() + 2 * ROOT, 1, 0);
                continue;
            }
            if (phase == 0 && run <= UNLABELED_BEFORE_TIR_SHARD || phase == 1 && run == 0) && starts(at, FLOOR) {
                let mut rest = &preimage[at + FLOOR.len()..];
                let before = rest.len();
                <crate::palw_real_share_v1::PalwFloorStateV1 as borsh::BorshDeserialize>::deserialize(&mut rest)
                    .map_err(|e| format!("the preimage's floor-state block does not decode: {e}"))?;
                (at, phase, run) = (at + FLOOR.len() + (before - rest.len()), 2, 0);
                continue;
            }
            let limit = if phase == 0 { UNLABELED_BEFORE_TIR_SHARD + UNLABELED_BEFORE_KERNEL } else { UNLABELED_BEFORE_KERNEL };
            if run < limit && fits(at, ROOT) {
                (at, run) = (at + ROOT, run + 1);
                continue;
            }
            return Ok(None);
        }
    }

    // ---- consistency ----

    /// Recomputes every derivable fact from the primary data and cross-checks the maintained
    /// copies: the exposure accumulator, both weights, and all three indices. Carriage loading
    /// runs this unconditionally; the differential tests run it after every apply.
    /// Takes `params` because one derivable fact needs them: audit C5's abandon hold is a
    /// function of the claim record AND the configured hold span, so an accumulator that could be
    /// recomputed without them would be one the hold is invisible to.
    pub fn assert_internal_consistency(&self, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
        self.assert_internal_consistency_v2(params, false)
    }

    /// [`Self::assert_internal_consistency`] with ADR-0069 Decision 7's weight rule.
    ///
    /// `uncertified_weightless` is `Params::palw_uncertified_weightless` resolved at the chain
    /// point this state stands at, and `false` — every shipped preset — is byte-identical to the
    /// check before the rule existed, refusals included. The face above passes `false` and is kept
    /// for the fixtures that predate the rule; **every production caller must reach this one**, the
    /// same rule `apply_palw_transition_v5` states for its own predecessors.
    pub fn assert_internal_consistency_v2(
        &self,
        params: &PalwStateParamsV2,
        uncertified_weightless: bool,
    ) -> Result<(), PalwStateV2Error> {
        self.assert_internal_consistency_v3(params, uncertified_weightless, None)
    }

    /// [`Self::assert_internal_consistency_v2`] with ADR-0145's derived work basis.
    ///
    /// `canonical_work_daa` is `Params::palw_canonical_work_daa()` — the fence's height, `None` on
    /// every shipped preset, and then byte-identical to the face above. Past it BOTH sums here and
    /// `retire_claim`'s attempt arm re-price through the same helper, so the identity stays an
    /// equality (and, with Decision 7 armed, the bound stays a bound): a claim priced on
    /// MAC-equivalents at its `Final` must not be re-derived in step leaves here, or every node
    /// refuses its own tip the first time a re-priced claim finalizes.
    pub fn assert_internal_consistency_v3(
        &self,
        params: &PalwStateParamsV2,
        uncertified_weightless: bool,
        canonical_work_daa: Option<u64>,
    ) -> Result<(), PalwStateV2Error> {
        for (class_id, window) in &self.class_court_windows {
            if !self.classes.contains_key(class_id) || *window < params.window_court() {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "class court window for {class_id} is unregistered or below the network minimum"
                )));
            }
        }
        self.assert_vertex_consistency_v1()?;
        self.assert_exec_v2_consistency_v1(params)?;
        self.assert_monetary_material_consistency_v1(params)?;
        self.assert_claim_local_context_v1(params)?;
        self.assert_material_release_recovery_consistency_v2(params)?;
        self.assert_mesh_consistency_v1()?;
        // RFC-0010: the permissionless Panel's engine is internally consistent and agrees with the claims, panels and duty rows.
        self.assert_panel_v3_consistency_v1(params)?;
        self.assert_panel_lifetime_consistency_v2(params)?;
        self.assert_paid_claim_consistency_v3(params)?;
        // RFC07/RFC08 signed economic contracts (aux 125–127): rows only past their fences, each its parties' consent.
        self.assert_econ_contracts_consistency_v1(params)?;
        // N3 (round 3): aux 130 rounds only past their fence; every stored judgement re-derived.
        self.assert_beacon_bootstrap_consistency_v1(params)?;
        self.assert_panel_source_fee_consistency_v2()?;
        // Lane BUDGET: the bond budget's own invariants (windows = Σ in-window reservations, open counts, releases, consumption within
        // reservation) — a carriage decodes whatever its tail holds, so an import is refused on a bent engine.
        if let Some(budget) = &self.bond_budget {
            budget.check_consistency().map_err(|why| PalwStateV2Error::CarriageInconsistent(format!("bond budget: {why}")))?;
            self.bond_budget_no_payout_before_final_v1()
                .map_err(|why| PalwStateV2Error::CarriageInconsistent(format!("bond budget: {why}")))?;
            for (id, contract) in &budget.consumer_contracts {
                let claim = self
                    .claims
                    .get(id)
                    .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("paid consumer contract has no native claim".into()))?;
                if contract.terms.accepted_daa != claim.accepted_daa
                    || contract.terms.material_until > claim.trace_retention_daa
                    || match contract.status {
                        crate::palw_bond_budget_v1::PalwPaidConsumerStatusV1::Exhausted { daa }
                        | crate::palw_bond_budget_v1::PalwPaidConsumerStatusV1::Terminal { daa } => {
                            self.last_point.as_ref().is_none_or(|point| daa > point.daa_score)
                        }
                        _ => false,
                    }
                    || match contract.status {
                        crate::palw_bond_budget_v1::PalwPaidConsumerStatusV1::Terminal { daa } => {
                            terminal_daa_of(claim).is_none_or(|at| at < daa)
                        }
                        _ => claim.phase.is_terminal(),
                    }
                {
                    return Err(PalwStateV2Error::CarriageInconsistent(
                        "paid consumer contract differs from accepted native claim".into(),
                    ));
                }
            }
            if budget.header.version >= crate::palw_bond_budget_v1::PALW_BOND_BUDGET_STATE_VERSION_V4 {
                for (id, claim) in &self.claims {
                    if params.paid_consumer_lifecycle_active_at(claim.accepted_daa) && !budget.consumer_contracts.contains_key(id) {
                        return Err(PalwStateV2Error::CarriageInconsistent(
                            "new accepted paid consumer has no immutable contract".into(),
                        ));
                    }
                }
            }
        }
        let mut exposure: BTreeMap<PalwBondKeyV2, u128> = BTreeMap::new();
        let mut safe: u128 = 0;
        // The same sum with Decision 7 switched OFF: the most `safe_weight` this claim set could
        // ever have justified. See the bound at the end of this function for why both are carried.
        let mut safe_ceiling: u128 = 0;
        let mut immature: u128 = 0;
        let mut unresolved: BTreeSet<(u64, Hash64)> = BTreeSet::new();
        // **2026-09-24 DoS audit review: the anchor ring's own invariants.** Every reader
        // (`palw_second_clock_depth_v1`, `palw_settled_anchor_floor_daa_v1`, the ring prune) answers
        // by `partition_point`, which is meaningful only on a sorted slice, and the floor's "pruned"
        // branch reads `ring.len() < settled_attempt_finals`; the fold's own entries refuse an
        // unsorted push or restore, so an honest fold never writes either, but a carriage decodes
        // whatever its tail holds. Every entry is one settled anchor, so the ring can never be
        // longer than the counter. An empty ring — every state below the fence — passes both
        // trivially, so no dormant state's load moves.
        if self.recent_anchor_daas.windows(2).any(|pair| pair[0] > pair[1]) {
            return Err(PalwStateV2Error::CarriageInconsistent("the anchor ring is not sorted".into()));
        }
        if self.recent_anchor_daas.len() as u64 > self.settled_attempt_finals {
            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                "the anchor ring holds {} entries and the chain has settled only {} anchors",
                self.recent_anchor_daas.len(),
                self.settled_attempt_finals
            )));
        }
        // ADR-0124 Decision 3: a duty row names a live claim and nothing else — it is dropped on
        // every terminal write, and it is never written for a claim the state does not hold.
        for claim_id in self.panel_duties.keys() {
            match self.claims.get(claim_id) {
                None => {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!("panel duty row {claim_id} names no claim")));
                }
                Some(claim) if claim.phase.is_terminal() => {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!(
                        "terminal claim {claim_id} still holds a panel duty row"
                    )));
                }
                Some(_) => {}
            }
        }
        // ADR-0132 Upgrade C: an economics snapshot names a live attempt claim of a model class and
        // nothing else — dropped on every terminal write, never written for the floor.
        for claim_id in self.claim_economics.keys() {
            match self.claims.get(claim_id) {
                None => {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!("economics snapshot {claim_id} names no claim")));
                }
                Some(claim) if claim.phase.is_terminal() => {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!(
                        "terminal claim {claim_id} still holds an economics snapshot"
                    )));
                }
                Some(claim) if !matches!(claim.source, PalwClaimSourceV2::Attempt) || claim.class_id == params.base_class_id() => {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!(
                        "economics snapshot {claim_id} names a claim the payout never prices"
                    )));
                }
                Some(_) => {}
            }
        }
        for (id, claim) in &self.claims {
            // Free-prompt structural facts hold in EVERY phase: quanta ≥ 1, uniform quanta, the
            // ledger inside range, spends only on a certified (Final) claim, and zero immature
            // contribution (a commitment is not a block's work).
            if let PalwClaimSourceV2::FreePrompt { quanta, spent } = &claim.source {
                // RFC-0004 A6 (MIP-17), RFC-0003 §I.4: a weightless claim — an evaluation claim, a tensor claim —
                // is the one free-prompt claim with no quanta, and then it earns nothing at all: no pwu, no spend,
                // no receipt rights, no escrow.
                if *quanta == 0 {
                    if claim.pwu != 0 || !spent.is_empty() || claim.rights_reserved != 0 || claim.escrowed_reward != 0 {
                        return Err(PalwStateV2Error::CarriageInconsistent(format!(
                            "free-prompt claim {id} has no quanta and earns something"
                        )));
                    }
                } else if claim.pwu % (*quanta as u64) != 0 || claim.pwu / (*quanta as u64) == 0 {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!("free-prompt claim {id} has non-uniform quanta")));
                }
                if spent.iter().any(|q| *q >= *quanta) {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!("free-prompt claim {id} spent an absent quantum")));
                }
                if !spent.is_empty() && !matches!(claim.phase, PalwClaimPhaseV2::Final { .. }) {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!("free-prompt claim {id} spent before Final")));
                }
                if claim.immature_contribution != 0 {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!(
                        "free-prompt claim {id} carries immature contribution — commitments must not pump live weight"
                    )));
                }
            }
            match claim.phase {
                PalwClaimPhaseV2::Final { .. } => match &claim.source {
                    // An attempt's Final IS its block's certified work.
                    PalwClaimSourceV2::Attempt => {
                        // ADR-0069 Decision 7, through the SAME helper the fold accumulates with —
                        // a re-derivation that priced weight by its own rule would refuse every
                        // state the fold had just built. ADR-0145's basis rides the same helper for
                        // the same reason, and the CEILING moves with it: `safe_ceiling` is "the
                        // most this claim could have carried", which is the same quantity in
                        // whichever unit the claim was priced in, so leaving it on `claim.pwu`
                        // would make the armed bound compare MAC-equivalents against step leaves
                        // and refuse every honest tip.
                        let canonical = self.palw_claim_canonical_weight_v1(claim, canonical_work_daa);
                        let contribution =
                            palw_claim_safe_contribution_v3(&self.class_shares, claim, uncertified_weightless, canonical);
                        // ADR-0160 F-W (D-3): both sums under the C7 ceiling, as `finalize_claim` and
                        // `retire_claim` price a new-rule claim (the identity on every other).
                        let final_safe = |c: u128| crate::palw_weight_cap_v1::palw_weight_final_safe_v1(params, claim, c);
                        // Lane BUDGET: a budgeted claim weighs what its Final was granted, in both sums.
                        let credited = self.bond_budget_weight_or(id, final_safe(contribution));
                        // RFC-0018 Gate C's final-weight site, re-derived through the same predicate (identity below its fence).
                        let credited = crate::palw_rfc0018_v1::palw_gate_c_weight_v1(params, self, id, claim, credited);
                        let ceiling = self.bond_budget_weight_or(id, final_safe(canonical.unwrap_or(claim.pwu as u128)));
                        safe = safe.checked_add(credited).ok_or(PalwStateV2Error::Overflow("consistency safe"))?;
                        safe_ceiling = safe_ceiling.checked_add(ceiling).ok_or(PalwStateV2Error::Overflow("consistency safe"))?;
                    }
                    // A free-prompt Final licenses; only SPENT quanta weighed blocks.
                    PalwClaimSourceV2::FreePrompt { quanta, spent } => {
                        // ADR-0148: the spend's own expression, by the claim's era.
                        let per_quantum =
                            palw_fp_spend_weight_v1(self, claim, *quanta, &PalwFpPricingV1::of(params, canonical_work_daa));
                        let spent_weight = per_quantum
                            .checked_mul(spent.len() as u128)
                            .ok_or(PalwStateV2Error::Overflow("consistency spent weight"))?;
                        // Lane BUDGET: a budgeted claim's spends added what they were granted.
                        let spent_weight = self.bond_budget_weight_or(id, spent_weight);
                        // RFC-0018 Gate C's receipt-spend site, re-derived (identity below its fence).
                        let spent_weight = crate::palw_rfc0018_v1::palw_gate_c_weight_v1(params, self, id, claim, spent_weight);
                        // **The free-prompt lane is NOT priced by Decision 7** — see the open
                        // question recorded at `apply_receipt_spend`. It contributes identically to
                        // both sums, so the bound below neither tightens nor loosens around it.
                        safe = safe.checked_add(spent_weight).ok_or(PalwStateV2Error::Overflow("consistency safe"))?;
                        safe_ceiling = safe_ceiling.checked_add(spent_weight).ok_or(PalwStateV2Error::Overflow("consistency safe"))?;
                    }
                },
                // A voided claim weighs nothing and holds no place in the frontier queue. It may
                // still hold COLLATERAL: audit C5's abandon hold keeps an abandoned free-prompt
                // commitment's reservation for a span, so a panel redraw costs collateral rather
                // than a transaction fee. Recomputed here from the record and the current point,
                // never from a stored flag, so the accumulator and the record cannot disagree.
                PalwClaimPhaseV2::Voided { .. } => {
                    // ADR-0160 E-4: an attempt claim accepted past `palw_capacity_escrow_at_licence`
                    // holds its commitment for `h_obl` past an unconvicted void, from the record alike.
                    if let Some(point) = &self.last_point
                        && (palw_claim_is_on_abandon_hold_v2(claim, params, point.daa_score)
                            || crate::palw_escrow_funding_v2::palw_claim_obligation_hold_v1(claim, params, point.daa_score))
                    {
                        let entry = exposure.entry(claim.bond).or_insert(0);
                        // ADR-0152 SR-1/SR-3: the claim's commitment at the last point, which on its
                        // abandon hold is the whole reservation, as `release_abandon_hold` returns it.
                        *entry = palw_claim_commitment_v1(params, claim, point.daa_score)
                            .and_then(|held| entry.checked_add(held))
                            .ok_or(PalwStateV2Error::Overflow("consistency exposure"))?;
                    }
                }
                _ => {
                    // ADR-0160 F-W: a new-rule claim weighs through its bond's capped term, summed below.
                    // RFC-0018 Gate C's provisional site, re-derived (identity below its fence).
                    if !crate::palw_weight_cap_v1::palw_weight_cap_applies_v1(params, claim) {
                        immature = immature
                            .checked_add(crate::palw_rfc0018_v1::palw_claim_provisional_weight_v1(params, claim))
                            .ok_or(PalwStateV2Error::Overflow("consistency immature"))?;
                    }
                    let entry = exposure.entry(claim.bond).or_insert(0);
                    // The claim's commitment (ADR-0152 SR-1): its whole reservation — option A's
                    // escrow term is 0 below its height, the receipt rights 0 for anything but a
                    // free-prompt claim past the audit fence — less the escrow term once a licence
                    // released it (past `palw_rcore_plus` only), exactly as the ledger's writers
                    // moved it. A pure function of the record and the last point.
                    let at = self.last_point.map(|point| point.daa_score).unwrap_or(0);
                    *entry = palw_claim_commitment_v1(params, claim, at)
                        .and_then(|held| entry.checked_add(held))
                        .ok_or(PalwStateV2Error::Overflow("consistency exposure"))?;
                    // **ADR-0124 Decision 3: every seat on duty holds its exposure**, rebuilt from the
                    // duty row so the ledger and the row cannot drift apart about who holds what —
                    // and at the amount the row STORES (ADR-0130), which is what binding reserved
                    // and what release will return, whatever the fences say at this point.
                    if let Some(row) = self.panel_duties.get(id) {
                        for seat in row.seats.keys() {
                            let held = exposure.entry(*seat).or_insert(0);
                            *held =
                                held.checked_add(row.seat_exposure).ok_or(PalwStateV2Error::Overflow("consistency seat exposure"))?;
                        }
                    }
                    // **ADR-0062 SA-1: a disputed claim also holds the ACCUSER's stake**, exactly
                    // as an open court holds the challenger's below — and rebuilt from the phase's
                    // own snapshot, so the ledger and the record cannot drift apart about what an
                    // accusation cost.
                    if let PalwClaimPhaseV2::DefaultDisputed { accuser, accuser_exposure, resumed, .. } = &claim.phase {
                        // **`resumed` is the one phase in this state nothing else re-derives**, so
                        // it is the one the loader has to police. The accusation arm refuses a
                        // terminal claim and refuses a disputed one, so the phase it snapshots is
                        // always live and undisputed; a record saying otherwise is not a state
                        // this fold could have written. Caught HERE rather than at the disclosure,
                        // where `rearm_claim_after_da_session` would meet it one block too late to
                        // say where it came from.
                        if !matches!(
                            **resumed,
                            PalwClaimPhaseV2::Provisional
                                | PalwClaimPhaseV2::PanelBound { .. }
                                | PalwClaimPhaseV2::ReceiptLicensed { .. }
                        ) {
                            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                                "disputed claim {id} resumes into {resumed:?}, which is not a phase an accusation could have found"
                            )));
                        }
                        let entry = exposure.entry(*accuser).or_insert(0);
                        *entry = entry.checked_add(*accuser_exposure).ok_or(PalwStateV2Error::Overflow("consistency exposure"))?;
                    }
                    unresolved.insert((claim.accepted_blue_score, *id));
                }
            }
        }
        // **ADR-0152 R-core+: the staged record's load invariants** (S-SPEC §3.2, §3.8), past the fence
        // only — below it every record is `Default` and nothing here is asked. A record the fold could
        // not have written is refused at load, where a node reading its own tip can still say which.
        if params.rcore_plus_from_daa().is_some() {
            for (id, claim) in &self.claims {
                let rcore = &claim.rcore;
                match &claim.phase {
                    PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. } if *rcore != PalwClaimRcoreV1::default() => {
                        return Err(PalwStateV2Error::CarriageInconsistent(format!(
                            "claim {id} has not licensed and carries an R-core+ record"
                        )));
                    }
                    PalwClaimPhaseV2::ReceiptLicensed { .. } if rcore.licence_door.is_none() => {
                        return Err(PalwStateV2Error::CarriageInconsistent(format!("licensed claim {id} records no licence door")));
                    }
                    _ => {}
                }
                // The S-4 review's G freeze: the licence-time gain is written with the door, never alone.
                if rcore.licence_door.is_none() && rcore.g_res_sompi != 0 {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!(
                        "claim {id} records a licence-time gain but no licence"
                    )));
                }
                if claim.phase.is_terminal() {
                    continue;
                }
                if let Some(panel) = self.panels.get(id) {
                    let full = crate::palw_verification_v2::PalwSegmentMaskV2::full(panel.seats.len().min(32) as u16).0;
                    if rcore.served_mask & !full != 0 {
                        return Err(PalwStateV2Error::CarriageInconsistent(format!(
                            "claim {id} serves a seat its panel does not have"
                        )));
                    }
                }
                // SR-4: the flag is set only where the record it was set on says the release was due
                // (not re-judged against the mutable lifecycle row or C7 list), and never un-set.
                if rcore.escrow_released && !palw_rcore_release_record_holds_v1(self, id, claim) {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!(
                        "claim {id} released its escrow without the release being due"
                    )));
                }
            }
        }
        let mut open_courts: BTreeMap<Hash64, u32> = BTreeMap::new();
        for session in self.court_sessions.values() {
            *open_courts.entry(session.claim).or_insert(0) += 1;
            // An open court also holds the CHALLENGER's stake, so the exposure this check rebuilds
            // is claims plus courts. Opening one used to cost nothing at all, which is why the
            // summary was once claims alone. **ADR-0152 A-6: past `palw_rcore_plus` it is the
            // accuser ledger's (`palw_accuser_exposure_v1`), re-derived from the sessions, and not
            // this one's.**
            if params.rcore_plus_from_daa().is_none()
                && let Some(claim) = self.claims.get(&session.claim)
            {
                let e = exposure.entry(session.challenger_bond).or_insert(0);
                *e = e.checked_add(claim.reserved).ok_or(PalwStateV2Error::Overflow("consistency exposure"))?;
            }
        }
        // ADR-0045 Decision 3: the share table's invariants hold at EVERY state, not once at
        // boot. The donation arithmetic conserves the denominator, so a populated table sums to
        // exactly 1000‰, and every share names a class.
        //
        // The key sets are NOT equal, and the difference is exactly the weightless registrations
        // (conditions 12/13): a class awaiting its activation edge is in the registry — it is
        // adjudicable, and a dispute against it must resolve — while holding no permille. What
        // must hold is the two directions separately: every SHARE names a class, and every
        // ACTIVE-or-FROZEN class holds a share. A `Registered` one holds none, by construction.
        //
        // **And a `Dormant` one holds none either** (ADR-0056 Decision 5). It is the same shape as
        // a pre-activation class from the table's point of view — in the registry, adjudicable,
        // carrying no permille — arrived at from the other end. This invariant caught the new
        // status the first time reclamation ran, which is what it is for.
        let share_bearing: BTreeSet<&Hash64> = self
            .classes
            .iter()
            .filter(|(_, c)| !matches!(c.status, PalwClassStatusV2::Registered { .. } | PalwClassStatusV2::Dormant { .. }))
            .map(|(id, _)| id)
            .collect();
        if !self.class_shares.keys().collect::<BTreeSet<_>>().eq(&share_bearing) {
            return Err(PalwStateV2Error::CarriageInconsistent(
                "the class set and the share table disagree — every share names a class, and every class past its activation edge holds a share".into(),
            ));
        }
        if !self.class_shares.is_empty() {
            let sum: u32 = self.class_shares.values().map(|s| *s as u32).sum();
            if sum != 1000 {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "the share table sums to {sum}‰ — the donation arithmetic conserves exactly 1000‰"
                )));
            }
        }
        // **RFC-0007 Part IV.1: the audit mesh's reservations** — an auditor's penalty and a trap setter's deposit ride in
        // `reserved_exposure` (so the withdrawal rule and every free-collateral reader see them), re-derived here from the rows
        // that hold them.
        for row in self.vertex.mesh.audits.values() {
            for assignment in &row.assignments {
                if assignment.reserved > 0 {
                    let held = exposure.entry(assignment.auditor).or_insert(0);
                    *held =
                        held.checked_add(assignment.reserved).ok_or(PalwStateV2Error::Overflow("consistency audit reservation"))?;
                }
            }
        }
        for row in self.vertex.mesh.traps.values() {
            if row.deposit > 0 {
                let held = exposure.entry(row.setter).or_insert(0);
                *held = held.checked_add(row.deposit).ok_or(PalwStateV2Error::Overflow("consistency trap deposit"))?;
            }
        }
        // **RFC-0008 v2: a live root's extra executors stand behind it** — the claim's own reservation, held in
        // `reserved_exposure` for the root's life and re-derived here from the root rows that hold it.
        for (bond, held_by_root) in self.exec_v2_exposure_rows() {
            let held = exposure.entry(bond).or_insert(0);
            *held = held.checked_add(held_by_root).ok_or(PalwStateV2Error::Overflow("consistency exec v2 executor exposure"))?;
        }
        // **ADR-0160 F-W: where the fence's mirror is set, a zero row and an absent row are one fact.**
        // A capped reservation can be 0 (a claim past its bond's `R_budget`), so a live claim may commit
        // nothing — and the ledger's writers drop a row that reaches 0 (`release_for_claim`,
        // `move_commitment`) while this re-derivation opens one for every live claim. Below the fence
        // every attempt commits `reserved > 0`, the two never differ by a zero row, and the comparison
        // is the exact one it always was.
        let exposure_matches = if params.capacity_weight_cap_from_daa().is_some() {
            let nonzero = |ledger: &BTreeMap<PalwBondKeyV2, u128>| -> BTreeMap<PalwBondKeyV2, u128> {
                ledger.iter().filter(|(_, held)| **held != 0).map(|(bond, held)| (*bond, *held)).collect()
            };
            nonzero(&exposure) == nonzero(&self.reserved_exposure)
        } else {
            exposure == self.reserved_exposure
        };
        if !exposure_matches {
            return Err(PalwStateV2Error::CarriageInconsistent("reserved_exposure differs from the claims it summarizes".into()));
        }
        // **ADR-0056 Decision 3, the same property for the registry's own ledger.** Two
        // accumulators, each derivable from the collection it summarizes and each checked against
        // it — which is exactly why they are not one map: folding registrations into
        // `reserved_exposure` would have made the check above unable to state anything.
        let mut registry: BTreeMap<PalwBondKeyV2, u128> = BTreeMap::new();
        let price = params.registration_exposure_sompi() as u128;
        if price > 0 {
            for record in self.classes.values() {
                let live = matches!(record.status, PalwClassStatusV2::Active | PalwClassStatusV2::Registered { .. });
                if let (true, Some(bond)) = (live, record.registrant_bond) {
                    *registry.entry(bond).or_insert(0) += price;
                }
            }
        }
        if registry != self.registration_exposure {
            return Err(PalwStateV2Error::CarriageInconsistent(
                "registration_exposure differs from the live registrations it summarizes".into(),
            ));
        }
        let safe = safe.checked_add(self.retired_safe_weight).ok_or(PalwStateV2Error::Overflow("consistency safe"))?;
        let safe_ceiling = safe_ceiling.checked_add(self.retired_safe_weight).ok_or(PalwStateV2Error::Overflow("consistency safe"))?;
        // **Dormant, this is the equality it has always been.** With Decision 7 off,
        // `palw_claim_safe_contribution_v2` is the identity on `pwu`, so `safe == safe_ceiling`
        // and the test below is `safe == self.safe_weight` — every state this check used to accept
        // or refuse it accepts or refuses identically, error string included.
        //
        // **Armed, it is an UPPER BOUND, and the reason is worth stating in full rather than
        // discovering.** `safe_weight` is a running total: a claim's contribution is priced once,
        // at the block that finalizes it, and never revisited — that is E3, the rule that stops a
        // later certification from reordering history. This re-derivation runs at a different
        // chain point, against a share table that can have MOVED since, in both directions: a
        // `Registered` class holds no row until its activation edge and then holds permille, and
        // reclamation takes a walked-away class's row away again. `retired_safe_weight` cannot
        // reconcile the difference either — the claims it summarizes are gone — so it counts
        // `claim.pwu`, the most any of them could have carried (see `retire_claim`).
        //
        // **The free-prompt lane is exempt from that slack.** A retiring free-prompt claim folds
        // `per_quantum × |spent|` — the same expression the live arm above uses — so its half of
        // both sums is an equality in both directions. That is not a nicety: before it, a spent
        // free-prompt claim's retirement stranded its weight in `safe_weight`, `safe_weight` rose
        // ABOVE `safe_ceiling`, and this check refused the node's OWN tip on the next restart. It
        // is fenced with the rest of Decision 7 because `retired_safe_weight` is rooted; dormant
        // the stranding is still there, and `retire_claim` says what that costs.
        //
        // **What this bound is worth, stated at its real strength rather than its hoped-for one.**
        // It stops a snapshot from claiming more safe weight than its own LIVE claims could have
        // justified — the ≈8× fabrication Decision 7 prices at zero — and nothing beyond that.
        // Understating is unchecked, which is harmless. Two things are NOT covered and saying so
        // here is the point of writing it down:
        //
        //  * `retired_safe_weight` is a carriage-supplied scalar with no re-derivation at all, in
        //    either fence position, because the claims that would re-derive it are gone by design.
        //    A carriage may declare any value for it and both the equality and the bound absorb it.
        //  * `class_shares` is carriage-supplied too, and the only checks on it are structural
        //    (keys match the non-`Registered`/non-`Dormant` classes, sum is 1000‰). Nothing ties a
        //    share to a certificate, so a peer can declare its own uncertified class `Active` at
        //    1000‰ and be weight-bearing by this fold's own predicate.
        //
        // So this is NOT the authority on an import, and an earlier draft of this comment claimed
        // `into_state_v2`'s `state_root` check was. It is not either: `palw_state_root` is not
        // checked at header validation (see the comment at `import_pruning_point_palw_state`), the
        // expected root comes from the pruning point's selected-chain-child header, and in a pruned
        // IBD that header arrived from the same peer that supplied the carriage — so the check
        // authenticates carriage-against-header, not carriage-against-rule. What actually stands
        // behind an imported weight is that the peer had to win the headers-proof and PoW race for
        // that child. Closing the two holes above needs the weight-bearing decision FROZEN on the
        // claim (a per-claim bit, so `retired_safe_weight` can be re-derived by class) and a
        // certification field the share table can be checked against — both change the claim /
        // class encoding, so both need a `PALW_STATE_V2_VERSION` bump and a re-mint.
        let consistent = if uncertified_weightless { self.safe_weight <= safe_ceiling } else { safe == self.safe_weight };
        if !consistent {
            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                "safe_weight {} differs from the Final claims' sum plus the retired total {safe}",
                self.safe_weight
            )));
        }
        // **ADR-0160 F-W (W-I3): the new-rule half, from the claims and the bonds alone** — the per-bond
        // index re-derived and compared (it is carried like `unresolved`), and each bond's staged sum
        // under its cap. Both are empty below the fence, so every dormant state loads as before.
        let weight_index = crate::palw_weight_cap_v1::PalwBondWeightIndexV1::of(params, self.claims.values());
        if weight_index != self.capacity_weight_index {
            return Err(PalwStateV2Error::CarriageInconsistent("the capacity weight index differs from the claims".into()));
        }
        let immature =
            immature.checked_add(weight_index.capped_total(self)).ok_or(PalwStateV2Error::Overflow("consistency immature"))?;
        if immature != self.bounded_immature {
            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                "bounded_immature {} differs from the immature claims' sum {immature}",
                self.bounded_immature
            )));
        }
        if unresolved != self.unresolved {
            return Err(PalwStateV2Error::CarriageInconsistent("unresolved index differs from the claims".into()));
        }
        let work_ids: BTreeMap<Hash64, Hash64> = self.claims.iter().filter_map(|(id, c)| c.work_id.map(|w| (w, *id))).collect();
        if work_ids != self.work_ids {
            return Err(PalwStateV2Error::CarriageInconsistent("work-id index differs from the claims".into()));
        }
        // **ADR-0145 §6 / ADR-0148: a prompt row outlives its claim, and not its retention.** The
        // row is not derivable from the claim table — it is deliberately kept past the claim, so a
        // retired claim's prefill is not sold twice — so this is a containment check: every row is
        // filed under a class the chain holds, and none has outlived the epoch-aligned expiry the
        // boundary sweep removes it at. A row past its expiry would discount later prompts of its
        // class for as long as the state lived, silently, which is the failure a carriage-supplied
        // index is checked for. (It used to require the row's CLAIM to be live, which the durable
        // row broke on the first retirement — every node would have refused its own tip.)
        let now = self.last_point.as_ref().map(|point| point.daa_score);
        for ((class_id, _, claim_id), row) in &self.fp_claim_prompts {
            if !self.classes.contains_key(class_id) {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "free-prompt prompt row of claim {claim_id} is filed under class {class_id}, which the chain does not hold"
                )));
            }
            if now.is_some_and(|now| row.expires_daa <= now) {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "free-prompt prompt row of claim {claim_id} expired at {} and was not swept",
                    row.expires_daa
                )));
            }
        }
        if open_courts != self.open_courts_by_claim {
            return Err(PalwStateV2Error::CarriageInconsistent("open-court index differs from the sessions".into()));
        }
        // The SAME key the writer and the rebuild use — whichever of the session's two clocks
        // runs out first. Recomputing it any other way here would make the checker disagree with
        // the index it is checking, which is how a ladder move looked like corruption.
        let court_deadlines: BTreeSet<(u64, Hash64)> =
            self.court_sessions.iter().map(|(id, s)| (court_next_deadline_v2(self, s), *id)).collect();
        if court_deadlines != self.court_deadlines {
            return Err(PalwStateV2Error::CarriageInconsistent("court-deadline index differs from the sessions".into()));
        }
        // RFC-0002 F7: an IR dissection is a row about a live session of its own id, which holds no
        // legacy phase — `write_court` drops it with its session.
        for (id, phase) in &self.tir_dissections {
            match self.court_sessions.get(id) {
                Some(session) if session.dissection.is_none() && phase.session_id() == *id => {}
                _ => {
                    return Err(PalwStateV2Error::CarriageInconsistent(
                        "an IR dissection is not a row about a live session without a legacy phase".into(),
                    ));
                }
            }
        }
        // The declared closes' own queue, checked the same way and against the same primary data
        // (audit H-2c). A stale one is a group whose bytes nothing sweeps.
        let court_close_deadlines: BTreeSet<(u64, (Hash64, PalwCourtSideV1))> =
            self.court_close_groups.iter().map(|(key, group)| (group.assembly_deadline_daa, *key)).collect();
        if court_close_deadlines != self.court_close_deadlines {
            return Err(PalwStateV2Error::CarriageInconsistent("close-assembly deadline index differs from the groups".into()));
        }
        // ADR-0152 R-core+ (S-SPEC 1g): the accuser and reporter indexes against the maps they index.
        if palw_courts_by_challenger_of_v1(&self.court_sessions) != self.courts_by_challenger {
            return Err(PalwStateV2Error::CarriageInconsistent("court-challenger index differs from the sessions".into()));
        }
        if palw_commitment_indexes_of_v1(&self.reporter_commitments)
            != (self.commitments_by_reporter.clone(), self.commitments_by_daa.clone())
        {
            return Err(PalwStateV2Error::CarriageInconsistent("reporter-commitment indexes differ from the commitments".into()));
        }
        // ADR-0152 V-3 (the vesting work): the rows' four derived indexes, against the rows.
        if !self.vesting_indices_are_consistent() {
            return Err(PalwStateV2Error::CarriageInconsistent("the vesting indexes differ from the vesting rows".into()));
        }
        // M3 (DA-2): the DA indexes against the maps, and the maps against each other and the claims.
        if palw_da_indexes_of_v1(&self.da_sessions, &self.da_claims) != (self.da_deadlines.clone(), self.da_by_accuser.clone()) {
            return Err(PalwStateV2Error::CarriageInconsistent("DA indexes differ from the DA sessions and records".into()));
        }
        self.assert_da_consistency_v1(params)?;
        // Lane LG14-A: the dispute records, their indexes and their caps.
        self.assert_legacy_disputes_consistency_v1(params)?;
        self.assert_public_fp_jobs_consistency_v1(params)?;
        self.assert_legacy_answers_v2()?;
        self.assert_terminal_evidence_v1()?;
        for (key, row) in &self.staged_courts {
            row.check(key).map_err(PalwStateV2Error::CarriageInconsistent)?;
        }
        let responders: BTreeSet<_> = self.staged_courts.values().flat_map(palw_staged_court_fold_v1::response_index_rows).collect();
        if responders != self.staged_prefix_responders {
            return Err(PalwStateV2Error::CarriageInconsistent(
                "public prefix responder index differs from literal live obligations".into(),
            ));
        }
        let selected: BTreeSet<_> =
            self.staged_courts.values().flat_map(palw_staged_court_fold_v1::selected_response_index_rows_v1).collect();
        if selected != self.staged_selected_responders {
            return Err(PalwStateV2Error::CarriageInconsistent(
                "selected responder index differs from literal paid obligations".into(),
            ));
        }
        let flat: BTreeSet<_> = self.staged_courts.values().flat_map(palw_staged_court_fold_v1::flat_response_index_rows_v1).collect();
        if flat != self.staged_flat_responders {
            return Err(PalwStateV2Error::CarriageInconsistent(
                "Flat responder index differs from literal paid phase obligations".into(),
            ));
        }
        let history: BTreeSet<_> =
            self.staged_courts.values().flat_map(palw_staged_court_fold_v1::history_response_index_rows_v1).collect();
        if history != self.staged_history_responders {
            return Err(PalwStateV2Error::CarriageInconsistent(
                "history responder index differs from literal accepted demand clocks".into(),
            ));
        }
        let inventory: BTreeSet<_> = self
            .staged_courts
            .values()
            .filter_map(|row| match row {
                crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::ArtifactInventoryOpen(row) => {
                    Some((row.offer.opener, row.offer.artifact_hash))
                }
                _ => None,
            })
            .collect();
        if inventory != self.staged_inventory_by_owner {
            return Err(PalwStateV2Error::CarriageInconsistent("artifact inventory owner index differs from paid attempts".into()));
        }
        self.assert_tir_acquisition_indices_v1()?;
        if let Some(service) = self.court_service_state_v1() {
            service.check_context(self).map_err(PalwStateV2Error::CarriageInconsistent)?;
        }
        // G14 lane r6-d1-release: the released form's strict ownership/record/edge check (also run by the row codec check).
        if let Some(row) = self.court_service_row_v3_v1() {
            row.check().map_err(PalwStateV2Error::CarriageInconsistent)?;
        }
        for row in self.staged_courts.values() {
            if let crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::CourtDutyClock(clock) = row {
                clock.check_context(self).map_err(PalwStateV2Error::CarriageInconsistent)?;
            }
            use crate::palw_staged_court_state_v1::PalwStagedCourtRowV1 as R;
            if let R::CourtArchiveProgress(r)=row {
                // Preserve local construction refusal instead of converting it
                // into a malformed state/offence while loading accepted bytes.
                r.check_loaded_context(self)?;
                continue;
            }
            match row {
                R::CourtSharedContext(c)=>c.check_context(self),
                R::CourtSharedQuery(q)=>q.check_context(self),
                R::CourtSharedIds(ids)=>ids.check_context(self),
                R::CourtSharedResponse(r)=>r.check_context(self),
                R::CourtEndArchive(r)=>r.check_loaded_context(self),
                R::CourtCloseReceipt(r)=>r.check_loaded_context(self),
                R::CourtSharedRoot(r)=>r.check_loaded_context(self),
                R::CourtSharedRound(r)=>r.check_loaded_context(self),
                R::CourtSharedBottom(r)=>r.check_loaded_context(self),
                R::CourtSharedStream(s)=>s.check_context(self),
                R::CourtSharedChunk(c)=>c.check_context(self),
                R::CourtMaterialDraft(d)=>d.check_context(self),
                R::CourtMaterial(m)=>m.check_context(self),_=>Ok(()),
            }.map_err(PalwStateV2Error::CarriageInconsistent)?;
        }
        // ADR-0152-adjacent: the Activation Pool's I1–I4.
        self.assert_activation_pool_consistency_v1()?;
        self.assert_held_forfeits_consistency_v1()?;
        self.assert_probation_memory_consistency_v1()?;
        // ADR-0160 lane liab (F-L): every freeze names a registered bond.
        self.assert_bond_freezes_consistency_v1()?;
        // ADR-0160 stage 2 (F-Q, F-S): receipts name live claims, buckets and exclusions registered bonds.
        self.assert_capacity_qs_consistency_v1()?;
        // ADR-0164: the capacity ledger's keys and rows agree, inside their bounds.
        self.assert_capacity_ledger_consistency_v1()?;
        // Every deadline belongs to a live, non-terminal claim in the phase its kind implies —
        // and every non-terminal claim without an open court has exactly one deadline.
        let mut expected_deadlines: BTreeSet<(u64, Hash64)> = BTreeSet::new();
        for (id, claim) in &self.claims {
            // ADR-0152 DA-5 (M3): a live claim a seat session pauses owes none (DL-1's DA row).
            if self.da_claims.get(id).is_some_and(|record| record.open_seat_sessions > 0) {
                continue;
            }
            // Lane LG14-A: a claim a live dispute reservation holds owes none (DL-1's reservation row; empty below the fence).
            if self.palw_legacy_dispute_holds_v1(id) || self.court_service_holds_retirement_v1(id, claim) {
                continue;
            }
            // RFC-0010: the engine clocks a claim it tracks until the claim is licensed (it owes no V2 deadline).
            if matches!(claim.phase, PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. })
                && self.panel_v3_clocks_claim_v1(id)
            {
                continue;
            }
            // ADR-0160 F-Q: a credited licensed claim awaiting its audit owes none (DL-1's audit row).
            if matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. })
                && matches!(
                    crate::palw_audit_door_v1::palw_capacity_audit_gate_v1(params, claim, self.audit_receipts.get(id)),
                    Some(None)
                )
            {
                continue;
            }
            // RFC-0008 v2: a licensed claim whose work session is not ready owes none (DL-1's session row).
            if matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) && self.exec_v2_holds_final_v1(id) {
                continue;
            }
            // RFC-0010 × G14 (SHARD): a V3 S2 licence a pending accusation holds owes none (DL-1's G14 row), nor does a lane-A
            // claim the part-C hold reaches while one is pending.
            if self.palw_v3_s2_licence_held_v1(id, claim) || self.palw_v2_expiry_held_v1(params, id, claim) {
                continue;
            }
            if let Some(deadline) = expected_deadline(claim, self.open_courts_by_claim.get(id).copied().unwrap_or(0)) {
                expected_deadlines.insert((deadline_with_params(&claim.phase, deadline), *id));
            }
        }
        // `expected_deadline` needs params to compute the exact daa; consistency without params
        // checks membership shape only. See `assert_deadline_consistency` for the parameterized
        // check used everywhere a params handle exists.
        //
        // Audit C5's abandon hold widens this into a RANGE rather than an equality: a
        // `BindTimeout`-voided free-prompt claim owns a deadline while its hold runs and none
        // afterwards, and without params this check cannot tell which. The exact count is the
        // parameterized check's business; here the abandoned claims are the slack.
        // The slack is exactly the set of claims that MAY own a terminal deadline. With retirement
        // on that is every terminal claim; with it off it is only the abandon holds, so a network
        // that does not retire keeps the tight bound it had.
        let holdable = if params.claim_retirement_daa > 0 {
            self.claims.values().filter(|c| c.phase.is_terminal()).count()
        } else {
            // ADR-0160 E-4: an attempt's obligation hold owns a terminal deadline too.
            self.claims
                .values()
                .filter(|c| {
                    abandon_hold_may_hold_a_deadline(c)
                        || crate::palw_escrow_funding_v2::palw_claim_obligation_release_at_v1(c, params).is_some()
                })
                .count()
        };
        let low = expected_deadlines.len();
        let high = low + holdable;
        if self.deadlines.len() < low || self.deadlines.len() > high {
            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                "deadline index holds {} entries, the claims imply {low}..={high}",
                self.deadlines.len()
            )));
        }
        // **ADR-0078 Decision 4: the derived table's bound is the claim table's, and that is only
        // true if every row still has a claim** (audit 2026-09-02).
        //
        // The transition enforces both halves — the cap when it writes a row, the sweep when the
        // claim retires — but neither is a property of a STATE, and a carriage is a state
        // somebody else wrote. An orphan row is the failure this repo has already paid for once:
        // retirement is claim-driven, so a row whose claim is gone is never swept by anything,
        // and the table stops being bounded by `claim_retirement_daa` at all. Rebuilt here from
        // the claims, like every other index, so a summary that is not derivable from the claims
        // is refused when it is READ rather than believed and grown.
        let mut per_claim: BTreeMap<Hash64, usize> = BTreeMap::new();
        for key in self.derived_artifacts.keys() {
            if !self.claims.contains_key(&key.claim) {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "a derived artifact names claim {}, which the state does not hold — the row would never retire",
                    key.claim
                )));
            }
            *per_claim.entry(key.claim).or_insert(0) += 1;
        }
        if let Some((claim, held)) = per_claim.iter().find(|(_, held)| **held > crate::palw_derived_v1::PALW_DERIVED_MAX_PER_CLAIM) {
            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                "claim {claim} holds {held} derivations, more than PALW_DERIVED_MAX_PER_CLAIM ({})",
                crate::palw_derived_v1::PALW_DERIVED_MAX_PER_CLAIM
            )));
        }
        // **ADR-0080 design A: a close group is a row ABOUT a session, and it says three things
        // twice.** `present` and the keys of `chunks` are the same fact in two encodings — the
        // design named the bitmap and the carriage has to hold the bytes — and a carriage is a
        // state somebody else wrote, so the two are compared here rather than believed. So is the
        // session: a group whose session the state does not hold is a row nothing will ever sweep,
        // which is exactly the orphan class the derived table above pays for.
        for ((session, side), group) in &self.court_close_groups {
            if !self.court_sessions.contains_key(session) {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "a close group names session {session}, which the state does not hold — the row would never be swept"
                )));
            }
            if group.count == 0 || group.count > PALW_COURT_CLOSE_MAX_CHUNKS {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "the close group of session {session} declares {} chunks, outside 1..={PALW_COURT_CLOSE_MAX_CHUNKS}",
                    group.count
                )));
            }
            if group.chunk_digests.len() != group.count as usize {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "the close group of session {session} declares {} chunks and pins {} digests",
                    group.count,
                    group.chunk_digests.len()
                )));
            }
            let mut mask = 0u64;
            for (index, bytes) in &group.chunks {
                if *index >= group.count {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!(
                        "the close group of session {session} holds chunk {index} of a {}-chunk close",
                        group.count
                    )));
                }
                if bytes.is_empty() || bytes.len() > PALW_COURT_CLOSE_CHUNK_MAX_BYTES {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!(
                        "chunk {index} of session {session}'s close carries {} bytes",
                        bytes.len()
                    )));
                }
                if palw_court_close_chunk_digest_v1(bytes) != group.chunk_digests[*index as usize] {
                    return Err(PalwStateV2Error::CarriageInconsistent(format!(
                        "chunk {index} of session {session}'s close is not the bytes its declaration pinned"
                    )));
                }
                mask |= 1u64 << index;
            }
            if mask != group.present {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "the close group of session {session} claims presence {:#x} and holds {mask:#x}",
                    group.present
                )));
            }
            // The declaration's own gate, re-asked of the row: it may never sit past the backstop,
            // because a declaration that could not finish inside it is a rung suspended for free.
            let backstop = self.court_sessions[session].deadline_daa;
            if group.assembly_deadline_daa > backstop {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "the close group of session {session} assembles until {} and the backstop is {backstop}",
                    group.assembly_deadline_daa
                )));
            }
            let _ = side;
        }
        Ok(())
    }

    /// **ADR-0152-adjacent: the Activation Pool's invariants** — what a carriage somebody else wrote
    /// must satisfy before it is believed:
    ///
    /// * **I1** every row is balanced: `funded == prep + bonus + paid + withheld`;
    /// * **I2** the counters are the sums of the rows (and so balanced too);
    /// * **I3** the three operator lists are strictly increasing and within the structural caps;
    /// * **I4** a row names a class the state holds.
    ///
    /// Empty on every network but testnet-12, where every check is a walk of the pools alone.
    pub(crate) fn assert_activation_pool_consistency_v1(&self) -> Result<(), PalwStateV2Error> {
        use crate::palw_activation_pool_v1::{
            PALW_ACTIVATION_PAYEE_CAP_MAX_V1, PALW_ACTIVATION_PROBE_CREDITED_MAX_V1, PalwActivationPoolCountersV1,
            palw_sorted_unique_v1,
        };
        let bad = |why: String| Err(PalwStateV2Error::CarriageInconsistent(why));
        for (class_id, row) in &self.activation_pools {
            if !self.classes.contains_key(class_id) {
                return bad(format!("an activation pool names class {class_id}, which the state does not hold (I4)"));
            }
            if !row.is_balanced() {
                return bad(format!("class {class_id}'s activation pool is not balanced: {row:?} (I1)"));
            }
            for (name, list) in [("prep_paid", &row.prep_paid), ("bonus_paid", &row.bonus_paid)] {
                if !palw_sorted_unique_v1(list) || list.len() > PALW_ACTIVATION_PAYEE_CAP_MAX_V1 {
                    return bad(format!(
                        "class {class_id}'s activation pool {name} is not sorted, unique and within {PALW_ACTIVATION_PAYEE_CAP_MAX_V1} (I3)"
                    ));
                }
            }
            if !row.credits_are_sorted_unique() || row.probe_credited.len() > PALW_ACTIVATION_PROBE_CREDITED_MAX_V1 {
                return bad(format!("class {class_id}'s activation pool probe_credited is not sorted, unique and capped (I3)"));
            }
        }
        // I5 (F5): every scheduled entry is owed by a pool, is positive, and each class's entries
        // sum to its row's `scheduled_sompi`.
        let mut scheduled_by_class: BTreeMap<Hash64, u128> = BTreeMap::new();
        for ((class_id, _), amount) in &self.activation_pool_scheduled {
            if *amount == 0 || !self.activation_pools.contains_key(class_id) {
                return bad(format!("a scheduled activation payout of class {class_id} is zero or owed by no pool (I5)"));
            }
            *scheduled_by_class.entry(*class_id).or_default() += u128::from(*amount);
        }
        for (class_id, row) in &self.activation_pools {
            if scheduled_by_class.get(class_id).copied().unwrap_or(0) != u128::from(row.scheduled_sompi) {
                return bad(format!("class {class_id}'s scheduled activation payouts do not sum to its row's scheduled (I5)"));
            }
        }
        // F4: a landing record names a class with a pool and a bond the state holds.
        for (class_id, bond) in self.activation_readiness_landed.keys() {
            if !self.activation_pools.contains_key(class_id) || !self.bonds.contains_key(bond) {
                return bad(format!("a readiness landing of class {class_id} names no pool or no bond"));
            }
        }
        if PalwActivationPoolCountersV1::of_rows(self.activation_pools.values()) != self.activation_pool_counters {
            return bad("the activation pool counters differ from the sums of the rows (I2)".into());
        }
        if !self.activation_pool_counters.is_balanced() {
            return bad("the activation pool counters are not balanced (I1)".into());
        }
        Ok(())
    }

    /// **The 2026-09-25 sweep's V03(2)/V05: a probation memory is a rowed class's, non-empty, keeps
    /// progress only while the class is `Held`, and names at most two failed bonds, sorted and unique.**
    /// Empty on every network but testnet-12.
    pub(crate) fn assert_probation_memory_consistency_v1(&self) -> Result<(), PalwStateV2Error> {
        use crate::palw_model_registry_v1::{PALW_PROBATION_RESET_DISTINCT_BONDS_V1, PalwModelLifecycleV1};
        for (class_id, memory) in &self.probation_memory {
            let bad = |why: &str| Err(PalwStateV2Error::CarriageInconsistent(format!("probation memory {class_id}: {why}")));
            let Some(row) = self.model_lifecycles.get(class_id) else { return bad("its class has no lifecycle row") };
            if memory.is_empty() {
                return bad("an empty memory is no row");
            }
            if memory.held_probes_passed.is_some() && !matches!(row.state, PalwModelLifecycleV1::Held) {
                return bad("kept probation progress on a class that is not Held");
            }
            if memory.failed_bonds.len() > PALW_PROBATION_RESET_DISTINCT_BONDS_V1
                || memory.failed_bonds.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return bad("its failed bonds are not sorted, unique and at most two");
            }
        }
        Ok(())
    }

    /// **ADR-0160 lane liab (AG-3, A-I2): a freeze names a registered bond, and a FINAL freeze one
    /// whose posted collateral is gone** — the forfeiture took it whole, and a registered bond's
    /// collateral is never raised. Empty on every network but a testnet-12 past F-L.
    /// **ADR-0160 stage 2: the receipts name held claims** (a receipt leaves with its claim's retirement),
    /// each with at least one receipt, distinct auditors in bond order; an exclusion and a bucket name
    /// registered bonds. Empty on every network but a testnet-12 past F-Q / F-S.
    pub(crate) fn assert_capacity_qs_consistency_v1(&self) -> Result<(), PalwStateV2Error> {
        for (claim_id, status) in &self.audit_receipts {
            let bad = |why: &str| Err(PalwStateV2Error::CarriageInconsistent(format!("audit receipts of {claim_id}: {why}")));
            if !self.claims.contains_key(claim_id) {
                return bad("its claim is not held");
            }
            if status.receipts.is_empty() {
                return bad("an empty status is no row");
            }
            if !status.receipts.windows(2).all(|pair| pair[0].0 < pair[1].0) {
                return bad("its auditors are not distinct and in bond order");
            }
        }
        for bond in
            self.excluded_auditors.keys().chain(self.issuance_buckets.keys()).chain(self.network_demand.keys().map(|(bond, _)| bond))
        {
            if !self.bonds.contains_key(bond) {
                return Err(PalwStateV2Error::CarriageInconsistent(format!(
                    "{bond:?}: an exclusion or a bucket of an unregistered bond"
                )));
            }
        }
        Ok(())
    }

    /// **ADR-0164: the capacity ledger's keys and rows agree and sit inside their bounds** — an emission row is positive and within
    /// the DAA's budget, a rider mark names 1..=`PALW_RIDERS_MAX_V1` riders, a breaker row holds a level on the ladder, at most two
    /// clean epochs and at most three distinct short bonds. Empty on every network but a testnet-12 past F-EM / F-M1 / F-K.
    pub(crate) fn assert_capacity_ledger_consistency_v1(&self) -> Result<(), PalwStateV2Error> {
        use crate::palw_capacity_s567_v1::{PalwCapacityLedgerKeyV1 as K, PalwCapacityLedgerRowV1 as R};
        for (key, row) in &self.capacity_ledger {
            let bad = |why: &str| Err(PalwStateV2Error::CarriageInconsistent(format!("capacity ledger {key:?}: {why}")));
            match (key, row) {
                (K::Emission(_), R::Emission { spent_milli }) => {
                    if *spent_milli == 0 || *spent_milli > crate::palw_capacity_s567_v1::palw_emission_budget_milli_v1() {
                        return bad("an emission row is positive and within the DAA's budget");
                    }
                }
                (K::RiderLead(lead), R::Riders { riders, .. }) => {
                    if *riders == 0 || usize::from(*riders) > crate::palw_capacity_s567_v1::PALW_RIDERS_MAX_V1 {
                        return bad("a rider mark names between one and 64 riders");
                    }
                    let _ = lead;
                }
                (K::BondBreaker(_), R::Bond(bond)) => {
                    if bond.level > crate::palw_capacity_s567_v1::PALW_BREAKER_TOP_LEVEL_V1 || bond.clean_epochs > 2 {
                        return bad("a bond's row holds a level on the ladder and at most two clean epochs");
                    }
                }
                (K::Breaker, R::Breaker(breaker)) => {
                    if breaker.level > crate::palw_capacity_s567_v1::PALW_BREAKER_TOP_LEVEL_V1
                        || breaker.clean_epochs > 2
                        || breaker.short_bonds.len() > crate::palw_capacity_s567_v1::PALW_BREAKER_K2_BONDS_V1
                        || breaker.short_bonds.iter().enumerate().any(|(i, bond)| breaker.short_bonds[..i].contains(bond))
                    {
                        return bad("the breaker's level, clean count or short bonds are out of bounds");
                    }
                }
                _ => return bad("the key and the row are of different kinds"),
            }
        }
        Ok(())
    }

    pub(crate) fn assert_bond_freezes_consistency_v1(&self) -> Result<(), PalwStateV2Error> {
        for (bond, freeze) in &self.bond_freezes {
            let bad = |why: &str| Err(PalwStateV2Error::CarriageInconsistent(format!("bond freeze {bond:?}: {why}")));
            let Some(record) = self.bonds.get(bond) else { return bad("its bond is not registered") };
            if freeze.final_ && record.collateral != 0 {
                return bad("a final freeze on a bond that still posts collateral (AG-2 takes it whole)");
            }
        }
        Ok(())
    }

    /// **ADR-0152 §4-ter.3 step 6: a held forfeit lives exactly as long as its claim can still be
    /// tried** — its claim held and not terminal (every terminal door drops it), a non-zero amount,
    /// one or two chunks, sorted and unique. Empty on every network but testnet-12.
    pub(crate) fn assert_held_forfeits_consistency_v1(&self) -> Result<(), PalwStateV2Error> {
        for ((claim_id, session_id), record) in &self.held_forfeits {
            let bad =
                |why: &str| Err(PalwStateV2Error::CarriageInconsistent(format!("held forfeit ({claim_id}, {session_id}): {why}")));
            if self.claims.get(claim_id).is_none_or(|claim| claim.phase.is_terminal()) {
                return bad("its claim is gone or terminal");
            }
            // The forfeit went into the challenger's `slashed` and nothing takes it out but its refund.
            if self.bonds.get(&record.challenger).is_none_or(|bond| record.amount > bond.slashed) {
                return bad("its amount is more than its challenger's bond has ever lost");
            }
            // A record whose pause is unspent has no non-pausing session of its challenger open on the
            // claim: the record's write converts one that predates it, and an opening under it pauses.
            if !record.paused && self.da_sessions.get(&(*claim_id, record.challenger)).is_some_and(|session| !session.accuser_is_seat)
            {
                return bad("its challenger's open demand does not pause the claim");
            }
            if record.amount == 0 || record.chunks.is_empty() || record.chunks.len() > 2 || !record.chunks.is_sorted_by(|a, b| a < b) {
                return bad("a zero amount, or chunks that are not one or two, sorted and unique");
            }
        }
        Ok(())
    }

    /// **ADR-0152 DA-1 / DA-2 (M3): the data-availability court's records agree with each other and
    /// with the claims** — what a carriage somebody else wrote must satisfy before it is believed.
    ///
    /// * Below `palw_rcore_plus` both DA maps are empty (no writer runs there).
    /// * Past it, no claim is `DefaultDisputed` and the held court's side maps are empty (DA-1: the
    ///   v1 sessions are never written there — the accused unit lives in the session, IMPL-12).
    /// * Every session and every record names a claim the state holds (retirement waits for the last
    ///   session and takes the record with it), and every session's claim has a record.
    /// * A record's open counts are exactly its sessions', split by `accuser_is_seat`, within DA-8's
    ///   caps, and `paused_since` is set exactly while a seat session is open (DA-5).
    /// * A session names one to four distinct units and its deadline is `opened + W_disclose`.
    pub(crate) fn assert_da_consistency_v1(&self, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
        use crate::palw_da_rcore_v1::{
            PALW_DA_DRAWN_UNITS_V1, PALW_DA_OPEN_NON_SEAT_PER_CLAIM_V1, PALW_DA_SESSIONS_PER_CLAIM_TOTAL_V1,
            PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1,
        };
        let bad = |why: String| Err(PalwStateV2Error::CarriageInconsistent(why));
        if params.rcore_plus_from_daa().is_none() {
            if !self.da_sessions.is_empty() || !self.da_claims.is_empty() {
                return bad("DA sessions or records on a network where palw_rcore_plus is dormant".into());
            }
            return Ok(());
        }
        // DA-1: the v1 sessions are never written past the fence — no claim is `DefaultDisputed` and
        // the held court's side maps stay empty (the accused unit lives in the session, IMPL-12).
        if let Some((id, _)) = self.claims.iter().find(|(_, claim)| matches!(claim.phase, PalwClaimPhaseV2::DefaultDisputed { .. })) {
            return bad(format!("claim {id} is DefaultDisputed past palw_rcore_plus (DA-1)"));
        }
        if !self.held_da_missing.is_empty() || !self.held_leaf_demands.is_empty() {
            return bad("the held court's side maps are written past palw_rcore_plus (DA-1)".into());
        }
        let window = palw_da_disclose_window_daa_v1(params);
        let mut counted: BTreeMap<Hash64, (u8, u8)> = BTreeMap::new();
        for ((claim_id, accuser), session) in &self.da_sessions {
            if !self.claims.contains_key(claim_id) {
                return bad(format!("a DA session names claim {claim_id}, which the state does not hold"));
            }
            if !self.da_claims.contains_key(claim_id) {
                return bad(format!("a DA session on claim {claim_id} has no DA record"));
            }
            let units: BTreeSet<_> = session.units.iter().collect();
            // Lane G14-L: a legacy held session (no draws: one unit) of more than one unit is a LANES session — only past both
            // legacy fences, at most 64 distinct held units, a reserved (seat-like) session, due between `opened + W_disclose` and the lane
            // bound (the join window, `W_disclose`, one carriage DAA a further lane). Every other session keeps DA-3's shape.
            let lanes = session.units.len() > 1
                && session.units.iter().all(|unit| matches!(unit, crate::palw_da_rcore_v1::PalwDaUnitV1::LegacyHeldV2(_)));
            if lanes {
                use crate::palw_legacy_parallel_v1::{PALW_LEGACY_LANES_PER_SESSION_V1, palw_legacy_lane_deadline_bound_v1};
                let due = session.opened_daa.saturating_add(window)..=palw_legacy_lane_deadline_bound_v1(session.opened_daa, window);
                if params.legacy_public_filer_from_daa().is_none()
                    || session.units.len() > PALW_LEGACY_LANES_PER_SESSION_V1 as usize
                    || units.len() != session.units.len()
                    || !session.accuser_is_seat
                    || !due.contains(&session.deadline_daa)
                {
                    return bad(format!(
                        "the lanes session of {accuser:?} on claim {claim_id} ({} units, due {}) is not a reserver's within its bounds",
                        session.units.len(),
                        session.deadline_daa
                    ));
                }
            } else {
                if session.units.is_empty() || session.units.len() > 1 + PALW_DA_DRAWN_UNITS_V1 || units.len() != session.units.len() {
                    return bad(format!("the DA session of {accuser:?} on claim {claim_id} names {} units", session.units.len()));
                }
                if session.opened_daa.checked_add(window) != Some(session.deadline_daa) {
                    return bad(format!("the DA session of {accuser:?} on claim {claim_id} is not due at opened + W_disclose"));
                }
            }
            let entry = counted.entry(*claim_id).or_insert((0, 0));
            if session.accuser_is_seat {
                entry.0 = entry.0.saturating_add(1);
            } else {
                entry.1 = entry.1.saturating_add(1);
            }
        }
        for (claim_id, record) in &self.da_claims {
            if !self.claims.contains_key(claim_id) {
                return bad(format!("a DA record names claim {claim_id}, which the state does not hold"));
            }
            let (seat, other) = counted.get(claim_id).copied().unwrap_or((0, 0));
            if (record.open_seat_sessions, record.open_other_sessions) != (seat, other) {
                return bad(format!(
                    "claim {claim_id}'s DA record counts ({}, {}) open sessions and holds ({seat}, {other})",
                    record.open_seat_sessions, record.open_other_sessions
                ));
            }
            if record.paused_since.is_some() != (seat > 0) {
                return bad(format!("claim {claim_id}'s DA pause does not match its open seat sessions (DA-5)"));
            }
            if other > PALW_DA_OPEN_NON_SEAT_PER_CLAIM_V1
                || record.opened_non_seat_total > PALW_DA_SESSIONS_PER_CLAIM_TOTAL_V1
                || record.opened_by_seat.values().any(|opened| *opened > PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1)
            {
                return bad(format!("claim {claim_id}'s DA record exceeds DA-8's caps"));
            }
        }
        Ok(())
    }

    /// The parameterized deadline check: with params in hand the exact deadline of every
    /// non-terminal claim is recomputable, and the index must match it entry for entry.
    pub fn assert_deadline_consistency(&self, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
        // **ADR-0152 DL-1, past `palw_rcore_plus`: one deadline function, and the index IS it.** Every
        // row of `palw_rcore_deadline_v1` is exact at rest — the licensed row's `max(floor, last)`
        // included (see `rebuild_deadline_index_v2`) — so the index is checked for equality, not
        // for membership above a floor. Below the fence the check is the one it always was.
        if params.rcore_plus_from_daa().is_some() {
            let last = self.last_point.map(|point| point.daa_score);
            let mut expected: BTreeSet<(u64, Hash64)> = BTreeSet::new();
            for (id, claim) in &self.claims {
                if let Some(at) = palw_rcore_deadline_v1(self, params, id, claim, last)? {
                    expected.insert((at, *id));
                }
            }
            if expected != self.deadlines {
                return Err(PalwStateV2Error::CarriageInconsistent("deadline index differs from DL-1's deadlines".into()));
            }
            return Ok(());
        }
        let mut expected: BTreeSet<(u64, Hash64)> = BTreeSet::new();
        for (id, claim) in &self.claims {
            if self.court_service_holds_retirement_v1(id, claim) { continue; }
            let open_courts = self.open_courts_by_claim.get(id).copied().unwrap_or(0);
            match claim.phase {
                PalwClaimPhaseV2::Provisional => {
                    expected.insert((palw_provisional_bind_deadline_v1(self, params, id, claim)?, *id));
                }
                PalwClaimPhaseV2::PanelBound { bound_daa } => {
                    expected.insert((
                        bound_daa
                            .checked_add(params.panel_bound_sweep_window_v1(
                                claim,
                                bound_daa,
                                params.receipt_window_for_claim_v1(
                                    self,
                                    &claim.class_id,
                                    crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim(claim),
                                    bound_daa,
                                ),
                            ))
                            .ok_or(PalwStateV2Error::Overflow("receipt deadline"))?,
                        *id,
                    ));
                }
                PalwClaimPhaseV2::ReceiptLicensed { .. } if open_courts > 0 || self.exec_v2_holds_final_v1(id) => {
                    // Final is gated by an open court or incomplete EXEC root on either native lifecycle.
                }
                PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => {
                    // The stored deadline may be LATER than licensed+window when a court cleared
                    // after the window had already passed (the re-arm rule). Membership by claim,
                    // with the stored daa at least the licensed floor, is the checkable fact.
                    // §4-quater V4: the floor is the one helper's (`max(L + wc, H)` past the fence).
                    let floor = palw_claim_final_floor_v1(self, params, id, claim, licensed_daa)?;
                    let stored = self
                        .deadlines
                        .iter()
                        .find(|(_, claim_id)| claim_id == id)
                        .ok_or_else(|| PalwStateV2Error::CarriageInconsistent(format!("claim {id} has no final deadline")))?;
                    if stored.0 < floor {
                        return Err(PalwStateV2Error::CarriageInconsistent(format!(
                            "claim {id} final deadline {} sits below its licensed floor {floor}",
                            stored.0
                        )));
                    }
                    expected.insert(*stored);
                }
                // ADR-0062 SA-3: the disclose deadline, derived from the record exactly as it was
                // armed. This is what makes "silence" a fold fact — the deadline is recomputed on
                // every branch from the claim alone, so two nodes cannot hold different ones.
                PalwClaimPhaseV2::DefaultDisputed { accused_daa, .. } => {
                    expected.insert((
                        accused_daa
                            .checked_add(palw_da_disclose_window_daa_v1(params))
                            .ok_or(PalwStateV2Error::Overflow("da disclose deadline"))?,
                        *id,
                    ));
                }
                // Audit C5's abandon hold and launch blockers §8's retirement: the two deadlines a
                // terminal record can own, both derived from the record exactly as they were
                // armed — which is what keeps the index rebuildable from the claims alone. Swept
                // holds leave no entry, and a swept retirement leaves no CLAIM, so a terminal
                // record that is still here owes at most one.
                PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. } => {
                    if let Some(at) = terminal_deadline_of(self, id, claim, params, self.last_point.as_ref())? {
                        expected.insert((at, *id));
                    }
                }
            }
        }
        if expected != self.deadlines {
            return Err(PalwStateV2Error::CarriageInconsistent("deadline index differs from the claims' recomputed deadlines".into()));
        }
        Ok(())
    }
}
