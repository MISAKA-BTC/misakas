use crate::Policy;
use kaspa_consensus_core::{
    block::TemplateTransactionSelector,
    subnets::SUBNETWORK_ID_STAKE_ATTESTATION_SHARD,
    tx::{Transaction, TransactionId},
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

/// kaspa-pq audit v24 (H-3/M-4): a shared per-block `StakeAttestationShard` budget
/// (tx count + mass) that EVERY selector path funnels through, so the cap holds
/// uniformly whether the frontier produced a [`TakeAllSelector`], a
/// [`SequenceSelector`], or a [`RebalancingWeightedTransactionSelector`]. Before
/// this, only the rebalancing selector enforced the tx-count cap and NO selector
/// enforced the mass cap, so a small/medium frontier (TakeAll/Sequence path) could
/// emit an unbounded number of shard txs and unbounded shard mass into a template.
///
/// `0` for a limit means "unlimited" (overlay off / unconfigured), which makes the
/// whole filter a no-op — non-overlay nets are byte-identical.
#[derive(Clone, Copy, Default)]
pub(crate) struct ShardCap {
    /// Max shard txs (`0` = unlimited).
    max_txs: u64,
    /// Max cumulative shard mass (`0` = unlimited).
    max_mass: u64,
    /// Shard txs already admitted across `admit` calls within one selection round.
    used_txs: u64,
    /// Shard mass already admitted across `admit` calls within one selection round.
    used_mass: u64,
}

impl ShardCap {
    pub fn from_policy(policy: &Policy) -> Self {
        Self { max_txs: policy.max_attestation_shard_txs, max_mass: policy.max_attestation_shard_mass, used_txs: 0, used_mass: 0 }
    }

    /// `true` when no caps are configured — callers can skip the per-tx accounting
    /// entirely (the common, non-overlay case).
    #[inline]
    pub fn is_unlimited(&self) -> bool {
        self.max_txs == 0 && self.max_mass == 0
    }

    /// Reset the running usage for a fresh selection round.
    #[inline]
    pub fn reset(&mut self) {
        self.used_txs = 0;
        self.used_mass = 0;
    }

    /// kaspa-pq audit v26 (H-2): release a previously-admitted shard's budget (one tx
    /// + its mass) when that shard is rejected during the template refill loop, so the
    /// persistent per-episode budget is given back rather than leaked. Saturating so it
    /// can never underflow. A no-op for the caller when the cap is unlimited (the caller
    /// gates on shard-ness before calling, matching `admit` which only charges shards).
    #[inline]
    pub fn release(&mut self, mass: u64) {
        self.used_txs = self.used_txs.saturating_sub(1);
        self.used_mass = self.used_mass.saturating_sub(mass);
    }

    /// Decide whether a tx may be admitted given the running shard usage. Non-shard
    /// txs are always admitted and never charged. A shard tx is admitted iff it
    /// would not exceed either configured cap; on admission its mass/count are
    /// charged. Returns `true` to keep, `false` to skip.
    #[inline]
    pub fn admit(&mut self, tx: &Transaction, mass: u64) -> bool {
        if tx.subnetwork_id != SUBNETWORK_ID_STAKE_ATTESTATION_SHARD {
            return true;
        }
        if self.max_txs != 0 && self.used_txs >= self.max_txs {
            return false;
        }
        let next_mass = self.used_mass.saturating_add(mass);
        if self.max_mass != 0 && next_mass > self.max_mass {
            return false;
        }
        self.used_txs += 1;
        self.used_mass = next_mass;
        true
    }
}

pub struct SequenceSelectorTransaction {
    pub tx: Arc<Transaction>,
    pub mass: u64,
}

impl SequenceSelectorTransaction {
    pub fn new(tx: Arc<Transaction>, mass: u64) -> Self {
        Self { tx, mass }
    }
}

type SequencePriorityIndex = u32;

/// The input sequence for the [`SequenceSelector`] transaction selector
#[derive(Default)]
pub struct SequenceSelectorInput {
    /// We use the btree map ordered by insertion order in order to follow
    /// the initial sequence order while allowing for efficient removal of previous selections
    inner: BTreeMap<SequencePriorityIndex, SequenceSelectorTransaction>,
}

impl FromIterator<SequenceSelectorTransaction> for SequenceSelectorInput {
    fn from_iter<T: IntoIterator<Item = SequenceSelectorTransaction>>(iter: T) -> Self {
        Self { inner: BTreeMap::from_iter(iter.into_iter().enumerate().map(|(i, v)| (i as SequencePriorityIndex, v))) }
    }
}

impl SequenceSelectorInput {
    pub fn push(&mut self, tx: Arc<Transaction>, mass: u64) {
        let idx = self.inner.len() as SequencePriorityIndex;
        self.inner.insert(idx, SequenceSelectorTransaction::new(tx, mass));
    }

    pub fn iter(&self) -> impl Iterator<Item = &SequenceSelectorTransaction> {
        self.inner.values()
    }
}

/// Helper struct for storing data related to previous selections
struct SequenceSelectorSelection {
    tx_id: TransactionId,
    mass: u64,
    priority_index: SequencePriorityIndex,
    /// kaspa-pq audit v26 (H-2): whether this selection is a `StakeAttestationShard` tx,
    /// so that on reject we release exactly its unit of the persistent shard budget
    /// (and never touch the budget for a non-shard reject).
    is_shard: bool,
}

/// A selector which selects transactions in the order they are provided. The selector assumes
/// that the transactions were already selected via weighted sampling and simply tries them one
/// after the other until the block mass limit is reached.  
pub struct SequenceSelector {
    input_sequence: SequenceSelectorInput,
    selected_vec: Vec<SequenceSelectorSelection>,
    /// Maps from selected tx ids to `(mass, is_shard)` so that on tx reject the total used
    /// mass can be subtracted and (for shards) the persistent shard budget can be released.
    selected_map: Option<HashMap<TransactionId, (u64, bool)>>,
    total_selected_mass: u64,
    overall_candidates: usize,
    overall_rejections: usize,
    /// kaspa-pq audit v24 (H-3/M-4): per-block `StakeAttestationShard` budget,
    /// enforced in lockstep with the block-mass cap. A no-op (`is_unlimited`) when
    /// the overlay is off, keeping non-overlay nets byte-identical.
    shard_cap: ShardCap,
    policy: Policy,
}

impl SequenceSelector {
    pub fn new(input_sequence: SequenceSelectorInput, policy: Policy) -> Self {
        Self {
            overall_candidates: input_sequence.inner.len(),
            selected_vec: Vec::with_capacity(input_sequence.inner.len()),
            input_sequence,
            selected_map: Default::default(),
            total_selected_mass: Default::default(),
            overall_rejections: Default::default(),
            shard_cap: ShardCap::from_policy(&policy),
            policy,
        }
    }

    #[inline]
    fn reset_selection(&mut self) {
        self.selected_vec.clear();
        self.selected_map = None;
        // kaspa-pq audit v26 (H-2): do NOT reset `shard_cap` here. The shard count/mass
        // budget must persist across the multiple `select_transactions` calls of one
        // template episode (mirroring how `total_selected_mass` persists), so the refill
        // loop cannot re-admit a fresh full cap of shards on every recall. The budget is
        // released per-reject in `reject_selection` and only starts fresh with a new
        // `SequenceSelector` (constructed per build).
    }
}

impl TemplateTransactionSelector for SequenceSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        // Remove selections from the previous round if any
        for selection in self.selected_vec.drain(..) {
            self.input_sequence.inner.remove(&selection.priority_index);
        }
        // Reset selection data structures
        self.reset_selection();
        let mut transactions = Vec::with_capacity(self.input_sequence.inner.len());

        // Iterate the input sequence in order
        for (&priority_index, tx) in self.input_sequence.inner.iter() {
            if self.total_selected_mass.saturating_add(tx.mass) > self.policy.max_block_mass {
                // We assume the sequence is relatively small, hence we keep on searching
                // for transactions with lower mass which might fit into the remaining gap
                continue;
            }
            // kaspa-pq audit v24 (H-3/M-4): enforce the per-block shard tx/mass budget
            // here too (not only in the rebalancing selector). A no-op when the overlay
            // is off. Skip a shard tx over budget but keep searching for fitting txs.
            if !self.shard_cap.admit(&tx.tx, tx.mass) {
                continue;
            }
            self.total_selected_mass += tx.mass;
            let is_shard = tx.tx.subnetwork_id == SUBNETWORK_ID_STAKE_ATTESTATION_SHARD;
            self.selected_vec.push(SequenceSelectorSelection { tx_id: tx.tx.id(), mass: tx.mass, priority_index, is_shard });
            transactions.push(tx.tx.as_ref().clone())
        }
        transactions
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        // Lazy-create the map only when there are actual rejections
        let selected_map =
            self.selected_map.get_or_insert_with(|| self.selected_vec.iter().map(|tx| (tx.tx_id, (tx.mass, tx.is_shard))).collect());
        let (mass, is_shard) = selected_map.remove(&tx_id).expect("only previously selected txs can be rejected (and only once)");
        // Selections must be counted in total selected mass, so this subtraction cannot underflow
        self.total_selected_mass -= mass;
        // kaspa-pq audit v26 (H-2): release the persistent shard budget for a rejected shard
        // so the freed unit can be re-used by the refill. A non-shard reject must NEVER touch
        // the shard budget (only shards are charged in `admit`).
        if is_shard {
            self.shard_cap.release(mass);
        }
        self.overall_rejections += 1;
    }

    fn is_successful(&self) -> bool {
        const SUFFICIENT_MASS_THRESHOLD: f64 = 0.8;
        const LOW_REJECTION_FRACTION: f64 = 0.2;

        // We consider the operation successful if either mass occupation is above 80% or rejection rate is below 20%
        self.overall_rejections == 0
            || (self.total_selected_mass as f64) > self.policy.max_block_mass as f64 * SUFFICIENT_MASS_THRESHOLD
            || (self.overall_rejections as f64) < self.overall_candidates as f64 * LOW_REJECTION_FRACTION
    }
}

