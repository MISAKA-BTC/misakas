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
pub use kaspa_evm::{execute_block_evm, AcceptedTxCandidate, EvmBlockInput};

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
    batch: &mut rocksdb::WriteBatch,
    accepting: kaspa_consensus_core::BlockHash,
    staged: &EvmStaged,
) -> Result<(), kaspa_database::prelude::StoreError> {
    use kaspa_consensus_core::evm::{EvmCandidateOutcome, MAX_TX_LOCATION_ACCEPTANCES, MAX_TX_LOCATION_INCLUSIONS};

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

#[cfg(feature = "evm")]
mod driver {
    use crate::model::stores::evm::{
        DbEvmHeaderStore, DbEvmPayloadStore, DbEvmStateStore, EvmHeaderStore, EvmHeaderStoreReader, EvmPayloadStoreReader,
        EvmStateStore, EvmStateStoreReader,
    };
    use kaspa_consensus_core::evm::{EvmExecutionPayload, EvmStateSnapshot};
    use kaspa_consensus_core::header::Header;
    use kaspa_consensus_core::BlockHash;
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
    ) -> Result<Option<super::EvmStaged>, EvmValidateError> {
        // No-replay: this block's EVM result was computed when it first joined the
        // selected chain; never recompute it.
        if header_store.has(block).map_err(EvmValidateError::Store)? {
            return Ok(None);
        }
        debug_assert!(!sorted_mergeset.contains(&block), "a block is never in its own mergeset (off-by-one, §3.1)");

        let (result, child_snapshot, candidate_meta) = evm_execute_acceptance(
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
        )?;

        // The only block-invalidating EVM condition: producer commitment mismatch
        // (user tx failures are status-0 receipts inside `result`, design §6.2).
        if result.header.commitment_root() != l1_header.evm_commitment_root {
            return Err(EvmValidateError::CommitmentMismatch { block });
        }

        Ok(Some(super::EvmStaged { result, snapshot: child_snapshot, candidate_meta }))
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
    ) -> Result<Option<super::EvmStaged>, EvmValidateError> {
        if header_store.has(block).map_err(EvmValidateError::Store)? {
            return Ok(None);
        }
        let (result, child_snapshot, candidate_meta) = evm_execute_acceptance_with_parent(
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
        )?;
        if result.header.commitment_root() != l1_header.evm_commitment_root {
            return Err(EvmValidateError::CommitmentMismatch { block });
        }
        Ok(Some(super::EvmStaged { result, snapshot: child_snapshot, candidate_meta }))
    }

    /// The shared execution core: run one block's mergeset acceptance from the
    /// stores. Used by the verifier ([`evm_validate`]) AND by the template
    /// builder (§15 — the producer computes the commitment it will declare,
    /// with the exact code the verifier later re-runs, so a mined block
    /// reproduces the commitment byte-for-byte). `l1_header` supplies only the
    /// env inputs (timestamp / blue_work / daa_score) — its EVM fields are not
    /// read here.
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
    ) -> Result<(kaspa_consensus_core::evm::EvmExecutionResult, EvmStateSnapshot, Vec<(kaspa_hashes::EvmH256, BlockHash)>), EvmValidateError>
    {
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
    pub(super) fn evm_execute_acceptance_with_parent(
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
    ) -> Result<(kaspa_consensus_core::evm::EvmExecutionResult, EvmStateSnapshot, Vec<(kaspa_hashes::EvmH256, BlockHash)>), EvmValidateError>
    {
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
                            )))
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
        };

        let (result, snapshot) =
            kaspa_evm::snapshot::execute_block_from_snapshot(&parent_snapshot, &input).map_err(|e| EvmValidateError::Exec(e.to_string()))?;
        Ok((result, snapshot, candidate_meta))
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
        batch: &mut WriteBatch,
        block: BlockHash,
        selected_parent: BlockHash,
        sorted_mergeset: &[BlockHash],
        l1_header: &Header,
        payload: &EvmExecutionPayload,
        gas_pool_v2_activation_daa_score: u64,
        f002_withdraw_cap_activation_daa_score: u64,
        f003_mldsa_verify_activation_daa_score: u64,
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
        )?
        else {
            return Ok(());
        };
        header_store.insert_batch(batch, block, staged.result.header.clone()).map_err(EvmValidateError::Store)?;
        super::stage_evm_index_rows(receipts_store, tx_index_store, batch, block, &staged).map_err(EvmValidateError::Store)?;
        state_store.insert_batch(batch, block, staged.snapshot).map_err(EvmValidateError::Store)?;
        Ok(())
    }
}

#[cfg(feature = "evm")]
pub use driver::{evm_execute_acceptance, evm_validate, evm_validate_and_persist, EvmValidateError};

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
        assert!(validate_evm_deposit_claims(&claim_payload(vec![claim(op, addr, 500, 7)]), &view, 1_000).is_err(), "refund window open");
        assert!(
            validate_evm_deposit_claims(&claim_payload(vec![claim(op, addr, 500, 7), claim(op, addr, 500, 7)]), &view, 999).is_err(),
            "duplicate outpoint"
        );
        let mut plain = UtxoCollection::default();
        plain.insert(op, UtxoEntry::new(500, kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[1u8; 64]), 10, false));
        assert!(validate_evm_deposit_claims(&claim_payload(vec![claim(op, addr, 500, 7)]), &MapView(plain), 999).is_err(), "not a lock");
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
            &mut b1,
            l1.hash,
            selected_parent,
            &mergeset,
            &l1,
            &payload,
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

        // No-replay: re-driving is a no-op (the already-stored result is reused).
        let mut b2 = WriteBatch::default();
        evm_validate_and_persist(
            &header_store,
            &state_store,
            &payload_store,
            &receipts_store,
            &tx_index_store,
            &mut b2,
            l1.hash,
            selected_parent,
            &mergeset,
            &l1,
            &payload,
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
            &mut b3,
            bad.hash,
            selected_parent,
            &mergeset,
            &bad,
            &payload,
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
            &mut b4,
            bad2.hash,
            selected_parent,
            &mergeset,
            &bad2,
            &payload,
            u64::MAX,
            u64::MAX,
            u64::MAX,
        );
        assert!(matches!(err, Err(EvmValidateError::CommitmentMismatch { .. })), "omitting the mergeset acceptance is a commitment fault");
    }
}
