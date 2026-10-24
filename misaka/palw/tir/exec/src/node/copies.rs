//! Before-copy admission for an evidence store. The operational limit covers requested
//! native output and construction scratch, not fees, measured RSS or consensus class rules.
//! Returned proof objects require their caller's longer-lived duty admission after store drop.

use std::cell::{Cell, RefCell};

use kaspa_consensus_core::{Hash64, palw_v2::PalwJobContextV2};

use super::memory::{TirMemoryLeaseV1, TirMemoryReserverV1, memory_reserver_v1, reserve_v1};

// The same operational scale as the existing replay-output budget. Chunking keeps the
// ledger/guard list bounded without reserving a whole GiB for a tiny leaf close.
pub(crate) const DEFAULT_COPY_LIMIT_V1: u64 = 1 << 30;
const GROWTH_V1: u64 = 1 << 20;

#[derive(Debug)]
pub(crate) enum TirCopyRefusalV2 {
    ConfiguredLimit,
    LocalResource(String),
}

pub(crate) struct TirCarriedCopiesV1 {
    refusal: RefCell<Option<TirCopyRefusalV2>>,
    per_move_limit: u64,
    lifetime_limit: u64,
    move_bytes: Cell<u64>,
    lifetime_bytes: Cell<u64>,
    reserved_bytes: Cell<u64>,
    leases: RefCell<Vec<TirMemoryLeaseV1>>,
    // Keep the provider alive with its guards, including after a global/pool change.
    reserver: Option<TirMemoryReserverV1>,
}

impl TirCarriedCopiesV1 {
    pub(crate) fn new() -> Self {
        Self {
            refusal: RefCell::new(None),
            per_move_limit: DEFAULT_COPY_LIMIT_V1,
            lifetime_limit: DEFAULT_COPY_LIMIT_V1,
            move_bytes: Cell::new(0),
            lifetime_bytes: Cell::new(0),
            reserved_bytes: Cell::new(0),
            leases: RefCell::new(Vec::new()),
            reserver: memory_reserver_v1(),
        }
    }
    pub(crate) fn reset_move(&self) {
        self.move_bytes.set(0);
        self.refusal.borrow_mut().take();
        // Issued owned objects may still exist: never drop guards or reset lifetime usage.
    }
    pub(crate) fn set_limits(&mut self, per_move: u64, lifetime: u64) {
        self.per_move_limit = per_move.min(DEFAULT_COPY_LIMIT_V1);
        self.lifetime_limit = lifetime.min(DEFAULT_COPY_LIMIT_V1);
    }
    pub(crate) fn used(&self) -> u64 {
        self.move_bytes.get()
    }
    pub(crate) fn lifetime_used(&self) -> u64 {
        self.lifetime_bytes.get()
    }
    pub(crate) fn reserved(&self) -> u64 {
        self.reserved_bytes.get()
    }

