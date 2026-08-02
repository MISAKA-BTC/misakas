use crate::{
    consensus::{
        services::{ConsensusServices, DbWindowManager},
        storage::ConsensusStorage,
    },
    errors::{BlockProcessResult, RuleError},
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            DB,
            block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
            ghostdag::{DbGhostdagStore, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            reachability::DbReachabilityStore,
            statuses::{DbStatusesStore, StatusesStore, StatusesStoreBatchExtensions, StatusesStoreReader},
            tips::{DbTipsStore, TipsStore},
        },
    },
    pipeline::{
        ProcessingCounters,
        deps_manager::{BlockProcessingMessage, BlockTaskDependencyManager, TaskId, VirtualStateProcessingMessage},
    },
    processes::{coinbase::CoinbaseManager, transaction_validator::TransactionValidator},
};
use crossbeam_channel::{Receiver, Sender};
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    KType,
    block::Block,
    blockstatus::BlockStatus::{self, StatusHeaderOnly, StatusInvalid},
    config::{genesis::GenesisBlock, params::Params},
    mass::{Mass, MassCalculator, MassOps},
    tx::Transaction,
};
use kaspa_consensus_notify::{
    notification::{BlockAddedNotification, Notification},
    root::ConsensusNotificationRoot,
};
// The EvmPayloadStore trait is in scope only where its `insert_batch` is called
// (the evm-gated content-addressed payload write in commit_body).
#[cfg(feature = "evm")]
use crate::model::stores::evm::EvmPayloadStore as _;
use kaspa_consensusmanager::SessionLock;
use kaspa_core::error;
use kaspa_notify::notifier::Notify;
use parking_lot::RwLock;
use rayon::ThreadPool;
use rocksdb::WriteBatch;
use std::sync::{Arc, atomic::Ordering};

/// A PALW view is consensus-load-bearing once algo-4 acceptance is enabled. The body-DAG downward
/// closure guarantees every header/body read used by its mergeset fold, so a store failure here is a
/// local consistency fault, not an alternative semantic result. Abort the process before committing a
/// view which silently omitted or re-epoch'd an effect.
#[cold]
#[inline(never)]
fn palw_overlay_view_fail_stop(message: String) -> ! {
    error!("{message}");
    std::process::abort()
}

pub struct BlockBodyProcessor {
    // Channels
    receiver: Receiver<BlockProcessingMessage>,
    pub(super) sender: Sender<VirtualStateProcessingMessage>,

    // Thread pool
    pub(super) thread_pool: Arc<ThreadPool>,

    // DB
    db: Arc<DB>,

    // Config
    pub(super) max_block_mass: u64,
    pub(super) genesis: GenesisBlock,
    pub(super) _ghostdag_k: KType,

    // Stores
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub(super) _ghostdag_store: Arc<DbGhostdagStore>,
    pub(super) headers_store: Arc<DbHeadersStore>,
    pub(super) block_transactions_store: Arc<DbBlockTransactionsStore>,
    /// kaspa-pq EVM Lane v0.4 (§3.1): each block's own payload, persisted at
    /// body commit so the virtual processor can assemble `AcceptedEvmTxs(B)`
    /// from MERGESET blocks' payloads. Only non-empty payloads are written
    /// (possible only on v2+ headers, i.e. post-activation), so this is inert
    /// on every current network.
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) evm_payload_store: Arc<crate::model::stores::evm::DbEvmPayloadStore>,
    /// §16 (audit R-2): raw EVM tx bytes by hash, written at body commit. Gated
    /// on the evm feature (its only writer needs `kaspa_evm::tx::tx_hash`).
    #[cfg(feature = "evm")]
    pub(super) evm_raw_tx_store: Arc<crate::model::stores::evm::DbEvmRawTxStore>,
    // Incremented only on the `evm`-gated payload path.
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) evm_raw_tx_owners_store: Arc<crate::model::stores::evm::DbEvmRawTxOwnersStore>,
    pub(super) body_tips_store: Arc<RwLock<DbTipsStore>>,
    /// ADR-0039 §14.2/§18.1: the PALW overlay store the algo-4 ticket check resolves its leaf/cert
    /// binding against, plus the lane's activation fence + epoch length.
    /// `palw_activation_daa_score` is `u64::MAX` on mainnet / testnet-10 / simnet / devnet, so
    /// `check_palw_ticket` returns before any store read and those nets are byte-identical — but it is
    /// 0 on the three PALW presets, where the read is live.
    pub(super) palw_store: Arc<crate::model::stores::palw::DbPalwStore>,
    pub(super) palw_overlay_view_store: Arc<crate::model::stores::palw_overlay_view::DbPalwOverlayViewStore>,
    /// ADR-0045 D3-b clause 13 — the PCPB context stores and windows, read at mint time to re-check
    /// (fail-closed) that the leaf's PCPB anchor is still resolvable and still agrees with the
    /// on-chain snapshot/registry.
    pub(super) palw_pcpb_store: Arc<crate::model::stores::palw_pcpb::DbPalwPcpbStore>,
    pub(super) palw_beacon_store_for_pcpb: Arc<crate::model::stores::palw_beacon::DbPalwBeaconStore>,
    pub(super) palw_snapshot_lag_epochs: u64,
    pub(super) palw_post_commit_delta_epochs: u64,
    pub(super) ghostdag_store: Arc<DbGhostdagStore>,
    pub(super) palw_activation_daa_score: u64,
    /// Bug report #6 layer 2 — above this score a disqualified selected parent no longer makes a
    /// body unacceptable, so the algo-4 ticket check must report a missing overlay view as
    /// provenance-unavailable instead of poisoning the block (params.rs has the incident).
    pub(super) palw_suture_disqualified_selected_parent_daa_score: u64,
    /// ADR-MA §17.1: the Compute Set registry band's own activation fence (`u64::MAX` on every
    /// shipped preset — registry txs are block-invalid everywhere today).
    pub(super) palw_compute_registry_activation_daa_score: u64,
    pub(super) palw_epoch_length_daa: u64,
    /// ADR-0039 §11.3 (K5): the beacon grace window, consumed by the clause-10 lagged halt indicator and
    /// the `advance_epoch_gated` activation freeze (both keyed off buried seed-carry runs).
    pub(super) palw_beacon_grace_epochs: u64,
    pub(super) palw_batch_admission: kaspa_consensus_core::palw::PalwBatchAdmissionParams,
    /// ADR-0039 §12.1 / C6 clause-6: `network_id` for `chain_commit` + the DNS params for resolving the
    /// finality-buried anchor from the block's past. Read only for algo-4 headers, none exist while gated.
    pub(super) palw_network_id: u32,
    /// ADR-0020 EVM lane activation fence. `check_evm_payload` decides EVM-inactive vs -active by this
    /// score (NOT by `version >= EVM_HEADER_VERSION`), because a PALW v3 header (version 3 ≥ 2) is admitted
    /// while the EVM lane is still inactive — such a block carries an EMPTY payload + zero EVM header
    /// commitments and must take the inactive branch. `u64::MAX` on every EVM-inert preset.
    pub(super) evm_activation_daa_score: u64,
    pub(super) dns_params: Option<kaspa_consensus_core::dns_finality::DnsParams>,

    // Managers and services
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) coinbase_manager: CoinbaseManager,
    pub(crate) mass_calculator: MassCalculator,
    pub(super) transaction_validator: TransactionValidator,
    pub(super) window_manager: DbWindowManager,

    // Pruning lock
    pruning_lock: SessionLock,

    // Dependency manager
    task_manager: BlockTaskDependencyManager,

    // Notifier
    notification_root: Arc<ConsensusNotificationRoot>,

    // Counters
    counters: Arc<ProcessingCounters>,
}