/// kaspa-pq DNS-finality block-template composition (P1).
///
/// Yields a fixed, pre-chosen set of "priority" attestation-shard transactions FIRST (deterministic
/// order), then refills from the remaining ready frontier while enforcing a single shared per-block
/// mass and shard budget across both lanes.
///
/// The shared budget is intentional: if the template classifier drops a stale/duplicate/canonical
/// mismatch priority shard, the freed block mass and shard-cap unit must be immediately reusable by
/// the refill lane. A fixed "remaining mass" inner selector cannot do that, which lets stale shards
/// wedge template assembly by occupying all priority mass and then vanishing after classification.
pub struct AttestationPrioritySelector {
    /// The pre-chosen priority attestation txs (with mass), yielded on the first call.
    priority: Vec<SequenceSelectorTransaction>,
    /// Non-priority candidates, searched in deterministic frontier order for refills.
    remainder: SequenceSelectorInput,
    /// Remainder txs selected on the previous call; drained from `remainder` on the next call.
    remainder_selected_vec: Vec<SequenceSelectorSelection>,
    /// Whether the priority batch has already been emitted (it is emitted exactly once).
    priority_emitted: bool,
    /// Mass of selected priority txs that survived (were not rejected/dropped).
    selected_priority_mass: u64,
    /// Mass of selected remainder txs that survived (were not rejected/dropped).
    selected_remainder_mass: u64,
    /// Map of currently-selected priority tx ids -> `(mass, is_shard)`.
    priority_selected_map: HashMap<TransactionId, (u64, bool)>,
    /// Map of currently-selected remainder tx ids -> `(mass, is_shard)`. Built lazily for rejects.
    remainder_selected_map: Option<HashMap<TransactionId, (u64, bool)>>,
    /// Number of validation rejections (classifier drops via `reject_selection_for_refill` do not count).
    validation_rejections: usize,
    overall_candidates: usize,
    /// Shared per-block shard budget across priority and remainder.
    shard_cap: ShardCap,
    policy: Policy,
}

