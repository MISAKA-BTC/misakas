//! **Bridge audit BR-1 regression (2026-09-25; node-only): a peer-chosen EVM payload never condemns
//! a block id.**
//!
//! Ported from the audit reproducers `audit_bridge_a3_1_{v1_foreign_payload,v2_mismatched_payload}_
//! poisons_a_valid_block` (branch `audit/bridge-0925`, 9d4f7274b), which PASSED while asserting the
//! defect: the block id is the header hash, which commits to the EVM payload only through the v2
//! `evm_payload_hash` (a pre-v2 header commits to "no payload", its two EVM fields outside the
//! hash), so the first relayer chose the payload bytes, the body processor persisted
//! `StatusInvalid`, the honest delivery then got `KnownInvalid` and every descendant
//! `InvalidParent` — one message, a permanent partition. Inverted here: the forged delivery is
//! refused but leaves the block `StatusHeaderOnly` (the `BadMerkleRoot` treatment), the honest body
//! and the child are accepted, and a payload the id DOES commit to is still judged in full.
//!
//! Run: `cargo test -p kaspa-consensus --lib br1_` (the v2 test needs `--features evm`).

use crate::{consensus::test_consensus::TestConsensus, errors::RuleError};
use kaspa_consensus_core::{
    api::ConsensusApi,
    blockstatus::BlockStatus,
    coinbase::MinerData,
    config::{ConfigBuilder, params::MAINNET_PARAMS},
    constants::EVM_HEADER_VERSION,
    evm::{EvmExecutionPayload, evm_payload_matches_header},
    mldsa87_primitives::p2pkh_mldsa87_spk,
};
use kaspa_core::assert_match;
use kaspa_hashes::Hash64;

fn miner() -> MinerData {
    MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![])
}

/// Pre-EVM header (v1) — the mainnet preset's shape (`evm_activation_daa_score = u64::MAX`). Neither
/// a foreign payload nor non-zero hash-invisible EVM header fields on a delivered copy of a valid
/// v1 block condemn it. Feature-independent.
#[tokio::test]
async fn br1_v1_foreign_payload_or_stray_evm_fields_leave_the_block_retryable() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    assert_eq!(config.params.evm_activation_daa_score, u64::MAX, "mainnet preset: EVM inert, v1 headers");

    // The block the network produced.
    let producer = TestConsensus::new(&config);
    let producer_handles = producer.init();
    let honest = producer.build_utxo_valid_block_with_parents(1.into(), vec![config.genesis.hash], miner(), vec![]);
    assert!(honest.header.version < EVM_HEADER_VERSION);
    assert!(honest.evm_payload.is_empty());

    // Control: a node that receives the honest body accepts it.
    let control = TestConsensus::new(&config);
    let control_handles = control.init();
    control.validate_and_insert_block(honest.clone().to_immutable()).virtual_state_task.await.expect("control: the block is valid");
    assert_eq!(control.block_status(1.into()), BlockStatus::StatusUTXOValid);

    // The upstream carve-out: a relay that tampers with the TRANSACTIONS does not poison. (This
    // also leaves the victim holding the header, so the next deliveries skip header validation.)
    let victim = TestConsensus::new(&config);
    let victim_handles = victim.init();
    let mut bad_txs = honest.clone();
    bad_txs.transactions[0].version += 1;
    assert_match!(victim.validate_and_insert_block(bad_txs.to_immutable()).virtual_state_task.await, Err(RuleError::BadMerkleRoot(_, _)));
    assert_eq!(victim.block_status(1.into()), BlockStatus::StatusHeaderOnly, "BadMerkleRoot leaves the block retryable");

    // Attack 1: a relayer attaches a payload. Same header ⇒ same block id.
    let mut forged = honest.clone();
    forged.evm_payload = EvmExecutionPayload { extra_data: vec![1], ..Default::default() };
    assert_eq!(forged.header.hash, honest.header.hash);
    assert_eq!(kaspa_consensus_core::hashing::header::hash(&forged.header), kaspa_consensus_core::hashing::header::hash(&honest.header));
    assert!(!evm_payload_matches_header(&forged.header, &forged.evm_payload), "the ingress check names the forgery");
    assert_match!(
        victim.validate_and_insert_block(forged.to_immutable()).virtual_state_task.await,
        Err(RuleError::NonEmptyEvmPayloadBeforeActivation)
    );
    // FIXED (BR-1; was StatusInvalid): the delivery is refused, the block id is not condemned.
    assert_eq!(victim.block_status(1.into()), BlockStatus::StatusHeaderOnly, "BR-1: a relay-chosen payload leaves the block retryable");

    // Attack 2: a relayer sets the v1 header's hash-invisible EVM fields. The id does not move, and
    // since the header is already held, the delivered copy reaches the body rule directly.
    let mut stray = honest.clone();
    stray.header.evm_commitment_root = Hash64::from_bytes([0x5A; 64]);
    assert_eq!(kaspa_consensus_core::hashing::header::hash(&stray.header), kaspa_consensus_core::hashing::header::hash(&honest.header));
    assert!(!evm_payload_matches_header(&stray.header, &stray.evm_payload), "the ingress check names the forgery");
    assert_match!(
        victim.validate_and_insert_block(stray.to_immutable()).virtual_state_task.await,
        Err(RuleError::NonZeroEvmHeaderFieldsBeforeActivation)
    );
    assert_eq!(victim.block_status(1.into()), BlockStatus::StatusHeaderOnly, "BR-1: relay-chosen EVM header fields leave the block retryable");

    // The honest body is accepted afterwards (was KnownInvalid) …
    assert!(evm_payload_matches_header(&honest.header, &honest.evm_payload));
    victim.validate_and_insert_block(honest.clone().to_immutable()).virtual_state_task.await.expect("BR-1: the honest body is accepted");
    assert_eq!(victim.block_status(1.into()), BlockStatus::StatusUTXOValid);
    // … and so is a descendant (was InvalidParent), exactly as on the control node.
    producer.validate_and_insert_block(honest.clone().to_immutable()).virtual_state_task.await.expect("producer: b1 is valid");
    let child = producer.build_utxo_valid_block_with_parents(2.into(), vec![1.into()], miner(), vec![]);
    control.validate_and_insert_block(child.clone().to_immutable()).virtual_state_task.await.expect("control: the child is valid");
    assert_eq!(control.block_status(2.into()), BlockStatus::StatusUTXOValid);
    victim.validate_and_insert_block(child.to_immutable()).virtual_state_task.await.expect("BR-1: the child is valid");
    assert_eq!(victim.block_status(2.into()), BlockStatus::StatusUTXOValid);

    victim.shutdown(victim_handles);
    control.shutdown(control_handles);
    producer.shutdown(producer_handles);
}

