//! Retained simulation-only setup. Legacy executor setup remains unchanged.
use super::*;
use kaspa_consensus_core::evm::view_owned_v2::NativeEvmViewOwnedV2;
use kaspa_consensus_core::palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1};
use std::{mem::size_of, ops::Deref, sync::Mutex};

#[derive(Clone)]
pub(super) enum ViewOwnerV2 {
    Legacy(Arc<PalwEvmViewV1>),
    Retained(NativeEvmViewOwnedV2),
}
impl Deref for ViewOwnerV2 {
    type Target = PalwEvmViewV1;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Legacy(v) => v,
            Self::Retained(v) => v,
        }
    }
}
/// Each legacy collect overwrites the same-address row in view iteration order.
/// Sorting ordinal descending keeps that exact last winner, even on collision.
type FacadeEntryV2 = (Address, Hash64, usize);
pub(super) struct RetainedSetupV2 {
    facades: Vec<FacadeEntryV2>,
    queued: AtomicUsize,
    accounts: Mutex<Vec<(Address, usize)>>,
    _lease: Option<NativeDemandLeaseV1>,
}
impl RetainedSetupV2 {
    pub(super) fn quote(lines: usize) -> Result<u64, NativeCoverageV1> {
        let fixed = size_of::<Self>() + 3 * size_of::<usize>();
        let facades = lines.checked_mul(size_of::<FacadeEntryV2>());
        let accounts = MAX_MARKET_ACTIONS_PER_EVM_BLOCK.checked_mul(size_of::<(Address, usize)>());
        fixed
            .checked_add(facades.ok_or_else(Self::overflow)?)
            .and_then(|n| n.checked_add(accounts?))
            .map(|n| n as u64)
            .ok_or_else(Self::overflow)
    }
    fn overflow() -> NativeCoverageV1 {
        NativeCoverageV1::overflow::<Self>("retained EVM market setup")
    }
    fn new(view: &PalwEvmViewV1) -> Result<Arc<Self>, NativeCoverageV1> {
        let lease = reserve_native_requests_v1(Self::quote(view.lines.len())?)?;
        let mut facades = Vec::with_capacity(view.lines.len());
        for (ordinal, (id, _)) in view.lines.iter().enumerate() {
            facades.push((to_address(&facade_address_v1(id)), *id, ordinal));
        }
        // In-place unstable sort uses no heap scratch; ordinal makes keys total.
        Self::sort_facades(&mut facades);
        Ok(Arc::new(Self {
            facades,
            queued: AtomicUsize::new(0),
            accounts: Mutex::new(Vec::with_capacity(MAX_MARKET_ACTIONS_PER_EVM_BLOCK)),
            _lease: lease,
        }))
    }
    fn sort_facades(facades: &mut Vec<FacadeEntryV2>) {
        facades.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.2.cmp(&a.2)));
        facades.dedup_by_key(|entry| entry.0);
    }
    pub(super) fn facade(&self, address: &Address) -> Option<&Hash64> {
        self.facades.binary_search_by_key(address, |entry| entry.0).ok().map(|i| &self.facades[i].1)
    }
    pub(super) fn queued(&self) -> usize {
        self.queued.load(Ordering::SeqCst)
    }
    pub(super) fn queued_for(&self, account: &Address) -> usize {
        self.accounts.lock().map(|a| a.iter().find(|(who, _)| who == account).map_or(0, |(_, n)| *n)).unwrap_or(0)
    }
    pub(super) fn reset(&self, actions: &[PalwEvmMarketActionV1]) {
        self.queued.store(actions.len(), Ordering::SeqCst);
        if let Ok(mut accounts) = self.accounts.lock() {
            accounts.clear();
            for action in actions {
                let account = Address::from(action.account.as_bytes());
                if let Some((_, n)) = accounts.iter_mut().find(|(who, _)| *who == account) {
                    *n += 1;
                } else {
                    accounts.push((account, 1));
                }
            }
        }
    }
    pub(super) fn record(&self, account: Address) {
        self.queued.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut accounts) = self.accounts.lock() {
            if let Some((_, n)) = accounts.iter_mut().find(|(who, _)| *who == account) {
                *n += 1;
            } else {
                accounts.push((account, 1));
            }
        }
    }
}
impl MarketHandlers {
    /// Private to this crate's simulation. Each call gets fresh counters and
    /// never calls executor set_committed. The existing intercept rejects once
    /// queued==128 BEFORE record(), so <=128 distinct accounts can be inserted,
    /// including repeated subcalls and reverted subcalls in one EVM execution.
    pub(crate) fn new_retained_v2(
        view: NativeEvmViewOwnedV2,
        fences: PalwEvmMarketFencesV1,
        chain_id: u64,
    ) -> Result<Self, NativeCoverageV1> {
        let retained = RetainedSetupV2::new(&view)?;
        Ok(Self {
            view: ViewOwnerV2::Retained(view),
            fences,
            chain_id,
            facades: None,
            queued: None,
            queued_by_account: None,
            retained: Some(retained),
        })
    }
    pub(super) fn facade_v2(&self, address: &Address) -> Option<&Hash64> {
        match &self.retained {
            Some(setup) => setup.facade(address),
            None => self.facades.as_ref().and_then(|f| f.get(address)),
        }
    }
    pub(super) fn record_v2(&self, account: Address) {
        if let Some(setup) = &self.retained {
            setup.record(account);
            return;
        }
        if let Some(queued) = &self.queued {
            queued.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(accounts) = &self.queued_by_account
            && let Ok(mut accounts) = accounts.lock()
        {
            *accounts.entry(account).or_insert(0) += 1;
        }
    }
}

#[cfg(test)]
#[path = "retained_tests_v2.rs"]
mod tests;
