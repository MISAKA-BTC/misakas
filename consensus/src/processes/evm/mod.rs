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
pub use kaspa_evm::{execute_block_evm, EvmBlockInput};

#[cfg(feature = "evm")]
mod driver {
    use crate::model::stores::evm::{DbEvmHeaderStore, DbEvmStateStore, EvmHeaderStore, EvmHeaderStoreReader, EvmStateStore, EvmStateStoreReader};
    use kaspa_consensus_core::evm::{EvmExecutionPayload, EvmStateSnapshot, EVM_GENESIS_STATE_ROOT};
    use kaspa_consensus_core::header::Header;
    use kaspa_consensus_core::BlockHash;
    use kaspa_database::prelude::StoreError;
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

    /// The lazy chain-context EVM step (design §6.1): re-execute a selected-chain
    /// block's EVM lane against its `selected_parent`'s committed state, verify the
    /// `evm_commitment_root`, and stage the resulting header + child state snapshot
    /// into `batch` (committed atomically with the block's UTXO diff by the caller).
    ///
    /// No-replay (design §2.1/§10.1): if this block's EVM result is already stored,
    /// returns `Ok(())` without re-executing — a virtual reorg only moves head
    /// pointers, it never recomputes a block's EVM state. The genesis EVM state
    /// (`EVM_GENESIS_STATE_ROOT`, empty snapshot) is the implicit parent of the
    /// first EVM block.
    #[allow(clippy::too_many_arguments)]
    pub fn evm_validate_and_persist(
        header_store: &DbEvmHeaderStore,
        state_store: &DbEvmStateStore,
        batch: &mut WriteBatch,
        block: BlockHash,
        selected_parent: BlockHash,
        l1_header: &Header,
        payload: &EvmExecutionPayload,
    ) -> Result<(), EvmValidateError> {
        // No-replay: this block's EVM result was computed when it first joined the
        // selected chain; never recompute it.
        if header_store.has(block).map_err(EvmValidateError::Store)? {
            return Ok(());
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
        };

        let (result, child_snapshot) =
            kaspa_evm::snapshot::execute_block_from_snapshot(&parent_snapshot, &input).map_err(|e| EvmValidateError::Exec(e.to_string()))?;

        // The only block-invalidating EVM condition: producer commitment mismatch
        // (user tx failures are status-0 receipts inside `result`, design §6.3).
        if result.header.commitment_root() != l1_header.evm_commitment_root {
            return Err(EvmValidateError::CommitmentMismatch { block });
        }

        header_store.insert_batch(batch, block, result.header).map_err(EvmValidateError::Store)?;
        state_store.insert_batch(batch, block, child_snapshot).map_err(EvmValidateError::Store)?;
        Ok(())
    }
}

#[cfg(feature = "evm")]
pub use driver::{evm_validate_and_persist, EvmValidateError};

#[cfg(all(test, feature = "evm"))]
mod tests {
    use super::*;
    use crate::model::stores::evm::{DbEvmHeaderStore, DbEvmStateStore, EvmHeaderStoreReader};
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

    /// A deposit-claim-only block (no user txs) exercises the driver's full
    /// validate → persist → no-replay → mismatch path without pulling alloy/revm
    /// into the consensus test (`DepositClaim` is a consensus-core type; the
    /// executor reaches kaspa-evm only through `execute_block_from_snapshot`).
    #[test]
    fn driver_validates_persists_and_never_replays() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let header_store = DbEvmHeaderStore::new(db.clone(), CachePolicy::Empty);
        let state_store = DbEvmStateStore::new(db.clone(), CachePolicy::Empty);

        // First EVM block on genesis: the driver reads the parent's state as
        // absent ⇒ the empty (genesis) snapshot — no seeding needed.
        let selected_parent = Hash64::from_bytes([0xAA; 64]);
        let payload = EvmExecutionPayload {
            system_ops: vec![EvmSystemOp::DepositClaim(DepositClaim {
                deposit_outpoint: Default::default(),
                evm_address: EvmAddress::from_bytes([0xCC; 20]),
                amount_sompi: 7,
            })],
            ..Default::default()
        };

        // Pre-compute the expected commitment with the exact env the driver derives.
        let l1 = header(7_000, 9);
        let input = EvmBlockInput {
            parent: None,
            header_timestamp_ms: l1.timestamp,
            selected_parent_hash: selected_parent.as_bytes(),
            blue_work_be: l1.blue_work.to_be_bytes().to_vec(),
            daa_score: l1.daa_score,
            payload: &payload,
        };
        let (expected, _) = kaspa_evm::snapshot::execute_block_from_snapshot(&EvmStateSnapshot::default(), &input).unwrap();
        let l1 = l1.with_evm_commitment(expected.header.commitment_root());

        // Drive: validates the commitment and persists header + child state.
        let mut b1 = WriteBatch::default();
        evm_validate_and_persist(&header_store, &state_store, &mut b1, l1.hash, selected_parent, &l1, &payload).unwrap();
        db.write(b1).unwrap();
        assert_eq!(header_store.get(l1.hash).unwrap(), expected.header);
        assert_eq!(expected.applied_deposit_claims.len(), 1, "the deposit claim was applied");

        // No-replay: re-driving is a no-op (the already-stored result is reused).
        let mut b2 = WriteBatch::default();
        evm_validate_and_persist(&header_store, &state_store, &mut b2, l1.hash, selected_parent, &l1, &payload).unwrap();

        // A wrong commitment for a fresh block ⇒ block-invalid.
        let bad = header(8_000, 10).with_evm_commitment(Hash64::from_bytes([0xEE; 64]));
        let mut b3 = WriteBatch::default();
        let err = evm_validate_and_persist(&header_store, &state_store, &mut b3, bad.hash, selected_parent, &bad, &payload);
        assert!(matches!(err, Err(EvmValidateError::CommitmentMismatch { .. })));
    }
}
