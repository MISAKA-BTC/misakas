//! **`Params`: activation methods** — moved out of `params.rs` (2026-10-13, a behaviour-preserving split: the methods are the same; each keeps the reach it had).

use super::*;

impl Params {
    /// `palw_court_model_scope_v1` is in force at `daa_score` (register D2).
    pub fn palw_court_model_scope_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_court_model_scope_v1.is_some_and(|f| f.is_active(daa_score))
    }

    /// `palw_public_evidence_reward_v1` is in force at `daa_score` (register N4).
    pub fn palw_public_evidence_reward_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_public_evidence_reward_v1.is_some_and(|f| f.is_active(daa_score))
    }

    /// `palw_claim_hard_deadline_v1` is in force at `daa_score` (register X3).
    pub fn palw_claim_hard_deadline_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_claim_hard_deadline_v1.is_some_and(|f| f.activation.is_active(daa_score))
    }

    /// Whether a claim accepted at `accepted_daa` is under F-E. `false` on every shipped preset.
    pub fn palw_capacity_escrow_active_at(&self, accepted_daa: u64) -> bool {
        self.palw_capacity_escrow_at_licence_fence().is_some_and(|fence| fence.is_active(accepted_daa))
    }

    /// Is [`Self::palw_gdn_key_heads_fence`] in force at `daa_score`?
    pub fn palw_gdn_key_heads_active_at(&self, daa_score: u64) -> bool {
        self.palw_gdn_key_heads_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Whether a gate evaluated at `daa_score` counts a resolved claim's lock against the whole
    /// collateral rather than the work ceiling. `false` on every shipped preset.
    pub fn palw_final_lock_full_collateral_active_at(&self, daa_score: u64) -> bool {
        self.palw_final_lock_full_collateral_fence().is_some_and(|fence| fence.is_active(daa_score))
    }

    /// Whether a `Valid` seat's lock dated at `daa_score` gets the shortened post-`Final` life
    /// ([`crate::palw_state_v2::PALW_FINAL_LOCK_LIFE_DAA_V1`]) rather than `window_court`. `false` on
    /// every shipped preset.
    pub fn palw_final_lock_life_active_at(&self, daa_score: u64) -> bool {
        self.palw_final_lock_life_fence().is_some_and(|fence| fence.is_active(daa_score))
    }

    /// Whether a block at `daa_score` is at or past lane F2-lock's height: the crossing block re-dates
    /// the long post-`Final` seat locks, and a `Final` there dates its locks to exactly `F + 1,000`.
    /// `false` on every shipped preset.
    pub fn palw_final_lock_life_retro_active_at(&self, daa_score: u64) -> bool {
        self.palw_final_lock_life_retro_fence().is_some_and(|fence| fence.is_active(daa_score))
    }

    /// Is ADR-0084 U-08's court ladder in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_court_ladder_active_at(&self, daa_score: u64) -> bool {
        matches!(self.palw_court_ladder_fence(), Some(fence) if fence.is_active(daa_score))
    }

    /// Is ADR-0087's model market in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_model_market_active_at(&self, daa_score: u64) -> bool {
        matches!(self.palw_model_market_fence(), Some(fence) if fence.is_active(daa_score))
    }

    pub fn palw_class_semantics_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_class_semantics_v1_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_court_commitment_scope_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_court_commitment_scope_v1_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_staged_public_court_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_staged_public_court_v1_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_staged_tir_route_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_staged_tir_route_v1_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_staged_float_constraint_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_staged_float_constraint_v1_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_model_immutable_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_immutable_v1_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Is ADR-0095's membership in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_model_benefits_active_at(&self, daa_score: u64) -> bool {
        matches!(self.palw_model_benefits_fence(), Some(fence) if fence.is_active(daa_score))
    }

    /// Is ADR-0114's five-percent leg in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_model_leg_v2_active_at(&self, daa_score: u64) -> bool {
        matches!(self.palw_model_leg_v2_fence(), Some(fence) if fence.is_active(daa_score))
    }

    /// Is ADR-0120's one-million-MSK floor in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_model_seed_v2_active_at(&self, daa_score: u64) -> bool {
        matches!(self.palw_model_seed_v2_fence(), Some(fence) if fence.is_active(daa_score))
    }

    /// Is ADR-0162's virtual reserve in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_model_virtual_v1_active_at(&self, daa_score: u64) -> bool {
        matches!(self.palw_model_virtual_v1_fence(), Some(fence) if fence.is_active(daa_score))
    }

    /// Is ADR-0088's model registry in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_model_lines_active_at(&self, daa_score: u64) -> bool {
        matches!(self.palw_model_lines_fence(), Some(fence) if fence.is_active(daa_score))
    }

    /// Is ADR-0089's EVM face in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_model_evm_active_at(&self, daa_score: u64) -> bool {
        matches!(self.palw_model_evm_fence(), Some(fence) if fence.is_active(daa_score))
    }

    /// Whether a claim ACCEPTED at `accepted_daa` is accounted under the weight cap. The fold reads
    /// the V2 bundle's mirror ([`crate::palw_state_v2::PalwStateParamsV2::capacity_weight_cap_applies_at`]),
    /// which `validate_palw_v2` holds equal to this.
    pub fn palw_capacity_weight_cap_active_at(&self, accepted_daa: u64) -> bool {
        self.palw_capacity_weight_cap_fence().is_some_and(|fence| fence.is_active(accepted_daa))
    }

    /// Whether a transaction in a block at `daa_score` must bind every model sink it pays. `false`
    /// on every shipped preset: the fence is dormant until an operator arms it.
    pub fn palw_model_sink_bound_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_sink_bound_fence().is_some_and(|fence| fence.is_active(daa_score))
    }

    /// Whether a block at `daa_score` may carry a batch licence (and a false-Valid filing a
    /// `Windowed` receipt). `false` on every shipped preset.
    pub fn palw_capacity_batch_licence_active_at(&self, daa_score: u64) -> bool {
        self.palw_capacity_batch_licence_fence().is_some_and(|fence| fence.is_active(daa_score))
    }

    /// Whether room v2 (the measured k = 2 capacity, the stake share, the floor room) judges a block
    /// at `daa_score`. `false` on every shipped preset.
    pub fn palw_capacity_verify_room_active_at(&self, daa_score: u64) -> bool {
        self.palw_capacity_verify_room_fence().is_some_and(|fence| fence.is_active(daa_score))
    }

    /// Is ADR-0082's k-ary court in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_kary_court_active_at(&self, daa_score: u64) -> bool {
        self.palw_kary_court_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Is per-model finite court-window admission in force at `daa_score`?
    pub fn palw_model_court_window_active_at(&self, daa_score: u64) -> bool {
        self.palw_model_court_window.is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_court_responder_coverage_active_at(&self, daa_score: u64) -> bool {
        self.palw_court_responder_coverage_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Are ADR-0082's decode rules in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_fp_decode_rules_active_at(&self, daa_score: u64) -> bool {
        self.palw_fp_decode_rules_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Is ADR-0145's derived free-prompt work in force at `daa_score`? `false` on every shipped
    /// preset.
    pub fn palw_fp_derived_work_active_at(&self, daa_score: u64) -> bool {
        self.palw_fp_derived_work_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Is ADR-0096's decode constraint in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_fp_decode_constraint_active_at(&self, daa_score: u64) -> bool {
        self.palw_fp_decode_constraint_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Is ADR-0099's shard court in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_shard_court_active_at(&self, daa_score: u64) -> bool {
        self.palw_shard_court_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_shard_licensing_active_at(&self, daa_score: u64) -> bool {
        self.palw_shard_licensing_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_panel_economy_active_at(&self, daa_score: u64) -> bool {
        self.palw_panel_economy_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_work_priced_reward_active_at(&self, daa_score: u64) -> bool {
        self.palw_work_priced_reward_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_token_lift_active_at(&self, daa_score: u64) -> bool {
        self.palw_token_lift_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_fused_dissectable_active_at(&self, daa_score: u64) -> bool {
        self.palw_fused_dissectable_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_attn_anchored_root_active_at(&self, daa_score: u64) -> bool {
        self.palw_attn_anchored_root_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_held_context_active_at(&self, daa_score: u64) -> bool {
        self.palw_held_context_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Is an attempt drawn at `daa_score` a one-forward draw (ADR-0117)? Asked with the attempt's
    /// OWN block's DAA score, by the producer that builds it and the seat that replays it.
    pub fn palw_prefill_draw_active_at(&self, daa_score: u64) -> bool {
        self.palw_prefill_draw_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_audit_2026_09_11_active_at(&self, daa_score: u64) -> bool {
        self.palw_audit_2026_09_11_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_audit_2026_09_11_deep_active_at(&self, daa_score: u64) -> bool {
        self.palw_audit_2026_09_11_deep_fence().is_some_and(|f| f.is_active(daa_score))
    }

    pub fn palw_audit_2026_09_23_active_at(&self, daa_score: u64) -> bool {
        self.palw_audit_2026_09_23_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Whether R-core+ is in force at `daa_score`. `false` on every preset but testnet-12. The v22
    /// skeleton arms nothing behind it yet.
    pub fn palw_rcore_plus_active_at(&self, daa_score: u64) -> bool {
        self.palw_rcore_plus_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Whether a claim whose ANCHOR block sits at `anchor_daa` draws its panel from the anchor
    /// attempt's execution commitment ([`crate::palw_panel_v2::palw_panel_draw_seed_v1`]). `false` on
    /// every shipped preset: the fence is dormant until an operator arms it.
    pub fn palw_panel_seed_execution_active_at(&self, anchor_daa: u64) -> bool {
        self.palw_panel_seed_execution_fence().is_some_and(|f| f.is_active(anchor_daa))
    }

    /// Whether a candidate anchor block at `anchor_daa` is under lane A's rule. `false` on every
    /// shipped preset: the fence is dormant until an operator arms it.
    pub fn palw_operator_anchor_active_at(&self, anchor_daa: u64) -> bool {
        matches!(self.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_))
            && self.palw_operator_anchor.as_ref().is_some_and(|rule| rule.activation.is_active(anchor_daa))
    }

    /// Whether F-L is in force at `daa`. `false` on every shipped preset.
    pub fn palw_capacity_aggregate_liability_active_at(&self, daa: u64) -> bool {
        self.palw_capacity_step_at_v1(daa).is_some()
    }

    /// Whether class-derived verification deadlines are in force at `daa_score`. `false` on every
    /// preset but testnet-12. The fold reads the bundle's mirror
    /// (`PalwStateParamsV2::class_verify_deadline_active_at`), which `validate_palw_v2` holds equal.
    pub fn palw_class_verify_deadline_active_at(&self, daa_score: u64) -> bool {
        self.palw_class_verify_deadline_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Whether a chain block at `daa_score` whose own attempt is at its bond's exposure ceiling may
    /// still bind the claims it anchors (a binder). `false` on every shipped preset.
    pub fn palw_anchor_at_ceiling_active_at(&self, daa_score: u64) -> bool {
        self.palw_anchor_at_ceiling_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Whether a block at `daa_score` applies its mergeset parents-first (and, a round block at
    /// `daa_score`, carries no EVM payload). `false` on every shipped preset.
    pub fn palw_lane_accept_parents_first_active_at(&self, daa_score: u64) -> bool {
        self.palw_lane_accept_parents_first_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Whether a false `Valid` is judged by `palw_check_panel_false_valid_v2` at `daa_score`
    /// (`PalwTransitionExtrasV1::offence_attribution_active`): the V1 kind refused, the V2 kind
    /// admitted. `false` on every preset but testnet-12.
    pub fn palw_offence_attribution_active_at(&self, daa_score: u64) -> bool {
        self.palw_offence_attribution_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// kaspa-pq: `true` when PQ-only enforcement is active at `daa_score`.
    /// In `Consensus` mode this gates legacy secp256k1 signature opcodes,
    /// P2SH, and non-ML-DSA-87 script classes at the consensus and script-
    /// engine level. See ADR-0019 / docs/design/pq/kaspa-pq-design-mldsa87.md.
    #[inline]
    #[must_use]
    pub fn is_pq_active(&self, daa_score: u64) -> bool {
        matches!(self.pq_enforcement, PqEnforcementMode::Consensus) && daa_score >= self.pq_activation_daa_score
    }

    /// kaspa-pq Selected-Parent EVM Lane (ADR-0020): `true` when the EVM
    /// execution lane is active at `daa_score` on this network. Below the fence
    /// (the default `u64::MAX` for non-EVM nets) the `evm_payload` must be empty.
    #[inline]
    #[must_use]
    pub fn is_evm_active(&self, daa_score: u64) -> bool {
        daa_score >= self.evm_activation_daa_score
    }

    /// kaspa-pq EVM Lane: `true` when the gas-pool v2 executor (the liveness fix) is
    /// active at `daa_score`. Below the fence (the default `u64::MAX`) the v1 strict
    /// declared-gas prefix-take executes. See `evm_gas_pool_v2_activation_daa_score`.
    #[inline]
    #[must_use]
    pub fn is_evm_gas_pool_v2_active(&self, daa_score: u64) -> bool {
        daa_score >= self.evm_gas_pool_v2_activation_daa_score
    }
}
