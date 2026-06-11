use super::error::ConversionError;
use super::header::{HeaderFormat, Versioned};
use crate::pb as protowire;
use kaspa_consensus_core::{block::Block, evm::EvmExecutionPayload, tx::Transaction};
type BlockBody = Vec<Transaction>;

/// kaspa-pq EVM Lane v0.4 (§3.1): decode the wire payload bytes (the canonical
/// borsh encoding — the exact bytes `evm_payload_hash` commits to). Empty bytes
/// = the empty payload (every pre-activation block). Body validation re-derives
/// `evm_payload_hash` from the decoded payload, so a tampered payload cannot
/// pass (the header field is in the v2 hash preimage).
///
/// §14.2 DoS gate (audit L1): the consensus byte cap is enforced BEFORE the
/// borsh decode. Borsh is canonical for these types (one encoding per value;
/// trailing bytes error), so wire length == the length `check_evm_payload`
/// measures — the early gate rejects exactly the payloads body validation
/// would reject, just without first paying a transient up-to-message-cap
/// allocation for a peer-supplied blob.
fn decode_evm_payload(bytes: &[u8]) -> Result<EvmExecutionPayload, ConversionError> {
    if bytes.is_empty() {
        return Ok(Default::default());
    }
    if bytes.len() > kaspa_consensus_core::evm::MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK {
        return Err(ConversionError::NoneValue);
    }
    borsh::from_slice::<EvmExecutionPayload>(bytes).map_err(|_| ConversionError::NoneValue)
}

#[inline]
fn encode_evm_payload(evm_payload: &EvmExecutionPayload) -> Vec<u8> {
    if evm_payload.is_empty() { Vec::new() } else { evm_payload.payload_bytes() }
}

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<(HeaderFormat, &Block)> for protowire::BlockMessage {
    fn from(value: (HeaderFormat, &Block)) -> Self {
        let (header_format, block) = value;
        Self {
            header: Some((header_format, block.header.as_ref()).into()),
            transactions: block.transactions.iter().map(|tx| tx.into()).collect(),
            // kaspa-pq EVM Lane v0.4 (§3.1): the payload travels as its
            // canonical borsh bytes (exactly what evmPayloadHash commits to).
            evm_payload: encode_evm_payload(&block.evm_payload),
        }
    }
}

// kaspa-pq EVM Lane v0.4 (§3.1): a block body travels WITH the block's own EVM
// payload — the body-only IBD requester reassembles `Block` from (stored header
// + this message), and on a v2 block a missing payload would fail the
// `evm_payload_hash` body rule and reject a VALID block. The payload rides as
// its canonical borsh bytes, exactly like `BlockMessage.evmPayload`.
impl From<(&BlockBody, &EvmExecutionPayload)> for protowire::BlockBodyMessage {
    fn from((block_body, evm_payload): (&BlockBody, &EvmExecutionPayload)) -> Self {
        Self {
            transactions: block_body.iter().map(|tx| tx.into()).collect(),
            evm_payload: encode_evm_payload(evm_payload),
        }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<Versioned<protowire::BlockMessage>> for Block {
    type Error = ConversionError;

    fn try_from(value: Versioned<protowire::BlockMessage>) -> Result<Self, Self::Error> {
        let Versioned(header_format, block) = value;
        let header = block.header.ok_or(ConversionError::NoneValue)?;
        let evm_payload = decode_evm_payload(&block.evm_payload)?;
        Ok(Self::new(
            Versioned(header_format, header).try_into()?,
            block.transactions.into_iter().map(|i| i.try_into()).collect::<Result<Vec<Transaction>, Self::Error>>()?,
        )
        .with_evm_payload(std::sync::Arc::new(evm_payload)))
    }
}

// kaspa-pq EVM Lane v0.4: the body-only IBD requester needs BOTH parts to
// reassemble a valid v2 block (see the From above).
impl TryFrom<protowire::BlockBodyMessage> for (BlockBody, EvmExecutionPayload) {
    type Error = ConversionError;
    fn try_from(body_message: protowire::BlockBodyMessage) -> Result<Self, Self::Error> {
        let blk_body: BlockBody =
            body_message.transactions.into_iter().map(|i| i.try_into()).collect::<Result<Vec<Transaction>, ConversionError>>()?;
        let evm_payload = decode_evm_payload(&body_message.evm_payload)?;
        Ok((blk_body, evm_payload))
    }
}

// NOTE: deliberately NO `TryFrom<BlockBodyMessage> for BlockBody` (transactions
// only) — such an impl silently drops the EVM payload, which is exactly the bug
// that rejected valid v2 blocks on the body-only IBD path. Always decode the
// tuple above.

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::evm::{DepositClaim, EvmAddress, EvmSystemOp};

    /// kaspa-pq EVM Lane v0.4 (§3.1): the body-only IBD path must round-trip the
    /// block's own EVM payload — a dropped payload on a v2 block fails the
    /// `evm_payload_hash` body rule and rejects a VALID block.
    #[test]
    fn block_body_message_roundtrips_evm_payload() {
        let payload = EvmExecutionPayload {
            system_ops: vec![EvmSystemOp::DepositClaim(DepositClaim {
                deposit_outpoint: Default::default(),
                evm_address: EvmAddress::from_bytes([0xAB; 20]),
                amount_sompi: 7,
                claim_tip_sompi: 1,
            })],
            transactions: vec![vec![0xEE; 16]],
            ..Default::default()
        };
        let body: BlockBody = vec![];

        // Non-empty payload survives the wire and hashes identically.
        let msg: protowire::BlockBodyMessage = (&body, &payload).into();
        assert!(!msg.evm_payload.is_empty());
        let (body2, payload2): (BlockBody, EvmExecutionPayload) = msg.try_into().unwrap();
        assert!(body2.is_empty());
        assert_eq!(payload2, payload);
        assert_eq!(payload2.payload_hash(), payload.payload_hash());

        // The empty payload travels as empty bytes (pre-activation form).
        let msg: protowire::BlockBodyMessage = (&body, &EvmExecutionPayload::default()).into();
        assert!(msg.evm_payload.is_empty());
        let (_, payload3): (BlockBody, EvmExecutionPayload) = msg.try_into().unwrap();
        assert!(payload3.is_empty());

        // Malformed payload bytes are a conversion error, not a panic.
        let bad = protowire::BlockBodyMessage { transactions: vec![], evm_payload: vec![0xFF, 0x01] };
        assert!(<(BlockBody, EvmExecutionPayload)>::try_from(bad).is_err());

        // §14.2 DoS gate: over-cap wire bytes are rejected BEFORE the borsh
        // decode (no transient allocation for a peer-supplied oversized blob).
        let oversized = protowire::BlockBodyMessage {
            transactions: vec![],
            evm_payload: vec![0u8; kaspa_consensus_core::evm::MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK + 1],
        };
        assert!(<(BlockBody, EvmExecutionPayload)>::try_from(oversized).is_err());
    }
}
