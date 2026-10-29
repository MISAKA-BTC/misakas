//! Transaction accounting carriage only. The served material's legacy FPA1 codec is not used here.
use super::PALW_FP_ANSWER_ENVELOPE_MAGIC_V1;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, reserve_native_requests_v1};

/// A completely checked, allocation-free accounting envelope. Borrowed bytes cannot authorize a claim or a payment.
#[derive(Clone, Copy, Debug)]
pub struct PalwFpAnswerEnvelopeViewV1<'a> {
    pub inner: &'a [u8],
    answer: &'a [u8],
}
impl PalwFpAnswerEnvelopeViewV1<'_> {
    pub fn answer_len_v1(&self) -> usize {
        self.answer.len() / 4
    }
    pub fn answer_ids_v1(&self) -> impl ExactSizeIterator<Item = u32> + '_ {
        self.answer.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().expect("four-byte chunk")))
    }
    /// Admission precedes allocation, and the returned owner keeps it through extraction and transition use.
    pub fn retain_answer_v1(&self) -> Result<NativeRetainedV1<Vec<u32>>, NativeCoverageV1> {
        self.retain_answer_with_v1(reserve_native_requests_v1)
    }
    fn retain_answer_with_v1(
        &self,
        reserve: impl FnOnce(u64) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, NativeCoverageV1>,
    ) -> Result<NativeRetainedV1<Vec<u32>>, NativeCoverageV1> {
        let bytes = self
            .answer
            .len()
            .checked_add(std::mem::size_of::<NativeRetainedV1<Vec<u32>>>())
            .ok_or_else(|| NativeCoverageV1::overflow::<Vec<u32>>("FPA1 answer owner"))?;
        let lease = reserve(bytes as u64)?;
        let mut ids = Vec::with_capacity(self.answer_len_v1());
        ids.extend(self.answer_ids_v1());
        Ok(NativeRetainedV1::from_owner_reserved_v1(ids, lease))
    }
}

/// Exactly one outer accounting envelope. Inner ceilings belong to their existing wire schemas;
/// all framing, count arithmetic and EOF checks finish before either payload is copied.
pub fn palw_fp_answer_envelope_view_v1(payload: &[u8], max_answer: usize) -> Option<PalwFpAnswerEnvelopeViewV1<'_>> {
    let rest = payload.strip_prefix(PALW_FP_ANSWER_ENVELOPE_MAGIC_V1)?;
    let len = u32::from_le_bytes(rest.get(..4)?.try_into().ok()?) as usize;
    let inner = rest.get(4..4usize.checked_add(len)?)?;
    if inner.is_empty() || inner.starts_with(PALW_FP_ANSWER_ENVELOPE_MAGIC_V1) {
        return None;
    }
    let max_inner = if inner.starts_with(b"TFP1") {
        crate::palw_tir_public_fp_envelope_v1::PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1
    } else if inner.starts_with(b"HFP1") {
        crate::palw_history_custody_v1::PALW_HISTORY_FP_ENVELOPE_MAX_BYTES_V1
    } else if inner.starts_with(b"TFO1") {
        crate::palw_tir_funded_original_envelope_v1::PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1
    } else {
        crate::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES
    };
    if len > max_inner {
        return None;
    }
    let tail = rest.get(4usize.checked_add(len)?..)?;
    let count = u32::from_le_bytes(tail.get(..4)?.try_into().ok()?) as usize;
    if count > max_answer || tail.len() != 4usize.checked_add(count.checked_mul(4)?)? {
        return None;
    }
    Some(PalwFpAnswerEnvelopeViewV1 { inner, answer: &tail[4..] })
}

