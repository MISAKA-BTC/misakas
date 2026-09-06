use crate::model::*;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::api::stats::BlockCount;
use kaspa_core::debug;
use kaspa_notify::subscription::{Command, context::SubscriptionContext, single::UtxosChangedSubscription};
use kaspa_utils::hex::ToHex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{
    fmt::{Display, Formatter},
    sync::Arc,
};
use workflow_serializer::prelude::*;

pub type RpcExtraData = Vec<u8>;

/// SubmitBlockRequest requests to submit a block into the DAG.
/// Blocks are generally expected to have been generated using the getBlockTemplate call.
///
/// See: [`GetBlockTemplateRequest`]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitBlockRequest {
    pub block: RpcRawBlock,
    #[serde(alias = "allowNonDAABlocks")]
    pub allow_non_daa_blocks: bool,
}
impl SubmitBlockRequest {
    pub fn new(block: RpcRawBlock, allow_non_daa_blocks: bool) -> Self {
        Self { block, allow_non_daa_blocks }
    }
}

impl Serializer for SubmitBlockRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcRawBlock, &self.block, writer)?;
        store!(bool, &self.allow_non_daa_blocks, writer)?;

        Ok(())
    }
}

impl Deserializer for SubmitBlockRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let block = deserialize!(RpcRawBlock, reader)?;
        let allow_non_daa_blocks = load!(bool, reader)?;

        Ok(Self { block, allow_non_daa_blocks })
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
#[borsh(use_discriminant = true)]
pub enum SubmitBlockRejectReason {
    BlockInvalid = 1,
    IsInIBD = 2,
    RouteIsFull = 3,
}
impl SubmitBlockRejectReason {
    fn as_str(&self) -> &'static str {
        // see app\appmessage\rpc_submit_block.go, line 35
        match self {
            SubmitBlockRejectReason::BlockInvalid => "block is invalid",
            SubmitBlockRejectReason::IsInIBD => "node is not synced",
            SubmitBlockRejectReason::RouteIsFull => "route is full",
        }
    }
}
impl Display for SubmitBlockRejectReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Eq, PartialEq, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type", content = "reason")]
#[borsh(use_discriminant = true)]
pub enum SubmitBlockReport {
    Success,
    Reject(SubmitBlockRejectReason),
}
impl SubmitBlockReport {
    pub fn is_success(&self) -> bool {
        *self == SubmitBlockReport::Success
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitBlockResponse {
    pub report: SubmitBlockReport,
}

impl Serializer for SubmitBlockResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(SubmitBlockReport, &self.report, writer)?;
        Ok(())
    }
}

impl Deserializer for SubmitBlockResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let report = load!(SubmitBlockReport, reader)?;

        Ok(Self { report })
    }
}

/// GetBlockTemplateRequest requests a current block template.
/// Callers are expected to solve the block template and submit it using the submitBlock call
///
/// See: [`SubmitBlockRequest`]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockTemplateRequest {
    /// Which kaspa address should the coinbase block reward transaction pay into
    pub pay_address: RpcAddress,
    // TODO: replace with hex serialization
    pub extra_data: RpcExtraData,
}
impl GetBlockTemplateRequest {
    pub fn new(pay_address: RpcAddress, extra_data: RpcExtraData) -> Self {
        Self { pay_address, extra_data }
    }
}

impl Serializer for GetBlockTemplateRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcAddress, &self.pay_address, writer)?;
        store!(RpcExtraData, &self.extra_data, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlockTemplateRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let pay_address = load!(RpcAddress, reader)?;
        let extra_data = load!(RpcExtraData, reader)?;

        Ok(Self { pay_address, extra_data })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockTemplateResponse {
    pub block: RpcRawBlock,

    /// Whether kaspad thinks that it's synced.
    /// Callers are discouraged (but not forbidden) from solving blocks when kaspad is not synced.
    /// That is because when kaspad isn't in sync with the rest of the network there's a high
    /// chance the block will never be accepted, thus the solving effort would have been wasted.
    pub is_synced: bool,
}

impl Serializer for GetBlockTemplateResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcRawBlock, &self.block, writer)?;
        store!(bool, &self.is_synced, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlockTemplateResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let block = deserialize!(RpcRawBlock, reader)?;
        let is_synced = load!(bool, reader)?;

        Ok(Self { block, is_synced })
    }
}

/// GetBlockRequest requests information about a specific block
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockRequest {
    /// The hash of the requested block
    pub hash: RpcHash,

    /// Whether to include transaction data in the response
    pub include_transactions: bool,
}
impl GetBlockRequest {
    pub fn new(hash: RpcHash, include_transactions: bool) -> Self {
        Self { hash, include_transactions }
    }
}

impl Serializer for GetBlockRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.hash, writer)?;
        store!(bool, &self.include_transactions, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlockRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let hash = load!(RpcHash, reader)?;
        let include_transactions = load!(bool, reader)?;

        Ok(Self { hash, include_transactions })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockResponse {
    pub block: RpcBlock,
}

impl Serializer for GetBlockResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcBlock, &self.block, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlockResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let block = deserialize!(RpcBlock, reader)?;

        Ok(Self { block })
    }
}

/// GetInfoRequest returns info about the node.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetInfoRequest {}

impl Serializer for GetInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetInfoResponse {
    pub p2p_id: String,
    pub mempool_size: u64,
    pub server_version: String,
    pub is_utxo_indexed: bool,
    pub is_synced: bool,
    pub has_notify_command: bool,
    pub has_message_id: bool,
}

impl Serializer for GetInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.p2p_id, writer)?;
        store!(u64, &self.mempool_size, writer)?;
        store!(String, &self.server_version, writer)?;
        store!(bool, &self.is_utxo_indexed, writer)?;
        store!(bool, &self.is_synced, writer)?;
        store!(bool, &self.has_notify_command, writer)?;
        store!(bool, &self.has_message_id, writer)?;

        Ok(())
    }
}

impl Deserializer for GetInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let p2p_id = load!(String, reader)?;
        let mempool_size = load!(u64, reader)?;
        let server_version = load!(String, reader)?;
        let is_utxo_indexed = load!(bool, reader)?;
        let is_synced = load!(bool, reader)?;
        let has_notify_command = load!(bool, reader)?;
        let has_message_id = load!(bool, reader)?;

        Ok(Self { p2p_id, mempool_size, server_version, is_utxo_indexed, is_synced, has_notify_command, has_message_id })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentNetworkRequest {}

impl Serializer for GetCurrentNetworkRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetCurrentNetworkRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentNetworkResponse {
    pub network: RpcNetworkType,
}

impl GetCurrentNetworkResponse {
    pub fn new(network: RpcNetworkType) -> Self {
        Self { network }
    }
}

impl Serializer for GetCurrentNetworkResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcNetworkType, &self.network, writer)?;
        Ok(())
    }
}

impl Deserializer for GetCurrentNetworkResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let network = load!(RpcNetworkType, reader)?;
        Ok(Self { network })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPeerAddressesRequest {}

impl Serializer for GetPeerAddressesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPeerAddressesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPeerAddressesResponse {
    pub known_addresses: Vec<RpcPeerAddress>,
    pub banned_addresses: Vec<RpcIpAddress>,
}

impl GetPeerAddressesResponse {
    pub fn new(known_addresses: Vec<RpcPeerAddress>, banned_addresses: Vec<RpcIpAddress>) -> Self {
        Self { known_addresses, banned_addresses }
    }
}

impl Serializer for GetPeerAddressesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcPeerAddress>, &self.known_addresses, writer)?;
        store!(Vec<RpcIpAddress>, &self.banned_addresses, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPeerAddressesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let known_addresses = load!(Vec<RpcPeerAddress>, reader)?;
        let banned_addresses = load!(Vec<RpcIpAddress>, reader)?;
        Ok(Self { known_addresses, banned_addresses })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSinkRequest {}

impl Serializer for GetSinkRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSinkRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSinkResponse {
    pub sink: RpcHash,
}

impl GetSinkResponse {
    pub fn new(selected_tip_hash: RpcHash) -> Self {
        Self { sink: selected_tip_hash }
    }
}

impl Serializer for GetSinkResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.sink, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSinkResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let sink = load!(RpcHash, reader)?;
        Ok(Self { sink })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntryRequest {
    pub transaction_id: RpcTransactionId,
    pub include_orphan_pool: bool,
    // TODO: replace with `include_transaction_pool`
    pub filter_transaction_pool: bool,
}

impl GetMempoolEntryRequest {
    pub fn new(transaction_id: RpcTransactionId, include_orphan_pool: bool, filter_transaction_pool: bool) -> Self {
        Self { transaction_id, include_orphan_pool, filter_transaction_pool }
    }
}

impl Serializer for GetMempoolEntryRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcTransactionId, &self.transaction_id, writer)?;
        store!(bool, &self.include_orphan_pool, writer)?;
        store!(bool, &self.filter_transaction_pool, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMempoolEntryRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction_id = load!(RpcTransactionId, reader)?;
        let include_orphan_pool = load!(bool, reader)?;
        let filter_transaction_pool = load!(bool, reader)?;

        Ok(Self { transaction_id, include_orphan_pool, filter_transaction_pool })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntryResponse {
    pub mempool_entry: RpcMempoolEntry,
}

impl GetMempoolEntryResponse {
    pub fn new(mempool_entry: RpcMempoolEntry) -> Self {
        Self { mempool_entry }
    }
}

impl Serializer for GetMempoolEntryResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcMempoolEntry, &self.mempool_entry, writer)?;
        Ok(())
    }
}

impl Deserializer for GetMempoolEntryResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let mempool_entry = deserialize!(RpcMempoolEntry, reader)?;
        Ok(Self { mempool_entry })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesRequest {
    pub include_orphan_pool: bool,
    // TODO: replace with `include_transaction_pool`
    pub filter_transaction_pool: bool,
}

impl GetMempoolEntriesRequest {
    pub fn new(include_orphan_pool: bool, filter_transaction_pool: bool) -> Self {
        Self { include_orphan_pool, filter_transaction_pool }
    }
}

impl Serializer for GetMempoolEntriesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.include_orphan_pool, writer)?;
        store!(bool, &self.filter_transaction_pool, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMempoolEntriesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let include_orphan_pool = load!(bool, reader)?;
        let filter_transaction_pool = load!(bool, reader)?;

        Ok(Self { include_orphan_pool, filter_transaction_pool })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesResponse {
    pub mempool_entries: Vec<RpcMempoolEntry>,
}

impl GetMempoolEntriesResponse {
    pub fn new(mempool_entries: Vec<RpcMempoolEntry>) -> Self {
        Self { mempool_entries }
    }
}

impl Serializer for GetMempoolEntriesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcMempoolEntry>, &self.mempool_entries, writer)?;
        Ok(())
    }
}

impl Deserializer for GetMempoolEntriesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let mempool_entries = deserialize!(Vec<RpcMempoolEntry>, reader)?;
        Ok(Self { mempool_entries })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConnectedPeerInfoRequest {}

impl Serializer for GetConnectedPeerInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetConnectedPeerInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConnectedPeerInfoResponse {
    pub peer_info: Vec<RpcPeerInfo>,
}

impl GetConnectedPeerInfoResponse {
    pub fn new(peer_info: Vec<RpcPeerInfo>) -> Self {
        Self { peer_info }
    }
}

impl Serializer for GetConnectedPeerInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcPeerInfo>, &self.peer_info, writer)?;
        Ok(())
    }
}

impl Deserializer for GetConnectedPeerInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let peer_info = load!(Vec<RpcPeerInfo>, reader)?;
        Ok(Self { peer_info })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPeerRequest {
    pub peer_address: RpcContextualPeerAddress,
    pub is_permanent: bool,
}

impl AddPeerRequest {
    pub fn new(peer_address: RpcContextualPeerAddress, is_permanent: bool) -> Self {
        Self { peer_address, is_permanent }
    }
}

impl Serializer for AddPeerRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcContextualPeerAddress, &self.peer_address, writer)?;
        store!(bool, &self.is_permanent, writer)?;

        Ok(())
    }
}

impl Deserializer for AddPeerRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let peer_address = load!(RpcContextualPeerAddress, reader)?;
        let is_permanent = load!(bool, reader)?;

        Ok(Self { peer_address, is_permanent })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPeerResponse {}

impl Serializer for AddPeerResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for AddPeerResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTransactionRequest {
    pub transaction: RpcTransaction,
    pub allow_orphan: bool,
}

impl SubmitTransactionRequest {
    pub fn new(transaction: RpcTransaction, allow_orphan: bool) -> Self {
        Self { transaction, allow_orphan }
    }
}

impl Serializer for SubmitTransactionRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcTransaction, &self.transaction, writer)?;
        store!(bool, &self.allow_orphan, writer)?;

        Ok(())
    }
}

impl Deserializer for SubmitTransactionRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction = deserialize!(RpcTransaction, reader)?;
        let allow_orphan = load!(bool, reader)?;

        Ok(Self { transaction, allow_orphan })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTransactionResponse {
    pub transaction_id: RpcTransactionId,
}

impl SubmitTransactionResponse {
    pub fn new(transaction_id: RpcTransactionId) -> Self {
        Self { transaction_id }
    }
}

impl Serializer for SubmitTransactionResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcTransactionId, &self.transaction_id, writer)?;

        Ok(())
    }
}

impl Deserializer for SubmitTransactionResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction_id = load!(RpcTransactionId, reader)?;

        Ok(Self { transaction_id })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTransactionReplacementRequest {
    pub transaction: RpcTransaction,
}

impl SubmitTransactionReplacementRequest {
    pub fn new(transaction: RpcTransaction) -> Self {
        Self { transaction }
    }
}

impl Serializer for SubmitTransactionReplacementRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcTransaction, &self.transaction, writer)?;

        Ok(())
    }
}

impl Deserializer for SubmitTransactionReplacementRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction = deserialize!(RpcTransaction, reader)?;

        Ok(Self { transaction })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTransactionReplacementResponse {
    pub transaction_id: RpcTransactionId,
    pub replaced_transaction: RpcTransaction,
}

impl SubmitTransactionReplacementResponse {
    pub fn new(transaction_id: RpcTransactionId, replaced_transaction: RpcTransaction) -> Self {
        Self { transaction_id, replaced_transaction }
    }
}

impl Serializer for SubmitTransactionReplacementResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcTransactionId, &self.transaction_id, writer)?;
        serialize!(RpcTransaction, &self.replaced_transaction, writer)?;

        Ok(())
    }
}

impl Deserializer for SubmitTransactionReplacementResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction_id = load!(RpcTransactionId, reader)?;
        let replaced_transaction = deserialize!(RpcTransaction, reader)?;

        Ok(Self { transaction_id, replaced_transaction })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSubnetworkRequest {
    pub subnetwork_id: RpcSubnetworkId,
}

impl GetSubnetworkRequest {
    pub fn new(subnetwork_id: RpcSubnetworkId) -> Self {
        Self { subnetwork_id }
    }
}

impl Serializer for GetSubnetworkRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcSubnetworkId, &self.subnetwork_id, writer)?;

        Ok(())
    }
}

impl Deserializer for GetSubnetworkRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let subnetwork_id = load!(RpcSubnetworkId, reader)?;

        Ok(Self { subnetwork_id })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSubnetworkResponse {
    pub gas_limit: u64,
}

impl GetSubnetworkResponse {
    pub fn new(gas_limit: u64) -> Self {
        Self { gas_limit }
    }
}

impl Serializer for GetSubnetworkResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.gas_limit, writer)?;

        Ok(())
    }
}

impl Deserializer for GetSubnetworkResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let gas_limit = load!(u64, reader)?;

        Ok(Self { gas_limit })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualChainFromBlockRequest {
    pub start_hash: RpcHash,
    pub include_accepted_transaction_ids: bool,
    pub min_confirmation_count: Option<u64>,
}

impl GetVirtualChainFromBlockRequest {
    pub fn new(start_hash: RpcHash, include_accepted_transaction_ids: bool, min_confirmation_count: Option<u64>) -> Self {
        Self { start_hash, include_accepted_transaction_ids, min_confirmation_count }
    }
}

impl Serializer for GetVirtualChainFromBlockRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(RpcHash, &self.start_hash, writer)?;
        store!(bool, &self.include_accepted_transaction_ids, writer)?;
        store!(Option<u64>, &self.min_confirmation_count, writer)?;

        Ok(())
    }
}

impl Deserializer for GetVirtualChainFromBlockRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let start_hash = load!(RpcHash, reader)?;
        let include_accepted_transaction_ids = load!(bool, reader)?;

        let min_confirmation_count = if version > 1 { load!(Option<u64>, reader)? } else { None };

        Ok(Self { start_hash, include_accepted_transaction_ids, min_confirmation_count })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualChainFromBlockResponse {
    pub removed_chain_block_hashes: Vec<RpcHash>,
    pub added_chain_block_hashes: Vec<RpcHash>,
    pub accepted_transaction_ids: Vec<RpcAcceptedTransactionIds>,
}

impl GetVirtualChainFromBlockResponse {
    pub fn new(
        removed_chain_block_hashes: Vec<RpcHash>,
        added_chain_block_hashes: Vec<RpcHash>,
        accepted_transaction_ids: Vec<RpcAcceptedTransactionIds>,
    ) -> Self {
        Self { removed_chain_block_hashes, added_chain_block_hashes, accepted_transaction_ids }
    }
}

impl Serializer for GetVirtualChainFromBlockResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcHash>, &self.removed_chain_block_hashes, writer)?;
        store!(Vec<RpcHash>, &self.added_chain_block_hashes, writer)?;
        store!(Vec<RpcAcceptedTransactionIds>, &self.accepted_transaction_ids, writer)?;

        Ok(())
    }
}

impl Deserializer for GetVirtualChainFromBlockResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let removed_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let added_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let accepted_transaction_ids = load!(Vec<RpcAcceptedTransactionIds>, reader)?;

        Ok(Self { removed_chain_block_hashes, added_chain_block_hashes, accepted_transaction_ids })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlocksRequest {
    pub low_hash: Option<RpcHash>,
    pub include_blocks: bool,
    pub include_transactions: bool,
}

impl GetBlocksRequest {
    pub fn new(low_hash: Option<RpcHash>, include_blocks: bool, include_transactions: bool) -> Self {
        Self { low_hash, include_blocks, include_transactions }
    }
}

impl Serializer for GetBlocksRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Option<RpcHash>, &self.low_hash, writer)?;
        store!(bool, &self.include_blocks, writer)?;
        store!(bool, &self.include_transactions, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlocksRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let low_hash = load!(Option<RpcHash>, reader)?;
        let include_blocks = load!(bool, reader)?;
        let include_transactions = load!(bool, reader)?;

        Ok(Self { low_hash, include_blocks, include_transactions })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlocksResponse {
    pub block_hashes: Vec<RpcHash>,
    pub blocks: Vec<RpcBlock>,
}

impl GetBlocksResponse {
    pub fn new(block_hashes: Vec<RpcHash>, blocks: Vec<RpcBlock>) -> Self {
        Self { block_hashes, blocks }
    }
}

impl Serializer for GetBlocksResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcHash>, &self.block_hashes, writer)?;
        serialize!(Vec<RpcBlock>, &self.blocks, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlocksResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let block_hashes = load!(Vec<RpcHash>, reader)?;
        let blocks = deserialize!(Vec<RpcBlock>, reader)?;

        Ok(Self { block_hashes, blocks })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockCountRequest {}

impl Serializer for GetBlockCountRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetBlockCountRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

pub type GetBlockCountResponse = BlockCount;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockDagInfoRequest {}

impl Serializer for GetBlockDagInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetBlockDagInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockDagInfoResponse {
    pub network: RpcNetworkId,
    pub block_count: u64,
    pub header_count: u64,
    pub tip_hashes: Vec<RpcHash>,
    pub difficulty: f64,
    pub past_median_time: u64, // NOTE: i64 in gRPC protowire
    pub virtual_parent_hashes: Vec<RpcHash>,
    pub pruning_point_hash: RpcHash,
    pub virtual_daa_score: u64,
    pub sink: RpcHash,
}

impl GetBlockDagInfoResponse {
    pub fn new(
        network: RpcNetworkId,
        block_count: u64,
        header_count: u64,
        tip_hashes: Vec<RpcHash>,
        difficulty: f64,
        past_median_time: u64,
        virtual_parent_hashes: Vec<RpcHash>,
        pruning_point_hash: RpcHash,
        virtual_daa_score: u64,
        sink: RpcHash,
    ) -> Self {
        Self {
            network,
            block_count,
            header_count,
            tip_hashes,
            difficulty,
            past_median_time,
            virtual_parent_hashes,
            pruning_point_hash,
            virtual_daa_score,
            sink,
        }
    }
}

impl Serializer for GetBlockDagInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcNetworkId, &self.network, writer)?;
        store!(u64, &self.block_count, writer)?;
        store!(u64, &self.header_count, writer)?;
        store!(Vec<RpcHash>, &self.tip_hashes, writer)?;
        store!(f64, &self.difficulty, writer)?;
        store!(u64, &self.past_median_time, writer)?;
        store!(Vec<RpcHash>, &self.virtual_parent_hashes, writer)?;
        store!(RpcHash, &self.pruning_point_hash, writer)?;
        store!(u64, &self.virtual_daa_score, writer)?;
        store!(RpcHash, &self.sink, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlockDagInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let network = load!(RpcNetworkId, reader)?;
        let block_count = load!(u64, reader)?;
        let header_count = load!(u64, reader)?;
        let tip_hashes = load!(Vec<RpcHash>, reader)?;
        let difficulty = load!(f64, reader)?;
        let past_median_time = load!(u64, reader)?;
        let virtual_parent_hashes = load!(Vec<RpcHash>, reader)?;
        let pruning_point_hash = load!(RpcHash, reader)?;
        let virtual_daa_score = load!(u64, reader)?;
        let sink = load!(RpcHash, reader)?;

        Ok(Self {
            network,
            block_count,
            header_count,
            tip_hashes,
            difficulty,
            past_median_time,
            virtual_parent_hashes,
            pruning_point_hash,
            virtual_daa_score,
            sink,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveFinalityConflictRequest {
    pub finality_block_hash: RpcHash,
}

impl ResolveFinalityConflictRequest {
    pub fn new(finality_block_hash: RpcHash) -> Self {
        Self { finality_block_hash }
    }
}

impl Serializer for ResolveFinalityConflictRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.finality_block_hash, writer)?;

        Ok(())
    }
}

