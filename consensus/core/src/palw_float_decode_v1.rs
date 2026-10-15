//! **Float32 decode-token adjudication** (G14 uncovered lane, adjudicator core).
//!
//! A `Float32` class commits each decode call's whole f32 logit row as one keyed event hash
//! ([`crate::palw_v2::logits_event_hash_v2`]) under the v2 event tree rooted in the binding's
//! `full_logits_trace_root`. The reference worker selects every token by **greedy argmax with
//! first-index tie-break** (strict `>`; a non-finite row aborts the run), event `0` being the
//! prefill's last position and event `i >= 1` decode step `i - 1`. Event `i` selects generated
//! token `i`.
//!
//! This module adjudicates exactly that rule from public, authenticated material only:
//!
//! * [`palw_float32_order_key_v1`] maps a finite f32 to an `i32` whose order is IEEE `<` with
//!   `-0.0 == +0.0`, so the integer lane's tie rule ([`crate::palw_decode_select_v2::decode_lane_beats_v2`])
//!   decides a float row exactly as the worker's strict `>` does;
//! * [`PalwFloatRowStreamV1`] authenticates one committed row in bounded parts (no resident
//!   vocabulary-sized buffer) and yields its greedy lane and any nominated lanes' keys;
//! * [`check_float32_greedy_decode_v1`] convicts a committed token that is not the greedy lane.
//!
//! **Not adjudicable here:** a seeded/temperature Float32 job (there is no canonical float
//! sampler), and arithmetic correctness of the f32 row itself (a Float32 class's logits are an
//! event summary, not a step output). Both stay refused/uncovered.
//!
//! The streaming verifier is **rooted and restartable**: its hash state is the serializable
//! [`crate::palw_rendering_stream_v1::LeafHashContinuationV1`] (the actual BLAKE2b compression
//! continuation, never prior bytes), so the staged court stores it in chain state between blocks
//! ([`crate::palw_float_constraint_source_v1`]) and [`PalwFloatRowStreamV1::check_v1`] re-derives
//! its byte position from the lane count on every load.

use crate::Hash64;
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_v2::{PalwTraceEventOpeningV2, PalwTracePhaseV2, PalwTraceSummaryV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeMap;

/// One streamed part: at most 64 KiB of little-endian f32 lanes.
pub const PALW_FLOAT_ROW_PART_BYTES_V1: usize = 65_536;
/// A verifier asks for at most the committed lane and one beating lane.
pub const PALW_FLOAT_ROW_NOMINATED_LANES_V1: usize = 2;

/// Order key of a finite f32: monotone in IEEE order, `-0.0` and `+0.0` equal. `None` for a
/// non-finite value, which the reference worker never commits (it aborts first).
pub fn palw_float32_order_key_v1(value: f32) -> Option<i32> {
    if !value.is_finite() {
        return None;
    }
    let value = if value == 0.0 { 0.0f32 } else { value };
    let bits = value.to_bits() as i32;
    Some(if bits < 0 { bits ^ i32::MAX } else { bits })
}

/// The reference worker's greedy rule over a whole row: strict `>`, first index wins a tie.
pub fn palw_float32_greedy_select_v1(row: &[f32]) -> Result<usize, String> {
    let mut best: Option<(i32, usize)> = None;
    for (lane, value) in row.iter().enumerate() {
        let key = palw_float32_order_key_v1(*value).ok_or("a committed Float32 row holds a non-finite logit")?;
        if best.is_none_or(|(best_key, _)| key > best_key) {
            best = Some((key, lane));
        }
    }
    best.map(|(_, lane)| lane).ok_or_else(|| "an empty Float32 row selects nothing".into())
}

/// The event's phase and step, from its index (the reference worker's order).
pub fn palw_float32_event_phase_v1(event_index: u32) -> (PalwTracePhaseV2, u32) {
    if event_index == 0 { (PalwTracePhaseV2::Prefill, 0) } else { (PalwTracePhaseV2::Decode, event_index - 1) }
}

/// What a fully authenticated row yields.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwFloatRowFactsV1 {
    pub event_index: u32,
    pub greedy_lane: u32,
    pub greedy_key: i32,
    /// The nominated lanes' order keys.
    pub keys: BTreeMap<u32, i32>,
}

/// Bounded streaming authentication of ONE committed f32 row. Parts arrive in order; nothing a
/// part claims is believed until [`Self::finish_v1`] re-derives the event hash, its opening and
/// the binding's trace root. Serializable: the chain stores it between parts.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwFloatRowStreamV1 {
    job_context_hash: Hash64,
    event_index: u32,
    n_vocab: u32,
    lanes_read: u32,
    hash: LeafHashContinuationV1,
    nominated: Vec<u32>,
    keys: BTreeMap<u32, i32>,
    best: Option<(i32, u32)>,
}

