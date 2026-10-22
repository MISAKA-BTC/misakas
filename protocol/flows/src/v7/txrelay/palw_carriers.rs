//! **Liveness hotfix (L-LC-02 / L-COURT-01): the relay flow's half of the H-1 carrier guard — node-local, never a block rule.**
//!
//! The mempool refuses an H-1 carrier its tip's fold would refuse (`palw_mempool_h1_carrier_refusal`), and that rehearsal is the
//! node's most expensive admission work (a state copy; for a completing split close, the assembly and adjudication of up to 2.7 MB).
//! The refusal was `RejectTxRule`, which this flow ignored, so a peer could relay refused carriers forever. Two rules, per peer:
//!
//! * **Penalty, only for a refusal that holds at every tip** — `TxRuleError::PalwH1CarrierInvalid` (a completing close that does not
//!   adjudicate to its declaration, a possession proof whose shape, challenge or immutable root is wrong). The peer is disconnected as
//!   misbehaving, exactly as `RejectInvalid` would. A refusal that can be an honest race (`PalwH1CarrierRefused`: a stale tip, a span
//!   one block ahead, a missing row) and the node's own budget (`PalwH1CarrierThrottled`) are never penalised.
//! * **A per-peer budget** of H-1 carriers handed to the mempool ([`PALW_PEER_CARRIER_BURST_V1`], refilled at
//!   [`PALW_PEER_CARRIERS_PER_SEC_V1`]): over it the carrier is not submitted (nor penalised) — one peer cannot spend the node's whole
//!   global rehearsal budget (consensus `palw_h1_guard`). Only where R-core+ (the gate's fence) is scheduled at all.

use kaspa_consensus_core::{errors::tx::TxRuleError, tx::Transaction};
use kaspa_mining::{errors::MiningManagerError, mempool::errors::RuleError};
use std::time::Instant;

/// H-1 carriers one peer may hand the mempool in a burst ...
pub(crate) const PALW_PEER_CARRIER_BURST_V1: u64 = 64;
/// ... refilled at this many a second. Honest H-1 traffic is a few carriers a block network-wide.
pub(crate) const PALW_PEER_CARRIERS_PER_SEC_V1: u64 = 16;

/// Whether a mempool refusal of a relayed transaction makes the relaying peer misbehaving: only an H-1 carrier refused for a reason
/// that holds at every tip. Timing refusals and the node's own budget are never the peer's fault.
pub(crate) fn palw_relay_refusal_is_misbehaviour_v1(error: &MiningManagerError) -> bool {
    matches!(error, MiningManagerError::MempoolError(RuleError::RejectTxRule(TxRuleError::PalwH1CarrierInvalid(_))))
}

/// Whether `tx` is a carrier the mempool's H-1 gate rehearses (a lifecycle transaction carrying an H-1 or readiness object).
pub(crate) fn palw_is_gated_carrier_v1(tx: &Transaction) -> bool {
    kaspa_consensus_core::palw_readiness_escalation_v1::palw_gated_carrier_object_of_tx_v1(tx).is_some()
}

/// One peer's carrier budget: a token bucket over a monotonic clock.
pub(crate) struct PalwPeerCarrierBudgetV1 {
    burst: u64,
    per_sec: u64,
    milli: u64,
    last: Option<Instant>,
}

impl Default for PalwPeerCarrierBudgetV1 {
    fn default() -> Self {
        Self::new(PALW_PEER_CARRIER_BURST_V1, PALW_PEER_CARRIERS_PER_SEC_V1)
    }
}

impl PalwPeerCarrierBudgetV1 {
    pub(crate) fn new(burst: u64, per_sec: u64) -> Self {
        Self { burst, per_sec, milli: burst.saturating_mul(1_000), last: None }
    }

    /// Admit one carrier at `now`; `false` (and nothing taken) when the peer's budget is spent.
    pub(crate) fn admit(&mut self, now: Instant) -> bool {
        if let Some(last) = self.last {
            let elapsed_ms = now.saturating_duration_since(last).as_millis().min(u64::MAX as u128) as u64;
            self.milli = self.milli.saturating_add(elapsed_ms.saturating_mul(self.per_sec)).min(self.burst.saturating_mul(1_000));
        }
        self.last = Some(now);
        if self.milli < 1_000 {
            return false;
        }
        self.milli -= 1_000;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::TransactionId;
    use std::time::Duration;

    #[test]
    fn only_an_every_tip_invalid_carrier_is_the_peers_fault() {
        let wrap = |e: TxRuleError| MiningManagerError::MempoolError(RuleError::RejectTxRule(e));
        assert!(palw_relay_refusal_is_misbehaviour_v1(&wrap(TxRuleError::PalwH1CarrierInvalid("junk proof".into()))));
        assert!(!palw_relay_refusal_is_misbehaviour_v1(&wrap(TxRuleError::PalwH1CarrierRefused("span ahead".into()))));
        assert!(!palw_relay_refusal_is_misbehaviour_v1(&wrap(TxRuleError::PalwH1CarrierThrottled("busy".into()))));
        assert!(!palw_relay_refusal_is_misbehaviour_v1(&MiningManagerError::MempoolError(RuleError::RejectDuplicate(
            TransactionId::default()
        ))));
    }

    #[test]
    fn a_peer_spends_its_burst_then_its_rate() {
        let t0 = Instant::now();
        let mut budget = PalwPeerCarrierBudgetV1::new(3, 2);
        assert!((0..3).all(|_| budget.admit(t0)), "the burst");
        assert!(!budget.admit(t0), "spent");
        assert!(budget.admit(t0 + Duration::from_millis(500)), "two a second: one after half a second");
        assert!(!budget.admit(t0 + Duration::from_millis(500)));
        assert_eq!((0..10).filter(|_| budget.admit(t0 + Duration::from_secs(100))).count(), 3, "refilled to the burst, no further");
    }
}
