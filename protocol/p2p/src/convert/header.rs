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
            palw_state_root: Some(item.palw_state_root.into()),
            // MISAKA ADR-0038: the post-PoW PALW commitment is part of the
            // block-identity preimage on PALW algo ids — dropping it on relay
            // would split-brain the identity hash between peers.
            palw_commitment: item.palw_commitment.clone(),
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
            //
            // `try_into`, NOT `as u8`: the field is u32 on the wire and u8 in the header, and a
            // truncating cast silently REINTERPRETS an out-of-range id — 259 arrives as 3, so a
            // peer could hand us a header we re-hash under a different algorithm than the one it
            // declared. Fail the conversion instead, exactly as `item.version` above does
            // (mainnet-readiness audit §5).
            item.pow_algo_id.try_into()?,
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
        .with_overlay_commitment(item.overlay_commitment_root.map(BlockHash::try_from).transpose()?.unwrap_or_default())
        .with_palw_state_root(item.palw_state_root.map(BlockHash::try_from).transpose()?.unwrap_or_default())
        // MISAKA ADR-0038: restore the post-PoW PALW commitment BEFORE the
        // identity re-hash — on a PALW header it is part of the preimage.
        .with_palw_commitment(item.palw_commitment))
    }
}

impl TryFrom<protowire::BlockLevelParents> for Vec<BlockHash> {
    type Error = ConversionError;
    fn try_from(item: protowire::BlockLevelParents) -> Result<Self, Self::Error> {
        item.parent_hashes.into_iter().map(|x| x.try_into()).collect()
    }
}

impl TryFrom<Versioned<protowire::IbdCandidateSummaryMessage>> for crate::convert::model::ibd_candidate::IbdCandidateSummary {
    type Error = ConversionError;
    fn try_from(Versioned(format, msg): Versioned<protowire::IbdCandidateSummaryMessage>) -> Result<Self, Self::Error> {
        let header = msg.virtual_selected_parent.ok_or(ConversionError::NoneValue)?;
        Ok(Self {
            virtual_selected_parent: Versioned(format, header).try_into()?,
            pruning_point: msg.pruning_point.try_into_ex()?,
            genesis_hash: msg.genesis_hash,
            consensus_params_id: msg.consensus_params_id,
        })
    }
}

#[cfg(test)]
mod tests {
    //! **A remote crash from a malformed header** (pre-freeze review, group A): the compressed
    //! (protocol >= 9) header wire form lets the PEER choose every run's `cumulative_level`, and a
    //! first run at level 0 panicked this node inside the conversion itself — `Header::new_finalized`
    //! hashes the header, the hash walks `expanded_iter`, and `expand_rle` `expect`s strictly
    //! increasing counts from 0. Every MISAKA peer speaks protocol 105, so the path is the relay
    //! (`RequestRelayBlocks` → `Block`), IBD headers, the IBD candidate summary, pruning-point proofs
    //! and trusted data: all of them convert through `TryFrom<Versioned<BlockHeader>>` below.
    use super::*;
    use crate::convert::model::ibd_candidate::IbdCandidateSummary;
    use kaspa_consensus_core::block::Block;
    use kaspa_consensus_core::errors::header::CompressedParentsError;
    use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH;

    fn h(v: u64) -> BlockHash {
        BlockHash::from_u64_word(v)
    }

    fn honest_header() -> Header {
        Header::new_finalized(
            1,
            vec![vec![h(1), h(2)], vec![h(1), h(2)], vec![h(3)]].try_into().unwrap(),
            Default::default(),
            Default::default(),
            Default::default(),
            1_758_800_000_000,
            0x1f00_ffff,
            7,
            POW_ALGO_ID_KHEAVYHASH,
            10,
            5u64.into(),
            9,
            Default::default(),
        )
    }

    fn run(cumulative_level: u32, parents: &[BlockHash]) -> protowire::BlockLevelParents {
        protowire::BlockLevelParents { cumulative_level, parent_hashes: parents.iter().map(|p| p.into()).collect() }
    }