impl PalwFloatRowStreamV1 {
    pub fn new_v1(job_context_hash: &Hash64, event_index: u32, n_vocab: u32, nominated: &[u32]) -> Result<Self, String> {
        if n_vocab == 0
            || event_index as usize >= crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS
            || nominated.len() > PALW_FLOAT_ROW_NOMINATED_LANES_V1
            || nominated.iter().any(|lane| *lane >= n_vocab)
        {
            return Err("Float32 row stream names no bounded vocabulary, event or nominated lanes".into());
        }
        let (phase, step) = palw_float32_event_phase_v1(event_index);
        // The keyed BLAKE2b-512 of `logits_event_hash_v2`, as a storable continuation.
        let mut hash = LeafHashContinuationV1::new(crate::palw_v2::PALW_V2_DOMAIN_LOGITS_EVENT);
        hash.update(&crate::palw_v2::logits_event_header_v2(job_context_hash, phase, step, n_vocab));
        let mut nominated = nominated.to_vec();
        nominated.sort_unstable();
        nominated.dedup();
        Ok(Self {
            job_context_hash: *job_context_hash,
            event_index,
            n_vocab,
            lanes_read: 0,
            hash,
            nominated,
            keys: BTreeMap::new(),
            best: None,
        })
    }
    pub fn lanes_read_v1(&self) -> u32 {
        self.lanes_read
    }
    pub fn event_index_v1(&self) -> u32 {
        self.event_index
    }
    pub fn n_vocab_v1(&self) -> u32 {
        self.n_vocab
    }
    pub fn nominated_v1(&self) -> &[u32] {
        &self.nominated
    }
    /// A stored continuation is never a credential: its hashed byte count must be exactly the
    /// key block, the canonical event header and the lanes read so far; its nominated keys and
    /// running best must lie inside the lanes read.
    pub fn check_v1(&self) -> Result<(), String> {
        let (phase, step) = palw_float32_event_phase_v1(self.event_index);
        let header = crate::palw_v2::logits_event_header_v2(&self.job_context_hash, phase, step, self.n_vocab).len() as u128;
        if self.n_vocab == 0
            || self.event_index as usize >= crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS
            || self.lanes_read > self.n_vocab
            || self.nominated.len() > PALW_FLOAT_ROW_NOMINATED_LANES_V1
            || self.nominated.windows(2).any(|w| w[0] >= w[1])
            || self.nominated.iter().any(|lane| *lane >= self.n_vocab)
            || self.keys.keys().any(|lane| *lane >= self.lanes_read || self.nominated.binary_search(lane).is_err())
            || self.nominated.iter().any(|lane| (*lane < self.lanes_read) != self.keys.contains_key(lane))
            || self.best.is_some() != (self.lanes_read > 0)
            || self.best.is_some_and(|(_, lane)| lane >= self.lanes_read)
            || self.hash.byte_count() != Some(128 + header + u128::from(self.lanes_read) * 4)
        {
            return Err("Float32 row continuation changed its original byte position or nominated lanes".into());
        }
        Ok(())
    }
    pub fn part_v1(&mut self, offset_lanes: u32, bytes: &[u8]) -> Result<(), String> {
        if offset_lanes != self.lanes_read
            || bytes.is_empty()
            || bytes.len() > PALW_FLOAT_ROW_PART_BYTES_V1
            || !bytes.len().is_multiple_of(4)
            || (bytes.len() / 4) as u64 > u64::from(self.n_vocab - self.lanes_read)
        {
            return Err("Float32 row part changes the ordered bounded lane interval".into());
        }
        let mut next = self.clone();
        for (i, word) in bytes.chunks_exact(4).enumerate() {
            let lane = next.lanes_read + i as u32;
            let value = f32::from_le_bytes(word.try_into().expect("4"));
            let key = palw_float32_order_key_v1(value).ok_or("a committed Float32 row holds a non-finite logit")?;
            if next.best.is_none_or(|(best_key, _)| key > best_key) {
                next.best = Some((key, lane));
            }
            if next.nominated.binary_search(&lane).is_ok() {
                next.keys.insert(lane, key);
            }
        }
        next.hash.update(bytes);
        next.lanes_read += (bytes.len() / 4) as u32;
        *self = next;
        Ok(())
    }
    /// Authenticate the complete row against the claim's own trace root, then expose its facts.
    pub fn finish_v1(
        self,
        job_context_hash: &Hash64,
        expected_trace_root: &Hash64,
        summary: &PalwTraceSummaryV2,
        ordered_event_commitment: &Hash64,
        opening: &PalwTraceEventOpeningV2,
    ) -> Result<PalwFloatRowFactsV1, String> {
        if self.lanes_read != self.n_vocab || summary.vocab_size != self.n_vocab {
            return Err("Float32 row is incomplete or not the committed vocabulary wide".into());
        }
        if *job_context_hash != self.job_context_hash {
            return Err("Float32 row continuation was opened for another job context".into());
        }
        let event_hash = self.hash.finish();
        if opening.event_index != self.event_index || opening.event_hash != event_hash {
            return Err("Float32 row bytes do not hash to the opened event".into());
        }
        let root = crate::palw_v2::trace_event_opening_root_v2(summary.event_count, opening).map_err(|e| e.to_string())?;
        if root != *ordered_event_commitment
            || crate::palw_v2::full_logits_trace_root_v2(job_context_hash, summary, ordered_event_commitment) != *expected_trace_root
        {
            return Err("Float32 event opening does not reproduce the claim's committed trace root".into());
        }
        let (greedy_key, greedy_lane) = self.best.ok_or("Float32 row selected nothing")?;
        Ok(PalwFloatRowFactsV1 { event_index: self.event_index, greedy_lane, greedy_key, keys: self.keys })
    }
}

