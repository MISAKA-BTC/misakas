use crate::v7::{
    address::{ReceiveAddressesFlow, SendAddressesFlow},
    blockrelay::{flow::HandleRelayInvsFlow, handle_requests::HandleRelayBlockRequests},
    ping::{ReceivePingsFlow, SendPingsFlow},
    request_antipast::HandleAntipastRequests,
    request_block_locator::RequestBlockLocatorFlow,
    request_headers::RequestHeadersFlow,
    request_ibd_blocks::HandleIbdBlockRequests,
    request_ibd_chain_block_locator::RequestIbdChainBlockLocatorFlow,
    request_pp_proof::RequestPruningPointProofFlow,
    request_pruning_point_and_anticone::PruningPointAndItsAnticoneRequestsFlow,
    request_pruning_point_utxo_set::RequestPruningPointUtxoSetFlow,
    txrelay::flow::{RelayTransactionsFlow, RequestTransactionsFlow},
};
pub(crate) mod claimrelay_evm;
pub(crate) mod request_block_bodies;
pub(crate) mod request_pruning_point_snapshots;
pub(crate) mod txrelay_evm;
use crate::{
    flow_context::{FlowContext, PROTOCOL_VERSION_CLAIM_RELAY, PROTOCOL_VERSION_EVM_RELAY},
    flow_trait::Flow,
};
use claimrelay_evm::{RelayEvmDepositClaimsFlow, RequestedEvmDepositClaimsFlow};
use txrelay_evm::{RelayEvmTransactionsFlow, RequestedEvmTransactionsFlow};

use crate::ibd::IbdFlow;
use kaspa_p2p_lib::{KaspadMessagePayloadType, Router, SharedIncomingRoute, convert::header::HeaderFormat};
use kaspa_utils::channel;
use request_block_bodies::HandleBlockBodyRequests;
use request_pruning_point_snapshots::{RequestPruningPointEvmStateFlow, RequestPruningPointOverlaySnapshotFlow};
use std::sync::Arc;

