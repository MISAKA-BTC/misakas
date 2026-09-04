use crate::flowcontext::{
    evm_deposit_claims::EvmDepositClaimsSpread,
    evm_transactions::EvmTransactionsSpread,
    ibd_candidates::{CHALLENGER_VERIFICATION_LEASE, CandidateId, CandidateValidation, IbdCandidateRegistry, PreferredIbdCandidate},
    orphans::{OrphanBlocksPool, OrphanOutput},
    process_queue::ProcessQueue,
    recovery_trace::{RecoveryStage, record_stage},
    transactions::TransactionsSpread,
};
use crate::{v7, v8};
use async_trait::async_trait;
use futures::future::join_all;
use kaspa_addressmanager::AddressManager;
use kaspa_connectionmanager::ConnectionManager;
use kaspa_consensus_core::api::{BlockValidationFuture, BlockValidationFutures};
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::evm::DepositClaim;
use kaspa_consensus_core::fork_id_v1::ForkIdVerdict;
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::{BlockHash, BlueWorkType}; // PR-9.5e: block hashes are Hash64
use kaspa_consensus_core::{
    subnets::SUBNETWORK_ID_STAKE_ATTESTATION_SHARD,
    tx::{Transaction, TransactionId, TransactionOutpoint},
};
use kaspa_consensus_notify::{
    notification::{Notification, PruningPointUtxoSetOverrideNotification},
    root::ConsensusNotificationRoot,
};
use kaspa_consensusmanager::{BlockProcessingBatch, ConsensusInstance, ConsensusManager, ConsensusProxy, ConsensusSessionOwned};
use kaspa_core::{
    chain_participation::{ChainParticipationGate, IbdLease},
    debug, info,
    kaspad_env::{name, version},
    task::tick::TickService,
};
use kaspa_core::{time::unix_now, warn};
use kaspa_hashes::EvmH256;
use kaspa_mining::evm_mempool::EvmMempoolError;
use kaspa_mining::mempool::tx::{Orphan, Priority};
use kaspa_mining::{manager::MiningManagerProxy, mempool::tx::RbfPolicy};
use kaspa_notify::notifier::Notify;
use kaspa_p2p_lib::{
    ConnectionInitializer, Hub, KaspadHandshake, PeerKey, PeerProperties, Router,
    common::ProtocolError,
    convert::model::version::Version,
    make_message,
    pb::{InvRelayBlockMessage, kaspad_message::Payload},
};
use kaspa_p2p_mining::rule_engine::MiningRuleEngine;
use kaspa_utils::iter::IterExtensions;
use kaspa_utils::networking::PeerId;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::time::Instant;
use std::{collections::hash_map::Entry, fmt::Display};
use std::{
    iter::once,
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{
    RwLock as AsyncRwLock, broadcast,
    mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
};
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};
use uuid::Uuid;

/// The P2P protocol version.
//
// kaspa-pq protocol version: bumped well above upstream Kaspa (which is at 9)
// so that any handshake with a mainline Kaspa peer fails the version check
// immediately. See docs/adr/0001-network-isolation.md.
//
// 101 (EVM Lane §14.2) adds the pending-EVM-tx relay messages; 102 adds the EVM
// deposit-claim relay messages (oneof 67-70); 103 adds `genesisHash` /
// `consensusParamsId` to the handshake. Peers below 103 send those empty, which the
// handshake treats as a mismatch — a peer that cannot state which rules it runs is
// exactly the testnet-22 shape, so it is disconnected rather than waved through. Lower-version peers are still fully
// served (they negotiate the same flow set minus the newer relay flows), but must
// never be sent a message they have no route for — routing an unknown payload type
// disconnects the peer, so all EVM gossip is version-filtered to the exact peer set
// that understands it (EVM-tx ≥101, deposit-claim ≥102).
const PROTOCOL_VERSION: u32 = 105;
/// The last protocol version WITHOUT the EVM relay messages (still accepted).
const PROTOCOL_VERSION_NO_EVM_RELAY: u32 = 100;
/// The minimum protocol version that understands the EVM-tx relay messages.
pub(crate) const PROTOCOL_VERSION_EVM_RELAY: u32 = 101;
/// The minimum protocol version that understands the EVM deposit-claim relay
/// messages. 101 peers (EVM-tx relay only) and older must NEVER be sent a claim
/// message (unroutable → disconnect), so claim gossip is filtered to >= this.
pub(crate) const PROTOCOL_VERSION_CLAIM_RELAY: u32 = 102;
/// The PALW material PULL (`PalwMaterialRequest`, oneof 77). Only the REQUEST is gated: the serve
/// side answers on the pre-existing broadcast message, which every older peer already routes.
/// A 103 peer simply never gets asked — it can still hear, hold and push materials as before.
pub(crate) const PROTOCOL_VERSION_PALW_PULL: u32 = 104;
/// The 104-set minus the material pull: everything a 103 peer can route.
pub(crate) const PROTOCOL_VERSION_PRE_PALW_PULL: u32 = 103;
/// **The interval lane** (ADR-0077 Decision 8: `PalwIntervalOpeningRequest` /
/// `PalwIntervalOpening`, oneof 78/79). BOTH directions are gated, unlike the material pull, whose
/// answer rides a message every peer already routes: here the answer is a new type too, so it can
/// only go to a peer that could ask. A 104 peer keeps the whole 104 set — material push, pull,
/// receipts — and is simply never asked for an opening; a seat that needs one from such an
/// executor hears nothing and reaches the half-window arm, which is the same outcome as an
/// executor that is not running at all.
pub(crate) const PROTOCOL_VERSION_PALW_INTERVAL: u32 = 105;

/// See `check_orphan_resolution_range`
const BASELINE_ORPHAN_RESOLUTION_RANGE: u32 = 5;

/// Orphans are kept as full blocks so we cannot hold too much of them in memory
const MAX_ORPHANS_UPPER_BOUND: usize = 1024;

/// How many challenger nominations may queue before the oldest is dropped.
///
/// Small on purpose: a nomination is a hint about what is worth checking right now, and a stale one
/// is worse than none. A flow that misses one will be nominated again on the next summary.
const CHALLENGER_NOMINATION_BACKLOG: usize = 16;

/// The min time to wait before allowing another parallel request
const REQUEST_SCOPE_WAIT_TIME: Duration = Duration::from_secs(1);

/// Represents a block event to be logged
#[derive(Debug, PartialEq)]
pub enum BlockLogEvent {
    /// Accepted block via *relay*
    Relay(BlockHash),
    /// Accepted block via *submit block*
    Submit(BlockHash),
    /// Orphaned block with x missing roots
    Orphaned(BlockHash, usize),
    /// Unorphaned x blocks with hash being a representative
    Unorphaned(BlockHash, usize),
}

pub struct BlockEventLogger {
    bps: usize,
    sender: UnboundedSender<BlockLogEvent>,
    receiver: Mutex<Option<UnboundedReceiver<BlockLogEvent>>>,
}

impl BlockEventLogger {
    pub fn new(bps: usize) -> Self {
        let (sender, receiver) = unbounded_channel();
        Self { bps, sender, receiver: Mutex::new(Some(receiver)) }
    }

    pub fn log(&self, event: BlockLogEvent) {
        self.sender.send(event).unwrap();
    }

