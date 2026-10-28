//! **RFC-0012 acceptance, op 246: the node reads the acceptance harness needs** — the DNS security reserve balance a block's
//! post-state holds (`reserve_balance_store`), and virtual UTXO point reads by outpoint (a deposit lock, a synthetic withdrawal
//! outpoint). Read-only; never an approval. Block hashes are 128 hex; outpoints are `txid:index`.

use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwAcceptanceReadsRequest {
    /// The blocks whose DNS security reserve balance is read (at most 64).
    pub reserve_blocks: Vec<String>,
    /// The outpoints read from the virtual UTXO set (`txid:index`, at most 64).
    pub outpoints: Vec<String>,
}

impl Serializer for GetPalwAcceptanceReadsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<String>, &self.reserve_blocks, writer)?;
        store!(Vec<String>, &self.outpoints, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwAcceptanceReadsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown acceptance reads request version"));
        }
        Ok(Self { reserve_blocks: load!(Vec<String>, reader)?, outpoints: load!(Vec<String>, reader)? })
    }
}

/// One block's reserve balance: `known` false where the node holds no reserve row for the block (not a chain block it validated).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwReserveBalance {
    pub block_hash: String,
    pub known: bool,
    pub balance_sompi: u64,
}

impl Serializer for RpcPalwReserveBalance {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.block_hash, writer)?;
        store!(bool, &self.known, writer)?;
        store!(u64, &self.balance_sompi, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwReserveBalance {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { block_hash: load!(String, reader)?, known: load!(bool, reader)?, balance_sompi: load!(u64, reader)? })
    }
}

/// One outpoint of the virtual UTXO set: `present` false where it is unspent nowhere (spent, or never created).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwOutpointEntry {
    pub outpoint: String,
    pub present: bool,
    pub amount: u64,
    /// The script public key: version and script hex.
    pub script_version: u16,
    pub script_hex: String,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

impl Serializer for RpcPalwOutpointEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.outpoint, writer)?;
        store!(bool, &self.present, writer)?;
        store!(u64, &self.amount, writer)?;
        store!(u16, &self.script_version, writer)?;
        store!(String, &self.script_hex, writer)?;
        store!(u64, &self.block_daa_score, writer)?;
        store!(bool, &self.is_coinbase, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcPalwOutpointEntry {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            outpoint: load!(String, reader)?,
            present: load!(bool, reader)?,
            amount: load!(u64, reader)?,
            script_version: load!(u16, reader)?,
            script_hex: load!(String, reader)?,
            block_daa_score: load!(u64, reader)?,
            is_coinbase: load!(bool, reader)?,
        })
    }
}

/// `available` false on a node without the reads (a node built before op 246 answers the default). `sink` / `virtualDaa` are read
/// once with the answers (the caller brackets them with its own generation stamps).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwAcceptanceReadsResponse {
    pub available: bool,
    pub sink: String,
    pub virtual_daa: u64,
    pub reserve_balances: Vec<RpcPalwReserveBalance>,
    pub outpoints: Vec<RpcPalwOutpointEntry>,
}

impl Serializer for GetPalwAcceptanceReadsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.sink, writer)?;
        store!(u64, &self.virtual_daa, writer)?;
        serialize!(Vec<RpcPalwReserveBalance>, &self.reserve_balances, writer)?;
        serialize!(Vec<RpcPalwOutpointEntry>, &self.outpoints, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwAcceptanceReadsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {
            available: load!(bool, reader)?,
            sink: load!(String, reader)?,
            virtual_daa: load!(u64, reader)?,
            reserve_balances: deserialize!(Vec<RpcPalwReserveBalance>, reader)?,
            outpoints: deserialize!(Vec<RpcPalwOutpointEntry>, reader)?,
        })
    }
}