pub fn register(ctx: FlowContext, router: Arc<Router>, protocol_version: u32) -> Vec<Box<dyn Flow>> {
    // IBD flow <-> invs flow communication uses a job channel in order to always
    // maintain at most a single pending job which can be updated
    let (ibd_sender, relay_receiver) = channel::job();
    let body_only_ibd_permitted = true;
    let header_format = HeaderFormat::from(protocol_version);
    let mut flows: Vec<Box<dyn Flow>> = vec![
        Box::new(IbdFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
                KaspadMessagePayloadType::BlockHeaders,
                KaspadMessagePayloadType::DoneHeaders,
                KaspadMessagePayloadType::IbdBlockLocatorHighestHash,
                KaspadMessagePayloadType::IbdBlockLocatorHighestHashNotFound,
                KaspadMessagePayloadType::BlockWithTrustedDataV4,
                KaspadMessagePayloadType::DoneBlocksWithTrustedData,
                KaspadMessagePayloadType::IbdChainBlockLocator,
                KaspadMessagePayloadType::IbdBlock,
                KaspadMessagePayloadType::BlockBody,
                KaspadMessagePayloadType::TrustedData,
                KaspadMessagePayloadType::PruningPoints,
                KaspadMessagePayloadType::PruningPointProof,
                KaspadMessagePayloadType::UnexpectedPruningPoint,
                KaspadMessagePayloadType::PruningPointUtxoSetChunk,
                KaspadMessagePayloadType::DonePruningPointUtxoSetChunks,
                // kaspa-pq ADR-0022: pruned-IBD EVM + overlay snapshot responses.
                KaspadMessagePayloadType::PruningPointEvmState,
                KaspadMessagePayloadType::PruningPointOverlaySnapshot,
            ]),
            relay_receiver,
            body_only_ibd_permitted,
            header_format,
        )),
        Box::new(HandleRelayBlockRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestRelayBlocks]),
            header_format,
        )),
        Box::new(ReceivePingsFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![KaspadMessagePayloadType::Ping]))),
        Box::new(SendPingsFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![KaspadMessagePayloadType::Pong]))),
        Box::new(RequestHeadersFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestHeaders, KaspadMessagePayloadType::RequestNextHeaders]),
            header_format,
        )),
        Box::new(RequestPruningPointProofFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestPruningPointProof]),
            header_format,
        )),
        Box::new(RequestIbdChainBlockLocatorFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestIbdChainBlockLocator]),
        )),
        Box::new(PruningPointAndItsAnticoneRequestsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
                KaspadMessagePayloadType::RequestPruningPointAndItsAnticone,
                KaspadMessagePayloadType::RequestNextPruningPointAndItsAnticoneBlocks,
            ]),
            header_format,
        )),
        Box::new(RequestPruningPointUtxoSetFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![
                KaspadMessagePayloadType::RequestPruningPointUtxoSet,
                KaspadMessagePayloadType::RequestNextPruningPointUtxoSetChunk,
            ]),
        )),
        // kaspa-pq ADR-0022: serve the pruning point's EVM + overlay snapshots for pruned-IBD.
        Box::new(RequestPruningPointEvmStateFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestPruningPointEvmState]),
        )),
        Box::new(RequestPruningPointOverlaySnapshotFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestPruningPointOverlaySnapshot]),
        )),
        Box::new(HandleIbdBlockRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestIbdBlocks]),
            header_format,
        )),
        Box::new(HandleBlockBodyRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestBlockBodies]),
        )),
        Box::new(HandleAntipastRequests::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestAntipast]),
            header_format,
        )),
        Box::new(RelayTransactionsFlow::new(
            ctx.clone(),
            router.clone(),
            router
                .subscribe_with_capacity(vec![KaspadMessagePayloadType::InvTransactions], RelayTransactionsFlow::invs_channel_size()),
            router.subscribe_with_capacity(
                vec![KaspadMessagePayloadType::Transaction, KaspadMessagePayloadType::TransactionNotFound],
                RelayTransactionsFlow::txs_channel_size(),
            ),
        )),
        Box::new(RequestTransactionsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestTransactions]),
        )),
        Box::new(ReceiveAddressesFlow::new(ctx.clone(), router.clone(), router.subscribe(vec![KaspadMessagePayloadType::Addresses]))),
        Box::new(SendAddressesFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestAddresses]),
        )),
        Box::new(RequestBlockLocatorFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestBlockLocator]),
        )),
    ];

    // kaspa-pq EVM Lane §14.2: pending-EVM-tx relay — only for peers whose
    // negotiated protocol knows the EVM message types (an unroutable payload
    // type disconnects the peer, so older peers must not get these routes
    // registered either: they can never legally send them).
    if protocol_version >= PROTOCOL_VERSION_EVM_RELAY {
        flows.push(Box::new(RelayEvmTransactionsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe_with_capacity(
                vec![KaspadMessagePayloadType::InvEvmTransactions],
                RelayEvmTransactionsFlow::invs_channel_size(),
            ),
            router.subscribe_with_capacity(
                vec![KaspadMessagePayloadType::EvmTransaction, KaspadMessagePayloadType::EvmTransactionNotFound],
                RelayEvmTransactionsFlow::txs_channel_size(),
            ),
        )));
        flows.push(Box::new(RequestedEvmTransactionsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestEvmTransactions]),
        )));
    }

    // kaspa-pq EVM Lane §14.2 / §9.2: deposit-claim relay (oneof 67-70) is a
    // SEPARATE, HIGHER protocol gate (≥102) than the EVM-tx relay (≥101). A 101
    // peer (EVM-tx relay only) has no route for the claim message types, so we
    // must NOT register claim routes for it nor send it a claim inv (the spread
    // also filters claim gossip to ≥102) — otherwise an unroutable payload type
    // would disconnect it.
    if protocol_version >= PROTOCOL_VERSION_CLAIM_RELAY {
        flows.push(Box::new(RelayEvmDepositClaimsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe_with_capacity(
                vec![KaspadMessagePayloadType::InvEvmDepositClaims],
                RelayEvmDepositClaimsFlow::invs_channel_size(),
            ),
            router.subscribe_with_capacity(
                vec![KaspadMessagePayloadType::EvmDepositClaim, KaspadMessagePayloadType::EvmDepositClaimNotFound],
                RelayEvmDepositClaimsFlow::claims_channel_size(),
            ),
        )));
        flows.push(Box::new(RequestedEvmDepositClaimsFlow::new(
            ctx.clone(),
            router.clone(),
            router.subscribe(vec![KaspadMessagePayloadType::RequestEvmDepositClaims]),
        )));
    }

    let invs_route = router.subscribe_with_capacity(vec![KaspadMessagePayloadType::InvRelayBlock], ctx.block_invs_channel_size());
    let shared_invs_route = SharedIncomingRoute::new(invs_route);

    let num_relay_flows = (ctx.config.bps() as usize / 2).max(1);
    flows.extend((0..num_relay_flows).map(|_| {
        Box::new(HandleRelayInvsFlow::new(
            ctx.clone(),
            router.clone(),
            shared_invs_route.clone(),
            router.subscribe(vec![]),
            ibd_sender.clone(),
            header_format,
        )) as Box<dyn Flow>
    }));

    // The reject message is handled as a special case by the router
    // KaspadMessagePayloadType::Reject,

    // We do not register the below two messages since they are deprecated also in go-kaspa
    // KaspadMessagePayloadType::BlockWithTrustedData,
    // KaspadMessagePayloadType::IbdBlockLocator,

    flows
}
