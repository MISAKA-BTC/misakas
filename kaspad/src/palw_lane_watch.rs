//! **The PALW lane watch: is this chain doing PALW work, or only ticking its clock?**
//!
//! The 2026-09-23 route-matrix audit read testnet-12 from genesis: 1,230 blocks, every one of them a
//! heartbeat, not one attempt — while every liveness signal an operator looked at was green. "Blocks
//! are arriving" and "the DAA is advancing" are both true of a chain that carries no PALW work at
//! all, because the heartbeat lane exists precisely to keep the clock moving when nothing else does.
//! That is what hid a dead attempt lane for a whole deployment.
//!
//! So this service counts LANES, not blocks: once a minute it walks the selected chain back from the
//! sink over [`PALW_LANE_WATCH_WINDOW_BLOCKS`] blocks, classifies each by its header's `pow_algo_id`,
//! publishes the mix through `getPalwNodeStatus`, and logs an ERROR while the window holds no PALW
//! work block at all. It runs on every ConsensusV2 node, producing or not — a seeder host or an
//! explorer backend is exactly where an operator reads the chain's health.

use std::sync::Arc;
use std::time::Duration;

use kaspa_consensus_core::pow_layer0::{
    POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_EXEC_V3, POW_ALGO_ID_PALW_RECEIPT_V3,
    POW_ALGO_ID_PALW_ROUND_V1,
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    error, info,
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};
use kaspa_p2p_flows::flow_context::FlowContext;

const PALW_LANE_WATCH: &str = "palw-lane-watch";
/// How often the mix is taken.
const PALW_LANE_WATCH_PERIOD: Duration = Duration::from_secs(60);
/// How many selected-chain blocks the mix is taken over — an hour and a quarter of blocks at the
/// frozen 120 s cadence of heartbeats, far more where the attempt lane is producing.
pub(crate) const PALW_LANE_WATCH_WINDOW_BLOCKS: usize = 600;
/// A window shorter than this is a chain too young to judge (a fresh genesis's first hours): no
/// alarm, only the mix.
pub(crate) const PALW_LANE_WATCH_MIN_BLOCKS_TO_ALARM: u64 = 30;
/// While the alarm stands it is repeated this often, so an operator scrolling a log still meets it.
const PALW_LANE_WATCH_REPEAT: Duration = Duration::from_secs(600);

/// **The selected chain's last blocks, by lane.**
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PalwLaneMixV1 {
    pub window_blocks: u64,
    /// `pow_algo_id` 6: a PALW attempt — a model or floor inference won a class ticket.
    pub attempt_blocks: u64,
    /// 7: a certified free-prompt quantum, spent.
    pub receipt_blocks: u64,
    /// 9: an execution-lane block.
    pub execution_blocks: u64,
    /// 10: a round permit (ADR-0130's execution rounds).
    pub round_blocks: u64,
    /// 8: the clock, and nothing else.
    pub heartbeat_blocks: u64,
    /// Anything else (the genesis, a legacy lane).
    pub other_blocks: u64,
    /// The DAA of the newest block in the window that carried PALW work, if any did.
    pub last_work_daa: Option<u64>,
    /// The DAA span the window covers, oldest first.
    pub from_daa: u64,
    pub to_daa: u64,
}

impl PalwLaneMixV1 {
    /// Blocks that carried PALW work — anything but the clock and the genesis.
    pub fn work_blocks(&self) -> u64 {
        self.attempt_blocks + self.receipt_blocks + self.execution_blocks + self.round_blocks
    }

