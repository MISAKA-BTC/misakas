//! kaspa-pq EVM Lane v0.4 (§14.2 / §9.2): pending EVM deposit-claim inv spread.
//!
//! Sibling of [`super::evm_transactions::EvmTransactionsSpread`]: same low-priority
//! profile (own queue, 4x longer batching interval, Drop overflow), so claim
//! gossip sheds under pressure instead of competing with UTXO traffic.
//!
//! A claim's identity is its deposit-lock [`TransactionOutpoint`] (NOT a hash),
//! so the inv carries `Outpoint`s. Invs are only sent to peers that negotiated
//! protocol ≥ 102 (the deposit-claim-relay-capable peer set); older peers (incl.
//! 101 EVM-tx-relay-only) have no route
//! for the claim message types and routing an unknown type disconnects them.

use super::process_queue::ProcessQueue;
use crate::flow_context::PROTOCOL_VERSION_CLAIM_RELAY;
use itertools::Itertools;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_core::debug;
use kaspa_p2p_lib::{
    Hub, make_message,
    pb::{InvEvmDepositClaimsMessage, kaspad_message::Payload},
};
use std::time::{Duration, Instant};

/// §14.2 low-priority cadence: claim invs batch 4x longer than UTXO tx invs.
const BROADCAST_INTERVAL: Duration = Duration::from_millis(2000);
/// Cap on outpoints per claim inv message. The claim queue holds at most
/// `EVM_MEMPOOL_MAX_CLAIMS` (4096), so one inv can announce a meaningful fraction.
pub(crate) const MAX_INV_PER_EVM_DEPOSIT_CLAIM_INV_MSG: usize = 512;

pub struct EvmDepositClaimsSpread {
    hub: Hub,
    claim_outpoints: ProcessQueue<TransactionOutpoint>,
    last_broadcast_time: Instant,
}

impl EvmDepositClaimsSpread {
    pub fn new(hub: Hub) -> Self {
        Self { hub, claim_outpoints: ProcessQueue::new(), last_broadcast_time: Instant::now() }
    }

    /// Queue the given deposit-lock outpoints for inv broadcast to EVM-relay-capable
    /// peers. Like the EVM-tx spread, the actual send happens at most every
    /// `BROADCAST_INTERVAL` or when a full inv message is pending. The tail of a
    /// burst waits for the next pump ([`Self::flush_due`], called per-block by
    /// [`crate::flow_context::FlowContext`]).
    pub async fn broadcast_evm_deposit_claims<I: IntoIterator<Item = TransactionOutpoint>>(&mut self, outpoints: I) {
        self.claim_outpoints.enqueue_chunk(outpoints);

        let now = Instant::now();
        if now < self.last_broadcast_time + BROADCAST_INTERVAL && self.claim_outpoints.len() < MAX_INV_PER_EVM_DEPOSIT_CLAIM_INV_MSG {
            return;
        }
        self.drain_queue().await;
    }

    /// Pump: if the batching interval elapsed and anything is queued, send it.
    /// Called per-block so a submit burst's tail does not linger until the next
    /// submit (relay-liveness). Cheap no-op when empty or the interval has not elapsed.
    pub async fn flush_due(&mut self) {
        if self.claim_outpoints.is_empty() || Instant::now() < self.last_broadcast_time + BROADCAST_INTERVAL {
            return;
        }
        self.drain_queue().await;
    }

    async fn drain_queue(&mut self) {
        while !self.claim_outpoints.is_empty() {
            let outpoints =
                self.claim_outpoints.dequeue_chunk(MAX_INV_PER_EVM_DEPOSIT_CLAIM_INV_MSG).map(|o| (&o).into()).collect_vec();
            debug!("EVM deposit-claim propagation: broadcasting {} claims", outpoints.len());
            let msg = make_message!(Payload::InvEvmDepositClaims, InvEvmDepositClaimsMessage { outpoints });
            self.hub.broadcast_to_peers_with_min_version(msg, PROTOCOL_VERSION_CLAIM_RELAY).await;
        }
        self.last_broadcast_time = Instant::now();
    }
}
