//! Canonical bodies for the two ordinary tokenizer material ranges. A body
//! digest is supplied by the enclosing signed request. This module does not
//! assign a target, consume a booked slot, or publish accepted material.

use super::PlainBatchTokenizerV2;
use crate::{
    Hash64,
    palw_court_shared_material_intake_v1::MATERIAL_PIPELINED_MAX_BYTES,
    palw_fp_constraint_job_v1::{PalwTokenTableEogOpeningV1, PalwTokenTableOpeningV1},
    palw_native_resource_v1::{
        NativeCoverageV1,
        NativeDecodeErrorV1::{self, Decode, Resource},
        NativeRetainedV1, TirNativeRequestsV1, reserve_native_pending_requests_v1,
    },
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row},
    palw_state_v2::PalwChainStateV2,
    palw_step_leg::{PalwStepOpeningV1, step_range_sibling_count_v1},
    palw_tir_public_owned_v1::palw_tir_public_body_hash_v1,
    palw_tir_public_source_v1::{PalwTirPublicAuthenticatedIdsV1, PalwTirPublicSourceV1},
};
use borsh::BorshSerialize;

type BodyError = NativeDecodeErrorV1<String>;
type BodyResult<T> = Result<T, BodyError>;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PlainTokenizerOpeningMaterialV2 {
    pub(crate) eog: PalwTokenTableEogOpeningV1,
    pub(crate) renderings: Vec<PalwTokenTableOpeningV1>,
}

impl PlainBatchTokenizerV2 {
    pub(crate) fn encode_input_material_v2(&self, ids: &[u32]) -> BodyResult<NativeRetainedV1<Vec<u8>>> {
        let len = self.bounds.input_ids_body_bytes;
        check_body_len(len, len)?;
        self.check_replayed_input_ids_v2(ids)?;
        encode_exact(ids, len)
    }

    pub(crate) fn decode_input_material_v2(&self, body: &[u8], expected_body_hash: Hash64) -> BodyResult<NativeRetainedV1<Vec<u32>>> {
        let mut rest = body;
        check_body_len(body.len() as u64, self.bounds.input_ids_body_bytes)?;
        let words = input_words(&mut rest, self.bounds.input_ids_count, self.vocab)?;
        if !rest.is_empty() || body.len() as u64 != self.bounds.input_ids_body_bytes {
            return Err(bad_body());
        }
        check_body_hash(body, expected_body_hash)?;
        let mut requests = TirNativeRequestsV1::new();
        requests.exact_v1::<u32>(self.bounds.input_ids_count.into()).map_err(Resource)?;
        let lease = reserve_native_pending_requests_v1(requests.bytes_v1()).map_err(Resource)?;
        let ids = NativeRetainedV1::from_owner_reserved_v1(decode_words(words)?, lease);
        self.check_replayed_input_ids_v2(&ids)?;
        Ok(ids)
    }

    pub(crate) fn encode_opening_material_v2(
        &self,
        state: &PalwChainStateV2,
        source: &PalwTirPublicSourceV1,
        ids: &PalwTirPublicAuthenticatedIdsV1,
        renderings: &[PalwTokenTableOpeningV1],
        eog: &PalwTokenTableEogOpeningV1,
    ) -> BodyResult<NativeRetainedV1<Vec<u8>>> {
        let bounds = self.bounds;
        if renderings.len() as u64 != u64::from(bounds.rendering_openings)
            || eog.ids.len() as u64 != u64::from(bounds.eog_ids)
            || eog.opening.siblings.len() as u64 != bounds.eog_siblings
            || !eog.ids.iter().all(|&id| id < self.vocab)
            || renderings.iter().any(|opening| {
                opening.opening.siblings.len() as u64 > bounds.maximum_rendering_siblings
                    || opening.rendering.as_ref().is_some_and(|bytes| bytes.len() as u64 > bounds.maximum_rendering_bytes)
            })
        {
            return Err(bad_body());
        }
        let material = (eog, renderings);
        let len = borsh::object_length(&material).map_err(|_| bad_body())? as u64;
        check_body_len(len, bounds.opening_material_bytes)?;
        self.check_accepted_full_ids(state, source, ids)?;
        self.verify_output_openings_v2(state, source, ids, renderings, eog)?;
        encode_exact(&material, len)
    }

