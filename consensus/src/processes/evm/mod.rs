//! Consensus → EVM executor seam (ADR-0020 §"PQ-only reconciliation").
//!
//! The lazy chain-context validation hook (P3 2/2 hot-path wiring) calls
//! [`evm_validate_and_persist`] when a block first becomes a selected-chain
//! candidate. Everything here is gated behind the non-default `evm` cargo
//! feature, so the default node never links revm/secp — the secp-free guarantee
//! enforced by `scripts/pq-ci-guard.sh` is unaffected. The EVM lane is also
//! `u64::MAX`-inert on every default network (`is_evm_active` is always false),
//! so even an `--features evm` node never runs this until a net sets a finite
//! `evm_activation_daa_score`.

#[cfg(feature = "evm")]
pub use kaspa_evm::{AcceptedTxCandidate, EvmBlockInput, execute_block_evm};

/// v0.4 §6.1 class-1 payload admission (syntactic, per tx): EIP-2718 decode +
/// ECDSA signer recovery + chain-id binding + a declared gas-limit sanity band
/// (≥ the 21k intrinsic floor, +32k for creates; ≤ the per-chain-block accepted
/// gas cap, since a never-acceptable tx must not be includable). Returns the
/// first offending tx index + reason. Cheap and context-free — it runs at body
/// validation, where a violation invalidates the PAYLOAD block itself (the
/// producer chose its own payload; design v0.4 §6.2).
///
/// Only an `evm` build can decode txs. The non-evm variant admits everything:
/// on every default net the lane is `u64::MAX`-inert so no v2 header (and no
/// non-empty payload) is ever admitted; an evm-ACTIVE net must run an
/// `--features evm` node (the executor seam below enforces the same).
#[cfg(feature = "evm")]
pub fn admit_evm_payload_txs(payload: &kaspa_consensus_core::evm::EvmExecutionPayload) -> Result<(), (usize, String)> {
    use rayon::prelude::*;
    // O2 (optimization design v0.1): per-tx admission is pure and independent,
    // and a full 128 KiB payload holds ~1,150 txs at ~80µs of k256 recovery
    // each (~92ms sequential — material at 10 BPS). Parallelize across the
    // body processor's rayon pool; determinism is preserved by reporting the
    // MINIMUM failing index (identical to the sequential first-failure).
    payload
        .transactions
        .par_iter()
        .enumerate()
        .filter_map(|(i, raw)| kaspa_evm::tx::admit_tx(raw).err().map(|reason| (i, reason)))
        .min_by_key(|(i, _)| *i)
        .map_or(Ok(()), Err)
}

#[cfg(not(feature = "evm"))]
pub fn admit_evm_payload_txs(_payload: &kaspa_consensus_core::evm::EvmExecutionPayload) -> Result<(), (usize, String)> {
    Ok(())
}

/// v0.4 §9.2: typed reason a `DepositClaim` cannot execute against its lock.
/// Shared by the consensus acceptance path ([`validate_evm_deposit_claims`]) and
/// the producer template path ([`prepare_deposit_claims`]) so construction ==
/// validation, and so the producer can distinguish a TRANSIENT miss (retain +
/// retry the queued claim) from a TERMINAL one (evict it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepositClaimValidationError {
    /// The lock outpoint is absent from the claim view — not yet on this node's
    /// selected chain (a lagging miner / forky DAG), or already consumed.
    /// TRANSIENT: the view alone cannot tell it apart from a soon-claimable lock,
    /// so it must NOT be evicted on sight — retain + retry.
    AbsentOrSpent,
    /// The outpoint exists but is not an `EVM_DEPOSIT_LOCK` output. Terminal.
    NotDepositLock,
    /// The lock's bound EVM address does not match the claim. Terminal.
    AddressMismatch,
    /// The lock value does not match the claimed amount. Terminal.
    AmountMismatch,
    /// The lock's committed claim tip does not match the claim. Terminal.
    TipMismatch,
    /// The claim tip exceeds the locked amount. Terminal.
    TipExceedsAmount,
    /// `pov_daa ≥ refund_timeout` — the lock now belongs to the refund path
    /// (AC-2 exclusivity). Terminal (it only ages further out of the claim window).
    RefundWindowOpen,
}

impl DepositClaimValidationError {
    /// Only the absent/spent miss is transient (retain + retry the queued claim);
    /// every other reason is terminal — the claim can never execute, so evict it.
    pub fn is_retryable(self) -> bool {
        matches!(self, DepositClaimValidationError::AbsentOrSpent)
    }
}

/// v0.4 §9.2: validate ONE deposit claim against its already-resolved lock entry
/// (`entry == None` ⇔ the lock is absent/spent in the view). The SINGLE source of
/// truth for claim validity — used by both the consensus acceptance path and the
/// producer template path so they agree byte-for-byte. Returns the consumed lock
/// entry on success. Pure + always compiled (the lock parses via kaspa-txscript;
/// no revm).
pub fn validate_one_deposit_claim(
    claim: &kaspa_consensus_core::evm::DepositClaim,
    entry: Option<kaspa_consensus_core::tx::UtxoEntry>,
    pov_daa_score: u64,
) -> Result<kaspa_consensus_core::tx::UtxoEntry, DepositClaimValidationError> {
    use DepositClaimValidationError as E;
    let entry = entry.ok_or(E::AbsentOrSpent)?;
    let lock = kaspa_txscript::script_class::parse_evm_deposit_lock(&entry.script_public_key).ok_or(E::NotDepositLock)?;
    if lock.evm_address != claim.evm_address.as_bytes() {
        return Err(E::AddressMismatch);
    }
    if entry.amount != claim.amount_sompi {
        return Err(E::AmountMismatch);
    }
    if lock.claim_tip_sompi != claim.claim_tip_sompi {
        return Err(E::TipMismatch);
    }
    if claim.claim_tip_sompi > claim.amount_sompi {
        return Err(E::TipExceedsAmount);
    }
    // AC-2 exclusivity: claim valid iff accepting daa < timeout (at/after the
    // timeout the lock belongs to the refund path).
    if pov_daa_score >= lock.timeout_daa_score {
        return Err(E::RefundWindowOpen);
    }
    Ok(entry)
}

/// v0.4 §9.2: validate a chain block's own `DepositClaim` system ops against
/// the claim view (the selected-parent UTXO set composed with the mergeset
/// diff so far — a lock spent by a mergeset tx is no longer claimable, and a
/// lock created in B's own body is not visible yet, the "same-block" rule).
/// Returns the consumed `(outpoint, entry)` pairs in payload order.
///
/// Every violation is a fault of the ACCEPTING producer (it selected its own
/// system ops, §6.2): the per-claim rules of [`validate_one_deposit_claim`] plus
/// a duplicate outpoint within the block. Pure + always compiled.
pub fn validate_evm_deposit_claims<V: kaspa_consensus_core::utxo::utxo_view::UtxoView>(
    payload: &kaspa_consensus_core::evm::EvmExecutionPayload,
    claim_view: &V,
    pov_daa_score: u64,
) -> Result<Vec<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)>, String> {
    use kaspa_consensus_core::evm::EvmSystemOp;
    let mut consumed = Vec::with_capacity(payload.system_ops.len());
    let mut seen = std::collections::HashSet::new();
    for (i, op) in payload.system_ops.iter().enumerate() {
        let EvmSystemOp::DepositClaim(claim) = op;
        if !seen.insert(claim.deposit_outpoint) {
            return Err(format!("system op #{i}: duplicate deposit-lock outpoint {}", claim.deposit_outpoint));
        }
        match validate_one_deposit_claim(claim, claim_view.get(&claim.deposit_outpoint), pov_daa_score) {
            Ok(entry) => consumed.push((claim.deposit_outpoint, entry)),
            Err(e) => return Err(format!("system op #{i}: deposit lock {} {e:?}", claim.deposit_outpoint)),
        }
    }
    Ok(consumed)
}

/// v0.4 §9.2 (narrow P0-1): the PRODUCER's snapshot of the deposit claims it puts
/// in a template, prepared in ONE virtual generation — the lock entries are
/// materialized from the SAME UTXO view the template's selected parent is taken
/// from, eliminating the mixed-generation TOCTOU (template built on generation A,
/// claims re-validated against a later generation B).
#[derive(Default)]
pub struct PreparedDepositClaims {
    /// Claims that validated against the captured view — go into `payload.system_ops`.
    pub accepted: Vec<kaspa_consensus_core::evm::DepositClaim>,
    /// The lock entries the accepted claims consume — folded into the template's
    /// `utxo_commitment` so the mined block reproduces the validator's recompute.
    pub consumed_locks: Vec<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)>,
    /// Claims that could not be included, tagged so the mining manager reconciles
    /// its queue: `Absent` ⇒ retain + retry, `Invalid` ⇒ evict.
    pub stale: Vec<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::block::EvmClaimStaleKind)>,
}

/// v0.4 §9.2 (narrow P0-1): classify the queued claims against a single captured
/// claim view (generation A), materializing the consumed lock entries while the
/// caller still holds the virtual read lock. Caps at
/// `MAX_DEPOSIT_CLAIMS_PER_EVM_BLOCK` accepted claims; a within-batch duplicate is
/// dropped from THIS template only (the queue entry is kept). Uses the SAME
/// per-claim rule as the acceptance path ([`validate_one_deposit_claim`]) so a
/// block built from this snapshot reproduces the validator's verdict. Pure +
/// always compiled (no revm, no store re-read).
pub fn prepare_deposit_claims<V: kaspa_consensus_core::utxo::utxo_view::UtxoView>(
    system_ops: &[kaspa_consensus_core::evm::DepositClaim],
    claim_view: &V,
    pov_daa_score: u64,
) -> PreparedDepositClaims {
    use kaspa_consensus_core::block::EvmClaimStaleKind;
    let cap = kaspa_consensus_core::evm::MAX_DEPOSIT_CLAIMS_PER_EVM_BLOCK;
    let mut prepared = PreparedDepositClaims::default();
    let mut seen = std::collections::HashSet::new();
    for claim in system_ops {
        if prepared.accepted.len() >= cap {
            // Beyond the per-block cap — not included AND not stale (still queued, retried next template).
            break;
        }
        if !seen.insert(claim.deposit_outpoint) {
            // Duplicate within this selection — drop from this template only; keep the queue entry.
            continue;
        }
        match validate_one_deposit_claim(claim, claim_view.get(&claim.deposit_outpoint), pov_daa_score) {
            Ok(entry) => {
                prepared.accepted.push(claim.clone());
                prepared.consumed_locks.push((claim.deposit_outpoint, entry));
            }
            Err(e) if e.is_retryable() => prepared.stale.push((claim.deposit_outpoint, EvmClaimStaleKind::Absent)),
            Err(_) => prepared.stale.push((claim.deposit_outpoint, EvmClaimStaleKind::Invalid)),
        }
    }
    prepared
}

/// v0.4 §9 — fold the bridge's UTXO side-effects into the accepting block's
/// OWN per-block diff + multiset (the slashing-side-effect mechanism,
/// verbatim): consumed deposit locks leave the UTXO set; each `WithdrawOp`
/// materializes as a synthetic output at
/// `(synthetic_withdrawal_txid(evm_tx_hash, op), 0)`. Because they ride the
/// persisted per-block diff, reorg apply/revert is the existing UTXO
/// machinery — the EVM side never reverts (pointer-switch only), so combined
/// supply is conserved across any reorg with zero bespoke code (invariant I7).
pub fn apply_evm_bridge_effects(
    diff: &mut kaspa_consensus_core::utxo::utxo_diff::UtxoDiff,
    multiset: &mut kaspa_muhash::MuHash,
    pov_daa_score: u64,
    consumed_locks: &[(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)],
    withdrawals: &[kaspa_consensus_core::evm::WithdrawOp],
) -> Result<(), String> {
    use kaspa_consensus_core::muhash::MuHashExtensions;
    for (outpoint, entry) in consumed_locks {
        diff.remove_utxo(outpoint, entry).map_err(|e| format!("consume deposit lock {outpoint}: {e}"))?;
        multiset.remove_utxo(outpoint, entry);
    }
    for w in withdrawals {
        // Defensive re-assertion (audit F2 — defense in depth): the producer's F002 handler already
        // ran validate_withdraw before emitting, and the verifier re-executes the EVM, so a valid
        // WithdrawOp always satisfies these (they can never false-reject). Re-asserting at the
        // materialization chokepoint means a regression in the executor→WithdrawOp path can never
        // mint a zero-value or oversized-script synthetic UTXO into the committed set.
        if w.amount_sompi == 0 {
            return Err(format!("withdrawal op (evm_tx {:?}, op {}) has zero amount", w.evm_tx_hash, w.op_index));
        }
        if w.script_public_key.script().len() > kaspa_consensus_core::evm::MAX_WITHDRAW_SCRIPT_BYTES {
            return Err(format!(
                "withdrawal op (evm_tx {:?}, op {}) destination script {} bytes exceeds MAX_WITHDRAW_SCRIPT_BYTES {}",
                w.evm_tx_hash,
                w.op_index,
                w.script_public_key.script().len(),
                kaspa_consensus_core::evm::MAX_WITHDRAW_SCRIPT_BYTES
            ));
        }
        // Keyed by the withdrawing EVM tx's hash (pre-mining-stable) — a block-hash key would be
        // circular for the PRODUCER, whose own utxo_commitment must already contain this output
        // before mining — AND by the full materialized content (from/amount/script) so a
        // contract-mediated, branch-dependent withdraw can never collide on the outpoint (audit F1).
        let txid = kaspa_consensus_core::evm::synthetic_withdrawal_txid(
            w.evm_tx_hash,
            w.op_index,
            w.from,
            w.amount_sompi,
            &w.script_public_key,
        );
        let outpoint = kaspa_consensus_core::tx::TransactionOutpoint::new(txid, 0);
        let entry = kaspa_consensus_core::tx::UtxoEntry::new(w.amount_sompi, w.script_public_key.clone(), pov_daa_score, false);
        diff.add_utxo(outpoint, entry.clone()).map_err(|e| format!("materialize withdrawal {outpoint}: {e}"))?;
        multiset.add_utxo(&outpoint, &entry);
    }
    Ok(())
}

/// The staged output of a validated EVM step: the full execution result (its
/// `.header` row + `withdrawals` feed the bridge; `receipts` +
/// `candidate_outcomes` feed the §16 indexes), the child state snapshot, and
/// the per-candidate `(tx hash, source payload block)` meta — committed by the
/// caller atomically with the block's UTXO diff. Always compiled (plain
/// consensus types) so the commit path signature is feature-free.
///
/// §14.1 disk-budget note: phase-1 DELIBERATELY shares the consensus RocksDB
/// batch instead of a separate EVM write queue — the no-replay/commitment
/// guarantees rest on the EVM rows landing atomically with the UTXO diff, and
/// the write volume is bounded per chain block by the payload byte cap +
/// accepted-gas cap (D4). A separate EVM state DB (with its own flush and
/// compaction queue) becomes mandatory only with Stage 2+ state growth, where
/// snapshots stop being the state representation.
pub struct EvmStaged {
    pub result: kaspa_consensus_core::evm::EvmExecutionResult,
    pub snapshot: kaspa_consensus_core::evm::EvmStateSnapshot,
    /// Parallel to `result.candidate_outcomes` (the acceptance input order).
    pub candidate_meta: Vec<(kaspa_hashes::EvmH256, kaspa_consensus_core::BlockHash)>,
    /// §11: the per-accepting-block `debug_traceTransaction` replay plan, assembled
    /// alongside execution. `None` for an acceptance with no candidate txs (nothing
    /// to trace). RPC/replay data only — never consensus-committed.
    pub trace_body: Option<kaspa_consensus_core::evm::EvmTraceReplayBodyV1>,
    /// §12 archive: this block's forward state DIFF over its selected parent
    /// (prefix 220) — `compute_state_diff(parent_snapshot, child_snapshot)`. Always
    /// `Some` on the validate path (even an empty diff is recorded so every
    /// canonical block N has a diff over N-1 and reconstruction has an unbroken
    /// parent chain). RPC/archive data only — never consensus-committed.
    pub state_diff: Option<kaspa_consensus_core::evm::EvmStateDiffV2>,
}

/// O12 (IBD catch-up pipeline): a worker thread that pre-executes the EVM
/// acceptance of a RUN of consecutive pending chain blocks, overlapping the
/// virtual thread's serial UTXO validation. Pure speculation: the worker
/// chains parent state IN MEMORY (block N+1 executes from N's just-computed
/// snapshot), performs the exact `evm_validate` logic (including the
/// commitment compare), and ships each result over a bounded channel. ALL
/// commits stay on the virtual thread in canonical order — the pipeline only
/// changes WHEN the pure execution happens, never its inputs or outputs, so
/// the committed bytes are identical to the inline path (any divergence would
/// surface as a CommitmentMismatch disqualification, the built-in oracle).
///
/// Disqualification safety: if the virtual thread disqualifies a block without
/// consuming its result (UTXO fault cascade), [`EvmPipeline::recv`] discards
/// the stale results that precede the next consumed block — stale entries are
/// always for earlier chain positions. When the walk ends, dropping the
/// pipeline closes the channel and the worker exits on its next send.
pub struct EvmPipeline {
    rx: std::sync::mpsc::Receiver<(kaspa_consensus_core::BlockHash, Result<EvmStaged, String>)>,
    _handle: std::thread::JoinHandle<()>,
}

impl EvmPipeline {
    /// Receive the pipelined result for `expected`, discarding stale results of
    /// blocks the walk disqualified before consuming theirs. `None` ⇒ the
    /// pipeline ended (worker error already delivered, or channel closed) — the
    /// caller falls back to inline validation.
    pub fn recv(&self, expected: kaspa_consensus_core::BlockHash) -> Option<Result<EvmStaged, String>> {
        while let Ok((block, res)) = self.rx.recv() {
            if block == expected {
                return Some(res);
            }
            // A result for an earlier, disqualified-and-skipped block — discard.
        }
        None
    }
}

