use crate::model::stores::{
    block_window_cache::BlockWindowHeap,
    ghostdag::{GhostdagData, GhostdagStoreReader},
    headers::HeaderStoreReader,
};
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    BlockHashSet, BlueWorkType, MAX_WORK_LEVEL,
    config::params::MAX_DIFFICULTY_TARGET_AS_F64,
    errors::difficulty::{DifficultyError, DifficultyResult},
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

    #[inline]
    #[must_use]
    fn internal_calc_daa_score(&self, ghostdag_data: &GhostdagData, mergeset_non_daa: &BlockHashSet) -> u64 {
        let sp_daa_score = self.headers_store().get_daa_score(ghostdag_data.selected_parent).unwrap();
        sp_daa_score + (ghostdag_data.mergeset_size() - mergeset_non_daa.len()) as u64
    }

    fn get_difficulty_blocks(&self, window: &BlockWindowHeap) -> Vec<DifficultyBlock> {
        window
            .iter()
            .map(|item| {
                let data = self.headers_store().get_compact_header_data(item.0.hash).unwrap();
                DifficultyBlock { timestamp: data.timestamp, bits: data.bits, sortable_block: item.0.clone() }
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
}

impl<T: HeaderStoreReader, U: GhostdagStoreReader> SampledDifficultyManager<T, U> {
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
        }
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

    #[inline]
    #[must_use]
    pub fn calc_daa_score(&self, ghostdag_data: &GhostdagData, mergeset_non_daa: &BlockHashSet) -> u64 {
        self.internal_calc_daa_score(ghostdag_data, mergeset_non_daa)
    }

    pub fn calc_daa_score_and_mergeset_non_daa_blocks(
        &self,
        ghostdag_data: &GhostdagData,
        store: &(impl GhostdagStoreReader + ?Sized),
    ) -> (u64, BlockHashSet) {
        let lowest_daa_blue_score = self.lowest_daa_blue_score(ghostdag_data);
        let mergeset_non_daa: BlockHashSet =
            ghostdag_data.unordered_mergeset().filter(|hash| store.get_blue_score(*hash).unwrap() < lowest_daa_blue_score).collect();
        (self.internal_calc_daa_score(ghostdag_data, &mergeset_non_daa), mergeset_non_daa)
    }

    pub fn calculate_difficulty_bits(&self, window: &BlockWindowHeap, ghostdag_data: &GhostdagData) -> u32 {
        let mut difficulty_blocks = self.get_difficulty_blocks(window);

        // Until there are enough blocks for a valid calculation the difficulty should remain constant.
        if difficulty_blocks.len() < self.min_difficulty_window_size {
            let selected_parent = ghostdag_data.selected_parent;
            if selected_parent == self.genesis_hash {
                return self.genesis_bits;
            }

            // We will use the selected parent as a source for the difficulty bits
            return self.headers_store.get_bits(selected_parent).unwrap();
        }

        let (min_ts_index, max_ts_index) = difficulty_blocks.iter().position_minmax().into_option().unwrap();

        let min_ts = difficulty_blocks[min_ts_index].timestamp;
        let max_ts = difficulty_blocks[max_ts_index].timestamp;

        // We remove the minimal block because we want the average target for the internal window.
        difficulty_blocks.swap_remove(min_ts_index);

        // We need Uint320 to avoid overflow when summing and multiplying by the window size.
        let difficulty_blocks_len = difficulty_blocks.len() as u64;
        let targets_sum: Uint320 =
            difficulty_blocks.into_iter().map(|diff_block| Uint320::from(Uint256::from_compact_target_bits(diff_block.bits))).sum();
        let average_target = targets_sum / difficulty_blocks_len;
        let measured_duration = max(max_ts - min_ts, 1);
        let expected_duration = self.target_time_per_block * self.difficulty_sample_rate * difficulty_blocks_len; // This does differ from FullDifficultyManager version
        let new_target = average_target * measured_duration / expected_duration;

        Uint256::try_from(new_target.min(self.max_difficulty_target)).expect("max target < Uint256::MAX").compact_target_bits()
    }

    pub fn estimate_network_hashes_per_second(&self, window: &BlockWindowHeap) -> DifficultyResult<u64> {
        self.internal_estimate_network_hashes_per_second(window)
    }
}

impl<T: HeaderStoreReader, U: GhostdagStoreReader> DifficultyManagerExtension for SampledDifficultyManager<T, U> {
    fn headers_store(&self) -> &dyn HeaderStoreReader {
        self.headers_store.deref()
    }
}

/// ADR-0039 §5.3/§16.3 — the certified COMPUTE-work delta `ΔC` credited for one algo-4 (PALW) source
/// block, in the SAME work unit as the hash lane so `E = H + min(C, 4H)` mixes like with like: it is
/// `compute_work_scale · calc_work(bits)` where `bits` is the block's one-shot eligibility-target
/// difficulty. Deliberately `calc_work` (32-bit compact), NEVER `calc_work_512` — mixing the two work
/// forms in one accounting domain would split the DAG (see `pow_layer0::calc_work_512` audit-L note).
/// Saturating so a pathological scale·work cannot wrap. Inert: no algo-4 block exists to be credited.
pub fn normalize_palw_work(bits: u32, compute_work_scale: u64) -> BlueWorkType {
    let (scaled, overflow) = calc_work(bits).overflowing_mul_u64(compute_work_scale);
    if overflow { BlueWorkType::MAX } else { scaled }
}

/// ADR-0039 §16.3 — the PURE per-lane retarget of `expected_bits` from a lane's sampled window
/// (panel-frozen). Mirrors the Adjust arithmetic of [`SampledDifficultyManager::calculate_difficulty_bits`]
/// (average of the sampled targets × measured/expected ratio, clamped to `max_target`) but adds the
/// per-step `max_adjust_factor` clamp the live single-lane engine lacks — via the shared pure
/// [`kaspa_consensus_core::palw::lane_retarget_decision`], so a sparse lane (few samples reached at ~10×
/// wall-clock) cannot collapse difficulty in one step (panel FS-6). Store-free + deterministic: the
/// caller passes the lane-filtered sample targets, the measured window duration, and the lane's
/// expected duration; below `min_samples` it HOLDs `hold_bits` (the carried lane bits, panel Q6). The
/// live `calculate_difficulty_bits` is left byte-for-byte untouched (panel Q4) — this is the dedicated
/// active-lane path.
///
/// `expected_ms` must be `lane_target_time_ms · sample_rate · sample_count` (the same product basis as
/// the live engine, but per-lane). `sample_bits` are the already-selected window targets (min-ts block
/// trimming is the caller's, matching the live engine's `swap_remove`).
pub fn lane_retarget_bits(
    sample_bits: &[u32],
    measured_ms: u64,
    expected_ms: u64,
    hold_bits: u32,
    min_samples: u64,
    max_adjust_factor: u64,
    max_target: Uint320,
) -> u32 {
    use kaspa_consensus_core::palw::{LaneRetargetDecision, lane_retarget_decision};
    let count = sample_bits.len() as u64;
    let clamped_measured_ms = match lane_retarget_decision(count, min_samples, measured_ms, expected_ms, max_adjust_factor) {
        LaneRetargetDecision::HoldLastBits => return hold_bits,
        LaneRetargetDecision::Adjust { clamped_measured_ms } => clamped_measured_ms,
    };
    let targets_sum: Uint320 = sample_bits.iter().map(|&bits| Uint320::from(Uint256::from_compact_target_bits(bits))).sum();
    let average_target = targets_sum / count;
    let new_target = average_target * clamped_measured_ms.max(1) / expected_ms.max(1);
    Uint256::try_from(new_target.min(max_target)).expect("max target < Uint256::MAX").compact_target_bits()
}

/// ADR-0039 §16.3 / C6 clause 7 — the PURE per-lane expected difficulty bits from a lane-filtered
/// window of `(bits, timestamp_ms)` samples. It mirrors [`SampledDifficultyManager::calculate_difficulty_bits`]
/// step-for-step — pre-trim window-size HOLD, drop the min-timestamp block, average the remaining
/// targets × measured/expected ratio, clamp to `max_target` — but per-lane and with the
/// `max_adjust_factor` clamp of [`lane_retarget_bits`]. Store-free + deterministic: the caller passes
/// the SAME-LANE window blocks (filtered by `pow_algo_id`) and the lane's `hold_bits` (a pure-header
/// carry — NOT the virtual, pruned lane-bits store). `expected_ms = lane_target_time_ms · sample_rate ·
/// post-trim-count`, the same product basis the live engine uses but per-lane.
///
/// Equivalence to the live single-lane engine: with `lane_target_time_ms`/`sample_rate`/`min_samples`
/// set to the live `target_time_per_block`/`difficulty_sample_rate`/`min_difficulty_window_size`, a
/// non-clamping `max_adjust_factor`, and `hold_bits` = the selected parent's bits, this returns the
/// identical `bits` on the same window (the ONLY structural differences are the lane filter and the
/// sparse-lane clamp, both intentional). The pre-trim HOLD check matches the live engine's
/// `difficulty_blocks.len() < min_difficulty_window_size`; we then pass `min_samples = 1` to
/// `lane_retarget_bits` so its own (post-trim) min-samples check cannot re-HOLD at the window boundary.
#[allow(clippy::too_many_arguments)]
pub fn lane_expected_bits(
    lane_samples: &[(u32, u64)],
    lane_target_time_ms: u64,
    sample_rate: u64,
    min_samples: u64,
    max_adjust_factor: u64,
    hold_bits: u32,
    max_target: Uint320,
) -> u32 {
    // Pre-trim HOLD, exactly like the live engine's `difficulty_blocks.len() < min_difficulty_window_size`.
    if (lane_samples.len() as u64) < min_samples {
        return hold_bits;
    }
    // Min-timestamp block (the live engine's `position_minmax` min side, then `swap_remove`).
    let min_i = lane_samples.iter().enumerate().min_by_key(|(_, (_, ts))| *ts).map(|(i, _)| i).unwrap();
    let min_ts = lane_samples[min_i].1;
    let max_ts = lane_samples.iter().map(|(_, ts)| *ts).max().unwrap();
    let measured_ms = max_ts.saturating_sub(min_ts).max(1);
    // Average target over the internal window = every sample EXCEPT the trimmed min-ts block.
    let sample_bits: Vec<u32> = lane_samples.iter().enumerate().filter(|(i, _)| *i != min_i).map(|(_, (b, _))| *b).collect();
    let expected_ms = lane_target_time_ms.saturating_mul(sample_rate).saturating_mul(sample_bits.len() as u64);
    lane_retarget_bits(&sample_bits, measured_ms, expected_ms, hold_bits, 1, max_adjust_factor, max_target)
}

/// ADR-0039 §16.3 / C6 clause 7 — **the single lane-`bits` derivation**, shared by the header-stage
/// difficulty check and the algo-4 mining template.
///
/// Filters `window` to the blocks on `header_algo_id`'s lane (each block's `pow_algo_id` read from its
/// header — it is not in `CompactHeaderData`) and runs [`lane_expected_bits`] over their
/// `(bits, timestamp)` pairs. Below the lane's `min_samples` that HOLDs at the lane's `genesis_bits`.
///
/// This exists as one function on purpose. `bits` is not a miner-selectable field: a template that
/// stamps anything other than what `pre_pow_validation` will recompute produces a block its own node
/// rejects with `UnexpectedDifficulty`. Using `genesis_replica_bits` directly happens to work while no
/// algo-4 block exists yet — that is the lane's HOLD value, not a constant — and silently stops working
/// at the `min_samples`-th algo-4 block. Two implementations of that rule would therefore agree in
/// every test written before the lane has real samples and diverge in production.
/// ADR-MA §12 — the optional per-set sublane restriction of [`lane_bits_from_window`]. On a
/// registry-active net a v5 PALW-lane header commits a `compute_set_id`, and its difficulty runs
/// over ONLY same-set samples against the set's own stretched interval; `None` (every shipped
/// preset, and every pre-v5 header) is the flat single-lane path, byte-identical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSetSublane {
    /// The header's committed `palw_compute_set_id` — the sublane sample predicate.
    pub compute_set_id: kaspa_hashes::Hash64,
    /// The share the header's committed allocation plan allots the set, ALREADY validated
    /// nonzero by §13.1 header-stage resolution (§12.2 — a zero-share set mines nothing, so it
    /// never reaches difficulty).
    pub target_share_bps: u16,
}

