//! Inner response envelope for the new tokenizer material grammar. It cannot
//! be decoded as a legacy MaterialTargetV1, nor does parsing admit a case.
//! The future lifecycle dispatcher must allocate an outer tag, authenticate
//! the expected scope from native history and check funding/slots atomically.
use super::{BatchResponsePositionV1, Stage, plain_tokenizer::PlainBatchTokenizerV2};
use crate::{
    Hash64,
    palw_court_shared_material_intake_v1::MATERIAL_PIPELINED_MAX_BYTES,
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1, TirNativeRequestsV1, reserve_native_pending_requests_v1},
    palw_state_v2::PalwBondKeyV2,
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io;

const SIG: usize = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
const VERSION: u16 = 2;
pub(crate) const TOKENIZER_MATERIAL_SIGNATURE_CONTEXT_V2: &[u8] = b"misaka-palw/tokenizer-material/v2/mldsa87";
type Error = NativeDecodeErrorV1<String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) enum TokenizerMaterialKindV2 {
    InputIds,
    OutputOpenings,
}

/// Plain signed coordinates, not an authority token. An accepting writer must
/// compare ALL fields with its independently derived native request. Case and
/// request identities cannot be supplied by the responder as authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct TokenizerMaterialScopeV2 {
    pub claim: Hash64,
    pub scope: Hash64,
    pub branch: Hash64,
    pub case: Hash64,
    pub request_root: Hash64,
    pub ordinal: u64,
    pub range_index: u32,
    pub kind: TokenizerMaterialKindV2,
}

impl TokenizerMaterialScopeV2 {
    fn check_shape_v2(self) -> Result<(), String> {
        if [self.claim, self.scope, self.branch, self.case, self.request_root].contains(&Hash64::default())
            || self.range_index as usize >= crate::palw_batch_court_v1::MAX_BATCH_RANGES_V1
        {
            return Err("tokenizer material has an empty or invalid scope".into());
        }
        Ok(())
    }

    /// Check branch and canonical partition position in addition to comparing
    /// the native expected scope. This does not check its paid service slot.
    pub(crate) fn check_expected_v2(self, expected: Self, branch: &PlainBatchTokenizerV2) -> Result<u64, String> {
        self.check_shape_v2()?;
        let (position, maximum) = tokenizer_material_position_v2(branch, self.kind)?;
        if self != expected
            || self.claim != branch.claim_id_v2()
            || self.branch != branch.branch_id_v2()
            || (self.ordinal, self.range_index) != (position.demand_ordinal, position.range_index)
        {
            return Err("tokenizer material differs from its native expected request".into());
        }
        Ok(maximum)
    }

    pub(crate) fn target_id_v2(self) -> Result<Hash64, String> {
        self.check_shape_v2()?;
        stream_digest(b"misaka-palw/tokenizer-material-target/v2", &self)
    }

    /// Scope includes the accepted request; responder binds a separate draft.
    /// An unfinished response cannot occupy another responder's draft key.
    pub(crate) fn draft_id_v2(self, submitter: PalwBondKeyV2, total: u64, body_hash: Hash64) -> Result<Hash64, String> {
        if !(1..=MATERIAL_PIPELINED_MAX_BYTES).contains(&total) || body_hash == Hash64::default() {
            return Err("tokenizer material draft size or digest".into());
        }
        stream_digest(b"misaka-palw/tokenizer-material-draft/v2", &(self.target_id_v2()?, submitter, total, body_hash))
    }
}

