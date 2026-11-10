//! Bounded presentation of authenticated registration reads. No signing or
//! submission occurs here; JSON is identical across the two RPC transports.
use kaspa_consensus_core::{
    api::TokenizerRegistrationReadSnapshotV1,
    palw_state_v2::{PalwBlockContextV2, TokenizerRegistrationStatusReadV1},
    palw_tokenizer_registration_state_v1::TokenizerRegistrationStatusV1,
    palw_tokenizer_registration_v1::TokenizerRegistrationQuoteV1,
};
use kaspa_rpc_core::{RpcError, RpcResult, TOKENIZER_REGISTRATION_OBSERVATION_MAX_BYTES_V1};
use serde_json::{Value, json};

pub(crate) fn decode_exact_hex(text: &str, bytes: usize) -> RpcResult<Vec<u8>> {
    if text.len() != bytes * 2 {
        return Err(RpcError::General(format!("tokenizer request requires exactly {} hex digits", bytes * 2)));
    }
    let mut decoded = vec![0u8; bytes];
    faster_hex::hex_decode(text.as_bytes(), &mut decoded).map_err(|e| RpcError::General(e.to_string()))?;
    Ok(decoded)
}

pub(crate) fn borsh_hex<T: borsh::BorshSerialize>(value: &T) -> RpcResult<String> {
    Ok(faster_hex::hex_string(&borsh::to_vec(value).map_err(|e| RpcError::General(e.to_string()))?))
}

pub(crate) fn point(at: PalwBlockContextV2) -> Value {
    json!({"block": at.block.to_string(), "daa": at.daa_score, "blueScore": at.blue_score})
}

pub(crate) fn snapshot<T>(read: &TokenizerRegistrationReadSnapshotV1<T>) -> Value {
    json!({"schema": 1, "acceptedParent": point(read.parent_point), "acceptedParentRoot": read.parent_root.to_string(),
        "prospectiveDaa": read.quote_daa, "prospectiveBlueScore": read.quote_blue_score})
}

pub(crate) fn quote(q: &TokenizerRegistrationQuoteV1) -> RpcResult<Value> {
    Ok(json!({"quoteHex": borsh_hex(q)?, "registrationChargeSompi": q.charge,
        "rawBytes": q.geometry.raw_bytes, "partCount": q.geometry.part_count,
        "parserScratchBytes": q.parser_scratch_bytes, "finishWorkspaceBytes": q.finish_workspace_bytes,
        "openCarrierBytes": q.open_carrier_bytes, "maximumPartCarrierBytes": q.maximum_part_carrier_bytes,
        "finishCarrierBytes": q.finish_carrier_bytes, "totalCarrierBytes": q.total_carrier_bytes,
        "retainedBytes": q.retained_bytes, "retainedByteDaa": q.retained_byte_daa, "work": q.work}))
}

pub(crate) fn status(s: &TokenizerRegistrationStatusReadV1) -> RpcResult<Value> {
    let (state, terminal, record) = match s.status {
        TokenizerRegistrationStatusV1::Pending => ("pending", None, None),
        TokenizerRegistrationStatusV1::Complete { point, record } => ("complete", Some(point), Some(record.to_string())),
        TokenizerRegistrationStatusV1::Superseded { point } => ("superseded", Some(point), None),
        TokenizerRegistrationStatusV1::Expired { point } => ("expired", Some(point), None),
    };
    Ok(json!({"openId": s.open_id.to_string(), "openBodyHex": borsh_hex(&s.open)?,
        "policyId": s.open.policy.to_string(), "accepted": point(s.accepted), "chargeId": s.charge_id.to_string(),
        "registeredServiceBurnSompi": s.quote.charge, "quote": quote(&s.quote)?, "status": state,
        "terminal": terminal.map(point), "registrationId": record, "releasedAt": s.released_at.map(point),
        "parts": s.parts.iter().map(|p| json!({"index": p.index, "offset": p.offset, "bytes": p.bytes,
            "reservedDaa": p.slot_daa, "received": p.accepted.is_some(), "accepted": p.accepted.map(point)})).collect::<Vec<_>>(),
        "finishes": s.finishes.iter().enumerate().map(|(i,f)| json!({"kind": if i == 0 { "normal" } else { "recovery" },
            "reservedDaa": f.slot_daa, "attempted": f.attempted})).collect::<Vec<_>>()}))
}

pub(crate) fn encode(observation: Value) -> RpcResult<String> {
    let text = serde_json::to_string(&observation).map_err(|e| RpcError::General(e.to_string()))?;
    if text.len() > TOKENIZER_REGISTRATION_OBSERVATION_MAX_BYTES_V1 {
        return Err(RpcError::General("tokenizer observation exceeds its response bound".into()));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tokenizer_registration_rpc_decode_is_exact_and_bounded() {
        assert_eq!(decode_exact_hex("00ab", 2).unwrap(), [0, 171]);
        for bad in ["00", "00ab00", "zzab", "0ab"] {
            assert!(decode_exact_hex(bad, 2).is_err());
        }
        assert!(encode(json!("a".repeat(TOKENIZER_REGISTRATION_OBSERVATION_MAX_BYTES_V1))).is_err());
    }
}