    /// The native caller supplies the completed FullIds for the booked Source.
    /// This checks exact equality with the native IDs row. The caller owns
    /// the scope, causal Finish and acceptance clock checks and the raw body
    /// reservation. This helper owns the decoded operands it returns.
    pub(crate) fn decode_opening_material_v2(
        &self,
        state: &PalwChainStateV2,
        source: &PalwTirPublicSourceV1,
        ids: &PalwTirPublicAuthenticatedIdsV1,
        body: &[u8],
        expected_body_hash: Hash64,
    ) -> BodyResult<NativeRetainedV1<PlainTokenizerOpeningMaterialV2>> {
        check_body_len(body.len() as u64, self.bounds.opening_material_bytes)?;
        self.check_accepted_full_ids(state, source, ids)?;
        let mut rest = body;
        let (eog_ids, eog_opening) = self.read_opening_prefix(&mut rest)?;
        let mut requests = TirNativeRequestsV1::new();
        requests.exact_v1::<PalwTokenTableOpeningV1>(self.bounds.rendering_openings.into()).map_err(Resource)?;
        requests.exact_v1::<u32>(self.bounds.eog_ids.into()).map_err(Resource)?;
        eog_opening.add_requests(&mut requests)?;
        let rendering_bytes = rest;
        for &id in ids.ids_v1() {
            let (rendering, opening) = self.read_rendering(&mut rest, id)?;
            requests.exact_v1::<u8>(rendering.map_or(0, <[u8]>::len) as u64).map_err(Resource)?;
            opening.add_requests(&mut requests)?;
        }
        if !rest.is_empty() {
            return Err(bad_body());
        }
        check_body_hash(body, expected_body_hash)?;
        let lease = reserve_native_pending_requests_v1(requests.bytes_v1()).map_err(Resource)?;
        let eog = PalwTokenTableEogOpeningV1 { ids: decode_words(eog_ids)?, opening: eog_opening.decode()? };
        let mut renderings = exact_vec(ids.ids_v1().len())?;
        let mut rest = rendering_bytes;
        for &id in ids.ids_v1() {
            let (rendering, opening) = self.read_rendering(&mut rest, id)?;
            let rendering = rendering.map(copy_bytes).transpose()?;
            renderings.push(PalwTokenTableOpeningV1 { id, rendering, opening: opening.decode()? });
        }
        let material = NativeRetainedV1::from_owner_reserved_v1(PlainTokenizerOpeningMaterialV2 { eog, renderings }, lease);
        self.verify_output_openings_v2(state, source, ids, &material.renderings, &material.eog)?;
        Ok(material)
    }

    fn check_accepted_full_ids(
        &self,
        state: &PalwChainStateV2,
        source: &PalwTirPublicSourceV1,
        ids: &PalwTirPublicAuthenticatedIdsV1,
    ) -> BodyResult<()> {
        let mismatch = || Decode("ordinary tokenizer material requires the exact accepted Source FullIds".into());
        if ids.source_id_v1() != self.source
            || source.source_id_v1() != self.source
            || ids.ids_v1().len() as u64 != u64::from(self.bounds.rendering_openings)
        {
            return Err(mismatch());
        }
        let key = Key::CourtSharedIds(self.claim);
        let Some(Row::CourtSharedIds(accepted)) = state.staged_court_row_v1(&key) else {
            return Err(mismatch());
        };
        accepted.check(&key).map_err(Decode)?;
        if accepted.rows_root != ids.rows_root_v1() || accepted.ids.as_slice() != ids.ids_v1() {
            return Err(mismatch());
        }
        Ok(())
    }

