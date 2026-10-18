//! RFC12 read-only reserve / outpoint observation (op 214).
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetRfc12StateRequest {
    /// Empty selects the current virtual selected parent; otherwise a 128-hex block hash.
    pub block_hash: String,
    /// At most 64 `transaction_id:index` values. A missing UTXO is returned as null.
    pub outpoints: Vec<String>,
}
impl Serializer for GetRfc12StateRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.block_hash, writer)?;
        store!(Vec<String>, &self.outpoints, writer)?;
        Ok(())
    }
}
impl Deserializer for GetRfc12StateRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown RFC12 state request version"));
        }
        Ok(Self { block_hash: load!(String, reader)?, outpoints: load!(Vec<String>, reader)? })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetRfc12StateResponse {
    /// Version-1 JSON. Reserve null means unavailable, never a known zero.
    /// UTXOs are point reads under the same virtual generation lock.
    pub snapshot_json: String,
}
impl Serializer for GetRfc12StateResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.snapshot_json, writer)?;
        Ok(())
    }
}
impl Deserializer for GetRfc12StateResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown RFC12 state response version"));
        }
        Ok(Self { snapshot_json: load!(String, reader)? })
    }
}