    fn count(&mut self, pow_algo_id: u8, daa_score: u64) {
        let lane = match pow_algo_id {
            POW_ALGO_ID_PALW_COMMITTED_V2 => &mut self.attempt_blocks,
            POW_ALGO_ID_PALW_RECEIPT_V3 => &mut self.receipt_blocks,
            POW_ALGO_ID_PALW_EXEC_V3 => &mut self.execution_blocks,
            POW_ALGO_ID_PALW_ROUND_V1 => &mut self.round_blocks,
            POW_ALGO_ID_HEARTBEAT_V1 => &mut self.heartbeat_blocks,
            _ => &mut self.other_blocks,
        };
        *lane += 1;
        self.window_blocks += 1;
        let works = matches!(
            pow_algo_id,
            POW_ALGO_ID_PALW_COMMITTED_V2 | POW_ALGO_ID_PALW_RECEIPT_V3 | POW_ALGO_ID_PALW_EXEC_V3 | POW_ALGO_ID_PALW_ROUND_V1
        );
        if works && self.last_work_daa.is_none_or(|daa| daa_score > daa) {
            self.last_work_daa = Some(daa_score);
        }
    }

    /// One line an operator can read: `attempt=… receipt=… execution=… round=… heartbeat=… other=…`.
    pub fn summary(&self) -> String {
        format!(
            "{} blocks (DAA {}..{}): attempt={} receipt={} execution={} round={} heartbeat={} other={}",
            self.window_blocks,
            self.from_daa,
            self.to_daa,
            self.attempt_blocks,
            self.receipt_blocks,
            self.execution_blocks,
            self.round_blocks,
            self.heartbeat_blocks,
            self.other_blocks
        )
    }

    /// **The alarm**: a window old enough to judge, with no PALW work block in it. `None` otherwise.
    pub fn alarm(&self) -> Option<String> {
        (self.window_blocks >= PALW_LANE_WATCH_MIN_BLOCKS_TO_ALARM && self.work_blocks() == 0).then(|| {
            format!(
                "no PALW work block in the last {} selected-chain blocks (DAA {}..{}) — {} of them are heartbeats: the chain \
                 is running on its clock alone. Check that producers are running for a class that can produce (the floor \
                 always can), that panel seats are up, and each producer's `holding:` line",
                self.window_blocks, self.from_daa, self.to_daa, self.heartbeat_blocks
            )
        })
    }
}

/// Walk the selected chain back from the sink over at most `window` blocks.
fn lane_mix_v1(consensus: &dyn kaspa_consensus_core::api::ConsensusApi, window: usize) -> PalwLaneMixV1 {
    let mut mix = PalwLaneMixV1::default();
    let mut cursor = consensus.get_sink();
    for _ in 0..window {
        let Ok(header) = consensus.get_header(cursor) else { break };
        if mix.window_blocks == 0 {
            mix.to_daa = header.daa_score;
        }
        mix.from_daa = header.daa_score;
        mix.count(header.pow_algo_id, header.daa_score);
        let Ok(ghostdag) = consensus.get_ghostdag_data(cursor) else { break };
        if ghostdag.selected_parent == cursor {
            break;
        }
        cursor = ghostdag.selected_parent;
    }
    mix
}

pub struct PalwLaneWatch {
    consensus_manager: Arc<ConsensusManager>,
    flow_context: Arc<FlowContext>,
    shutdown: kaspa_utils::triggers::SingleTrigger,
}

impl PalwLaneWatch {
    pub fn new(consensus_manager: Arc<ConsensusManager>, flow_context: Arc<FlowContext>) -> Self {
        Self { consensus_manager, flow_context, shutdown: kaspa_utils::triggers::SingleTrigger::default() }
    }

