//! **Bridge audit BR-1 (2026-09-25; node-only): a delivered EVM payload is checked against the
//! block id before consensus sees it.**
//!
//! The block id is the header hash, and the id commits to the EVM payload only through the v2
//! header's `evm_payload_hash` (a pre-v2 header commits to "no payload", with its two EVM fields
//! zero and outside the hash). So whoever delivers a body first — the first relayer, an IBD sync
//! peer, an RPC client — chooses the payload bytes under a genuine block id. Such a delivery is
//! the sender's fault, not the block's: every ingress refuses it here and blames the peer, and the
//! body processor (the second line) never persists `StatusInvalid` for the matching rule errors,
//! so the honest body is still accepted when it arrives.
//!
//! The predicate is [`kaspa_consensus_core::evm::evm_payload_matches_header`] — the same check the
//! body rule `check_evm_payload` runs first — so no body consensus accepts is refused here.

use kaspa_consensus_core::{block::Block, constants::EVM_HEADER_VERSION, errors::block::RuleError, evm::evm_payload_matches_header};
use kaspa_p2p_lib::common::ProtocolError;

/// The body rule error consensus reports for a payload the block id does not commit to, or `None`
/// when the delivered payload is the committed one.
pub fn evm_payload_delivery_fault(block: &Block) -> Option<RuleError> {
    if evm_payload_matches_header(&block.header, &block.evm_payload) {
        None
    } else if block.header.version >= EVM_HEADER_VERSION {
        Some(RuleError::EvmPayloadHashMismatch)
    } else if !block.evm_payload.is_empty() {
        Some(RuleError::NonEmptyEvmPayloadBeforeActivation)
    } else {
        Some(RuleError::NonZeroEvmHeaderFieldsBeforeActivation)
    }
}

/// P2P ingress (relay, IBD bodies, trusted IBD entries): refuse a block whose EVM payload is not
/// the one its id commits to, as peer misbehaviour (the flow disconnects the sender). `block.header`
/// must be the header whose hash the requester asked for — the relay/IBD caller has already
/// checked `block.hash()`, or reassembled the block from its own stored header.
pub fn check_delivered_evm_payload(block: &Block) -> Result<(), ProtocolError> {
    match evm_payload_delivery_fault(block) {
        None => Ok(()),
        Some(fault) => Err(ProtocolError::MisbehavingPeer(format!(
            "sent block {} with an EVM payload its header does not commit to ({fault})",
            block.hash()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{evm::EvmExecutionPayload, header::Header};
    use kaspa_hashes::Hash64;
    use std::sync::Arc;

    fn block(header: Header, payload: EvmExecutionPayload) -> Block {
        Block::new(header, vec![]).with_evm_payload(Arc::new(payload))
    }

    /// The relay/IBD/RPC ingress refuses exactly the deliveries the block id does not commit to,
    /// and names the rule error consensus would (non-poisoning) report for each.
    #[test]
    fn br1_ingress_refuses_a_payload_the_id_does_not_commit_to() {
        let foreign = EvmExecutionPayload { extra_data: vec![1], ..Default::default() };

        // v1 (the mainnet preset's shape): only the empty payload with zero EVM fields.
        let v1 = Header::from_precomputed_hash(Hash64::from_u64_word(7), vec![]);
        assert!(v1.version < EVM_HEADER_VERSION);
        assert!(check_delivered_evm_payload(&block(v1.clone(), EvmExecutionPayload::default())).is_ok());
        let forged = block(v1.clone(), foreign.clone());
        assert_eq!(forged.hash(), v1.hash, "the payload does not move the id");
        assert!(matches!(evm_payload_delivery_fault(&forged), Some(RuleError::NonEmptyEvmPayloadBeforeActivation)));
        assert!(matches!(check_delivered_evm_payload(&forged), Err(ProtocolError::MisbehavingPeer(_))));
        let mut stray = v1.clone();
        stray.evm_commitment_root = Hash64::from_bytes([5; 64]);
        let forged = block(stray, EvmExecutionPayload::default());
        assert!(matches!(evm_payload_delivery_fault(&forged), Some(RuleError::NonZeroEvmHeaderFieldsBeforeActivation)));
        assert!(check_delivered_evm_payload(&forged).is_err());

        // v2 (testnet-11/12's shape): the payload that hashes to `evm_payload_hash`, nothing else.
        let mut v2 = v1.clone();
        v2.version = EVM_HEADER_VERSION;
        let v2 = v2.with_evm_payload_hash(foreign.payload_hash());
        assert!(check_delivered_evm_payload(&block(v2.clone(), foreign.clone())).is_ok());
        let forged = block(v2.clone(), EvmExecutionPayload::default());
        assert_eq!(forged.hash(), v2.hash);
        assert!(matches!(evm_payload_delivery_fault(&forged), Some(RuleError::EvmPayloadHashMismatch)));
        assert!(matches!(check_delivered_evm_payload(&forged), Err(ProtocolError::MisbehavingPeer(_))));
        // An over-cap payload under an honest header is a delivery fault too, never a verdict.
        let huge = EvmExecutionPayload {
            extra_data: vec![0u8; kaspa_consensus_core::evm::MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK + 1],
            ..Default::default()
        };
        assert!(matches!(evm_payload_delivery_fault(&block(v2, huge)), Some(RuleError::EvmPayloadHashMismatch)));
    }
}
