//! **Telemetry for the clock lane and the attempt lottery** — ADR-0140 D3/§9 step 1, ADR-0141 D2.
//!
//! Neither accumulator is read by a rule. They exist because both ADRs turn on numbers nobody has:
//! ADR-0140 asks which of the heartbeat's four jobs actually matter in production, and ADR-0141
//! asks how much inference the lottery discards. Without these, the first half of either argument
//! is a guess, and a guess is what this repository keeps catching itself making.
//!
//! Pure accumulators: no clock, no store, no I/O. The caller supplies the facts of one observation
//! and the node prints the totals; the tests below are the specification.

use serde::{Deserialize, Serialize};

/// **What one chain block tells us about the heartbeat lane.** Supplied by the node from facts it
/// already holds when it advances the virtual chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeartbeatObservationV1 {
    /// Is this chain block itself a heartbeat?
    pub is_heartbeat: bool,
    /// Did this block's mergeset carry anything a price paced? ADR-0138's first clause.
    pub mergeset_had_priced_block: bool,
    /// How many beats its mergeset carried.
    pub heartbeats_in_mergeset: u64,
    /// How many transactions it carried. The CARRIER role, counted apart from the clock role.
    pub transactions: u64,
    /// Was a heartbeat this block's selected parent? The ORDERING role — a beat that became the
    /// spine the next block was built on, which is the job ADR-0060 D1.2's ε exists for.
    pub selected_parent_is_heartbeat: bool,
    /// Median time past of this block, in milliseconds.
    pub mtp_ms: u64,
}

/// **The lane's four jobs, counted apart.** ADR-0140 D3: they are four decisions today answered in
/// one place, and a later ADR that replaces the price of time needs to know which of them anyone
/// was relying on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatTelemetryV1 {
    /// Chain blocks observed.
    pub chain_blocks: u64,
    /// Of those, heartbeats.
    pub heartbeat_chain_blocks: u64,
    /// CLOCK: mergesets where a beat stood in for a missing price, so the DAA advanced because of
    /// the heartbeat lane and not despite it.
    pub clock_advances_by_heartbeat: u64,
    /// CLOCK: mergesets that carried something priced, so beats added nothing to the score.
    pub clock_advances_by_price: u64,
    /// CARRIER: transactions that rode a heartbeat.
    pub transactions_carried_by_heartbeats: u64,
    /// CARRIER: heartbeats that carried at least one transaction.
    pub heartbeats_carrying_transactions: u64,
    /// ORDERING: chain blocks whose selected parent was a beat.
    pub blocks_parented_by_a_heartbeat: u64,
    /// Episodes: how many times the chain entered a stretch with no priced block.
    pub heartbeat_only_episodes: u64,
    /// Milliseconds spent in those stretches, summed.
    pub heartbeat_only_ms: u64,
    /// The longest single stretch, in milliseconds — the number an operator actually reacts to.
    pub longest_heartbeat_only_ms: u64,
    /// Internal: the MTP at which the current unpriced stretch began, if one is open.
    episode_began_mtp_ms: Option<u64>,
    /// Internal: the MTP of the last observation, so an episode can be closed at the right point.
    last_mtp_ms: u64,
}

impl HeartbeatTelemetryV1 {
    /// Fold one chain block in. Order matters — the caller walks the selected chain forwards.
    pub fn observe(&mut self, o: &HeartbeatObservationV1) {
        self.chain_blocks += 1;
        if o.is_heartbeat {
            self.heartbeat_chain_blocks += 1;
        }
        if o.mergeset_had_priced_block {
            self.clock_advances_by_price += 1;
        } else if o.heartbeats_in_mergeset > 0 {
            self.clock_advances_by_heartbeat += 1;
        }
        if o.is_heartbeat && o.transactions > 0 {
            self.heartbeats_carrying_transactions += 1;
            self.transactions_carried_by_heartbeats += o.transactions;
        }
        if o.selected_parent_is_heartbeat {
            self.blocks_parented_by_a_heartbeat += 1;
        }

        // An episode is a run of chain blocks whose mergesets carried nothing priced. It opens on
        // the first such block and closes on the first priced one, so a stretch that is still open
        // when the report is printed is counted up to the last observation rather than dropped.
        if o.mergeset_had_priced_block {
            if let Some(began) = self.episode_began_mtp_ms.take() {
                let len = o.mtp_ms.saturating_sub(began);
                self.heartbeat_only_ms += len;
                self.longest_heartbeat_only_ms = self.longest_heartbeat_only_ms.max(len);
            }
        } else if self.episode_began_mtp_ms.is_none() {
            self.heartbeat_only_episodes += 1;
            self.episode_began_mtp_ms = Some(o.mtp_ms);
        }
        self.last_mtp_ms = o.mtp_ms;
    }

