//! **RFC-0001 §2.7 stage 2 — the in-process decode scheduler** (continuous batching, node only).
//!
//! A resident worker answering several sequences at once (the `n` candidates of one request, or
//! several requests the gateway hands one process) runs their decode steps TOGETHER: each layer's
//! projections and the LM head read their weights once for the whole batch
//! ([`crate::engine_a16::A16Engine::forward_decode_batch_planned`]) instead of once a sequence.
//!
//! The condition on it is batch invariance, and it is a property of the kernels rather than of
//! this scheduler: a batched projection is the single-row one on each row, attention reads only the
//! row's own cache, and the decoder that picks each token is the sequence's own
//! ([`kaspa_consensus_core::palw_decode_pipeline_v4::PalwFpDecoderV1`]). So a sequence's ids are the
//! ones it would select ALONE, whatever else is in the batch, whenever it joined it and in whatever
//! order the rows sit — which `in_batch_equals_alone` in the backend's tests holds as a golden
//! test. This module owns the bookkeeping only: which sequences are running, the batch width, the
//! round-robin that keeps a wide batch fair, and when a sequence is done.

use crate::engine_a16::{A16Cache, A16DecodeRowV1, A16Engine, A16ProfilePlanV1};
use kaspa_consensus_core::palw_decode_pipeline_v4::PalwFpDecoderV1;

/// One sequence the scheduler runs: its cache (holding the prompt), its decoder and its budget.
pub struct DecodeSequenceV1 {
    pub id: u64,
    cache: A16Cache,
    decoder: PalwFpDecoderV1,
    /// Prompt positions in the cache.
    prefill: usize,
    limit: usize,
    stop_ids: Vec<u32>,
    generated: Vec<u32>,
    /// The id to feed next, once one has been selected and the sequence is not done.
    pending: Option<u32>,
    ended_on_stop_id: bool,
    done: bool,
}

/// A finished sequence.
pub struct FinishedSequenceV1 {
    pub id: u64,
    pub output_token_ids: Vec<u32>,
    pub ended_on_stop_id: bool,
    /// The cache the sequence ended with (prompt plus every id fed forward), for a prefix cache.
    pub cache: A16Cache,
    pub prefill: usize,
}

impl DecodeSequenceV1 {
    /// A sequence whose prompt is already in `cache` (`prefill` positions) and whose prefill left
    /// `logits`. The first id is selected here, so the first token is available at admission.
    pub fn start(
        id: u64,
        cache: A16Cache,
        prefill: usize,
        logits: &[i32],
        decoder: PalwFpDecoderV1,
        limit: usize,
        stop_ids: Vec<u32>,
    ) -> Self {
        let mut seq = Self {
            id,
            cache,
            decoder,
            prefill,
            limit,
            stop_ids,
            generated: Vec::new(),
            pending: None,
            ended_on_stop_id: false,
            done: false,
        };
        seq.select(logits);
        seq
    }

    /// Select the next id from `logits` and decide whether the sequence goes on — the SAME rule, in
    /// the same order, as the single-sequence loop of `answer_free_prompt_v1`.
    fn select(&mut self, logits: &[i32]) {
        let next = self.decoder.select(logits);
        if self.decoder.generated().len() == self.generated.len() {
            // The decoder committed nothing from this row: no lane was admissible.
            self.done = true;
            return;
        }
        self.generated.push(next);
        if self.stop_ids.contains(&next) {
            self.ended_on_stop_id = true;
            self.done = true;
        } else if self.decoder.stop().is_some() || self.generated.len() >= self.limit {
            self.done = true;
        } else {
            self.pending = Some(next);
        }
    }

    pub fn generated(&self) -> &[u32] {
        &self.generated
    }

    pub fn is_done(&self) -> bool {
        self.done
    }
}

/// The scheduler: sequences join at any time, run in batches of at most `max_batch`, and leave when
/// done.
pub struct DecodeSchedulerV1 {
    running: Vec<DecodeSequenceV1>,
    finished: Vec<FinishedSequenceV1>,
    /// `(sequence id, id)` for every selected id not yet drained, in selection order.
    events: Vec<(u64, u32)>,
    max_batch: usize,
    steps: u64,
    rows_stepped: u64,
}

