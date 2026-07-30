use crate::pb as protowire;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    BlueWorkType,
    header::{Header, PalwHeaderFields},
}; // PR-9.5e: p2p block-hash convert sites widened to Hash64

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
            // kaspa-pq ADR-0039 PALW: the ten Header-v3 fields. Part of the header-hash
            // preimage only for version >= 3, so they MUST survive relay/IBD for a v3
            // header to re-hash identically; zero and hash-invisible on v0/v1/v2.
            blue_hash_work: item.blue_hash_work.to_be_bytes_var(),
            blue_compute_work: item.blue_compute_work.to_be_bytes_var(),
            palw_batch_id: Some(item.palw_batch_id.into()),
            palw_leaf_index: item.palw_leaf_index,
            palw_ticket_nullifier: Some(item.palw_ticket_nullifier.into()),
            palw_epoch_certificate_hash: Some(item.palw_epoch_certificate_hash.into()),
            palw_chain_commit: Some(item.palw_chain_commit.into()),
            palw_target_daa_interval: item.palw_target_daa_interval,
            palw_authorization_hash: Some(item.palw_authorization_hash.into()),
            palw_proof_type: item.palw_proof_type as u32,
            palw_beacon_seed: Some(item.palw_beacon_seed.into()),
            // PALW Header-v4: authenticated fork-local accumulator plus the
            // independent objective anti-spam stamp nonce.
            palw_spam_accumulator_commitment: Some(item.palw_spam_accumulator_commitment.into()),
            palw_spam_nonce: item.palw_spam_nonce,
            // ADR-MA Header-v5: the three Compute Set registry references (part of the v5+
            // preimage; zero and hash-invisible below).
            palw_compute_set_id: Some(item.palw_compute_set_id.into()),
            palw_compute_policy_id: Some(item.palw_compute_policy_id.into()),
            palw_allocation_plan_id: Some(item.palw_allocation_plan_id.into()),
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
        .with_overlay_commitment(item.overlay_commitment_root.map(BlockHash::try_from).transpose()?.unwrap_or_default())
        // kaspa-pq ADR-0039 PALW: restore the ten Header-v3 fields (absent from an old peer / on a
        // pre-v3 header ⇒ zero, where they are hash-invisible anyway; on a v3 header a wrong value
        // would simply fail the header-hash check, never silently fork). `from_be_bytes_var(&[])` = 0.
        .with_palw_fields(PalwHeaderFields {
            blue_hash_work: BlueWorkType::from_be_bytes_var(&item.blue_hash_work)?,
            blue_compute_work: BlueWorkType::from_be_bytes_var(&item.blue_compute_work)?,
            palw_batch_id: item.palw_batch_id.map(BlockHash::try_from).transpose()?.unwrap_or_default(),
            palw_leaf_index: item.palw_leaf_index,
            palw_ticket_nullifier: item.palw_ticket_nullifier.map(BlockHash::try_from).transpose()?.unwrap_or_default(),
            palw_epoch_certificate_hash: item.palw_epoch_certificate_hash.map(BlockHash::try_from).transpose()?.unwrap_or_default(),
            palw_chain_commit: item.palw_chain_commit.map(BlockHash::try_from).transpose()?.unwrap_or_default(),
            palw_target_daa_interval: item.palw_target_daa_interval,
            palw_authorization_hash: item.palw_authorization_hash.map(BlockHash::try_from).transpose()?.unwrap_or_default(),
            palw_proof_type: item.palw_proof_type as u8,
            palw_beacon_seed: item.palw_beacon_seed.map(BlockHash::try_from).transpose()?.unwrap_or_default(),
            palw_spam_accumulator_commitment: item
                .palw_spam_accumulator_commitment
                .map(BlockHash::try_from)
                .transpose()?
                .unwrap_or_default(),
            palw_spam_nonce: item.palw_spam_nonce,
            // ADR-MA Header-v5: restore the registry references (absent from an old peer / on a
            // pre-v5 header ⇒ zero, hash-invisible there; on a v5 header a wrong value simply
            // fails the header-hash check, never a silent fork).
            palw_compute_set_id: item.palw_compute_set_id.map(BlockHash::try_from).transpose()?.unwrap_or_default(),
            palw_compute_policy_id: item.palw_compute_policy_id.map(BlockHash::try_from).transpose()?.unwrap_or_default(),
            palw_allocation_plan_id: item.palw_allocation_plan_id.map(BlockHash::try_from).transpose()?.unwrap_or_default(),
        }))
    }
}