impl BlockBodyProcessor {
    pub fn new(
        receiver: Receiver<BlockProcessingMessage>,
        sender: Sender<VirtualStateProcessingMessage>,
        thread_pool: Arc<ThreadPool>,

        params: &Params,
        db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        services: &Arc<ConsensusServices>,

        pruning_lock: SessionLock,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
    ) -> Self {
        Self {
            receiver,
            sender,
            thread_pool,
            db,

            max_block_mass: params.max_block_mass,
            genesis: params.genesis.clone(),
            _ghostdag_k: params.ghostdag_k(),

            statuses_store: storage.statuses_store.clone(),
            _ghostdag_store: storage.ghostdag_store.clone(),
            headers_store: storage.headers_store.clone(),
            block_transactions_store: storage.block_transactions_store.clone(),
            evm_payload_store: storage.evm_payload_store.clone(),
            #[cfg(feature = "evm")]
            evm_raw_tx_store: storage.evm_raw_tx_store.clone(),
            evm_raw_tx_owners_store: storage.evm_raw_tx_owners_store.clone(),
            body_tips_store: storage.body_tips_store.clone(),
            palw_store: storage.palw_store.clone(),
            palw_overlay_view_store: storage.palw_overlay_view_store.clone(),
            palw_pcpb_store: storage.palw_pcpb_store.clone(),
            palw_beacon_store_for_pcpb: storage.palw_beacon_store.clone(),
            palw_snapshot_lag_epochs: params.palw_snapshot_lag_epochs,
            palw_post_commit_delta_epochs: params.palw_post_commit_delta_epochs,
            ghostdag_store: storage.ghostdag_store.clone(),
            palw_activation_daa_score: params.palw_activation_daa_score,
            palw_suture_disqualified_selected_parent_daa_score: params.palw_suture_disqualified_selected_parent_daa_score,
            palw_compute_registry_activation_daa_score: params.palw_compute_registry_activation_daa_score,
            palw_epoch_length_daa: params.palw_epoch_length_daa,
            palw_beacon_grace_epochs: params.palw_beacon_grace_epochs,
            palw_batch_admission: params.palw_batch_admission,
            palw_network_id: params.net.suffix().unwrap_or(0),
            evm_activation_daa_score: params.evm_activation_daa_score,
            dns_params: params.dns_params.clone(),

            reachability_service: services.reachability_service.clone(),
            coinbase_manager: services.coinbase_manager.clone(),
            mass_calculator: services.mass_calculator.clone(),
            transaction_validator: services.transaction_validator.clone(),
            window_manager: services.window_manager.clone(),

            pruning_lock,
            task_manager: BlockTaskDependencyManager::new(),
            notification_root,
            counters,
        }
    }

    pub fn worker(self: &Arc<BlockBodyProcessor>) {
        while let Ok(msg) = self.receiver.recv() {
            match msg {
                BlockProcessingMessage::Exit => break,
                BlockProcessingMessage::Process(task, block_result_transmitter, virtual_result_transmitter) => {
                    if let Some(task_id) = self.task_manager.register(task, block_result_transmitter, virtual_result_transmitter) {
                        let processor = self.clone();
                        self.thread_pool.spawn(move || {
                            processor.queue_block(task_id);
                        });
                    }
                }
            };
        }

        // Wait until all workers are idle before exiting
        self.task_manager.wait_for_idle();

        // Pass the exit signal on to the following processor
        self.sender.send(VirtualStateProcessingMessage::Exit).unwrap();
    }