impl Deserializer for ResolveFinalityConflictRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let finality_block_hash = load!(RpcHash, reader)?;

        Ok(Self { finality_block_hash })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveFinalityConflictResponse {}

impl Serializer for ResolveFinalityConflictResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for ResolveFinalityConflictResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownRequest {}

impl Serializer for ShutdownRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for ShutdownRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownResponse {}

impl Serializer for ShutdownResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for ShutdownResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetHeadersRequest {
    pub start_hash: RpcHash,
    pub limit: u64,
    pub is_ascending: bool,
}

impl GetHeadersRequest {
    pub fn new(start_hash: RpcHash, limit: u64, is_ascending: bool) -> Self {
        Self { start_hash, limit, is_ascending }
    }
}

impl Serializer for GetHeadersRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.start_hash, writer)?;
        store!(u64, &self.limit, writer)?;
        store!(bool, &self.is_ascending, writer)?;

        Ok(())
    }
}

impl Deserializer for GetHeadersRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let start_hash = load!(RpcHash, reader)?;
        let limit = load!(u64, reader)?;
        let is_ascending = load!(bool, reader)?;

        Ok(Self { start_hash, limit, is_ascending })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetHeadersResponse {
    pub headers: Vec<RpcHeader>,
}

impl GetHeadersResponse {
    pub fn new(headers: Vec<RpcHeader>) -> Self {
        Self { headers }
    }
}

impl Serializer for GetHeadersResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcHeader>, &self.headers, writer)?;

        Ok(())
    }
}

impl Deserializer for GetHeadersResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let headers = load!(Vec<RpcHeader>, reader)?;

        Ok(Self { headers })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBalanceByAddressRequest {
    pub address: RpcAddress,
}

impl GetBalanceByAddressRequest {
    pub fn new(address: RpcAddress) -> Self {
        Self { address }
    }
}

impl Serializer for GetBalanceByAddressRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcAddress, &self.address, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBalanceByAddressRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let address = load!(RpcAddress, reader)?;

        Ok(Self { address })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBalanceByAddressResponse {
    pub balance: u64,
}

impl GetBalanceByAddressResponse {
    pub fn new(balance: u64) -> Self {
        Self { balance }
    }
}

impl Serializer for GetBalanceByAddressResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.balance, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBalanceByAddressResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let balance = load!(u64, reader)?;

        Ok(Self { balance })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBalancesByAddressesRequest {
    pub addresses: Vec<RpcAddress>,
}

impl GetBalancesByAddressesRequest {
    pub fn new(addresses: Vec<RpcAddress>) -> Self {
        Self { addresses }
    }
}

impl Serializer for GetBalancesByAddressesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcAddress>, &self.addresses, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBalancesByAddressesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let addresses = load!(Vec<RpcAddress>, reader)?;

        Ok(Self { addresses })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBalancesByAddressesResponse {
    pub entries: Vec<RpcBalancesByAddressesEntry>,
}

impl GetBalancesByAddressesResponse {
    pub fn new(entries: Vec<RpcBalancesByAddressesEntry>) -> Self {
        Self { entries }
    }
}

impl Serializer for GetBalancesByAddressesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcBalancesByAddressesEntry>, &self.entries, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBalancesByAddressesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let entries = deserialize!(Vec<RpcBalancesByAddressesEntry>, reader)?;

        Ok(Self { entries })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSinkBlueScoreRequest {}

impl Serializer for GetSinkBlueScoreRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSinkBlueScoreRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSinkBlueScoreResponse {
    pub blue_score: u64,
}

impl GetSinkBlueScoreResponse {
    pub fn new(blue_score: u64) -> Self {
        Self { blue_score }
    }
}

impl Serializer for GetSinkBlueScoreResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.blue_score, writer)?;

        Ok(())
    }
}

impl Deserializer for GetSinkBlueScoreResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let blue_score = load!(u64, reader)?;

        Ok(Self { blue_score })
    }
}

// kaspa-pq Phase 10 (ADR-0009): getDnsConfirmation. The response carries
// RPC-friendly encodings (hex / decimal strings for Hash64 / Uint576 / u128)
// built from the consensus `DnsConfirmation`. `available` is false when the
// DNS overlay is not configured for the network (or no DnsState exists yet),
// in which case the remaining fields are defaults.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDnsConfirmationRequest {
    /// Optional. When non-empty, the response additionally reports whether THIS block (128-hex hash)
    /// is DNS-final — a selected-chain ancestor of (or equal to) the stake-confirmed anchor. Empty
    /// (the back-compatible default) returns only the node-wide current view.
    #[serde(default)]
    pub block_hash: String,
}

impl Serializer for GetDnsConfirmationRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(String, &self.block_hash, writer)?;
        Ok(())
    }
}

impl Deserializer for GetDnsConfirmationRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let block_hash = if version >= 2 { load!(String, reader)? } else { String::new() };
        Ok(Self { block_hash })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDnsConfirmationResponse {
    pub available: bool,
    pub block_hash: String,
    pub work_depth: String,
    pub required_work_depth: String,
    pub stake_depth: String,
    pub required_stake_depth: String,
    pub pow_confirmed: bool,
    pub dns_confirmed: bool,
    pub rollout_stage: u32,
    pub expected_dns_confirmation_seconds: u64,
    pub work_reorg_risk_upper_bound: String,
    pub stake_reorg_risk_upper_bound: String,
    pub dns_reorg_risk_conservative_bound: String,
    pub note: String,
    /// kaspa-pq Phase 13 (ADR-0018 §C): DnsHealth discriminant (0 = DisabledBeforeActivation,
    /// 1 = Active, 2 = DegradedStakeQualityLow, 3 = DegradedCertificateCensored). A read-only
    /// liveness signal — a degraded value means the DNS-confirmed anchor has stalled, never
    /// that any block is invalid. `0` when `available` is false.
    pub health: u32,
    /// audit M-01: the LAST DNS-confirmed canonical lagged anchor (the stable finality point) and
    /// its DAA score — distinct from `block_hash`, which is the pov-dependent selected-chain sink.
    /// Explorers/exchanges MUST treat THIS as DNS-final, not `block_hash`. Empty / 0 until confirmed.
    pub last_dns_confirmed_anchor: String,
    pub last_dns_confirmed_anchor_daa_score: u64,
    /// kaspa-pq: per-block DNS finality — populated only when the request carried a `block_hash`.
    /// `block_found` = the block exists; `block_is_dns_final` = it is a selected-chain ancestor of
    /// (or equal to) the stake-confirmed anchor (i.e. irreversible under DNS finality);
    /// `block_is_confirmed_anchor` = it IS the current confirmed anchor; `block_daa_score` = its DAA.
    #[serde(default)]
    pub block_found: bool,
    #[serde(default)]
    pub block_is_dns_final: bool,
    #[serde(default)]
    pub block_is_confirmed_anchor: bool,
    #[serde(default)]
    pub block_daa_score: u64,
    /// MISAKA VLT activation/finality state, appended in v4. These are what a monitoring system
    /// alerts on, and they are here rather than behind a new RPC because DNS confirmation IS the
    /// finality report — the VLT fences decide whether the numbers above are stake- or
    /// compute-denominated, and whether they can move at all.
    ///
    /// `vlt_state` is the stable label (`pre_shadow` | `shadow` | `fence_reached_no_snapshot` |
    /// `active` | `recovery`). The alert worth writing is
    /// `vlt_weight_fence_reached && !vlt_finality_active`: the hard fork happened and nothing is
    /// being finalized. Weights are decimal strings because `W(E)` is µRTE-scaled `u128`.
    #[serde(default)]
    pub vlt_state: String,
    #[serde(default)]
    pub vlt_shadow_active: bool,
    #[serde(default)]
    pub vlt_weight_fence_reached: bool,
    #[serde(default)]
    pub vlt_finality_active: bool,
    #[serde(default)]
    pub vlt_total_weight: String,
    #[serde(default)]
    pub vlt_quorum_weight: String,
    #[serde(default)]
    pub vlt_snapshot_epoch: u64,
    #[serde(default)]
    pub vlt_snapshot_root: String,
    /// Sink DAA the gauges were written at — lets a scraper tell a steady state from a recompute
    /// that has stopped, which the values alone cannot express.
    #[serde(default)]
    pub vlt_gauges_daa_score: u64,
}

impl Serializer for GetDnsConfirmationResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &4, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.block_hash, writer)?;
        store!(String, &self.work_depth, writer)?;
        store!(String, &self.required_work_depth, writer)?;
        store!(String, &self.stake_depth, writer)?;
        store!(String, &self.required_stake_depth, writer)?;
        store!(bool, &self.pow_confirmed, writer)?;
        store!(bool, &self.dns_confirmed, writer)?;
        store!(u32, &self.rollout_stage, writer)?;
        store!(u64, &self.expected_dns_confirmation_seconds, writer)?;
        store!(String, &self.work_reorg_risk_upper_bound, writer)?;
        store!(String, &self.stake_reorg_risk_upper_bound, writer)?;
        store!(String, &self.dns_reorg_risk_conservative_bound, writer)?;
        store!(String, &self.note, writer)?;
        store!(u32, &self.health, writer)?;
        store!(String, &self.last_dns_confirmed_anchor, writer)?;
        store!(u64, &self.last_dns_confirmed_anchor_daa_score, writer)?;
        store!(bool, &self.block_found, writer)?;
        store!(bool, &self.block_is_dns_final, writer)?;
        store!(bool, &self.block_is_confirmed_anchor, writer)?;
        store!(u64, &self.block_daa_score, writer)?;
        store!(String, &self.vlt_state, writer)?;
        store!(bool, &self.vlt_shadow_active, writer)?;
        store!(bool, &self.vlt_weight_fence_reached, writer)?;
        store!(bool, &self.vlt_finality_active, writer)?;
        store!(String, &self.vlt_total_weight, writer)?;
        store!(String, &self.vlt_quorum_weight, writer)?;
        store!(u64, &self.vlt_snapshot_epoch, writer)?;
        store!(String, &self.vlt_snapshot_root, writer)?;
        store!(u64, &self.vlt_gauges_daa_score, writer)?;
        Ok(())
    }
}

impl Deserializer for GetDnsConfirmationResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let block_hash = load!(String, reader)?;
        let work_depth = load!(String, reader)?;
        let required_work_depth = load!(String, reader)?;
        let stake_depth = load!(String, reader)?;
        let required_stake_depth = load!(String, reader)?;
        let pow_confirmed = load!(bool, reader)?;
        let dns_confirmed = load!(bool, reader)?;
        let rollout_stage = load!(u32, reader)?;
        let expected_dns_confirmation_seconds = load!(u64, reader)?;
        let work_reorg_risk_upper_bound = load!(String, reader)?;
        let stake_reorg_risk_upper_bound = load!(String, reader)?;
        let dns_reorg_risk_conservative_bound = load!(String, reader)?;
        let note = load!(String, reader)?;
        let health = load!(u32, reader)?;
        // audit M-01: fields appended in v2 — tolerate v1 payloads (empty/0).
        let (last_dns_confirmed_anchor, last_dns_confirmed_anchor_daa_score) =
            if version >= 2 { (load!(String, reader)?, load!(u64, reader)?) } else { (String::new(), 0) };
        // kaspa-pq: per-block DNS finality appended in v3 — tolerate v1/v2 payloads (false/0).
        let (block_found, block_is_dns_final, block_is_confirmed_anchor, block_daa_score) = if version >= 3 {
            (load!(bool, reader)?, load!(bool, reader)?, load!(bool, reader)?, load!(u64, reader)?)
        } else {
            (false, false, false, 0)
        };
        // MISAKA: VLT activation/finality gauges appended in v4 — tolerate v1..v3 payloads. A peer
        // that predates the fences reports `pre_shadow`, which is what it is.
        let vlt = if version >= 4 {
            (
                load!(String, reader)?,
                load!(bool, reader)?,
                load!(bool, reader)?,
                load!(bool, reader)?,
                load!(String, reader)?,
                load!(String, reader)?,
                load!(u64, reader)?,
                load!(String, reader)?,
                load!(u64, reader)?,
            )
        } else {
            ("pre_shadow".to_string(), false, false, false, "0".to_string(), "0".to_string(), 0, String::new(), 0)
        };
        Ok(Self {
            available,
            block_hash,
            work_depth,
            required_work_depth,
            stake_depth,
            required_stake_depth,
            pow_confirmed,
            dns_confirmed,
            rollout_stage,
            expected_dns_confirmation_seconds,
            work_reorg_risk_upper_bound,
            stake_reorg_risk_upper_bound,
            dns_reorg_risk_conservative_bound,
            note,
            health,
            last_dns_confirmed_anchor,
            last_dns_confirmed_anchor_daa_score,
            vlt_state: vlt.0,
            vlt_shadow_active: vlt.1,
            vlt_weight_fence_reached: vlt.2,
            vlt_finality_active: vlt.3,
            vlt_total_weight: vlt.4,
            vlt_quorum_weight: vlt.5,
            vlt_snapshot_epoch: vlt.6,
            vlt_snapshot_root: vlt.7,
            vlt_gauges_daa_score: vlt.8,
            block_found,
            block_is_dns_final,
            block_is_confirmed_anchor,
            block_daa_score,
        })
    }
}

// kaspa-pq DNS v3: liveness-first attestation quality monitoring. This is distinct from
// mandatory attestation deficits: shipped networks keep hard inclusion inert, but operators
// still need an RPC-visible list of ready epochs that are below the StakeScore quality floor.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAttestationQualityDeficitsRequest {}

impl Serializer for GetAttestationQualityDeficitsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetAttestationQualityDeficitsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcAttestationQualityDeficit {
    pub epoch: u64,
    /// Ready epoch target hash (Hash64, hex).
    pub target_hash: String,
    pub target_daa_score: u64,
    pub included_stake: u64,
    pub expected_stake: u64,
    pub required_stake: u64,
    pub required_stake_delta: u64,
    pub quality_floor_bps: u16,
    /// DnsHealth discriminant (0 = DisabledBeforeActivation, 1 = Active,
    /// 2 = DegradedStakeQualityLow, 3 = DegradedCertificateCensored).
    pub health: u32,
}

impl Serializer for RpcAttestationQualityDeficit {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.epoch, writer)?;
        store!(String, &self.target_hash, writer)?;
        store!(u64, &self.target_daa_score, writer)?;
        store!(u64, &self.included_stake, writer)?;
        store!(u64, &self.expected_stake, writer)?;
        store!(u64, &self.required_stake, writer)?;
        store!(u64, &self.required_stake_delta, writer)?;
        store!(u16, &self.quality_floor_bps, writer)?;
        store!(u32, &self.health, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcAttestationQualityDeficit {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let epoch = load!(u64, reader)?;
        let target_hash = load!(String, reader)?;
        let target_daa_score = load!(u64, reader)?;
        let included_stake = load!(u64, reader)?;
        let expected_stake = load!(u64, reader)?;
        let required_stake = load!(u64, reader)?;
        let required_stake_delta = load!(u64, reader)?;
        let quality_floor_bps = load!(u16, reader)?;
        let health = load!(u32, reader)?;
        Ok(Self {
            epoch,
            target_hash,
            target_daa_score,
            included_stake,
            expected_stake,
            required_stake,
            required_stake_delta,
            quality_floor_bps,
            health,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAttestationQualityDeficitsResponse {
    pub deficits: Vec<RpcAttestationQualityDeficit>,
}

impl Serializer for GetAttestationQualityDeficitsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcAttestationQualityDeficit>, &self.deficits, writer)?;
        Ok(())
    }
}

impl Deserializer for GetAttestationQualityDeficitsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let deficits = deserialize!(Vec<RpcAttestationQualityDeficit>, reader)?;
        Ok(Self { deficits })
    }
}

// kaspa-pq Phase 11 (ADR-0010): getValidatorStatus. Reports the in-process
// validator service's operational status. `enabled` is false when the node was
// started without `--enable-validator`, in which case the other fields are defaults.
/// kaspa-pq EVM Lane v0.4 (§16): one EVM log entry (RPC view).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcEvmLog {
    /// 20-byte address, hex.
    pub address: String,
    /// 32-byte topics, hex.
    pub topics: Vec<String>,
    /// Log data, hex.
    pub data: String,
}

impl Serializer for RpcEvmLog {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.address, writer)?;
        store!(Vec<String>, &self.topics, writer)?;
        store!(String, &self.data, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcEvmLog {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let address = load!(String, reader)?;
        let topics = load!(Vec<String>, reader)?;
        let data = load!(String, reader)?;
        Ok(Self { address, topics, data })
    }
}

/// kaspa-pq EVM Lane v0.4 (§16): canonical-resolved EVM receipt
/// (`eth_getTransactionReceipt` semantics — `found: false` when the tx is not
/// accepted under the current selected chain).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetEvmTransactionReceiptRequest {
    /// 32-byte Ethereum tx hash, hex (optional 0x).
    pub transaction_hash: String,
}

impl Serializer for GetEvmTransactionReceiptRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.transaction_hash, writer)?;
        Ok(())
    }
}

impl Deserializer for GetEvmTransactionReceiptRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction_hash = load!(String, reader)?;
        Ok(Self { transaction_hash })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetEvmTransactionReceiptResponse {
    pub found: bool,
    /// Accepting chain block (64-byte hash, hex; empty when not found).
    pub accepting_block: String,
    /// The EVM block number formed by the accepting chain block.
    pub evm_number: u64,
    pub receipt_index: u32,
    /// Receipt status (true = success, false = reverted/OOG — §6.1 class 4).
    pub succeeded: bool,
    pub gas_used: u64,
    pub cumulative_gas_used: u64,
    pub logs: Vec<RpcEvmLog>,
}

impl Serializer for GetEvmTransactionReceiptResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.accepting_block, writer)?;
        store!(u64, &self.evm_number, writer)?;
        store!(u32, &self.receipt_index, writer)?;
        store!(bool, &self.succeeded, writer)?;
        store!(u64, &self.gas_used, writer)?;
        store!(u64, &self.cumulative_gas_used, writer)?;
        serialize!(Vec<RpcEvmLog>, &self.logs, writer)?;
        Ok(())
    }
}

impl Deserializer for GetEvmTransactionReceiptResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let found = load!(bool, reader)?;
        let accepting_block = load!(String, reader)?;
        let evm_number = load!(u64, reader)?;
        let receipt_index = load!(u32, reader)?;
        let succeeded = load!(bool, reader)?;
        let gas_used = load!(u64, reader)?;
        let cumulative_gas_used = load!(u64, reader)?;
        let logs = deserialize!(Vec<RpcEvmLog>, reader)?;
        Ok(Self { found, accepting_block, evm_number, receipt_index, succeeded, gas_used, cumulative_gas_used, logs })
    }
}

/// kaspa-pq EVM Lane v0.4 (§16): `misaka_getTxInclusionStatus` — DA inclusion
/// vs acceptance vs skip, the §18.1 latency-layer surface.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetEvmTxInclusionStatusRequest {
    /// 32-byte Ethereum tx hash, hex (optional 0x).
    pub transaction_hash: String,
}

impl Serializer for GetEvmTxInclusionStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.transaction_hash, writer)?;
        Ok(())
    }
}

impl Deserializer for GetEvmTxInclusionStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction_hash = load!(String, reader)?;
        Ok(Self { transaction_hash })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetEvmTxInclusionStatusResponse {
    /// Pending in this node's EVM mempool (§14/§18.1: the pre-inclusion tier).
    pub pending: bool,
    /// Payload blocks carrying the raw tx (DA visibility ≠ execution).
    pub included_in: Vec<String>,
    /// The CANONICAL accepting chain block (empty = not accepted at `latest`).
    pub accepted_in: String,
    /// Valid only when `accepted_in` is non-empty.
    pub receipt_index: u32,
    /// §6.1 class of the most recent skip while never accepted (0 = none).
    pub last_skip_class: u32,
}

impl Serializer for GetEvmTxInclusionStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.pending, writer)?;
        store!(Vec<String>, &self.included_in, writer)?;
        store!(String, &self.accepted_in, writer)?;
        store!(u32, &self.receipt_index, writer)?;
        store!(u32, &self.last_skip_class, writer)?;
        Ok(())
    }
}

impl Deserializer for GetEvmTxInclusionStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let pending = load!(bool, reader)?;
        let included_in = load!(Vec<String>, reader)?;
        let accepted_in = load!(String, reader)?;
        let receipt_index = load!(u32, reader)?;
        let last_skip_class = load!(u32, reader)?;
        Ok(Self { pending, included_in, accepted_in, receipt_index, last_skip_class })
    }
}

/// kaspa-pq EVM Lane v0.4 (§16): submit a raw EIP-2718 EVM transaction (hex,
/// optional 0x prefix) to the node's EVM mempool. Admission applies the
/// body-validation class-1 rule; the tx is data-only until a chain block
/// ACCEPTS the payload block that includes it (mergeset delayed acceptance).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitEvmTransactionRequest {
    /// Raw EIP-2718 transaction bytes, hex-encoded (eth_sendRawTransaction convention).
    pub transaction: String,
}