    /// Start the logger listener. Must be called from an async tokio context
    fn start(&self) {
        let chunk_limit = self.bps * 10; // We prefer that the 1 sec timeout forces the log, but nonetheless still want a reasonable bound on each chunk
        let receiver = self.receiver.lock().take().expect("expected to be called once");
        tokio::spawn(async move {
            let chunk_stream = UnboundedReceiverStream::new(receiver).chunks_timeout(chunk_limit, Duration::from_secs(1));
            tokio::pin!(chunk_stream);
            while let Some(chunk) = chunk_stream.next().await {
                #[derive(Default)]
                struct LogSummary {
                    // Representatives
                    relay_rep: Option<BlockHash>,
                    submit_rep: Option<BlockHash>,
                    orphan_rep: Option<BlockHash>,
                    unorphan_rep: Option<BlockHash>,
                    // Counts
                    relay_count: usize,
                    submit_count: usize,
                    orphan_count: usize,
                    unorphan_count: usize,
                    orphan_roots_count: usize,
                }

                struct LogHash {
                    op: Option<BlockHash>,
                }

                impl From<Option<BlockHash>> for LogHash {
                    fn from(op: Option<BlockHash>) -> Self {
                        Self { op }
                    }
                }

                impl Display for LogHash {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        if let Some(hash) = self.op { hash.fmt(f) } else { Ok(()) }
                    }
                }

                impl LogSummary {
                    fn relay(&self) -> LogHash {
                        self.relay_rep.into()
                    }

                    fn submit(&self) -> LogHash {
                        self.submit_rep.into()
                    }

                    fn orphan(&self) -> LogHash {
                        self.orphan_rep.into()
                    }

                    fn unorphan(&self) -> LogHash {
                        self.unorphan_rep.into()
                    }
                }

                let summary = chunk.into_iter().fold(LogSummary::default(), |mut summary, ev| {
                    match ev {
                        BlockLogEvent::Relay(hash) => {
                            summary.relay_count += 1;
                            summary.relay_rep = Some(hash)
                        }
                        BlockLogEvent::Submit(hash) => {
                            summary.submit_count += 1;
                            summary.submit_rep = Some(hash)
                        }
                        BlockLogEvent::Orphaned(hash, roots_count) => {
                            summary.orphan_roots_count += roots_count;
                            summary.orphan_count += 1;
                            summary.orphan_rep = Some(hash)
                        }
                        BlockLogEvent::Unorphaned(hash, count) => {
                            summary.unorphan_count += count;
                            summary.unorphan_rep = Some(hash)
                        }
                    }
                    summary
                });

                match (summary.submit_count, summary.relay_count) {
                    (0, 0) => {}
                    (1, 0) => info!("Accepted block {} via submit block", summary.submit()),
                    (n, 0) => info!("Accepted {} blocks ...{} via submit block", n, summary.submit()),
                    (0, 1) => info!("Accepted block {} via relay", summary.relay()),
                    (0, m) => info!("Accepted {} blocks ...{} via relay", m, summary.relay()),
                    (n, m) => {
                        info!("Accepted {} blocks ...{}, {} via relay and {} via submit block", n + m, summary.submit(), m, n)
                    }
                }

                match (summary.orphan_count, summary.orphan_roots_count) {
                    (0, 0) => {}
                    (n, m) => info!("Orphaned {} block(s) ...{} and queued {} missing roots", n, summary.orphan(), m),
                }

                match summary.unorphan_count {
                    0 => {}
                    1 => info!("Unorphaned block {}", summary.unorphan()),
                    n => info!("Unorphaned {} block(s) ...{}", n, summary.unorphan()),
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        constants::TX_VERSION,
        subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD},
    };

    fn tx(subnetwork_id: kaspa_consensus_core::subnets::SubnetworkId) -> Transaction {
        Transaction::new(TX_VERSION, vec![], vec![], 0, subnetwork_id, 0, vec![])
    }

    /// **A node's own reserved funding outpoints are answerable, and de-duplicated** (audit3 H12).
    ///
    /// Found in production, not by reading: `wallet send` from the producer's pay address selected
    /// the panel's `--palw-fee-outpoint` and was rejected only because the panel's own carrier had
    /// already spent it in the mempool that second. Had the timing differed the send would have
    /// succeeded and taken the panel's funding — and a panel that cannot carry a lifecycle object
    /// is the failure the M2-10 startup gate exists to prevent: "not a lost dispute, it is a
    /// slashed bond". H3 taught the wallet about consensus-locked collateral; this is the second
    /// reserved outpoint, and it is node-local so consensus cannot answer for it.
    #[test]
    fn reserved_outpoints_are_declared_once_and_come_back_sorted() {
        use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
        let reserved = ReservedOutpoints::default();
        assert!(reserved.all().is_empty(), "a node reserves nothing until its panel says so");

        let configured = TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xBB), index: 41 };
        let rolling = TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xAA), index: 0 };
        reserved.reserve(configured);
        reserved.reserve(rolling);
        reserved.reserve(configured); // the panel re-declares on every tick

        let all = reserved.all();
        assert_eq!(all.len(), 2, "re-declaring is idempotent; both the configured and the rolling outpoint are held");
        assert_eq!(all[0], rolling, "and the answer is ordered, so two readers of this RPC agree");
        assert_eq!(all[1], configured);

        // The property the wallet actually depends on: the panel's fee outpoint is in the set the
        // node hands out, so the input selector can skip it.
        assert!(all.contains(&configured), "the configured --palw-fee-outpoint must be answerable");
    }

    /// **One peer's `found: false` must not end a node's life** (audit3 H2/H9).
    ///
    /// The server holds exactly one pruning-point snapshot and serves it only on an exact match, so
    /// `found: false` is what any peer answers once its own pruning point has advanced — which it
    /// does routinely during a multi-minute header sync. That answer used to reach
    /// `chain_participation().quarantine()`, whose only exit is an operator restarting with
    /// `--clear-quarantine`: the node stopped mining, stopped attesting, and reported
    /// `is_synced=false` across restarts, for a chain it had committed correctly.
    #[test]
    fn a_sidecar_shortfall_is_retried_and_a_chain_failure_is_not() {
        // A chain that did not finish syncing is the case quarantine exists for — unchanged, and
        // it does not wait for a count.
        assert!(post_commit_failure_should_quarantine(false, 0));
        assert!(post_commit_failure_should_quarantine(false, 1));

        // A sidecar shortfall is survivable while there is another peer to ask…
        assert!(!post_commit_failure_should_quarantine(true, 1), "the FIRST refusal must not be fatal");
        for seen in 1..FlowContext::SIDECAR_SHORTFALL_TOLERANCE {
            assert!(!post_commit_failure_should_quarantine(true, seen), "shortfall {seen} is still retryable");
        }
        // …and finite, because a node that can get its sidecars from nobody really is running a
        // chain it cannot vouch for.
        assert!(post_commit_failure_should_quarantine(true, FlowContext::SIDECAR_SHORTFALL_TOLERANCE));
        assert!(post_commit_failure_should_quarantine(true, FlowContext::SIDECAR_SHORTFALL_TOLERANCE + 5));
        assert!(FlowContext::SIDECAR_SHORTFALL_TOLERANCE > 1, "a tolerance of one is the old behaviour wearing a counter");
    }

    /// **A log formatter fed by an unauthenticated peer cannot be an amplifier** (audit3 H11).
    ///
    /// `describe_fingerprint` is called on three peer-controlled `bytes` fields inside
    /// `initialize_connection`, before the peer is registered in the hub or given a flow. It used
    /// to be one heap allocation per input byte and two output bytes per input byte, with the
    /// transport accepting messages up to 1 GB and the proto declaring the fields as plain `bytes`
    /// with no cap: one connection, one upload, ~10^9 allocations and a ~2 GiB String.
    #[test]
    fn describe_fingerprint_is_bounded_by_the_field_width_not_by_the_peer() {
        let cap = kaspa_p2p_lib::convert::messages::MAX_HANDSHAKE_FINGERPRINT_BYTES;
        assert_eq!(describe_fingerprint(&[]), "absent (peer predates this field)");
        assert_eq!(describe_fingerprint(&[0xab, 0xcd]), "abcd");

        // A full-width fingerprint renders whole — the diagnostic still does its job.
        let full = vec![0x5au8; cap];
        assert_eq!(describe_fingerprint(&full).len(), cap * 2, "a real fingerprint is shown in full");

        // And an absurd one renders SHORT, and says so.
        let absurd = vec![0u8; 8 << 20];
        let rendered = describe_fingerprint(&absurd);
        assert!(rendered.len() < cap * 2 + 64, "the rendering is bounded, got {} chars", rendered.len());
        assert!(rendered.contains("oversized"), "and it names what happened: {rendered}");
    }

    #[test]
    fn rpc_priority_is_high_only_for_attestation_shards() {
        assert_eq!(rpc_transaction_priority(&tx(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD)), Priority::High);
        assert_eq!(rpc_transaction_priority(&tx(SUBNETWORK_ID_NATIVE)), Priority::Low);
    }

    #[test]
    fn low_priority_rpc_broadcasts_are_throttled() {
        assert!(!rpc_transaction_should_throttle_broadcast(Priority::High));
        assert!(rpc_transaction_should_throttle_broadcast(Priority::Low));
    }
}

pub struct FlowContextInner {
    pub node_id: PeerId,
    pub consensus_manager: Arc<ConsensusManager>,
    pub config: Arc<Config>,
    hub: Hub,
    orphans_pool: AsyncRwLock<OrphanBlocksPool>,
    shared_block_requests: Arc<Mutex<HashMap<BlockHash, RequestScopeMetadata>>>,
    transactions_spread: AsyncRwLock<TransactionsSpread>,
    shared_transaction_requests: Arc<Mutex<HashMap<TransactionId, RequestScopeMetadata>>>,
    // kaspa-pq EVM Lane §14.2: pending-EVM-tx gossip state, fully separate from
    // the UTXO tx spread (independent queue, longer batching interval).
    evm_transactions_spread: AsyncRwLock<EvmTransactionsSpread>,
    shared_evm_transaction_requests: Arc<Mutex<HashMap<EvmH256, RequestScopeMetadata>>>,
    // kaspa-pq EVM Lane §14.2 / §9.2: pending EVM deposit-claim gossip state.
    // Identity is the deposit-lock outpoint (one claim per lock); same low-priority
    // profile as the EVM-tx spread.
    evm_deposit_claims_spread: AsyncRwLock<EvmDepositClaimsSpread>,
    shared_evm_deposit_claim_requests: Arc<Mutex<HashMap<TransactionOutpoint, RequestScopeMetadata>>>,
    is_ibd_running: Arc<AtomicBool>,
    ibd_metadata: Arc<RwLock<Option<IbdMetadata>>>,
    /// Identifies the IBD attempt that currently owns the participation state, so one that finishes
    /// late cannot restore a state the node has already moved past.
    ibd_lease: Arc<RwLock<Option<IbdLease>>>,
    /// Chains peers advertised while an IBD held the latch, keyed by chain rather than by peer.
    ibd_candidates: Arc<RwLock<IbdCandidateRegistry>>,
    /// The chain this node has decided to sync next, reserved so that cancelling an IBD hands the
    /// latch to the winner rather than to whoever relays first. See [`PreferredIbdCandidate`].
    preferred_ibd_candidate: Arc<RwLock<Option<PreferredIbdCandidate>>>,
    /// Wakes the flow that should serve a reserved candidate, so the handoff does not depend on that
    /// peer happening to relay something.
    handoff_tx: broadcast::Sender<CandidateId>,
    /// Nominates a candidate for proof verification. Broadcast because only the IBD flow of a peer
    /// that actually offers the chain can fetch its proof — `PruningPointProof` is routed to that
    /// flow — and every idle IBD flow listens to see whether the nomination is theirs to serve.
    challenger_tx: broadcast::Sender<CandidateId>,
    /// Set once `staging.commit()` has swapped in a new active consensus during the running IBD.
    /// A failure after this point is not a no-op — see `finish_ibd_after_failure`.
    active_consensus_replaced: Arc<AtomicBool>,
    /// Consecutive post-commit IBDs that failed ONLY because a peer could not serve a
    /// pruning-point sidecar (audit3 H2/H9). Reset by any IBD that gets past them.
    sidecar_shortfalls: Arc<std::sync::atomic::AtomicU32>,
    /// **Outpoints this NODE has reserved and no wallet may spend** (audit3 H12).
    ///
    /// The PALW panel funds every lifecycle object it carries — a court disclosure, a verdict, a
    /// quorum — from `--palw-fee-outpoint` or the rolling outpoint it persists from one. That
    /// output sits at the producer's own pay address, which is also where its mining rewards land,
    /// so it is exactly what an operator's `wallet send` selects. Nothing told the wallet, and the
    /// consequence is the one the M2-10 startup gate exists to prevent: a node that can open claims
    /// and cannot answer for them, which on a clocked ladder "is not a lost dispute, it is a
    /// slashed bond".
    ///
    /// Node-local rather than chain state, which is why it lives here and not in consensus: the
    /// chain has no opinion about which of a producer's outputs its own panel intends to spend.
    palw_reserved_outpoints: Arc<ReservedOutpoints>,
    pub address_manager: Arc<Mutex<AddressManager>>,
    connection_manager: RwLock<Option<Arc<ConnectionManager>>>,
    mining_manager: MiningManagerProxy,
    pub(crate) tick_service: Arc<TickService>,
    notification_root: Arc<ConsensusNotificationRoot>,

    // Special sampling logger used only for high-bps networks where logs must be throttled
    block_event_logger: Option<BlockEventLogger>,

    bps: usize,

    // Orphan parameters
    orphan_resolution_range: u32,
    max_orphans: usize,

    // Mining rule engine
    mining_rule_engine: Arc<MiningRuleEngine>,

    /// ADR-0042 Decision 7 transport: the PALW material/receipt gossip's dedup, caps and inbox.
    /// Present on every node (the state is a few KB); active only where the flow feeds it, and the
    /// flow refuses everything on a network with no ConsensusV2 ruleset.
    palw_gossip: crate::palw_gossip::PalwGossipCenter,
}

#[derive(Clone)]
pub struct FlowContext {
    inner: Arc<FlowContextInner>,
}

pub struct IbdRunningGuard {
    indicator: Arc<AtomicBool>,
}

impl Drop for IbdRunningGuard {
    fn drop(&mut self) {
        let result = self.indicator.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst);
        assert!(result.is_ok())
    }
}

