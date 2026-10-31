//! Bounded private PAYLOAD codec for a prospective lightweight attempt notice.
//!
//! No native magic, header form, PoW algorithm, domain or activation is allocated
//! here. Parsing/authenticating a Result is not header acceptance, a canonical
//! job assignment, resource/funding admission, inference correctness or credit.
//! The native header owner must supply the independently derived origin facts.

use crate::{
    Hash64,
    palw_state_v2::PalwBondKeyV2,
    palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{PUBKEY_BYTES, SIGNATURE_BYTES},
    messages::{AuthenticatedResult, MinerResultCommit},
};

/// The r1 Result is fixed-width and contains no Vec/String or optional fields.
pub const POSITIVE_NOTICE_RESULT_BYTES_V2: usize = 1076;
/// version/revision, challenge, native outpoint, key, Result, layout length.
pub const POSITIVE_NOTICE_PREFIX_BYTES_V2: usize = 4 + 64 + 68 + PUBKEY_BYTES + POSITIVE_NOTICE_RESULT_BYTES_V2 + 4;
pub const POSITIVE_NOTICE_OVERHEAD_BYTES_V2: usize = POSITIVE_NOTICE_PREFIX_BYTES_V2 + 2 * SIGNATURE_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveNoticeLimitsV2 {
    pub max_payload_bytes: u32,
    pub max_layout_bytes: u32,
    /// Combined commit/state entry count, checked before any layout allocation.
    pub max_layout_entries: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositiveNoticeErrorV2 {
    Budget,
    Version,
    Framing,
    Result(ContractError),
}

/// Canonical layout bytes only. Authentication against a rooted class is a
/// separate metadata-resolver check. Scanning this view allocates no Vec.
#[derive(Clone, Copy, Debug)]
pub struct PositiveNoticeLayoutV2<'a> {
    bytes: &'a [u8],
    commit_end: usize,
}
impl<'a> PositiveNoticeLayoutV2<'a> {
    fn decode(bytes: &'a [u8], limits: PositiveNoticeLimitsV2) -> Result<Self, PositiveNoticeErrorV2> {
        if bytes.len() < 22 || bytes.len() as u64 > u64::from(limits.max_layout_bytes) {
            return Err(PositiveNoticeErrorV2::Budget);
        }
        if u16::from_le_bytes(bytes[..2].try_into().map_err(|_| PositiveNoticeErrorV2::Framing)?) != PALW_TIR_LAYOUT_VERSION_V1 {
            return Err(PositiveNoticeErrorV2::Version);
        }
        let commits = word(bytes, 14)? as u64;
        if commits > u64::from(limits.max_layout_entries) {
            return Err(PositiveNoticeErrorV2::Budget);
        }
        let commit_end =
            18u64.checked_add(commits.checked_mul(4).ok_or(PositiveNoticeErrorV2::Budget)?).ok_or(PositiveNoticeErrorV2::Budget)?;
        let commit_end = usize::try_from(commit_end).map_err(|_| PositiveNoticeErrorV2::Budget)?;
        let states = word(bytes, commit_end)? as u64;
        if commits.checked_add(states).is_none_or(|n| n > u64::from(limits.max_layout_entries)) {
            return Err(PositiveNoticeErrorV2::Budget);
        }
        let end = (commit_end as u64)
            .checked_add(4)
            .and_then(|n| n.checked_add(states.checked_mul(4)?))
            .ok_or(PositiveNoticeErrorV2::Budget)?;
        if end != bytes.len() as u64 {
            return Err(PositiveNoticeErrorV2::Framing);
        }
        Ok(Self { bytes, commit_end })
    }
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
    pub fn commit_tiles(&self) -> impl ExactSizeIterator<Item = u32> + 'a {
        self.bytes[18..self.commit_end].chunks_exact(4).map(|x| u32::from_le_bytes(x.try_into().expect("four-byte chunk")))
    }
    pub fn state_tiles(&self) -> impl ExactSizeIterator<Item = u32> + 'a {
        self.bytes[self.commit_end + 4..].chunks_exact(4).map(|x| u32::from_le_bytes(x.try_into().expect("four-byte chunk")))
    }
    /// Explicit allocating step AFTER the native caller obtains its real
    /// reservation. This method itself grants no reservation/capacity token.
    pub fn decode_owned_after_reservation(&self) -> Result<PalwTirLayoutV1, PositiveNoticeErrorV2> {
        borsh::from_slice(self.bytes).map_err(|_| PositiveNoticeErrorV2::Framing)
    }
}

