//! Node-local committed court outcomes. Missing/evicted/restarted observations are unavailable;
//! this endpoint grants no consensus credit and is not a computation proof.
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwMemoryRoutingOutcomeV1Request {
    pub block_hash: String,
    pub claim_id: String,
}
impl Serializer for GetPalwMemoryRoutingOutcomeV1Request {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.block_hash, writer)?;
        store!(String, &self.claim_id, writer)
    }
}
impl Deserializer for GetPalwMemoryRoutingOutcomeV1Request {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown court outcome request version"));
        }
        Ok(Self { block_hash: load!(String, reader)?, claim_id: load!(String, reader)? })
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwMemoryRoutingOutcomeV1Response {
    pub available: bool,
    pub observation_json: String,
    pub reason: String,
}
impl Serializer for GetPalwMemoryRoutingOutcomeV1Response {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.observation_json, writer)?;
        store!(String, &self.reason, writer)
    }
}
impl Deserializer for GetPalwMemoryRoutingOutcomeV1Response {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown court outcome response version"));
        }
        Ok(Self { available: load!(bool, reader)?, observation_json: load!(String, reader)?, reason: load!(String, reader)? })
    }
}