impl AttestationPrioritySelector {
    pub fn new(priority: Vec<SequenceSelectorTransaction>, remainder: SequenceSelectorInput, policy: Policy) -> Self {
        let overall_candidates = priority.len() + remainder.inner.len();
        Self {
            priority,
            remainder,
            remainder_selected_vec: Vec::with_capacity(overall_candidates),
            priority_emitted: false,
            selected_priority_mass: 0,
            selected_remainder_mass: 0,
            priority_selected_map: Default::default(),
            remainder_selected_map: Default::default(),
            validation_rejections: 0,
            overall_candidates,
            shard_cap: ShardCap::from_policy(&policy),
            policy,
        }
    }

    #[inline]
    fn selected_mass(&self) -> u64 {
        self.selected_priority_mass.saturating_add(self.selected_remainder_mass)
    }

    #[inline]
    fn has_mass_room_for(&self, mass: u64) -> bool {
        self.selected_mass().saturating_add(mass) <= self.policy.max_block_mass
    }

    fn emit_priority(&mut self, txs: &mut Vec<Transaction>) {
        for item in &self.priority {
            if !self.has_mass_room_for(item.mass) {
                continue;
            }
            if !self.shard_cap.admit(&item.tx, item.mass) {
                continue;
            }

            let is_shard = item.tx.subnetwork_id == SUBNETWORK_ID_STAKE_ATTESTATION_SHARD;
            self.selected_priority_mass += item.mass;
            self.priority_selected_map.insert(item.tx.id(), (item.mass, is_shard));
            txs.push(item.tx.as_ref().clone());
        }
    }

