//! Op242: bounded pages of the complete authenticated kernel auxiliary table.
use super::*;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwMonetaryMaterialRequest {
    pub block_hash: String,
    pub row: u64,
    pub offset: u64,
    pub limit_bytes: u32,
}
impl Serializer for GetPalwMonetaryMaterialRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.block_hash, writer)?;
        store!(u64, &self.row, writer)?;
        store!(u64, &self.offset, writer)?;
        store!(u32, &self.limit_bytes, writer)?;
        Ok(())
    }
}
impl Deserializer for GetPalwMonetaryMaterialRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported monetary proof request version"));
        }
        Ok(Self {
            block_hash: load!(String, reader)?,
            row: load!(u64, reader)?,
            offset: load!(u64, reader)?,
            limit_bytes: load!(u32, reader)?,
        })
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPalwMonetaryAuxFragment {
    pub row: u64,
    pub table: u8,
    pub key: Vec<u8>,
    pub row_bytes: u64,
    pub offset: u64,
    pub bytes: Vec<u8>,
}
impl Serializer for RpcPalwMonetaryAuxFragment {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.row, writer)?;
        store!(u8, &self.table, writer)?;
        store!(Vec<u8>, &self.key, writer)?;
        store!(u64, &self.row_bytes, writer)?;
        store!(u64, &self.offset, writer)?;
        store!(Vec<u8>, &self.bytes, writer)?;
        Ok(())
    }
}
impl Deserializer for RpcPalwMonetaryAuxFragment {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported monetary fragment version"));
        }
        Ok(Self {
            row: load!(u64, reader)?,
            table: load!(u8, reader)?,
            key: load!(Vec<u8>, reader)?,
            row_bytes: load!(u64, reader)?,
            offset: load!(u64, reader)?,
            bytes: load!(Vec<u8>, reader)?,
        })
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwMonetaryMaterialResponse {
    pub available: bool,
    pub reason: String,
    pub header: Option<RpcHeader>,
    pub parent_header: Option<RpcHeader>,
    pub state_preimage: Vec<u8>,
    pub fork_choice_opening: Vec<u8>,
    /// Version 2: where the serving node's writer began the `kernel-route/v1` block in `state_preimage` (0 = not carried, a
    /// version-1 peer). Never trusted: the client derives the offset from the authenticated preimage and compares.
    pub kernel_route_offset: u64,
    pub aux_root: String,
    pub total_rows: u64,
    pub start_row: u64,
    pub start_offset: u64,
    pub next_row: u64,
    pub next_offset: u64,
    pub fragments: Vec<RpcPalwMonetaryAuxFragment>,
}
impl Serializer for GetPalwMonetaryMaterialResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.reason, writer)?;
        store!(Option<RpcHeader>, &self.header, writer)?;
        store!(Option<RpcHeader>, &self.parent_header, writer)?;
        store!(Vec<u8>, &self.state_preimage, writer)?;
        store!(Vec<u8>, &self.fork_choice_opening, writer)?;
        store!(u64, &self.kernel_route_offset, writer)?;
        store!(String, &self.aux_root, writer)?;
        store!(u64, &self.total_rows, writer)?;
        store!(u64, &self.start_row, writer)?;
        store!(u64, &self.start_offset, writer)?;
        store!(u64, &self.next_row, writer)?;
        store!(u64, &self.next_offset, writer)?;
        serialize!(Vec<RpcPalwMonetaryAuxFragment>, &self.fragments, writer)?;
        Ok(())
    }
}
impl Deserializer for GetPalwMonetaryMaterialResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if !(1..=2).contains(&version) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported monetary proof response version"));
        }
        Ok(Self {
            available: load!(bool, reader)?,
            reason: load!(String, reader)?,
            header: load!(Option<RpcHeader>, reader)?,
            parent_header: load!(Option<RpcHeader>, reader)?,
            state_preimage: load!(Vec<u8>, reader)?,
            fork_choice_opening: load!(Vec<u8>, reader)?,
            kernel_route_offset: if version >= 2 { load!(u64, reader)? } else { 0 },
            aux_root: load!(String, reader)?,
            total_rows: load!(u64, reader)?,
            start_row: load!(u64, reader)?,
            start_offset: load!(u64, reader)?,
            next_row: load!(u64, reader)?,
            next_offset: load!(u64, reader)?,
            fragments: deserialize!(Vec<RpcPalwMonetaryAuxFragment>, reader)?,
        })
    }
}
