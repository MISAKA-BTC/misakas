//!
//! The main [`RpcApi`] trait that defines all RPC methods available in the Rusty Kaspa p2p node.
//!
//! Rpc = External RPC Service
//! All data provided by the RPC server can be trusted by the client
//! No data submitted by the client to the node can be trusted
//!

use crate::api::connection::DynRpcConnection;
use crate::{RpcError, RpcResult, model::*, notify::connection::ChannelConnection};
use async_trait::async_trait;
use downcast::{AnySync, downcast_sync};
use kaspa_notify::{listener::ListenerId, scope::Scope, subscription::Command};
use std::sync::Arc;

pub const MAX_SAFE_WINDOW_SIZE: u32 = 10_000;

/// Client RPC Api
///
/// The [`RpcApi`] trait defines RPC calls taking a request message as unique parameter.
///
/// For each RPC call a matching readily implemented function taking detailed parameters is also provided.
#[async_trait]
pub trait RpcApi: Sync + Send + AnySync {
    ///
    async fn ping(&self) -> RpcResult<()> {
        self.ping_call(None, PingRequest {}).await?;
        Ok(())
    }
    async fn ping_call(&self, connection: Option<&DynRpcConnection>, request: PingRequest) -> RpcResult<PingResponse>;

    // ---

    async fn get_system_info(&self) -> RpcResult<GetSystemInfoResponse> {
        Ok(self.get_system_info_call(None, GetSystemInfoRequest {}).await?)
    }
    async fn get_system_info_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetSystemInfoRequest,
    ) -> RpcResult<GetSystemInfoResponse>;

    // ---

    async fn get_connections(&self, include_profile_data: bool) -> RpcResult<GetConnectionsResponse> {
        self.get_connections_call(None, GetConnectionsRequest { include_profile_data }).await
    }
    async fn get_connections_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetConnectionsRequest,
    ) -> RpcResult<GetConnectionsResponse>;

    // ---

    async fn get_metrics(
        &self,
        process_metrics: bool,
        connection_metrics: bool,
        bandwidth_metrics: bool,
        consensus_metrics: bool,
        storage_metrics: bool,
        custom_metrics: bool,
    ) -> RpcResult<GetMetricsResponse> {
        self.get_metrics_call(
            None,
            GetMetricsRequest {
                process_metrics,
                connection_metrics,
                bandwidth_metrics,
                consensus_metrics,
                storage_metrics,
                custom_metrics,
            },
        )
        .await
    }
    async fn get_metrics_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetMetricsRequest,
    ) -> RpcResult<GetMetricsResponse>;

    // get_info alternative that carries only version, network_id (full), is_synced, virtual_daa_score
    // these are the only variables needed to negotiate a wRPC connection (besides the wRPC handshake)
    async fn get_server_info(&self) -> RpcResult<GetServerInfoResponse> {
        self.get_server_info_call(None, GetServerInfoRequest {}).await
    }
    async fn get_server_info_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetServerInfoRequest,
    ) -> RpcResult<GetServerInfoResponse>;

    // Get current sync status of the node (should be converted to a notification subscription)
    async fn get_sync_status(&self) -> RpcResult<bool> {
        Ok(self.get_sync_status_call(None, GetSyncStatusRequest {}).await?.is_synced)
    }
    async fn get_sync_status_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetSyncStatusRequest,
    ) -> RpcResult<GetSyncStatusResponse>;

    // ---

    /// Requests the network the node is currently running against.
    async fn get_current_network(&self) -> RpcResult<RpcNetworkType> {
        Ok(self.get_current_network_call(None, GetCurrentNetworkRequest {}).await?.network)
    }
    async fn get_current_network_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetCurrentNetworkRequest,
    ) -> RpcResult<GetCurrentNetworkResponse>;

    /// Submit a block into the DAG.
    ///
    /// Blocks are generally expected to have been generated using the get_block_template call.
    async fn submit_block(&self, block: RpcRawBlock, allow_non_daa_blocks: bool) -> RpcResult<SubmitBlockResponse> {
        self.submit_block_call(None, SubmitBlockRequest::new(block, allow_non_daa_blocks)).await
    }
    async fn submit_block_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: SubmitBlockRequest,
    ) -> RpcResult<SubmitBlockResponse>;

    /// Request a current block template.
    ///
    /// Callers are expected to solve the block template and submit it using the submit_block call.
    async fn get_block_template(&self, pay_address: RpcAddress, extra_data: RpcExtraData) -> RpcResult<GetBlockTemplateResponse> {
        self.get_block_template_call(None, GetBlockTemplateRequest::new(pay_address, extra_data)).await
    }
    async fn get_block_template_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetBlockTemplateRequest,
    ) -> RpcResult<GetBlockTemplateResponse>;

    /// Requests the list of known kaspad addresses in the current network (mainnet, testnet, etc.)
    async fn get_peer_addresses(&self) -> RpcResult<GetPeerAddressesResponse> {
        self.get_peer_addresses_call(None, GetPeerAddressesRequest {}).await
    }
    async fn get_peer_addresses_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetPeerAddressesRequest,
    ) -> RpcResult<GetPeerAddressesResponse>;

    /// requests the hash of the current virtual's selected parent.
    async fn get_sink(&self) -> RpcResult<GetSinkResponse> {
        self.get_sink_call(None, GetSinkRequest {}).await
    }
    async fn get_sink_call(&self, connection: Option<&DynRpcConnection>, request: GetSinkRequest) -> RpcResult<GetSinkResponse>;

    /// Requests information about a specific transaction in the mempool.
    async fn get_mempool_entry(
        &self,
        transaction_id: RpcTransactionId,
        include_orphan_pool: bool,
        filter_transaction_pool: bool,
    ) -> RpcResult<RpcMempoolEntry> {
        Ok(self
            .get_mempool_entry_call(None, GetMempoolEntryRequest::new(transaction_id, include_orphan_pool, filter_transaction_pool))
            .await?
            .mempool_entry)
    }
    async fn get_mempool_entry_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetMempoolEntryRequest,
    ) -> RpcResult<GetMempoolEntryResponse>;

    /// Requests information about all the transactions currently in the mempool.
    async fn get_mempool_entries(&self, include_orphan_pool: bool, filter_transaction_pool: bool) -> RpcResult<Vec<RpcMempoolEntry>> {
        Ok(self
            .get_mempool_entries_call(None, GetMempoolEntriesRequest::new(include_orphan_pool, filter_transaction_pool))
            .await?
            .mempool_entries)
    }
    async fn get_mempool_entries_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetMempoolEntriesRequest,
    ) -> RpcResult<GetMempoolEntriesResponse>;

    /// requests information about all the p2p peers currently connected to this node.
    async fn get_connected_peer_info(&self) -> RpcResult<GetConnectedPeerInfoResponse> {
        self.get_connected_peer_info_call(None, GetConnectedPeerInfoRequest {}).await
    }
    async fn get_connected_peer_info_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetConnectedPeerInfoRequest,
    ) -> RpcResult<GetConnectedPeerInfoResponse>;

    /// Adds a peer to the node's outgoing connection list.
    ///
    /// This will, in most cases, result in the node connecting to said peer.
    async fn add_peer(&self, peer_address: RpcContextualPeerAddress, is_permanent: bool) -> RpcResult<()> {
        self.add_peer_call(None, AddPeerRequest::new(peer_address, is_permanent)).await?;
        Ok(())
    }
    async fn add_peer_call(&self, connection: Option<&DynRpcConnection>, request: AddPeerRequest) -> RpcResult<AddPeerResponse>;

    /// Submits a transaction to the mempool.
    async fn submit_transaction(&self, transaction: RpcTransaction, allow_orphan: bool) -> RpcResult<RpcTransactionId> {
        Ok(self.submit_transaction_call(None, SubmitTransactionRequest { transaction, allow_orphan }).await?.transaction_id)
    }
    async fn submit_transaction_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: SubmitTransactionRequest,
    ) -> RpcResult<SubmitTransactionResponse>;

    /// Submits a transaction replacement to the mempool, applying a mandatory Replace by Fee policy.
    ///
    /// Returns the ID of the inserted transaction and the transaction the submission replaced in the mempool.
    async fn submit_transaction_replacement(&self, transaction: RpcTransaction) -> RpcResult<SubmitTransactionReplacementResponse> {
        self.submit_transaction_replacement_call(None, SubmitTransactionReplacementRequest { transaction }).await
    }
    async fn submit_transaction_replacement_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: SubmitTransactionReplacementRequest,
    ) -> RpcResult<SubmitTransactionReplacementResponse>;

    /// Requests information about a specific block.
    async fn get_block(&self, hash: RpcHash, include_transactions: bool) -> RpcResult<RpcBlock> {
        Ok(self.get_block_call(None, GetBlockRequest::new(hash, include_transactions)).await?.block)
    }
    async fn get_block_call(&self, connection: Option<&DynRpcConnection>, request: GetBlockRequest) -> RpcResult<GetBlockResponse>;

    /// Requests information about a specific subnetwork.
    async fn get_subnetwork(&self, subnetwork_id: RpcSubnetworkId) -> RpcResult<GetSubnetworkResponse> {
        self.get_subnetwork_call(None, GetSubnetworkRequest::new(subnetwork_id)).await
    }
    async fn get_subnetwork_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetSubnetworkRequest,
    ) -> RpcResult<GetSubnetworkResponse>;

    /// Requests the virtual selected parent chain from some `start_hash` to this node's current virtual.
    async fn get_virtual_chain_from_block(
        &self,
        start_hash: RpcHash,
        include_accepted_transaction_ids: bool,
        min_confirmation_count: Option<u64>,
    ) -> RpcResult<GetVirtualChainFromBlockResponse> {
        self.get_virtual_chain_from_block_call(
            None,
            GetVirtualChainFromBlockRequest::new(start_hash, include_accepted_transaction_ids, min_confirmation_count),
        )
        .await
    }
    async fn get_virtual_chain_from_block_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetVirtualChainFromBlockRequest,
    ) -> RpcResult<GetVirtualChainFromBlockResponse>;

    /// Requests blocks between a certain block `low_hash` up to this node's current virtual.
    async fn get_blocks(
        &self,
        low_hash: Option<RpcHash>,
        include_blocks: bool,
        include_transactions: bool,
    ) -> RpcResult<GetBlocksResponse> {
        self.get_blocks_call(None, GetBlocksRequest::new(low_hash, include_blocks, include_transactions)).await
    }
    async fn get_blocks_call(&self, connection: Option<&DynRpcConnection>, request: GetBlocksRequest) -> RpcResult<GetBlocksResponse>;

    /// Requests the current number of blocks in this node.
    ///
    /// Note that this number may decrease as pruning occurs.
    async fn get_block_count(&self) -> RpcResult<GetBlockCountResponse> {
        self.get_block_count_call(None, GetBlockCountRequest {}).await
    }
    async fn get_block_count_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetBlockCountRequest,
    ) -> RpcResult<GetBlockCountResponse>;

    /// Requests general information about the current state of this node's DAG.
    async fn get_block_dag_info(&self) -> RpcResult<GetBlockDagInfoResponse> {
        self.get_block_dag_info_call(None, GetBlockDagInfoRequest {}).await
    }
    async fn get_block_dag_info_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetBlockDagInfoRequest,
    ) -> RpcResult<GetBlockDagInfoResponse>;

    ///
    async fn resolve_finality_conflict(&self, finality_block_hash: RpcHash) -> RpcResult<()> {
        self.resolve_finality_conflict_call(None, ResolveFinalityConflictRequest::new(finality_block_hash)).await?;
        Ok(())
    }
    async fn resolve_finality_conflict_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: ResolveFinalityConflictRequest,
    ) -> RpcResult<ResolveFinalityConflictResponse>;

    /// Shuts down this node.
    async fn shutdown(&self) -> RpcResult<()> {
        self.shutdown_call(None, ShutdownRequest {}).await?;
        Ok(())
    }
    async fn shutdown_call(&self, connection: Option<&DynRpcConnection>, request: ShutdownRequest) -> RpcResult<ShutdownResponse>;

    /// Requests headers between the given `start_hash` and the current virtual, up to the given limit.
    async fn get_headers(&self, start_hash: RpcHash, limit: u64, is_ascending: bool) -> RpcResult<Vec<RpcHeader>> {
        Ok(self.get_headers_call(None, GetHeadersRequest::new(start_hash, limit, is_ascending)).await?.headers)
    }
    async fn get_headers_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetHeadersRequest,
    ) -> RpcResult<GetHeadersResponse>;

    /// Returns the total balance in unspent transactions towards a given address.
    ///
    /// This call is only available when this node was started with `--utxoindex`.
    async fn get_balance_by_address(&self, address: RpcAddress) -> RpcResult<u64> {
        Ok(self.get_balance_by_address_call(None, GetBalanceByAddressRequest::new(address)).await?.balance)
    }
    async fn get_balance_by_address_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetBalanceByAddressRequest,
    ) -> RpcResult<GetBalanceByAddressResponse>;

    ///
    async fn get_balances_by_addresses(&self, addresses: Vec<RpcAddress>) -> RpcResult<Vec<RpcBalancesByAddressesEntry>> {
        Ok(self.get_balances_by_addresses_call(None, GetBalancesByAddressesRequest::new(addresses)).await?.entries)
    }
    async fn get_balances_by_addresses_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetBalancesByAddressesRequest,
    ) -> RpcResult<GetBalancesByAddressesResponse>;

    /// Requests all current UTXOs for the given node addresses.
    ///
    /// This call is only available when this node was started with `--utxoindex`.
    async fn get_utxos_by_addresses(&self, addresses: Vec<RpcAddress>) -> RpcResult<Vec<RpcUtxosByAddressesEntry>> {
        Ok(self.get_utxos_by_addresses_call(None, GetUtxosByAddressesRequest::new(addresses)).await?.entries)
    }
    async fn get_utxos_by_addresses_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetUtxosByAddressesRequest,
    ) -> RpcResult<GetUtxosByAddressesResponse>;

    /// Requests a bounded, cursor-paginated page of UTXOs for a single address.
    ///
    /// This call is only available when this node was started with `--utxoindex`. Prefer it over
    /// [Self::get_utxos_by_addresses] for addresses with very large UTXO sets, whose full response
    /// can exceed client message-size / timeout limits.
    async fn get_utxos_by_address_page(
        &self,
        address: RpcAddress,
        cursor: String,
        limit: u64,
    ) -> RpcResult<GetUtxosByAddressPageResponse> {
        self.get_utxos_by_address_page_call(None, GetUtxosByAddressPageRequest::new(address, cursor, limit)).await
    }
    async fn get_utxos_by_address_page_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetUtxosByAddressPageRequest,
    ) -> RpcResult<GetUtxosByAddressPageResponse>;

    /// Requests the blue score of the current selected parent of the virtual block.
    async fn get_sink_blue_score(&self) -> RpcResult<u64> {
        Ok(self.get_sink_blue_score_call(None, GetSinkBlueScoreRequest {}).await?.blue_score)
    }
    async fn get_sink_blue_score_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetSinkBlueScoreRequest,
    ) -> RpcResult<GetSinkBlueScoreResponse>;

    /// kaspa-pq Phase 10 (ADR-0009): the current DNS finality confirmation view.
    async fn get_dns_confirmation(&self) -> RpcResult<GetDnsConfirmationResponse> {
        self.get_dns_confirmation_call(None, GetDnsConfirmationRequest { block_hash: String::new() }).await
    }
    /// kaspa-pq: DNS finality scoped to a single block — populates the response's `block_*` fields
    /// (`block_found`, `block_is_dns_final`, `block_is_confirmed_anchor`, `block_daa_score`).
    /// `block_is_dns_final` means the block is a selected-chain ancestor of (or equal to) the
    /// stake-confirmed anchor, i.e. irreversible under DNS finality.
    async fn get_block_dns_score(&self, block_hash: String) -> RpcResult<GetDnsConfirmationResponse> {
        self.get_dns_confirmation_call(None, GetDnsConfirmationRequest { block_hash }).await
    }
    /// Default returns `available: false`, so non-server `RpcApi` impls (gRPC /
    /// wRPC clients, mocks) inherit a no-op; the node's core service overrides it.
    async fn get_dns_confirmation_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetDnsConfirmationRequest,
    ) -> RpcResult<GetDnsConfirmationResponse> {
        let _ = (connection, request);
        Ok(GetDnsConfirmationResponse::default())
    }

    /// kaspa-pq DNS v3: ready epochs below the StakeScore attestation quality floor.
    /// This remains populated on liveness-first presets where mandatory attestation inclusion is
    /// intentionally inert, so operators can monitor validator-layer degradation without making it
    /// a base-ledger validity rule.
    async fn get_attestation_quality_deficits(&self) -> RpcResult<GetAttestationQualityDeficitsResponse> {
        self.get_attestation_quality_deficits_call(None, GetAttestationQualityDeficitsRequest {}).await
    }
    /// Default returns an empty list, so non-server `RpcApi` impls inherit a no-op;
    /// the node's core service overrides it.
    async fn get_attestation_quality_deficits_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetAttestationQualityDeficitsRequest,
    ) -> RpcResult<GetAttestationQualityDeficitsResponse> {
        let _ = (connection, request);
        Ok(GetAttestationQualityDeficitsResponse::default())
    }

    /// kaspa-pq EVM Lane v0.4 (§16): submit a raw EIP-2718 EVM transaction
    /// (hex) to the EVM mempool. Returns the Ethereum tx hash.
    async fn submit_evm_transaction(&self, transaction_hex: String) -> RpcResult<SubmitEvmTransactionResponse> {
        self.submit_evm_transaction_call(None, SubmitEvmTransactionRequest { transaction: transaction_hex }).await
    }
    /// Default rejects, so non-server `RpcApi` impls inherit a refusal; the
    /// node's core service overrides it.
    async fn submit_evm_transaction_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: SubmitEvmTransactionRequest,
    ) -> RpcResult<SubmitEvmTransactionResponse> {
        let _ = (connection, request);
        Err(RpcError::NotImplemented)
    }

    /// kaspa-pq EVM Lane v0.4 (§16): canonical-resolved EVM receipt.
    async fn get_evm_transaction_receipt(&self, transaction_hash: String) -> RpcResult<GetEvmTransactionReceiptResponse> {
        self.get_evm_transaction_receipt_call(None, GetEvmTransactionReceiptRequest { transaction_hash }).await
    }
    async fn get_evm_transaction_receipt_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetEvmTransactionReceiptRequest,
    ) -> RpcResult<GetEvmTransactionReceiptResponse> {
        let _ = (connection, request);
        Err(RpcError::NotImplemented)
    }

    /// kaspa-pq EVM Lane v0.4 (§16): DA-inclusion / acceptance / skip status.
    async fn get_evm_tx_inclusion_status(&self, transaction_hash: String) -> RpcResult<GetEvmTxInclusionStatusResponse> {
        self.get_evm_tx_inclusion_status_call(None, GetEvmTxInclusionStatusRequest { transaction_hash }).await
    }
    async fn get_evm_tx_inclusion_status_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetEvmTxInclusionStatusRequest,
    ) -> RpcResult<GetEvmTxInclusionStatusResponse> {
        let _ = (connection, request);
        Err(RpcError::NotImplemented)
    }

    /// kaspa-pq EVM Lane v0.4 (§9.2): submit an EVM_DEPOSIT_LOCK outpoint to be
    /// claimed (bridge deposit). The node resolves + validates it and queues a
    /// `DepositClaim` for its own template.
    async fn submit_evm_deposit_claim(&self, transaction_id: String, index: u32) -> RpcResult<SubmitEvmDepositClaimResponse> {
        self.submit_evm_deposit_claim_call(None, SubmitEvmDepositClaimRequest { transaction_id, index }).await
    }
    async fn submit_evm_deposit_claim_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: SubmitEvmDepositClaimRequest,
    ) -> RpcResult<SubmitEvmDepositClaimResponse> {
        let _ = (connection, request);
        Err(RpcError::NotImplemented)
    }

    /// kaspa-pq Phase 11 (ADR-0010): the in-process validator service's status.
    async fn get_validator_status(&self) -> RpcResult<GetValidatorStatusResponse> {
        self.get_validator_status_call(None, GetValidatorStatusRequest {}).await
    }
    /// Default returns `enabled: false`, so non-server `RpcApi` impls (gRPC / wRPC
    /// clients, mocks) inherit a no-op; the node's core service overrides it.
    async fn get_validator_status_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetValidatorStatusRequest,
    ) -> RpcResult<GetValidatorStatusResponse> {
        let _ = (connection, request);
        Ok(GetValidatorStatusResponse::default())
    }

    /// kaspa-pq Phase 12 (ADR-0011): the ready-to-sign attestation target for the given
    /// stake-bond outpoint — the message the `kaspa-pq-validator` sidecar ML-DSA-87-signs.
    async fn get_validator_attestation_target(
        &self,
        request: GetValidatorAttestationTargetRequest,
    ) -> RpcResult<GetValidatorAttestationTargetResponse> {
        self.get_validator_attestation_target_call(None, request).await
    }
    /// Default returns `available: false`, so non-server `RpcApi` impls inherit a no-op;
    /// the node's core service overrides it.
    async fn get_validator_attestation_target_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetValidatorAttestationTargetRequest,
    ) -> RpcResult<GetValidatorAttestationTargetResponse> {
        let _ = (connection, request);
        Ok(GetValidatorAttestationTargetResponse::default())
    }

    /// kaspa-pq Phase 12 (ADR-0011): a stake bond's status at the node's sink, for the
    /// `kaspa-pq-validator` sidecar's bond-lifecycle state machine.
    async fn get_stake_bond(&self, request: GetStakeBondRequest) -> RpcResult<GetStakeBondResponse> {
        self.get_stake_bond_call(None, request).await
    }
    /// Default returns `available: false`, so non-server `RpcApi` impls inherit a no-op;
    /// the node's core service overrides it.
    async fn get_stake_bond_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetStakeBondRequest,
    ) -> RpcResult<GetStakeBondResponse> {
        let _ = (connection, request);
        Ok(GetStakeBondResponse::default())
    }

    /// kaspa-pq: a paged, filtered enumeration of stake bonds (see
    /// `GetStakeBondsRequest`). Lets a bond owner recover the outpoint(s) of the
    /// bonds they funded — the key a `StakeUnbondRequest` binds to.
    async fn get_stake_bonds(&self, request: GetStakeBondsRequest) -> RpcResult<GetStakeBondsResponse> {
        self.get_stake_bonds_call(None, request).await
    }
    /// Default returns an empty page, so non-server `RpcApi` impls inherit a no-op;
    /// the node's core service overrides it.
    async fn get_stake_bonds_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetStakeBondsRequest,
    ) -> RpcResult<GetStakeBondsResponse> {
        let _ = (connection, request);
        Ok(GetStakeBondsResponse::default())
    }

    /// Bans the given ip.
    async fn ban(&self, ip: RpcIpAddress) -> RpcResult<()> {
        self.ban_call(None, BanRequest::new(ip)).await?;
        Ok(())
    }
    async fn ban_call(&self, connection: Option<&DynRpcConnection>, request: BanRequest) -> RpcResult<BanResponse>;

    /// Unbans the given ip.
    async fn unban(&self, ip: RpcIpAddress) -> RpcResult<()> {
        self.unban_call(None, UnbanRequest::new(ip)).await?;
        Ok(())
    }
    async fn unban_call(&self, connection: Option<&DynRpcConnection>, request: UnbanRequest) -> RpcResult<UnbanResponse>;

    /// Returns info about the node.
    async fn get_info(&self) -> RpcResult<GetInfoResponse> {
        self.get_info_call(None, GetInfoRequest {}).await
    }
    async fn get_info_call(&self, connection: Option<&DynRpcConnection>, request: GetInfoRequest) -> RpcResult<GetInfoResponse>;

    ///
    async fn estimate_network_hashes_per_second(&self, window_size: u32, start_hash: Option<RpcHash>) -> RpcResult<u64> {
        Ok(self
            .estimate_network_hashes_per_second_call(None, EstimateNetworkHashesPerSecondRequest::new(window_size, start_hash))
            .await?
            .network_hashes_per_second)
    }
    async fn estimate_network_hashes_per_second_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: EstimateNetworkHashesPerSecondRequest,
    ) -> RpcResult<EstimateNetworkHashesPerSecondResponse>;

    ///
    async fn get_mempool_entries_by_addresses(
        &self,
        addresses: Vec<RpcAddress>,
        include_orphan_pool: bool,
        filter_transaction_pool: bool,
    ) -> RpcResult<Vec<RpcMempoolEntryByAddress>> {
        Ok(self
            .get_mempool_entries_by_addresses_call(
                None,
                GetMempoolEntriesByAddressesRequest::new(addresses, include_orphan_pool, filter_transaction_pool),
            )
            .await?
            .entries)
    }
    async fn get_mempool_entries_by_addresses_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetMempoolEntriesByAddressesRequest,
    ) -> RpcResult<GetMempoolEntriesByAddressesResponse>;

    ///
    async fn get_coin_supply(&self) -> RpcResult<GetCoinSupplyResponse> {
        self.get_coin_supply_call(None, GetCoinSupplyRequest {}).await
    }
    async fn get_coin_supply_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetCoinSupplyRequest,
    ) -> RpcResult<GetCoinSupplyResponse>;

    async fn get_daa_score_timestamp_estimate(&self, daa_scores: Vec<u64>) -> RpcResult<Vec<u64>> {
        Ok(self.get_daa_score_timestamp_estimate_call(None, GetDaaScoreTimestampEstimateRequest { daa_scores }).await?.timestamps)
    }
    async fn get_daa_score_timestamp_estimate_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetDaaScoreTimestampEstimateRequest,
    ) -> RpcResult<GetDaaScoreTimestampEstimateResponse>;

    // PR-9.5f: txid widened to RpcTransactionId (= Hash64).
    async fn get_utxo_return_address(&self, txid: RpcTransactionId, accepting_block_daa_score: u64) -> RpcResult<RpcAddress> {
        Ok(self
            .get_utxo_return_address_call(None, GetUtxoReturnAddressRequest { txid, accepting_block_daa_score })
            .await?
            .return_address)
    }
    async fn get_utxo_return_address_call(
        &self,
        _connection: Option<&DynRpcConnection>,
        request: GetUtxoReturnAddressRequest,
    ) -> RpcResult<GetUtxoReturnAddressResponse>;

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Fee estimation API

    async fn get_fee_estimate(&self) -> RpcResult<RpcFeeEstimate> {
        Ok(self.get_fee_estimate_call(None, GetFeeEstimateRequest {}).await?.estimate)
    }
    async fn get_fee_estimate_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetFeeEstimateRequest,
    ) -> RpcResult<GetFeeEstimateResponse>;

    async fn get_fee_estimate_experimental(&self, verbose: bool) -> RpcResult<GetFeeEstimateExperimentalResponse> {
        self.get_fee_estimate_experimental_call(None, GetFeeEstimateExperimentalRequest { verbose }).await
    }
    async fn get_fee_estimate_experimental_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetFeeEstimateExperimentalRequest,
    ) -> RpcResult<GetFeeEstimateExperimentalResponse>;

    ///
    async fn get_current_block_color(&self, hash: RpcHash) -> RpcResult<GetCurrentBlockColorResponse> {
        Ok(self.get_current_block_color_call(None, GetCurrentBlockColorRequest { hash }).await?)
    }
    async fn get_current_block_color_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetCurrentBlockColorRequest,
    ) -> RpcResult<GetCurrentBlockColorResponse>;

    async fn get_virtual_chain_from_block_v2(
        &self,
        start_hash: RpcHash,
        data_verbosity_level: Option<RpcDataVerbosityLevel>,
        min_confirmation_count: Option<u64>,
    ) -> RpcResult<GetVirtualChainFromBlockV2Response> {
        self.get_virtual_chain_from_block_v2_call(
            None,
            GetVirtualChainFromBlockV2Request::new(start_hash, data_verbosity_level, min_confirmation_count),
        )
        .await
    }
    async fn get_virtual_chain_from_block_v2_call(
        &self,
        connection: Option<&DynRpcConnection>,
        request: GetVirtualChainFromBlockV2Request,
    ) -> RpcResult<GetVirtualChainFromBlockV2Response>;

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id identifying it.
    fn register_new_listener(&self, connection: ChannelConnection) -> ListenerId;

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener, unregister the id and its associated connection.
    async fn unregister_listener(&self, id: ListenerId) -> RpcResult<()>;

    /// Start sending notifications of some type to a listener.
    async fn start_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()>;

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()>;

    /// Execute a subscription command leading to either start or stop sending notifications
    /// of some type to a listener.
    async fn execute_subscribe_command(&self, id: ListenerId, scope: Scope, command: Command) -> RpcResult<()> {
        match command {
            Command::Start => self.start_notify(id, scope).await,
            Command::Stop => self.stop_notify(id, scope).await,
        }
    }
}

pub type DynRpcService = Arc<dyn RpcApi>;

downcast_sync!(dyn RpcApi);
