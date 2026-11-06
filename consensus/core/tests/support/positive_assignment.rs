//! Test-only finite Assignment policy. This fixture grants no native readiness.
use kaspa_consensus_core::config::params::PositiveCourtFenceV2;
use misaka_palw_positive_contract::{
    assignment::{
        AssignmentModeV2,
        policy::{AssignmentLiveCapsV2, AssignmentPolicyV2, canonical_assignment_derivation_id},
    },
    identity::Digest,
};

fn d(n: u8) -> Digest {
    Digest([n; 64])
}

pub fn policy(positive: &PositiveCourtFenceV2) -> AssignmentPolicyV2 {
    AssignmentPolicyV2 {
        version: 2,
        revision: 1,
        mode: AssignmentModeV2::CanonicalTirAssignmentV1,
        positive_policy: Digest(positive.policy.root().as_bytes()),
        derivation_policy: canonical_assignment_derivation_id(),
        source_policy: d(11),
        assignment_fee_schedule: d(41),
        case_fee_schedule: d(42),
        reserve_schedule: d(12),
        physical_service_profile: d(43),
        cancellation_policy: d(18),
        live: AssignmentLiveCapsV2 { global: 10, per_poster: 2, per_worker: 2, per_prover: 2, per_class: 4 },
        max_frame_bytes: 32_000,
        max_program_bytes: 65_536,
        max_layout_entries: 1000,
        max_source_catalog_entries: 1000,
        max_role_candidates: 5,
        max_prover_joins: 2,
        max_history_frames: 100,
        max_history_bytes: 100_000,
        max_retained_bytes: 200_000,
        max_replay_bytes: 300_000,
        max_replay_work: 1_000_000,
        max_heap_bytes: 1_000_000,
        accept_window_daa: 100,
        result_window_daa: 100,
        build_window_daa: 100,
        check_window_daa: 100,
        fraud_tail_daa: 100,
        refund_window_daa: 100,
        retention_window_daa: 1000,
    }
}