    async fn worker(self: &Arc<Self>) {
        info!(
            "[{PALW_LANE_WATCH}] counting the selected chain's last {PALW_LANE_WATCH_WINDOW_BLOCKS} blocks by lane every {} s; \
             an ERROR while none of them carries PALW work",
            PALW_LANE_WATCH_PERIOD.as_secs()
        );
        let mut alarmed_at: Option<std::time::Instant> = None;
        loop {
            let session = self.consensus_manager.consensus().unguarded_session();
            if !session.async_is_consensus_in_transitional_ibd_state().await {
                let mix = session.spawn_blocking(|c| lane_mix_v1(c, PALW_LANE_WATCH_WINDOW_BLOCKS)).await;
                let alarm = mix.alarm();
                self.flow_context.update_palw_runtime(|r| {
                    r.lane_window_blocks = mix.window_blocks;
                    r.lane_work_blocks = mix.work_blocks();
                    r.lane_heartbeat_blocks = mix.heartbeat_blocks;
                    r.lane_last_work_daa = mix.last_work_daa.unwrap_or(0);
                    r.lane_mix = mix.summary();
                    r.lane_alarm = alarm.clone().unwrap_or_default();
                });
                match (&alarm, alarmed_at) {
                    (Some(why), None) => {
                        error!("[{PALW_LANE_WATCH}] {why}");
                        alarmed_at = Some(std::time::Instant::now());
                    }
                    (Some(why), Some(at)) if at.elapsed() >= PALW_LANE_WATCH_REPEAT => {
                        error!("[{PALW_LANE_WATCH}] still: {why}");
                        alarmed_at = Some(std::time::Instant::now());
                    }
                    (None, Some(_)) => {
                        info!("[{PALW_LANE_WATCH}] PALW work is back on the chain: {}", mix.summary());
                        alarmed_at = None;
                    }
                    _ => {}
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(PALW_LANE_WATCH_PERIOD) => {}
                _ = self.shutdown.listener.clone() => break,
            }
        }
    }
}

impl AsyncService for PalwLaneWatch {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_LANE_WATCH
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_LANE_WATCH);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_LANE_WATCH);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mix_of(lanes: &[(u8, u64)]) -> PalwLaneMixV1 {
        let mut mix = PalwLaneMixV1::default();
        for &(algo, daa) in lanes {
            if mix.window_blocks == 0 {
                mix.to_daa = daa;
            }
            mix.from_daa = daa;
            mix.count(algo, daa);
        }
        mix
    }

    /// **The live testnet-12 of 2026-09-23, in miniature**: a chain of heartbeats alarms once it is old
    /// enough to judge, whatever its block count — the signal that "blocks are arriving" missed.
    #[test]
    fn a_heartbeat_only_window_alarms_and_a_young_one_does_not() {
        let heartbeats: Vec<(u8, u64)> = (0..600u64).rev().map(|d| (POW_ALGO_ID_HEARTBEAT_V1, d)).collect();
        let mix = mix_of(&heartbeats);
        assert_eq!((mix.window_blocks, mix.work_blocks(), mix.heartbeat_blocks), (600, 0, 600));
        let alarm = mix.alarm().expect("600 heartbeats and no work is the alarm");
        assert!(alarm.contains("no PALW work block") && alarm.contains("600"), "{alarm}");
        let young = mix_of(&heartbeats[..10]);
        assert_eq!(young.alarm(), None, "ten blocks are too few to judge a chain by");
    }

    /// One block of any PALW lane clears it, and the newest one's DAA is the one reported.
    #[test]
    fn any_palw_work_block_clears_the_alarm() {
        for lane in [POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3, POW_ALGO_ID_PALW_EXEC_V3, POW_ALGO_ID_PALW_ROUND_V1] {
            let mut blocks: Vec<(u8, u64)> = (0..100u64).rev().map(|d| (POW_ALGO_ID_HEARTBEAT_V1, d)).collect();
            blocks[40] = (lane, 59);
            blocks[70] = (lane, 29);
            let mix = mix_of(&blocks);
            assert_eq!(mix.work_blocks(), 2, "lane {lane}");
            assert_eq!(mix.last_work_daa, Some(59), "the newest work block, lane {lane}");
            assert_eq!(mix.alarm(), None, "lane {lane} clears the alarm");
        }
        let genesis_only = mix_of(&[(1, 0)]);
        assert_eq!((genesis_only.other_blocks, genesis_only.work_blocks()), (1, 0), "the genesis is not work");
    }
}