/// §16: stage the receipt + tx-lookup index rows of one validated ACCEPTING
/// chain block into `batch` (called inside the same `commit_utxo_state` batch
/// as the EVM header/state rows). Index data only — never consensus-committed.
/// Bounded per row (`MAX_TX_LOCATION_*`); the reader resolves canonicality of
/// `accepted_in` entries against the current selected chain.
pub fn stage_evm_index_rows(
    receipts_store: &crate::model::stores::evm::DbEvmReceiptsStore,
    tx_index_store: &crate::model::stores::evm::DbEvmTxIndexStore,
    log_index_store: &crate::model::stores::evm::DbEvmLogIndexStore,
    trace_store: &crate::model::stores::evm::DbEvmTraceReplayStore,
    diff_store: &crate::model::stores::evm::DbEvmStateDiffStore,
    code_store: &crate::model::stores::evm::DbEvmCodeStore,
    checkpoint_store: &crate::model::stores::evm::DbEvmStateCheckpointStore,
    batch: &mut rocksdb::WriteBatch,
    accepting: kaspa_consensus_core::BlockHash,
    staged: &EvmStaged,
) -> Result<(), kaspa_database::prelude::StoreError> {
    use kaspa_consensus_core::evm::{EvmCandidateOutcome, MAX_TX_LOCATION_ACCEPTANCES, MAX_TX_LOCATION_INCLUSIONS};

    // §11: the per-block `debug_traceTransaction` replay plan (prefix 219), keyed by
    // the accepting block. Present only when the block accepted candidate txs.
    if let Some(body) = &staged.trace_body {
        trace_store.insert_batch(batch, accepting, body.clone())?;
    }

    // §12 archive state history: the block's forward state DIFF over its selected
    // parent (prefix 220) + any bytecode it newly deployed (prefix 222, content-
    // addressed) + a full CHECKPOINT every EVM_CHECKPOINT_INTERVAL canonical blocks
    // (prefix 221) — the anchors a historical reconstruction seeds from. Always
    // written when the lane is active (the diff is smaller than the full snapshot
    // already persisted to prefix 206); a later retention slice GCs them per the
    // node's `--evm-history-mode`. RPC/archive data only — never consensus-committed.
    if let Some(diff) = &staged.state_diff {
        diff_store.insert_batch(batch, accepting, diff.clone())?;
        for (code_hash, code) in kaspa_consensus_core::evm::diff_code_entries(diff, &staged.snapshot) {
            code_store.write_batch(batch, code_hash, code.to_vec())?;
        }
        let evm_number = staged.result.header.evm_number;
        if evm_number % kaspa_consensus_core::evm::EVM_CHECKPOINT_INTERVAL == 0 {
            let checkpoint = kaspa_consensus_core::evm::EvmStateCheckpointV1::build(
                accepting,
                evm_number,
                staged.result.header.state_root,
                &staged.snapshot,
            );
            checkpoint_store.insert_batch(batch, accepting, checkpoint)?;
        }
    }

    // audit R2-#6: candidate_meta and candidate_outcomes are produced in lockstep
    // by the executor, and every receipt_index it emits is < receipts.len(). These
    // indexes are staged into the consensus commit batch, so rather than trust that
    // invariant with raw `[i]` / `[receipt_index]` indexing (an out-of-bounds would
    // panic the node mid-commit), verify it once and fail closed as a store error.
    if staged.candidate_meta.len() != staged.result.candidate_outcomes.len() {
        return Err(kaspa_database::prelude::StoreError::DataInconsistency(format!(
            "EVM index staging: candidate_meta ({}) != candidate_outcomes ({})",
            staged.candidate_meta.len(),
            staged.result.candidate_outcomes.len()
        )));
    }

    // §8: record that this block is now indexed (the floor the get_evm_logs index
    // fast path may trust). Runs for every EVM block, including those with no logs.
    log_index_store.set_floor_batch(batch, staged.result.header.evm_number)?;

    if !staged.result.receipts.is_empty() {
        // tx_hashes parallel to the receipts: the accepted candidates in order.
        let mut tx_hashes = vec![Default::default(); staged.result.receipts.len()];
        for (i, (hash, _src)) in staged.candidate_meta.iter().enumerate() {
            if let EvmCandidateOutcome::Accepted { receipt_index } = staged.result.candidate_outcomes[i] {
                let ri = receipt_index as usize;
                if ri >= tx_hashes.len() {
                    return Err(kaspa_database::prelude::StoreError::DataInconsistency(format!(
                        "EVM index staging: receipt_index {ri} >= receipts {}",
                        tx_hashes.len()
                    )));
                }
                tx_hashes[ri] = *hash;
            }
        }
        receipts_store.insert_batch(
            batch,
            accepting,
            kaspa_consensus_core::evm::EvmBlockReceipts { receipts: staged.result.receipts.clone(), tx_hashes },
        )?;

        // §8: secondary log postings (address + topic0..3) for each log, so a
        // long-range eth_getLogs can range-scan a contract/topic instead of
        // re-walking every block's receipts. Keyed by block-global position;
        // written for every UTXO-valid block (side branches included) — the query
        // canonical-filters each posting's l1_hash. RPC index only.
        let evm_number = staged.result.header.evm_number;
        for (rcpt_idx, receipt) in staged.result.receipts.iter().enumerate() {
            for (in_rcpt_idx, log) in receipt.logs.iter().enumerate() {
                let loc = kaspa_consensus_core::evm::LogPostingLoc {
                    evm_number,
                    l1_hash: accepting,
                    tx_index: rcpt_idx as u32,
                    in_receipt_log_index: in_rcpt_idx as u32,
                };
                log_index_store.write_posting_batch(
                    batch,
                    kaspa_consensus_core::evm::LogPostingKind::Address,
                    &log.address.as_bytes(),
                    &loc,
                )?;
                for (ti, topic) in log.topics.iter().take(4).enumerate() {
                    if let Some(kind) = kaspa_consensus_core::evm::LogPostingKind::topic(ti) {
                        log_index_store.write_posting_batch(batch, kind, &topic.as_bytes(), &loc)?;
                    }
                }
            }
        }
    }

    for (i, (hash, src)) in staged.candidate_meta.iter().enumerate() {
        let mut row = tx_index_store.get_or_default(*hash)?;
        if !row.included_in.contains(src) {
            if row.included_in.len() >= MAX_TX_LOCATION_INCLUSIONS {
                row.included_in.remove(0);
            }
            row.included_in.push(*src);
        }
        match staged.result.candidate_outcomes[i] {
            EvmCandidateOutcome::Accepted { receipt_index } => {
                if !row.accepted_in.iter().any(|(b, _)| *b == accepting) {
                    if row.accepted_in.len() >= MAX_TX_LOCATION_ACCEPTANCES {
                        row.accepted_in.remove(0);
                    }
                    row.accepted_in.push((accepting, receipt_index));
                }
                row.last_skip_class = None;
            }
            EvmCandidateOutcome::Skipped { class } => {
                if row.accepted_in.is_empty() {
                    row.last_skip_class = Some(class);
                }
            }
        }
        tx_index_store.write_batch(batch, *hash, row)?;
    }
    Ok(())
}

/// A §12 reconstruction-gather failure (design §12.4) — fail closed; the caller
/// maps it to an RPC error, never a silent empty state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconstructGatherError {
    /// A store read failed (RocksDB / decode).
    Store(String),
    /// A checkpoint's snapshot could not be decoded (bad checksum / encoding).
    Checkpoint(String),
    /// The block IS an EVM block but a diff/checkpoint on its parent chain is not
    /// retained on this node (GC'd past `--evm-history-mode` retention).
    Unavailable(String),
    /// The backward walk exceeded its bound — the checkpoint/diff chain is broken.
    TooDeep(String),
}

impl std::fmt::Display for ReconstructGatherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReconstructGatherError::Store(m) => write!(f, "EVM reconstruction store read: {m}"),
            ReconstructGatherError::Checkpoint(m) => write!(f, "EVM reconstruction checkpoint: {m}"),
            ReconstructGatherError::Unavailable(m) => write!(f, "EVM state history unavailable: {m}"),
            ReconstructGatherError::TooDeep(m) => write!(f, "EVM reconstruction walk too deep: {m}"),
        }
    }
}

/// §12 reconstruction GATHER (design §12.4): walk `block`'s selected-parent chain
/// backward, collecting forward diffs until anchoring on a checkpoint (its full
/// state — that block's own diff is then unneeded) or reaching the pre-activation
/// genesis (the empty state). Returns `(seed_snapshot, forward_diffs)` ordered
/// anchor-child..target for `kaspa_evm::reconstruct::reconstruct_evm_state` to
/// replay + verify. Following the BLOCK'S OWN parent links (not the canonical
/// number map) serves canonical and side-branch blocks uniformly. Pure store-walk
/// (no revm); the three reads are closures so it is unit-testable offline.
#[allow(clippy::type_complexity)]
pub fn gather_reconstruction_inputs<E: std::fmt::Display>(
    block: kaspa_consensus_core::BlockHash,
    get_checkpoint: impl Fn(kaspa_consensus_core::BlockHash) -> Result<Option<kaspa_consensus_core::evm::EvmStateCheckpointV1>, E>,
    get_diff: impl Fn(kaspa_consensus_core::BlockHash) -> Result<Option<kaspa_consensus_core::evm::EvmStateDiffV2>, E>,
    has_header: impl Fn(kaspa_consensus_core::BlockHash) -> bool,
) -> Result<(kaspa_consensus_core::evm::EvmStateSnapshot, Vec<kaspa_consensus_core::evm::EvmStateDiffV2>), ReconstructGatherError> {
    // A healthy chain anchors on a checkpoint within EVM_CHECKPOINT_INTERVAL steps
    // (or reaches genesis for an early block). Far beyond ⇒ broken chain — fail closed.
    const MAX_RECONSTRUCTION_DIFFS: usize = 8 * kaspa_consensus_core::evm::EVM_CHECKPOINT_INTERVAL as usize;

    let mut pending: Vec<kaspa_consensus_core::evm::EvmStateDiffV2> = Vec::new();
    let mut cur = block;
    let seed = loop {
        if let Some(cp) = get_checkpoint(cur).map_err(|e| ReconstructGatherError::Store(e.to_string()))? {
            break cp.decode_snapshot().map_err(|e| ReconstructGatherError::Checkpoint(format!("{cur}: {e}")))?;
        }
        let diff = get_diff(cur)
            .map_err(|e| ReconstructGatherError::Store(e.to_string()))?
            .ok_or_else(|| ReconstructGatherError::Unavailable(format!("no diff for {cur} (older than retention)")))?;
        let parent = diff.parent;
        pending.push(diff);
        if pending.len() > MAX_RECONSTRUCTION_DIFFS {
            return Err(ReconstructGatherError::TooDeep(format!("{block}: exceeded {MAX_RECONSTRUCTION_DIFFS} diffs")));
        }
        // A parent with no EVM header is pre-activation ⇒ anchor = empty genesis.
        if !has_header(parent) {
            break kaspa_consensus_core::evm::EvmStateSnapshot::default();
        }
        cur = parent;
    };
    pending.reverse();
    Ok((seed, pending))
}

/// C-01 Stage 1: apply a forward [`EvmStateDiffV2`] to the flat latest-canonical
/// store (prefix 234) — the persistent analog of the §12 in-memory `apply_state_diff`.
/// Writes changed accounts and deletes destroyed / EIP-161-empty ones into `batch`,
/// reusing the §12 diff (220) as the change set (no new format). The caller commits
/// `batch` atomically with the block so the flat state advances with the canonical head.
pub fn apply_diff_to_flat(
    flat: &crate::model::stores::evm::DbEvmFlatAccountStore,
    batch: &mut rocksdb::WriteBatch,
    diff: &kaspa_consensus_core::evm::EvmStateDiffV2,
) -> Result<(), kaspa_database::prelude::StoreError> {
    use kaspa_consensus_core::evm::{EVM_EMPTY_CODE_HASH, EvmU256, FlatAccount};
    use std::collections::BTreeMap;
    for ch in &diff.account_changes {
        let addr = ch.address;
        let Some(after) = &ch.after else {
            flat.delete_batch(batch, addr)?; // destroyed
            continue;
        };
        // Read current storage (or empty for a new account), apply the slot changes.
        let cur = flat.get(addr)?.unwrap_or_default();
        let mut storage: BTreeMap<[u8; 32], [u8; 32]> = cur.storage.iter().map(|(s, v)| (s.to_be_bytes(), v.to_be_bytes())).collect();
        for sc in &ch.storage_changes {
            let slot = sc.slot.to_be_bytes();
            if sc.after.is_zero() {
                storage.remove(&slot);
            } else {
                storage.insert(slot, sc.after.to_be_bytes());
            }
        }
        // An account left EIP-161-empty is not in canonical form ⇒ delete it.
        if after.nonce == 0 && after.balance.is_zero() && after.code_hash == EVM_EMPTY_CODE_HASH && storage.is_empty() {
            flat.delete_batch(batch, addr)?;
        } else {
            let storage = storage.into_iter().map(|(s, v)| (EvmU256::from_be_bytes(s), EvmU256::from_be_bytes(v))).collect();
            flat.write_batch(batch, addr, FlatAccount { core: after.clone(), storage })?;
        }
    }
    Ok(())
}

/// C-01 Stage 1: materialize the full canonical [`EvmStateSnapshot`] from the flat
/// store — the input to the keccak-MPT `state_root` recompute and to the IBD
/// pruning-point snapshot. Code is resolved from the content-addressed store (222);
/// a missing code (`code_hash != empty`, absent) is store corruption — fail closed.
/// `flat.iter()` yields address order, so the snapshot is already canonical-sorted.
pub fn materialize_snapshot(
    flat: &crate::model::stores::evm::DbEvmFlatAccountStore,
    code: &crate::model::stores::evm::DbEvmCodeStore,
) -> Result<kaspa_consensus_core::evm::EvmStateSnapshot, kaspa_database::prelude::StoreError> {
    use crate::model::stores::evm::EvmCodeStoreReader;
    use kaspa_consensus_core::evm::{EVM_EMPTY_CODE_HASH, EvmStateSnapshot};
    let mut accounts = Vec::new();
    for entry in flat.iter() {
        let (addr, fa) = entry?;
        let code_bytes = if fa.core.code_hash == EVM_EMPTY_CODE_HASH {
            Vec::new()
        } else {
            code.get(fa.core.code_hash)?.ok_or_else(|| {
                kaspa_database::prelude::StoreError::DataInconsistency(format!("flat materialize: missing code for {addr}"))
            })?
        };
        accounts.push(fa.to_snapshot(addr, code_bytes));
    }
    Ok(EvmStateSnapshot { accounts })
}

/// C-01 Stage 1 (slice S9c) — the production [`FlatStateReader`] seam.
///
/// A thin O(1) point-lookup adapter that wires the inert kaspa-evm flat backend
/// ([`kaspa_evm::flat_backend::FlatStateBackend`] / [`kaspa_evm::flat_backend::flat_backed_cachedb`],
/// slice S3) to the real consensus stores: account rows from [`DbEvmFlatAccountStore`] (prefix 234)
/// and contract code from the content-addressed [`DbEvmCodeStore`] (prefix 222). It holds borrows,
/// so it is cheap to construct per seed and never enumerates (the full-state walk uses the store's
/// own `iter()` directly).
///
/// **INERT — not yet on any live execution path.** Wrapping this in `flat_backed_cachedb` gives a
/// `CacheDB` the executor could seed LAZILY (reading only the accounts a block touches) instead of
/// eagerly materializing the full parent snapshot ([`materialize_snapshot`] → `seed_cachedb`). That
/// live cutover is **deferred to Stage 2** for a concrete reason: the committed `state_root` is a
/// keccak-MPT over the FULL post-state, but a lazy `CacheDB` holds only the TOUCHED accounts, so a
/// correct root after lazy execution still requires a full O(state) flat enumeration — which negates
/// the lazy-seed win until Stage 2 supplies a persistent incremental-MPT root. Generalizing
/// `execute_block_evm` from its `EmptyDB` (`Infallible` errors) to a fallible backend is the same
/// Stage-2 work. This adapter is the stable interface that work plugs into; today it only proves the
/// production reads reproduce the eager materialization (see the test).
#[cfg(feature = "evm")]
#[derive(Clone, Copy)]
pub struct StoreFlatReader<'a> {
    flat: &'a crate::model::stores::evm::DbEvmFlatAccountStore,
    code: &'a crate::model::stores::evm::DbEvmCodeStore,
}

#[cfg(feature = "evm")]
impl<'a> StoreFlatReader<'a> {
    pub fn new(
        flat: &'a crate::model::stores::evm::DbEvmFlatAccountStore,
        code: &'a crate::model::stores::evm::DbEvmCodeStore,
    ) -> Self {
        Self { flat, code }
    }
}

#[cfg(feature = "evm")]
impl kaspa_evm::flat_backend::FlatStateReader for StoreFlatReader<'_> {
    fn flat_account(
        &self,
        address: kaspa_consensus_core::evm::EvmAddress,
    ) -> Result<Option<kaspa_consensus_core::evm::FlatAccount>, kaspa_evm::flat_backend::FlatBackendError> {
        self.flat.get(address).map_err(|e| kaspa_evm::flat_backend::FlatBackendError::Store(e.to_string()))
    }

    fn flat_code(&self, code_hash: kaspa_hashes::EvmH256) -> Result<Option<Vec<u8>>, kaspa_evm::flat_backend::FlatBackendError> {
        use crate::model::stores::evm::EvmCodeStoreReader;
        self.code.get(code_hash).map_err(|e| kaspa_evm::flat_backend::FlatBackendError::Store(e.to_string()))
    }
}