/// Render a peer's handshake fingerprint for an error message, distinguishing "an older build that
/// does not send this" from "a different value", since the operator response differs.
/// **The outpoints this node has reserved for its own use** (audit3 H12).
///
/// Its own type rather than a bare set behind a lock, so the behaviour a wallet depends on —
/// declaring twice is idempotent, and the answer is ordered so two readers agree — is stated and
/// testable instead of being three lines inside `FlowContext`.
#[derive(Default)]
pub struct ReservedOutpoints {
    inner: RwLock<std::collections::HashSet<kaspa_consensus_core::tx::TransactionOutpoint>>,
}

impl ReservedOutpoints {
    pub fn reserve(&self, outpoint: kaspa_consensus_core::tx::TransactionOutpoint) {
        self.inner.write().insert(outpoint);
    }

    pub fn all(&self) -> Vec<kaspa_consensus_core::tx::TransactionOutpoint> {
        let mut out: Vec<_> = self.inner.read().iter().copied().collect();
        out.sort_by(|a, b| (a.transaction_id, a.index).cmp(&(b.transaction_id, b.index)));
        out
    }
}

/// **Does a post-commit IBD failure mean this node can no longer vouch for what it is running?**
///
/// Split out as a pure rule so it can be stated and tested rather than inferred from a field, an
/// atomic and three branches (audit3 H2/H9). `seen` is how many consecutive post-commit failures
/// have been sidecar-only, counting this one.
fn post_commit_failure_should_quarantine(sidecar_shortfall: bool, seen: u32) -> bool {
    if !sidecar_shortfall {
        // The chain itself did not finish syncing. This is the case `quarantine` was written for.
        return true;
    }
    seen >= FlowContext::SIDECAR_SHORTFALL_TOLERANCE
}

/// **Bounded, because the input is an unauthenticated peer's** (audit3 H11). The conversion at
/// `TryFrom<protowire::VersionMessage>` already refuses anything wider than a fingerprint, so this
/// cap is the second of two — but it is the one that makes the function safe to call on ANY byte
/// string, which is what a log-formatting helper has to be. It used to be
/// `bytes.iter().map(|b| format!("{b:02x}")).collect()`: one heap allocation per input byte and two
/// output bytes per input byte, called on three peer-controlled fields inside
/// `initialize_connection` before the peer is registered in the hub or given a flow. With the
/// transport's 1 GB message ceiling that was ~10^9 allocations and a ~2 GiB String per connection,
/// for the cost of one upload.
fn describe_fingerprint(bytes: &[u8]) -> String {
    use std::fmt::Write;
    if bytes.is_empty() {
        return "absent (peer predates this field)".to_owned();
    }
    let shown = bytes.len().min(kaspa_p2p_lib::convert::messages::MAX_HANDSHAKE_FINGERPRINT_BYTES);
    let mut out = String::with_capacity(shown * 2 + 24);
    for b in &bytes[..shown] {
        let _ = write!(out, "{b:02x}");
    }
    if bytes.len() > shown {
        let _ = write!(out, "… ({} bytes, oversized)", bytes.len());
    }
    out
}

#[derive(Debug, Clone, Copy)]
struct IbdMetadata {
    /// The peer from which current IBD is syncing from
    peer: PeerKey,
    /// The DAA score of the relay block which triggered the current IBD
    daa_score: u64,
    /// The blue work the relay block which triggered the current IBD claimed. Recorded so that
    /// completion can say what was adopted, next to what was passed over.
    blue_work: BlueWorkType,
}

pub struct RequestScopeMetadata {
    pub timestamp: Instant,
    pub obtained: bool,
}

pub struct RequestScope<T: PartialEq + Eq + std::hash::Hash> {
    set: Arc<Mutex<HashMap<T, RequestScopeMetadata>>>,
    pub req: T,
}

impl<T: PartialEq + Eq + std::hash::Hash> RequestScope<T> {
    pub fn new(set: Arc<Mutex<HashMap<T, RequestScopeMetadata>>>, req: T) -> Self {
        Self { set, req }
    }

    /// Scope holders should use this function to report that the request has
    /// successfully been obtained from the peer and is now being processed
    pub fn report_obtained(&self) {
        if let Some(e) = self.set.lock().get_mut(&self.req) {
            e.obtained = true;
        }
    }
}

impl<T: PartialEq + Eq + std::hash::Hash> Drop for RequestScope<T> {
    fn drop(&mut self) {
        self.set.lock().remove(&self.req);
    }
}

impl Deref for FlowContext {
    type Target = FlowContextInner;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

fn rpc_transaction_priority(transaction: &Transaction) -> Priority {
    if transaction.subnetwork_id == SUBNETWORK_ID_STAKE_ATTESTATION_SHARD { Priority::High } else { Priority::Low }
}

fn rpc_transaction_should_throttle_broadcast(priority: Priority) -> bool {
    matches!(priority, Priority::Low)
}

impl FlowContext {
    pub fn new(
        consensus_manager: Arc<ConsensusManager>,
        address_manager: Arc<Mutex<AddressManager>>,
        config: Arc<Config>,
        mining_manager: MiningManagerProxy,
        tick_service: Arc<TickService>,
        notification_root: Arc<ConsensusNotificationRoot>,
        hub: Hub,
        mining_rule_engine: Arc<MiningRuleEngine>,
    ) -> Self {
        let bps = config.bps() as usize;
        let orphan_resolution_range = BASELINE_ORPHAN_RESOLUTION_RANGE + (bps as f64).log2().ceil() as u32;

        // The maximum amount of orphans allowed in the orphans pool. This number is an approximation
        // of how many orphans there can possibly be on average bounded by an upper bound.
        let max_orphans = (2u64.pow(orphan_resolution_range) as usize * config.ghostdag_k() as usize).min(MAX_ORPHANS_UPPER_BOUND);
        Self {
            inner: Arc::new(FlowContextInner {
                node_id: Uuid::new_v4().into(),
                consensus_manager,
                palw_gossip: crate::palw_gossip::PalwGossipCenter::default(),
                orphans_pool: AsyncRwLock::new(OrphanBlocksPool::new(max_orphans)),
                shared_block_requests: Arc::new(Mutex::new(HashMap::new())),
                transactions_spread: AsyncRwLock::new(TransactionsSpread::new(hub.clone())),
                shared_transaction_requests: Arc::new(Mutex::new(HashMap::new())),
                evm_transactions_spread: AsyncRwLock::new(EvmTransactionsSpread::new(hub.clone())),
                shared_evm_transaction_requests: Arc::new(Mutex::new(HashMap::new())),
                evm_deposit_claims_spread: AsyncRwLock::new(EvmDepositClaimsSpread::new(hub.clone())),
                shared_evm_deposit_claim_requests: Arc::new(Mutex::new(HashMap::new())),
                is_ibd_running: Default::default(),
                ibd_metadata: Default::default(),
                ibd_lease: Default::default(),
                ibd_candidates: Default::default(),
                challenger_tx: broadcast::channel(CHALLENGER_NOMINATION_BACKLOG).0,
                preferred_ibd_candidate: Default::default(),
                handoff_tx: broadcast::channel(CHALLENGER_NOMINATION_BACKLOG).0,
                active_consensus_replaced: Default::default(),
                sidecar_shortfalls: Default::default(),
                palw_reserved_outpoints: Default::default(),
                hub,
                address_manager,
                connection_manager: Default::default(),
                mining_manager,
                tick_service,
                notification_root,
                block_event_logger: Some(BlockEventLogger::new(bps)),
                bps,
                orphan_resolution_range,
                max_orphans,
                config,
                mining_rule_engine,
            }),
        }
    }

    pub fn block_invs_channel_size(&self) -> usize {
        self.bps * Router::incoming_flow_baseline_channel_size()
    }

    pub fn orphan_resolution_range(&self) -> u32 {
        self.orphan_resolution_range
    }

    pub fn max_orphans(&self) -> usize {
        self.max_orphans
    }

    pub fn start_async_services(&self) {
        if let Some(logger) = self.block_event_logger.as_ref() {
            logger.start();
        }
    }

    pub fn set_connection_manager(&self, connection_manager: Arc<ConnectionManager>) {
        self.connection_manager.write().replace(connection_manager);
    }

    pub fn drop_connection_manager(&self) {
        self.connection_manager.write().take();
    }

    pub fn connection_manager(&self) -> Option<Arc<ConnectionManager>> {
        self.connection_manager.read().clone()
    }

    pub fn consensus(&self) -> ConsensusInstance {
        self.consensus_manager.consensus()
    }

    pub fn palw_gossip(&self) -> &crate::palw_gossip::PalwGossipCenter {
        &self.palw_gossip
    }

    /// Whether this network runs the PALW ConsensusV2 ruleset — the gate on every PALW gossip
    /// message, in and out. On any other network the band does not exist on the wire.
    pub fn palw_v2_active(&self) -> bool {
        matches!(self.config.params.palw_consensus_mode, kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_))
    }

    /// Broadcast a claim's execution material to every peer, marking it seen so the echo is not a
    /// second inbox event. The producer calls this after publishing (and periodically while its
    /// claim is unresolved); the bytes prove themselves against the claim's committed roots.
    pub async fn broadcast_palw_material(&self, claim: kaspa_hashes::Hash64, bytes: Vec<u8>) {
        if !self.palw_v2_active() {
            return;
        }
        self.palw_gossip.mark_own_material(claim, &bytes);
        let msg = kaspa_p2p_lib::make_message!(
            kaspa_p2p_lib::pb::kaspad_message::Payload::PalwTraceMaterialBroadcast,
            kaspa_p2p_lib::pb::PalwTraceMaterialBroadcastMessage { claim_id: Some(claim.into()), material: bytes }
        );
        self.hub().broadcast(msg, None).await;
    }

    /// **Ask the network for a claim's material** — the pull half of the transport.
    ///
    /// Sent when a panel seat holds a duty and no material: the producer may be gone, but any
    /// peer that heard the broadcast once (or produced it) re-serves it — to EVERYBODY, on the
    /// push message, so one answered request refills the whole neighbourhood. Version-filtered:
    /// a pre-pull peer has no route for this type and an unroutable payload disconnects it.
    pub async fn request_palw_material(&self, claim: kaspa_hashes::Hash64) {
        if !self.palw_v2_active() {
            return;
        }
        let msg = kaspa_p2p_lib::make_message!(
            kaspa_p2p_lib::pb::kaspad_message::Payload::PalwMaterialRequest,
            // Unsigned: this is the pre-SA-2 shape, kept so a node whose panel has not registered
            // an authorizer can still ask. `request_palw_material_signed` is the form SA-2
            // prefers and the one the seat uses.
            kaspa_p2p_lib::pb::PalwMaterialRequestMessage {
                claim_id: Some(claim.into()),
                requester_pubkey: Vec::new(),
                signature: Vec::new(),
                requested_daa: 0,
            }
        );
        self.hub().broadcast_to_peers_with_min_version(msg, PROTOCOL_VERSION_PALW_PULL).await;
    }

