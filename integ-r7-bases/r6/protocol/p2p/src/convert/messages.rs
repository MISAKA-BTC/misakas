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
            fork_id_fired: item.fork_id_fired,
            fork_id_next: item.fork_id_next,
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
            // ADR-0072 SA-2. Bounded like its four siblings and for the same reason: an
            // unauthenticated peer supplies it, and the gate compares it against locally computed
            // digests one prefix at a time — an unbounded field would be an unbounded comparison.
            fork_id_fired: bounded_fingerprint("forkIdFired", msg.fork_id_fired)?,
            fork_id_next: msg.fork_id_next,
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

    /// A handshake message with every field an old build writes, and the two ADR-0072 fields at
    /// their proto3 defaults — which is to say, exactly what an old build writes.
    fn version_message_without_a_fork_id() -> protowire::VersionMessage {
        protowire::VersionMessage {
            protocol_version: 5,
            services: 0,
            timestamp: 0,
            address: None,
            id: vec![0u8; 16],
            user_agent: "old-build/1.0".to_owned(),
            disable_relay_tx: false,
            subnetwork_id: None,
            network: "misaka-testnet-11".to_owned(),
            genesis_hash: vec![0u8; MAX_HANDSHAKE_FINGERPRINT_BYTES],
            consensus_params_id: vec![1u8; 32],
            consensus_identity_id: vec![2u8; 32],
            consensus_schedule_id: vec![3u8; 32],
            fork_id_fired: vec![],
            fork_id_next: 0,
        }
    }

    /// **A peer predating the fork id handshakes exactly as it does today** (ADR-0072 SA-2).
    ///
    /// This is the claim the whole lane rests on, and it is asserted on real wire bytes rather
    /// than on the struct: proto3 does not write a field at its default, so the encoding of the
    /// message above IS byte-for-byte the encoding a build without fields 15 and 16 produces.
    /// `starts_with` is what says so — the two new fields are appended after every field an old
    /// build writes, and removing them leaves exactly the old message.
    ///
    /// The direction that matters for a running network is the decode: an old peer's bytes must
    /// reach `Version` with every other field intact and the fork id simply absent, because with
    /// the fence off (`fork_id_gate_armed_v1` is `false` on every shipped preset) absent is what
    /// the gate is required to ignore.
    #[test]
    fn a_peer_predating_the_fork_id_field_round_trips_unchanged() {
        use prost::Message;

        let old_peer = version_message_without_a_fork_id();
        let old_wire = old_peer.encode_to_vec();

        let mut upgraded = old_peer.clone();
        upgraded.fork_id_fired = vec![0xABu8; 32];
        upgraded.fork_id_next = 2_125_000;
        let new_wire = upgraded.encode_to_vec();

        assert!(
            new_wire.starts_with(&old_wire),
            "fields 15/16 are appended after every field an old build writes — an old peer's bytes are this encoding minus them"
        );
        assert!(new_wire.len() > old_wire.len(), "the upgraded message must actually carry the two fields");

        // The decode: an old peer's bytes, read by this build.
        let decoded = protowire::VersionMessage::decode(old_wire.as_slice()).expect("an old peer's bytes must still parse");
        assert_eq!(decoded, old_peer, "no field an old build writes may be disturbed by the two new ones");
        assert!(decoded.fork_id_fired.is_empty(), "absent, not garbage");
        assert_eq!(decoded.fork_id_next, 0);

        // And through the conversion the handshake actually uses.
        let version = Version::try_from(decoded).expect("an old peer must still convert");
        assert!(version.fork_id_fired.is_empty());
        assert_eq!(version.fork_id_next, 0);
        assert_eq!(version.consensus_identity_id, vec![2u8; 32], "the fields the gate before it reads are untouched");
        assert_eq!(version.consensus_schedule_id, vec![3u8; 32]);

        // The upgraded message survives its own round trip too, so the field is readable and not
        // merely writable.
        let back = protowire::VersionMessage::decode(new_wire.as_slice()).expect("this build's own bytes must parse");
        assert_eq!(back, upgraded);
        let version = Version::try_from(back).expect("a full-width fork id is what an upgraded peer sends");
        assert_eq!(version.fork_id_fired, vec![0xABu8; 32]);
        assert_eq!(version.fork_id_next, 2_125_000);
    }

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
            // ADR-0072 SA-2's two fields, at the defaults an old peer's bytes decode to.
            fork_id_fired: vec![],
            fork_id_next: 0,
        };
        assert!(Version::try_from(base.clone()).is_ok(), "a full-width genesis hash is exactly what a peer sends");

        for (name, mut msg) in [
            ("genesisHash", base.clone()),
            ("consensusParamsId", base.clone()),
            ("consensusIdentityId", base.clone()),
            ("consensusScheduleId", base.clone()),
            // ADR-0072 SA-2's field is peer-controlled bytes like the four above it.
            ("forkIdFired", base.clone()),
        ] {
            let oversized = vec![0u8; MAX_HANDSHAKE_FINGERPRINT_BYTES + 1];
            match name {
                "genesisHash" => msg.genesis_hash = oversized,
                "consensusParamsId" => msg.consensus_params_id = oversized,
                "consensusIdentityId" => msg.consensus_identity_id = oversized,
                "forkIdFired" => msg.fork_id_fired = oversized,
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