/// C-01 Stage 1 (S8, audit M-01): seed the flat latest-canonical state from a full
/// snapshot — the pruned-IBD pruning-point import path. The caller has already verified
/// `snapshot` against the committed EVM state root, so this is a trusted seed. Writes into
/// the caller's `batch` (atomic with the 206 import): the flat accounts (234), each
/// contract's bytecode into the content-addressed code store (222, keyed by `code_hash` —
/// the joining node's code store is otherwise empty, and the flat rows reference code only
/// by hash), the block→root index (232), and the latest pointer (231) pinned to
/// `pruning_point`. Any pre-existing flat row is cleared first so the store ends EXACTLY
/// equal to `snapshot` (a fresh-IBD node has none; the clear is defensive). Flat / code /
/// root / pointer are state data only — never part of any commitment — so seeding here is
/// consensus-neutral. INERT unless the shadow state backend is enabled (the caller gates it).
#[allow(clippy::too_many_arguments)]
pub fn seed_flat_from_snapshot(
    flat: &crate::model::stores::evm::DbEvmFlatAccountStore,
    code_store: &crate::model::stores::evm::DbEvmCodeStore,
    root_store: &crate::model::stores::evm::DbEvmBlockStateRootStore,
    ptr_store: &mut crate::model::stores::evm::DbEvmLatestStatePtrStore,
    batch: &mut rocksdb::WriteBatch,
    pruning_point: kaspa_consensus_core::BlockHash,
    state_root: kaspa_hashes::EvmH256,
    snapshot: &kaspa_consensus_core::evm::EvmStateSnapshot,
) -> Result<(), kaspa_database::prelude::StoreError> {
    use kaspa_consensus_core::evm::{EvmLatestStatePtr, FlatAccount};
    // Defensive clear: drop any stale flat rows so a re-import (or a previously-shadowed node)
    // ends with exactly the imported snapshot. Empty (no-op) on a fresh-IBD node. Batched deletes
    // precede the writes below, so an address present in both is correctly overwritten.
    let stale: Vec<_> = flat.iter().filter_map(|r| r.ok().map(|(addr, _)| addr)).collect();
    for addr in stale {
        flat.delete_batch(batch, addr)?;
    }
    for account in &snapshot.accounts {
        if !account.code.is_empty() {
            code_store.write_batch(batch, account.code_hash, account.code.clone())?;
        }
        flat.write_batch(batch, account.address, FlatAccount::from_snapshot(account))?;
    }
    root_store.write_batch(batch, pruning_point, state_root)?;
    ptr_store.set_batch(batch, EvmLatestStatePtr { canonical_head: pruning_point, state_root })?;
    Ok(())
}

/// C-01 Stage 1: build a [`ReconState`] directly from the flat store (prefix 234)
/// — the cores + storage the keccak-MPT state root commits to. Code bytes are NOT
/// resolved (the reconstruction comparison is over `code_hash`, not code), so this
/// cannot fail on a content-addressed-store gap and is cheaper than
/// [`materialize_snapshot`]. Address order is the store's key order.
pub fn flat_to_recon(
    flat: &crate::model::stores::evm::DbEvmFlatAccountStore,
) -> Result<kaspa_consensus_core::evm::ReconState, kaspa_database::prelude::StoreError> {
    let mut state = kaspa_consensus_core::evm::ReconState::new();
    for entry in flat.iter() {
        let (addr, fa) = entry?;
        let storage = fa.storage.iter().map(|(s, v)| (s.to_be_bytes(), v.to_be_bytes())).collect();
        state.insert(addr.as_bytes(), kaspa_consensus_core::evm::ReconAccount { core: fa.core, storage });
    }
    Ok(state)
}

/// C-01 Stage 1 (slice S4) outcome of one shadow dual-write — for the caller's log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowOutcome {
    /// Clean head extension: the differential passed and the diff applied incrementally (O(changed)).
    Extended,
    /// Reorg (slice S5): the flat store was re-based to the new head by reverting/applying §12
    /// diffs along the divergence (O(reorg-depth × changed)), differential-checked.
    Rebased,
    /// Bootstrap (no pointer) or a retention gap that blocks the incremental re-base: the flat
    /// store was reseeded from the committed snapshot (the 206 source of truth).
    Reseeded,
    /// `head` history mode dropped the diff — no state history to maintain.
    SkippedNoDiff,
}

/// C-01 Stage 1 (slice S4) error. A [`Self::Divergence`] is a backend bug the
/// caller turns into a node HALT (never serve a wrong root).
#[derive(Debug)]
pub enum ShadowError {
    /// The flat backend disagrees with the committed post-state — halt the node.
    Divergence { block: kaspa_consensus_core::BlockHash, committed_root: kaspa_hashes::EvmH256, detail: String },
    /// A store read/write failed (treated like any other commit-path store error).
    Store(kaspa_database::prelude::StoreError),
}

impl std::fmt::Display for ShadowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShadowError::Divergence { block, committed_root, detail } => write!(
                f,
                "C-01 shadow state-backend DIVERGENCE at block {block} (committed EVM state_root {committed_root}): {detail}. \
                 The flat backend disagrees with the committed snapshot — HALTING this node. The committed bytes come from the \
                 206 snapshot, not the flat store, so chain integrity is intact; fix the backend and re-shadow."
            ),
            ShadowError::Store(e) => write!(f, "C-01 shadow state-backend store error: {e}"),
        }
    }
}
impl std::error::Error for ShadowError {}

/// Cap on the §12-diff divergence the incremental re-base will walk before
/// giving up and falling back to a full reseed (a deeper move is past any
/// realistic finality-bounded reorg; the reseed is correct, just O(state)).
const MAX_REBASE_STEPS: usize = 8192;

/// The `(revert, forward)` diff paths of a re-base: diffs to apply in REVERSE off
/// the old head down to the common ancestor, and diffs to apply FORWARD from the
/// ancestor up to the new parent.
type RebasePaths = (Vec<kaspa_consensus_core::evm::EvmStateDiffV2>, Vec<kaspa_consensus_core::evm::EvmStateDiffV2>);

/// C-01 Stage 1 (slice S5) — walk the §12 diff chain from `from` and `to` (via
/// `diff.parent` + the sequential `evm_number`) to their common ancestor. Returns
/// `(revert, forward)`: the diffs to apply IN REVERSE off `from` (down to the
/// ancestor) and the diffs to apply FORWARD from the ancestor up to `to`. `None`
/// ⇒ a diff or number was unavailable (retention gap / pre-activation) or the
/// divergence exceeded [`MAX_REBASE_STEPS`] ⇒ the caller falls back to a reseed.
fn rebase_diff_paths(
    from: kaspa_consensus_core::BlockHash,
    to: kaspa_consensus_core::BlockHash,
    get_diff: &impl Fn(
        kaspa_consensus_core::BlockHash,
    ) -> Result<Option<kaspa_consensus_core::evm::EvmStateDiffV2>, kaspa_database::prelude::StoreError>,
    get_number: &impl Fn(kaspa_consensus_core::BlockHash) -> Result<Option<u64>, kaspa_database::prelude::StoreError>,
) -> Result<Option<RebasePaths>, kaspa_database::prelude::StoreError> {
    let (mut a, mut b) = (from, to);
    let (mut na, mut nb) = match (get_number(a)?, get_number(b)?) {
        (Some(x), Some(y)) => (x, y),
        _ => return Ok(None),
    };
    let mut revert = Vec::new();
    let mut forward = Vec::new();
    // Align the deeper side down to the shallower side's evm_number (sequential
    // along the selected-parent chain, so each step decrements by exactly one).
    while na > nb {
        let Some(d) = get_diff(a)? else { return Ok(None) };
        a = d.parent;
        revert.push(d);
        na -= 1;
        if revert.len() + forward.len() > MAX_REBASE_STEPS {
            return Ok(None);
        }
    }
    while nb > na {
        let Some(d) = get_diff(b)? else { return Ok(None) };
        b = d.parent;
        forward.push(d);
        nb -= 1;
        if revert.len() + forward.len() > MAX_REBASE_STEPS {
            return Ok(None);
        }
    }
    // Now at equal depth — step both back in lockstep until they meet.
    while a != b {
        let Some(da) = get_diff(a)? else { return Ok(None) };
        let Some(db) = get_diff(b)? else { return Ok(None) };
        a = da.parent;
        b = db.parent;
        revert.push(da);
        forward.push(db);
        if revert.len() + forward.len() > MAX_REBASE_STEPS {
            return Ok(None);
        }
    }
    forward.reverse(); // ancestor → … → `to`
    Ok(Some((revert, forward)))
}

/// Whether a reconstructed account equals a committed-snapshot account (cores +
/// non-zero storage; code is committed by `code_hash`). Both `None` = absent in
/// both.
fn recon_account_matches(
    got: Option<&kaspa_consensus_core::evm::ReconAccount>,
    want: Option<&kaspa_consensus_core::evm::EvmAccountSnapshot>,
) -> bool {
    match (got, want) {
        (None, None) => true,
        (Some(g), Some(w)) => {
            g.core.nonce == w.nonce
                && g.core.balance == w.balance
                && g.core.code_hash == w.code_hash
                && g.storage.len() == w.storage.len()
                && w.storage.iter().all(|(s, v)| g.storage.get(&s.to_be_bytes()) == Some(&v.to_be_bytes()))
        }
        _ => false,
    }
}

/// C-01 Stage 1 (slice S4 + S5) — node-local SHADOW dual-write + live differential.
/// OFF by default; enabled per node by `--evm-shadow-state-backend`.
///
/// On every canonical-head-extending block this maintains the flat latest-
/// canonical store (234) + block→root index (232) + latest pointer (231)
/// ALONGSIDE the existing 206 snapshot, and verifies that applying the block's
/// §12 diff to the persisted flat state reproduces the committed post-state
/// (`staged.snapshot`, whose keccak-MPT root IS the committed `state_root`). The
/// check is structural over the reconstructed state (nonce/balance/code_hash/
/// storage) — root-equivalent and secp-free (no keccak). A divergence is a
/// BACKEND bug ⇒ [`ShadowError::Divergence`] so the caller HALTS the node; the
/// committed 206 bytes never depended on the flat store, so a halt costs only
/// this node's availability — chain integrity is untouched (design §7 failure
/// mode). 206 is still written by the caller.
///
/// Reorg (pointer ≠ parent, slice S5): re-base the flat store to the new head by
/// reverting/applying §12 diffs along the divergence (`get_diff`/`get_number`
/// walk the chain), differential-checked over the touched accounts. A retention
/// gap falls back to a full reseed; bootstrap (no pointer) always reseeds. All
/// writes go into the caller's `batch`, so the flat state advances atomically
/// with the block (risk R3). secp-free; never invoked on a non-evm build
/// (`staged` is always `None` there) nor on a default network (lane inert).
#[allow(clippy::too_many_arguments)]
pub fn shadow_dual_write_flat(
    flat: &crate::model::stores::evm::DbEvmFlatAccountStore,
    block_root: &crate::model::stores::evm::DbEvmBlockStateRootStore,
    latest_ptr: &mut crate::model::stores::evm::DbEvmLatestStatePtrStore,
    code: &crate::model::stores::evm::DbEvmCodeStore,
    batch: &mut rocksdb::WriteBatch,
    current: kaspa_consensus_core::BlockHash,
    staged: &EvmStaged,
    get_diff: impl Fn(
        kaspa_consensus_core::BlockHash,
    ) -> Result<Option<kaspa_consensus_core::evm::EvmStateDiffV2>, kaspa_database::prelude::StoreError>,
    get_number: impl Fn(kaspa_consensus_core::BlockHash) -> Result<Option<u64>, kaspa_database::prelude::StoreError>,
) -> Result<ShadowOutcome, ShadowError> {
    use kaspa_consensus_core::evm::{
        EvmAddress, EvmLatestStatePtr, EvmU256, FlatAccount, ReconAccount, ReconState, apply_inverse_state_diff, apply_state_diff,
        compute_state_diff, recon_from_snapshot,
    };
    use std::collections::BTreeSet;

    let Some(diff) = &staged.state_diff else {
        // `head` mode dropped the diff (no state history kept) ⇒ nothing to maintain.
        return Ok(ShadowOutcome::SkippedNoDiff);
    };
    let committed_root = staged.result.header.state_root;
    let parent = diff.parent; // selected_parent(current) — the EVM parent
    let ptr = latest_ptr.get().map_err(ShadowError::Store)?;

    let advance = |batch: &mut rocksdb::WriteBatch,
                   latest_ptr: &mut crate::model::stores::evm::DbEvmLatestStatePtrStore|
     -> Result<(), ShadowError> {
        block_root.write_batch(batch, current, committed_root).map_err(ShadowError::Store)?;
        latest_ptr
            .set_batch(batch, EvmLatestStatePtr { canonical_head: current, state_root: committed_root })
            .map_err(ShadowError::Store)?;
        Ok(())
    };
    let divergence = |detail: String| ShadowError::Divergence { block: current, committed_root, detail };

    // (1) Clean head extension: the flat store represents `parent`.
    if ptr.map(|p| p.canonical_head == parent).unwrap_or(false) {
        let mut got = flat_to_recon(flat).map_err(ShadowError::Store)?;
        apply_state_diff(&mut got, diff)
            .map_err(|e| divergence(format!("§12 diff is inconsistent with the persisted flat parent state: {e}")))?;
        let expected = recon_from_snapshot(&staged.snapshot);
        if got != expected {
            return Err(divergence(format!(
                "flat-derived post-state ({} accounts) != committed snapshot ({} accounts)",
                got.len(),
                expected.len()
            )));
        }
        apply_diff_to_flat(flat, batch, diff).map_err(ShadowError::Store)?;
        advance(batch, latest_ptr)?;
        return Ok(ShadowOutcome::Extended);
    }

    // (2) Reorg: the pointer is at some other head. Re-base incrementally by
    // reverting the flat state back to the common ancestor of `old_head` and
    // `parent`, then applying forward to `parent`, then applying this block. A
    // retention gap (`None`) drops to the reseed below.
    let rebase = match ptr {
        Some(p) => rebase_diff_paths(p.canonical_head, parent, &get_diff, &get_number).map_err(ShadowError::Store)?,
        None => None,
    };
    if let Some((revert, forward)) = rebase {
        // Touched accounts = every account named by a path diff or this block's diff.
        let mut touched: BTreeSet<[u8; 20]> = BTreeSet::new();
        for d in revert.iter().chain(forward.iter()).chain(std::iter::once(diff)) {
            for ch in &d.account_changes {
                touched.insert(ch.address.as_bytes());
            }
        }
        // Load only the touched accounts from the flat store (= old-head state).
        let mut recon: ReconState = ReconState::new();
        for &a in &touched {
            if let Some(fa) = flat.get(EvmAddress::from_bytes(a)).map_err(ShadowError::Store)? {
                let storage = fa.storage.iter().map(|(s, v)| (s.to_be_bytes(), v.to_be_bytes())).collect();
                recon.insert(a, ReconAccount { core: fa.core, storage });
            }
        }
        // Revert off the old head, replay forward to the new parent, then this block.
        for d in &revert {
            apply_inverse_state_diff(&mut recon, d)
                .map_err(|e| divergence(format!("reorg revert inconsistent with flat state: {e}")))?;
        }
        for d in &forward {
            apply_state_diff(&mut recon, d).map_err(|e| divergence(format!("reorg forward inconsistent with flat state: {e}")))?;
        }
        apply_state_diff(&mut recon, diff).map_err(|e| divergence(format!("§12 diff inconsistent after re-base: {e}")))?;

        // Differential: every touched account must now equal the committed snapshot.
        for &a in &touched {
            let want = staged
                .snapshot
                .accounts
                .binary_search_by(|acc| acc.address.as_bytes().cmp(&a))
                .ok()
                .map(|i| &staged.snapshot.accounts[i]);
            if !recon_account_matches(recon.get(&a), want) {
                return Err(divergence(format!(
                    "re-based account 0x{} != committed snapshot",
                    a.iter().map(|b| format!("{b:02x}")).collect::<String>()
                )));
            }
        }
        // Persist only the touched accounts (O(touched)).
        for &a in &touched {
            let addr = EvmAddress::from_bytes(a);
            match recon.get(&a) {
                Some(acc) => {
                    let storage = acc.storage.iter().map(|(s, v)| (EvmU256::from_be_bytes(*s), EvmU256::from_be_bytes(*v))).collect();
                    flat.write_batch(batch, addr, FlatAccount { core: acc.core.clone(), storage }).map_err(ShadowError::Store)?;
                }
                None => flat.delete_batch(batch, addr).map_err(ShadowError::Store)?,
            }
        }
        advance(batch, latest_ptr)?;
        return Ok(ShadowOutcome::Rebased);
    }

    // (3) Bootstrap (no pointer) or a retention gap: reseed the flat store to the
    // committed post-state. The committed snapshot (206) is the source of truth;
    // the single diff from the current flat content to it deletes any stale rows.
    let flat_now = materialize_snapshot(flat, code).map_err(ShadowError::Store)?;
    let reseed = compute_state_diff(&flat_now, &staged.snapshot, current, parent);
    apply_diff_to_flat(flat, batch, &reseed).map_err(ShadowError::Store)?;
    advance(batch, latest_ptr)?;
    Ok(ShadowOutcome::Reseeded)
}

#[cfg(test)]
mod flat_state_tests {
    use super::{EvmStaged, ShadowError, ShadowOutcome, shadow_dual_write_flat};
    use crate::model::stores::evm::{DbEvmBlockStateRootStore, DbEvmCodeStore, DbEvmFlatAccountStore, DbEvmLatestStatePtrStore};
    use kaspa_consensus_core::BlockHash;
    use kaspa_consensus_core::evm::{
        EVM_EMPTY_CODE_HASH, EvmAccountSnapshot, EvmAddress, EvmExecutionResult, EvmStateDiffV2, EvmStateSnapshot, EvmU256,
        compute_state_diff,
    };
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{CachePolicy, ConnBuilder, StoreError};
    use kaspa_hashes::{EvmH256, Hash64};
    use rocksdb::WriteBatch;
    use std::collections::HashMap;

