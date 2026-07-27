use super::{
    error::ConversionError,
    header::Versioned,
    model::{
        trusted::{TrustedDataEntry, TrustedDataPackage},
        version::Version,
    },
    option::TryIntoOptionEx,
};
use crate::pb as protowire;
use kaspa_consensus_core::{BlockHash, Hash64}; // PR-9.5e: p2p block-hash convert sites widened to Hash64
use kaspa_consensus_core::{
    block::Block,
    header::Header,
    pruning::{PruningPointProof, PruningPointsList},
    tx::{TransactionId, TransactionOutpoint, UtxoEntry},
};
use kaspa_utils::networking::{IpAddress, PeerId};

use std::{collections::HashMap, sync::Arc};

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<Version> for protowire::VersionMessage {
    fn from(item: Version) -> Self {
        Self {
            protocol_version: item.protocol_version,
            services: item.services,
            timestamp: item.timestamp as i64,
            address: item.address.map(|x| x.into()),
            id: item.id.as_bytes().to_vec(),
            user_agent: item.user_agent,
            disable_relay_tx: item.disable_relay_tx,
            subnetwork_id: item.subnetwork_id.map(|x| x.into()),
            network: item.network.clone(),
        }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::VersionMessage> for Version {
    type Error = ConversionError;
    fn try_from(msg: protowire::VersionMessage) -> Result<Self, Self::Error> {
        Ok(Self {
            protocol_version: msg.protocol_version,
            services: msg.services,
            timestamp: msg.timestamp as u64,
            address: msg.address.map(TryInto::try_into).transpose()?,
            id: PeerId::from_slice(&msg.id)?,
            user_agent: msg.user_agent.clone(),
            disable_relay_tx: msg.disable_relay_tx,
            subnetwork_id: msg.subnetwork_id.map(TryInto::try_into).transpose()?,
            network: msg.network.clone(),
        })
    }
}

impl TryFrom<protowire::RequestHeadersMessage> for (BlockHash, BlockHash) {
    type Error = ConversionError;
    fn try_from(msg: protowire::RequestHeadersMessage) -> Result<Self, Self::Error> {
        Ok((msg.high_hash.try_into_ex()?, msg.low_hash.try_into_ex()?))
    }
}

impl TryFrom<protowire::RequestIbdChainBlockLocatorMessage> for (Option<BlockHash>, Option<BlockHash>) {
    type Error = ConversionError;
    fn try_from(msg: protowire::RequestIbdChainBlockLocatorMessage) -> Result<Self, Self::Error> {
        let low = match msg.low_hash {
            Some(low) => Some(low.try_into()?),
            None => None,
        };

        let high = match msg.high_hash {
            Some(high) => Some(high.try_into()?),
            None => None,
        };

        Ok((low, high))
    }
}

impl TryFrom<Versioned<protowire::PruningPointProofMessage>> for PruningPointProof {
    type Error = ConversionError;
    fn try_from(value: Versioned<protowire::PruningPointProofMessage>) -> Result<Self, Self::Error> {
        let Versioned(header_format, msg) = value;
        // The pruning proof can contain many duplicate headers (across levels), so we use a local cache in order
        // to make sure we hold a single Arc per header
        let mut cache: HashMap<BlockHash, Arc<Header>> = HashMap::with_capacity(4000);
        msg.headers
            .into_iter()
            .map(|level| {
                level
                    .headers
                    .into_iter()
                    .map(|x| {
                        let header: Header = Versioned(header_format, x).try_into()?;
                        // Clone the existing Arc if found
                        Ok(cache.entry(header.hash).or_insert_with(|| Arc::new(header)).clone())
                    })
                    .collect()
            })
            .collect()
    }
}

impl TryFrom<Versioned<protowire::PruningPointsMessage>> for PruningPointsList {
    type Error = ConversionError;
    fn try_from(value: Versioned<protowire::PruningPointsMessage>) -> Result<Self, Self::Error> {
        let Versioned(header_format, msg) = value;
        msg.headers.into_iter().map(|x| Versioned(header_format, x).try_into().map(Arc::new)).collect()
    }
}

impl TryFrom<Versioned<protowire::TrustedDataMessage>> for TrustedDataPackage {
    type Error = ConversionError;
    fn try_from(value: Versioned<protowire::TrustedDataMessage>) -> Result<Self, Self::Error> {
        let Versioned(header_format, msg) = value;
        let palw_digest = if msg.palw_pruning_snapshot_digest.is_empty() {
            None
        } else {
            Some(Hash64::from_bytes(msg.palw_pruning_snapshot_digest.as_slice().try_into()?))
        };
        // ADR-0042: identical empty-or-exactly-64 rule as field 3. A wrong-length digest is a
        // conversion error rather than a silently-ignored field, so a peer cannot advertise a
        // truncated binding and have the requester treat it as "no bundle offered".
        let palw_chain_derived_bundle_digest = if msg.palw_chain_derived_bundle_digest.is_empty() {
            None
        } else {
            Some(Hash64::from_bytes(msg.palw_chain_derived_bundle_digest.as_slice().try_into()?))
        };
        Ok(TrustedDataPackage::new(
            msg.daa_window.into_iter().map(|x| Versioned(header_format, x).try_into()).collect::<Result<Vec<_>, ConversionError>>()?,
            msg.ghostdag_data.into_iter().map(|x| x.try_into()).collect::<Result<Vec<_>, ConversionError>>()?,
            palw_digest,
            palw_chain_derived_bundle_digest,
        ))
    }
}

impl TryFrom<Versioned<protowire::BlockWithTrustedDataV4Message>> for TrustedDataEntry {
    type Error = ConversionError;
    fn try_from(value: Versioned<protowire::BlockWithTrustedDataV4Message>) -> Result<Self, Self::Error> {
        let Versioned(header_format, msg) = value;
        let block: Block = Versioned(header_format, msg.block.ok_or(ConversionError::NoneValue)?).try_into()?;
        Ok(TrustedDataEntry::new(block, msg.daa_window_indices, msg.ghostdag_data_indices))
    }
}

impl TryFrom<protowire::IbdChainBlockLocatorMessage> for Vec<BlockHash> {
    type Error = ConversionError;
    fn try_from(msg: protowire::IbdChainBlockLocatorMessage) -> Result<Self, Self::Error> {
        msg.block_locator_hashes.into_iter().map(|v| v.try_into()).collect()
    }
}

impl TryFrom<Versioned<protowire::BlockHeadersMessage>> for Vec<Arc<Header>> {
    type Error = ConversionError;
    fn try_from(value: Versioned<protowire::BlockHeadersMessage>) -> Result<Self, Self::Error> {
        let Versioned(header_format, msg) = value;
        msg.block_headers.into_iter().map(|v| Versioned(header_format, v).try_into().map(Arc::new)).collect()
    }
}

impl TryFrom<protowire::PruningPointUtxoSetChunkMessage> for Vec<(TransactionOutpoint, UtxoEntry)> {
    type Error = ConversionError;

    fn try_from(msg: protowire::PruningPointUtxoSetChunkMessage) -> Result<Self, Self::Error> {
        msg.outpoint_and_utxo_entry_pairs.into_iter().map(|p| p.try_into()).collect()
    }
}

impl TryFrom<protowire::RequestPruningPointUtxoSetMessage> for BlockHash {
    type Error = ConversionError;

    fn try_from(msg: protowire::RequestPruningPointUtxoSetMessage) -> Result<Self, Self::Error> {
        msg.pruning_point_hash.try_into_ex()
    }
}

impl TryFrom<protowire::InvRelayBlockMessage> for BlockHash {
    type Error = ConversionError;

    fn try_from(msg: protowire::InvRelayBlockMessage) -> Result<Self, Self::Error> {
        msg.hash.try_into_ex()
    }
}

impl TryFrom<protowire::RequestRelayBlocksMessage> for Vec<BlockHash> {
    type Error = ConversionError;

    fn try_from(msg: protowire::RequestRelayBlocksMessage) -> Result<Self, Self::Error> {
        msg.hashes.into_iter().map(|v| v.try_into()).collect()
    }
}

impl TryFrom<protowire::RequestIbdBlocksMessage> for Vec<BlockHash> {
    type Error = ConversionError;

    fn try_from(msg: protowire::RequestIbdBlocksMessage) -> Result<Self, Self::Error> {
        msg.hashes.into_iter().map(|v| v.try_into()).collect()
    }
}
impl TryFrom<protowire::RequestBlockBodiesMessage> for Vec<BlockHash> {
    type Error = ConversionError;

    fn try_from(msg: protowire::RequestBlockBodiesMessage) -> Result<Self, Self::Error> {
        msg.hashes.into_iter().map(|v| v.try_into()).collect()
    }
}

impl TryFrom<protowire::BlockLocatorMessage> for Vec<BlockHash> {
    type Error = ConversionError;

    fn try_from(msg: protowire::BlockLocatorMessage) -> Result<Self, Self::Error> {
        msg.hashes.into_iter().map(|v| v.try_into()).collect()
    }
}

impl TryFrom<protowire::AddressesMessage> for Vec<(IpAddress, u16)> {
    type Error = ConversionError;

    fn try_from(msg: protowire::AddressesMessage) -> Result<Self, Self::Error> {
        msg.address_list.into_iter().map(|addr| addr.try_into()).collect::<Result<_, _>>()
    }
}

impl TryFrom<protowire::RequestTransactionsMessage> for Vec<TransactionId> {
    type Error = ConversionError;

    fn try_from(msg: protowire::RequestTransactionsMessage) -> Result<Self, Self::Error> {
        msg.ids.into_iter().map(|v| v.try_into()).collect()
    }
}

impl TryFrom<protowire::InvTransactionsMessage> for Vec<TransactionId> {
    type Error = ConversionError;

    fn try_from(msg: protowire::InvTransactionsMessage) -> Result<Self, Self::Error> {
        msg.ids.into_iter().map(|v| v.try_into()).collect()
    }
}

impl TryFrom<protowire::TransactionNotFoundMessage> for TransactionId {
    type Error = ConversionError;

    fn try_from(msg: protowire::TransactionNotFoundMessage) -> Result<Self, Self::Error> {
        msg.id.try_into_ex()
    }
}

impl TryFrom<protowire::RequestBlockLocatorMessage> for (BlockHash, u32) {
    type Error = ConversionError;
    fn try_from(msg: protowire::RequestBlockLocatorMessage) -> Result<Self, Self::Error> {
        Ok((msg.high_hash.try_into_ex()?, msg.limit))
    }
}

impl TryFrom<protowire::RequestAntipastMessage> for (BlockHash, BlockHash) {
    type Error = ConversionError;
    fn try_from(msg: protowire::RequestAntipastMessage) -> Result<Self, Self::Error> {
        Ok((msg.block_hash.try_into_ex()?, msg.context_hash.try_into_ex()?))
    }
}

#[cfg(test)]
mod palw_trusted_digest_tests {
    use super::*;
    use crate::convert::header::HeaderFormat;

    fn message(digest: Vec<u8>, chain_derived: Vec<u8>) -> protowire::TrustedDataMessage {
        protowire::TrustedDataMessage {
            daa_window: vec![],
            ghostdag_data: vec![],
            palw_pruning_snapshot_digest: digest,
            palw_chain_derived_bundle_digest: chain_derived,
        }
    }

    fn package(digest: Vec<u8>) -> Result<TrustedDataPackage, ConversionError> {
        Versioned(HeaderFormat::Compressed, message(digest, vec![])).try_into()
    }

    fn chain_derived_package(chain_derived: Vec<u8>) -> Result<TrustedDataPackage, ConversionError> {
        Versioned(HeaderFormat::Compressed, message(vec![], chain_derived)).try_into()
    }

    #[test]
    fn trusted_palw_digest_is_absent_or_exactly_64_bytes() {
        assert!(package(vec![]).unwrap().palw_pruning_snapshot_digest.is_none());
        assert_eq!(package(vec![0x5a; 64]).unwrap().palw_pruning_snapshot_digest, Some(Hash64::from_bytes([0x5a; 64])));
        assert!(package(vec![0x5a; 63]).is_err());
        assert!(package(vec![0x5a; 65]).is_err());
    }

    /// ADR-0042: the chain-derived bundle digest obeys the same empty-or-exactly-64 rule, and is
    /// decoded independently of field 3 so neither can be mistaken for the other.
    #[test]
    fn trusted_chain_derived_bundle_digest_is_absent_or_exactly_64_bytes() {
        assert!(chain_derived_package(vec![]).unwrap().palw_chain_derived_bundle_digest.is_none());
        assert_eq!(
            chain_derived_package(vec![0xa5; 64]).unwrap().palw_chain_derived_bundle_digest,
            Some(Hash64::from_bytes([0xa5; 64]))
        );
        assert!(chain_derived_package(vec![0xa5; 63]).is_err());
        assert!(chain_derived_package(vec![0xa5; 65]).is_err());
    }

    /// A v7-era message (no field 4 on the wire) decodes to `None`, never to a zero digest: proto3
    /// absence is indistinguishable from an empty `bytes`, and "no bundle offered" is precisely the
    /// meaning that keeps existing peers on the unchanged operator-pin path.
    #[test]
    fn absent_chain_derived_digest_is_none_and_does_not_disturb_the_snapshot_digest() {
        let pkg = package(vec![0x5a; 64]).unwrap();
        assert_eq!(pkg.palw_pruning_snapshot_digest, Some(Hash64::from_bytes([0x5a; 64])));
        assert!(pkg.palw_chain_derived_bundle_digest.is_none());

        let both: TrustedDataPackage =
            Versioned(HeaderFormat::Compressed, message(vec![0x5a; 64], vec![0xa5; 64])).try_into().unwrap();
        assert_eq!(both.palw_pruning_snapshot_digest, Some(Hash64::from_bytes([0x5a; 64])));
        assert_eq!(both.palw_chain_derived_bundle_digest, Some(Hash64::from_bytes([0xa5; 64])));
    }

    /// Pin the four ADR-0042 oneof tags (75-78) and their payload-type mapping. Tags are appended and
    /// never reused, so a regression here is a wire-compatibility break with every deployed peer: the
    /// same bytes would decode as a different message. Also asserts the tags do not collide with the
    /// PALW pruning sidecar (71/72) or the DA chunk transport (73/74).
    #[test]
    fn adr_0042_bundle_tags_round_trip_and_do_not_collide() {
        use crate::KaspadMessagePayloadType;
        use crate::pb::{
            DonePalwChainDerivedBundleMessage, KaspadMessage, PalwChainDerivedBundleChunkMessage,
            RequestNextPalwChainDerivedBundleChunksMessage, RequestPalwChainDerivedBundleMessage, kaspad_message::Payload,
        };
        use prost::Message;

        let messages = [
            Payload::RequestPalwChainDerivedBundle(RequestPalwChainDerivedBundleMessage { pruning_point_hash: None }),
            Payload::PalwChainDerivedBundleChunk(PalwChainDerivedBundleChunkMessage {
                found: true,
                chunk_index: 3,
                chunk_count: 9,
                chunk: vec![0x42; 16],
            }),
            Payload::DonePalwChainDerivedBundle(DonePalwChainDerivedBundleMessage {}),
            Payload::RequestNextPalwChainDerivedBundleChunks(RequestNextPalwChainDerivedBundleChunksMessage {}),
        ];
        let expected = [
            KaspadMessagePayloadType::RequestPalwChainDerivedBundle,
            KaspadMessagePayloadType::PalwChainDerivedBundleChunk,
            KaspadMessagePayloadType::DonePalwChainDerivedBundle,
            KaspadMessagePayloadType::RequestNextPalwChainDerivedBundleChunks,
        ];
        for (payload, expected_type) in messages.into_iter().zip(expected) {
            let message = KaspadMessage { payload: Some(payload), ..Default::default() };
            let decoded = KaspadMessage::decode(message.encode_to_vec().as_slice()).unwrap();
            let decoded_payload = decoded.payload.expect("payload survives the wire");
            assert_eq!(KaspadMessagePayloadType::from(&decoded_payload), expected_type);
        }

        // The chunk body itself must survive verbatim — it is a slice of a Borsh encoding.
        let chunk = KaspadMessage {
            payload: Some(Payload::PalwChainDerivedBundleChunk(PalwChainDerivedBundleChunkMessage {
                found: true,
                chunk_index: 1,
                chunk_count: 2,
                chunk: (0u8..=255).collect(),
            })),
            ..Default::default()
        };
        let decoded = KaspadMessage::decode(chunk.encode_to_vec().as_slice()).unwrap();
        let Some(Payload::PalwChainDerivedBundleChunk(body)) = decoded.payload else { panic!("wrong payload") };
        assert_eq!(body.chunk, (0u8..=255).collect::<Vec<_>>());
        assert_eq!((body.chunk_index, body.chunk_count, body.found), (1, 2, true));
    }
}
