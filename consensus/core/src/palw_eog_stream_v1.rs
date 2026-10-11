//! Bounded, restart-safe authentication of the EXISTING legacy EOG leaf.
//! Tentative membership is private until its complete keyed digest reaches the opened leaf.
use crate::Hash64;
use crate::palw_fp_constraint_job_v1::{PALW_TOKEN_TABLE_DOMAIN_EOG_V1, PALW_TOKEN_TABLE_MAX_LEAVES_V1};
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_step_leg::{PalwStepOpeningV1, step_opening_root_capped_v1};
use borsh::{BorshDeserialize, BorshSerialize};
pub const PALW_EOG_STREAM_PART_IDS_V1: usize = 16_384;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEogStreamV1 {
    context: Hash64,
    vocab: u32,
    count: u32,
    offset: u32,
    last: Option<u32>,
    lowest: Option<u32>,
    relevant: Vec<u32>,
    hits: Vec<bool>,
    expected_leaf: Hash64,
    hash: LeafHashContinuationV1,
}
/// This result is derived by finish, never deserialized from a caller as an authentication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwAuthenticatedEogV1 {
    pub lowest: u32,
    pub relevant_members: Vec<u32>,
}
impl PalwEogStreamV1 {
    pub fn new(
        context: Hash64,
        root: Hash64,
        vocab: u32,
        count: u32,
        relevant: &[u32],
        opening: &PalwStepOpeningV1,
    ) -> Result<Self, String> {
        if count == 0
            || count > vocab
            || u64::from(vocab) + 1 > PALW_TOKEN_TABLE_MAX_LEAVES_V1
            || opening.leaf_index != u64::from(vocab)
            || relevant.len() > crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS + 2
        {
            return Err("EOG stream exceeds registered table/trace envelope".into());
        }
        if step_opening_root_capped_v1(u64::from(vocab) + 1, opening, PALW_TOKEN_TABLE_MAX_LEAVES_V1).map_err(|e| e.to_string())?
            != root
        {
            return Err("EOG opening is not rooted in the claim's legacy table".into());
        }
        let mut relevant = relevant.to_vec();
        relevant.sort_unstable();
        relevant.dedup();
        let mut hash = LeafHashContinuationV1::new(PALW_TOKEN_TABLE_DOMAIN_EOG_V1);
        hash.update(&count.to_le_bytes());
        let hits = vec![false; relevant.len()];
        Ok(Self { context, vocab, count, offset: 0, last: None, lowest: None, relevant, hits, expected_leaf: opening.leaf_hash, hash })
    }
    pub fn offset(&self) -> u32 {
        self.offset
    }
    pub fn check(&self) -> Result<(), String> {
        if self.offset > self.count
            || self.count == 0
            || self.count > self.vocab
            || self.relevant.len() != self.hits.len()
            || self.relevant.len() > crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS + 2
            || self.relevant.windows(2).any(|w| w[0] >= w[1])
            || self.hash.byte_count() != Some(132 + u128::from(self.offset) * 4)
            || (self.offset == 0) != self.last.is_none()
            || (self.offset == 0) != self.lowest.is_none()
        {
            return Err("EOG continuation counters or bounded membership are inconsistent".into());
        }
        Ok(())
    }
    pub fn push(&mut self, context: Hash64, offset: u32, ids: &[u32]) -> Result<(), String> {
        self.check()?;
        // Validate the whole bounded move BEFORE changing any continuation/membership.
        if context != self.context
            || offset != self.offset
            || ids.is_empty()
            || ids.len() > PALW_EOG_STREAM_PART_IDS_V1
            || ids.len() as u64 > u64::from(self.count - self.offset)
            || ids.iter().any(|id| *id >= self.vocab)
            || ids.windows(2).any(|w| w[0] >= w[1])
            || self.last.zip(ids.first()).is_some_and(|(a, b)| a >= *b)
        {
            return Err("EOG move changes identity/order or exceeds remaining bounded list".into());
        }
        if self.lowest.is_none() {
            self.lowest = ids.first().copied();
        }
        for id in ids {
            self.hash.update(&id.to_le_bytes());
            if let Ok(index) = self.relevant.binary_search(id) {
                self.hits[index] = true;
            }
        }
        self.last = ids.last().copied();
        self.offset += ids.len() as u32;
        Ok(())
    }
    pub fn finish(&self) -> Result<PalwAuthenticatedEogV1, String> {
        self.check()?;
        if self.offset != self.count || self.hash.finish() != self.expected_leaf {
            return Err("EOG list is incomplete or does not authenticate the legacy leaf".into());
        }
        Ok(PalwAuthenticatedEogV1 {
            lowest: self.lowest.ok_or("authenticated EOG list is empty")?,
            relevant_members: self.relevant.iter().zip(&self.hits).filter_map(|(id, hit)| hit.then_some(*id)).collect(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_fp_constraint_job_v1::PalwTokenTableV1;
    #[test]
    fn huge_legacy_list_restarts_without_retaining_all_ids_and_matches_original_leaf() {
        let table =
            PalwTokenTableV1 { entries: vec![None; 100_000], eog_token_ids: (0..100_000).collect(), tokenizer_id: Hash64::default() };
        let eog = table.eog_opening().unwrap();
        let mut row =
            PalwEogStreamV1::new(Hash64::default(), table.root(), table.vocab(), 100_000, &[1, 99_999], &eog.opening).unwrap();
        for part in eog.ids.chunks(PALW_EOG_STREAM_PART_IDS_V1) {
            row.push(Hash64::default(), row.offset(), part).unwrap();
            let bytes = borsh::to_vec(&row).unwrap();
            assert!(bytes.len() < 1024);
            row = borsh::from_slice(&bytes).unwrap();
        }
        assert_eq!(row.finish().unwrap().relevant_members, vec![1, 99_999]);
    }
    #[test]
    fn foreign_order_replays_and_incomplete_or_wrong_digest_cannot_expose_membership() {
        let table = PalwTokenTableV1 { entries: vec![None; 4], eog_token_ids: vec![1, 3], tokenizer_id: Hash64::default() };
        let eog = table.eog_opening().unwrap();
        let mut row = PalwEogStreamV1::new(Hash64::default(), table.root(), 4, 2, &[1, 3], &eog.opening).unwrap();
        assert!(row.finish().is_err());
        let before = row.clone();
        assert!(row.push(Hash64::default(), 0, &[3, 1]).is_err());
        assert_eq!(row, before);
        row.push(Hash64::default(), 0, &[0, 2]).unwrap();
        assert!(row.finish().is_err());
        assert!(row.push(Hash64::default(), 0, &[0]).is_err());
    }
}
