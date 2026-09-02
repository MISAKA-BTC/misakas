use crate::pb::kaspad_message::Payload as KaspadMessagePayload;

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
pub enum KaspadMessagePayloadType {
    Addresses = 0,
    Block,
    Transaction,
    BlockLocator,
    RequestAddresses,
    RequestRelayBlocks,
    RequestTransactions,
    IbdBlock,
    InvRelayBlock,
    InvTransactions,
    Ping,
    Pong,
    Verack,
    Version,
    TransactionNotFound,
    Reject,
    PruningPointUtxoSetChunk,
    RequestIbdBlocks,
    UnexpectedPruningPoint,
    IbdBlockLocator,
    IbdBlockLocatorHighestHash,
    RequestNextPruningPointUtxoSetChunk,
    DonePruningPointUtxoSetChunks,
    IbdBlockLocatorHighestHashNotFound,
    BlockWithTrustedData,
    DoneBlocksWithTrustedData,
    RequestPruningPointAndItsAnticone,
    BlockHeaders,
    RequestNextHeaders,
    DoneHeaders,
    RequestPruningPointUtxoSet,
    RequestHeaders,
    RequestBlockLocator,
    PruningPoints,
    RequestPruningPointProof,
    PruningPointProof,
    Ready,
    BlockWithTrustedDataV4,
    TrustedData,
    RequestIbdChainBlockLocator,
    IbdChainBlockLocator,
    RequestAntipast,
    RequestNextPruningPointAndItsAnticoneBlocks,
    BlockBody,
    RequestBlockBodies,
    InvEvmTransactions,
    RequestEvmTransactions,
    EvmTransaction,
    EvmTransactionNotFound,
    // kaspa-pq ADR-0022: pruned-IBD EVM + overlay snapshot transfer.
    RequestPruningPointEvmState,
    PruningPointEvmState,
    RequestPruningPointOverlaySnapshot,
    PruningPointOverlaySnapshot,
    RequestPruningPointPalwState,
    PruningPointPalwState,
    PalwTraceMaterialBroadcast,
    PalwSeatReceiptBroadcast,
    // The pull half of the material transport (protocol >= 104): request only — the serve side
    // rides PalwTraceMaterialBroadcast.
    PalwMaterialRequest,
    // ADR-0077 Decision 8 (protocol >= 105): one checkpoint interval's opening, asked of and
    // answered by the executor directly. Neither type is relayed — an opening is bytes exactly one
    // peer wanted.
    PalwIntervalOpeningRequest,
    PalwIntervalOpening,
    // kaspa-pq EVM Lane v0.4 (§14.2): pending EVM deposit-claim relay (protocol ≥ 101).
    InvEvmDepositClaims,
    RequestEvmDepositClaims,
    EvmDepositClaim,
    EvmDepositClaimNotFound,
    // kaspa-pq: chain-candidate summaries (protocol >= 103).
    RequestIbdCandidateSummary,
    IbdCandidateSummary,
}

