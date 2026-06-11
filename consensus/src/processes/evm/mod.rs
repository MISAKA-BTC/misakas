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
    for (i, raw) in payload.transactions.iter().enumerate() {
        kaspa_evm::tx::admit_tx(raw).map_err(|reason| (i, reason))?;
    }
    Ok(())
}

#[cfg(not(feature = "evm"))]
pub fn admit_evm_payload_txs(_payload: &kaspa_consensus_core::evm::EvmExecutionPayload) -> Result<(), (usize, String)> {
    Ok(())
}

/// v0.4 §9.2: validate a chain block's own `DepositClaim` system ops against
/// the claim view (the selected-parent UTXO set composed with the mergeset
/// diff so far — a lock spent by a mergeset tx is no longer claimable, and a
/// lock created in B's own body is not visible yet, the "same-block" rule).
/// Returns the consumed `(outpoint, entry)` pairs in payload order.
///
/// Every violation is a fault of the ACCEPTING producer (it selected its own
/// system ops, §6.2): missing/spent lock, a non-lock outpoint, field mismatch
/// (address / amount / tip), tip > amount, a claim at/after the refund
/// timeout (AC-2 exclusivity), or a duplicate outpoint within the block.
/// Pure + always compiled (the lock parses via kaspa-txscript; no revm).
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
        let Some(entry) = claim_view.get(&claim.deposit_outpoint) else {
            return Err(format!("system op #{i}: deposit lock {} is absent/spent in the claim view", claim.deposit_outpoint));
        };
        let Some(lock) = kaspa_txscript::script_class::parse_evm_deposit_lock(&entry.script_public_key) else {
            return Err(format!("system op #{i}: outpoint {} is not an EVM_DEPOSIT_LOCK output", claim.deposit_outpoint));
        };
        if lock.evm_address != claim.evm_address.as_bytes() {
            return Err(format!("system op #{i}: claim address does not match the lock"));
        }
        if entry.amount != claim.amount_sompi {
            return Err(format!("system op #{i}: claim amount {} != lock value {}", claim.amount_sompi, entry.amount));
        }
        if lock.claim_tip_sompi != claim.claim_tip_sompi {
            return Err(format!("system op #{i}: claim tip {} != lock tip {}", claim.claim_tip_sompi, lock.claim_tip_sompi));
        }
        if claim.claim_tip_sompi > claim.amount_sompi {
            return Err(format!("system op #{i}: tip exceeds the locked amount"));
        }
        // AC-2 exclusivity: claim valid iff accepting daa < timeout (at/after
        // the timeout the lock belongs to the refund path).
        if pov_daa_score >= lock.timeout_daa_score {
            return Err(format!(
                "system op #{i}: claim at daa {pov_daa_score} ≥ refund timeout {} (refund window open)",
                lock.timeout_daa_score
            ));
        }
        consumed.push((claim.deposit_outpoint, entry));
    }
    Ok(consumed)
}

