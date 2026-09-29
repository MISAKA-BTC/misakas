//! **RFC-0004's epoch watcher, the node's half (work item A10).**
//!
//! Past `palw_improvement_v1` a node follows every governed line's epoch and serves it: it fetches the
//! candidates it can hold (a composite candidate's adapter section over a parent it holds) and runs the
//! evaluation jobs it can (the subject classes it holds), each derived as the chain derives it. The
//! plan is the SDK's (`misaka_palw_sdk::improve::palw_improve_duties_v1`, pure, over the chain view
//! `PalwImproveChainV1`); this module keeps what the panel's loop remembers between ticks — the state
//! it last saw each line's epoch in, for the log — and what the node holds.
//!
//! **Dormant**: nothing here runs below the fence ([`palw_improve_watch_armed_v1`]), and the chain
//! view is read through the one door the core lane exposes for it
//! (`ConsensusApi::palw_improvement_open_epochs_v1`, read into the SDK's `PalwImproveViewsChainV1` —
//! this module never names a row's field).
//!
//! **On the panel's loop**: every [`PALW_IMPROVE_READ_EVERY_V1`] the loop reads the door and ticks the
//! watcher ([`PalwImproveWatchV1::tick_views`]); it logs each epoch's state change and the plan. The
//! plan is not run yet — the prefetch transport (the candidates lane's carriage) and the evaluation
//! claims (the evaluation lane's job type) are the next steps — so the loop plans prefetch only.

use std::collections::{BTreeMap, BTreeSet};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_improve_state_v1::{PalwEpochStateV1, PalwEvalItemV1, PalwImprovementEpochViewV1};
use kaspa_core::{info, trace};
use misaka_palw_sdk::improve::{
    PalwImproveChainV1, PalwImproveDutyV1, PalwImproveNodeV1, PalwImproveViewsChainV1, palw_improve_duties_v1,
    palw_improve_epoch_moved_v1,
};

/// The most evaluation jobs one tick plans (each is a whole run of a subject class).
pub(crate) const PALW_IMPROVE_JOBS_PER_TICK_V1: usize = 4;

/// How often the panel's loop reads the improvement door (epochs move in tens of DAA).
pub(crate) const PALW_IMPROVE_READ_EVERY_V1: std::time::Duration = std::time::Duration::from_secs(10);

/// **Is the watcher armed at `daa_score`?** Only past `palw_improvement_v1` — `None` on every shipped
/// preset, so on every network today the watcher does nothing.
pub(crate) fn palw_improve_watch_armed_v1(params: &Params, daa_score: u64) -> bool {
    params.palw_improvement_v1_active_at(daa_score)
}

/// **The IR classes this node holds** — every class its loaded IR artifacts register (the node's
/// `--palw-class-artifact` holdings): a line's head it can evaluate, the parent a composite candidate
/// is fetched over.
pub(crate) fn palw_improve_held_classes_v1(holdings: &[misaka_palw_sdk::PalwLoadedArtifactV1]) -> BTreeSet<Hash64> {
    misaka_palw_sdk::tir_registration::tir_entries_of_v1(holdings).iter().map(|entry| entry.class_id()).collect()
}

/// **What the watcher remembers between ticks.**
#[derive(Debug, Default)]
pub(crate) struct PalwImproveWatchV1 {
    /// Each governed line's open epoch and the state it was last seen in.
    seen: BTreeMap<Hash64, (u64, PalwEpochStateV1)>,
}

/// One tick's result: what moved (for the log) and what to do.
#[derive(Debug, Default)]
pub(crate) struct PalwImproveTickV1 {
    /// `(line, epoch, the state it entered)` for every line whose epoch moved since the last tick.
    pub moved: Vec<(Hash64, u64, PalwEpochStateV1)>,
    /// Lines that left the governed set or whose epoch closed since the last tick.
    pub gone: Vec<Hash64>,
    pub duties: Vec<PalwImproveDutyV1>,
}

impl PalwImproveWatchV1 {
    /// **One tick**: every governed line's epoch read through `chain`, what moved since the last tick,
    /// and the plan for `node` at `current_daa`.
    pub(crate) fn tick(&mut self, chain: &dyn PalwImproveChainV1, node: &PalwImproveNodeV1, current_daa: u64) -> PalwImproveTickV1 {
        let mut tick = PalwImproveTickV1::default();
        let mut now: BTreeMap<Hash64, (u64, PalwEpochStateV1)> = BTreeMap::new();
        for line_id in chain.governed_lines() {
            let Some(line) = chain.line(&line_id) else { continue };
            let current = line.open_epoch.and_then(|e| chain.epoch(&line_id, e)).map(|epoch| (epoch.epoch, epoch.state));
            if let Some((epoch, state)) = palw_improve_epoch_moved_v1(self.seen.get(&line_id).copied(), current) {
                tick.moved.push((line_id, epoch, state));
            }
            if let Some(current) = current {
                now.insert(line_id, current);
            }
        }
        tick.gone = self.seen.keys().filter(|line| !now.contains_key(line)).copied().collect();
        self.seen = now;
        tick.duties = palw_improve_duties_v1(chain, node, current_daa, PALW_IMPROVE_JOBS_PER_TICK_V1);
        tick
    }

