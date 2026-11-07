use derive_more::Display;
use kaspa_consensus_core::BlockHash; // PR-9.5e: notification block-hash fields widened to Hash64
use kaspa_consensus_core::{acceptance_data::AcceptanceData, block::Block, utxo::utxo_diff::UtxoDiff};
use kaspa_notify::{
    events::EventType,
    full_featured,
    notification::Notification as NotificationTrait,
    subscription::{
        Subscription,
        context::SubscriptionContext,
        single::{OverallSubscription, UtxosChangedSubscription, VirtualChainChangedSubscription},
    },
};
use std::sync::Arc;

full_featured! {
#[derive(Clone, Debug, Display)]
pub enum Notification {
    #[display(fmt = "BlockAdded notification: block hash {}", "_0.block.header.hash")]
    BlockAdded(BlockAddedNotification),

    #[display(fmt = "VirtualChainChanged notification: {} removed blocks, {} added blocks, {} accepted transactions", "_0.removed_chain_block_hashes.len()", "_0.added_chain_block_hashes.len()", "_0.added_chain_blocks_acceptance_data.len()")]
    VirtualChainChanged(VirtualChainChangedNotification),

    #[display(fmt = "FinalityConflict notification: violating block hash {}", "_0.violating_block_hash")]
    FinalityConflict(FinalityConflictNotification),

    #[display(fmt = "FinalityConflict notification: violating block hash {}", "_0.finality_block_hash")]
    FinalityConflictResolved(FinalityConflictResolvedNotification),

    #[display(fmt = "UtxosChanged notification")]
    UtxosChanged(UtxosChangedNotification),

    #[display(fmt = "SinkBlueScoreChanged notification: virtual selected parent blue score {}", "_0.sink_blue_score")]
    SinkBlueScoreChanged(SinkBlueScoreChangedNotification),

    #[display(fmt = "VirtualDaaScoreChanged notification: virtual DAA score {}", "_0.virtual_daa_score")]
    VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification),

    #[display(fmt = "PruningPointUtxoSetOverride notification")]
    PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideNotification),

    #[display(fmt = "NewBlockTemplate notification")]
    NewBlockTemplate(NewBlockTemplateNotification),

    #[display(fmt = "PalwClassReadinessChanged notification: class {} ready {}/{}", "_0.class_id", "_0.ready_seats", "_0.required_ready_seats")]
    PalwClassReadinessChanged(PalwClassReadinessChangedNotification),

    #[display(fmt = "PalwPanelAssignment notification: claim {}", "_0.claim_id")]
    PalwPanelAssignment(PalwPanelAssignmentNotification),

    #[display(fmt = "PalwPanelReceipt notification: claim {} valid {}", "_0.claim_id", "_0.valid_receipt_seats")]
    PalwPanelReceipt(PalwPanelReceiptNotification),

    #[display(fmt = "PalwPanelEligibilityChanged notification: seat {} eligible {}", "_0.seat_id", "_0.eligible")]
    PalwPanelEligibilityChanged(PalwPanelEligibilityChangedNotification),
}
}

impl NotificationTrait for Notification {
    fn apply_overall_subscription(&self, subscription: &OverallSubscription, _context: &SubscriptionContext) -> Option<Self> {
        match subscription.active() {
            true => Some(self.clone()),
            false => None,
        }
    }

    fn apply_virtual_chain_changed_subscription(
        &self,
        subscription: &VirtualChainChangedSubscription,
        _context: &SubscriptionContext,
    ) -> Option<Self> {
        match subscription.active() {
            true => {
                // If the subscription excludes accepted transaction ids and the notification includes some
                // then we must re-create the object and drop the ids, otherwise we can clone it as is.
                if let Notification::VirtualChainChanged(payload) = self
                    && !subscription.include_accepted_transaction_ids()
                    && !payload.added_chain_blocks_acceptance_data.is_empty()
                {
                    return Some(Notification::VirtualChainChanged(VirtualChainChangedNotification {
                        removed_chain_block_hashes: payload.removed_chain_block_hashes.clone(),
                        added_chain_block_hashes: payload.added_chain_block_hashes.clone(),
                        added_chain_blocks_acceptance_data: Arc::new(vec![]),
                    }));
                }
                Some(self.clone())
            }
            false => None,
        }
    }

    fn apply_utxos_changed_subscription(
        &self,
        _subscription: &UtxosChangedSubscription,
        _context: &SubscriptionContext,
    ) -> Option<Self> {
        // No effort is made here to apply the subscription addresses.
        // This will be achieved farther along the notification backbone.
        Some(self.clone())
    }

