//! Node-local admission accounting for TKR1. The source work partition is the
//! unit of the bucket, so a maximum legal Finish can fit and a policy change
//! does not restart or relabel the accumulated allowance.
use super::*;
use std::time::Instant;

const SOURCE_UNITS: u64 = 1_000_000;
const NANOS_PER_SEC: u128 = 1_000_000_000;

#[derive(Clone, Copy)]
pub struct TokenizerRegistrationLocalChargeV1 {
    units: u64,
    fits_source: bool,
}

/// This does not decode an Open/Part, read authenticated state, or run the
/// parser. Unknown, malformed and over-partition controls consume allowance
/// before ordinary validation. Finish uses the maximum quote for its policy.
pub fn tokenizer_registration_local_charge_v1(
    tx: &Transaction,
    policy: &TokenizerRegistrationPolicyV1,
) -> Option<TokenizerRegistrationLocalChargeV1> {
    body(tx)?;
    let budget = policy.source_budget_v1();
    let scan = scan_tokenizer_registration_work_v1(tx, policy, |_| None);
    let (Ok(budget), Ok(Some(scan))) = (budget, scan) else {
        return Some(TokenizerRegistrationLocalChargeV1 { units: SOURCE_UNITS, fits_source: false });
    };
    let cost = scan.cost;
    let dimensions = [(cost.bytes, budget.bytes), (cost.work, budget.work), (cost.io, budget.io), (cost.controls, budget.controls)];
    let ratio = dimensions
        .into_iter()
        .map(|(cost, limit)| (u128::from(cost) * u128::from(SOURCE_UNITS)).div_ceil(u128::from(limit)))
        .max()
        .expect("a service cost has four dimensions");
    Some(TokenizerRegistrationLocalChargeV1 {
        units: ratio.clamp(1, u128::from(SOURCE_UNITS)) as u64,
        fits_source: ratio <= u128::from(SOURCE_UNITS),
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenizerRegistrationLocalWorkRefusalV1 {
    Throttled,
    SourceExceeded,
}

/// Apply separately to a peer's relay work and the process's mempool
/// admission work. Neither bucket changes the chain's source/body limits.
pub struct TokenizerRegistrationLocalWorkV1 {
    burst: u128,
    refill_per_sec: u128,
    credit: u128,
    last: Option<Instant>,
}

impl TokenizerRegistrationLocalWorkV1 {
    /// Burst and refill are local operating values in full source partitions.
    pub fn new(burst: u64, refill_per_sec: u64) -> Self {
        let burst = u128::from(burst) * u128::from(SOURCE_UNITS) * NANOS_PER_SEC;
        Self { burst, refill_per_sec: u128::from(refill_per_sec) * u128::from(SOURCE_UNITS), credit: burst, last: None }
    }

    pub fn charge_v1(
        &mut self,
        charge: TokenizerRegistrationLocalChargeV1,
        now: Instant,
    ) -> Result<(), TokenizerRegistrationLocalWorkRefusalV1> {
        let now = self.last.map_or(now, |last| last.max(now));
        if let Some(last) = self.last {
            let elapsed = now.duration_since(last).as_nanos();
            self.credit = self.credit.saturating_add(elapsed.saturating_mul(self.refill_per_sec)).min(self.burst);
        }
        self.last = Some(now);
        let cost = u128::from(charge.units) * NANOS_PER_SEC;
        if self.credit < cost {
            return Err(TokenizerRegistrationLocalWorkRefusalV1::Throttled);
        }
        self.credit -= cost;
        if !charge.fits_source {
            return Err(TokenizerRegistrationLocalWorkRefusalV1::SourceExceeded);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn carrier(payload: &[u8]) -> Transaction {
        Transaction::new(0, vec![], vec![], 0, crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE, 0, payload.to_vec())
    }

    #[test]
    fn tokenizer_registration_local_meter_charges_maximum_finish_and_retries() {
        let (policy, _) = crate::palw_tokenizer_registration_v1::tests::fixture();
        let finish = carrier(b"TKR1\x02");
        let charge = tokenizer_registration_local_charge_v1(&finish, &policy).unwrap();
        let now = Instant::now();
        let mut meter = TokenizerRegistrationLocalWorkV1::new(1, 1);
        assert_eq!(meter.charge_v1(charge, now), Ok(()), "the maximum Finish fallback fits a one-partition burst");
        // Even identical transaction bytes cost work again. Consume every
        // remaining fractional allowance before checking retry/refill.
        while meter.charge_v1(charge, now).is_ok() {}
        assert_eq!(meter.charge_v1(charge, now), Err(TokenizerRegistrationLocalWorkRefusalV1::Throttled));
        assert_eq!(meter.charge_v1(charge, now + Duration::from_secs(1)), Ok(()));
        assert!(tokenizer_registration_local_charge_v1(&carrier(b"ordinary"), &policy).is_none());
    }

    #[test]
    fn tokenizer_registration_local_meter_charges_invalid_source_before_refusing() {
        let (mut policy, _) = crate::palw_tokenizer_registration_v1::tests::fixture();
        let now = Instant::now();
        let mut meter = TokenizerRegistrationLocalWorkV1::new(1, 1);
        let mut oversized = b"TKR1".to_vec();
        oversized.resize(policy.service_budget.bytes as usize + 1, 0xff);
        let charge = tokenizer_registration_local_charge_v1(&carrier(&oversized), &policy).unwrap();
        assert_eq!(meter.charge_v1(charge, now), Err(TokenizerRegistrationLocalWorkRefusalV1::SourceExceeded));
        assert_eq!(meter.charge_v1(charge, now), Err(TokenizerRegistrationLocalWorkRefusalV1::Throttled));
        policy.maximum_source_blocks = 0;
        let invalid = tokenizer_registration_local_charge_v1(&carrier(b"TKR1"), &policy).unwrap();
        assert_eq!(meter.charge_v1(invalid, now), Err(TokenizerRegistrationLocalWorkRefusalV1::Throttled));
        assert_eq!(
            meter.charge_v1(invalid, now + Duration::from_secs(1)),
            Err(TokenizerRegistrationLocalWorkRefusalV1::SourceExceeded)
        );
    }
}
