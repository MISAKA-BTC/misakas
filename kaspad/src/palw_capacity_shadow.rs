//! **ADR-0160 §7.5 — the capacity shadow, on the node: every 10 DAA, what the capacity formulas
//! would say about this chain.**
//!
//! Stage 0 of ADR-0160's ramp (§9) needs the live chain measured under the formulas before any
//! capacity fence arms: would-be reservations and weights, claims per bond, seat capital, the
//! carriage queue, and the attribution counters the ramp gate G2 reads. This service computes
//! [`kaspa_consensus_core::palw_capacity_shadow_v1::palw_capacity_shadow_with_v1`] on the committed
//! tip (through the consensus read `palw_capacity_shadow_v1`) whenever the tip has moved
//! [`PALW_CAPACITY_SHADOW_INTERVAL_DAA_V1`] DAA since the last one, caches the answer, and logs ONE
//! compact line (`capacity-shadow: daa=… immature today/new=… N13k[ρ]=… seatcap[ρ]=…`), with a WARN
//! while some class's measured `q` is below twice what a step needs (the A8 alarm).
//!
//! **Node-only.** It reads; it never builds a block, a template or a transaction, and no consensus
//! rule reads what it computes. It runs on every ConsensusV2 node and judges only once the node is
//! nearly synced — a node catching up would be describing history. `getPalwCapacityShadow` (op 201)
//! serves the same computation on demand.

use std::sync::Arc;
use std::time::Duration;

use kaspa_consensus_core::palw_capacity_shadow_v1::{
    PALW_CAPACITY_SHADOW_INTERVAL_DAA_V1, PalwCapacityShadowOptionsV1, PalwCapacityShadowV1,
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    info,
    task::service::{AsyncService, AsyncServiceFuture},
    trace, warn,
};
use kaspa_p2p_flows::flow_context::FlowContext;
use std::sync::Mutex;

const PALW_CAPACITY_SHADOW: &str = "palw-capacity-shadow";
/// How often the tip's DAA is looked at (the shadow itself is recomputed per 10 DAA of it).
const PALW_CAPACITY_SHADOW_POLL: Duration = Duration::from_secs(30);

/// **Should the shadow be recomputed at `tip_daa`?** The first time, and then whenever the tip has
/// moved at least `interval` DAA — forward, or backward past it (a reorg to a shorter chain).
pub(crate) fn palw_capacity_shadow_due(last: Option<u64>, tip_daa: u64, interval: u64) -> bool {
    match last {
        None => true,
        Some(last) => tip_daa.abs_diff(last) >= interval.max(1),
    }
}

pub struct PalwCapacityShadowService {
    consensus_manager: Arc<ConsensusManager>,
    flow_context: Arc<FlowContext>,
    block_mass_limit: u64,
    latest: Mutex<Option<Arc<PalwCapacityShadowV1>>>,
    shutdown: kaspa_utils::triggers::SingleTrigger,
}

impl PalwCapacityShadowService {
    pub fn new(consensus_manager: Arc<ConsensusManager>, flow_context: Arc<FlowContext>, block_mass_limit: u64) -> Self {
        Self {
            consensus_manager,
            flow_context,
            block_mass_limit,
            latest: Mutex::new(None),
            shutdown: kaspa_utils::triggers::SingleTrigger::default(),
        }
    }

    /// The last shadow computed (`None` before the first).
    pub fn latest(&self) -> Option<Arc<PalwCapacityShadowV1>> {
        self.latest.lock().ok().and_then(|latest| latest.clone())
    }

    async fn worker(self: &Arc<Self>) {
        info!(
            "[{PALW_CAPACITY_SHADOW}] ADR-0160 shadow accounting: every {PALW_CAPACITY_SHADOW_INTERVAL_DAA_V1} DAA of the tip, \
             what the capacity formulas would reserve, weigh and allow (node-only; no verdict reads it)"
        );
        let mut last_daa: Option<u64> = None;
        loop {
            let session = self.consensus_manager.consensus().unguarded_session();
            if !session.async_is_consensus_in_transitional_ibd_state().await && self.flow_context.is_nearly_synced(&session).await {
                let tip_daa = session.async_get_sink_daa_score_timestamp().await.daa_score;
                if palw_capacity_shadow_due(last_daa, tip_daa, PALW_CAPACITY_SHADOW_INTERVAL_DAA_V1) {
                    let options = PalwCapacityShadowOptionsV1 { block_mass_limit: self.block_mass_limit, ..Default::default() };
                    if let Some(shadow) = session.spawn_blocking(move |c| c.palw_capacity_shadow_v1(options)).await {
                        info!("[{PALW_CAPACITY_SHADOW}] {}", shadow.summary());
                        if shadow.steps.iter().any(|s| s.q_alarm) {
                            warn!(
                                "[{PALW_CAPACITY_SHADOW}] A8: a class's measured attribution rate is below twice what a ramp step needs \
                                 — no ρ step may be armed on this measurement"
                            );
                        }
                        if let Ok(mut latest) = self.latest.lock() {
                            *latest = Some(Arc::new(shadow));
                        }
                    }
                    last_daa = Some(tip_daa);
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(PALW_CAPACITY_SHADOW_POLL) => {}
                _ = self.shutdown.listener.clone() => break,
            }
        }
    }
}

impl AsyncService for PalwCapacityShadowService {
    fn ident(self: Arc<Self>) -> &'static str {
        PALW_CAPACITY_SHADOW
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", PALW_CAPACITY_SHADOW);
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", PALW_CAPACITY_SHADOW);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every 10 DAA of the tip, the first time at once, and a reorg that moves the tip back by as
    /// much counts too.
    #[test]
    fn the_shadow_is_due_every_ten_daa_of_the_tip() {
        assert!(palw_capacity_shadow_due(None, 0, 10), "the first look computes");
        assert!(!palw_capacity_shadow_due(Some(100), 109, 10));
        assert!(palw_capacity_shadow_due(Some(100), 110, 10));
        assert!(palw_capacity_shadow_due(Some(100), 90, 10), "a reorg back by ten");
        assert!(!palw_capacity_shadow_due(Some(100), 100, 0), "an interval of 0 is read as 1");
        assert!(palw_capacity_shadow_due(Some(100), 101, 0));
    }

    /// The log line is the shadow's own summary: one line, the ADR's fields.
    #[test]
    fn the_logged_line_is_one_compact_line() {
        use kaspa_consensus_core::palw_capacity_formulas_v1::PALW_CAPACITY_REFERENCE_STEPS_V1;
        use kaspa_consensus_core::palw_capacity_shadow_v1::palw_capacity_shadow_v1;
        use kaspa_consensus_core::palw_state_v2::{PalwChainStateV2, PalwStateParamsV2};
        let params =
            PalwStateParamsV2::new(100, 600, 600, 120, 3_000, 1_000, kaspa_hashes::Hash64::from_u64_word(1), 4, 1_000, 1, 800, 600)
                .unwrap();
        let shadow = palw_capacity_shadow_v1(&PalwChainStateV2::genesis(), &params, 7, &PALW_CAPACITY_REFERENCE_STEPS_V1);
        let line = shadow.summary();
        assert!(!line.contains('\n'), "{line}");
        for field in ["capacity-shadow: daa=7", "immature today/new=", "bonds=0", "N13k[ρ]=", "seatcap[ρ]=", "queue=0"] {
            assert!(line.contains(field), "{field} in {line}");
        }
    }
}