impl Serializer for SubmitEvmTransactionRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.transaction, writer)?;
        Ok(())
    }
}

impl Deserializer for SubmitEvmTransactionRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction = load!(String, reader)?;
        Ok(Self { transaction })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitEvmTransactionResponse {
    /// Ethereum transaction hash (keccak256 of the raw bytes), hex.
    pub transaction_hash: String,
}

impl Serializer for SubmitEvmTransactionResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.transaction_hash, writer)?;
        Ok(())
    }
}

impl Deserializer for SubmitEvmTransactionResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction_hash = load!(String, reader)?;
        Ok(Self { transaction_hash })
    }
}

/// kaspa-pq EVM Lane v0.4 (§9.2): submit an `EVM_DEPOSIT_LOCK` UTXO outpoint to
/// be claimed as a bridge deposit. The depositor knows their own lock outpoint;
/// the node resolves it in the virtual UTXO set, reads the locked address /
/// amount / claim-tip, builds + validates a `DepositClaim`, and queues it for
/// this node's own template `system_ops` (the claim then executes — crediting
/// the EVM account — in a chain block that accepts the payload).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitEvmDepositClaimRequest {
    /// The `EVM_DEPOSIT_LOCK` transaction id, hex (64 bytes / 128 hex chars).
    pub transaction_id: String,
    /// The output index of the lock within that transaction.
    pub index: u32,
}

impl Serializer for SubmitEvmDepositClaimRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.transaction_id, writer)?;
        store!(u32, &self.index, writer)?;
        Ok(())
    }
}

impl Deserializer for SubmitEvmDepositClaimRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction_id = load!(String, reader)?;
        let index = load!(u32, reader)?;
        Ok(Self { transaction_id, index })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitEvmDepositClaimResponse {
    /// The EVM address (hex, 20 bytes) that will be credited on acceptance.
    pub evm_address: String,
    /// The sompi amount credited (amount − tip to the address; tip to the coinbase).
    pub amount_sompi: u64,
    /// The claim-inclusion tip (sompi) routed to the accepting block's coinbase.
    pub claim_tip_sompi: u64,
}

impl Serializer for SubmitEvmDepositClaimResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.evm_address, writer)?;
        store!(u64, &self.amount_sompi, writer)?;
        store!(u64, &self.claim_tip_sompi, writer)?;
        Ok(())
    }
}

impl Deserializer for SubmitEvmDepositClaimResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let evm_address = load!(String, reader)?;
        let amount_sompi = load!(u64, reader)?;
        let claim_tip_sompi = load!(u64, reader)?;
        Ok(Self { evm_address, amount_sompi, claim_tip_sompi })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidatorStatusRequest {}

impl Serializer for GetValidatorStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetValidatorStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidatorStatusResponse {
    /// Whether the in-process validator service is running (`--enable-validator`).
    pub enabled: bool,
    /// Operating mode: "active" / "standby" / "observer".
    pub mode: String,
    pub has_key: bool,
    /// 64-byte overlay validator id, hex (empty when no key is loaded).
    pub validator_id: String,
    /// P2PKH-ML-DSA funding address, bech32 (empty when no key is loaded).
    pub funding_address: String,
    /// Whether the DNS overlay is configured for this network (epoch available).
    pub overlay_configured: bool,
    pub epoch: u64,
    /// Effective bond status: "none" / "pending" / "active" / "unbonding" / "slashed".
    pub bond_status: String,
    pub is_active_validator: bool,
    pub has_signed_epoch: bool,
    pub last_signed_epoch: u64,
    /// `dns_finality::ValidatorStatus` discriminant.
    pub status: u32,
    /// Human-readable status label.
    pub status_label: String,
}

impl Serializer for GetValidatorStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.enabled, writer)?;
        store!(String, &self.mode, writer)?;
        store!(bool, &self.has_key, writer)?;
        store!(String, &self.validator_id, writer)?;
        store!(String, &self.funding_address, writer)?;
        store!(bool, &self.overlay_configured, writer)?;
        store!(u64, &self.epoch, writer)?;
        store!(String, &self.bond_status, writer)?;
        store!(bool, &self.is_active_validator, writer)?;
        store!(bool, &self.has_signed_epoch, writer)?;
        store!(u64, &self.last_signed_epoch, writer)?;
        store!(u32, &self.status, writer)?;
        store!(String, &self.status_label, writer)?;
        Ok(())
    }
}

impl Deserializer for GetValidatorStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let enabled = load!(bool, reader)?;
        let mode = load!(String, reader)?;
        let has_key = load!(bool, reader)?;
        let validator_id = load!(String, reader)?;
        let funding_address = load!(String, reader)?;
        let overlay_configured = load!(bool, reader)?;
        let epoch = load!(u64, reader)?;
        let bond_status = load!(String, reader)?;
        let is_active_validator = load!(bool, reader)?;
        let has_signed_epoch = load!(bool, reader)?;
        let last_signed_epoch = load!(u64, reader)?;
        let status = load!(u32, reader)?;
        let status_label = load!(String, reader)?;
        Ok(Self {
            enabled,
            mode,
            has_key,
            validator_id,
            funding_address,
            overlay_configured,
            epoch,
            bond_status,
            is_active_validator,
            has_signed_epoch,
            last_signed_epoch,
            status,
            status_label,
        })
    }
}

// MISAKA Compute Token Program (design §9.3): the TOK read surface. Same
// RPC-friendly encodings as getDnsConfirmation — `u128` as decimal strings,
// `Hash64` identities and roots as hex strings — and `available = false`
// (with default fields) when the token program is not configured for the
// network, mirroring that RPC's "not configured" stance.

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenLedgerEntryRequest {
    /// Asset id (0 = TOK, the only asset until Phase B).
    pub asset_id: u64,
    /// Owner id — the 128-hex overlay identity (`BLAKE2b-512(pubkey)`).
    pub owner: String,
}

impl Serializer for GetTokenLedgerEntryRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.asset_id, writer)?;
        store!(String, &self.owner, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenLedgerEntryRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let asset_id = load!(u64, reader)?;
        let owner = load!(String, reader)?;
        Ok(Self { asset_id, owner })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenLedgerEntryResponse {
    /// False when the token program is not configured for this network (or the
    /// owner id did not parse); the other fields are then defaults. An absent
    /// ledger row is NOT an error — it reads as balance 0 / nonce 0 (design §4.2).
    pub available: bool,
    /// Atomic units, decimal string (`u128`).
    pub balance: String,
    /// Last applied nonce; the next payload must carry `nonce + 1`.
    pub nonce: u64,
}

impl Serializer for GetTokenLedgerEntryResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.balance, writer)?;
        store!(u64, &self.nonce, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenLedgerEntryResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let balance = load!(String, reader)?;
        let nonce = load!(u64, reader)?;
        Ok(Self { available, balance, nonce })
    }
}

/// **PALW ConsensusV2: what a block producer must read from chain state** (ADR-0042 Decision 6).
///
/// `bond` is optional: a producer that only wants the class's target and pwu may omit it, and one
/// that names its bond additionally gets the exposure facts and a ready-to-produce verdict.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwProducerFactsRequest {
    /// 128-hex class id (`execution_class_id`, the shape profile id).
    pub class_id: String,
    /// Optional bond outpoint: 128-hex transaction id.
    pub bond_transaction_id: String,
    pub bond_index: u32,
    /// False when `bond_transaction_id` is not to be read at all.
    pub with_bond: bool,
}

impl Serializer for GetPalwProducerFactsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.bond_transaction_id, writer)?;
        store!(u32, &self.bond_index, writer)?;
        store!(bool, &self.with_bond, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwProducerFactsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let class_id = load!(String, reader)?;
        let bond_transaction_id = load!(String, reader)?;
        let bond_index = load!(u32, reader)?;
        let with_bond = load!(bool, reader)?;
        Ok(Self { class_id, bond_transaction_id, bond_index, with_bond })
    }
}

/// The producer facts, **derived** — never the ingredients (ADR-0046: derive, never declare).
///
/// Exposing the ingredients and letting a producer multiply them would give every producer an
/// independent chance to disagree with admission, which is the exact shape of the correspondence
/// defects this codebase has found repeatedly. `u128` quantities travel as decimal strings and
/// `Hash64` identities as 128-hex, the same convention the token surface uses.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwProducerFactsResponse {
    /// False on every network that is not `ConsensusV2`, and on one that is but does not know the
    /// class. Both are honest answers rather than errors: a producer asking a hash-only chain for
    /// a class target is on the wrong network, and should be able to find that out by asking.
    pub available: bool,
    /// The state's chain point these facts were read at — a producer that builds against a
    /// different point built against a different state.
    pub chain_point: String,
    /// The CANDIDATE's DAA score, not the tip's: admission's epoch index comes from the candidate.
    pub daa_score: u64,
    pub class_id: String,
    pub artifact_root: String,
    /// Decimal string (`u128`).
    pub class_target: String,
    /// The one legal `pwu` for an attempt of this class — admission item 6 is equality, not a bound.
    pub pwu: u64,
    pub is_base_class: bool,
    /// The retention obligation a producer takes on by accepting: bind + receipt + challenge + court.
    pub min_trace_retention_daa: u64,
    pub epoch_index: u64,
    pub epoch_budget_blocks: u64,
    pub epoch_produced_blocks: u64,
    /// False when the request named no bond, or the chain does not know it.
    pub bond_known: bool,
    /// Hex of the bond's registered ML-DSA-87 public key — a producer compares it against the key
    /// it holds rather than discovering the mismatch when its block is refused.
    pub bond_registered_pubkey: String,
    pub bond_operator_id: String,
    pub bond_collateral: u64,
    /// Decimal strings (`u128`).
    pub bond_reserved_exposure: String,
    pub bond_exposure_ceiling: String,
    pub bond_claim_exposure: String,
    /// Empty when this bond may produce now; otherwise the reason it may not, which is exactly
    /// what `PalwProducerFactsV2::ready_to_produce` says — one answer, not two.
    pub not_ready_reason: String,
    /// **Is this class seated on the FREE-PROMPT lane** (ADR-0075 `ClassLaneCertified`, the genesis
    /// set ∪ the chain set)? Read off `PalwProducerFactsV2::fp_certified`, which reads the same two
    /// sets `FreePromptCommitted` refuses on (`FreePromptLaneUncertified`).
    ///
    /// Carried because ADR-0077 Decision 3 makes it a thing a gateway must know BEFORE it commits:
    /// a job on a class the chain does not certify is still answered — the answer is the product —
    /// but its commitment is unsubmittable, and the gateway can only say so by name if the chain
    /// told it. Every other fact here was already on the wire; this was the one genuinely missing
    /// read, and without it `identity.json`'s `class_id` was checked by nothing (ADR-0075 §4).
    ///
    /// False whenever `available` is false, because an unknown class is certified for nothing.
    pub fp_certified: bool,
    /// **The free-prompt lane's price, as the network's own bundle declares it** — the two numbers
    /// that turn a claim's `work_leaves` into quanta and therefore into exposure
    /// (`fp_class_quantum_leaves_v1`, `fp_quanta_v3`; ADR-0074 Decision 5).
    ///
    /// A gateway that hardcoded "an eighth" would be declaring a number the chain owns, and would
    /// mis-size its own exposure room on any network that set it otherwise. Zero means this network
    /// prices no free-prompt lane at all — the attempt-only configuration — and a commitment on it
    /// would never enter the state.
    pub fp_quanta_per_canonical_job: u32,
    /// The per-receipt jackpot bound (`fp_quanta_v3`'s cap). Zero on a network with no lane.
    pub fp_max_quanta_per_receipt: u32,
    /// **Is ADR-0082's free-prompt decode ruleset IN FORCE at `daa_score`** — the fence
    /// `Params::palw_fp_decode_rules`, answered by `palw_fp_decode_rules_active_at` at the same
    /// chain point every other fact here was read at.
    ///
    /// It is a node-config fact and not chain state, exactly like the two prices above, and it is
    /// on the wire for the same reason `fp_certified` is: it changes what a commitment must
    /// contain before the commitment is built. Past the fence a job carries `(sampling_seed,
    /// temperature_q)` inside its context hash and decode leaves are what earn; a gateway that
    /// guessed would either omit fields the chain requires or publish fields it will not read, and
    /// in both directions the claim is unreproducible rather than rejected.
    ///
    /// False whenever `available` is false, and false on any pre-version-4 peer — the same
    /// fail-closed reading `fp_certified` takes, and for the same reason: a submitter that held
    /// back on a stale `false` loses nothing it cannot retry.
    pub fp_decode_rules_armed: bool,
    /// **The directory this node's PALW panel serves material from** (ADR-0084 Decision 5),
    /// derived from the app dir and named by no flag — so a submitter on this host learns where
    /// to stage a claim's material and answer envelope from the node itself. Empty on a node
    /// running no panel, and on a version-4 peer. Node-local, like `locked_bond_outpoints`.
    pub palw_retention_dir: String,
    /// **ADR-0077 Decision 16's `PanelDa` fence, at the candidate's DAA** (version 6). A gateway
    /// may file a mode-2 (prompt-withheld) commitment only where this is `true`; `false` — every
    /// shipped preset, and every node that does not report it — refuses the request with the
    /// fence's name rather than spending an inference on a commitment the chain will not extract.
    pub panel_da_armed: bool,
    /// **ADR-0081 Decision 3's prompt-commitment form** (version 6): `true` where the network
    /// hashes prompts as a tiled Merkle root, `false` for the flat digest. The gateway re-binds
    /// the worker's result under it; a node that does not report it reads as flat, which on a
    /// Merkle network refuses the result rather than filing a job the chain calls something else.
    pub prompt_ids_merkle: bool,
    /// **Every outpoint a wallet must not spend**, `txid:index` with a 128-hex transaction id.
    ///
    /// Two sources, deliberately in ONE list so a wallet cannot read half of it (audit3 H3, H12):
    /// PALW bond collateral locked by consensus, and the outpoints this NODE has reserved to fund
    /// the lifecycle objects its own panel carries (`--palw-fee-outpoint` and every rolling
    /// successor). The second is node-local — the chain has no opinion about which of a producer's
    /// outputs its panel intends to spend — and it is exactly what `wallet send` selects, because
    /// it sits at the producer's pay address beside its mining rewards.
    ///
    /// Answered whenever this is a `ConsensusV2` network, INCLUDING for a request that names no
    /// class — a wallet needs this and has no class id to offer. `get_stake_bonds` reads the DNS
    /// overlay store only, so before this a PALW producer's collateral was invisible to the very
    /// input selector that exists to skip it, and it sits at the producer's own pay address by
    /// construction, usually as the largest output there.
    pub locked_bond_outpoints: Vec<String>,
}

impl Serializer for GetPalwProducerFactsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // Version 6: `panel_da_armed` and `prompt_ids_merkle` (ADR-0077 D16 / ADR-0081 D3, the
        // private-prompts design of 2026-09-05). Version 5: `palw_retention_dir` (ADR-0084 Decision 5). Version 4 added
        // `fp_decode_rules_armed` (ADR-0082 Decisions 10/11's fence). Version 3 added
        // `fp_certified` and the free-prompt price (ADR-0077 Decision 3). Version 2 added
        // `locked_bond_outpoints` (audit3 H3). Every version is a strict suffix, so an older
        // reader stops where its version ended and this reader tolerates an older writer by
        // leaving the later fields at their defaults — additive, never re-ordered.
        store!(u16, &6, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.chain_point, writer)?;
        store!(u64, &self.daa_score, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.artifact_root, writer)?;
        store!(String, &self.class_target, writer)?;
        store!(u64, &self.pwu, writer)?;
        store!(bool, &self.is_base_class, writer)?;
        store!(u64, &self.min_trace_retention_daa, writer)?;
        store!(u64, &self.epoch_index, writer)?;
        store!(u64, &self.epoch_budget_blocks, writer)?;
        store!(u64, &self.epoch_produced_blocks, writer)?;
        store!(bool, &self.bond_known, writer)?;
        store!(String, &self.bond_registered_pubkey, writer)?;
        store!(String, &self.bond_operator_id, writer)?;
        store!(u64, &self.bond_collateral, writer)?;
        store!(String, &self.bond_reserved_exposure, writer)?;
        store!(String, &self.bond_exposure_ceiling, writer)?;
        store!(String, &self.bond_claim_exposure, writer)?;
        store!(String, &self.not_ready_reason, writer)?;
        store!(Vec<String>, &self.locked_bond_outpoints, writer)?;
        store!(bool, &self.fp_certified, writer)?;
        store!(u32, &self.fp_quanta_per_canonical_job, writer)?;
        store!(u32, &self.fp_max_quanta_per_receipt, writer)?;
        store!(bool, &self.fp_decode_rules_armed, writer)?;
        store!(String, &self.palw_retention_dir, writer)?;
        store!(bool, &self.panel_da_armed, writer)?;
        store!(bool, &self.prompt_ids_merkle, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwProducerFactsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let chain_point = load!(String, reader)?;
        let daa_score = load!(u64, reader)?;
        let class_id = load!(String, reader)?;
        let artifact_root = load!(String, reader)?;
        let class_target = load!(String, reader)?;
        let pwu = load!(u64, reader)?;
        let is_base_class = load!(bool, reader)?;
        let min_trace_retention_daa = load!(u64, reader)?;
        let epoch_index = load!(u64, reader)?;
        let epoch_budget_blocks = load!(u64, reader)?;
        let epoch_produced_blocks = load!(u64, reader)?;
        let bond_known = load!(bool, reader)?;
        let bond_registered_pubkey = load!(String, reader)?;
        let bond_operator_id = load!(String, reader)?;
        let bond_collateral = load!(u64, reader)?;
        let bond_reserved_exposure = load!(String, reader)?;
        let bond_exposure_ceiling = load!(String, reader)?;
        let bond_claim_exposure = load!(String, reader)?;
        let not_ready_reason = load!(String, reader)?;
        let locked_bond_outpoints = if version >= 2 { load!(Vec<String>, reader)? } else { Vec::new() };
        // A version-2 peer knows nothing about the free-prompt lane, so it is read as uncertified
        // and unpriced rather than as certified-by-omission: a gateway that submits on a `false`
        // it should not have trusted loses a fee, and one that holds back on a stale `false` loses
        // nothing it cannot retry from the outbox. Fail closed.
        let (fp_certified, fp_quanta_per_canonical_job, fp_max_quanta_per_receipt) =
            if version >= 3 { (load!(bool, reader)?, load!(u32, reader)?, load!(u32, reader)?) } else { (false, 0, 0) };
        // Same fail-closed reading one version on: a peer that predates the fence cannot be
        // asserting it is dormant, so "unknown" is read as "not in force" and a submitter that
        // holds back retries. The opposite default would have a gateway building jobs with decode
        // fields against a chain that will not read them.
        let fp_decode_rules_armed = if version >= 4 { load!(bool, reader)? } else { false };
        // Version 5 (ADR-0084 Decision 5): an older writer names no directory, which reads as "this
        // node told us nothing" — a submitter then refuses to stage anywhere it was not told to.
        let palw_retention_dir = if version >= 5 { load!(String, reader)? } else { String::new() };
        // Version 6: both fail closed — an older node cannot be asserting either fence is in
        // force, so a gateway reads "unknown" as "not armed" / "flat" and refuses rather than files.
        let (panel_da_armed, prompt_ids_merkle) =
            if version >= 6 { (load!(bool, reader)?, load!(bool, reader)?) } else { (false, false) };
        Ok(Self {
            available,
            chain_point,
            daa_score,
            class_id,
            artifact_root,
            class_target,
            pwu,
            is_base_class,
            min_trace_retention_daa,
            epoch_index,
            epoch_budget_blocks,
            epoch_produced_blocks,
            bond_known,
            bond_registered_pubkey,
            bond_operator_id,
            bond_collateral,
            bond_reserved_exposure,
            bond_exposure_ceiling,
            bond_claim_exposure,
            not_ready_reason,
            locked_bond_outpoints,
            fp_certified,
            fp_quanta_per_canonical_job,
            fp_max_quanta_per_receipt,
            fp_decode_rules_armed,
            palw_retention_dir,
            panel_da_armed,
            prompt_ids_merkle,
        })
    }
}

// ------------------------------------------------------------------------------------------
// ADR-0078 Decision 5 — the consumer's read path
// ------------------------------------------------------------------------------------------
//
// "Verification belongs to the consumer, and the chain makes it possible." The chain stores one
// `DerivedArtifactV1` per (claim, transformer) and never checks its content: the whole guarantee
// is that anyone holding the answer can recompute `output_root`, `dsl_hash` and `artifact_hash`
// and compare them with what the chain says (invariant X6). That is a promise about ids the
// consumer can FETCH, and until these two calls existed the `derived_artifacts` table had no
// reader outside the transition that wrote it.
//
// **What is NOT here, deliberately: `output_token_ids`.** The claim commits
// `output_root = output_commitment_v2(job_context_hash, ids, family_rendered_hash)` — the ids are
// not on the chain in any form, and ADR-0044 Decision 8's sentence about not publishing prompts
// applies to answers word for word. So a verifier holds the ids from the gateway's own response
// (`misaka.output_token_ids`, beside `job_context_hash` and `family`) and these calls return the
// chain-side facts to check them against. A response that carried the ids would be the chain
// publishing the answer.

/// ADR-0078 Decision 5: which claim's derivations to read. 128-hex `fp_claim_id`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwDerivedArtifactsRequest {
    /// 128-hex free-prompt claim id (`fp_claim_id`, what the gateway returns and what the
    /// derivation's `claim_id` names).
    pub claim_id: String,
}

impl Serializer for GetPalwDerivedArtifactsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwDerivedArtifactsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let claim_id = load!(String, reader)?;
        Ok(Self { claim_id })
    }
}

/// **One row of the chain's derived-artifact table** (ADR-0078 Decision 4), as a reader sees it.
///
/// The key's half and the row's half in one record: `transformer_id` is the key's, and the state
/// row does not repeat it, so a response that returned rows alone would hand a verifier a
/// `dsl_hash` with no way to name the function that produced it.
///
/// `kind_name` is resolved from the kind table shipped in `kaspa-consensus-core`, and it is a
/// convenience for a human reader and nothing more: the chain checks `kind != 0` and interprets
/// no kind (Decision 9 / X8). A `kind` this build has no name for reads as an empty string rather
/// than an error — a newer network may have assigned it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwDerivedArtifact {
    /// 128-hex. The transformer this row is keyed by — a content name, which the chain never
    /// resolves (SA-5 binds the submitter and the consumer, not consensus).
    pub transformer_id: String,
    /// 128-hex `derived_id_v1` — total over every field of the object, including the ones the
    /// state row does not keep, so it is the id a signature was made over.
    pub derived_id: String,
    /// 128-hex.
    pub grammar_id: String,
    pub kind: u32,
    /// The kind table's name for `kind`, or empty when this build has none.
    pub kind_name: String,
    /// 128-hex `H(grammar_id ‖ canonical DSL bytes)`.
    pub dsl_hash: String,
    /// 128-hex `H(artifact bytes)`.
    pub artifact_hash: String,
    pub artifact_bytes: u64,
    /// The DAA score of the block whose transition accepted this derivation.
    pub accepted_daa: u64,
}

impl Serializer for RpcPalwDerivedArtifact {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.transformer_id, writer)?;
        store!(String, &self.derived_id, writer)?;
        store!(String, &self.grammar_id, writer)?;
        store!(u32, &self.kind, writer)?;
        store!(String, &self.kind_name, writer)?;
        store!(String, &self.dsl_hash, writer)?;
        store!(String, &self.artifact_hash, writer)?;
        store!(u64, &self.artifact_bytes, writer)?;
        store!(u64, &self.accepted_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwDerivedArtifact {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transformer_id = load!(String, reader)?;
        let derived_id = load!(String, reader)?;
        let grammar_id = load!(String, reader)?;
        let kind = load!(u32, reader)?;
        let kind_name = load!(String, reader)?;
        let dsl_hash = load!(String, reader)?;
        let artifact_hash = load!(String, reader)?;
        let artifact_bytes = load!(u64, reader)?;
        let accepted_daa = load!(u64, reader)?;
        Ok(Self { transformer_id, derived_id, grammar_id, kind, kind_name, dsl_hash, artifact_hash, artifact_bytes, accepted_daa })
    }
}

/// **ADR-0078 Decision 5: everything the chain has about one claim's derivations.**
///
/// A verifier needs the rows AND the claim they hang off, because a row alone proves nothing: the
/// object's `output_root` had to equal the claim's to be accepted, and the executor's key had to
/// be the claim's bond key. Both are here so the check is one call.
///
/// The three recomputations X6 names are then the consumer's, over bytes the chain does not have:
///
/// ```text
/// output_root   = output_commitment_v2(job_context_hash, output_token_ids, family_rendered_hash)
/// dsl_hash      = H(grammar_id ‖ grammar.canonicalize(answer))
/// artifact_hash = H(transformer.run(canonical DSL))
/// ```
///
/// `output_token_ids` are NOT returned by this call and are not on the chain in any form: the
/// consumer holds them from the gateway's response beside `job_context_hash` and `family`. What
/// this call returns is the chain's side of the comparison.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwDerivedArtifactsResponse {
    /// False when this network is not `ConsensusV2`, or the chain does not hold that claim. An
    /// honest answer rather than an error: a claim on another chain is a claim this node cannot
    /// speak for, and a verifier should be able to find that out by asking.
    pub found: bool,
    /// 128-hex, echoed so a response can be filed without its request.
    pub claim_id: String,
    /// 128-hex — the claim's committed `output_root`, which every accepted row's own
    /// `output_root` equals (Decision 4's cross-check) and which X6 recomputes.
    pub output_root: String,
    /// Hex of the executor bond's registered ML-DSA-87 public key — whose name is on this
    /// provenance. Empty when the bond has since retired: the claim outlives the bond record.
    pub executor_pubkey: String,
    /// The executor's bond outpoint, `txid_hex:index`.
    pub executor_bond: String,
    /// 128-hex class id the claim was executed under.
    pub class_id: String,
    /// The claim's phase, named: `provisional`, `panel_bound`, `receipt_licensed`, `final`, or
    /// `voided`. Decision 4: "a derivation of a claim that later voids is a derivation of a voided
    /// claim, and says so when read" — this field is that sentence.
    pub claim_phase: String,
    /// For `voided`, the reason; empty otherwise.
    pub claim_void_reason: String,
    /// The block that carried the claim's commitment, and its DAA score.
    pub claim_accepted_block: String,
    pub claim_accepted_daa: u64,
    /// The derivations, in transformer-id order (the table's own order). Empty for a claim with
    /// none — which is the common case and not a failure.
    pub artifacts: Vec<RpcPalwDerivedArtifact>,
}

impl Serializer for GetPalwDerivedArtifactsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.output_root, writer)?;
        store!(String, &self.executor_pubkey, writer)?;
        store!(String, &self.executor_bond, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.claim_phase, writer)?;
        store!(String, &self.claim_void_reason, writer)?;
        store!(String, &self.claim_accepted_block, writer)?;
        store!(u64, &self.claim_accepted_daa, writer)?;
        serialize!(Vec<RpcPalwDerivedArtifact>, &self.artifacts, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwDerivedArtifactsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let found = load!(bool, reader)?;
        let claim_id = load!(String, reader)?;
        let output_root = load!(String, reader)?;
        let executor_pubkey = load!(String, reader)?;
        let executor_bond = load!(String, reader)?;
        let class_id = load!(String, reader)?;
        let claim_phase = load!(String, reader)?;
        let claim_void_reason = load!(String, reader)?;
        let claim_accepted_block = load!(String, reader)?;
        let claim_accepted_daa = load!(u64, reader)?;
        let artifacts = deserialize!(Vec<RpcPalwDerivedArtifact>, reader)?;
        Ok(Self {
            found,
            claim_id,
            output_root,
            executor_pubkey,
            executor_bond,
            class_id,
            claim_phase,
            claim_void_reason,
            claim_accepted_block,
            claim_accepted_daa,
            artifacts,
        })
    }
}

/// ADR-0077 R0: which free-prompt claim to read. 128-hex `fp_claim_id`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwFreePromptClaimRequest {
    pub claim_id: String,
}

impl Serializer for GetPalwFreePromptClaimRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwFreePromptClaimRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let claim_id = load!(String, reader)?;
        Ok(Self { claim_id })
    }
}

/// **The claim a derivation names, as the chain holds it** — ADR-0077 R0's "one inference, one
/// claim", read back.
///
/// The committed roots are here because they are what a verifier compares against and what a
/// disputer opens against: `output_root` for ADR-0078 X6, `trace_root` and `execution_root` for
/// the court's own bindings. The ids the answer consists of are NOT here and are not on the chain
/// (see [`GetPalwDerivedArtifactsResponse`]).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwFreePromptClaimResponse {
    /// False off `ConsensusV2` and for a claim this chain does not hold.
    pub found: bool,
    pub claim_id: String,
    /// True when the claim is a free-prompt claim (ADR-0044); false for a block's own attempt
    /// claim, which has no derivable answer.
    pub is_free_prompt: bool,
    pub class_id: String,
    /// Hex of the executor bond's registered ML-DSA-87 public key; empty once the bond retires.
    pub executor_pubkey: String,
    /// `txid_hex:index`.
    pub executor_bond: String,
    /// 128-hex. What ADR-0078 X6 recomputes from (ids, job context hash, family).
    pub output_root: String,
    /// 128-hex. The committed step-trace root the court adjudicates against.
    pub trace_root: String,
    /// 128-hex. The committed execution root a refutation's binding must equal.
    pub execution_root: String,
    /// The capture's leaf count this claim was priced from (ADR-0074 Decision 5).
    pub work_leaves: u64,
    /// The work identity a free-prompt claim holds while it lives; empty on an attempt claim.
    pub work_id: String,
    /// Certified quanta, and how many of them this chain has already spent into receipt blocks.
    pub quanta: u32,
    pub quanta_spent: u32,
    /// `provisional` / `panel_bound` / `receipt_licensed` / `final` / `voided`.
    pub phase: String,
    /// For `voided`, the reason; empty otherwise.
    pub void_reason: String,
    /// The DAA the phase was entered at (bound / licensed / final / voided); 0 while provisional.
    pub phase_daa: u64,
    pub accepted_block: String,
    pub accepted_daa: u64,
    /// The DAA through which the producer owes openings and chunks — and, for a claim whose DSL
    /// was elected into the ADR-0078 Decision 6 obligation, the window in which it is served.
    pub trace_retention_daa: u64,
    /// How many derivations the chain holds for this claim (at most
    /// `PALW_DERIVED_MAX_PER_CLAIM`); the rows themselves are `GetPalwDerivedArtifacts`.
    pub derived_count: u32,
}

impl Serializer for GetPalwFreePromptClaimResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(bool, &self.is_free_prompt, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.executor_pubkey, writer)?;
        store!(String, &self.executor_bond, writer)?;
        store!(String, &self.output_root, writer)?;
        store!(String, &self.trace_root, writer)?;
        store!(String, &self.execution_root, writer)?;
        store!(u64, &self.work_leaves, writer)?;
        store!(String, &self.work_id, writer)?;
        store!(u32, &self.quanta, writer)?;
        store!(u32, &self.quanta_spent, writer)?;
        store!(String, &self.phase, writer)?;
        store!(String, &self.void_reason, writer)?;
        store!(u64, &self.phase_daa, writer)?;
        store!(String, &self.accepted_block, writer)?;
        store!(u64, &self.accepted_daa, writer)?;
        store!(u64, &self.trace_retention_daa, writer)?;
        store!(u32, &self.derived_count, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwFreePromptClaimResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let found = load!(bool, reader)?;
        let claim_id = load!(String, reader)?;
        let is_free_prompt = load!(bool, reader)?;
        let class_id = load!(String, reader)?;
        let executor_pubkey = load!(String, reader)?;
        let executor_bond = load!(String, reader)?;
        let output_root = load!(String, reader)?;
        let trace_root = load!(String, reader)?;
        let execution_root = load!(String, reader)?;
        let work_leaves = load!(u64, reader)?;
        let work_id = load!(String, reader)?;
        let quanta = load!(u32, reader)?;
        let quanta_spent = load!(u32, reader)?;
        let phase = load!(String, reader)?;
        let void_reason = load!(String, reader)?;
        let phase_daa = load!(u64, reader)?;
        let accepted_block = load!(String, reader)?;
        let accepted_daa = load!(u64, reader)?;
        let trace_retention_daa = load!(u64, reader)?;
        let derived_count = load!(u32, reader)?;
        Ok(Self {
            found,
            claim_id,
            is_free_prompt,
            class_id,
            executor_pubkey,
            executor_bond,
            output_root,
            trace_root,
            execution_root,
            work_leaves,
            work_id,
            quanta,
            quanta_spent,
            phase,
            void_reason,
            phase_daa,
            accepted_block,
            accepted_daa,
            trace_retention_daa,
            derived_count,
        })
    }
}

// -----------------------------------------------------------------------------------------------
// ADR-0080 design A — a declared court close, mid-assembly
// -----------------------------------------------------------------------------------------------
//
// A close too wide for one carrier rides as a signed `CourtCloseDeclared` and its chunks, in a
// table keyed `(session_id, side)`. The mover is under a court deadline and its carriers can be
// orphaned, so it has to be able to ask what the CHAIN thinks it has received. Before this call the
// only answer was `misaka palw court-close`'s own journal on the mover's disk — and a journal that
// believes itself skips a part whose carrier was reorged out and completes a group that can never
// assemble. It also could not answer the two preflights that matter: whether a declaration for this
// `(session, side)` already exists (one per side, ever) and how much time is left to assemble.

/// ADR-0080 design A: which declared close to read — a session id and one of its two sides.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPendingChunkGroupRequest {
    /// 128-hex court session id.
    pub session_id: String,
    /// `challenger` or `executor`. Not defaulted: a group is keyed by the side, and answering for
    /// the wrong one is answering about the other party's carriage.
    pub side: String,
}

impl Serializer for GetPalwPendingChunkGroupRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.session_id, writer)?;
        store!(String, &self.side, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPendingChunkGroupRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let session_id = load!(String, reader)?;
        let side = load!(String, reader)?;
        Ok(Self { session_id, side })
    }
}

/// **One side's declared close, as the chain holds it** (ADR-0080 design A §2.2).
///
/// `present` is the row's own `u64` bitmap and is carried as a bitmap rather than as a count,
/// because chunks arrive in ANY order: "four of seven have landed" does not say which three to
/// send, and a resume that guessed would re-pay for a carrier the chain already has and leave a
/// hole it does not.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPendingChunkGroupResponse {
    /// False off `ConsensusV2` and for a `(session, side)` that has declared no close. It is also
    /// false once the group is GONE — adjudicated, convicted or swept with its session — so a
    /// filer that saw a group and now does not has an answer rather than a silence.
    pub found: bool,
    pub session_id: String,
    pub side: String,
    /// The chunks the declaration pinned. Every consensus rule on a split close counts these.
    pub count: u32,
    /// One bit per index, exactly as `PalwCourtCloseGroupV2::present` holds it.
    pub present: u64,
    pub parts_present: u32,
    pub complete: bool,
    pub declared_daa: u64,
    /// `declared_daa + PALW_COURT_CLOSE_INCLUSION_MARGIN × count`, and never past the session's
    /// own backstop — the declaration may not extend it.
    pub assembly_deadline_daa: u64,
    /// 128-hex keyed hash of the concatenation. A filer compares its own cut against this before
    /// spending a carrier on a group it would convict itself by completing.
    pub close_digest: String,
    /// `executor_guilty` / `challenger_defeated`.
    pub verdict: String,
    /// `txid:index` — the bond that declared, which is also the outpoint backing the deposit.
    pub declarer_bond: String,
    /// The assembly reserve at risk, forfeited if the group never assembles.
    pub deposit: u64,
}

impl Serializer for GetPalwPendingChunkGroupResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.session_id, writer)?;
        store!(String, &self.side, writer)?;
        store!(u32, &self.count, writer)?;
        store!(u64, &self.present, writer)?;
        store!(u32, &self.parts_present, writer)?;
        store!(bool, &self.complete, writer)?;
        store!(u64, &self.declared_daa, writer)?;
        store!(u64, &self.assembly_deadline_daa, writer)?;
        store!(String, &self.close_digest, writer)?;
        store!(String, &self.verdict, writer)?;
        store!(String, &self.declarer_bond, writer)?;
        store!(u64, &self.deposit, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPendingChunkGroupResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let found = load!(bool, reader)?;
        let session_id = load!(String, reader)?;
        let side = load!(String, reader)?;
        let count = load!(u32, reader)?;
        let present = load!(u64, reader)?;
        let parts_present = load!(u32, reader)?;
        let complete = load!(bool, reader)?;
        let declared_daa = load!(u64, reader)?;
        let assembly_deadline_daa = load!(u64, reader)?;
        let close_digest = load!(String, reader)?;
        let verdict = load!(String, reader)?;
        let declarer_bond = load!(String, reader)?;
        let deposit = load!(u64, reader)?;
        Ok(Self {
            found,
            session_id,
            side,
            count,
            present,
            parts_present,
            complete,
            declared_daa,
            assembly_deadline_daa,
            close_digest,
            verdict,
            declarer_bond,
            deposit,
        })
    }
}

/// ADR-0087 Decision 8: the market of one line (ADR-0088 Decision 9: keyed by line; a class's
/// founding line has the class id as its line id).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelMarketRequest {
    /// 128-hex line id — a class id names the class's founding line.
    pub line_id: String,
}

impl Serializer for GetPalwModelMarketRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelMarketRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        Ok(Self { line_id })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelMarketResponse {
    /// False when the line (or its class) does not exist on this chain (or the chain is not
    /// ConsensusV2).
    pub found: bool,
    pub line_id: String,
    /// False until the first buy folded a row; the numbers below are then the unopened market's.
    pub opened: bool,
    pub opened_daa: u64,
    pub msk_reserve: u64,
    pub position_units: u64,
    pub sold_units: u64,
    pub burned_sompi: u64,
    /// The owner's total of the 1 % leg (ADR-0088 Decision 8: the field keeps its name and now
    /// means the owner's).
    pub registrant_paid_sompi: u64,
    pub closed_to_buys: bool,
    /// `(reserve + V) / positions`, in sompi per position, rounded down.
    pub price_sompi_per_position: u64,
    pub supply_units: u64,
    pub virtual_sompi: u64,
    /// The class's status as the registry names it (`Active`, `Frozen {..}`, `Registered {..}`).
    pub class_status: String,
    /// ADR-0088 Decision 8: the part of the leg paid to an adopted contributor.
    pub contributor_paid_sompi: u64,
    /// ADR-0090: the seed the market opened with — locked for good (0 while unseeded).
    pub seed_sompi: u64,
    /// ADR-0090: who seeded it, as a payout payload (128 hex; empty while unseeded).
    pub seeded_by: String,
    /// ADR-0090: the least seed this network takes, in sompi.
    pub seed_min_sompi: u64,
}

impl Serializer for GetPalwModelMarketResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &3, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(bool, &self.opened, writer)?;
        store!(u64, &self.opened_daa, writer)?;
        store!(u64, &self.msk_reserve, writer)?;
        store!(u64, &self.position_units, writer)?;
        store!(u64, &self.sold_units, writer)?;
        store!(u64, &self.burned_sompi, writer)?;
        store!(u64, &self.registrant_paid_sompi, writer)?;
        store!(bool, &self.closed_to_buys, writer)?;
        store!(u64, &self.price_sompi_per_position, writer)?;
        store!(u64, &self.supply_units, writer)?;
        store!(u64, &self.virtual_sompi, writer)?;
        store!(String, &self.class_status, writer)?;
        store!(u64, &self.contributor_paid_sompi, writer)?;
        // Version 3 (ADR-0090): the seed, its payer and the network's least seed.
        store!(u64, &self.seed_sompi, writer)?;
        store!(String, &self.seeded_by, writer)?;
        store!(u64, &self.seed_min_sompi, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelMarketResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let found = load!(bool, reader)?;
        let line_id = load!(String, reader)?;
        let opened = load!(bool, reader)?;
        let opened_daa = load!(u64, reader)?;
        let msk_reserve = load!(u64, reader)?;
        let position_units = load!(u64, reader)?;
        let sold_units = load!(u64, reader)?;
        let burned_sompi = load!(u64, reader)?;
        let registrant_paid_sompi = load!(u64, reader)?;
        let closed_to_buys = load!(bool, reader)?;
        let price_sompi_per_position = load!(u64, reader)?;
        let supply_units = load!(u64, reader)?;
        let virtual_sompi = load!(u64, reader)?;
        let class_status = load!(String, reader)?;
        // Version 2 (ADR-0088) appended the contributor's leg; a version-1 peer sent none.
        let contributor_paid_sompi = if version >= 2 { load!(u64, reader)? } else { 0 };
        // Version 3 (ADR-0090): the seed, its payer and the network's least seed.
        let (seed_sompi, seeded_by, seed_min_sompi) =
            if version >= 3 { (load!(u64, reader)?, load!(String, reader)?, load!(u64, reader)?) } else { (0, String::new(), 0) };
        Ok(Self {
            found,
            line_id,
            opened,
            opened_daa,
            msk_reserve,
            position_units,
            sold_units,
            burned_sompi,
            registrant_paid_sompi,
            closed_to_buys,
            price_sompi_per_position,
            supply_units,
            virtual_sompi,
            class_status,
            contributor_paid_sompi,
            seed_sompi,
            seeded_by,
            seed_min_sompi,
        })
    }
}

/// ADR-0087 Decision 8: a holder's positions.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelPositionsRequest {
    /// 128-hex holder — the payout payload (the BLAKE2b-512 of the ML-DSA-87 public key).
    pub holder: String,
}

impl Serializer for GetPalwModelPositionsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.holder, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelPositionsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let holder = load!(String, reader)?;
        Ok(Self { holder })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelPosition {
    /// 128-hex line id (ADR-0088 Decision 9).
    pub line_id: String,
    pub units: u64,
}

impl Serializer for RpcPalwModelPosition {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(u64, &self.units, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelPosition {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        let units = load!(u64, reader)?;
        Ok(Self { line_id, units })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelPositionsResponse {
    pub holder: String,
    pub positions: Vec<RpcPalwModelPosition>,
}

impl Serializer for GetPalwModelPositionsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.holder, writer)?;
        serialize!(Vec<RpcPalwModelPosition>, &self.positions, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelPositionsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let holder = load!(String, reader)?;
        let positions = deserialize!(Vec<RpcPalwModelPosition>, reader)?;
        Ok(Self { holder, positions })
    }
}

// ---- ADR-0088 Decision 12: the model registry -------------------------------------------------

/// ADR-0088 Decision 1: one line's row as the tip holds it. A founding line nothing touched is
/// synthesised from its class (`has_row: false`). `developer` / `maintainer` are the row's own
/// fields — `None` means "the owner" (Decision 6). The payout payloads are the bonds' as the
/// registry holds them at the same tip, `None` when the role names no bond or a bond the
/// registry no longer has.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelLine {
    /// 128-hex line id; the founding line's is its class id.
    pub line_id: String,
    pub class_id: String,
    pub has_row: bool,
    pub owner: Option<RpcTransactionOutpoint>,
    pub owner_payout_payload: Option<String>,
    pub developer: Option<RpcTransactionOutpoint>,
    pub developer_payout_payload: Option<String>,
    pub maintainer: Option<RpcTransactionOutpoint>,
    pub maintainer_payout_payload: Option<String>,
    /// The name as UTF-8 (lossy); `name_hex` is the exact bytes the chain holds.
    pub name: String,
    pub name_hex: String,
    pub founded_daa: u64,
    /// The current version's number.
    pub current: u32,
    /// Versions published as previews and not yet promoted or withdrawn.
    pub previews: Vec<u32>,
    /// The commit count: versions are dense and monotone, and the next one is this plus one.
    pub versions_published: u32,
    pub contributor_permille_of_leg: u32,
    /// `Active` or `Retired`.
    pub status: String,
    pub retired_daa: Option<u64>,
}

