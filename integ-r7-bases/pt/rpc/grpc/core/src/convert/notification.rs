use crate::protowire::{
    BlockAddedNotificationMessage, KaspadResponse, NewBlockTemplateNotificationMessage, PalwClassReadinessChangedNotificationMessage,
    PalwPanelAssignmentNotificationMessage, PalwPanelEligibilityChangedNotificationMessage, PalwPanelReceiptNotificationMessage,
    RpcNotifyCommand, kaspad_response::Payload,
};
use crate::protowire::{
    FinalityConflictNotificationMessage, FinalityConflictResolvedNotificationMessage, NotifyPruningPointUtxoSetOverrideRequestMessage,
    NotifyPruningPointUtxoSetOverrideResponseMessage, NotifyUtxosChangedRequestMessage, NotifyUtxosChangedResponseMessage,
    PruningPointUtxoSetOverrideNotificationMessage, SinkBlueScoreChangedNotificationMessage,
    StopNotifyingPruningPointUtxoSetOverrideRequestMessage, StopNotifyingPruningPointUtxoSetOverrideResponseMessage,
    StopNotifyingUtxosChangedRequestMessage, StopNotifyingUtxosChangedResponseMessage, UtxosChangedNotificationMessage,
    VirtualChainChangedNotificationMessage, VirtualDaaScoreChangedNotificationMessage,
};
use crate::{from, try_from};
use kaspa_notify::subscription::Command;
use kaspa_rpc_core::{Notification, RpcError, RpcHash, RpcResult};
use std::str::FromStr;
use std::sync::Arc;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::Notification, KaspadResponse, { Self { id: 0, payload: Some(item.into()) } });

from!(item: &kaspa_rpc_core::Notification, Payload, {
    match item {
        Notification::BlockAdded(notification) => Payload::BlockAddedNotification(notification.into()),
        Notification::NewBlockTemplate(notification) => Payload::NewBlockTemplateNotification(notification.into()),
        Notification::PalwClassReadinessChanged(notification) => Payload::PalwClassReadinessChangedNotification(notification.into()),
        Notification::PalwPanelAssignment(notification) => Payload::PalwPanelAssignmentNotification(notification.into()),
        Notification::PalwPanelReceipt(notification) => Payload::PalwPanelReceiptNotification(notification.into()),
        Notification::PalwPanelEligibilityChanged(notification) => {
            Payload::PalwPanelEligibilityChangedNotification(notification.into())
        },
        Notification::VirtualChainChanged(notification) => Payload::VirtualChainChangedNotification(notification.into()),
        Notification::FinalityConflict(notification) => Payload::FinalityConflictNotification(notification.into()),
        Notification::FinalityConflictResolved(notification) => Payload::FinalityConflictResolvedNotification(notification.into()),
        Notification::UtxosChanged(notification) => Payload::UtxosChangedNotification(notification.into()),
        Notification::SinkBlueScoreChanged(notification) => Payload::SinkBlueScoreChangedNotification(notification.into()),
        Notification::VirtualDaaScoreChanged(notification) => Payload::VirtualDaaScoreChangedNotification(notification.into()),
        Notification::PruningPointUtxoSetOverride(notification) => {
            Payload::PruningPointUtxoSetOverrideNotification(notification.into())
        },
    }
});

from!(item: &kaspa_rpc_core::BlockAddedNotification, BlockAddedNotificationMessage, { Self { block: Some((&*item.block).into()) } });

from!(&kaspa_rpc_core::NewBlockTemplateNotification, NewBlockTemplateNotificationMessage);

from!(item: &kaspa_rpc_core::PalwClassReadinessChangedNotification, PalwClassReadinessChangedNotificationMessage, {
    Self {
        class_id: item.class_id.clone(),
        model_name: item.model_name.clone(),
        registry_state: item.registry_state.clone(),
        previous_registry_state: item.previous_registry_state.clone(),
        ready_seats: item.ready_seats,
        previous_ready_seats: item.previous_ready_seats,
        required_ready_seats: item.required_ready_seats,
        bonded_seats: item.bonded_seats,
    }
});
from!(item: &kaspa_rpc_core::PalwPanelAssignmentNotification, PalwPanelAssignmentNotificationMessage, {
    Self {
        claim_id: item.claim_id.clone(),
        class_id: item.class_id.clone(),
        licensed_state: item.licensed_state.clone(),
        deadline_daa: item.deadline_daa,
        coverage_mask: item.coverage_mask,
        full_seat: item.full_seat.clone(),
        valid_receipt_seats: item.valid_receipt_seats,
        selected_panel_seats: item.selected_panel_seats,
        seats: item.seats.iter().map(crate::protowire::RpcPalwPanelAssignmentSeat::from).collect(),
    }
});
from!(item: &kaspa_rpc_core::PalwPanelReceiptNotification, PalwPanelReceiptNotificationMessage, {
    Self {
        claim_id: item.claim_id.clone(),
        class_id: item.class_id.clone(),
        coverage_mask: item.coverage_mask,
        previous_coverage_mask: item.previous_coverage_mask,
        valid_receipt_seats: item.valid_receipt_seats,
        previous_valid_receipt_seats: item.previous_valid_receipt_seats,
        selected_panel_seats: item.selected_panel_seats,
    }
});
from!(item: &kaspa_rpc_core::PalwPanelEligibilityChangedNotification, PalwPanelEligibilityChangedNotificationMessage, {
    Self {
        seat_id: item.seat_id.clone(),
        class_id: item.class_id.clone(),
        eligible: item.eligible,
        ready: item.ready,
        hold: item.hold.as_ref().map(crate::protowire::RpcPalwPanelHoldReason::from),
    }
});

