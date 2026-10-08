//! **ADR-0175: rule E, the virtual processor's half** (`Params::palw_fork_choice_rule_e_v1`, dormant).
//!
//! The comparator itself is pure and lives in [`kaspa_consensus_core::palw_fork_choice_rule_e_v1`]. This file supplies what only a
//! node holding both tips can: each tip's PALW state, the two tips' common selected-chain ancestor `F` (chain reachability) and its
//! state — whose blue score bounds each side's exclusive claims and whose registry is the common past's — the fork span that says
//! whether participation counts, and the two places the sink search asks:
//!
//! * the deep-reorg gate ([`VirtualStateProcessor::palw_rule_e_gate_v1`], asked by `dns_reorg_outcome` for a non-extension);
//! * the search's continuation past its first acceptable candidate ([`VirtualStateProcessor::palw_rule_e_finish_search_v1`]), which
//!   ranks the lighter candidates by their header-level participation, UTXO-validates and weighs at most
//!   [`PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1`] of them, and keeps the best — so a lighter branch that rule E ranks first is weighed,
//!   not skipped because GHOSTDAG popped a heavier one first, and a branch no registered bond attempted on costs no validation.
//!
//! Every read is this node's own committed stores; the peer that sent a branch supplies none of it.

use super::VirtualStateProcessor;
use crate::model::{
    services::reachability::ReachabilityService,
    stores::{depth::DepthStoreReader, ghostdag::GhostdagStoreReader, headers::HeaderStoreReader, virtual_state::VirtualStores},
};
use crate::processes::ghostdag::ordering::SortableBlock;
use kaspa_consensus_core::{
    BlockHash,
    blockhash::ORIGIN,
    config::params::ForkActivation,
    dns_finality::ActiveBondView,
    palw_attempt_v2::PalwAttemptEnvelopeV2,
    palw_fork_authority_v2::{PALW_REORG_SHALLOW_TIE_DAA_V1, PalwDeepReorgV2},
    palw_fork_choice_rule_e_v1::{
        PALW_RULE_E_FORK_WALK_V1, PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1, PALW_RULE_E_MAX_SCORED_V1, PALW_RULE_E_SCORE_WALK_V1,
        PalwRuleEPairV1, palw_rule_e_decide_v1, palw_rule_e_order_v1, palw_rule_e_participation_counts_v1,
        palw_rule_e_sides_above_fork_v1,
    },
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2},
    pow_layer0::is_palw_attempt_algo_id,
    utxo::utxo_diff::UtxoDiff,
};
use kaspa_core::{debug, info};
use kaspa_utils::binary_heap::BinaryHeapExtensions;
use std::{
    collections::{BTreeSet, BinaryHeap, HashMap, VecDeque},
    sync::Arc,
};

/// The PALW states one search has materialized, by block — a candidate is compared with several others, and materializing a state
/// (a delta walk from the stored tip) is the cost.
#[derive(Default)]
pub(crate) struct PalwRuleEStatesV1(HashMap<BlockHash, Option<Arc<PalwChainStateV2>>>);

/// The fork of two tips, as rule E reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PalwRuleEForkV1 {
    /// Their common selected-chain ancestor `F`.
    pub(crate) fork: BlockHash,
    pub(crate) fork_blue_score: u64,
    /// The lower tip's DAA above `F` (participation counts at `W_p`).
    pub(crate) lower_span: u64,
}

/// What one search's continuation did (for the log line and the cost tests).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PalwRuleESearchV1 {
    /// Heap entries the continuation scored by header-level participation.
    pub(crate) scored: usize,
    /// Of those, the ones no registered bond attempted on above the fork (passed over without a validation).
    pub(crate) unbonded: usize,
    /// Candidates passed over as slot races of the first.
    pub(crate) slot_races: usize,
    /// Candidates the continuation UTXO-validated.
    pub(crate) validated: usize,
    /// Of those, the ones the gate admitted.
    pub(crate) admitted: usize,
}

