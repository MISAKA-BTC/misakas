//! **Lane LG14-A — the legacy route's public dispute reads (ops 204–206; RFC-0014 §6.3)** — moved out of `message.rs` (2026-10-13, a behaviour-preserving split: every item is the same and keeps its path through `pub use palw_legacy_dispute_reads::*`).

use super::*;

// ---------------------------------------------------------------------------------------------------------------------------
// Lane LG14-A — the legacy route's public dispute reads (ops 204–206; RFC-0014 §6.3)
// ---------------------------------------------------------------------------------------------------------------------------

/// **`getPalwLegacyDispute` (op 204)**: one claim's dispute view — what any verifier needs to pursue it with no operator allowlist:
/// roots, retention, DL-1's deadline and the hard deadline, the reservations, the open DA sessions and their units, the answered units,
/// the open courts and the recorded outcomes. **Op 204 is new: a node built before it drops the WebSocket on it.**
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwLegacyDisputeRequest {
    /// The claim id, 128 hex.
    pub claim_id: String,
}

impl Serializer for GetPalwLegacyDisputeRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_id, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwLegacyDisputeRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { claim_id: load!(String, reader)? })
    }
}

/// **The `getPalwLegacyDispute` answer.** `json` is the camelCase serde form of the node's `PalwLegacyDisputeObservationV1`, whose own
/// `version` field is `observation_version` (fields are only ever appended). `available` false off `ConsensusV2`, before PALW state
/// exists, or for a claim the tip does not hold.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwLegacyDisputeResponse {
    pub available: bool,
    pub observation_version: u32,
    pub json: String,
}

impl Serializer for GetPalwLegacyDisputeResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u32, &self.observation_version, writer)?;
        store!(String, &self.json, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwLegacyDisputeResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { available: load!(bool, reader)?, observation_version: load!(u32, reader)?, json: load!(String, reader)? })
    }
}

/// **`getPalwLegacyDisputes` (op 205)**: the claims a live reservation holds at the tip, in claim order — every one, or `reserver`'s.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwLegacyDisputesRequest {
    /// A bond `txid_hex:index`, or empty for every reserver.
    pub reserver: String,
    /// At most this many claims; 0 asks for the node's default (64); the node's cap is 1,024.
    pub limit: u32,
}

impl Serializer for GetPalwLegacyDisputesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.reserver, writer)?;
        store!(u32, &self.limit, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwLegacyDisputesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { reserver: load!(String, reader)?, limit: load!(u32, reader)? })
    }
}

/// **The `getPalwLegacyDisputes` answer**: claim ids, 128 hex each.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwLegacyDisputesResponse {
    pub available: bool,
    pub claim_ids: Vec<String>,
}

impl Serializer for GetPalwLegacyDisputesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(Vec<String>, &self.claim_ids, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwLegacyDisputesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { available: load!(bool, reader)?, claim_ids: load!(Vec<String>, reader)? })
    }
}

/// **`getPalwFraudFilerStatus` (op 206)**: one bond's standing as a filer — the fence, its live reservations, the deposits held on it
/// and the exposure they put on its free half.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwFraudFilerStatusRequest {
    /// A bond `txid_hex:index`.
    pub bond: String,
}

impl Serializer for GetPalwFraudFilerStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwFraudFilerStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { bond: load!(String, reader)? })
    }
}

/// **The `getPalwFraudFilerStatus` answer.** `json` is the camelCase serde form of the node's `PalwFraudFilerStatusV1`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwFraudFilerStatusResponse {
    pub available: bool,
    pub observation_version: u32,
    pub json: String,
}

impl Serializer for GetPalwFraudFilerStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(u32, &self.observation_version, writer)?;
        store!(String, &self.json, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwFraudFilerStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self { available: load!(bool, reader)?, observation_version: load!(u32, reader)?, json: load!(String, reader)? })
    }
}
