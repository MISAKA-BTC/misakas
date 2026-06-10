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

    /// The staged output of a validated EVM step: the rows the caller commits
    /// in ITS batch (atomically with the block's UTXO diff).
    pub type EvmStaged = (kaspa_consensus_core::evm::EvmExecutionHeader, EvmStateSnapshot);

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
    ) -> Result<Option<EvmStaged>, EvmValidateError> {
        // No-replay: this block's EVM result was computed when it first joined the
        // selected chain; never recompute it.
        if header_store.has(block).map_err(EvmValidateError::Store)? {
            return Ok(None);
        }

        // AcceptedEvmTxs(B): the mergeset's payload txs in canonical order
        // (sorted_mergeset, then payload order — design §3.1). The class-5
        // prefix-take and class-2/3 skips are applied inside the executor.
        debug_assert!(!sorted_mergeset.contains(&block), "a block is never in its own mergeset (off-by-one, §3.1)");
        let mut accepted_txs: Vec<AcceptedTxCandidate> = Vec::new();
        for merged in sorted_mergeset {
            let merged_payload = match payload_store.get(*merged) {
                Ok(p) => p,
                Err(StoreError::KeyNotFound(_)) => continue, // empty payloads are not persisted
                Err(e) => return Err(EvmValidateError::Store(e)),
            };
            let payload_coinbase = merged_payload.evm_coinbase;
            accepted_txs.extend(merged_payload.transactions.into_iter().map(|raw| AcceptedTxCandidate { raw, payload_coinbase }));
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

        let (result, child_snapshot) =
            kaspa_evm::snapshot::execute_block_from_snapshot(&parent_snapshot, &input).map_err(|e| EvmValidateError::Exec(e.to_string()))?;

        // The only block-invalidating EVM condition: producer commitment mismatch
        // (user tx failures are status-0 receipts inside `result`, design §6.2).
        if result.header.commitment_root() != l1_header.evm_commitment_root {
            return Err(EvmValidateError::CommitmentMismatch { block });
        }

        Ok(Some((result.header, child_snapshot)))
    }

    /// Validate + stage into `batch` in one call (the unit-test surface; the
    /// virtual processor calls [`evm_validate`] and stages inside its own
    /// `commit_utxo_state` batch instead).
    #[allow(clippy::too_many_arguments)]
    pub fn evm_validate_and_persist(
        header_store: &DbEvmHeaderStore,
        state_store: &DbEvmStateStore,
        payload_store: &DbEvmPayloadStore,
        batch: &mut WriteBatch,
        block: BlockHash,
        selected_parent: BlockHash,
        sorted_mergeset: &[BlockHash],
        l1_header: &Header,
        payload: &EvmExecutionPayload,
    ) -> Result<(), EvmValidateError> {
        let Some((header, snapshot)) =
            evm_validate(header_store, state_store, payload_store, block, selected_parent, sorted_mergeset, l1_header, payload)?
        else {
            return Ok(());
        };
        header_store.insert_batch(batch, block, header).map_err(EvmValidateError::Store)?;
        state_store.insert_batch(batch, block, snapshot).map_err(EvmValidateError::Store)?;
        Ok(())
    }
}

#[cfg(feature = "evm")]
pub use driver::{evm_validate, evm_validate_and_persist, EvmStaged, EvmValidateError};

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

        // First EVM block on genesis: the driver reads the parent's state as
        // absent => the empty (genesis) snapshot — no seeding needed.
        let selected_parent = Hash64::from_bytes([0xAA; 64]);
        let merged = Hash64::from_bytes([0xBB; 64]);
        let payload = EvmExecutionPayload {
            system_ops: vec![EvmSystemOp::DepositClaim(DepositClaim {
                deposit_outpoint: Default::default(),
                evm_address: EvmAddress::from_bytes([0xCC; 20]),
                amount_sompi: 7,
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
        evm_validate_and_persist(&header_store, &state_store, &payload_store, &mut b1, l1.hash, selected_parent, &mergeset, &l1, &payload)
            .unwrap();
        db.write(b1).unwrap();
        assert_eq!(header_store.get(l1.hash).unwrap(), expected.header);
        assert_eq!(expected.applied_deposit_claims.len(), 1, "the deposit claim was applied");

        // No-replay: re-driving is a no-op (the already-stored result is reused).
        let mut b2 = WriteBatch::default();
        evm_validate_and_persist(&header_store, &state_store, &payload_store, &mut b2, l1.hash, selected_parent, &mergeset, &l1, &payload)
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
