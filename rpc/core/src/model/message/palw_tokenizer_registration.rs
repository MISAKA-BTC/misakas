//! RFC-0051 read-only quote and accepted progress. An unsigned quote neither
//! purchases service nor reserves slots. `observation_json` schema 1 pins the
//! accepted parent separately from the prospective clock; all transports carry it.
use super::*;

pub const TOKENIZER_REGISTRATION_OBSERVATION_MAX_BYTES_V1: usize = 262_144;
pub const TOKENIZER_REGISTRATION_REFUSAL_MAX_BYTES_V1: usize = 8192;
pub const TOKENIZER_REGISTRATION_OPEN_HEX_BYTES_V1: usize =
    kaspa_consensus_core::palw_tokenizer_registration_v1::TOKENIZER_REGISTRATION_OPEN_BODY_BYTES_V1 * 2;
pub const TOKENIZER_REGISTRATION_ID_HEX_BYTES_V1: usize = 128;

pub fn check_tokenizer_registration_text_bound_v1(text: &str, maximum: usize) -> crate::RpcResult<()> {
    if text.len() > maximum {
        return Err(crate::RpcError::General("tokenizer RPC string exceeds its bound".into()));
    }
    Ok(())
}

pub fn check_tokenizer_registration_hex_v1(text: &str, exact: usize) -> crate::RpcResult<()> {
    if text.len() != exact || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(crate::RpcError::General("tokenizer RPC requires exact-length hexadecimal input".into()));
    }
    Ok(())
}

fn bounded_string<R: std::io::Read>(reader: &mut R, maximum: usize) -> std::io::Result<String> {
    let length = load!(u32, reader)? as usize;
    if length > maximum {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "tokenizer RPC string exceeds its bound"));
    }
    let mut bytes = vec![0u8; length];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwTokenizerRegistrationQuoteRequest {
    pub open_body_hex: String,
}
impl Serializer for GetPalwTokenizerRegistrationQuoteRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        if self.open_body_hex.len() > TOKENIZER_REGISTRATION_OPEN_HEX_BYTES_V1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "tokenizer RPC open_body_hex exceeds its bound"));
        }
        store!(u16, &1, writer)?;
        store!(String, &self.open_body_hex, writer)?;
        Ok(())
    }
}
impl Deserializer for GetPalwTokenizerRegistrationQuoteRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown tokenizer RPC version"));
        }
        Ok(Self { open_body_hex: bounded_string(reader, TOKENIZER_REGISTRATION_OPEN_HEX_BYTES_V1)? })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwTokenizerRegistrationQuoteResponse {
    pub available: bool,
    pub quoted: bool,
    pub refusal: String,
    pub observation_json: String,
}
impl Serializer for GetPalwTokenizerRegistrationQuoteResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        if self.refusal.len() > TOKENIZER_REGISTRATION_REFUSAL_MAX_BYTES_V1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "tokenizer RPC refusal exceeds its bound"));
        }
        if self.observation_json.len() > TOKENIZER_REGISTRATION_OBSERVATION_MAX_BYTES_V1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "tokenizer RPC observation_json exceeds its bound"));
        }
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.quoted, writer)?;
        store!(String, &self.refusal, writer)?;
        store!(String, &self.observation_json, writer)?;
        Ok(())
    }
}
impl Deserializer for GetPalwTokenizerRegistrationQuoteResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown tokenizer RPC version"));
        }
        Ok(Self {
            available: load!(bool, reader)?,
            quoted: load!(bool, reader)?,
            refusal: bounded_string(reader, TOKENIZER_REGISTRATION_REFUSAL_MAX_BYTES_V1)?,
            observation_json: bounded_string(reader, TOKENIZER_REGISTRATION_OBSERVATION_MAX_BYTES_V1)?,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwTokenizerRegistrationStatusRequest {
    pub open_id: String,
}
impl Serializer for GetPalwTokenizerRegistrationStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        if self.open_id.len() > TOKENIZER_REGISTRATION_ID_HEX_BYTES_V1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "tokenizer RPC open_id exceeds its bound"));
        }
        store!(u16, &1, writer)?;
        store!(String, &self.open_id, writer)?;
        Ok(())
    }
}
impl Deserializer for GetPalwTokenizerRegistrationStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown tokenizer RPC version"));
        }
        Ok(Self { open_id: bounded_string(reader, TOKENIZER_REGISTRATION_ID_HEX_BYTES_V1)? })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwTokenizerRegistrationStatusResponse {
    pub available: bool,
    pub found: bool,
    pub observation_json: String,
}
impl Serializer for GetPalwTokenizerRegistrationStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        if self.observation_json.len() > TOKENIZER_REGISTRATION_OBSERVATION_MAX_BYTES_V1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "tokenizer RPC observation_json exceeds its bound"));
        }
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.found, writer)?;
        store!(String, &self.observation_json, writer)?;
        Ok(())
    }
}
impl Deserializer for GetPalwTokenizerRegistrationStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        if load!(u16, reader)? != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown tokenizer RPC version"));
        }
        Ok(Self {
            available: load!(bool, reader)?,
            found: load!(bool, reader)?,
            observation_json: bounded_string(reader, TOKENIZER_REGISTRATION_OBSERVATION_MAX_BYTES_V1)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn roundtrip<T: Serializer + Deserializer + PartialEq + std::fmt::Debug>(value: T) {
        let mut bytes = Vec::new();
        Serializer::serialize(&value, &mut bytes).unwrap();
        assert_eq!(<T as Deserializer>::deserialize(&mut &bytes[..]).unwrap(), value);
        bytes[0] = 2;
        assert!(<T as Deserializer>::deserialize(&mut &bytes[..]).is_err());
    }
    #[test]
    fn tokenizer_registration_rpc_roundtrips_and_bounds_before_allocation() {
        roundtrip(GetPalwTokenizerRegistrationQuoteRequest { open_body_hex: "ab".repeat(819) });
        roundtrip(GetPalwTokenizerRegistrationQuoteResponse {
            available: true,
            quoted: true,
            refusal: String::new(),
            observation_json: "{\"schema\":1}".into(),
        });
        roundtrip(GetPalwTokenizerRegistrationStatusRequest { open_id: "cd".repeat(64) });
        roundtrip(GetPalwTokenizerRegistrationStatusResponse {
            available: true,
            found: false,
            observation_json: "{\"schema\":1}".into(),
        });
        let mut bytes = Vec::from(1u16.to_le_bytes());
        bytes.extend(u32::MAX.to_le_bytes());
        assert!(<GetPalwTokenizerRegistrationQuoteRequest as Deserializer>::deserialize(&mut &bytes[..]).is_err());
        assert!(<GetPalwTokenizerRegistrationStatusRequest as Deserializer>::deserialize(&mut &bytes[..]).is_err());
        let mut bytes = Vec::new();
        assert!(
            Serializer::serialize(&GetPalwTokenizerRegistrationQuoteRequest { open_body_hex: "a".repeat(1639) }, &mut bytes).is_err()
        );
        assert!(bytes.is_empty());
    }
}