pub(crate) fn tokenizer_material_position_v2(
    branch: &PlainBatchTokenizerV2,
    kind: TokenizerMaterialKindV2,
) -> Result<(BatchResponsePositionV1, u64), String> {
    let b = branch.bounds_v2();
    let maximums = [b.input_ids_body_bytes, b.opening_material_bytes];
    if maximums.iter().any(|n| !(1..=MATERIAL_PIPELINED_MAX_BYTES).contains(n)) {
        return Err("tokenizer material exceeds one bounded range".into());
    }
    let wanted = match kind {
        TokenizerMaterialKindV2::InputIds => 0,
        TokenizerMaterialKindV2::OutputOpenings => 1,
    };
    let mut offset = 0;
    for (group, ranges) in super::native_ranges::partition_maximums_v1(&maximums).enumerate() {
        if wanted < offset + ranges.len() {
            return Ok((
                BatchResponsePositionV1 {
                    demand_ordinal: super::admission::plain_tokenizer::TOKENIZER_DEMAND_ORDINAL_V2
                        .checked_sub(group as u64)
                        .ok_or("tokenizer material ordinal overflow")?,
                    range_index: (wanted - offset) as u32,
                },
                maximums[wanted],
            ));
        }
        offset += ranges.len();
    }
    Err("tokenizer material partition missing".into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub(crate) enum TokenizerMaterialActionV2<'a> {
    Begin { total_bytes: u64, body_hash: Hash64 },
    Part { draft_id: Hash64, offset: u64, bytes: &'a [u8] },
    Finish { draft_id: Hash64 },
}

impl TokenizerMaterialActionV2<'_> {
    fn check_v2(self) -> Result<(), String> {
        let bad = || "invalid tokenizer material action".to_string();
        match self {
            Self::Begin { total_bytes, body_hash } => {
                if !(1..=MATERIAL_PIPELINED_MAX_BYTES).contains(&total_bytes) || body_hash == Hash64::default() {
                    return Err(bad());
                }
            }
            Self::Part { draft_id, offset, bytes } => {
                if draft_id == Hash64::default()
                    || bytes.is_empty()
                    || bytes.len() as u64 > super::PART
                    || offset % super::PART != 0
                    || offset.checked_add(bytes.len() as u64).is_none_or(|end| end > MATERIAL_PIPELINED_MAX_BYTES)
                {
                    return Err(bad());
                }
            }
            Self::Finish { draft_id } if draft_id == Hash64::default() => return Err(bad()),
            Self::Finish { .. } => {}
        }
        Ok(())
    }
}

/// Borrows the literal carrier. Malformed length prefixes never allocate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TokenizerMaterialEnvelopeV2<'a> {
    pub scope: TokenizerMaterialScopeV2,
    pub submitter: PalwBondKeyV2,
    pub action: TokenizerMaterialActionV2<'a>,
    pub signature: &'a [u8],
}

impl TokenizerMaterialEnvelopeV2<'_> {
    pub(crate) fn signing_message_v2(&self, network: Hash64) -> Result<Hash64, String> {
        self.scope.check_shape_v2()?;
        self.action.check_v2()?;
        if network == Hash64::default() {
            return Err("tokenizer material network absent".into());
        }
        stream_digest(b"misaka-palw/tokenizer-material-signing/v2", &(network, VERSION, self.scope, self.submitter, self.action))
    }

    /// Key lookup, Active standing and funding are the native writer's duties.
    pub(crate) fn verify_signature_v2(&self, network: Hash64, pubkey: &[u8]) -> Result<(), String> {
        if self.signature.len() != SIG
            || !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
                pubkey,
                self.signing_message_v2(network)?.as_byte_slice(),
                self.signature,
                TOKENIZER_MATERIAL_SIGNATURE_CONTEXT_V2,
            )
        {
            return Err("invalid tokenizer material signature".into());
        }
        Ok(())
    }

    pub(crate) fn encode_v2(&self) -> Result<NativeRetainedV1<Vec<u8>>, Error> {
        self.scope.check_shape_v2().map_err(Error::Decode)?;
        self.action.check_v2().map_err(Error::Decode)?;
        if self.signature.len() != SIG {
            return Err(Error::Decode("tokenizer material signature width".into()));
        }
        let value = (VERSION, self.scope, self.submitter, self.action, self.signature);
        let bytes = borsh::object_length(&value).map_err(|e| Error::Decode(e.to_string()))?;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<Vec<u8>>(1).map_err(Error::Resource)?;
        q.exact_v1::<u8>(bytes as u64).map_err(Error::Resource)?;
        let owner = reserve_native_pending_requests_v1(q.bytes_v1()).map_err(Error::Resource)?;
        let mut out = Vec::new();
        out.try_reserve_exact(bytes).map_err(|_| {
            Error::Resource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<Vec<u8>>(
                "tokenizer material encoding",
                "exact allocation refused",
            ))
        })?;
        if out.capacity() != bytes {
            return Err(Error::Resource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<Vec<u8>>(
                "tokenizer material encoding",
                "capacity differs from the reserved request",
            )));
        }
        value.serialize(&mut out).map_err(|e| Error::Decode(e.to_string()))?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(out, owner))
    }
}

fn take<'a>(body: &mut &'a [u8], count: usize) -> io::Result<&'a [u8]> {
    if count > body.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "short tokenizer material"));
    }
    let (value, rest) = body.split_at(count);
    *body = rest;
    Ok(value)
}