    fn queue_block(self: &Arc<BlockBodyProcessor>, task_id: TaskId) {
        if let Some(task) = self.task_manager.try_begin(task_id) {
            let res = self.process_body(task.block(), task.is_trusted());

            let dependent_tasks = self.task_manager.end(task, |task, block_result_transmitter, virtual_state_result_transmitter| {
                let _ = block_result_transmitter.send(res.clone());
                if res.is_err() || !task.requires_virtual_processing() {
                    // We don't care if receivers were dropped
                    let _ = virtual_state_result_transmitter.send(res.clone());
                } else {
                    self.sender.send(VirtualStateProcessingMessage::Process(task, virtual_state_result_transmitter)).unwrap();
                }
            });

            for dep in dependent_tasks {
                let processor = self.clone();
                self.thread_pool.spawn(move || processor.queue_block(dep));
            }
        }
    }

    /// Does this body-validation failure justify writing a PERMANENT `StatusInvalid` row?
    ///
    /// `StatusInvalid` is not a rejection — it is a promise never to reconsider the block. The body-sync
    /// list retains only header-only blocks, so a marked block is never re-requested and every child
    /// fails on a parent that can no longer be satisfied. A wrong mark therefore does not cost one
    /// block; it costs the node its ability to ever finish IBD (2026-07-29 testnet-200 dead loop).
    ///
    /// The mark is correct only when the failure is a self-contained verdict on the block's own bytes,
    /// evaluated the same way by every node. These four are not:
    ///  * `MissingParents` — a delivery-ordering gap; the block becomes valid once its parents land.
    ///  * `BadMerkleRoot` — rejects THIS transaction set; the same header may still arrive with a body
    ///    that fits the root.
    ///  * `PrunedBlock` — rejects the body, not the block as a whole.
    ///  * `InvalidParentBodies` — the parent carries a local `StatusInvalid` mark, possibly STALE
    ///    (persisted by an older binary under different consensus rules). Cascading it forward would
    ///    grow the poisoned cone faster than `--reset-invalid-marks` can clear it.
    ///  * `PalwParentProvenanceUnavailable` — a Header-v4 parent's UTXO/lifecycle classification is not
    ///    resolvable from this node's CURRENT virtual state (disqualified-cache, still-pending, below
    ///    the local finality point, worker shutting down). Every one of those is point-of-view and
    ///    transient; see `ensure_palw_v4_parent_provenance` for the per-outcome argument.
    ///
    /// Not marking is always consensus-safe: the block is still rejected right now. The only cost is
    /// re-validating it if a peer re-offers it — the same trade `MissingParents` has always made.
    fn error_marks_block_invalid(error: &RuleError) -> bool {
        !matches!(
            error,
            RuleError::BadMerkleRoot(_, _)
                | RuleError::MissingParents(_)
                | RuleError::InvalidParentBodies(_)
                | RuleError::PalwParentProvenanceUnavailable(_)
                | RuleError::PrunedBlock
        )
    }

    fn process_body(self: &Arc<BlockBodyProcessor>, block: &Block, is_trusted: bool) -> BlockProcessResult<BlockStatus> {
        let _prune_guard = self.pruning_lock.blocking_read();
        let status = self.statuses_store.read().get(block.hash()).unwrap();
        match status {
            StatusInvalid => return Err(RuleError::KnownInvalid),
            StatusHeaderOnly => {} // Proceed to body processing
            _ if status.has_block_body() => return Ok(status),
            _ => panic!("unexpected block status {status:?}"),
        }

        let mass = match self.validate_body(block, is_trusted) {
            Ok(mass) => mass,
            Err(e) => {
                if Self::error_marks_block_invalid(&e) {
                    self.statuses_store.write().set(block.hash(), BlockStatus::StatusInvalid).unwrap();
                }
                return Err(e);
            }
        };

        self.commit_body(block.hash(), block.header.direct_parents(), block.transactions.clone(), &block.evm_payload);

        // Send a BlockAdded notification
        self.notification_root
            .notify(Notification::BlockAdded(BlockAddedNotification::new(block.to_owned())))
            .expect("expecting an open unbounded channel");

        // Report counters
        self.counters.body_counts.fetch_add(1, Ordering::Relaxed);
        self.counters.txs_counts.fetch_add(block.transactions.len() as u64, Ordering::Relaxed);
        self.counters.mass_counts.fetch_add(mass.max(), Ordering::Relaxed);
        Ok(BlockStatus::StatusUTXOPendingVerification)
    }

    fn validate_body(self: &Arc<BlockBodyProcessor>, block: &Block, is_trusted: bool) -> BlockProcessResult<Mass> {
        let mass = self.validate_body_in_isolation(block)?;
        if !is_trusted {
            self.validate_body_in_context(block)?;
        }
        Ok(mass)
    }