impl TryFrom<protowire::BlockLevelParents> for Vec<BlockHash> {
    type Error = ConversionError;
    fn try_from(item: protowire::BlockLevelParents) -> Result<Self, Self::Error> {
        item.parent_hashes.into_iter().map(|x| x.try_into()).collect()
    }
}

#[cfg(test)]
mod palw_header_roundtrip_tests {
    use super::*;
    use kaspa_consensus_core::header::PalwHeaderFields;
    use kaspa_hashes::Hash64;

    fn h(b: u8) -> BlockHash {
        Hash64::from_bytes([b; 64])
    }

    fn build(version: u16, algo: u8, palw: PalwHeaderFields) -> Header {
        Header::new_finalized(
            version,
            vec![vec![h(1), h(2)]].try_into().unwrap(),
            h(3),                         // hash_merkle_root
            h(4),                         // accepted_id_merkle_root
            h(5),                         // utxo_commitment
            123,                          // timestamp
            0x1d00ffff,                   // bits
            42,                           // nonce
            algo,                         // pow_algo_id
            77,                           // daa_score
            BlueWorkType::from_u64(9999), // blue_work (effective E)
            88,                           // blue_score
            h(6),                         // pruning_point
        )
        .with_palw_fields(palw)
    }

    /// ADR-0039 §18.4/§22: a Header-v3 with populated PALW fields round-trips through the P2P protobuf
    /// and back byte-for-byte, INCLUDING its block hash (the fields are in the v3 preimage, so a relayed
    /// v3 header must re-hash identically — the powAlgoId / EVM / overlay split-brain precedent).
    #[test]
    fn v3_header_p2p_roundtrip_preserves_palw_fields_and_hash() {
        let palw = PalwHeaderFields {
            blue_hash_work: BlueWorkType::from_u64(500),
            blue_compute_work: BlueWorkType::from_u64(120),
            palw_batch_id: h(10),
            palw_leaf_index: 7,
            palw_ticket_nullifier: h(11),
            palw_epoch_certificate_hash: h(12),
            palw_chain_commit: h(13),
            palw_target_daa_interval: 2400,
            palw_authorization_hash: h(14),
            palw_proof_type: 1,
            palw_beacon_seed: h(15),
            palw_spam_accumulator_commitment: Default::default(),
            palw_spam_nonce: 0,
            palw_compute_set_id: Default::default(),
            palw_compute_policy_id: Default::default(),
            palw_allocation_plan_id: Default::default(),
        };
        let header = build(3, 4, palw);
        for (name, fmt) in [("legacy", HeaderFormat::Legacy), ("compressed", HeaderFormat::Compressed)] {
            let proto: protowire::BlockHeader = (fmt, &header).into();
            let back: Header = Versioned(fmt, proto).try_into().unwrap();
            // The block hash is the load-bearing check: it re-covers every hashed field incl. the v3 PALW ones.
            assert_eq!(header.hash, back.hash, "{name}: v3 block hash preserved");
            assert_eq!(back.blue_hash_work, BlueWorkType::from_u64(500), "{name}: blue_hash_work");
            assert_eq!(back.blue_compute_work, BlueWorkType::from_u64(120), "{name}: blue_compute_work");
            assert_eq!(back.palw_batch_id, h(10), "{name}: batch_id");
            assert_eq!(back.palw_leaf_index, 7, "{name}: leaf_index");
            assert_eq!(back.palw_ticket_nullifier, h(11), "{name}: nullifier");
            assert_eq!(back.palw_target_daa_interval, 2400, "{name}: target interval");
            assert_eq!(back.palw_proof_type, 1, "{name}: proof_type");
            assert_eq!(back.palw_spam_accumulator_commitment, Hash64::default(), "{name}: v3 accumulator is inert");
            assert_eq!(back.palw_spam_nonce, 0, "{name}: v3 spam nonce is inert");
        }
    }

