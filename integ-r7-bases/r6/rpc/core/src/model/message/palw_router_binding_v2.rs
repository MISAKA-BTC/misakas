//! **RFC-0006 consumer v2, op 245: a class's immutable per-reader router binding** (kernel-route aux 139) — what an SDK or a filer
//! needs to assemble a routed-reader accusation (`canonical_input_leaves_routed_v2` with the bound router) exactly as the court will
//! derive it. Read-only. `rowHex` is the lowercase hex of the row's borsh encoding (`PalwRouterBindingRowV2`).

use super::*;

/// `class_hex`: the class id (lowercase hex of its 64 bytes).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwRouterBindingV2Request {
    pub class_hex: String,
}

impl Serializer for GetPalwRouterBindingV2Request {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.class_hex, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwRouterBindingV2Request {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown router binding request version"));
        }
        let class_hex = load!(String, reader)?;
        Ok(Self { class_hex })
    }
}

/// `available` false off ConsensusV2 or with no PALW tip; `armed` false below `palw_memory_routing_court_v2` (every shipped preset);
/// `found` false where the class declared no binding.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwRouterBindingV2Response {
    pub available: bool,
    pub armed: bool,
    pub now_daa: u64,
    pub found: bool,
    pub bound_from_daa: u64,
    pub row_hex: String,
}

impl Serializer for GetPalwRouterBindingV2Response {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.armed, writer)?;
        store!(u64, &self.now_daa, writer)?;
        store!(bool, &self.found, writer)?;
        store!(u64, &self.bound_from_daa, writer)?;
        store!(String, &self.row_hex, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwRouterBindingV2Response {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown router binding response version"));
        }
        let available = load!(bool, reader)?;
        let armed = load!(bool, reader)?;
        let now_daa = load!(u64, reader)?;
        let found = load!(bool, reader)?;
        let bound_from_daa = load!(u64, reader)?;
        let row_hex = load!(String, reader)?;
        Ok(Self { available, armed, now_daa, found, bound_from_daa, row_hex })
    }
}
