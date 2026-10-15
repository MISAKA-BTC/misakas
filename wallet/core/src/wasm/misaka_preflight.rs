//! WASM32 bindings for the MISAKA external-wallet preflight (RFC-0012 R12-OPS, 2026-10-15 round 3; unverified source).
//! JSON in, JSON out, so a JavaScript wallet passes the wRPC JSON responses it already holds.
#![allow(non_snake_case)]

use crate::imports::*;
use crate::misaka_preflight::{MISAKA_WALLET_CAPABILITY_SCHEMA_V1, capabilities_from_reads_v1, evaluate_preflight_json_v1};

/// Build the versioned capability from the three node reads (`getPalwNodeStatus`, `getPalwSettlement`,
/// `getDnsConfirmation`) as JSON strings; pass an empty string for a read that failed.
/// @returns The capability JSON (schema `misaka.wallet.capability.v1`).
/// @category Wallet SDK
#[wasm_bindgen(js_name = "misakaCapabilitiesFromReadsV1")]
pub fn js_misaka_capabilities_from_reads_v1(statusJson: String, settlementJson: String, dnsJson: String) -> String {
    fn read<T: serde::de::DeserializeOwned>(json: &str, name: &str) -> std::result::Result<T, String> {
        if json.is_empty() {
            return Err(format!("{name} read failed"));
        }
        serde_json::from_str(json).map_err(|e| format!("{name}: {e}"))
    }
    let status = read::<kaspa_rpc_core::GetPalwNodeStatusResponse>(&statusJson, "getPalwNodeStatus");
    let settlement = read::<kaspa_rpc_core::GetPalwSettlementResponse>(&settlementJson, "getPalwSettlement");
    let dns = read::<kaspa_rpc_core::GetDnsConfirmationResponse>(&dnsJson, "getDnsConfirmation");
    let capabilities = capabilities_from_reads_v1(
        status.as_ref().map_err(Clone::clone),
        settlement.as_ref().map_err(Clone::clone),
        dns.as_ref().map_err(Clone::clone),
    );
    debug_assert_eq!(capabilities.schema, MISAKA_WALLET_CAPABILITY_SCHEMA_V1);
    serde_json::to_string(&capabilities).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

/// The preflight verdict before signing: `{"ok":true,...}` or `{"ok":false,"refusal":{...}}`.
/// `subnetworkIdsJson` is a JSON array of hex subnetwork ids of every transaction about to be signed.
/// @category Wallet SDK
#[wasm_bindgen(js_name = "misakaPreflightV1")]
pub fn js_misaka_preflight_v1(capabilitiesJson: String, expectationsJson: String, subnetworkIdsJson: String) -> String {
    evaluate_preflight_json_v1(&capabilitiesJson, &expectationsJson, &subnetworkIdsJson)
}