    fn commit_body(
        self: &Arc<BlockBodyProcessor>,
        hash: BlockHash,
        parents: &[BlockHash],
        transactions: Arc<Vec<Transaction>>,
        evm_payload: &kaspa_consensus_core::evm::EvmExecutionPayload,
    ) {
        // The EVM payload is persisted only under the evm feature (its content-
        // addressed write needs `kaspa_evm::tx::tx_hash`); a non-evm build carries
        // the empty payload on every block, so nothing is stored.
        #[cfg(not(feature = "evm"))]
        let _ = &evm_payload;
        let mut batch = WriteBatch::default();

        // This is an append only store so it requires no lock.
        self.block_transactions_store.insert_batch(&mut batch, hash, transactions).unwrap();

        // kaspa-pq EVM Lane v0.4 (§3.1): persist the block's own payload so the
        // virtual processor can later read it as part of some chain block's
        // mergeset acceptance. CONTENT-ADDRESSED: the raw tx bytes go ONCE into
        // the raw-tx store (217, keyed by hash) and the 211/235 payload stores
        // only the envelope + those hashes (SlimEvmPayload), so a tx repeated
        // across payloads costs one stored copy. The full payload reconstructs
        // from 217 on read, byte-identically. Empty payloads are skipped (absent =
        // empty); insert is idempotent under body revalidation. tx_hash needs
        // kaspa-evm (the evm feature); every non-evm build / pre-activation block
        // carries the empty payload and never reaches here.
        #[cfg(feature = "evm")]
        if !evm_payload.is_empty() {
            let slim = kaspa_consensus_core::evm::SlimEvmPayload::from_full(evm_payload, kaspa_evm::tx::tx_hash);
            for (raw, txh) in evm_payload.transactions.iter().zip(slim.tx_hashes.iter()) {
                self.evm_raw_tx_store.write_batch(&mut batch, *txh, raw.clone(), hash).unwrap();
                // Ownership ledger for the raw-tx segment pruner: the SAME tx can
                // appear in several payloads. Counting owners here — in the same
                // batch as the slim payload — is what lets the pruner reclaim the
                // bytes when the LAST owning block goes, and only then.
                self.evm_raw_tx_owners_store.increment_batch(&mut batch, *txh).unwrap();
            }
            self.evm_payload_store.insert_batch(&mut batch, hash, slim).unwrap();
        }

        // ADR-0039 §18.2 (C5 option B): build this block's fork-local batch-lifecycle view
        // `view(B) = view(SP(B)) ⊕ Δ(mergeset(B))` in the same commit batch (block-keyed, past-relative,
        // read at the selected parent by the algo-4 ticket check). Inert fast-path return on every
        // shipped preset. Its bodies-of-the-mergeset reads are sound here: the body-DAG downward closure
        // (`check_parent_bodies_exist`) guarantees every mergeset block already has a committed body.
        self.commit_palw_overlay_view(&mut batch, hash);

        let mut body_tips_write_guard = self.body_tips_store.write();
        body_tips_write_guard.add_tip_batch(&mut batch, hash, parents).unwrap();
        let statuses_write_guard =
            self.statuses_store.set_batch(&mut batch, hash, BlockStatus::StatusUTXOPendingVerification).unwrap();

        self.db.write(batch).unwrap();

        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(statuses_write_guard);
        drop(body_tips_write_guard);
    }

