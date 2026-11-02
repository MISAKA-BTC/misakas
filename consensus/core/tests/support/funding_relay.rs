//! Explicit private test values only. No activation or native custody.
use kaspa_consensus_core::config::params::{ForkActivation, PositiveFundingRelayFenceV2};
pub fn relay_fence(at: u64) -> PositiveFundingRelayFenceV2 {
    use misaka_palw_positive_contract::assignment::funding_relay_policy::*;
    let fees = FundingRelayFeeScheduleV2 {
        version: 2,
        revision: 1,
        admission: 1,
        per_signature: 1,
        per_frame_byte: 1,
        per_retained_byte_daa: 1,
        per_database_byte: 1,
        per_replay_read: 1,
        per_replay_work: 1,
        retirement: 1,
    };
    let physical = FundingRelayPhysicalProfileV2 {
        version: 2,
        revision: 1,
        max_frame_bytes: 64_000,
        max_decoded_bytes: 64_000,
        max_heap_bytes: 128_000,
        max_retained_bytes: 64_000,
        max_database_bytes: 128_000,
        max_batch_bytes: 256_000,
        max_replay_reads: 100,
        max_replay_work: 1_000_000,
        max_signatures: 2,
    };
    let policy = FundingRelayPolicyV2 {
        version: 2,
        revision: 1,
        max_pending_global: 100,
        max_pending_per_poster: 10,
        max_pending_per_prover: 10,
        max_account_candidates: 3,
        max_account_continuations: 5,
        max_stored_rows: 1000,
        max_declared_amount: 1_000_000,
        register_window_daa: 100,
        funding_window_daa: 300,
        custody_window_daa: 1000,
        rollback_margin_daa: 10,
        metadata_fee_schedule: fees.id(),
        physical_custody_profile: physical.id(),
    };
    PositiveFundingRelayFenceV2 { activation: ForkActivation::new(at), policy, fees, physical }
}