impl DecodeSchedulerV1 {
    pub fn new(max_batch: usize) -> Self {
        Self { running: Vec::new(), finished: Vec::new(), events: Vec::new(), max_batch: max_batch.max(1), steps: 0, rows_stepped: 0 }
    }

    /// Admit a sequence (its first id is already selected). A sequence done at admission (a budget
    /// of one, a stop id first) goes straight to `finished`.
    pub fn admit(&mut self, seq: DecodeSequenceV1) {
        if let Some(first) = seq.generated.first() {
            self.events.push((seq.id, *first));
        }
        self.settle(seq);
    }

    fn settle(&mut self, seq: DecodeSequenceV1) {
        if seq.done {
            self.finished.push(FinishedSequenceV1 {
                id: seq.id,
                output_token_ids: seq.generated,
                ended_on_stop_id: seq.ended_on_stop_id,
                cache: seq.cache,
                prefill: seq.prefill,
            });
        } else {
            self.running.push(seq);
        }
    }

    pub fn is_idle(&self) -> bool {
        self.running.is_empty()
    }

    pub fn running(&self) -> usize {
        self.running.len()
    }

    /// `(steps, rows)` — how many batched forwards ran and how many rows they carried, so the
    /// batching is a measured number.
    pub fn counters(&self) -> (u64, u64) {
        (self.steps, self.rows_stepped)
    }

    pub fn drain_events(&mut self) -> Vec<(u64, u32)> {
        std::mem::take(&mut self.events)
    }

    pub fn take_finished(&mut self) -> Vec<FinishedSequenceV1> {
        std::mem::take(&mut self.finished)
    }

    /// **One batched decode step** over at most `max_batch` running sequences, round-robin: every
    /// chosen sequence feeds its pending id at its next position and selects its next id from its
    /// own logits. Returns how many rows it carried (0 when idle).
    pub fn step(&mut self, engine: &A16Engine<'_>, plan: &A16ProfilePlanV1) -> Result<usize, String> {
        let n = self.running.len();
        if n == 0 {
            return Ok(0);
        }
        // The front `width` run; whoever ran and is still going moves to the back, so a set wider
        // than the batch is served round-robin and nobody starves.
        let width = n.min(self.max_batch);
        let chosen: Vec<usize> = (0..width).collect();
        let logits = {
            // Disjoint `&mut` borrows of the chosen sequences' caches, in `chosen` order.
            let mut slots: Vec<Option<&mut DecodeSequenceV1>> = self.running.iter_mut().map(Some).collect();
            let mut rows: Vec<A16DecodeRowV1<'_>> = Vec::with_capacity(width);
            for &i in &chosen {
                let seq = slots[i].take().expect("each chosen sequence once");
                let token = seq.pending.take().expect("a running sequence has a pending id") as usize;
                let position = seq.prefill + seq.generated.len() - 1;
                rows.push(A16DecodeRowV1 { cache: &mut seq.cache, token, position });
            }
            engine.forward_decode_batch_planned(plan, &mut rows).map_err(|e| format!("a batched decode step: {e:?}"))?
        };
        for (&i, row_logits) in chosen.iter().zip(logits.iter()) {
            let seq = &mut self.running[i];
            seq.select(row_logits);
            if let Some(last) = seq.generated.last() {
                self.events.push((seq.id, *last));
            }
        }
        self.steps += 1;
        self.rows_stepped += width as u64;
        // Move the done sequences out, keeping the order of the rest.
        let mut still = Vec::with_capacity(self.running.len());
        let mut survivors_of_this_step = 0usize;
        for (at, seq) in std::mem::take(&mut self.running).into_iter().enumerate() {
            if seq.done {
                self.finished.push(FinishedSequenceV1 {
                    id: seq.id,
                    output_token_ids: seq.generated,
                    ended_on_stop_id: seq.ended_on_stop_id,
                    cache: seq.cache,
                    prefill: seq.prefill,
                });
            } else {
                if at < width {
                    survivors_of_this_step += 1;
                }
                still.push(seq);
            }
        }
        self.running = still;
        self.running.rotate_left(survivors_of_this_step);
        Ok(width)
    }
}