/// A transaction-local answer collection. Each insertion retains both the answer ticket and its typed BTree membership.
/// No implicit deep Clone: rehearsal and canonical extras obtain their own reservations before copying.
#[derive(Default)]
pub struct PalwFpClaimAnswersV1 {
    answers: std::collections::BTreeMap<
        crate::Hash64,
        (NativeRetainedV1<Vec<u32>>, Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>),
    >,
    /// First accepted carrier of each claim. A later identical answer does not replace it.
    carriers: std::collections::BTreeMap<crate::Hash64, crate::Hash64>,
}
/// Scoped reservation for *only the answer overlay*. Existing extras fields keep their original admission contract;
/// this is deliberately not a `NativeRetainedV1<Extras>` and has no ownership extraction API.
pub struct PalwFpAnswerExtrasV1 {
    extras: crate::palw_state_v2::PalwTransitionExtrasV1,
    _answer_lease: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
}
impl std::ops::Deref for PalwFpAnswerExtrasV1 {
    type Target = crate::palw_state_v2::PalwTransitionExtrasV1;
    fn deref(&self) -> &Self::Target {
        &self.extras
    }
}
impl PalwFpClaimAnswersV1 {
    pub fn insert_v1(&mut self, claim: crate::Hash64, answer: NativeRetainedV1<Vec<u32>>) -> Result<bool, NativeCoverageV1> {
        if let Some((existing, _)) = self.answers.get(&claim) {
            return Ok(existing.as_slice() == answer.as_slice());
        }
        let bytes = crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<
            crate::Hash64,
            (NativeRetainedV1<Vec<u32>>, Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>),
        >();
        let lease = reserve_native_requests_v1(bytes as u64)?;
        self.answers.insert(claim, (answer, lease));
        Ok(true)
    }
    /// Record the transaction that carried this claim's accepted answer. The first id stays.
    pub fn note_carrier_v1(&mut self, claim: crate::Hash64, carrier: crate::Hash64) {
        self.carriers.entry(claim).or_insert(carrier);
    }
    /// Move already constructed extras, then reserve the typed answer copies before insertion.
    /// The caller retains this owner across the full fold, rather than extracting a bare map.
    pub fn attach_v1(
        &self,
        mut extras: crate::palw_state_v2::PalwTransitionExtrasV1,
    ) -> Result<PalwFpAnswerExtrasV1, NativeCoverageV1> {
        if self.answers.is_empty() {
            return Ok(PalwFpAnswerExtrasV1 { extras, _answer_lease: None });
        }
        let mut bytes = std::mem::size_of::<PalwFpAnswerExtrasV1>() as u64;
        for (ids, _) in self.answers.values() {
            let row = (crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<crate::Hash64, Vec<u32>>() as u64)
                .checked_add((ids.len() as u64).checked_mul(4).ok_or_else(|| NativeCoverageV1::overflow::<Self>("FPA1 extras"))?)
                .ok_or_else(|| NativeCoverageV1::overflow::<Self>("FPA1 extras"))?;
            bytes = bytes.checked_add(row).ok_or_else(|| NativeCoverageV1::overflow::<Self>("FPA1 extras"))?;
        }
        let carrier_row = crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<crate::Hash64, crate::Hash64>() as u64;
        bytes = bytes
            .checked_add(carrier_row.checked_mul(self.carriers.len() as u64).ok_or_else(|| NativeCoverageV1::overflow::<Self>("FPA1 carrier"))?)
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>("FPA1 carrier"))?;
        let lease = reserve_native_requests_v1(bytes)?;
        for (claim, (ids, _)) in &self.answers {
            extras.fp_claim_answers.insert(*claim, ids.to_vec());
        }
        extras.fp_winning_carriers.clone_from(&self.carriers);
        Ok(PalwFpAnswerExtrasV1 { extras, _answer_lease: lease })
    }
    pub fn compatible_v1(&self, other: &Self) -> bool {
        other.answers.iter().all(|(claim, (ids, _))| self.answers.get(claim).is_none_or(|(old, _)| old.as_slice() == ids.as_slice()))
    }
    /// A contradictory answer is a semantic refusal (`false`), never a capacity error. Check before any mutation.
    pub fn append_v1(&mut self, other: Self) -> Result<bool, NativeCoverageV1> {
        if !self.compatible_v1(&other) {
            return Ok(false);
        }
        for (claim, (answer, _membership)) in other.answers {
            self.insert_v1(claim, answer)?;
        }
        for (claim, carrier) in other.carriers {
            self.carriers.entry(claim).or_insert(carrier);
        }
        Ok(true)
    }
}

/// Extra extraction workspace introduced by accounting carriage: the original-payload transaction views and
/// the transaction views (answers are allocated only after authenticated extraction and count agreement). These are transient; the lease stays in the real extractor's scope.
/// Counts come from actual accepted transaction fields, not a caller's raw-byte growth factor.
pub fn palw_fp_answer_extraction_workspace_v1(
    txs: &[crate::tx::Transaction],
    armed: bool,
) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, NativeCoverageV1> {
    use crate::tx::{Transaction, TransactionInput, TransactionOutput};
    use std::mem::size_of;
    if !armed {
        return Ok(None);
    }
    let overflow = || NativeCoverageV1::overflow::<Transaction>("FPA1 extraction views");
    let mut bytes = txs.len().checked_mul(size_of::<Transaction>()).ok_or_else(overflow)?;
    for tx in txs {
        bytes = bytes
            .checked_add(tx.inputs.len().checked_mul(size_of::<TransactionInput>()).ok_or_else(overflow)?)
            .ok_or_else(overflow)?;
        bytes = bytes
            .checked_add(tx.outputs.len().checked_mul(size_of::<TransactionOutput>()).ok_or_else(overflow)?)
            .ok_or_else(overflow)?;
        for input in &tx.inputs {
            bytes = bytes.checked_add(input.signature_script.len()).ok_or_else(overflow)?;
        }
        for output in &tx.outputs {
            bytes = bytes.checked_add(output.script_public_key.script().len()).ok_or_else(overflow)?;
        }
        // TX Clone copies its payload; projection reuses that same Vec allocation via copy_within/truncate.
        bytes = bytes.checked_add(tx.payload.len()).ok_or_else(overflow)?;
    }
    reserve_native_requests_v1(bytes as u64)
}