/// v0.4 §9 — fold the bridge's UTXO side-effects into the accepting block's
/// OWN per-block diff + multiset (the slashing-side-effect mechanism,
/// verbatim): consumed deposit locks leave the UTXO set; each `WithdrawOp`
/// materializes as a synthetic output at
/// `(synthetic_withdrawal_txid(block, tx, op), 0)`. Because they ride the
/// persisted per-block diff, reorg apply/revert is the existing UTXO
/// machinery — the EVM side never reverts (pointer-switch only), so combined
/// supply is conserved across any reorg with zero bespoke code (invariant I7).
pub fn apply_evm_bridge_effects(
    diff: &mut kaspa_consensus_core::utxo::utxo_diff::UtxoDiff,
    multiset: &mut kaspa_muhash::MuHash,
    block: kaspa_consensus_core::BlockHash,
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
        let txid = kaspa_consensus_core::evm::synthetic_withdrawal_txid(block, w.evm_tx_index, w.op_index);
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

    if !staged.result.receipts.is_empty() {
        // tx_hashes parallel to the receipts: the accepted candidates in order.
        let mut tx_hashes = vec![Default::default(); staged.result.receipts.len()];
        for (i, (hash, _src)) in staged.candidate_meta.iter().enumerate() {
            if let EvmCandidateOutcome::Accepted { receipt_index } = staged.result.candidate_outcomes[i] {
                tx_hashes[receipt_index as usize] = *hash;
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
    use kaspa_consensus_core::evm::{EvmExecutionPayload, EvmStateSnapshot, EVM_GENESIS_STATE_ROOT};
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
    pub fn evm_validate(
        header_store: &DbEvmHeaderStore,
        state_store: &DbEvmStateStore,
        payload_store: &DbEvmPayloadStore,
        block: BlockHash,
        selected_parent: BlockHash,
        sorted_mergeset: &[BlockHash],
        l1_header: &Header,
        payload: &EvmExecutionPayload,
    ) -> Result<Option<super::EvmStaged>, EvmValidateError> {
        // No-replay: this block's EVM result was computed when it first joined the
        // selected chain; never recompute it.
        if header_store.has(block).map_err(EvmValidateError::Store)? {
            return Ok(None);
        }
        debug_assert!(!sorted_mergeset.contains(&block), "a block is never in its own mergeset (off-by-one, §3.1)");

        let (result, child_snapshot, candidate_meta) =
            evm_execute_acceptance(header_store, state_store, payload_store, selected_parent, sorted_mergeset, l1_header, payload)?;

        // The only block-invalidating EVM condition: producer commitment mismatch
        // (user tx failures are status-0 receipts inside `result`, design §6.2).
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

        // Selected-parent EVM header + state (absent ⇒ first EVM block on genesis).
        let parent_header = match header_store.get(selected_parent) {
            Ok(h) => Some(h),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(e) => return Err(EvmValidateError::Store(e)),
        };
        let parent_snapshot = match state_store.get(selected_parent) {
            Ok(s) => s,
            Err(StoreError::KeyNotFound(_)) => EvmStateSnapshot::default(),
            Err(e) => return Err(EvmValidateError::Store(e)),
        };
        debug_assert!(
            parent_header.as_ref().map(|h| h.state_root).unwrap_or(EVM_GENESIS_STATE_ROOT) == EVM_GENESIS_STATE_ROOT || !parent_snapshot.is_empty(),
            "a non-genesis EVM parent must have a persisted state snapshot"
        );

        let input = super::EvmBlockInput {
            parent: parent_header.as_ref(),
            header_timestamp_ms: l1_header.timestamp,
            selected_parent_hash: selected_parent.as_bytes(),
            blue_work_be: l1_header.blue_work.to_be_bytes().to_vec(),
            daa_score: l1_header.daa_score,
            payload,
            accepted_txs: &accepted_txs,
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
    ) -> Result<(), EvmValidateError> {
        let Some(staged) =
            evm_validate(header_store, state_store, payload_store, block, selected_parent, sorted_mergeset, l1_header, payload)?
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
        let block = Hash64::from_bytes([7; 64]);
        let op = outpoint(1);
        let lock_entry = UtxoEntry::new(500, lock_spk([0xCC; 20], 1_000, 0), 10, false);
        let spk = kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[0x42u8; 64]);
        let w = WithdrawOp { evm_tx_index: 3, op_index: 1, from: EvmAddress::from_bytes([0xAA; 20]), script_public_key: spk.clone(), amount_sompi: 5 };

        let mut diff = UtxoDiff::default();
        let mut multiset = MuHash::new();
        let baseline = multiset.clone();
        apply_evm_bridge_effects(&mut diff, &mut multiset, block, 42, &[(op, lock_entry.clone())], &[w.clone()]).unwrap();

        assert!(diff.remove.contains_key(&op), "the consumed lock leaves the UTXO set via this block's diff");
        let expected_txid = kaspa_consensus_core::evm::synthetic_withdrawal_txid(block, 3, 1);
        let synthetic = TransactionOutpoint::new(expected_txid, 0);
        let entry = diff.add.get(&synthetic).expect("the withdrawal materialized at the frozen-domain outpoint");
        assert_eq!(entry.amount, 5);
        assert_eq!(entry.script_public_key, spk);
        assert_eq!(entry.block_daa_score, 42);
        assert!(!entry.is_coinbase, "synthetic outputs are NOT coinbase (no maturity wait)");
        assert_ne!(multiset.finalize(), baseline.clone().finalize(), "the multiset covers the bridge");

        // Determinism + uniqueness of the synthetic txid.
        assert_eq!(expected_txid, kaspa_consensus_core::evm::synthetic_withdrawal_txid(block, 3, 1));
        assert_ne!(expected_txid, kaspa_consensus_core::evm::synthetic_withdrawal_txid(block, 3, 2));
        assert_ne!(expected_txid, kaspa_consensus_core::evm::synthetic_withdrawal_txid(Hash64::from_bytes([8; 64]), 3, 1));
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
        };
        let (expected, _) = kaspa_evm::snapshot::execute_block_from_snapshot(&EvmStateSnapshot::default(), &input).unwrap();
        assert_eq!(expected.header.skipped_tx_count, 1, "the gathered mergeset tx was deterministically skipped");
        let l1 = l1.with_evm_commitment(expected.header.commitment_root());
        let mergeset = [selected_parent, merged];

        // Drive: gathers the mergeset payloads, validates the commitment and
        // persists header + child state.
        let mut b1 = WriteBatch::default();
        evm_validate_and_persist(&header_store, &state_store, &payload_store, &receipts_store, &tx_index_store, &mut b1, l1.hash, selected_parent, &mergeset, &l1, &payload)
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
        evm_validate_and_persist(&header_store, &state_store, &payload_store, &receipts_store, &tx_index_store, &mut b2, l1.hash, selected_parent, &mergeset, &l1, &payload)
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
        );
        assert!(matches!(err, Err(EvmValidateError::CommitmentMismatch { .. })), "omitting the mergeset acceptance is a commitment fault");
    }
}
