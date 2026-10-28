//! One carrier per live session-owned answer step. Accepted chain progress, rather than a private
//! sent bitmap, is the restart/reorg authority. This planner does not authenticate remote state.
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_legacy_answer_chunks_v2::{
    PALW_LEGACY_ANSWER_CHUNK_BYTES_V2, PALW_LEGACY_ANSWER_DECLARE_CONTEXT_V2, PalwLegacyAnswerChunkV2, PalwLegacyAnswerProgressV2,
    palw_legacy_answer_declaration_message_v2, palw_legacy_answer_split_v2,
};
use kaspa_consensus_core::palw_producer_v2::PalwDisclosureDutyV1;
use kaspa_consensus_core::palw_state_v2::{
    PALW_COURT_CLOSE_INCLUSION_MARGIN, PALW_OBJECT_CHUNK_MAX_BYTES, PalwConsensusObjectV2 as Obj,
};

fn stage_key(duty: &PalwDisclosureDutyV1, stage: u32) -> (Hash64, u32, bool) {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-node/owned-da-answer-queue/v2").to_state();
    h.update(duty.claim_id.as_byte_slice());
    h.update(&borsh::to_vec(&(duty.unit, duty.discloser)).expect("serializable duty key"));
    (Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("64 bytes")), stage, true)
}

pub(super) fn queue_key_v2(duty: &PalwDisclosureDutyV1, progress: Option<&PalwLegacyAnswerProgressV2>) -> (Hash64, u32, bool) {
    stage_key(duty, progress.and_then(|p| p.next_index()).map_or(0, |i| u32::from(i) + 1))
}

pub(super) fn object_queue_key_v2(duty: &PalwDisclosureDutyV1, object: &Obj) -> (Hash64, u32, bool) {
    match object {
        Obj::LegacyHeldAnswerChunkV2 { chunk } => stage_key(duty, u32::from(chunk.index) + 1),
        Obj::LegacyHeldAnswerDeclaredV2 { .. } | Obj::LegacyHeldAnsweredV2 { .. } => stage_key(duty, 0),
        _ => super::palw_disclosure_queue_key_v1(duty),
    }
}

pub(super) fn chunk_is_pending_v2(chunk: &PalwLegacyAnswerChunkV2, progress: Option<&PalwLegacyAnswerProgressV2>) -> bool {
    progress.is_some_and(|p| {
        chunk.key() == p.declaration.key()
            && chunk.answer_digest == p.declaration.answer_digest
            && (chunk.index as usize) < p.declaration.chunk_digests.len()
            && p.received & (1u32.checked_shl(u32::from(chunk.index)).unwrap_or(0)) == 0
    })
}

/// Choose exactly one object. The original answer and manifest use deterministic signing in the
/// node, so a rebuilt answer must reproduce the entire accepted declaration before any part is sent.
#[allow(clippy::too_many_arguments)]
pub(super) fn next_answer_object_v2(
    object: Obj,
    progress: Option<&PalwLegacyAnswerProgressV2>,
    now_daa: u64,
    duty_deadline: u64,
    maximum: u64,
    network_domain: &Hash64,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<Obj, String> {
    let Obj::LegacyHeldAnsweredV2 { answer } = object else { return Ok(object) };
    if progress.is_none() {
        let direct = Obj::LegacyHeldAnsweredV2 { answer };
        if borsh::object_length(&direct).map_err(|e| e.to_string())? <= PALW_OBJECT_CHUNK_MAX_BYTES
            && kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&direct).is_ok()
        {
            return Ok(direct);
        }
        let Obj::LegacyHeldAnsweredV2 { answer } = direct else { unreachable!() };
        return plan_parts(*answer, None, now_daa, duty_deadline, maximum, network_domain, sign);
    }
    plan_parts(*answer, progress, now_daa, duty_deadline, maximum, network_domain, sign)
}