    fn event_type(&self) -> EventType {
        self.into()
    }
}

#[derive(Debug, Clone)]
pub struct BlockAddedNotification {
    pub block: Block,
}

impl BlockAddedNotification {
    pub fn new(block: Block) -> Self {
        Self { block }
    }
}

#[derive(Debug, Clone)]
pub struct VirtualChainChangedNotification {
    pub added_chain_block_hashes: Arc<Vec<BlockHash>>,
    pub removed_chain_block_hashes: Arc<Vec<BlockHash>>,
    pub added_chain_blocks_acceptance_data: Arc<Vec<Arc<AcceptanceData>>>,
}
impl VirtualChainChangedNotification {
    pub fn new(
        added_chain_block_hashes: Arc<Vec<BlockHash>>,
        removed_chain_block_hashes: Arc<Vec<BlockHash>>,
        added_chain_blocks_acceptance_data: Arc<Vec<Arc<AcceptanceData>>>,
    ) -> Self {
        Self { added_chain_block_hashes, removed_chain_block_hashes, added_chain_blocks_acceptance_data }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FinalityConflictNotification {
    pub violating_block_hash: BlockHash,
}

impl FinalityConflictNotification {
    pub fn new(violating_block_hash: BlockHash) -> Self {
        Self { violating_block_hash }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FinalityConflictResolvedNotification {
    pub finality_block_hash: BlockHash,
}

impl FinalityConflictResolvedNotification {
    pub fn new(finality_block_hash: BlockHash) -> Self {
        Self { finality_block_hash }
    }
}

#[derive(Debug, Clone)]
pub struct UtxosChangedNotification {
    /// Accumulated UTXO diff between the last virtual state and the current virtual state
    pub accumulated_utxo_diff: Arc<UtxoDiff>,
    pub virtual_parents: Arc<Vec<BlockHash>>,
}

impl UtxosChangedNotification {
    pub fn new(accumulated_utxo_diff: Arc<UtxoDiff>, virtual_parents: Arc<Vec<BlockHash>>) -> Self {
        Self { accumulated_utxo_diff, virtual_parents }
    }
}

#[derive(Debug, Clone)]
pub struct SinkBlueScoreChangedNotification {
    pub sink_blue_score: u64,
}

impl SinkBlueScoreChangedNotification {
    pub fn new(sink_blue_score: u64) -> Self {
        Self { sink_blue_score }
    }
}

#[derive(Debug, Clone)]
pub struct VirtualDaaScoreChangedNotification {
    pub virtual_daa_score: u64,
}

impl VirtualDaaScoreChangedNotification {
    pub fn new(virtual_daa_score: u64) -> Self {
        Self { virtual_daa_score }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PruningPointUtxoSetOverrideNotification {}

#[derive(Debug, Clone)]
pub struct NewBlockTemplateNotification {}

#[derive(Debug, Clone)]
pub struct PalwClassReadinessChangedNotification {
    pub class_id: String,
    pub model_name: String,
    pub registry_state: String,
    pub previous_registry_state: String,
    pub ready_seats: u32,
    pub previous_ready_seats: u32,
    pub required_ready_seats: u32,
    pub bonded_seats: u32,
}

#[derive(Debug, Clone)]
pub struct PalwPanelAssignmentSeatNotification {
    pub seat_id: String,
    pub seat_index: u16,
    pub full_seat: bool,
    pub segment_index: Option<u16>,
    pub mask: u32,
    pub receipt_status: String,
}

#[derive(Debug, Clone)]
pub struct PalwPanelAssignmentNotification {
    pub claim_id: String,
    pub class_id: String,
    pub licensed_state: String,
    pub deadline_daa: u64,
    pub coverage_mask: u32,
    pub full_seat: String,
    pub valid_receipt_seats: u32,
    pub selected_panel_seats: u32,
    pub seats: Vec<PalwPanelAssignmentSeatNotification>,
}

#[derive(Debug, Clone)]
pub struct PalwPanelReceiptNotification {
    pub claim_id: String,
    pub class_id: String,
    pub coverage_mask: u32,
    pub previous_coverage_mask: u32,
    pub valid_receipt_seats: u32,
    pub previous_valid_receipt_seats: u32,
    pub selected_panel_seats: u32,
}

#[derive(Debug, Clone)]
pub struct PalwPanelEligibilityChangedNotification {
    pub seat_id: String,
    pub class_id: String,
    pub eligible: bool,
    pub ready: bool,
    pub hold_code: String,
    pub hold_message: String,
}