impl Serializer for RpcPalwModelLine {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(bool, &self.has_row, writer)?;
        serialize!(Option<RpcTransactionOutpoint>, &self.owner, writer)?;
        store!(Option<String>, &self.owner_payout_payload, writer)?;
        serialize!(Option<RpcTransactionOutpoint>, &self.developer, writer)?;
        store!(Option<String>, &self.developer_payout_payload, writer)?;
        serialize!(Option<RpcTransactionOutpoint>, &self.maintainer, writer)?;
        store!(Option<String>, &self.maintainer_payout_payload, writer)?;
        store!(String, &self.name, writer)?;
        store!(String, &self.name_hex, writer)?;
        store!(u64, &self.founded_daa, writer)?;
        store!(u32, &self.current, writer)?;
        store!(Vec<u32>, &self.previews, writer)?;
        store!(u32, &self.versions_published, writer)?;
        store!(u32, &self.contributor_permille_of_leg, writer)?;
        store!(String, &self.status, writer)?;
        store!(Option<u64>, &self.retired_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelLine {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        let class_id = load!(String, reader)?;
        let has_row = load!(bool, reader)?;
        let owner = deserialize!(Option<RpcTransactionOutpoint>, reader)?;
        let owner_payout_payload = load!(Option<String>, reader)?;
        let developer = deserialize!(Option<RpcTransactionOutpoint>, reader)?;
        let developer_payout_payload = load!(Option<String>, reader)?;
        let maintainer = deserialize!(Option<RpcTransactionOutpoint>, reader)?;
        let maintainer_payout_payload = load!(Option<String>, reader)?;
        let name = load!(String, reader)?;
        let name_hex = load!(String, reader)?;
        let founded_daa = load!(u64, reader)?;
        let current = load!(u32, reader)?;
        let previews = load!(Vec<u32>, reader)?;
        let versions_published = load!(u32, reader)?;
        let contributor_permille_of_leg = load!(u32, reader)?;
        let status = load!(String, reader)?;
        let retired_daa = load!(Option<u64>, reader)?;
        Ok(Self {
            line_id,
            class_id,
            has_row,
            owner,
            owner_payout_payload,
            developer,
            developer_payout_payload,
            maintainer,
            maintainer_payout_payload,
            name,
            name_hex,
            founded_daa,
            current,
            previews,
            versions_published,
            contributor_permille_of_leg,
            status,
            retired_daa,
        })
    }
}

/// ADR-0088 Decision 12: `getPalwModelLine(line_id)`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelLineRequest {
    /// 128-hex line id — a class id names the class's founding line.
    pub line_id: String,
}

impl Serializer for GetPalwModelLineRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelLineRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        Ok(Self { line_id })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelLineResponse {
    /// False when neither a line row nor a class of that id exists (or the chain is not
    /// ConsensusV2); `line` is then `None`.
    pub exists: bool,
    pub line_id: String,
    pub line: Option<RpcPalwModelLine>,
    /// The current version's root, when the node holds that version.
    pub current_root: Option<String>,
    /// Decision 3: the roots in force for the line's CLASS at `tip_daa` — every line's.
    pub roots_in_force: Vec<String>,
    pub tip_daa: u64,
}

impl Serializer for GetPalwModelLineResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.exists, writer)?;
        store!(String, &self.line_id, writer)?;
        serialize!(Option<RpcPalwModelLine>, &self.line, writer)?;
        store!(Option<String>, &self.current_root, writer)?;
        store!(Vec<String>, &self.roots_in_force, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelLineResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let exists = load!(bool, reader)?;
        let line_id = load!(String, reader)?;
        let line = deserialize!(Option<RpcPalwModelLine>, reader)?;
        let current_root = load!(Option<String>, reader)?;
        let roots_in_force = load!(Vec<String>, reader)?;
        let tip_daa = load!(u64, reader)?;
        Ok(Self { exists, line_id, line, current_root, roots_in_force, tip_daa })
    }
}

/// ADR-0088 Decision 2: one version's row. The four hashes are DECLARATIONS the chain recorded
/// and never read (Decision 2); the usage is what the fold counted (Decision 4).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelVersion {
    pub line_id: String,
    pub version: u32,
    pub root: String,
    pub parent: Option<u32>,
    /// The proposal this version adopted, if any.
    pub adopted_from: Option<String>,
    pub runtime_hash: Option<String>,
    pub dataset_commitment: Option<String>,
    pub training_config_hash: Option<String>,
    pub notes_hash: Option<String>,
    pub published_daa: u64,
    pub published_by: Option<RpcTransactionOutpoint>,
    /// `Current`, `Preview`, `Superseded` or `Withdrawn`.
    pub status: String,
    /// A superseded version's grace end: its root is in force while the DAA is below it.
    pub until_daa: Option<u64>,
    /// Whether the root was in force at the tip DAA the answer was read at.
    pub in_force: bool,
    pub attempt_claims: u64,
    pub fp_claims: u64,
    /// A decimal string: the fold counts leaves in a u128.
    pub work_leaves: String,
    pub first_used_daa: Option<u64>,
    pub last_used_daa: Option<u64>,
}

impl Serializer for RpcPalwModelVersion {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(u32, &self.version, writer)?;
        store!(String, &self.root, writer)?;
        store!(Option<u32>, &self.parent, writer)?;
        store!(Option<String>, &self.adopted_from, writer)?;
        store!(Option<String>, &self.runtime_hash, writer)?;
        store!(Option<String>, &self.dataset_commitment, writer)?;
        store!(Option<String>, &self.training_config_hash, writer)?;
        store!(Option<String>, &self.notes_hash, writer)?;
        store!(u64, &self.published_daa, writer)?;
        serialize!(Option<RpcTransactionOutpoint>, &self.published_by, writer)?;
        store!(String, &self.status, writer)?;
        store!(Option<u64>, &self.until_daa, writer)?;
        store!(bool, &self.in_force, writer)?;
        store!(u64, &self.attempt_claims, writer)?;
        store!(u64, &self.fp_claims, writer)?;
        store!(String, &self.work_leaves, writer)?;
        store!(Option<u64>, &self.first_used_daa, writer)?;
        store!(Option<u64>, &self.last_used_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelVersion {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        let version = load!(u32, reader)?;
        let root = load!(String, reader)?;
        let parent = load!(Option<u32>, reader)?;
        let adopted_from = load!(Option<String>, reader)?;
        let runtime_hash = load!(Option<String>, reader)?;
        let dataset_commitment = load!(Option<String>, reader)?;
        let training_config_hash = load!(Option<String>, reader)?;
        let notes_hash = load!(Option<String>, reader)?;
        let published_daa = load!(u64, reader)?;
        let published_by = deserialize!(Option<RpcTransactionOutpoint>, reader)?;
        let status = load!(String, reader)?;
        let until_daa = load!(Option<u64>, reader)?;
        let in_force = load!(bool, reader)?;
        let attempt_claims = load!(u64, reader)?;
        let fp_claims = load!(u64, reader)?;
        let work_leaves = load!(String, reader)?;
        let first_used_daa = load!(Option<u64>, reader)?;
        let last_used_daa = load!(Option<u64>, reader)?;
        Ok(Self {
            line_id,
            version,
            root,
            parent,
            adopted_from,
            runtime_hash,
            dataset_commitment,
            training_config_hash,
            notes_hash,
            published_daa,
            published_by,
            status,
            until_daa,
            in_force,
            attempt_claims,
            fp_claims,
            work_leaves,
            first_used_daa,
            last_used_daa,
        })
    }
}

/// ADR-0088 Decision 5: one evaluation — a declaration, saying who declared it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelEvaluation {
    pub evaluator_id: String,
    pub score_permille: u32,
    pub report_hash: String,
    pub posted_daa: u64,
    /// The bond that posted it.
    pub by: RpcTransactionOutpoint,
    /// Posted by the line's developer or maintainer — the line's own word.
    pub is_lines_own: bool,
}

impl Serializer for RpcPalwModelEvaluation {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.evaluator_id, writer)?;
        store!(u32, &self.score_permille, writer)?;
        store!(String, &self.report_hash, writer)?;
        store!(u64, &self.posted_daa, writer)?;
        serialize!(RpcTransactionOutpoint, &self.by, writer)?;
        store!(bool, &self.is_lines_own, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelEvaluation {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let evaluator_id = load!(String, reader)?;
        let score_permille = load!(u32, reader)?;
        let report_hash = load!(String, reader)?;
        let posted_daa = load!(u64, reader)?;
        let by = deserialize!(RpcTransactionOutpoint, reader)?;
        let is_lines_own = load!(bool, reader)?;
        Ok(Self { evaluator_id, score_permille, report_hash, posted_daa, by, is_lines_own })
    }
}

/// ADR-0088 Decision 12: `getPalwModelVersion(line_id, version)`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelVersionRequest {
    pub line_id: String,
    pub version: u32,
}

impl Serializer for GetPalwModelVersionRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(u32, &self.version, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelVersionRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        let version = load!(u32, reader)?;
        Ok(Self { line_id, version })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelVersionResponse {
    /// False when the node holds no such row — never published, or evicted past the history
    /// window (the explorer keeps the whole history); `version` is then `None`.
    pub exists: bool,
    pub line_id: String,
    pub version_number: u32,
    pub version: Option<RpcPalwModelVersion>,
    pub evaluations: Vec<RpcPalwModelEvaluation>,
    pub tip_daa: u64,
}

impl Serializer for GetPalwModelVersionResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.exists, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(u32, &self.version_number, writer)?;
        serialize!(Option<RpcPalwModelVersion>, &self.version, writer)?;
        serialize!(Vec<RpcPalwModelEvaluation>, &self.evaluations, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelVersionResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let exists = load!(bool, reader)?;
        let line_id = load!(String, reader)?;
        let version_number = load!(u32, reader)?;
        let version = deserialize!(Option<RpcPalwModelVersion>, reader)?;
        let evaluations = deserialize!(Vec<RpcPalwModelEvaluation>, reader)?;
        let tip_daa = load!(u64, reader)?;
        Ok(Self { exists, line_id, version_number, version, evaluations, tip_daa })
    }
}

/// ADR-0088 Decision 12: `getPalwModelLines(class_id)`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelLinesRequest {
    /// 128-hex class id.
    pub class_id: String,
}

impl Serializer for GetPalwModelLinesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelLinesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let class_id = load!(String, reader)?;
        Ok(Self { class_id })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelLinesResponse {
    /// False when the class is not registered (or the chain is not ConsensusV2).
    pub exists: bool,
    pub class_id: String,
    /// The founding line first (synthesised when it has no row), then the others in id order.
    pub lines: Vec<RpcPalwModelLine>,
}

impl Serializer for GetPalwModelLinesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.exists, writer)?;
        store!(String, &self.class_id, writer)?;
        serialize!(Vec<RpcPalwModelLine>, &self.lines, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelLinesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let exists = load!(bool, reader)?;
        let class_id = load!(String, reader)?;
        let lines = deserialize!(Vec<RpcPalwModelLine>, reader)?;
        Ok(Self { exists, class_id, lines })
    }
}

/// ADR-0088 Decision 7: one proposal — a root and a note from any bond, adopted or not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwModelProposal {
    pub proposal_id: String,
    pub line_id: String,
    pub root: String,
    pub note_hash: String,
    pub by: RpcTransactionOutpoint,
    pub posted_daa: u64,
    /// The version that adopted it, once one did.
    pub adopted_in: Option<u32>,
}

impl Serializer for RpcPalwModelProposal {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.proposal_id, writer)?;
        store!(String, &self.line_id, writer)?;
        store!(String, &self.root, writer)?;
        store!(String, &self.note_hash, writer)?;
        serialize!(RpcTransactionOutpoint, &self.by, writer)?;
        store!(u64, &self.posted_daa, writer)?;
        store!(Option<u32>, &self.adopted_in, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwModelProposal {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let proposal_id = load!(String, reader)?;
        let line_id = load!(String, reader)?;
        let root = load!(String, reader)?;
        let note_hash = load!(String, reader)?;
        let by = deserialize!(RpcTransactionOutpoint, reader)?;
        let posted_daa = load!(u64, reader)?;
        let adopted_in = load!(Option<u32>, reader)?;
        Ok(Self { proposal_id, line_id, root, note_hash, by, posted_daa, adopted_in })
    }
}

/// ADR-0088 Decision 12: `getPalwModelProposals(line_id)`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelProposalsRequest {
    pub line_id: String,
}

impl Serializer for GetPalwModelProposalsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.line_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelProposalsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let line_id = load!(String, reader)?;
        Ok(Self { line_id })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwModelProposalsResponse {
    /// False when the line does not exist (or the chain is not ConsensusV2).
    pub exists: bool,
    pub line_id: String,
    pub proposals: Vec<RpcPalwModelProposal>,
}

impl Serializer for GetPalwModelProposalsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.exists, writer)?;
        store!(String, &self.line_id, writer)?;
        serialize!(Vec<RpcPalwModelProposal>, &self.proposals, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwModelProposalsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let exists = load!(bool, reader)?;
        let line_id = load!(String, reader)?;
        let proposals = deserialize!(Vec<RpcPalwModelProposal>, reader)?;
        Ok(Self { exists, line_id, proposals })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenSupplyRequest {
    pub asset_id: u64,
}

impl Serializer for GetTokenSupplyRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.asset_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenSupplyRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let asset_id = load!(u64, reader)?;
        Ok(Self { asset_id })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenSupplyResponse {
    pub available: bool,
    /// Decimal strings (`u128` atomic units).
    pub minted: String,
    pub burned: String,
    pub circulating: String,
}

impl Serializer for GetTokenSupplyResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.minted, writer)?;
        store!(String, &self.burned, writer)?;
        store!(String, &self.circulating, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenSupplyResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let minted = load!(String, reader)?;
        let burned = load!(String, reader)?;
        let circulating = load!(String, reader)?;
        Ok(Self { available, minted, burned, circulating })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenEmissionInfoRequest {
    /// The epoch to read. Ignored when `latest` is true.
    #[serde(default)]
    pub epoch: u64,
    /// When true (the sugar default), read the most recently settled epoch.
    #[serde(default)]
    pub latest: bool,
}

impl Serializer for GetTokenEmissionInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.epoch, writer)?;
        store!(bool, &self.latest, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenEmissionInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let epoch = load!(u64, reader)?;
        let latest = load!(bool, reader)?;
        Ok(Self { epoch, latest })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTokenEmissionInfoResponse {
    pub available: bool,
    pub epoch: u64,
    /// False ⇒ the epoch has no settlement row yet (numeric fields are zero).
    pub settled: bool,
    /// Decimal strings (`u128`): R(E), X(E), Σ reward_i.
    pub budget: String,
    pub network_compute: String,
    pub paid_total: String,
    /// Audit-emission v0.2: the counted-verdict share of `paid_total` (decimal `u128`).
    #[serde(default)]
    pub audit_paid: String,
    pub reward_count: u32,
    /// Hex keyed-BLAKE2b-256 digest of the whole settlement (cross-node comparable).
    pub settlement_root: String,
    /// The live cursors — the ops gauges (design §9.2): next epoch settlement
    /// will consider, and the next selected-chain index the ledger fold processes.
    pub next_settlement_epoch: u64,
    pub fold_cursor: u64,
}

impl Serializer for GetTokenEmissionInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.epoch, writer)?;
        store!(bool, &self.settled, writer)?;
        store!(String, &self.budget, writer)?;
        store!(String, &self.network_compute, writer)?;
        store!(String, &self.paid_total, writer)?;
        store!(String, &self.audit_paid, writer)?;
        store!(u32, &self.reward_count, writer)?;
        store!(String, &self.settlement_root, writer)?;
        store!(u64, &self.next_settlement_epoch, writer)?;
        store!(u64, &self.fold_cursor, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenEmissionInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let epoch = load!(u64, reader)?;
        let settled = load!(bool, reader)?;
        let budget = load!(String, reader)?;
        let network_compute = load!(String, reader)?;
        let paid_total = load!(String, reader)?;
        let audit_paid = if version >= 2 { load!(String, reader)? } else { String::new() };
        let reward_count = load!(u32, reader)?;
        let settlement_root = load!(String, reader)?;
        let next_settlement_epoch = load!(u64, reader)?;
        let fold_cursor = load!(u64, reader)?;
        Ok(Self {
            available,
            epoch,
            settled,
            budget,
            network_compute,
            paid_total,
            audit_paid,
            reward_count,
            settlement_root,
            next_settlement_epoch,
            fold_cursor,
        })
    }
}

// kaspa-pq Phase 12 (ADR-0011): getValidatorAttestationTarget. Given a stake-bond
// outpoint ("txid_hex:index"), returns the exact ready-to-sign attestation message
// (and its bound fields) the validator must ML-DSA-87-sign for the current sink — so
// the `kaspa-pq-validator` sidecar can fetch the signing target over local wRPC.
// `available` is false when the overlay is not configured or no target can be assembled.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidatorAttestationTargetRequest {
    /// Stake-bond outpoint, "txid_hex:index" (txid is a 64-byte Hash64 = 128 hex chars).
    pub bond_outpoint: String,
}

impl Serializer for GetValidatorAttestationTargetRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond_outpoint, writer)?;
        Ok(())
    }
}

impl Deserializer for GetValidatorAttestationTargetRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let bond_outpoint = load!(String, reader)?;
        Ok(Self { bond_outpoint })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidatorAttestationTargetResponse {
    /// False when the overlay is not configured or no target could be assembled; the
    /// remaining fields are then defaults. (A malformed outpoint is a request error.)
    pub available: bool,
    pub epoch: u64,
    /// Selected-chain anchor the attestation approves (Hash64, hex).
    pub target_hash: String,
    pub target_daa_score: u64,
    /// Commitment over the active validator set (Hash64, hex).
    pub validator_set_commitment: String,
    /// The ready-to-sign 32-byte attestation message digest (hex). The sidecar signs
    /// this with its ML-DSA-87 validator key under the attestation context.
    pub message: String,
}

impl Serializer for GetValidatorAttestationTargetResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u64, &self.epoch, writer)?;
        store!(String, &self.target_hash, writer)?;
        store!(u64, &self.target_daa_score, writer)?;
        store!(String, &self.validator_set_commitment, writer)?;
        store!(String, &self.message, writer)?;
        Ok(())
    }
}

impl Deserializer for GetValidatorAttestationTargetResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let epoch = load!(u64, reader)?;
        let target_hash = load!(String, reader)?;
        let target_daa_score = load!(u64, reader)?;
        let validator_set_commitment = load!(String, reader)?;
        let message = load!(String, reader)?;
        Ok(Self { available, epoch, target_hash, target_daa_score, validator_set_commitment, message })
    }
}

// kaspa-pq DNS v3 (batch): getValidatorAttestationTargets. Returns every READY, creditable
// attestation target for a bond in `[from_epoch, latest_ready]` (ascending, capped at `limit`),
// so an external `kaspa-pq-validator` that fell behind can sign every missed epoch in one poll
// instead of one epoch per poll (which lets a briefly-slow validator lag the epoch cadence).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidatorAttestationTargetsRequest {
    /// Stake-bond outpoint, "txid_hex:index" (txid is a 64-byte Hash64 = 128 hex chars).
    pub bond_outpoint: String,
    /// Lowest epoch to return (inclusive). Pass `last_attested_epoch + 1` to fetch the backlog.
    pub from_epoch: u64,
    /// Max targets to return; `0` yields none. The server also caps this.
    pub limit: u32,
}

impl Serializer for GetValidatorAttestationTargetsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond_outpoint, writer)?;
        store!(u64, &self.from_epoch, writer)?;
        store!(u32, &self.limit, writer)?;
        Ok(())
    }
}

impl Deserializer for GetValidatorAttestationTargetsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let bond_outpoint = load!(String, reader)?;
        let from_epoch = load!(u64, reader)?;
        let limit = load!(u32, reader)?;
        Ok(Self { bond_outpoint, from_epoch, limit })
    }
}

/// kaspa-pq DNS v3: one ready-to-sign attestation target in a [`GetValidatorAttestationTargetsResponse`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcValidatorAttestationTarget {
    pub epoch: u64,
    /// Selected-chain anchor the attestation approves (Hash64, hex).
    pub target_hash: String,
    pub target_daa_score: u64,
    /// Commitment over the active validator set (Hash64, hex).
    pub validator_set_commitment: String,
    /// The ready-to-sign 32-byte attestation message digest (hex).
    pub message: String,
}

