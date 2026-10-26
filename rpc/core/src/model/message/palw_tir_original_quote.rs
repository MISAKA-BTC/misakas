//! **G14 op 248: the ORIGINAL TIR frame quote** (`TFP1`, lane FPSEL) — the node's `palw_tir_public_original_quote_v1` on its accepted
//! snapshot, so a producer that is not this node (the free-prompt rail, a node-less miner over public wRPC) signs exactly the pledges
//! the node producer signs (`kaspa_pq_validator_core::palw_tir_public_original_pledges_v1`). Read-only: it reads state and buys
//! nothing. The plan is returned as the lowercase hex of its borsh encoding (`PalwTirPublicAllocationPlanV1`).

use super::*;

/// One original FP commitment and its public execution binding header (`PalwPublicExecutionBindingHeaderV1`), each as the lowercase
/// hex of its borsh encoding — the same request shape as op 244.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwTirOriginalQuoteRequest {
    pub commitment_hex: String,
    pub binding_hex: String,
}

impl Serializer for GetPalwTirOriginalQuoteRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.commitment_hex, writer)?;
        store!(String, &self.binding_hex, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwTirOriginalQuoteRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown TIR original quote request version"));
        }
        let commitment_hex = load!(String, reader)?;
        let binding_hex = load!(String, reader)?;
        Ok(Self { commitment_hex, binding_hex })
    }
}

/// `quoted`: the plan to pledge over (`available` is then true). Not quoted with an empty `refusal`: no frame applies here — off
/// ConsensusV2, a legacy header, below `palw_tir_public_conviction_v1`, or an original the frame route does not admit (job tail,
/// flat class, no judged selection); the producer keeps its other route exactly as the node producer does, and consensus refuses a
/// frameless TIR original past the fence (G-03). `available` with a `refusal`: the node's quote failed, by name.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwTirOriginalQuoteResponse {
    pub available: bool,
    pub quoted: bool,
    pub refusal: String,
    /// The accepted snapshot's DAA the plan was quoted at.
    pub quote_daa: u64,
    /// Borsh hex of `PalwTirPublicAllocationPlanV1`; empty unless `quoted`.
    pub plan_hex: String,
}

impl Serializer for GetPalwTirOriginalQuoteResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.available, writer)?;
        store!(bool, &self.quoted, writer)?;
        store!(String, &self.refusal, writer)?;
        store!(u64, &self.quote_daa, writer)?;
        store!(String, &self.plan_hex, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwTirOriginalQuoteResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown TIR original quote response version"));
        }
        let available = load!(bool, reader)?;
        let quoted = load!(bool, reader)?;
        let refusal = load!(String, reader)?;
        let quote_daa = load!(u64, reader)?;
        let plan_hex = load!(String, reader)?;
        Ok(Self { available, quoted, refusal, quote_daa, plan_hex })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fpsel_op248_messages_round_trip_and_refuse_unknown_versions() {
        let request = GetPalwTirOriginalQuoteRequest { commitment_hex: "ab".into(), binding_hex: "cd".into() };
        let mut bytes = Vec::new();
        Serializer::serialize(&request, &mut bytes).unwrap();
        let back = <GetPalwTirOriginalQuoteRequest as Deserializer>::deserialize(&mut &bytes[..]).unwrap();
        assert_eq!((back.commitment_hex, back.binding_hex), ("ab".to_string(), "cd".to_string()));
        let response =
            GetPalwTirOriginalQuoteResponse { available: true, quoted: true, refusal: String::new(), quote_daa: 17_001, plan_hex: "00ff".into() };
        let mut bytes = Vec::new();
        Serializer::serialize(&response, &mut bytes).unwrap();
        let back = <GetPalwTirOriginalQuoteResponse as Deserializer>::deserialize(&mut &bytes[..]).unwrap();
        assert!(back.available && back.quoted && back.refusal.is_empty());
        assert_eq!((back.quote_daa, back.plan_hex.as_str()), (17_001, "00ff"));
        bytes[0] = 2;
        assert!(<GetPalwTirOriginalQuoteResponse as Deserializer>::deserialize(&mut &bytes[..]).is_err());
    }
}