    /// Header-v4 makes both anti-spam fields canonical. Relay/IBD must preserve
    /// them in both legacy and compressed parent encodings, including the
    /// resulting block identity.
    #[test]
    fn v4_header_p2p_roundtrip_preserves_antispam_fields_and_hash() {
        let palw =
            PalwHeaderFields { palw_spam_accumulator_commitment: h(16), palw_spam_nonce: 0x0123_4567_89ab_cdef, ..Default::default() };
        let header = build(4, 4, palw);

        for (name, fmt) in [("legacy", HeaderFormat::Legacy), ("compressed", HeaderFormat::Compressed)] {
            let proto: protowire::BlockHeader = (fmt, &header).into();
            assert_eq!(proto.palw_spam_accumulator_commitment, Some((&h(16)).into()), "{name}: proto accumulator");
            assert_eq!(proto.palw_spam_nonce, 0x0123_4567_89ab_cdef, "{name}: proto spam nonce");

            let back: Header = Versioned(fmt, proto).try_into().unwrap();
            assert_eq!(header.hash, back.hash, "{name}: v4 block hash preserved");
            assert_eq!(back.palw_spam_accumulator_commitment, h(16), "{name}: accumulator");
            assert_eq!(back.palw_spam_nonce, 0x0123_4567_89ab_cdef, "{name}: spam nonce");
        }
    }

    /// ADR-MA Header-v5 makes the three Compute Set registry references canonical. Relay/IBD must
    /// preserve them in both parent encodings, including the resulting block identity — and an old
    /// peer that omits the optional fields must decode to the inert zeros.
    #[test]
    fn v5_header_p2p_roundtrip_preserves_compute_set_refs_and_hash() {
        let palw = PalwHeaderFields {
            palw_compute_set_id: h(20),
            palw_compute_policy_id: h(21),
            palw_allocation_plan_id: h(22),
            ..Default::default()
        };
        let header = build(5, 4, palw);

        for (name, fmt) in [("legacy", HeaderFormat::Legacy), ("compressed", HeaderFormat::Compressed)] {
            let proto: protowire::BlockHeader = (fmt, &header).into();
            assert_eq!(proto.palw_compute_set_id, Some((&h(20)).into()), "{name}: proto set id");
            assert_eq!(proto.palw_compute_policy_id, Some((&h(21)).into()), "{name}: proto policy id");
            assert_eq!(proto.palw_allocation_plan_id, Some((&h(22)).into()), "{name}: proto plan id");

            let back: Header = Versioned(fmt, proto).try_into().unwrap();
            assert_eq!(header.hash, back.hash, "{name}: v5 block hash preserved");
            assert_eq!(back.palw_compute_set_id, h(20), "{name}: set id");
            assert_eq!(back.palw_compute_policy_id, h(21), "{name}: policy id");
            assert_eq!(back.palw_allocation_plan_id, h(22), "{name}: plan id");
        }

        // An old peer omitting the optional v5 fields decodes them to zero (and on a REAL v5
        // header that zero simply fails the hash check — never a silent fork).
        let mut proto: protowire::BlockHeader = (HeaderFormat::Legacy, &header).into();
        proto.palw_compute_set_id = None;
        proto.palw_compute_policy_id = None;
        proto.palw_allocation_plan_id = None;
        let back: Header = Versioned(HeaderFormat::Legacy, proto).try_into().unwrap();
        assert_eq!(back.palw_compute_set_id, Hash64::default());
        assert_ne!(header.hash, back.hash, "a v5 header stripped of its registry refs re-hashes differently");
    }

    /// A pre-v3 header carries zero PALW fields (hash-invisible), so the round-trip preserves the hash
    /// and the fields stay zero (an old peer omitting them decodes to zero identically).
    #[test]
    fn prev3_header_p2p_roundtrip_hash_unchanged() {
        let header = build(1, 3, PalwHeaderFields::default());
        let mut proto: protowire::BlockHeader = (HeaderFormat::Legacy, &header).into();
        // A peer predating Header-v4 omits the optional Hash field entirely.
        proto.palw_spam_accumulator_commitment = None;
        let back: Header = Versioned(HeaderFormat::Legacy, proto).try_into().unwrap();
        assert_eq!(header.hash, back.hash, "pre-v3 hash unchanged");
        assert_eq!(back.blue_hash_work, BlueWorkType::from_u64(0));
        assert_eq!(back.palw_batch_id, Hash64::from_bytes([0u8; 64]));
        assert_eq!(back.palw_spam_accumulator_commitment, Hash64::default());
        assert_eq!(back.palw_spam_nonce, 0);
    }
}
