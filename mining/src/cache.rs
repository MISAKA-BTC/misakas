use kaspa_consensus_core::block::{BlockTemplate, VirtualStateApproxId};
use kaspa_core::time::unix_now;
use parking_lot::{Mutex, MutexGuard};
use std::sync::Arc;

/// CACHE_LIFETIME indicates the default duration in milliseconds after which the cached data expires.
const DEFAULT_CACHE_LIFETIME: u64 = 1_000;

pub(crate) struct Inner {
    /// Time, in milliseconds, at which the cache was last updated
    last_update_time: u64,

    /// The optional template
    block_template: Option<Arc<BlockTemplate>>,

    /// Duration in milliseconds after which the cached data expires
    cache_lifetime: u64,
}

impl Inner {
    pub(crate) fn new(cache_lifetime: Option<u64>) -> Self {
        let cache_lifetime = cache_lifetime.unwrap_or(DEFAULT_CACHE_LIFETIME);
        Self { last_update_time: 0, block_template: None, cache_lifetime }
    }

    pub(crate) fn clear(&mut self) {
        self.block_template = None;
    }

    pub(crate) fn get_immutable_cached_template(&mut self) -> Option<Arc<BlockTemplate>> {
        let now = unix_now();
        // We verify that `now > last update` in order to avoid theoretic clock change bugs
        if now.checked_sub(self.last_update_time).is_none_or(|elapsed| elapsed > self.cache_lifetime) {
            // An unusable cache entry must release its retained reservation
            // before the replacement template asks for capacity.
            self.clear();
            None
        } else {
            self.block_template.clone()
        }
    }

    pub(crate) fn set_immutable_cached_template(&mut self, block_template: BlockTemplate) -> Arc<BlockTemplate> {
        self.last_update_time = unix_now();
        let block_template = Arc::new(block_template);
        self.block_template = Some(block_template.clone());
        block_template
    }
}

pub(crate) struct BlockTemplateCache {
    inner: Mutex<Inner>,
}

impl BlockTemplateCache {
    pub(crate) fn new(cache_lifetime: Option<u64>) -> Self {
        Self { inner: Mutex::new(Inner::new(cache_lifetime)) }
    }

    // Invalidate the cached template so a freshly admitted tx is included on the next
    // get_block_template. Used by tests, the EVM submit path, and the kaspa-pq
    // DNS-finality (E4/§6.2) attestation-admission path — when a subnetwork-0x11
    // `StakeAttestationShard` is accepted into the mempool, the cached (possibly
    // near-empty) template must be dropped so the next template re-runs the
    // selection-loop classifier and picks up the fresh shard instead of serving a
    // stale template for up to the cache lifetime.
    pub(crate) fn clear(&self) {
        self.inner.lock().clear();
    }

    pub(crate) fn lock(&self, virtual_state_approx_id: VirtualStateApproxId) -> MutexGuard<'_, Inner> {
        let mut guard = self.inner.lock();
        if guard.block_template.as_ref().is_some_and(|template| template.to_virtual_state_approx_id() != virtual_state_approx_id) {
            // If the VirtualStateApproxId is different from ours, our template is likely expired and we should clear it
            guard.clear();
        }
        guard
    }
}

#[cfg(test)]
mod owner_lifetime_tests {
    use super::*;
    use kaspa_consensus_core::{
        block::MutableBlock,
        coinbase::MinerData,
        config::params::MAINNET_PARAMS,
        tx::{ScriptPublicKey, scriptvec},
    };

    // Only Arc ownership is under test; this is not a coinbase acceptance or
    // quota fixture. The retained allocation owner lives inside BlockTemplate.
    fn template() -> BlockTemplate {
        BlockTemplate::new(
            MutableBlock::new((&MAINNET_PARAMS.genesis).into(), vec![]),
            MinerData::new(ScriptPublicKey::new(0, scriptvec![0x51]), vec![]),
            false,
            vec![],
            0,
            0,
            MAINNET_PARAMS.genesis.hash,
            vec![],
            vec![],
            vec![],
        )
    }

    fn invalidate_clock(cache: &mut Inner, rollback: bool) {
        // No sleeps or wall-clock boundary race: epoch zero has expired, and
        // u64::MAX is strictly after every practical unix millisecond clock.
        cache.cache_lifetime = 0;
        cache.last_update_time = if rollback { u64::MAX } else { 0 };
    }

    #[test]
    fn expiry_and_clock_rollback_release_the_final_cached_owner() {
        for rollback in [false, true] {
            let mut cache = Inner::new(None);
            let external = cache.set_immutable_cached_template(template());
            let weak = Arc::downgrade(&external);
            assert_eq!(weak.strong_count(), 2);
            drop(external);
            assert_eq!(weak.strong_count(), 1, "only the cache should still own the template");
            invalidate_clock(&mut cache, rollback);
            assert!(cache.get_immutable_cached_template().is_none());
            assert!(cache.block_template.is_none());
            assert!(weak.upgrade().is_none(), "an unusable cache must drop its final template/allocation owner");
        }
    }

    #[test]
    fn invalidating_cache_preserves_an_external_template_owner_until_drop() {
        for rollback in [false, true] {
            let mut cache = Inner::new(None);
            let external = cache.set_immutable_cached_template(template());
            let weak = Arc::downgrade(&external);
            invalidate_clock(&mut cache, rollback);
            assert!(cache.get_immutable_cached_template().is_none());
            assert!(cache.block_template.is_none());
            assert_eq!(weak.strong_count(), 1, "an external consumer retains its own lifetime");
            let retained = weak.upgrade().expect("cache invalidation cannot destroy an external owner's template");
            assert!(Arc::ptr_eq(&retained, &external));
            drop(retained);
            drop(external);
            assert!(weak.upgrade().is_none());
        }
    }
}
