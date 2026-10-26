//! Typed native paid-check read. String compatibility belongs at the SDK edge;
//! a local admission failure must never select a legacy authority fallback.
//! RED staging: this sibling is compiled but the native callers still use the
//! existing String API until the coordinated baseline failure is observed.
use crate::palw_claim_challenge_v2::*;
use crate::palw_kernel_route_v1::PalwKernelRouteStateV1;
use crate::palw_native_resource_v1::NativeDecodeErrorV1 as E;
use misaka_palw_kernel::claim_paid_v2::{paid_claim_bootstrap_anchor_v2, paid_claim_seal_anchor_v2, PaidClaimCheckAuthorityV2};
use misaka_palw_kernel::hash::Digest;

fn invalid(why: impl Into<String>) -> E<String> {
    E::Decode(why.into())
}

impl PalwKernelRouteStateV1 {
    pub fn paid_claim_check_authority_checked_v2(&self, context: &Digest, now: u64) -> Result<PaidClaimCheckAuthorityV2, E<String>> {
        // This result is authority, not a probe for a format/version. In
        // particular Resource cannot be used to choose an unretained raw read.
        let a = self.paid_claim_authority_checked_v6(context)?;
        a.validate(&*self.ledger_checked_v1()?).map_err(invalid)?;
        if let Some(binding) = self.paid_bootstrap_binding_v6(&a.enrollment.id())? {
            if binding.original_context != *context || binding.enrollment != a.enrollment.id() {
                return Err(invalid("paid claim context/target mismatch"));
            }
            return self.paid_bootstrap_check_authority_v6(&a, now);
        }
        let lock = self.paid_claim_effective_lock_checked_v6(&a)?;
        let seeded = lock
            .as_ref()
            .is_some_and(|lock| lock.anchor == paid_claim_bootstrap_anchor_v2() || lock.anchor == paid_claim_seal_anchor_v2());
        if !seeded && self.beacon_source_target_v4(context).map_err(invalid)? != a.source_target {
            return Err(invalid("paid claim context/target mismatch"));
        }
        if let Some(lock) = lock.as_ref().filter(|_| seeded) {
            let ledger = self.ledger_checked_v1()?;
            let base = ledger.claim_challenge_subject(&a.enrollment.base_claim).ok_or_else(|| invalid("base statement missing"))?;
            return PaidClaimCheckAuthorityV2::verify_bootstrap_v2(
                &a.class,
                &a.enrollment,
                &base,
                &a.context,
                now,
                paid_checker_snapshot_root_v2(&a.checker_candidates),
                lock.seed,
            )
            .map_err(invalid);
        }
        // Preserve the existing normal source algorithm/provenance. Its nested
        // String-returning legacy readers remain an explicit separate boundary;
        // do not pretend to recover a lost Resource by parsing its display text.
        let (contract, sources, promises) =
            self.paid_beacon_material_v5(context).map_err(invalid)?.ok_or_else(|| invalid("paid claim source funding missing"))?;
        let collected = misaka_palw_challenge::collect_paid_opt_in_work_beacon_v5(&a.context, &contract, &sources, &promises, now)
            .map_err(|why| invalid(why.to_string()))?;
        let misaka_palw_challenge::SealedBeaconStateV3::Locked(beacon) = collected else {
            return Err(invalid("paid claim source has not locked"));
        };
        let ledger = self.ledger_checked_v1()?;
        let base = ledger.claim_challenge_subject(&a.enrollment.base_claim).ok_or_else(|| invalid("base statement missing"))?;
        PaidClaimCheckAuthorityV2::verify(
            &a.class,
            &a.enrollment,
            &base,
            &a.context,
            &beacon,
            now,
            paid_checker_snapshot_root_v2(&a.checker_candidates),
        )
        .map_err(invalid)
    }
}

// Scheduling-only test seam. It runs after the first native reads have passed,
// so a quota refusal in the second authority read is not confused with refusal
// at the outer Receipt/FS entry. It never supplies a result or an authority.
#[cfg(test)]
thread_local! {
    static BEFORE_CHECK: std::cell::RefCell<Option<Box<dyn FnOnce(&'static str)>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn before_check_authority_v2(site: &'static str) {
    let callback = BEFORE_CHECK.with(|slot| slot.borrow_mut().take());
    if let Some(callback) = callback {
        callback(site);
    }
}
#[cfg(test)]
pub(crate) fn with_before_check_authority_v2<T>(before: impl FnOnce(&'static str) + 'static, run: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            BEFORE_CHECK.with(|slot| *slot.borrow_mut() = None);
        }
    }
    BEFORE_CHECK.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(Box::new(before));
    });
    let _reset = Reset;
    run()
}