    /// Chain readers (S5) backed by in-memory maps; empty maps force the reseed
    /// fallback (the S4 paths never walk the chain).
    fn diff_reader(map: &HashMap<BlockHash, EvmStateDiffV2>) -> impl Fn(BlockHash) -> Result<Option<EvmStateDiffV2>, StoreError> + '_ {
        move |b| Ok(map.get(&b).cloned())
    }
    fn number_reader(map: &HashMap<BlockHash, u64>) -> impl Fn(BlockHash) -> Result<Option<u64>, StoreError> + '_ {
        move |b| Ok(map.get(&b).copied())
    }
    fn no_diffs() -> HashMap<BlockHash, EvmStateDiffV2> {
        HashMap::new()
    }
    fn no_numbers() -> HashMap<BlockHash, u64> {
        HashMap::new()
    }

    fn acc(a: u8, nonce: u64, bal: u64, ch: EvmH256, code: &[u8], storage: &[(u64, u64)]) -> EvmAccountSnapshot {
        let mut st: Vec<(EvmU256, EvmU256)> =
            storage.iter().map(|(s, v)| (EvmU256::from_u128(*s as u128), EvmU256::from_u128(*v as u128))).collect();
        st.sort_unstable_by(|x, y| x.0.to_be_bytes().cmp(&y.0.to_be_bytes()));
        EvmAccountSnapshot {
            address: EvmAddress::from_bytes([a; 20]),
            nonce,
            balance: EvmU256::from_u128(bal as u128),
            code_hash: ch,
            code: code.to_vec(),
            storage: st,
        }
    }
    fn snap(accs: Vec<EvmAccountSnapshot>) -> EvmStateSnapshot {
        let mut a = accs;
        a.sort_unstable_by(|x, y| x.address.as_bytes().cmp(&y.address.as_bytes()));
        EvmStateSnapshot { accounts: a }
    }

    /// C-01 S8 (audit M-01): `seed_flat_from_snapshot` (the pruned-IBD import path) makes the flat
    /// store + content-addressed code materialize back to EXACTLY the imported pruning-point
    /// snapshot, pins the latest pointer + block→root index to the pruning point, and clears any
    /// pre-existing stale flat row.
    #[test]
    fn s8_seed_flat_from_snapshot_round_trips_and_clears_stale() {
        use super::{materialize_snapshot, seed_flat_from_snapshot};
        use crate::model::stores::evm::EvmCodeStoreReader;
        use kaspa_consensus_core::evm::FlatAccount;

        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let flat = DbEvmFlatAccountStore::new(db.clone(), CachePolicy::Empty);
        let code = DbEvmCodeStore::new(db.clone(), CachePolicy::Empty);
        let roots = DbEvmBlockStateRootStore::new(db.clone(), CachePolicy::Empty);
        let mut ptr = DbEvmLatestStatePtrStore::new(db.clone());

        // A stale row (address NOT in the snapshot) that the defensive clear must remove.
        let stale_addr = EvmAddress::from_bytes([0x99; 20]);
        let mut pre = WriteBatch::default();
        flat.write_batch(&mut pre, stale_addr, FlatAccount::default()).unwrap();
        db.write(pre).unwrap();
        assert!(flat.get(stale_addr).unwrap().is_some());

        // EOA (empty code, no storage) + contract (code + storage).
        let contract_code: &[u8] = &[0x60, 0x80, 0x60, 0x40, 0x52];
        let code_hash = EvmH256::from_bytes([0xcd; 32]);
        let pp = Hash64::from_bytes([0x07; 64]);
        let state_root = EvmH256::from_bytes([0x55; 32]);
        let snapshot = snap(vec![
            acc(0x11, 7, 1_000, EVM_EMPTY_CODE_HASH, &[], &[]),
            acc(0x22, 1, 0, code_hash, contract_code, &[(3, 9), (1, 4)]),
        ]);

        let mut b = WriteBatch::default();
        seed_flat_from_snapshot(&flat, &code, &roots, &mut ptr, &mut b, pp, state_root, &snapshot).unwrap();
        db.write(b).unwrap();

        // Flat store + code store materialize back to EXACTLY the imported snapshot.
        assert_eq!(materialize_snapshot(&flat, &code).unwrap(), snapshot);
        // Stale row is gone.
        assert_eq!(flat.get(stale_addr).unwrap(), None);
        // Pointer + block→root index pinned to the pruning point.
        let p = ptr.get().unwrap().unwrap();
        assert_eq!(p.canonical_head, pp);
        assert_eq!(p.state_root, state_root);
        assert_eq!(roots.get(pp).unwrap(), Some(state_root));
        // Contract bytecode is content-addressed in the code store (222); EOA wrote none.
        assert_eq!(code.get(code_hash).unwrap().as_deref(), Some(contract_code));
        assert_eq!(code.get(EVM_EMPTY_CODE_HASH).unwrap(), None);
    }

    /// C-01 S9c: the production `StoreFlatReader` seam reproduces EXACTLY the eager
    /// `materialize_snapshot` output it would replace — for every account the reader returns the
    /// `FlatAccount` form of the snapshot, contract code resolves by hash, and an absent address /
    /// the empty code-hash read as `None`. Composed with the kaspa-evm flat_backend proof
    /// (`FlatStateBackend` reads == `seed_cachedb` for any reader returning this data), the lazy
    /// store-backed seed is byte-identical to the eager snapshot seed. The live executor cutover
    /// (generalizing `execute_block_evm` + an incremental-MPT root) is deferred to Stage 2.
    #[cfg(feature = "evm")]
    #[test]
    fn s9c_store_flat_reader_reproduces_materialized_snapshot() {
        use super::{StoreFlatReader, materialize_snapshot, seed_flat_from_snapshot};
        use kaspa_consensus_core::evm::FlatAccount;
        use kaspa_evm::flat_backend::FlatStateReader;

        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let flat = DbEvmFlatAccountStore::new(db.clone(), CachePolicy::Empty);
        let code = DbEvmCodeStore::new(db.clone(), CachePolicy::Empty);
        let roots = DbEvmBlockStateRootStore::new(db.clone(), CachePolicy::Empty);
        let mut ptr = DbEvmLatestStatePtrStore::new(db.clone());

        // EOA (no code) + a contract (code + storage), so the code resolution path is exercised.
        let contract_code: &[u8] = &[0x60, 0x80, 0x60, 0x40, 0x52];
        let code_hash = EvmH256::from_bytes([0xcd; 32]);
        let snapshot = snap(vec![
            acc(0x11, 7, 1_000, EVM_EMPTY_CODE_HASH, &[], &[]),
            acc(0x22, 1, 0, code_hash, contract_code, &[(1, 4), (3, 9)]),
        ]);
        let mut b = WriteBatch::default();
        seed_flat_from_snapshot(
            &flat,
            &code,
            &roots,
            &mut ptr,
            &mut b,
            Hash64::from_bytes([0x07; 64]),
            EvmH256::from_bytes([0x55; 32]),
            &snapshot,
        )
        .unwrap();
        db.write(b).unwrap();

        // The eager path materializes back to the snapshot (the reference seed source).
        let materialized = materialize_snapshot(&flat, &code).unwrap();
        assert_eq!(materialized, snapshot);

        // The production lazy reader returns, for each materialized account, exactly the FlatAccount
        // form of it and resolves contract code by hash — the same data `seed_cachedb` consumes.
        let reader = StoreFlatReader::new(&flat, &code);
        for a in &materialized.accounts {
            assert_eq!(
                reader.flat_account(a.address).unwrap().expect("present"),
                FlatAccount::from_snapshot(a),
                "reader account != materialized"
            );
            if a.code_hash != EVM_EMPTY_CODE_HASH {
                assert_eq!(reader.flat_code(a.code_hash).unwrap().as_deref(), Some(a.code.as_slice()), "reader code != snapshot code");
            }
        }
        // Absent address ⇒ None; the empty code-hash is never stored ⇒ None (the EOA-no-code path).
        assert_eq!(reader.flat_account(EvmAddress::from_bytes([0xAB; 20])).unwrap(), None);
        assert_eq!(reader.flat_code(EVM_EMPTY_CODE_HASH).unwrap(), None);
    }

    /// Applying the §12 diff chain to the FLAT store, then materializing, reproduces
    /// the canonical snapshot at each block (the persistent equivalence the writer relies on).
    #[test]
    fn flat_apply_then_materialize_round_trips_a_chain() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let flat = DbEvmFlatAccountStore::new(db.clone(), CachePolicy::Empty);
        let code = DbEvmCodeStore::new(db.clone(), CachePolicy::Empty);

        let ca = EvmH256::from_bytes([0xAA; 32]);
        let blob: &[u8] = &[0x60, 0x00, 0xfd];
        // seed the content-addressed code (the writer does this from diff_code_entries).
        let mut cb = WriteBatch::default();
        code.write_batch(&mut cb, ca, blob.to_vec()).unwrap();
        db.write(cb).unwrap();

        let chain = [
            EvmStateSnapshot::default(),
            snap(vec![acc(0x01, 1, 1000, EVM_EMPTY_CODE_HASH, &[], &[]), acc(0x02, 0, 500, EVM_EMPTY_CODE_HASH, &[], &[])]),
            snap(vec![acc(0x01, 2, 800, EVM_EMPTY_CODE_HASH, &[], &[]), acc(0x03, 1, 0, ca, blob, &[(1, 7), (2, 9)])]), // 0x02 self-destructed; contract deployed
            snap(vec![acc(0x01, 2, 800, EVM_EMPTY_CODE_HASH, &[], &[]), acc(0x03, 1, 0, ca, blob, &[(1, 7), (3, 4)])]), // slot 2 cleared, slot 3 set
        ];
        for i in 1..chain.len() {
            let diff = compute_state_diff(
                &chain[i - 1],
                &chain[i],
                Hash64::from_bytes([i as u8; 64]),
                Hash64::from_bytes([(i - 1) as u8; 64]),
            );
            let mut b = WriteBatch::default();
            super::apply_diff_to_flat(&flat, &mut b, &diff).unwrap();
            db.write(b).unwrap();
            let got = super::materialize_snapshot(&flat, &code).unwrap();
            assert_eq!(got, chain[i], "flat materialize at block {i} must equal the canonical snapshot");
        }
    }

    // ---- slice S4: shadow dual-write + live differential ----

    fn h(b: u8) -> BlockHash {
        BlockHash::from_bytes([b; 64])
    }

    /// A minimal [`EvmStaged`] carrying the three fields the dual-write reads:
    /// the committed `state_root` (recorded in 232/231; not re-derived here — the
    /// differential is over reconstructed state, not roots), the post-state
    /// snapshot, and the §12 diff.
    fn mk_staged(state_root: EvmH256, snapshot: EvmStateSnapshot, diff: Option<EvmStateDiffV2>) -> EvmStaged {
        let mut result = EvmExecutionResult::default();
        result.header.state_root = state_root;
        EvmStaged { result, snapshot, candidate_meta: vec![], trace_body: None, state_diff: diff }
    }

    struct Stores {
        // Fields drop in declaration order; the DbLifetime guard asserts zero
        // strong DB refs on drop, so it MUST be declared last (after every store
        // and the db handle that hold an `Arc<DB>`).
        db: std::sync::Arc<kaspa_database::prelude::DB>,
        flat: DbEvmFlatAccountStore,
        block_root: DbEvmBlockStateRootStore,
        ptr: DbEvmLatestStatePtrStore,
        code: DbEvmCodeStore,
        _lt: kaspa_database::utils::DbLifetime,
    }
    fn stores() -> Stores {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let flat = DbEvmFlatAccountStore::new(db.clone(), CachePolicy::Empty);
        let block_root = DbEvmBlockStateRootStore::new(db.clone(), CachePolicy::Empty);
        let ptr = DbEvmLatestStatePtrStore::new(db.clone());
        let code = DbEvmCodeStore::new(db.clone(), CachePolicy::Empty);
        Stores { db, flat, block_root, ptr, code, _lt }
    }

    /// Shadow dual-write over a synthetic chain: block 1 BOOTSTRAPS (no pointer →
    /// reseed), 2 and 3 cleanly EXTEND; the persisted flat store materializes to
    /// the canonical snapshot at every block, the pointer advances, and 232 holds
    /// each committed root.
    #[test]
    fn shadow_dual_write_maintains_flat_and_matches_committed() {
        let s = stores();
        let blob: &[u8] = &[0x60, 0x00, 0xfd];
        let ca = EvmH256::from_bytes([0xAA; 32]);
        // The §12 writer seeds the content-addressed code; mirror that for materialize.
        let mut cb = WriteBatch::default();
        s.code.write_batch(&mut cb, ca, blob.to_vec()).unwrap();
        s.db.write(cb).unwrap();

        let chain = [
            EvmStateSnapshot::default(), // 0 (genesis)
            snap(vec![acc(0x01, 1, 1000, EVM_EMPTY_CODE_HASH, &[], &[]), acc(0x02, 0, 500, EVM_EMPTY_CODE_HASH, &[], &[])]), // 1
            snap(vec![acc(0x01, 2, 800, EVM_EMPTY_CODE_HASH, &[], &[]), acc(0x03, 1, 0, ca, blob, &[(1, 7), (2, 9)])]), // 2 (0x02 gone, contract deployed)
            snap(vec![acc(0x01, 2, 800, EVM_EMPTY_CODE_HASH, &[], &[]), acc(0x03, 1, 0, ca, blob, &[(1, 7), (3, 4)])]), // 3 (slot 2→0, slot 3 set)
        ];
        let mut ptr = s.ptr;
        for n in 1..=3u8 {
            let i = n as usize;
            let diff = compute_state_diff(&chain[i - 1], &chain[i], h(n), h(n - 1));
            let staged = mk_staged(EvmH256::from_bytes([n; 32]), chain[i].clone(), Some(diff));
            let mut batch = WriteBatch::default();
            let (dr, nr) = (no_diffs(), no_numbers());
            let outcome = shadow_dual_write_flat(
                &s.flat,
                &s.block_root,
                &mut ptr,
                &s.code,
                &mut batch,
                h(n),
                &staged,
                diff_reader(&dr),
                number_reader(&nr),
            )
            .unwrap();
            s.db.write(batch).unwrap();

            assert_eq!(outcome, if n == 1 { ShadowOutcome::Reseeded } else { ShadowOutcome::Extended }, "block {n} outcome");
            assert_eq!(
                super::materialize_snapshot(&s.flat, &s.code).unwrap(),
                chain[i],
                "flat materializes to canonical at block {n}"
            );
            assert_eq!(ptr.get().unwrap().unwrap().canonical_head, h(n), "pointer advanced to block {n}");
            assert_eq!(s.block_root.get(h(n)).unwrap(), Some(EvmH256::from_bytes([n; 32])), "232 holds committed root for block {n}");
        }
    }

    /// A flat backend that disagrees with the committed snapshot HALTS: feeding a
    /// correct diff but a wrong post-state snapshot yields `Divergence`, and the
    /// flat store is left untouched (the check is pre-commit).
    #[test]
    fn shadow_dual_write_halts_on_divergence() {
        let s = stores();
        let b1 = snap(vec![acc(0x01, 1, 1000, EVM_EMPTY_CODE_HASH, &[], &[])]);
        let b2 = snap(vec![acc(0x01, 2, 900, EVM_EMPTY_CODE_HASH, &[], &[])]);
        let b3 = snap(vec![acc(0x01, 3, 700, EVM_EMPTY_CODE_HASH, &[], &[])]);
        let mut ptr = s.ptr;

        // Bootstrap to b1.
        let d1 = compute_state_diff(&EvmStateSnapshot::default(), &b1, h(1), h(0));
        let mut batch = WriteBatch::default();
        let (dr, nr) = (no_diffs(), no_numbers());
        shadow_dual_write_flat(
            &s.flat,
            &s.block_root,
            &mut ptr,
            &s.code,
            &mut batch,
            h(1),
            &mk_staged(EvmH256::from_bytes([1; 32]), b1.clone(), Some(d1)),
            diff_reader(&dr),
            number_reader(&nr),
        )
        .unwrap();
        s.db.write(batch).unwrap();

        // Clean-extend block 2 with the CORRECT diff (b1→b2) but a WRONG committed
        // snapshot (b3). got = apply(diff, flat@b1) = b2 ≠ expected = b3 → Divergence.
        let d2 = compute_state_diff(&b1, &b2, h(2), h(1));
        let staged_bad = mk_staged(EvmH256::from_bytes([2; 32]), b3.clone(), Some(d2));
        let mut batch = WriteBatch::default();
        let res = shadow_dual_write_flat(
            &s.flat,
            &s.block_root,
            &mut ptr,
            &s.code,
            &mut batch,
            h(2),
            &staged_bad,
            diff_reader(&dr),
            number_reader(&nr),
        );
        assert!(matches!(res, Err(ShadowError::Divergence { .. })), "wrong committed snapshot must diverge, got {res:?}");
        // Pre-commit halt: the flat store is still at b1.
        assert_eq!(super::materialize_snapshot(&s.flat, &s.code).unwrap(), b1, "flat store untouched on divergence");
        assert_eq!(ptr.get().unwrap().unwrap().canonical_head, h(1), "pointer not advanced on divergence");
    }

    /// `head` history mode drops the §12 diff, so there is no flat state to
    /// maintain — the dual-write is a no-op (no flat rows, no pointer).
    #[test]
    fn shadow_dual_write_skips_without_a_diff() {
        let s = stores();
        let b1 = snap(vec![acc(0x01, 1, 1000, EVM_EMPTY_CODE_HASH, &[], &[])]);
        let mut ptr = s.ptr;
        let staged = mk_staged(EvmH256::from_bytes([1; 32]), b1, None);
        let mut batch = WriteBatch::default();
        let (dr, nr) = (no_diffs(), no_numbers());
        let outcome = shadow_dual_write_flat(
            &s.flat,
            &s.block_root,
            &mut ptr,
            &s.code,
            &mut batch,
            h(1),
            &staged,
            diff_reader(&dr),
            number_reader(&nr),
        )
        .unwrap();
        s.db.write(batch).unwrap();
        assert_eq!(outcome, ShadowOutcome::SkippedNoDiff);
        assert_eq!(s.flat.iter().count(), 0, "no flat rows written in head mode");
        assert!(ptr.get().unwrap().is_none(), "pointer not set in head mode");
    }

    /// Slice S5: a sibling reorg. The flat store advances along branch A
    /// (A1→A2a→A3a), then a block on branch B (parent A1) triggers an INCREMENTAL
    /// re-base — revert A3a,A2a back to the common ancestor A1, then apply A2b —
    /// landing the flat store exactly on branch B's state. A follow-on B block
    /// then cleanly extends.
    #[test]
    fn shadow_dual_write_rebases_across_a_sibling_reorg() {
        let s = stores();
        // Common ancestor A1, branch A (A2a,A3a), branch B (A2b,A3b). Branch B
        // touches 0x02 (untouched by branch A's revert path) and creates 0x03, so
        // the touched-set persist must cover both branches' changes.
        let s_a1 = snap(vec![acc(0x01, 1, 1000, EVM_EMPTY_CODE_HASH, &[], &[]), acc(0x02, 0, 500, EVM_EMPTY_CODE_HASH, &[], &[])]);
        let s_a2a = snap(vec![acc(0x01, 2, 900, EVM_EMPTY_CODE_HASH, &[], &[]), acc(0x02, 0, 500, EVM_EMPTY_CODE_HASH, &[], &[])]);
        let s_a3a = snap(vec![acc(0x01, 3, 800, EVM_EMPTY_CODE_HASH, &[], &[]), acc(0x02, 0, 500, EVM_EMPTY_CODE_HASH, &[], &[])]);
        let s_a2b = snap(vec![
            acc(0x01, 2, 600, EVM_EMPTY_CODE_HASH, &[], &[]),
            acc(0x02, 0, 400, EVM_EMPTY_CODE_HASH, &[], &[]),
            acc(0x03, 1, 300, EVM_EMPTY_CODE_HASH, &[], &[]),
        ]);
        let s_a3b = snap(vec![
            acc(0x01, 2, 600, EVM_EMPTY_CODE_HASH, &[], &[]),
            acc(0x02, 0, 300, EVM_EMPTY_CODE_HASH, &[], &[]),
            acc(0x03, 2, 100, EVM_EMPTY_CODE_HASH, &[], &[]),
        ]);
        let (a0, a1, a2a, a3a, a2b, a3b) = (h(0x00), h(0x10), h(0x21), h(0x31), h(0x22), h(0x32));
        let empty = EvmStateSnapshot::default();
        let d_a1 = compute_state_diff(&empty, &s_a1, a1, a0);
        let d_a2a = compute_state_diff(&s_a1, &s_a2a, a2a, a1);
        let d_a3a = compute_state_diff(&s_a2a, &s_a3a, a3a, a2a);
        let d_a2b = compute_state_diff(&s_a1, &s_a2b, a2b, a1);
        let d_a3b = compute_state_diff(&s_a2b, &s_a3b, a3b, a2b);

        // Chain readers (the persistent §12 diff store + EVM-header evm_numbers).
        let diffs: HashMap<BlockHash, EvmStateDiffV2> =
            [(a1, d_a1.clone()), (a2a, d_a2a.clone()), (a3a, d_a3a.clone()), (a2b, d_a2b.clone()), (a3b, d_a3b.clone())]
                .into_iter()
                .collect();
        let numbers: HashMap<BlockHash, u64> = [(a1, 1u64), (a2a, 2), (a3a, 3), (a2b, 2), (a3b, 3)].into_iter().collect();

        let mut ptr = s.ptr;
        let commit = |ptr: &mut DbEvmLatestStatePtrStore, blk: BlockHash, snapshot: &EvmStateSnapshot, diff: EvmStateDiffV2| {
            let staged = mk_staged(EvmH256::from_bytes([blk.as_bytes()[0]; 32]), snapshot.clone(), Some(diff));
            let mut batch = WriteBatch::default();
            let outcome = shadow_dual_write_flat(
                &s.flat,
                &s.block_root,
                ptr,
                &s.code,
                &mut batch,
                blk,
                &staged,
                diff_reader(&diffs),
                number_reader(&numbers),
            )
            .unwrap();
            s.db.write(batch).unwrap();
            outcome
        };

        // Advance along branch A.
        assert_eq!(commit(&mut ptr, a1, &s_a1, d_a1.clone()), ShadowOutcome::Reseeded);
        assert_eq!(commit(&mut ptr, a2a, &s_a2a, d_a2a), ShadowOutcome::Extended);
        assert_eq!(commit(&mut ptr, a3a, &s_a3a, d_a3a), ShadowOutcome::Extended);
        assert_eq!(super::materialize_snapshot(&s.flat, &s.code).unwrap(), s_a3a, "flat at branch-A tip");

        // Reorg to branch B: A2b's parent is A1, but the flat store is at A3a.
        assert_eq!(commit(&mut ptr, a2b, &s_a2b, d_a2b), ShadowOutcome::Rebased, "sibling reorg re-bases");
        assert_eq!(super::materialize_snapshot(&s.flat, &s.code).unwrap(), s_a2b, "flat re-based to branch B");
        assert_eq!(ptr.get().unwrap().unwrap().canonical_head, a2b);

        // Branch B then cleanly extends.
        assert_eq!(commit(&mut ptr, a3b, &s_a3b, d_a3b), ShadowOutcome::Extended);
        assert_eq!(super::materialize_snapshot(&s.flat, &s.code).unwrap(), s_a3b, "flat at branch-B tip");
    }
}

