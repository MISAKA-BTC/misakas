use super::RpcRawHeader;
use crate::prelude::{RpcHash, RpcHeader, RpcTransaction};
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

/// Raw Rpc block type - without a cached header hash and without verbose data.
/// Used for mining APIs (get_block_template & submit_block)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcRawBlock {
    pub header: RpcRawHeader,
    pub transactions: Vec<RpcTransaction>,
    /// kaspa-pq EVM Lane v0.4 (§3.1): the block's own EvmExecutionPayload as
    /// its canonical borsh bytes (what `evm_payload_hash` commits to). Empty =
    /// the empty payload. MUST round-trip through get_block_template /
    /// submit_block on an evm-active net.
    pub evm_payload: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcBlock {
    pub header: RpcHeader,
    pub transactions: Vec<RpcTransaction>,
    pub verbose_data: Option<RpcBlockVerboseData>,
    /// kaspa-pq EVM Lane v0.4 (§3.1): the block's own payload (canonical borsh
    /// bytes; empty = the empty payload).
    pub evm_payload: Vec<u8>,
}

impl Serializer for RpcBlock {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        serialize!(RpcHeader, &self.header, writer)?;
        serialize!(Vec<RpcTransaction>, &self.transactions, writer)?;
        serialize!(Option<RpcBlockVerboseData>, &self.verbose_data, writer)?;
        // kaspa-pq EVM Lane v0.4 (serializer v2): the block's own payload bytes.
        store!(Vec<u8>, &self.evm_payload, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcBlock {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let header = deserialize!(RpcHeader, reader)?;
        let transactions = deserialize!(Vec<RpcTransaction>, reader)?;
        let verbose_data = deserialize!(Option<RpcBlockVerboseData>, reader)?;
        // kaspa-pq EVM Lane v0.4: added in serializer v2; older peers ⇒ empty.
        let evm_payload = if version >= 2 { load!(Vec<u8>, reader)? } else { Vec::new() };

        Ok(Self { header, transactions, verbose_data, evm_payload })
    }
}

impl Serializer for RpcRawBlock {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        serialize!(RpcRawHeader, &self.header, writer)?;
        serialize!(Vec<RpcTransaction>, &self.transactions, writer)?;
        // kaspa-pq EVM Lane v0.4 (serializer v2): the block's own payload bytes.
        store!(Vec<u8>, &self.evm_payload, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcRawBlock {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let header = deserialize!(RpcRawHeader, reader)?;
        let transactions = deserialize!(Vec<RpcTransaction>, reader)?;
        // kaspa-pq EVM Lane v0.4: added in serializer v2; older peers ⇒ empty.
        let evm_payload = if version >= 2 { load!(Vec<u8>, reader)? } else { Vec::new() };

        Ok(Self { header, transactions, evm_payload })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcBlockVerboseData {
    pub hash: RpcHash,
    pub difficulty: f64,
    pub selected_parent_hash: RpcHash,
    // PR-9.5c: TransactionId widened to Hash64.
    pub transaction_ids: Vec<kaspa_consensus_core::TransactionId>,
    pub is_header_only: bool,
    pub blue_score: u64,
    pub children_hashes: Vec<RpcHash>,
    pub merge_set_blues_hashes: Vec<RpcHash>,
    pub merge_set_reds_hashes: Vec<RpcHash>,
    pub is_chain_block: bool,
    /// **Lane SCAN (node-only, no consensus rule): the block's class** — `BLUE` (chain block or merged
    /// blue), `EXEC` (an accepted execution-lane round block, algo 10), `RED` (merged red: a round
    /// block that did not hold its permit, or an ordinary red), `ROUND` (a round block whose verdict
    /// the node no longer holds) or empty (not merged yet / a node without this field).
    #[serde(default)]
    pub lane_class: String,
    /// A round block's lineage; `None` on every other block.
    #[serde(default)]
    pub exec: Option<RpcPalwExecBlock>,
}

/// **A round block as the execution lane sees it**: the round and permit it signed for, the bond that
/// produced it and — once the node judged it — the ticket it spent and the claim behind the ticket.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwExecBlock {
    pub round: u64,
    pub permit_index: u16,
    /// The producing bond, `<txid>:<index>`; empty where the envelope did not decode.
    pub bond: String,
    /// `granted` (the node's verdict held the permit), `refused`, or `unknown` (no verdict kept).
    pub verdict: String,
    /// The named reason, on a refusal.
    pub refusal: Option<String>,
    pub claim_id: Option<String>,
    pub class_id: Option<String>,
    /// The ticket's index within its Final's tickets, and the ticket id (zero-length for a lottery permit).
    pub quantum_index: Option<u32>,
    pub quantum_id: Option<String>,
}

impl Serializer for RpcPalwExecBlock {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.round, writer)?;
        store!(u16, &self.permit_index, writer)?;
        store!(String, &self.bond, writer)?;
        store!(String, &self.verdict, writer)?;
        store!(Option<String>, &self.refusal, writer)?;
        store!(Option<String>, &self.claim_id, writer)?;
        store!(Option<String>, &self.class_id, writer)?;
        store!(Option<u32>, &self.quantum_index, writer)?;
        store!(Option<String>, &self.quantum_id, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwExecBlock {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            round: load!(u64, reader)?,
            permit_index: load!(u16, reader)?,
            bond: load!(String, reader)?,
            verdict: load!(String, reader)?,
            refusal: load!(Option<String>, reader)?,
            claim_id: load!(Option<String>, reader)?,
            class_id: load!(Option<String>, reader)?,
            quantum_index: load!(Option<u32>, reader)?,
            quantum_id: load!(Option<String>, reader)?,
        })
    }
}

impl Serializer for RpcBlockVerboseData {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &2, writer)?;
        store!(RpcHash, &self.hash, writer)?;
        store!(f64, &self.difficulty, writer)?;
        store!(RpcHash, &self.selected_parent_hash, writer)?;
        // PR-9.5c: TransactionId widened to Hash64; serialise the
        // Vec accordingly.
        store!(Vec<kaspa_hashes::Hash64>, &self.transaction_ids, writer)?;
        store!(bool, &self.is_header_only, writer)?;
        store!(u64, &self.blue_score, writer)?;
        store!(Vec<RpcHash>, &self.children_hashes, writer)?;
        store!(Vec<RpcHash>, &self.merge_set_blues_hashes, writer)?;
        store!(Vec<RpcHash>, &self.merge_set_reds_hashes, writer)?;
        store!(bool, &self.is_chain_block, writer)?;
        // Version 2 (lane SCAN): the lane class and a round block's lineage. Appended, so a version-1
        // reader stops before them.
        store!(String, &self.lane_class, writer)?;
        serialize!(Option<RpcPalwExecBlock>, &self.exec, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcBlockVerboseData {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u8, reader)?;
        let hash = load!(RpcHash, reader)?;
        let difficulty = load!(f64, reader)?;
        let selected_parent_hash = load!(RpcHash, reader)?;
        // PR-9.5c: TransactionId widened to Hash64.
        let transaction_ids = load!(Vec<kaspa_hashes::Hash64>, reader)?;
        let is_header_only = load!(bool, reader)?;
        let blue_score = load!(u64, reader)?;
        let children_hashes = load!(Vec<RpcHash>, reader)?;
        let merge_set_blues_hashes = load!(Vec<RpcHash>, reader)?;
        let merge_set_reds_hashes = load!(Vec<RpcHash>, reader)?;
        let is_chain_block = load!(bool, reader)?;
        let (lane_class, exec) = if version >= 2 {
            (load!(String, reader)?, deserialize!(Option<RpcPalwExecBlock>, reader)?)
        } else {
            (String::new(), None)
        };

        Ok(Self {
            hash,
            difficulty,
            selected_parent_hash,
            transaction_ids,
            is_header_only,
            blue_score,
            children_hashes,
            merge_set_blues_hashes,
            merge_set_reds_hashes,
            is_chain_block,
            lane_class,
            exec,
        })
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(typescript_custom_section)]
        const TS_BLOCK: &'static str = r#"
        /**
         * Interface defining the structure of a block.
         *
         * @category Consensus
         */
        export interface IBlock {
            header: IHeader;
            transactions: ITransaction[];
            verboseData?: IBlockVerboseData;
        }

        /**
         * Interface defining the structure of a block verbose data.
         *
         * @category Node RPC
         */
        export interface IBlockVerboseData {
            hash: HexString;
            difficulty: number;
            selectedParentHash: HexString;
            transactionIds: HexString[];
            isHeaderOnly: boolean;
            blueScore: number;
            childrenHashes: HexString[];
            mergeSetBluesHashes: HexString[];
            mergeSetRedsHashes: HexString[];
            isChainBlock: boolean;
        }

        /**
         * Interface defining the structure of a raw block.
         *
         * Raw block is a structure used by GetBlockTemplate and SubmitBlock RPCs
         * and differs from `IBlock` in that it does not include verbose data and carries
         * `IRawHeader` that does not include a cached block hash.
         *
         * @category Consensus
         */
        export interface IRawBlock {
            header: IRawHeader;
            transactions: ITransaction[];
        }

        "#;
    }
}