#[cfg(feature = "evm")]
mod evm {
    use super::*;
    use kaspa_consensus_core::{
        BlockHash,
        block::MutableBlock,
        evm::{EvmExecutionHeader, EvmStateSnapshot, MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK},
    };
    use kaspa_evm::EvmBlockInput;

    type EvmRow = (EvmExecutionHeader, EvmStateSnapshot);

    fn evm_config() -> kaspa_consensus_core::config::Config {
        ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().edit_consensus_params(|p| p.evm_activation_daa_score = 0).build()
    }

    /// A v2 block with an empty payload whose `evm_commitment_root` is the real acceptance result
    /// over `evm_parent` (the producer computes it the way the verifier will) — the shape
    /// `evm_active_chain_executes_persists_and_moves_heads` uses.
    fn evm_block(consensus: &TestConsensus, hash: u64, parent: BlockHash, evm_parent: Option<&EvmRow>) -> (MutableBlock, EvmRow) {
        let payload = EvmExecutionPayload::default();
        let mut b = consensus.build_utxo_valid_block_with_parents(hash.into(), vec![parent], miner(), vec![]);
        b.header.version = EVM_HEADER_VERSION;
        b.header.evm_payload_hash = payload.payload_hash();
        let empty = EvmStateSnapshot::default();
        let (parent_header, parent_snapshot) = match evm_parent {
            Some((h, s)) => (Some(h), s),
            None => (None, &empty),
        };
        let input = EvmBlockInput {
            market: Default::default(),
            parent: parent_header,
            header_timestamp_ms: b.header.timestamp,
            selected_parent_hash: parent.as_bytes(),
            blue_work_be: b.header.blue_work.to_be_bytes().to_vec(),
            daa_score: b.header.daa_score,
            payload: &payload,
            accepted_txs: &[],
            gas_pool_v2_activation_daa_score: u64::MAX,
            f002_withdraw_cap_activation_daa_score: u64::MAX,
            bridge_ledger_activation_daa_score: u64::MAX,
            f003_mldsa_verify_activation_daa_score: u64::MAX,
            typed_receipt_root_activation_daa_score: u64::MAX,
            user_gas_cap: kaspa_consensus_core::evm::MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK,
        };
        let (res, snap) = kaspa_evm::snapshot::execute_block_from_snapshot(parent_snapshot, &input).unwrap();
        b.header.evm_commitment_root = res.header.commitment_root();
        b.evm_payload = payload;
        (b, (res.header, snap))
    }