    /// ADR-0039 §18.2 (C5 option B) — build `hash`'s fork-local batch-lifecycle view as
    /// `view(SP(hash)) ⊕ Δ(mergeset(hash))`: clone the selected parent's view, fold in the raw/body-valid
    /// overlay-tx effects of every mergeset-blue block (manifest ⇒ Registering, leaf chunks ⇒ Committed
    /// on completeness, certificate ⇒ Certified), advance the epoch-driven edges, and drop the no-longer-
    /// referenceable batches. Written into the block's commit batch, keyed by `hash`. This is the
    /// past-relative overlay the algo-4 ticket check resolves against (C5), replacing the tip-global
    /// `DbPalwStore` read. Each manifest is admitted at ITS CARRIER block's epoch (`registration_epoch ==
    /// carrier_epoch`), a deterministic, mergeset-consistent coordinate.
    ///
    /// The fast-path guard makes this a no-op on mainnet, testnet-10, simnet and devnet, where
    /// `palw_activation_daa_score == u64::MAX`. The three PALW presets use `0`, so this builder writes
    /// a row for every block.
    ///
    /// `palw_algo4_accept` gates header admission, not persistence of the overlay view. Changes to the
    /// persisted row encoding require a database-version transition.
    ///
    /// The mergeset bodies are guaranteed present by the body-DAG downward closure.
    fn commit_palw_overlay_view(self: &Arc<BlockBodyProcessor>, batch: &mut WriteBatch, hash: BlockHash) {
        use crate::processes::palw::PalwOverlayEffect;
        use kaspa_consensus_core::palw::{PalwBatchAdmissionParams, PalwBatchViewV1};
        if self.palw_activation_daa_score == u64::MAX {
            return; // inert fast path
        }
        let current_header = self.headers_store.get_header(hash).unwrap_or_else(|store_error| {
            palw_overlay_view_fail_stop(format!("PALW body view could not read header for block {hash}: {store_error}"))
        });
        let cur_daa = current_header.daa_score;
        if cur_daa < self.palw_activation_daa_score {
            return;
        }
        // Header-v4 lifecycle provenance is acceptance-derived and is committed by the virtual worker
        // atomically with the UTXO result. The sole body-stage v4 row is the empty genesis boundary.
        // Retaining the legacy raw fold only below v4 keeps all existing presets byte-identical.
        if current_header.version >= kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION {
            if hash == self.genesis.hash {
                self.palw_overlay_view_store.set_batch(batch, hash, Arc::new(PalwBatchViewV1::new())).unwrap_or_else(|store_error| {
                    palw_overlay_view_fail_stop(format!("PALW body view could not stage v4 genesis view {hash}: {store_error}"))
                });
            }
            return;
        }
        let gd = self.ghostdag_store.get_data(hash).unwrap_or_else(|store_error| {
            palw_overlay_view_fail_stop(format!("PALW body view could not read GHOSTDAG data for block {hash}: {store_error}"))
        });
        let selected_parent = gd.selected_parent;
        let epoch_len = self.palw_epoch_length_daa.max(1);
        let epoch = cur_daa / epoch_len;
        let a: &PalwBatchAdmissionParams = &self.palw_batch_admission;

        // Seed from the selected parent's carried view (empty at genesis / a pre-activation parent).
        let mut view = self
            .palw_overlay_view_store
            .view(selected_parent)
            .unwrap_or_else(|store_error| {
                palw_overlay_view_fail_stop(format!(
                    "PALW body view could not read selected-parent view {selected_parent} while processing {hash}: {store_error}"
                ))
            })
            .map(|v| (*v).clone())
            .unwrap_or_default();

        // Fold in the COMPLETE blue mergeset, INCLUDING the selected parent. `view(SP)` deliberately
        // excludes SP's own body (a block is not in its own mergeset), so SP's effects are NOT already
        // present in the seed. The other mergeset blues are outside SP's past by definition. Thus every
        // source below is new at this coordinate and none is double-applied. Overlay txs are admitted at
        // their carrier block's epoch.
        //
        // **ADR-0040 VIEW-01 — the block's OWN body is deliberately not folded here.**
        //
        // A block is not in its own mergeset, so `B`'s own PALW overlay txs never enter `view(B)`; they
        // enter `view(C)` when a descendant C merges B. The audit read this as half of C-03 (a missing
        // self-fold), and the code did not say which it was. It is DELIBERATE. `check_palw_ticket(C)`
        // reads `view(SP(C))`, so on a linear edge B→C it still reads `view(B)` and cannot consume B's
        // just-carried lifecycle facts. C commits those facts into `view(C)` only after its own body
        // check. This full carrier gap matters because the immutable manifest/leaf/certificate blobs
        // used by the ticket are persisted at the virtual acceptance coordinate. Folding B directly
        // into `view(B)` would expose lifecycle state to the immediate child before the acceptance path
        // has even had one carrier interval in which to persist those blobs. The lagged coordinate
        // preserves that interval; the epoch registration lead is an additional protocol delay, not a
        // substitute for this coordinate rule.
        //
        // **ADR-0040 P1-5 (DOS-02 / BIND-03) — the coordinate decision, and why the view STAYS here.**
        //
        // The other half of C-03 is that this fold reads RAW mergeset transactions with no acceptance
        // filter, so a never-accepted or double-spending tx still moves the view. The obvious remedy —
        // move the view to the acceptance coordinate — is NOT available, and the reason is decisive:
        //
        //   `check_palw_ticket` resolves against `view(SP)` at BODY validation. Acceptance data exists
        //   only for blocks that have been VIRTUAL-processed, i.e. that became chain blocks. A
        //   side-chain selected parent never is. An acceptance-coordinate view would therefore be
        //   `None` for such an SP, making body validation succeed or fail depending on chain-selection
        //   and arrival order — a permanent, order-dependent `StatusInvalid`. That is a consensus split,
        //   which is strictly worse than the resource issue it would fix.
        //
        // So the view is body/mergeset-coordinate BY NECESSITY, not by oversight, and DOS-02 is closed
        // by BOUNDING what an unaccepted fold can achieve rather than by filtering it out. It is now
        // closed by REMOVAL, which is stronger than a bound:
        //
        //   * the fold writes NO per-leaf state at all. The `job_nullifiers` map this arm used to grow —
        //     up to 64 unpriced, ownership-unbound entries per leaf-chunk tx, retained to an
        //     attacker-chosen expiry, in a struct cloned and re-persisted every block — is DELETED
        //     (ADR-0040 P1-9, withdrawn from this coordinate as a spec change). The persisted view is
        //     therefore `|batches| ≤ max_view_batches` entries and nothing else: an EXACT,
        //     parameter-free bound of ZERO per-leaf bytes on every fork at every height;
        //   * a forged batch cannot become MINEABLE — ADR-0040 CERT-TRUST made this fold monotone and
        //     non-destructive (promotion + write-once `cert_hash` only), and the certificate a ticket
        //     actually uses must resolve out of `palw_store`, which is written only behind the STORE
        //     gate `verify_certificate_attestation` (real ML-DSA quorum over active bonds) at the
        //     virtual coordinate. `apply_certificate` itself verifies nothing — the bound is the store
        //     gate, and the ticket reads that store, never this view's `cert_hash`. View mutation alone
        //     certifies nothing and, crucially, DESTROYS nothing;
        //   * the number of view entries is capped (`max_view_batches`, DOS-03), so slots are finite —
        //     and that cap is enforced: a preset that activates PALW
        //     with `max_view_batches == 0` fails `PalwBatchAdmissionParams::is_consistent_for_activation`;
        //   * leaves are write-once and manifest-bounded (P1-1), so entries cannot be grown or rewritten;
        //   * every fold source is a mergeset BLUE, i.e. a block someone had to mine, so consuming a view
        //     slot costs block production — the network's own rate limit — rather than being free.
        //
        // Residual risk: refusal at capacity prevents eviction but permits pre-emption. Once the cap is
        // full, every subsequent manifest is
        // refused until entries expire. And the pre-emption is nearly free, because `min_leaf_bond_sompi
        // = 0` on every shipped preset, so `admission_valid`'s bond requirement (`leaf_count ·
        // min_leaf_bond_sompi`) is vacuous — the only cost is producing the blues that carry the
        // manifests. So the true residual is: an attacker who can mine can lock honest providers out of
        // the view for up to one expiry window, at block-production cost alone.
        //
        // Pricing is a re-genesis calibration decision: `min_leaf_bond_sompi` must become
        // non-zero, large enough that filling `max_view_batches` slots costs more than the value of the
        // censorship window. Two things have to be re-checked together whenever either moves —
        // raising `max_view_batches` raises the flood cost but also the per-block clone cost, and
        // raising the bond prices out small honest providers. This is an ACTIVATION-blocking item; see
        // the ADR-0040 §5.12 gate row.
        for &blue in gd.mergeset_blues.iter() {
            let carrier_daa = self.headers_store.get_daa_score(blue).unwrap_or_else(|store_error| {
                palw_overlay_view_fail_stop(format!(
                    "PALW body view could not read DAA score for mergeset block {blue} while processing {hash}: {store_error}"
                ))
            });
            let carrier_epoch = carrier_daa / epoch_len;
            let txs = self.block_transactions_store.get(blue).unwrap_or_else(|store_error| {
                palw_overlay_view_fail_stop(format!(
                    "PALW body view could not read guaranteed mergeset body {blue} while processing {hash}: {store_error}"
                ))
            });
            for tx in txs.iter() {
                let Some(kind) = tx.subnetwork_id.palw_tx_kind() else { continue };
                match crate::processes::palw::parse_palw_overlay(kind, &tx.payload) {
                    Ok(PalwOverlayEffect::Manifest(m)) => {
                        view.apply_manifest(
                            &m,
                            carrier_epoch,
                            a.max_batch_leaves,
                            a.max_leaf_chunk_leaves,
                            a.registration_lead_epochs,
                            a.active_window_epochs,
                            a.audit_window_epochs,
                            a.min_leaf_bond_sompi,
                            a.max_view_batches,
                            // Static-audit finding H-01 — INERT here, deliberately. This legacy raw
                            // fold returns above for every Header-v4+ block, so it only ever runs on
                            // pre-v4 presets, where sponsorship is not fenced on and where the whole
                            // point of retaining this path is byte-identical replay. Passing INERT
                            // writes exactly the lifecycle row it has always written.
                            kaspa_consensus_core::palw::PalwManifestSponsorshipV1::INERT,
                        );
                    }
                    Ok(PalwOverlayEffect::LeafChunk(c)) => {
                        // ADR-0040 P1-9 — the GLOBAL job-nullifier claim is WITHDRAWN from this
                        // coordinate (spec change; see `PalwBatchViewV1`'s doc and ADR-0040). It was
                        // never in force — its bool fed a `continue` that ended a loop body containing
                        // nothing else, and `job_nullifier_spent` had no production reader — and it
                        // cannot be armed here: authorising a claim needs the provider's ML-DSA
                        // signature over `ReplicaExecutionReceiptV1::signing_hash`, which requires an
                        // `ActiveBondView` that exists only at the virtual coordinate. The rule re-lands
                        // there as a REWARD rule; here, a chunk's applicability is fully expressed by
                        // the bitmap, so this is the whole delta. `apply_leaf_chunk`'s bool is
                        // intentionally unused: refusal (unknown batch / non-Registering status /
                        // duplicate or out-of-range `chunk_index`) is a no-op on the view by design.
                        view.apply_leaf_chunk(&c.batch_id, c.chunk_index);
                    }
                    Ok(PalwOverlayEffect::Certificate(cert)) => {
                        // kaspa-pq **ADR-0040 CERT-TRUST** — this fold is MONOTONE and reads NOTHING the
                        // certificate declares beyond its own content hash.
                        //
                        // Accurate statement of which coordinate verifies what (the previous comment
                        // here was false and is replaced):
                        //
                        //   * BODY (here): no `ActiveBondView` exists, and — per the DOS-02 note above —
                        //     the tx need not even be accepted. Nothing a certificate says can be
                        //     checked. So this only promotes `Committed|Auditing → Certified` and sets
                        //     `cert_hash` write-once. It never ranks, never overwrites, never copies a
                        //     window or a stake figure. §12′ supersession is REMOVED from this
                        //     coordinate (spec change): it ranked by a self-declared `approving_stake`,
                        //     so `u128::MAX` won every comparison and evicted honest certificates.
                        //   * VIRTUAL (`apply_palw_overlay_effect` → `verify_certificate_attestation`):
                        //     the bond view exists, the vote tally is RECOMPUTED, and `approving_stake`
                        //     is bound to it. Only then may the blob be persisted into `palw_store`.
                        //   * TICKET (`body_validation_in_context`): the certificate a header actually
                        //     uses is resolved out of that attested store, and its `[activation,
                        //     expiry)` window is taken from the attested blob — never from this view.
                        //
                        // Hence a junk certificate tx can at worst promote a batch to `Certified` with a
                        // `cert_hash` naming no attested blob, which mines nothing.
                        view.apply_certificate(&cert.batch_id, cert.hash(), carrier_daa);
                    }
                    // Beacon commit/reveal (0x35/0x36) stay on the acceptance/virtual coordinate; provider
                    // bond (0x30) + slashing/unbond are their own slices; malformed payloads are dropped.
                    _ => {}
                }
            }
        }

        // ADR-0039 §11.3 (K5): freeze Certified→Active while the lagged buried beacon-health signal is
        // not Healthy. Computed LAZILY — only when a Certified batch could actually flip this epoch (the
        // gate cannot influence any other transition, so the walk is skipped otherwise) — from THIS
        // block's selected parent, the SAME coordinate `check_palw_ticket` gates its in-memory advance
        // on (the two sites must never diverge on an activation net). Fail-closed: no dns_params / no
        // buried anchor / < 2 samples ⇒ frozen.
        let could_activate = view
            .batches
            .values()
            .any(|e| e.status == kaspa_consensus_core::palw::PalwBatchStatus::Certified && epoch >= e.activation_not_before_epoch);
        let activation_open = could_activate
            && self
                .dns_params
                .as_ref()
                .and_then(|dns| {
                    crate::processes::palw::resolve_palw_lagged_anchor(
                        &self.headers_store,
                        &self.reachability_service,
                        dns,
                        selected_parent,
                    )
                })
                .map(|anchor| {
                    kaspa_consensus_core::palw::palw_lagged_activation_open(&self.palw_buried_epoch_samples(anchor.anchor_hash))
                })
                .unwrap_or(false);
        view.advance_epoch_gated(epoch, a.registration_lead_epochs, a.audit_window_epochs, activation_open);
        view.retain(epoch, cur_daa, a.registration_lead_epochs, a.audit_window_epochs);
        self.palw_overlay_view_store.set_batch(batch, hash, Arc::new(view)).unwrap_or_else(|store_error| {
            palw_overlay_view_fail_stop(format!("PALW body view could not stage view for block {hash}: {store_error}"))
        });
    }

