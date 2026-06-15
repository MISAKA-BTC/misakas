use crate::pb as protowire;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{BlueWorkType, header::Header}; // PR-9.5e: p2p block-hash convert sites widened to Hash64

use super::error::ConversionError;
use super::option::TryIntoOptionEx;

#[derive(Copy, Clone)]
pub enum HeaderFormat {
    Legacy,
    Compressed,
}

/// Determines the header format based on the protocol version.
impl From<u32> for HeaderFormat {
    fn from(version: u32) -> Self {
        if version >= 9 { Self::Compressed } else { Self::Legacy }
    }
}

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<(HeaderFormat, &Header)> for protowire::BlockHeader {
    fn from(value: (HeaderFormat, &Header)) -> Self {
        let (header_type, item) = value;

        Self {
            version: item.version.into(),
            parents: match header_type {
                HeaderFormat::Legacy => item.parents_by_level.expanded_iter().map(protowire::BlockLevelParents::from).collect(),
                HeaderFormat::Compressed => item
                    .parents_by_level
                    .raw()
                    .iter()
                    .map(|(cum, hashes)| protowire::BlockLevelParents {
                        cumulative_level: (*cum).into(),
                        parent_hashes: hashes.iter().map(|h| h.into()).collect(),
                    })
                    .collect(),
            },
            hash_merkle_root: Some(item.hash_merkle_root.into()),
            accepted_id_merkle_root: Some(item.accepted_id_merkle_root.into()),
            utxo_commitment: Some(item.utxo_commitment.into()),
            timestamp: item.timestamp.try_into().expect("timestamp is always convertible to i64"),
            bits: item.bits,
            nonce: item.nonce,
            daa_score: item.daa_score,
            // We follow the golang specification of variable big-endian here
            blue_work: item.blue_work.to_be_bytes_var(),
            blue_score: item.blue_score,
            pruning_point: Some(item.pruning_point.into()),
            // kaspa-pq Phase 2 (ADR-0007): carry the Layer-1 algo id so the
            // relay/IBD peer reconstructs the identical block hash.
            pow_algo_id: item.pow_algo_id as u32,
            // kaspa-pq EVM Lane v0.4 (ADR-0020 §4): both EVM commitments are
            // part of the v2+ header-hash preimage — they MUST survive relay/
            // IBD (the powAlgoId split-brain precedent). Zero on v0/v1 headers.
            evm_payload_hash: Some(item.evm_payload_hash.into()),
            evm_commitment_root: Some(item.evm_commitment_root.into()),
            // kaspa-pq ADR-0022: the overlay-state commitment is part of the
            // header-hash preimage on every version, so it MUST survive relay/IBD
            // (the powAlgoId / EVM-commitment split-brain precedent).
            overlay_commitment_root: Some(item.overlay_commitment_root.into()),
        }
    }
}

impl From<&[BlockHash]> for protowire::BlockLevelParents {
    fn from(item: &[BlockHash]) -> Self {
        // When converting to legacy p2p header, cumulative_level is set to 0
        Self { parent_hashes: item.iter().map(|h| h.into()).collect(), cumulative_level: 0 }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

/// A wrapper for P2P header messages indicating the expected header format during conversion.
pub struct Versioned<T>(pub HeaderFormat, pub T);

impl TryFrom<Versioned<protowire::BlockHeader>> for Header {
    type Error = ConversionError;
    fn try_from(value: Versioned<protowire::BlockHeader>) -> Result<Self, Self::Error> {
        let Versioned(header_format, item) = value;

        let parents_by_level = match header_format {
            HeaderFormat::Compressed => item
                .parents
                .into_iter()
                .map(|p| {
                    let cum = u8::try_from(p.cumulative_level)?;
                    let parents = p.parent_hashes.into_iter().map(BlockHash::try_from).collect::<Result<_, _>>()?;
                    Ok((cum, parents))
                })
                .collect::<Result<Vec<(u8, Vec<BlockHash>)>, ConversionError>>()?
                .try_into()?,
            HeaderFormat::Legacy => item
                .parents
                .into_iter()
                .map(|p| p.parent_hashes.into_iter().map(BlockHash::try_from).collect::<Result<Vec<BlockHash>, ConversionError>>())
                .collect::<Result<Vec<Vec<BlockHash>>, ConversionError>>()?
                .try_into()?,
        };

        Ok(Header::new_finalized(
            item.version.try_into()?,
            parents_by_level,
            item.hash_merkle_root.try_into_ex()?,
            item.accepted_id_merkle_root.try_into_ex()?,
            item.utxo_commitment.try_into_ex()?,
            item.timestamp.try_into()?,
            item.bits,
            item.nonce,
            // kaspa-pq Phase 2 (ADR-0007): read the Layer-1 algo id from the
            // wire so relay/IBD reconstructs the identical block hash. (Was
            // hardcoded to kHeavyHash, which silently split-brained an
            // Argon2id chain: relayed algo_id=2 headers re-hashed as algo_id=1
            // -> "requested X but got Y".)
            item.pow_algo_id as u8,
            item.daa_score,
            // We follow the golang specification of variable big-endian here
            BlueWorkType::from_be_bytes_var(&item.blue_work)?,
            item.blue_score,
            item.pruning_point.try_into_ex()?,
        )
        // kaspa-pq EVM Lane v0.4: restore the EVM commitments (absent from an
        // old peer ⇒ zero, matching every v0/v1 header where they are
        // hash-invisible anyway; on a v2 header a zero would simply fail the
        // header-hash check, never silently fork).
        .with_evm_payload_hash(item.evm_payload_hash.map(BlockHash::try_from).transpose()?.unwrap_or_default())
        .with_evm_commitment(item.evm_commitment_root.map(BlockHash::try_from).transpose()?.unwrap_or_default())
        // kaspa-pq ADR-0022: restore the overlay-state commitment (absent from an
        // old peer ⇒ zero; on a re-genesis chain a zero would simply fail the
        // header-hash check, never silently fork).
        .with_overlay_commitment(item.overlay_commitment_root.map(BlockHash::try_from).transpose()?.unwrap_or_default()))
    }
}

impl TryFrom<protowire::BlockLevelParents> for Vec<BlockHash> {
    type Error = ConversionError;
    fn try_from(item: protowire::BlockLevelParents) -> Result<Self, Self::Error> {
        item.parent_hashes.into_iter().map(|x| x.try_into()).collect()
    }
}