    fn select_sequence_lane(
        input: &mut SequenceSelectorInput,
        selected_vec: &mut Vec<SequenceSelectorSelection>,
        selected_map: &mut Option<HashMap<TransactionId, (u64, bool)>>,
        selected_lane_mass: &mut u64,
        other_selected_mass: u64,
        shard_cap: &mut ShardCap,
        policy: &Policy,
        txs: &mut Vec<Transaction>,
    ) {
        // Remove accepted/rejected selections from the previous round before searching for
        // replacements, matching `SequenceSelector` refill semantics.
        for selection in selected_vec.drain(..) {
            input.inner.remove(&selection.priority_index);
        }
        *selected_map = None;

        for (&priority_index, item) in input.inner.iter() {
            if other_selected_mass.saturating_add(*selected_lane_mass).saturating_add(item.mass) > policy.max_block_mass {
                continue;
            }
            if !shard_cap.admit(&item.tx, item.mass) {
                continue;
            }

            let is_shard = item.tx.subnetwork_id == SUBNETWORK_ID_STAKE_ATTESTATION_SHARD;
            *selected_lane_mass += item.mass;
            selected_vec.push(SequenceSelectorSelection { tx_id: item.tx.id(), mass: item.mass, priority_index, is_shard });
            txs.push(item.tx.as_ref().clone());
        }
    }

    fn select_remainder(&mut self, txs: &mut Vec<Transaction>) {
        let priority_mass = self.selected_priority_mass;
        Self::select_sequence_lane(
            &mut self.remainder,
            &mut self.remainder_selected_vec,
            &mut self.remainder_selected_map,
            &mut self.selected_remainder_mass,
            priority_mass,
            &mut self.shard_cap,
            &self.policy,
            txs,
        );
    }

    /// Remove a tx from the shared occupation accounting, freeing its mass and optional shard-cap
    /// unit for a subsequent refill. The caller decides whether this is a validation rejection or
    /// merely a classifier/policy drop.
    fn remove_selection(&mut self, tx_id: TransactionId) {
        if let Some((mass, is_shard)) = self.priority_selected_map.remove(&tx_id) {
            self.selected_priority_mass = self.selected_priority_mass.saturating_sub(mass);
            if is_shard {
                self.shard_cap.release(mass);
            }
            return;
        }

        let selected_map = self
            .remainder_selected_map
            .get_or_insert_with(|| self.remainder_selected_vec.iter().map(|tx| (tx.tx_id, (tx.mass, tx.is_shard))).collect());
        let (mass, is_shard) = selected_map.remove(&tx_id).expect("only previously selected txs can be rejected (and only once)");
        self.selected_remainder_mass = self.selected_remainder_mass.saturating_sub(mass);
        if is_shard {
            self.shard_cap.release(mass);
        }
    }
}