impl Serializer for RpcValidatorAttestationTarget {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.epoch, writer)?;
        store!(String, &self.target_hash, writer)?;
        store!(u64, &self.target_daa_score, writer)?;
        store!(String, &self.validator_set_commitment, writer)?;
        store!(String, &self.message, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcValidatorAttestationTarget {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let epoch = load!(u64, reader)?;
        let target_hash = load!(String, reader)?;
        let target_daa_score = load!(u64, reader)?;
        let validator_set_commitment = load!(String, reader)?;
        let message = load!(String, reader)?;
        Ok(Self { epoch, target_hash, target_daa_score, validator_set_commitment, message })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetValidatorAttestationTargetsResponse {
    /// Ready targets in ascending epoch order (empty when the overlay is off or none are ready).
    pub targets: Vec<RpcValidatorAttestationTarget>,
}

impl Serializer for GetValidatorAttestationTargetsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcValidatorAttestationTarget>, &self.targets, writer)?;
        Ok(())
    }
}

impl Deserializer for GetValidatorAttestationTargetsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let targets = deserialize!(Vec<RpcValidatorAttestationTarget>, reader)?;
        Ok(Self { targets })
    }
}

// kaspa-pq Phase 12 (ADR-0011): getStakeBond. The sidecar's own stake-bond status,
// evaluated at the node's sink so it matches what the validator would attest for.
// `available` is false when the overlay is not configured or no such bond exists.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStakeBondRequest {
    /// Stake-bond outpoint, "txid_hex:index" (txid is a 64-byte Hash64 = 128 hex chars).
    pub bond_outpoint: String,
}

impl Serializer for GetStakeBondRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond_outpoint, writer)?;
        Ok(())
    }
}

impl Deserializer for GetStakeBondRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let bond_outpoint = load!(String, reader)?;
        Ok(Self { bond_outpoint })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStakeBondResponse {
    /// False when the overlay is not configured or no such bond exists; the remaining
    /// fields are then defaults. (A malformed outpoint is a request error.)
    pub available: bool,
    /// The bond's validator id (validator_pubkey_hash, Hash64 hex). The sidecar checks
    /// this matches its own key.
    pub validator_id: String,
    pub amount: u64,
    pub activation_daa_score: u64,
    /// Effective status at the node's sink: "pending" / "active" / "unbonding" / "slashed".
    pub effective_status: String,
}

impl Serializer for GetStakeBondResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.validator_id, writer)?;
        store!(u64, &self.amount, writer)?;
        store!(u64, &self.activation_daa_score, writer)?;
        store!(String, &self.effective_status, writer)?;
        Ok(())
    }
}

impl Deserializer for GetStakeBondResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let available = load!(bool, reader)?;
        let validator_id = load!(String, reader)?;
        let amount = load!(u64, reader)?;
        let activation_daa_score = load!(u64, reader)?;
        let effective_status = load!(String, reader)?;
        Ok(Self { available, validator_id, amount, activation_daa_score, effective_status })
    }
}

// kaspa-pq: getStakeBonds — a paged, filtered enumeration of the StakeBonds
// overlay store. The store is outpoint-keyed with no owner index, so the node
// does a full scan + in-memory filter; the request is always bounded by `limit`
// and walked with an outpoint `cursor`, so an owner can recover the outpoint(s)
// of bonds they funded (the only key `StakeUnbondRequest` binds to) without ever
// listing the whole set unbounded.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStakeBondsRequest {
    /// Restrict to bonds owned by this `owner_pubkey_hash` (Hash64 hex); `None` = any owner.
    pub owner_pubkey_hash: Option<String>,
    /// Restrict to bonds whose effective status is in this set. Each entry is one
    /// of "pending" / "active" / "unbonding" / "slashed" (case-insensitive).
    /// `None`/empty = any status.
    pub status_in: Option<Vec<String>>,
    /// Return only bonds ordered strictly after this outpoint ("txid_hex:index",
    /// exclusive). Pass a previous response's `next_cursor` to page.
    pub cursor: Option<String>,
    /// Max entries to return; `0` selects the server default and values above the
    /// server cap are clamped.
    pub limit: u32,
    /// Point-of-view DAA score for the effective-status filter/report. `None`
    /// uses the live sink. Pin it to page 1's `pov_daa_score` when walking a
    /// `status_in`-filtered result across pages so the effective-status set is a
    /// consistent snapshot (otherwise a bond whose status changes mid-walk can be
    /// skipped). Status is a read-only view, so this never affects consensus.
    pub pov_daa_score: Option<u64>,
}

impl Serializer for GetStakeBondsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Option<String>, &self.owner_pubkey_hash, writer)?;
        store!(Option<Vec<String>>, &self.status_in, writer)?;
        store!(Option<String>, &self.cursor, writer)?;
        store!(u32, &self.limit, writer)?;
        store!(Option<u64>, &self.pov_daa_score, writer)?;
        Ok(())
    }
}

impl Deserializer for GetStakeBondsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let owner_pubkey_hash = load!(Option<String>, reader)?;
        let status_in = load!(Option<Vec<String>>, reader)?;
        let cursor = load!(Option<String>, reader)?;
        let limit = load!(u32, reader)?;
        let pov_daa_score = load!(Option<u64>, reader)?;
        Ok(Self { owner_pubkey_hash, status_in, cursor, limit, pov_daa_score })
    }
}

/// kaspa-pq: one stake-bond entry in a [`GetStakeBondsResponse`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcStakeBondEntry {
    /// Bond outpoint, "txid_hex:index" — the key a `StakeUnbondRequest` binds to.
    pub bond_outpoint: String,
    /// The bond owner (owner_pubkey_hash, Hash64 hex).
    pub owner_pubkey_hash: String,
    /// The bond's validator id (validator_pubkey_hash, Hash64 hex).
    pub validator_id: String,
    pub amount: u64,
    pub activation_daa_score: u64,
    /// Unbonding period (blocks). With `unbond_request_daa_score` a client can
    /// compute the release height = `unbond_request_daa_score + unbonding_period_blocks`.
    pub unbonding_period_blocks: u64,
    /// DAA score at which an unbond request was accepted, or `None`.
    pub unbond_request_daa_score: Option<u64>,
    /// Stored bond status ("pending"/"active"/"unbonding"/"slashed").
    pub stored_status: String,
    /// Effective status at `pov_daa_score` (the sink), matching GetStakeBond.
    pub effective_status: String,
}

impl Serializer for RpcStakeBondEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond_outpoint, writer)?;
        store!(String, &self.owner_pubkey_hash, writer)?;
        store!(String, &self.validator_id, writer)?;
        store!(u64, &self.amount, writer)?;
        store!(u64, &self.activation_daa_score, writer)?;
        store!(u64, &self.unbonding_period_blocks, writer)?;
        store!(Option<u64>, &self.unbond_request_daa_score, writer)?;
        store!(String, &self.stored_status, writer)?;
        store!(String, &self.effective_status, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcStakeBondEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let bond_outpoint = load!(String, reader)?;
        let owner_pubkey_hash = load!(String, reader)?;
        let validator_id = load!(String, reader)?;
        let amount = load!(u64, reader)?;
        let activation_daa_score = load!(u64, reader)?;
        let unbonding_period_blocks = load!(u64, reader)?;
        let unbond_request_daa_score = load!(Option<u64>, reader)?;
        let stored_status = load!(String, reader)?;
        let effective_status = load!(String, reader)?;
        Ok(Self {
            bond_outpoint,
            owner_pubkey_hash,
            validator_id,
            amount,
            activation_daa_score,
            unbonding_period_blocks,
            unbond_request_daa_score,
            stored_status,
            effective_status,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStakeBondsResponse {
    pub bonds: Vec<RpcStakeBondEntry>,
    /// Pass as the next request's `cursor` to page; `None` on the last page.
    pub next_cursor: Option<String>,
    /// Sink DAA score the effective statuses were evaluated at.
    pub pov_daa_score: u64,
}

impl Serializer for GetStakeBondsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcStakeBondEntry>, &self.bonds, writer)?;
        store!(Option<String>, &self.next_cursor, writer)?;
        store!(u64, &self.pov_daa_score, writer)?;
        Ok(())
    }
}

impl Deserializer for GetStakeBondsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let bonds = deserialize!(Vec<RpcStakeBondEntry>, reader)?;
        let next_cursor = load!(Option<String>, reader)?;
        let pov_daa_score = load!(u64, reader)?;
        Ok(Self { bonds, next_cursor, pov_daa_score })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxosByAddressesRequest {
    pub addresses: Vec<RpcAddress>,
}

impl GetUtxosByAddressesRequest {
    pub fn new(addresses: Vec<RpcAddress>) -> Self {
        Self { addresses }
    }
}

impl Serializer for GetUtxosByAddressesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcAddress>, &self.addresses, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxosByAddressesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let addresses = load!(Vec<RpcAddress>, reader)?;

        Ok(Self { addresses })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxosByAddressesResponse {
    pub entries: Vec<RpcUtxosByAddressesEntry>,
}

impl GetUtxosByAddressesResponse {
    pub fn new(entries: Vec<RpcUtxosByAddressesEntry>) -> Self {
        Self { entries }
    }
}

impl Serializer for GetUtxosByAddressesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcUtxosByAddressesEntry>, &self.entries, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxosByAddressesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let entries = deserialize!(Vec<RpcUtxosByAddressesEntry>, reader)?;

        Ok(Self { entries })
    }
}

/// Cursor-paginated single-address UTXO query. Use this instead of `GetUtxosByAddresses` for
/// addresses with very large UTXO sets (e.g. an unconsolidated mining payout), whose full response
/// can exceed client message-size / timeout limits.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxosByAddressPageRequest {
    pub address: RpcAddress,
    /// Opaque resume token returned as `next_cursor` by the previous page; empty = start at the beginning.
    pub cursor: String,
    /// Maximum entries to return; 0 selects the server default. The server caps the effective value.
    pub limit: u64,
}

impl GetUtxosByAddressPageRequest {
    pub fn new(address: RpcAddress, cursor: String, limit: u64) -> Self {
        Self { address, cursor, limit }
    }
}

impl Serializer for GetUtxosByAddressPageRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcAddress, &self.address, writer)?;
        store!(String, &self.cursor, writer)?;
        store!(u64, &self.limit, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxosByAddressPageRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let address = load!(RpcAddress, reader)?;
        let cursor = load!(String, reader)?;
        let limit = load!(u64, reader)?;

        Ok(Self { address, cursor, limit })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxosByAddressPageResponse {
    pub entries: Vec<RpcUtxosByAddressesEntry>,
    /// Opaque resume token for the next page; empty = no further pages.
    pub next_cursor: String,
}

impl GetUtxosByAddressPageResponse {
    pub fn new(entries: Vec<RpcUtxosByAddressesEntry>, next_cursor: String) -> Self {
        Self { entries, next_cursor }
    }
}

impl Serializer for GetUtxosByAddressPageResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcUtxosByAddressesEntry>, &self.entries, writer)?;
        store!(String, &self.next_cursor, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxosByAddressPageResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let entries = deserialize!(Vec<RpcUtxosByAddressesEntry>, reader)?;
        let next_cursor = load!(String, reader)?;

        Ok(Self { entries, next_cursor })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BanRequest {
    pub ip: RpcIpAddress,
}

impl BanRequest {
    pub fn new(ip: RpcIpAddress) -> Self {
        Self { ip }
    }
}

impl Serializer for BanRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcIpAddress, &self.ip, writer)?;

        Ok(())
    }
}

impl Deserializer for BanRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let ip = load!(RpcIpAddress, reader)?;

        Ok(Self { ip })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BanResponse {}

impl Serializer for BanResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for BanResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnbanRequest {
    pub ip: RpcIpAddress,
}

impl UnbanRequest {
    pub fn new(ip: RpcIpAddress) -> Self {
        Self { ip }
    }
}

impl Serializer for UnbanRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcIpAddress, &self.ip, writer)?;

        Ok(())
    }
}

impl Deserializer for UnbanRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let ip = load!(RpcIpAddress, reader)?;

        Ok(Self { ip })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnbanResponse {}

impl Serializer for UnbanResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for UnbanResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateNetworkHashesPerSecondRequest {
    pub window_size: u32,
    pub start_hash: Option<RpcHash>,
}

impl EstimateNetworkHashesPerSecondRequest {
    pub fn new(window_size: u32, start_hash: Option<RpcHash>) -> Self {
        Self { window_size, start_hash }
    }
}

impl Serializer for EstimateNetworkHashesPerSecondRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.window_size, writer)?;
        store!(Option<RpcHash>, &self.start_hash, writer)?;

        Ok(())
    }
}

impl Deserializer for EstimateNetworkHashesPerSecondRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let window_size = load!(u32, reader)?;
        let start_hash = load!(Option<RpcHash>, reader)?;

        Ok(Self { window_size, start_hash })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateNetworkHashesPerSecondResponse {
    pub network_hashes_per_second: u64,
}

impl EstimateNetworkHashesPerSecondResponse {
    pub fn new(network_hashes_per_second: u64) -> Self {
        Self { network_hashes_per_second }
    }
}

impl Serializer for EstimateNetworkHashesPerSecondResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.network_hashes_per_second, writer)?;

        Ok(())
    }
}

impl Deserializer for EstimateNetworkHashesPerSecondResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let network_hashes_per_second = load!(u64, reader)?;

        Ok(Self { network_hashes_per_second })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesByAddressesRequest {
    pub addresses: Vec<RpcAddress>,
    pub include_orphan_pool: bool,
    // TODO: replace with `include_transaction_pool`
    pub filter_transaction_pool: bool,
}

impl GetMempoolEntriesByAddressesRequest {
    pub fn new(addresses: Vec<RpcAddress>, include_orphan_pool: bool, filter_transaction_pool: bool) -> Self {
        Self { addresses, include_orphan_pool, filter_transaction_pool }
    }
}

impl Serializer for GetMempoolEntriesByAddressesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcAddress>, &self.addresses, writer)?;
        store!(bool, &self.include_orphan_pool, writer)?;
        store!(bool, &self.filter_transaction_pool, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMempoolEntriesByAddressesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let addresses = load!(Vec<RpcAddress>, reader)?;
        let include_orphan_pool = load!(bool, reader)?;
        let filter_transaction_pool = load!(bool, reader)?;

        Ok(Self { addresses, include_orphan_pool, filter_transaction_pool })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesByAddressesResponse {
    pub entries: Vec<RpcMempoolEntryByAddress>,
}

impl GetMempoolEntriesByAddressesResponse {
    pub fn new(entries: Vec<RpcMempoolEntryByAddress>) -> Self {
        Self { entries }
    }
}

impl Serializer for GetMempoolEntriesByAddressesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcMempoolEntryByAddress>, &self.entries, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMempoolEntriesByAddressesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let entries = deserialize!(Vec<RpcMempoolEntryByAddress>, reader)?;

        Ok(Self { entries })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCoinSupplyRequest {}

impl Serializer for GetCoinSupplyRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetCoinSupplyRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCoinSupplyResponse {
    pub max_sompi: u64,
    pub circulating_sompi: u64,
}

impl GetCoinSupplyResponse {
    pub fn new(max_sompi: u64, circulating_sompi: u64) -> Self {
        Self { max_sompi, circulating_sompi }
    }
}

impl Serializer for GetCoinSupplyResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.max_sompi, writer)?;
        store!(u64, &self.circulating_sompi, writer)?;

        Ok(())
    }
}

impl Deserializer for GetCoinSupplyResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let max_sompi = load!(u64, reader)?;
        let circulating_sompi = load!(u64, reader)?;

        Ok(Self { max_sompi, circulating_sompi })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingRequest {}

impl Serializer for PingRequest {
    fn serialize<W: std::io::Write>(&self, _writer: &mut W) -> std::io::Result<()> {
        Ok(())
    }
}

impl Deserializer for PingRequest {
    fn deserialize<R: std::io::Read>(_reader: &mut R) -> std::io::Result<Self> {
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {}

impl Serializer for PingResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for PingResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsProfileData {
    pub cpu_usage: f32,
    pub memory_usage: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConnectionsRequest {
    pub include_profile_data: bool,
}

impl Serializer for GetConnectionsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(bool, &self.include_profile_data, writer)?;
        Ok(())
    }
}

impl Deserializer for GetConnectionsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let include_profile_data = load!(bool, reader)?;
        Ok(Self { include_profile_data })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConnectionsResponse {
    pub clients: u32,
    pub peers: u16,
    pub profile_data: Option<ConnectionsProfileData>,
}

impl Serializer for GetConnectionsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.clients, writer)?;
        store!(u16, &self.peers, writer)?;
        store!(Option<ConnectionsProfileData>, &self.profile_data, writer)?;
        Ok(())
    }
}

impl Deserializer for GetConnectionsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let clients = load!(u32, reader)?;
        let peers = load!(u16, reader)?;
        let extra = load!(Option<ConnectionsProfileData>, reader)?;
        Ok(Self { clients, peers, profile_data: extra })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSystemInfoRequest {}

impl Serializer for GetSystemInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        Ok(())
    }
}

impl Deserializer for GetSystemInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        Ok(Self {})
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSystemInfoResponse {
    pub version: String,
    pub system_id: Option<Vec<u8>>,
    pub git_hash: Option<Vec<u8>>,
    pub cpu_physical_cores: u16,
    pub total_memory: u64,
    pub fd_limit: u32,
    pub proxy_socket_limit_per_cpu_core: Option<u32>,
}

impl std::fmt::Debug for GetSystemInfoResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GetSystemInfoResponse")
            .field("version", &self.version)
            .field("system_id", &self.system_id.as_ref().map(|id| id.to_hex()))
            .field("git_hash", &self.git_hash.as_ref().map(|hash| hash.to_hex()))
            .field("cpu_physical_cores", &self.cpu_physical_cores)
            .field("total_memory", &self.total_memory)
            .field("fd_limit", &self.fd_limit)
            .field("proxy_socket_limit_per_cpu_core", &self.proxy_socket_limit_per_cpu_core)
            .finish()
    }
}

impl Serializer for GetSystemInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(String, &self.version, writer)?;
        store!(Option<Vec<u8>>, &self.system_id, writer)?;
        store!(Option<Vec<u8>>, &self.git_hash, writer)?;
        store!(u16, &self.cpu_physical_cores, writer)?;
        store!(u64, &self.total_memory, writer)?;
        store!(u32, &self.fd_limit, writer)?;
        store!(Option<u32>, &self.proxy_socket_limit_per_cpu_core, writer)?;

        Ok(())
    }
}

impl Deserializer for GetSystemInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let payload_version = load!(u16, reader)?;
        let version = load!(String, reader)?;
        let system_id = load!(Option<Vec<u8>>, reader)?;
        let git_hash = load!(Option<Vec<u8>>, reader)?;
        let cpu_physical_cores = load!(u16, reader)?;
        let total_memory = load!(u64, reader)?;
        let fd_limit = load!(u32, reader)?;

        let proxy_socket_limit_per_cpu_core = if payload_version > 1 { load!(Option<u32>, reader)? } else { None };

        Ok(Self { version, system_id, git_hash, cpu_physical_cores, total_memory, fd_limit, proxy_socket_limit_per_cpu_core })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMetricsRequest {
    pub process_metrics: bool,
    pub connection_metrics: bool,
    pub bandwidth_metrics: bool,
    pub consensus_metrics: bool,
    pub storage_metrics: bool,
    pub custom_metrics: bool,
}

impl Serializer for GetMetricsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.process_metrics, writer)?;
        store!(bool, &self.connection_metrics, writer)?;
        store!(bool, &self.bandwidth_metrics, writer)?;
        store!(bool, &self.consensus_metrics, writer)?;
        store!(bool, &self.storage_metrics, writer)?;
        store!(bool, &self.custom_metrics, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMetricsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let process_metrics = load!(bool, reader)?;
        let connection_metrics = load!(bool, reader)?;
        let bandwidth_metrics = load!(bool, reader)?;
        let consensus_metrics = load!(bool, reader)?;
        let storage_metrics = load!(bool, reader)?;
        let custom_metrics = load!(bool, reader)?;

        Ok(Self { process_metrics, connection_metrics, bandwidth_metrics, consensus_metrics, storage_metrics, custom_metrics })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetrics {
    pub resident_set_size: u64,
    pub virtual_memory_size: u64,
    pub core_num: u32,
    pub cpu_usage: f32,
    pub fd_num: u32,
    pub disk_io_read_bytes: u64,
    pub disk_io_write_bytes: u64,
    pub disk_io_read_per_sec: f32,
    pub disk_io_write_per_sec: f32,
}

impl Serializer for ProcessMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.resident_set_size, writer)?;
        store!(u64, &self.virtual_memory_size, writer)?;
        store!(u32, &self.core_num, writer)?;
        store!(f32, &self.cpu_usage, writer)?;
        store!(u32, &self.fd_num, writer)?;
        store!(u64, &self.disk_io_read_bytes, writer)?;
        store!(u64, &self.disk_io_write_bytes, writer)?;
        store!(f32, &self.disk_io_read_per_sec, writer)?;
        store!(f32, &self.disk_io_write_per_sec, writer)?;

        Ok(())
    }
}

impl Deserializer for ProcessMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let resident_set_size = load!(u64, reader)?;
        let virtual_memory_size = load!(u64, reader)?;
        let core_num = load!(u32, reader)?;
        let cpu_usage = load!(f32, reader)?;
        let fd_num = load!(u32, reader)?;
        let disk_io_read_bytes = load!(u64, reader)?;
        let disk_io_write_bytes = load!(u64, reader)?;
        let disk_io_read_per_sec = load!(f32, reader)?;
        let disk_io_write_per_sec = load!(f32, reader)?;

        Ok(Self {
            resident_set_size,
            virtual_memory_size,
            core_num,
            cpu_usage,
            fd_num,
            disk_io_read_bytes,
            disk_io_write_bytes,
            disk_io_read_per_sec,
            disk_io_write_per_sec,
        })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionMetrics {
    pub borsh_live_connections: u32,
    pub borsh_connection_attempts: u64,
    pub borsh_handshake_failures: u64,
    pub json_live_connections: u32,
    pub json_connection_attempts: u64,
    pub json_handshake_failures: u64,

    pub active_peers: u32,
}

