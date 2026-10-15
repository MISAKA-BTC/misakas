//! **RFC-0016 op 255: the per-model report** — `P_m` and issued rights as TARGETS, physical blocks / rights / Finals / payouts / queue
//! wait / REAL gap / heartbeat dependence as OUTCOMES, refusals as NODE-LOCAL observations (not consensus), and the pre-lottery quote
//! for one class where asked. Read-only. The payload is JSON (`reportJson`); an improvement is never to be read from a target alone.

use super::*;

/// `class_hex`: an optional model class id (lowercase hex of its 64 bytes) to quote one more Attempt for; empty for no quote.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwRfc0016ReportRequest {
    pub class_hex: String,
}

impl Serializer for GetPalwRfc0016ReportRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_hex, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwRfc0016ReportRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown RFC-0016 report request version"));
        }
        let class_hex = load!(String, reader)?;
        Ok(Self { class_hex })
    }
}

/// `available` false off ConsensusV2 or with no PALW tip; `armed` false below both RFC-0016 fences (every shipped preset).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwRfc0016ReportResponse {
    pub available: bool,
    pub armed: bool,
    pub now_daa: u64,
    pub report_json: String,
}

impl Serializer for GetPalwRfc0016ReportResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.armed, writer)?;
        store!(u64, &self.now_daa, writer)?;
        store!(String, &self.report_json, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwRfc0016ReportResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown RFC-0016 report response version"));
        }
        let available = load!(bool, reader)?;
        let armed = load!(bool, reader)?;
        let now_daa = load!(u64, reader)?;
        let report_json = load!(String, reader)?;
        Ok(Self { available, armed, now_daa, report_json })
    }
}
