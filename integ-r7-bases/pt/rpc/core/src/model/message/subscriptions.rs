//! **Subscriptions & notifications** — moved out of `message.rs` (2026-10-13, a behaviour-preserving split: every item is the same and keeps its path through `pub use subscriptions::*`).

use super::*;

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

macro_rules! palw_notify_command {
    ($req:ident, $resp:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Clone, Debug, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct $req {
            pub command: Command,
        }
        impl $req {
            pub fn new(command: Command) -> Self {
                Self { command }
            }
        }
        impl Serializer for $req {
            fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
                store!(u16, &1, writer)?;
                store!(Command, &self.command, writer)?;
                Ok(())
            }
        }
        impl Deserializer for $req {
            fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
                let _version = load!(u16, reader)?;
                let command = load!(Command, reader)?;
                Ok(Self { command })
            }
        }
        #[derive(Clone, Debug, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct $resp {}
        impl Serializer for $resp {
            fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
                store!(u16, &1, writer)?;
                Ok(())
            }
        }
        impl Deserializer for $resp {
            fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
                let _version = load!(u16, reader)?;
                Ok(Self {})
            }
        }
    };
}

palw_notify_command!(
    NotifyPalwClassReadinessChangedRequest,
    NotifyPalwClassReadinessChangedResponse,
    "Subscribe to class ready-seat / registry-state changes."
);
palw_notify_command!(
    NotifyPalwPanelAssignmentRequest,
    NotifyPalwPanelAssignmentResponse,
    "Subscribe to a claim drawing its S1 panel."
);
palw_notify_command!(
    NotifyPalwPanelReceiptRequest,
    NotifyPalwPanelReceiptResponse,
    "Subscribe to a selected seat's Valid receipt being credited."
);
palw_notify_command!(
    NotifyPalwPanelEligibilityChangedRequest,
    NotifyPalwPanelEligibilityChangedResponse,
    "Subscribe to a bonded seat becoming eligible or losing eligibility, with a reason code."
);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

impl Serializer for PalwClassReadinessChangedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.model_name, writer)?;
        store!(String, &self.registry_state, writer)?;
        store!(String, &self.previous_registry_state, writer)?;
        store!(u32, &self.ready_seats, writer)?;
        store!(u32, &self.previous_ready_seats, writer)?;
        store!(u32, &self.required_ready_seats, writer)?;
        store!(u32, &self.bonded_seats, writer)?;
        Ok(())
    }
}

impl Deserializer for PalwClassReadinessChangedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            class_id: load!(String, reader)?,
            model_name: load!(String, reader)?,
            registry_state: load!(String, reader)?,
            previous_registry_state: load!(String, reader)?,
            ready_seats: load!(u32, reader)?,
            previous_ready_seats: load!(u32, reader)?,
            required_ready_seats: load!(u32, reader)?,
            bonded_seats: load!(u32, reader)?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwPanelAssignmentNotification {
    pub claim_id: String,
    pub class_id: String,
    pub licensed_state: String,
    pub deadline_daa: u64,
    pub coverage_mask: u32,
    pub full_seat: String,
    pub valid_receipt_seats: u32,
    pub selected_panel_seats: u32,
    pub seats: Vec<RpcPalwPanelAssignmentSeat>,
}

impl Serializer for PalwPanelAssignmentNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.licensed_state, writer)?;
        store!(u64, &self.deadline_daa, writer)?;
        store!(u32, &self.coverage_mask, writer)?;
        store!(String, &self.full_seat, writer)?;
        store!(u32, &self.valid_receipt_seats, writer)?;
        store!(u32, &self.selected_panel_seats, writer)?;
        serialize!(Vec<RpcPalwPanelAssignmentSeat>, &self.seats, writer)?;
        Ok(())
    }
}

impl Deserializer for PalwPanelAssignmentNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            claim_id: load!(String, reader)?,
            class_id: load!(String, reader)?,
            licensed_state: load!(String, reader)?,
            deadline_daa: load!(u64, reader)?,
            coverage_mask: load!(u32, reader)?,
            full_seat: load!(String, reader)?,
            valid_receipt_seats: load!(u32, reader)?,
            selected_panel_seats: load!(u32, reader)?,
            seats: deserialize!(Vec<RpcPalwPanelAssignmentSeat>, reader)?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwPanelReceiptNotification {
    pub claim_id: String,
    pub class_id: String,
    pub coverage_mask: u32,
    pub previous_coverage_mask: u32,
    pub valid_receipt_seats: u32,
    pub previous_valid_receipt_seats: u32,
    pub selected_panel_seats: u32,
}

impl Serializer for PalwPanelReceiptNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(u32, &self.coverage_mask, writer)?;
        store!(u32, &self.previous_coverage_mask, writer)?;
        store!(u32, &self.valid_receipt_seats, writer)?;
        store!(u32, &self.previous_valid_receipt_seats, writer)?;
        store!(u32, &self.selected_panel_seats, writer)?;
        Ok(())
    }
}

impl Deserializer for PalwPanelReceiptNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            claim_id: load!(String, reader)?,
            class_id: load!(String, reader)?,
            coverage_mask: load!(u32, reader)?,
            previous_coverage_mask: load!(u32, reader)?,
            valid_receipt_seats: load!(u32, reader)?,
            previous_valid_receipt_seats: load!(u32, reader)?,
            selected_panel_seats: load!(u32, reader)?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwPanelEligibilityChangedNotification {
    pub seat_id: String,
    pub class_id: String,
    pub eligible: bool,
    pub ready: bool,
    pub hold: Option<RpcPalwPanelHoldReason>,
}

impl Serializer for PalwPanelEligibilityChangedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.seat_id, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(bool, &self.eligible, writer)?;
        store!(bool, &self.ready, writer)?;
        serialize!(Option<RpcPalwPanelHoldReason>, &self.hold, writer)?;
        Ok(())
    }
}

impl Deserializer for PalwPanelEligibilityChangedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            seat_id: load!(String, reader)?,
            class_id: load!(String, reader)?,
            eligible: load!(bool, reader)?,
            ready: load!(bool, reader)?,
            hold: deserialize!(Option<RpcPalwPanelHoldReason>, reader)?,
        })
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