    /// The totals with any still-open episode closed at the last observation — what a report prints.
    /// Idempotent, and does not disturb the accumulator.
    pub fn sealed(&self) -> Self {
        let mut out = *self;
        if let Some(began) = out.episode_began_mtp_ms.take() {
            let len = out.last_mtp_ms.saturating_sub(began);
            out.heartbeat_only_ms += len;
            out.longest_heartbeat_only_ms = out.longest_heartbeat_only_ms.max(len);
        }
        out
    }

    /// Permille of observed chain blocks that are heartbeats.
    pub fn heartbeat_share_permille(&self) -> u64 {
        if self.chain_blocks == 0 { 0 } else { self.heartbeat_chain_blocks * 1_000 / self.chain_blocks }
    }

    /// Permille of clock advances the heartbeat lane was responsible for. On a chain with a priced
    /// lane this is near zero; on testnet-11 past `palw_anchor_clock` it is the whole clock, which
    /// is the fact ADR-0140 was written around.
    pub fn clock_by_heartbeat_permille(&self) -> u64 {
        let total = self.clock_advances_by_heartbeat + self.clock_advances_by_price;
        if total == 0 { 0 } else { self.clock_advances_by_heartbeat * 1_000 / total }
    }
}

/// **What one draw tells us about the lottery.** ADR-0141 M1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttemptDrawObservationV1 {
    /// Did this draw win? A losing draw is a forward pass the network bought and discarded.
    pub won: bool,
    /// Milliseconds this draw took — one forward pass, since ADR-0117 fixed one forward to one draw.
    pub elapsed_ms: u64,
}

/// **What the lottery costs, counted.** ADR-0141 D2: built now because it costs nothing, is useful
/// to an operator regardless, and is the evidence the lottery question cannot be argued without.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptLotteryTelemetryV1 {
    /// Forward passes spent.
    pub draws: u64,
    /// Draws that produced a block.
    pub wins: u64,
    /// Milliseconds of inference spent on draws that produced nothing.
    pub wasted_inference_ms: u64,
    /// Milliseconds of inference spent on draws that produced a block.
    pub winning_inference_ms: u64,
    /// The longest run of consecutive losing draws — the tail an operator feels, which an average
    /// hides completely.
    pub longest_losing_run: u64,
    /// Internal: the current run.
    current_losing_run: u64,
}

impl AttemptLotteryTelemetryV1 {
    pub fn observe(&mut self, o: &AttemptDrawObservationV1) {
        self.draws += 1;
        if o.won {
            self.wins += 1;
            self.winning_inference_ms += o.elapsed_ms;
            self.current_losing_run = 0;
        } else {
            self.wasted_inference_ms += o.elapsed_ms;
            self.current_losing_run += 1;
            self.longest_losing_run = self.longest_losing_run.max(self.current_losing_run);
        }
    }

    /// Draws per accepted block, in milli — so "1,000" reads as one draw per block and "7,000" as
    /// seven. Integer, because this is a report and not a rule.
    pub fn draws_per_win_milli(&self) -> u64 {
        if self.wins == 0 { 0 } else { self.draws.saturating_mul(1_000) / self.wins }
    }

