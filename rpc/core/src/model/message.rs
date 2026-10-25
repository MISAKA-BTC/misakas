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
    /// **Is ADR-0096's decode CONSTRAINT in force at `daa_score`** (version 7) — the fence
    /// `Params::palw_fp_decode_constraint`, answered by `palw_fp_decode_constraint_active_at` at
    /// the same chain point every other fact here was read at. Past the fence a free-prompt job
    /// may carry a `constraint_id` and an entrance may serve `response_format` COMMITTED; before
    /// it — every shipped preset, and every node that does not report it — an entrance serves a
    /// format advisory and says so. Fail-closed like `fp_decode_rules_armed`, for the same
    /// reason: a node that predates the fence cannot be asserting it is dormant.
    pub fp_decode_constraint_armed: bool,
    /// **The form THIS CLASS's jobs commit their prompt ids in** (version 8; ADR-0118 Decision 3):
    /// `true` for the tiled Merkle root. It is `prompt_ids_merkle` for every class but one under
    /// the held regime on a network minted flat, whose jobs commit Merkle ids whatever the
    /// network's form — so a gateway re-binds a worker's result under this one, and reads the pair
    /// to learn that such a class's ids cannot ride a PublicDa carrier (the chain's stateless check
    /// holds no class and reads the network's). A version-7 or older node cannot have a class that
    /// differs, so it reads as `prompt_ids_merkle`.
    pub class_prompt_ids_merkle: bool,
    /// **ADR-0152 P6 (version 9): the one committed ledger** — decimal `u128`,
    /// `PalwProducerBondFactsV2::committed`, the number the fold compares with the ceiling past
    /// `palw_rcore_plus` (below it, the same ledger `bond_reserved_exposure` reports). A reader
    /// that sized room from `bond_reserved_exposure` past the fence saw room the fold refuses. A
    /// version-8 or older node reads as its `bond_reserved_exposure` — the only ledger it has.
    pub bond_committed: String,
    /// **ADR-0152 U2 (version 9): how far the bond's posted collateral is below the producer
    /// floor**, in sompi; `0` when it meets the floor or the fence is not in force. Past the fence
    /// a bond short of it produces nothing (`ProducerBelowFloor` on both lanes) and
    /// `not_ready_reason` says so. An older node reads as `0`: it cannot be asserting a floor.
    pub bond_producer_floor_shortfall: u64,
    /// **ADR-0152 A-6 (version 9): what the bond holds as an accuser** — decimal `u128`, counted
    /// against its collateral beside `bond_committed` by every gate. `"0"` below the fence and
    /// from an older node.
    pub bond_accuser_exposure: String,
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
        // Version 8: `class_prompt_ids_merkle` (ADR-0118 Decision 3, the class's own prompt-ids
        // form). Version 7: `fp_decode_constraint_armed` (ADR-0096 Decision 8's fence). Version 6:
        // `panel_da_armed` and `prompt_ids_merkle` (ADR-0077 D16 / ADR-0081 D3, the
        // private-prompts design of 2026-09-05). Version 5: `palw_retention_dir` (ADR-0084 Decision 5). Version 4 added
        // `fp_decode_rules_armed` (ADR-0082 Decisions 10/11's fence). Version 3 added
        // `fp_certified` and the free-prompt price (ADR-0077 Decision 3). Version 2 added
        // `locked_bond_outpoints` (audit3 H3). Every version is a strict suffix, so an older
        // reader stops where its version ended and this reader tolerates an older writer by
        // leaving the later fields at their defaults — additive, never re-ordered.
        store!(u16, &9, writer)?;
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
        store!(bool, &self.fp_decode_constraint_armed, writer)?;
        store!(bool, &self.class_prompt_ids_merkle, writer)?;
        store!(String, &self.bond_committed, writer)?;
        store!(u64, &self.bond_producer_floor_shortfall, writer)?;
        store!(String, &self.bond_accuser_exposure, writer)?;
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
        // Version 7 (ADR-0096 Decision 8): fail closed — an older node cannot be asserting the
        // constraint fence is in force, so an entrance reads "unknown" as "advisory only".
        let fp_decode_constraint_armed = if version >= 7 { load!(bool, reader)? } else { false };
        // Version 8 (ADR-0118 Decision 3): an older node cannot hold a class whose form differs from
        // the network's (it refuses the held fence past genesis), so its class form IS the network
        // form it reported — the one reading that neither invents a difference nor hides one.
        let class_prompt_ids_merkle = if version >= 8 { load!(bool, reader)? } else { prompt_ids_merkle };
        // Version 9 (ADR-0152 P6): an older node has one ledger, the one it reported; it cannot be
        // asserting a producer floor or an accuser ledger.
        let (bond_committed, bond_producer_floor_shortfall, bond_accuser_exposure) = if version >= 9 {
            (load!(String, reader)?, load!(u64, reader)?, load!(String, reader)?)
        } else {
            (bond_reserved_exposure.clone(), 0, "0".to_string())
        };
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
            fp_decode_constraint_armed,
            class_prompt_ids_merkle,
            bond_committed,
            bond_producer_floor_shortfall,
            bond_accuser_exposure,
        })
    }
}

mod palw_consumer_read;
pub use palw_consumer_read::*;

mod palw_court_close;
pub use palw_court_close::*;

/// **`getPalwModelPositions` version 2 on the borsh wire** (the 2026-09-23 Position route matrix,
/// P-B2): the tenure and the tier a provider serves a line's holders on. wRPC is the transport a
/// gateway reads, so a field lost here is a membership served on units alone — and an older peer
/// on either side of the wire must neither break nor be read as granting anything.
#[cfg(test)]
mod palw_model_positions_wire_tests;

mod palw_model_registry;
pub use palw_model_registry::*;

mod palw_operator_reads;
pub use palw_operator_reads::*;

mod palw_vesting;
pub use palw_vesting::*;

#[cfg(test)]
mod palw_node_status_wire_tests;

mod subscriptions;
pub use subscriptions::*;

/// **ADR-0078 Decision 5's read path, on the borsh wire.** wRPC is the transport the CLI verifier
/// uses, so a field that does not survive this round trip is a field a consumer verifies without.
#[cfg(test)]
mod palw_derived_artifacts_wire_tests;
#[cfg(test)]
mod palw_producer_facts_wire_tests;

mod palw_activation_pool;
pub use palw_activation_pool::*;

mod palw_capacity_shadow;
pub use palw_capacity_shadow::*;

mod palw_legacy_dispute_reads;
pub use palw_legacy_dispute_reads::*;

mod palw_kernel_route_reads;
pub use palw_kernel_route_reads::*;

#[cfg(test)]
mod palw_model_market_wire_tests;
/// **ADR-0152 P2-10 (T51's wire half): claim row v3 and `getPalwClaims` v2 against older peers.**
#[cfg(test)]
mod palw_vesting_wire_tests;

#[cfg(test)]
mod reward_vesting_codec_version_tests {
    use super::*;
    #[test]
    fn reward_vesting_rejects_unknown_versions_before_reading_fields() {
        let bytes = 2u16.to_le_bytes();
        assert!(<GetPalwRewardVestingRequest as Deserializer>::deserialize(&mut bytes.as_slice()).is_err());
        assert!(<RpcPalwExecRewardLeg as Deserializer>::deserialize(&mut bytes.as_slice()).is_err());
        assert!(<GetPalwRewardVestingResponse as Deserializer>::deserialize(&mut bytes.as_slice()).is_err());
    }
}

mod palw_monetary_material;
pub use palw_monetary_material::*;
mod palw_state_carriage;
pub use palw_state_carriage::*;
