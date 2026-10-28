//! **The legacy V2 route's public dispute reads (ops 204–206; lane LG14-A, RFC-0014 §6.3): one parser and one builder per op, from a
//! node's [`ConsensusApi`] to the op's response.**
//!
//! The RPC service's handlers parse the request, check the network is ConsensusV2, and then call these builders and nothing else. The
//! real-node suite (`consensus/src/pipeline/virtual_processor/tests/lg14a_legacy_filer_e2e.rs`) calls the same parsers and builders
//! against a node started after the claim and carries each response through the RPC's JSON wire form. Every request is parsed before a
//! byte of chain state is read: a malformed request is an error on every network, never an absence.

use crate::{
    GetPalwFraudFilerStatusRequest, GetPalwFraudFilerStatusResponse, GetPalwLegacyDisputeRequest, GetPalwLegacyDisputeResponse,
    GetPalwLegacyDisputesRequest, GetPalwLegacyDisputesResponse, RpcError, RpcResult,
};
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwLegacyDisputeObservationV1};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;

/// `getPalwLegacyDisputes`' default page and the most a caller may ask.
pub const PALW_LEGACY_DISPUTES_RPC_DEFAULT: usize = 64;
pub const PALW_LEGACY_DISPUTES_RPC_MAX: usize = 1_024;

fn parse_hash64(text: &str, what: &str) -> RpcResult<Hash64> {
    text.trim().parse::<Hash64>().map_err(|_| RpcError::General(format!("{what} '{text}' is not a 128-hex Hash64")))
}

/// A bond `txid_hex:index` off the wire.
fn parse_bond(text: &str) -> RpcResult<PalwBondKeyV2> {
    let (txid, index) =
        text.trim().split_once(':').ok_or_else(|| RpcError::General(format!("bond '{text}' must be 'txid_hex:index'")))?;
    let index: u32 = index.parse().map_err(|_| RpcError::General(format!("bond '{text}' has a non-numeric index")))?;
    Ok(PalwBondKeyV2(TransactionOutpoint::new(parse_hash64(txid, "bond txid")?, index)))
}

/// The claim op 204 asks about.
pub fn palw_legacy_dispute_request_v1(request: &GetPalwLegacyDisputeRequest) -> RpcResult<Hash64> {
    parse_hash64(&request.claim_id, "claim id")
}

/// Op 204's answer on a ConsensusV2 network: the claim's dispute view as `PalwLegacyDisputeObservationV1`'s JSON.
pub fn palw_legacy_dispute_response_v1(c: &dyn ConsensusApi, claim: Hash64) -> GetPalwLegacyDisputeResponse {
    let Some(view) = c.palw_legacy_dispute_v1(claim) else { return GetPalwLegacyDisputeResponse::default() };
    let observation = PalwLegacyDisputeObservationV1::of(&view);
    GetPalwLegacyDisputeResponse { available: true, observation_version: u32::from(observation.version), json: observation.to_json() }
}

/// Op 205's `(reserver, limit)`.
pub fn palw_legacy_disputes_request_v1(request: &GetPalwLegacyDisputesRequest) -> RpcResult<(Option<PalwBondKeyV2>, usize)> {
    let reserver = if request.reserver.trim().is_empty() { None } else { Some(parse_bond(&request.reserver)?) };
    let limit =
        if request.limit == 0 { PALW_LEGACY_DISPUTES_RPC_DEFAULT } else { (request.limit as usize).min(PALW_LEGACY_DISPUTES_RPC_MAX) };
    Ok((reserver, limit))
}

/// Op 205's answer on a ConsensusV2 network.
pub fn palw_legacy_disputes_response_v1(
    c: &dyn ConsensusApi,
    reserver: Option<PalwBondKeyV2>,
    limit: usize,
) -> GetPalwLegacyDisputesResponse {
    let claims = c.palw_legacy_disputes_v1(reserver, limit);
    GetPalwLegacyDisputesResponse { available: true, claim_ids: claims.iter().map(|claim| claim.to_string()).collect() }
}

/// The bond op 206 asks about.
pub fn palw_fraud_filer_status_request_v1(request: &GetPalwFraudFilerStatusRequest) -> RpcResult<PalwBondKeyV2> {
    parse_bond(&request.bond)
}

/// Op 206's answer on a ConsensusV2 network.
pub fn palw_fraud_filer_status_response_v1(c: &dyn ConsensusApi, bond: PalwBondKeyV2) -> GetPalwFraudFilerStatusResponse {
    let Some(status) = c.palw_fraud_filer_status_v1(bond) else { return GetPalwFraudFilerStatusResponse::default() };
    GetPalwFraudFilerStatusResponse { available: true, observation_version: u32::from(status.version), json: status.to_json() }
}