    fn refuse_v2(&self, refusal: TirCopyRefusalV2) {
        let mut current = self.refusal.borrow_mut();
        // A later missing operand/cap may not erase an observed local failure.
        if !matches!(*current, Some(TirCopyRefusalV2::LocalResource(_))) {
            *current = Some(refusal);
        }
    }
    pub(crate) fn take_refusal_v2(&self) -> Option<TirCopyRefusalV2> {
        self.refusal.borrow_mut().take()
    }
    pub(crate) fn take_leases_v2(&self) -> Vec<TirMemoryLeaseV1> {
        self.reserved_bytes.set(0);
        std::mem::take(&mut *self.leases.borrow_mut())
    }
    pub(crate) fn admit(&self, class: Hash64, ctx: &PalwJobContextV2, need: u64) -> Option<()> {
        let Some((next_move, next_lifetime)) =
            self.move_bytes.get().checked_add(need).zip(self.lifetime_bytes.get().checked_add(need))
        else {
            self.refuse_v2(TirCopyRefusalV2::ConfiguredLimit);
            return None;
        };
        if next_move > self.per_move_limit || next_lifetime > self.lifetime_limit {
            self.refuse_v2(TirCopyRefusalV2::ConfiguredLimit);
            return None;
        }
        if next_lifetime > self.reserved_bytes.get() {
            let Some(rounded) = next_lifetime.checked_add(GROWTH_V1 - 1).and_then(|n| (n / GROWTH_V1).checked_mul(GROWTH_V1)) else {
                self.refuse_v2(TirCopyRefusalV2::ConfiguredLimit);
                return None;
            };
            let new_reserved = rounded.min(self.lifetime_limit);
            let growth = new_reserved.checked_sub(self.reserved_bytes.get())?;
            if let Err(why) = self.leases.borrow_mut().try_reserve(1) {
                self.refuse_v2(TirCopyRefusalV2::LocalResource(why.to_string()));
                return None;
            }
            let lease = match reserve_v1(&self.reserver, "tir-carried-copies", class, ctx, growth) {
                Ok(lease) => lease,
                Err(why) => {
                    self.refuse_v2(TirCopyRefusalV2::LocalResource(why));
                    return None;
                }
            };
            if let Some(lease) = lease {
                self.leases.borrow_mut().push(lease);
            }
            self.reserved_bytes.set(new_reserved);
        }
        self.move_bytes.set(next_move);
        self.lifetime_bytes.set(next_lifetime);
        Some(())
    }
    pub(crate) fn scratch(&self, class: Hash64, ctx: &PalwJobContextV2, need: u64) -> Option<Option<TirMemoryLeaseV1>> {
        if need > self.per_move_limit || need > self.lifetime_limit {
            self.refuse_v2(TirCopyRefusalV2::ConfiguredLimit);
            return None;
        }
        if need == 0 {
            return Some(None);
        }
        match reserve_v1(&self.reserver, "tir-param-scratch", class, ctx, need) {
            Ok(lease) => Some(lease),
            Err(why) => {
                self.refuse_v2(TirCopyRefusalV2::LocalResource(why));
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    };
    struct Held {
        bytes: u64,
        held: Arc<AtomicU64>,
    }
    impl Drop for Held {
        fn drop(&mut self) {
            self.held.fetch_sub(self.bytes, Ordering::SeqCst);
        }
    }

    #[test]
    fn copy_growth_refusal_reset_lifetime_and_overflow_are_atomic() {
        let (held, refused) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicBool::new(false)));
        let mut copies = TirCarriedCopiesV1::new();
        copies.reserver = Some({
            let (held, refused) = (held.clone(), refused.clone());
            Arc::new(move |_, _, _, bytes| {
                if refused.load(Ordering::SeqCst) {
                    return Err("refused before copy".into());
                }
                held.fetch_add(bytes, Ordering::SeqCst);
                Ok(Box::new(Held { bytes, held: held.clone() }))
            })
        });
        // This local probe needs only identity, not a valid execution job.
        let z = Hash64::from_bytes([0; 64]);
        let ctx = PalwJobContextV2 {
            version: 2,
            network_id: vec![],
            job_id: z,
            job_nullifier: z,
            assignment_id: z,
            execution_seed: [0; 32],
            model_profile_id: z,
            runtime_manifest_hash: z,
            runtime_class_id: z,
            shape_profile_id: z,
            trace_scheme_id: z,
            cu_ruleset_id: z,
            tokenizer_id: z,
            prompt_token_ids_hash: z,
            declared_prefill_tokens: 1,
            exact_decode_tokens: 1,
            max_context_tokens: 2,
        };
        let class = Hash64::from_bytes([42; 64]);
        copies.set_limits(2 * GROWTH_V1, 2 * GROWTH_V1);
        assert!(copies.scratch(class, &ctx, 2 * GROWTH_V1 + 1).is_none());
        assert_eq!(held.load(Ordering::SeqCst), 0, "oversized scratch is refused before reservation");
        let scratch = copies.scratch(class, &ctx, 2 * GROWTH_V1).unwrap();
        assert_eq!(held.load(Ordering::SeqCst), 2 * GROWTH_V1);
        assert_eq!(copies.lifetime_used(), 0, "temporary scratch does not consume retained lifetime");
        drop(scratch);
        assert_eq!(held.load(Ordering::SeqCst), 0);
        copies.set_limits(0, 2 * GROWTH_V1);
        assert!(copies.scratch(class, &ctx, 1).is_none());
        assert!(copies.scratch(class, &ctx, 0).unwrap().is_none());
        copies.set_limits(2 * GROWTH_V1, 0);
        assert!(copies.scratch(class, &ctx, 1).is_none());
        copies.set_limits(2 * GROWTH_V1, 2 * GROWTH_V1);
        assert!(copies.admit(class, &ctx, GROWTH_V1).is_some());
        assert_eq!(held.load(Ordering::SeqCst), GROWTH_V1);
        refused.store(true, Ordering::SeqCst);
        assert!(copies.admit(class, &ctx, 1).is_none());
        assert_eq!((copies.used(), copies.lifetime_used(), copies.reserved()), (GROWTH_V1, GROWTH_V1, GROWTH_V1));
        copies.reset_move();
        assert_eq!(copies.used(), 0);
        assert_eq!(held.load(Ordering::SeqCst), GROWTH_V1);
        refused.store(false, Ordering::SeqCst);
        assert!(copies.admit(class, &ctx, GROWTH_V1).is_some());
        assert!(copies.admit(class, &ctx, 1).is_none());
        assert!(copies.admit(class, &ctx, u64::MAX).is_none());
        assert_eq!(held.load(Ordering::SeqCst), 2 * GROWTH_V1);
        let provider = copies.reserver.clone();
        drop(copies);
        assert_eq!(held.load(Ordering::SeqCst), 0);
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut copies = TirCarriedCopiesV1::new();
            copies.reserver = provider.clone();
            copies.admit(class, &ctx, 1).unwrap();
            panic!("copy lease unwind control");
        }));
        assert!(unwind.is_err());
        assert_eq!(held.load(Ordering::SeqCst), 0, "unwind drops admitted copy guards");
    }
}
