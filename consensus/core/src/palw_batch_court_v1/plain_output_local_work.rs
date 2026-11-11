//! Local BCT2 intake allowance. Every framed carrier consumes one worst-case
//! carrier attempt, including malformed payloads and retries. This is not a
//! consensus capacity limit and never authenticates a request or invokes replay.

use super::carriage::BATCH_PLAIN_OUTPUT_CARRIER_MAGIC_V2;
use crate::{subnets::SUBNETWORK_ID_PALW_LIFECYCLE, tx::Transaction};
use std::time::Instant;

const NANOS_PER_SEC: u128 = 1_000_000_000;

#[derive(Clone, Copy)]
pub struct BatchPlainOutputLocalChargeV2(());

/// The caller resolves the active transport policy first. Do not decode here:
/// truncated/unknown controls need the same local allowance as well-formed ones.
pub fn plain_output_local_charge_v2(transaction: &Transaction) -> Option<BatchPlainOutputLocalChargeV2> {
    (transaction.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE && transaction.payload.starts_with(BATCH_PLAIN_OUTPUT_CARRIER_MAGIC_V2))
        .then_some(BatchPlainOutputLocalChargeV2(()))
}

/// Separate peer and process instances. Policy/fence changes do not refill it.
/// Operational values count worst-case carriers, not calibrated CPU seconds.
pub struct BatchPlainOutputLocalWorkV2 {
    burst: u128,
    refill_per_sec: u128,
    credit: u128,
    last: Option<Instant>,
}

impl BatchPlainOutputLocalWorkV2 {
    pub fn new(burst: u64, refill_per_sec: u64) -> Self {
        let burst = u128::from(burst) * NANOS_PER_SEC;
        Self { burst, refill_per_sec: u128::from(refill_per_sec), credit: burst, last: None }
    }

    /// `false` is local backpressure, never a malicious-peer or chain verdict.
    pub fn charge_v2(&mut self, _: BatchPlainOutputLocalChargeV2, now: Instant) -> bool {
        let now = self.last.map_or(now, |last| last.max(now));
        if let Some(last) = self.last {
            self.credit =
                self.credit.saturating_add(now.duration_since(last).as_nanos().saturating_mul(self.refill_per_sec)).min(self.burst);
        }
        self.last = Some(now);
        if self.credit < NANOS_PER_SEC {
            return false;
        }
        self.credit -= NANOS_PER_SEC;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn bct2_local_intake_charges_malformed_and_duplicates_and_refills_without_clock_reversal() {
        let transaction = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_LIFECYCLE, 0, b"BCT2\xff".to_vec());
        let malformed = plain_output_local_charge_v2(&transaction).unwrap();
        let mut meter = BatchPlainOutputLocalWorkV2::new(2, 1);
        let now = Instant::now();
        assert!(meter.charge_v2(malformed, now));
        assert!(meter.charge_v2(malformed, now));
        assert!(!meter.charge_v2(malformed, now));
        assert!(!meter.charge_v2(malformed, now + Duration::from_millis(999)));
        assert!(meter.charge_v2(malformed, now + Duration::from_secs(1)));
        assert!(!meter.charge_v2(malformed, now));
        assert!(!meter.charge_v2(malformed, now + Duration::from_secs(1)));
        assert!(meter.charge_v2(malformed, now + Duration::from_secs(2)));
        let mut other = transaction.clone();
        other.payload = b"BCT1\xff".to_vec();
        assert!(plain_output_local_charge_v2(&other).is_none());
        other.payload = b"BCT2\xff".to_vec();
        other.subnetwork_id = crate::subnets::SUBNETWORK_ID_NATIVE;
        assert!(plain_output_local_charge_v2(&other).is_none());
        assert!(!BatchPlainOutputLocalWorkV2::new(0, u64::MAX).charge_v2(malformed, now));
    }
}