pub struct PositiveNoticePayloadV2<'a> {
    bytes: &'a [u8],
    challenge: Hash64,
    producer_bond: PalwBondKeyV2,
    key: &'a [u8; PUBKEY_BYTES],
    result: MinerResultCommit,
    layout: PositiveNoticeLayoutV2<'a>,
    result_signature: &'a [u8; SIGNATURE_BYTES],
    position_signature: &'a [u8; SIGNATURE_BYTES],
    unsigned_end: usize,
}
impl<'a> PositiveNoticePayloadV2<'a> {
    pub fn decode(bytes: &'a [u8], limits: PositiveNoticeLimitsV2) -> Result<Self, PositiveNoticeErrorV2> {
        if limits.max_payload_bytes == 0
            || limits.max_layout_bytes == 0
            || limits.max_layout_entries == 0
            || bytes.len() < POSITIVE_NOTICE_OVERHEAD_BYTES_V2 + 22
            || bytes.len() as u64 > u64::from(limits.max_payload_bytes)
        {
            return Err(PositiveNoticeErrorV2::Budget);
        }
        if bytes[..2] != CONTRACT_VERSION.to_le_bytes() || bytes[2..4] != CONTRACT_REVISION.to_le_bytes() {
            return Err(PositiveNoticeErrorV2::Version);
        }
        let result_start = 4 + 64 + 68 + PUBKEY_BYTES;
        let layout_length_at = result_start + POSITIVE_NOTICE_RESULT_BYTES_V2;
        let layout_length = word(bytes, layout_length_at)? as usize;
        let unsigned_end = POSITIVE_NOTICE_PREFIX_BYTES_V2.checked_add(layout_length).ok_or(PositiveNoticeErrorV2::Budget)?;
        let end = unsigned_end.checked_add(2 * SIGNATURE_BYTES).ok_or(PositiveNoticeErrorV2::Budget)?;
        if end != bytes.len() {
            return Err(PositiveNoticeErrorV2::Framing);
        }
        let layout = PositiveNoticeLayoutV2::decode(&bytes[POSITIVE_NOTICE_PREFIX_BYTES_V2..unsigned_end], limits)?;
        // All dynamic lengths/exact EOF have been checked. The remaining fields
        // are fixed-width, and decoding Result allocates no dynamic collection.
        let challenge = Hash64::from_bytes(bytes[4..68].try_into().map_err(|_| PositiveNoticeErrorV2::Framing)?);
        let producer_bond = PalwBondKeyV2::try_from_slice(&bytes[68..136]).map_err(|_| PositiveNoticeErrorV2::Framing)?;
        let key = bytes[136..result_start].try_into().map_err(|_| PositiveNoticeErrorV2::Framing)?;
        let result: MinerResultCommit =
            borsh::from_slice(&bytes[result_start..layout_length_at]).map_err(|_| PositiveNoticeErrorV2::Framing)?;
        result.validate_shape().map_err(PositiveNoticeErrorV2::Result)?;
        let result_signature =
            bytes[unsigned_end..unsigned_end + SIGNATURE_BYTES].try_into().map_err(|_| PositiveNoticeErrorV2::Framing)?;
        let position_signature = bytes[unsigned_end + SIGNATURE_BYTES..].try_into().map_err(|_| PositiveNoticeErrorV2::Framing)?;
        Ok(Self { bytes, challenge, producer_bond, key, result, layout, result_signature, position_signature, unsigned_end })
    }
    /// Exact canonical unsigned payload. A NEW-form-only identity projection
    /// may hash this under its centrally allocated domain. Neither signature
    /// may enter that identity or its position-signature digest.
    pub fn unsigned_bytes(&self) -> &'a [u8] {
        &self.bytes[..self.unsigned_end]
    }
    pub fn challenge(&self) -> Hash64 {
        self.challenge
    }
    pub fn producer_bond(&self) -> PalwBondKeyV2 {
        self.producer_bond
    }
    pub fn key(&self) -> &'a [u8; PUBKEY_BYTES] {
        self.key
    }
    pub fn result(&self) -> &MinerResultCommit {
        &self.result
    }
    pub fn layout(&self) -> PositiveNoticeLayoutV2<'a> {
        self.layout
    }
    pub fn result_signature(&self) -> &'a [u8; SIGNATURE_BYTES] {
        self.result_signature
    }
    pub fn position_signature(&self) -> &'a [u8; SIGNATURE_BYTES] {
        self.position_signature
    }
    /// Result author authentication only. A native caller must independently
    /// authenticate the separate position signature and rooted bond/class/job.
    pub fn authenticate_result(&self) -> Result<AuthenticatedResult<'_>, PositiveNoticeErrorV2> {
        self.result.authenticate(self.key, self.result_signature).map_err(PositiveNoticeErrorV2::Result)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn encode_positive_notice_payload_v2(
    challenge: Hash64,
    producer_bond: PalwBondKeyV2,
    key: &[u8; PUBKEY_BYTES],
    result: &MinerResultCommit,
    layout: &PalwTirLayoutV1,
    result_signature: &[u8; SIGNATURE_BYTES],
    position_signature: &[u8; SIGNATURE_BYTES],
    limits: PositiveNoticeLimitsV2,
) -> Result<Vec<u8>, PositiveNoticeErrorV2> {
    let entries =
        (layout.commit_tiles.len() as u64).checked_add(layout.state_tiles.len() as u64).ok_or(PositiveNoticeErrorV2::Budget)?;
    let layout_bytes = entries.checked_mul(4).and_then(|n| n.checked_add(22)).ok_or(PositiveNoticeErrorV2::Budget)?;
    let total = layout_bytes.checked_add(POSITIVE_NOTICE_OVERHEAD_BYTES_V2 as u64).ok_or(PositiveNoticeErrorV2::Budget)?;
    if entries > u64::from(limits.max_layout_entries)
        || layout_bytes > u64::from(limits.max_layout_bytes)
        || total > u64::from(limits.max_payload_bytes)
        || limits.max_layout_entries == 0
    {
        return Err(PositiveNoticeErrorV2::Budget);
    }
    result.validate_shape().map_err(PositiveNoticeErrorV2::Result)?;
    if borsh::object_length(result).map_err(|_| PositiveNoticeErrorV2::Framing)? != POSITIVE_NOTICE_RESULT_BYTES_V2 {
        return Err(PositiveNoticeErrorV2::Version);
    }
    let mut out = Vec::with_capacity(usize::try_from(total).map_err(|_| PositiveNoticeErrorV2::Budget)?);
    out.extend(CONTRACT_VERSION.to_le_bytes());
    out.extend(CONTRACT_REVISION.to_le_bytes());
    out.extend(challenge.as_byte_slice());
    producer_bond.serialize(&mut out).map_err(|_| PositiveNoticeErrorV2::Framing)?;
    out.extend(key);
    result.serialize(&mut out).map_err(|_| PositiveNoticeErrorV2::Framing)?;
    out.extend((layout_bytes as u32).to_le_bytes());
    layout.serialize(&mut out).map_err(|_| PositiveNoticeErrorV2::Framing)?;
    out.extend(result_signature);
    out.extend(position_signature);
    PositiveNoticePayloadV2::decode(&out, limits)?;
    Ok(out)
}

