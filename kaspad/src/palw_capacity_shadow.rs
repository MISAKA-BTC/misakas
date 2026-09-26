//! **ADR-0160 §7.5 — the capacity shadow, on the node: every 10 DAA, what the capacity formulas
//! would say about this chain.**
//!
//! Stage 0 of ADR-0160's ramp (§9) needs the live chain measured under the formulas before any
//! capacity fence arms: would-be reservations and weights, claims per bond, seat capital, the
//! carriage queue, and the attribution counters the ramp gate G2 reads. This service computes
//! [`kaspa_consensus_core::palw_capacity_shadow_v1::palw_capacity_shadow_with_v1`] on the committed
//! tip (through the consensus read `palw_capacity_shadow_v1`) whenever the tip has moved
//! [`PALW_CAPACITY_SHADOW_INTERVAL_DAA_V1`] DAA since the last one, caches the answer, and logs ONE
//! compact line (`capacity-shadow: daa=… immature today/new=… N13k[ρ@q‰]=… seatcap[ρ@q‰]=…`), every
//! per-step figure labelled with the attribution rate it credits. The steps are the node's default
//! display: the schedule F-L arms, else the uncredited ramp (q 0, `m_c = E`) — never ADR-0160's
//! reference ramp (q 143‰), which is conditional on a measured rate and only priced when an RPC
//! caller names it.
//!
//! **The A8 alarm.** `q` is measured only on the bonds of an O-3 run the operator names with
//! `--palw-capacity-shadow-adversary` (repeatable): over their RESOLVED claims (caught vs `Final` or
//! voided with nobody convicted; in-flight claims are left out). While some class's measured `q` is
//! below twice what a step needs, the line ends `q-ALARM` and a WARN follows. A node that names no
//! bond measures nothing and never alarms; `getPalwCapacityShadow` measures on the bonds its caller
//! names.
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
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
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

/// **`--palw-capacity-shadow-adversary`'s bonds**, parsed (`<txid>:<index>`, as `--stake-bond`);
/// the first malformed one is an error naming it.
pub fn palw_capacity_shadow_adversary_bonds(named: &[String]) -> Result<Vec<PalwBondKeyV2>, String> {
    named.iter().map(|b| crate::palw_producer::parse_outpoint(b.trim()).map(PalwBondKeyV2)).collect()
}

/// The options of every interval's computation: the block mass and the O-3 run's bonds.
pub(crate) fn palw_capacity_shadow_options(block_mass_limit: u64, adversary_bonds: &[PalwBondKeyV2]) -> PalwCapacityShadowOptionsV1 {
    PalwCapacityShadowOptionsV1 { block_mass_limit, adversary_bonds: adversary_bonds.to_vec(), ..Default::default() }
}

pub struct PalwCapacityShadowService {
    consensus_manager: Arc<ConsensusManager>,
    flow_context: Arc<FlowContext>,
    block_mass_limit: u64,
    adversary_bonds: Vec<PalwBondKeyV2>,
    latest: Mutex<Option<Arc<PalwCapacityShadowV1>>>,
    shutdown: kaspa_utils::triggers::SingleTrigger,
}

