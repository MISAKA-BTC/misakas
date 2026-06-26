use crate::Policy;
use kaspa_consensus_core::{
    block::TemplateTransactionSelector,
    tx::{Transaction, TransactionId},
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

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
}

/// A selector which selects transactions in the order they are provided. The selector assumes
/// that the transactions were already selected via weighted sampling and simply tries them one
/// after the other until the block mass limit is reached.  
pub struct SequenceSelector {
    input_sequence: SequenceSelectorInput,
    selected_vec: Vec<SequenceSelectorSelection>,
    /// Maps from selected tx ids to tx mass so that the total used mass can be subtracted on tx reject
    selected_map: Option<HashMap<TransactionId, u64>>,
    total_selected_mass: u64,
    overall_candidates: usize,
    overall_rejections: usize,
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
            policy,
        }
    }

    #[inline]
    fn reset_selection(&mut self) {
        self.selected_vec.clear();
        self.selected_map = None;
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
            self.total_selected_mass += tx.mass;
            self.selected_vec.push(SequenceSelectorSelection { tx_id: tx.tx.id(), mass: tx.mass, priority_index });
            transactions.push(tx.tx.as_ref().clone())
        }
        transactions
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        // Lazy-create the map only when there are actual rejections
        let selected_map = self.selected_map.get_or_insert_with(|| self.selected_vec.iter().map(|tx| (tx.tx_id, tx.mass)).collect());
        let mass = selected_map.remove(&tx_id).expect("only previously selected txs can be rejected (and only once)");
        // Selections must be counted in total selected mass, so this subtraction cannot underflow
        self.total_selected_mass -= mass;
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
/// order, bounded by the per-block shard tx/mass budget), then delegates to an inner selector built
/// over the remaining (non-priority) candidates with the remaining block mass.
///
/// This makes current/recent-epoch (and reward-fresh) attestation shards win over stale ones in the
/// template even when the stale ones have accumulated, which was the root cause of the live-testnet
/// DNS-finality stall. Correct-over-clever: the priority set is selected once up front; rejects of
/// priority txs are tracked so the inner selector's mass accounting is never double-counted, and the
/// total selected mass never exceeds the block limit.
pub struct AttestationPrioritySelector {
    /// The pre-chosen priority attestation txs (with mass), yielded on the first call.
    priority: Vec<SequenceSelectorTransaction>,
    /// Inner selector for the remaining (non-priority) candidates.
    inner: Box<dyn TemplateTransactionSelector>,
    /// Whether the priority batch has already been emitted (it is emitted exactly once).
    priority_emitted: bool,
    /// Mass of the priority txs that survived (were not rejected), for the success heuristic.
    selected_priority_mass: u64,
    /// Map of currently-selected priority tx ids -> mass (for reject bookkeeping). Built lazily.
    priority_selected_map: Option<HashMap<TransactionId, u64>>,
    /// Number of priority rejections (counts toward the success heuristic).
    priority_rejections: usize,
    policy: Policy,
}

impl AttestationPrioritySelector {
    pub fn new(priority: Vec<SequenceSelectorTransaction>, inner: Box<dyn TemplateTransactionSelector>, policy: Policy) -> Self {
        Self {
            priority,
            inner,
            priority_emitted: false,
            selected_priority_mass: 0,
            priority_selected_map: None,
            priority_rejections: 0,
            policy,
        }
    }
}

impl TemplateTransactionSelector for AttestationPrioritySelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        if !self.priority_emitted {
            self.priority_emitted = true;
            // Emit all priority txs first (their cumulative mass was already bounded at construction
            // time against the per-block shard mass budget and the block mass). Then top up from the
            // inner selector with whatever block mass remains.
            self.selected_priority_mass = self.priority.iter().map(|t| t.mass).sum();
            self.priority_selected_map = None;
            let mut txs: Vec<Transaction> = self.priority.iter().map(|t| t.tx.as_ref().clone()).collect();
            txs.extend(self.inner.select_transactions());
            txs
        } else {
            // Subsequent calls only refill from the inner selector (priority txs are emitted once).
            self.inner.select_transactions()
        }
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        // A rejection may target either a priority tx or an inner tx. Resolve against the priority
        // set first (lazily built map), else delegate to the inner selector.
        let map = self.priority_selected_map.get_or_insert_with(|| self.priority.iter().map(|t| (t.tx.id(), t.mass)).collect());
        if let Some(mass) = map.remove(&tx_id) {
            self.selected_priority_mass = self.selected_priority_mass.saturating_sub(mass);
            self.priority_rejections += 1;
        } else {
            self.inner.reject_selection(tx_id);
        }
    }

    fn is_successful(&self) -> bool {
        const SUFFICIENT_MASS_THRESHOLD: f64 = 0.8;

        // Successful if the inner selection is successful, or if the priority txs alone already fill
        // most of the block, or if priority had no rejections and the inner selector is satisfied.
        self.inner.is_successful()
            && (self.priority_rejections == 0
                || (self.selected_priority_mass as f64) > self.policy.max_block_mass as f64 * SUFFICIENT_MASS_THRESHOLD)
    }
}

/// A selector that selects all the transactions it holds and is always considered successful.
/// If all mempool transactions have combined mass which is <= block mass limit, this selector
/// should be called and provided with all the transactions.
pub struct TakeAllSelector {
    txs: Vec<Arc<Transaction>>,
}

impl TakeAllSelector {
    pub fn new(txs: Vec<Arc<Transaction>>) -> Self {
        Self { txs }
    }
}

impl TemplateTransactionSelector for TakeAllSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        // Drain on the first call so that subsequent calls return nothing
        self.txs.drain(..).map(|tx| tx.as_ref().clone()).collect()
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