impl VirtualStateProcessor {
    /// Whether rule E (ADR-0175) is in force at `daa_score` — the incumbent's DAA at every reader.
    pub(crate) fn palw_rule_e_active_at(&self, daa_score: u64) -> bool {
        self.palw_fork_choice_rule_e.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa_score))
    }

    /// **Does `from` stand at most `window` DAA above its common chain ancestor with `other`?** Walks `from`'s selected chain down
    /// while the DAA stays within `window` of `from`'s; `Some(true)` at the first block that is a chain ancestor of `other`,
    /// `Some(false)` once the DAA leaves the window. `None` on a read this node cannot make or a walk past
    /// [`PALW_RULE_E_FORK_WALK_V1`] blocks — each caller turns that into its own conservative answer.
    fn palw_rule_e_span_at_most_v1(&self, from: BlockHash, other: BlockHash, window: u64) -> Option<bool> {
        let floor = self.headers_store.get_daa_score(from).ok()?.saturating_sub(window);
        let mut block = from;
        for _ in 0..PALW_RULE_E_FORK_WALK_V1 {
            if self.headers_store.get_daa_score(block).ok()? < floor {
                return Some(false);
            }
            if self.reachability_service.try_is_chain_ancestor_of(block, other).ok()? {
                return Some(true);
            }
            match self.ghostdag_store.get_selected_parent(block) {
                Ok(parent) if parent != block && parent != ORIGIN => block = parent,
                _ => return None,
            }
        }
        None
    }

    /// The two tips ordered by `(DAA, hash)`: `(lower, higher)`. `None` on a missing header.
    fn palw_rule_e_by_daa_v1(&self, a: BlockHash, b: BlockHash) -> Option<(BlockHash, BlockHash)> {
        let (da, db) = (self.headers_store.get_daa_score(a).ok()?, self.headers_store.get_daa_score(b).ok()?);
        Some(if (da, a) <= (db, b) { (a, b) } else { (b, a) })
    }

    /// **The fork of `a` and `b`**: their common selected-chain ancestor `F` (chain reachability, a bounded walk — the binary search
    /// where `b` is on this node's selected chain), its blue score, and the lower tip's DAA span above it. `None` past the
    /// walk's bound (below any finality point a candidate can stand on) or on a read this node cannot make. Participation counts
    /// when the lower span reaches `W_p` — symmetric in the pair, so the gate and the search ask one question of it; facing a branch
    /// at least as long as its own, the incumbent's span is the lower one (this node's own history since the fork).
    pub(crate) fn palw_rule_e_fork_v1(&self, a: BlockHash, b: BlockHash) -> Option<PalwRuleEForkV1> {
        let fork = self.chain_common_ancestor_within(a, b, PALW_RULE_E_FORK_WALK_V1 as u64)?;
        let fork_daa = self.headers_store.get_daa_score(fork).ok()?;
        let (da, db) = (self.headers_store.get_daa_score(a).ok()?, self.headers_store.get_daa_score(b).ok()?);
        Some(PalwRuleEForkV1 {
            fork,
            fork_blue_score: self.ghostdag_store.get_blue_score(fork).ok()?,
            lower_span: da.min(db).saturating_sub(fork_daa),
        })
    }

    /// **Whether `a` and `b` are a slot race**: both within strict-win's shallow window of their common chain ancestor. The search's
    /// continuation leaves such a pair to GHOSTDAG's order, as strict-win's shallow tie does; anything it cannot read is not a race.
    pub(crate) fn palw_rule_e_slot_race_v1(&self, a: BlockHash, b: BlockHash) -> bool {
        let Some((lower, higher)) = self.palw_rule_e_by_daa_v1(a, b) else { return false };
        matches!(self.palw_rule_e_span_at_most_v1(higher, lower, PALW_REORG_SHALLOW_TIE_DAA_V1), Some(true))
    }

    fn palw_rule_e_state_v1(&self, states: &mut PalwRuleEStatesV1, block: BlockHash) -> Option<Arc<PalwChainStateV2>> {
        states.0.entry(block).or_insert_with(|| self.palw_candidate_state_v2(block).map(Arc::new)).clone()
    }

    /// **Rule E's two sides for tips `a` and `b`, and whether participation counts**: each tip's claims accepted above their fork
    /// `F` that the other tip's state does not hold, weighed by the fold's own expressions at that tip; participation over `F`'s
    /// registry; the even split over `F`'s bond count. Each side priced at its own tip, the weight fence read at that tip's DAA.
    /// `None` where either tip or `F` cannot be weighed or a side holds more than the bound allows (the callers fail closed).
    pub(crate) fn palw_rule_e_pair_v1(
        &self,
        states: &mut PalwRuleEStatesV1,
        a: BlockHash,
        b: BlockHash,
    ) -> Option<(PalwRuleEPairV1, bool)> {
        let params = self.palw_state_params_v2.as_ref()?;
        let fork = self.palw_rule_e_fork_v1(a, b)?;
        let (state_a, state_b) = (self.palw_rule_e_state_v1(states, a)?, self.palw_rule_e_state_v1(states, b)?);
        let state_fork = self.palw_rule_e_state_v1(states, fork.fork)?;
        let weightless = |block: BlockHash| {
            self.headers_store.get_daa_score(block).ok().map(|daa| self.palw_uncertified_weightless.is_some_and(|f| f.is_active(daa)))
        };
        let pair = palw_rule_e_sides_above_fork_v1(
            &state_a,
            &state_b,
            &state_fork,
            fork.fork_blue_score,
            params,
            weightless(a)?,
            weightless(b)?,
            self.palw_canonical_work_daa,
        )
        .ok()?;
        Some((pair, palw_rule_e_participation_counts_v1(fork.lower_span)))
    }

    /// **Rule E's deep-reorg gate** (ADR-0175): may `candidate` replace the incumbent `prev_sink`? Unweighable refuses.
    pub(crate) fn palw_rule_e_gate_v1(&self, candidate: BlockHash, prev_sink: BlockHash, incumbent_daa: u64) -> PalwDeepReorgV2 {
        let mut states = PalwRuleEStatesV1::default();
        let Some((pair, counts)) = self.palw_rule_e_pair_v1(&mut states, candidate, prev_sink) else {
            info!(
                "deep reorg refused (rule E): candidate {candidate} or the incumbent {prev_sink} cannot be weighed over their exclusive pasts"
            );
            return PalwDeepReorgV2::Refuse;
        };
        let heavier = || match (self.ghostdag_store.get_blue_work(candidate), self.ghostdag_store.get_blue_work(prev_sink)) {
            (Ok(c), Ok(p)) => SortableBlock::new(candidate, c) > SortableBlock::new(prev_sink, p),
            _ => false,
        };
        let decision = palw_rule_e_decide_v1(&pair.b, &pair.a, counts, pair.even_split_min, heavier, || {
            self.palw_reorg_shallow_ghostdag_win_v1(candidate, prev_sink, incumbent_daa)
        });
        debug!(
            "rule E gate: candidate {candidate} {:?} against the incumbent {prev_sink} {:?} (participation counts: {counts}, even split at {}) — {decision:?}",
            pair.a, pair.b, pair.even_split_min
        );
        decision
    }

    /// **Rule E's choice between two admitted candidates**: `true` iff `challenger` replaces `best` — a strict win in rule E's
    /// order over their exclusive pasts, or, on a tie where participation counts and both reach the even split, the heavier in
    /// GHOSTDAG's order (the gate's own tie rule; any other tie keeps `best`). Unweighable never replaces.
    pub(crate) fn palw_rule_e_prefers_v1(&self, states: &mut PalwRuleEStatesV1, challenger: BlockHash, best: BlockHash) -> bool {
        use std::cmp::Ordering;
        let Some((pair, counts)) = self.palw_rule_e_pair_v1(states, challenger, best) else { return false };
        match palw_rule_e_order_v1(&pair.a, &pair.b, counts) {
            Ordering::Greater => true,
            Ordering::Less => false,
            Ordering::Equal => {
                counts
                    && pair.a.participation >= pair.even_split_min
                    && pair.b.participation >= pair.even_split_min
                    && match (self.ghostdag_store.get_blue_work(challenger), self.ghostdag_store.get_blue_work(best)) {
                        (Ok(c), Ok(b)) => SortableBlock::new(challenger, c) > SortableBlock::new(best, b),
                        _ => false,
                    }
            }
        }
    }

    /// **Header-level participation of `candidate` against `first`**: the distinct executor bonds, registered in `registry`
    /// (`first`'s state), of the attempt headers among the blocks `candidate`'s selected chain accepts above its common chain
    /// ancestor with `first` that are not in `first`'s past. An upper bound on what rule E would count for `candidate` (it adds
    /// losing draws, and asks one registry rather than two), read from headers and reachability alone — no UTXO validation, no
    /// state walk — and capped at [`PALW_RULE_E_SCORE_WALK_V1`] blocks (past the cap, what was counted so far).
    pub(crate) fn palw_rule_e_header_participation_v1(
        &self,
        candidate: BlockHash,
        first: BlockHash,
        registry: &PalwChainStateV2,
    ) -> usize {
        let mut bonds: BTreeSet<PalwBondKeyV2> = BTreeSet::new();
        let mut reads = 0usize;
        let mut block = candidate;
        loop {
            if !matches!(self.reachability_service.try_is_chain_ancestor_of(block, first), Ok(false)) {
                break;
            }
            let Ok(data) = self.ghostdag_store.get_data(block) else { break };
            for accepted in std::iter::once(block).chain(data.unordered_mergeset_without_selected_parent()) {
                reads += 1;
                if reads > PALW_RULE_E_SCORE_WALK_V1 {
                    return bonds.len();
                }
                if !matches!(self.reachability_service.try_is_dag_ancestor_of(accepted, first), Ok(false)) {
                    continue;
                }
                let Ok(header) = self.headers_store.get_header(accepted) else { continue };
                if !is_palw_attempt_algo_id(header.pow_algo_id) {
                    continue;
                }
                let Ok(envelope) = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment) else { continue };
                let bond = PalwBondKeyV2(envelope.attempt.executor_bond);
                if registry.bond(&bond).is_some() {
                    bonds.insert(bond);
                }
            }
            if data.selected_parent == block || data.selected_parent == ORIGIN {
                break;
            }
            block = data.selected_parent;
        }
        bonds.len()
    }

    /// **The sink search past its first acceptable candidate, under rule E.** `first` is the candidate the status quo returns —
    /// GHOSTDAG's heaviest that is UTXO-valid and that `dns_reorg_outcome` admits — and `heap` what is left of the search, with
    /// `diff` standing at `diff_point`.
    ///
    /// 1. **Rank, cheaply.** Up to [`PALW_RULE_E_MAX_SCORED_V1`] heap entries, in the heap's order, are scored by their header-level
    ///    participation against `first` ([`Self::palw_rule_e_header_participation_v1`]); an entry in `first`'s past, a slot race of
    ///    `first` (GHOSTDAG's, as strict-win's shallow tie) and an entry below the finality point are passed over, and so is an
    ///    entry no registered bond attempted on above the fork — it cannot rank first on participation, and the search does not
    ///    UTXO-validate a branch to look for economic keys alone. So a heartbeat-only branch, however heavy and however many, costs
    ///    header reads and no validation.
    /// 2. **Weigh, boundedly.** The scored entries, highest score first (the heap's order among equals), are UTXO-validated and
    ///    put to the gate — at most [`PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1`] of them — and each one the gate admits replaces the
    ///    best so far on a strict win in rule E's order. A junk branch takes a validation from an honest one only by carrying at
    ///    least as many registered bonds' attempts — rule E's own trust line.
    ///
    /// Where `first` stays best the answer is the status quo's, byte for byte (the same sink and the same parent candidates). Where
    /// a lighter candidate wins it becomes the sink and the virtual merges only what is lighter than it — as the DNS stake
    /// preference does, so GHOSTDAG's selected parent of the virtual is the chosen sink.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn palw_rule_e_finish_search_v1(
        &self,
        stores: &VirtualStores,
        diff: &mut UtxoDiff,
        bond_view: &mut ActiveBondView,
        prev_sink: BlockHash,
        first: BlockHash,
        mut heap: BinaryHeap<SortableBlock>,
        mut diff_point: BlockHash,
        finality_point: BlockHash,
    ) -> (BlockHash, VecDeque<BlockHash>, PalwRuleESearchV1) {
        let blue_work = |h: BlockHash| self.ghostdag_store.get_blue_work(h).unwrap();
        let snapshot: Vec<SortableBlock> = heap.clone().into_sorted_iter().collect();
        let mut states = PalwRuleEStatesV1::default();
        let mut report = PalwRuleESearchV1::default();
        let mut best = first;
        if let Some(registry) = self.palw_rule_e_state_v1(&mut states, first) {
            let mut ranked: Vec<(usize, usize, BlockHash)> = Vec::new();
            while report.scored < PALW_RULE_E_MAX_SCORED_V1 {
                let Some(entry) = heap.pop() else { break };
                let candidate = entry.hash;
                if !matches!(self.reachability_service.try_is_dag_ancestor_of(candidate, first), Ok(false))
                    || !matches!(self.reachability_service.try_is_chain_ancestor_of(finality_point, candidate), Ok(true))
                {
                    continue;
                }
                if self.palw_rule_e_slot_race_v1(candidate, first) {
                    report.slot_races += 1;
                    continue;
                }
                report.scored += 1;
                let score = self.palw_rule_e_header_participation_v1(candidate, first, &registry);
                if score == 0 {
                    report.unbonded += 1;
                    continue;
                }
                ranked.push((score, report.scored, candidate));
            }
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let mut accepted: Vec<BlockHash> = vec![first];
            for (_, _, candidate) in ranked.into_iter().take(PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1) {
                if accepted.iter().any(|a| matches!(self.reachability_service.try_is_dag_ancestor_of(candidate, *a), Ok(true))) {
                    continue;
                }
                report.validated += 1;
                diff_point = self.calculate_utxo_state_relatively(stores, diff, bond_view, diff_point, candidate);
                if diff_point == candidate && self.dns_reorg_outcome(candidate, prev_sink, bond_view).is_accept() {
                    report.admitted += 1;
                    accepted.push(candidate);
                    if self.palw_rule_e_prefers_v1(&mut states, candidate, best) {
                        best = candidate;
                    }
                }
            }
        }
        self.palw_rule_e_max_extra_validated.fetch_max(report.validated, std::sync::atomic::Ordering::Relaxed);
        self.palw_rule_e_searches.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // `diff` back at the chosen sink: it was UTXO-validated above (or is `first`), so the walk ends there.
        let restored = self.calculate_utxo_state_relatively(stores, diff, bond_view, diff_point, best);
        assert_eq!(restored, best, "rule E's chosen sink {best} was UTXO-valid a moment ago");
        let root_blue_work = self.ghostdag_store.get_blue_work(self.depth_store.merge_depth_root(best).unwrap()).unwrap_or_default();
        if best == first {
            // The status quo's answer, exactly: the heap as it stood at `first`.
            return (first, snapshot.into_iter().take_while(|s| s.blue_work >= root_blue_work).map(|s| s.hash).collect(), report);
        }
        info!(
            "rule E: sink {best} (blue work {}) chosen over the GHOSTDAG-heavier candidate {first} (blue work {}) — it ranks first over \
             the two exclusive pasts; the heavier branch is not merged ({report:?})",
            blue_work(best),
            blue_work(first),
        );
        let best_key = SortableBlock::new(best, blue_work(best));
        let parents = snapshot
            .into_iter()
            .filter(|s| {
                *s < best_key
                    && s.blue_work >= root_blue_work
                    && !self.reachability_service.is_dag_ancestor_of(s.hash, best)
                    && !self.reachability_service.is_dag_ancestor_of(best, s.hash)
            })
            .map(|s| s.hash)
            .collect();
        (best, parents, report)
    }
}