    /// Permille of inference time that produced nothing. The single number ADR-0141 M1 is about:
    /// if it is small the question is aesthetic, and if it is large it is not.
    pub fn wasted_permille(&self) -> u64 {
        let total = self.wasted_inference_ms + self.winning_inference_ms;
        if total == 0 { 0 } else { self.wasted_inference_ms.saturating_mul(1_000) / total }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hb(priced: bool, beats: u64, is_beat: bool, txs: u64, parent_beat: bool, mtp: u64) -> HeartbeatObservationV1 {
        HeartbeatObservationV1 {
            is_heartbeat: is_beat,
            mergeset_had_priced_block: priced,
            heartbeats_in_mergeset: beats,
            transactions: txs,
            selected_parent_is_heartbeat: parent_beat,
            mtp_ms: mtp,
        }
    }

    /// The three jobs are counted apart, which is the whole point: a beat that advanced the clock,
    /// a beat that carried a transaction and a beat that became a parent are three different facts
    /// about the lane, and a later ADR needs to know which of them anyone relied on.
    #[test]
    fn the_lanes_jobs_are_counted_separately_and_do_not_bleed_into_each_other() {
        let mut t = HeartbeatTelemetryV1::default();
        // A priced chain: beats exist, carry nothing, parent nothing, and pace no clock.
        t.observe(&hb(true, 0, false, 5, false, 1_000));
        t.observe(&hb(true, 1, true, 0, false, 2_000));
        assert_eq!(t.clock_advances_by_heartbeat, 0, "a beat beside a priced block paces nothing");
        assert_eq!(t.clock_advances_by_price, 2);
        assert_eq!(t.transactions_carried_by_heartbeats, 0, "the transactions rode the priced block");
        assert_eq!(t.blocks_parented_by_a_heartbeat, 0);
        assert_eq!(t.heartbeat_chain_blocks, 1);

        // The price stops. Now the beat is the clock, and it is also the parent and the carrier —
        // three roles the counters keep apart.
        t.observe(&hb(false, 1, true, 3, false, 3_000));
        t.observe(&hb(false, 1, true, 0, true, 4_000));
        assert_eq!(t.clock_advances_by_heartbeat, 2);
        assert_eq!((t.heartbeats_carrying_transactions, t.transactions_carried_by_heartbeats), (1, 3));
        assert_eq!(t.blocks_parented_by_a_heartbeat, 1);
        assert_eq!(t.clock_by_heartbeat_permille(), 500, "half the clock advances came from the lane");
    }

    /// An episode is the number an operator reacts to, so its start and end have to be right, and a
    /// stretch still open when the report prints must not vanish.
    #[test]
    fn an_unpriced_stretch_is_one_episode_and_an_open_one_still_counts() {
        let mut t = HeartbeatTelemetryV1::default();
        t.observe(&hb(true, 0, false, 0, false, 0));
        for (i, mtp) in [1_000u64, 2_000, 3_000].into_iter().enumerate() {
            t.observe(&hb(false, 1, true, 0, i > 0, mtp));
        }
        t.observe(&hb(true, 0, false, 0, true, 10_000));
        assert_eq!(t.heartbeat_only_episodes, 1, "one stretch, not three");
        assert_eq!(t.heartbeat_only_ms, 9_000, "from the first unpriced block to the priced one that ended it");
        assert_eq!(t.longest_heartbeat_only_ms, 9_000);

        // A second stretch that never ends: `sealed` closes it at the last observation.
        t.observe(&hb(false, 1, true, 0, false, 11_000));
        t.observe(&hb(false, 1, true, 0, true, 14_000));
        assert_eq!(t.heartbeat_only_episodes, 2);
        assert_eq!(t.heartbeat_only_ms, 9_000, "the open one is not yet counted in the raw accumulator");
        let sealed = t.sealed();
        assert_eq!(sealed.heartbeat_only_ms, 12_000, "sealed closes it at the last block seen");
        assert_eq!(sealed.longest_heartbeat_only_ms, 9_000);
        assert_eq!(t.sealed(), sealed, "sealing is idempotent and does not disturb the accumulator");
    }

    /// Empty is empty, and no ratio divides by zero.
    #[test]
    fn nothing_observed_divides_by_nothing() {
        let t = HeartbeatTelemetryV1::default();
        assert_eq!((t.heartbeat_share_permille(), t.clock_by_heartbeat_permille()), (0, 0));
        assert_eq!(t.sealed(), t);
        let l = AttemptLotteryTelemetryV1::default();
        assert_eq!((l.draws_per_win_milli(), l.wasted_permille()), (0, 0));
    }

    /// ADR-0141 M1's numbers. The averages are not the interesting part — the longest losing run is,
    /// because that is the wait a producer actually experiences and an average erases it.
    #[test]
    fn the_lottery_counts_what_it_discards_and_the_tail_it_makes_a_producer_wait() {
        let mut l = AttemptLotteryTelemetryV1::default();
        for _ in 0..6 {
            l.observe(&AttemptDrawObservationV1 { won: false, elapsed_ms: 1_000 });
        }
        l.observe(&AttemptDrawObservationV1 { won: true, elapsed_ms: 1_000 });
        for _ in 0..2 {
            l.observe(&AttemptDrawObservationV1 { won: false, elapsed_ms: 1_000 });
        }
        l.observe(&AttemptDrawObservationV1 { won: true, elapsed_ms: 1_000 });

        assert_eq!((l.draws, l.wins), (10, 2));
        assert_eq!(l.draws_per_win_milli(), 5_000, "five forwards an accepted block");
        assert_eq!(l.wasted_permille(), 800, "four fifths of the inference produced nothing");
        assert_eq!(l.longest_losing_run, 6, "the tail, which the 5-per-block average hides");
    }
}