impl TemplateTransactionSelector for AttestationPrioritySelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        let mut txs = Vec::with_capacity(self.priority.len().saturating_add(self.remainder.inner.len()));
        if !self.priority_emitted {
            self.priority_emitted = true;
            self.emit_priority(&mut txs);
        }
        self.select_remainder(&mut txs);
        txs
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        self.remove_selection(tx_id);
        self.validation_rejections += 1;
    }

    fn reject_selection_for_refill(&mut self, tx_id: TransactionId) {
        // A classifier/policy DROP (not a validation failure). Free the mass exactly like
        // `reject_selection`, but do not count it against the selector success heuristic.
        self.remove_selection(tx_id);
    }

    fn is_successful(&self) -> bool {
        const SUFFICIENT_MASS_THRESHOLD: f64 = 0.8;
        const LOW_REJECTION_FRACTION: f64 = 0.2;

        // Classifier drops are expected refill events. Only consensus-validation rejections should
        // make the mining loop retry, and even then the standard "sufficient mass / low rejection
        // fraction" heuristic applies.
        self.validation_rejections == 0
            || (self.selected_mass() as f64) > self.policy.max_block_mass as f64 * SUFFICIENT_MASS_THRESHOLD
            || (self.validation_rejections as f64) < self.overall_candidates as f64 * LOW_REJECTION_FRACTION
    }
}

/// A selector that selects all the transactions it holds and is always considered successful.
/// If all mempool transactions have combined mass which is <= block mass limit, this selector
/// should be called and provided with all the transactions.
pub struct TakeAllSelector {
    txs: Vec<Arc<Transaction>>,
    /// kaspa-pq audit v24 (H-3/M-4): per-tx masses, parallel to `txs`, present only
    /// when a `StakeAttestationShard` cap is configured (otherwise empty — the
    /// non-overlay path stays allocation-free). Used to enforce the shard mass cap.
    masses: Vec<u64>,
    /// kaspa-pq audit v24 (H-3/M-4): the per-block shard budget. A no-op
    /// (`is_unlimited`) when the overlay is off, keeping non-overlay nets identical.
    cap: ShardCap,
}

impl TakeAllSelector {
    pub fn new(txs: Vec<Arc<Transaction>>) -> Self {
        Self { txs, masses: Vec::new(), cap: ShardCap::default() }
    }

    /// kaspa-pq audit v24 (H-3/M-4): construct with a per-block shard cap. `masses`
    /// must be 1:1 with `txs`. Shard txs beyond the cap (count or mass) are skipped;
    /// non-shard txs are always taken. When the cap is unlimited this behaves exactly
    /// like [`TakeAllSelector::new`].
    pub(crate) fn with_cap(txs: Vec<Arc<Transaction>>, masses: Vec<u64>, cap: ShardCap) -> Self {
        debug_assert!(cap.is_unlimited() || masses.len() == txs.len(), "masses must be 1:1 with txs when a shard cap applies");
        Self { txs, masses, cap }
    }
}

