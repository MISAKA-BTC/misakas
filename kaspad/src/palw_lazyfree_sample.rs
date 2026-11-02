//! A reclaimable-byte reading is a dated observation, never a standing credit.
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub(super) struct LazyFreeSample {
    pub(super) bytes: u64,
    pub(super) sampled_at: Option<Instant>,
    pub(super) succeeded: bool,
}
impl LazyFreeSample {
    pub(super) fn complete(&mut self, started_at: Instant, result: Option<u64>) {
        self.sampled_at = Some(started_at);
        self.succeeded = result.is_some();
        self.bytes = result.unwrap_or(0);
    }
    pub(super) fn credit(&self, now: Instant, expiry: Duration) -> u64 {
        if !self.succeeded {
            return 0;
        }
        match self.sampled_at.and_then(|at| now.checked_duration_since(at)) {
            Some(age) if age < expiry => self.bytes,
            _ => 0,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn success_failure_expiry_and_recovery() {
        let start = Instant::now();
        let expiry = Duration::from_secs(30);
        let mut sample = LazyFreeSample::default();
        assert_eq!(sample.credit(start, expiry), 0);
        sample.complete(start, Some(100));
        assert_eq!(sample.credit(start + Duration::from_secs(29), expiry), 100);
        assert_eq!(sample.credit(start + expiry, expiry), 0);
        sample.complete(start + expiry, None);
        assert!(!sample.succeeded);
        assert_eq!(sample.credit(start + expiry, expiry), 0);
        sample.complete(start + expiry, Some(50));
        assert_eq!(sample.credit(start + expiry, expiry), 50);
        assert_eq!(sample.credit(start, expiry), 0, "a backwards clock cannot freshen a reading");
    }
    #[test]
    fn slow_read_is_dated_at_start_not_completion() {
        let start = Instant::now();
        let mut sample = LazyFreeSample::default();
        sample.complete(start, Some(500));
        assert_eq!(sample.credit(start + Duration::from_secs(31), Duration::from_secs(30)), 0);
    }
}
