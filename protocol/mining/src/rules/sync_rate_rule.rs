use std::{
    collections::VecDeque,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;
use kaspa_core::{time::unix_now, trace, warn};

use crate::rule_engine::SNAPSHOT_INTERVAL;

use super::{ExtraData, mining_rule::MiningRule};

// within a 5 minute period, we expect sync rate less sensitive to sudden changes
// but we use a lower threshold anyway because we want the warns to be less frequent
const SYNC_RATE_THRESHOLD: f64 = 0.50;
// number of samples you expect in a 5 minute interval, sampled every 10s
const SYNC_RATE_WINDOW_MAX_SIZE: usize = 5 * 60 / (SNAPSHOT_INTERVAL as usize);
// number of samples required before considering this rule. This allows using the sync rate rule
// even before the full window size is reached. Represents the number of samples in 1 minute
const SYNC_RATE_WINDOW_MIN_THRESHOLD: usize = 60 / (SNAPSHOT_INTERVAL as usize);

pub struct SyncRateRule {
    pub use_sync_rate_rule: Arc<AtomicBool>,
    /// `(received blocks, elapsed milliseconds)` per sample. The elapsed TIME is kept, not the
    /// expected block count derived from it: dividing per sample truncates, and once the block
    /// interval exceeds the 10 s sampling interval every sample truncates to zero. See
    /// `expected_blocks_in_window`.
    sync_rate_samples: RwLock<VecDeque<(u64, u64)>>,
    total_elapsed_ms: AtomicU64,
    total_received_blocks: AtomicU64,
}

impl SyncRateRule {
    pub fn new(use_sync_rate_rule: Arc<AtomicBool>) -> Self {
        Self {
            use_sync_rate_rule,
            sync_rate_samples: RwLock::new(VecDeque::new()),
            total_elapsed_ms: AtomicU64::new(0),
            total_received_blocks: AtomicU64::new(0),
        }
    }

    /// Adds current observation of received and expected blocks to the sample window, and removes
    /// old samples. Returns true if there are enough samples in the window to start triggering the
    /// sync rate rule.
    fn update_sync_rate_window(&self, received_blocks: u64, elapsed_ms: u64) -> bool {
        self.total_received_blocks.fetch_add(received_blocks, Ordering::SeqCst);
        self.total_elapsed_ms.fetch_add(elapsed_ms, Ordering::SeqCst);

        let mut samples = self.sync_rate_samples.write().unwrap();

        samples.push_back((received_blocks, elapsed_ms));

        // Remove old samples. Usually is a single op after the window is full per 10s:
        while samples.len() > SYNC_RATE_WINDOW_MAX_SIZE {
            let (old_received_blocks, old_elapsed_ms) = samples.pop_front().unwrap();
            self.total_received_blocks.fetch_sub(old_received_blocks, Ordering::SeqCst);
            self.total_elapsed_ms.fetch_sub(old_elapsed_ms, Ordering::SeqCst);
        }

        samples.len() >= SYNC_RATE_WINDOW_MIN_THRESHOLD
    }
}

/// SyncRateRule
/// Allow mining even if the node is "not nearly synced" if the sync rate is below threshold
/// and the finality point is recent. This is to prevent the network from undermining and to allow
/// the network to automatically recover from any short-term mining halt.
///
/// Trigger: Sync rate is below threshold and finality point is recent
/// Recovery: Sync rate is back above threshold
impl MiningRule for SyncRateRule {
    fn check_rule(&self, delta: &ProcessingCountersSnapshot, extra_data: &ExtraData) {
        let elapsed_ms = extra_data.elapsed_time.as_millis() as u64;
        let received_blocks = delta.body_counts.max(delta.header_counts);

        if !self.update_sync_rate_window(received_blocks, elapsed_ms) {
            // Don't process the sync rule if the window doesn't have enough samples to filter out noise
            return;
        }

        // Expected blocks are derived ONCE over the whole window, not per sample. Per sample this
        // is `elapsed / interval` truncated to an integer: at the 10 s sampling interval that is
        // exactly 1 for a 10 s block time, but 0 for anything slower than the sampling interval —
        // so on a 120 s network every sample contributed 0, the window total stayed 0, and the
        // rate was `received / 0` = NaN. Every comparison against a NaN is false, so the rule
        // silently never fired and never recovered. Summing the time first keeps the arithmetic
        // exact at any block interval and is identical to the old value wherever the old one was
        // not truncating.
        let total_elapsed_ms = self.total_elapsed_ms.load(Ordering::SeqCst);
        let expected_blocks = total_elapsed_ms / extra_data.target_time_per_block;
        if expected_blocks == 0 {
            // Not enough wall clock has passed for even one block to be due; there is no rate to
            // judge yet. (Reachable on a slow network before the window fills.)
            return;
        }

        let rate: f64 = (self.total_received_blocks.load(Ordering::SeqCst) as f64) / (expected_blocks as f64);

        // Finality point is considered "recent" if it is within 3 finality durations from the current time
        let is_finality_recent = extra_data.finality_point_timestamp >= unix_now().saturating_sub(extra_data.finality_duration * 3);

        trace!(
            "Sync rate: {:.2} | Finality point recent: {} | Elapsed time: {}s | Connected: {} | Found/Expected blocks: {}/{}",
            rate,
            is_finality_recent,
            extra_data.elapsed_time.as_secs(),
            extra_data.has_sufficient_peer_connectivity,
            delta.body_counts,
            expected_blocks,
        );

        if is_finality_recent && rate < SYNC_RATE_THRESHOLD {
            // if sync rate rule conditions are met:
            if let Ok(false) = self.use_sync_rate_rule.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed) {
                warn!("Sync rate {:.2} is below threshold: {}", rate, SYNC_RATE_THRESHOLD);
            }
        } else {
            // else when sync rate conditions are not met:
            if let Ok(true) = self.use_sync_rate_rule.compare_exchange(true, false, Ordering::Relaxed, Ordering::Relaxed) {
                if !is_finality_recent {
                    warn!("Sync rate {:.2} recovered: {} by entering IBD", rate, SYNC_RATE_THRESHOLD);
                } else {
                    warn!("Sync rate {:.2} recovered: {}", rate, SYNC_RATE_THRESHOLD);
                }
            } else if !is_finality_recent {
                trace!("Finality period is old. Timestamp: {}. Sync rate: {:.2}", extra_data.finality_point_timestamp, rate);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::AtomicBool};

    use crate::rules::{ExtraData, mining_rule::MiningRule, sync_rate_rule::SYNC_RATE_WINDOW_MAX_SIZE};
    use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;
    use kaspa_core::time::unix_now;
    use std::sync::atomic::*;

    use super::{SYNC_RATE_WINDOW_MIN_THRESHOLD, SyncRateRule};

    fn create_rule() -> (Arc<AtomicBool>, SyncRateRule) {
        let use_sync_rate_rule = Arc::new(AtomicBool::new(false));
        let rule = SyncRateRule::new(use_sync_rate_rule.clone());
        (use_sync_rate_rule, rule)
    }

    #[test]
    fn test_rule_end_to_end_flow() {
        let (use_sync_rate_rule, rule) = create_rule();

        let good_snapshot =
            ProcessingCountersSnapshot { blocks_submitted: 100, header_counts: 100, body_counts: 100, ..Default::default() };

        let bad_snapshot = ProcessingCountersSnapshot::default();

        let extra_data = &ExtraData {
            elapsed_time: std::time::Duration::from_secs(10),
            target_time_per_block: 100, // 10bps value
            finality_point_timestamp: unix_now(),
            finality_duration: 1000,
            has_sufficient_peer_connectivity: true,
        };

        // Sync rate should be at 1.0
        for _ in 0..10 {
            rule.check_rule(&good_snapshot, extra_data);
        }

        assert!(
            !use_sync_rate_rule.load(Ordering::SeqCst),
            "Expected rule to not be triggered during normal operation. {} | {}",
            rule.total_received_blocks.load(Ordering::SeqCst),
            rule.total_elapsed_ms.load(Ordering::SeqCst)
        );

        // Sync rate should be at 0.5
        for _ in 0..11 {
            rule.check_rule(&bad_snapshot, extra_data);
        }

        assert!(
            use_sync_rate_rule.load(Ordering::SeqCst),
            "Expected rule to trigger. {} | {}",
            rule.total_received_blocks.load(Ordering::SeqCst),
            rule.total_elapsed_ms.load(Ordering::SeqCst)
        );

        for _ in 0..10 {
            rule.check_rule(&good_snapshot, extra_data);
        }

        assert!(
            !use_sync_rate_rule.load(Ordering::SeqCst),
            "Expected rule to not be triggered during normal operation. {} | {}",
            rule.total_received_blocks.load(Ordering::SeqCst),
            rule.total_elapsed_ms.load(Ordering::SeqCst)
        );
    }

    /// The rule must judge a network whose block interval is LONGER than the 10 s sampling
    /// interval. Per-sample `elapsed / interval` truncates to 0 there, the window total stayed 0,
    /// and `received / 0` is NaN — every comparison false, so the rule could neither fire nor
    /// recover on a 120 s network. Windowed arithmetic is what makes this expressible at all.
    #[test]
    fn test_rule_fires_when_block_interval_exceeds_the_sampling_interval() {
        let (use_sync_rate_rule, rule) = create_rule();
        let extra_data = &ExtraData {
            elapsed_time: std::time::Duration::from_secs(10),
            target_time_per_block: 120_000, // the PALW-era 120 s interval
            finality_point_timestamp: unix_now(),
            finality_duration: 1000,
            has_sufficient_peer_connectivity: true,
        };

        // A minute of samples receiving nothing: 60 s of wall clock is half a 120 s block, so the
        // window expects 0 blocks and there is nothing to judge yet — silence, not a false alarm.
        for _ in 0..(SYNC_RATE_WINDOW_MIN_THRESHOLD + 1) {
            rule.check_rule(&ProcessingCountersSnapshot::default(), extra_data);
        }
        assert!(!use_sync_rate_rule.load(Ordering::SeqCst), "no verdict before one block is even due");

        // Keep receiving nothing until blocks ARE due. Now the rate is a real 0 and the rule fires.
        for _ in 0..24 {
            rule.check_rule(&ProcessingCountersSnapshot::default(), extra_data);
        }
        assert!(
            use_sync_rate_rule.load(Ordering::SeqCst),
            "expected the rule to fire on a stalled 120 s network. received={} elapsed_ms={}",
            rule.total_received_blocks.load(Ordering::SeqCst),
            rule.total_elapsed_ms.load(Ordering::SeqCst)
        );
    }

    #[test]
    fn test_rule_with_old_finality() {
        let (use_sync_rate_rule, rule) = create_rule();

        let bad_snapshot = ProcessingCountersSnapshot::default();

        let extra_data = &ExtraData {
            elapsed_time: std::time::Duration::from_secs(10),
            target_time_per_block: 100,                                        // 10bps value
            finality_point_timestamp: unix_now().saturating_sub(1000 * 3) - 1, // the millisecond right before timestamp is "old enough"
            finality_duration: 1000,
            has_sufficient_peer_connectivity: true,
        };

        for _ in 0..10 {
            rule.check_rule(&bad_snapshot, extra_data);
        }

        assert!(
            !use_sync_rate_rule.load(Ordering::SeqCst),
            "Expected rule to trigger even with low sync rate if finality is old. {} | {}",
            rule.total_received_blocks.load(Ordering::SeqCst),
            rule.total_elapsed_ms.load(Ordering::SeqCst)
        );
    }

    #[test]
    fn test_sync_rate_window_updates() {
        let (_, rule) = create_rule();

        let received_blocks = 123;
        let elapsed_ms = 456;

        let old_received_total = rule.total_received_blocks.load(Ordering::SeqCst);
        let old_elapsed_total = rule.total_elapsed_ms.load(Ordering::SeqCst);

        rule.update_sync_rate_window(received_blocks, elapsed_ms);

        assert_eq!(rule.total_received_blocks.load(Ordering::SeqCst), old_received_total + received_blocks);
        assert_eq!(rule.total_elapsed_ms.load(Ordering::SeqCst), old_elapsed_total + elapsed_ms);
    }

    #[test]
    fn test_sync_rate_window_update_result_sample_sizes() {
        let (_, rule) = create_rule();

        for _ in 0..(SYNC_RATE_WINDOW_MIN_THRESHOLD - 1) {
            assert!(!rule.update_sync_rate_window(1, 1), "Expected false when window min size threshold is not filled but got true");
        }

        // sample is greater than threshold now
        assert!(rule.update_sync_rate_window(1, 1), "Expected true when window min size threshold is filled but got false");

        for _ in 0..SYNC_RATE_WINDOW_MAX_SIZE {
            // sample is greater than threshold now
            assert!(rule.update_sync_rate_window(1, 1), "Expected true when window min size threshold is filled but got false");
        }

        assert_eq!(
            rule.sync_rate_samples.read().unwrap().len(),
            SYNC_RATE_WINDOW_MAX_SIZE,
            "Expected window size to be at max after updating window it was already full"
        );
    }

    #[test]
    fn test_sync_rate_window_update_result_when_window_is_filled() {
        let (_, rule) = create_rule();

        let received_blocks = 10;
        let expected_blocks = 10;

        // Fill the window
        for _ in 0..SYNC_RATE_WINDOW_MAX_SIZE {
            rule.update_sync_rate_window(received_blocks, expected_blocks);
        }

        let total_received = rule.total_received_blocks.load(Ordering::SeqCst);
        let total_expected = rule.total_elapsed_ms.load(Ordering::SeqCst);

        let new_received_block = received_blocks * 2;
        let new_expected_block = expected_blocks * 2;
        // Add one more sample
        rule.update_sync_rate_window(new_received_block, new_expected_block);

        assert_eq!(
            rule.total_received_blocks.load(Ordering::SeqCst),
            total_received + new_received_block - received_blocks,
            "Expected total received blocks to be updated correctly"
        );

        assert_eq!(
            rule.total_elapsed_ms.load(Ordering::SeqCst),
            total_expected + new_expected_block - expected_blocks,
            "Expected total expected blocks to be updated correctly"
        );
    }
}