/// The objective verdict of one Float32 greedy decode position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwFloatDecodeVerdictV1 {
    Honest,
    DecodeTokenMismatch { position: u32, committed: u32, expected: u32 },
}

/// **Adjudicate one committed Float32 token** from the claim's own pin and an authenticated row.
/// `sampling` comes from the CLAIM's job, never from a challenger; a seeded job is refused.
pub fn check_float32_greedy_decode_v1(
    pin: &crate::palw_step_refute::PalwDecodeTokensV1,
    sampling: crate::palw_decode_select_v2::PalwDecodeSamplingV2,
    job_context_hash: &Hash64,
    expected_trace_root: &Hash64,
    position: u32,
    row: &PalwFloatRowFactsV1,
) -> Result<PalwFloatDecodeVerdictV1, String> {
    if !sampling.is_greedy() {
        return Err("a seeded Float32 job has no canonical selection rule to adjudicate".into());
    }
    if crate::palw_v2::output_token_ids_hash_v2(&pin.generated_token_ids) != pin.summary.output_token_ids_hash
        || crate::palw_v2::full_logits_trace_root_v2(job_context_hash, &pin.summary, &pin.ordered_event_commitment)
            != *expected_trace_root
        || pin.generated_token_ids.len() != pin.summary.exact_decode_tokens as usize
    {
        return Err("the Float32 pin does not reproduce the claim's committed ids and trace root".into());
    }
    let committed = *pin.generated_token_ids.get(position as usize).ok_or("position is past the generated ids")?;
    if row.event_index != position {
        return Err("the authenticated row is not the event that selects this position".into());
    }
    Ok(if committed == row.greedy_lane {
        PalwFloatDecodeVerdictV1::Honest
    } else {
        PalwFloatDecodeVerdictV1::DecodeTokenMismatch { position, committed, expected: row.greedy_lane }
    })
}