pub fn lane_bits_from_window<S: HeaderStoreReader + ?Sized>(
    headers_store: &S,
    window: &BlockWindowHeap,
    header_algo_id: u8,
    palw_lane_difficulty: &kaspa_consensus_core::palw::LaneDifficultyParams,
    per_set: Option<PalwSetSublane>,
) -> u32 {
    use kaspa_consensus_core::pow_layer0::check_live_algo_id;
    let lane = check_live_algo_id(header_algo_id, true).expect("a PALW-active header carries a live lane algo id");
    // Filter the DAA window to the header's lane (same-lane blocks only). Bounded by the window size.
    // ADR-MA §12: on a registry-active net the PALW lane splits into per-set virtual sublanes — the
    // predicate then also requires the sample's committed `palw_compute_set_id` to equal the
    // header's. Pre-v5 samples carry the all-zero id (hash-invisible zero-guard) and a real set id
    // is content-derived (never zero), so old samples cannot leak into a sublane window.
    let mut lane_samples: Vec<(u32, u64)> = Vec::new();
    for item in window.iter() {
        let hdr = headers_store.get_header(item.0.hash).unwrap();
        if check_live_algo_id(hdr.pow_algo_id, true).ok() != Some(lane) {
            continue;
        }
        if let Some(sublane) = per_set
            && hdr.palw_compute_set_id != sublane.compute_set_id
        {
            continue;
        }
        lane_samples.push((hdr.bits, hdr.timestamp));
    }
    let p = palw_lane_difficulty;
    // §12 — the sublane's expected spacing: a set holding `share/10000` of the lane paces its own
    // blocks `10000/share` lane intervals apart (`per_set_target_interval_ms`: integer ceil,
    // overflow-saturating). The share was validated nonzero at §13.1 resolution, so `None` here is
    // unreachable; failing loud beats minting a full-rate sublane for a set that owns none of it.
    let lane_target_time_ms = match per_set {
        Some(sublane) => {
            kaspa_consensus_core::palw_compute_set::per_set_target_interval_ms(p.lane_target_time_ms(lane), sublane.target_share_bps)
                .expect("per-set difficulty requires the §13.1-validated nonzero share (§12.2)")
        }
        None => p.lane_target_time_ms(lane),
    };
    lane_expected_bits(
        &lane_samples,
        lane_target_time_ms,
        p.lane_sample_rate(lane),
        p.min_samples,
        p.max_adjust_factor,
        p.genesis_bits(lane),
        kaspa_consensus_core::config::params::MAX_DIFFICULTY_TARGET.into(),
    )
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

#[cfg(test)]
mod tests {
    use kaspa_consensus_core::{BlockLevel, BlueWorkType, MAX_WORK_LEVEL};
    use kaspa_math::{Uint256, Uint320};
    use kaspa_pow::calc_level_from_pow;

    use crate::processes::difficulty::{calc_work, lane_expected_bits, lane_retarget_bits, level_work, normalize_palw_work};
    use kaspa_utils::hex::ToHex;

    /// ADR-0039 §16.3 lane retarget: below `min_samples` HOLDs `hold_bits` (no collapse); a fast lane
    /// (measured ≪ expected) raises difficulty but the step is bounded by `max_adjust_factor`; a slow
    /// lane (measured ≫ expected) lowers it, also bounded. Steady state (measured == expected) ≈ the
    /// window average.
    #[test]
    fn test_lane_retarget_bits() {
        let max_target: Uint320 = kaspa_consensus_core::config::params::MAX_DIFFICULTY_TARGET.into();
        let bits = 0x1d00ffff_u32;
        // The window's own canonical target bits (compact encoding round-trips through the target).
        let window_bits = Uint256::from_compact_target_bits(bits).compact_target_bits();
        let samples = vec![bits; 60];
        let expected = 6000u64; // 60 samples × 100 ms

        // below min_samples ⇒ HOLD, ignoring measured.
        assert_eq!(lane_retarget_bits(&samples[..10], 999_999, expected, 0xaabbccdd, 60, 2, max_target), 0xaabbccdd);

        // steady state (measured == expected) with a uniform window ⇒ the window's own target.
        let steady = lane_retarget_bits(&samples, expected, expected, 0, 60, 2, max_target);
        assert_eq!(steady, window_bits, "measured==expected over a uniform window is the window target");

        // fast lane: measured = expected/10 ⇒ target wants ×1/10, but the ×2 clamp bounds the step, so
        // the new target is at most average/(1/2)=average×... i.e. clamped_measured >= expected/2 ⇒
        // new_target >= average×(1/2). Difficulty rises (target shrinks) but not below half the average.
        let fast = lane_retarget_bits(&samples, expected / 10, expected, 0, 60, 2, max_target);
        let half = lane_retarget_bits(&samples, expected / 2, expected, 0, 60, 2, max_target);
        assert_eq!(fast, half, "fast lane is clamped to the max_adjust_factor floor (expected/2)");
        assert_ne!(fast, window_bits, "a fast lane does move difficulty");

        // slow lane: measured = expected×10 ⇒ clamped to ×2 (easier), symmetric.
        let slow = lane_retarget_bits(&samples, expected * 10, expected, 0, 60, 2, max_target);
        let dbl = lane_retarget_bits(&samples, expected * 2, expected, 0, 60, 2, max_target);
        assert_eq!(slow, dbl, "slow lane is clamped to the max_adjust_factor ceiling (expected×2)");
    }

    /// C6 clause 7: `lane_expected_bits` mirrors the live engine's pre-trim HOLD + min-ts trim +
    /// measured/expected duration, then delegates the Adjust+clamp to `lane_retarget_bits`. Below
    /// `min_samples` it HOLDs; otherwise it equals the manual (trim min-ts block; measured = max−min ts;
    /// expected = target·rate·post-trim-count) → `lane_retarget_bits`; a uniform steady-state window
    /// returns the window's own canonical target.
    #[test]
    fn test_lane_expected_bits() {
        let max_target: Uint320 = kaspa_consensus_core::config::params::MAX_DIFFICULTY_TARGET.into();
        let bits = 0x1d00ffff_u32;
        let (target_ms, rate, min_samples, adjust) = (100u64, 1u64, 4u64, 1000u64); // large adjust ⇒ no clamp

        // fewer than min_samples ⇒ HOLD (ignores timestamps).
        let few = [(bits, 1000u64), (bits, 2000), (bits, 3000)]; // 3 < 4
        assert_eq!(lane_expected_bits(&few, target_ms, rate, min_samples, adjust, 0xaabbccdd, max_target), 0xaabbccdd);

        // enough samples ⇒ trim the min-ts block, delegate. min_ts=1000@idx0, max_ts=5500, measured=4500;
        // sample_bits=[bits;3]; expected = 100·1·3 = 300.
        let samples = [(bits, 1000u64), (bits, 2500), (bits, 4000), (bits, 5500)]; // n = 4 == min_samples
        let got = lane_expected_bits(&samples, target_ms, rate, min_samples, adjust, 0, max_target);
        let manual = lane_retarget_bits(&[bits, bits, bits], 4500, 300, 0, 1, adjust, max_target);
        assert_eq!(got, manual, "wrapper == manual trim+duration → lane_retarget_bits");

        // steady state: after trimming the min-ts block, measured == expected over a uniform window ⇒ the
        // window's own canonical target. ts [0,100,200,300] ⇒ min-ts=0 trimmed, measured = 300, expected = 300.
        let steady = [(bits, 0u64), (bits, 100), (bits, 200), (bits, 300)];
        let window_bits = Uint256::from_compact_target_bits(bits).compact_target_bits();
        assert_eq!(lane_expected_bits(&steady, target_ms, rate, min_samples, adjust, 0, max_target), window_bits);
    }

    /// ADR-0039 §5.3: `normalize_palw_work` credits `scale · calc_work(bits)` in the SAME unit as the
    /// hash lane — so `ΔC` at scale 1 equals the block's hash-equivalent work, and the scale multiplies
    /// linearly without wrapping.
    #[test]
    fn test_normalize_palw_work_matches_calc_work_scaled() {
        for bits in [0x1e00ffff_u32, 0x1d00ffff, 0x1b0404cb] {
            assert_eq!(normalize_palw_work(bits, 0), BlueWorkType::ZERO, "scale 0 is Stage-A weight zero");
            assert_eq!(normalize_palw_work(bits, 1), calc_work(bits), "scale 1 == hash unit");
            assert_eq!(normalize_palw_work(bits, 4), calc_work(bits).overflowing_mul_u64(4).0, "linear in scale");
        }
        // Saturating (does not panic / wrap) at an extreme scale.
        let _ = normalize_palw_work(0x1d00ffff, u64::MAX);
    }

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

    /// ADR-MA §12 — the per-set sublane split of [`lane_bits_from_window`]: with `per_set` the
    /// sample predicate narrows to the header's committed `palw_compute_set_id` and the retarget
    /// runs against the share-stretched interval; with `None` it is byte-identical to the flat
    /// lane. Verified as EQUALITIES against [`lane_expected_bits`] over hand-filtered samples, so
    /// a filter or interval-composition bug cannot cancel out.
    #[test]
    fn test_lane_bits_per_set_sublane() {
        use crate::model::stores::block_window_cache::BlockWindowHeap;
        use crate::model::stores::headers::{HeaderStoreReader, HeaderWithBlockLevel};
        use crate::processes::difficulty::{PalwSetSublane, lane_bits_from_window};
        use crate::processes::ghostdag::ordering::SortableBlock;
        use kaspa_consensus_core::header::Header;
        use kaspa_consensus_core::{BlockHash, BlockHashMap, HashMapCustomHasher};
        use kaspa_database::prelude::StoreError;
        use std::cmp::Reverse;
        use std::sync::Arc;

        struct LaneHeaders(BlockHashMap<Arc<Header>>);
        #[allow(unused_variables)]
        impl HeaderStoreReader for LaneHeaders {
            fn get_daa_score(&self, hash: BlockHash) -> Result<u64, StoreError> {
                unimplemented!()
            }
            fn get_blue_score(&self, hash: BlockHash) -> Result<u64, StoreError> {
                unimplemented!()
            }
            fn get_timestamp(&self, hash: BlockHash) -> Result<u64, StoreError> {
                unimplemented!()
            }
            fn get_bits(&self, hash: BlockHash) -> Result<u32, StoreError> {
                unimplemented!()
            }
            fn get_header(&self, hash: BlockHash) -> Result<Arc<Header>, StoreError> {
                Ok(self.0.get(&hash).unwrap().clone())
            }
            fn get_header_with_block_level(&self, hash: BlockHash) -> Result<HeaderWithBlockLevel, StoreError> {
                unimplemented!()
            }
            fn get_compact_header_data(
                &self,
                hash: BlockHash,
            ) -> Result<crate::model::stores::headers::CompactHeaderData, StoreError> {
                unimplemented!()
            }
        }

        let set_a = kaspa_hashes::Hash64::from_bytes([0xa1; 64]);
        let set_b = kaspa_hashes::Hash64::from_bytes([0xb2; 64]);
        let unknown_set = kaspa_hashes::Hash64::from_bytes([0xcc; 64]);

        let bits = 0x1d00ffff_u32;
        let mut map = BlockHashMap::new();
        let mut window = BlockWindowHeap::new();
        let mut tag = 0u8;
        // (algo, bits, timestamp, set): set A paced at the lane interval, set B twice as fast,
        // and two hash-lane blocks with WILD bits that must never sample into lane 4.
        for (algo, b, ts, set) in [
            (4u8, bits, 0u64, set_a),
            (4, bits, 1_000, set_a),
            (4, bits, 2_000, set_a),
            (4, bits, 3_000, set_a),
            (4, bits, 100, set_b),
            (4, bits, 600, set_b),
            (4, bits, 1_100, set_b),
            (4, bits, 1_600, set_b),
            (3, 0x1f00ffff, 50, kaspa_hashes::Hash64::default()),
            (3, 0x1f00ffff, 150, kaspa_hashes::Hash64::default()),
        ] {
            tag += 1;
            let hash = kaspa_hashes::Hash64::from_bytes([tag; 64]);
            let mut hdr = Header::from_precomputed_hash(hash, vec![]);
            hdr.pow_algo_id = algo;
            hdr.bits = b;
            hdr.timestamp = ts;
            hdr.palw_compute_set_id = set;
            map.insert(hash, Arc::new(hdr));
            window.push(Reverse(SortableBlock::new(hash, BlueWorkType::from_u64(tag as u64))));
        }
        let store = LaneHeaders(map);

        let p = kaspa_consensus_core::palw::LaneDifficultyParams {
            hash_target_time_ms: 1_000,
            replica_target_time_ms: 1_000,
            hash_window_size: 60,
            replica_window_size: 60,
            min_samples: 2,
            compute_work_scale: 1,
            max_adjust_factor: 1_000_000, // effectively unclamped: isolate the interval math
            hash_sample_rate: 1,
            replica_sample_rate: 1,
            genesis_hash_bits: 0x1e00aaaa,
            genesis_replica_bits: 0x1e00bbbb,
        };
        let max_target: Uint320 = kaspa_consensus_core::config::params::MAX_DIFFICULTY_TARGET.into();
        let a_samples: Vec<(u32, u64)> = vec![(bits, 0), (bits, 1_000), (bits, 2_000), (bits, 3_000)];
        let ab_samples: Vec<(u32, u64)> = vec![
            (bits, 0),
            (bits, 1_000),
            (bits, 2_000),
            (bits, 3_000),
            (bits, 100),
            (bits, 600),
            (bits, 1_100),
            (bits, 1_600),
        ];

        // Flat lane (None): every algo-4 block samples regardless of set — and the algo-3 wild
        // bits stay out (equality would break if they leaked in).
        let flat = lane_bits_from_window(&store, &window, 4, &p, None);
        assert_eq!(flat, lane_expected_bits(&ab_samples, 1_000, 1, 2, 1_000_000, p.genesis_replica_bits, max_target));

        // Sublane A at full share: ONLY set-A samples, lane interval unchanged. Set B's faster
        // blocks no longer drag the retarget (this differs from `flat` — proof the filter bit).
        let full_a = lane_bits_from_window(&store, &window, 4, &p, Some(PalwSetSublane { compute_set_id: set_a, target_share_bps: 10_000 }));
        assert_eq!(full_a, lane_expected_bits(&a_samples, 1_000, 1, 2, 1_000_000, p.genesis_replica_bits, max_target));
        assert_ne!(full_a, flat);

        // Sublane A at 2500 bps: same samples, interval stretched ×4 (§12
        // `ceil(lane_interval × 10000 / share)`), so the set is EXPECTED 4× slower.
        let quarter_a =
            lane_bits_from_window(&store, &window, 4, &p, Some(PalwSetSublane { compute_set_id: set_a, target_share_bps: 2_500 }));
        assert_eq!(quarter_a, lane_expected_bits(&a_samples, 4_000, 1, 2, 1_000_000, p.genesis_replica_bits, max_target));
        assert_ne!(quarter_a, full_a);

        // A set with no samples in the window: below min_samples ⇒ HOLD at the lane's genesis
        // bits (a fresh set enters at lane genesis difficulty; §12.1 initial-window calibration
        // is governance's lever, not a silent default).
        let fresh =
            lane_bits_from_window(&store, &window, 4, &p, Some(PalwSetSublane { compute_set_id: unknown_set, target_share_bps: 5_000 }));
        assert_eq!(fresh, p.genesis_replica_bits);
    }
}
