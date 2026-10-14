//! Op243: bounded byte pages of the canonical carriage of the PALW state a pinned header commits.
use super::*;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwStateCarriageRequest {
    /// The pinned block (hex). Empty: the node's sink (a client that pins nothing learns nothing it can verify against).
    pub block_hash: String,
    pub offset: u64,
    /// 1024..=1048576.
    pub limit_bytes: u32,
}
impl Serializer for GetPalwStateCarriageRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.block_hash, writer)?;
        store!(u64, &self.offset, writer)?;
        store!(u32, &self.limit_bytes, writer)?;
        Ok(())
    }
}
impl Deserializer for GetPalwStateCarriageRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported state carriage request version"));
        }
        Ok(Self { block_hash: load!(String, reader)?, offset: load!(u64, reader)?, limit_bytes: load!(u32, reader)? })
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwStateCarriageResponse {
    pub available: bool,
    pub reason: String,
    /// The pinned block's header and its selected parent's (the served state is the parent's post-state).
    pub header: Option<RpcHeader>,
    pub parent_header: Option<RpcHeader>,
    /// Borsh `PalwForkChoiceOpeningV1` past `palw_fork_choice_commitment_v1`; empty below it.
    pub fork_choice_opening: Vec<u8>,
    /// The serving node's ADR-0043 state root (informational; the client derives it).
    pub state_root: String,
    pub total_bytes: u64,
    pub offset: u64,
    pub bytes: Vec<u8>,
}
impl Serializer for GetPalwStateCarriageResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(String, &self.reason, writer)?;
        store!(Option<RpcHeader>, &self.header, writer)?;
        store!(Option<RpcHeader>, &self.parent_header, writer)?;
        store!(Vec<u8>, &self.fork_choice_opening, writer)?;
        store!(String, &self.state_root, writer)?;
        store!(u64, &self.total_bytes, writer)?;
        store!(u64, &self.offset, writer)?;
        store!(Vec<u8>, &self.bytes, writer)?;
        Ok(())
    }
}
impl Deserializer for GetPalwStateCarriageResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported state carriage response version"));
        }
        Ok(Self {
            available: load!(bool, reader)?,
            reason: load!(String, reader)?,
            header: load!(Option<RpcHeader>, reader)?,
            parent_header: load!(Option<RpcHeader>, reader)?,
            fork_choice_opening: load!(Vec<u8>, reader)?,
            state_root: load!(String, reader)?,
            total_bytes: load!(u64, reader)?,
            offset: load!(u64, reader)?,
            bytes: load!(Vec<u8>, reader)?,
        })
    }
}

#[cfg(test)]
mod state_carriage_codec_version_tests {
    use super::*;
    #[test]
    fn state_carriage_rejects_unknown_versions_before_reading_fields() {
        let bytes = 2u16.to_le_bytes();
        assert!(<GetPalwStateCarriageRequest as Deserializer>::deserialize(&mut bytes.as_slice()).is_err());
        assert!(<GetPalwStateCarriageResponse as Deserializer>::deserialize(&mut bytes.as_slice()).is_err());
    }
}