    fn read_opening_prefix<'a>(&self, rest: &mut &'a [u8]) -> BodyResult<(&'a [u8], OpeningBytes<'a>)> {
        let input_len = rest.len();
        let ids = input_words(rest, self.bounds.eog_ids, self.vocab)?;
        let mut ordered = words(ids);
        if let Some(first) = ordered.next() {
            ordered.try_fold(first, |previous, id| if previous < id { Ok(id) } else { Err(bad_body()) })?;
        }
        let opening = read_opening(rest, u64::from(self.vocab), self.bounds.eog_siblings)?;
        if (input_len - rest.len()) as u64 != self.bounds.eog_opening_bytes || read_u32(rest)? != self.bounds.rendering_openings {
            return Err(bad_body());
        }
        Ok((ids, opening))
    }

    fn read_rendering<'a>(&self, rest: &mut &'a [u8], expected_id: u32) -> BodyResult<(Option<&'a [u8]>, OpeningBytes<'a>)> {
        let input_len = rest.len();
        if read_u32(rest)? != expected_id || expected_id >= self.vocab {
            return Err(bad_body());
        }
        let rendering = match take(rest, 1)?[0] {
            0 => None,
            1 => {
                let len = read_u32(rest)?;
                if u64::from(len) > self.bounds.maximum_rendering_bytes {
                    return Err(bad_body());
                }
                Some(take(rest, len as usize)?)
            }
            _ => return Err(bad_body()),
        };
        let index = u64::from(expected_id);
        let siblings = step_range_sibling_count_v1(u64::from(self.vocab) + 1, index, 1);
        if siblings > self.bounds.maximum_rendering_siblings {
            return Err(bad_body());
        }
        let opening = read_opening(rest, index, siblings)?;
        if (input_len - rest.len()) as u64 > self.bounds.maximum_rendering_opening_bytes {
            return Err(bad_body());
        }
        Ok((rendering, opening))
    }
}

fn bad_body() -> BodyError {
    Decode("ordinary tokenizer material has a noncanonical or out-of-bound body".into())
}

fn check_body_len(len: u64, bound: u64) -> BodyResult<()> {
    if len == 0 || len > bound || bound > MATERIAL_PIPELINED_MAX_BYTES {
        return Err(bad_body());
    }
    Ok(())
}

fn check_body_hash(body: &[u8], expected: Hash64) -> BodyResult<()> {
    if palw_tir_public_body_hash_v1(body) != expected {
        return Err(Decode("ordinary tokenizer material differs from the committed body hash".into()));
    }
    Ok(())
}

fn encode_exact<T: BorshSerialize + ?Sized>(body: &T, len: u64) -> BodyResult<NativeRetainedV1<Vec<u8>>> {
    let lease = reserve_native_pending_requests_v1(len).map_err(Resource)?;
    let mut bytes = exact_vec(len as usize)?;
    bytes.resize(len as usize, 0);
    let mut writer = bytes.as_mut_slice();
    body.serialize(&mut writer).map_err(|_| bad_body())?;
    if !writer.is_empty() {
        return Err(bad_body());
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(bytes, lease))
}

fn exact_vec<T>(count: usize) -> BodyResult<Vec<T>> {
    let mut vec = Vec::new();
    vec.try_reserve_exact(count)
        .map_err(|_| Resource(NativeCoverageV1::unsupported::<Vec<T>>("ordinary tokenizer material", "allocation refused")))?;
    if vec.capacity() != count {
        return Err(Resource(NativeCoverageV1::unsupported::<Vec<T>>(
            "ordinary tokenizer material",
            "capacity differs from the reserved request",
        )));
    }
    Ok(vec)
}

fn copy_bytes(bytes: &[u8]) -> BodyResult<Vec<u8>> {
    let mut value = exact_vec(bytes.len())?;
    value.extend_from_slice(bytes);
    Ok(value)
}

fn take<'a>(rest: &mut &'a [u8], n: usize) -> BodyResult<&'a [u8]> {
    let (head, tail) = rest.split_at_checked(n).ok_or_else(bad_body)?;
    *rest = tail;
    Ok(head)
}

