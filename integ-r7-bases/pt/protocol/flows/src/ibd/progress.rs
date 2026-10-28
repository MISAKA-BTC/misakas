use std::time::{Duration, Instant};

use chrono::{Local, LocalResult, TimeZone};
use kaspa_core::info;

/// Minimum number of items to report
const REPORT_BATCH_GRANULARITY: usize = 500;
/// Maximum time to go without report
const REPORT_TIME_GRANULARITY: Duration = Duration::from_secs(2);

pub struct ProgressReporter {
    low_daa_score: u64,
    high_daa_score: u64,
    object_name: &'static str,
    last_reported_percent: i32,
    last_log_time: Instant,
    current_batch: usize,
    processed: usize,
}

impl ProgressReporter {
    pub fn new(low_daa_score: u64, mut high_daa_score: u64, object_name: &'static str) -> Self {
        if high_daa_score <= low_daa_score {
            // Avoid a zero or negative diff
            high_daa_score = low_daa_score + 1;
        }
        Self {
            low_daa_score,
            high_daa_score,
            object_name,
            last_reported_percent: 0,
            last_log_time: Instant::now(),
            current_batch: 0,
            processed: 0,
        }
    }

    pub fn report(&mut self, processed_delta: usize, current_daa_score: u64, current_timestamp: u64) {
        self.current_batch += processed_delta;
        let now = Instant::now();
        if now - self.last_log_time < REPORT_TIME_GRANULARITY && self.current_batch < REPORT_BATCH_GRANULARITY && self.processed > 0 {
            return;
        }
        self.processed += self.current_batch;
        self.current_batch = 0;
        if current_daa_score > self.high_daa_score {
            self.high_daa_score = current_daa_score + 1; // + 1 for keeping it at 99%
        }
        let percent = self.percent_at(current_daa_score);
        if percent > self.last_reported_percent {
            let date = match Local.timestamp_opt(current_timestamp as i64 / 1000, 1000 * (current_timestamp as u32 % 1000)) {
                LocalResult::None | LocalResult::Ambiguous(_, _) => "cannot parse date".into(),
                LocalResult::Single(date) => date.format("%Y-%m-%d %H:%M:%S.%3f:%z").to_string(),
            };
            info!("IBD: Processed {} {} ({}%) last block timestamp: {}", self.processed, self.object_name, percent, date);
            self.last_reported_percent = percent;
        }
        self.last_log_time = now;
    }

    /// The progress percentage at `current_daa_score`, as a fraction of this reporter's
    /// `[low_daa_score, high_daa_score]` DAA-score window (`new` guarantees `high > low`, so this
    /// never divides by zero). IBD progress is therefore DAA-score-based, NOT a fraction of object
    /// COUNT: each phase (block headers, then block bodies) builds its OWN reporter over its OWN
    /// window and so restarts from 0%. That is why the body phase legitimately reports a low percent
    /// right after the header phase reached 100% — it is a new phase, not a rollback/re-sync.
    fn percent_at(&self, current_daa_score: u64) -> i32 {
        let relative_daa_score = current_daa_score.saturating_sub(self.low_daa_score);
        ((relative_daa_score as f64 / (self.high_daa_score - self.low_daa_score) as f64) * 100.0) as i32
    }

    pub fn report_completion(mut self, processed_delta: usize) {
        self.processed += self.current_batch + processed_delta;
        info!("IBD: Processed {} {} (100%)", self.processed, self.object_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The IBD progress percent is computed from the DAA-score window, NOT from object count, and a
    /// fresh reporter starts at 0% — so the header→body phase change is a per-phase reset, not the
    /// 66k-header rollback it can superficially look like in the logs.
    #[test]
    fn percent_is_daa_score_based_and_resets_per_phase() {
        // A "block headers" phase over the DAA window [1000, 2000].
        let headers = ProgressReporter::new(1000, 2000, "block headers");
        assert_eq!(headers.percent_at(1000), 0);
        assert_eq!(headers.percent_at(1500), 50, "percent is the score fraction, not a count ratio");
        assert_eq!(headers.percent_at(2000), 100);
        assert_eq!(headers.processed, 0, "a fresh reporter starts at 0 processed");
        assert_eq!(headers.last_reported_percent, 0, "and 0% reported");

        // A separate "block bodies" phase over its OWN window starts fresh at 0%, even though the
        // header phase had just reached 100% — exactly the observed "(100%) then (low %)" sequence.
        let bodies = ProgressReporter::new(1800, 2000, "block bodies");
        assert_eq!(bodies.percent_at(1800), 0);
        assert_eq!(bodies.percent_at(1810), 5);
        assert_eq!(bodies.last_reported_percent, 0);
    }

    #[test]
    fn new_guards_against_zero_or_inverted_window() {
        // high <= low is bumped to low + 1 so percent never divides by zero.
        let r = ProgressReporter::new(5000, 5000, "block bodies");
        assert_eq!(r.high_daa_score, 5001);
        assert_eq!(r.percent_at(5000), 0);
        assert_eq!(r.percent_at(5001), 100);
    }
}
