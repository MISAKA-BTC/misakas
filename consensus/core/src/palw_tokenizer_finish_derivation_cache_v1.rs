//! Candidate-scoped memoization of the deterministic part of TKR1 Finish.

use crate::{
    Hash64,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1, btree_node_envelope_bytes_v1, reserve_native_pending_requests_v1,
    },
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2},
    palw_tokenizer_registration_v1::{TOKENIZER_REGISTRATION_FINISH_ATTEMPTS_V1, TokenizerRegistrationPolicyV1},
    palw_tokenizer_rendering_v2::{PalwRootedTokenizerRegistrationRowV1, PalwVerifiedTokenizerRegistrationErrorV1},
};
use kaspa_core::memory_budget::{MemoryBudgetError, MemoryClass};
use std::{cell::RefCell, collections::BTreeMap, mem, ptr, rc::Rc};

type Derivation = Result<PalwRootedTokenizerRegistrationRowV1, PalwVerifiedTokenizerRegistrationErrorV1>;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FinishDerivationKeyV1 {
    pub open: Hash64,
    pub slot: u8,
    pub eog: Hash64,
}

enum RetainedDerivationV1 {
    Verified(PalwRootedTokenizerRegistrationRowV1),
    Evidence(String),
}
impl RetainedDerivationV1 {
    fn replay(&self) -> Derivation {
        match self {
            Self::Verified(row) => Ok(row.clone()),
            Self::Evidence(reason) => Err(PalwVerifiedTokenizerRegistrationErrorV1::Evidence(reason.clone())),
        }
    }
}

type Entry = (RetainedDerivationV1, NativeDemandLeaseV1);
struct MemoV1 {
    parent_addr: usize,
    parent_root: Hash64,
    at: PalwBlockContextV2,
    policy: TokenizerRegistrationPolicyV1,
    maximum_entries: u64,
    entries: RefCell<BTreeMap<FinishDerivationKeyV1, Entry>>,
    _lease: NativeDemandLeaseV1,
}

thread_local! {
    static ACTIVE: RefCell<Option<Rc<MemoV1>>> = const { RefCell::new(None) };
}

/// Keep one owner across synchronous preview, observed rehearsal and final
/// fold of one candidate. The immutable borrow prevents reuse of the parent's
/// address for a different snapshot during this owner's lifetime.
pub struct TokenizerFinishDerivationCacheV1<'parent> {
    _parent: &'parent PalwChainStateV2,
    memo: Rc<MemoV1>,
}

fn reserve(bytes: u64) -> Result<NativeDemandLeaseV1, NativeCoverageV1> {
    reserve_native_pending_requests_v1(bytes)?
        .ok_or_else(|| NativeCoverageV1::unsupported::<MemoV1>("tokenizer Finish cache", "native lease unavailable"))
}

impl<'parent> TokenizerFinishDerivationCacheV1<'parent> {
    pub fn new(
        parent: &'parent PalwChainStateV2,
        at: PalwBlockContextV2,
        policy: &TokenizerRegistrationPolicyV1,
    ) -> Result<Self, NativeCoverageV1> {
        policy
            .effective_reservation_budget_v1()
            .map_err(|why| NativeCoverageV1::unsupported::<Self>("tokenizer Finish cache", why))?;
        let maximum_entries =
            (u64::from(policy.max_pending_total) * TOKENIZER_REGISTRATION_FINISH_ATTEMPTS_V1).min(policy.service_budget.controls);
        let lease = reserve((mem::size_of::<MemoV1>() + mem::align_of::<MemoV1>() + mem::size_of::<[usize; 2]>()) as u64)?;
        let memo = Rc::new(MemoV1 {
            parent_addr: ptr::from_ref(parent).addr(),
            parent_root: parent.state_root(),
            at,
            policy: *policy,
            maximum_entries,
            entries: RefCell::new(BTreeMap::new()),
            _lease: lease,
        });
        memo._lease.materialized_v1();
        Ok(Self { _parent: parent, memo })
    }

    pub fn parent_root_v1(&self) -> Hash64 {
        self.memo.parent_root
    }

    pub fn scope_v1<T>(&self, run: impl FnOnce() -> T) -> T {
        struct Restore(Option<Rc<MemoV1>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                ACTIVE.with(|active| active.replace(self.0.take()));
            }
        }
        let _restore = Restore(ACTIVE.with(|active| active.replace(Some(Rc::clone(&self.memo)))));
        run()
    }
}