impl Serializer for ConnectionMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.borsh_live_connections, writer)?;
        store!(u64, &self.borsh_connection_attempts, writer)?;
        store!(u64, &self.borsh_handshake_failures, writer)?;
        store!(u32, &self.json_live_connections, writer)?;
        store!(u64, &self.json_connection_attempts, writer)?;
        store!(u64, &self.json_handshake_failures, writer)?;
        store!(u32, &self.active_peers, writer)?;

        Ok(())
    }
}

impl Deserializer for ConnectionMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let borsh_live_connections = load!(u32, reader)?;
        let borsh_connection_attempts = load!(u64, reader)?;
        let borsh_handshake_failures = load!(u64, reader)?;
        let json_live_connections = load!(u32, reader)?;
        let json_connection_attempts = load!(u64, reader)?;
        let json_handshake_failures = load!(u64, reader)?;
        let active_peers = load!(u32, reader)?;

        Ok(Self {
            borsh_live_connections,
            borsh_connection_attempts,
            borsh_handshake_failures,
            json_live_connections,
            json_connection_attempts,
            json_handshake_failures,
            active_peers,
        })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BandwidthMetrics {
    pub borsh_bytes_tx: u64,
    pub borsh_bytes_rx: u64,
    pub json_bytes_tx: u64,
    pub json_bytes_rx: u64,
    pub p2p_bytes_tx: u64,
    pub p2p_bytes_rx: u64,
    pub grpc_bytes_tx: u64,
    pub grpc_bytes_rx: u64,
}

impl Serializer for BandwidthMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.borsh_bytes_tx, writer)?;
        store!(u64, &self.borsh_bytes_rx, writer)?;
        store!(u64, &self.json_bytes_tx, writer)?;
        store!(u64, &self.json_bytes_rx, writer)?;
        store!(u64, &self.p2p_bytes_tx, writer)?;
        store!(u64, &self.p2p_bytes_rx, writer)?;
        store!(u64, &self.grpc_bytes_tx, writer)?;
        store!(u64, &self.grpc_bytes_rx, writer)?;

        Ok(())
    }
}

impl Deserializer for BandwidthMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let borsh_bytes_tx = load!(u64, reader)?;
        let borsh_bytes_rx = load!(u64, reader)?;
        let json_bytes_tx = load!(u64, reader)?;
        let json_bytes_rx = load!(u64, reader)?;
        let p2p_bytes_tx = load!(u64, reader)?;
        let p2p_bytes_rx = load!(u64, reader)?;
        let grpc_bytes_tx = load!(u64, reader)?;
        let grpc_bytes_rx = load!(u64, reader)?;

        Ok(Self {
            borsh_bytes_tx,
            borsh_bytes_rx,
            json_bytes_tx,
            json_bytes_rx,
            p2p_bytes_tx,
            p2p_bytes_rx,
            grpc_bytes_tx,
            grpc_bytes_rx,
        })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsensusMetrics {
    pub node_blocks_submitted_count: u64,
    pub node_headers_processed_count: u64,
    pub node_dependencies_processed_count: u64,
    pub node_bodies_processed_count: u64,
    pub node_transactions_processed_count: u64,
    pub node_chain_blocks_processed_count: u64,
    pub node_mass_processed_count: u64,

    pub node_database_blocks_count: u64,
    pub node_database_headers_count: u64,

    pub network_mempool_size: u64,
    pub network_tip_hashes_count: u32,
    pub network_difficulty: f64,
    pub network_past_median_time: u64,
    pub network_virtual_parent_hashes_count: u32,
    pub network_virtual_daa_score: u64,
}

impl Serializer for ConsensusMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.node_blocks_submitted_count, writer)?;
        store!(u64, &self.node_headers_processed_count, writer)?;
        store!(u64, &self.node_dependencies_processed_count, writer)?;
        store!(u64, &self.node_bodies_processed_count, writer)?;
        store!(u64, &self.node_transactions_processed_count, writer)?;
        store!(u64, &self.node_chain_blocks_processed_count, writer)?;
        store!(u64, &self.node_mass_processed_count, writer)?;
        store!(u64, &self.node_database_blocks_count, writer)?;
        store!(u64, &self.node_database_headers_count, writer)?;
        store!(u64, &self.network_mempool_size, writer)?;
        store!(u32, &self.network_tip_hashes_count, writer)?;
        store!(f64, &self.network_difficulty, writer)?;
        store!(u64, &self.network_past_median_time, writer)?;
        store!(u32, &self.network_virtual_parent_hashes_count, writer)?;
        store!(u64, &self.network_virtual_daa_score, writer)?;

        Ok(())
    }
}

impl Deserializer for ConsensusMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let node_blocks_submitted_count = load!(u64, reader)?;
        let node_headers_processed_count = load!(u64, reader)?;
        let node_dependencies_processed_count = load!(u64, reader)?;
        let node_bodies_processed_count = load!(u64, reader)?;
        let node_transactions_processed_count = load!(u64, reader)?;
        let node_chain_blocks_processed_count = load!(u64, reader)?;
        let node_mass_processed_count = load!(u64, reader)?;
        let node_database_blocks_count = load!(u64, reader)?;
        let node_database_headers_count = load!(u64, reader)?;
        let network_mempool_size = load!(u64, reader)?;
        let network_tip_hashes_count = load!(u32, reader)?;
        let network_difficulty = load!(f64, reader)?;
        let network_past_median_time = load!(u64, reader)?;
        let network_virtual_parent_hashes_count = load!(u32, reader)?;
        let network_virtual_daa_score = load!(u64, reader)?;

        Ok(Self {
            node_blocks_submitted_count,
            node_headers_processed_count,
            node_dependencies_processed_count,
            node_bodies_processed_count,
            node_transactions_processed_count,
            node_chain_blocks_processed_count,
            node_mass_processed_count,
            node_database_blocks_count,
            node_database_headers_count,
            network_mempool_size,
            network_tip_hashes_count,
            network_difficulty,
            network_past_median_time,
            network_virtual_parent_hashes_count,
            network_virtual_daa_score,
        })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageMetrics {
    pub storage_size_bytes: u64,
}

impl Serializer for StorageMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.storage_size_bytes, writer)?;

        Ok(())
    }
}

impl Deserializer for StorageMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let storage_size_bytes = load!(u64, reader)?;

        Ok(Self { storage_size_bytes })
    }
}

// TODO: Custom metrics dictionary
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CustomMetricValue {
    Placeholder,
}

impl Serializer for CustomMetricValue {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        Ok(())
    }
}

impl Deserializer for CustomMetricValue {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        Ok(CustomMetricValue::Placeholder)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMetricsResponse {
    pub server_time: u64,
    pub process_metrics: Option<ProcessMetrics>,
    pub connection_metrics: Option<ConnectionMetrics>,
    pub bandwidth_metrics: Option<BandwidthMetrics>,
    pub consensus_metrics: Option<ConsensusMetrics>,
    pub storage_metrics: Option<StorageMetrics>,
    // TODO: this is currently a placeholder
    pub custom_metrics: Option<HashMap<String, CustomMetricValue>>,
}

impl GetMetricsResponse {
    pub fn new(
        server_time: u64,
        process_metrics: Option<ProcessMetrics>,
        connection_metrics: Option<ConnectionMetrics>,
        bandwidth_metrics: Option<BandwidthMetrics>,
        consensus_metrics: Option<ConsensusMetrics>,
        storage_metrics: Option<StorageMetrics>,
        custom_metrics: Option<HashMap<String, CustomMetricValue>>,
    ) -> Self {
        Self {
            process_metrics,
            connection_metrics,
            bandwidth_metrics,
            consensus_metrics,
            storage_metrics,
            server_time,
            custom_metrics,
        }
    }
}

impl Serializer for GetMetricsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.server_time, writer)?;
        serialize!(Option<ProcessMetrics>, &self.process_metrics, writer)?;
        serialize!(Option<ConnectionMetrics>, &self.connection_metrics, writer)?;
        serialize!(Option<BandwidthMetrics>, &self.bandwidth_metrics, writer)?;
        serialize!(Option<ConsensusMetrics>, &self.consensus_metrics, writer)?;
        serialize!(Option<StorageMetrics>, &self.storage_metrics, writer)?;
        serialize!(Option<HashMap<String, CustomMetricValue>>, &self.custom_metrics, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMetricsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let server_time = load!(u64, reader)?;
        let process_metrics = deserialize!(Option<ProcessMetrics>, reader)?;
        let connection_metrics = deserialize!(Option<ConnectionMetrics>, reader)?;
        let bandwidth_metrics = deserialize!(Option<BandwidthMetrics>, reader)?;
        let consensus_metrics = deserialize!(Option<ConsensusMetrics>, reader)?;
        let storage_metrics = deserialize!(Option<StorageMetrics>, reader)?;
        let custom_metrics = deserialize!(Option<HashMap<String, CustomMetricValue>>, reader)?;

        Ok(Self {
            server_time,
            process_metrics,
            connection_metrics,
            bandwidth_metrics,
            consensus_metrics,
            storage_metrics,
            custom_metrics,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
#[borsh(use_discriminant = true)]
pub enum RpcCaps {
    Full = 0,
    Blocks,
    UtxoIndex,
    Mempool,
    Metrics,
    Visualizer,
    Mining,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetServerInfoRequest {}

impl Serializer for GetServerInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetServerInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetServerInfoResponse {
    pub rpc_api_version: u16,
    pub rpc_api_revision: u16,
    pub server_version: String,
    pub network_id: RpcNetworkId,
    pub has_utxo_index: bool,
    pub is_synced: bool,
    pub virtual_daa_score: u64,
}

impl Serializer for GetServerInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        store!(u16, &self.rpc_api_version, writer)?;
        store!(u16, &self.rpc_api_revision, writer)?;

        store!(String, &self.server_version, writer)?;
        store!(RpcNetworkId, &self.network_id, writer)?;
        store!(bool, &self.has_utxo_index, writer)?;
        store!(bool, &self.is_synced, writer)?;
        store!(u64, &self.virtual_daa_score, writer)?;

        Ok(())
    }
}

impl Deserializer for GetServerInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        let rpc_api_version = load!(u16, reader)?;
        let rpc_api_revision = load!(u16, reader)?;

        let server_version = load!(String, reader)?;
        let network_id = load!(RpcNetworkId, reader)?;
        let has_utxo_index = load!(bool, reader)?;
        let is_synced = load!(bool, reader)?;
        let virtual_daa_score = load!(u64, reader)?;

        Ok(Self { rpc_api_version, rpc_api_revision, server_version, network_id, has_utxo_index, is_synced, virtual_daa_score })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSyncStatusRequest {}

impl Serializer for GetSyncStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSyncStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSyncStatusResponse {
    pub is_synced: bool,
}

impl Serializer for GetSyncStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.is_synced, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSyncStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let is_synced = load!(bool, reader)?;
        Ok(Self { is_synced })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDaaScoreTimestampEstimateRequest {
    pub daa_scores: Vec<u64>,
}

impl GetDaaScoreTimestampEstimateRequest {
    pub fn new(daa_scores: Vec<u64>) -> Self {
        Self { daa_scores }
    }
}

impl Serializer for GetDaaScoreTimestampEstimateRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<u64>, &self.daa_scores, writer)?;
        Ok(())
    }
}

impl Deserializer for GetDaaScoreTimestampEstimateRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let daa_scores = load!(Vec<u64>, reader)?;
        Ok(Self { daa_scores })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDaaScoreTimestampEstimateResponse {
    pub timestamps: Vec<u64>,
}

impl GetDaaScoreTimestampEstimateResponse {
    pub fn new(timestamps: Vec<u64>) -> Self {
        Self { timestamps }
    }
}

impl Serializer for GetDaaScoreTimestampEstimateResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<u64>, &self.timestamps, writer)?;
        Ok(())
    }
}

impl Deserializer for GetDaaScoreTimestampEstimateResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let timestamps = load!(Vec<u64>, reader)?;
        Ok(Self { timestamps })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// Fee rate estimations

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeEstimateRequest {}

impl Serializer for GetFeeEstimateRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetFeeEstimateRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeEstimateResponse {
    pub estimate: RpcFeeEstimate,
}

impl Serializer for GetFeeEstimateResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcFeeEstimate, &self.estimate, writer)?;
        Ok(())
    }
}

impl Deserializer for GetFeeEstimateResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let estimate = deserialize!(RpcFeeEstimate, reader)?;
        Ok(Self { estimate })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeEstimateExperimentalRequest {
    pub verbose: bool,
}

impl Serializer for GetFeeEstimateExperimentalRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.verbose, writer)?;
        Ok(())
    }
}

impl Deserializer for GetFeeEstimateExperimentalRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let verbose = load!(bool, reader)?;
        Ok(Self { verbose })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeEstimateExperimentalResponse {
    /// The usual feerate estimate response
    pub estimate: RpcFeeEstimate,

    /// Experimental verbose data
    pub verbose: Option<RpcFeeEstimateVerboseExperimentalData>,
}

impl Serializer for GetFeeEstimateExperimentalResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcFeeEstimate, &self.estimate, writer)?;
        serialize!(Option<RpcFeeEstimateVerboseExperimentalData>, &self.verbose, writer)?;
        Ok(())
    }
}

impl Deserializer for GetFeeEstimateExperimentalResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let estimate = deserialize!(RpcFeeEstimate, reader)?;
        let verbose = deserialize!(Option<RpcFeeEstimateVerboseExperimentalData>, reader)?;
        Ok(Self { estimate, verbose })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentBlockColorRequest {
    pub hash: RpcHash,
}

impl Serializer for GetCurrentBlockColorRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.hash, writer)?;

        Ok(())
    }
}

impl Deserializer for GetCurrentBlockColorRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let hash = load!(RpcHash, reader)?;

        Ok(Self { hash })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentBlockColorResponse {
    pub blue: bool,
}

impl Serializer for GetCurrentBlockColorResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.blue, writer)?;

        Ok(())
    }
}

impl Deserializer for GetCurrentBlockColorResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let blue = load!(bool, reader)?;

        Ok(Self { blue })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxoReturnAddressRequest {
    // PR-9.5f: txid widened to RpcTransactionId (= Hash64) so it
    // feeds the consensus `get_transactions_by_accepting_daa_score`
    // API, which takes `Option<Vec<TransactionId>>`.
    pub txid: RpcTransactionId,
    pub accepting_block_daa_score: u64,
}

impl GetUtxoReturnAddressRequest {
    pub fn new(txid: RpcTransactionId, accepting_block_daa_score: u64) -> Self {
        Self { txid, accepting_block_daa_score }
    }
}

impl Serializer for GetUtxoReturnAddressRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        // PR-9.5f: Hash64 wire width.
        store!(kaspa_consensus_core::Hash64, &self.txid, writer)?;
        store!(u64, &self.accepting_block_daa_score, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxoReturnAddressRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let txid = load!(kaspa_consensus_core::Hash64, reader)?;
        let accepting_block_daa_score = load!(u64, reader)?;

        Ok(Self { txid, accepting_block_daa_score })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxoReturnAddressResponse {
    pub return_address: RpcAddress,
}

impl GetUtxoReturnAddressResponse {
    pub fn new(return_address: RpcAddress) -> Self {
        Self { return_address }
    }
}

impl Serializer for GetUtxoReturnAddressResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcAddress, &self.return_address, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxoReturnAddressResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let return_address = load!(RpcAddress, reader)?;

        Ok(Self { return_address })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualChainFromBlockV2Request {
    pub start_hash: RpcHash,
    pub data_verbosity_level: Option<RpcDataVerbosityLevel>,
    pub min_confirmation_count: Option<u64>,
}

impl GetVirtualChainFromBlockV2Request {
    pub fn new(start_hash: RpcHash, data_verbosity_level: Option<RpcDataVerbosityLevel>, min_confirmation_count: Option<u64>) -> Self {
        Self { start_hash, data_verbosity_level, min_confirmation_count }
    }
}

impl Serializer for GetVirtualChainFromBlockV2Request {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.start_hash, writer)?;
        serialize!(Option<RpcDataVerbosityLevel>, &self.data_verbosity_level, writer)?;
        store!(Option<u64>, &self.min_confirmation_count, writer)?;

        Ok(())
    }
}

impl Deserializer for GetVirtualChainFromBlockV2Request {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let start_hash = load!(RpcHash, reader)?;
        let data_verbosity_level = deserialize!(Option<RpcDataVerbosityLevel>, reader)?;
        let min_confirmation_count = load!(Option<u64>, reader)?;

        Ok(Self { start_hash, data_verbosity_level, min_confirmation_count })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualChainFromBlockV2Response {
    /// always present, no matter the verbosity level
    pub removed_chain_block_hashes: Arc<Vec<RpcHash>>,
    /// always present, no matter the verbosity level
    pub added_chain_block_hashes: Arc<Vec<RpcHash>>,
    /// struct properties are optionally returned depending on the verbosity level
    pub chain_block_accepted_transactions: Arc<Vec<RpcChainBlockAcceptedTransactions>>,
}

impl Serializer for GetVirtualChainFromBlockV2Response {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcHash>, &self.removed_chain_block_hashes, writer)?;
        store!(Vec<RpcHash>, &self.added_chain_block_hashes, writer)?;
        serialize!(Vec<RpcChainBlockAcceptedTransactions>, &self.chain_block_accepted_transactions, writer)?;
        Ok(())
    }
}

impl Deserializer for GetVirtualChainFromBlockV2Response {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let removed_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let added_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let chain_block_accepted_transactions = deserialize!(Vec<RpcChainBlockAcceptedTransactions>, reader)?;
        Ok(Self {
            removed_chain_block_hashes: removed_chain_block_hashes.into(),
            added_chain_block_hashes: added_chain_block_hashes.into(),
            chain_block_accepted_transactions: chain_block_accepted_transactions.into(),
        })
    }
}

// ----------------------------------------------------------------------------
// Subscriptions & notifications
// ----------------------------------------------------------------------------

// ~~~~~~~~~~~~~~~~~~~~~~
// BlockAddedNotification

/// NotifyBlockAddedRequest registers this connection for blockAdded notifications.
///
/// See: BlockAddedNotification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyBlockAddedRequest {
    pub command: Command,
}
impl NotifyBlockAddedRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifyBlockAddedRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyBlockAddedRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyBlockAddedResponse {}

impl Serializer for NotifyBlockAddedResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyBlockAddedResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

/// BlockAddedNotification is sent whenever a blocks has been added (NOT accepted)
/// into the DAG.
///
/// See: NotifyBlockAddedRequest
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockAddedNotification {
    pub block: Arc<RpcBlock>,
}

impl Serializer for BlockAddedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcBlock, &self.block, writer)?;
        Ok(())
    }
}

impl Deserializer for BlockAddedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let block = deserialize!(RpcBlock, reader)?;
        Ok(Self { block: block.into() })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// VirtualChainChangedNotification

// NotifyVirtualChainChangedRequest registers this connection for
// virtualDaaScoreChanged notifications.
//
// See: VirtualChainChangedNotification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyVirtualChainChangedRequest {
    pub include_accepted_transaction_ids: bool,
    pub command: Command,
}

impl NotifyVirtualChainChangedRequest {
    pub fn new(include_accepted_transaction_ids: bool, command: Command) -> Self {
        Self { include_accepted_transaction_ids, command }
    }
}

impl Serializer for NotifyVirtualChainChangedRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.include_accepted_transaction_ids, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyVirtualChainChangedRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let include_accepted_transaction_ids = load!(bool, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { include_accepted_transaction_ids, command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyVirtualChainChangedResponse {}

impl Serializer for NotifyVirtualChainChangedResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyVirtualChainChangedResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

// VirtualChainChangedNotification is sent whenever the DAG's selected parent
// chain had changed.
//
// See: NotifyVirtualChainChangedRequest
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualChainChangedNotification {
    pub removed_chain_block_hashes: Arc<Vec<RpcHash>>,
    pub added_chain_block_hashes: Arc<Vec<RpcHash>>,
    pub accepted_transaction_ids: Arc<Vec<RpcAcceptedTransactionIds>>,
}

impl Serializer for VirtualChainChangedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcHash>, &self.removed_chain_block_hashes, writer)?;
        store!(Vec<RpcHash>, &self.added_chain_block_hashes, writer)?;
        store!(Vec<RpcAcceptedTransactionIds>, &self.accepted_transaction_ids, writer)?;
        Ok(())
    }
}

impl Deserializer for VirtualChainChangedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let removed_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let added_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let accepted_transaction_ids = load!(Vec<RpcAcceptedTransactionIds>, reader)?;
        Ok(Self {
            removed_chain_block_hashes: removed_chain_block_hashes.into(),
            added_chain_block_hashes: added_chain_block_hashes.into(),
            accepted_transaction_ids: accepted_transaction_ids.into(),
        })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// FinalityConflictNotification

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyFinalityConflictRequest {
    pub command: Command,
}

impl NotifyFinalityConflictRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifyFinalityConflictRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyFinalityConflictRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyFinalityConflictResponse {}

impl Serializer for NotifyFinalityConflictResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyFinalityConflictResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalityConflictNotification {
    pub violating_block_hash: RpcHash,
}

impl Serializer for FinalityConflictNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.violating_block_hash, writer)?;
        Ok(())
    }
}

impl Deserializer for FinalityConflictNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let violating_block_hash = load!(RpcHash, reader)?;
        Ok(Self { violating_block_hash })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// FinalityConflictResolvedNotification

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyFinalityConflictResolvedRequest {
    pub command: Command,
}

impl NotifyFinalityConflictResolvedRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifyFinalityConflictResolvedRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyFinalityConflictResolvedRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyFinalityConflictResolvedResponse {}

impl Serializer for NotifyFinalityConflictResolvedResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyFinalityConflictResolvedResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalityConflictResolvedNotification {
    pub finality_block_hash: RpcHash,
}

impl Serializer for FinalityConflictResolvedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.finality_block_hash, writer)?;
        Ok(())
    }
}

impl Deserializer for FinalityConflictResolvedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let finality_block_hash = load!(RpcHash, reader)?;
        Ok(Self { finality_block_hash })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~
// UtxosChangedNotification

// NotifyUtxosChangedRequestMessage registers this connection for utxoChanged notifications
// for the given addresses. Depending on the provided `command`, notifications will
// start or stop for the provided `addresses`.
//
// If `addresses` is empty, the notifications will start or stop for all addresses.
//
// This call is only available when this kaspad was started with `--utxoindex`
//
// See: UtxosChangedNotification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyUtxosChangedRequest {
    pub addresses: Vec<RpcAddress>,
    pub command: Command,
}

impl NotifyUtxosChangedRequest {
    pub fn new(addresses: Vec<RpcAddress>, command: Command) -> Self {
        Self { addresses, command }
    }
}

impl Serializer for NotifyUtxosChangedRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcAddress>, &self.addresses, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyUtxosChangedRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let addresses = load!(Vec<RpcAddress>, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { addresses, command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyUtxosChangedResponse {}

impl Serializer for NotifyUtxosChangedResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyUtxosChangedResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

// UtxosChangedNotificationMessage is sent whenever the UTXO index had been updated.
//
// See: NotifyUtxosChangedRequest
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UtxosChangedNotification {
    pub added: Arc<Vec<RpcUtxosByAddressesEntry>>,
    pub removed: Arc<Vec<RpcUtxosByAddressesEntry>>,
}

impl UtxosChangedNotification {
    pub(crate) fn apply_utxos_changed_subscription(
        &self,
        subscription: &UtxosChangedSubscription,
        context: &SubscriptionContext,
    ) -> Option<Self> {
        if subscription.to_all() {
            Some(self.clone())
        } else {
            let added = Self::filter_utxos(&self.added, subscription, context);
            let removed = Self::filter_utxos(&self.removed, subscription, context);
            if added.is_empty() && removed.is_empty() {
                None
            } else {
                debug!("CRPC, Creating UtxosChanged notifications with {} added and {} removed utxos", added.len(), removed.len());
                Some(Self { added: Arc::new(added), removed: Arc::new(removed) })
            }
        }
    }

    fn filter_utxos(
        utxo_set: &[RpcUtxosByAddressesEntry],
        subscription: &UtxosChangedSubscription,
        context: &SubscriptionContext,
    ) -> Vec<RpcUtxosByAddressesEntry> {
        let subscription_data = subscription.data();
        utxo_set.iter().filter(|x| subscription_data.contains(&x.utxo_entry.script_public_key, context)).cloned().collect()
    }
}

impl Serializer for UtxosChangedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcUtxosByAddressesEntry>, &self.added, writer)?;
        serialize!(Vec<RpcUtxosByAddressesEntry>, &self.removed, writer)?;
        Ok(())
    }
}

impl Deserializer for UtxosChangedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let added = deserialize!(Vec<RpcUtxosByAddressesEntry>, reader)?;
        let removed = deserialize!(Vec<RpcUtxosByAddressesEntry>, reader)?;
        Ok(Self { added: added.into(), removed: removed.into() })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// SinkBlueScoreChangedNotification

// NotifySinkBlueScoreChangedRequest registers this connection for
// sinkBlueScoreChanged notifications.
//
// See: SinkBlueScoreChangedNotification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifySinkBlueScoreChangedRequest {
    pub command: Command,
}

impl NotifySinkBlueScoreChangedRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifySinkBlueScoreChangedRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifySinkBlueScoreChangedRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifySinkBlueScoreChangedResponse {}

impl Serializer for NotifySinkBlueScoreChangedResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifySinkBlueScoreChangedResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

// SinkBlueScoreChangedNotification is sent whenever the blue score
// of the virtual's selected parent changes.
//
/// See: NotifySinkBlueScoreChangedRequest
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SinkBlueScoreChangedNotification {
    pub sink_blue_score: u64,
}

impl Serializer for SinkBlueScoreChangedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.sink_blue_score, writer)?;
        Ok(())
    }
}

impl Deserializer for SinkBlueScoreChangedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let sink_blue_score = load!(u64, reader)?;
        Ok(Self { sink_blue_score })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// VirtualDaaScoreChangedNotification

// NotifyVirtualDaaScoreChangedRequest registers this connection for
// virtualDaaScoreChanged notifications.
//
// See: VirtualDaaScoreChangedNotification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyVirtualDaaScoreChangedRequest {
    pub command: Command,
}

impl NotifyVirtualDaaScoreChangedRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifyVirtualDaaScoreChangedRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyVirtualDaaScoreChangedRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyVirtualDaaScoreChangedResponse {}

impl Serializer for NotifyVirtualDaaScoreChangedResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyVirtualDaaScoreChangedResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

// VirtualDaaScoreChangedNotification is sent whenever the DAA score
// of the virtual changes.
//
// See NotifyVirtualDaaScoreChangedRequest
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualDaaScoreChangedNotification {
    pub virtual_daa_score: u64,
}

impl Serializer for VirtualDaaScoreChangedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.virtual_daa_score, writer)?;
        Ok(())
    }
}

impl Deserializer for VirtualDaaScoreChangedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let virtual_daa_score = load!(u64, reader)?;
        Ok(Self { virtual_daa_score })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// PruningPointUtxoSetOverrideNotification

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyPruningPointUtxoSetOverrideRequest {
    pub command: Command,
}

impl NotifyPruningPointUtxoSetOverrideRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifyPruningPointUtxoSetOverrideRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyPruningPointUtxoSetOverrideRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyPruningPointUtxoSetOverrideResponse {}

impl Serializer for NotifyPruningPointUtxoSetOverrideResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyPruningPointUtxoSetOverrideResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PruningPointUtxoSetOverrideNotification {}

impl Serializer for PruningPointUtxoSetOverrideNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for PruningPointUtxoSetOverrideNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// NewBlockTemplateNotification

/// NotifyNewBlockTemplateRequest registers this connection for blockAdded notifications.
///
/// See: NewBlockTemplateNotification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyNewBlockTemplateRequest {
    pub command: Command,
}
impl NotifyNewBlockTemplateRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifyNewBlockTemplateRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyNewBlockTemplateRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyNewBlockTemplateResponse {}

impl Serializer for NotifyNewBlockTemplateResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyNewBlockTemplateResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

/// NewBlockTemplateNotification is sent whenever a blocks has been added (NOT accepted)
/// into the DAG.
///
/// See: NotifyNewBlockTemplateRequest
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewBlockTemplateNotification {}

impl Serializer for NewBlockTemplateNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NewBlockTemplateNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

///
///  wRPC response for RpcApiOps::Subscribe request
///
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeResponse {
    id: u64,
}

impl SubscribeResponse {
    pub fn new(id: u64) -> Self {
        Self { id }
    }
}

impl Serializer for SubscribeResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.id, writer)?;
        Ok(())
    }
}

impl Deserializer for SubscribeResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let id = load!(u64, reader)?;
        Ok(Self { id })
    }
}

///
///  wRPC response for RpcApiOps::Unsubscribe request
///
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsubscribeResponse {}

impl Serializer for UnsubscribeResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)
    }
}

impl Deserializer for UnsubscribeResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader);
        Ok(Self {})
    }
}

#[cfg(test)]
mod palw_producer_facts_wire_tests {
    use super::*;

    fn v4_response() -> GetPalwProducerFactsResponse {
        GetPalwProducerFactsResponse {
            available: true,
            chain_point: "cc".repeat(64),
            daa_score: 1_234,
            class_id: "aa".repeat(64),
            artifact_root: "bb".repeat(64),
            class_target: "340282366920938463463374607431768211455".to_string(),
            pwu: 7_708,
            is_base_class: false,
            min_trace_retention_daa: 9_000,
            epoch_index: 12,
            epoch_budget_blocks: 40,
            epoch_produced_blocks: 3,
            bond_known: true,
            bond_registered_pubkey: "dd".repeat(32),
            bond_operator_id: "ee".repeat(64),
            bond_collateral: 20_000_000_000,
            bond_reserved_exposure: "1000".to_string(),
            bond_exposure_ceiling: "10000000".to_string(),
            bond_claim_exposure: "250".to_string(),
            not_ready_reason: String::new(),
            fp_certified: true,
            fp_quanta_per_canonical_job: 8,
            fp_max_quanta_per_receipt: 64,
            // ADR-0082 Decisions 10/11: `true` so the round trip distinguishes "carried" from
            // "defaulted" — the field's own default is false and a lost field would pass silently.
            fp_decode_rules_armed: true,
            locked_bond_outpoints: vec![format!("{}:0", "aa".repeat(64)), format!("{}:7", "bb".repeat(64))],
            // Version 5 and 6 fields, non-default so a lost field cannot pass as a carried one.
            palw_retention_dir: "/var/lib/misaka/palw".to_string(),
            panel_da_armed: true,
            prompt_ids_merkle: true,
        }
    }

    /// **The borsh wire, both directions.** wRPC carries this response through `Serializer` /
    /// `Deserializer`, and a gateway that reads its class's certification over wRPC gets a
    /// *different* answer from a gRPC one if the two encodings disagree — which is exactly the
    /// failure the version prefix exists to make impossible.
    #[test]
    fn the_producer_facts_survive_the_borsh_round_trip() {
        let response = v4_response();
        let mut bytes = Vec::new();
        Serializer::serialize(&response, &mut bytes).unwrap();
        let back = <GetPalwProducerFactsResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
        assert!(back.fp_certified, "ADR-0077 Decision 3: a gateway cannot name its refusal without this");
        assert_eq!(back.fp_quanta_per_canonical_job, 8, "the lane's price is the chain's, not the gateway's");
        assert_eq!(back.fp_max_quanta_per_receipt, 64);
        assert!(back.fp_decode_rules_armed, "ADR-0082 D10/D11: a builder on the wrong side of this fence is unreproducible");
        assert_eq!(back.locked_bond_outpoints, response.locked_bond_outpoints, "the must-not-spend set must not shorten");
        assert_eq!(back.class_target, response.class_target);
        assert_eq!(back.bond_exposure_ceiling, response.bond_exposure_ceiling);
        assert_eq!(back.not_ready_reason, response.not_ready_reason);
    }

    /// **ADR-0080 design A: a declared close's arrival bitmap survives the wRPC wire.**
    ///
    /// `present` is the field a resume is decided on and it is a BITMAP because chunks arrive in any
    /// order — so an encoding that lost a bit, or that carried a count of parts instead, would send
    /// a mover under a court deadline to re-pay for a carrier the chain already holds and leave the
    /// hole that convicts it. A sparse pattern with a gap in the middle and the top index set is the
    /// one that catches that; a full or empty bitmap round-trips through either shape.
    #[test]
    fn the_declared_close_survives_the_wrpc_round_trip() {
        let response = GetPalwPendingChunkGroupResponse {
            found: true,
            session_id: "5e".repeat(64),
            side: "executor".to_string(),
            count: 27,
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
        let mut bytes = Vec::new();
        Serializer::serialize(&response, &mut bytes).unwrap();
        let back = <GetPalwPendingChunkGroupResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
        assert_eq!(back.present, response.present, "the arrival bitmap did not survive the wire");
        assert_eq!(back.count, 27);
        assert_eq!(back.parts_present, 4);
        assert!(!back.complete);
        assert_eq!(back.side, "executor");
        assert_eq!(back.close_digest, response.close_digest);
        assert_eq!(back.assembly_deadline_daa, 91_408);
        assert_eq!(back.declarer_bond, response.declarer_bond);
        assert_eq!(back.deposit, 33_750_000);
        // The default is the honest negative every non-ConsensusV2 node answers with.
        let mut bytes = Vec::new();
        Serializer::serialize(&GetPalwPendingChunkGroupResponse::default(), &mut bytes).unwrap();
        let back = <GetPalwPendingChunkGroupResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
        assert!(!back.found && back.count == 0 && back.present == 0);
    }

    /// **An older writer is read fail-CLOSED.** A version-2 peer knows nothing about the free-prompt
    /// lane; reading its silence as "certified" would make a gateway submit on a peer's ignorance.
    /// Uncertified and unpriced is the safe reading, and it costs a retry, not a fee.
    #[test]
    fn a_version_two_writer_reads_as_uncertified_and_unpriced() {
        let r = v4_response();
        let mut v2 = Vec::new();
        store!(u16, &2, &mut v2).unwrap();
        store!(bool, &r.available, &mut v2).unwrap();
        store!(String, &r.chain_point, &mut v2).unwrap();
        store!(u64, &r.daa_score, &mut v2).unwrap();
        store!(String, &r.class_id, &mut v2).unwrap();
        store!(String, &r.artifact_root, &mut v2).unwrap();
        store!(String, &r.class_target, &mut v2).unwrap();
        store!(u64, &r.pwu, &mut v2).unwrap();
        store!(bool, &r.is_base_class, &mut v2).unwrap();
        store!(u64, &r.min_trace_retention_daa, &mut v2).unwrap();
        store!(u64, &r.epoch_index, &mut v2).unwrap();
        store!(u64, &r.epoch_budget_blocks, &mut v2).unwrap();
        store!(u64, &r.epoch_produced_blocks, &mut v2).unwrap();
        store!(bool, &r.bond_known, &mut v2).unwrap();
        store!(String, &r.bond_registered_pubkey, &mut v2).unwrap();
        store!(String, &r.bond_operator_id, &mut v2).unwrap();
        store!(u64, &r.bond_collateral, &mut v2).unwrap();
        store!(String, &r.bond_reserved_exposure, &mut v2).unwrap();
        store!(String, &r.bond_exposure_ceiling, &mut v2).unwrap();
        store!(String, &r.bond_claim_exposure, &mut v2).unwrap();
        store!(String, &r.not_ready_reason, &mut v2).unwrap();
        store!(Vec<String>, &r.locked_bond_outpoints, &mut v2).unwrap();

        let back = <GetPalwProducerFactsResponse as Deserializer>::deserialize(&mut v2.as_slice()).unwrap();
        assert_eq!(back.locked_bond_outpoints, r.locked_bond_outpoints, "everything version 2 DID say is read");
        assert!(!back.fp_certified, "silence is not certification");
        assert_eq!((back.fp_quanta_per_canonical_job, back.fp_max_quanta_per_receipt), (0, 0), "and it prices nothing");
        assert!(!back.fp_decode_rules_armed, "and a peer older than the fence is not asserting it is dormant either");
    }

    /// **A version-THREE writer reads as decode-rules-dormant**, and everything version 3 did say
    /// is still read. The suffix property one version on: each version's fields are a strict
    /// prefix of the next, so a new field can never shift an old one.
    #[test]
    fn a_version_three_writer_reads_as_decode_rules_dormant() {
        let r = v4_response();
        let mut v3 = Vec::new();
        store!(u16, &3, &mut v3).unwrap();
        store!(bool, &r.available, &mut v3).unwrap();
        store!(String, &r.chain_point, &mut v3).unwrap();
        store!(u64, &r.daa_score, &mut v3).unwrap();
        store!(String, &r.class_id, &mut v3).unwrap();
        store!(String, &r.artifact_root, &mut v3).unwrap();
        store!(String, &r.class_target, &mut v3).unwrap();
        store!(u64, &r.pwu, &mut v3).unwrap();
        store!(bool, &r.is_base_class, &mut v3).unwrap();
        store!(u64, &r.min_trace_retention_daa, &mut v3).unwrap();
        store!(u64, &r.epoch_index, &mut v3).unwrap();
        store!(u64, &r.epoch_budget_blocks, &mut v3).unwrap();
        store!(u64, &r.epoch_produced_blocks, &mut v3).unwrap();
        store!(bool, &r.bond_known, &mut v3).unwrap();
        store!(String, &r.bond_registered_pubkey, &mut v3).unwrap();
        store!(String, &r.bond_operator_id, &mut v3).unwrap();
        store!(u64, &r.bond_collateral, &mut v3).unwrap();
        store!(String, &r.bond_reserved_exposure, &mut v3).unwrap();
        store!(String, &r.bond_exposure_ceiling, &mut v3).unwrap();
        store!(String, &r.bond_claim_exposure, &mut v3).unwrap();
        store!(String, &r.not_ready_reason, &mut v3).unwrap();
        store!(Vec<String>, &r.locked_bond_outpoints, &mut v3).unwrap();
        store!(bool, &r.fp_certified, &mut v3).unwrap();
        store!(u32, &r.fp_quanta_per_canonical_job, &mut v3).unwrap();
        store!(u32, &r.fp_max_quanta_per_receipt, &mut v3).unwrap();

        let back = <GetPalwProducerFactsResponse as Deserializer>::deserialize(&mut v3.as_slice()).unwrap();
        assert!(back.fp_certified, "everything version 3 DID say is read");
        assert_eq!(back.fp_quanta_per_canonical_job, r.fp_quanta_per_canonical_job);
        assert_eq!(back.fp_max_quanta_per_receipt, r.fp_max_quanta_per_receipt);
        assert!(!back.fp_decode_rules_armed, "a version-3 peer knows nothing about the fence — read fail-closed");
    }
}

/// **ADR-0078 Decision 5's read path, on the borsh wire.** wRPC is the transport the CLI verifier
/// uses, so a field that does not survive this round trip is a field a consumer verifies without.
#[cfg(test)]
mod palw_derived_artifacts_wire_tests {
    use super::*;

    fn a_row() -> RpcPalwDerivedArtifact {
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
        }
    }

    fn a_response() -> GetPalwDerivedArtifactsResponse {
        GetPalwDerivedArtifactsResponse {
            found: true,
            claim_id: "cc".repeat(64),
            output_root: "07".repeat(64),
            executor_pubkey: "ab".repeat(2592),
            executor_bond: format!("{}:3", "b0".repeat(32)),
            class_id: "c1".repeat(64),
            claim_phase: "voided".to_string(),
            claim_void_reason: "receipt_timeout".to_string(),
            claim_accepted_block: "bb".repeat(32),
            claim_accepted_daa: 91_300,
            artifacts: vec![a_row(), RpcPalwDerivedArtifact { transformer_id: "7b".repeat(64), ..a_row() }],
        }
    }

    /// Every field, both directions. The two that would be silently lost are the ones the row does
    /// not repeat: `transformer_id` lives in the state table's KEY, and `output_root` on the CLAIM
    /// — a verifier that received neither would be checking a `dsl_hash` against nothing.
    #[test]
    fn the_derived_rows_survive_the_borsh_round_trip() {
        let response = a_response();
        let mut bytes = Vec::new();
        Serializer::serialize(&response, &mut bytes).unwrap();
        let back = <GetPalwDerivedArtifactsResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
        assert!(back.found);
        assert_eq!(back.output_root, response.output_root, "X6 recomputes against this and nothing else");
        assert_eq!(back.executor_pubkey, response.executor_pubkey, "whose name is on the provenance");
        assert_eq!(back.executor_bond, response.executor_bond);
        assert_eq!(back.claim_phase, "voided", "Decision 4: a derivation of a voided claim says so when read");
        assert_eq!(back.claim_void_reason, "receipt_timeout");
        assert_eq!(back.artifacts.len(), 2);
        assert_eq!(back.artifacts[0].transformer_id, response.artifacts[0].transformer_id, "the key's half of the row");
        assert_eq!(back.artifacts[1].transformer_id, response.artifacts[1].transformer_id);
        assert_eq!(back.artifacts[0].dsl_hash, response.artifacts[0].dsl_hash);
        assert_eq!(back.artifacts[0].artifact_hash, response.artifacts[0].artifact_hash);
        assert_eq!(back.artifacts[0].artifact_bytes, 4_096);
        assert_eq!(back.artifacts[0].kind, 6);
        assert_eq!(back.artifacts[0].derived_id, response.artifacts[0].derived_id);
        assert_eq!(back.artifacts[0].accepted_daa, 91_337);
    }

    /// A claim this chain does not hold reads as `found: false` with nothing filled in — the
    /// answer a node gives about another chain's claim, and not an error.
    #[test]
    fn an_unknown_claim_round_trips_as_not_found() {
        let mut bytes = Vec::new();
        Serializer::serialize(&GetPalwDerivedArtifactsResponse::default(), &mut bytes).unwrap();
        let back = <GetPalwDerivedArtifactsResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
        assert!(!back.found);
        assert!(back.artifacts.is_empty());
        assert!(back.output_root.is_empty(), "no output_root is not the empty hash");
    }

    #[test]
    fn the_claim_facts_survive_the_borsh_round_trip() {
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
        let mut bytes = Vec::new();
        Serializer::serialize(&response, &mut bytes).unwrap();
        let back = <GetPalwFreePromptClaimResponse as Deserializer>::deserialize(&mut bytes.as_slice()).unwrap();
        assert_eq!(back.output_root, response.output_root);
        assert_eq!(back.trace_root, response.trace_root);
        assert_eq!(back.execution_root, response.execution_root);
        assert_eq!((back.quanta, back.quanta_spent), (8, 3));
        assert_eq!(back.work_leaves, 4_194_304);
        assert_eq!(back.work_id, response.work_id);
        assert_eq!(back.phase, "final");
        assert_eq!(back.derived_count, 2);
        assert!(back.is_free_prompt);
    }
}
