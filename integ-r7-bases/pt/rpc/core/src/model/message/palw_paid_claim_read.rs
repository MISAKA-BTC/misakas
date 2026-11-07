//! **RFC-0012 acceptance, op 247: one paid claim at the tip** (`kaspa_consensus_core::palw_paid_claim_read_v1`) — the enrollment
//! indexed by its base claim, one round's assigned seats/quorum/accepted receipt signers, and the Final row. Read-only; never an
//! approval, credit or Final authority. Bond keys are `txid:index`; ids are lowercase hex of their 64 bytes.

use super::*;

/// `base_claim`: the base claim id (128 hex digits); `round`: the paid round to report.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPaidClaimRequest {
    pub base_claim: String,
    pub round: u32,
}

impl Serializer for GetPalwPaidClaimRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.base_claim, writer)?;
        store!(u32, &self.round, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPaidClaimRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown paid claim request version"));
        }
        let base_claim = load!(String, reader)?;
        let round = load!(u32, reader)?;
        Ok(Self { base_claim, round })
    }
}

/// `available` false off ConsensusV2 or with no kernel route at the tip; `found` false where no enrollment indexes the base
/// claim. `state` is `final`, `convicted`, `round-complete`, `round-locked`, `round-open` or `enrolled`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwPaidClaimResponse {
    pub available: bool,
    pub found: bool,
    pub sink: String,
    pub tip_daa: u64,
    pub base_claim: String,
    pub enrollment: String,
    pub class_id: String,
    pub job_id: String,
    pub producer_bond: String,
    pub material_consent: String,
    pub state: String,
    pub round: u32,
    pub round_found: bool,
    pub seats: Vec<String>,
    pub quorum: u32,
    pub receipts: Vec<String>,
    /// JSON `final` (the harness's field name; `final` is a reserved word in Rust).
    #[serde(rename = "final")]
    pub final_reached: bool,
    pub final_daa: u64,
}

impl Serializer for GetPalwPaidClaimResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.sink, writer)?;
        store!(u64, &self.tip_daa, writer)?;
        store!(String, &self.base_claim, writer)?;
        store!(String, &self.enrollment, writer)?;
        store!(String, &self.class_id, writer)?;
        store!(String, &self.job_id, writer)?;
        store!(String, &self.producer_bond, writer)?;
        store!(String, &self.material_consent, writer)?;
        store!(String, &self.state, writer)?;
        store!(u32, &self.round, writer)?;
        store!(bool, &self.round_found, writer)?;
        store!(Vec<String>, &self.seats, writer)?;
        store!(u32, &self.quorum, writer)?;
        store!(Vec<String>, &self.receipts, writer)?;
        store!(bool, &self.final_reached, writer)?;
        store!(u64, &self.final_daa, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwPaidClaimResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown paid claim response version"));
        }
        Ok(Self {
            available: load!(bool, reader)?,
            found: load!(bool, reader)?,
            sink: load!(String, reader)?,
            tip_daa: load!(u64, reader)?,
            base_claim: load!(String, reader)?,
            enrollment: load!(String, reader)?,
            class_id: load!(String, reader)?,
            job_id: load!(String, reader)?,
            producer_bond: load!(String, reader)?,
            material_consent: load!(String, reader)?,
            state: load!(String, reader)?,
            round: load!(u32, reader)?,
            round_found: load!(bool, reader)?,
            seats: load!(Vec<String>, reader)?,
            quorum: load!(u32, reader)?,
            receipts: load!(Vec<String>, reader)?,
            final_reached: load!(bool, reader)?,
            final_daa: load!(u64, reader)?,
        })
    }
}