    /// Broadcast one signed seat receipt (borsh(`PalwSeatReceiptV2`)) to every peer.
    pub async fn broadcast_palw_seat_receipt(&self, bytes: Vec<u8>) {
        if !self.palw_v2_active() {
            return;
        }
        self.palw_gossip.mark_own_receipt(&bytes);
        let msg = kaspa_p2p_lib::make_message!(
            kaspa_p2p_lib::pb::kaspad_message::Payload::PalwSeatReceiptBroadcast,
            kaspa_p2p_lib::pb::PalwSeatReceiptBroadcastMessage { receipt: bytes }
        );
        self.hub().broadcast(msg, None).await;
    }

    pub fn hub(&self) -> &Hub {
        &self.hub
    }

    pub fn mining_manager(&self) -> &MiningManagerProxy {
        &self.mining_manager
    }

    pub fn try_set_ibd_running(&self, peer: PeerKey, relay_daa_score: u64, relay_blue_work: BlueWorkType) -> Option<IbdRunningGuard> {
        // A reservation closes the latch to everyone but the reserved chain's sources. This is what
        // makes a switch a handoff rather than a fresh race: without it, abandoning an IBD would
        // simply re-run the arrival-order lottery, and the branch just rejected could win it.
        if let Some(preferred) = self.preferred_ibd_candidate.read().as_ref()
            && !preferred.preferred_sources.contains(&peer)
        {
            return None;
        }
        if self.is_ibd_running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            self.ibd_metadata.write().replace(IbdMetadata { peer, daa_score: relay_daa_score, blue_work: relay_blue_work });
            // Deliberately NOT cleared. A validated pruning proof costs the prover minutes and this
            // node a large transfer, and the reason an IBD is starting may well be that the last one
            // was abandoned for a candidate verified during it. Throwing that away would mean
            // re-fetching the same proofs and, worse, forgetting which chain won — the registry
            // expires by TTL instead.
            self.active_consensus_replaced.store(false, Ordering::SeqCst);
            // Stop participating NOW. `staging.commit()` happens partway through an IBD, so waiting
            // for the IBD to report success leaves a window in which this node is already running
            // the new chain and still mining and attesting on it.
            // The lease is what lets a late-finishing attempt be told from the current one.
            *self.ibd_lease.write() = Some(self.chain_participation().enter_ibd());
            Some(IbdRunningGuard { indicator: self.is_ibd_running.clone() })
        } else {
            None
        }
    }

    /// What the currently running IBD's syncer claimed, for comparison against passed-over offers.
    pub fn ibd_relay_blue_work(&self) -> Option<BlueWorkType> {
        self.ibd_metadata.read().map(|md| md.blue_work)
    }

    /// Note that `peer` offered something while an IBD held the latch, so it can be asked what
    /// chain it is on. Records only that the peer is worth asking — an inv hash is not a chain.
    pub fn observe_ibd_candidate_peer(&self, peer: PeerKey) {
        self.ibd_candidates.write().observe_peer(peer, Instant::now());
    }

    /// Whether `peer` may be asked for a candidate summary now, claiming the rate-limit budget.
    pub fn claim_ibd_summary_request(&self, peer: PeerKey) -> bool {
        self.ibd_candidates.write().claim_summary_request(peer, Instant::now())
    }

    /// A summary arrived from `peer`; leave it alone for a while.
    pub fn note_ibd_summary_success(&self, peer: PeerKey) {
        self.ibd_candidates.write().note_summary_success(peer, Instant::now());
    }

    /// Asking `peer` failed; retry soon, backing off.
    pub fn note_ibd_summary_failure(&self, peer: PeerKey) {
        self.ibd_candidates.write().note_summary_failure(peer, Instant::now());
    }

    /// Record what a peer says it is on. Merges into an existing candidate when the chain matches,
    /// so peers on the same branch become sources of one candidate rather than rivals.
    pub fn observe_ibd_candidate_summary(&self, peer: PeerKey, header: &Header, pruning_point: BlockHash) -> CandidateId {
        self.ibd_candidates.write().observe_summary(peer, header, pruning_point, Instant::now())
    }

    pub fn set_ibd_candidate_validation(&self, id: CandidateId, validation: CandidateValidation) {
        self.ibd_candidates.write().set_validation(id, validation);
    }

    /// A proof fetch failed in transport (not judged). Charges the candidate's retry budget so a
    /// flapping source cannot pin it at `proof_attempts == 0` and block the commit forever. See
    /// [`IbdCandidateRegistry::note_proof_transport_failure`].
    pub fn note_ibd_candidate_transport_failure(&self, id: CandidateId) {
        self.ibd_candidates.write().note_proof_transport_failure(&id);
    }

    /// Ask whoever can to verify the strongest chain nobody has checked yet.
    ///
    /// A no-op when there is nothing worth checking. Verified candidates are never re-nominated,
    /// and a candidate already being verified is not nominated again, so this is safe to call on
    /// every summary that arrives.
    /// Write off verification attempts that have held the single slot too long.
    ///
    /// Called from the relay flow's idle poll rather than only at the commit barrier. The barrier
    /// runs during an IBD; after one finishes there may be no further IBD at all, so a request whose
    /// flow died would hold the nomination slot for as long as the lease and nothing would ever
    /// clear it. That is not hypothetical — it is what a failing recovery round looked like.
    pub fn expire_stale_verifications(&self) -> Vec<CandidateId> {
        self.ibd_candidates.write().expire_proof_requests(Instant::now(), CHALLENGER_VERIFICATION_LEASE)
    }

    pub fn nominate_challenger(&self) {
        let nominee = {
            let registry = self.ibd_candidates.read();
            registry.strongest_unverified().and_then(|c| match c.validation {
                CandidateValidation::SummaryReceived { claimed_blue_work } => Some((c.id, claimed_blue_work)),
                _ => None,
            })
        };
        match nominee {
            Some((id, claimed_blue_work)) => {
                self.ibd_candidates
                    .write()
                    .set_validation(id, CandidateValidation::ProofRequested { since: Instant::now(), claimed_blue_work });
                record_stage(RecoveryStage::CandidateNominated, None, Some(id), None, self.chain_participation().state().as_str(), "");
                // Failure means no IBD flow is listening, which simply means nobody can serve it.
                let _ = self.challenger_tx.send(id);
            }
            None => record_stage(
                RecoveryStage::Rejected,
                None,
                None,
                None,
                self.chain_participation().state().as_str(),
                "nothing to nominate: no SummaryReceived candidate, or a verification is already in flight",
            ),
        }
    }

    pub fn subscribe_challenger_nominations(&self) -> broadcast::Receiver<CandidateId> {
        self.challenger_tx.subscribe()
    }

    pub fn subscribe_ibd_handoffs(&self) -> broadcast::Receiver<CandidateId> {
        self.handoff_tx.subscribe()
    }

    /// Reserve the next IBD for a chain this node verified, and wake whoever can serve it.
    ///
    /// Called when the commit barrier abandons a sync for a better candidate. Until the reservation
    /// is consumed or cleared, `try_set_ibd_running` refuses every peer that does not offer this
    /// chain — otherwise the latch would go to whichever peer relayed next, which may be the branch
    /// just rejected. The switch would then be decided by arrival order, which is the thing the
    /// switch exists to stop.
    pub fn reserve_preferred_ibd_candidate(&self, candidate_id: CandidateId, verified_blue_work: BlueWorkType) -> bool {
        let (header, preferred_sources) = {
            let registry = self.ibd_candidates.read();
            match registry.get(&candidate_id) {
                Some(c) => (c.header.clone(), c.sources.clone()),
                None => {
                    record_stage(
                        RecoveryStage::Rejected,
                        None,
                        Some(candidate_id),
                        None,
                        self.chain_participation().state().as_str(),
                        "cannot reserve: candidate is no longer in the registry",
                    );
                    return false;
                }
            }
        };
        if preferred_sources.is_empty() {
            record_stage(
                RecoveryStage::Rejected,
                None,
                Some(candidate_id),
                None,
                self.chain_participation().state().as_str(),
                "cannot reserve: no connected peer still offers this chain",
            );
            return false;
        }
        let switch_generation = self.ibd_candidates.read().switches();
        let now = Instant::now();
        self.preferred_ibd_candidate.write().replace(PreferredIbdCandidate {
            candidate_id,
            preferred_sources,
            header,
            verified_blue_work,
            switch_generation,
            reserved_at: now,
            unclaimed_since: now,
        });
        record_stage(
            RecoveryStage::PreferredCandidateReserved,
            None,
            Some(candidate_id),
            None,
            self.chain_participation().state().as_str(),
            format!(
                "verified_blue_work={verified_blue_work} sources={} generation={switch_generation}",
                self.ibd_candidates.read().sources_of(&candidate_id).len()
            ),
        );
        // Failure means no IBD flow is listening; the reservation still stands and will be honoured
        // when one of the sources next relays.
        let _ = self.handoff_tx.send(candidate_id);
        true
    }

    pub fn preferred_ibd_candidate(&self) -> Option<PreferredIbdCandidate> {
        self.preferred_ibd_candidate.read().clone()
    }

    /// Release the reservation. Called once the reserved chain has had its turn at the latch,
    /// whether it succeeded or failed — a reservation that outlived its attempt would lock the node
    /// out of syncing from anyone.
    pub fn clear_preferred_ibd_candidate(&self) {
        self.preferred_ibd_candidate.write().take();
    }

    /// Note that the reserved chain has just taken the latch, restarting its unclaimed clock.
    ///
    /// A reservation survives a failed attempt on purpose, so the no-progress clock has to measure
    /// time spent waiting rather than time spent trying. The absolute lifetime is untouched — that
    /// one is what stops an endlessly-retrying reservation from waiting forever in instalments.
    pub fn note_preferred_candidate_claimed(&self) {
        if let Some(preferred) = self.preferred_ibd_candidate.write().as_mut() {
            preferred.unclaimed_since = Instant::now();
        }
    }

    /// Release a reservation that has stopped making progress, so the node can sync from anyone.
    ///
    /// Call only when no IBD is running: a sync in flight is progress, and cutting a reservation out
    /// from under its own attempt would re-open the race this node already decided.
    ///
    /// Releasing a reservation does NOT resume participation. It re-opens the latch, nothing else —
    /// the gate stays shut until a chain is actually committed.
    pub fn expire_stale_preferred_candidate(&self) -> bool {
        let Some((candidate_id, reason)) = ({
            let guard = self.preferred_ibd_candidate.read();
            guard.as_ref().and_then(|p| p.expiry_reason(Instant::now()).map(|r| (p.candidate_id, r)))
        }) else {
            return false;
        };
        self.preferred_ibd_candidate.write().take();
        warn!("releasing the IBD reservation for {}: {}", candidate_id.virtual_selected_parent, reason);
        record_stage(RecoveryStage::Rejected, None, Some(candidate_id), None, self.chain_participation().state().as_str(), reason);
        true
    }

    /// The candidate registry, for the commit barrier and for reporting.
    pub fn ibd_candidates(&self) -> &Arc<RwLock<IbdCandidateRegistry>> {
        &self.ibd_candidates
    }

    /// Forget a disconnected peer as a source. Candidates it shared with other peers survive: a
    /// dropped connection is a source failover, not a reason to reconsider which chain to follow.
    pub fn forget_ibd_candidate_peer(&self, peer: &PeerKey) {
        self.ibd_candidates.write().forget_peer(peer);
    }

    pub fn is_ibd_running(&self) -> bool {
        self.is_ibd_running.load(Ordering::SeqCst)
    }

    /// If IBD is running, returns the IBD peer we are syncing from
    pub fn ibd_peer_key(&self) -> Option<PeerKey> {
        if self.is_ibd_running() { self.ibd_metadata.read().map(|md| md.peer) } else { None }
    }

    /// If IBD is running, returns the DAA score of the relay block which triggered it
    pub fn ibd_relay_daa_score(&self) -> Option<u64> {
        if self.is_ibd_running() { self.ibd_metadata.read().map(|md| md.daa_score) } else { None }
    }

    // PR-9.5e: generic over the hash width because
    // `shared_block_requests` holds `BlockHash` (now `Hash64`)
    // while `shared_transaction_requests` holds `TransactionId`
    // (also `Hash64`). Both implement `std::hash::Hash + Eq +
    // Copy`, so the same HashMap-entry logic works for both.
    fn try_adding_request_impl<H>(req: H, map: &Arc<Mutex<HashMap<H, RequestScopeMetadata>>>) -> Option<RequestScope<H>>
    where
        H: std::hash::Hash + Eq + Copy,
    {
        match map.lock().entry(req) {
            Entry::Occupied(mut e) => {
                if e.get().obtained {
                    None
                } else {
                    let now = Instant::now();
                    if now > e.get().timestamp + REQUEST_SCOPE_WAIT_TIME {
                        e.get_mut().timestamp = now;
                        Some(RequestScope::new(map.clone(), req))
                    } else {
                        None
                    }
                }
            }
            Entry::Vacant(e) => {
                e.insert(RequestScopeMetadata { timestamp: Instant::now(), obtained: false });
                Some(RequestScope::new(map.clone(), req))
            }
        }
    }

    pub fn try_adding_block_request(&self, req: BlockHash) -> Option<RequestScope<BlockHash>> {
        Self::try_adding_request_impl(req, &self.shared_block_requests)
    }

    pub fn try_adding_transaction_request(&self, req: TransactionId) -> Option<RequestScope<TransactionId>> {
        Self::try_adding_request_impl(req, &self.shared_transaction_requests)
    }

    /// §14.2: cross-peer dedup for pending-EVM-tx requests (same scope semantics
    /// as UTXO tx requests; `EvmH256` is `Hash + Eq + Copy` like the other keys).
    pub fn try_adding_evm_transaction_request(&self, req: EvmH256) -> Option<RequestScope<EvmH256>> {
        Self::try_adding_request_impl(req, &self.shared_evm_transaction_requests)
    }

    /// §14.2: cross-peer dedup for pending EVM deposit-claim requests. The claim's
    /// identity is its deposit-lock `TransactionOutpoint` (`Hash + Eq + Copy`).
    pub fn try_adding_evm_deposit_claim_request(&self, req: TransactionOutpoint) -> Option<RequestScope<TransactionOutpoint>> {
        Self::try_adding_request_impl(req, &self.shared_evm_deposit_claim_requests)
    }

    pub async fn add_orphan(&self, consensus: &ConsensusProxy, orphan_block: Block) -> Option<OrphanOutput> {
        self.orphans_pool.write().await.add_orphan(consensus, orphan_block).await
    }

    pub async fn is_known_orphan(&self, hash: BlockHash) -> bool {
        self.orphans_pool.read().await.is_known_orphan(hash)
    }

    pub async fn get_orphan_roots_if_known(&self, consensus: &ConsensusProxy, orphan: BlockHash) -> OrphanOutput {
        self.orphans_pool.read().await.get_orphan_roots_if_known(consensus, orphan).await
    }

    pub async fn unorphan_blocks(&self, consensus: &ConsensusProxy, root: BlockHash) -> Vec<(Block, BlockValidationFuture)> {
        let (blocks, block_tasks, virtual_state_tasks) = self.orphans_pool.write().await.unorphan_blocks(consensus, root).await;
        let mut unorphaned_blocks = Vec::with_capacity(blocks.len());
        let results = join_all(block_tasks).await;
        for ((block, result), virtual_state_task) in blocks.into_iter().zip(results).zip(virtual_state_tasks) {
            match result {
                Ok(_) => {
                    unorphaned_blocks.push((block, virtual_state_task));
                }
                Err(e) => warn!("Validation failed for orphan block {}: {}", block.hash(), e),
            }
        }

        // Log or send to event logger
        if !unorphaned_blocks.is_empty() {
            if let Some(logger) = self.block_event_logger.as_ref() {
                logger.log(BlockLogEvent::Unorphaned(unorphaned_blocks[0].0.hash(), unorphaned_blocks.len()));
            } else {
                match unorphaned_blocks.len() {
                    1 => info!("Unorphaned block {}", unorphaned_blocks[0].0.hash()),
                    n => info!("Unorphaned {} blocks: {}", n, unorphaned_blocks.iter().map(|b| b.0.hash()).reusable_format(", ")),
                }
            }
        }
        unorphaned_blocks
    }

    pub async fn revalidate_orphans(&self, consensus: &ConsensusProxy) -> (Vec<BlockHash>, Vec<BlockValidationFuture>) {
        self.orphans_pool.write().await.revalidate_orphans(consensus).await
    }

    /// Adds the rpc-submitted block to the DAG and propagates it to peers.
    pub async fn submit_rpc_block(&self, consensus: &ConsensusProxy, block: Block) -> Result<(), ProtocolError> {
        if block.transactions.is_empty() {
            return Err(RuleError::NoTransactions)?;
        }
        let hash = block.hash();
        let BlockValidationFutures { block_task, virtual_state_task } = consensus.validate_and_insert_block(block.clone());
        if let Err(err) = block_task.await {
            warn!("Validation failed for block {}: {}", hash, err);
            return Err(err)?;
        }
        // Broadcast as soon as the block has been validated and inserted into the DAG
        self.hub.broadcast(make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(hash.into()) }), None).await;

        self.on_new_block(consensus, Default::default(), block, virtual_state_task).await;
        self.log_block_event(BlockLogEvent::Submit(hash));

        Ok(())
    }

    pub fn log_block_event(&self, event: BlockLogEvent) {
        if let Some(logger) = self.block_event_logger.as_ref() {
            logger.log(event)
        } else {
            match event {
                BlockLogEvent::Relay(hash) => info!("Accepted block {} via relay", hash),
                BlockLogEvent::Submit(hash) => info!("Accepted block {} via submit block", hash),
                BlockLogEvent::Orphaned(orphan, roots_count) => {
                    info!("Received a block with {} missing ancestors, adding to orphan pool: {}", roots_count, orphan)
                }
                _ => {}
            }
        }
    }

    /// Updates the mempool after a new block arrival, relays newly unorphaned transactions
    /// and possibly rebroadcast manually added transactions when not in IBD.
    ///
    /// _GO-KASPAD: OnNewBlock + broadcastTransactionsAfterBlockAdded_
    pub async fn on_new_block(
        &self,
        consensus: &ConsensusProxy,
        ancestor_batch: BlockProcessingBatch,
        block: Block,
        virtual_state_task: BlockValidationFuture,
    ) {
        let hash = block.hash();
        let mut blocks = self.unorphan_blocks(consensus, hash).await;

        // Broadcast unorphaned blocks
        let msgs = blocks
            .iter()
            .map(|(b, _)| make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(b.hash().into()) }))
            .collect();
        self.hub.broadcast_many(msgs, None).await;

        // Process blocks in topological order
        blocks.sort_by(|a, b| a.0.header.blue_work.partial_cmp(&b.0.header.blue_work).unwrap());
        // Use a ProcessQueue so we get rid of duplicates
        let mut transactions_to_broadcast = ProcessQueue::new();
        for (block, virtual_state_task) in ancestor_batch.zip().chain(once((block, virtual_state_task))).chain(blocks.into_iter()) {
            // We only care about waiting for virtual to process the block at this point, before proceeding with post-processing
            // actions such as updating the mempool. We know this will not err since `block_task` already completed w/o error
            let _ = virtual_state_task.await;
            if let Ok(txs) = self
                .mining_manager()
                .clone()
                .handle_new_block_transactions(consensus, block.header.daa_score, block.transactions.clone())
                .await
            {
                transactions_to_broadcast.enqueue_chunk(txs.into_iter().map(|x| x.id()));
            }
        }

        // Transaction relay is disabled if the node is out of sync
        if !self.is_nearly_synced(consensus).await {
            return;
        }

        // TODO: Throttle these transactions as well if needed
        self.broadcast_transactions(transactions_to_broadcast, false).await;

        // §14.2: pump the EVM-tx relay spread on the same per-block cadence as
        // the UTXO spread. The EVM spread is otherwise submit-driven, so a
        // low-rate submitter's burst tail would linger unsent until its next
        // submit; this flushes anything whose batch interval has elapsed.
        self.evm_transactions_spread.write().await.flush_due().await;
        // §14.2: pump the deposit-claim relay spread on the same cadence.
        self.evm_deposit_claims_spread.write().await.flush_due().await;

        if self.should_run_mempool_scanning_task().await {
            // Spawn a task executing the removal of expired low priority transactions and, if time has come too,
            // the revalidation of high priority transactions.
            //
            // The TransactionSpread member ensures at most one instance of this task is running at any
            // given time.
            let mining_manager = self.mining_manager().clone();
            let consensus_clone = consensus.clone();
            let context = self.clone();
            debug!("<> Starting mempool scanning task #{}...", self.mempool_scanning_job_count().await);
            tokio::spawn(async move {
                mining_manager.clone().expire_low_priority_transactions(&consensus_clone).await;
                if context.should_rebroadcast().await {
                    let (tx, mut rx) = unbounded_channel();
                    tokio::spawn(async move {
                        mining_manager.revalidate_high_priority_transactions(&consensus_clone, tx).await;
                    });
                    while let Some(transactions) = rx.recv().await {
                        let _ = context
                            .broadcast_transactions(
                                transactions,
                                true, // We throttle high priority even when the network is not flooded since they will be rebroadcast if not accepted within reasonable time.
                            )
                            .await;
                    }
                }
                context.mempool_scanning_is_done().await;
                debug!("<> Mempool scanning task is done");
            });
        }
    }

    pub async fn is_nearly_synced(&self, session: &ConsensusSessionOwned) -> bool {
        let sink_daa_score_and_timestamp = session.async_get_sink_daa_score_timestamp().await;
        self.mining_rule_engine.is_nearly_synced(sink_daa_score_and_timestamp)
    }

    pub async fn should_mine(&self, session: &ConsensusSessionOwned) -> bool {
        let sink_daa_score_and_timestamp = session.async_get_sink_daa_score_timestamp().await;
        self.mining_rule_engine.should_mine(sink_daa_score_and_timestamp)
    }

    /// The gate every participation path consults — mining, both validators, compute.
    pub fn chain_participation(&self) -> &Arc<ChainParticipationGate> {
        self.mining_rule_engine.chain_participation()
    }

    /// Whether this node may act on the chain it is holding: mine, attest, sign, call itself synced.
    ///
    /// Deliberately not `should_mine`, which folds in the sync-rate rule and peer connectivity —
    /// conditions about mining throughput, not about whether the chain under us has been settled.
    /// A validator that reused `should_mine` would inherit the sync-rate override, which can hold
    /// `true` on a chain nobody has compared.
    pub fn is_consensus_participation_allowed(&self) -> bool {
        self.chain_participation().allows_participation()
    }

    /// Record that an IBD has replaced the active consensus (`staging.commit()` returned).
    ///
    /// From here on the node is running on the new chain whether or not the rest of the IBD
    /// succeeds, so a later failure cannot simply be reported and forgotten — see
    /// [`FlowContext::finish_ibd_after_failure`].
    pub fn mark_active_consensus_replaced(&self) {
        self.active_consensus_replaced.store(true, Ordering::SeqCst);
        // …and durably, because the in-memory flag dies with the process and a restart is exactly
        // when the question gets asked. Without it every interrupted IBD restored as quarantined,
        // including the common case where nothing had been adopted (ADR-0068 launch audit, F19).
        self.chain_participation().mark_chain_replaced();
    }

    /// Settle the gate after an IBD that failed.
    ///
    /// If the active consensus was already replaced, the node is now running a chain whose sync
    /// never finished: quarantine, because nothing here can tell whether that chain is usable and
    /// guessing means signing on it. If nothing was replaced, the failure changed nothing and the
    /// node goes back to whatever it was doing.
    pub fn finish_ibd_after_failure(&self) -> bool {
        self.finish_ibd_after_failure_kind(false)
    }

    /// How many consecutive post-commit IBDs may fail on a sidecar alone before the node gives up
    /// and fails closed. Above one, because a peer answering `found: false` is ordinary: it holds a
    /// single snapshot keyed to its own pruning point, so any peer whose pruning point advanced
    /// during a multi-minute header sync answers that to a perfectly honest request. Finite,
    /// because a node that cannot complete its sidecars from anybody really is running a chain it
    /// cannot vouch for (audit3 H2/H9).
    pub const SIDECAR_SHORTFALL_TOLERANCE: u32 = 3;

    /// Settle the gate after an IBD that failed, saying whether the failure was ONLY a peer being
    /// unable to serve a pruning-point sidecar.
    ///
    /// **A sidecar shortfall is not "cannot fix by itself"** (audit3 H2/H9), which is the bar
    /// `quarantine`'s own doc sets. The chain is committed and correct; one auxiliary snapshot did
    /// not arrive from THIS peer, and the fix is to ask a different one — which the node never did,
    /// because the first refusal latched a state whose only exit is an operator restarting with
    /// `--clear-quarantine`. A node that hits it stops mining, stops attesting and reports
    /// `is_synced=false` across restarts, and enough of them at once stall DNS finality. So the
    /// shortfall is counted rather than latched, and only a node that cannot get its sidecars from
    /// anybody, repeatedly, is treated as unable to vouch for what it is running.
    pub fn finish_ibd_after_failure_kind(&self, sidecar_shortfall: bool) -> bool {
        let replaced = self.active_consensus_replaced.swap(false, Ordering::SeqCst);
        if replaced {
            let seen = if sidecar_shortfall { self.sidecar_shortfalls.fetch_add(1, Ordering::SeqCst) + 1 } else { 0 };
            if !post_commit_failure_should_quarantine(sidecar_shortfall, seen) {
                // Not Ready — the node still holds a chain it cannot advance past the pruning point
                // without the sidecar, and `is_synced` stays false. It is simply free to try
                // another peer, which is the whole difference.
                return false;
            }
            self.chain_participation().quarantine();
        } else if let Some(lease) = self.ibd_lease.write().take() {
            self.chain_participation().release_after_noop_ibd(lease);
        }
        replaced
    }

    /// Declare an outpoint this node's own PALW panel intends to spend, so a wallet asking the
    /// node which of its outputs are untouchable is told about it (audit3 H12). Idempotent; both
    /// the configured `--palw-fee-outpoint` and every rolling successor are declared.
    pub fn palw_reserve_outpoint(&self, outpoint: kaspa_consensus_core::tx::TransactionOutpoint) {
        self.palw_reserved_outpoints.reserve(outpoint);
    }

    /// Everything this node has reserved, for the wallet-facing RPC.
    pub fn palw_reserved_outpoints(&self) -> Vec<kaspa_consensus_core::tx::TransactionOutpoint> {
        self.palw_reserved_outpoints.all()
    }

    /// An IBD got past the sidecars, so the run of shortfalls is over.
    pub fn clear_sidecar_shortfalls(&self) {
        self.sidecar_shortfalls.store(0, Ordering::SeqCst);
    }

    /// Settle the gate after an IBD that succeeded, holding the node for at least `min_review`.
    ///
    /// Deliberately does NOT quarantine on an unverified competing claim. A passed-over offer is
    /// just an inv hash from some peer: it may be a side block, a merge block, or simply a newer
    /// block on the very chain we just synced. None of those are competing branches, and all of
    /// them out-weigh an older tip, so quarantining on "someone claimed more work" would fire on
    /// routine relay traffic and take honest nodes offline. Quarantine is reserved for the case
    /// this node can state without guessing — see [`FlowContext::finish_ibd_after_failure`].
    ///
    /// **Only an IBD that ADOPTED something re-enters the review.** This entered it
    /// unconditionally, on the reading that any finished IBD produces "a chain nothing has yet
    /// been compared against". Most IBDs produce nothing of the sort: they are forward syncs on
    /// the chain this node already committed to, and on a fast network they are routine. Each one
    /// re-armed the floor through `fetch_max`, so a node that IBDs more often than the floor is
    /// long never leaves review — it cannot mine, cannot attest, and reports `is_synced=false`,
    /// which is what a DNS seeder gates on.
    ///
    /// Measured on testnet-11: a node AT THE TIP (557 of 558 blocks, load 0.4) ran 22 IBDs in 16
    /// minutes and was held the entire time, its floor resetting to ~168s before each expiry.
    ///
    /// `active_consensus_replaced` is exactly the signal, and this function was clearing it
    /// without reading it. It is set in one place — where a staging consensus is committed, which
    /// is the node replacing its chain with a different one. A forward sync sets nothing, so it is
    /// as much a no-op for the gate as a FAILED IBD that replaced nothing, and it takes the same
    /// path back: restore whatever the node was before, review floor included if it was in one.
    ///
    /// The `ever_ready` half is not optional. A node's FIRST sync replaces nothing (there is no
    /// incumbent to stage against) and is precisely the case the review exists for — a chain the
    /// node raced onto and has compared against nothing. Without this it would go straight to
    /// Ready on it.
    pub fn finish_ibd_after_success(&self, min_review: Duration) {
        let replaced = self.active_consensus_replaced.swap(false, Ordering::SeqCst);
        if replaced || !self.chain_participation().ever_ready() {
            self.chain_participation().enter_candidate_review(min_review.as_millis() as u64);
        } else if let Some(lease) = self.ibd_lease.write().take() {
            self.chain_participation().release_after_noop_ibd(lease);
        }
    }

    /// Notifies that the UTXO set was reset due to pruning point change via IBD.
    pub fn on_pruning_point_utxoset_override(&self) {
        // Notifications from the flow context might be ignored if the inner channel is already closing
        // due to global shutdown, hence we ignore the possible error
        let _ = self.notification_root.notify(Notification::PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideNotification {}));
    }

    /// Notifies that a transaction has been added to the mempool.
    pub async fn on_transaction_added_to_mempool(&self) {
        // TODO: call a handler function or a predefined registered service
    }

    /// Adds the rpc-submitted transaction to the mempool and propagates it to peers.
    ///
    /// kaspa-pq DoS hardening: only stake-attestation shards get high-priority/no-expiry
    /// treatment. Ordinary RPC txs (wallet sends, consolidation, bond/deposit funding, etc.)
    /// must remain low priority and broadcast-throttled, otherwise an operator-side loop can
    /// flood the high-priority lane and starve attestation liveness.
    pub async fn submit_rpc_transaction(
        &self,
        consensus: &ConsensusProxy,
        transaction: Transaction,
        orphan: Orphan,
    ) -> Result<(), ProtocolError> {
        let priority = rpc_transaction_priority(&transaction);
        let transaction_insertion = self
            .mining_manager()
            .clone()
            .validate_and_insert_transaction(consensus, transaction, priority, orphan, RbfPolicy::Forbidden)
            .await?;
        self.broadcast_transactions(
            transaction_insertion.accepted.iter().map(|x| x.id()),
            rpc_transaction_should_throttle_broadcast(priority),
        )
        .await;
        Ok(())
    }

    /// Replaces the rpc-submitted transaction into the mempool and propagates it to peers.
    ///
    /// Returns the removed mempool transaction on successful replace by fee.
    ///
    /// kaspa-pq DoS hardening: same priority split as [`Self::submit_rpc_transaction`].
    pub async fn submit_rpc_transaction_replacement(
        &self,
        consensus: &ConsensusProxy,
        transaction: Transaction,
    ) -> Result<Arc<Transaction>, ProtocolError> {
        let priority = rpc_transaction_priority(&transaction);
        let transaction_insertion = self
            .mining_manager()
            .clone()
            .validate_and_insert_transaction(consensus, transaction, priority, Orphan::Forbidden, RbfPolicy::Mandatory)
            .await?;
        self.broadcast_transactions(
            transaction_insertion.accepted.iter().map(|x| x.id()),
            rpc_transaction_should_throttle_broadcast(priority),
        )
        .await;
        // The combination of args above of Orphan::Forbidden and RbfPolicy::Mandatory should always result
        // in a removed transaction returned, however we prefer failing gracefully in case of future internal mempool changes
        transaction_insertion.removed.ok_or(ProtocolError::Other(
            "Replacement transaction was actually accepted but the *replaced* transaction was not returned from the mempool",
        ))
    }

    /// Returns true if the time has come for running the task cleaning mempool transactions.
    async fn should_run_mempool_scanning_task(&self) -> bool {
        self.transactions_spread.write().await.should_run_mempool_scanning_task()
    }

    /// Returns true if the time has come for a rebroadcast of the mempool high priority transactions.
    async fn should_rebroadcast(&self) -> bool {
        self.transactions_spread.read().await.should_rebroadcast()
    }

    async fn mempool_scanning_job_count(&self) -> u64 {
        self.transactions_spread.read().await.mempool_scanning_job_count()
    }

    async fn mempool_scanning_is_done(&self) {
        self.transactions_spread.write().await.mempool_scanning_is_done()
    }

    /// Add the given transactions IDs to a set of IDs to broadcast. The IDs will be broadcasted to all peers
    /// within transaction Inv messages.
    ///
    /// The broadcast itself may happen only during a subsequent call to this function since it is done at most
    /// after a predefined interval or when the queue length is larger than the Inv message capacity.
    pub async fn broadcast_transactions<I: IntoIterator<Item = TransactionId>>(&self, transaction_ids: I, should_throttle: bool) {
        self.transactions_spread.write().await.broadcast_transactions(transaction_ids, should_throttle).await
    }

    /// §14.2: queue pending-EVM-tx hashes for inv broadcast to EVM-relay-capable
    /// (protocol ≥ 101) peers. Lower priority than UTXO tx gossip by design:
    /// the spread batches on a longer interval and its invs are shed (not
    /// disconnected) on receiver overflow.
    pub async fn broadcast_evm_transactions<I: IntoIterator<Item = EvmH256>>(&self, tx_hashes: I) {
        self.evm_transactions_spread.write().await.broadcast_evm_transactions(tx_hashes).await
    }

    /// Adds the rpc-submitted EVM transaction to the EVM mempool (class-1
    /// admission inside) and, on success, queues it for P2P relay (§14.2).
    ///
    /// Audit H-1: the RPC ingress (both `eth_sendRawTransaction` and the gRPC
    /// `SubmitEvmTransaction`, which funnel through here) routes to the STATEFUL
    /// admission path: it reads the sender's canonical `(nonce, balance)` from the
    /// sink's committed EVM snapshot and rejects clearly-unselectable txs (unfunded /
    /// below-state-nonce / far-future-nonce) BEFORE they occupy a pool slot — closing
    /// the gap where the stateless path let them squat the mempool. It FAILS CLOSED
    /// (returns the retryable [`EvmMempoolError::StateUnavailable`]) when no canonical
    /// view is available (no committed snapshot at the sink — early / pre-activation),
    /// never falling back to the stateless submit (that fallback IS the gap).
    ///
    /// The P2P relay path intentionally KEEPS the stateless submit (no cheap canonical
    /// view there, by design — see `v8::txrelay_evm`). Below the EVM feature gate (the
    /// native, non-evm build) this is byte-identical to the previous stateless ingress.
    #[cfg(feature = "evm")]
    pub async fn submit_rpc_evm_transaction(&self, raw: Vec<u8>) -> Result<EvmH256, EvmMempoolError> {
        use kaspa_consensus_core::evm::FlatHeadAccount;
        // Recover the class-1-admitted sender locally (same rule the stateful submit
        // below re-applies, so the two never disagree on admissibility).
        let sender = self.mining_manager().evm_recover_sender(&raw)?;
        // Read the sender's canonical (nonce, balance) at the EVM head via the consensus
        // session. PREFER the O(1) flat-head point-lookup (audit H-03 — avoids a
        // full-snapshot scan per submit); fall back to the authoritative single-sender
        // snapshot read when the flat store is not at the head. `Ok(None)` ⇒ no committed
        // snapshot at the sink ⇒ fail closed (StateUnavailable); the absent-ACCOUNT case
        // is `Ok(Some((0, 0)))`, which correctly rejects an unfunded sender downstream.
        let session = self.consensus().session().await;
        let st: Option<(u64, u128)> = session
            .spawn_blocking(move |c| -> Option<(u64, u128)> {
                // Flat head fast path: AtHead(Some) ⇒ the account; AtHead(None) ⇒ absent
                // account at a materialized head ⇒ (0, 0). Stale ⇒ fall through.
                match c.get_evm_flat_account_at_head(sender) {
                    Ok(FlatHeadAccount::AtHead(Some(acct))) => {
                        return Some((acct.nonce, acct.balance.try_to_u128().unwrap_or(u128::MAX)));
                    }
                    Ok(FlatHeadAccount::AtHead(None)) => return Some((0u64, 0u128)),
                    _ => {}
                }
                // Authoritative single-sender read (same source the mining template path
                // uses). `Err` ⇒ no committed snapshot at the sink ⇒ no canonical view.
                match c.get_evm_account_states(&[sender]) {
                    Ok(map) => Some(map.get(&sender).copied().unwrap_or((0u64, 0u128))),
                    Err(_) => None,
                }
            })
            .await;
        let Some(st) = st else {
            return Err(EvmMempoolError::StateUnavailable(
                "no committed EVM state snapshot at the sink (early / pre-activation) — retry".to_string(),
            ));
        };
        let hash = self.mining_manager().clone().submit_evm_transaction_with_state(raw, Some(st))?;
        self.broadcast_evm_transactions(once(hash)).await;
        Ok(hash)
    }

    /// Native (non-evm) build: the lane is inert; admission refuses with
    /// `Inadmissible` (this build cannot decode/recover EVM txs). Byte-identical to
    /// the pre-H-1 ingress — no canonical-state read, no new dependency.
    #[cfg(not(feature = "evm"))]
    pub async fn submit_rpc_evm_transaction(&self, raw: Vec<u8>) -> Result<EvmH256, EvmMempoolError> {
        let hash = self.mining_manager().clone().submit_evm_transaction(raw)?;
        self.broadcast_evm_transactions(once(hash)).await;
        Ok(hash)
    }

    /// §14.2 / §9.2: queue deposit-lock outpoints for claim-inv broadcast to
    /// EVM-relay-capable (protocol ≥ 101) peers. Same low-priority profile as the
    /// EVM-tx spread.
    pub async fn broadcast_evm_deposit_claims<I: IntoIterator<Item = TransactionOutpoint>>(&self, outpoints: I) {
        self.evm_deposit_claims_spread.write().await.broadcast_evm_deposit_claims(outpoints).await
    }

    /// Queues an rpc-submitted (pre-validated) deposit claim into the local claim
    /// queue and, on success, gossips its lock outpoint for P2P relay (§14.2) so
    /// it reaches the dominant selected-chain producer regardless of which node
    /// the depositor submitted to. Returns `false` only when the queue is full.
    pub async fn submit_rpc_evm_deposit_claim(&self, claim: DepositClaim) -> bool {
        let outpoint = claim.deposit_outpoint;
        let queued = self.mining_manager().clone().submit_evm_deposit_claim(claim);
        if queued {
            self.broadcast_evm_deposit_claims(once(outpoint)).await;
        }
        queued
    }
}