#[cfg(test)]
mod gather_tests {
    use super::*;
    use kaspa_consensus_core::BlockHash;
    use kaspa_consensus_core::evm::{EVM_EMPTY_CODE_HASH, EvmStateCheckpointV1, EvmStateDiffV2, EvmStateSnapshot, compute_state_diff};
    use std::collections::HashMap;
    use std::convert::Infallible;

    fn h(b: u8) -> BlockHash {
        BlockHash::from_bytes([b; 64])
    }

    /// Build a tiny EOA-only snapshot for block `n` (balance encodes the block).
    fn snap_at(n: u8) -> EvmStateSnapshot {
        use kaspa_consensus_core::evm::{EvmAccountSnapshot, EvmAddress, EvmU256};
        EvmStateSnapshot {
            accounts: vec![EvmAccountSnapshot {
                address: EvmAddress::from_bytes([0x01; 20]),
                nonce: n as u64,
                balance: EvmU256::from_u128(1000 - n as u128),
                code_hash: EVM_EMPTY_CODE_HASH,
                code: vec![],
                storage: vec![],
            }],
        }
    }

    /// A 3-block chain blocks 1,2,3 (genesis = block 0 = empty). Diffs keyed by block,
    /// each diff.parent points to the previous block; block 0 has no header.
    struct Chain {
        diffs: HashMap<BlockHash, EvmStateDiffV2>,
        checkpoints: HashMap<BlockHash, EvmStateCheckpointV1>,
        snaps: Vec<EvmStateSnapshot>, // index = block number
    }

    fn chain() -> Chain {
        let snaps = vec![EvmStateSnapshot::default(), snap_at(1), snap_at(2), snap_at(3)];
        let mut diffs = HashMap::new();
        for n in 1..=3u8 {
            diffs.insert(h(n), compute_state_diff(&snaps[(n - 1) as usize], &snaps[n as usize], h(n), h(n - 1)));
        }
        Chain { diffs, checkpoints: HashMap::new(), snaps }
    }

    fn gather(c: &Chain, block: BlockHash) -> Result<(EvmStateSnapshot, Vec<EvmStateDiffV2>), ReconstructGatherError> {
        gather_reconstruction_inputs::<Infallible>(
            block,
            |b| Ok(c.checkpoints.get(&b).cloned()),
            |b| Ok(c.diffs.get(&b).cloned()),
            // Blocks 1..3 have headers; block 0 (genesis) does not.
            |b| (1..=3u8).any(|n| h(n) == b),
        )
    }

    /// With no checkpoints, the walk reaches genesis and returns the empty seed
    /// plus all diffs 1..target in forward order.
    #[test]
    fn walks_to_genesis_collecting_all_diffs() {
        let c = chain();
        let (seed, diffs) = gather(&c, h(3)).unwrap();
        assert!(seed.is_empty(), "no checkpoint ⇒ genesis seed");
        assert_eq!(diffs.len(), 3);
        assert_eq!(diffs[0], c.diffs[&h(1)]);
        assert_eq!(diffs[1], c.diffs[&h(2)]);
        assert_eq!(diffs[2], c.diffs[&h(3)]);
    }

    /// A checkpoint at block 2 anchors the walk: seed = state@2, diffs = [diff@3].
    #[test]
    fn anchors_on_checkpoint() {
        let mut c = chain();
        c.checkpoints.insert(h(2), EvmStateCheckpointV1::build(h(2), 2, kaspa_hashes::EvmH256::from_bytes([0; 32]), &c.snaps[2]));
        let (seed, diffs) = gather(&c, h(3)).unwrap();
        assert_eq!(seed, c.snaps[2], "seed = checkpoint's full state at block 2");
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0], c.diffs[&h(3)]);
    }

    /// A checkpoint AT the target returns it directly (no diffs to apply).
    #[test]
    fn checkpoint_at_target_needs_no_diffs() {
        let mut c = chain();
        c.checkpoints.insert(h(3), EvmStateCheckpointV1::build(h(3), 3, kaspa_hashes::EvmH256::from_bytes([0; 32]), &c.snaps[3]));
        let (seed, diffs) = gather(&c, h(3)).unwrap();
        assert_eq!(seed, c.snaps[3]);
        assert!(diffs.is_empty());
    }

    /// A missing diff mid-walk (GC'd) fails closed as Unavailable — never a partial state.
    #[test]
    fn missing_diff_fails_closed() {
        let mut c = chain();
        c.diffs.remove(&h(2)); // GC block 2's diff, no checkpoint to anchor on
        let err = gather(&c, h(3)).unwrap_err();
        assert!(matches!(err, ReconstructGatherError::Unavailable(_)));
    }
}

#[cfg(feature = "evm")]
mod driver {
    use crate::model::stores::evm::{
        DbEvmHeaderStore, DbEvmPayloadStore, DbEvmStateStore, EvmHeaderStore, EvmHeaderStoreReader, EvmPayloadStoreReader,
        EvmStateStore, EvmStateStoreReader,
    };
    use kaspa_consensus_core::BlockHash;
    use kaspa_consensus_core::evm::{EvmExecutionPayload, EvmReplayEnv, EvmReplayTx, EvmStateSnapshot, EvmTraceReplayBodyV1};
    use kaspa_consensus_core::header::Header;
    use kaspa_database::prelude::StoreError;
    use kaspa_evm::AcceptedTxCandidate;
    use rocksdb::WriteBatch;

    /// Outcome of validating + persisting a block's EVM lane.
    #[derive(Debug)]
    pub enum EvmValidateError {
        /// The producer's `evm_commitment_root` does not match the re-executed
        /// result — the one EVM condition that makes a block invalid (design §6.3).
        CommitmentMismatch { block: BlockHash },
        /// The executor rejected the block body (e.g. an undecodable payload tx).
        Exec(String),
        /// A store read/write failed.
        Store(StoreError),
    }