    /// **One tick over the node's read door** (`ConsensusApi::palw_improvement_open_epochs_v1`): the
    /// chain view its open epochs make. No case and no claimed job is read yet — the candidates' and
    /// the evaluation lanes' readers land beside the door — so an item's evaluation is planned only
    /// once its case can be read, and every job counts as open.
    pub(crate) fn tick_views(
        &mut self,
        views: &[PalwImprovementEpochViewV1],
        node: &PalwImproveNodeV1,
        current_daa: u64,
    ) -> PalwImproveTickV1 {
        let case = |_: &Hash64, _: u64, _: &PalwEvalItemV1| None;
        let claimed = |_: &Hash64| false;
        self.tick(&PalwImproveViewsChainV1 { views, case: &case, claimed: &claimed }, node, current_daa)
    }
}

/// **A tick, logged**: each epoch's state change once (with the plan's size then), each epoch that
/// closed or line that left governance; the plan itself at trace level. Nothing in the plan runs yet.
pub(crate) fn palw_improve_log_tick_v1(tick: &PalwImproveTickV1) {
    let prefetch = tick.duties.iter().filter(|d| matches!(d, PalwImproveDutyV1::Prefetch { .. })).count();
    let evaluate = tick.duties.len() - prefetch;
    for (line, epoch, state) in &tick.moved {
        info!(
            "[palw-improve] line {line}: epoch {epoch} is {state:?} — this node plans {prefetch} prefetch(es) and {evaluate} \
             evaluation(s), not run yet (RFC-0004 A10)"
        );
    }
    for line in &tick.gone {
        info!("[palw-improve] line {line}: its open epoch closed, or the line left governance (RFC-0004)");
    }
    for duty in &tick.duties {
        trace!("[palw-improve] planned: {duty:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1;
    use misaka_palw_sdk::improve::{PalwImproveMemChainV1, PalwImprovePrefetchV1};

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// **The watcher follows an epoch through its states and serves it**: each state change is logged
    /// once; in `Submission` it plans the composite candidate's adapter section; in `Evaluating` the
    /// tasks of the classes it holds, at most the tick's budget; the epoch's close is noticed; an empty
    /// read door plans nothing; the panel's loop ticks it over the door behind the fence; and below the
    /// fence the watcher is not armed on any shipped preset.
    #[test]
    fn the_watcher_follows_an_epoch_and_plans_its_prefetch_and_evaluation() {
        let mut chain: PalwImproveMemChainV1 = misaka_palw_sdk::improve::testing::chain(PalwEpochStateV1::Submission, false);
        let line = h(misaka_palw_sdk::improve::testing::LINE);
        let head = h(misaka_palw_sdk::improve::testing::HEAD);
        let mut node = PalwImproveNodeV1 { holds: [head].into(), evaluates: true, prefetch_full: false };
        let mut watch = PalwImproveWatchV1::default();
        let tick = watch.tick(&chain, &node, 160);
        assert_eq!(tick.moved, vec![(line, 3, PalwEpochStateV1::Submission)]);
        assert!(matches!(
            tick.duties.as_slice(),
            [PalwImproveDutyV1::Prefetch { plan: PalwImprovePrefetchV1::Adapter { p: 40, .. }, .. }]
        ));
        assert!(watch.tick(&chain, &node, 161).moved.is_empty(), "logged once");
        node.holds.insert(h(0xC1));
        chain.epochs.get_mut(&(line, 3)).unwrap().state = PalwEpochStateV1::Evaluating;
        let tick = watch.tick(&chain, &node, 250);
        assert_eq!(tick.moved, vec![(line, 3, PalwEpochStateV1::Evaluating)]);
        let evaluations = tick.duties.iter().filter(|d| matches!(d, PalwImproveDutyV1::Evaluate { .. })).count();
        assert_eq!(evaluations, PALW_IMPROVE_JOBS_PER_TICK_V1, "three items × two subjects, at most the tick's budget");
        chain.lines.get_mut(&line).unwrap().open_epoch = None;
        let tick = watch.tick(&chain, &node, 400);
        assert_eq!(tick.gone, vec![line], "the epoch closed");
        assert!(tick.duties.is_empty());
        // The read door with no open epoch: nothing moves, nothing is planned.
        let tick = watch.tick_views(&[], &node, 401);
        assert!(tick.moved.is_empty() && tick.gone.is_empty() && tick.duties.is_empty());
        // The panel's loop reads the door behind the fence and ticks the watcher over it.
        let panel = include_str!("palw_panel.rs");
        let at = panel
            .find("crate::palw_improve_watch::palw_improve_watch_armed_v1(&self.consensus_config.params, current_daa)")
            .expect("armed");
        let read = &panel[at..at + 1400];
        assert!(read.contains(".spawn_blocking(|c| c.palw_improvement_open_epochs_v1())"));
        assert!(read.contains("palw_improve_log_tick_v1(&improve_watch.tick_views(&views, &node, current_daa))"));
        let _ = PalwTirArtifactRefV1::Single { root: h(0) };
        assert!(palw_improve_held_classes_v1(&[]).is_empty(), "a node with no IR artifact holds no class");
        for net in [
            kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 12),
            kaspa_consensus_core::network::NetworkId::new(kaspa_consensus_core::network::NetworkType::Mainnet),
        ] {
            let params = Params::from(net);
            assert!(!palw_improve_watch_armed_v1(&params, u64::MAX / 2), "{net}: dormant");
        }
    }
}