impl PalwCapacityShadowService {
    pub fn new(
        consensus_manager: Arc<ConsensusManager>,
        flow_context: Arc<FlowContext>,
        block_mass_limit: u64,
        adversary_bonds: Vec<PalwBondKeyV2>,
    ) -> Self {
        Self {
            consensus_manager,
            flow_context,
            block_mass_limit,
            adversary_bonds,
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
             what the capacity formulas would reserve, weigh and allow (node-only; no verdict reads it); A8's q measured on \
             {} named O-3 bond(s)",
            self.adversary_bonds.len()
        );
        let mut last_daa: Option<u64> = None;
        loop {
            let session = self.consensus_manager.consensus().unguarded_session();
            if !session.async_is_consensus_in_transitional_ibd_state().await && self.flow_context.is_nearly_synced(&session).await {
                let tip_daa = session.async_get_sink_daa_score_timestamp().await.daa_score;
                if palw_capacity_shadow_due(last_daa, tip_daa, PALW_CAPACITY_SHADOW_INTERVAL_DAA_V1) {
                    let options = palw_capacity_shadow_options(self.block_mass_limit, &self.adversary_bonds);
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

    /// `--palw-capacity-shadow-adversary` parses `<txid>:<index>` and refuses anything else.
    #[test]
    fn the_adversary_flag_parses_outpoints() {
        let txid = "ab".repeat(64);
        let bonds = palw_capacity_shadow_adversary_bonds(&[format!("{txid}:3"), format!(" {txid}:0 ")]).unwrap();
        assert_eq!(bonds.iter().map(|b| b.0.index).collect::<Vec<_>>(), vec![3, 0]);
        assert!(palw_capacity_shadow_adversary_bonds(&[]).unwrap().is_empty());
        assert!(palw_capacity_shadow_adversary_bonds(&["nope".to_string()]).is_err());
        assert!(palw_capacity_shadow_adversary_bonds(&[format!("{txid}:x")]).is_err());
        let options = palw_capacity_shadow_options(500_000, &bonds);
        assert_eq!((options.block_mass_limit, options.adversary_bonds.len(), options.steps.len()), (500_000, 2, 0));
    }

    /// **The node's A8 alarm fires once an O-3 bond is named** (review of lane shadow, finding 2):
    /// the options the worker builds carry the named bonds, so a campaign whose claims resolve with
    /// nobody convicted (one Final, one timed out) reads q = 0 and the line ends `q-ALARM`; with no
    /// bond named nothing is measured and nothing alarms.
    #[test]
    fn the_node_alarms_on_a_named_bond_and_not_without_one() {
        use kaspa_consensus_core::palw_capacity_shadow_v1::palw_capacity_shadow_with_v1;
        use kaspa_consensus_core::palw_state_v2::{
            PalwBlockContextV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimRcoreV1, PalwClaimSourceV2, PalwClaimStateV2,
            PalwDeltaEntryV2, PalwStateDeltaV2, PalwStateParamsV2, PalwVoidReasonV2, apply_delta_v2,
        };
        use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
        use kaspa_hashes::Hash64;
        let floor = Hash64::from_u64_word(1);
        let params = PalwStateParamsV2::new(100, 600, 600, 120, 3_000, 1_000, floor, 4, 1_000, 1, 800, 600).unwrap();
        let bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0));
        let claim = |phase, accepted_daa| PalwClaimStateV2 {
            source: PalwClaimSourceV2::Attempt,
            class_id: floor,
            bond,
            pwu: 1,
            accepted_daa,
            rebound_daa: None,
            accepted_blue_score: accepted_daa,
            accepted_block: Hash64::from_u64_word(0xB0),
            trace_root: Hash64::from_u64_word(0x71),
            output_root: Hash64::from_u64_word(0x72),
            execution_root: Hash64::from_u64_word(0xE0),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            reserved: 0,
            immature_contribution: 0,
            // testnet-12's E (720‰ of the 4,445.62 MSK subsidy): the q a step needs is priced on it.
            escrowed_reward: 320_084_650_080,
            work_leaves: 0,
            work_id: None,
            phase,
            rights_reserved: 0,
            job_identity: Hash64::default(),
            rcore: PalwClaimRcoreV1::default(),
        };
        let point = PalwBlockContextV2 { block: Hash64::from_u64_word(0xB1), daa_score: 1_000, blue_score: 1_000, subsidy: 0 };
        let entries = vec![
            PalwDeltaEntryV2::Claim {
                key: Hash64::from_u64_word(0x60),
                old: None,
                new: Some(claim(PalwClaimPhaseV2::Final { final_daa: 990 }, 800)),
            },
            PalwDeltaEntryV2::Claim {
                key: Hash64::from_u64_word(0x61),
                old: None,
                new: Some(claim(PalwClaimPhaseV2::Voided { voided_daa: 995, reason: PalwVoidReasonV2::ReceiptTimeout }, 300)),
            },
            PalwDeltaEntryV2::LastPoint { old: None, new: Some(point) },
        ];
        let state = apply_delta_v2(&PalwChainStateV2::genesis(), &PalwStateDeltaV2 { point, entries }, &params).expect("folds");
        let named = palw_capacity_shadow_with_v1(&state, &params, 1_000, &palw_capacity_shadow_options(500_000, &[bond]));
        assert_eq!(named.attribution[0].q_measured_permille, Some(0));
        assert!(named.steps.iter().all(|s| s.q_alarm) && named.summary().ends_with("q-ALARM"), "{}", named.summary());
        let unnamed = palw_capacity_shadow_with_v1(&state, &params, 1_000, &palw_capacity_shadow_options(500_000, &[]));
        assert!(unnamed.steps.iter().all(|s| !s.q_alarm) && !unnamed.summary().contains("ALARM"));
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
        for field in ["capacity-shadow: daa=7", "immature today/new=", "bonds=0", "N13k[ρ@q‰]=10@143:", "seatcap[ρ@q‰]=", "queue=0"] {
            assert!(line.contains(field), "{field} in {line}");
        }
    }
}