// A separate parts planner permits an initial large answer without manufacturing a chain row.
#[allow(clippy::too_many_arguments)]
fn plan_parts(
    answer: kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldAnswerCarriageV2,
    progress: Option<&PalwLegacyAnswerProgressV2>,
    now_daa: u64,
    duty_deadline: u64,
    maximum: u64,
    network_domain: &Hash64,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<Obj, String> {
    let part_bytes = progress.map_or(PALW_LEGACY_ANSWER_CHUNK_BYTES_V2, |p| p.declaration.part_bytes);
    let (declaration, chunks) = palw_legacy_answer_split_v2(&answer, part_bytes, maximum, |d| {
        let message = palw_legacy_answer_declaration_message_v2(network_domain.as_byte_slice(), d);
        sign(message.as_byte_slice(), PALW_LEGACY_ANSWER_DECLARE_CONTEXT_V2)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "no signing key for the answer declaration".into())
    })?;
    let object = if let Some(p) = progress {
        if declaration != p.declaration || p.deadline_daa > duty_deadline {
            return Err("rebuilt answer does not reproduce the accepted manifest/deadline".into());
        }
        let count = chunks.len();
        let allowed = if count == 32 { u32::MAX } else { (1u32 << count) - 1 };
        if p.received & !allowed != 0 {
            return Err("accepted progress bitmap exceeds the declaration".into());
        }
        if now_daa >= p.deadline_daa {
            return Err("accepted multipart answer deadline has passed".into());
        }
        let index = p.next_index().ok_or("a completed answer cannot remain pending")?;
        Obj::LegacyHeldAnswerChunkV2 { chunk: Box::new(chunks.into_iter().nth(index as usize).ok_or("missing answer part")?) }
    } else {
        if now_daa.saturating_add(PALW_COURT_CLOSE_INCLUSION_MARGIN.saturating_mul(chunks.len() as u64)) >= duty_deadline {
            return Err("not enough of the existing DA window remains for multipart answer".into());
        }
        Obj::LegacyHeldAnswerDeclaredV2 { declaration: Box::new(declaration) }
    };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object).map_err(|e| e.to_string())?;
    Ok(object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_legacy_answer_chunks_v2::PalwPendingLegacyAnswerV2;
    use kaspa_consensus_core::palw_legacy_held_da_v2::{PalwLegacyHeldAnswerCarriageV2, PalwLegacyHeldAnswerV2, PalwLegacyHeldUnitV2};
    use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use std::collections::BTreeMap;

    // Transport fixture only: semantic root verification is covered by the real V-fold test.
    fn carriage() -> PalwLegacyHeldAnswerCarriageV2 {
        let backend = super::super::seat_s_tests::floor_backend();
        let (job, ids) = backend.job_for_anchor(Hash64::from_u64_word(4)).unwrap();
        let run = backend.execute(&job, &ids).unwrap();
        let binding = backend.disclose_trace_event(&run.material, u32::MAX, 0).unwrap().binding().clone();
        PalwLegacyHeldAnswerCarriageV2 {
            version: 2,
            claim: Hash64::from_u64_word(10),
            unit: PalwLegacyHeldUnitV2::PromptIds { chunk: 0 },
            binding,
            answer: PalwLegacyHeldAnswerV2::PromptIds { ids: vec![7; 30_000], tree_root: None, siblings: vec![] },
            discloser: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(11), 0)),
            signature: vec![1; 4627],
        }
    }

    #[test]
    fn a_large_answer_declares_once_then_resumes_only_missing_parts_from_accepted_progress() {
        let a = carriage();
        let object = || Obj::LegacyHeldAnsweredV2 { answer: Box::new(a.clone()) };
        let domain = Hash64::from_u64_word(99);
        let Obj::LegacyHeldAnswerDeclaredV2 { declaration } =
            next_answer_object_v2(object(), None, 100, 200, 300_000, &domain, |_, _| Some(vec![2; 4627])).unwrap()
        else {
            panic!("a large answer must declare, not attempt a direct carrier")
        };
        assert!(declaration.chunk_digests.len() >= 2);
        let mut row = PalwPendingLegacyAnswerV2 { declaration: *declaration, deadline_daa: 200, parts: BTreeMap::new() };
        for i in 0..row.declaration.chunk_digests.len() {
            let progress = row.progress();
            let chosen =
                next_answer_object_v2(object(), Some(&progress), 101 + i as u64, 200, 300_000, &domain, |_, _| Some(vec![2; 4627]))
                    .unwrap();
            let Obj::LegacyHeldAnswerChunkV2 { chunk } = chosen else { panic!("resume must send a part") };
            assert_eq!(chunk.index as usize, i);
            assert!(chunk_is_pending_v2(&chunk, Some(&progress)));
            assert!(!chunk_is_pending_v2(&chunk, None));
            // Recreating node policy from only the accepted progress returns identical bytes.
            let again = next_answer_object_v2(object(), Some(&row.progress()), 102 + i as u64, 200, 300_000, &domain, |_, _| {
                Some(vec![2; 4627])
            })
            .unwrap();
            assert_eq!(again, Obj::LegacyHeldAnswerChunkV2 { chunk: chunk.clone() });
            let assembled = row.preview(&chunk).unwrap();
            if i + 1 == row.declaration.chunk_digests.len() {
                assert_eq!(assembled, Some(a.clone()));
            } else {
                assert!(assembled.is_none());
            }
            row.parts.insert(chunk.index, chunk.bytes.clone());
            assert!(!chunk_is_pending_v2(&chunk, Some(&row.progress())));
        }
        assert!(
            next_answer_object_v2(object(), Some(&row.progress()), 110, 200, 300_000, &domain, |_, _| Some(vec![2; 4627])).is_err()
        );
        assert!(next_answer_object_v2(object(), None, 199, 200, 300_000, &domain, |_, _| Some(vec![2; 4627])).is_err());
    }

    #[test]
    fn restart_never_substitutes_another_envelope_or_deadline_and_bounds_before_manifest_signing() {
        let a = carriage();
        let domain = Hash64::from_u64_word(99);
        let (declaration, _) =
            palw_legacy_answer_split_v2(&a, PALW_LEGACY_ANSWER_CHUNK_BYTES_V2, 300_000, |_| Ok(vec![2; 4627])).unwrap();
        let p = PalwLegacyAnswerProgressV2 { declaration, deadline_daa: 200, received: 0 };
        let mut altered = a.clone();
        altered.signature[0] ^= 1;
        assert!(
            next_answer_object_v2(
                Obj::LegacyHeldAnsweredV2 { answer: Box::new(altered) },
                Some(&p),
                101,
                200,
                300_000,
                &domain,
                |_, _| Some(vec![2; 4627])
            )
            .is_err()
        );
        for (now, deadline, bitmap) in [(200, 200, 0), (101, 199, 0), (101, 200, 1u32 << 31)] {
            let mut p = p.clone();
            p.received = bitmap;
            assert!(
                next_answer_object_v2(
                    Obj::LegacyHeldAnsweredV2 { answer: Box::new(a.clone()) },
                    Some(&p),
                    now,
                    deadline,
                    300_000,
                    &domain,
                    |_, _| Some(vec![2; 4627])
                )
                .is_err()
            );
        }
        let exact = borsh::object_length(&a).unwrap() as u64;
        assert!(
            next_answer_object_v2(
                Obj::LegacyHeldAnsweredV2 { answer: Box::new(a) },
                None,
                100,
                200,
                exact - 1,
                &domain,
                |_, _| panic!("oversize rejected before manifest signing")
            )
            .is_err()
        );
    }
}