    /// The lazy chain-context EVM step (design v0.4 §2.3/§3): execute a
    /// selected-chain block's **mergeset acceptance** against its
    /// `selected_parent`'s committed state, verify the `evm_commitment_root`,
    /// and hand back the resulting header + child state snapshot for the caller
    /// to commit atomically with the block's UTXO diff.
    ///
    /// `AcceptedEvmTxs(B)` (§3.1) is assembled here: `sorted_mergeset` is B's
    /// consensus mergeset in canonical order (it never contains B itself — the
    /// off-by-one rule: B's own payload is accepted by B's selected child); each
    /// mergeset block's payload is read from the payload store (absent ⇒ empty —
    /// only non-empty payloads are persisted), and its txs join the candidate
    /// list paired with that PAYLOAD block's declared coinbase (§8.1 fee
    /// routing). B's own `payload` contributes only `system_ops` + the accepting
    /// coinbase.
    ///
    /// No-replay (design §2.2/§10): if this block's EVM result is already stored,
    /// returns `Ok(())` without re-executing — a virtual reorg only moves head
    /// pointers, it never recomputes a block's EVM state. The genesis EVM state
    /// (`EVM_GENESIS_STATE_ROOT`, empty snapshot) is the implicit parent of the
    /// first EVM block.
    /// Validation half of the step: computes + verifies and RETURNS the rows to
    /// stage; `None` = already stored (no-replay). The caller decides the batch.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub fn evm_validate(
        header_store: &DbEvmHeaderStore,
        state_store: &DbEvmStateStore,
        payload_store: &DbEvmPayloadStore,
        block: BlockHash,
        selected_parent: BlockHash,
        sorted_mergeset: &[BlockHash],
        l1_header: &Header,
        payload: &EvmExecutionPayload,
        gas_pool_v2_activation_daa_score: u64,
        f002_withdraw_cap_activation_daa_score: u64,
        f003_mldsa_verify_activation_daa_score: u64,
        typed_receipt_root_activation_daa_score: u64,
    ) -> Result<Option<super::EvmStaged>, EvmValidateError> {
        // No-replay: this block's EVM result was computed when it first joined the
        // selected chain; never recompute it.
        if header_store.has(block).map_err(EvmValidateError::Store)? {
            return Ok(None);
        }
        debug_assert!(!sorted_mergeset.contains(&block), "a block is never in its own mergeset (off-by-one, §3.1)");

        let (result, child_snapshot, candidate_meta, trace_body, parent_snapshot) = evm_execute_acceptance(
            header_store,
            state_store,
            payload_store,
            selected_parent,
            sorted_mergeset,
            l1_header,
            payload,
            gas_pool_v2_activation_daa_score,
            f002_withdraw_cap_activation_daa_score,
            f003_mldsa_verify_activation_daa_score,
            typed_receipt_root_activation_daa_score,
        )?;

        // The only block-invalidating EVM condition: producer commitment mismatch
        // (user tx failures are status-0 receipts inside `result`, design §6.2).
        if result.header.commitment_root() != l1_header.evm_commitment_root {
            return Err(EvmValidateError::CommitmentMismatch { block });
        }

        // §12: this block's forward state diff over its selected parent (prefix 220).
        let state_diff =
            Some(kaspa_consensus_core::evm::compute_state_diff(&parent_snapshot, &child_snapshot, block, selected_parent));

        Ok(Some(super::EvmStaged { result, snapshot: child_snapshot, candidate_meta, trace_body, state_diff }))
    }

    /// O12 (IBD pipeline): [`evm_validate`] with an in-memory parent override —
    /// the worker-side step for one block of a pipelined run. Identical logic
    /// (no-replay check, execution, commitment compare); only the SOURCE of the
    /// parent rows differs.
    #[allow(clippy::too_many_arguments)]
    pub fn evm_validate_chained(
        header_store: &DbEvmHeaderStore,
        state_store: &DbEvmStateStore,
        payload_store: &DbEvmPayloadStore,
        block: BlockHash,
        selected_parent: BlockHash,
        sorted_mergeset: &[BlockHash],
        l1_header: &Header,
        payload: &EvmExecutionPayload,
        parent_override: Option<(kaspa_consensus_core::evm::EvmExecutionHeader, EvmStateSnapshot)>,
        gas_pool_v2_activation_daa_score: u64,
        f002_withdraw_cap_activation_daa_score: u64,
        f003_mldsa_verify_activation_daa_score: u64,
        typed_receipt_root_activation_daa_score: u64,
    ) -> Result<Option<super::EvmStaged>, EvmValidateError> {
        if header_store.has(block).map_err(EvmValidateError::Store)? {
            return Ok(None);
        }
        let (result, child_snapshot, candidate_meta, trace_body, parent_snapshot) = evm_execute_acceptance_with_parent(
            header_store,
            state_store,
            payload_store,
            selected_parent,
            sorted_mergeset,
            l1_header,
            payload,
            parent_override,
            gas_pool_v2_activation_daa_score,
            f002_withdraw_cap_activation_daa_score,
            f003_mldsa_verify_activation_daa_score,
            typed_receipt_root_activation_daa_score,
        )?;
        if result.header.commitment_root() != l1_header.evm_commitment_root {
            return Err(EvmValidateError::CommitmentMismatch { block });
        }
        // §12: forward state diff over the selected parent (prefix 220), same as `evm_validate`.
        let state_diff =
            Some(kaspa_consensus_core::evm::compute_state_diff(&parent_snapshot, &child_snapshot, block, selected_parent));
        Ok(Some(super::EvmStaged { result, snapshot: child_snapshot, candidate_meta, trace_body, state_diff }))
    }

    /// The shared execution core: run one block's mergeset acceptance from the
    /// stores. Used by the verifier ([`evm_validate`]) AND by the template
    /// builder (§15 — the producer computes the commitment it will declare,
    /// with the exact code the verifier later re-runs, so a mined block
    /// reproduces the commitment byte-for-byte). `l1_header` supplies only the
    /// env inputs (timestamp / blue_work / daa_score) — its EVM fields are not
    /// read here.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub fn evm_execute_acceptance(
        header_store: &DbEvmHeaderStore,
        state_store: &DbEvmStateStore,
        payload_store: &DbEvmPayloadStore,
        selected_parent: BlockHash,
        sorted_mergeset: &[BlockHash],
        l1_header: &Header,
        payload: &EvmExecutionPayload,
        gas_pool_v2_activation_daa_score: u64,
        f002_withdraw_cap_activation_daa_score: u64,
        f003_mldsa_verify_activation_daa_score: u64,
        typed_receipt_root_activation_daa_score: u64,
    ) -> Result<
        (
            kaspa_consensus_core::evm::EvmExecutionResult,
            EvmStateSnapshot,
            Vec<(kaspa_hashes::EvmH256, BlockHash)>,
            Option<EvmTraceReplayBodyV1>,
            // §12: the SELECTED-PARENT snapshot, returned (a free move — it is read
            // by ref to seed execution and would otherwise be dropped). The validate
            // path diffs `(parent, child)` from it; the template path ignores it.
            EvmStateSnapshot,
        ),
        EvmValidateError,
    > {
        evm_execute_acceptance_with_parent(
            header_store,
            state_store,
            payload_store,
            selected_parent,
            sorted_mergeset,
            l1_header,
            payload,
            None,
            gas_pool_v2_activation_daa_score,
            f002_withdraw_cap_activation_daa_score,
            f003_mldsa_verify_activation_daa_score,
            typed_receipt_root_activation_daa_score,
        )
    }

    /// [`evm_execute_acceptance`] with an optional IN-MEMORY parent override
    /// (O12 IBD pipeline): when a worker validates a run of consecutive chain
    /// blocks ahead of the committing thread, block N+1's parent rows are N's
    /// just-computed result — not yet in the store. The override supplies them
    /// directly; `None` keeps the store-read path (identical inputs either way,
    /// so the result is identical — the pipeline only changes WHERE the parent
    /// bytes come from, never their value).
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub fn evm_execute_acceptance_with_parent(
        header_store: &DbEvmHeaderStore,
        state_store: &DbEvmStateStore,
        payload_store: &DbEvmPayloadStore,
        selected_parent: BlockHash,
        sorted_mergeset: &[BlockHash],
        l1_header: &Header,
        payload: &EvmExecutionPayload,
        parent_override: Option<(kaspa_consensus_core::evm::EvmExecutionHeader, EvmStateSnapshot)>,
        gas_pool_v2_activation_daa_score: u64,
        f002_withdraw_cap_activation_daa_score: u64,
        f003_mldsa_verify_activation_daa_score: u64,
        typed_receipt_root_activation_daa_score: u64,
    ) -> Result<
        (
            kaspa_consensus_core::evm::EvmExecutionResult,
            EvmStateSnapshot,
            Vec<(kaspa_hashes::EvmH256, BlockHash)>,
            Option<EvmTraceReplayBodyV1>,
            // §12: the selected-parent snapshot (free move — see `evm_execute_acceptance`).
            EvmStateSnapshot,
        ),
        EvmValidateError,
    > {
        // AcceptedEvmTxs(B): the mergeset's payload txs in canonical order
        // (sorted_mergeset, then payload order — design §3.1). The class-5
        // prefix-take and class-2/3 skips are applied inside the executor.
        // `candidate_meta` records (tx hash, source payload block) per candidate
        // for the §16 indexes — parallel to the executor's candidate_outcomes.
        let mut accepted_txs: Vec<AcceptedTxCandidate> = Vec::new();
        let mut candidate_meta: Vec<(kaspa_hashes::EvmH256, BlockHash)> = Vec::new();
        for merged in sorted_mergeset {
            let merged_payload = match payload_store.get(*merged) {
                Ok(p) => p,
                Err(StoreError::KeyNotFound(_)) => continue, // empty payloads are not persisted
                Err(e) => return Err(EvmValidateError::Store(e)),
            };
            let payload_coinbase = merged_payload.evm_coinbase;
            for raw in merged_payload.transactions {
                candidate_meta.push((kaspa_evm::tx::tx_hash(&raw), *merged));
                accepted_txs.push(AcceptedTxCandidate { raw, payload_coinbase });
            }
        }

        // Selected-parent EVM header + state. An EVM-active parent ALWAYS persists
        // both rows together (every v2 chain block forms an EVM block; see the
        // commit batch in the virtual processor). So:
        //   - parent has an EVM header  ⇒ its state snapshot MUST be present;
        //     a missing snapshot is store corruption / a pruning or migration bug,
        //     NOT an implicit genesis. Fail closed (audit #4) — a producer must
        //     not build, nor a verifier accept, on a fabricated empty parent state.
        //   - parent has NO EVM header  ⇒ it is pre-activation: the first EVM
        //     block's implicit genesis parent ⇒ the empty default state is correct.
        // O12: a pipelined run supplies the parent IN MEMORY (the predecessor's
        // just-computed result) instead of the store rows.
        let (parent_header, parent_snapshot) = match parent_override {
            Some((h, s)) => (Some(h), s),
            None => {
                let parent_header = match header_store.get(selected_parent) {
                    Ok(h) => Some(h),
                    Err(StoreError::KeyNotFound(_)) => None,
                    Err(e) => return Err(EvmValidateError::Store(e)),
                };
                let parent_snapshot = if parent_header.is_some() {
                    match state_store.get(selected_parent) {
                        Ok(s) => s,
                        Err(StoreError::KeyNotFound(_)) => {
                            return Err(EvmValidateError::Exec(format!(
                                "EVM-active selected parent {selected_parent} has an EVM header but no persisted state snapshot (store corruption / pruning bug)"
                            )));
                        }
                        Err(e) => return Err(EvmValidateError::Store(e)),
                    }
                } else {
                    EvmStateSnapshot::default()
                };
                (parent_header, parent_snapshot)
            }
        };

        let input = super::EvmBlockInput {
            parent: parent_header.as_ref(),
            header_timestamp_ms: l1_header.timestamp,
            selected_parent_hash: selected_parent.as_bytes(),
            blue_work_be: l1_header.blue_work.to_be_bytes().to_vec(),
            daa_score: l1_header.daa_score,
            payload,
            accepted_txs: &accepted_txs,
            gas_pool_v2_activation_daa_score,
            f002_withdraw_cap_activation_daa_score,
            f003_mldsa_verify_activation_daa_score,
            typed_receipt_root_activation_daa_score,
        };

        let (result, snapshot) = kaspa_evm::snapshot::execute_block_from_snapshot(&parent_snapshot, &input)
            .map_err(|e| EvmValidateError::Exec(e.to_string()))?;

        // §11: assemble the `debug_traceTransaction` replay plan from the exact
        // acceptance this block performed — the env inputs, B's own system ops, and
        // the FULL ordered candidate list (accepted + skipped) with each candidate's
        // recorded outcome. A replay feeds the identical candidate list to the same
        // executor, so it reproduces the same accept/skip/gas decisions and the same
        // pre-state for any traced tx. RPC/replay data only; it MUST NOT influence
        // acceptance, so it is built defensively (never an error) and is `None` when
        // there is nothing to trace (no candidate txs). The three parallel vectors
        // (`accepted_txs` / `candidate_meta` / `result.candidate_outcomes`) are
        // produced in lockstep by the executor (asserted again at staging).
        let trace_body = if accepted_txs.is_empty() {
            None
        } else {
            debug_assert_eq!(accepted_txs.len(), candidate_meta.len());
            debug_assert_eq!(accepted_txs.len(), result.candidate_outcomes.len());
            let txs = accepted_txs
                .iter()
                .zip(candidate_meta.iter())
                .zip(result.candidate_outcomes.iter())
                .map(|((cand, (tx_hash, src)), outcome)| EvmReplayTx {
                    tx_hash: *tx_hash,
                    raw: cand.raw.clone(),
                    payload_coinbase: cand.payload_coinbase,
                    originating_payload_block: *src,
                    outcome: *outcome,
                })
                .collect();
            Some(EvmTraceReplayBodyV1 {
                selected_parent,
                env: EvmReplayEnv {
                    header_timestamp_ms: l1_header.timestamp,
                    blue_work_be: l1_header.blue_work.to_be_bytes().to_vec(),
                    daa_score: l1_header.daa_score,
                    coinbase: payload.evm_coinbase,
                },
                system_ops: payload.system_ops.clone(),
                txs,
            })
        };

        Ok((result, snapshot, candidate_meta, trace_body, parent_snapshot))
    }

    /// Validate + stage into `batch` in one call (the unit-test surface; the
    /// virtual processor calls [`evm_validate`] and stages inside its own
    /// `commit_utxo_state` batch instead).
    #[allow(clippy::too_many_arguments)]
    pub fn evm_validate_and_persist(
        header_store: &DbEvmHeaderStore,
        state_store: &DbEvmStateStore,
        payload_store: &DbEvmPayloadStore,
        receipts_store: &crate::model::stores::evm::DbEvmReceiptsStore,
        tx_index_store: &crate::model::stores::evm::DbEvmTxIndexStore,
        log_index_store: &crate::model::stores::evm::DbEvmLogIndexStore,
        trace_store: &crate::model::stores::evm::DbEvmTraceReplayStore,
        diff_store: &crate::model::stores::evm::DbEvmStateDiffStore,
        code_store: &crate::model::stores::evm::DbEvmCodeStore,
        checkpoint_store: &crate::model::stores::evm::DbEvmStateCheckpointStore,
        batch: &mut WriteBatch,
        block: BlockHash,
        selected_parent: BlockHash,
        sorted_mergeset: &[BlockHash],
        l1_header: &Header,
        payload: &EvmExecutionPayload,
        gas_pool_v2_activation_daa_score: u64,
        f002_withdraw_cap_activation_daa_score: u64,
        f003_mldsa_verify_activation_daa_score: u64,
        typed_receipt_root_activation_daa_score: u64,
    ) -> Result<(), EvmValidateError> {
        let Some(staged) = evm_validate(
            header_store,
            state_store,
            payload_store,
            block,
            selected_parent,
            sorted_mergeset,
            l1_header,
            payload,
            gas_pool_v2_activation_daa_score,
            f002_withdraw_cap_activation_daa_score,
            f003_mldsa_verify_activation_daa_score,
            typed_receipt_root_activation_daa_score,
        )?
        else {
            return Ok(());
        };
        header_store.insert_batch(batch, block, staged.result.header.clone()).map_err(EvmValidateError::Store)?;
        super::stage_evm_index_rows(
            receipts_store,
            tx_index_store,
            log_index_store,
            trace_store,
            diff_store,
            code_store,
            checkpoint_store,
            batch,
            block,
            &staged,
        )
        .map_err(EvmValidateError::Store)?;
        state_store.insert_batch(batch, block, staged.snapshot).map_err(EvmValidateError::Store)?;
        Ok(())
    }
}

#[cfg(feature = "evm")]
pub use driver::{
    EvmValidateError, evm_execute_acceptance, evm_execute_acceptance_with_parent, evm_validate, evm_validate_and_persist,
    evm_validate_chained,
};

// ---------------------------------------------------------------------------
// C-01 state-backend (design v0.1, Stage 1, slice S6) — the executor's PARENT
// SEED, sourced from the flat state backend instead of the per-block 206
// snapshot. This is the source the cutover (slice S9) switches the executor to;
// until then it runs only as the per-block shadow check
// (`VirtualStateProcessor::shadow_validate_parent_seed`) that asserts it is
// byte-identical to the still-authoritative 206 source. `cfg(feature="evm")`
// because the non-head path keccak-MPT-verifies the reconstruction (kaspa-evm).
// ---------------------------------------------------------------------------

/// Which source produced a parent seed (for the shadow log / outcome).
#[cfg(feature = "evm")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentSeedSource {
    /// The parent precedes EVM activation — the empty genesis state.
    PreActivation,
    /// The parent IS the flat store's canonical head — materialized from prefix 234 (+222).
    FlatHead,
    /// A non-head parent (post-reorg first block / bootstrap) — §12-reconstructed and root-verified.
    Reconstructed,
}

/// Failure obtaining a parent seed from the flat backend (slice S6). Two classes,
/// with deliberately different caller actions (the shadow check HALTS on a real
/// divergence but only SKIPS when it simply cannot read the data to compare):
#[cfg(feature = "evm")]
#[derive(Debug)]
pub enum ParentSeedError {
    /// A real backend/data fault — the §12 reconstruction's keccak-MPT root did not
    /// verify, a diff's `before` view was inconsistent, a checkpoint failed to
    /// decode, or the flat store referenced code absent from the content store
    /// (`StoreError::DataInconsistency`). The caller HALTS.
    Corrupt(String),
    /// The seed cannot be validated HERE — a transient store read failed
    /// (`DbError`/decode) or a non-head parent's §12 history is GC'd past
    /// `--evm-history-mode` retention. NOT a divergence: the caller SKIPS (and
    /// warns); the authoritative 206 seed is unaffected.
    Unavailable(String),
}

#[cfg(feature = "evm")]
impl std::fmt::Display for ParentSeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParentSeedError::Corrupt(m) => write!(f, "parent-seed reconstruction corrupt: {m}"),
            ParentSeedError::Unavailable(m) => write!(f, "parent-seed unavailable: {m}"),
        }
    }
}

/// Classify a store read failure for the parent-seed path: a `DataInconsistency`
/// is real corruption (the seed would be wrong ⇒ HALT); any other store error
/// (RocksDB I/O, decode) is transient ⇒ the seed cannot be validated here ⇒ SKIP.
#[cfg(feature = "evm")]
fn classify_seed_store_error(e: kaspa_database::prelude::StoreError) -> ParentSeedError {
    match e {
        kaspa_database::prelude::StoreError::DataInconsistency(m) => {
            ParentSeedError::Corrupt(format!("store data inconsistency: {m}"))
        }
        other => ParentSeedError::Unavailable(format!("store read: {other}")),
    }
}

/// C-01 Stage 1 (slice S6) — obtain `selected_parent`'s full EVM state snapshot
/// from the flat state backend: materialize the flat store (prefix 234 + 222)
/// when `selected_parent` IS the flat store's canonical head (`flat_head`), else
/// §12-reconstruct it (root-verified, the same path RPC `reconstruct_evm_state_at`
/// uses) for a non-head parent. A pre-activation parent (no EVM header) is the
/// empty genesis state. This MUST reproduce the per-block 206 snapshot the
/// executor reads today — the shadow check halts the node on any divergence.
#[cfg(feature = "evm")]
#[allow(clippy::too_many_arguments)]
pub fn flat_or_reconstruct_parent_snapshot(
    selected_parent: kaspa_consensus_core::BlockHash,
    flat_head: Option<kaspa_consensus_core::BlockHash>,
    flat: &crate::model::stores::evm::DbEvmFlatAccountStore,
    code: &crate::model::stores::evm::DbEvmCodeStore,
    header_store: &crate::model::stores::evm::DbEvmHeaderStore,
    checkpoint_store: &crate::model::stores::evm::DbEvmStateCheckpointStore,
    diff_store: &crate::model::stores::evm::DbEvmStateDiffStore,
) -> Result<(kaspa_consensus_core::evm::EvmStateSnapshot, ParentSeedSource), ParentSeedError> {
    use crate::model::stores::evm::{
        EvmCodeStoreReader, EvmHeaderStoreReader, EvmStateCheckpointStoreReader, EvmStateDiffStoreReader,
    };
    use kaspa_consensus_core::evm::EvmStateSnapshot;

    // Pre-activation parent (no EVM header) ⇒ the empty genesis state.
    let parent_header = match header_store.get(selected_parent) {
        Ok(h) => h,
        Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => {
            return Ok((EvmStateSnapshot::default(), ParentSeedSource::PreActivation));
        }
        Err(e) => return Err(classify_seed_store_error(e)),
    };

    // Canonical head ⇒ materialize the flat store directly (the head fast path).
    // A `DataInconsistency` here (e.g. a head account referencing absent code) is a
    // real backend fault ⇒ Corrupt; a transient read failure ⇒ Unavailable (skip).
    if flat_head == Some(selected_parent) {
        let snap = materialize_snapshot(flat, code).map_err(classify_seed_store_error)?;
        return Ok((snap, ParentSeedSource::FlatHead));
    }

    // Non-head parent ⇒ §12 reconstruct + keccak-MPT root verify against the
    // parent's committed state root (fail-closed on a broken chain / bad root).
    // `gather_reconstruction_inputs`'s `has_header` is bool-valued, so a store read
    // failure there would otherwise be swallowed as "no header" and anchor the walk
    // at the wrong seed — capture it so it surfaces as Unavailable (a skip), never
    // a silently-wrong reconstruction. The same applies to the code resolver.
    let store_errored = std::cell::Cell::new(false);
    let (seed, forward_diffs) = gather_reconstruction_inputs(
        selected_parent,
        |b| checkpoint_store.get(b),
        |b| diff_store.get(b),
        |b| match header_store.has(b) {
            Ok(v) => v,
            Err(_) => {
                store_errored.set(true);
                false
            }
        },
    )
    .map_err(|e| match e {
        // Retention gap / depth bound / a store read inside the walk ⇒ cannot
        // validate here (skip). A bad checkpoint ENCODING is real corruption (halt).
        ReconstructGatherError::Unavailable(m) | ReconstructGatherError::TooDeep(m) | ReconstructGatherError::Store(m) => {
            ParentSeedError::Unavailable(m)
        }
        ReconstructGatherError::Checkpoint(m) => ParentSeedError::Corrupt(m),
    })?;
    if store_errored.get() {
        return Err(ParentSeedError::Unavailable(format!(
            "header store read failed while gathering reconstruction inputs for {selected_parent}"
        )));
    }
    let snap = kaspa_evm::reconstruct::reconstruct_evm_state(
        &seed,
        &forward_diffs,
        |h| match code.get(*h) {
            Ok(v) => v,
            Err(_) => {
                store_errored.set(true);
                None
            }
        },
        parent_header.state_root,
    )
    .map_err(|e| ParentSeedError::Corrupt(e.to_string()))?;
    // A swallowed code-store read failure could have surfaced as MissingCode inside
    // the engine; reclassify it as a skip (transient I/O), not a Corrupt halt.
    if store_errored.get() {
        return Err(ParentSeedError::Unavailable(format!("code store read failed while reconstructing {selected_parent}")));
    }
    Ok((snap, ParentSeedSource::Reconstructed))
}