from!(item: &kaspa_rpc_core::VirtualChainChangedNotification, VirtualChainChangedNotificationMessage, {
    Self {
        removed_chain_block_hashes: item.removed_chain_block_hashes.iter().map(|x| x.to_string()).collect(),
        added_chain_block_hashes: item.added_chain_block_hashes.iter().map(|x| x.to_string()).collect(),
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| x.into()).collect(),
    }
});

from!(item: &kaspa_rpc_core::FinalityConflictNotification, FinalityConflictNotificationMessage, {
    Self { violating_block_hash: item.violating_block_hash.to_string() }
});

from!(item: &kaspa_rpc_core::FinalityConflictResolvedNotification, FinalityConflictResolvedNotificationMessage, {
    Self { finality_block_hash: item.finality_block_hash.to_string() }
});

from!(item: &kaspa_rpc_core::UtxosChangedNotification, UtxosChangedNotificationMessage, {
    Self {
        added: item.added.iter().map(|x| x.into()).collect::<Vec<_>>(),
        removed: item.removed.iter().map(|x| x.into()).collect::<Vec<_>>(),
    }
});

from!(item: &kaspa_rpc_core::SinkBlueScoreChangedNotification, SinkBlueScoreChangedNotificationMessage, {
    Self { sink_blue_score: item.sink_blue_score }
});

from!(item: &kaspa_rpc_core::VirtualDaaScoreChangedNotification, VirtualDaaScoreChangedNotificationMessage, {
    Self { virtual_daa_score: item.virtual_daa_score }
});

from!(&kaspa_rpc_core::PruningPointUtxoSetOverrideNotification, PruningPointUtxoSetOverrideNotificationMessage);

from!(item: Command, RpcNotifyCommand, {
    match item {
        Command::Start => RpcNotifyCommand::NotifyStart,
        Command::Stop => RpcNotifyCommand::NotifyStop,
    }
});

from!(item: &StopNotifyingUtxosChangedRequestMessage, NotifyUtxosChangedRequestMessage, {
    Self { addresses: item.addresses.clone(), command: Command::Stop.into() }
});