#[async_trait]
impl ConnectionInitializer for FlowContext {
    async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ProtocolError> {
        // Build the handshake object and subscribe to handshake messages
        let mut handshake = KaspadHandshake::new(&router);

        // We start the router receive loop only after we registered to handshake routes
        router.start();

        let network_name = self.config.network_name();

        let local_address = self.address_manager.lock().best_local_address();

        // **ADR-0072 SA-2 — the fork id, and the DAA score it is taken at.**
        //
        // `unguarded_session` rather than `session().await`: this runs before the peer is in the
        // hub or holds a flow, and blocking a handshake on the consensus session lock would let one
        // connection attempt wait on an IBD commit. The score is a snapshot and does not need to be
        // more than that — a node one block either side of a fence advertises a fired set the gate
        // already accepts as "the same schedule seen from another height".
        let local_daa_score = self.consensus().unguarded_session().get_virtual_daa_score();
        let local_fork_id = kaspa_consensus_core::fork_id_v1::fork_id_v1(&self.config.params, local_daa_score);

        // Build the local version message
        // Subnets are not currently supported
        let mut self_version_message = Version::new(
            local_address,
            self.node_id,
            network_name.clone(),
            None,
            PROTOCOL_VERSION,
            self.config.genesis.hash.as_bytes().to_vec(),
            self.config.params.consensus_params_id().as_bytes().to_vec(),
            self.config.params.consensus_identity_id().as_bytes().to_vec(),
            self.config.params.consensus_schedule_id().as_bytes().to_vec(),
            local_fork_id.fired.as_bytes().to_vec(),
            local_fork_id.next,
        );
        self_version_message.add_user_agent(name(), version(), &self.config.user_agent_comments);
        // TODO: get number of live services
        // TODO: disable_relay_tx from config/cmd

        // Perform the handshake
        let peer_version_message = handshake.handshake(self_version_message.into()).await?;
        // Get time_offset as accurate as possible by computing right after the handshake
        let time_offset = unix_now() as i64 - peer_version_message.timestamp;

        let peer_version: Version = peer_version_message.try_into()?;
        router.set_identity(peer_version.id);
        // Avoid duplicate connections
        if self.hub.has_peer(router.key()) {
            return Err(ProtocolError::PeerAlreadyExists(router.key()));
        }
        // And loopback connections...
        if self.node_id == router.identity() {
            return Err(ProtocolError::LoopbackConnection(router.key()));
        }

        if peer_version.network != network_name {
            return Err(ProtocolError::WrongNetwork(network_name, peer_version.network));
        }

        // Answering to the same network name does not mean running the same rules.
        //
        // testnet-22 forked because an older build computed different overlay commitments while
        // presenting a handshake indistinguishable from a correct node's. The two peered, synced
        // from each other, and disagreed about block validity — which no amount of candidate
        // selection downstream can repair, because by then both sides believe they are right.
        // Separate them here, before either can become an IBD source for the other.
        //
        // Peers predating these fields send them empty. Treated as a mismatch rather than waved
        // through: an unknown rule set is exactly the case this check exists for, and the protocol
        // version bump means anything that omits them is an older build.
        let local_genesis = self.config.genesis.hash.as_bytes().to_vec();
        if peer_version.genesis_hash != local_genesis {
            return Err(ProtocolError::WrongGenesis(
                network_name,
                self.config.genesis.hash.to_string(),
                describe_fingerprint(&peer_version.genesis_hash),
            ));
        }

        // **Refuse on the rules; report on the schedule** (audit M1-6).
        //
        // The exact comparison below is what a peer predating the split expects, and it is still
        // what runs against one. Between two builds that carry the identity id, the gate moves to
        // the half whose difference actually invalidates history: a build that merely SCHEDULES a
        // fence at a future height agrees with an un-upgraded peer about every block either can
        // produce today, and refusing it there partitions the network for the whole rollout —
        // which is what made "ship consensus changes as an activation, never a re-genesis"
        // impossible to obey. They diverge at H, where fork choice is the right instrument.
        //
        // **"Scheduled" is not "already in force"** (re-audit R-1). `consensus_identity_id` keeps a
        // fence that is active at GENESIS distinguishable, because two builds that disagree about
        // one of those disagree about block 1 — they are not on the same chain, and letting them
        // peer turns a handshake refusal into a silent fork. The residual, which this warning must
        // not overstate: two builds that both arm a fence in the past at DIFFERENT non-zero heights
        // still land here, and they can disagree about history between those heights.
        let local_params_id = self.config.params.consensus_params_id();
        if peer_version.consensus_params_id != local_params_id.as_bytes().to_vec() {
            let local_identity = self.config.params.consensus_identity_id();
            let identity_agrees =
                !peer_version.consensus_identity_id.is_empty() && peer_version.consensus_identity_id == local_identity.as_bytes();
            if !identity_agrees {
                return Err(ProtocolError::WrongConsensusParams(
                    network_name,
                    local_params_id.to_string(),
                    describe_fingerprint(&peer_version.consensus_params_id),
                ));
            }
            warn!(
                "peer {} agrees on every rule in force now and schedules a FUTURE fence differently (local {}, \
                 peer {}); keeping the peer — the two builds agree on every block either can produce today and \
                 diverge at the earliest height they disagree about. If that height is already behind this chain, \
                 they are diverging NOW: compare the two builds' fence values before trusting this peer's tip",
                peer_version.id,
                self.config.params.consensus_schedule_id(),
                describe_fingerprint(&peer_version.consensus_schedule_id),
            );
        }

        // **ADR-0072 SA-2 — and this is the check the two above cannot make.**
        //
        // `consensus_identity_id` refuses a peer that disagrees about a rule in force NOW, and the
        // warning above reports one that disagrees about a fence in the FUTURE without refusing it,
        // because refusing there would partition the network for the whole rollout (M1-6). Both are
        // computed with every fence normalised away, which is exactly what makes them blind to the
        // moment the schedule stops being about the future: at the fence, a build that carries it
        // and a build that does not disagree about every block produced from there on, and both
        // still fingerprint as peers. The un-upgraded minority keeps extending the old arm and
        // refuses the majority's blocks, and nothing in the handshake says a word.
        //
        // So the position goes on the wire. Everything compared here is derived locally — this
        // node's own `Params` and its own DAA score; the peer's bytes are only ever compared
        // against digests this node computed, never interpreted — and the verdict distinguishes
        // "behind on the same schedule" (every syncing node, kept) from "a different schedule"
        // (refused, but only once this node has actually crossed something).
        //
        // Armed only where an operator scheduled a gate fence: `fork_id_gate_armed_v1` reads
        // `palw_attempt_activation` (ADR-0072) and `palw_difficulty_priced_rows` (ADR-0083) —
        // testnet-11 schedules the second at DAA 1150 (2026-09-04), every other shipped preset
        // leaves both `None` and returns `Unfenced` before comparing anything. Three presets DO
        // schedule crescendo, which is why the gate is armed by those fields rather than by "has
        // this node crossed a fence" — see the module doc. Below its fence an armed gate only
        // warns (the arm below); from the fence it refuses the build that does not carry it.
        match kaspa_consensus_core::fork_id_v1::evaluate_fork_id_v1(
            &self.config.params,
            local_daa_score,
            &peer_version.fork_id_fired,
            peer_version.fork_id_next,
        ) {
            ForkIdVerdict::Unfenced | ForkIdVerdict::Agree => {}
            ForkIdVerdict::DisagreeBeforeAnyFence(mismatch) => {
                warn!(
                    "peer {} advertises a different fence schedule ({}), and this node has not crossed a fence yet \
                     (DAA {}); keeping the peer — the two builds agree about every block either can produce today. \
                     They will NOT agree past the first fence: compare the two builds before that height arrives",
                    peer_version.id, mismatch, local_daa_score,
                );
            }
            ForkIdVerdict::DisagreePastFence { mismatch, fired_through } => {
                return Err(ProtocolError::WrongForkId(network_name, local_daa_score, fired_through, mismatch.to_string()));
            }
        }

        debug!("protocol versions - self: {}, peer: {}", PROTOCOL_VERSION, peer_version.protocol_version);

        // Register all flows according to version
        let (flows, applied_protocol_version) = match peer_version.protocol_version {
            v if v >= PROTOCOL_VERSION => (v8::register(self.clone(), router.clone(), PROTOCOL_VERSION), PROTOCOL_VERSION),
            // Back-compat: the 104 set (everything but ADR-0077 Decision 8's interval lane). Such
            // a peer is never SENT an interval message — both sends are version-filtered — and
            // never emits one, so a seat pulling an opening from a 104 executor hears nothing and
            // reaches the half-window arm exactly as it would for an executor that is offline.
            PROTOCOL_VERSION_PALW_PULL => {
                (v8::register(self.clone(), router.clone(), PROTOCOL_VERSION_PALW_PULL), PROTOCOL_VERSION_PALW_PULL)
            }
            // Back-compat: the 103 set (everything but the material pull). Claim/EVM relays and
            // the PALW push gossip are all present; such a peer is never sent a
            // PalwMaterialRequest (the send is version-filtered), so nothing unroutable reaches it.
            PROTOCOL_VERSION_PRE_PALW_PULL => {
                (v8::register(self.clone(), router.clone(), PROTOCOL_VERSION_PRE_PALW_PULL), PROTOCOL_VERSION_PRE_PALW_PULL)
            }
            // §14.2 back-compat: an EVM-tx-relay (101) peer that predates the
            // deposit-claim relay. Register the 101 flow set (EVM-tx relay, NO
            // claim relay) — claim messages (oneof 67-70) are version-filtered to
            // >= 102, so we never send one to a 101 peer (unroutable → disconnect).
            PROTOCOL_VERSION_EVM_RELAY => {
                (v8::register(self.clone(), router.clone(), PROTOCOL_VERSION_EVM_RELAY), PROTOCOL_VERSION_EVM_RELAY)
            }
            // §14.2 back-compat: pre-EVM-relay kaspa-pq binaries. Same flow set
            // minus the EVM relay flows; all EVM gossip towards such peers is
            // version-filtered (an unroutable payload type disconnects them).
            PROTOCOL_VERSION_NO_EVM_RELAY => {
                (v8::register(self.clone(), router.clone(), PROTOCOL_VERSION_NO_EVM_RELAY), PROTOCOL_VERSION_NO_EVM_RELAY)
            }
            8 => (v8::register(self.clone(), router.clone(), 8), 8),
            7 => (v7::register(self.clone(), router.clone()), 7),
            v => return Err(ProtocolError::VersionMismatch(PROTOCOL_VERSION, v)),
        };

        // Build and register the peer properties
        let peer_properties = Arc::new(PeerProperties {
            user_agent: peer_version.user_agent.to_owned(),
            advertised_protocol_version: peer_version.protocol_version,
            protocol_version: applied_protocol_version,
            disable_relay_tx: peer_version.disable_relay_tx,
            subnetwork_id: peer_version.subnetwork_id.to_owned(),
            time_offset,
        });
        router.set_properties(peer_properties);

        // Send and receive the ready signal
        handshake.exchange_ready_messages().await?;

        info!("Registering p2p flows for peer {} for protocol version {}", router, applied_protocol_version);

        // Launch all flows. Note we launch only after the ready signal was exchanged
        for flow in flows {
            flow.launch();
        }

        if router.is_outbound() || peer_version.address.is_some() {
            let mut address_manager = self.address_manager.lock();

            if router.is_outbound() {
                address_manager.add_address(router.net_address().into());
            }

            if let Some(peer_ip_address) = peer_version.address {
                address_manager.add_address(peer_ip_address);
            }
        }

        // Note: we deliberately do not hold the handshake in memory so at this point receivers for handshake subscriptions
        // are dropped, hence effectively unsubscribing from these messages. This means that if the peer re-sends them
        // it is considered a protocol error and the connection will disconnect

        Ok(())
    }
}