pub(crate) fn decode_tokenizer_material_envelope_v2(mut body: &[u8]) -> io::Result<TokenizerMaterialEnvelopeV2<'_>> {
    fn invalid(why: impl Into<String>) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, why.into())
    }
    if u16::deserialize_reader(&mut body)? != VERSION {
        return Err(invalid("tokenizer material version"));
    }
    let scope = TokenizerMaterialScopeV2::deserialize_reader(&mut body)?;
    scope.check_shape_v2().map_err(invalid)?;
    let submitter = PalwBondKeyV2::deserialize_reader(&mut body)?;
    let action = match u8::deserialize_reader(&mut body)? {
        0 => TokenizerMaterialActionV2::Begin {
            total_bytes: u64::deserialize_reader(&mut body)?,
            body_hash: Hash64::deserialize_reader(&mut body)?,
        },
        1 => {
            let draft_id = Hash64::deserialize_reader(&mut body)?;
            let offset = u64::deserialize_reader(&mut body)?;
            let count = u32::deserialize_reader(&mut body)? as usize;
            if count == 0 || count as u64 > super::PART {
                return Err(invalid("tokenizer material part width"));
            }
            TokenizerMaterialActionV2::Part { draft_id, offset, bytes: take(&mut body, count)? }
        }
        2 => TokenizerMaterialActionV2::Finish { draft_id: Hash64::deserialize_reader(&mut body)? },
        _ => return Err(invalid("tokenizer material action tag")),
    };
    action.check_v2().map_err(invalid)?;
    if u32::deserialize_reader(&mut body)? as usize != SIG {
        return Err(invalid("tokenizer material signature width"));
    }
    let signature = take(&mut body, SIG)?;
    if !body.is_empty() {
        return Err(invalid("trailing tokenizer material bytes"));
    }
    Ok(TokenizerMaterialEnvelopeV2 { scope, submitter, action, signature })
}

/// Exact inner response length, plus the five-byte outer Batch prefix/tag
/// allowance. No outer tag is assigned or accepted by this module.
pub(super) fn tokenizer_response_wire_bytes_v2(stage: Stage, part_bytes: u64) -> Result<u64, String> {
    let fixed = 5 + 2 + 5 * 64 + 8 + 4 + 1 + 68 + 1 + 4 + SIG as u64;
    let action = match stage {
        Stage::Begin => 8 + 64,
        Stage::Part(_) if (1..=super::PART).contains(&part_bytes) => 64 + 8 + 4 + part_bytes,
        Stage::Part(_) => return Err("tokenizer material part size".into()),
        Stage::Finish => 64,
    };
    Ok(fixed + action)
}

/// Checked immutable Begin facts, not proof of a paid/accepted Begin. The
/// writer stores these only after its signature, standing and slot checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TokenizerMaterialDraftV2 {
    scope: TokenizerMaterialScopeV2,
    submitter: PalwBondKeyV2,
    total: u64,
    body_hash: Hash64,
    id: Hash64,
}

impl TokenizerMaterialDraftV2 {
    pub(crate) fn from_begin_v2(
        begin: &TokenizerMaterialEnvelopeV2<'_>,
        expected: TokenizerMaterialScopeV2,
        branch: &PlainBatchTokenizerV2,
        requested_bytes: u64,
    ) -> Result<Self, String> {
        let maximum = begin.scope.check_expected_v2(expected, branch)?;
        let TokenizerMaterialActionV2::Begin { total_bytes, body_hash } = begin.action else {
            return Err("tokenizer material expected Begin".into());
        };
        if total_bytes != requested_bytes
            || total_bytes > maximum
            || (expected.kind == TokenizerMaterialKindV2::InputIds && total_bytes != maximum)
        {
            return Err("tokenizer Begin changes the accepted range length".into());
        }
        let id = begin.scope.draft_id_v2(begin.submitter, total_bytes, body_hash)?;
        Ok(Self { scope: begin.scope, submitter: begin.submitter, total: total_bytes, body_hash, id })
    }

    pub(crate) fn id_v2(self) -> Hash64 {
        self.id
    }
    pub(crate) fn body_hash_v2(self) -> Hash64 {
        self.body_hash
    }

