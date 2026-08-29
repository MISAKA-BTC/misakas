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
use kaspa_consensus_core::BlockHash; // PR-9.5e: p2p block-hash convert sites widened to Hash64
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
            genesis_hash: item.genesis_hash,
            consensus_params_id: item.consensus_params_id,
            consensus_identity_id: item.consensus_identity_id,
            consensus_schedule_id: item.consensus_schedule_id,
        }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

/// The widest handshake fingerprint any shipped build sends: a `Hash64` genesis hash. The
/// consensus ids are 32-byte values, so this bounds every one of them with room to spare — and it
/// is a bound, not a guess: a field wider than this is not a fingerprint this build understands.
pub const MAX_HANDSHAKE_FINGERPRINT_BYTES: usize = 64;

fn bounded_fingerprint(name: &'static str, bytes: Vec<u8>) -> Result<Vec<u8>, ConversionError> {
    if bytes.len() > MAX_HANDSHAKE_FINGERPRINT_BYTES {
        return Err(ConversionError::OversizedFingerprint(name, bytes.len(), MAX_HANDSHAKE_FINGERPRINT_BYTES));
    }
    Ok(bytes)
}

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
            // **Bounded here, before anything renders or compares them** (audit3 H11).
            genesis_hash: bounded_fingerprint("genesisHash", msg.genesis_hash)?,
            consensus_params_id: bounded_fingerprint("consensusParamsId", msg.consensus_params_id)?,
            consensus_identity_id: bounded_fingerprint("consensusIdentityId", msg.consensus_identity_id)?,
            consensus_schedule_id: bounded_fingerprint("consensusScheduleId", msg.consensus_schedule_id)?,
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
        Ok(TrustedDataPackage::new(
            msg.daa_window.into_iter().map(|x| Versioned(header_format, x).try_into()).collect::<Result<Vec<_>, ConversionError>>()?,
            msg.ghostdag_data.into_iter().map(|x| x.try_into()).collect::<Result<Vec<_>, ConversionError>>()?,
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
mod fingerprint_bound_tests {
    use super::*;

    /// **The handshake's fingerprint fields are bounded at the boundary** (audit3 H11).
    ///
    /// The transport accepts messages up to `P2P_MAX_MESSAGE_SIZE` (1 GB) and the proto declares
    /// these as plain `bytes` with no cap, so before this every one of them arrived unbounded and
    /// was rendered into a log line by an unauthenticated peer's choosing. A field wider than a
    /// `Hash64` is not a fingerprint this build understands, so refusing it is not a heuristic.
    #[test]
    fn an_oversized_fingerprint_is_refused_before_it_is_compared_or_rendered() {
        let base = protowire::VersionMessage {
            protocol_version: 5,
            services: 0,
            timestamp: 0,
            address: None,
            id: vec![0u8; 16],
            user_agent: String::new(),
            disable_relay_tx: false,
            subnetwork_id: None,
            network: "misaka-testnet-11".to_owned(),
            genesis_hash: vec![0u8; MAX_HANDSHAKE_FINGERPRINT_BYTES],
            consensus_params_id: vec![],
            consensus_identity_id: vec![],
            consensus_schedule_id: vec![],
        };
        assert!(Version::try_from(base.clone()).is_ok(), "a full-width genesis hash is exactly what a peer sends");

        for (name, mut msg) in [
            ("genesisHash", base.clone()),
            ("consensusParamsId", base.clone()),
            ("consensusIdentityId", base.clone()),
            ("consensusScheduleId", base.clone()),
        ] {
            let oversized = vec![0u8; MAX_HANDSHAKE_FINGERPRINT_BYTES + 1];
            match name {
                "genesisHash" => msg.genesis_hash = oversized,
                "consensusParamsId" => msg.consensus_params_id = oversized,
                "consensusIdentityId" => msg.consensus_identity_id = oversized,
                _ => msg.consensus_schedule_id = oversized,
            }
            let Err(err) = Version::try_from(msg) else {
                panic!("{name} must be refused when oversized");
            };
            assert!(
                matches!(err, ConversionError::OversizedFingerprint(field, _, _) if field == name),
                "{name}: the refusal names the field it refused, got {err:?}"
            );
        }
    }
}
