//! Payload-free reasons for an unchanged PALW `found: false` response.
//!
//! Never retain or emit the underlying StoreError, declaration/program error
//! or IO path: those may contain peer-supplied material. Only static labels and
//! the standard IO error kind cross this diagnostic boundary.
use std::{
    collections::BTreeMap,
    io,
    time::{Duration, Instant},
};

/// Reasons are chosen by code, never by a request. Throttle across peers so
/// a flood of arbitrary pruning-point hashes cannot amplify the log volume.
#[derive(Default)]
pub(super) struct ReportLimiter(BTreeMap<(&'static str, &'static str, &'static str), Instant>);

impl ReportLimiter {
    pub fn admit(&mut self, mode: &'static str, failure: ServeFailure, now: Instant) -> bool {
        let key = (mode, failure.stage, failure.reason);
        if self.0.get(&key).is_some_and(|last| now.saturating_duration_since(*last) < Duration::from_secs(5)) {
            return false;
        }
        self.0.insert(key, now);
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ServeFailure {
    pub stage: &'static str,
    pub reason: &'static str,
    pub io_kind: Option<io::ErrorKind>,
}

impl ServeFailure {
    pub const fn new(stage: &'static str, reason: &'static str) -> Self {
        Self { stage, reason, io_kind: None }
    }

    pub fn io(stage: &'static str, error: &io::Error) -> Self {
        Self { stage, reason: "io_error", io_kind: Some(error.kind()) }
    }

    pub fn spill(error: &io::Error) -> Self {
        let reason = if error.kind() == io::ErrorKind::WouldBlock {
            if error.to_string() == "PALW snapshot session budget is busy" { "session_busy" } else { "storage_budget" }
        } else {
            "spill_io"
        };
        Self { stage: "spill", reason, io_kind: Some(error.kind()) }
    }

    pub fn declaration(error: &str) -> Self {
        let reason = if error.contains("has no declaration index row") {
            "index_row_missing"
        } else if error.contains("declaration index belongs to another registration") {
            "registration_mismatch"
        } else {
            "declaration_validation"
        };
        Self::new("declarations", reason)
    }

    pub fn encoding(error: &io::Error) -> Self {
        let reason = if error.kind() == io::ErrorKind::InvalidData {
            match error.to_string().as_str() {
                "snapshot exceeds existing total byte cap" => "byte_cap",
                "snapshot serialization lease expired" => "lease_expired",
                _ => "invalid_encoding",
            }
        } else {
            "encoding_io"
        };
        Self { stage: "encoding", reason, io_kind: Some(error.kind()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declaration_reasons_distinguish_missing_registration_and_validation_without_data() {
        let cases = [
            ("snapshot class PRIVATE has no declaration index row", "index_row_missing"),
            ("snapshot class PRIVATE declaration index belongs to another registration", "registration_mismatch"),
            ("invalid PRIVATE raw claim/key/program", "declaration_validation"),
        ];
        for (error, expected) in cases {
            let failure = ServeFailure::declaration(error);
            assert_eq!(failure.reason, expected);
            assert!(!format!("{failure:?}").contains("PRIVATE"));
        }
    }

    #[test]
    fn spill_session_and_storage_refusal_are_distinct_without_exposing_paths() {
        let session = io::Error::new(io::ErrorKind::WouldBlock, "PALW snapshot session budget is busy");
        let storage = io::Error::new(io::ErrorKind::WouldBlock, "/PRIVATE/storage quota exhausted");
        assert_eq!(ServeFailure::spill(&session).reason, "session_busy");
        assert_eq!(ServeFailure::spill(&storage).reason, "storage_budget");
        assert!(!format!("{:?}", ServeFailure::spill(&storage)).contains("PRIVATE"));
    }

    #[test]
    fn encoding_and_storage_keep_only_io_kind() {
        let raw = io::Error::new(io::ErrorKind::InvalidData, "PRIVATE raw bytes and path");
        let failure = ServeFailure::io("encoding", &raw);
        assert_eq!(failure.io_kind, Some(io::ErrorKind::InvalidData));
        assert!(!format!("{failure:?}").contains("PRIVATE"));
        assert_eq!(ServeFailure::encoding(&raw).reason, "invalid_encoding");
        assert!(!format!("{:?}", ServeFailure::encoding(&raw)).contains("PRIVATE"));
        let cap = io::Error::new(io::ErrorKind::InvalidData, "snapshot exceeds existing total byte cap");
        let expired = io::Error::new(io::ErrorKind::InvalidData, "snapshot serialization lease expired");
        assert_eq!(ServeFailure::encoding(&cap).reason, "byte_cap");
        assert_eq!(ServeFailure::encoding(&expired).reason, "lease_expired");
    }

    #[test]
    fn repeated_refusals_are_throttled_without_suppressing_different_causes() {
        let now = Instant::now();
        let mut limiter = ReportLimiter::default();
        let no_row = ServeFailure::new("snapshot_read", "not_available");
        let capacity = ServeFailure::new("snapshot_read", "resource_unavailable");
        assert!(limiter.admit("chunked", no_row, now));
        assert!(!limiter.admit("chunked", no_row, now + Duration::from_secs(4)));
        assert!(limiter.admit("chunked", capacity, now));
        assert!(limiter.admit("legacy", no_row, now));
        assert!(limiter.admit("chunked", no_row, now + Duration::from_secs(5)));
    }
}