    /// EVM-active header (v2) — testnet-11 / testnet-12's shape (both arm the lane at DAA 0, so
    /// every post-genesis header is v2). A payload that does not hash to the header's
    /// `evm_payload_hash` — including an over-cap one — is a delivery fault; a payload the id
    /// commits to is still judged, and an over-cap committed payload still condemns its block.
    #[tokio::test]
    async fn br1_v2_mismatched_payload_leaves_the_block_retryable_and_a_committed_one_is_still_judged() {
        let config = evm_config();
        let genesis = config.genesis.hash;

        // The producer (also the control): b1 and its child b2 are valid on a node that received
        // the honest bodies.
        let producer = TestConsensus::new(&config);
        let producer_handles = producer.init();
        let (honest, b1_row) = evm_block(&producer, 1, genesis, None);
        assert_eq!(honest.header.version, EVM_HEADER_VERSION);
        producer.validate_and_insert_block(honest.clone().to_immutable()).virtual_state_task.await.expect("control: b1 is valid");
        assert_eq!(producer.block_status(1.into()), BlockStatus::StatusUTXOValid);
        let (child, _) = evm_block(&producer, 2, 1.into(), Some(&b1_row));
        producer.validate_and_insert_block(child.clone().to_immutable()).virtual_state_task.await.expect("control: b2 is valid");

        // The attack: same header (so the same block id), a payload of the relayer's choosing.
        let victim = TestConsensus::new(&config);
        let victim_handles = victim.init();
        let mut forged = honest.clone();
        forged.evm_payload = EvmExecutionPayload { extra_data: vec![1], ..Default::default() };
        assert_eq!(forged.header.hash, honest.header.hash, "the header — and so the block id — is untouched");
        assert!(!evm_payload_matches_header(&forged.header, &forged.evm_payload), "the ingress check names the forgery");
        assert_match!(
            victim.validate_and_insert_block(forged.to_immutable()).virtual_state_task.await,
            Err(RuleError::EvmPayloadHashMismatch)
        );
        // FIXED (BR-1; was StatusInvalid → KnownInvalid → InvalidParent).
        assert_eq!(victim.block_status(1.into()), BlockStatus::StatusHeaderOnly, "BR-1: a relay-chosen payload leaves the block retryable");
        victim.validate_and_insert_block(honest.clone().to_immutable()).virtual_state_task.await.expect("BR-1: the honest body is accepted");
        assert_eq!(victim.block_status(1.into()), BlockStatus::StatusUTXOValid);
        victim.validate_and_insert_block(child.to_immutable()).virtual_state_task.await.expect("BR-1: the child is valid");
        assert_eq!(victim.block_status(2.into()), BlockStatus::StatusUTXOValid);

        // An over-cap FORGED payload under the honest header: the hash is checked before the cap,
        // so it is a mismatch (a delivery fault), not a verdict on the id.
        let victim2 = TestConsensus::new(&config);
        let victim2_handles = victim2.init();
        let oversized = EvmExecutionPayload { extra_data: vec![0u8; MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK + 1], ..Default::default() };
        let mut forged_big = honest.clone();
        forged_big.evm_payload = oversized.clone();
        assert_match!(
            victim2.validate_and_insert_block(forged_big.to_immutable()).virtual_state_task.await,
            Err(RuleError::EvmPayloadHashMismatch)
        );
        assert_eq!(victim2.block_status(1.into()), BlockStatus::StatusHeaderOnly, "BR-1: an over-cap FORGED payload is a delivery fault");
        victim2.validate_and_insert_block(honest.clone().to_immutable()).virtual_state_task.await.expect("BR-1: the honest body is accepted");

        // Unchanged verdict: a block whose id COMMITS to an over-cap payload is itself invalid, and
        // stays condemned — the exemption covers only payloads the id does not commit to.
        let mut committed_big = honest.clone();
        committed_big.header.hash = 3.into(); // a distinct id (test consensus uses precomputed ids)
        committed_big.header.evm_payload_hash = oversized.payload_hash();
        committed_big.evm_payload = oversized;
        assert!(evm_payload_matches_header(&committed_big.header, &committed_big.evm_payload), "the id commits to it");
        assert_match!(
            victim2.validate_and_insert_block(committed_big.clone().to_immutable()).virtual_state_task.await,
            Err(RuleError::EvmPayloadTooLarge(_, _))
        );
        assert_eq!(victim2.block_status(3.into()), BlockStatus::StatusInvalid, "a committed over-cap payload condemns its block, as before");
        assert_match!(
            victim2.validate_and_insert_block(committed_big.to_immutable()).virtual_state_task.await,
            Err(RuleError::KnownInvalid)
        );
        victim2.shutdown(victim2_handles);

        victim.shutdown(victim_handles);
        producer.shutdown(producer_handles);
    }
}