    /// Every malformed parents shape a peer can put on the compressed wire: each must come back as
    /// a conversion error (the flow then drops the peer), never as a panic of this process.
    fn malformed_runs() -> Vec<Vec<protowire::BlockLevelParents>> {
        vec![
            // One run at level 0: expands to zero levels, and the hash's `expand_rle` panics.
            vec![run(0, &[h(1)])],
            // A zero first run followed by a well-formed one: passes the pairwise
            // strictly-increasing check, then panics in the hash (and `get` would underflow).
            vec![run(0, &[h(1)]), run(3, &[h(2)])],
            // The LEGACY encoding (every run at level 0) sent under the compressed format.
            vec![run(0, &[h(1)]), run(0, &[h(2)])],
            // Non-increasing and repeated runs (refused before this fix too).
            vec![run(2, &[h(1)]), run(2, &[h(2)])],
            vec![run(1, &[h(1)]), run(2, &[h(1)])],
            // A level that does not fit the u8 the header stores.
            vec![run(256, &[h(1)])],
        ]
    }

    #[test]
    fn a_malformed_compressed_parents_run_is_a_conversion_error_not_a_panic() {
        for parents in malformed_runs() {
            let mut wire = protowire::BlockHeader::from((HeaderFormat::Compressed, &honest_header()));
            wire.parents = parents.clone();
            let converted = std::panic::catch_unwind(|| Header::try_from(Versioned(HeaderFormat::Compressed, wire)));
            let converted = converted.unwrap_or_else(|_| panic!("a peer's header with parents {parents:?} panicked the conversion"));
            assert!(converted.is_err(), "a peer's header with parents {parents:?} was accepted");
        }
        // The zero first run is refused by NAME, where the rule lives.
        let mut wire = protowire::BlockHeader::from((HeaderFormat::Compressed, &honest_header()));
        wire.parents = vec![run(0, &[h(1)]), run(3, &[h(2)])];
        assert!(matches!(
            Header::try_from(Versioned(HeaderFormat::Compressed, wire)),
            Err(ConversionError::CompressedParentsError(CompressedParentsError::LevelsNotStrictlyIncreasing))
        ));
    }

    /// The same header inside the messages that carry one: the relayed block and the IBD candidate
    /// summary. (Proof, headers and trusted-data messages map each element through the same impl.)
    #[test]
    fn the_messages_that_carry_a_header_refuse_it_too() {
        let mut header = protowire::BlockHeader::from((HeaderFormat::Compressed, &honest_header()));
        header.parents = vec![run(0, &[h(1)])];
        let block = protowire::BlockMessage { header: Some(header.clone()), transactions: vec![], evm_payload: vec![] };
        let converted = std::panic::catch_unwind(|| Block::try_from(Versioned(HeaderFormat::Compressed, block)));
        assert!(converted.expect("the relayed block panicked the conversion").is_err());
        let summary = protowire::IbdCandidateSummaryMessage {
            virtual_selected_parent: Some(header),
            pruning_point: Some(h(4).into()),
            genesis_hash: vec![],
            consensus_params_id: vec![],
        };
        let converted = std::panic::catch_unwind(|| IbdCandidateSummary::try_from(Versioned(HeaderFormat::Compressed, summary)));
        assert!(converted.expect("the IBD candidate summary panicked the conversion").is_err());
    }

    /// No verdict moves for a well-formed header: both wire forms still round-trip to the same
    /// header and the same block hash.
    #[test]
    fn a_well_formed_header_converts_to_the_same_hash_in_both_formats() {
        let header = honest_header();
        for format in [HeaderFormat::Compressed, HeaderFormat::Legacy] {
            let wire = protowire::BlockHeader::from((format, &header));
            let back = Header::try_from(Versioned(format, wire)).expect("an honest header converts");
            assert_eq!(back.hash, header.hash);
            assert_eq!(back.parents_by_level, header.parents_by_level);
        }
    }
}