fn word(bytes: &[u8], at: usize) -> Result<u32, PositiveNoticeErrorV2> {
    let end = at.checked_add(4).ok_or(PositiveNoticeErrorV2::Budget)?;
    Ok(u32::from_le_bytes(
        bytes.get(at..end).ok_or(PositiveNoticeErrorV2::Framing)?.try_into().map_err(|_| PositiveNoticeErrorV2::Framing)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_positive_contract::identity::{ChainScope, Digest, PrincipalId};

    fn fixture() -> (MinerResultCommit, PalwTirLayoutV1, PositiveNoticeLimitsV2) {
        let d = Digest([1; 64]);
        let result = MinerResultCommit {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            scope: ChainScope { network: d, ruleset: d, branch: d },
            job: d,
            execution: d,
            miner: PrincipalId(d),
            registered_class: d,
            artifact: d,
            program: d,
            input: d,
            tokenizer: Digest([0; 64]),
            execution_rules: d,
            selector: d,
            generation_seed: [2; 32],
            temperature_q24: 0,
            prefill: 7,
            decode: 1,
            max_context: 64,
            output: d,
            dependencies: d,
            service_contract: d,
        };
        let layout = PalwTirLayoutV1 {
            version: 1,
            max_context: 64,
            checkpoint_interval: 4,
            h_tile: 8,
            commit_tiles: vec![3, 5],
            state_tiles: vec![7],
        };
        let limits = PositiveNoticeLimitsV2 { max_payload_bytes: 20_000, max_layout_bytes: 1024, max_layout_entries: 64 };
        (result, layout, limits)
    }
    fn encode(result: &MinerResultCommit, layout: &PalwTirLayoutV1, limits: PositiveNoticeLimitsV2) -> Vec<u8> {
        encode_positive_notice_payload_v2(
            Hash64::from_bytes([4; 64]),
            PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::TransactionId::from_bytes([3; 64]), 2)),
            &[5; PUBKEY_BYTES],
            result,
            layout,
            &[6; SIGNATURE_BYTES],
            &[7; SIGNATURE_BYTES],
            limits,
        )
        .unwrap()
    }

    #[test]
    fn literal_result_width_and_borrowed_layout_round_trip() {
        let (result, layout, limits) = fixture();
        assert_eq!(borsh::to_vec(&result).unwrap().len(), POSITIVE_NOTICE_RESULT_BYTES_V2);
        assert_eq!(POSITIVE_NOTICE_OVERHEAD_BYTES_V2, 13_062);
        let bytes = encode(&result, &layout, limits);
        assert_eq!(bytes.len(), POSITIVE_NOTICE_OVERHEAD_BYTES_V2 + 22 + 4 * 3);
        let decoded = PositiveNoticePayloadV2::decode(&bytes, limits).unwrap();
        assert_eq!(decoded.result(), &result);
        assert_eq!(decoded.result().tokenizer, Digest([0; 64]), "attempt tokenizer is not overwritten");
        assert_eq!(decoded.layout().commit_tiles().collect::<Vec<_>>(), layout.commit_tiles);
        assert_eq!(decoded.layout().state_tiles().collect::<Vec<_>>(), layout.state_tiles);
        assert_eq!(decoded.layout().decode_owned_after_reservation().unwrap(), layout);
        assert_eq!(decoded.layout().bytes().as_ptr(), bytes[POSITIVE_NOTICE_PREFIX_BYTES_V2..].as_ptr());
        // Framing with arbitrary bytes never counts as an authenticated Result.
        assert!(decoded.authenticate_result().is_err());
    }

    #[test]
    fn signatures_are_not_part_of_unsigned_notice_identity_material() {
        let (mut result, layout, limits) = fixture();
        let bytes = encode(&result, &layout, limits);
        let mut changed = bytes.clone();
        *changed.last_mut().unwrap() ^= 1;
        changed[bytes.len() - 2 * SIGNATURE_BYTES] ^= 1;
        let a = PositiveNoticePayloadV2::decode(&bytes, limits).unwrap();
        let b = PositiveNoticePayloadV2::decode(&changed, limits).unwrap();
        assert_eq!(a.unsigned_bytes(), b.unsigned_bytes());
        assert_ne!(a.result_signature(), b.result_signature());
        assert_ne!(a.position_signature(), b.position_signature());
        result.service_contract = Digest([8; 64]);
        let new_terms = encode(&result, &layout, limits);
        assert_ne!(
            a.unsigned_bytes(),
            PositiveNoticePayloadV2::decode(&new_terms, limits).unwrap().unsigned_bytes(),
            "funding terms affect identity; codec makes no lottery projection for them"
        );
    }

    #[test]
    fn hostile_counts_lengths_truncation_and_extra_bytes_refuse_before_layout_allocation() {
        let (result, layout, limits) = fixture();
        let bytes = encode(&result, &layout, limits);
        for n in [0, 4, POSITIVE_NOTICE_PREFIX_BYTES_V2, bytes.len() - 1] {
            assert!(PositiveNoticePayloadV2::decode(&bytes[..n], limits).is_err());
        }
        let mut changed = bytes.clone();
        changed.push(0);
        assert!(matches!(PositiveNoticePayloadV2::decode(&changed, limits), Err(PositiveNoticeErrorV2::Framing)));
        let mut changed = bytes.clone();
        changed[POSITIVE_NOTICE_PREFIX_BYTES_V2 - 4..POSITIVE_NOTICE_PREFIX_BYTES_V2].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(PositiveNoticePayloadV2::decode(&changed, limits).is_err());
        let mut changed = bytes.clone();
        let commits_at = POSITIVE_NOTICE_PREFIX_BYTES_V2 + 14;
        changed[commits_at..commits_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(PositiveNoticePayloadV2::decode(&changed, limits), Err(PositiveNoticeErrorV2::Budget)));
        let mut changed = bytes.clone();
        let states_at = POSITIVE_NOTICE_PREFIX_BYTES_V2 + 18 + 8;
        changed[states_at..states_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(PositiveNoticePayloadV2::decode(&changed, limits), Err(PositiveNoticeErrorV2::Budget)));
        assert!(matches!(
            PositiveNoticePayloadV2::decode(&bytes, PositiveNoticeLimitsV2 { max_payload_bytes: bytes.len() as u32 - 1, ..limits }),
            Err(PositiveNoticeErrorV2::Budget)
        ));
    }

    #[test]
    fn version_and_invalid_result_are_refused_without_any_native_permit() {
        let (mut result, mut layout, limits) = fixture();
        let mut bytes = encode(&result, &layout, limits);
        bytes[0] = 1;
        assert!(matches!(PositiveNoticePayloadV2::decode(&bytes, limits), Err(PositiveNoticeErrorV2::Version)));
        result.decode = 0;
        assert!(
            encode_positive_notice_payload_v2(
                Hash64::default(),
                PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::TransactionId::from_bytes([3; 64]), 0)),
                &[5; PUBKEY_BYTES],
                &result,
                &layout,
                &[6; SIGNATURE_BYTES],
                &[7; SIGNATURE_BYTES],
                limits
            )
            .is_err()
        );
        result.decode = 1;
        layout.version = 2;
        assert!(
            encode_positive_notice_payload_v2(
                Hash64::default(),
                PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::TransactionId::from_bytes([3; 64]), 0)),
                &[5; PUBKEY_BYTES],
                &result,
                &layout,
                &[6; SIGNATURE_BYTES],
                &[7; SIGNATURE_BYTES],
                limits
            )
            .is_err()
        );
    }
}