    pub(crate) fn check_response_v2(self, response: &TokenizerMaterialEnvelopeV2<'_>) -> Result<(), String> {
        if response.scope != self.scope || response.submitter != self.submitter {
            return Err("tokenizer response changes the accepted draft scope or responder".into());
        }
        response.action.check_v2()?;
        match response.action {
            TokenizerMaterialActionV2::Part { draft_id, offset, bytes } => {
                let remaining = self.total.checked_sub(offset).filter(|n| *n != 0).ok_or("tokenizer part offset outside draft")?;
                if draft_id != self.id || bytes.len() as u64 != remaining.min(super::PART) {
                    return Err("tokenizer part changes the accepted draft geometry".into());
                }
            }
            TokenizerMaterialActionV2::Finish { draft_id } if draft_id == self.id => {}
            _ => return Err("tokenizer response does not name the accepted draft".into()),
        }
        Ok(())
    }

    /// Assemble bounded canonical parts without publishing a completion row.
    /// Callers supply signature-checked parts from this draft and must run the
    /// typed input/opening decoder before storing authenticated completion.
    pub(crate) fn assemble_v2(
        self,
        parts: &[TokenizerMaterialEnvelopeV2<'_>],
        finish: &TokenizerMaterialEnvelopeV2<'_>,
    ) -> Result<NativeRetainedV1<Vec<u8>>, Error> {
        self.check_response_v2(finish).map_err(Error::Decode)?;
        if !matches!(finish.action, TokenizerMaterialActionV2::Finish { .. }) || parts.len() as u64 != self.total.div_ceil(super::PART)
        {
            return Err(Error::Decode("tokenizer assembly requires every part and Finish".into()));
        }
        for (i, part) in parts.iter().enumerate() {
            self.check_response_v2(part).map_err(Error::Decode)?;
            if !matches!(part.action, TokenizerMaterialActionV2::Part { offset, .. } if offset == i as u64 * super::PART) {
                return Err(Error::Decode("tokenizer assembly part order or duplicate".into()));
            }
        }
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<Vec<u8>>(1).map_err(Error::Resource)?;
        q.exact_v1::<u8>(self.total).map_err(Error::Resource)?;
        let owner = reserve_native_pending_requests_v1(q.bytes_v1()).map_err(Error::Resource)?;
        let mut body = Vec::new();
        body.try_reserve_exact(self.total as usize).map_err(|_| {
            Error::Resource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<Vec<u8>>(
                "tokenizer material assembly",
                "exact allocation refused",
            ))
        })?;
        if body.capacity() as u64 != self.total {
            return Err(Error::Resource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<Vec<u8>>(
                "tokenizer material assembly",
                "capacity differs from the reserved request",
            )));
        }
        for part in parts {
            if let TokenizerMaterialActionV2::Part { bytes, .. } = part.action {
                body.extend_from_slice(bytes);
            }
        }
        if crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body) != self.body_hash {
            return Err(Error::Decode("tokenizer assembly body hash mismatch".into()));
        }
        Ok(NativeRetainedV1::from_owner_reserved_v1(body, owner))
    }
}