    /// ADR-0039 §11.3 (K5): the lagged buried `(palw_epoch, seed)` samples below a clause-6 anchor —
    /// the shared input of the clause-10 halt indicator, the activation gate, and (future) the algo-4
    /// template's `palw_template_lane_open` check. `grace + 2` distinct epochs suffice to certify a
    /// carry run `> grace` and to answer the two-newest-distinct-epochs activation question.
    pub(super) fn palw_buried_epoch_samples(&self, anchor_hash: BlockHash) -> Vec<(u64, kaspa_hashes::Hash64)> {
        crate::processes::palw::resolve_palw_buried_epoch_seeds(
            &self.headers_store,
            &self.reachability_service,
            anchor_hash,
            self.palw_activation_daa_score,
            self.palw_epoch_length_daa,
            self.palw_beacon_grace_epochs.saturating_add(2),
        )
    }

    pub fn process_genesis(self: &Arc<BlockBodyProcessor>) {
        // Init tips store
        let mut batch = WriteBatch::default();
        let mut body_tips_write_guard = self.body_tips_store.write();
        body_tips_write_guard.init_batch(&mut batch, &[]).unwrap();
        self.db.write(batch).unwrap();
        drop(body_tips_write_guard);

        // Write the genesis body
        self.commit_body(self.genesis.hash, &[], Arc::new(self.genesis.build_genesis_transactions()), &Default::default())
    }
}