#[cfg(all(test, feature = "evm"))]
mod s6_seed_tests {
    use super::{ParentSeedSource, flat_or_reconstruct_parent_snapshot};
    use crate::model::stores::evm::{
        DbEvmCodeStore, DbEvmFlatAccountStore, DbEvmHeaderStore, DbEvmStateCheckpointStore, DbEvmStateDiffStore, EvmHeaderStore,
    };
    use kaspa_consensus_core::BlockHash;
    use kaspa_consensus_core::evm::{
        EVM_EMPTY_CODE_HASH, EvmAccountSnapshot, EvmAddress, EvmExecutionHeader, EvmStateSnapshot, EvmU256, compute_state_diff,
    };
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{CachePolicy, ConnBuilder};
    use kaspa_hashes::EvmH256;
    use rocksdb::WriteBatch;

    fn h(b: u8) -> BlockHash {
        BlockHash::from_bytes([b; 64])
    }

    /// A canonical EOA-only snapshot (sorted, no code/storage).
    fn eoa_snap(accounts: &[(u8, u64, u64)]) -> EvmStateSnapshot {
        let mut accs: Vec<EvmAccountSnapshot> = accounts
            .iter()
            .map(|(a, n, bal)| EvmAccountSnapshot {
                address: EvmAddress::from_bytes([*a; 20]),
                nonce: *n,
                balance: EvmU256::from_u128(*bal as u128),
                code_hash: EVM_EMPTY_CODE_HASH,
                code: vec![],
                storage: vec![],
            })
            .collect();
        accs.sort_by(|x, y| x.address.as_bytes().cmp(&y.address.as_bytes()));
        EvmStateSnapshot { accounts: accs }
    }

    /// The real keccak-MPT state root of a snapshot (what a committed header holds),
    /// via the kaspa-evm trie — so reconstruction's root-verify passes.
    fn root_of(snap: &EvmStateSnapshot) -> EvmH256 {
        let db = kaspa_evm::snapshot::seed_cachedb(snap).expect("canonical snapshot seeds");
        EvmH256::from_bytes(kaspa_evm::state::state_root(&db).0)
    }

    fn header_with_root(root: EvmH256, number: u64) -> EvmExecutionHeader {
        EvmExecutionHeader { state_root: root, evm_number: number, ..Default::default() }
    }

    /// The flat/reconstruct parent seed reproduces the committed 206 snapshot for
    /// all three sources: canonical head (materialize 234), non-head (§12
    /// reconstruct + root-verify), and a pre-activation parent (empty genesis).
    #[test]
    fn parent_seed_matches_206_for_head_reconstruct_and_pre_activation() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let flat = DbEvmFlatAccountStore::new(db.clone(), CachePolicy::Empty);
        let code = DbEvmCodeStore::new(db.clone(), CachePolicy::Empty);
        let header_store = DbEvmHeaderStore::new(db.clone(), CachePolicy::Empty);
        let checkpoint_store = DbEvmStateCheckpointStore::new(db.clone(), CachePolicy::Empty);
        let diff_store = DbEvmStateDiffStore::new(db.clone(), CachePolicy::Empty);

        // Genesis (block 0) has NO header (pre-activation boundary). Branch the flat
        // store onto block H = state s_h; an independent block 1 = state s_1 is only
        // present as a §12 diff + header (the reconstruct case).
        let (genesis, block_h, block_1, pre) = (h(0), h(0x11), h(0x21), h(0x99));
        let s_h = eoa_snap(&[(0x01, 3, 800), (0x02, 0, 500)]);
        let s_1 = eoa_snap(&[(0x01, 1, 1000), (0x03, 2, 250)]);

        // Persist headers (committed state roots) for the EVM blocks.
        let mut batch = WriteBatch::default();
        header_store.insert_batch(&mut batch, block_h, header_with_root(root_of(&s_h), 5)).unwrap();
        header_store.insert_batch(&mut batch, block_1, header_with_root(root_of(&s_1), 1)).unwrap();
        // §12 diff for block 1 over the empty genesis (genesis has no header ⇒ gather anchors empty).
        diff_store
            .insert_batch(&mut batch, block_1, compute_state_diff(&EvmStateSnapshot::default(), &s_1, block_1, genesis))
            .unwrap();
        // Flat store at block H (apply its diff over empty genesis).
        super::apply_diff_to_flat(&flat, &mut batch, &compute_state_diff(&EvmStateSnapshot::default(), &s_h, block_h, genesis))
            .unwrap();
        db.write(batch).unwrap();

        let seed = |parent: BlockHash, flat_head: Option<BlockHash>| {
            flat_or_reconstruct_parent_snapshot(parent, flat_head, &flat, &code, &header_store, &checkpoint_store, &diff_store)
        };

        // (1) Canonical head ⇒ materialize the flat store == s_h.
        let (got_h, src_h) = seed(block_h, Some(block_h)).unwrap();
        assert_eq!(src_h, ParentSeedSource::FlatHead);
        assert_eq!(got_h, s_h, "flat-head materialize must equal the committed state");

        // (2) Non-head parent ⇒ §12 reconstruct (root-verified) == s_1.
        let (got_1, src_1) = seed(block_1, Some(block_h)).unwrap();
        assert_eq!(src_1, ParentSeedSource::Reconstructed);
        assert_eq!(got_1, s_1, "reconstructed non-head parent must equal the committed state");

        // (3) Pre-activation parent (no header) ⇒ empty genesis state.
        let (got_pre, src_pre) = seed(pre, None).unwrap();
        assert_eq!(src_pre, ParentSeedSource::PreActivation);
        assert_eq!(got_pre, EvmStateSnapshot::default(), "pre-activation parent is empty");
    }

    /// C-01 S9b: with 206 retired, the executor seeds the head parent from the flat-materialized
    /// snapshot, validated against the committed root (NOT against 206). This locks in the property
    /// the retire-206 FlatHead check anchors to: `materialize_snapshot(flat_head)` both reproduces
    /// the committed snapshot bytes AND keccak-MPT-hashes to the committed header `state_root` — so
    /// seeding from flat is byte-equivalent to seeding from the (now-absent) 206 snapshot.
    #[test]
    fn flat_head_seed_reproduces_committed_root_without_206() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let flat = DbEvmFlatAccountStore::new(db.clone(), CachePolicy::Empty);
        let code = DbEvmCodeStore::new(db.clone(), CachePolicy::Empty);

        // A committed head state s_h, its committed keccak-MPT root, and the flat store built by
        // applying s_h's §12 diff over the empty genesis (exactly the shadow dual-write head path).
        let (genesis, block_h) = (h(0), h(0x11));
        let s_h = eoa_snap(&[(0x01, 7, 4242), (0x05, 1, 9), (0x09, 0, 1_000_000)]);
        let committed_root = root_of(&s_h);

        let mut batch = WriteBatch::default();
        super::apply_diff_to_flat(&flat, &mut batch, &compute_state_diff(&EvmStateSnapshot::default(), &s_h, block_h, genesis))
            .unwrap();
        db.write(batch).unwrap();

        // The retire-206 seed = materialize the flat store directly (no 206 read).
        let seed = super::materialize_snapshot(&flat, &code).unwrap();
        assert_eq!(seed, s_h, "flat-materialized head seed must equal the committed snapshot (206-free)");
        assert_eq!(
            root_of(&seed),
            committed_root,
            "the flat-materialized head seed must keccak-MPT-hash to the committed header state_root"
        );
    }

    /// A non-head parent whose §12 diff was GC'd surfaces as `Unavailable` (the
    /// shadow check skips it), not a divergence/halt.
    #[test]
    fn non_head_parent_missing_history_is_unavailable() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let flat = DbEvmFlatAccountStore::new(db.clone(), CachePolicy::Empty);
        let code = DbEvmCodeStore::new(db.clone(), CachePolicy::Empty);
        let header_store = DbEvmHeaderStore::new(db.clone(), CachePolicy::Empty);
        let checkpoint_store = DbEvmStateCheckpointStore::new(db.clone(), CachePolicy::Empty);
        let diff_store = DbEvmStateDiffStore::new(db.clone(), CachePolicy::Empty);

        let block_1 = h(0x21);
        let s_1 = eoa_snap(&[(0x01, 1, 1000)]);
        // Header present (EVM block), but NO diff/checkpoint ⇒ gather can't anchor.
        let mut batch = WriteBatch::default();
        header_store.insert_batch(&mut batch, block_1, header_with_root(root_of(&s_1), 1)).unwrap();
        db.write(batch).unwrap();

        let res =
            flat_or_reconstruct_parent_snapshot(block_1, Some(h(0x11)), &flat, &code, &header_store, &checkpoint_store, &diff_store);
        assert!(matches!(res, Err(super::ParentSeedError::Unavailable(_))), "missing §12 history ⇒ Unavailable, got {res:?}");
    }

    /// A flat HEAD whose account references code absent from the content store (222)
    /// is a real backend inconsistency ⇒ `Corrupt` (a HALT), not a silent skip.
    #[test]
    fn flat_head_missing_code_is_corrupt() {
        use crate::model::stores::evm::EvmHeaderStore;
        use kaspa_consensus_core::evm::{AccountCore, FlatAccount};

        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let flat = DbEvmFlatAccountStore::new(db.clone(), CachePolicy::Empty);
        let code = DbEvmCodeStore::new(db.clone(), CachePolicy::Empty);
        let header_store = DbEvmHeaderStore::new(db.clone(), CachePolicy::Empty);
        let checkpoint_store = DbEvmStateCheckpointStore::new(db.clone(), CachePolicy::Empty);
        let diff_store = DbEvmStateDiffStore::new(db.clone(), CachePolicy::Empty);

        let block_h = h(0x11);
        // Header present (so not pre-activation); flat head account references a
        // code_hash with NO entry in the code store ⇒ materialize hits DataInconsistency.
        let mut batch = WriteBatch::default();
        header_store.insert_batch(&mut batch, block_h, header_with_root(EvmH256::from_bytes([0; 32]), 1)).unwrap();
        flat.write_batch(
            &mut batch,
            EvmAddress::from_bytes([0x07; 20]),
            FlatAccount {
                core: AccountCore { nonce: 1, balance: EvmU256::from_u128(1), code_hash: EvmH256::from_bytes([0xAB; 32]) },
                storage: vec![],
            },
        )
        .unwrap();
        db.write(batch).unwrap();

        let res =
            flat_or_reconstruct_parent_snapshot(block_h, Some(block_h), &flat, &code, &header_store, &checkpoint_store, &diff_store);
        assert!(matches!(res, Err(super::ParentSeedError::Corrupt(_))), "flat-head missing code ⇒ Corrupt (halt), got {res:?}");
    }
}

/// O12: one pending chain block for the pipeline worker, in chain order.
pub struct EvmPipelineItem {
    pub block: kaspa_consensus_core::BlockHash,
    pub selected_parent: kaspa_consensus_core::BlockHash,
    /// True when `selected_parent` is the PREVIOUS item in this run — the worker
    /// then chains the in-memory parent state. False at a gap (the predecessor
    /// was already validated/committed earlier), where the worker reads the
    /// parent rows from the store like the inline path.
    pub chain_from_prev: bool,
}

#[cfg(feature = "evm")]
impl EvmPipeline {
    /// Spawn the pipeline worker over `pending` (consecutive chain order). Store
    /// handles are cheap `Arc` clones; the worker reads ONLY committed rows plus
    /// its own in-memory chain, and writes nothing.
    pub fn spawn(
        evm_header_store: std::sync::Arc<crate::model::stores::evm::DbEvmHeaderStore>,
        evm_state_store: std::sync::Arc<crate::model::stores::evm::DbEvmStateStore>,
        evm_payload_store: std::sync::Arc<crate::model::stores::evm::DbEvmPayloadStore>,
        headers_store: std::sync::Arc<crate::model::stores::headers::DbHeadersStore>,
        ghostdag_store: std::sync::Arc<crate::model::stores::ghostdag::DbGhostdagStore>,
        pending: Vec<EvmPipelineItem>,
        gas_pool_v2_activation_daa_score: u64,
        f002_withdraw_cap_activation_daa_score: u64,
        f003_mldsa_verify_activation_daa_score: u64,
        typed_receipt_root_activation_daa_score: u64,
    ) -> EvmPipeline {
        use crate::model::stores::evm::EvmPayloadStoreReader;
        use crate::model::stores::ghostdag::GhostdagStoreReader;
        use crate::model::stores::headers::HeaderStoreReader;
        use kaspa_database::prelude::StoreError;

        // Bounded: at most 2 undelivered results (+1 in flight) — keeps the
        // worker a few blocks ahead without holding many snapshots in memory.
        let (tx, rx) = std::sync::mpsc::sync_channel(2);
        let handle = std::thread::Builder::new()
            .name("evm-pipeline".to_string())
            .spawn(move || {
                // The in-memory parent chain: the previous item's (header, snapshot).
                let mut chained: Option<(kaspa_consensus_core::evm::EvmExecutionHeader, kaspa_consensus_core::evm::EvmStateSnapshot)> =
                    None;
                for item in pending {
                    if !item.chain_from_prev {
                        chained = None; // gap: predecessor already committed — store reads.
                    }
                    let step = (|| -> Result<EvmStaged, driver::EvmValidateError> {
                        let header = headers_store.get_header(item.block).map_err(driver::EvmValidateError::Store)?;
                        let ghostdag = ghostdag_store.get_data(item.block).map_err(driver::EvmValidateError::Store)?;
                        let sorted_mergeset: Vec<kaspa_consensus_core::BlockHash> =
                            ghostdag.consensus_ordered_mergeset(ghostdag_store.as_ref()).collect();
                        let payload = match evm_payload_store.get(item.block) {
                            Ok(p) => p,
                            Err(StoreError::KeyNotFound(_)) => Default::default(),
                            Err(e) => return Err(driver::EvmValidateError::Store(e)),
                        };
                        let staged = driver::evm_validate_chained(
                            &evm_header_store,
                            &evm_state_store,
                            &evm_payload_store,
                            item.block,
                            item.selected_parent,
                            &sorted_mergeset,
                            &header,
                            &payload,
                            chained.take(),
                            gas_pool_v2_activation_daa_score,
                            f002_withdraw_cap_activation_daa_score,
                            f003_mldsa_verify_activation_daa_score,
                            typed_receipt_root_activation_daa_score,
                        )?;
                        // The pipeline only runs over blocks WITHOUT committed EVM rows
                        // (filtered by the caller), so a None (no-replay hit) is a race
                        // that cannot happen on the single-writer virtual thread; treat
                        // it as an internal error rather than panicking a worker.
                        staged.ok_or_else(|| {
                            driver::EvmValidateError::Exec(format!("pipeline raced an existing EVM row for {}", item.block))
                        })
                    })();
                    match step {
                        Ok(staged) => {
                            chained = Some((staged.result.header.clone(), staged.snapshot.clone()));
                            if tx.send((item.block, Ok(staged))).is_err() {
                                return; // walk ended / pipeline dropped
                            }
                        }
                        Err(e) => {
                            // Map to the EXACT strings the inline path produces, so a
                            // pipelined disqualification reads identically in logs.
                            let msg = match e {
                                driver::EvmValidateError::CommitmentMismatch { .. } => {
                                    "evm_commitment_root mismatch (mergeset acceptance re-execution)".to_string()
                                }
                                driver::EvmValidateError::Exec(e) => format!("evm execution: {e}"),
                                driver::EvmValidateError::Store(e) => format!("evm store: {e}"),
                            };
                            let _ = tx.send((item.block, Err(msg)));
                            return; // the chain past an invalid block is dead on this path
                        }
                    }
                }
            })
            .expect("spawn evm-pipeline thread");
        EvmPipeline { rx, _handle: handle }
    }
}

#[cfg(test)]
mod bridge_tests {
    use super::*;
    use kaspa_consensus_core::evm::{DepositClaim, EvmAddress, EvmExecutionPayload, EvmSystemOp, WithdrawOp};
    use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
    use kaspa_consensus_core::utxo::{utxo_collection::UtxoCollection, utxo_diff::UtxoDiff, utxo_view::UtxoView};
    use kaspa_hashes::Hash64;
    use kaspa_muhash::MuHash;
    use kaspa_txscript::script_class::evm_deposit_lock_script;

    struct MapView(UtxoCollection);
    impl UtxoView for MapView {
        fn get(&self, outpoint: &TransactionOutpoint) -> Option<UtxoEntry> {
            self.0.get(outpoint).cloned()
        }
    }