/// Call only after authorization and the accepted-parent slot and Parts have
/// been checked. No provisional attempt or rooted state enters this cache.
pub(crate) fn derive_tokenizer_finish_once_v1(
    parent: &PalwChainStateV2,
    parent_root: Hash64,
    at: PalwBlockContextV2,
    policy: &TokenizerRegistrationPolicyV1,
    key: FinishDerivationKeyV1,
    derive: impl FnOnce() -> Derivation,
) -> Derivation {
    let memo = ACTIVE.with(|active| active.borrow().clone());
    let Some(memo) = memo.filter(|m| {
        m.parent_addr == ptr::from_ref(parent).addr() && m.parent_root == parent_root && m.at == at && m.policy == *policy
    }) else {
        return derive();
    };
    let node = btree_node_envelope_bytes_v1::<FinishDerivationKeyV1, Entry>() as u64;
    {
        let entries = memo.entries.borrow();
        if let Some((entry, _)) = entries.get(&key) {
            return entry.replay();
        }
        if entries.len() as u64 >= memo.maximum_entries {
            return Err(capacity(node, "tokenizer Finish cache entry budget exhausted"));
        }
    }
    let result = derive();
    let overflow = || capacity(u64::MAX, "tokenizer Finish cache retention size overflow");
    let extra = match &result {
        // The row and its nested fields are entirely inline. The typed
        // visitor guards this assumption against a future schema change.
        Ok(row) if row.native_heap_v1("tokenizer Finish cache result").bytes() == Some(0) => 0,
        Ok(_) => return Err(capacity(node, "tokenizer Finish cache row has unexpected heap ownership")),
        // The original Evidence or one replayed copy can coexist with the
        // retained error. Its actual capacity bounds the exact-length copy.
        Err(PalwVerifiedTokenizerRegistrationErrorV1::Evidence(reason)) => {
            u64::try_from(reason.capacity()).ok().and_then(|bytes| bytes.checked_mul(2)).ok_or_else(overflow)?
        }
        Err(_) => return result,
    };
    let bytes = node.checked_add(extra).ok_or_else(overflow)?;
    let lease = reserve(bytes).map_err(|why| capacity(bytes, &why.to_string()))?;
    let entry = match &result {
        Ok(row) => RetainedDerivationV1::Verified(row.clone()),
        Err(PalwVerifiedTokenizerRegistrationErrorV1::Evidence(reason)) => RetainedDerivationV1::Evidence(reason.as_str().to_owned()),
        Err(_) => unreachable!("only verified rows and evidence are retained"),
    };
    let mut entries = memo.entries.borrow_mut();
    let (_, lease) = entries.entry(key).or_insert((entry, lease));
    lease.materialized_v1();
    result
}

fn capacity(bytes: u64, reason: &str) -> PalwVerifiedTokenizerRegistrationErrorV1 {
    PalwVerifiedTokenizerRegistrationErrorV1::Capacity(MemoryBudgetError {
        class: MemoryClass::Native,
        requested_bytes: bytes,
        available_bytes: 0,
        external_reason: Some(reason.into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn same_parent_candidate_slot_and_eog_reuse_only_deterministic_results() {
        let h = Hash64::from_u64_word;
        let parent = PalwChainStateV2::genesis();
        let another_instance = parent.clone();
        let at = PalwBlockContextV2 { block: h(1), daa_score: 42, blue_score: 50, subsidy: 0 };
        let (mut policy, _) = crate::palw_tokenizer_registration_v1::tests::fixture();
        policy.service_budget.controls = 3;
        let cache = TokenizerFinishDerivationCacheV1::new(&parent, at, &policy).unwrap();
        let root = cache.parent_root_v1();
        let key = FinishDerivationKeyV1 { open: h(2), slot: 0, eog: h(3) };
        let calls = Cell::new(0);
        let failure = || {
            calls.set(calls.get() + 1);
            let mut reason = String::with_capacity(128);
            reason.push_str("bad tokenizer EOF");
            Err(PalwVerifiedTokenizerRegistrationErrorV1::Evidence(reason))
        };
        let run = |parent, root, point, policy: &_, key| {
            assert!(matches!(
                derive_tokenizer_finish_once_v1(parent, root, point, policy, key, failure),
                Err(PalwVerifiedTokenizerRegistrationErrorV1::Evidence(reason)) if reason == "bad tokenizer EOF"
            ));
        };
        cache.scope_v1(|| run(&parent, root, at, &policy, key));
        cache.scope_v1(|| run(&parent, root, at, &policy, key));
        assert_eq!(calls.get(), 1);
        let retained_bytes = cache.memo.entries.borrow().get(&key).unwrap().1.memory_v1();
        assert!(retained_bytes >= btree_node_envelope_bytes_v1::<FinishDerivationKeyV1, Entry>() as u64 + 2 * 128);
        cache.scope_v1(|| run(&another_instance, root, at, &policy, key));
        cache.scope_v1(|| run(&parent, h(9), at, &policy, key));
        cache.scope_v1(|| run(&parent, root, PalwBlockContextV2 { block: h(8), ..at }, &policy, key));
        cache.scope_v1(|| run(&parent, root, at, &TokenizerRegistrationPolicyV1 { rent_per_work: 2, ..policy }, key));
        run(&parent, root, at, &policy, key); // No process-global reuse outside the scope.
        assert_eq!(calls.get(), 6);

        let new_eog = FinishDerivationKeyV1 { eog: h(4), ..key };
        let new_slot = FinishDerivationKeyV1 { slot: 1, ..key };
        cache.scope_v1(|| {
            run(&parent, root, at, &policy, new_eog);
            let transient_calls = Cell::new(0);
            let transient = || {
                transient_calls.set(transient_calls.get() + 1);
                Err(capacity(8, "temporary resource error"))
            };
            for _ in 0..2 {
                assert!(matches!(
                    derive_tokenizer_finish_once_v1(&parent, root, at, &policy, new_slot, transient),
                    Err(PalwVerifiedTokenizerRegistrationErrorV1::Capacity(_))
                ));
            }
            assert_eq!(transient_calls.get(), 2);
            run(&parent, root, at, &policy, new_slot);
            run(&parent, root, at, &policy, new_slot);
            assert_eq!(calls.get(), 8);
            assert!(matches!(
                derive_tokenizer_finish_once_v1(&parent, root, at, &policy, FinishDerivationKeyV1 { open: h(5), ..key }, failure),
                Err(PalwVerifiedTokenizerRegistrationErrorV1::Capacity(_))
            ));
            assert_eq!(calls.get(), 8); // A new key at capacity cannot run the derivation.
            run(&parent, root, at, &policy, key); // Budget exhaustion cannot evict an earlier result.
            assert_eq!(calls.get(), 8);
        });
    }
}