/// A constrained Float32 selection: does an admissible `beat` lane outrank the committed lane
/// under the same order and tie rule? Admissibility is the constraint court's question.
pub fn palw_float32_lane_beats_v1(row: &PalwFloatRowFactsV1, beat_lane: u32, committed_lane: u32) -> Result<bool, String> {
    let beat = *row.keys.get(&beat_lane).ok_or("beating lane was not nominated and authenticated")?;
    let committed = *row.keys.get(&committed_lane).ok_or("committed lane was not nominated and authenticated")?;
    Ok(crate::palw_decode_select_v2::decode_lane_beats_v2(
        i64::from(beat),
        beat_lane as usize,
        i64::from(committed),
        committed_lane as usize,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_v2::{
        PalwLogitsDtypeV2, PalwStopReasonV2, logits_event_hash_v2, output_token_ids_hash_v2, trace_event_merkle_root_v2,
        trace_event_opening_v2,
    };

    fn reference_argmax(row: &[f32]) -> usize {
        let mut best = 0usize;
        for (i, l) in row.iter().enumerate().skip(1) {
            if *l > row[best] {
                best = i;
            }
        }
        best
    }

    #[test]
    fn order_key_reproduces_the_reference_worker_argmax_including_ties_and_signed_zero() {
        let rows: Vec<Vec<f32>> = vec![
            vec![0.0, -0.0, 0.0],
            vec![-0.0, 0.0, -1.0],
            vec![-3.5, -1.25, -1.25, -7.0],
            vec![1.0e-38, -1.0e-38, 1.0e-38],
            vec![f32::MIN, f32::MAX, f32::MAX],
            vec![2.0, 3.0, 3.0, -0.0, 3.0],
        ];
        for row in rows {
            assert_eq!(palw_float32_greedy_select_v1(&row).unwrap(), reference_argmax(&row), "row {row:?}");
        }
        assert!(palw_float32_greedy_select_v1(&[1.0, f32::NAN]).is_err());
        assert!(palw_float32_order_key_v1(f32::INFINITY).is_none());
        assert!(palw_float32_order_key_v1(-2.0).unwrap() < palw_float32_order_key_v1(-1.0).unwrap());
        assert_eq!(palw_float32_order_key_v1(-0.0), palw_float32_order_key_v1(0.0));
    }

    #[test]
    fn streamed_row_authenticates_against_the_trace_root_and_convicts_a_non_greedy_token() {
        let context = Hash64::from_u64_word(0xF1);
        let rows: Vec<Vec<f32>> = vec![vec![0.5, 2.0, -1.0, 2.0], vec![9.0, 1.0, 0.0, 3.0], vec![-1.0, -2.0, 4.0, 4.0]];
        let mut scratch = Vec::new();
        let events: Vec<Hash64> = rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let (phase, step) = palw_float32_event_phase_v1(i as u32);
                logits_event_hash_v2(&context, phase, step, i as u32, 4, row, &mut scratch).unwrap()
            })
            .collect();
        let ordered = trace_event_merkle_root_v2(&events).unwrap();
        // The producer committed token 3 at position 0, but the greedy lane is 1 (first of a tie).
        let generated = vec![3u32, 0, 2];
        let summary = PalwTraceSummaryV2 {
            vocab_size: 4,
            logits_dtype: PalwLogitsDtypeV2::F32Le,
            declared_prefill_tokens: 2,
            exact_decode_tokens: 3,
            event_count: 3,
            first_event_kind: PalwTracePhaseV2::Prefill,
            last_event_kind: PalwTracePhaseV2::Decode,
            output_token_ids_hash: output_token_ids_hash_v2(&generated),
            stop_reason: PalwStopReasonV2::ExactBudgetReached,
        };
        let root = crate::palw_v2::full_logits_trace_root_v2(&context, &summary, &ordered);
        let pin = crate::palw_step_refute::PalwDecodeTokensV1 { summary, ordered_event_commitment: ordered, generated_token_ids: generated };
        let greedy = crate::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY;
        let stream_row = |index: u32, nominated: &[u32]| {
            let mut stream = PalwFloatRowStreamV1::new_v1(&context, index, 4, nominated).unwrap();
            let bytes: Vec<u8> = rows[index as usize].iter().flat_map(|v| v.to_le_bytes()).collect();
            stream.part_v1(0, &bytes[..8]).unwrap();
            assert!(stream.part_v1(0, &bytes[8..]).is_err(), "parts arrive in lane order");
            stream.part_v1(2, &bytes[8..]).unwrap();
            stream.finish_v1(&context, &root, &pin.summary, &ordered, &trace_event_opening_v2(&events, index).unwrap()).unwrap()
        };
        let first = stream_row(0, &[3, 1]);
        assert_eq!(first.greedy_lane, 1);
        assert_eq!(
            check_float32_greedy_decode_v1(&pin, greedy, &context, &root, 0, &first).unwrap(),
            PalwFloatDecodeVerdictV1::DecodeTokenMismatch { position: 0, committed: 3, expected: 1 }
        );
        assert!(palw_float32_lane_beats_v1(&first, 1, 3).unwrap(), "an equal key at a lower index wins");
        assert_eq!(check_float32_greedy_decode_v1(&pin, greedy, &context, &root, 1, &stream_row(1, &[])).unwrap(), PalwFloatDecodeVerdictV1::Honest);
        assert_eq!(check_float32_greedy_decode_v1(&pin, greedy, &context, &root, 2, &stream_row(2, &[])).unwrap(), PalwFloatDecodeVerdictV1::Honest);
        // A row from another event, a tampered byte, or a seeded job is never a verdict.
        assert!(check_float32_greedy_decode_v1(&pin, greedy, &context, &root, 1, &first).is_err());
        let mut tampered = PalwFloatRowStreamV1::new_v1(&context, 0, 4, &[]).unwrap();
        let mut bytes: Vec<u8> = rows[0].iter().flat_map(|v| v.to_le_bytes()).collect();
        bytes[0] ^= 1;
        tampered.part_v1(0, &bytes).unwrap();
        assert!(tampered.finish_v1(&context, &root, &pin.summary, &ordered, &trace_event_opening_v2(&events, 0).unwrap()).is_err());
        let seeded = crate::palw_decode_select_v2::PalwDecodeSamplingV2 { seed: [7; 32], temperature_q: 3 };
        assert!(check_float32_greedy_decode_v1(&pin, seeded, &context, &root, 0, &first).is_err());
    }
}

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_float_decode_v1.rs"]
mod native_resource_inventory_v1;