    fn refund_script() -> Vec<u8> {
        // The standard 69-byte ML-DSA P2PKH shape.
        let spk = kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[0x42u8; 64]);
        spk.script().to_vec()
    }

    fn lock_spk(addr: [u8; 20], timeout: u64, tip: u64) -> ScriptPublicKey {
        evm_deposit_lock_script(addr, timeout, tip, &refund_script())
    }

    fn outpoint(b: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
    }

    fn claim_payload(claims: Vec<DepositClaim>) -> EvmExecutionPayload {
        EvmExecutionPayload { system_ops: claims.into_iter().map(EvmSystemOp::DepositClaim).collect(), ..Default::default() }
    }

    fn claim(op: TransactionOutpoint, addr: [u8; 20], amount: u64, tip: u64) -> DepositClaim {
        DepositClaim { deposit_outpoint: op, evm_address: EvmAddress::from_bytes(addr), amount_sompi: amount, claim_tip_sompi: tip }
    }

    /// v0.4 §9.2: the full claim-validation matrix — one valid claim passes and
    /// returns the consumed entry; every producer fault is rejected.
    #[test]
    fn deposit_claim_validation_matrix() {
        let addr = [0xCC; 20];
        let op = outpoint(1);
        let mut view = UtxoCollection::default();
        view.insert(op, UtxoEntry::new(500, lock_spk(addr, 1_000, 7), 10, false));
        let view = MapView(view);

        // Valid: fields match, pov below the timeout.
        let consumed = validate_evm_deposit_claims(&claim_payload(vec![claim(op, addr, 500, 7)]), &view, 999).unwrap();
        assert_eq!(consumed.len(), 1);
        assert_eq!(consumed[0].0, op);
        assert_eq!(consumed[0].1.amount, 500);

        // Faults, each rejected: absent lock / wrong amount / wrong tip /
        // wrong address / claim at-or-after the refund timeout (AC-2) /
        // duplicate outpoint / a non-lock outpoint.
        assert!(validate_evm_deposit_claims(&claim_payload(vec![claim(outpoint(9), addr, 500, 7)]), &view, 999).is_err());
        assert!(validate_evm_deposit_claims(&claim_payload(vec![claim(op, addr, 400, 7)]), &view, 999).is_err());
        assert!(validate_evm_deposit_claims(&claim_payload(vec![claim(op, addr, 500, 8)]), &view, 999).is_err());
        assert!(validate_evm_deposit_claims(&claim_payload(vec![claim(op, [0xDD; 20], 500, 7)]), &view, 999).is_err());
        assert!(
            validate_evm_deposit_claims(&claim_payload(vec![claim(op, addr, 500, 7)]), &view, 1_000).is_err(),
            "refund window open"
        );
        assert!(
            validate_evm_deposit_claims(&claim_payload(vec![claim(op, addr, 500, 7), claim(op, addr, 500, 7)]), &view, 999).is_err(),
            "duplicate outpoint"
        );
        let mut plain = UtxoCollection::default();
        plain.insert(op, UtxoEntry::new(500, kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[1u8; 64]), 10, false));
        assert!(
            validate_evm_deposit_claims(&claim_payload(vec![claim(op, addr, 500, 7)]), &MapView(plain), 999).is_err(),
            "not a lock"
        );
    }

    /// v0.4 §9 / I7: the bridge effects ride the block's own diff + multiset —
    /// a consumed lock lands in `diff.remove`, a withdrawal materializes as a
    /// synthetic output at the frozen-domain txid in `diff.add`, and the
    /// multiset mirrors both (so `utxo_commitment` covers the bridge).
    #[test]
    fn bridge_effects_enter_diff_and_multiset() {
        let evm_tx_hash = kaspa_hashes::EvmH256::from_bytes([7; 32]);
        let op = outpoint(1);
        let lock_entry = UtxoEntry::new(500, lock_spk([0xCC; 20], 1_000, 0), 10, false);
        let spk = kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[0x42u8; 64]);
        let w = WithdrawOp {
            receipt_index: 3,
            op_index: 1,
            evm_tx_hash,
            from: EvmAddress::from_bytes([0xAA; 20]),
            script_public_key: spk.clone(),
            amount_sompi: 5,
        };

        let mut diff = UtxoDiff::default();
        let mut multiset = MuHash::new();
        let baseline = multiset.clone();
        apply_evm_bridge_effects(&mut diff, &mut multiset, 42, &[(op, lock_entry.clone())], &[w.clone()]).unwrap();

        assert!(diff.remove.contains_key(&op), "the consumed lock leaves the UTXO set via this block's diff");
        // Keyed by the WITHDRAWING TX's hash — pre-mining-stable (a block-hash
        // key was circular: the producer's own utxo_commitment must contain
        // this output BEFORE the block hash exists).
        let expected_txid =
            kaspa_consensus_core::evm::synthetic_withdrawal_txid(evm_tx_hash, 1, w.from, w.amount_sompi, &w.script_public_key);
        let synthetic = TransactionOutpoint::new(expected_txid, 0);
        let entry = diff.add.get(&synthetic).expect("the withdrawal materialized at the frozen-domain outpoint");
        assert_eq!(entry.amount, 5);
        assert_eq!(entry.script_public_key, spk);
        assert_eq!(entry.block_daa_score, 42);
        assert!(!entry.is_coinbase, "synthetic outputs are NOT coinbase (no maturity wait)");
        assert_ne!(multiset.finalize(), baseline.clone().finalize(), "the multiset covers the bridge");

        // Determinism: identical content ⇒ identical txid.
        assert_eq!(
            expected_txid,
            kaspa_consensus_core::evm::synthetic_withdrawal_txid(evm_tx_hash, 1, w.from, w.amount_sompi, &w.script_public_key)
        );
        // Uniqueness across op_index and evm tx hash (unchanged from before).
        assert_ne!(
            expected_txid,
            kaspa_consensus_core::evm::synthetic_withdrawal_txid(evm_tx_hash, 2, w.from, w.amount_sompi, &w.script_public_key)
        );
        assert_ne!(
            expected_txid,
            kaspa_consensus_core::evm::synthetic_withdrawal_txid(
                kaspa_hashes::EvmH256::from_bytes([8; 32]),
                1,
                w.from,
                w.amount_sompi,
                &w.script_public_key
            )
        );
        // Audit F1: the SAME (evm_tx_hash, op_index) with DIFFERENT materialized content must yield a
        // DIFFERENT outpoint — a contract-mediated, branch-dependent withdraw can never collide.
        assert_ne!(
            expected_txid,
            kaspa_consensus_core::evm::synthetic_withdrawal_txid(evm_tx_hash, 1, w.from, w.amount_sompi + 1, &w.script_public_key),
            "amount_sompi must bind the outpoint"
        );
        assert_ne!(
            expected_txid,
            kaspa_consensus_core::evm::synthetic_withdrawal_txid(
                evm_tx_hash,
                1,
                EvmAddress::from_bytes([0xBB; 20]),
                w.amount_sompi,
                &w.script_public_key
            ),
            "from must bind the outpoint"
        );
        let other_spk = kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[0x43u8; 64]);
        assert_ne!(
            expected_txid,
            kaspa_consensus_core::evm::synthetic_withdrawal_txid(evm_tx_hash, 1, w.from, w.amount_sompi, &other_spk),
            "destination script must bind the outpoint"
        );
    }
}

#[cfg(all(test, feature = "evm"))]
mod tests {
    use super::*;
    use crate::model::stores::evm::{DbEvmHeaderStore, DbEvmPayloadStore, DbEvmStateStore, EvmHeaderStoreReader, EvmPayloadStore};
    use kaspa_consensus_core::constants::EVM_HEADER_VERSION;
    use kaspa_consensus_core::evm::{DepositClaim, EvmAddress, EvmExecutionPayload, EvmStateSnapshot, EvmSystemOp};
    use kaspa_consensus_core::header::Header;
    use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{CachePolicy, ConnBuilder};
    use kaspa_hashes::Hash64;
    use rocksdb::WriteBatch;

    fn header(timestamp: u64, daa: u64) -> Header {
        Header::new_finalized(
            EVM_HEADER_VERSION,
            vec![vec![Hash64::from_bytes([1; 64])]].try_into().unwrap(),
            Default::default(),
            Default::default(),
            Default::default(),
            timestamp,
            0,
            0,
            POW_ALGO_ID_KHEAVYHASH,
            daa,
            5000u64.into(),
            0,
            Default::default(),
        )
    }

    /// v0.4 mergeset-acceptance driver e2e without pulling alloy into the
    /// consensus test: B's OWN payload carries a deposit claim (system ops
    /// execute in B, §3.2) while a MERGESET block's stored payload contributes
    /// the user-tx candidates — here an undecodable tx, which the executor
    /// deterministically skips (defense-in-depth class-1 material), proving the
    /// driver gathered it. Covers validate → persist → no-replay → mismatch.
    #[test]
    fn driver_gathers_mergeset_validates_persists_and_never_replays() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let header_store = DbEvmHeaderStore::new(db.clone(), CachePolicy::Empty);
        let state_store = DbEvmStateStore::new(db.clone(), CachePolicy::Empty);
        let payload_store = DbEvmPayloadStore::new(db.clone(), CachePolicy::Empty);
        let receipts_store = crate::model::stores::evm::DbEvmReceiptsStore::new(db.clone(), CachePolicy::Empty);
        let tx_index_store = crate::model::stores::evm::DbEvmTxIndexStore::new(db.clone(), CachePolicy::Empty);
        let log_index_store = crate::model::stores::evm::DbEvmLogIndexStore::new(db.clone());
        let trace_store = crate::model::stores::evm::DbEvmTraceReplayStore::new(db.clone(), CachePolicy::Empty);
        let diff_store = crate::model::stores::evm::DbEvmStateDiffStore::new(db.clone(), CachePolicy::Empty);
        let code_store = crate::model::stores::evm::DbEvmCodeStore::new(db.clone(), CachePolicy::Empty);
        let checkpoint_store = crate::model::stores::evm::DbEvmStateCheckpointStore::new(db.clone(), CachePolicy::Empty);

        // First EVM block on genesis: the driver reads the parent's state as
        // absent => the empty (genesis) snapshot — no seeding needed.
        let selected_parent = Hash64::from_bytes([0xAA; 64]);
        let merged = Hash64::from_bytes([0xBB; 64]);
        let payload = EvmExecutionPayload {
            system_ops: vec![EvmSystemOp::DepositClaim(DepositClaim {
                deposit_outpoint: Default::default(),
                evm_address: EvmAddress::from_bytes([0xCC; 20]),
                amount_sompi: 7,
                claim_tip_sompi: 0,
            })],
            ..Default::default()
        };

        // The mergeset block's payload (one undecodable user tx) sits in the
        // payload store, exactly as commit_body persists it (M10-D).
        let merged_payload = EvmExecutionPayload {
            transactions: vec![vec![0xde, 0xad, 0xbe, 0xef]],
            evm_coinbase: EvmAddress::from_bytes([0xAB; 20]),
            ..Default::default()
        };
        let mut b0 = WriteBatch::default();
        payload_store.insert_batch(&mut b0, merged, merged_payload.clone()).unwrap();
        db.write(b0).unwrap();

        // Pre-compute the expected commitment with the exact candidates the
        // driver gathers: sorted_mergeset = [selected_parent (no payload stored
        // => empty), merged (one tx)].
        let l1 = header(7_000, 9);
        let candidates =
            vec![AcceptedTxCandidate { raw: vec![0xde, 0xad, 0xbe, 0xef], payload_coinbase: merged_payload.evm_coinbase }];
        let input = EvmBlockInput {
            parent: None,
            header_timestamp_ms: l1.timestamp,
            selected_parent_hash: selected_parent.as_bytes(),
            blue_work_be: l1.blue_work.to_be_bytes().to_vec(),
            daa_score: l1.daa_score,
            payload: &payload,
            accepted_txs: &candidates,
            gas_pool_v2_activation_daa_score: u64::MAX,
            f002_withdraw_cap_activation_daa_score: u64::MAX,
            f003_mldsa_verify_activation_daa_score: u64::MAX,
            typed_receipt_root_activation_daa_score: u64::MAX,
        };
        let (expected, _) = kaspa_evm::snapshot::execute_block_from_snapshot(&EvmStateSnapshot::default(), &input).unwrap();
        assert_eq!(expected.header.skipped_tx_count, 1, "the gathered mergeset tx was deterministically skipped");
        let l1 = l1.with_evm_commitment(expected.header.commitment_root());
        let mergeset = [selected_parent, merged];

        // Drive: gathers the mergeset payloads, validates the commitment and
        // persists header + child state.
        let mut b1 = WriteBatch::default();
        evm_validate_and_persist(
            &header_store,
            &state_store,
            &payload_store,
            &receipts_store,
            &tx_index_store,
            &log_index_store,
            &trace_store,
            &diff_store,
            &code_store,
            &checkpoint_store,
            &mut b1,
            l1.hash,
            selected_parent,
            &mergeset,
            &l1,
            &payload,
            u64::MAX,
            u64::MAX,
            u64::MAX,
            u64::MAX,
        )
        .unwrap();
        db.write(b1).unwrap();
        assert_eq!(header_store.get(l1.hash).unwrap(), expected.header);
        assert_eq!(expected.applied_deposit_claims.len(), 1, "the deposit claim was applied");
        // §16: the index rows landed in the same batch — the (skipped) mergeset
        // tx is visible in the lookup: included in `merged`, never accepted.
        let tx_h = kaspa_evm::tx::tx_hash(&[0xde, 0xad, 0xbe, 0xef]);
        let row = tx_index_store.get_or_default(tx_h).unwrap();
        assert_eq!(row.included_in, vec![merged]);
        assert!(row.accepted_in.is_empty());
        // Audit L5: undecodable material carries its DESIGN class (1, syntactic)
        // in the index — a defensive label; body validation rejects such
        // payloads outright, so the path is unreachable for relayed blocks.
        assert_eq!(row.last_skip_class, Some(1), "undecodable candidate = defensive class-1 skip label");
        use crate::model::stores::evm::EvmReceiptsStoreReader;
        assert!(!receipts_store.has(l1.hash).unwrap(), "no receipts row for a block with zero accepted txs");
        // §11: the trace replay plan WAS persisted (the block had a candidate tx,
        // even though it was skipped) — its full ordered candidate list is recorded
        // so a replay reproduces the skip. The body is keyed by the accepting block.
        use crate::model::stores::evm::EvmTraceReplayStoreReader;
        let trace_body = trace_store.get(l1.hash).unwrap().expect("a trace replay body was persisted");
        assert_eq!(trace_body.selected_parent, selected_parent);
        assert_eq!(trace_body.txs.len(), 1, "the one mergeset candidate is recorded");
        assert_eq!(trace_body.txs[0].tx_hash, tx_h);
        assert_eq!(trace_body.txs[0].originating_payload_block, merged);
        assert!(
            matches!(trace_body.txs[0].outcome, kaspa_consensus_core::evm::EvmCandidateOutcome::Skipped { .. }),
            "the undecodable candidate is recorded as skipped"
        );

        // §12 archive: the block's forward state DIFF (prefix 220) landed in the same
        // batch and equals the diff recomputed from (genesis parent, committed child).
        use crate::model::stores::evm::{
            EvmCodeStoreReader, EvmStateCheckpointStoreReader, EvmStateDiffStoreReader, EvmStateStoreReader,
        };
        let stored_diff = diff_store.get(l1.hash).unwrap().expect("a §12 state diff was persisted");
        let child_snap = state_store.get(l1.hash).unwrap();
        assert!(!child_snap.is_empty(), "the deposit claim credited an account, so the diff is non-trivial");
        let recomputed =
            kaspa_consensus_core::evm::compute_state_diff(&EvmStateSnapshot::default(), &child_snap, l1.hash, selected_parent);
        assert_eq!(stored_diff, recomputed, "stored diff == diff over the selected parent");
        assert_eq!(stored_diff.parent, selected_parent);
        // Reconstruct from the genesis seed + the stored diff, resolving code from the
        // content-addressed store (prefix 222) — it reproduces the committed snapshot.
        let mut recon = kaspa_consensus_core::evm::recon_from_snapshot(&EvmStateSnapshot::default());
        kaspa_consensus_core::evm::apply_state_diff(&mut recon, &stored_diff).unwrap();
        let rebuilt = kaspa_consensus_core::evm::recon_to_snapshot(&recon, |h| code_store.get(*h).ok().flatten()).unwrap();
        assert_eq!(rebuilt, child_snap, "reconstruction from genesis + stored diff == the committed state");
        // Checkpoint presence follows the interval rule; when present it decodes back
        // to the committed state and carries the committed state root.
        let evm_number = expected.header.evm_number;
        let has_cp = checkpoint_store.has(l1.hash).unwrap();
        assert_eq!(has_cp, evm_number % kaspa_consensus_core::evm::EVM_CHECKPOINT_INTERVAL == 0);
        if has_cp {
            let cp = checkpoint_store.get(l1.hash).unwrap().unwrap();
            assert_eq!(cp.state_root, expected.header.state_root);
            assert_eq!(cp.decode_snapshot().unwrap(), child_snap, "checkpoint decodes to the committed state");
        }

        // No-replay: re-driving is a no-op (the already-stored result is reused).
        let mut b2 = WriteBatch::default();
        evm_validate_and_persist(
            &header_store,
            &state_store,
            &payload_store,
            &receipts_store,
            &tx_index_store,
            &log_index_store,
            &trace_store,
            &diff_store,
            &code_store,
            &checkpoint_store,
            &mut b2,
            l1.hash,
            selected_parent,
            &mergeset,
            &l1,
            &payload,
            u64::MAX,
            u64::MAX,
            u64::MAX,
            u64::MAX,
        )
        .unwrap();

        // A wrong commitment for a fresh block => block-invalid. The same holds
        // for a producer that committed WITHOUT the mergeset txs: gathering is
        // consensus (a commitment over the empty candidate set must mismatch).
        let bad = header(8_000, 10).with_evm_commitment(Hash64::from_bytes([0xEE; 64]));
        let mut b3 = WriteBatch::default();
        let err = evm_validate_and_persist(
            &header_store,
            &state_store,
            &payload_store,
            &receipts_store,
            &tx_index_store,
            &log_index_store,
            &trace_store,
            &diff_store,
            &code_store,
            &checkpoint_store,
            &mut b3,
            bad.hash,
            selected_parent,
            &mergeset,
            &bad,
            &payload,
            u64::MAX,
            u64::MAX,
            u64::MAX,
            u64::MAX,
        );
        assert!(matches!(err, Err(EvmValidateError::CommitmentMismatch { .. })));

        let no_mergeset_input = EvmBlockInput { accepted_txs: &[], ..input };
        let (no_mergeset, _) =
            kaspa_evm::snapshot::execute_block_from_snapshot(&EvmStateSnapshot::default(), &no_mergeset_input).unwrap();
        let bad2 = header(7_000, 9).with_evm_commitment(no_mergeset.header.commitment_root());
        let mut b4 = WriteBatch::default();
        let err = evm_validate_and_persist(
            &header_store,
            &state_store,
            &payload_store,
            &receipts_store,
            &tx_index_store,
            &log_index_store,
            &trace_store,
            &diff_store,
            &code_store,
            &checkpoint_store,
            &mut b4,
            bad2.hash,
            selected_parent,
            &mergeset,
            &bad2,
            &payload,
            u64::MAX,
            u64::MAX,
            u64::MAX,
            u64::MAX,
        );
        assert!(
            matches!(err, Err(EvmValidateError::CommitmentMismatch { .. })),
            "omitting the mergeset acceptance is a commitment fault"
        );
    }
}
