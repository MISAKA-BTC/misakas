//! **`Params`: mirror methods** — moved out of `params.rs` (2026-10-13, a behaviour-preserving split: the methods are the same; each keeps the reach it had).

use super::*;

impl Params {
    /// **F-E's mirror**: the `#[borsh(skip)]` copy on `PalwStateParamsV2` the fold reads
    /// (`capacity_escrow_active_at`, keyed on a claim's `accepted_daa`) — the ledger's load-time
    /// re-derivation and every view have no transition extras, only the params they are handed.
    /// Written here and nowhere else; `None` where the fence is not armed. Call it wherever the fence
    /// is set on an assembled ruleset (the post-launch list's entry does); `validate_palw_v2` refuses a
    /// ruleset whose copy disagrees, so a missed call is a startup refusal, not a fold on the old rules.
    pub fn sync_palw_capacity_escrow(&mut self) {
        let from_daa =
            self.palw_capacity_escrow_at_licence.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_capacity_escrow_from_daa(from_daa);
        }
        // Rewriting the bundle must not leave the G14 route mirror behind the fences.
        self.sync_palw_g14_gates_v1();
    }

    /// ADR-0132 §7.6: arms (or clears) the short challenge window's fence AND the V2 bundle's copy
    /// of its height in one move, so the two cannot drift; `validate_palw_v2` refuses them apart.
    /// ADR-0132 §7.6: re-mirrors the fence's height into the V2 bundle (a bundle assembled after
    /// the fence was armed starts without the copy).
    pub fn sync_palw_short_challenge_window(&mut self) {
        self.set_palw_short_challenge_window(self.palw_short_challenge_window);
    }

    /// **Option A: mirror `palw_audit_2026_09_23`'s height into the V2 bundle** as the height from
    /// which a claim's escrow is reserved on its bond. Called after a bundle is assembled over a
    /// preset that armed the fence (the assembly builds a fresh state-params record); `validate_palw_v2`
    /// refuses a ruleset where the two disagree, so a missed call is a startup refusal, not a quietly
    /// under-reserved ledger.
    pub fn sync_palw_escrow_backed_exposure(&mut self) {
        let from_daa = self.palw_audit_2026_09_23.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_escrow_backed_exposure_from_daa(from_daa);
        }
    }

    /// **ADR-0152 R-core+: mirror `palw_rcore_plus`, the withdrawal delay the processor enforces and
    /// C7's list into the V2 bundle** — the three `#[borsh(skip)]` copies on `PalwStateParamsV2`, in
    /// one setter so they cannot drift (IMPL-6, ADR row 26). The delay is
    /// `palw_v2_bond_withdrawal_delay_at_v1(bundle, palw_da_court, 0)`: the bond's delay plus the DA
    /// lattice, not `bundle.bond.withdrawal_delay_daa()`. Where the fence is not armed all three are
    /// the dormant values (`None`, 0, empty). Called after a bundle is assembled over a preset that
    /// armed the fence, beside `sync_palw_escrow_backed_exposure`; `validate_palw_v2` refuses a
    /// ruleset whose copies disagree, so a missed call is a startup refusal.
    pub fn sync_palw_rcore_plus(&mut self) {
        let from_daa = self.palw_rcore_plus.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        let da_court = self.palw_da_court;
        let classes = if from_daa.is_some() { self.palw_rcore_conservative_classes.to_vec() } else { Vec::new() };
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            let delay = if from_daa.is_some() { palw_v2_bond_withdrawal_delay_at_v1(bundle, da_court, 0) } else { 0 };
            bundle.state = bundle.state.clone().with_rcore_plus_mirrors(from_daa, delay, classes);
        }
        // Every caller that re-mirrors R-core+ after moving a fence re-mirrors this one too: it reads
        // `palw_offence_attribution`, R-core+'s own prerequisite.
        self.sync_palw_held_answerability();
        // Lane V02 (post-launch): the fence that splits R-core+'s one ledger rides the same re-mirror.
        self.sync_palw_final_lock_full_collateral();
        // Lane V02's lock life (post-launch): the shortened lock life rides R-core+'s re-mirror — its
        // vesting rows are what cover `E` while a seat's lock is released early, so the two are armed
        // together.
        self.sync_palw_final_lock_life();
        // Lane F2-lock (post-launch): the lock life applied retroactively rides the same re-mirror, beside
        // the lane V02 fence it extends.
        self.sync_palw_final_lock_life_retro();
        // FP Job V4 (RFC-0001 §A, lane fp-sampler): ADR-0082 D10/D11's height rides the same re-mirror,
        // so every site that re-assembles a bundle carries it.
        self.sync_palw_fp_decode_rules();
    }

    // ---- FP Job V4 (RFC-0001 §A, 2026-09-27): ADR-0082 D10 + D11 + "V4 from here" ----

    /// **Mirrors [`Self::palw_fp_decode_rules`] into the V2 bundle's state params** — the
    /// borsh-skipped copy the transition reads (`PalwStateParamsV2::fp_decode_rules_at`), because the
    /// fold holds only the bundle. `None` where the fence is not armed. Called by
    /// [`Self::sync_palw_rcore_plus`] and by the flag-day entry
    /// [`PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1`]; `validate_palw_v2` refuses a ruleset whose copy
    /// disagrees, so a missed call is a startup refusal, not a quiet dormant rule.
    pub fn sync_palw_fp_decode_rules(&mut self) {
        let from_daa = self.palw_fp_decode_rules.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_fp_decode_rules_v1(from_daa);
        }
    }

    /// **Lane V02's mirror**: the `#[borsh(skip)]` copy on `PalwStateParamsV2` every reader of the
    /// work gate reads (`final_lock_full_collateral_active_at`) — the fold, admission, the draw's seat
    /// filter and the producer's facts hold only the state params. Written here and nowhere else;
    /// `None` where the fence is not armed. Called by `sync_palw_rcore_plus` (so every site that
    /// assembles or re-fences a bundle re-mirrors it) and callable alone; `validate_palw_v2` refuses a
    /// ruleset whose copy disagrees, so a missed call is a startup refusal.
    pub fn sync_palw_final_lock_full_collateral(&mut self) {
        let from_daa =
            self.palw_final_lock_full_collateral.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_final_lock_full_collateral_from_daa(from_daa);
        }
    }

    /// **Lane V02's mirror**: the `#[borsh(skip)]` copy on `PalwStateParamsV2` the state machine reads
    /// (`final_lock_life_at`) — the fold holds only the state params, never the outer `Params`.
    /// Written here and nowhere else; `None` where the fence is not armed. Called by
    /// `sync_palw_rcore_plus` (so every site that assembles or re-fences a bundle re-mirrors it) and
    /// callable alone; `validate_palw_v2` refuses a ruleset whose copy disagrees, so a missed call is
    /// a startup refusal.
    pub fn sync_palw_final_lock_life(&mut self) {
        let from_daa = self.palw_final_lock_life.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_final_lock_life_from_daa(from_daa);
        }
    }

    /// **Lane F2-lock's mirror**: the `#[borsh(skip)]` copy on `PalwStateParamsV2` the fold reads
    /// (`final_lock_life_retro_from_daa`) — the fold holds only the state params, never the outer
    /// `Params`. Written here and nowhere else; `None` where the fence is not armed. Called by
    /// `sync_palw_rcore_plus` (so every site that assembles or re-fences a bundle re-mirrors it) and
    /// callable alone; `validate_palw_v2` refuses a ruleset whose copy disagrees, so a missed call is a
    /// startup refusal.
    pub fn sync_palw_final_lock_life_retro(&mut self) {
        let from_daa =
            self.palw_final_lock_life_retro.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_final_lock_life_retro_from_daa(from_daa);
        }
    }

    /// **ADR-0152 §4-ter (A-held): mirror [`Self::palw_held_unanswerable_classes_v1`] into the V2
    /// bundle** — the `#[borsh(skip)]` copy on `PalwStateParamsV2` the fold's mercy and one-move
    /// refusal read (the fold holds no `Params`). Called by `sync_palw_rcore_plus`, so every site
    /// that assembles or re-fences a bundle re-mirrors it; `validate_palw_v2` refuses a ruleset whose
    /// copy disagrees, so a missed call is a startup refusal, never a 2M class read as answerable.
    pub fn sync_palw_held_answerability(&mut self) {
        let classes = self.palw_held_unanswerable_classes_v1();
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_held_unanswerable_classes(classes);
        }
    }

    /// **ADR-0152 §4-quater: mirror `palw_class_verify_deadline` and its measured rows into the V2
    /// bundle** — the two `#[borsh(skip)]` copies on `PalwStateParamsV2` the fold, the rebuild and
    /// every view read, in one setter so they cannot drift. Where the fence is not armed both are
    /// the dormant values (`None`, empty). Called where a bundle is assembled over a preset that
    /// armed the fence, beside `sync_palw_rcore_plus`; `validate_palw_v2` refuses a ruleset whose
    /// copies disagree, so a missed call is a startup refusal.
    pub fn sync_palw_class_verify_deadline(&mut self) {
        let from_daa = self.palw_class_verify_deadline.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        let rows = if from_daa.is_some() { self.palw_class_verify_rows.to_vec() } else { Vec::new() };
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_class_verify_deadline(from_daa, rows);
        }
    }

    /// **The readiness-V2 horizon's mirror** (user decision 2026-09-25, readiness capacity option
    /// (a)): the `#[borsh(skip)]` copy on `PalwStateParamsV2` the registry fold's globals are built
    /// from (`palw_registry_globals_of_bundle_v1`), written here and nowhere else. `None` where the
    /// fence is not armed. Called where a bundle is assembled over a preset that armed the fence,
    /// beside `sync_palw_class_verify_deadline`; `validate_palw_v2` refuses a ruleset whose copy
    /// disagrees, so a missed call is a startup refusal rather than a fold counting by eight spans.
    pub fn sync_palw_readiness_v2_max_age_spans(&mut self) {
        let spans = self
            .palw_readiness_v2_max_age_spans
            .filter(|horizon| horizon.activation != ForkActivation::never())
            .map(|horizon| horizon.max_age_spans);
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_readiness_v2_max_age_spans(spans);
        }
    }

    // ---- lane F1: registry resilience (V03/V05) — the mirror and what the fence refuses ----

    /// **The registry-resilience fence's mirror** (lane F1): the `#[borsh(skip)]` copy on
    /// `PalwStateParamsV2` the fold reads (`registry_resilience_active_at`) — the fold's load-time
    /// re-derivations (a retried claim's bind deadline) have no transition extras, only the params
    /// they are handed. Written here and nowhere else; `None` where the fence is not armed. Call it
    /// wherever the fence is set on an assembled ruleset; `validate_palw_v2` refuses a ruleset whose
    /// copy disagrees, so a missed call is a startup refusal rather than a fold on the old rules.
    pub fn sync_palw_registry_resilience(&mut self) {
        let from_daa = self.palw_registry_resilience.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_registry_resilience_from_daa(from_daa);
        }
    }

    // ---- lane F2: the floor-refusal retry — the mirror and what the fence refuses ----

    /// **The floor-refusal retry's mirror** (lane F2): the `#[borsh(skip)]` copy on `PalwStateParamsV2`
    /// the fold reads (`floor_refusal_retry_active_at`) — a retried claim's bind deadline is re-derived
    /// at load, where only the params the fold is handed are at hand. Written here and nowhere else;
    /// `None` where the fence is not armed. Call it wherever the fence is set on an assembled ruleset;
    /// `validate_palw_v2` refuses a ruleset whose copy disagrees, so a missed call is a startup refusal
    /// rather than a fold on the old rules.
    pub fn sync_palw_floor_refusal_retry(&mut self) {
        let from_daa = self.palw_floor_refusal_retry.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_floor_refusal_retry_from_daa(from_daa);
        }
    }

    /// **Arm ADR-0133 §11.3's class-derived receipt deadline, fence and bundle together.**
    ///
    /// The bundle's copy carries the lane's schedule span beside the height, because the window a
    /// class derives is counted in SPANS and the deadline has to be stated in DAA. Call it after
    /// the lane is configured — `validate_palw_v2` refuses a schedule that arms this fence
    /// without one, so a call in the wrong order is a startup refusal rather than a network that
    /// measures its deadlines against the fallback span.
    /// Re-mirror ADR-0133 §11.3's height and span onto a bundle assembled after the fence was
    /// armed — `sync_palw_short_challenge_window`'s job for §11.3's rule.
    pub fn sync_palw_class_receipt_window(&mut self) {
        self.set_palw_class_receipt_window(self.palw_class_receipt_window);
    }

    /// **Mirror F-W's height into the V2 bundle** — the `#[borsh(skip)]` copy the fold and its load-time
    /// re-derivation read (`capacity_weight_cap_from_daa`). Called by the post-launch list's `set` and by
    /// the assembly (a base that set the fence before it had a bundle is re-mirrored there);
    /// `validate_palw_v2` refuses the two apart.
    pub fn sync_palw_capacity_weight_cap(&mut self) {
        let from_daa = self.palw_capacity_weight_cap.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_capacity_weight_cap_from_daa(from_daa);
        }
        self.sync_palw_g14_gates_v1();
    }

    /// **ADR-0164: the fold's mirror of F-EM, F-M1 and F-K** — their heights on the `#[borsh(skip)]` copy of `PalwStateParamsV2`.
    /// Written here and nowhere else; call it wherever one of the three is set on an assembled ruleset
    /// (`validate_palw_v2` refuses a ruleset whose copy disagrees, so a missed call is a startup refusal).
    pub fn sync_palw_capacity_s567(&mut self) {
        let height =
            |fence: Option<ForkActivation>| fence.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        let (emission, riders, breaker) = (
            height(self.palw_capacity_emission_budget),
            height(self.palw_capacity_multi_claim),
            height(self.palw_capacity_rho_breaker),
        );
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_capacity_s567_mirror(emission, riders, breaker);
        }
        self.sync_palw_g14_gates_v1();
    }

    /// **The fold's mirror of the verification term** (int-11): its height on the `#[borsh(skip)]` copy of
    /// `PalwStateParamsV2`. Written here and nowhere else; call it wherever the fence is set on an assembled ruleset.
    pub fn sync_palw_capacity_network_verify(&mut self) {
        let at = self.palw_capacity_network_verify.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_capacity_network_verify_mirror(at);
        }
    }

    /// **The fold's mirror of [`Self::palw_panel_unavailable_expiry`]**: its height on the `#[borsh(skip)]` copy of `PalwStateParamsV2`. Written
    /// here and nowhere else; call it wherever the fence is set on an assembled ruleset.
    pub fn sync_palw_panel_unavailable_expiry(&mut self) {
        let at = self.palw_panel_unavailable_expiry.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_panel_unavailable_expiry_mirror(at);
        }
    }

    /// **The fold's mirror of [`Self::palw_panel_fast_switch`]**: its height on the `#[borsh(skip)]` copy of `PalwStateParamsV2`. Written
    /// here and nowhere else; call it wherever the fence is set on an assembled ruleset.
    pub fn sync_palw_panel_fast_switch(&mut self) {
        let at = self.palw_panel_fast_switch.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_panel_fast_switch_mirror(at);
        }
    }

    /// **The fold's mirror of [`Self::palw_seat_availability`]**: its height on the `#[borsh(skip)]` copy of `PalwStateParamsV2`. Written
    /// here and nowhere else; call it wherever the fence is set on an assembled ruleset.
    pub fn sync_palw_seat_availability(&mut self) {
        let at = self.palw_seat_availability.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_seat_availability_mirror(at);
        }
    }

    /// **The fold's mirrors of F-Q and F-S** (ADR-0160 stage 2): F-Q's height with the operator bonds
    /// its audit pool draws from (`palw_operator_anchor`'s list — empty where lane A is not armed, and
    /// then `validate_palw_v2` refuses F-Q), and F-S's height, on the `#[borsh(skip)]` copies of
    /// `PalwStateParamsV2`. Written here and nowhere else; call it wherever either fence (or lane A) is
    /// set on an assembled ruleset.
    pub fn sync_palw_capacity_stage2(&mut self) {
        let at =
            |fence: Option<ForkActivation>| fence.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        let (audit, slots) = (at(self.palw_capacity_audit_door), at(self.palw_capacity_issuance_slots));
        let network = at(self.palw_capacity_network_room);
        // The operator bonds: F-Q's pool and F-N's ring both read them.
        let operators = match (audit.is_some() || network.is_some(), &self.palw_operator_anchor) {
            (true, Some(anchor)) => anchor.operators.clone(),
            _ => Vec::new(),
        };
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            let anchor_delay = bundle.panel.anchor_delay();
            bundle.state = bundle
                .state
                .clone()
                .with_capacity_stage2_mirrors(audit, operators, slots)
                .with_capacity_network_mirror(network, anchor_delay);
        }
        self.sync_palw_g14_gates_v1();
    }

    /// **The fold's mirrors of F-B and F-R** (ADR-0160 §6.2): the `#[borsh(skip)]` copies on
    /// `PalwStateParamsV2` the fold, its load-time re-derivations and every outside reader take
    /// (`capacity_batch_active_at`, `capacity_room_active_at`). Written here and nowhere else; `None`
    /// where a fence is not armed. Call it wherever either fence is set on an assembled ruleset;
    /// `validate_palw_v2` refuses a ruleset whose copies disagree.
    pub fn sync_palw_capacity_verify(&mut self) {
        let at =
            |fence: Option<ForkActivation>| fence.filter(|fence| *fence != ForkActivation::never()).map(|fence| fence.daa_score());
        let (batch, room) = (at(self.palw_capacity_batch_licence), at(self.palw_capacity_verify_room));
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_capacity_verify_mirrors(room, batch);
        }
        self.sync_palw_g14_gates_v1();
    }

    /// **F-L's mirror** (ADR-0160 lane liab): the `#[borsh(skip)]` copy on `PalwStateParamsV2` the fold
    /// reads — the conviction funnel, the seat pricing, and every view that holds no transition extras.
    /// Written here and nowhere else; `None` where the fence is not armed (or `never()`). Call it
    /// wherever the fence is set on an assembled ruleset; `validate_palw_v2` refuses a ruleset whose
    /// copy disagrees, so a missed call is a startup refusal rather than a fold on the old rules.
    pub fn sync_palw_capacity_liability(&mut self) {
        let value = self.palw_capacity_aggregate_liability.clone().filter(|value| value.activation != ForkActivation::never());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_capacity_liability(value);
        }
        self.sync_palw_g14_gates_v1();
    }
}
