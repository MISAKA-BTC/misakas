//! Conversions of protowire messages from and to rpc core counterparts.
//!
//! Response payloads in protowire do always contain an error field and generally a set of
//! fields providing the requested data.
//!
//! Responses in rpc core are expressed as `RpcResult<XxxResponse>`, where `Xxx` is the called
//! RPC method.
//!
//! The general conversion convention from protowire to rpc core is to consider the error
//! field first and, if present, to return a matching Err(RpcError). If absent, try to
//! convert the set of data fields into a matching XxxResponse rpc core response and, on
//! success, return Ok(XxxResponse), otherwise return a conversion error.
//!
//! Conversely, the general conversion convention from rpc core to protowire, depending on
//! a provided RpcResult is to either convert the Ok(XxxResponse) into the matching set
//! of data fields and provide no error or provide no data fields but an error field in case
//! of Err(RpcError).
//!
//! The SubmitBlockResponse is a notable exception to this general rule.

use crate::protowire::{self, submit_block_response_message::RejectReason};
use kaspa_addresses::Address;
use kaspa_consensus_core::network::NetworkId;
use kaspa_core::debug;
use kaspa_notify::subscription::Command;
use kaspa_rpc_core::{
    RpcContextualPeerAddress, RpcDataVerbosityLevel, RpcError, RpcExtraData, RpcHash, RpcIpAddress, RpcNetworkType, RpcPeerAddress,
    RpcResult, SubmitBlockRejectReason, SubmitBlockReport,
};
use kaspa_utils::hex::*;
use std::{str::FromStr, sync::Arc};

macro_rules! from {
    // Response capture
    ($name:ident : RpcResult<&$from_type:ty>, $to_type:ty, $ctor:block) => {
        impl From<RpcResult<&$from_type>> for $to_type {
            fn from(item: RpcResult<&$from_type>) -> Self {
                match item {
                    Ok($name) => $ctor,
                    Err(err) => {
                        let mut message = Self::default();
                        message.error = Some(err.into());
                        message
                    }
                }
            }
        }
    };

    // Response without parameter capture
    (RpcResult<&$from_type:ty>, $to_type:ty) => {
        impl From<RpcResult<&$from_type>> for $to_type {
            fn from(item: RpcResult<&$from_type>) -> Self {
                Self { error: item.map_err(protowire::RpcError::from).err() }
            }
        }
    };

    // Request and other capture
    ($name:ident : $from_type:ty, $to_type:ty, $body:block) => {
        impl From<$from_type> for $to_type {
            fn from($name: $from_type) -> Self {
                $body
            }
        }
    };

    // Request and other without parameter capture
    ($from_type:ty, $to_type:ty) => {
        impl From<$from_type> for $to_type {
            fn from(_: $from_type) -> Self {
                Self {}
            }
        }
    };
}

macro_rules! try_from {
    // Response capture
    ($name:ident : $from_type:ty, RpcResult<$to_type:ty>, $ctor:block) => {
        impl TryFrom<$from_type> for $to_type {
            type Error = RpcError;
            fn try_from($name: $from_type) -> RpcResult<Self> {
                if let Some(ref err) = $name.error {
                    Err(err.into())
                } else {
                    #[allow(unreachable_code)] // TODO: remove attribute when all converters are implemented
                    Ok($ctor)
                }
            }
        }
    };

    // Response without parameter capture
    ($from_type:ty, RpcResult<$to_type:ty>) => {
        impl TryFrom<$from_type> for $to_type {
            type Error = RpcError;
            fn try_from(item: $from_type) -> RpcResult<Self> {
                item.error.as_ref().map_or(Ok(Self {}), |x| Err(x.into()))
            }
        }
    };

    // Request and other capture
    ($name:ident : $from_type:ty, $to_type:ty, $body:block) => {
        impl TryFrom<$from_type> for $to_type {
            type Error = RpcError;
            fn try_from($name: $from_type) -> RpcResult<Self> {
                #[allow(unreachable_code)] // TODO: remove attribute when all converters are implemented
                Ok($body)
            }
        }
    };

    // Request and other without parameter capture
    ($from_type:ty, $to_type:ty) => {
        impl TryFrom<$from_type> for $to_type {
            type Error = RpcError;
            fn try_from(_: $from_type) -> RpcResult<Self> {
                Ok(Self {})
            }
        }
    };
}

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::SubmitBlockReport, RejectReason, {
    match item {
        kaspa_rpc_core::SubmitBlockReport::Success => RejectReason::None,
        kaspa_rpc_core::SubmitBlockReport::Reject(kaspa_rpc_core::SubmitBlockRejectReason::BlockInvalid) => RejectReason::BlockInvalid,
        kaspa_rpc_core::SubmitBlockReport::Reject(kaspa_rpc_core::SubmitBlockRejectReason::IsInIBD) => RejectReason::IsInIbd,
        // The conversion of RouteIsFull falls back to None since there exist no such variant in the original protowire version
        // and we do not want to break backwards compatibility
        kaspa_rpc_core::SubmitBlockReport::Reject(kaspa_rpc_core::SubmitBlockRejectReason::RouteIsFull) => RejectReason::None,
    }
});

from!(item: &kaspa_rpc_core::SubmitBlockRequest, protowire::SubmitBlockRequestMessage, {
    Self { block: Some((&item.block).into()), allow_non_daa_blocks: item.allow_non_daa_blocks }
});
// This conversion breaks the general conversion convention (see file header) since the message may
// contain both a non default reject_reason and a matching error message. In the RouteIsFull case
// reject_reason is None (because this reason has no variant in protowire) but a specific error
// message is provided.
from!(item: RpcResult<&kaspa_rpc_core::SubmitBlockResponse>, protowire::SubmitBlockResponseMessage, {
    let error: Option<protowire::RpcError> = match item.report {
        kaspa_rpc_core::SubmitBlockReport::Success => None,
        kaspa_rpc_core::SubmitBlockReport::Reject(reason) => Some(RpcError::SubmitBlockError(reason).into())
    };
    Self { reject_reason: RejectReason::from(&item.report) as i32, error }
});