fn read_u32(rest: &mut &[u8]) -> BodyResult<u32> {
    Ok(u32::from_le_bytes(take(rest, 4)?.try_into().expect("fixed u32")))
}

fn words(bytes: &[u8]) -> impl Iterator<Item = u32> + '_ {
    bytes.chunks_exact(4).map(|word| u32::from_le_bytes(word.try_into().expect("fixed u32")))
}

fn input_words<'a>(rest: &mut &'a [u8], count: u32, vocab: u32) -> BodyResult<&'a [u8]> {
    if read_u32(rest)? != count {
        return Err(bad_body());
    }
    let bytes = take(rest, (count as usize).checked_mul(4).ok_or_else(bad_body)?)?;
    if words(bytes).any(|id| id >= vocab) {
        return Err(bad_body());
    }
    Ok(bytes)
}

fn decode_words(bytes: &[u8]) -> BodyResult<Vec<u32>> {
    let mut value = exact_vec(bytes.len() / 4)?;
    value.extend(words(bytes));
    Ok(value)
}

struct OpeningBytes<'a> {
    index: u64,
    hash: Hash64,
    siblings: &'a [u8],
}

impl OpeningBytes<'_> {
    fn add_requests(&self, requests: &mut TirNativeRequestsV1) -> BodyResult<()> {
        requests.exact_v1::<Hash64>((self.siblings.len() / 64) as u64).map_err(Resource)
    }

    fn decode(self) -> BodyResult<PalwStepOpeningV1> {
        let mut siblings = exact_vec(self.siblings.len() / 64)?;
        siblings.extend(self.siblings.chunks_exact(64).map(|bytes| Hash64::from_bytes(bytes.try_into().expect("fixed hash"))));
        Ok(PalwStepOpeningV1 { leaf_index: self.index, leaf_hash: self.hash, siblings })
    }
}

