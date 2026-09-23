use crate::model::stores::{
    block_window_cache::BlockWindowHeap,
    ghostdag::{GhostdagData, GhostdagStoreReader},
    headers::HeaderStoreReader,
};
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::palw_clock_cursor_v1::PalwClockStepV1;
use kaspa_consensus_core::{
    BlockHashSet, BlueWorkType, MAX_WORK_LEVEL,
    config::params::{ForkActivation, MAX_DIFFICULTY_TARGET_AS_F64},
    errors::difficulty::{DifficultyError, DifficultyResult},
    pow_layer0::{POW_ALGO_ID_PALW_RECEIPT_V3, algo_id_is_priced_by_bits, is_palw_attempt_algo_id},
};
use kaspa_core::{info, log::CRESCENDO_KEYWORD};
use kaspa_math::{Uint256, Uint320};
use std::{
    cmp::{Ordering, max},
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering as AtomicOrdering},
    },
};

use super::ghostdag::ordering::SortableBlock;
use itertools::Itertools;

trait DifficultyManagerExtension {
    fn headers_store(&self) -> &dyn HeaderStoreReader;

    /// **ADR-0138: how many mergeset blocks are in the DAA window yet do not advance the DAA
    /// score** — the blocks `bits` does not price, past `palw_anchor_clock`. Zero where the fence is
    /// not in force. Returned WITH the clock's decision for the block (ADR-0142), because the two
    /// are one computation and every other reader needs the second half.
    fn daa_exempt_count(
        &self,
        ghostdag_data: &GhostdagData,
        mergeset_non_daa: &BlockHashSet,
        window: &BlockWindowHeap,
    ) -> (u64, PalwClockStepV1);

    #[inline]
    #[must_use]
    fn internal_calc_daa_score(
        &self,
        ghostdag_data: &GhostdagData,
        mergeset_non_daa: &BlockHashSet,
        window: &BlockWindowHeap,
    ) -> (u64, PalwClockStepV1) {
        let sp_daa_score = self.headers_store().get_daa_score(ghostdag_data.selected_parent).unwrap();
        // ADR-0138: the DAA score is the anchor's clock. A block outside the window (`mergeset_non_daa`)
        // never counted; past the fence a block `bits` does not price does not count either — it is
        // still merged, still paid, still folded, it just does not move the clock every window reads.
        let (exempt, clock) = self.daa_exempt_count(ghostdag_data, mergeset_non_daa, window);
        (sp_daa_score + (ghostdag_data.mergeset_size() - mergeset_non_daa.len()) as u64 - exempt, clock)
    }

    fn get_difficulty_blocks(&self, window: &BlockWindowHeap) -> Vec<DifficultyBlock> {
        window
            .iter()
            .map(|item| {
                let data = self.headers_store().get_compact_header_data(item.0.hash).unwrap();
                DifficultyBlock {
                    timestamp: data.timestamp,
                    bits: data.bits,
                    sortable_block: item.0.clone(),
                    priced: true,
                    receipt: false,
                    attempt: false,
                }
            })
            .collect()
    }

    /// The same rows with their lane read — ADR-0083 Decision 1. The compact header data does not
    /// carry `pow_algo_id`, so this reads the full header once per row; it runs only past the fence,
    /// which keeps the legacy path's cost (and arithmetic) exactly what it was.
    fn get_difficulty_blocks_with_lane(&self, window: &BlockWindowHeap) -> Vec<DifficultyBlock> {
        window
            .iter()
            .map(|item| {
                let header = self.headers_store().get_header(item.0.hash).unwrap();
                DifficultyBlock {
                    timestamp: header.timestamp,
                    bits: header.bits,
                    sortable_block: item.0.clone(),
                    priced: algo_id_is_priced_by_bits(header.pow_algo_id),
                    receipt: header.pow_algo_id == POW_ALGO_ID_PALW_RECEIPT_V3,
                    attempt: is_palw_attempt_algo_id(header.pow_algo_id),
                }
            })
            .collect()
    }

    fn internal_estimate_network_hashes_per_second(&self, window: &BlockWindowHeap) -> DifficultyResult<u64> {
        // TODO: perhaps move this const
        const MIN_WINDOW_SIZE: usize = 1000;
        let window_size = window.len();
        if window_size < MIN_WINDOW_SIZE {
            return Err(DifficultyError::UnderMinWindowSizeAllowed(window_size, MIN_WINDOW_SIZE));
        }
        let difficulty_blocks = self.get_difficulty_blocks(window);
        let (min_ts, max_ts) = difficulty_blocks.iter().map(|x| x.timestamp).minmax().into_option().unwrap();
        if min_ts == max_ts {
            return Err(DifficultyError::EmptyTimestampRange);
        }
        let window_duration = (max_ts - min_ts) / 1000; // Divided by 1000 to convert milliseconds to seconds
        if window_duration == 0 {
            return Ok(0);
        }

        let (min_blue_work, max_blue_work) =
            difficulty_blocks.iter().map(|x| x.sortable_block.blue_work).minmax().into_option().unwrap();

        Ok(((max_blue_work - min_blue_work) / window_duration).as_u64())
    }

    #[inline]
    fn check_min_difficulty_window_size(difficulty_window_size: usize, min_difficulty_window_size: usize) {
        assert!(
            min_difficulty_window_size <= difficulty_window_size,
            "min_difficulty_window_size {} is expected to be <= difficulty_window_size {}",
            min_difficulty_window_size,
            difficulty_window_size
        );
    }
}

#[derive(Clone)]
struct _CrescendoLogger {
    _steps: Arc<AtomicU8>,
}

impl _CrescendoLogger {
    fn _new() -> Self {
        Self { _steps: Arc::new(AtomicU8::new(Self::_ACTIVATE)) }
    }

    const _ACTIVATE: u8 = 0;
    const _DYNAMIC: u8 = 1;
    const _FULL: u8 = 2;