from!(item: &kaspa_rpc_core::GetBlockTemplateRequest, protowire::GetBlockTemplateRequestMessage, {
    Self {
        pay_address: (&item.pay_address).into(),
        extra_data: String::from_utf8(item.extra_data.clone()).expect("extra data has to be valid UTF-8"),
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBlockTemplateResponse>, protowire::GetBlockTemplateResponseMessage, {
    Self { block: Some((&item.block).into()), is_synced: item.is_synced, error: None }
});

from!(item: &kaspa_rpc_core::GetBlockRequest, protowire::GetBlockRequestMessage, {
    Self { hash: item.hash.to_string(), include_transactions: item.include_transactions }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBlockResponse>, protowire::GetBlockResponseMessage, {
    Self { block: Some((&item.block).into()), error: None }
});

from!(item: &kaspa_rpc_core::NotifyBlockAddedRequest, protowire::NotifyBlockAddedRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyBlockAddedResponse>, protowire::NotifyBlockAddedResponseMessage);

from!(&kaspa_rpc_core::GetInfoRequest, protowire::GetInfoRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetInfoResponse>, protowire::GetInfoResponseMessage, {
    Self {
        p2p_id: item.p2p_id.clone(),
        mempool_size: item.mempool_size,
        server_version: item.server_version.clone(),
        is_utxo_indexed: item.is_utxo_indexed,
        is_synced: item.is_synced,
        has_notify_command: item.has_notify_command,
        has_message_id: item.has_message_id,
        error: None,
    }
});

from!(item: &kaspa_rpc_core::NotifyNewBlockTemplateRequest, protowire::NotifyNewBlockTemplateRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyNewBlockTemplateResponse>, protowire::NotifyNewBlockTemplateResponseMessage);

from!(item: &kaspa_rpc_core::NotifyPalwClassReadinessChangedRequest, protowire::NotifyPalwClassReadinessChangedRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyPalwClassReadinessChangedResponse>, protowire::NotifyPalwClassReadinessChangedResponseMessage);
from!(item: &kaspa_rpc_core::NotifyPalwPanelAssignmentRequest, protowire::NotifyPalwPanelAssignmentRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyPalwPanelAssignmentResponse>, protowire::NotifyPalwPanelAssignmentResponseMessage);
from!(item: &kaspa_rpc_core::NotifyPalwPanelReceiptRequest, protowire::NotifyPalwPanelReceiptRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyPalwPanelReceiptResponse>, protowire::NotifyPalwPanelReceiptResponseMessage);
from!(item: &kaspa_rpc_core::NotifyPalwPanelEligibilityChangedRequest, protowire::NotifyPalwPanelEligibilityChangedRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyPalwPanelEligibilityChangedResponse>, protowire::NotifyPalwPanelEligibilityChangedResponseMessage);

// ~~~

from!(&kaspa_rpc_core::GetCurrentNetworkRequest, protowire::GetCurrentNetworkRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetCurrentNetworkResponse>, protowire::GetCurrentNetworkResponseMessage, {
    Self { current_network: item.network.to_string(), error: None }
});

from!(&kaspa_rpc_core::GetPeerAddressesRequest, protowire::GetPeerAddressesRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetPeerAddressesResponse>, protowire::GetPeerAddressesResponseMessage, {
    Self {
        addresses: item.known_addresses.iter().map(|x| x.into()).collect(),
        banned_addresses: item.banned_addresses.iter().map(|x| x.into()).collect(),
        error: None,
    }
});

from!(&kaspa_rpc_core::GetSinkRequest, protowire::GetSinkRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetSinkResponse>, protowire::GetSinkResponseMessage, {
    Self { sink: item.sink.to_string(), error: None }
});

from!(item: &kaspa_rpc_core::GetMempoolEntryRequest, protowire::GetMempoolEntryRequestMessage, {
    Self {
        tx_id: item.transaction_id.to_string(),
        include_orphan_pool: item.include_orphan_pool,
        filter_transaction_pool: item.filter_transaction_pool,
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetMempoolEntryResponse>, protowire::GetMempoolEntryResponseMessage, {
    Self { entry: Some((&item.mempool_entry).into()), error: None }
});

from!(item: &kaspa_rpc_core::GetMempoolEntriesRequest, protowire::GetMempoolEntriesRequestMessage, {
    Self { include_orphan_pool: item.include_orphan_pool, filter_transaction_pool: item.filter_transaction_pool }
});
from!(item: RpcResult<&kaspa_rpc_core::GetMempoolEntriesResponse>, protowire::GetMempoolEntriesResponseMessage, {
    Self { entries: item.mempool_entries.iter().map(|x| x.into()).collect(), error: None }
});

from!(&kaspa_rpc_core::GetConnectedPeerInfoRequest, protowire::GetConnectedPeerInfoRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetConnectedPeerInfoResponse>, protowire::GetConnectedPeerInfoResponseMessage, {
    Self { infos: item.peer_info.iter().map(|x| x.into()).collect(), error: None }
});

from!(item: &kaspa_rpc_core::AddPeerRequest, protowire::AddPeerRequestMessage, {
    Self { address: item.peer_address.to_string(), is_permanent: item.is_permanent }
});
from!(RpcResult<&kaspa_rpc_core::AddPeerResponse>, protowire::AddPeerResponseMessage);

from!(item: &kaspa_rpc_core::SubmitTransactionRequest, protowire::SubmitTransactionRequestMessage, {
    Self { transaction: Some((&item.transaction).into()), allow_orphan: item.allow_orphan }
});
from!(item: RpcResult<&kaspa_rpc_core::SubmitTransactionResponse>, protowire::SubmitTransactionResponseMessage, {
    Self { transaction_id: item.transaction_id.to_string(), error: None }
});

from!(item: &kaspa_rpc_core::SubmitTransactionReplacementRequest, protowire::SubmitTransactionReplacementRequestMessage, {
    Self { transaction: Some((&item.transaction).into()) }
});
from!(item: RpcResult<&kaspa_rpc_core::SubmitTransactionReplacementResponse>, protowire::SubmitTransactionReplacementResponseMessage, {
    Self { transaction_id: item.transaction_id.to_string(), replaced_transaction: Some((&item.replaced_transaction).into()), error: None }
});

from!(item: &kaspa_rpc_core::GetSubnetworkRequest, protowire::GetSubnetworkRequestMessage, {
    Self { subnetwork_id: item.subnetwork_id.to_string() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetSubnetworkResponse>, protowire::GetSubnetworkResponseMessage, {
    Self { gas_limit: item.gas_limit, error: None }
});

from!(item: &kaspa_rpc_core::GetVirtualChainFromBlockRequest, protowire::GetVirtualChainFromBlockRequestMessage, {
    Self { start_hash: item.start_hash.to_string(), include_accepted_transaction_ids: item.include_accepted_transaction_ids, min_confirmation_count: item.min_confirmation_count }
});
from!(item: RpcResult<&kaspa_rpc_core::GetVirtualChainFromBlockResponse>, protowire::GetVirtualChainFromBlockResponseMessage, {
    Self {
        removed_chain_block_hashes: item.removed_chain_block_hashes.iter().map(|x| x.to_string()).collect(),
        added_chain_block_hashes: item.added_chain_block_hashes.iter().map(|x| x.to_string()).collect(),
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| x.into()).collect(),
        error: None,
    }
});

from!(item: &kaspa_rpc_core::GetBlocksRequest, protowire::GetBlocksRequestMessage, {
    Self {
        low_hash: item.low_hash.map_or(Default::default(), |x| x.to_string()),
        include_blocks: item.include_blocks,
        include_transactions: item.include_transactions,
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBlocksResponse>, protowire::GetBlocksResponseMessage, {
    Self {
        block_hashes: item.block_hashes.iter().map(|x| x.to_string()).collect::<Vec<_>>(),
        blocks: item.blocks.iter().map(|x| x.into()).collect::<Vec<_>>(),
        error: None,
    }
});

from!(&kaspa_rpc_core::GetBlockCountRequest, protowire::GetBlockCountRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetBlockCountResponse>, protowire::GetBlockCountResponseMessage, {
    Self { block_count: item.block_count, header_count: item.header_count, error: None }
});

from!(&kaspa_rpc_core::GetBlockDagInfoRequest, protowire::GetBlockDagInfoRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetBlockDagInfoResponse>, protowire::GetBlockDagInfoResponseMessage, {
    Self {
        network_name: item.network.to_prefixed(),
        block_count: item.block_count,
        header_count: item.header_count,
        tip_hashes: item.tip_hashes.iter().map(|x| x.to_string()).collect(),
        difficulty: item.difficulty,
        past_median_time: item.past_median_time as i64,
        virtual_parent_hashes: item.virtual_parent_hashes.iter().map(|x| x.to_string()).collect(),
        pruning_point_hash: item.pruning_point_hash.to_string(),
        virtual_daa_score: item.virtual_daa_score,
        sink: item.sink.to_string(),
        error: None,
    }
});

from!(item: &kaspa_rpc_core::ResolveFinalityConflictRequest, protowire::ResolveFinalityConflictRequestMessage, {
    Self { finality_block_hash: item.finality_block_hash.to_string() }
});
from!(_item: RpcResult<&kaspa_rpc_core::ResolveFinalityConflictResponse>, protowire::ResolveFinalityConflictResponseMessage, {
    Self { error: None }
});

from!(&kaspa_rpc_core::ShutdownRequest, protowire::ShutdownRequestMessage);
from!(RpcResult<&kaspa_rpc_core::ShutdownResponse>, protowire::ShutdownResponseMessage);

from!(item: &kaspa_rpc_core::GetHeadersRequest, protowire::GetHeadersRequestMessage, {
    Self { start_hash: item.start_hash.to_string(), limit: item.limit, is_ascending: item.is_ascending }
});
from!(item: RpcResult<&kaspa_rpc_core::GetHeadersResponse>, protowire::GetHeadersResponseMessage, {
    Self { headers: item.headers.iter().map(|x| x.hash.to_string()).collect(), error: None }
});

from!(item: &kaspa_rpc_core::GetUtxosByAddressesRequest, protowire::GetUtxosByAddressesRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetUtxosByAddressesResponse>, protowire::GetUtxosByAddressesResponseMessage, {
    debug!("GRPC, Creating GetUtxosByAddresses message with {} entries", item.entries.len());
    Self { entries: item.entries.iter().map(|x| x.into()).collect(), error: None }
});

from!(item: &kaspa_rpc_core::GetUtxosByAddressPageRequest, protowire::GetUtxosByAddressPageRequestMessage, {
    Self { address: (&item.address).into(), cursor: item.cursor.clone(), limit: item.limit }
});
from!(item: RpcResult<&kaspa_rpc_core::GetUtxosByAddressPageResponse>, protowire::GetUtxosByAddressPageResponseMessage, {
    debug!("GRPC, Creating GetUtxosByAddressPage message with {} entries", item.entries.len());
    Self { entries: item.entries.iter().map(|x| x.into()).collect(), next_cursor: item.next_cursor.clone(), error: None }
});

from!(item: &kaspa_rpc_core::GetBalanceByAddressRequest, protowire::GetBalanceByAddressRequestMessage, {
    Self { address: (&item.address).into() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBalanceByAddressResponse>, protowire::GetBalanceByAddressResponseMessage, {
    debug!("GRPC, Creating GetBalanceByAddress messages");
    Self { balance: item.balance, error: None }
});

from!(item: &kaspa_rpc_core::GetBalancesByAddressesRequest, protowire::GetBalancesByAddressesRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetBalancesByAddressesResponse>, protowire::GetBalancesByAddressesResponseMessage, {
    debug!("GRPC, Creating GetUtxosByAddresses message with {} entries", item.entries.len());
    Self { entries: item.entries.iter().map(|x| x.into()).collect(), error: None }
});

from!(&kaspa_rpc_core::GetSinkBlueScoreRequest, protowire::GetSinkBlueScoreRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetSinkBlueScoreResponse>, protowire::GetSinkBlueScoreResponseMessage, {
    Self { blue_score: item.blue_score, error: None }
});

from!(item: &kaspa_rpc_core::GetDnsConfirmationRequest, protowire::GetDnsConfirmationRequestMessage, { Self { block_hash: item.block_hash.clone() } });
from!(item: &kaspa_rpc_core::GetTokenLedgerEntryRequest, protowire::GetTokenLedgerEntryRequestMessage, {
    Self { asset_id: item.asset_id, owner: item.owner.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetTokenLedgerEntryResponse>, protowire::GetTokenLedgerEntryResponseMessage, {
    Self { available: item.available, balance: item.balance.clone(), nonce: item.nonce, error: None }
});
from!(item: &kaspa_rpc_core::GetPalwProducerFactsRequest, protowire::GetPalwProducerFactsRequestMessage, {
    Self {
        class_id: item.class_id.clone(),
        bond_transaction_id: item.bond_transaction_id.clone(),
        bond_index: item.bond_index,
        with_bond: item.with_bond,
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwProducerFactsResponse>, protowire::GetPalwProducerFactsResponseMessage, {
    Self {
        available: item.available,
        chain_point: item.chain_point.clone(),
        daa_score: item.daa_score,
        class_id: item.class_id.clone(),
        artifact_root: item.artifact_root.clone(),
        class_target: item.class_target.clone(),
        pwu: item.pwu,
        is_base_class: item.is_base_class,
        min_trace_retention_daa: item.min_trace_retention_daa,
        epoch_index: item.epoch_index,
        epoch_budget_blocks: item.epoch_budget_blocks,
        epoch_produced_blocks: item.epoch_produced_blocks,
        bond_known: item.bond_known,
        bond_registered_pubkey: item.bond_registered_pubkey.clone(),
        bond_operator_id: item.bond_operator_id.clone(),
        bond_collateral: item.bond_collateral,
        bond_reserved_exposure: item.bond_reserved_exposure.clone(),
        bond_exposure_ceiling: item.bond_exposure_ceiling.clone(),
        bond_claim_exposure: item.bond_claim_exposure.clone(),
        not_ready_reason: item.not_ready_reason.clone(),
        locked_bond_outpoints: item.locked_bond_outpoints.clone(),
        fp_certified: item.fp_certified,
        fp_quanta_per_canonical_job: item.fp_quanta_per_canonical_job,
        fp_max_quanta_per_receipt: item.fp_max_quanta_per_receipt,
        fp_decode_rules_armed: item.fp_decode_rules_armed,
        palw_retention_dir: item.palw_retention_dir.clone(),
        panel_da_armed: item.panel_da_armed,
        prompt_ids_merkle: item.prompt_ids_merkle,
        fp_decode_constraint_armed: item.fp_decode_constraint_armed,
        class_prompt_ids_merkle: item.class_prompt_ids_merkle,
        bond_committed: item.bond_committed.clone(),
        bond_producer_floor_shortfall: item.bond_producer_floor_shortfall,
        bond_accuser_exposure: item.bond_accuser_exposure.clone(),
        error: None,
    }
});
// ADR-0078 Decision 5 — the consumer's read path. The row's `transformer_id` is the state table's
// KEY and the row does not repeat it, so it is carried explicitly on both sides of this wire; drop
// it and a verifier receives a `dsl_hash` with no name for the function that made it.
from!(item: &kaspa_rpc_core::GetPalwDerivedArtifactsRequest, protowire::GetPalwDerivedArtifactsRequestMessage, {
    Self { claim_id: item.claim_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwDerivedArtifactsResponse>, protowire::GetPalwDerivedArtifactsResponseMessage, {
    Self {
        found: item.found,
        claim_id: item.claim_id.clone(),
        output_root: item.output_root.clone(),
        executor_pubkey: item.executor_pubkey.clone(),
        executor_bond: item.executor_bond.clone(),
        class_id: item.class_id.clone(),
        claim_phase: item.claim_phase.clone(),
        claim_void_reason: item.claim_void_reason.clone(),
        claim_accepted_block: item.claim_accepted_block.clone(),
        claim_accepted_daa: item.claim_accepted_daa,
        artifacts: item
            .artifacts
            .iter()
            .map(|a| protowire::RpcPalwDerivedArtifactMessage {
                transformer_id: a.transformer_id.clone(),
                derived_id: a.derived_id.clone(),
                grammar_id: a.grammar_id.clone(),
                kind: a.kind,
                kind_name: a.kind_name.clone(),
                dsl_hash: a.dsl_hash.clone(),
                artifact_hash: a.artifact_hash.clone(),
                artifact_bytes: a.artifact_bytes,
                accepted_daa: a.accepted_daa,
            })
            .collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwFreePromptClaimRequest, protowire::GetPalwFreePromptClaimRequestMessage, {
    Self { claim_id: item.claim_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwFreePromptClaimResponse>, protowire::GetPalwFreePromptClaimResponseMessage, {
    Self {
        found: item.found,
        claim_id: item.claim_id.clone(),
        is_free_prompt: item.is_free_prompt,
        class_id: item.class_id.clone(),
        executor_pubkey: item.executor_pubkey.clone(),
        executor_bond: item.executor_bond.clone(),
        output_root: item.output_root.clone(),
        trace_root: item.trace_root.clone(),
        execution_root: item.execution_root.clone(),
        work_leaves: item.work_leaves,
        work_id: item.work_id.clone(),
        quanta: item.quanta,
        quanta_spent: item.quanta_spent,
        phase: item.phase.clone(),
        void_reason: item.void_reason.clone(),
        phase_daa: item.phase_daa,
        accepted_block: item.accepted_block.clone(),
        accepted_daa: item.accepted_daa,
        trace_retention_daa: item.trace_retention_daa,
        derived_count: item.derived_count,
        error: None,
    }
});
// ADR-0080 design A — the chain's own account of a declared close, mid-assembly. `present` crosses
// as the bitmap it is: chunks arrive in any order, so a count of parts is not a resume point.
from!(item: &kaspa_rpc_core::GetPalwPendingChunkGroupRequest, protowire::GetPalwPendingChunkGroupRequestMessage, {
    Self { session_id: item.session_id.clone(), side: item.side.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwPendingChunkGroupResponse>, protowire::GetPalwPendingChunkGroupResponseMessage, {
    Self {
        found: item.found,
        session_id: item.session_id.clone(),
        side: item.side.clone(),
        count: item.count,
        present: item.present,
        parts_present: item.parts_present,
        complete: item.complete,
        declared_daa: item.declared_daa,
        assembly_deadline_daa: item.assembly_deadline_daa,
        close_digest: item.close_digest.clone(),
        verdict: item.verdict.clone(),
        declarer_bond: item.declarer_bond.clone(),
        deposit: item.deposit,
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwModelMarketRequest, protowire::GetPalwModelMarketRequestMessage, {
    Self { line_id: item.line_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelMarketResponse>, protowire::GetPalwModelMarketResponseMessage, {
    Self {
        found: item.found,
        line_id: item.line_id.clone(),
        opened: item.opened,
        opened_daa: item.opened_daa,
        msk_reserve: item.msk_reserve,
        position_units: item.position_units,
        sold_units: item.sold_units,
        burned_sompi: item.burned_sompi,
        registrant_paid_sompi: item.registrant_paid_sompi,
        closed_to_buys: item.closed_to_buys,
        price_sompi_per_position: item.price_sompi_per_position,
        supply_units: item.supply_units,
        virtual_sompi: item.virtual_sompi,
        class_status: item.class_status.clone(),
        contributor_paid_sompi: item.contributor_paid_sompi,
        seed_sompi: item.seed_sompi,
        seeded_by: item.seeded_by.clone(),
        seed_min_sompi: item.seed_min_sompi,
        seed_pledged_sompi: item.seed_pledged_sompi,
        buyback_sompi: item.buyback_sompi,
        retired_units: item.retired_units,
        burn_permille: item.burn_permille,
        leg_permille: item.leg_permille,
        leg_v2_activation_daa: item.leg_v2_activation_daa,
        class_lifecycle: item.class_lifecycle.clone(),
        market_refusal: item.market_refusal.clone(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwModelPositionsRequest, protowire::GetPalwModelPositionsRequestMessage, {
    Self { holder: item.holder.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelPositionsResponse>, protowire::GetPalwModelPositionsResponseMessage, {
    Self {
        holder: item.holder.clone(),
        positions: item
            .positions
            .iter()
            .map(|p| protowire::RpcPalwModelPosition {
                line_id: p.line_id.clone(),
                units: p.units,
                holding_since_daa: p.holding_since_daa,
                tenure_daa: p.tenure_daa,
                tier_index: p.tier_index,
                tier: p.tier.as_ref().map(protowire::RpcPalwModelBenefitTier::from),
            })
            .collect(),
        tip_daa: item.tip_daa,
        tip_hash: item.tip_hash.clone(),
        error: None,
    }
});
// ---- ADR-0088 Decision 12: the model registry ----
from!(item: &kaspa_rpc_core::RpcPalwModelLine, protowire::RpcPalwModelLine, {
    Self {
        line_id: item.line_id.clone(),
        class_id: item.class_id.clone(),
        has_row: item.has_row,
        owner: item.owner.as_ref().map(protowire::RpcOutpoint::from),
        owner_payout_payload: item.owner_payout_payload.clone(),
        developer: item.developer.as_ref().map(protowire::RpcOutpoint::from),
        developer_payout_payload: item.developer_payout_payload.clone(),
        maintainer: item.maintainer.as_ref().map(protowire::RpcOutpoint::from),
        maintainer_payout_payload: item.maintainer_payout_payload.clone(),
        name: item.name.clone(),
        name_hex: item.name_hex.clone(),
        founded_daa: item.founded_daa,
        current: item.current,
        previews: item.previews.clone(),
        versions_published: item.versions_published,
        contributor_permille_of_leg: item.contributor_permille_of_leg,
        status: item.status.clone(),
        retired_daa: item.retired_daa,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwModelVersion, protowire::RpcPalwModelVersion, {
    Self {
        line_id: item.line_id.clone(),
        version: item.version,
        root: item.root.clone(),
        parent: item.parent,
        adopted_from: item.adopted_from.clone(),
        runtime_hash: item.runtime_hash.clone(),
        dataset_commitment: item.dataset_commitment.clone(),
        training_config_hash: item.training_config_hash.clone(),
        notes_hash: item.notes_hash.clone(),
        published_daa: item.published_daa,
        published_by: item.published_by.as_ref().map(protowire::RpcOutpoint::from),
        status: item.status.clone(),
        until_daa: item.until_daa,
        in_force: item.in_force,
        attempt_claims: item.attempt_claims,
        fp_claims: item.fp_claims,
        work_leaves: item.work_leaves.clone(),
        first_used_daa: item.first_used_daa,
        last_used_daa: item.last_used_daa,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwModelEvaluation, protowire::RpcPalwModelEvaluation, {
    Self {
        evaluator_id: item.evaluator_id.clone(),
        score_permille: item.score_permille,
        report_hash: item.report_hash.clone(),
        posted_daa: item.posted_daa,
        by: Some(protowire::RpcOutpoint::from(&item.by)),
        is_lines_own: item.is_lines_own,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwModelProposal, protowire::RpcPalwModelProposal, {
    Self {
        proposal_id: item.proposal_id.clone(),
        line_id: item.line_id.clone(),
        root: item.root.clone(),
        note_hash: item.note_hash.clone(),
        by: Some(protowire::RpcOutpoint::from(&item.by)),
        posted_daa: item.posted_daa,
        adopted_in: item.adopted_in,
    }
});
from!(item: &kaspa_rpc_core::GetPalwModelLineRequest, protowire::GetPalwModelLineRequestMessage, {
    Self { line_id: item.line_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelLineResponse>, protowire::GetPalwModelLineResponseMessage, {
    Self {
        exists: item.exists,
        line_id: item.line_id.clone(),
        line: item.line.as_ref().map(protowire::RpcPalwModelLine::from),
        current_root: item.current_root.clone(),
        roots_in_force: item.roots_in_force.clone(),
        tip_daa: item.tip_daa,
        benefits: item.benefits.as_ref().map(protowire::RpcPalwModelBenefits::from),
        service_facts: Some((&item.service_facts).into()),
        error: None,
    }
});

from!(item: &kaspa_rpc_core::RpcPalwLineServiceFacts, protowire::RpcPalwLineServiceFacts, {
    Self { declared_grants: item.declared_grants, roots: item.roots.clone(), origin_pubkeys: item.origin_pubkeys.clone() }
});

from!(item: &kaspa_rpc_core::RpcPalwModelBenefitTier, protowire::RpcPalwModelBenefitTier, {
    Self {
        min_units: item.min_units,
        grants: item.grants,
        grant_names: item.grant_names.clone(),
        lead_daa: item.lead_daa,
        min_hold_daa: item.min_hold_daa,
        note: item.note.clone(),
    }
});

from!(item: &kaspa_rpc_core::RpcPalwModelBenefits, protowire::RpcPalwModelBenefits, {
    Self {
        tiers: item.tiers.iter().map(protowire::RpcPalwModelBenefitTier::from).collect(),
        pending_tiers: item.pending_tiers.iter().map(protowire::RpcPalwModelBenefitTier::from).collect(),
        pending_effective_daa: item.pending_effective_daa,
        cadence_daa: item.cadence_daa,
        expires_daa: item.expires_daa,
        declared_daa: item.declared_daa,
        lapsed: item.lapsed.clone(),
        lapse_daa: item.lapse_daa,
        enforced_lead_daa: item.enforced_lead_daa,
    }
});
from!(item: &kaspa_rpc_core::GetPalwModelVersionRequest, protowire::GetPalwModelVersionRequestMessage, {
    Self { line_id: item.line_id.clone(), version: item.version }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelVersionResponse>, protowire::GetPalwModelVersionResponseMessage, {
    Self {
        exists: item.exists,
        line_id: item.line_id.clone(),
        version_number: item.version_number,
        version: item.version.as_ref().map(protowire::RpcPalwModelVersion::from),
        evaluations: item.evaluations.iter().map(protowire::RpcPalwModelEvaluation::from).collect(),
        tip_daa: item.tip_daa,
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwModelLinesRequest, protowire::GetPalwModelLinesRequestMessage, {
    Self { class_id: item.class_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelLinesResponse>, protowire::GetPalwModelLinesResponseMessage, {
    Self {
        exists: item.exists,
        class_id: item.class_id.clone(),
        lines: item.lines.iter().map(protowire::RpcPalwModelLine::from).collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwModelProposalsRequest, protowire::GetPalwModelProposalsRequestMessage, {
    Self { line_id: item.line_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelProposalsResponse>, protowire::GetPalwModelProposalsResponseMessage, {
    Self {
        exists: item.exists,
        line_id: item.line_id.clone(),
        proposals: item.proposals.iter().map(protowire::RpcPalwModelProposal::from).collect(),
        error: None,
    }
});
// ADR-0122 §6.5
from!(item: &kaspa_rpc_core::RpcPalwClaimRow, protowire::RpcPalwClaimRow, {
    Self {
        claim_id: item.claim_id.clone(),
        is_free_prompt: item.is_free_prompt,
        class_id: item.class_id.clone(),
        executor_bond: item.executor_bond.clone(),
        phase: item.phase.clone(),
        void_reason: item.void_reason.clone(),
        phase_daa: item.phase_daa,
        accepted_daa: item.accepted_daa,
        accepted_block: item.accepted_block.clone(),
        rebound_daa: item.rebound_daa,
        bound_daa: item.bound_daa,
        seats: item.seats.clone(),
        deadline_daa: item.deadline_daa,
        reserved_sompi: item.reserved_sompi.clone(),
        escrow_sompi: item.escrow_sompi,
        payout_pending_sompi: item.payout_pending_sompi,
        quanta: item.quanta,
        quanta_spent: item.quanta_spent,
        work_leaves: item.work_leaves,
        open_courts: item.open_courts,
        exec_stage: item.exec_stage.clone(),
        exec_credit: item.exec_credit,
        exec_span: item.exec_span,
        exec_tickets: item.exec_tickets,
        exec_tickets_spent: item.exec_tickets_spent,
        exec_first_round: item.exec_first_round,
        exec_last_round: item.exec_last_round,
        vesting_stage: item.vesting_stage.clone(),
        vesting_sompi: item.vesting_sompi,
        vesting_payee_sompi: item.vesting_payee_sompi,
        vesting_expiry_daa: item.vesting_expiry_daa,
        vesting_licences_since_final: item.vesting_licences_since_final,
        vesting_licences_needed: item.vesting_licences_needed,
        vesting_matured_at: item.vesting_matured_at,
        vesting_eta_daa: item.vesting_eta_daa,
        vesting_eta_estimated: item.vesting_eta_estimated,
    }
});
from!(item: &kaspa_rpc_core::GetPalwClaimsRequest, protowire::GetPalwClaimsRequestMessage, {
    Self { bond: item.bond.clone(), role: item.role.clone(), include_terminal: item.include_terminal, limit: item.limit }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwClaimsResponse>, protowire::GetPalwClaimsResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        bond: item.bond.clone(),
        role: item.role.clone(),
        claims: item.claims.iter().map(protowire::RpcPalwClaimRow::from).collect(),
        truncated: item.truncated,
        bond_known: item.bond_known,
        bond_pubkey: item.bond_pubkey.clone(),
        bond_retiring_since_daa: item.bond_retiring_since_daa,
        bond_collateral: item.bond_collateral,
        bond_slashed: item.bond_slashed,
        bond_registered_daa: item.bond_registered_daa,
        bond_capable_classes: item.bond_capable_classes.clone(),
        vesting_only_rows: item.vesting_only_rows.iter().map(protowire::RpcPalwClaimRow::from).collect(),
        vesting_only_truncated: item.vesting_only_truncated,
        error: None,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwClassRow, protowire::RpcPalwClassRow, {
    Self {
        class_id: item.class_id.clone(),
        is_base_class: item.is_base_class,
        status: item.status.clone(),
        share_permille: item.share_permille.map(u32::from),
        budget_blocks: item.budget_blocks,
        canonical_leaves: item.canonical_leaves,
        artifact_root: item.artifact_root.clone(),
        fp_certified: item.fp_certified,
        held: item.held,
        registered_daa: item.registered_daa,
    }
});
from!(&kaspa_rpc_core::GetPalwClassesRequest, protowire::GetPalwClassesRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetPalwClassesResponse>, protowire::GetPalwClassesResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        classes: item.classes.iter().map(protowire::RpcPalwClassRow::from).collect(),
        error: None,
    }
});
from!(&kaspa_rpc_core::GetPalwNodeStatusRequest, protowire::GetPalwNodeStatusRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetPalwNodeStatusResponse>, protowire::GetPalwNodeStatusResponseMessage, {
    Self {
        consensus_params_id: item.consensus_params_id.clone(),
        fence_schedule: item.fence_schedule.clone(),
        consensus_schedule_id: item.consensus_schedule_id.clone(),
        producer_state: item.producer_state.clone(),
        producer_reason: item.producer_reason.clone(),
        producer_since_unix: item.producer_since_unix,
        producer_bond: item.producer_bond.clone(),
        producer_class: item.producer_class.clone(),
        draws: item.draws,
        produced_blocks: item.produced_blocks,
        receipt_blocks: item.receipt_blocks,
        network_lost: item.network_lost,
        last_block: item.last_block.clone(),
        last_block_unix: item.last_block_unix,
        last_draw_unix: item.last_draw_unix,
        panel_running: item.panel_running,
        panel_submitter: item.panel_submitter,
        retention_dir: item.retention_dir.clone(),
        memory_share_bytes: item.memory_share_bytes,
        memory_headroom_bytes: item.memory_headroom_bytes,
        memory_reserved_bytes: item.memory_reserved_bytes,
        memory_available_bytes: item.memory_available_bytes,
        memory_bounded: item.memory_bounded,
        memory_holders: item.memory_holders.clone(),
        lane_window_blocks: item.lane_window_blocks,
        lane_work_blocks: item.lane_work_blocks,
        lane_heartbeat_blocks: item.lane_heartbeat_blocks,
        lane_last_work_daa: item.lane_last_work_daa,
        lane_mix: item.lane_mix.clone(),
        lane_alarm: item.lane_alarm.clone(),
        genesis_hash: item.genesis_hash.clone(),
        drill_salt_id: item.drill_salt_id.clone(),
        error: None,
    }
});
from!(&kaspa_rpc_core::GetPalwRoundLaneRequest, protowire::GetPalwRoundLaneRequestMessage);
from!(item: &kaspa_rpc_core::RpcPalwRoundLaneStage, protowire::RpcPalwRoundLaneStage, {
    Self { activation_daa: item.activation_daa, permits_per_round: item.permits_per_round.into() }
});
from!(item: &kaspa_rpc_core::RpcPalwRoundPermit, protowire::RpcPalwRoundPermit, {
    Self {
        index: item.index.into(),
        bond: item.bond.clone(),
        operator_id: item.operator_id.clone(),
        domain: item.domain.clone(),
        used: item.used,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwRoundLaneDomain, protowire::RpcPalwRoundLaneDomain, {
    Self {
        domain: item.domain.clone(),
        credits: item.credits,
        quota_permille: item.quota_permille.into(),
        parity: item.parity.into(),
        bonds: item.bonds,
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwRoundLaneResponse>, protowire::GetPalwRoundLaneResponseMessage, {
    Self {
        armed: item.armed,
        open: item.open,
        schedule_span_daa: item.schedule_span_daa,
        max_per_mergeset: item.max_per_mergeset,
        stages: item.stages.iter().map(protowire::RpcPalwRoundLaneStage::from).collect(),
        virtual_daa: item.virtual_daa,
        round: item.round,
        span: item.span,
        permits_per_round: item.permits_per_round.into(),
        permits: item.permits.iter().map(protowire::RpcPalwRoundPermit::from).collect(),
        domains: item.domains.iter().map(protowire::RpcPalwRoundLaneDomain::from).collect(),
        accepted_in_span: item.accepted_in_span,
        finals_span: item.finals_span,
        finals: item.finals,
        next_round_permits: item.next_round_permits.into(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwSettlementRequest, protowire::GetPalwSettlementRequestMessage, {
    Self { daa_score: item.daa_score }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwSettlementResponse>, protowire::GetPalwSettlementResponseMessage, {
    Self {
        available: item.available,
        sink_daa: item.sink_daa,
        daa_score: item.daa_score,
        settled: item.settled,
        depth: item.depth,
        pending_anchors: item.pending_anchors,
        depth_is_lower_bound: item.depth_is_lower_bound,
        safe_frontier_blue_score: item.safe_frontier_blue_score,
        safe_frontier_daa: item.safe_frontier_daa,
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPrecommitDutyRequest, protowire::GetPrecommitDutyRequestMessage, {
    Self { validator_id: item.validator_id.clone(), bond_outpoint: item.bond_outpoint.clone() }
});
from!(item: &kaspa_rpc_core::RpcPrecommitDue, protowire::RpcPrecommitDue, {
    Self {
        epoch: item.epoch,
        anchor_hash: item.anchor_hash.clone(),
        anchor_daa_score: item.anchor_daa_score,
        snapshot_commitment: item.snapshot_commitment.clone(),
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPrecommitDutyResponse>, protowire::GetPrecommitDutyResponseMessage, {
    Self {
        available: item.available,
        round_active: item.round_active,
        sink_daa_score: item.sink_daa_score,
        held_epoch: item.held_epoch,
        held_anchor: item.held_anchor.clone(),
        due: item.due.iter().map(protowire::RpcPrecommitDue::from).collect(),
        error: None,
    }
});
from!(&kaspa_rpc_core::GetPalwClassContextsRequest, protowire::GetPalwClassContextsRequestMessage);
from!(item: &kaspa_rpc_core::RpcPalwClassContext, protowire::RpcPalwClassContext, {
    Self {
        class_id: item.class_id.clone(),
        model_id: item.model_id.clone(),
        n_ctx: item.n_ctx,
        canonical_prefill_tokens: item.canonical_prefill_tokens,
        canonical_decode_tokens: item.canonical_decode_tokens,
        canonical_footprint_positions: item.canonical_footprint_positions,
        max_context_tokens: item.max_context_tokens,
        source: item.source.clone(),
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwClassContextsResponse>, protowire::GetPalwClassContextsResponseMessage, {
    Self {
        available: item.available,
        fp_max_prompt_tokens: item.fp_max_prompt_tokens,
        fp_max_decode_tokens: item.fp_max_decode_tokens,
        classes: item.classes.iter().map(protowire::RpcPalwClassContext::from).collect(),
        error: None,
    }
});
from!(&kaspa_rpc_core::GetPalwClassEconomicsRequest, protowire::GetPalwClassEconomicsRequestMessage);
from!(item: &kaspa_rpc_core::RpcPalwClassLedgerTotals, protowire::RpcPalwClassLedgerTotals, {
    Self {
        available: item.available,
        claims: item.claims,
        bound: item.bound,
        licensed: item.licensed,
        finals: item.finals,
        voided: item.voided,
        redrawn: item.redrawn,
        paid_at_acceptance: item.paid_at_acceptance,
        escrow_final_sompi: item.escrow_final_sompi.clone(),
        producer_named_sompi: item.producer_named_sompi.clone(),
        panel_named_sompi: item.panel_named_sompi.clone(),
        reserve_sompi: item.reserve_sompi.clone(),
        burned_sompi: item.burned_sompi.clone(),
        attempted_compute: item.attempted_compute.clone(),
        final_compute: item.final_compute.clone(),
        verification_compute: item.verification_compute.clone(),
        producer_per_attempted_compute: item.producer_per_attempted_compute.clone(),
        panel_per_verification_compute: item.panel_per_verification_compute.clone(),
        total_per_attempted_compute: item.total_per_attempted_compute.clone(),
        total_per_final_compute: item.total_per_final_compute.clone(),
        licence_rate_permille: item.licence_rate_permille,
        final_of_licensed_permille: item.final_of_licensed_permille,
        final_rate_permille: item.final_rate_permille,
        avg_bind_wait_daa: item.avg_bind_wait_daa,
        avg_licence_wait_daa: item.avg_licence_wait_daa,
        avg_final_wait_daa: item.avg_final_wait_daa,
        avg_void_wait_daa: item.avg_void_wait_daa,
        avg_expected_attempts_q32: item.avg_expected_attempts_q32.clone(),
        avg_network_expected_attempts_q32: item.avg_network_expected_attempts_q32.clone(),
        first_accepted_daa: item.first_accepted_daa,
        last_accepted_daa: item.last_accepted_daa,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwClassNodeTelemetry, protowire::RpcPalwClassNodeTelemetry, {
    Self {
        available: item.available,
        draws: item.draws,
        class_wins: item.class_wins,
        produced: item.produced,
        draw_millis: item.draw_millis,
        storage_read_mib: item.storage_read_mib,
        replays: item.replays,
        replay_millis: item.replay_millis,
        replay_leaves: item.replay_leaves,
        receipts_valid: item.receipts_valid,
        receipts_unavailable: item.receipts_unavailable,
        receipts_incapable: item.receipts_incapable,
        receipts_other: item.receipts_other,
        openings_held: item.openings_held,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwClassEconomics, protowire::RpcPalwClassEconomics, {
    Self {
        class_id: item.class_id.clone(),
        model_id: item.model_id.clone(),
        is_base_class: item.is_base_class,
        status: item.status.clone(),
        share_permille: item.share_permille as u32,
        pwu_per_inference: item.pwu_per_inference,
        class_target: item.class_target.clone(),
        expected_attempts: item.expected_attempts,
        expected_attempts_q32: item.expected_attempts_q32.clone(),
        economic_compute_job: item.economic_compute_job.clone(),
        economic_compute_canonical: item.economic_compute_canonical.clone(),
        economic_source: item.economic_source.clone(),
        claims_accepted: item.claims_accepted,
        claims_provisional: item.claims_provisional,
        claims_panel_bound: item.claims_panel_bound,
        claims_licensed: item.claims_licensed,
        claims_final: item.claims_final,
        claims_voided: item.claims_voided,
        claims_redrawn: item.claims_redrawn,
        escrow_accepted_sompi: item.escrow_accepted_sompi.clone(),
        escrow_final_sompi: item.escrow_final_sompi.clone(),
        ledger: Some(protowire::RpcPalwClassLedgerTotals::from(&item.ledger)),
        telemetry: Some(protowire::RpcPalwClassNodeTelemetry::from(&item.telemetry)),
        eligible_seats: item.eligible_seats,
        duty_seats_inflight: item.duty_seats_inflight,
        seat_exposure_inflight_sompi: item.seat_exposure_inflight_sompi.clone(),
        free_collateral_sompi: item.free_collateral_sompi.clone(),
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwClassEconomicsResponse>, protowire::GetPalwClassEconomicsResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        economic_compute_version: item.economic_compute_version as u32,
        seat_count: item.seat_count as u32,
        prefill_draw: item.prefill_draw,
        classes: item.classes.iter().map(protowire::RpcPalwClassEconomics::from).collect(),
        network_bits: item.network_bits,
        network_expected_attempts_q32: item.network_expected_attempts_q32.clone(),
        ledger_available: item.ledger_available,
        ledger_claims: item.ledger_claims,
        ledger_first_daa: item.ledger_first_daa,
        ledger_last_daa: item.ledger_last_daa,
        error: None,
    }
});
from!(&kaspa_rpc_core::GetPalwModelRegistryRequest, protowire::GetPalwModelRegistryRequestMessage);
// ADR-0148: the free-prompt lane's price for one job.
from!(item: &kaspa_rpc_core::GetPalwFreePromptPriceRequest, protowire::GetPalwFreePromptPriceRequestMessage, {
    Self {
        class_id: item.class_id.clone(),
        prompt_token_ids: item.prompt_token_ids.clone(),
        prompt_tokens: item.prompt_tokens,
        decode_tokens_executed: item.decode_tokens_executed,
        work_leaves: item.work_leaves,
        bond: item.bond.clone(),
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwFreePromptPriceResponse>, protowire::GetPalwFreePromptPriceResponseMessage, {
    Self {
        available: item.available,
        daa_score: item.daa_score,
        priced: item.priced,
        refusal: item.refusal.clone(),
        priced_in_compute: item.priced_in_compute,
        quanta: item.quanta,
        pwu: item.pwu,
        reserved_sompi: item.reserved_sompi.clone(),
        bond_room_sompi: item.bond_room_sompi.clone(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwPanelHoldReason, protowire::RpcPalwPanelHoldReason, {
    Self { code: item.code.clone(), message: item.message.clone() }
});
from!(item: &kaspa_rpc_core::RpcPalwPanelHoldReasonCount, protowire::RpcPalwPanelHoldReasonCount, {
    Self { code: item.code.clone(), message: item.message.clone(), seats: item.seats }
});
from!(item: &kaspa_rpc_core::RpcPalwClassPanelStatus, protowire::RpcPalwClassPanelStatus, {
    Self {
        class_id: item.class_id.clone(),
        model_name: item.model_name.clone(),
        registry_state: item.registry_state.clone(),
        bonded_seats: item.bonded_seats,
        ready_seats: item.ready_seats,
        required_ready_seats: item.required_ready_seats,
        selected_panel_seats: item.selected_panel_seats,
        valid_receipt_seats: item.valid_receipt_seats,
        panel_size: item.panel_size as u32,
        receipt_quorum: item.receipt_quorum as u32,
        full_seats_per_panel: item.full_seats_per_panel as u32,
        partial_seats_per_panel: item.partial_seats_per_panel as u32,
        segment_count: item.segment_count as u32,
        inflight_claims: item.inflight_claims,
        active_assignments: item.active_assignments,
        admission_permille: item.admission_permille,
        verification_mode: item.verification_mode.clone(),
        s1_active: item.s1_active,
        s1_scheduled_daa: item.s1_scheduled_daa,
        s3_active: item.s3_active,
        s3_scheduled_daa: item.s3_scheduled_daa,
        s2_active: item.s2_active,
        s2_scheduled_daa: item.s2_scheduled_daa,
        holds_local: item.holds_local,
        missing: item.missing.iter().map(protowire::RpcPalwPanelHoldReasonCount::from).collect(),
    }
});
from!(item: &kaspa_rpc_core::RpcPalwPanelSeat, protowire::RpcPalwPanelSeat, {
    Self {
        seat_id: item.seat_id.clone(),
        bond_outpoint: item.bond_outpoint.clone(),
        class_id: item.class_id.clone(),
        ready: item.ready,
        eligible: item.eligible,
        readiness_version: item.readiness_version as u32,
        readiness_proved_daa: item.readiness_proved_daa,
        readiness_expires_daa: item.readiness_expires_daa,
        collateral_available: item.collateral_available.clone(),
        collateral_locked: item.collateral_locked.clone(),
        assigned: item.assigned,
        hold: item.hold.as_ref().map(protowire::RpcPalwPanelHoldReason::from),
    }
});
from!(item: &kaspa_rpc_core::RpcPalwPanelAssignmentSeat, protowire::RpcPalwPanelAssignmentSeat, {
    Self {
        seat_id: item.seat_id.clone(),
        seat_index: item.seat_index as u32,
        full_seat: item.full_seat,
        segment_index: item.segment_index.unwrap_or(0) as u32,
        has_segment_index: item.segment_index.is_some(),
        mask: item.mask,
        receipt_status: item.receipt_status.clone(),
        credited_daa: item.credited_daa,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwPanelAssignment, protowire::RpcPalwPanelAssignment, {
    Self {
        claim_id: item.claim_id.clone(),
        class_id: item.class_id.clone(),
        licensed_state: item.licensed_state.clone(),
        deadline_daa: item.deadline_daa,
        coverage_mask: item.coverage_mask,
        full_seat: item.full_seat.clone(),
        valid_receipt_seats: item.valid_receipt_seats,
        selected_panel_seats: item.selected_panel_seats,
        seats: item.seats.iter().map(protowire::RpcPalwPanelAssignmentSeat::from).collect(),
    }
});
from!(item: &kaspa_rpc_core::RpcPalwLocalPanelClass, protowire::RpcPalwLocalPanelClass, {
    Self {
        class_id: item.class_id.clone(),
        model_name: item.model_name.clone(),
        seat_id: item.seat_id.clone(),
        artifact_loaded: item.artifact_loaded,
        artifact_root: item.artifact_root.clone(),
        working_set_bytes: item.working_set_bytes,
        replay_capable: item.replay_capable,
        synced: item.synced,
        bond_active: item.bond_active,
        collateral_sompi: item.collateral_sompi,
        readiness_proof_accepted: item.readiness_proof_accepted,
        readiness_proved_daa: item.readiness_proved_daa,
        chain_state: item.chain_state.clone(),
        assignments: item.assignments,
        hold: item.hold.as_ref().map(protowire::RpcPalwPanelHoldReason::from),
        runtime_profile: item.runtime_profile.clone(),
        artifact_resident_bytes: item.artifact_resident_bytes,
        producer_working_set_bytes: item.producer_working_set_bytes,
        full_seat_working_set_bytes: item.full_seat_working_set_bytes,
        partial_seat_working_set_bytes: item.partial_seat_working_set_bytes,
        producer_capable: item.producer_capable,
        full_seat_capable: item.full_seat_capable,
        partial_seat_capable: item.partial_seat_capable,
    }
});
from!(item: &kaspa_rpc_core::GetPalwClassPanelStatusRequest, protowire::GetPalwClassPanelStatusRequestMessage, {
    Self { class_id: item.class_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwClassPanelStatusResponse>, protowire::GetPalwClassPanelStatusResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        found: item.found,
        status: Some((&item.status).into()),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwPanelSeatsRequest, protowire::GetPalwPanelSeatsRequestMessage, {
    Self { class_id: item.class_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwPanelSeatsResponse>, protowire::GetPalwPanelSeatsResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        seats: item.seats.iter().map(protowire::RpcPalwPanelSeat::from).collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwPanelStatusRequest, protowire::GetPalwPanelStatusRequestMessage, {
    Self { class_id: item.class_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwPanelStatusResponse>, protowire::GetPalwPanelStatusResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        panel_running: item.panel_running,
        panel_submitter: item.panel_submitter,
        synced: item.synced,
        classes: item.classes.iter().map(protowire::RpcPalwLocalPanelClass::from).collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwPanelAssignmentsRequest, protowire::GetPalwPanelAssignmentsRequestMessage, {
    Self { claim_id: item.claim_id.clone(), seat_id: item.seat_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwPanelAssignmentsResponse>, protowire::GetPalwPanelAssignmentsResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        truncated: item.truncated,
        assignments: item.assignments.iter().map(protowire::RpcPalwPanelAssignment::from).collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwModelPreflightCheck, protowire::RpcPalwModelPreflightCheck, {
    Self { code: item.code.clone(), ok: item.ok, message: item.message.clone() }
});
from!(item: &kaspa_rpc_core::RpcPalwModelRegistration, protowire::RpcPalwModelRegistration, {
    Self {
        object_id: item.object_id.clone(),
        class_id: item.class_id.clone(),
        constructed: item.constructed,
        submitted: item.submitted,
        accepted: item.accepted,
        included: item.included,
        folded: item.folded,
        submission_state: item.submission_state.clone(),
        processor_verdict: item.processor_verdict.clone(),
        reject_code: item.reject_code.clone(),
        mempool_accepted: item.mempool_accepted,
        included_block: item.included_block.clone(),
        included_daa: item.included_daa,
        registry_state: item.registry_state.clone(),
        transaction_id: item.transaction_id.clone(),
    }
});
from!(item: &kaspa_rpc_core::GetPalwModelPreflightRequest, protowire::GetPalwModelPreflightRequestMessage, {
    Self { object_hex: item.object_hex.clone(), class_id: item.class_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelPreflightResponse>, protowire::GetPalwModelPreflightResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        class_id: item.class_id.clone(),
        artifact_root: item.artifact_root.clone(),
        n_ctx: item.n_ctx,
        layer_count: item.layer_count,
        admissible: item.admissible,
        processor_verdict: item.processor_verdict.clone(),
        reject_code: item.reject_code.clone(),
        checks: item.checks.iter().map(protowire::RpcPalwModelPreflightCheck::from).collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::SubmitPalwModelRegistrationRequest, protowire::SubmitPalwModelRegistrationRequestMessage, {
    Self { object_hex: item.object_hex.clone(), transaction_id: item.transaction_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::SubmitPalwModelRegistrationResponse>, protowire::SubmitPalwModelRegistrationResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        registration: Some((&item.registration).into()),
        checks: item.checks.iter().map(protowire::RpcPalwModelPreflightCheck::from).collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwModelRegistrationStatusRequest, protowire::GetPalwModelRegistrationStatusRequestMessage, {
    Self { class_id: item.class_id.clone(), object_id: item.object_id.clone(), transaction_id: item.transaction_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelRegistrationStatusResponse>, protowire::GetPalwModelRegistrationStatusResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        found: item.found,
        registration: Some((&item.registration).into()),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwModelRequest, protowire::GetPalwModelRequestMessage, {
    Self { class_id: item.class_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelResponse>, protowire::GetPalwModelResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        found: item.found,
        class_id: item.class_id.clone(),
        model_name: item.model_name.clone(),
        n_ctx: item.n_ctx,
        artifact_root: item.artifact_root.clone(),
        class_status: item.class_status.clone(),
        registry_state: item.registry_state.clone(),
        ready_seats: item.ready_seats,
        required_ready_seats: item.required_ready_seats,
        inflight_claims: item.inflight_claims,
        admission_permille: item.admission_permille,
        share_permille: item.share_permille as u32,
        certified_family: item.certified_family.clone(),
        fence_active: item.fence_active,
        reason: item.reason.clone(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwModelSeatReadiness, protowire::RpcPalwModelSeatReadiness, {
    Self {
        seat_id: item.seat_id.clone(),
        bond_txid: item.bond_txid.clone(),
        bond_index: item.bond_index,
        proved_daa: item.proved_daa,
        proved_span: item.proved_span,
        expires_daa: item.expires_daa,
        fresh: item.fresh,
        ready: item.ready,
        collateral_sompi: item.collateral_sompi,
        needed_collateral_sompi: item.needed_collateral_sompi,
        not_ready_reason: item.not_ready_reason.clone(),
    }
});
from!(item: &kaspa_rpc_core::GetPalwModelReadinessRequest, protowire::GetPalwModelReadinessRequestMessage, {
    Self { class_id: item.class_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelReadinessResponse>, protowire::GetPalwModelReadinessResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        found: item.found,
        class_id: item.class_id.clone(),
        registry_state: item.registry_state.clone(),
        ready_seats: item.ready_seats,
        required_ready_seats: item.required_ready_seats,
        seats: item.seats.iter().map(protowire::RpcPalwModelSeatReadiness::from).collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwModelAdmissionRequest, protowire::GetPalwModelAdmissionRequestMessage, {
    Self { class_id: item.class_id.clone(), object_hex: item.object_hex.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelAdmissionResponse>, protowire::GetPalwModelAdmissionResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        class_id: item.class_id.clone(),
        admissible: item.admissible,
        processor_verdict: item.processor_verdict.clone(),
        reject_code: item.reject_code.clone(),
        checks: item.checks.iter().map(protowire::RpcPalwModelPreflightCheck::from).collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwModelCertifiedFamily, protowire::RpcPalwModelCertifiedFamily, {
    Self { lane: item.lane.clone(), digest: item.digest.clone(), covers: item.covers }
});
from!(item: &kaspa_rpc_core::GetPalwModelCertificationRequest, protowire::GetPalwModelCertificationRequestMessage, {
    Self { class_id: item.class_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelCertificationResponse>, protowire::GetPalwModelCertificationResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        found: item.found,
        class_id: item.class_id.clone(),
        end_to_end_certified: item.end_to_end_certified,
        families: item.families.iter().map(protowire::RpcPalwModelCertifiedFamily::from).collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwActivationPoolRequest, protowire::GetPalwActivationPoolRequestMessage, {
    Self { class_id: item.class_id.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwActivationPoolResponse>, protowire::GetPalwActivationPoolResponseMessage, {
    Self {
        available: item.available,
        pool_armed: item.pool_armed,
        tip_daa: item.tip_daa,
        class_found: item.class_found,
        class_id: item.class_id.clone(),
        class_status: item.class_status.clone(),
        lifecycle: item.lifecycle.clone(),
        has_pool: item.has_pool,
        prep_sompi: item.prep_sompi,
        bonus_sompi: item.bonus_sompi,
        funded_sompi: item.funded_sompi,
        paid_sompi: item.paid_sompi,
        withheld_sompi: item.withheld_sompi,
        opened_daa: item.opened_daa,
        prep_paid: item.prep_paid.clone(),
        bonus_paid: item.bonus_paid.clone(),
        probe_credited: item.probe_credited.clone(),
        registrant_operator: item.registrant_operator.clone(),
        prep_reward_now_sompi: item.prep_reward_now_sompi,
        prep_cap_now_sompi: item.prep_cap_now_sompi,
        next_audit_span: item.next_audit_span,
        span_daa: item.span_daa,
        sink_script: item.sink_script.clone(),
        min_topup_sompi: item.min_topup_sompi,
        prep_base_sompi: item.prep_base_sompi,
        prep_share_permille: item.prep_share_permille,
        bonus_share_permille: item.bonus_share_permille,
        ramp_daa: item.ramp_daa,
        prep_payee_cap: item.prep_payee_cap,
        bonus_payee_cap: item.bonus_payee_cap,
        total_funded_sompi: item.total_funded_sompi,
        total_paid_sompi: item.total_paid_sompi,
        total_withheld_sompi: item.total_withheld_sompi,
        total_available_sompi: item.total_available_sompi,
        scheduled_sompi: item.scheduled_sompi,
        class_is_floor: item.class_is_floor,
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetPalwVestingRequest, protowire::GetPalwVestingRequestMessage, {
    Self {
        bond: item.bond.clone(),
        payout_address: item.payout_address.clone(),
        claim_id: item.claim_id.clone(),
        limit: item.limit,
        after: item.after.clone(),
    }
});
from!(item: &kaspa_rpc_core::RpcPalwVestingLeg, protowire::RpcPalwVestingLeg, {
    Self {
        kind: item.kind.clone(),
        payee_bond: item.payee_bond.clone(),
        payload: item.payload.clone(),
        sompi: item.sompi,
        queue_key: item.queue_key.clone(),
    }
});
from!(item: &kaspa_rpc_core::RpcPalwVestingRow, protowire::RpcPalwVestingRow, {
    Self {
        claim_id: item.claim_id.clone(),
        class_id: item.class_id.clone(),
        producer_bond: item.producer_bond.clone(),
        licence_door: item.licence_door.clone(),
        basis_k: item.basis_k,
        escrow_sompi: item.escrow_sompi,
        buyback_bound_sompi: item.buyback_bound_sompi,
        total_sompi: item.total_sompi,
        reserve_sompi: item.reserve_sompi,
        final_daa: item.final_daa,
        expiry_daa: item.expiry_daa,
        settled_at_final: item.settled_at_final,
        matured_at: item.matured_at,
        stage: item.stage.clone(),
        daa_clock_met: item.daa_clock_met,
        licences_since_final: item.licences_since_final,
        licences_needed: item.licences_needed,
        second_clock_bound_daa: item.second_clock_bound_daa,
        da_session_open: item.da_session_open,
        mature_now: item.mature_now,
        lock_live: item.lock_live,
        moves_ahead: item.moves_ahead,
        keys_ahead: item.keys_ahead,
        in_next_block: item.in_next_block,
        eta_daa: item.eta_daa,
        eta_estimated: item.eta_estimated,
        legs: item.legs.iter().map(protowire::RpcPalwVestingLeg::from).collect(),
        legs_sompi: item.legs_sompi,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwReporterReward, protowire::RpcPalwReporterReward, {
    Self {
        offence_key: item.offence_key.clone(),
        stage: item.stage.clone(),
        reporter_bond: item.reporter_bond.clone(),
        payload: item.payload.clone(),
        sompi: item.sompi,
        reveal_until: item.reveal_until,
        in_next_block: item.in_next_block,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwVestingMove, protowire::RpcPalwVestingMove, {
    Self { source: item.source.clone(), id: item.id.clone(), legs: item.legs.iter().map(protowire::RpcPalwVestingLeg::from).collect() }
});
from!(item: &kaspa_rpc_core::RpcPalwVestingDoorCount, protowire::RpcPalwVestingDoorCount, {
    Self { door: item.door.clone(), rows: item.rows, latched_rows: item.latched_rows, sompi: item.sompi.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwVestingResponse>, protowire::GetPalwVestingResponseMessage, {
    Self {
        available: item.available,
        rcore_plus_active: item.rcore_plus_active,
        tip_daa: item.tip_daa,
        next_daa: item.next_daa,
        halted: item.halted,
        second_clock_depth: item.second_clock_depth,
        second_clock_escaped_depth: item.second_clock_escaped_depth,
        settled_anchors: item.settled_anchors,
        measured_ms_per_daa: item.measured_ms_per_daa,
        created_sompi: item.created_sompi.clone(),
        moved_sompi: item.moved_sompi.clone(),
        burned_sompi: item.burned_sompi.clone(),
        live_rows: item.live_rows,
        latched_rows: item.latched_rows,
        live_sompi: item.live_sompi.clone(),
        latched_sompi: item.latched_sompi.clone(),
        latched_behind_head: item.latched_behind_head,
        reporter_pending_rows: item.reporter_pending_rows,
        reporter_awarded_rows: item.reporter_awarded_rows,
        next_block_moves: item.next_block_moves.iter().map(protowire::RpcPalwVestingMove::from).collect(),
        next_block_legs: item.next_block_legs,
        next_block_new_keys: item.next_block_new_keys,
        next_block_stopped: item.next_block_stopped.clone(),
        next_block_stopped_at: item.next_block_stopped_at.clone(),
        backlog_keys: item.backlog_keys,
        backlog_blocks_est: item.backlog_blocks_est,
        licence_histogram: item.licence_histogram.iter().map(protowire::RpcPalwVestingDoorCount::from).collect(),
        bond: item.bond.clone(),
        payout_address: item.payout_address.clone(),
        claim_id: item.claim_id.clone(),
        claim_stage: item.claim_stage.clone(),
        bond_known: item.bond_known,
        payee_holds_collateral: item.payee_holds_collateral,
        lock_live_rows: item.lock_live_rows,
        lock_live_last_expiry_daa: item.lock_live_last_expiry_daa,
        rows: item.rows.iter().map(protowire::RpcPalwVestingRow::from).collect(),
        rows_total: item.rows_total,
        next_after: item.next_after.clone(),
        maturing_sompi: item.maturing_sompi.clone(),
        query_latched_sompi: item.query_latched_sompi.clone(),
        reporter_rewards: item.reporter_rewards.iter().map(protowire::RpcPalwReporterReward::from).collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwModelLifecycle, protowire::RpcPalwModelLifecycle, {
    Self {
        class_id: item.class_id.clone(),
        artifact_root: item.artifact_root.clone(),
        no_capable_panel_voids: item.no_capable_panel_voids,
        reason: item.reason.clone(),
        is_base_class: item.is_base_class,
        has_row: item.has_row,
        state: item.state.clone(),
        since_span: item.since_span,
        verification_ccu: item.verification_ccu.clone(),
        economic_ccu_per_claim: item.economic_ccu_per_claim.clone(),
        artifact_bytes: item.artifact_bytes,
        ops_supported: item.ops_supported,
        verification_window_spans: item.verification_window_spans,
        artifact_prefetch_spans: item.artifact_prefetch_spans,
        max_inflight_claims: item.max_inflight_claims,
        required_ready_seats: item.required_ready_seats,
        registration_bond_sompi: item.registration_bond_sompi,
        admission_claims_per_span_milli: item.admission_claims_per_span_milli,
        probes_passed: item.probes_passed,
        probes_failed: item.probes_failed,
        ready_seats: item.ready_seats,
        inflight_claims: item.inflight_claims,
        utilization_permille: item.utilization_permille,
        admission_milli: item.admission_milli,
        cap_utilization_permille: item.cap_utilization_permille,
        priced_share_permille: item.priced_share_permille as u32,
        work_ratio_permille: item.work_ratio_permille,
        expected_forwards_q32: item.expected_forwards_q32.clone(),
        work_ticket_target: item.work_ticket_target.clone(),
        class_target: item.class_target.clone(),
        panel_room: item.panel_room,
        final_work_share10_permille: item.final_work_share_10_permille as u32,
        final_work_share100_permille: item.final_work_share_100_permille as u32,
        ready_seats_now: item.ready_seats_now,
        inflight_now: item.inflight_now,
        share_permille: item.share_permille as u32,
    }
});
from!(item: &kaspa_rpc_core::RpcPalwSeatReadiness, protowire::RpcPalwSeatReadiness, {
    Self {
        bond_txid: item.bond_txid.clone(),
        bond_index: item.bond_index,
        class_id: item.class_id.clone(),
        proved_daa: item.proved_daa,
        proved_span: item.proved_span,
        leaf_index: item.leaf_index,
        fresh: item.fresh,
        not_ready_reason: item.not_ready_reason.clone(),
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwModelRegistryResponse>, protowire::GetPalwModelRegistryResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        scheduled: item.scheduled,
        fence_daa: item.fence_daa,
        active: item.active,
        grace_until_daa: item.grace_until_daa,
        span_daa: item.span_daa,
        reference_work_per_span: item.reference_work_per_span.clone(),
        reference_bytes_per_span: item.reference_bytes_per_span,
        seat_count: item.seat_count as u32,
        spare_seats: item.spare_seats as u32,
        utilization_permille: item.utilization_permille,
        probation_claims: item.probation_claims,
        stable_epochs: item.stable_epochs,
        readiness_probe_max_age_spans: item.readiness_probe_max_age_spans,
        readiness_collateral_multiple: item.readiness_collateral_multiple,
        classes: item.classes.iter().map(protowire::RpcPalwModelLifecycle::from).collect(),
        readiness: item.readiness.iter().map(protowire::RpcPalwSeatReadiness::from).collect(),
        classes_active: item.classes_active,
        classes_active_limited: item.classes_active_limited,
        classes_probation: item.classes_probation,
        classes_prefetching: item.classes_prefetching,
        classes_registered: item.classes_registered,
        classes_held: item.classes_held,
        bonds_active: item.bonds_active,
        bonds_with_headroom: item.bonds_with_headroom,
        work_target_shadow: item.work_target_shadow,
        work_target: item.work_target.clone(),
        work_floor: item.work_floor.clone(),
        work_network_draws_q32: item.work_network_draws_q32.clone(),
        work_effective: item.work_effective.clone(),
        work_epoch_index: item.work_epoch_index,
        work_closed_model_blocks: item.work_closed_model_blocks,
        work_closed_expected_blocks: item.work_closed_expected_blocks,
        work_rate_sompi_per_giga: item.work_rate_sompi_per_giga,
        panel_inflight_replay: item.panel_inflight_replay.clone(),
        panel_horizon_spans: item.panel_horizon_spans,
        final_work_epochs: item.final_work_epochs,
        error: None,
    }
});
from!(&kaspa_rpc_core::GetPalwRegistrationTermsRequest, protowire::GetPalwRegistrationTermsRequestMessage);
from!(item: &kaspa_rpc_core::RpcPalwCertifiedFamily, protowire::RpcPalwCertifiedFamily, {
    Self { lane: item.lane.clone(), digest: item.digest.clone(), certified_daa: item.certified_daa, family_hex: item.family_hex.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetPalwRegistrationTermsResponse>, protowire::GetPalwRegistrationTermsResponseMessage, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        base_class_id: item.base_class_id.clone(),
        min_grantable_share_permille: item.min_grantable_share_permille.into(),
        slash_value_per_pwu: item.slash_value_per_pwu,
        initial_target: item.initial_target.clone(),
        registered_class_ids: item.registered_class_ids.clone(),
        registered_artifact_roots: item.registered_artifact_roots.clone(),
        families: item.families.iter().map(protowire::RpcPalwCertifiedFamily::from).collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetTokenSupplyRequest, protowire::GetTokenSupplyRequestMessage, { Self { asset_id: item.asset_id } });
from!(item: RpcResult<&kaspa_rpc_core::GetTokenSupplyResponse>, protowire::GetTokenSupplyResponseMessage, {
    Self {
        available: item.available,
        minted: item.minted.clone(),
        burned: item.burned.clone(),
        circulating: item.circulating.clone(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetTokenEmissionInfoRequest, protowire::GetTokenEmissionInfoRequestMessage, {
    Self { epoch: item.epoch, latest: item.latest }
});
from!(item: RpcResult<&kaspa_rpc_core::GetTokenEmissionInfoResponse>, protowire::GetTokenEmissionInfoResponseMessage, {
    Self {
        available: item.available,
        epoch: item.epoch,
        settled: item.settled,
        budget: item.budget.clone(),
        network_compute: item.network_compute.clone(),
        paid_total: item.paid_total.clone(),
        audit_paid: item.audit_paid.clone(),
        reward_count: item.reward_count,
        settlement_root: item.settlement_root.clone(),
        next_settlement_epoch: item.next_settlement_epoch,
        fold_cursor: item.fold_cursor,
        error: None,
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetDnsConfirmationResponse>, protowire::GetDnsConfirmationResponseMessage, {
    Self {
        available: item.available,
        block_hash: item.block_hash.clone(),
        work_depth: item.work_depth.clone(),
        required_work_depth: item.required_work_depth.clone(),
        stake_depth: item.stake_depth.clone(),
        required_stake_depth: item.required_stake_depth.clone(),
        pow_confirmed: item.pow_confirmed,
        dns_confirmed: item.dns_confirmed,
        rollout_stage: item.rollout_stage,
        expected_dns_confirmation_seconds: item.expected_dns_confirmation_seconds,
        work_reorg_risk_upper_bound: item.work_reorg_risk_upper_bound.clone(),
        stake_reorg_risk_upper_bound: item.stake_reorg_risk_upper_bound.clone(),
        dns_reorg_risk_conservative_bound: item.dns_reorg_risk_conservative_bound.clone(),
        note: item.note.clone(),
        health: item.health,
        last_dns_confirmed_anchor: item.last_dns_confirmed_anchor.clone(),
        last_dns_confirmed_anchor_daa_score: item.last_dns_confirmed_anchor_daa_score,
        block_found: item.block_found,
        block_is_dns_final: item.block_is_dns_final,
        block_is_confirmed_anchor: item.block_is_confirmed_anchor,
        block_daa_score: item.block_daa_score,
        vlt_state: item.vlt_state.clone(),
        vlt_shadow_active: item.vlt_shadow_active,
        vlt_weight_fence_reached: item.vlt_weight_fence_reached,
        vlt_finality_active: item.vlt_finality_active,
        vlt_total_weight: item.vlt_total_weight.clone(),
        vlt_quorum_weight: item.vlt_quorum_weight.clone(),
        vlt_snapshot_epoch: item.vlt_snapshot_epoch,
        vlt_snapshot_root: item.vlt_snapshot_root.clone(),
        vlt_gauges_daa_score: item.vlt_gauges_daa_score,
        error: None,
    }
});

from!(item: &kaspa_rpc_core::SubmitEvmTransactionRequest, protowire::SubmitEvmTransactionRequestMessage, {
    Self { transaction: item.transaction.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::SubmitEvmTransactionResponse>, protowire::SubmitEvmTransactionResponseMessage, {
    Self { transaction_hash: item.transaction_hash.clone(), error: None }
});
from!(item: &kaspa_rpc_core::SubmitEvmDepositClaimRequest, protowire::SubmitEvmDepositClaimRequestMessage, {
    Self { transaction_id: item.transaction_id.clone(), index: item.index }
});
from!(item: RpcResult<&kaspa_rpc_core::SubmitEvmDepositClaimResponse>, protowire::SubmitEvmDepositClaimResponseMessage, {
    Self { evm_address: item.evm_address.clone(), amount_sompi: item.amount_sompi, claim_tip_sompi: item.claim_tip_sompi, error: None }
});

from!(item: &kaspa_rpc_core::GetEvmTransactionReceiptRequest, protowire::GetEvmTransactionReceiptRequestMessage, {
    Self { transaction_hash: item.transaction_hash.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetEvmTransactionReceiptResponse>, protowire::GetEvmTransactionReceiptResponseMessage, {
    Self {
        found: item.found,
        accepting_block: item.accepting_block.clone(),
        evm_number: item.evm_number,
        receipt_index: item.receipt_index,
        succeeded: item.succeeded,
        gas_used: item.gas_used,
        cumulative_gas_used: item.cumulative_gas_used,
        logs: item
            .logs
            .iter()
            .map(|l| protowire::RpcEvmLogMessage { address: l.address.clone(), topics: l.topics.clone(), data: l.data.clone() })
            .collect(),
        error: None,
    }
});
from!(item: &kaspa_rpc_core::GetEvmTxInclusionStatusRequest, protowire::GetEvmTxInclusionStatusRequestMessage, {
    Self { transaction_hash: item.transaction_hash.clone() }
});
from!(item: RpcResult<&kaspa_rpc_core::GetEvmTxInclusionStatusResponse>, protowire::GetEvmTxInclusionStatusResponseMessage, {
    Self {
        pending: item.pending,
        included_in: item.included_in.clone(),
        accepted_in: item.accepted_in.clone(),
        receipt_index: item.receipt_index,
        last_skip_class: item.last_skip_class,
        error: None,
    }
});

from!(&kaspa_rpc_core::GetValidatorStatusRequest, protowire::GetValidatorStatusRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetValidatorStatusResponse>, protowire::GetValidatorStatusResponseMessage, {
    Self {
        enabled: item.enabled,
        mode: item.mode.clone(),
        has_key: item.has_key,
        validator_id: item.validator_id.clone(),
        funding_address: item.funding_address.clone(),
        overlay_configured: item.overlay_configured,
        epoch: item.epoch,
        bond_status: item.bond_status.clone(),
        is_active_validator: item.is_active_validator,
        has_signed_epoch: item.has_signed_epoch,
        last_signed_epoch: item.last_signed_epoch,
        status: item.status,
        status_label: item.status_label.clone(),
        error: None,
    }
});

from!(item: &kaspa_rpc_core::BanRequest, protowire::BanRequestMessage, { Self { ip: item.ip.to_string() } });
from!(_item: RpcResult<&kaspa_rpc_core::BanResponse>, protowire::BanResponseMessage, { Self { error: None } });

from!(item: &kaspa_rpc_core::UnbanRequest, protowire::UnbanRequestMessage, { Self { ip: item.ip.to_string() } });
from!(_item: RpcResult<&kaspa_rpc_core::UnbanResponse>, protowire::UnbanResponseMessage, { Self { error: None } });

from!(item: &kaspa_rpc_core::EstimateNetworkHashesPerSecondRequest, protowire::EstimateNetworkHashesPerSecondRequestMessage, {
    Self { window_size: item.window_size, start_hash: item.start_hash.map_or(Default::default(), |x| x.to_string()) }
});
from!(
    item: RpcResult<&kaspa_rpc_core::EstimateNetworkHashesPerSecondResponse>,
    protowire::EstimateNetworkHashesPerSecondResponseMessage,
    { Self { network_hashes_per_second: item.network_hashes_per_second, error: None } }
);

from!(item: &kaspa_rpc_core::GetMempoolEntriesByAddressesRequest, protowire::GetMempoolEntriesByAddressesRequestMessage, {
    Self {
        addresses: item.addresses.iter().map(|x| x.into()).collect(),
        include_orphan_pool: item.include_orphan_pool,
        filter_transaction_pool: item.filter_transaction_pool,
    }
});
from!(
    item: RpcResult<&kaspa_rpc_core::GetMempoolEntriesByAddressesResponse>,
    protowire::GetMempoolEntriesByAddressesResponseMessage,
    { Self { entries: item.entries.iter().map(|x| x.into()).collect(), error: None } }
);

from!(&kaspa_rpc_core::GetCoinSupplyRequest, protowire::GetCoinSupplyRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetCoinSupplyResponse>, protowire::GetCoinSupplyResponseMessage, {
    Self { max_sompi: item.max_sompi, circulating_sompi: item.circulating_sompi, error: None }
});

from!(item: &kaspa_rpc_core::GetDaaScoreTimestampEstimateRequest, protowire::GetDaaScoreTimestampEstimateRequestMessage, {
    Self {
        daa_scores: item.daa_scores.clone()
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetDaaScoreTimestampEstimateResponse>, protowire::GetDaaScoreTimestampEstimateResponseMessage, {
    Self { timestamps: item.timestamps.clone(), error: None }
});

// Fee estimate API

from!(&kaspa_rpc_core::GetFeeEstimateRequest, protowire::GetFeeEstimateRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetFeeEstimateResponse>, protowire::GetFeeEstimateResponseMessage, {
    Self { estimate: Some((&item.estimate).into()), error: None }
});
from!(item: &kaspa_rpc_core::GetFeeEstimateExperimentalRequest, protowire::GetFeeEstimateExperimentalRequestMessage, {
    Self {
        verbose: item.verbose
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetFeeEstimateExperimentalResponse>, protowire::GetFeeEstimateExperimentalResponseMessage, {
    Self {
        estimate: Some((&item.estimate).into()),
        verbose: item.verbose.as_ref().map(|x| x.into()),
        error: None
    }
});

from!(item: &kaspa_rpc_core::GetCurrentBlockColorRequest, protowire::GetCurrentBlockColorRequestMessage, {
    Self {
        hash: item.hash.to_string()
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetCurrentBlockColorResponse>, protowire::GetCurrentBlockColorResponseMessage, {
    Self { blue: item.blue, error: None }
});

from!(item: &kaspa_rpc_core::GetUtxoReturnAddressRequest, protowire::GetUtxoReturnAddressRequestMessage, {
    Self {
        txid: item.txid.to_string(),
        accepting_block_daa_score: item.accepting_block_daa_score
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetUtxoReturnAddressResponse>, protowire::GetUtxoReturnAddressResponseMessage, {
    Self { return_address: item.return_address.address_to_string(), error: None }
});

from!(&kaspa_rpc_core::PingRequest, protowire::PingRequestMessage);
from!(RpcResult<&kaspa_rpc_core::PingResponse>, protowire::PingResponseMessage);

from!(item: &kaspa_rpc_core::GetMetricsRequest, protowire::GetMetricsRequestMessage, {
    Self {
        process_metrics: item.process_metrics,
        connection_metrics: item.connection_metrics,
        bandwidth_metrics: item.bandwidth_metrics,
        consensus_metrics: item.consensus_metrics,
        storage_metrics: item.storage_metrics,
        custom_metrics: item.custom_metrics,
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetMetricsResponse>, protowire::GetMetricsResponseMessage, {
    Self {
        server_time: item.server_time,
        process_metrics: item.process_metrics.as_ref().map(|x| x.into()),
        connection_metrics: item.connection_metrics.as_ref().map(|x| x.into()),
        bandwidth_metrics: item.bandwidth_metrics.as_ref().map(|x| x.into()),
        consensus_metrics: item.consensus_metrics.as_ref().map(|x| x.into()),
        storage_metrics: item.storage_metrics.as_ref().map(|x| x.into()),
        // TODO
        // custom_metrics : None,
        error: None,
    }
});

from!(item: &kaspa_rpc_core::GetConnectionsRequest, protowire::GetConnectionsRequestMessage, {
    Self {
        include_profile_data : item.include_profile_data,
    }
});
from!(item: RpcResult<&kaspa_rpc_core::GetConnectionsResponse>, protowire::GetConnectionsResponseMessage, {
    Self {
        clients: item.clients,
        peers: item.peers as u32,
        profile_data: item.profile_data.as_ref().map(|x| x.into()),
        error: None,
    }
});

from!(&kaspa_rpc_core::GetSystemInfoRequest, protowire::GetSystemInfoRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetSystemInfoResponse>, protowire::GetSystemInfoResponseMessage, {
    Self {
        version : item.version.clone(),
        system_id : item.system_id.as_ref().map(|system_id|system_id.to_hex()).unwrap_or_default(),
        git_hash : item.git_hash.as_ref().map(|git_hash|git_hash.to_hex()).unwrap_or_default(),
        total_memory : item.total_memory,
        core_num : item.cpu_physical_cores as u32,
        fd_limit : item.fd_limit,
        proxy_socket_limit_per_cpu_core : item.proxy_socket_limit_per_cpu_core.unwrap_or_default(),
        error: None,
    }
});

from!(&kaspa_rpc_core::GetServerInfoRequest, protowire::GetServerInfoRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetServerInfoResponse>, protowire::GetServerInfoResponseMessage, {
    Self {
        rpc_api_version: item.rpc_api_version as u32,
        rpc_api_revision: item.rpc_api_revision as u32,
        server_version: item.server_version.clone(),
        network_id: item.network_id.to_string(),
        has_utxo_index: item.has_utxo_index,
        is_synced: item.is_synced,
        virtual_daa_score: item.virtual_daa_score,
        error: None,
    }
});

from!(&kaspa_rpc_core::GetSyncStatusRequest, protowire::GetSyncStatusRequestMessage);
from!(item: RpcResult<&kaspa_rpc_core::GetSyncStatusResponse>, protowire::GetSyncStatusResponseMessage, {
    Self {
        is_synced: item.is_synced,
        error: None,
    }
});

from!(item: &kaspa_rpc_core::GetVirtualChainFromBlockV2Request, protowire::GetVirtualChainFromBlockV2RequestMessage, {
    Self {
        start_hash: item.start_hash.to_string(),
        data_verbosity_level: item.data_verbosity_level.map(|v| v as i32),
        min_confirmation_count: item.min_confirmation_count
    }
});

from!(item: RpcResult<&kaspa_rpc_core::GetVirtualChainFromBlockV2Response>, protowire::GetVirtualChainFromBlockV2ResponseMessage, {
    Self {
        removed_chain_block_hashes: item.removed_chain_block_hashes.iter().map(|x| x.to_string()).collect(),
        added_chain_block_hashes: item.added_chain_block_hashes.iter().map(|x| x.to_string()).collect(),
        chain_block_accepted_transactions: item.chain_block_accepted_transactions.iter().map(|x| x.into()).collect(),
        error: None,
    }
});

from!(item: &kaspa_rpc_core::NotifyUtxosChangedRequest, protowire::NotifyUtxosChangedRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect(), command: item.command.into() }
});
from!(item: &kaspa_rpc_core::NotifyUtxosChangedRequest, protowire::StopNotifyingUtxosChangedRequestMessage, {
    Self { addresses: item.addresses.iter().map(|x| x.into()).collect() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyUtxosChangedResponse>, protowire::NotifyUtxosChangedResponseMessage);
from!(RpcResult<&kaspa_rpc_core::NotifyUtxosChangedResponse>, protowire::StopNotifyingUtxosChangedResponseMessage);

from!(item: &kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideRequest, protowire::NotifyPruningPointUtxoSetOverrideRequestMessage, {
    Self { command: item.command.into() }
});
from!(&kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideRequest, protowire::StopNotifyingPruningPointUtxoSetOverrideRequestMessage);
from!(
    RpcResult<&kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideResponse>,
    protowire::NotifyPruningPointUtxoSetOverrideResponseMessage
);
from!(
    RpcResult<&kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideResponse>,
    protowire::StopNotifyingPruningPointUtxoSetOverrideResponseMessage
);

from!(item: &kaspa_rpc_core::NotifyFinalityConflictRequest, protowire::NotifyFinalityConflictRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyFinalityConflictResponse>, protowire::NotifyFinalityConflictResponseMessage);

from!(item: &kaspa_rpc_core::NotifyVirtualDaaScoreChangedRequest, protowire::NotifyVirtualDaaScoreChangedRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyVirtualDaaScoreChangedResponse>, protowire::NotifyVirtualDaaScoreChangedResponseMessage);

from!(item: &kaspa_rpc_core::NotifyVirtualChainChangedRequest, protowire::NotifyVirtualChainChangedRequestMessage, {
    Self { include_accepted_transaction_ids: item.include_accepted_transaction_ids, command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifyVirtualChainChangedResponse>, protowire::NotifyVirtualChainChangedResponseMessage);

from!(item: &kaspa_rpc_core::NotifySinkBlueScoreChangedRequest, protowire::NotifySinkBlueScoreChangedRequestMessage, {
    Self { command: item.command.into() }
});
from!(RpcResult<&kaspa_rpc_core::NotifySinkBlueScoreChangedResponse>, protowire::NotifySinkBlueScoreChangedResponseMessage);

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

from!(item: RejectReason, kaspa_rpc_core::SubmitBlockReport, {
    match item {
        RejectReason::None => kaspa_rpc_core::SubmitBlockReport::Success,
        RejectReason::BlockInvalid => kaspa_rpc_core::SubmitBlockReport::Reject(kaspa_rpc_core::SubmitBlockRejectReason::BlockInvalid),
        RejectReason::IsInIbd => kaspa_rpc_core::SubmitBlockReport::Reject(kaspa_rpc_core::SubmitBlockRejectReason::IsInIBD),
    }
});

try_from!(item: &protowire::SubmitBlockRequestMessage, kaspa_rpc_core::SubmitBlockRequest, {
    Self {
        block: item
            .block
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("SubmitBlockRequestMessage".to_string(), "block".to_string()))?
            .try_into()?,
        allow_non_daa_blocks: item.allow_non_daa_blocks,
    }
});
impl TryFrom<&protowire::SubmitBlockResponseMessage> for kaspa_rpc_core::SubmitBlockResponse {
    type Error = RpcError;
    // This conversion breaks the general conversion convention (see file header) since the message may
    // contain both a non-None reject_reason and a matching error message. Things get even challenging
    // in the RouteIsFull case where reject_reason is None (because this reason has no variant in protowire)
    // but a specific error message is provided.
    fn try_from(item: &protowire::SubmitBlockResponseMessage) -> RpcResult<Self> {
        let report: SubmitBlockReport =
            RejectReason::try_from(item.reject_reason).map_err(|_| RpcError::PrimitiveToEnumConversionError)?.into();
        if let Some(ref err) = item.error {
            match report {
                SubmitBlockReport::Success => {
                    if err.message == RpcError::SubmitBlockError(SubmitBlockRejectReason::RouteIsFull).to_string() {
                        Ok(Self { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::RouteIsFull) })
                    } else {
                        Err(err.into())
                    }
                }
                SubmitBlockReport::Reject(_) => Ok(Self { report }),
            }
        } else {
            Ok(Self { report })
        }
    }
}

try_from!(item: &protowire::GetBlockTemplateRequestMessage, kaspa_rpc_core::GetBlockTemplateRequest, {
    Self { pay_address: item.pay_address.clone().try_into()?, extra_data: RpcExtraData::from_iter(item.extra_data.bytes()) }
});
try_from!(item: &protowire::GetBlockTemplateResponseMessage, RpcResult<kaspa_rpc_core::GetBlockTemplateResponse>, {
    Self {
        block: item
            .block
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("GetBlockTemplateResponseMessage".to_string(), "block".to_string()))?
            .try_into()?,
        is_synced: item.is_synced,
    }
});

try_from!(item: &protowire::GetBlockRequestMessage, kaspa_rpc_core::GetBlockRequest, {
    Self { hash: RpcHash::from_str(&item.hash)?, include_transactions: item.include_transactions }
});
try_from!(item: &protowire::GetBlockResponseMessage, RpcResult<kaspa_rpc_core::GetBlockResponse>, {
    Self {
        block: item
            .block
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("GetBlockResponseMessage".to_string(), "block".to_string()))?
            .try_into()?,
    }
});

try_from!(item: &protowire::NotifyBlockAddedRequestMessage, kaspa_rpc_core::NotifyBlockAddedRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyBlockAddedResponseMessage, RpcResult<kaspa_rpc_core::NotifyBlockAddedResponse>);

try_from!(&protowire::GetInfoRequestMessage, kaspa_rpc_core::GetInfoRequest);
try_from!(item: &protowire::GetInfoResponseMessage, RpcResult<kaspa_rpc_core::GetInfoResponse>, {
    Self {
        p2p_id: item.p2p_id.clone(),
        mempool_size: item.mempool_size,
        server_version: item.server_version.clone(),
        is_utxo_indexed: item.is_utxo_indexed,
        is_synced: item.is_synced,
        has_notify_command: item.has_notify_command,
        has_message_id: item.has_message_id,
    }
});

try_from!(item: &protowire::NotifyNewBlockTemplateRequestMessage, kaspa_rpc_core::NotifyNewBlockTemplateRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyNewBlockTemplateResponseMessage, RpcResult<kaspa_rpc_core::NotifyNewBlockTemplateResponse>);

try_from!(item: &protowire::NotifyPalwClassReadinessChangedRequestMessage, kaspa_rpc_core::NotifyPalwClassReadinessChangedRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyPalwClassReadinessChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifyPalwClassReadinessChangedResponse>);
try_from!(item: &protowire::NotifyPalwPanelAssignmentRequestMessage, kaspa_rpc_core::NotifyPalwPanelAssignmentRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyPalwPanelAssignmentResponseMessage, RpcResult<kaspa_rpc_core::NotifyPalwPanelAssignmentResponse>);
try_from!(item: &protowire::NotifyPalwPanelReceiptRequestMessage, kaspa_rpc_core::NotifyPalwPanelReceiptRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyPalwPanelReceiptResponseMessage, RpcResult<kaspa_rpc_core::NotifyPalwPanelReceiptResponse>);
try_from!(item: &protowire::NotifyPalwPanelEligibilityChangedRequestMessage, kaspa_rpc_core::NotifyPalwPanelEligibilityChangedRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyPalwPanelEligibilityChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifyPalwPanelEligibilityChangedResponse>);

// ~~~

try_from!(&protowire::GetCurrentNetworkRequestMessage, kaspa_rpc_core::GetCurrentNetworkRequest);
try_from!(item: &protowire::GetCurrentNetworkResponseMessage, RpcResult<kaspa_rpc_core::GetCurrentNetworkResponse>, {
    // Note that current_network is first converted to lowercase because the golang implementation
    // returns a "human readable" version with a capital first letter while the rusty version
    // is fully lowercase.
    Self { network: RpcNetworkType::from_str(&item.current_network.to_lowercase())? }
});

try_from!(&protowire::GetPeerAddressesRequestMessage, kaspa_rpc_core::GetPeerAddressesRequest);
try_from!(item: &protowire::GetPeerAddressesResponseMessage, RpcResult<kaspa_rpc_core::GetPeerAddressesResponse>, {
    Self {
        known_addresses: item.addresses.iter().map(RpcPeerAddress::try_from).collect::<Result<Vec<_>, _>>()?,
        banned_addresses: item.banned_addresses.iter().map(RpcIpAddress::try_from).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(&protowire::GetSinkRequestMessage, kaspa_rpc_core::GetSinkRequest);
try_from!(item: &protowire::GetSinkResponseMessage, RpcResult<kaspa_rpc_core::GetSinkResponse>, {
    Self { sink: RpcHash::from_str(&item.sink)? }
});

try_from!(item: &protowire::GetMempoolEntryRequestMessage, kaspa_rpc_core::GetMempoolEntryRequest, {
    Self {
        transaction_id: kaspa_rpc_core::RpcTransactionId::from_str(&item.tx_id)?,
        include_orphan_pool: item.include_orphan_pool,
        filter_transaction_pool: item.filter_transaction_pool,
    }
});
try_from!(item: &protowire::GetMempoolEntryResponseMessage, RpcResult<kaspa_rpc_core::GetMempoolEntryResponse>, {
    Self {
        mempool_entry: item
            .entry
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("GetMempoolEntryResponseMessage".to_string(), "entry".to_string()))?
            .try_into()?,
    }
});

try_from!(item: &protowire::GetMempoolEntriesRequestMessage, kaspa_rpc_core::GetMempoolEntriesRequest, {
    Self { include_orphan_pool: item.include_orphan_pool, filter_transaction_pool: item.filter_transaction_pool }
});
try_from!(item: &protowire::GetMempoolEntriesResponseMessage, RpcResult<kaspa_rpc_core::GetMempoolEntriesResponse>, {
    Self { mempool_entries: item.entries.iter().map(kaspa_rpc_core::RpcMempoolEntry::try_from).collect::<Result<Vec<_>, _>>()? }
});

try_from!(&protowire::GetConnectedPeerInfoRequestMessage, kaspa_rpc_core::GetConnectedPeerInfoRequest);
try_from!(item: &protowire::GetConnectedPeerInfoResponseMessage, RpcResult<kaspa_rpc_core::GetConnectedPeerInfoResponse>, {
    Self { peer_info: item.infos.iter().map(kaspa_rpc_core::RpcPeerInfo::try_from).collect::<Result<Vec<_>, _>>()? }
});

try_from!(item: &protowire::AddPeerRequestMessage, kaspa_rpc_core::AddPeerRequest, {
    Self { peer_address: RpcContextualPeerAddress::from_str(&item.address)?, is_permanent: item.is_permanent }
});
try_from!(&protowire::AddPeerResponseMessage, RpcResult<kaspa_rpc_core::AddPeerResponse>);

try_from!(item: &protowire::SubmitTransactionRequestMessage, kaspa_rpc_core::SubmitTransactionRequest, {
    Self {
        transaction: item
            .transaction
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("SubmitTransactionRequestMessage".to_string(), "transaction".to_string()))?
            .try_into()?,
        allow_orphan: item.allow_orphan,
    }
});
try_from!(item: &protowire::SubmitTransactionResponseMessage, RpcResult<kaspa_rpc_core::SubmitTransactionResponse>, {
    // PR-9.5c/f: TransactionId widened to Hash64.
    Self { transaction_id: kaspa_consensus_core::Hash64::from_str(&item.transaction_id)? }
});

try_from!(item: &protowire::SubmitTransactionReplacementRequestMessage, kaspa_rpc_core::SubmitTransactionReplacementRequest, {
    Self {
        transaction: item
            .transaction
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("SubmitTransactionReplacementRequestMessage".to_string(), "transaction".to_string()))?
            .try_into()?,
    }
});
try_from!(item: &protowire::SubmitTransactionReplacementResponseMessage, RpcResult<kaspa_rpc_core::SubmitTransactionReplacementResponse>, {
    Self {
        // PR-9.5c/f: TransactionId widened to Hash64.
        transaction_id: kaspa_consensus_core::Hash64::from_str(&item.transaction_id)?,
        replaced_transaction: item
            .replaced_transaction
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("SubmitTransactionReplacementRequestMessage".to_string(), "replaced_transaction".to_string()))?
            .try_into()?,
    }
});

try_from!(item: &protowire::GetSubnetworkRequestMessage, kaspa_rpc_core::GetSubnetworkRequest, {
    Self { subnetwork_id: kaspa_rpc_core::RpcSubnetworkId::from_str(&item.subnetwork_id)? }
});
try_from!(item: &protowire::GetSubnetworkResponseMessage, RpcResult<kaspa_rpc_core::GetSubnetworkResponse>, {
    Self { gas_limit: item.gas_limit }
});

try_from!(item: &protowire::GetVirtualChainFromBlockRequestMessage, kaspa_rpc_core::GetVirtualChainFromBlockRequest, {
    Self { start_hash: RpcHash::from_str(&item.start_hash)?, include_accepted_transaction_ids: item.include_accepted_transaction_ids, min_confirmation_count: item.min_confirmation_count }
});
try_from!(item: &protowire::GetVirtualChainFromBlockResponseMessage, RpcResult<kaspa_rpc_core::GetVirtualChainFromBlockResponse>, {
    Self {
        removed_chain_block_hashes: item
            .removed_chain_block_hashes
            .iter()
            .map(|x| RpcHash::from_str(x))
            .collect::<Result<Vec<_>, _>>()?,
        added_chain_block_hashes: item.added_chain_block_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(item: &protowire::GetVirtualChainFromBlockV2RequestMessage, kaspa_rpc_core::GetVirtualChainFromBlockV2Request, {
    Self {
        start_hash: RpcHash::from_str(&item.start_hash)?,
        data_verbosity_level: item.data_verbosity_level.map(RpcDataVerbosityLevel::try_from).transpose()?,
        min_confirmation_count: item.min_confirmation_count
    }
});
try_from!(item: &protowire::GetVirtualChainFromBlockV2ResponseMessage, RpcResult<kaspa_rpc_core::GetVirtualChainFromBlockV2Response>, {
    Self {
        removed_chain_block_hashes: Arc::new(item.removed_chain_block_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?),
        added_chain_block_hashes: Arc::new(item.added_chain_block_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?),
        chain_block_accepted_transactions: Arc::new(item.chain_block_accepted_transactions.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?),
    }
});

try_from!(item: &protowire::GetBlocksRequestMessage, kaspa_rpc_core::GetBlocksRequest, {
    Self {
        low_hash: if item.low_hash.is_empty() { None } else { Some(RpcHash::from_str(&item.low_hash)?) },
        include_blocks: item.include_blocks,
        include_transactions: item.include_transactions,
    }
});
try_from!(item: &protowire::GetBlocksResponseMessage, RpcResult<kaspa_rpc_core::GetBlocksResponse>, {
    Self {
        block_hashes: item.block_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        blocks: item.blocks.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(&protowire::GetBlockCountRequestMessage, kaspa_rpc_core::GetBlockCountRequest);
try_from!(item: &protowire::GetBlockCountResponseMessage, RpcResult<kaspa_rpc_core::GetBlockCountResponse>, {
    Self { header_count: item.header_count, block_count: item.block_count }
});

try_from!(&protowire::GetBlockDagInfoRequestMessage, kaspa_rpc_core::GetBlockDagInfoRequest);
try_from!(item: &protowire::GetBlockDagInfoResponseMessage, RpcResult<kaspa_rpc_core::GetBlockDagInfoResponse>, {
    Self {
        network: kaspa_rpc_core::RpcNetworkId::from_prefixed(&item.network_name)?,
        block_count: item.block_count,
        header_count: item.header_count,
        tip_hashes: item.tip_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        difficulty: item.difficulty,
        past_median_time: item.past_median_time as u64,
        virtual_parent_hashes: item.virtual_parent_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        pruning_point_hash: RpcHash::from_str(&item.pruning_point_hash)?,
        virtual_daa_score: item.virtual_daa_score,
        sink: item.sink.parse()?,
    }
});

try_from!(item: &protowire::ResolveFinalityConflictRequestMessage, kaspa_rpc_core::ResolveFinalityConflictRequest, {
    Self { finality_block_hash: RpcHash::from_str(&item.finality_block_hash)? }
});
try_from!(&protowire::ResolveFinalityConflictResponseMessage, RpcResult<kaspa_rpc_core::ResolveFinalityConflictResponse>);

try_from!(&protowire::ShutdownRequestMessage, kaspa_rpc_core::ShutdownRequest);
try_from!(&protowire::ShutdownResponseMessage, RpcResult<kaspa_rpc_core::ShutdownResponse>);

try_from!(item: &protowire::GetHeadersRequestMessage, kaspa_rpc_core::GetHeadersRequest, {
    Self { start_hash: RpcHash::from_str(&item.start_hash)?, limit: item.limit, is_ascending: item.is_ascending }
});
try_from!(item: &protowire::GetHeadersResponseMessage, RpcResult<kaspa_rpc_core::GetHeadersResponse>, {
    // TODO
    Self { headers: vec![] }
});

try_from!(item: &protowire::GetUtxosByAddressesRequestMessage, kaspa_rpc_core::GetUtxosByAddressesRequest, {
    Self { addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()? }
});
try_from!(item: &protowire::GetUtxosByAddressesResponseMessage, RpcResult<kaspa_rpc_core::GetUtxosByAddressesResponse>, {
    Self { entries: item.entries.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()? }
});

try_from!(item: &protowire::GetUtxosByAddressPageRequestMessage, kaspa_rpc_core::GetUtxosByAddressPageRequest, {
    Self { address: item.address.as_str().try_into()?, cursor: item.cursor.clone(), limit: item.limit }
});
try_from!(item: &protowire::GetUtxosByAddressPageResponseMessage, RpcResult<kaspa_rpc_core::GetUtxosByAddressPageResponse>, {
    Self {
        entries: item.entries.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?,
        next_cursor: item.next_cursor.clone(),
    }
});

try_from!(item: &protowire::GetBalanceByAddressRequestMessage, kaspa_rpc_core::GetBalanceByAddressRequest, {
    Self { address: item.address.as_str().try_into()? }
});
try_from!(item: &protowire::GetBalanceByAddressResponseMessage, RpcResult<kaspa_rpc_core::GetBalanceByAddressResponse>, {
    Self { balance: item.balance }
});

try_from!(item: &protowire::GetBalancesByAddressesRequestMessage, kaspa_rpc_core::GetBalancesByAddressesRequest, {
    Self { addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()? }
});
try_from!(item: &protowire::GetBalancesByAddressesResponseMessage, RpcResult<kaspa_rpc_core::GetBalancesByAddressesResponse>, {
    Self { entries: item.entries.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()? }
});

try_from!(&protowire::GetSinkBlueScoreRequestMessage, kaspa_rpc_core::GetSinkBlueScoreRequest);
try_from!(item: &protowire::GetSinkBlueScoreResponseMessage, RpcResult<kaspa_rpc_core::GetSinkBlueScoreResponse>, {
    Self { blue_score: item.blue_score }
});

try_from!(item: &protowire::GetDnsConfirmationRequestMessage, kaspa_rpc_core::GetDnsConfirmationRequest, { Self { block_hash: item.block_hash.clone() } });
try_from!(item: &protowire::GetTokenLedgerEntryRequestMessage, kaspa_rpc_core::GetTokenLedgerEntryRequest, {
    Self { asset_id: item.asset_id, owner: item.owner.clone() }
});
try_from!(item: &protowire::GetTokenLedgerEntryResponseMessage, RpcResult<kaspa_rpc_core::GetTokenLedgerEntryResponse>, {
    Self { available: item.available, balance: item.balance.clone(), nonce: item.nonce }
});
try_from!(item: &protowire::GetPalwProducerFactsRequestMessage, kaspa_rpc_core::GetPalwProducerFactsRequest, {
    Self {
        class_id: item.class_id.clone(),
        bond_transaction_id: item.bond_transaction_id.clone(),
        bond_index: item.bond_index,
        with_bond: item.with_bond,
    }
});
try_from!(item: &protowire::GetPalwProducerFactsResponseMessage, RpcResult<kaspa_rpc_core::GetPalwProducerFactsResponse>, {
    Self {
        available: item.available,
        chain_point: item.chain_point.clone(),
        daa_score: item.daa_score,
        class_id: item.class_id.clone(),
        artifact_root: item.artifact_root.clone(),
        class_target: item.class_target.clone(),
        pwu: item.pwu,
        is_base_class: item.is_base_class,
        min_trace_retention_daa: item.min_trace_retention_daa,
        epoch_index: item.epoch_index,
        epoch_budget_blocks: item.epoch_budget_blocks,
        epoch_produced_blocks: item.epoch_produced_blocks,
        bond_known: item.bond_known,
        bond_registered_pubkey: item.bond_registered_pubkey.clone(),
        bond_operator_id: item.bond_operator_id.clone(),
        bond_collateral: item.bond_collateral,
        bond_reserved_exposure: item.bond_reserved_exposure.clone(),
        bond_exposure_ceiling: item.bond_exposure_ceiling.clone(),
        bond_claim_exposure: item.bond_claim_exposure.clone(),
        not_ready_reason: item.not_ready_reason.clone(),
        locked_bond_outpoints: item.locked_bond_outpoints.clone(),
        fp_certified: item.fp_certified,
        fp_quanta_per_canonical_job: item.fp_quanta_per_canonical_job,
        fp_max_quanta_per_receipt: item.fp_max_quanta_per_receipt,
        fp_decode_rules_armed: item.fp_decode_rules_armed,
        palw_retention_dir: item.palw_retention_dir.clone(),
        panel_da_armed: item.panel_da_armed,
        prompt_ids_merkle: item.prompt_ids_merkle,
        fp_decode_constraint_armed: item.fp_decode_constraint_armed,
        // ADR-0118 Decision 3. A class's form is Merkle wherever the network's is, so the class
        // bit is never below the network's — and a node that predates the field (proto3 reads an
        // absent bool as false) answers the network's form, which is its every class's.
        class_prompt_ids_merkle: item.class_prompt_ids_merkle || item.prompt_ids_merkle,
        // ADR-0152 P6. proto3 reads an absent string as "": a node that predates the fields has one
        // ledger, the one it reported, and asserts no floor and no accuser ledger (as the wRPC
        // reader of a version-8 writer).
        bond_committed: if item.bond_committed.is_empty() { item.bond_reserved_exposure.clone() } else { item.bond_committed.clone() },
        bond_producer_floor_shortfall: item.bond_producer_floor_shortfall,
        bond_accuser_exposure: if item.bond_accuser_exposure.is_empty() { "0".to_string() } else { item.bond_accuser_exposure.clone() },
    }
});
try_from!(item: &protowire::GetPalwDerivedArtifactsRequestMessage, kaspa_rpc_core::GetPalwDerivedArtifactsRequest, {
    Self { claim_id: item.claim_id.clone() }
});
try_from!(item: &protowire::GetPalwDerivedArtifactsResponseMessage, RpcResult<kaspa_rpc_core::GetPalwDerivedArtifactsResponse>, {
    Self {
        found: item.found,
        claim_id: item.claim_id.clone(),
        output_root: item.output_root.clone(),
        executor_pubkey: item.executor_pubkey.clone(),
        executor_bond: item.executor_bond.clone(),
        class_id: item.class_id.clone(),
        claim_phase: item.claim_phase.clone(),
        claim_void_reason: item.claim_void_reason.clone(),
        claim_accepted_block: item.claim_accepted_block.clone(),
        claim_accepted_daa: item.claim_accepted_daa,
        artifacts: item
            .artifacts
            .iter()
            .map(|a| kaspa_rpc_core::RpcPalwDerivedArtifact {
                transformer_id: a.transformer_id.clone(),
                derived_id: a.derived_id.clone(),
                grammar_id: a.grammar_id.clone(),
                kind: a.kind,
                kind_name: a.kind_name.clone(),
                dsl_hash: a.dsl_hash.clone(),
                artifact_hash: a.artifact_hash.clone(),
                artifact_bytes: a.artifact_bytes,
                accepted_daa: a.accepted_daa,
            })
            .collect(),
    }
});
try_from!(item: &protowire::GetPalwFreePromptClaimRequestMessage, kaspa_rpc_core::GetPalwFreePromptClaimRequest, {
    Self { claim_id: item.claim_id.clone() }
});
try_from!(item: &protowire::GetPalwFreePromptClaimResponseMessage, RpcResult<kaspa_rpc_core::GetPalwFreePromptClaimResponse>, {
    Self {
        found: item.found,
        claim_id: item.claim_id.clone(),
        is_free_prompt: item.is_free_prompt,
        class_id: item.class_id.clone(),
        executor_pubkey: item.executor_pubkey.clone(),
        executor_bond: item.executor_bond.clone(),
        output_root: item.output_root.clone(),
        trace_root: item.trace_root.clone(),
        execution_root: item.execution_root.clone(),
        work_leaves: item.work_leaves,
        work_id: item.work_id.clone(),
        quanta: item.quanta,
        quanta_spent: item.quanta_spent,
        phase: item.phase.clone(),
        void_reason: item.void_reason.clone(),
        phase_daa: item.phase_daa,
        accepted_block: item.accepted_block.clone(),
        accepted_daa: item.accepted_daa,
        trace_retention_daa: item.trace_retention_daa,
        derived_count: item.derived_count,
    }
});
try_from!(item: &protowire::GetPalwPendingChunkGroupRequestMessage, kaspa_rpc_core::GetPalwPendingChunkGroupRequest, {
    Self { session_id: item.session_id.clone(), side: item.side.clone() }
});
try_from!(item: &protowire::GetPalwPendingChunkGroupResponseMessage, RpcResult<kaspa_rpc_core::GetPalwPendingChunkGroupResponse>, {
    Self {
        found: item.found,
        session_id: item.session_id.clone(),
        side: item.side.clone(),
        count: item.count,
        present: item.present,
        parts_present: item.parts_present,
        complete: item.complete,
        declared_daa: item.declared_daa,
        assembly_deadline_daa: item.assembly_deadline_daa,
        close_digest: item.close_digest.clone(),
        verdict: item.verdict.clone(),
        declarer_bond: item.declarer_bond.clone(),
        deposit: item.deposit,
    }
});
try_from!(item: &protowire::GetPalwModelMarketRequestMessage, kaspa_rpc_core::GetPalwModelMarketRequest, {
    Self { line_id: item.line_id.clone() }
});
try_from!(item: &protowire::GetPalwModelMarketResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelMarketResponse>, {
    Self {
        found: item.found,
        line_id: item.line_id.clone(),
        opened: item.opened,
        opened_daa: item.opened_daa,
        msk_reserve: item.msk_reserve,
        position_units: item.position_units,
        sold_units: item.sold_units,
        burned_sompi: item.burned_sompi,
        registrant_paid_sompi: item.registrant_paid_sompi,
        closed_to_buys: item.closed_to_buys,
        price_sompi_per_position: item.price_sompi_per_position,
        supply_units: item.supply_units,
        virtual_sompi: item.virtual_sompi,
        class_status: item.class_status.clone(),
        contributor_paid_sompi: item.contributor_paid_sompi,
        seed_sompi: item.seed_sompi,
        seeded_by: item.seeded_by.clone(),
        seed_min_sompi: item.seed_min_sompi,
        seed_pledged_sompi: item.seed_pledged_sompi,
        buyback_sompi: item.buyback_sompi,
        retired_units: item.retired_units,
        // A gRPC peer from before ADR-0114 sends zeros: its fold split 50/10.
        burn_permille: if item.burn_permille == 0 && item.leg_permille == 0 { 50 } else { item.burn_permille },
        leg_permille: if item.burn_permille == 0 && item.leg_permille == 0 { 10 } else { item.leg_permille },
        leg_v2_activation_daa: item.leg_v2_activation_daa,
        // P-B3: a gRPC peer from before the gate sends empty strings — no lifecycle, no refusal.
        class_lifecycle: item.class_lifecycle.clone(),
        market_refusal: item.market_refusal.clone(),
    }
});
try_from!(item: &protowire::GetPalwModelPositionsRequestMessage, kaspa_rpc_core::GetPalwModelPositionsRequest, {
    Self { holder: item.holder.clone() }
});
try_from!(item: &protowire::GetPalwModelPositionsResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelPositionsResponse>, {
    Self {
        holder: item.holder.clone(),
        positions: item
            .positions
            .iter()
            .map(|p| -> RpcResult<kaspa_rpc_core::RpcPalwModelPosition> {
                Ok(kaspa_rpc_core::RpcPalwModelPosition {
                    line_id: p.line_id.clone(),
                    units: p.units,
                    holding_since_daa: p.holding_since_daa,
                    tenure_daa: p.tenure_daa,
                    tier_index: p.tier_index,
                    tier: p.tier.as_ref().map(kaspa_rpc_core::RpcPalwModelBenefitTier::try_from).transpose()?,
                })
            })
            .collect::<RpcResult<Vec<_>>>()?,
        tip_daa: item.tip_daa,
        tip_hash: item.tip_hash.clone(),
    }
});
// ---- ADR-0088 Decision 12: the model registry ----
try_from!(item: &protowire::RpcPalwModelLine, kaspa_rpc_core::RpcPalwModelLine, {
    Self {
        line_id: item.line_id.clone(),
        class_id: item.class_id.clone(),
        has_row: item.has_row,
        owner: item.owner.as_ref().map(kaspa_rpc_core::RpcTransactionOutpoint::try_from).transpose()?,
        owner_payout_payload: item.owner_payout_payload.clone(),
        developer: item.developer.as_ref().map(kaspa_rpc_core::RpcTransactionOutpoint::try_from).transpose()?,
        developer_payout_payload: item.developer_payout_payload.clone(),
        maintainer: item.maintainer.as_ref().map(kaspa_rpc_core::RpcTransactionOutpoint::try_from).transpose()?,
        maintainer_payout_payload: item.maintainer_payout_payload.clone(),
        name: item.name.clone(),
        name_hex: item.name_hex.clone(),
        founded_daa: item.founded_daa,
        current: item.current,
        previews: item.previews.clone(),
        versions_published: item.versions_published,
        contributor_permille_of_leg: item.contributor_permille_of_leg,
        status: item.status.clone(),
        retired_daa: item.retired_daa,
    }
});
try_from!(item: &protowire::RpcPalwModelVersion, kaspa_rpc_core::RpcPalwModelVersion, {
    Self {
        line_id: item.line_id.clone(),
        version: item.version,
        root: item.root.clone(),
        parent: item.parent,
        adopted_from: item.adopted_from.clone(),
        runtime_hash: item.runtime_hash.clone(),
        dataset_commitment: item.dataset_commitment.clone(),
        training_config_hash: item.training_config_hash.clone(),
        notes_hash: item.notes_hash.clone(),
        published_daa: item.published_daa,
        published_by: item.published_by.as_ref().map(kaspa_rpc_core::RpcTransactionOutpoint::try_from).transpose()?,
        status: item.status.clone(),
        until_daa: item.until_daa,
        in_force: item.in_force,
        attempt_claims: item.attempt_claims,
        fp_claims: item.fp_claims,
        work_leaves: item.work_leaves.clone(),
        first_used_daa: item.first_used_daa,
        last_used_daa: item.last_used_daa,
    }
});
try_from!(item: &protowire::RpcPalwModelEvaluation, kaspa_rpc_core::RpcPalwModelEvaluation, {
    Self {
        evaluator_id: item.evaluator_id.clone(),
        score_permille: item.score_permille,
        report_hash: item.report_hash.clone(),
        posted_daa: item.posted_daa,
        by: item
            .by
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcPalwModelEvaluation".to_string(), "by".to_string()))?
            .try_into()?,
        is_lines_own: item.is_lines_own,
    }
});
try_from!(item: &protowire::RpcPalwModelProposal, kaspa_rpc_core::RpcPalwModelProposal, {
    Self {
        proposal_id: item.proposal_id.clone(),
        line_id: item.line_id.clone(),
        root: item.root.clone(),
        note_hash: item.note_hash.clone(),
        by: item
            .by
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcPalwModelProposal".to_string(), "by".to_string()))?
            .try_into()?,
        posted_daa: item.posted_daa,
        adopted_in: item.adopted_in,
    }
});
try_from!(item: &protowire::GetPalwModelLineRequestMessage, kaspa_rpc_core::GetPalwModelLineRequest, {
    Self { line_id: item.line_id.clone() }
});
try_from!(item: &protowire::GetPalwModelLineResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelLineResponse>, {
    Self {
        exists: item.exists,
        line_id: item.line_id.clone(),
        line: item.line.as_ref().map(kaspa_rpc_core::RpcPalwModelLine::try_from).transpose()?,
        current_root: item.current_root.clone(),
        roots_in_force: item.roots_in_force.clone(),
        tip_daa: item.tip_daa,
        benefits: item.benefits.as_ref().map(kaspa_rpc_core::RpcPalwModelBenefits::try_from).transpose()?,
        // A peer that predates ADR-0101's facts sends none, and empty is the honest reading: a
        // client that needs them asks a node that has them rather than treating silence as a
        // declaration.
        service_facts: item.service_facts.as_ref().map(kaspa_rpc_core::RpcPalwLineServiceFacts::from).unwrap_or_default(),
    }
});

impl From<&protowire::RpcPalwLineServiceFacts> for kaspa_rpc_core::RpcPalwLineServiceFacts {
    fn from(item: &protowire::RpcPalwLineServiceFacts) -> Self {
        Self { declared_grants: item.declared_grants, roots: item.roots.clone(), origin_pubkeys: item.origin_pubkeys.clone() }
    }
}

try_from!(item: &protowire::RpcPalwModelBenefitTier, kaspa_rpc_core::RpcPalwModelBenefitTier, {
    Self {
        min_units: item.min_units,
        grants: item.grants,
        grant_names: item.grant_names.clone(),
        lead_daa: item.lead_daa,
        min_hold_daa: item.min_hold_daa,
        note: item.note.clone(),
    }
});

try_from!(item: &protowire::RpcPalwModelBenefits, kaspa_rpc_core::RpcPalwModelBenefits, {
    Self {
        tiers: item.tiers.iter().map(kaspa_rpc_core::RpcPalwModelBenefitTier::try_from).collect::<RpcResult<Vec<_>>>()?,
        pending_tiers: item
            .pending_tiers
            .iter()
            .map(kaspa_rpc_core::RpcPalwModelBenefitTier::try_from)
            .collect::<RpcResult<Vec<_>>>()?,
        pending_effective_daa: item.pending_effective_daa,
        cadence_daa: item.cadence_daa,
        expires_daa: item.expires_daa,
        declared_daa: item.declared_daa,
        lapsed: item.lapsed.clone(),
        lapse_daa: item.lapse_daa,
        enforced_lead_daa: item.enforced_lead_daa,
    }
});
try_from!(item: &protowire::GetPalwModelVersionRequestMessage, kaspa_rpc_core::GetPalwModelVersionRequest, {
    Self { line_id: item.line_id.clone(), version: item.version }
});
try_from!(item: &protowire::GetPalwModelVersionResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelVersionResponse>, {
    Self {
        exists: item.exists,
        line_id: item.line_id.clone(),
        version_number: item.version_number,
        version: item.version.as_ref().map(kaspa_rpc_core::RpcPalwModelVersion::try_from).transpose()?,
        evaluations: item.evaluations.iter().map(kaspa_rpc_core::RpcPalwModelEvaluation::try_from).collect::<RpcResult<Vec<_>>>()?,
        tip_daa: item.tip_daa,
    }
});
try_from!(item: &protowire::GetPalwModelLinesRequestMessage, kaspa_rpc_core::GetPalwModelLinesRequest, {
    Self { class_id: item.class_id.clone() }
});
try_from!(item: &protowire::GetPalwModelLinesResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelLinesResponse>, {
    Self {
        exists: item.exists,
        class_id: item.class_id.clone(),
        lines: item.lines.iter().map(kaspa_rpc_core::RpcPalwModelLine::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::GetPalwModelProposalsRequestMessage, kaspa_rpc_core::GetPalwModelProposalsRequest, {
    Self { line_id: item.line_id.clone() }
});
try_from!(item: &protowire::GetPalwModelProposalsResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelProposalsResponse>, {
    Self {
        exists: item.exists,
        line_id: item.line_id.clone(),
        proposals: item.proposals.iter().map(kaspa_rpc_core::RpcPalwModelProposal::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
// ADR-0122 §6.5
try_from!(item: &protowire::RpcPalwClaimRow, kaspa_rpc_core::RpcPalwClaimRow, {
    Self {
        claim_id: item.claim_id.clone(),
        is_free_prompt: item.is_free_prompt,
        class_id: item.class_id.clone(),
        executor_bond: item.executor_bond.clone(),
        phase: item.phase.clone(),
        void_reason: item.void_reason.clone(),
        phase_daa: item.phase_daa,
        accepted_daa: item.accepted_daa,
        accepted_block: item.accepted_block.clone(),
        rebound_daa: item.rebound_daa,
        bound_daa: item.bound_daa,
        seats: item.seats.clone(),
        deadline_daa: item.deadline_daa,
        reserved_sompi: item.reserved_sompi.clone(),
        escrow_sompi: item.escrow_sompi,
        payout_pending_sompi: item.payout_pending_sompi,
        quanta: item.quanta,
        quanta_spent: item.quanta_spent,
        work_leaves: item.work_leaves,
        open_courts: item.open_courts,
        exec_stage: item.exec_stage.clone(),
        exec_credit: item.exec_credit,
        exec_span: item.exec_span,
        exec_tickets: item.exec_tickets,
        exec_tickets_spent: item.exec_tickets_spent,
        exec_first_round: item.exec_first_round,
        exec_last_round: item.exec_last_round,
        vesting_stage: item.vesting_stage.clone(),
        vesting_sompi: item.vesting_sompi,
        vesting_payee_sompi: item.vesting_payee_sompi,
        vesting_expiry_daa: item.vesting_expiry_daa,
        vesting_licences_since_final: item.vesting_licences_since_final,
        vesting_licences_needed: item.vesting_licences_needed,
        vesting_matured_at: item.vesting_matured_at,
        vesting_eta_daa: item.vesting_eta_daa,
        vesting_eta_estimated: item.vesting_eta_estimated,
    }
});
try_from!(item: &protowire::GetPalwClaimsRequestMessage, kaspa_rpc_core::GetPalwClaimsRequest, {
    Self { bond: item.bond.clone(), role: item.role.clone(), include_terminal: item.include_terminal, limit: item.limit }
});
try_from!(item: &protowire::GetPalwClaimsResponseMessage, RpcResult<kaspa_rpc_core::GetPalwClaimsResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        bond: item.bond.clone(),
        role: item.role.clone(),
        claims: item.claims.iter().map(kaspa_rpc_core::RpcPalwClaimRow::try_from).collect::<RpcResult<Vec<_>>>()?,
        truncated: item.truncated,
        bond_known: item.bond_known,
        bond_pubkey: item.bond_pubkey.clone(),
        bond_retiring_since_daa: item.bond_retiring_since_daa,
        bond_collateral: item.bond_collateral,
        bond_slashed: item.bond_slashed,
        bond_registered_daa: item.bond_registered_daa,
        bond_capable_classes: item.bond_capable_classes.clone(),
        vesting_only_rows: item
            .vesting_only_rows
            .iter()
            .map(kaspa_rpc_core::RpcPalwClaimRow::try_from)
            .collect::<RpcResult<Vec<_>>>()?,
        vesting_only_truncated: item.vesting_only_truncated,
    }
});
try_from!(item: &protowire::RpcPalwClassRow, kaspa_rpc_core::RpcPalwClassRow, {
    Self {
        class_id: item.class_id.clone(),
        is_base_class: item.is_base_class,
        status: item.status.clone(),
        share_permille: item
            .share_permille
            .map(|p| u16::try_from(p).map_err(|_| RpcError::General(format!("share_permille {p} is not a permille"))))
            .transpose()?,
        budget_blocks: item.budget_blocks,
        canonical_leaves: item.canonical_leaves,
        artifact_root: item.artifact_root.clone(),
        fp_certified: item.fp_certified,
        held: item.held,
        registered_daa: item.registered_daa,
    }
});
try_from!(&protowire::GetPalwClassesRequestMessage, kaspa_rpc_core::GetPalwClassesRequest);
try_from!(item: &protowire::GetPalwClassesResponseMessage, RpcResult<kaspa_rpc_core::GetPalwClassesResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        classes: item.classes.iter().map(kaspa_rpc_core::RpcPalwClassRow::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(&protowire::GetPalwNodeStatusRequestMessage, kaspa_rpc_core::GetPalwNodeStatusRequest);
try_from!(item: &protowire::GetPalwNodeStatusResponseMessage, RpcResult<kaspa_rpc_core::GetPalwNodeStatusResponse>, {
    Self {
        consensus_params_id: item.consensus_params_id.clone(),
        fence_schedule: item.fence_schedule.clone(),
        consensus_schedule_id: item.consensus_schedule_id.clone(),
        producer_state: item.producer_state.clone(),
        producer_reason: item.producer_reason.clone(),
        producer_since_unix: item.producer_since_unix,
        producer_bond: item.producer_bond.clone(),
        producer_class: item.producer_class.clone(),
        draws: item.draws,
        produced_blocks: item.produced_blocks,
        receipt_blocks: item.receipt_blocks,
        network_lost: item.network_lost,
        last_block: item.last_block.clone(),
        last_block_unix: item.last_block_unix,
        last_draw_unix: item.last_draw_unix,
        panel_running: item.panel_running,
        panel_submitter: item.panel_submitter,
        retention_dir: item.retention_dir.clone(),
        memory_share_bytes: item.memory_share_bytes,
        memory_headroom_bytes: item.memory_headroom_bytes,
        memory_reserved_bytes: item.memory_reserved_bytes,
        memory_available_bytes: item.memory_available_bytes,
        memory_bounded: item.memory_bounded,
        memory_holders: item.memory_holders.clone(),
        lane_window_blocks: item.lane_window_blocks,
        lane_work_blocks: item.lane_work_blocks,
        lane_heartbeat_blocks: item.lane_heartbeat_blocks,
        lane_last_work_daa: item.lane_last_work_daa,
        lane_mix: item.lane_mix.clone(),
        lane_alarm: item.lane_alarm.clone(),
        genesis_hash: item.genesis_hash.clone(),
        drill_salt_id: item.drill_salt_id.clone(),
    }
});
try_from!(&protowire::GetPalwRoundLaneRequestMessage, kaspa_rpc_core::GetPalwRoundLaneRequest);
try_from!(item: &protowire::RpcPalwRoundLaneStage, kaspa_rpc_core::RpcPalwRoundLaneStage, {
    Self {
        activation_daa: item.activation_daa,
        permits_per_round: u16::try_from(item.permits_per_round)
            .map_err(|_| RpcError::General(format!("permits_per_round {} is not a width", item.permits_per_round)))?,
    }
});
try_from!(item: &protowire::RpcPalwRoundPermit, kaspa_rpc_core::RpcPalwRoundPermit, {
    Self {
        index: u16::try_from(item.index).map_err(|_| RpcError::General(format!("permit index {} is not a permit", item.index)))?,
        bond: item.bond.clone(),
        operator_id: item.operator_id.clone(),
        domain: item.domain.clone(),
        used: item.used,
    }
});
try_from!(item: &protowire::RpcPalwRoundLaneDomain, kaspa_rpc_core::RpcPalwRoundLaneDomain, {
    Self {
        domain: item.domain.clone(),
        credits: item.credits,
        quota_permille: u16::try_from(item.quota_permille)
            .map_err(|_| RpcError::General(format!("quota {} is not a permille", item.quota_permille)))?,
        parity: u8::try_from(item.parity).map_err(|_| RpcError::General(format!("parity {} is not a parity", item.parity)))?,
        bonds: item.bonds,
    }
});
try_from!(item: &protowire::GetPalwRoundLaneResponseMessage, RpcResult<kaspa_rpc_core::GetPalwRoundLaneResponse>, {
    Self {
        armed: item.armed,
        open: item.open,
        schedule_span_daa: item.schedule_span_daa,
        max_per_mergeset: item.max_per_mergeset,
        stages: item.stages.iter().map(kaspa_rpc_core::RpcPalwRoundLaneStage::try_from).collect::<RpcResult<Vec<_>>>()?,
        virtual_daa: item.virtual_daa,
        round: item.round,
        span: item.span,
        permits_per_round: u16::try_from(item.permits_per_round)
            .map_err(|_| RpcError::General(format!("permits_per_round {} is not a width", item.permits_per_round)))?,
        permits: item.permits.iter().map(kaspa_rpc_core::RpcPalwRoundPermit::try_from).collect::<RpcResult<Vec<_>>>()?,
        domains: item.domains.iter().map(kaspa_rpc_core::RpcPalwRoundLaneDomain::try_from).collect::<RpcResult<Vec<_>>>()?,
        accepted_in_span: item.accepted_in_span,
        finals_span: item.finals_span,
        finals: item.finals,
        next_round_permits: u16::try_from(item.next_round_permits)
            .map_err(|_| RpcError::General(format!("next_round_permits {} is not a width", item.next_round_permits)))?,
    }
});
try_from!(item: &protowire::GetPalwSettlementRequestMessage, kaspa_rpc_core::GetPalwSettlementRequest, {
    Self { daa_score: item.daa_score }
});
try_from!(item: &protowire::GetPalwSettlementResponseMessage, RpcResult<kaspa_rpc_core::GetPalwSettlementResponse>, {
    Self {
        available: item.available,
        sink_daa: item.sink_daa,
        daa_score: item.daa_score,
        settled: item.settled,
        depth: item.depth,
        pending_anchors: item.pending_anchors,
        depth_is_lower_bound: item.depth_is_lower_bound,
        safe_frontier_blue_score: item.safe_frontier_blue_score,
        safe_frontier_daa: item.safe_frontier_daa,
    }
});
try_from!(item: &protowire::GetPrecommitDutyRequestMessage, kaspa_rpc_core::GetPrecommitDutyRequest, {
    Self { validator_id: item.validator_id.clone(), bond_outpoint: item.bond_outpoint.clone() }
});
try_from!(item: &protowire::RpcPrecommitDue, kaspa_rpc_core::RpcPrecommitDue, {
    Self {
        epoch: item.epoch,
        anchor_hash: item.anchor_hash.clone(),
        anchor_daa_score: item.anchor_daa_score,
        snapshot_commitment: item.snapshot_commitment.clone(),
    }
});
try_from!(item: &protowire::GetPrecommitDutyResponseMessage, RpcResult<kaspa_rpc_core::GetPrecommitDutyResponse>, {
    Self {
        available: item.available,
        round_active: item.round_active,
        sink_daa_score: item.sink_daa_score,
        held_epoch: item.held_epoch,
        held_anchor: item.held_anchor.clone(),
        due: item.due.iter().map(kaspa_rpc_core::RpcPrecommitDue::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(&protowire::GetPalwClassContextsRequestMessage, kaspa_rpc_core::GetPalwClassContextsRequest);
try_from!(item: &protowire::RpcPalwClassContext, kaspa_rpc_core::RpcPalwClassContext, {
    Self {
        class_id: item.class_id.clone(),
        model_id: item.model_id.clone(),
        n_ctx: item.n_ctx,
        canonical_prefill_tokens: item.canonical_prefill_tokens,
        canonical_decode_tokens: item.canonical_decode_tokens,
        canonical_footprint_positions: item.canonical_footprint_positions,
        max_context_tokens: item.max_context_tokens,
        source: item.source.clone(),
    }
});
try_from!(item: &protowire::GetPalwClassContextsResponseMessage, RpcResult<kaspa_rpc_core::GetPalwClassContextsResponse>, {
    Self {
        available: item.available,
        fp_max_prompt_tokens: item.fp_max_prompt_tokens,
        fp_max_decode_tokens: item.fp_max_decode_tokens,
        classes: item.classes.iter().map(kaspa_rpc_core::RpcPalwClassContext::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(&protowire::GetPalwClassEconomicsRequestMessage, kaspa_rpc_core::GetPalwClassEconomicsRequest);
try_from!(item: &protowire::RpcPalwClassLedgerTotals, kaspa_rpc_core::RpcPalwClassLedgerTotals, {
    Self {
        available: item.available,
        claims: item.claims,
        bound: item.bound,
        licensed: item.licensed,
        finals: item.finals,
        voided: item.voided,
        redrawn: item.redrawn,
        paid_at_acceptance: item.paid_at_acceptance,
        escrow_final_sompi: item.escrow_final_sompi.clone(),
        producer_named_sompi: item.producer_named_sompi.clone(),
        panel_named_sompi: item.panel_named_sompi.clone(),
        reserve_sompi: item.reserve_sompi.clone(),
        burned_sompi: item.burned_sompi.clone(),
        attempted_compute: item.attempted_compute.clone(),
        final_compute: item.final_compute.clone(),
        verification_compute: item.verification_compute.clone(),
        producer_per_attempted_compute: item.producer_per_attempted_compute.clone(),
        panel_per_verification_compute: item.panel_per_verification_compute.clone(),
        total_per_attempted_compute: item.total_per_attempted_compute.clone(),
        total_per_final_compute: item.total_per_final_compute.clone(),
        licence_rate_permille: item.licence_rate_permille,
        final_of_licensed_permille: item.final_of_licensed_permille,
        final_rate_permille: item.final_rate_permille,
        avg_bind_wait_daa: item.avg_bind_wait_daa,
        avg_licence_wait_daa: item.avg_licence_wait_daa,
        avg_final_wait_daa: item.avg_final_wait_daa,
        avg_void_wait_daa: item.avg_void_wait_daa,
        avg_expected_attempts_q32: item.avg_expected_attempts_q32.clone(),
        avg_network_expected_attempts_q32: item.avg_network_expected_attempts_q32.clone(),
        first_accepted_daa: item.first_accepted_daa,
        last_accepted_daa: item.last_accepted_daa,
    }
});
try_from!(item: &protowire::RpcPalwClassNodeTelemetry, kaspa_rpc_core::RpcPalwClassNodeTelemetry, {
    Self {
        available: item.available,
        draws: item.draws,
        class_wins: item.class_wins,
        produced: item.produced,
        draw_millis: item.draw_millis,
        storage_read_mib: item.storage_read_mib,
        replays: item.replays,
        replay_millis: item.replay_millis,
        replay_leaves: item.replay_leaves,
        receipts_valid: item.receipts_valid,
        receipts_unavailable: item.receipts_unavailable,
        receipts_incapable: item.receipts_incapable,
        receipts_other: item.receipts_other,
        openings_held: item.openings_held,
    }
});
try_from!(item: &protowire::RpcPalwClassEconomics, kaspa_rpc_core::RpcPalwClassEconomics, {
    Self {
        class_id: item.class_id.clone(),
        model_id: item.model_id.clone(),
        is_base_class: item.is_base_class,
        status: item.status.clone(),
        share_permille: u16::try_from(item.share_permille).map_err(|_| RpcError::General("sharePermille is not a u16".to_string()))?,
        pwu_per_inference: item.pwu_per_inference,
        class_target: item.class_target.clone(),
        expected_attempts: item.expected_attempts,
        expected_attempts_q32: item.expected_attempts_q32.clone(),
        economic_compute_job: item.economic_compute_job.clone(),
        economic_compute_canonical: item.economic_compute_canonical.clone(),
        economic_source: item.economic_source.clone(),
        claims_accepted: item.claims_accepted,
        claims_provisional: item.claims_provisional,
        claims_panel_bound: item.claims_panel_bound,
        claims_licensed: item.claims_licensed,
        claims_final: item.claims_final,
        claims_voided: item.claims_voided,
        claims_redrawn: item.claims_redrawn,
        escrow_accepted_sompi: item.escrow_accepted_sompi.clone(),
        escrow_final_sompi: item.escrow_final_sompi.clone(),
        ledger: item.ledger.as_ref().map(kaspa_rpc_core::RpcPalwClassLedgerTotals::try_from).transpose()?.unwrap_or_default(),
        telemetry: item.telemetry.as_ref().map(kaspa_rpc_core::RpcPalwClassNodeTelemetry::try_from).transpose()?.unwrap_or_default(),
        eligible_seats: item.eligible_seats,
        duty_seats_inflight: item.duty_seats_inflight,
        seat_exposure_inflight_sompi: item.seat_exposure_inflight_sompi.clone(),
        free_collateral_sompi: item.free_collateral_sompi.clone(),
    }
});
try_from!(item: &protowire::GetPalwClassEconomicsResponseMessage, RpcResult<kaspa_rpc_core::GetPalwClassEconomicsResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        economic_compute_version: u16::try_from(item.economic_compute_version)
            .map_err(|_| RpcError::General("economicComputeVersion is not a u16".to_string()))?,
        seat_count: u16::try_from(item.seat_count).map_err(|_| RpcError::General("seatCount is not a u16".to_string()))?,
        prefill_draw: item.prefill_draw,
        classes: item.classes.iter().map(kaspa_rpc_core::RpcPalwClassEconomics::try_from).collect::<RpcResult<Vec<_>>>()?,
        network_bits: item.network_bits,
        network_expected_attempts_q32: item.network_expected_attempts_q32.clone(),
        ledger_available: item.ledger_available,
        ledger_claims: item.ledger_claims,
        ledger_first_daa: item.ledger_first_daa,
        ledger_last_daa: item.ledger_last_daa,
    }
});
try_from!(&protowire::GetPalwModelRegistryRequestMessage, kaspa_rpc_core::GetPalwModelRegistryRequest);
try_from!(item: &protowire::GetPalwFreePromptPriceRequestMessage, kaspa_rpc_core::GetPalwFreePromptPriceRequest, {
    Self {
        class_id: item.class_id.clone(),
        prompt_token_ids: item.prompt_token_ids.clone(),
        prompt_tokens: item.prompt_tokens,
        decode_tokens_executed: item.decode_tokens_executed,
        work_leaves: item.work_leaves,
        bond: item.bond.clone(),
    }
});
try_from!(item: &protowire::GetPalwFreePromptPriceResponseMessage, RpcResult<kaspa_rpc_core::GetPalwFreePromptPriceResponse>, {
    Self {
        available: item.available,
        daa_score: item.daa_score,
        priced: item.priced,
        refusal: item.refusal.clone(),
        priced_in_compute: item.priced_in_compute,
        quanta: item.quanta,
        pwu: item.pwu,
        reserved_sompi: item.reserved_sompi.clone(),
        bond_room_sompi: item.bond_room_sompi.clone(),
    }
});
try_from!(item: &protowire::RpcPalwPanelHoldReason, kaspa_rpc_core::RpcPalwPanelHoldReason, {
    Self { code: item.code.clone(), message: item.message.clone() }
});
try_from!(item: &protowire::RpcPalwPanelHoldReasonCount, kaspa_rpc_core::RpcPalwPanelHoldReasonCount, {
    Self { code: item.code.clone(), message: item.message.clone(), seats: item.seats }
});
try_from!(item: &protowire::RpcPalwClassPanelStatus, kaspa_rpc_core::RpcPalwClassPanelStatus, {
    Self {
        class_id: item.class_id.clone(),
        model_name: item.model_name.clone(),
        registry_state: item.registry_state.clone(),
        bonded_seats: item.bonded_seats,
        ready_seats: item.ready_seats,
        required_ready_seats: item.required_ready_seats,
        selected_panel_seats: item.selected_panel_seats,
        valid_receipt_seats: item.valid_receipt_seats,
        panel_size: u16::try_from(item.panel_size).map_err(|_| RpcError::General("panelSize is not a u16".to_string()))?,
        receipt_quorum: u16::try_from(item.receipt_quorum).map_err(|_| RpcError::General("receiptQuorum is not a u16".to_string()))?,
        full_seats_per_panel: u16::try_from(item.full_seats_per_panel)
            .map_err(|_| RpcError::General("fullSeatsPerPanel is not a u16".to_string()))?,
        partial_seats_per_panel: u16::try_from(item.partial_seats_per_panel)
            .map_err(|_| RpcError::General("partialSeatsPerPanel is not a u16".to_string()))?,
        segment_count: u16::try_from(item.segment_count).map_err(|_| RpcError::General("segmentCount is not a u16".to_string()))?,
        inflight_claims: item.inflight_claims,
        active_assignments: item.active_assignments,
        admission_permille: item.admission_permille,
        verification_mode: item.verification_mode.clone(),
        s1_active: item.s1_active,
        s1_scheduled_daa: item.s1_scheduled_daa,
        s3_active: item.s3_active,
        s3_scheduled_daa: item.s3_scheduled_daa,
        s2_active: item.s2_active,
        s2_scheduled_daa: item.s2_scheduled_daa,
        holds_local: item.holds_local,
        missing: item.missing.iter().map(kaspa_rpc_core::RpcPalwPanelHoldReasonCount::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::RpcPalwPanelSeat, kaspa_rpc_core::RpcPalwPanelSeat, {
    Self {
        seat_id: item.seat_id.clone(),
        bond_outpoint: item.bond_outpoint.clone(),
        class_id: item.class_id.clone(),
        ready: item.ready,
        eligible: item.eligible,
        readiness_version: u8::try_from(item.readiness_version)
            .map_err(|_| RpcError::General("readinessVersion is not a u8".to_string()))?,
        readiness_proved_daa: item.readiness_proved_daa,
        readiness_expires_daa: item.readiness_expires_daa,
        collateral_available: item.collateral_available.clone(),
        collateral_locked: item.collateral_locked.clone(),
        assigned: item.assigned,
        hold: item.hold.as_ref().map(kaspa_rpc_core::RpcPalwPanelHoldReason::try_from).transpose()?,
    }
});
try_from!(item: &protowire::RpcPalwPanelAssignmentSeat, kaspa_rpc_core::RpcPalwPanelAssignmentSeat, {
    Self {
        seat_id: item.seat_id.clone(),
        seat_index: u16::try_from(item.seat_index).map_err(|_| RpcError::General("seatIndex is not a u16".to_string()))?,
        full_seat: item.full_seat,
        segment_index: if item.has_segment_index {
            Some(u16::try_from(item.segment_index).map_err(|_| RpcError::General("segmentIndex is not a u16".to_string()))?)
        } else {
            None
        },
        mask: item.mask,
        receipt_status: item.receipt_status.clone(),
        credited_daa: item.credited_daa,
    }
});
try_from!(item: &protowire::RpcPalwPanelAssignment, kaspa_rpc_core::RpcPalwPanelAssignment, {
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
try_from!(item: &protowire::RpcPalwLocalPanelClass, kaspa_rpc_core::RpcPalwLocalPanelClass, {
    Self {
        class_id: item.class_id.clone(),
        model_name: item.model_name.clone(),
        seat_id: item.seat_id.clone(),
        artifact_loaded: item.artifact_loaded,
        artifact_root: item.artifact_root.clone(),
        working_set_bytes: item.working_set_bytes,
        replay_capable: item.replay_capable,
        synced: item.synced,
        bond_active: item.bond_active,
        collateral_sompi: item.collateral_sompi,
        readiness_proof_accepted: item.readiness_proof_accepted,
        readiness_proved_daa: item.readiness_proved_daa,
        chain_state: item.chain_state.clone(),
        assignments: item.assignments,
        hold: item.hold.as_ref().map(kaspa_rpc_core::RpcPalwPanelHoldReason::try_from).transpose()?,
        runtime_profile: item.runtime_profile.clone(),
        artifact_resident_bytes: item.artifact_resident_bytes,
        producer_working_set_bytes: item.producer_working_set_bytes,
        full_seat_working_set_bytes: item.full_seat_working_set_bytes,
        partial_seat_working_set_bytes: item.partial_seat_working_set_bytes,
        producer_capable: item.producer_capable,
        full_seat_capable: item.full_seat_capable,
        partial_seat_capable: item.partial_seat_capable,
    }
});
try_from!(item: &protowire::GetPalwClassPanelStatusRequestMessage, kaspa_rpc_core::GetPalwClassPanelStatusRequest, {
    Self { class_id: item.class_id.clone() }
});
try_from!(item: &protowire::GetPalwClassPanelStatusResponseMessage, RpcResult<kaspa_rpc_core::GetPalwClassPanelStatusResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        found: item.found,
        status: item.status.as_ref().map(kaspa_rpc_core::RpcPalwClassPanelStatus::try_from).transpose()?.unwrap_or_default(),
    }
});
try_from!(item: &protowire::GetPalwPanelSeatsRequestMessage, kaspa_rpc_core::GetPalwPanelSeatsRequest, {
    Self { class_id: item.class_id.clone() }
});
try_from!(item: &protowire::GetPalwPanelSeatsResponseMessage, RpcResult<kaspa_rpc_core::GetPalwPanelSeatsResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        seats: item.seats.iter().map(kaspa_rpc_core::RpcPalwPanelSeat::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::GetPalwPanelStatusRequestMessage, kaspa_rpc_core::GetPalwPanelStatusRequest, {
    Self { class_id: item.class_id.clone() }
});
try_from!(item: &protowire::GetPalwPanelStatusResponseMessage, RpcResult<kaspa_rpc_core::GetPalwPanelStatusResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        panel_running: item.panel_running,
        panel_submitter: item.panel_submitter,
        synced: item.synced,
        classes: item.classes.iter().map(kaspa_rpc_core::RpcPalwLocalPanelClass::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::GetPalwPanelAssignmentsRequestMessage, kaspa_rpc_core::GetPalwPanelAssignmentsRequest, {
    Self { claim_id: item.claim_id.clone(), seat_id: item.seat_id.clone() }
});
try_from!(item: &protowire::GetPalwPanelAssignmentsResponseMessage, RpcResult<kaspa_rpc_core::GetPalwPanelAssignmentsResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        truncated: item.truncated,
        assignments: item.assignments.iter().map(kaspa_rpc_core::RpcPalwPanelAssignment::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::RpcPalwModelPreflightCheck, kaspa_rpc_core::RpcPalwModelPreflightCheck, {
    Self { code: item.code.clone(), ok: item.ok, message: item.message.clone() }
});
try_from!(item: &protowire::RpcPalwModelRegistration, kaspa_rpc_core::RpcPalwModelRegistration, {
    Self {
        object_id: item.object_id.clone(),
        class_id: item.class_id.clone(),
        constructed: item.constructed,
        submitted: item.submitted,
        accepted: item.accepted,
        included: item.included,
        folded: item.folded,
        submission_state: item.submission_state.clone(),
        processor_verdict: item.processor_verdict.clone(),
        reject_code: item.reject_code.clone(),
        mempool_accepted: item.mempool_accepted,
        included_block: item.included_block.clone(),
        included_daa: item.included_daa,
        registry_state: item.registry_state.clone(),
        transaction_id: item.transaction_id.clone(),
    }
});
try_from!(item: &protowire::GetPalwModelPreflightRequestMessage, kaspa_rpc_core::GetPalwModelPreflightRequest, {
    Self { object_hex: item.object_hex.clone(), class_id: item.class_id.clone() }
});
try_from!(item: &protowire::GetPalwModelPreflightResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelPreflightResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        class_id: item.class_id.clone(),
        artifact_root: item.artifact_root.clone(),
        n_ctx: item.n_ctx,
        layer_count: item.layer_count,
        admissible: item.admissible,
        processor_verdict: item.processor_verdict.clone(),
        reject_code: item.reject_code.clone(),
        checks: item.checks.iter().map(kaspa_rpc_core::RpcPalwModelPreflightCheck::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::SubmitPalwModelRegistrationRequestMessage, kaspa_rpc_core::SubmitPalwModelRegistrationRequest, {
    Self { object_hex: item.object_hex.clone(), transaction_id: item.transaction_id.clone() }
});
try_from!(item: &protowire::SubmitPalwModelRegistrationResponseMessage, RpcResult<kaspa_rpc_core::SubmitPalwModelRegistrationResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        registration: item.registration.as_ref().map(kaspa_rpc_core::RpcPalwModelRegistration::try_from).transpose()?.unwrap_or_default(),
        checks: item.checks.iter().map(kaspa_rpc_core::RpcPalwModelPreflightCheck::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::GetPalwModelRegistrationStatusRequestMessage, kaspa_rpc_core::GetPalwModelRegistrationStatusRequest, {
    Self { class_id: item.class_id.clone(), object_id: item.object_id.clone(), transaction_id: item.transaction_id.clone() }
});
try_from!(item: &protowire::GetPalwModelRegistrationStatusResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelRegistrationStatusResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        found: item.found,
        registration: item.registration.as_ref().map(kaspa_rpc_core::RpcPalwModelRegistration::try_from).transpose()?.unwrap_or_default(),
    }
});
try_from!(item: &protowire::GetPalwModelRequestMessage, kaspa_rpc_core::GetPalwModelRequest, {
    Self { class_id: item.class_id.clone() }
});
try_from!(item: &protowire::GetPalwModelResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        found: item.found,
        class_id: item.class_id.clone(),
        model_name: item.model_name.clone(),
        n_ctx: item.n_ctx,
        artifact_root: item.artifact_root.clone(),
        class_status: item.class_status.clone(),
        registry_state: item.registry_state.clone(),
        ready_seats: item.ready_seats,
        required_ready_seats: item.required_ready_seats,
        inflight_claims: item.inflight_claims,
        admission_permille: item.admission_permille,
        share_permille: item.share_permille.min(u32::from(u16::MAX)) as u16,
        certified_family: item.certified_family.clone(),
        fence_active: item.fence_active,
        reason: item.reason.clone(),
    }
});
try_from!(item: &protowire::RpcPalwModelSeatReadiness, kaspa_rpc_core::RpcPalwModelSeatReadiness, {
    Self {
        seat_id: item.seat_id.clone(),
        bond_txid: item.bond_txid.clone(),
        bond_index: item.bond_index,
        proved_daa: item.proved_daa,
        proved_span: item.proved_span,
        expires_daa: item.expires_daa,
        fresh: item.fresh,
        ready: item.ready,
        collateral_sompi: item.collateral_sompi,
        needed_collateral_sompi: item.needed_collateral_sompi,
        not_ready_reason: item.not_ready_reason.clone(),
    }
});
try_from!(item: &protowire::GetPalwModelReadinessRequestMessage, kaspa_rpc_core::GetPalwModelReadinessRequest, {
    Self { class_id: item.class_id.clone() }
});
try_from!(item: &protowire::GetPalwModelReadinessResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelReadinessResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        found: item.found,
        class_id: item.class_id.clone(),
        registry_state: item.registry_state.clone(),
        ready_seats: item.ready_seats,
        required_ready_seats: item.required_ready_seats,
        seats: item.seats.iter().map(kaspa_rpc_core::RpcPalwModelSeatReadiness::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::GetPalwModelAdmissionRequestMessage, kaspa_rpc_core::GetPalwModelAdmissionRequest, {
    Self { class_id: item.class_id.clone(), object_hex: item.object_hex.clone() }
});
try_from!(item: &protowire::GetPalwModelAdmissionResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelAdmissionResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        class_id: item.class_id.clone(),
        admissible: item.admissible,
        processor_verdict: item.processor_verdict.clone(),
        reject_code: item.reject_code.clone(),
        checks: item.checks.iter().map(kaspa_rpc_core::RpcPalwModelPreflightCheck::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::RpcPalwModelCertifiedFamily, kaspa_rpc_core::RpcPalwModelCertifiedFamily, {
    Self { lane: item.lane.clone(), digest: item.digest.clone(), covers: item.covers }
});
try_from!(item: &protowire::GetPalwModelCertificationRequestMessage, kaspa_rpc_core::GetPalwModelCertificationRequest, {
    Self { class_id: item.class_id.clone() }
});
try_from!(item: &protowire::GetPalwModelCertificationResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelCertificationResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        found: item.found,
        class_id: item.class_id.clone(),
        end_to_end_certified: item.end_to_end_certified,
        families: item.families.iter().map(kaspa_rpc_core::RpcPalwModelCertifiedFamily::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::GetPalwActivationPoolRequestMessage, kaspa_rpc_core::GetPalwActivationPoolRequest, {
    Self { class_id: item.class_id.clone() }
});
try_from!(item: &protowire::GetPalwActivationPoolResponseMessage, RpcResult<kaspa_rpc_core::GetPalwActivationPoolResponse>, {
    Self {
        available: item.available,
        pool_armed: item.pool_armed,
        tip_daa: item.tip_daa,
        class_found: item.class_found,
        class_id: item.class_id.clone(),
        class_status: item.class_status.clone(),
        lifecycle: item.lifecycle.clone(),
        has_pool: item.has_pool,
        prep_sompi: item.prep_sompi,
        bonus_sompi: item.bonus_sompi,
        funded_sompi: item.funded_sompi,
        paid_sompi: item.paid_sompi,
        withheld_sompi: item.withheld_sompi,
        opened_daa: item.opened_daa,
        prep_paid: item.prep_paid.clone(),
        bonus_paid: item.bonus_paid.clone(),
        probe_credited: item.probe_credited.clone(),
        registrant_operator: item.registrant_operator.clone(),
        prep_reward_now_sompi: item.prep_reward_now_sompi,
        prep_cap_now_sompi: item.prep_cap_now_sompi,
        next_audit_span: item.next_audit_span,
        span_daa: item.span_daa,
        sink_script: item.sink_script.clone(),
        min_topup_sompi: item.min_topup_sompi,
        prep_base_sompi: item.prep_base_sompi,
        prep_share_permille: item.prep_share_permille,
        bonus_share_permille: item.bonus_share_permille,
        ramp_daa: item.ramp_daa,
        prep_payee_cap: item.prep_payee_cap,
        bonus_payee_cap: item.bonus_payee_cap,
        total_funded_sompi: item.total_funded_sompi,
        total_paid_sompi: item.total_paid_sompi,
        total_withheld_sompi: item.total_withheld_sompi,
        total_available_sompi: item.total_available_sompi,
        scheduled_sompi: item.scheduled_sompi,
        class_is_floor: item.class_is_floor,
    }
});
try_from!(item: &protowire::GetPalwVestingRequestMessage, kaspa_rpc_core::GetPalwVestingRequest, {
    Self {
        bond: item.bond.clone(),
        payout_address: item.payout_address.clone(),
        claim_id: item.claim_id.clone(),
        limit: item.limit,
        after: item.after.clone(),
    }
});
try_from!(item: &protowire::RpcPalwVestingLeg, kaspa_rpc_core::RpcPalwVestingLeg, {
    Self {
        kind: item.kind.clone(),
        payee_bond: item.payee_bond.clone(),
        payload: item.payload.clone(),
        sompi: item.sompi,
        queue_key: item.queue_key.clone(),
    }
});
try_from!(item: &protowire::RpcPalwVestingRow, kaspa_rpc_core::RpcPalwVestingRow, {
    Self {
        claim_id: item.claim_id.clone(),
        class_id: item.class_id.clone(),
        producer_bond: item.producer_bond.clone(),
        licence_door: item.licence_door.clone(),
        basis_k: item.basis_k,
        escrow_sompi: item.escrow_sompi,
        buyback_bound_sompi: item.buyback_bound_sompi,
        total_sompi: item.total_sompi,
        reserve_sompi: item.reserve_sompi,
        final_daa: item.final_daa,
        expiry_daa: item.expiry_daa,
        settled_at_final: item.settled_at_final,
        matured_at: item.matured_at,
        stage: item.stage.clone(),
        daa_clock_met: item.daa_clock_met,
        licences_since_final: item.licences_since_final,
        licences_needed: item.licences_needed,
        second_clock_bound_daa: item.second_clock_bound_daa,
        da_session_open: item.da_session_open,
        mature_now: item.mature_now,
        lock_live: item.lock_live,
        moves_ahead: item.moves_ahead,
        keys_ahead: item.keys_ahead,
        in_next_block: item.in_next_block,
        eta_daa: item.eta_daa,
        eta_estimated: item.eta_estimated,
        legs: item.legs.iter().map(kaspa_rpc_core::RpcPalwVestingLeg::try_from).collect::<RpcResult<Vec<_>>>()?,
        legs_sompi: item.legs_sompi,
    }
});
try_from!(item: &protowire::RpcPalwReporterReward, kaspa_rpc_core::RpcPalwReporterReward, {
    Self {
        offence_key: item.offence_key.clone(),
        stage: item.stage.clone(),
        reporter_bond: item.reporter_bond.clone(),
        payload: item.payload.clone(),
        sompi: item.sompi,
        reveal_until: item.reveal_until,
        in_next_block: item.in_next_block,
    }
});
try_from!(item: &protowire::RpcPalwVestingMove, kaspa_rpc_core::RpcPalwVestingMove, {
    Self {
        source: item.source.clone(),
        id: item.id.clone(),
        legs: item.legs.iter().map(kaspa_rpc_core::RpcPalwVestingLeg::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::RpcPalwVestingDoorCount, kaspa_rpc_core::RpcPalwVestingDoorCount, {
    Self { door: item.door.clone(), rows: item.rows, latched_rows: item.latched_rows, sompi: item.sompi.clone() }
});
try_from!(item: &protowire::GetPalwVestingResponseMessage, RpcResult<kaspa_rpc_core::GetPalwVestingResponse>, {
    Self {
        available: item.available,
        rcore_plus_active: item.rcore_plus_active,
        tip_daa: item.tip_daa,
        next_daa: item.next_daa,
        halted: item.halted,
        second_clock_depth: item.second_clock_depth,
        second_clock_escaped_depth: item.second_clock_escaped_depth,
        settled_anchors: item.settled_anchors,
        measured_ms_per_daa: item.measured_ms_per_daa,
        created_sompi: item.created_sompi.clone(),
        moved_sompi: item.moved_sompi.clone(),
        burned_sompi: item.burned_sompi.clone(),
        live_rows: item.live_rows,
        latched_rows: item.latched_rows,
        live_sompi: item.live_sompi.clone(),
        latched_sompi: item.latched_sompi.clone(),
        latched_behind_head: item.latched_behind_head,
        reporter_pending_rows: item.reporter_pending_rows,
        reporter_awarded_rows: item.reporter_awarded_rows,
        next_block_moves: item
            .next_block_moves
            .iter()
            .map(kaspa_rpc_core::RpcPalwVestingMove::try_from)
            .collect::<RpcResult<Vec<_>>>()?,
        next_block_legs: item.next_block_legs,
        next_block_new_keys: item.next_block_new_keys,
        next_block_stopped: item.next_block_stopped.clone(),
        next_block_stopped_at: item.next_block_stopped_at.clone(),
        backlog_keys: item.backlog_keys,
        backlog_blocks_est: item.backlog_blocks_est,
        licence_histogram: item
            .licence_histogram
            .iter()
            .map(kaspa_rpc_core::RpcPalwVestingDoorCount::try_from)
            .collect::<RpcResult<Vec<_>>>()?,
        bond: item.bond.clone(),
        payout_address: item.payout_address.clone(),
        claim_id: item.claim_id.clone(),
        claim_stage: item.claim_stage.clone(),
        bond_known: item.bond_known,
        payee_holds_collateral: item.payee_holds_collateral,
        lock_live_rows: item.lock_live_rows,
        lock_live_last_expiry_daa: item.lock_live_last_expiry_daa,
        rows: item.rows.iter().map(kaspa_rpc_core::RpcPalwVestingRow::try_from).collect::<RpcResult<Vec<_>>>()?,
        rows_total: item.rows_total,
        next_after: item.next_after.clone(),
        maturing_sompi: item.maturing_sompi.clone(),
        query_latched_sompi: item.query_latched_sompi.clone(),
        reporter_rewards: item
            .reporter_rewards
            .iter()
            .map(kaspa_rpc_core::RpcPalwReporterReward::try_from)
            .collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::RpcPalwModelLifecycle, kaspa_rpc_core::RpcPalwModelLifecycle, {
    Self {
        class_id: item.class_id.clone(),
        artifact_root: item.artifact_root.clone(),
        no_capable_panel_voids: item.no_capable_panel_voids,
        reason: item.reason.clone(),
        is_base_class: item.is_base_class,
        has_row: item.has_row,
        state: item.state.clone(),
        since_span: item.since_span,
        verification_ccu: item.verification_ccu.clone(),
        economic_ccu_per_claim: item.economic_ccu_per_claim.clone(),
        artifact_bytes: item.artifact_bytes,
        ops_supported: item.ops_supported,
        verification_window_spans: item.verification_window_spans,
        artifact_prefetch_spans: item.artifact_prefetch_spans,
        max_inflight_claims: item.max_inflight_claims,
        required_ready_seats: item.required_ready_seats,
        registration_bond_sompi: item.registration_bond_sompi,
        admission_claims_per_span_milli: item.admission_claims_per_span_milli,
        probes_passed: item.probes_passed,
        probes_failed: item.probes_failed,
        ready_seats: item.ready_seats,
        inflight_claims: item.inflight_claims,
        utilization_permille: item.utilization_permille,
        admission_milli: item.admission_milli,
        cap_utilization_permille: item.cap_utilization_permille,
        priced_share_permille: u16::try_from(item.priced_share_permille)
            .map_err(|_| RpcError::General("pricedSharePermille is not a u16".to_string()))?,
        work_ratio_permille: item.work_ratio_permille,
        expected_forwards_q32: item.expected_forwards_q32.clone(),
        work_ticket_target: item.work_ticket_target.clone(),
        class_target: item.class_target.clone(),
        panel_room: item.panel_room,
        final_work_share_10_permille: u16::try_from(item.final_work_share10_permille)
            .map_err(|_| RpcError::General("finalWorkShare10Permille is not a u16".to_string()))?,
        final_work_share_100_permille: u16::try_from(item.final_work_share100_permille)
            .map_err(|_| RpcError::General("finalWorkShare100Permille is not a u16".to_string()))?,
        ready_seats_now: item.ready_seats_now,
        inflight_now: item.inflight_now,
        share_permille: u16::try_from(item.share_permille).map_err(|_| RpcError::General("sharePermille is not a u16".to_string()))?,
    }
});
try_from!(item: &protowire::RpcPalwSeatReadiness, kaspa_rpc_core::RpcPalwSeatReadiness, {
    Self {
        bond_txid: item.bond_txid.clone(),
        bond_index: item.bond_index,
        class_id: item.class_id.clone(),
        proved_daa: item.proved_daa,
        proved_span: item.proved_span,
        leaf_index: item.leaf_index,
        fresh: item.fresh,
        not_ready_reason: item.not_ready_reason.clone(),
    }
});
try_from!(item: &protowire::GetPalwModelRegistryResponseMessage, RpcResult<kaspa_rpc_core::GetPalwModelRegistryResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        scheduled: item.scheduled,
        fence_daa: item.fence_daa,
        active: item.active,
        grace_until_daa: item.grace_until_daa,
        span_daa: item.span_daa,
        reference_work_per_span: item.reference_work_per_span.clone(),
        reference_bytes_per_span: item.reference_bytes_per_span,
        seat_count: u16::try_from(item.seat_count).map_err(|_| RpcError::General("seatCount is not a u16".to_string()))?,
        spare_seats: u16::try_from(item.spare_seats).map_err(|_| RpcError::General("spareSeats is not a u16".to_string()))?,
        utilization_permille: item.utilization_permille,
        probation_claims: item.probation_claims,
        stable_epochs: item.stable_epochs,
        readiness_probe_max_age_spans: item.readiness_probe_max_age_spans,
        readiness_collateral_multiple: item.readiness_collateral_multiple,
        classes: item.classes.iter().map(kaspa_rpc_core::RpcPalwModelLifecycle::try_from).collect::<RpcResult<Vec<_>>>()?,
        readiness: item.readiness.iter().map(kaspa_rpc_core::RpcPalwSeatReadiness::try_from).collect::<RpcResult<Vec<_>>>()?,
        classes_active: item.classes_active,
        classes_active_limited: item.classes_active_limited,
        classes_probation: item.classes_probation,
        classes_prefetching: item.classes_prefetching,
        classes_registered: item.classes_registered,
        classes_held: item.classes_held,
        bonds_active: item.bonds_active,
        bonds_with_headroom: item.bonds_with_headroom,
        work_target_shadow: item.work_target_shadow,
        work_target: item.work_target.clone(),
        work_floor: item.work_floor.clone(),
        work_network_draws_q32: item.work_network_draws_q32.clone(),
        work_effective: item.work_effective.clone(),
        work_epoch_index: item.work_epoch_index,
        work_closed_model_blocks: item.work_closed_model_blocks,
        work_closed_expected_blocks: item.work_closed_expected_blocks,
        work_rate_sompi_per_giga: item.work_rate_sompi_per_giga,
        panel_inflight_replay: item.panel_inflight_replay.clone(),
        panel_horizon_spans: item.panel_horizon_spans,
        final_work_epochs: item.final_work_epochs,
    }
});
try_from!(&protowire::GetPalwRegistrationTermsRequestMessage, kaspa_rpc_core::GetPalwRegistrationTermsRequest);
try_from!(item: &protowire::RpcPalwCertifiedFamily, kaspa_rpc_core::RpcPalwCertifiedFamily, {
    Self { lane: item.lane.clone(), digest: item.digest.clone(), certified_daa: item.certified_daa, family_hex: item.family_hex.clone() }
});
try_from!(item: &protowire::GetPalwRegistrationTermsResponseMessage, RpcResult<kaspa_rpc_core::GetPalwRegistrationTermsResponse>, {
    Self {
        available: item.available,
        tip_daa: item.tip_daa,
        base_class_id: item.base_class_id.clone(),
        min_grantable_share_permille: u16::try_from(item.min_grantable_share_permille)
            .map_err(|_| RpcError::General(format!("min_grantable_share_permille {} is not a permille", item.min_grantable_share_permille)))?,
        slash_value_per_pwu: item.slash_value_per_pwu,
        initial_target: item.initial_target.clone(),
        registered_class_ids: item.registered_class_ids.clone(),
        registered_artifact_roots: item.registered_artifact_roots.clone(),
        families: item.families.iter().map(kaspa_rpc_core::RpcPalwCertifiedFamily::try_from).collect::<RpcResult<Vec<_>>>()?,
    }
});
try_from!(item: &protowire::GetTokenSupplyRequestMessage, kaspa_rpc_core::GetTokenSupplyRequest, { Self { asset_id: item.asset_id } });
try_from!(item: &protowire::GetTokenSupplyResponseMessage, RpcResult<kaspa_rpc_core::GetTokenSupplyResponse>, {
    Self {
        available: item.available,
        minted: item.minted.clone(),
        burned: item.burned.clone(),
        circulating: item.circulating.clone(),
    }
});
try_from!(item: &protowire::GetTokenEmissionInfoRequestMessage, kaspa_rpc_core::GetTokenEmissionInfoRequest, {
    Self { epoch: item.epoch, latest: item.latest }
});
try_from!(item: &protowire::GetTokenEmissionInfoResponseMessage, RpcResult<kaspa_rpc_core::GetTokenEmissionInfoResponse>, {
    Self {
        available: item.available,
        epoch: item.epoch,
        settled: item.settled,
        budget: item.budget.clone(),
        network_compute: item.network_compute.clone(),
        paid_total: item.paid_total.clone(),
        audit_paid: item.audit_paid.clone(),
        reward_count: item.reward_count,
        settlement_root: item.settlement_root.clone(),
        next_settlement_epoch: item.next_settlement_epoch,
        fold_cursor: item.fold_cursor,
    }
});
try_from!(item: &protowire::GetDnsConfirmationResponseMessage, RpcResult<kaspa_rpc_core::GetDnsConfirmationResponse>, {
    Self {
        available: item.available,
        block_hash: item.block_hash.clone(),
        work_depth: item.work_depth.clone(),
        required_work_depth: item.required_work_depth.clone(),
        stake_depth: item.stake_depth.clone(),
        required_stake_depth: item.required_stake_depth.clone(),
        pow_confirmed: item.pow_confirmed,
        dns_confirmed: item.dns_confirmed,
        rollout_stage: item.rollout_stage,
        expected_dns_confirmation_seconds: item.expected_dns_confirmation_seconds,
        work_reorg_risk_upper_bound: item.work_reorg_risk_upper_bound.clone(),
        stake_reorg_risk_upper_bound: item.stake_reorg_risk_upper_bound.clone(),
        dns_reorg_risk_conservative_bound: item.dns_reorg_risk_conservative_bound.clone(),
        note: item.note.clone(),
        health: item.health,
        last_dns_confirmed_anchor: item.last_dns_confirmed_anchor.clone(),
        last_dns_confirmed_anchor_daa_score: item.last_dns_confirmed_anchor_daa_score,
        block_found: item.block_found,
        block_is_dns_final: item.block_is_dns_final,
        block_is_confirmed_anchor: item.block_is_confirmed_anchor,
        block_daa_score: item.block_daa_score,
        vlt_state: item.vlt_state.clone(),
        vlt_shadow_active: item.vlt_shadow_active,
        vlt_weight_fence_reached: item.vlt_weight_fence_reached,
        vlt_finality_active: item.vlt_finality_active,
        vlt_total_weight: item.vlt_total_weight.clone(),
        vlt_quorum_weight: item.vlt_quorum_weight.clone(),
        vlt_snapshot_epoch: item.vlt_snapshot_epoch,
        vlt_snapshot_root: item.vlt_snapshot_root.clone(),
        vlt_gauges_daa_score: item.vlt_gauges_daa_score,
    }
});

try_from!(item: &protowire::SubmitEvmTransactionRequestMessage, kaspa_rpc_core::SubmitEvmTransactionRequest, {
    Self { transaction: item.transaction.clone() }
});
try_from!(item: &protowire::SubmitEvmTransactionResponseMessage, RpcResult<kaspa_rpc_core::SubmitEvmTransactionResponse>, {
    Self { transaction_hash: item.transaction_hash.clone() }
});
try_from!(item: &protowire::SubmitEvmDepositClaimRequestMessage, kaspa_rpc_core::SubmitEvmDepositClaimRequest, {
    Self { transaction_id: item.transaction_id.clone(), index: item.index }
});
try_from!(item: &protowire::SubmitEvmDepositClaimResponseMessage, RpcResult<kaspa_rpc_core::SubmitEvmDepositClaimResponse>, {
    Self { evm_address: item.evm_address.clone(), amount_sompi: item.amount_sompi, claim_tip_sompi: item.claim_tip_sompi }
});

try_from!(item: &protowire::GetEvmTransactionReceiptRequestMessage, kaspa_rpc_core::GetEvmTransactionReceiptRequest, {
    Self { transaction_hash: item.transaction_hash.clone() }
});
try_from!(item: &protowire::GetEvmTransactionReceiptResponseMessage, RpcResult<kaspa_rpc_core::GetEvmTransactionReceiptResponse>, {
    Self {
        found: item.found,
        accepting_block: item.accepting_block.clone(),
        evm_number: item.evm_number,
        receipt_index: item.receipt_index,
        succeeded: item.succeeded,
        gas_used: item.gas_used,
        cumulative_gas_used: item.cumulative_gas_used,
        logs: item
            .logs
            .iter()
            .map(|l| kaspa_rpc_core::RpcEvmLog { address: l.address.clone(), topics: l.topics.clone(), data: l.data.clone() })
            .collect(),
    }
});
try_from!(item: &protowire::GetEvmTxInclusionStatusRequestMessage, kaspa_rpc_core::GetEvmTxInclusionStatusRequest, {
    Self { transaction_hash: item.transaction_hash.clone() }
});
try_from!(item: &protowire::GetEvmTxInclusionStatusResponseMessage, RpcResult<kaspa_rpc_core::GetEvmTxInclusionStatusResponse>, {
    Self {
        pending: item.pending,
        included_in: item.included_in.clone(),
        accepted_in: item.accepted_in.clone(),
        receipt_index: item.receipt_index,
        last_skip_class: item.last_skip_class,
    }
});

try_from!(&protowire::GetValidatorStatusRequestMessage, kaspa_rpc_core::GetValidatorStatusRequest);
try_from!(item: &protowire::GetValidatorStatusResponseMessage, RpcResult<kaspa_rpc_core::GetValidatorStatusResponse>, {
    Self {
        enabled: item.enabled,
        mode: item.mode.clone(),
        has_key: item.has_key,
        validator_id: item.validator_id.clone(),
        funding_address: item.funding_address.clone(),
        overlay_configured: item.overlay_configured,
        epoch: item.epoch,
        bond_status: item.bond_status.clone(),
        is_active_validator: item.is_active_validator,
        has_signed_epoch: item.has_signed_epoch,
        last_signed_epoch: item.last_signed_epoch,
        status: item.status,
        status_label: item.status_label.clone(),
    }
});

try_from!(item: &protowire::BanRequestMessage, kaspa_rpc_core::BanRequest, { Self { ip: RpcIpAddress::from_str(&item.ip)? } });
try_from!(&protowire::BanResponseMessage, RpcResult<kaspa_rpc_core::BanResponse>);

try_from!(item: &protowire::UnbanRequestMessage, kaspa_rpc_core::UnbanRequest, { Self { ip: RpcIpAddress::from_str(&item.ip)? } });
try_from!(&protowire::UnbanResponseMessage, RpcResult<kaspa_rpc_core::UnbanResponse>);

try_from!(item: &protowire::EstimateNetworkHashesPerSecondRequestMessage, kaspa_rpc_core::EstimateNetworkHashesPerSecondRequest, {
    Self {
        window_size: item.window_size,
        start_hash: if item.start_hash.is_empty() { None } else { Some(RpcHash::from_str(&item.start_hash)?) },
    }
});
try_from!(
    item: &protowire::EstimateNetworkHashesPerSecondResponseMessage,
    RpcResult<kaspa_rpc_core::EstimateNetworkHashesPerSecondResponse>,
    { Self { network_hashes_per_second: item.network_hashes_per_second } }
);

try_from!(item: &protowire::GetMempoolEntriesByAddressesRequestMessage, kaspa_rpc_core::GetMempoolEntriesByAddressesRequest, {
    Self {
        addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()?,
        include_orphan_pool: item.include_orphan_pool,
        filter_transaction_pool: item.filter_transaction_pool,
    }
});
try_from!(
    item: &protowire::GetMempoolEntriesByAddressesResponseMessage,
    RpcResult<kaspa_rpc_core::GetMempoolEntriesByAddressesResponse>,
    { Self { entries: item.entries.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()? } }
);

try_from!(&protowire::GetCoinSupplyRequestMessage, kaspa_rpc_core::GetCoinSupplyRequest);
try_from!(item: &protowire::GetCoinSupplyResponseMessage, RpcResult<kaspa_rpc_core::GetCoinSupplyResponse>, {
    Self { max_sompi: item.max_sompi, circulating_sompi: item.circulating_sompi }
});

try_from!(item: &protowire::GetDaaScoreTimestampEstimateRequestMessage, kaspa_rpc_core::GetDaaScoreTimestampEstimateRequest , {
    Self {
        daa_scores: item.daa_scores.clone()
    }
});
try_from!(item: &protowire::GetDaaScoreTimestampEstimateResponseMessage, RpcResult<kaspa_rpc_core::GetDaaScoreTimestampEstimateResponse>, {
    Self { timestamps: item.timestamps.clone() }
});

try_from!(&protowire::GetFeeEstimateRequestMessage, kaspa_rpc_core::GetFeeEstimateRequest);
try_from!(item: &protowire::GetFeeEstimateResponseMessage, RpcResult<kaspa_rpc_core::GetFeeEstimateResponse>, {
    Self {
        estimate: item.estimate
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("GetFeeEstimateResponseMessage".to_string(), "estimate".to_string()))?
            .try_into()?
    }
});
try_from!(item: &protowire::GetFeeEstimateExperimentalRequestMessage, kaspa_rpc_core::GetFeeEstimateExperimentalRequest, {
    Self {
        verbose: item.verbose
    }
});
try_from!(item: &protowire::GetFeeEstimateExperimentalResponseMessage, RpcResult<kaspa_rpc_core::GetFeeEstimateExperimentalResponse>, {
    Self {
        estimate: item.estimate
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("GetFeeEstimateExperimentalResponseMessage".to_string(), "estimate".to_string()))?
            .try_into()?,
        verbose: item.verbose.as_ref().map(|x| x.try_into()).transpose()?
    }
});

try_from!(item: &protowire::GetCurrentBlockColorRequestMessage, kaspa_rpc_core::GetCurrentBlockColorRequest, {
    Self {
        hash: RpcHash::from_str(&item.hash)?
    }
});
try_from!(item: &protowire::GetCurrentBlockColorResponseMessage, RpcResult<kaspa_rpc_core::GetCurrentBlockColorResponse>, {
    Self {
        blue: item.blue
    }
});
try_from!(item: &protowire::GetUtxoReturnAddressRequestMessage, kaspa_rpc_core::GetUtxoReturnAddressRequest , {
    Self {
        // PR-9.5f: txid widened to Hash64.
        txid: kaspa_consensus_core::Hash64::from_str(&item.txid).unwrap_or_default(),
        accepting_block_daa_score: item.accepting_block_daa_score
    }
});
try_from!(item: &protowire::GetUtxoReturnAddressResponseMessage, RpcResult<kaspa_rpc_core::GetUtxoReturnAddressResponse>, {
    Self { return_address: Address::try_from(item.return_address.clone())? }
});

try_from!(&protowire::PingRequestMessage, kaspa_rpc_core::PingRequest);
try_from!(&protowire::PingResponseMessage, RpcResult<kaspa_rpc_core::PingResponse>);

try_from!(item: &protowire::GetMetricsRequestMessage, kaspa_rpc_core::GetMetricsRequest, {
    Self {
        process_metrics: item.process_metrics,
        connection_metrics: item.connection_metrics,
        bandwidth_metrics:item.bandwidth_metrics,
        consensus_metrics: item.consensus_metrics,
        storage_metrics: item.storage_metrics,
        custom_metrics : item.custom_metrics,
    }
});
try_from!(item: &protowire::GetMetricsResponseMessage, RpcResult<kaspa_rpc_core::GetMetricsResponse>, {
    Self {
        server_time: item.server_time,
        process_metrics: item.process_metrics.as_ref().map(|x| x.try_into()).transpose()?,
        connection_metrics: item.connection_metrics.as_ref().map(|x| x.try_into()).transpose()?,
        bandwidth_metrics: item.bandwidth_metrics.as_ref().map(|x| x.try_into()).transpose()?,
        consensus_metrics: item.consensus_metrics.as_ref().map(|x| x.try_into()).transpose()?,
        storage_metrics: item.storage_metrics.as_ref().map(|x| x.try_into()).transpose()?,
        // TODO
        custom_metrics: None,
    }
});

try_from!(item: &protowire::GetConnectionsRequestMessage, kaspa_rpc_core::GetConnectionsRequest, {
    Self { include_profile_data : item.include_profile_data }
});
try_from!(item: &protowire::GetConnectionsResponseMessage, RpcResult<kaspa_rpc_core::GetConnectionsResponse>, {
    Self {
        clients: item.clients,
        peers: item.peers as u16,
        profile_data: item.profile_data.as_ref().map(|x| x.try_into()).transpose()?,
    }
});

try_from!(&protowire::GetSystemInfoRequestMessage, kaspa_rpc_core::GetSystemInfoRequest);
try_from!(item: &protowire::GetSystemInfoResponseMessage, RpcResult<kaspa_rpc_core::GetSystemInfoResponse>, {
    Self {
        version: item.version.clone(),
        system_id: (!item.system_id.is_empty()).then(|| FromHex::from_hex(&item.system_id)).transpose()?,
        git_hash: (!item.git_hash.is_empty()).then(|| FromHex::from_hex(&item.git_hash)).transpose()?,
        total_memory: item.total_memory,
        cpu_physical_cores: item.core_num as u16,
        fd_limit: item.fd_limit,
        proxy_socket_limit_per_cpu_core : (item.proxy_socket_limit_per_cpu_core > 0).then_some(item.proxy_socket_limit_per_cpu_core),
    }
});

try_from!(&protowire::GetServerInfoRequestMessage, kaspa_rpc_core::GetServerInfoRequest);
try_from!(item: &protowire::GetServerInfoResponseMessage, RpcResult<kaspa_rpc_core::GetServerInfoResponse>, {
    Self {
        rpc_api_version: item.rpc_api_version as u16,
        rpc_api_revision: item.rpc_api_revision as u16,
        server_version: item.server_version.clone(),
        network_id: NetworkId::from_str(&item.network_id)?,
        has_utxo_index: item.has_utxo_index,
        is_synced: item.is_synced,
        virtual_daa_score: item.virtual_daa_score,
    }
});

try_from!(&protowire::GetSyncStatusRequestMessage, kaspa_rpc_core::GetSyncStatusRequest);
try_from!(item: &protowire::GetSyncStatusResponseMessage, RpcResult<kaspa_rpc_core::GetSyncStatusResponse>, {
    Self {
        is_synced: item.is_synced,
    }
});

try_from!(item: &protowire::NotifyUtxosChangedRequestMessage, kaspa_rpc_core::NotifyUtxosChangedRequest, {
    Self {
        addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()?,
        command: item.command.into(),
    }
});
try_from!(item: &protowire::StopNotifyingUtxosChangedRequestMessage, kaspa_rpc_core::NotifyUtxosChangedRequest, {
    Self {
        addresses: item.addresses.iter().map(|x| x.as_str().try_into()).collect::<Result<Vec<_>, _>>()?,
        command: Command::Stop,
    }
});
try_from!(&protowire::NotifyUtxosChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifyUtxosChangedResponse>);
try_from!(&protowire::StopNotifyingUtxosChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifyUtxosChangedResponse>);

try_from!(
    item: &protowire::NotifyPruningPointUtxoSetOverrideRequestMessage,
    kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideRequest,
    { Self { command: item.command.into() } }
);
try_from!(
    _item: &protowire::StopNotifyingPruningPointUtxoSetOverrideRequestMessage,
    kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideRequest,
    { Self { command: Command::Stop } }
);
try_from!(
    &protowire::NotifyPruningPointUtxoSetOverrideResponseMessage,
    RpcResult<kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideResponse>
);
try_from!(
    &protowire::StopNotifyingPruningPointUtxoSetOverrideResponseMessage,
    RpcResult<kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideResponse>
);

try_from!(item: &protowire::NotifyFinalityConflictRequestMessage, kaspa_rpc_core::NotifyFinalityConflictRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyFinalityConflictResponseMessage, RpcResult<kaspa_rpc_core::NotifyFinalityConflictResponse>);

try_from!(item: &protowire::NotifyVirtualDaaScoreChangedRequestMessage, kaspa_rpc_core::NotifyVirtualDaaScoreChangedRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifyVirtualDaaScoreChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifyVirtualDaaScoreChangedResponse>);

try_from!(item: &protowire::NotifyVirtualChainChangedRequestMessage, kaspa_rpc_core::NotifyVirtualChainChangedRequest, {
    Self { include_accepted_transaction_ids: item.include_accepted_transaction_ids, command: item.command.into() }
});
try_from!(&protowire::NotifyVirtualChainChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifyVirtualChainChangedResponse>);

try_from!(item: &protowire::NotifySinkBlueScoreChangedRequestMessage, kaspa_rpc_core::NotifySinkBlueScoreChangedRequest, {
    Self { command: item.command.into() }
});
try_from!(&protowire::NotifySinkBlueScoreChangedResponseMessage, RpcResult<kaspa_rpc_core::NotifySinkBlueScoreChangedResponse>);

// ----------------------------------------------------------------------------
// Unit tests
// ----------------------------------------------------------------------------

// TODO: tests

#[cfg(test)]
mod palw_producer_facts_tests {
    use kaspa_rpc_core::{GetPalwProducerFactsRequest, GetPalwProducerFactsResponse, RpcResult};

    /// **Every field of the producer facts survives the grpc wire, both ways.**
    ///
    /// Twenty fields cross `from!`/`try_from!` by hand, and a dropped one is invisible: it
    /// arrives as a type-correct default. The facts are what a third-party miner builds an
    /// admissible attempt from — a silently-zeroed `pwu` or `class_target` is a miner that
    /// mines into a refusal it cannot diagnose. Distinct non-default values throughout, so a
    /// field copied from its neighbour fails too.
    #[test]
    fn every_producer_fact_survives_the_grpc_round_trip() {
        let request = GetPalwProducerFactsRequest {
            class_id: "aa".repeat(64),
            bond_transaction_id: "bb".repeat(64),
            bond_index: 7,
            with_bond: true,
        };
        let wire: crate::protowire::GetPalwProducerFactsRequestMessage = (&request).into();
        let back: GetPalwProducerFactsRequest = (&wire).try_into().unwrap();
        assert_eq!(back.class_id, request.class_id);
        assert_eq!(back.bond_transaction_id, request.bond_transaction_id);
        assert_eq!(back.bond_index, request.bond_index);
        assert_eq!(back.with_bond, request.with_bond);

        let response = GetPalwProducerFactsResponse {
            available: true,
            chain_point: "01".repeat(64),
            daa_score: 30_200_001,
            class_id: "02".repeat(64),
            artifact_root: "03".repeat(64),
            class_target: "340282366920938463463374607431768211455".to_string(),
            pwu: 15_800,
            is_base_class: true,
            min_trace_retention_daa: 2_400,
            epoch_index: 41,
            epoch_budget_blocks: 720,
            epoch_produced_blocks: 719,
            bond_known: true,
            bond_registered_pubkey: "04".repeat(32),
            bond_operator_id: "05".repeat(64),
            bond_collateral: 400_000,
            bond_reserved_exposure: "79000".to_string(),
            bond_exposure_ceiling: "200000".to_string(),
            bond_claim_exposure: "94800".to_string(),
            not_ready_reason: "the bond's exposure ceiling leaves no room for another claim".to_string(),
            // audit3 H3: the set a wallet must have before it selects inputs.
            locked_bond_outpoints: vec![format!("{}:0", "aa".repeat(64)), format!("{}:7", "bb".repeat(64))],
            panel_da_armed: true,
            // ADR-0118 Decision 3: the held-on-flat pair — the network flat, the class Merkle — so
            // a conversion that dropped the class bit, or read the network's into it, fails below.
            prompt_ids_merkle: false,
            class_prompt_ids_merkle: true,
            // ADR-0096 Decision 8: `true` so the round trip distinguishes carried from defaulted.
            fp_decode_constraint_armed: true,
            // ADR-0077 Decision 3: what a gateway reads before it commits.
            fp_certified: true,
            fp_quanta_per_canonical_job: 8,
            fp_max_quanta_per_receipt: 64,
            // ADR-0082 Decisions 10/11: which decode ruleset the chain plays. `true` here so the
            // round trip can distinguish "carried" from "defaulted" — the field's own default is
            // false, and a conversion that dropped it would pass against a false fixture.
            fp_decode_rules_armed: true,
            palw_retention_dir: "/var/lib/misaka/testnet-11/palw-retention".to_string(),
            // ADR-0152 P6: committed DIFFERENT from reserved, so a dropped field cannot pass.
            bond_committed: "123000".to_string(),
            bond_producer_floor_shortfall: 4_200,
            bond_accuser_exposure: "320".to_string(),
        };
        let wire: crate::protowire::GetPalwProducerFactsResponseMessage = RpcResult::Ok(&response).into();
        let back: GetPalwProducerFactsResponse = GetPalwProducerFactsResponse::try_from(&wire).unwrap();
        assert!(!back.prompt_ids_merkle && back.class_prompt_ids_merkle, "ADR-0118 D3: the class's form is its own field");
        // A node that predates the field answers proto3's absent `false` for the class; its every
        // class's form is the network's, and that is what is read.
        let older = crate::protowire::GetPalwProducerFactsResponseMessage {
            prompt_ids_merkle: true,
            class_prompt_ids_merkle: false,
            ..wire.clone()
        };
        assert!(GetPalwProducerFactsResponse::try_from(&older).unwrap().class_prompt_ids_merkle, "a Merkle genesis's classes");
        assert_eq!(
            (back.bond_committed.as_str(), back.bond_producer_floor_shortfall, back.bond_accuser_exposure.as_str()),
            ("123000", 4_200, "320"),
            "ADR-0152 P6: the one ledger, the floor and the accuser ledger survive the gRPC wire"
        );
        // A node that predates them sends none: its one ledger, no floor, no accuser ledger.
        let pre_p6 = crate::protowire::GetPalwProducerFactsResponseMessage {
            bond_committed: String::new(),
            bond_producer_floor_shortfall: 0,
            bond_accuser_exposure: String::new(),
            ..wire.clone()
        };
        let pre_p6 = GetPalwProducerFactsResponse::try_from(&pre_p6).unwrap();
        assert_eq!((pre_p6.bond_committed, pre_p6.bond_accuser_exposure), (response.bond_reserved_exposure.clone(), "0".to_string()));
        assert_eq!(back.available, response.available);
        assert_eq!(back.chain_point, response.chain_point);
        assert_eq!(back.daa_score, response.daa_score);
        assert_eq!(back.class_id, response.class_id);
        assert_eq!(back.artifact_root, response.artifact_root);
        assert_eq!(back.class_target, response.class_target, "a u128 target must survive as its full decimal string");
        assert_eq!(back.pwu, response.pwu);
        assert_eq!(back.is_base_class, response.is_base_class);
        assert_eq!(back.min_trace_retention_daa, response.min_trace_retention_daa);
        assert_eq!(back.epoch_index, response.epoch_index);
        assert_eq!(back.epoch_budget_blocks, response.epoch_budget_blocks);
        assert_eq!(back.epoch_produced_blocks, response.epoch_produced_blocks);
        assert_eq!(back.bond_known, response.bond_known);
        assert_eq!(back.bond_registered_pubkey, response.bond_registered_pubkey);
        assert_eq!(back.bond_operator_id, response.bond_operator_id);
        assert_eq!(back.bond_collateral, response.bond_collateral);
        assert_eq!(back.bond_reserved_exposure, response.bond_reserved_exposure);
        assert_eq!(back.bond_exposure_ceiling, response.bond_exposure_ceiling);
        assert_eq!(back.bond_claim_exposure, response.bond_claim_exposure);
        assert_eq!(back.not_ready_reason, response.not_ready_reason);
        assert_eq!(
            back.locked_bond_outpoints, response.locked_bond_outpoints,
            "the locked-collateral set must survive the wire — a wallet that loses it selects a bonded input"
        );
        assert!(back.fp_certified, "a gateway that loses fp_certified cannot say why its commitment is unsubmittable");
        assert_eq!(back.fp_quanta_per_canonical_job, response.fp_quanta_per_canonical_job);
        assert_eq!(back.fp_max_quanta_per_receipt, response.fp_max_quanta_per_receipt);
        assert!(
            back.fp_decode_rules_armed,
            "a builder that loses fp_decode_rules_armed builds jobs for the wrong decode ruleset — honest and unreproducible"
        );
        assert!(
            back.fp_decode_constraint_armed,
            "an entrance that loses fp_decode_constraint_armed serves a committed format nobody replays (ADR-0096)"
        );
        assert_eq!(
            back.palw_retention_dir, response.palw_retention_dir,
            "a submitter that loses the retention directory stages a claim's material where the node never looks (ADR-0084)"
        );
    }
}

#[cfg(test)]
mod palw_model_positions_tests {
    use kaspa_rpc_core::{GetPalwModelPositionsResponse, RpcPalwModelBenefitTier, RpcPalwModelPosition, RpcResult};

    /// **`getPalwModelPositions` v2 survives the grpc wire, both ways** (the 2026-09-23 Position
    /// route matrix, P-B2 — the review's missing test). The five membership fields cross
    /// `from!`/`try_from!` by hand, and a dropped one arrives as a type-correct default: no clock,
    /// tenure 0, no tier — a membership silently served on units alone. Distinct non-default values
    /// throughout, a second row whose `None`s must stay `None`, and the tip pinned by hash as well
    /// as by score.
    #[test]
    fn every_membership_field_survives_the_grpc_round_trip() {
        let tier = RpcPalwModelBenefitTier {
            min_units: 2,
            grants: 0b1000_0100,
            grant_names: vec!["PRIORITY_INFERENCE".to_string(), "SUPPORT".to_string()],
            lead_daa: 3,
            min_hold_daa: 50,
            note: "two jobs ahead".to_string(),
        };
        let response = GetPalwModelPositionsResponse {
            holder: "b0".repeat(64),
            positions: vec![
                RpcPalwModelPosition {
                    line_id: "c1".repeat(64),
                    units: 4_656,
                    holding_since_daa: Some(260),
                    tenure_daa: 60,
                    tier_index: Some(1),
                    tier: Some(tier.clone()),
                },
                RpcPalwModelPosition { line_id: "c2".repeat(64), units: 7, ..Default::default() },
            ],
            tip_daa: 320,
            tip_hash: "a7".repeat(64),
        };
        let wire: crate::protowire::GetPalwModelPositionsResponseMessage = RpcResult::Ok(&response).into();
        let back = GetPalwModelPositionsResponse::try_from(&wire).unwrap();
        assert_eq!(back.holder, response.holder);
        assert_eq!(back.positions, response.positions, "every row, the clock and the tier included, and the Nones kept");
        assert_eq!(back.positions[0].tier.as_ref(), Some(&tier));
        assert_eq!((back.positions[1].holding_since_daa, back.positions[1].tier_index), (None, None), "no clock stays no clock");
        assert_eq!(back.tip_daa, 320, "the height a challenge names");
        assert_eq!(back.tip_hash, response.tip_hash, "and the block it was read at");

        // An older node's message carries none of the new fields: read fail-closed.
        let older = crate::protowire::GetPalwModelPositionsResponseMessage {
            positions: vec![crate::protowire::RpcPalwModelPosition { line_id: "c1".repeat(64), units: 4_656, ..Default::default() }],
            tip_daa: 0,
            tip_hash: String::new(),
            ..wire
        };
        let old = GetPalwModelPositionsResponse::try_from(&older).unwrap();
        assert_eq!((old.positions[0].holding_since_daa, old.positions[0].tier_index), (None, None));
        assert!(old.positions[0].tier.is_none() && old.tip_hash.is_empty() && old.tip_daa == 0, "no tier, never tier 0");
    }
}

#[cfg(test)]
mod tests {
    use kaspa_rpc_core::{RpcError, RpcResult, SubmitBlockRejectReason, SubmitBlockReport, SubmitBlockResponse};

    use crate::protowire::{self, SubmitBlockResponseMessage, submit_block_response_message::RejectReason};

    #[test]
    fn test_submit_block_response() {
        struct Test {
            rpc_core: RpcResult<kaspa_rpc_core::SubmitBlockResponse>,
            protowire: protowire::SubmitBlockResponseMessage,
        }
        impl Test {
            fn new(
                rpc_core: RpcResult<kaspa_rpc_core::SubmitBlockResponse>,
                protowire: protowire::SubmitBlockResponseMessage,
            ) -> Self {
                Self { rpc_core, protowire }
            }
        }
        let tests = vec![
            Test::new(
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Success }),
                SubmitBlockResponseMessage { reject_reason: RejectReason::None as i32, error: None },
            ),
            Test::new(
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) }),
                SubmitBlockResponseMessage {
                    reject_reason: RejectReason::BlockInvalid as i32,
                    error: Some(protowire::RpcError {
                        message: RpcError::SubmitBlockError(SubmitBlockRejectReason::BlockInvalid).to_string(),
                    }),
                },
            ),
            Test::new(
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::IsInIBD) }),
                SubmitBlockResponseMessage {
                    reject_reason: RejectReason::IsInIbd as i32,
                    error: Some(protowire::RpcError {
                        message: RpcError::SubmitBlockError(SubmitBlockRejectReason::IsInIBD).to_string(),
                    }),
                },
            ),
            Test::new(
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::RouteIsFull) }),
                SubmitBlockResponseMessage {
                    reject_reason: RejectReason::None as i32, // This rpc core reject reason has no matching protowire variant
                    error: Some(protowire::RpcError {
                        message: RpcError::SubmitBlockError(SubmitBlockRejectReason::RouteIsFull).to_string(),
                    }),
                },
            ),
        ];

        for test in tests {
            let cnv_protowire: SubmitBlockResponseMessage = test.rpc_core.as_ref().map_err(|x| x.clone()).into();
            assert_eq!(cnv_protowire.reject_reason, test.protowire.reject_reason);
            assert_eq!(cnv_protowire.error.is_some(), test.protowire.error.is_some());
            assert_eq!(cnv_protowire.error, test.protowire.error);

            let cnv_rpc_core: RpcResult<SubmitBlockResponse> = (&test.protowire).try_into();
            assert_eq!(cnv_rpc_core.is_ok(), test.rpc_core.is_ok());
            match cnv_rpc_core {
                Ok(ref cnv_response) => {
                    let Ok(ref response) = test.rpc_core else { panic!() };
                    assert_eq!(cnv_response.report, response.report);
                }
                Err(ref cnv_err) => {
                    let Err(ref err) = test.rpc_core else { panic!() };
                    assert_eq!(cnv_err.to_string(), err.to_string());
                }
            }
        }
    }
}

/// **ADR-0078 Decision 5 on the gRPC wire.** The read exists so a stranger can check a derivation;
/// a field that does not survive the conversion is a check they cannot make.
#[cfg(test)]
mod palw_derived_artifacts_tests {
    use kaspa_rpc_core::{GetPalwDerivedArtifactsResponse, GetPalwFreePromptClaimResponse, RpcPalwDerivedArtifact, RpcResult};

    #[test]
    fn a_claims_derivations_survive_the_grpc_conversion() {
        let response = GetPalwDerivedArtifactsResponse {
            found: true,
            claim_id: "cc".repeat(64),
            output_root: "07".repeat(64),
            executor_pubkey: "ab".repeat(2592),
            executor_bond: format!("{}:3", "b0".repeat(32)),
            class_id: "c1".repeat(64),
            claim_phase: "voided".to_string(),
            claim_void_reason: "court_fraud".to_string(),
            claim_accepted_block: "bb".repeat(32),
            claim_accepted_daa: 91_300,
            artifacts: vec![
                RpcPalwDerivedArtifact {
                    transformer_id: "7a".repeat(64),
                    derived_id: "d1".repeat(64),
                    grammar_id: "6a".repeat(64),
                    kind: 6,
                    kind_name: "music".to_string(),
                    dsl_hash: "d5".repeat(64),
                    artifact_hash: "a7".repeat(64),
                    artifact_bytes: 4_096,
                    accepted_daa: 91_337,
                },
                RpcPalwDerivedArtifact {
                    transformer_id: "7b".repeat(64),
                    derived_id: "d2".repeat(64),
                    grammar_id: "6b".repeat(64),
                    kind: 1,
                    kind_name: "scene".to_string(),
                    dsl_hash: "d6".repeat(64),
                    artifact_hash: "a8".repeat(64),
                    artifact_bytes: 1_048_576,
                    accepted_daa: 91_338,
                },
            ],
        };
        let wire: crate::protowire::GetPalwDerivedArtifactsResponseMessage = RpcResult::Ok(&response).into();
        let back: GetPalwDerivedArtifactsResponse = GetPalwDerivedArtifactsResponse::try_from(&wire).unwrap();
        assert!(back.found);
        assert_eq!(back.output_root, response.output_root, "ADR-0078 X6 recomputes against exactly this");
        assert_eq!(back.executor_pubkey, response.executor_pubkey, "the executor's name on the provenance");
        assert_eq!(back.executor_bond, response.executor_bond);
        assert_eq!(back.claim_phase, "voided");
        assert_eq!(back.claim_void_reason, "court_fraud", "Decision 4: a voided claim's derivation says so when read");
        assert_eq!(back.artifacts.len(), 2, "a bounded table, not a page");
        for (b, r) in back.artifacts.iter().zip(response.artifacts.iter()) {
            assert_eq!(b.transformer_id, r.transformer_id, "the key's half of the row must not be lost");
            assert_eq!(b.derived_id, r.derived_id);
            assert_eq!(b.grammar_id, r.grammar_id);
            assert_eq!((b.kind, &b.kind_name), (r.kind, &r.kind_name));
            assert_eq!(b.dsl_hash, r.dsl_hash);
            assert_eq!(b.artifact_hash, r.artifact_hash);
            assert_eq!(b.artifact_bytes, r.artifact_bytes);
            assert_eq!(b.accepted_daa, r.accepted_daa);
        }
    }

    #[test]
    fn the_claim_facts_survive_the_grpc_conversion() {
        let response = GetPalwFreePromptClaimResponse {
            found: true,
            claim_id: "cc".repeat(64),
            is_free_prompt: true,
            class_id: "c1".repeat(64),
            executor_pubkey: "ab".repeat(2592),
            executor_bond: format!("{}:3", "b0".repeat(32)),
            output_root: "07".repeat(64),
            trace_root: "77".repeat(64),
            execution_root: "e7".repeat(64),
            work_leaves: 4_194_304,
            work_id: "17".repeat(64),
            quanta: 8,
            quanta_spent: 3,
            phase: "final".to_string(),
            void_reason: String::new(),
            phase_daa: 91_500,
            accepted_block: "bb".repeat(32),
            accepted_daa: 91_300,
            trace_retention_daa: 100_000,
            derived_count: 2,
        };
        let wire: crate::protowire::GetPalwFreePromptClaimResponseMessage = RpcResult::Ok(&response).into();
        let back: GetPalwFreePromptClaimResponse = GetPalwFreePromptClaimResponse::try_from(&wire).unwrap();
        assert_eq!(back.output_root, response.output_root);
        assert_eq!(back.trace_root, response.trace_root);
        assert_eq!(back.execution_root, response.execution_root);
        assert_eq!(back.work_id, response.work_id);
        assert_eq!((back.quanta, back.quanta_spent), (8, 3));
        assert_eq!(back.derived_count, 2);
        assert!(back.is_free_prompt);
        assert_eq!(back.phase, "final");
    }

    /// **ADR-0080 design A: the group's own bitmap survives the wire.**
    ///
    /// `present` is the field a resume is decided on, and it is a bitmap because chunks arrive in
    /// any order — so a conversion that lost a bit, or that carried a COUNT of parts instead, would
    /// send a mover to re-pay for a carrier the chain already holds and leave a hole it does not.
    /// A sparse pattern with a gap in the middle is the one that catches that; a full or empty
    /// bitmap would round-trip through either shape.
    #[test]
    fn a_declared_closes_arrival_bitmap_survives_the_grpc_conversion() {
        use kaspa_rpc_core::GetPalwPendingChunkGroupResponse;
        let response = GetPalwPendingChunkGroupResponse {
            found: true,
            session_id: "5e".repeat(64),
            side: "executor".to_string(),
            count: 27,
            // 0, 1, 3 and 26 have landed: a gap at 2 and the top index set.
            present: 0b1011 | (1u64 << 26),
            parts_present: 4,
            complete: false,
            declared_daa: 91_300,
            assembly_deadline_daa: 91_408,
            close_digest: "c1".repeat(64),
            verdict: "executor_guilty".to_string(),
            declarer_bond: format!("{}:1", "b0".repeat(32)),
            deposit: 33_750_000,
        };
        let wire: crate::protowire::GetPalwPendingChunkGroupResponseMessage = RpcResult::Ok(&response).into();
        let back: GetPalwPendingChunkGroupResponse = GetPalwPendingChunkGroupResponse::try_from(&wire).unwrap();
        assert_eq!(back.present, response.present, "the arrival bitmap did not survive the wire");
        assert_eq!(back.count, 27);
        assert_eq!(back.parts_present, 4);
        assert!(!back.complete);
        assert_eq!(back.side, "executor");
        assert_eq!(back.session_id, response.session_id);
        assert_eq!(back.close_digest, response.close_digest);
        assert_eq!(back.assembly_deadline_daa, 91_408);
        assert_eq!(back.declarer_bond, response.declarer_bond);
        assert_eq!(back.deposit, 33_750_000);
        assert_eq!(back.verdict, "executor_guilty");
        // The wRPC half of the same wire is `the_declared_close_survives_the_wrpc_round_trip`, in
        // `rpc/core`, where the `Serializer`/`Deserializer` traits live.
    }
}