/// **ADR-0077 Decision 8's asks** (workstream B2) — kept in their own `impl` block, at the end of
/// the file, so that this lane and the handshake/version code above never share a merge hunk.
impl FlowContext {
    /// **Ask the network for one checkpoint interval's opening** (ADR-0077 Decision 8).
    ///
    /// Sent by a free-prompt seat for each interval its draw names. Broadcast to every capable
    /// peer, the shape `request_palw_material` starts from: the executor's own node answers from
    /// its retained capture, and any node that still holds a whole payload for the claim (a
    /// foreign retention) can answer too. The answer comes back to this node alone and is admitted
    /// only because this node asked (`note_interval_pull_request` first, or the center drops it as
    /// unsolicited). Version-filtered to the interval lane: a 104 peer has no route for the
    /// request type and would disconnect on it.
    ///
    /// `pubkey` / `signature` / `requested_daa` are ADR-0077 SA-2's authentication, assembled by
    /// the caller — the panel service, which is the only party holding the bond's ML-DSA-87 key.
    /// This layer carries them and checks nothing: a transport that verified signatures would be a
    /// transport holding opinions about who is bonded.
    pub async fn request_palw_interval_opening(
        &self,
        claim: kaspa_hashes::Hash64,
        interval_index: u32,
        pubkey: Vec<u8>,
        signature: Vec<u8>,
        requested_daa: u64,
    ) {
        if !self.palw_v2_active() {
            return;
        }
        let msg = kaspa_p2p_lib::make_message!(
            kaspa_p2p_lib::pb::kaspad_message::Payload::PalwIntervalOpeningRequest,
            kaspa_p2p_lib::pb::PalwIntervalOpeningRequestMessage {
                claim_id: Some(claim.into()),
                interval_index,
                requester_pubkey: pubkey,
                signature,
                requested_daa,
            }
        );
        self.hub().broadcast_to_peers_with_min_version(msg, PROTOCOL_VERSION_PALW_INTERVAL).await;
    }

    /// **The signed whole-capture pull** (ADR-0077 SA-2's last sentence: "the same rule covers
    /// `request_palw_material` today"). Identical to [`Self::request_palw_material`] but carrying
    /// the requester's bond key and signature, so a serving node can refuse a stranger. Preferred
    /// by every caller that holds a key; the unsigned form stays for a node that does not.
    pub async fn request_palw_material_signed(
        &self,
        claim: kaspa_hashes::Hash64,
        pubkey: Vec<u8>,
        signature: Vec<u8>,
        requested_daa: u64,
    ) {
        if !self.palw_v2_active() {
            return;
        }
        let msg = kaspa_p2p_lib::make_message!(
            kaspa_p2p_lib::pb::kaspad_message::Payload::PalwMaterialRequest,
            kaspa_p2p_lib::pb::PalwMaterialRequestMessage {
                claim_id: Some(claim.into()),
                requester_pubkey: pubkey,
                signature,
                requested_daa,
            }
        );
        self.hub().broadcast_to_peers_with_min_version(msg, PROTOCOL_VERSION_PALW_PULL).await;
    }
}