    pub fn _report_activation_progress(&self, step: u8) -> bool {
        if self._steps.compare_exchange(step, step + 1, AtomicOrdering::SeqCst, AtomicOrdering::SeqCst).is_ok() {
            match step {
                Self::_ACTIVATE => {
                    info!(target: CRESCENDO_KEYWORD,
                        r#"
        ____                                  _             
       / ___|_ __ ___  ___  ___ ___ _ __   __| | ___        
      | |   | '__/ _ \/ __|/ __/ _ \ '_ \ / _` |/ _ \       
      | |___| | |  __/\__ \ (_|  __/ | | | (_| | (_) |      
       \____|_|  \___||___/\___\___|_| |_|\__,_|\___/       
  _ _                       __      _  ___  _               
 / | |__  _ __  ___         \ \    / |/ _ \| |__  _ __  ___ 
 | | '_ \| '_ \/ __|    _____\ \   | | | | | '_ \| '_ \/ __|
 | | |_) | |_) \__ \   |_____/ /   | | |_| | |_) | |_) \__ \
 |_|_.__/| .__/|___/        /_/    |_|\___/|_.__/| .__/|___/
         |_|                                     |_|    
"#
                    );
                    info!(target: CRESCENDO_KEYWORD, "[Crescendo] Accelerating block rate 10 fold")
                }
                Self::_DYNAMIC => {}
                Self::_FULL => {}
                _ => {}
            }
            true
        } else {
            false
        }
    }
}

fn _hash_suffix(n: f64) -> (f64, &'static str) {
    match n {
        n if n < 1_000.0 => (n, "hash/block"),
        n if n < 1_000_000.0 => (n / 1_000.0, "Khash/block"),
        n if n < 1_000_000_000.0 => (n / 1_000_000.0, "Mhash/block"),
        n if n < 1_000_000_000_000.0 => (n / 1_000_000_000.0, "Ghash/block"),
        n if n < 1_000_000_000_000_000.0 => (n / 1_000_000_000_000.0, "Thash/block"),
        n if n < 1_000_000_000_000_000_000.0 => (n / 1_000_000_000_000_000.0, "Phash/block"),
        n => (n / 1_000_000_000_000_000_000.0, "Ehash/block"),
    }
}

fn _difficulty_desc(target: Uint320) -> String {
    let difficulty = MAX_DIFFICULTY_TARGET_AS_F64 / target.as_f64();
    let hashrate = difficulty * 2.0;
    let (rate, suffix) = _hash_suffix(hashrate);
    format!("{:.2} {}", rate, suffix)
}

/// A difficulty manager based on sampled block windows, implementing [KIP-0004](https://github.com/kaspanet/kips/blob/master/kip-0004.md)
#[derive(Clone)]
pub struct SampledDifficultyManager<T: HeaderStoreReader, U: GhostdagStoreReader> {
    headers_store: Arc<T>,
    _ghostdag_store: Arc<U>,
    genesis_hash: BlockHash,
    genesis_bits: u32,
    max_difficulty_target: Uint320,
    difficulty_window_size: usize,
    min_difficulty_window_size: usize,
    difficulty_sample_rate: u64,
    target_time_per_block: u64,
    /// ADR-0083 Decision 1: past this fence the expected duration counts only bits-priced rows.
    priced_rows_activation: ForkActivation,
    /// ADR-0083 Decision 1 as it states itself (mainnet audit, 2026-09-05): past this fence a
    /// receipt row — whose digest `check_pow_layer0` admits unconditionally — is not a priced row
    /// either. See [`kaspa_consensus_core::pow_layer0::algo_id_is_priced_by_bits_v2`].
    receipt_rows_activation: ForkActivation,
    /// ADR-0125: the execution lane's fence, mode folded in. Past it a round block is outside the
    /// DAA set of every block that merges it — not counted in the score, never sampled into a
    /// window — so the chain's DAA score, difficulty and median time are what they would be
    /// without the lane.
    round_lane: Option<ForkActivation>,
    /// ADR-0132 S: past it an attempt row prices nothing (its digest is admitted unconditionally).
    single_lottery: Option<ForkActivation>,
    /// ADR-0138: past this fence only a `bits`-priced block advances the DAA score.
    anchor_clock: Option<ForkActivation>,
    /// **ADR-0142: `Params::palw_clock_cursor`, and the cursor of every block already processed.**
    ///
    /// Past this fence the heartbeat lane earns its exemption only at or past the cursor, so the
    /// DAA advances at most once per interval of WALL CLOCK rather than once per chain block —
    /// which is what ADR-0138 needs and what the selected-parent slot rule could not give, because
    /// a block that advances no clock was moving the next opportunity to advance it.
    clock_cursor: Option<ForkActivation>,
}

impl<T: HeaderStoreReader, U: GhostdagStoreReader> SampledDifficultyManager<T, U> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        headers_store: Arc<T>,
        ghostdag_store: Arc<U>,
        genesis_hash: BlockHash,
        genesis_bits: u32,
        max_difficulty_target: Uint256,
        difficulty_window_size: usize,
        min_difficulty_window_size: usize,
        difficulty_sample_rate: u64,
        target_time_per_block: u64,
        priced_rows_activation: ForkActivation,
        receipt_rows_activation: ForkActivation,
        round_lane: Option<ForkActivation>,
        single_lottery: Option<ForkActivation>,
        anchor_clock: Option<ForkActivation>,
        clock_cursor: Option<ForkActivation>,
    ) -> Self {
        Self::check_min_difficulty_window_size(difficulty_window_size, min_difficulty_window_size);
        Self {
            headers_store,
            _ghostdag_store: ghostdag_store,
            genesis_hash,
            genesis_bits,
            max_difficulty_target: max_difficulty_target.into(),
            difficulty_window_size,
            min_difficulty_window_size,
            difficulty_sample_rate,
            target_time_per_block,
            priced_rows_activation,
            receipt_rows_activation,
            single_lottery,
            round_lane,
            anchor_clock,
            clock_cursor,
        }
    }

    /// [`palw_lane_advances_daa_v1`] with this manager's own fences.
    /// **ADR-0138: does a block of this lane, minted at `daa_score`, advance the DAA score?**
    ///
    /// Yes below `palw_anchor_clock`. Past it, a lane ticks the clock when **the chain sets its
    /// rate** — and that is two things, not one:
    ///
    /// * the lanes `bits` prices (the anchor), on the difficulty window's own three-generation
    ///   predicate (`algo_id_is_priced_by_bits`, `_v2` past `palw_receipt_rows_unpriced`, `_v3` past
    ///   the single lottery), so the clock and the retarget count the same blocks; and
    /// * **the heartbeat lane.** ADR-0066 took the heartbeat out of `bits` so a stock of certified
    ///   quanta could not tighten the attempt lane's target, but the heartbeat IS the chain's
    ///   liveness tick: one an hour (`HEARTBEAT_NOMINAL_INTERVAL_MS`), at most
    ///   `PALW_HEARTBEAT_MAX_PER_MERGESET` a mergeset, on a constant target no producer moves. A
    ///   chain whose hash lane stops still beats, and if the beat did not tick the clock the DAA
    ///   score would FREEZE — every fence, deadline, retention and leak window with it — while
    ///   blocks kept coming. Its contribution where the hash lane runs is one tick an hour against
    ///   thirty, so it distorts the 120-second meaning by nothing that matters.
    ///
    /// The attempt lane (rate `CCU/W`, set by compute) and the receipt lane (rate set by licensed
    /// receipts) are what this rule takes out of the clock, and the round lane was never in it.
    pub fn lane_advances_daa_at(&self, pow_algo_id: u8, daa_score: u64) -> bool {
        palw_lane_advances_daa_v1(pow_algo_id, daa_score, self.anchor_clock, self.single_lottery, self.receipt_rows_activation)
    }

    /// **ADR-0125: is `hash` a round block?** Its lane's id where the lane is open at its own DAA
    /// score. No header is read on a network that has not configured the lane.
    pub fn is_round_block(&self, hash: BlockHash) -> bool {
        let Some(fence) = self.round_lane else { return false };
        self.headers_store.get_header(hash).is_ok_and(|header| {
            header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1 && fence.is_active(header.daa_score)
        })
    }

    #[inline]
    #[must_use]
    pub fn difficulty_full_window_size(&self) -> u64 {
        self.difficulty_window_size as u64 * self.difficulty_sample_rate
    }

    /// Returns the DAA window lowest accepted blue score
    #[inline]
    #[must_use]
    pub fn lowest_daa_blue_score(&self, ghostdag_data: &GhostdagData) -> u64 {
        let difficulty_full_window_size = self.difficulty_full_window_size();
        ghostdag_data.blue_score.max(difficulty_full_window_size) - difficulty_full_window_size
    }

    /// The DAA score and the clock's decision it was computed from (ADR-0142 §5: one answer).
    #[inline]
    #[must_use]
    pub fn calc_daa_score(
        &self,
        ghostdag_data: &GhostdagData,
        mergeset_non_daa: &BlockHashSet,
        window: &BlockWindowHeap,
    ) -> (u64, PalwClockStepV1) {
        self.internal_calc_daa_score(ghostdag_data, mergeset_non_daa, window)
    }

    pub fn calc_daa_score_and_mergeset_non_daa_blocks(
        &self,
        ghostdag_data: &GhostdagData,
        window: &BlockWindowHeap,
        store: &(impl GhostdagStoreReader + ?Sized),
    ) -> (u64, BlockHashSet, PalwClockStepV1) {
        let lowest_daa_blue_score = self.lowest_daa_blue_score(ghostdag_data);
        // ADR-0125: a round block is outside the DAA set whatever its blue score.
        let mergeset_non_daa: BlockHashSet = ghostdag_data
            .unordered_mergeset()
            .filter(|hash| store.get_blue_score(*hash).unwrap() < lowest_daa_blue_score || self.is_round_block(*hash))
            .collect();
        let (daa_score, clock) = self.internal_calc_daa_score(ghostdag_data, &mergeset_non_daa, window);
        (daa_score, mergeset_non_daa, clock)
    }

    /// `daa_score` is the score of the block whose bits are being computed (the virtual's, for a
    /// template): it decides whether ADR-0083 Decision 1's fence is in force for this window.
    pub fn calculate_difficulty_bits(&self, window: &BlockWindowHeap, ghostdag_data: &GhostdagData, daa_score: u64) -> u32 {
        let priced_rows_only = self.priced_rows_activation.is_active(daa_score);
        let receipt_rows_unpriced = self.receipt_rows_activation.is_active(daa_score);
        // ADR-0132 S: past the single lottery an attempt row's `bits` priced nothing, so it may not
        // price the window either — the retarget would otherwise chase a cadence the class tickets
        // set against a target no block pays.
        let attempt_rows_unpriced = self.single_lottery.is_some_and(|fence| fence.is_active(daa_score));
        let difficulty_blocks = if priced_rows_only || receipt_rows_unpriced || attempt_rows_unpriced {
            self.get_difficulty_blocks_with_lane(window)
        } else {
            self.get_difficulty_blocks(window)
        };

        // Until there are enough blocks for a valid calculation the difficulty should remain constant.
        if difficulty_blocks.len() < self.min_difficulty_window_size {
            let selected_parent = ghostdag_data.selected_parent;
            if selected_parent == self.genesis_hash {
                return self.genesis_bits;
            }

            // We will use the selected parent as a source for the difficulty bits
            return self.headers_store.get_bits(selected_parent).unwrap();
        }

        retarget_bits(
            difficulty_blocks,
            self.target_time_per_block,
            self.difficulty_sample_rate,
            self.max_difficulty_target,
            priced_rows_only,
            receipt_rows_unpriced,
            attempt_rows_unpriced,
        )
    }

    pub fn estimate_network_hashes_per_second(&self, window: &BlockWindowHeap) -> DifficultyResult<u64> {
        self.internal_estimate_network_hashes_per_second(window)
    }
}

impl<T: HeaderStoreReader, U: GhostdagStoreReader> SampledDifficultyManager<T, U> {
    /// **ADR-0142 §5: one function, and every reader asks it.** The DAA score's exemption count and
    /// the clock's decision for this block are one computation, so they are one answer: `.0` is what
    /// `daa_exempt_count` subtracts, and `.1` rides in the DAA window for everything else that must
    /// agree with the score — the heartbeat adapter's `earliest`, the miner's slot hint. Two
    /// functions here is how construction and validation drifted apart twice already.
    ///
    /// Returns `(exempt_count, clock)`. `clock.cursor` is `None` below the fence and wherever the
    /// window holds no block at the selected parent's score.
    pub fn palw_clock_step_v1(
        &self,
        ghostdag_data: &GhostdagData,
        mergeset_non_daa: &BlockHashSet,
        window: &BlockWindowHeap,
    ) -> (u64, PalwClockStepV1) {
        // The fence is read at each merged block's OWN DAA score, as `is_round_block` reads the round
        // lane's: a block minted under the rule is exempt wherever it is later merged.
        if self.anchor_clock.is_none() {
            return (0, PalwClockStepV1::default());
        }
        let mut exempt = 0u64;
        let mut priced = 0u64;
        let mut heartbeats = 0u64;
        let mut newest_beat_ms: Option<u64> = None;
        for hash in ghostdag_data.unordered_mergeset() {
            if mergeset_non_daa.contains(&hash) {
                continue;
            }
            // One full-header read a mergeset block, the same read `is_round_block` has made since
            // ADR-0125 (the compact header carries no algo id), and the mergeset is bounded by
            // `mergeset_size_limit`. A header store hit is a cache hit on the hot path — the window
            // walk has just read the same blocks.
            // `expect`, not a silent skip. A skipped block would be counted as advancing the
            // clock, so a node that cannot read one mergeset header would compute a DIFFERENT DAA
            // score from its peers and fork quietly — the shape of this bundle's own Critical C-1.
            // Every mergeset member was header-processed before this block reached the DAA
            // computation, and `internal_calc_daa_score` already unwraps the selected parent's
            // score two lines up, so an unreadable header here is a broken database. Halting is the
            // safe answer to that; diverging is not.
            let header = self
                .headers_store
                .get_header(hash)
                .expect("every mergeset member was header-processed before its merging block's DAA score");
            if self.lane_advances_daa_at(header.pow_algo_id, header.daa_score) {
                priced += 1;
            } else {
                exempt += 1;
                if header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1 {
                    heartbeats += 1;
                    // ADR-0142: the newest beat in the mergeset is the one that would consume the
                    // slot, so it is the one the cursor is measured against and advanced from.
                    newest_beat_ms = Some(newest_beat_ms.map_or(header.timestamp, |t: u64| t.max(header.timestamp)));
                }
            }
        }
        // **The heartbeat stands in for a missing anchor, and only then** (ADR-0138 §3b). ADR-0066
        // took the beat out of `bits`, and its slot rule gives it the RECOVERY interval — one every
        // 120 s — whenever its selected parent is not a PALW-v2 block, which on a hash-lane chain is
        // every block. Counting it always would run the clock at twice the cadence and halve every
        // DAA window again; counting it never would let a chain whose hash lane stops keep producing
        // with a frozen clock. So: where the mergeset carries something `bits` priced, the beat adds
        // nothing; where it carries none, exactly one beat ticks in the anchor's place.
        //
        // **ADR-0142 puts a clock on that stand-in.** Below the cursor's fence the rule above is the
        // whole of it, and a chain producing faster than the interval starves the lane — measured
        // on the drill as a DAA frozen at its own flag day. Past it the beat earns its exemption
        // only at or past the cursor, and the cursor is moved by nothing else, so the score advances
        // at most once per interval of WALL CLOCK however fast the chain produces.
        let stand_in = priced == 0 && heartbeats > 0;
        let parent_daa = self.headers_store.get_daa_score(ghostdag_data.selected_parent).unwrap_or(0);
        if !self.clock_cursor.is_some_and(|fence| fence.is_active(parent_daa)) {
            // Below the cursor the stand-in is unconditional, so it is a grant in every sense but
            // the cursor's: reported as one, with no cursor, so a reader never mistakes it for a
            // slot being held.
            let clock = PalwClockStepV1 { governs: false, cursor: None, granted: stand_in };
            return (if stand_in { exempt.saturating_sub(1) } else { exempt }, clock);
        }
        // **ADR-0142 §6a: the reference is derived, not stored.** The block that took the score to
        // its current value is the one at the parent's score with the LOWEST BLUE SCORE, and the
        // window already in hand holds it. Every node that can process this block has this window,
        // so a pruned node and one that joined by pruning proof answer exactly as an archival one —
        // which a stored cursor could not, and which is why there is no store here.
        let parent_cursor = kaspa_consensus_core::palw_clock_cursor_v1::palw_clock_reference_v1(
            parent_daa,
            window.iter().filter_map(|item| {
                self.headers_store.get_compact_header_data(item.0.hash).ok().map(|h| {
                    kaspa_consensus_core::palw_clock_cursor_v1::ClockWindowBlockV1 {
                        daa_score: h.daa_score,
                        blue_score: h.blue_score,
                        timestamp_ms: h.timestamp,
                        // The identity that makes the selection total — see `ClockWindowBlockV1`.
                        hash: item.0.hash,
                    }
                })
            }),
        )
        .map(|reference| {
            kaspa_consensus_core::palw_clock_cursor_v1::palw_clock_cursor_from_reference_v1(
                reference,
                kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS,
            )
        });
        let beat_ms = newest_beat_ms.unwrap_or(0);
        let granted = stand_in
            && parent_cursor
                .is_none_or(|cursor| kaspa_consensus_core::palw_clock_cursor_v1::palw_clock_slot_admits_v1(&cursor, beat_ms).is_ok());
        // **The cursor this block leaves behind is NOT carried.** ADR-0142 §6a settled that the
        // reference is DERIVED from the window on every block, precisely so a pruned node and one
        // that joined by pruning proof answer as an archival one — a stored cursor could not, and
        // that was the bug the ADR replaced. An advanced cursor was computed here and written into a
        // `DaaWindow` field no reader ever read, which is the shape that makes a property suite pass
        // over a path nothing runs. It is gone; the derivation above is the whole rule.
        //
        // What IS returned is the cursor this block was JUDGED against — the parent's, derived from
        // this block's own window — and whether it granted. That is not a carried value: it is
        // recomputed with the score on every call, and it has readers (the heartbeat adapter and
        // the miner's hint read it for the virtual; `DaaWindow::clock` is where they find it).
        let clock = PalwClockStepV1 { governs: true, cursor: parent_cursor, granted };
        (if granted { exempt.saturating_sub(1) } else { exempt }, clock)
    }
}

impl<T: HeaderStoreReader, U: GhostdagStoreReader> DifficultyManagerExtension for SampledDifficultyManager<T, U> {
    fn daa_exempt_count(
        &self,
        ghostdag_data: &GhostdagData,
        mergeset_non_daa: &BlockHashSet,
        window: &BlockWindowHeap,
    ) -> (u64, PalwClockStepV1) {
        self.palw_clock_step_v1(ghostdag_data, mergeset_non_daa, window)
    }

    fn headers_store(&self) -> &dyn HeaderStoreReader {
        self.headers_store.deref()
    }
}

/// **The retarget, as arithmetic on rows.** Legacy Kaspa when `priced_rows_only` is false; ADR-0083
/// Decision 1 when it is true. The two differ in exactly one factor and one answer:
///
/// * the expected duration multiplies the number of rows the retarget must slow down — every row
///   (legacy) or only the rows priced by `bits` (past the fence); heartbeat rows still carry the
///   global bits into the average and still bound the span, since they are real elapsed time;
/// * a window with no priced row answers `max_difficulty_target` — the V2 doctrine (the class
///   lottery is the throttle), not the selected parent's bits, which would pin a chain whose
///   attempt lanes died at the bits that killed them.
///
/// Rows are at least `min_difficulty_window_size` (the caller's check), so at least two. The min-ts
/// row leaves the average exactly as it did in the legacy code, tie-broken by the same `Ord`, so a
/// build past the fence and one before it compute byte-identical bits until the fence.
fn retarget_bits(
    mut difficulty_blocks: Vec<DifficultyBlock>,
    target_time_per_block: u64,
    difficulty_sample_rate: u64,
    max_difficulty_target: Uint320,
    priced_rows_only: bool,
    receipt_rows_unpriced: bool,
    attempt_rows_unpriced: bool,
) -> u32 {
    let (min_ts_index, max_ts_index) = difficulty_blocks.iter().position_minmax().into_option().unwrap();

    let min_ts = difficulty_blocks[min_ts_index].timestamp;
    let max_ts = difficulty_blocks[max_ts_index].timestamp;

    // We remove the minimal block because we want the average target for the internal window.
    difficulty_blocks.swap_remove(min_ts_index);

    // We need Uint320 to avoid overflow when summing and multiplying by the window size.
    let difficulty_blocks_len = difficulty_blocks.len() as u64;
    // A row counts toward the expected duration iff its lane is priced by `bits` (ADR-0083
    // Decision 1). Before the first fence every row counts; past it the heartbeat lane does not;
    // past the second the receipt lane does not either — its digest is admitted unconditionally,
    // so counting it measured quanta being spent rather than work (mainnet audit, 2026-09-05).
    let counts = |diff_block: &DifficultyBlock| {
        let unpriced_lane = (priced_rows_only && !diff_block.priced)
            || (receipt_rows_unpriced && diff_block.receipt)
            || (attempt_rows_unpriced && diff_block.attempt);
        !unpriced_lane
    };
    let counted_rows = if priced_rows_only || receipt_rows_unpriced || attempt_rows_unpriced {
        difficulty_blocks.iter().filter(|diff_block| counts(diff_block)).count() as u64
    } else {
        difficulty_blocks_len
    };
    if counted_rows == 0 {
        return Uint256::try_from(max_difficulty_target).expect("max target < Uint256::MAX").compact_target_bits();
    }
    let targets_sum: Uint320 =
        difficulty_blocks.into_iter().map(|diff_block| Uint320::from(Uint256::from_compact_target_bits(diff_block.bits))).sum();
    let average_target = targets_sum / difficulty_blocks_len;
    let measured_duration = max(max_ts - min_ts, 1);
    let expected_duration = target_time_per_block * difficulty_sample_rate * counted_rows; // This does differ from FullDifficultyManager version
    let new_target = average_target * measured_duration / expected_duration;

    Uint256::try_from(new_target.min(max_difficulty_target)).expect("max target < Uint256::MAX").compact_target_bits()
}

/// One row of a difficulty window as a node's own chain reports it (`getBlock`: hash, blue work,
/// timestamp, bits, `pow_algo_id`), for replaying [`retarget_bits_from_rows`] over real history.
#[derive(Clone, Debug)]
pub struct DifficultyRow {
    pub hash: BlockHash,
    pub blue_work: BlueWorkType,
    pub timestamp: u64,
    pub bits: u32,
    pub pow_algo_id: u8,
}

/// **The retarget over rows anyone can read off a node** — the same arithmetic the header
/// processor runs, exposed so a chain's own history can be replayed through both rules (ADR-0083
/// §4's check). Returns `None` when there are fewer than two rows, where the header processor
/// would have held the previous bits instead.
pub fn retarget_bits_from_rows(
    rows: &[DifficultyRow],
    target_time_per_block: u64,
    difficulty_sample_rate: u64,
    max_difficulty_target: Uint256,
    priced_rows_only: bool,
    receipt_rows_unpriced: bool,
    attempt_rows_unpriced: bool,
) -> Option<u32> {
    if rows.len() < 2 {
        return None;
    }
    let blocks = rows
        .iter()
        .map(|row| DifficultyBlock {
            timestamp: row.timestamp,
            bits: row.bits,
            sortable_block: SortableBlock::new(row.hash, row.blue_work),
            priced: algo_id_is_priced_by_bits(row.pow_algo_id),
            receipt: row.pow_algo_id == POW_ALGO_ID_PALW_RECEIPT_V3,
            attempt: is_palw_attempt_algo_id(row.pow_algo_id),
        })
        .collect();
    Some(retarget_bits(
        blocks,
        target_time_per_block,
        difficulty_sample_rate,
        max_difficulty_target.into(),
        priced_rows_only,
        receipt_rows_unpriced,
        attempt_rows_unpriced,
    ))
}

pub fn calc_work(bits: u32) -> BlueWorkType {
    let target = Uint256::from_compact_target_bits(bits);
    // Source: https://github.com/bitcoin/bitcoin/blob/2e34374bf3e12b37b0c66824a6c998073cdfab01/src/chain.cpp#L131
    // We need to compute 2**256 / (bnTarget+1), but we can't represent 2**256
    // as it's too large for an arith_uint256. However, as 2**256 is at least as large
    // as bnTarget+1, it is equal to ((2**256 - bnTarget - 1) / (bnTarget+1)) + 1,
    // or ~bnTarget / (bnTarget+1) + 1.

    let res = (!target / (target + 1)) + 1;
    res.into()
}

pub fn level_work(level: u8, max_block_level: u8) -> BlueWorkType {
    // Need to make a special condition for level 0 to ensure true work is always used
    if level == 0 {
        return 0.into();
    }
    // We use 256 here so the result corresponds to the work at the level from calc_level_from_pow
    let exp = (level as u32) + 256 - (max_block_level as u32);
    BlueWorkType::from_u64(1) << exp.min(MAX_WORK_LEVEL as u32)
}

#[derive(Eq)]
struct DifficultyBlock {
    timestamp: u64,
    bits: u32,
    sortable_block: SortableBlock,
    /// ADR-0083 Decision 1: whether this row's lane is priced by `bits` (every row, on the legacy path).
    priced: bool,
    /// Whether this row is a receipt-lane row — unpriced past `Params::palw_receipt_rows_unpriced`
    /// (`false` on the legacy path, which never reads the lane).
    receipt: bool,
    /// ADR-0132 S: whether this row is a PALW attempt row — unpriced past
    /// `Params::palw_single_lottery`, where its digest stopped being compared to `bits` at all.
    attempt: bool,
}

impl PartialEq for DifficultyBlock {
    fn eq(&self, other: &Self) -> bool {
        // If the sortable blocks are equal the timestamps and bits that are associated with the block are equal for sure.
        self.sortable_block == other.sortable_block
    }
}

impl PartialOrd for DifficultyBlock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DifficultyBlock {
    fn cmp(&self, other: &Self) -> Ordering {
        self.timestamp.cmp(&other.timestamp).then_with(|| self.sortable_block.cmp(&other.sortable_block))
    }
}

/// **ADR-0138's rule, as a function of the fences alone** — so it can be read and tested without a
/// store: does a block of this lane, minted at this height, advance the DAA score on its own?
/// The heartbeat's stand-in tick is decided per MERGESET, not per lane, so it lives in
/// [`DifficultyManagerExtension::daa_exempt_count`] rather than here.
pub fn palw_lane_advances_daa_v1(
    pow_algo_id: u8,
    daa_score: u64,
    anchor_clock: Option<ForkActivation>,
    single_lottery: Option<ForkActivation>,
    receipt_rows_activation: ForkActivation,
) -> bool {
    if !anchor_clock.is_some_and(|fence| fence.is_active(daa_score)) {
        return true;
    }
    if single_lottery.is_some_and(|fence| fence.is_active(daa_score)) {
        kaspa_consensus_core::pow_layer0::algo_id_is_priced_by_bits_v3(pow_algo_id)
    } else if receipt_rows_activation.is_active(daa_score) {
        kaspa_consensus_core::pow_layer0::algo_id_is_priced_by_bits_v2(pow_algo_id)
    } else {
        algo_id_is_priced_by_bits(pow_algo_id)
    }
}

#[cfg(test)]
mod tests {
    use kaspa_consensus_core::{BlockLevel, BlueWorkType, MAX_WORK_LEVEL};
    use kaspa_math::{Uint256, Uint320};
    use kaspa_pow::calc_level_from_pow;

    use crate::processes::difficulty::{calc_work, level_work};
    use kaspa_utils::hex::ToHex;

    #[test]
    fn test_target_levels() {
        let max_block_level: BlockLevel = 225;
        for level in 1..=max_block_level {
            // required pow for level
            let level_target = (Uint320::from_u64(1) << (max_block_level - level).max(MAX_WORK_LEVEL) as u32) - Uint320::from_u64(1);
            let level_target = Uint256::from_be_bytes(level_target.to_be_bytes()[8..40].try_into().unwrap());
            let calculated_level = calc_level_from_pow(level_target, max_block_level);

            let true_level_work = calc_work(level_target.compact_target_bits());
            let calc_level_work = level_work(level, max_block_level);

            // A "good enough" estimate of level work is within 1% diff from work with actual level target
            // It's hard to calculate percentages with these large numbers, so to get around using floats
            // we multiply the difference by 100. if the result is <= the calc_level_work it means
            // difference must have been less than 1%
            let (percent_diff, overflowed) = (true_level_work - calc_level_work).overflowing_mul(BlueWorkType::from_u64(100));
            let is_good_enough = percent_diff <= calc_level_work;

            println!("Level {}:", level);
            println!(
                "    data | {} | {} | {} / {} |",
                level_target.compact_target_bits(),
                level_target.bits(),
                calculated_level,
                max_block_level
            );
            println!("    pow  | {}", level_target.to_hex());
            println!("    work | 0000000000000000{}", true_level_work.to_hex());
            println!("  lvwork | 0000000000000000{}", calc_level_work.to_hex());
            println!(" diff<1% | {}", !overflowed && (is_good_enough));

            assert!(is_good_enough);
        }
    }

    #[test]
    fn test_base_level_work() {
        // Expect that at level 0, the level work is always 0
        assert_eq!(BlueWorkType::from(0), level_work(0, 255));
    }

    /// **ADR-0083 Decision 1, on the shape that killed testnet-11 5f.** 264 rows, 122 s slots,
    /// ~3.2 rows per slot of which one is priced (the chain heartbeat and two sibling heartbeats per
    /// slot, all algo 8, plus an attempt row every third slot), two-minute cadence. Legacy reads it
    /// as a fast chain and tightens; the fence reads the priced rows' true rate and eases to MAX.
    #[test]
    fn heartbeat_rows_no_longer_tighten_the_bits_past_the_fence() {
        use super::{DifficultyRow, retarget_bits_from_rows};
        use kaspa_consensus_core::BlockHash;
        use kaspa_consensus_core::config::params::MAX_DIFFICULTY_TARGET;
        use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_PALW_COMMITTED_V2};

        let bits_now = 0x1f65bd04u32; // testnet-11 at DAA 826
        let cadence_ms = 120_000u64;
        let mut rows = Vec::new();
        let mut slot = 0u64;
        while rows.len() < 264 {
            let ts = 1_788_000_000_000 + slot * 122_000;
            for sibling in 0..3u64 {
                let attempt = sibling == 2 && slot.is_multiple_of(3);
                rows.push(DifficultyRow {
                    hash: BlockHash::from_u64_word(slot * 8 + sibling + 1),
                    blue_work: BlueWorkType::from_u64(slot * 8 + sibling + 1),
                    timestamp: ts,
                    bits: bits_now,
                    pow_algo_id: if attempt { POW_ALGO_ID_PALW_COMMITTED_V2 } else { POW_ALGO_ID_HEARTBEAT_V1 },
                });
            }
            slot += 1;
        }
        rows.truncate(264);
        let legacy = retarget_bits_from_rows(&rows, cadence_ms, 1, MAX_DIFFICULTY_TARGET, false, false, false).unwrap();
        let fenced = retarget_bits_from_rows(&rows, cadence_ms, 1, MAX_DIFFICULTY_TARGET, true, false, false).unwrap();
        let target = |bits: u32| Uint256::from_compact_target_bits(bits);
        assert!(
            target(legacy) < target(bits_now),
            "legacy: three rows per two-minute slot read as a fast chain and tighten ({legacy:#010x})"
        );
        // The retarget is multiplicative in the window's AVERAGE: 30 priced rows over 87 slots of
        // 122 s against 120 s each is a ratio of ~2.95, so a window whose rows are still tight
        // eases by that factor per window rather than jumping — the opposite direction to legacy.
        // (The live chain's window held NO priced row, which is the next case: MAX in one step.)
        let ratio = target(fenced).as_f64() / target(bits_now).as_f64();
        assert!(target(fenced) > target(bits_now), "past the fence the priced rate is one per six minutes: it eases ({fenced:#010x})");
        assert!((2.5..3.5).contains(&ratio), "eases by the priced-rate ratio, ~2.95, got {ratio:.3}");

        // ADR-0132 S: past the single lottery the attempt rows price nothing either — the same
        // window that eased by the priced-rate ratio reads as a window with no priced row.
        let single = retarget_bits_from_rows(&rows, cadence_ms, 1, MAX_DIFFICULTY_TARGET, true, false, true).unwrap();
        assert_eq!(single, MAX_DIFFICULTY_TARGET.compact_target_bits(), "no row prices the window past the single lottery");
        assert_ne!(single, fenced, "and that is not the receipt fence's verdict on the same rows");
        // No priced row at all: MAX, not the previous bits — the recovery ADR-0083 §3 needs.
        for row in rows.iter_mut() {
            row.pow_algo_id = POW_ALGO_ID_HEARTBEAT_V1;
        }
        assert_eq!(
            retarget_bits_from_rows(&rows, cadence_ms, 1, MAX_DIFFICULTY_TARGET, true, false, false).unwrap(),
            MAX_DIFFICULTY_TARGET.compact_target_bits()
        );

        // The steady state the fence reaches: every row at MAX, attempts every third slot — the
        // priced rate is a third of the cadence, so the window stays at MAX.
        for row in rows.iter_mut() {
            row.bits = MAX_DIFFICULTY_TARGET.compact_target_bits();
            let slot = (row.hash.as_bytes()[0] as u64).wrapping_sub(1) / 8;
            row.pow_algo_id = if row.blue_work == BlueWorkType::from_u64(slot * 8 + 3) {
                POW_ALGO_ID_PALW_COMMITTED_V2
            } else {
                POW_ALGO_ID_HEARTBEAT_V1
            };
        }
        assert_eq!(
            retarget_bits_from_rows(&rows, cadence_ms, 1, MAX_DIFFICULTY_TARGET, true, false, false).unwrap(),
            MAX_DIFFICULTY_TARGET.compact_target_bits(),
            "at MAX with sparse attempts the window stays at MAX"
        );

        // Every row priced (a hash network, or a V2 chain with the heartbeat silent): the fence is
        // the legacy arithmetic, bit for bit.
        for row in rows.iter_mut() {
            row.pow_algo_id = POW_ALGO_ID_PALW_COMMITTED_V2;
        }
        assert_eq!(
            retarget_bits_from_rows(&rows, cadence_ms, 1, MAX_DIFFICULTY_TARGET, true, false, false),
            retarget_bits_from_rows(&rows, cadence_ms, 1, MAX_DIFFICULTY_TARGET, false, false, false),
            "with every row priced the two rules are one arithmetic"
        );

        // Priced rows denser than the cadence still tighten past the fence: the interval control
        // ADR-0071 kept is intact — the fence removes the emitters from the count, not the meter.
        let dense: Vec<DifficultyRow> = (0..264u64)
            .map(|i| DifficultyRow {
                hash: BlockHash::from_u64_word(i + 1),
                blue_work: BlueWorkType::from_u64(i + 1),
                timestamp: 1_788_000_000_000 + i * 30_000,
                bits: MAX_DIFFICULTY_TARGET.compact_target_bits(),
                pow_algo_id: POW_ALGO_ID_PALW_COMMITTED_V2,
            })
            .collect();
        let tightened = retarget_bits_from_rows(&dense, cadence_ms, 1, MAX_DIFFICULTY_TARGET, true, false, false).unwrap();
        assert!(target(tightened) < MAX_DIFFICULTY_TARGET, "attempts every 30 s against a 120 s cadence tighten");
        assert!(retarget_bits_from_rows(&dense[..1], cadence_ms, 1, MAX_DIFFICULTY_TARGET, true, false, false).is_none());
    }

    /// **A receipt row is not a priced row** (mainnet audit, 2026-09-05). `check_pow_layer0`
    /// admits every receipt digest — ADR-0044 Decision 6 — so a window full of receipt rows read
    /// as a chain running fast and tightened the attempt lanes' `bits` exactly as heartbeat rows did
    /// before ADR-0083. Past the second fence the receipt rows leave the count, and a window
    /// holding nothing but receipts answers MAX.
    #[test]
    fn receipt_rows_leave_the_count_past_the_second_fence() {
        use super::{DifficultyRow, retarget_bits_from_rows};
        use kaspa_consensus_core::BlockHash;
        use kaspa_consensus_core::config::params::MAX_DIFFICULTY_TARGET;
        use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3};

        let bits_now = 0x1f65bd04u32;
        let cadence_ms = 120_000u64;
        let mut rows = Vec::new();
        let mut slot = 0u64;
        while rows.len() < 264 {
            let ts = 1_788_000_000_000 + slot * 122_000;
            for sibling in 0..3u64 {
                let attempt = sibling == 2 && slot.is_multiple_of(3);
                rows.push(DifficultyRow {
                    hash: BlockHash::from_u64_word(slot * 8 + sibling + 1),
                    blue_work: BlueWorkType::from_u64(slot * 8 + sibling + 1),
                    timestamp: ts,
                    bits: bits_now,
                    pow_algo_id: if attempt { POW_ALGO_ID_PALW_COMMITTED_V2 } else { POW_ALGO_ID_PALW_RECEIPT_V3 },
                });
            }
            slot += 1;
        }
        rows.truncate(264);
        let target = |bits: u32| Uint256::from_compact_target_bits(bits);
        // Under the first fence alone the receipt rows still count: three rows a slot tightens.
        let first = retarget_bits_from_rows(&rows, cadence_ms, 1, MAX_DIFFICULTY_TARGET, true, false, false).unwrap();
        assert!(target(first) < target(bits_now), "receipt rows counted as priced tighten the attempt lanes ({first:#010x})");
        // Past the second fence only the attempt rows count: one per six minutes, it eases.
        let second = retarget_bits_from_rows(&rows, cadence_ms, 1, MAX_DIFFICULTY_TARGET, true, true, false).unwrap();
        assert!(target(second) > target(bits_now), "with receipts unpriced the chain reads as slow and eases ({second:#010x})");
        // Nothing but receipts: MAX, the recovery answer, not the bits that killed the lanes.
        for row in rows.iter_mut() {
            row.pow_algo_id = POW_ALGO_ID_PALW_RECEIPT_V3;
        }
        assert_eq!(
            retarget_bits_from_rows(&rows, cadence_ms, 1, MAX_DIFFICULTY_TARGET, true, true, false).unwrap(),
            MAX_DIFFICULTY_TARGET.compact_target_bits(),
            "a window of receipt rows alone holds no priced row"
        );
    }
}

#[cfg(test)]
mod adr0138_clock_tests {
    use super::palw_lane_advances_daa_v1 as ticks;
    use kaspa_consensus_core::config::params::ForkActivation;
    use kaspa_consensus_core::pow_layer0::{
        POW_ALGO_ID_BLAKE2B_SHA3, POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_EXEC_V3,
        POW_ALGO_ID_PALW_RECEIPT_V3, POW_ALGO_ID_PALW_ROUND_V1,
    };

    const ANCHOR: u8 = POW_ALGO_ID_BLAKE2B_SHA3;
    const LANES: [(&str, u8); 6] = [
        ("anchor", ANCHOR),
        ("attempt-committed", POW_ALGO_ID_PALW_COMMITTED_V2),
        ("attempt-exec", POW_ALGO_ID_PALW_EXEC_V3),
        ("receipt", POW_ALGO_ID_PALW_RECEIPT_V3),
        ("heartbeat", POW_ALGO_ID_HEARTBEAT_V1),
        ("round", POW_ALGO_ID_PALW_ROUND_V1),
    ];

    /// **The whole classification, one table, at every fence combination.** The round lane is absent
    /// from the answers because it never reaches this rule — `is_round_block` puts it in
    /// `mergeset_non_daa`, out of the window and out of the count, before the exemption is asked.
    #[test]
    fn the_clock_is_the_anchor_and_the_heartbeat_and_nothing_else_past_the_fence() {
        let never = ForkActivation::never();
        let always = ForkActivation::always();
        // Below the fence: every lane ticks, exactly as it did before ADR-0138.
        for (name, algo) in LANES {
            assert!(ticks(algo, 100, None, None, never), "{name}: no clock fence, no exemption");
            assert!(ticks(algo, 100, Some(ForkActivation::new(101)), Some(always), always), "{name}: the fence is one past");
        }
        // Past the fence, with the single lottery in force (the 6,001 shape).
        let armed = Some(always);
        assert!(ticks(ANCHOR, 100, armed, armed, always), "the anchor ticks: `bits` prices it");
        assert!(
            !ticks(POW_ALGO_ID_HEARTBEAT_V1, 100, armed, armed, always),
            "the heartbeat does not tick ON ITS OWN — it stands in for a missing anchor, which is a fact about the \
             MERGESET and lives in `daa_exempt_count`"
        );
        for (name, algo) in [("attempt-committed", POW_ALGO_ID_PALW_COMMITTED_V2), ("attempt-exec", POW_ALGO_ID_PALW_EXEC_V3)] {
            assert!(!ticks(algo, 100, armed, armed, always), "{name}: unpriced past the single lottery, out of the clock");
        }
        assert!(!ticks(POW_ALGO_ID_PALW_RECEIPT_V3, 100, armed, armed, always), "the receipt lane: unpriced since ADR-0083");
        assert!(!ticks(POW_ALGO_ID_PALW_ROUND_V1, 100, armed, armed, always), "the round lane: never in the clock");
        // Past the clock fence but BELOW the single lottery: the attempt lane is still priced, so it
        // still ticks — the clock and the difficulty window agree at every height, which is the rule.
        let lottery_later = Some(ForkActivation::new(200));
        for (name, algo) in [("attempt-committed", POW_ALGO_ID_PALW_COMMITTED_V2), ("attempt-exec", POW_ALGO_ID_PALW_EXEC_V3)] {
            assert!(ticks(algo, 100, armed, lottery_later, always), "{name}: priced below the lottery, so it ticks");
            assert!(!ticks(algo, 200, armed, lottery_later, always), "{name}: and stops at the lottery's own height");
        }
        // The receipt lane before ADR-0083's fence: priced, so it ticks.
        assert!(ticks(POW_ALGO_ID_PALW_RECEIPT_V3, 100, armed, None, ForkActivation::new(500)), "receipt: priced below its fence");
        assert!(!ticks(POW_ALGO_ID_PALW_RECEIPT_V3, 500, armed, None, ForkActivation::new(500)), "…and unpriced at it");
    }

    /// **Determinism: the answer is a function of (lane, height, fences) and nothing else.** Called
    /// a thousand times over every lane and a spread of heights, it never differs — no clock, no
    /// store, no host state enters it, so two nodes folding one block classify it alike.
    #[test]
    fn the_classification_is_a_pure_function_every_node_computes_alike() {
        let armed = Some(ForkActivation::new(6_001));
        let lottery = Some(ForkActivation::new(6_001));
        let receipts = ForkActivation::new(2_400);
        for (_, algo) in LANES {
            for daa in [0u64, 1, 2_399, 2_400, 6_000, 6_001, 6_002, 1 << 40] {
                let first = ticks(algo, daa, armed, lottery, receipts);
                for _ in 0..1_000 {
                    assert_eq!(ticks(algo, daa, armed, lottery, receipts), first, "algo {algo} at daa {daa} is one answer");
                }
            }
        }
    }
}