/// The actual extractor supplies the carrier ID and executed count of its authenticated FP object.
/// Identity/count/framing refusals are semantic; native capacity refusal remains a distinct error arm.
pub fn palw_fp_answer_for_carrier_v1(
    tx: &crate::tx::Transaction,
    carrier: crate::tx::TransactionId,
    decoded: u32,
) -> Result<NativeRetainedV1<Vec<u32>>, NativeDecodeErrorV1<&'static str>> {
    if tx.id() != carrier || tx.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT {
        return Err(NativeDecodeErrorV1::Decode("FPA1 answer is not this object's carrier"));
    }
    let view = palw_fp_answer_envelope_view_v1(&tx.payload, decoded as usize)
        .ok_or(NativeDecodeErrorV1::Decode("FPA1 framing or answer count"))?;
    if view.answer_len_v1() != decoded as usize {
        return Err(NativeDecodeErrorV1::Decode("FPA1 answer count differs from executed decode"));
    }
    view.retain_answer_v1().map_err(NativeDecodeErrorV1::Resource)
}

/// Decode errors stay separate from local capacity refusal.
pub fn palw_fp_answer_retain_v1(
    payload: &[u8],
    max_answer: usize,
) -> Result<NativeRetainedV1<Vec<u32>>, NativeDecodeErrorV1<&'static str>> {
    palw_fp_answer_envelope_view_v1(payload, max_answer)
        .ok_or(NativeDecodeErrorV1::Decode("invalid FPA1 accounting envelope"))?
        .retain_answer_v1()
        .map_err(NativeDecodeErrorV1::Resource)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_fp_answer_accounting_v1::palw_fp_answer_envelope_encode_v1 as encode;
    #[test]
    fn checked_borrowed_framing_rejects_every_truncation_double_and_count_overflow() {
        let bytes = encode(&[3, 0, 1], &[11, 22]);
        for end in 0..bytes.len() {
            assert!(palw_fp_answer_envelope_view_v1(&bytes[..end], 2).is_none());
        }
        let view = palw_fp_answer_envelope_view_v1(&bytes, 2).unwrap();
        assert_eq!(view.answer_ids_v1().collect::<Vec<_>>(), vec![11, 22]);
        assert_eq!(view.inner.as_ptr(), bytes[8..].as_ptr(), "no payload copy before admission");
        assert!(palw_fp_answer_envelope_view_v1(&bytes, 1).is_none());
        assert!(palw_fp_answer_envelope_view_v1(&encode(&bytes, &[11]), 9).is_none());
        let mut overflow = bytes.clone();
        overflow[11..15].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(palw_fp_answer_envelope_view_v1(&overflow, usize::MAX).is_none());
        let mut trailing = bytes;
        trailing.push(0);
        assert!(palw_fp_answer_envelope_view_v1(&trailing, 2).is_none());
    }
    #[test]
    fn existing_inner_schema_ceilings_are_distinct_and_legacy_served_magic_is_not_carriage() {
        for (magic, cap) in [
            (b"TFP1", crate::palw_tir_public_fp_envelope_v1::PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1),
            (b"HFP1", crate::palw_history_custody_v1::PALW_HISTORY_FP_ENVELOPE_MAX_BYTES_V1),
            (b"TFO1", crate::palw_tir_funded_original_envelope_v1::PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1),
        ] {
            let mut inner = vec![0; cap];
            inner[..4].copy_from_slice(magic);
            assert!(palw_fp_answer_envelope_view_v1(&encode(&inner, &[1]), 1).is_some());
            inner.push(0);
            assert!(palw_fp_answer_envelope_view_v1(&encode(&inner, &[1]), 1).is_none());
        }
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_fixture_v1();
        let old = crate::palw_freeprompt_v3::palw_fp_answer_encode_v1(&frame.original_v1().job, &[1, 2], &[3]);
        assert!(old.starts_with(b"FPA1"));
        assert!(palw_fp_answer_envelope_view_v1(&old, usize::MAX).is_none());
    }
    #[test]
    fn owned_answers_are_copied_into_only_their_claim_and_survive_the_rehearsal_scope() {
        let bytes = encode(&[3, 0, 1], &[7, 8]);
        let answer = palw_fp_answer_retain_v1(&bytes, 2).unwrap();
        let mut answers = PalwFpClaimAnswersV1::default();
        let claim = crate::Hash64::from_u64_word(1);
        answers.insert_v1(claim, answer).unwrap();
        let carrier = crate::Hash64::from_u64_word(9);
        answers.note_carrier_v1(claim, carrier);
        answers.note_carrier_v1(claim, crate::Hash64::from_u64_word(8));
        let first = answers.attach_v1(Default::default()).unwrap();
        let second = answers.attach_v1(Default::default()).unwrap();
        assert_ne!(first.fp_claim_answers[&claim].as_ptr(), second.fp_claim_answers[&claim].as_ptr());
        drop(answers);
        assert_eq!(first.fp_claim_answers[&claim], vec![7, 8]);
        assert!(!first.fp_claim_answers.contains_key(&crate::Hash64::from_u64_word(2)));
        assert_eq!(first.fp_winning_carriers.get(&claim), Some(&carrier));
    }
    #[test]
    fn local_capacity_refusal_precedes_copy_and_owner_handoff_keeps_the_ticket() {
        use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1};
        let bytes = encode(&[3, 0, 1], &[7, 8]);
        let view = palw_fp_answer_envelope_view_v1(&bytes, 2).unwrap();
        let small = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 1, database_bytes: 0 });
        assert!(view.retain_answer_with_v1(|n| small.reserve_requests_v1(n, 0).map(Some)).is_err());
        assert_eq!(small.outstanding(), (0, 0));
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 4096, database_bytes: 0 });
        let owner = view.retain_answer_with_v1(|n| checker.reserve_requests_v1(n, 0).map(Some)).unwrap();
        let charged = checker.outstanding().0;
        assert!(charged >= 8);
        let mut answers = PalwFpClaimAnswersV1::default();
        answers.insert_v1(crate::Hash64::from_u64_word(1), owner).unwrap();
        assert_eq!(checker.outstanding().0, charged);
        let mut accepted = PalwFpClaimAnswersV1::default();
        accepted.append_v1(answers).unwrap();
        assert_eq!(checker.outstanding().0, charged);
        drop(accepted);
        assert_eq!(checker.outstanding(), (0, 0));
    }
    #[test]
    fn duplicate_claim_answers_are_idempotent_or_semantically_refused_without_overwrite() {
        let claim = crate::Hash64::from_u64_word(1);
        let owner = |ids: &[u32]| palw_fp_answer_retain_v1(&encode(&[3, 0, 1], ids), ids.len()).unwrap();
        let mut rows = PalwFpClaimAnswersV1::default();
        assert!(rows.insert_v1(claim, owner(&[7, 8])).unwrap());
        assert!(rows.insert_v1(claim, owner(&[7, 8])).unwrap());
        assert!(!rows.insert_v1(claim, owner(&[7, 9])).unwrap());
        let mut conflict = PalwFpClaimAnswersV1::default();
        conflict.insert_v1(claim, owner(&[9, 9])).unwrap();
        assert!(!rows.compatible_v1(&conflict));
        assert!(!rows.append_v1(conflict).unwrap());
        assert_eq!(rows.attach_v1(Default::default()).unwrap().fp_claim_answers[&claim], vec![7, 8]);
    }
    #[test]
    fn authenticated_extraction_carrier_and_decode_count_are_checked_before_answer_copy() {
        use crate::palw_native_resource_v1::NativeDecodeErrorV1;
        let tx = crate::tx::Transaction::new(
            0,
            vec![],
            vec![],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
            0,
            encode(&[3, 0, 1], &[7, 8]),
        );
        assert_eq!(palw_fp_answer_for_carrier_v1(&tx, tx.id(), 2).unwrap().as_slice(), &[7, 8]);
        assert!(matches!(palw_fp_answer_for_carrier_v1(&tx, crate::Hash64::from_u64_word(8), 2), Err(NativeDecodeErrorV1::Decode(_))));
        assert!(matches!(palw_fp_answer_for_carrier_v1(&tx, tx.id(), 1), Err(NativeDecodeErrorV1::Decode(_))));
        assert!(matches!(palw_fp_answer_for_carrier_v1(&tx, tx.id(), 3), Err(NativeDecodeErrorV1::Decode(_))));
    }
}