#[cfg(test)]
pub(crate) fn exercise_tokenizer_transport_fixture_v2(
    branch: &PlainBatchTokenizerV2,
    kind: TokenizerMaterialKindV2,
    body: &[u8],
) -> NativeRetainedV1<Vec<u8>> {
    let h = Hash64::from_u64_word;
    let (position, _) = tokenizer_material_position_v2(branch, kind).unwrap();
    // Request and funding are fixture inputs, not an accepted public Open.
    let scope = TokenizerMaterialScopeV2 {
        claim: branch.claim_id_v2(),
        scope: h(901),
        branch: branch.branch_id_v2(),
        case: h(902),
        request_root: h(903),
        ordinal: position.demand_ordinal,
        range_index: position.range_index,
        kind,
    };
    let submitter = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(904), 0));
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([127; 32]);
    let sign = |action| {
        let unsigned = TokenizerMaterialEnvelopeV2 { scope, submitter, action, signature: &[] };
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            unsigned.signing_message_v2(h(905)).unwrap().as_byte_slice(),
            TOKENIZER_MATERIAL_SIGNATURE_CONTEXT_V2,
            [128; 32],
        )
        .unwrap();
        TokenizerMaterialEnvelopeV2 { signature: signature.as_ref(), ..unsigned }.encode_v2().unwrap()
    };
    let hash = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(body);
    let begin_bytes = sign(TokenizerMaterialActionV2::Begin { total_bytes: body.len() as u64, body_hash: hash });
    let begin = decode_tokenizer_material_envelope_v2(&begin_bytes).unwrap();
    begin.verify_signature_v2(h(905), key.verification_key.as_ref()).unwrap();
    let draft = TokenizerMaterialDraftV2::from_begin_v2(&begin, scope, branch, body.len() as u64).unwrap();
    assert_eq!(draft.body_hash_v2(), hash);
    assert!(TokenizerMaterialDraftV2::from_begin_v2(&begin, scope, branch, body.len() as u64 + 1).is_err());
    assert!(
        TokenizerMaterialDraftV2::from_begin_v2(&begin, TokenizerMaterialScopeV2 { case: h(999), ..scope }, branch, body.len() as u64)
            .is_err()
    );
    let wire: Vec<_> = body
        .chunks(super::PART as usize)
        .enumerate()
        .map(|(index, bytes)| {
            sign(TokenizerMaterialActionV2::Part { draft_id: draft.id_v2(), offset: index as u64 * super::PART, bytes })
        })
        .collect();
    let parts: Vec<_> = wire
        .iter()
        .map(|wire| {
            let part = decode_tokenizer_material_envelope_v2(wire).unwrap();
            part.verify_signature_v2(h(905), key.verification_key.as_ref()).unwrap();
            part
        })
        .collect();
    let finish_bytes = sign(TokenizerMaterialActionV2::Finish { draft_id: draft.id_v2() });
    let finish = decode_tokenizer_material_envelope_v2(&finish_bytes).unwrap();
    finish.verify_signature_v2(h(905), key.verification_key.as_ref()).unwrap();
    assert!(draft.assemble_v2(&parts[..parts.len() - 1], &finish).is_err());
    let changed = TokenizerMaterialEnvelopeV2 { scope: TokenizerMaterialScopeV2 { case: h(999), ..scope }, ..finish };
    assert!(draft.assemble_v2(&parts, &changed).is_err());
    let assembled = draft.assemble_v2(&parts, &finish).unwrap();
    assert_eq!(assembled.as_slice(), body);
    assembled
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> TokenizerMaterialScopeV2 {
        let h = Hash64::from_u64_word;
        TokenizerMaterialScopeV2 {
            claim: h(1),
            scope: h(2),
            branch: h(3),
            case: h(4),
            request_root: h(5),
            ordinal: super::super::admission::plain_tokenizer::TOKENIZER_DEMAND_ORDINAL_V2,
            range_index: 0,
            kind: TokenizerMaterialKindV2::InputIds,
        }
    }
    fn signer() -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(8), 0))
    }

    #[test]
    fn tokenizer_envelope_binds_every_coordinate_and_preserves_literal_lengths() {
        let h = Hash64::from_u64_word;
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([125; 32]);
        let payload = [1; 19];
        let actions = [
            (Stage::Begin, TokenizerMaterialActionV2::Begin { total_bytes: 19, body_hash: h(9) }),
            (Stage::Part(0), TokenizerMaterialActionV2::Part { draft_id: h(10), offset: 0, bytes: &payload }),
            (Stage::Finish, TokenizerMaterialActionV2::Finish { draft_id: h(10) }),
        ];
        for (stage, action) in actions {
            let unsigned = TokenizerMaterialEnvelopeV2 { scope: scope(), submitter: signer(), action, signature: &[] };
            let sig = libcrux_ml_dsa::ml_dsa_87::sign(
                &key.signing_key,
                unsigned.signing_message_v2(h(11)).unwrap().as_byte_slice(),
                TOKENIZER_MATERIAL_SIGNATURE_CONTEXT_V2,
                [126; 32],
            )
            .unwrap();
            let signed = TokenizerMaterialEnvelopeV2 { signature: sig.as_ref(), ..unsigned };
            let encoded = signed.encode_v2().unwrap();
            assert_eq!(encoded.len() as u64 + 5, tokenizer_response_wire_bytes_v2(stage, payload.len() as u64).unwrap());
            let decoded = decode_tokenizer_material_envelope_v2(&encoded).unwrap();
            assert_eq!(decoded, signed);
            decoded.verify_signature_v2(h(11), key.verification_key.as_ref()).unwrap();
            assert!(decoded.verify_signature_v2(h(12), key.verification_key.as_ref()).is_err());
            for i in 0..8 {
                let mut changed = decoded;
                match i {
                    0 => changed.scope.claim = h(99),
                    1 => changed.scope.scope = h(99),
                    2 => changed.scope.branch = h(99),
                    3 => changed.scope.case = h(99),
                    4 => changed.scope.request_root = h(99),
                    5 => changed.scope.ordinal -= 1,
                    6 => changed.scope.range_index += 1,
                    7 => changed.scope.kind = TokenizerMaterialKindV2::OutputOpenings,
                    _ => unreachable!(),
                }
                assert!(changed.verify_signature_v2(h(11), key.verification_key.as_ref()).is_err());
            }
            assert!(decode_tokenizer_material_envelope_v2(&encoded[..encoded.len() - 1]).is_err());
            let mut trailing = encoded.to_vec();
            trailing.push(0);
            assert!(decode_tokenizer_material_envelope_v2(&trailing).is_err());
            let mut wrong = encoded.to_vec();
            wrong[0] = 1;
            assert!(decode_tokenizer_material_envelope_v2(&wrong).is_err());
        }
        let a = scope();
        let b = TokenizerMaterialScopeV2 { case: h(90), ..a };
        assert_ne!(a.target_id_v2().unwrap(), b.target_id_v2().unwrap());
        assert_ne!(a.draft_id_v2(signer(), 19, h(9)).unwrap(), b.draft_id_v2(signer(), 19, h(9)).unwrap());
        let other = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(8), 1));
        assert_ne!(a.draft_id_v2(signer(), 19, h(9)).unwrap(), a.draft_id_v2(other, 19, h(9)).unwrap());
    }

    #[test]
    fn tokenizer_envelope_rejects_unbounded_parts_without_allocating() {
        let sig = [0; SIG];
        let body = [0; 1];
        let envelope = TokenizerMaterialEnvelopeV2 {
            scope: scope(),
            submitter: signer(),
            action: TokenizerMaterialActionV2::Part { draft_id: Hash64::from_u64_word(9), offset: 0, bytes: &body },
            signature: &sig,
        };
        let encoded = envelope.encode_v2().unwrap();
        let part_count_offset = 2 + borsh::object_length(&scope()).unwrap() + borsh::object_length(&signer()).unwrap() + 1 + 64 + 8;
        let mut bomb = encoded.to_vec();
        bomb[part_count_offset..part_count_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_tokenizer_material_envelope_v2(&bomb).is_err());
        assert!(
            TokenizerMaterialActionV2::Part { draft_id: Hash64::from_u64_word(9), offset: u64::MAX, bytes: &body }.check_v2().is_err()
        );
    }

    #[test]
    fn tokenizer_assembly_rejects_gaps_duplicates_foreign_drafts_and_wrong_hash() {
        let body = vec![7; super::super::PART as usize + 3];
        let scope = scope();
        let total = body.len() as u64;
        let body_hash = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body);
        let id = scope.draft_id_v2(signer(), total, body_hash).unwrap();
        let draft = TokenizerMaterialDraftV2 { scope, submitter: signer(), total, body_hash, id };
        let first = TokenizerMaterialEnvelopeV2 {
            scope,
            submitter: signer(),
            signature: &[],
            action: TokenizerMaterialActionV2::Part { draft_id: id, offset: 0, bytes: &body[..super::super::PART as usize] },
        };
        let last = TokenizerMaterialEnvelopeV2 {
            action: TokenizerMaterialActionV2::Part {
                draft_id: id,
                offset: super::super::PART,
                bytes: &body[super::super::PART as usize..],
            },
            ..first
        };
        let finish = TokenizerMaterialEnvelopeV2 { action: TokenizerMaterialActionV2::Finish { draft_id: id }, ..first };
        assert_eq!(draft.assemble_v2(&[first, last], &finish).unwrap().as_slice(), body);
        assert!(draft.assemble_v2(&[last, first], &finish).is_err());
        assert!(draft.assemble_v2(&[first, first], &finish).is_err());
        assert!(draft.assemble_v2(&[first], &finish).is_err());
        let wrong_responder = TokenizerMaterialEnvelopeV2 {
            submitter: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(88), 1)),
            ..last
        };
        assert!(draft.assemble_v2(&[first, wrong_responder], &finish).is_err());
        let corrupt = [8; 3];
        let wrong_body = TokenizerMaterialEnvelopeV2 {
            action: TokenizerMaterialActionV2::Part { draft_id: id, offset: super::super::PART, bytes: &corrupt },
            ..last
        };
        assert!(draft.assemble_v2(&[first, wrong_body], &finish).is_err());
    }
}
