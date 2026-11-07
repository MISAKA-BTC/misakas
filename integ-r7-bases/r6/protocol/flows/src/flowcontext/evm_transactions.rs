//! kaspa-pq EVM Lane v0.4 (§14.2): pending-EVM-tx inv spread.
//!
//! Mirrors [`super::transactions::TransactionsSpread`] but is deliberately
//! LOWER priority than UTXO tx gossip (§14.2 network rules): its own queue
//! (no shared budget with UTXO invs), a 4x longer batching interval, and a
//! receiver-side overflow policy of Drop (see `IncomingRouteOverflowPolicy`),
//! so EVM gossip sheds under pressure instead of competing with UTXO traffic.
//!
//! Invs are only sent to peers that negotiated protocol ≥ 101: older peers
//! have no route for the EVM message types and routing an unknown type
//! disconnects them.

use super::process_queue::ProcessQueue;
use crate::flow_context::PROTOCOL_VERSION_EVM_RELAY;
use itertools::Itertools;
use kaspa_core::debug;
use kaspa_hashes::EvmH256;
use kaspa_p2p_lib::{
    Hub, make_message,
    pb::{InvEvmTransactionsMessage, kaspad_message::Payload},
};
use std::time::{Duration, Instant};

/// §14.2 low-priority cadence: EVM invs batch 4x longer than UTXO tx invs (500ms).
const BROADCAST_INTERVAL: Duration = Duration::from_millis(2000);
/// Cap on hashes per EVM tx inv message. The EVM mempool holds at most 4096
/// txs, so one inv message can always announce a meaningful pool fraction.
pub(crate) const MAX_INV_PER_EVM_TX_INV_MSG: usize = 512;

pub struct EvmTransactionsSpread {
    hub: Hub,
    tx_hashes: ProcessQueue<EvmH256>,
    last_broadcast_time: Instant,
}

impl EvmTransactionsSpread {
    pub fn new(hub: Hub) -> Self {
        Self { hub, tx_hashes: ProcessQueue::new(), last_broadcast_time: Instant::now() }
    }

    /// Queue the given pending-EVM-tx hashes for inv broadcast to EVM-relay-capable
    /// peers. Like the UTXO spread, the actual send happens at most every
    /// `BROADCAST_INTERVAL` or when a full inv message is pending.
    ///
    /// NOTE: this only flushes when its own interval has elapsed, so the tail of
    /// a burst (everything after the call that triggers the flush) waits for the
    /// NEXT pump. Unlike the UTXO spread — which the per-block relay path pumps
    /// continuously — the EVM spread is submit-driven, so a low-rate submitter's
    /// tail would sit unsent. [`FlowContext`] therefore also calls [`Self::flush_due`]
    /// on every block to pump the tail (mirrors the UTXO spread's cadence).
    pub async fn broadcast_evm_transactions<I: IntoIterator<Item = EvmH256>>(&mut self, tx_hashes: I) {
        self.tx_hashes.enqueue_chunk(tx_hashes);

        let now = Instant::now();
        if now < self.last_broadcast_time + BROADCAST_INTERVAL && self.tx_hashes.len() < MAX_INV_PER_EVM_TX_INV_MSG {
            return;
        }
        self.drain_queue().await;
    }

    /// Pump: if the batching interval has elapsed and anything is queued, send
    /// it. Called per-block by [`FlowContext`] so a submit burst's tail does not
    /// linger until the next submit (relay-liveness fix). Cheap no-op when the
    /// queue is empty or the interval has not elapsed.
    pub async fn flush_due(&mut self) {
        if self.tx_hashes.is_empty() || Instant::now() < self.last_broadcast_time + BROADCAST_INTERVAL {
            return;
        }
        self.drain_queue().await;
    }

    async fn drain_queue(&mut self) {
        while !self.tx_hashes.is_empty() {
            let hashes = self.tx_hashes.dequeue_chunk(MAX_INV_PER_EVM_TX_INV_MSG).map(|h| h.as_bytes().to_vec()).collect_vec();
            debug!("EVM transaction propagation: broadcasting {} transactions", hashes.len());
            let msg = make_message!(Payload::InvEvmTransactions, InvEvmTransactionsMessage { hashes });
            self.hub.broadcast_to_peers_with_min_version(msg, PROTOCOL_VERSION_EVM_RELAY).await;
        }
        self.last_broadcast_time = Instant::now();
    }
}