fn read_opening<'a>(rest: &mut &'a [u8], expected_index: u64, expected_siblings: u64) -> BodyResult<OpeningBytes<'a>> {
    let index = u64::from_le_bytes(take(rest, 8)?.try_into().expect("fixed index"));
    if index != expected_index {
        return Err(bad_body());
    }
    let hash = Hash64::from_bytes(take(rest, 64)?.try_into().expect("fixed hash"));
    let count = read_u32(rest)?;
    if u64::from(count) != expected_siblings {
        return Err(bad_body());
    }
    let siblings = take(rest, (count as usize).checked_mul(64).ok_or_else(bad_body)?)?;
    Ok(OpeningBytes { index, hash, siblings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        palw_court_shared_material_intake_v1::batch::plain_tokenizer::PlainBatchTokenizerBoundsV2,
        palw_fp_constraint_job_v1::PalwTokenTableV1,
        palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1},
    };

    fn branch(form: PalwPromptIdsFormV1) -> PlainBatchTokenizerV2 {
        PlainBatchTokenizerV2 {
            claim: Hash64::default(),
            source: Hash64::default(),
            registration: Hash64::default(),
            branch: Hash64::default(),
            acceptance_clocks: (1, 1),
            prompt_form: form,
            prompt_ids_root: prompt_token_ids_commitment_v1(form, &[0, 2]).unwrap(),
            vocab: 4,
            bounds: PlainBatchTokenizerBoundsV2 {
                full_ids_body_bytes: 128,
                input_ids_count: 2,
                input_ids_body_bytes: 12,
                rendering_openings: 3,
                maximum_rendering_opening_bytes: 4 + 1 + 4 + 1 + 8 + 64 + 4 + 3 * 64,
                maximum_rendering_bytes: 1,
                maximum_rendering_siblings: 3,
                eog_opening_bytes: 4 + 2 * 4 + 8 + 64 + 4 + 64,
                eog_ids: 2,
                eog_siblings: 1,
                eog_replay_calls: 0,
                opening_material_bytes: 1024,
                maximum_opening_hashes: 18,
                eog_scratch_bytes: 8,
                input_ids_scratch_bytes: 4096,
            },
        }
    }

    #[test]
    fn bct2_tokenizer_input_material_checks_body_and_native_prompt_commitments() {
        let input = [0u32, 2];
        let canonical = borsh::to_vec(&input.to_vec()).unwrap();
        for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
            let branch = branch(form);
            let encoded = branch.encode_input_material_v2(&input).unwrap();
            assert_eq!(encoded.as_slice(), canonical);
            let digest = palw_tir_public_body_hash_v1(&encoded);
            assert_eq!(branch.decode_input_material_v2(&encoded, digest).unwrap().as_slice(), &input);
            assert!(branch.decode_input_material_v2(&encoded, Hash64::default()).is_err());

            let mut bad_count = canonical.clone();
            bad_count[..4].copy_from_slice(&u32::MAX.to_le_bytes());
            for wrong in [bad_count, borsh::to_vec(&vec![0u32, 3]).unwrap(), borsh::to_vec(&vec![0u32, 4]).unwrap()] {
                assert!(matches!(branch.decode_input_material_v2(&wrong, palw_tir_public_body_hash_v1(&wrong)), Err(Decode(_))));
            }
            let mut trailing = canonical.clone();
            trailing.push(0);
            for wrong in [&canonical[..canonical.len() - 1], &trailing] {
                assert!(branch.decode_input_material_v2(wrong, palw_tir_public_body_hash_v1(wrong)).is_err());
            }
        }
    }

    #[test]
    fn bct2_tokenizer_opening_material_scans_borsh_and_checks_path_geometry() {
        let branch = branch(PalwPromptIdsFormV1::Flat);
        let table = PalwTokenTableV1 {
            entries: vec![None, Some(vec![]), Some(vec![b'a']), None],
            eog_token_ids: vec![0, 3],
            tokenizer_id: Hash64::default(),
        };
        let eog = table.eog_opening().unwrap();
        let generated = [0, 1, 1];
        let renderings: Vec<_> = generated.into_iter().map(|id| table.opening_of(id).unwrap()).collect();
        let canonical = borsh::to_vec(&(&eog, &renderings)).unwrap();
        let encoded = encode_exact(&(&eog, renderings.as_slice()), canonical.len() as u64).unwrap();
        assert_eq!(encoded.as_slice(), canonical);
        let mut rest = canonical.as_slice();
        let (ids, opening) = branch.read_opening_prefix(&mut rest).unwrap();
        assert_eq!(decode_words(ids).unwrap(), eog.ids);
        assert_eq!(opening.decode().unwrap(), eog.opening);
        for expected in &renderings {
            let (rendering, opening) = branch.read_rendering(&mut rest, expected.id).unwrap();
            assert_eq!(rendering, expected.rendering.as_deref());
            assert_eq!(opening.decode().unwrap(), expected.opening);
        }
        assert!(rest.is_empty());

        let first_rendering = branch.bounds.eog_opening_bytes as usize + 4;
        let second_rendering = first_rendering + borsh::object_length(&renderings[0]).unwrap();
        let first_sibling_count = first_rendering + 4 + 1 + 8 + 64;
        let mut invalid = canonical.clone();
        invalid[first_sibling_count..first_sibling_count + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(branch.read_rendering(&mut &invalid[first_rendering..], 0).is_err());
        let mut invalid = canonical.clone();
        invalid[second_rendering + 4] = 2;
        assert!(branch.read_rendering(&mut &invalid[second_rendering..], 1).is_err());
        let mut invalid = canonical.clone();
        invalid[second_rendering + 5..second_rendering + 9].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(branch.read_rendering(&mut &invalid[second_rendering..], 1).is_err());
        assert!(branch.read_rendering(&mut &canonical[first_rendering..], 1).is_err());
        let mut invalid = canonical.clone();
        invalid[8..12].copy_from_slice(&0u32.to_le_bytes());
        assert!(branch.read_opening_prefix(&mut invalid.as_slice()).is_err());
        assert!(branch.read_opening_prefix(&mut &canonical[..first_rendering - 1]).is_err());
    }
}