#[cfg(test)]
mod invalid_marking_tests {
    use super::BlockBodyProcessor;
    use crate::errors::RuleError;
    use kaspa_consensus_core::MerkleRoot;

    /// The persisted-`StatusInvalid` set is a consensus-liveness surface, not a stylistic choice:
    /// every wrongly-marked block is permanently un-re-requestable and takes its whole future cone
    /// with it. Pin both directions so a future error variant has to make the choice explicitly.
    #[test]
    fn point_of_view_failures_never_persist_an_invalid_mark() {
        // Node-local / ordering / point-of-view — rejected now, reconsidered later.
        for transient in [
            RuleError::MissingParents(vec![1.into()]),
            RuleError::InvalidParentBodies(vec![1.into()]),
            RuleError::BadMerkleRoot(MerkleRoot::from_u64_word(2), MerkleRoot::from_u64_word(3)),
            RuleError::PrunedBlock,
            // 2026-07-29 testnet-200: the first poisoned block was a v4 child whose selected parent
            // was disqualified by a beacon-seed mismatch — never a fault of its own body.
            RuleError::PalwParentProvenanceUnavailable("selected parent is disqualified".to_string()),
        ] {
            assert!(
                !BlockBodyProcessor::error_marks_block_invalid(&transient),
                "{transient} is a point-of-view condition and must stay re-requestable"
            );
        }

        // Self-contained verdicts on the block's own bytes — every node reaches the same one.
        for permanent in [
            RuleError::WrongSubsidy(1, 2),
            RuleError::PalwTicketInvalid("leaf is not active".to_string()),
            RuleError::NoTransactions,
            RuleError::FirstTxNotCoinbase,
        ] {
            assert!(
                BlockBodyProcessor::error_marks_block_invalid(&permanent),
                "{permanent} is a verdict on the block itself and must be marked"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::ConfigBuilder,
        consensus::test_consensus::TestConsensus,
        constants::TX_VERSION,
        model::stores::{
            block_transactions::BlockTransactionsStoreReader,
            ghostdag::{GhostdagStore, GhostdagStoreReader},
            headers::HeaderStore,
        },
    };
    use kaspa_consensus_core::{
        api::{BlockValidationFutures, ConsensusApi},
        blockstatus::BlockStatus,
        coinbase::MinerData,
        config::params::DEVNET_PALW_PARAMS,
        dns_finality::p2pkh_mldsa87_spk,
        merkle::calc_hash_merkle_root,
        palw::PalwBatchManifestV1,
        subnets::SUBNETWORK_ID_PALW_BATCH_MANIFEST,
        tx::{Transaction, TransactionInput, TransactionOutpoint},
    };
    use kaspa_hashes::Hash64;
    use rocksdb::WriteBatch;
    use std::sync::Arc;

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    /// `view(B)` excludes B's own body, but `view(child(B))` must include B through the child's
    /// complete blue mergeset. A two-parent merge must likewise fold both the selected parent and the
    /// non-SP blue. The old selected-parent filter made every view on a linear chain empty and dropped
    /// one carrier at every merge.
    #[tokio::test]
    async fn palw_overlay_view_folds_complete_mergeset_on_linear_and_dag() {
        let config = ConfigBuilder::new(DEVNET_PALW_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let admission = &config.params.palw_batch_admission;

        let registration_epoch = 0;
        let activation_not_before_epoch = registration_epoch + admission.registration_lead_epochs + admission.audit_window_epochs;
        let mut manifest = PalwBatchManifestV1 {
            version: 1,
            batch_id: Hash64::default(),
            registration_epoch,
            model_profile_id: h(0x11),
            runtime_class_id: h(0x12),
            leaf_count: 1,
            chunk_count: 1,
            leaf_root: h(0x13),
            descriptor_root: h(0x14),
            total_leaf_bond_sompi: admission.min_leaf_bond_sompi,
            audit_policy_id: h(0x15),
            activation_not_before_epoch,
            expiry_epoch: activation_not_before_epoch + admission.active_window_epochs,
        };
        manifest.batch_id = manifest.content_id();
        assert!(manifest.admission_valid(
            registration_epoch,
            admission.max_batch_leaves,
            admission.max_leaf_chunk_leaves,
            admission.registration_lead_epochs,
            admission.active_window_epochs,
            admission.audit_window_epochs,
            admission.min_leaf_bond_sompi,
        ));

        // A missing outpoint is intentional: the body/mergeset-coordinate view consumes raw body-valid
        // transactions, independently of later UTXO acceptance. The source body therefore needs only an
        // isolation-valid input shape for this regression.
        let manifest_tx = Transaction::new(
            TX_VERSION,
            vec![TransactionInput::new(TransactionOutpoint::new(h(0xa1), 0), vec![], u64::MAX, 0)],
            vec![],
            0,
            SUBNETWORK_ID_PALW_BATCH_MANIFEST,
            0,
            borsh::to_vec(&manifest).unwrap(),
        );
        let miner = MinerData::new(p2pkh_mldsa87_spk(&[0x21; 64]), vec![]);
        let mut carrier = consensus.build_utxo_valid_block_with_parents(h(0xb1), vec![config.genesis.hash], miner.clone(), vec![]);
        carrier.transactions.push(manifest_tx);
        carrier.header.hash_merkle_root = calc_hash_merkle_root(carrier.transactions.iter());
        let carrier_hash = carrier.header.hash;
        let BlockValidationFutures { block_task, virtual_state_task } = consensus.validate_and_insert_block(carrier.to_immutable());
        assert_eq!(block_task.await.unwrap(), BlockStatus::StatusUTXOPendingVerification);
        let _ = virtual_state_task.await;
        let carrier_txs = consensus.storage.block_transactions_store.get(carrier_hash).unwrap();
        assert!(carrier_txs.iter().any(|tx| tx.subnetwork_id == SUBNETWORK_ID_PALW_BATCH_MANIFEST));

        let carrier_view = consensus.storage.palw_overlay_view_store.view(carrier_hash).unwrap().expect("PALW-active carrier view");
        assert!(carrier_view.entry(&manifest.batch_id).is_none(), "a carrier must not fold its own body into its own view");

        // The deliberately unfunded carrier is UTXO-disqualified, so the ordinary template builder
        // will not choose it as a parent. Install the exact one-edge header/GHOSTDAG facts and invoke
        // the real view builder directly; this keeps the regression about the body-coordinate rule,
        // not transaction signing or virtual-chain selection.
        let child_hash = h(0xb2);
        let child_header = Arc::new(consensus.build_header_with_parents(child_hash, vec![carrier_hash]));
        let child_ghostdag = Arc::new(consensus.ghostdag_manager().ghostdag(&[carrier_hash]));
        consensus.storage.headers_store.insert(child_hash, child_header, 0).unwrap();
        consensus.storage.ghostdag_store.insert(child_hash, child_ghostdag).unwrap();
        let body_processor = consensus.block_body_processor();
        let mut batch = WriteBatch::default();
        body_processor.commit_palw_overlay_view(&mut batch, child_hash);
        body_processor.db.write(batch).unwrap();

        let child_ghostdag = consensus.storage.ghostdag_store.get_data(child_hash).unwrap();
        assert_eq!(child_ghostdag.selected_parent, carrier_hash);
        assert!(child_ghostdag.mergeset_blues.contains(&carrier_hash));

        let child_view = consensus.storage.palw_overlay_view_store.view(child_hash).unwrap().expect("PALW-active child view");
        assert!(child_view.entry(&manifest.batch_id).is_some(), "the child must fold its selected parent's overlay body");

        let mut side_manifest = manifest.clone();
        side_manifest.batch_id = Hash64::default();
        side_manifest.model_profile_id = h(0x31);
        side_manifest.leaf_root = h(0x32);
        side_manifest.batch_id = side_manifest.content_id();
        let side_manifest_tx = Transaction::new(
            TX_VERSION,
            vec![TransactionInput::new(TransactionOutpoint::new(h(0xa2), 0), vec![], u64::MAX, 0)],
            vec![],
            0,
            SUBNETWORK_ID_PALW_BATCH_MANIFEST,
            0,
            borsh::to_vec(&side_manifest).unwrap(),
        );
        let mut side_carrier = consensus.build_utxo_valid_block_with_parents(h(0xc1), vec![config.genesis.hash], miner, vec![]);
        side_carrier.transactions.push(side_manifest_tx);
        side_carrier.header.hash_merkle_root = calc_hash_merkle_root(side_carrier.transactions.iter());
        let side_carrier_hash = side_carrier.header.hash;
        let BlockValidationFutures { block_task, virtual_state_task } =
            consensus.validate_and_insert_block(side_carrier.to_immutable());
        assert_eq!(block_task.await.unwrap(), BlockStatus::StatusUTXOPendingVerification);
        let _ = virtual_state_task.await;
        assert!(
            consensus
                .storage
                .palw_overlay_view_store
                .view(side_carrier_hash)
                .unwrap()
                .expect("PALW-active side-carrier view")
                .entry(&side_manifest.batch_id)
                .is_none(),
            "the second carrier must also exclude its own body"
        );

        let merger_hash = h(0xc2);
        let merger_parents = vec![carrier_hash, side_carrier_hash];
        let merger_header = Arc::new(consensus.build_header_with_parents(merger_hash, merger_parents.clone()));
        let merger_ghostdag = Arc::new(consensus.ghostdag_manager().ghostdag(&merger_parents));
        assert!(merger_ghostdag.mergeset_blues.contains(&carrier_hash));
        assert!(merger_ghostdag.mergeset_blues.contains(&side_carrier_hash));
        consensus.storage.headers_store.insert(merger_hash, merger_header, 0).unwrap();
        consensus.storage.ghostdag_store.insert(merger_hash, merger_ghostdag).unwrap();
        let mut batch = WriteBatch::default();
        body_processor.commit_palw_overlay_view(&mut batch, merger_hash);
        body_processor.db.write(batch).unwrap();

        let merger_view = consensus.storage.palw_overlay_view_store.view(merger_hash).unwrap().expect("PALW-active merger view");
        assert!(merger_view.entry(&manifest.batch_id).is_some(), "merger must retain the first carrier");
        assert!(merger_view.entry(&side_manifest.batch_id).is_some(), "merger must fold the second carrier");

        consensus.shutdown(wait_handles);
    }
}