impl TemplateTransactionSelector for TakeAllSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        // Fast path: no shard cap → drain everything (byte-identical to upstream).
        if self.cap.is_unlimited() {
            return self.txs.drain(..).map(|tx| tx.as_ref().clone()).collect();
        }
        // Cap path: take all non-shard txs and shard txs within the per-block budget.
        self.cap.reset();
        let mut out = Vec::with_capacity(self.txs.len());
        for (tx, &mass) in self.txs.drain(..).zip(self.masses.iter()) {
            if self.cap.admit(&tx, mass) {
                out.push(tx.as_ref().clone());
            }
        }
        out
    }

    fn reject_selection(&mut self, _tx_id: TransactionId) {
        // No need to track rejections (for reduced mass), since there's nothing else to select
    }

    fn is_successful(&self) -> bool {
        // Considered successful because we provided all mempool transactions to this
        // selector, so there's no point in retries
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        constants::TX_VERSION,
        subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD},
        tx::{Transaction, TransactionOutput},
    };

    const SHARD_MASS: u64 = 1_000;

    /// A unique `StakeAttestationShard` tx (differing by output value so ids differ).
    fn shard_tx(value: u64) -> Arc<Transaction> {
        let spk = kaspa_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0x51]);
        let out = TransactionOutput::new(value, spk);
        Arc::new(Transaction::new(TX_VERSION, vec![], vec![out], 0, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, 0, vec![]))
    }

    /// A unique native tx.
    fn native_tx(value: u64) -> Arc<Transaction> {
        let spk = kaspa_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0x51]);
        let out = TransactionOutput::new(value, spk);
        Arc::new(Transaction::new(TX_VERSION, vec![], vec![out], 0, SUBNETWORK_ID_NATIVE, 0, vec![]))
    }

    /// kaspa-pq audit v26 (H-2): the SequenceSelector shard budget must PERSIST across the
    /// multiple `select_transactions` calls of one template episode. With a cap of 2 shards,
    /// a first batch takes 2; a forced second call must admit 0 more (cumulative kept shards
    /// <= 2). Before the fix, `reset_selection` zeroed the budget every call, so the second
    /// call re-admitted a fresh 2 (cumulative 4).
    #[test]
    fn sequence_selector_shard_budget_persists_across_calls() {
        let policy = Policy::new(1_000_000).with_max_attestation_shard_txs(2);
        // Four shard txs, ample block mass; cap = 2.
        let input: SequenceSelectorInput = (0..4).map(|i| SequenceSelectorTransaction::new(shard_tx(100 + i), SHARD_MASS)).collect();
        let mut selector = SequenceSelector::new(input, policy);

        let mut kept: std::collections::HashSet<TransactionId> = std::collections::HashSet::new();
        let batch1 = selector.select_transactions();
        for tx in &batch1 {
            kept.insert(tx.id());
        }
        assert_eq!(batch1.len(), 2, "first batch must be capped at 2 shards");

        // Force a SECOND select without rejecting anything. The 4-element input had its 2
        // selections drained; the budget (now used=2) must block the remaining 2 shards.
        let batch2 = selector.select_transactions();
        for tx in &batch2 {
            kept.insert(tx.id());
        }
        assert!(batch2.is_empty(), "second call must admit 0 more shards (budget persists)");
        let shard_kept = kept.len();
        assert!(shard_kept <= 2, "cumulative kept shards must not exceed the cap of 2 (H-2); got {shard_kept}");
    }

    /// kaspa-pq audit v26 (H-2): rejecting a shard releases exactly one unit of the persistent
    /// budget, so the refill can admit one replacement (and only one).
    #[test]
    fn sequence_selector_reject_releases_one_budget_unit() {
        let policy = Policy::new(1_000_000).with_max_attestation_shard_txs(2);
        let input: SequenceSelectorInput = (0..4).map(|i| SequenceSelectorTransaction::new(shard_tx(100 + i), SHARD_MASS)).collect();
        let mut selector = SequenceSelector::new(input, policy);

        let batch1 = selector.select_transactions();
        assert_eq!(batch1.len(), 2);
        // Reject ONE selected shard -> releases one unit (used 2 -> 1).
        selector.reject_selection(batch1[0].id());

        // Recall: the rejected+other selection are drained from input; with one unit free, the
        // refill admits exactly ONE replacement shard.
        let batch2 = selector.select_transactions();
        assert_eq!(batch2.len(), 1, "exactly one unit of shard budget must be released on a shard reject (H-2)");
    }

    /// kaspa-pq audit v26 (H-2): a NON-shard reject must not touch the shard budget.
    #[test]
    fn sequence_selector_non_shard_reject_does_not_release_shard_budget() {
        let policy = Policy::new(1_000_000).with_max_attestation_shard_txs(1);
        // One native + two shards. Cap = 1 shard.
        let mut input = SequenceSelectorInput::default();
        let native = native_tx(7);
        input.push(native.clone(), SHARD_MASS);
        input.push(shard_tx(100), SHARD_MASS);
        input.push(shard_tx(101), SHARD_MASS);
        let mut selector = SequenceSelector::new(input, policy);

        let batch1 = selector.select_transactions();
        // native + 1 shard (cap=1).
        let shards1 = batch1.iter().filter(|t| t.subnetwork_id == SUBNETWORK_ID_STAKE_ATTESTATION_SHARD).count();
        assert_eq!(shards1, 1);
        // Reject the NATIVE tx -> must NOT free shard budget.
        selector.reject_selection(native.id());
        let batch2 = selector.select_transactions();
        let shards2 = batch2.iter().filter(|t| t.subnetwork_id == SUBNETWORK_ID_STAKE_ATTESTATION_SHARD).count();
        assert_eq!(shards2, 0, "a non-shard reject must not release any shard budget (H-2)");
    }

    /// kaspa-pq audit v26 (H-3): a classifier DROP of a priority shard via
    /// `reject_selection_for_refill` frees its mass but does NOT flip `is_successful` to false,
    /// whereas a validation `reject_selection` of a priority shard DOES count as a rejection.
    /// Mass accounting (`selected_priority_mass`) is decremented in BOTH cases.
    #[test]
    fn attestation_priority_selector_refill_drop_vs_reject() {
        let policy = Policy::new(1_000_000);
        let priority = vec![SequenceSelectorTransaction::new(shard_tx(100), SHARD_MASS)];
        let shard_id = priority[0].tx.id();
        let mut sel = AttestationPrioritySelector::new(priority, SequenceSelectorInput::default(), policy.clone());
        let _ = sel.select_transactions();
        assert_eq!(sel.selected_priority_mass, SHARD_MASS);

        // Classifier drop (refill): mass freed, success preserved.
        sel.reject_selection_for_refill(shard_id);
        assert_eq!(sel.selected_priority_mass, 0, "refill drop must free the priority mass");
        assert!(sel.is_successful(), "a refill drop of a priority shard must NOT flip is_successful (H-3)");

        // Validation reject of a priority shard: counts as a rejection.
        let priority2 = vec![SequenceSelectorTransaction::new(shard_tx(200), SHARD_MASS)];
        let shard_id2 = priority2[0].tx.id();
        let mut sel2 = AttestationPrioritySelector::new(priority2, SequenceSelectorInput::default(), policy);
        let _ = sel2.select_transactions();
        sel2.reject_selection(shard_id2);
        assert_eq!(sel2.selected_priority_mass, 0, "validation reject must also free the priority mass");
        // The only priority shard was rejected (100% of priority mass gone, and a rejection
        // recorded), so the priority arm of the success heuristic is not satisfied.
        assert!(!sel2.is_successful(), "a validation reject of the sole priority shard must flip is_successful (H-3)");
    }

    /// A priority lane drop must return block mass to the refill lane. This covers the failure mode
    /// where stale/invalid priority shards filled the whole priority budget, were dropped by the
    /// classifier, and the old fixed-remaining-mass inner selector could not use the freed capacity.
    #[test]
    fn attestation_priority_drop_frees_mass_for_remainder_refill() {
        let policy = Policy::new(1_200);
        let priority = vec![SequenceSelectorTransaction::new(shard_tx(100), 1_000)];
        let priority_id = priority[0].tx.id();
        let remainder_tx = native_tx(7);
        let mut remainder = SequenceSelectorInput::default();
        remainder.push(remainder_tx.clone(), 700);

        let mut sel = AttestationPrioritySelector::new(priority, remainder, policy);
        let first = sel.select_transactions();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id(), priority_id);

        sel.reject_selection_for_refill(priority_id);
        let refill = sel.select_transactions();
        assert_eq!(refill.len(), 1, "refill must use mass freed by a priority classifier drop");
        assert_eq!(refill[0].id(), remainder_tx.id());
        assert!(sel.is_successful(), "classifier drops should not make an otherwise refilled selector fail");
    }

    /// The shared budget also applies to finite shard-count caps: dropping one priority shard should
    /// release exactly one shard-cap unit to a non-priority replacement shard.
    #[test]
    fn attestation_priority_drop_frees_shard_cap_for_remainder_refill() {
        let policy = Policy::new(1_000_000).with_max_attestation_shard_txs(1);
        let priority = vec![SequenceSelectorTransaction::new(shard_tx(100), SHARD_MASS)];
        let priority_id = priority[0].tx.id();
        let replacement = shard_tx(200);
        let mut remainder = SequenceSelectorInput::default();
        remainder.push(replacement.clone(), SHARD_MASS);

        let mut sel = AttestationPrioritySelector::new(priority, remainder, policy);
        let first = sel.select_transactions();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id(), priority_id);

        sel.reject_selection_for_refill(priority_id);
        let refill = sel.select_transactions();
        assert_eq!(refill.len(), 1, "refill must use shard-cap unit freed by the dropped priority shard");
        assert_eq!(refill[0].id(), replacement.id());
    }
}