from!(_item: &StopNotifyingPruningPointUtxoSetOverrideRequestMessage, NotifyPruningPointUtxoSetOverrideRequestMessage, {
    Self { command: Command::Stop.into() }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &KaspadResponse, kaspa_rpc_core::Notification, {
    item.payload
        .as_ref()
        .ok_or_else(|| RpcError::MissingRpcFieldError("KaspadResponse".to_string(), "payload".to_string()))?
        .try_into()?
});

try_from!(item: &Payload, kaspa_rpc_core::Notification, {
    match item {
        Payload::BlockAddedNotification(notification) => Notification::BlockAdded(notification.try_into()?),
        Payload::NewBlockTemplateNotification(notification) => Notification::NewBlockTemplate(notification.try_into()?),
        Payload::PalwClassReadinessChangedNotification(notification) => {
            Notification::PalwClassReadinessChanged(notification.try_into()?)
        }
        Payload::PalwPanelAssignmentNotification(notification) => Notification::PalwPanelAssignment(notification.try_into()?),
        Payload::PalwPanelReceiptNotification(notification) => Notification::PalwPanelReceipt(notification.try_into()?),
        Payload::PalwPanelEligibilityChangedNotification(notification) => {
            Notification::PalwPanelEligibilityChanged(notification.try_into()?)
        }
        Payload::VirtualChainChangedNotification(notification) => Notification::VirtualChainChanged(notification.try_into()?),
        Payload::FinalityConflictNotification(notification) => Notification::FinalityConflict(notification.try_into()?),
        Payload::FinalityConflictResolvedNotification(notification) => {
            Notification::FinalityConflictResolved(notification.try_into()?)
        }
        Payload::UtxosChangedNotification(notification) => Notification::UtxosChanged(notification.try_into()?),
        Payload::SinkBlueScoreChangedNotification(notification) => Notification::SinkBlueScoreChanged(notification.try_into()?),
        Payload::VirtualDaaScoreChangedNotification(notification) => {
            Notification::VirtualDaaScoreChanged(notification.try_into()?)
        }
        Payload::PruningPointUtxoSetOverrideNotification(notification) => {
            Notification::PruningPointUtxoSetOverride(notification.try_into()?)
        }
        _ => Err(RpcError::UnsupportedFeature)?,
    }
});

try_from!(item: &BlockAddedNotificationMessage, kaspa_rpc_core::BlockAddedNotification, {
    Self {
        block: Arc::new(
            item.block
                .as_ref()
                .ok_or_else(|| RpcError::MissingRpcFieldError("BlockAddedNotificationMessage".to_string(), "block".to_string()))?
                .try_into()?,
        ),
    }
});

try_from!(&NewBlockTemplateNotificationMessage, kaspa_rpc_core::NewBlockTemplateNotification);

try_from!(item: &PalwClassReadinessChangedNotificationMessage, kaspa_rpc_core::PalwClassReadinessChangedNotification, {
    Self {
        class_id: item.class_id.clone(),
        model_name: item.model_name.clone(),
        registry_state: item.registry_state.clone(),
        previous_registry_state: item.previous_registry_state.clone(),
        ready_seats: item.ready_seats,
        previous_ready_seats: item.previous_ready_seats,
        required_ready_seats: item.required_ready_seats,
        bonded_seats: item.bonded_seats,
    }
});
try_from!(item: &PalwPanelAssignmentNotificationMessage, kaspa_rpc_core::PalwPanelAssignmentNotification, {
    Self {
        claim_id: item.claim_id.clone(),
        class_id: item.class_id.clone(),
        licensed_state: item.licensed_state.clone(),
        deadline_daa: item.deadline_daa,
        coverage_mask: item.coverage_mask,
        full_seat: item.full_seat.clone(),
        valid_receipt_seats: item.valid_receipt_seats,
        selected_panel_seats: item.selected_panel_seats,
        seats: item.seats.iter().map(kaspa_rpc_core::RpcPalwPanelAssignmentSeat::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &PalwPanelReceiptNotificationMessage, kaspa_rpc_core::PalwPanelReceiptNotification, {
    Self {
        claim_id: item.claim_id.clone(),
        class_id: item.class_id.clone(),
        coverage_mask: item.coverage_mask,
        previous_coverage_mask: item.previous_coverage_mask,
        valid_receipt_seats: item.valid_receipt_seats,
        previous_valid_receipt_seats: item.previous_valid_receipt_seats,
        selected_panel_seats: item.selected_panel_seats,
    }
});
try_from!(item: &PalwPanelEligibilityChangedNotificationMessage, kaspa_rpc_core::PalwPanelEligibilityChangedNotification, {
    Self {
        seat_id: item.seat_id.clone(),
        class_id: item.class_id.clone(),
        eligible: item.eligible,
        ready: item.ready,
        hold: item.hold.as_ref().map(kaspa_rpc_core::RpcPalwPanelHoldReason::try_from).transpose()?,
    }
});

try_from!(item: &VirtualChainChangedNotificationMessage, kaspa_rpc_core::VirtualChainChangedNotification, {
    Self {
        removed_chain_block_hashes: Arc::new(
            item.removed_chain_block_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        ),
        added_chain_block_hashes: Arc::new(
            item.added_chain_block_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        ),
        accepted_transaction_ids: Arc::new(item.accepted_transaction_ids.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?),
    }
});

try_from!(item: &FinalityConflictNotificationMessage, kaspa_rpc_core::FinalityConflictNotification, {
    Self { violating_block_hash: RpcHash::from_str(&item.violating_block_hash)? }
});

try_from!(item: &FinalityConflictResolvedNotificationMessage, kaspa_rpc_core::FinalityConflictResolvedNotification, {
    Self { finality_block_hash: RpcHash::from_str(&item.finality_block_hash)? }
});

try_from!(item: &UtxosChangedNotificationMessage, kaspa_rpc_core::UtxosChangedNotification, {
    Self {
        added: Arc::new(item.added.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?),
        removed: Arc::new(item.removed.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?),
    }
});

try_from!(item: &SinkBlueScoreChangedNotificationMessage, kaspa_rpc_core::SinkBlueScoreChangedNotification, {
    Self { sink_blue_score: item.sink_blue_score }
});

try_from!(item: &VirtualDaaScoreChangedNotificationMessage, kaspa_rpc_core::VirtualDaaScoreChangedNotification, {
    Self { virtual_daa_score: item.virtual_daa_score }
});

try_from!(&PruningPointUtxoSetOverrideNotificationMessage, kaspa_rpc_core::PruningPointUtxoSetOverrideNotification);

from!(item: RpcNotifyCommand, Command, {
    match item {
        RpcNotifyCommand::NotifyStart => Command::Start,
        RpcNotifyCommand::NotifyStop => Command::Stop,
    }
});

from!(item: NotifyUtxosChangedResponseMessage, StopNotifyingUtxosChangedResponseMessage, { Self { error: item.error } });

from!(item: NotifyPruningPointUtxoSetOverrideResponseMessage, StopNotifyingPruningPointUtxoSetOverrideResponseMessage, {
    Self { error: item.error }
});