impl From<&KaspadMessagePayload> for KaspadMessagePayloadType {
    fn from(payload: &KaspadMessagePayload) -> Self {
        match payload {
            KaspadMessagePayload::Addresses(_) => KaspadMessagePayloadType::Addresses,
            KaspadMessagePayload::Block(_) => KaspadMessagePayloadType::Block,
            KaspadMessagePayload::Transaction(_) => KaspadMessagePayloadType::Transaction,
            KaspadMessagePayload::BlockLocator(_) => KaspadMessagePayloadType::BlockLocator,
            KaspadMessagePayload::RequestAddresses(_) => KaspadMessagePayloadType::RequestAddresses,
            KaspadMessagePayload::RequestRelayBlocks(_) => KaspadMessagePayloadType::RequestRelayBlocks,
            KaspadMessagePayload::RequestTransactions(_) => KaspadMessagePayloadType::RequestTransactions,
            KaspadMessagePayload::IbdBlock(_) => KaspadMessagePayloadType::IbdBlock,
            KaspadMessagePayload::InvRelayBlock(_) => KaspadMessagePayloadType::InvRelayBlock,
            KaspadMessagePayload::InvTransactions(_) => KaspadMessagePayloadType::InvTransactions,
            KaspadMessagePayload::Ping(_) => KaspadMessagePayloadType::Ping,
            KaspadMessagePayload::Pong(_) => KaspadMessagePayloadType::Pong,
            KaspadMessagePayload::Verack(_) => KaspadMessagePayloadType::Verack,
            KaspadMessagePayload::Version(_) => KaspadMessagePayloadType::Version,
            KaspadMessagePayload::TransactionNotFound(_) => KaspadMessagePayloadType::TransactionNotFound,
            KaspadMessagePayload::Reject(_) => KaspadMessagePayloadType::Reject,
            KaspadMessagePayload::PruningPointUtxoSetChunk(_) => KaspadMessagePayloadType::PruningPointUtxoSetChunk,
            KaspadMessagePayload::RequestIbdBlocks(_) => KaspadMessagePayloadType::RequestIbdBlocks,
            KaspadMessagePayload::UnexpectedPruningPoint(_) => KaspadMessagePayloadType::UnexpectedPruningPoint,
            KaspadMessagePayload::IbdBlockLocator(_) => KaspadMessagePayloadType::IbdBlockLocator,
            KaspadMessagePayload::IbdBlockLocatorHighestHash(_) => KaspadMessagePayloadType::IbdBlockLocatorHighestHash,
            KaspadMessagePayload::RequestNextPruningPointUtxoSetChunk(_) => {
                KaspadMessagePayloadType::RequestNextPruningPointUtxoSetChunk
            }
            KaspadMessagePayload::DonePruningPointUtxoSetChunks(_) => KaspadMessagePayloadType::DonePruningPointUtxoSetChunks,
            KaspadMessagePayload::IbdBlockLocatorHighestHashNotFound(_) => {
                KaspadMessagePayloadType::IbdBlockLocatorHighestHashNotFound
            }
            KaspadMessagePayload::BlockWithTrustedData(_) => KaspadMessagePayloadType::BlockWithTrustedData,
            KaspadMessagePayload::DoneBlocksWithTrustedData(_) => KaspadMessagePayloadType::DoneBlocksWithTrustedData,
            KaspadMessagePayload::RequestPruningPointAndItsAnticone(_) => KaspadMessagePayloadType::RequestPruningPointAndItsAnticone,
            KaspadMessagePayload::BlockHeaders(_) => KaspadMessagePayloadType::BlockHeaders,
            KaspadMessagePayload::RequestNextHeaders(_) => KaspadMessagePayloadType::RequestNextHeaders,
            KaspadMessagePayload::DoneHeaders(_) => KaspadMessagePayloadType::DoneHeaders,
            KaspadMessagePayload::RequestPruningPointUtxoSet(_) => KaspadMessagePayloadType::RequestPruningPointUtxoSet,
            KaspadMessagePayload::RequestHeaders(_) => KaspadMessagePayloadType::RequestHeaders,
            KaspadMessagePayload::RequestBlockLocator(_) => KaspadMessagePayloadType::RequestBlockLocator,
            KaspadMessagePayload::PruningPoints(_) => KaspadMessagePayloadType::PruningPoints,
            KaspadMessagePayload::RequestPruningPointProof(_) => KaspadMessagePayloadType::RequestPruningPointProof,
            KaspadMessagePayload::PruningPointProof(_) => KaspadMessagePayloadType::PruningPointProof,
            KaspadMessagePayload::Ready(_) => KaspadMessagePayloadType::Ready,
            KaspadMessagePayload::BlockWithTrustedDataV4(_) => KaspadMessagePayloadType::BlockWithTrustedDataV4,
            KaspadMessagePayload::TrustedData(_) => KaspadMessagePayloadType::TrustedData,
            KaspadMessagePayload::RequestIbdChainBlockLocator(_) => KaspadMessagePayloadType::RequestIbdChainBlockLocator,
            KaspadMessagePayload::IbdChainBlockLocator(_) => KaspadMessagePayloadType::IbdChainBlockLocator,
            KaspadMessagePayload::RequestAntipast(_) => KaspadMessagePayloadType::RequestAntipast,
            KaspadMessagePayload::RequestNextPruningPointAndItsAnticoneBlocks(_) => {
                KaspadMessagePayloadType::RequestNextPruningPointAndItsAnticoneBlocks
            }
            KaspadMessagePayload::BlockBody(_) => KaspadMessagePayloadType::BlockBody,
            KaspadMessagePayload::RequestBlockBodies(_) => KaspadMessagePayloadType::RequestBlockBodies,
            KaspadMessagePayload::InvEvmTransactions(_) => KaspadMessagePayloadType::InvEvmTransactions,
            KaspadMessagePayload::RequestEvmTransactions(_) => KaspadMessagePayloadType::RequestEvmTransactions,
            KaspadMessagePayload::EvmTransaction(_) => KaspadMessagePayloadType::EvmTransaction,
            KaspadMessagePayload::EvmTransactionNotFound(_) => KaspadMessagePayloadType::EvmTransactionNotFound,
            KaspadMessagePayload::RequestPruningPointEvmState(_) => KaspadMessagePayloadType::RequestPruningPointEvmState,
            KaspadMessagePayload::PruningPointEvmState(_) => KaspadMessagePayloadType::PruningPointEvmState,
            KaspadMessagePayload::RequestPruningPointOverlaySnapshot(_) => {
                KaspadMessagePayloadType::RequestPruningPointOverlaySnapshot
            }
            KaspadMessagePayload::PruningPointOverlaySnapshot(_) => KaspadMessagePayloadType::PruningPointOverlaySnapshot,
            KaspadMessagePayload::RequestPruningPointPalwState(_) => KaspadMessagePayloadType::RequestPruningPointPalwState,
            KaspadMessagePayload::PruningPointPalwState(_) => KaspadMessagePayloadType::PruningPointPalwState,
            KaspadMessagePayload::PalwTraceMaterialBroadcast(_) => KaspadMessagePayloadType::PalwTraceMaterialBroadcast,
            KaspadMessagePayload::PalwSeatReceiptBroadcast(_) => KaspadMessagePayloadType::PalwSeatReceiptBroadcast,
            KaspadMessagePayload::PalwMaterialRequest(_) => KaspadMessagePayloadType::PalwMaterialRequest,
            KaspadMessagePayload::PalwIntervalOpeningRequest(_) => KaspadMessagePayloadType::PalwIntervalOpeningRequest,
            KaspadMessagePayload::PalwIntervalOpening(_) => KaspadMessagePayloadType::PalwIntervalOpening,
            KaspadMessagePayload::InvEvmDepositClaims(_) => KaspadMessagePayloadType::InvEvmDepositClaims,
            KaspadMessagePayload::RequestEvmDepositClaims(_) => KaspadMessagePayloadType::RequestEvmDepositClaims,
            KaspadMessagePayload::EvmDepositClaim(_) => KaspadMessagePayloadType::EvmDepositClaim,
            KaspadMessagePayload::EvmDepositClaimNotFound(_) => KaspadMessagePayloadType::EvmDepositClaimNotFound,
            KaspadMessagePayload::RequestIbdCandidateSummary(_) => KaspadMessagePayloadType::RequestIbdCandidateSummary,
            KaspadMessagePayload::IbdCandidateSummary(_) => KaspadMessagePayloadType::IbdCandidateSummary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pb::{KaspadMessage, PalwIntervalOpeningMessage, PalwIntervalOpeningRequestMessage, PalwMaterialRequestMessage};
    use prost::Message;

    /// **The interval lane's two messages survive the wire and route to their own types**
    /// (ADR-0077 Decision 8). A oneof number that decodes into the wrong arm, or a payload type
    /// the router cannot name, is a disconnect on the first honest request — and that failure is
    /// silent at the asker, which simply never hears an opening and accuses the executor at the
    /// half-window. So the round trip is pinned here, field by field, including the SA-2
    /// authentication fields, whose absence is what "unsigned" means on the wire.
    #[test]
    fn the_interval_opening_messages_round_trip_and_route_to_their_own_payload_types() {
        let claim = crate::pb::Hash { bytes: vec![0xAB; 64] };
        let request = KaspadMessage {
            payload: Some(KaspadMessagePayload::PalwIntervalOpeningRequest(PalwIntervalOpeningRequestMessage {
                claim_id: Some(claim.clone()),
                interval_index: 645,
                requester_pubkey: vec![7u8; 2_592],
                signature: vec![9u8; 4_627],
                requested_daa: 1_234_567,
            })),
            response_id: 0,
            request_id: 0,
        };
        let decoded = KaspadMessage::decode(request.encode_to_vec().as_slice()).expect("the request decodes");
        let Some(KaspadMessagePayload::PalwIntervalOpeningRequest(inner)) = &decoded.payload else {
            panic!("the request decoded into another arm: {:?}", decoded.payload);
        };
        assert_eq!(inner.claim_id.as_ref(), Some(&claim));
        assert_eq!(inner.interval_index, 645);
        assert_eq!(inner.requester_pubkey.len(), 2_592);
        assert_eq!(inner.signature.len(), 4_627);
        assert_eq!(inner.requested_daa, 1_234_567);
        assert_eq!(
            KaspadMessagePayloadType::from(decoded.payload.as_ref().unwrap()),
            KaspadMessagePayloadType::PalwIntervalOpeningRequest
        );

        let opening = KaspadMessage {
            payload: Some(KaspadMessagePayload::PalwIntervalOpening(PalwIntervalOpeningMessage {
                claim_id: Some(claim.clone()),
                interval_index: 20,
                opening: vec![1, 2, 3, 4, 5],
            })),
            response_id: 0,
            request_id: 0,
        };
        let decoded = KaspadMessage::decode(opening.encode_to_vec().as_slice()).expect("the opening decodes");
        let Some(KaspadMessagePayload::PalwIntervalOpening(inner)) = &decoded.payload else {
            panic!("the opening decoded into another arm: {:?}", decoded.payload);
        };
        assert_eq!(inner.claim_id.as_ref(), Some(&claim));
        assert_eq!(inner.interval_index, 20);
        assert_eq!(inner.opening, vec![1, 2, 3, 4, 5]);
        assert_eq!(KaspadMessagePayloadType::from(decoded.payload.as_ref().unwrap()), KaspadMessagePayloadType::PalwIntervalOpening);

        // Two different types, and neither is the material pull's — the router keys its
        // subscriptions on this value, so a collision here is a message delivered to the wrong
        // flow.
        assert_ne!(KaspadMessagePayloadType::PalwIntervalOpeningRequest, KaspadMessagePayloadType::PalwIntervalOpening);
        assert_ne!(KaspadMessagePayloadType::PalwIntervalOpeningRequest, KaspadMessagePayloadType::PalwMaterialRequest);
    }

    /// **A pre-5f material request still decodes, and decodes as UNSIGNED** (ADR-0077 SA-2 on the
    /// existing pull). The three authentication fields were appended to a shipped message, so an
    /// old peer's one-field request must remain readable — and must arrive with empty key, empty
    /// signature and a zero DAA, because that triple is exactly what the serving node's shape
    /// check refuses by name. If absence decoded to anything else the refusal would be silent.
    #[test]
    fn a_material_request_without_the_signature_fields_decodes_as_unsigned() {
        // Encoded by hand as a peer that has never heard of fields 2-4 would: field 1 only.
        let legacy = PalwMaterialRequestMessage {
            claim_id: Some(crate::pb::Hash { bytes: vec![0x11; 64] }),
            requester_pubkey: Vec::new(),
            signature: Vec::new(),
            requested_daa: 0,
        };
        let bytes = legacy.encode_to_vec();
        let decoded = PalwMaterialRequestMessage::decode(bytes.as_slice()).expect("the legacy request decodes");
        assert!(decoded.requester_pubkey.is_empty(), "absent means unsigned");
        assert!(decoded.signature.is_empty(), "absent means unsigned");
        assert_eq!(decoded.requested_daa, 0);
        assert_eq!(decoded.claim_id.map(|h| h.bytes.len()), Some(64));
    }
}
