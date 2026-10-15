//! **MISAKA external-wallet preflight v1** (RFC-0012 R12-OPS, 2026-10-15 round 3; unverified source — nothing here has been
//! compiled or run).
//!
//! Third-party wallets, SDKs and EVM tooling that sign MISAKA transactions call [`preflight`] (or, holding the three node
//! reads already, [`evaluate_preflight_v1`]) before choosing funds or signing. It is the same verdict the first-party CLI
//! reaches (`misaka/apps/cli/src/wallet.rs` `node_import_verdict`, `validator_reader` finality authority, the R12-OPS
//! retired-kind rule), as one versioned, serializable surface:
//!
//! * **Capability** ([`MisakaNodeCapabilitiesV1`], schema [`MISAKA_WALLET_CAPABILITY_SCHEMA_V1`]): what the node reports —
//!   genesis, durable import status, DNS retirement height, native settlement availability, DNS authority availability and
//!   the resulting finality authority (`native` / `dns` / `unknown`). Built from `getPalwNodeStatus`, `getPalwSettlement(0)`
//!   and `getDnsConfirmation`; a failed read is recorded, never guessed.
//! * **Refusals** ([`MisakaPreflightRefusalV1`]): wrong genesis; an incomplete pruning import; a native-retirement chain whose
//!   node does not report import status; an unknown finality authority; and any transaction of a retired kind (new DNS
//!   bonds `0x10`, attestation shards `0x11`, precommits `0x19`) once retirement is active or scheduled within
//!   [`MISAKA_RETIREMENT_SIGNING_MARGIN_DAA_V1`] of the node's sink.
//!
//! A pass is a precondition for signing, never a statement that a payment is settled: settlement depth stays
//! `getPalwSettlement(daa)`'s.
use kaspa_consensus_core::{config::params::Params, subnets::SubnetworkId, tx::Transaction};
use kaspa_rpc_core::{GetDnsConfirmationResponse, GetPalwNodeStatusResponse, GetPalwSettlementResponse, api::rpc::RpcApi};
use serde::{Deserialize, Serialize};

pub const MISAKA_WALLET_CAPABILITY_VERSION_V1: u16 = 1;
pub const MISAKA_WALLET_CAPABILITY_SCHEMA_V1: &str = "misaka.wallet.capability.v1";
/// A transaction signed now may be included later: retired kinds are refused this many DAA before the scheduled fence.
pub const MISAKA_RETIREMENT_SIGNING_MARGIN_DAA_V1: u64 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MisakaFinalityAuthorityV1 {
    /// The node reports DNS retirement and a native settlement read.
    Native,
    /// The node's DNS finality overlay answered.
    Dns,
    /// Neither answered: nothing is signed.
    Unknown,
}

/// **What a node reports to a wallet**, versioned. Every field is the node's word; [`evaluate_preflight_v1`] judges it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MisakaNodeCapabilitiesV1 {
    pub schema: String,
    pub version: u16,
    pub genesis_hash: Option<String>,
    pub drill_salt_id: Option<String>,
    pub consensus_params_id: Option<String>,
    /// The node reports the durable import status fields at all.
    pub import_status_reported: bool,
    pub import_stable: bool,
    pub import_phase: String,
    pub import_point: String,
    pub dns_retired_at: Option<u64>,
    pub native_settlement_available: bool,
    pub dns_authority_available: bool,
    pub finality_authority: MisakaFinalityAuthorityV1,
    pub sink_daa: Option<u64>,
    /// The reads that failed, by name.
    pub read_errors: Vec<String>,
}

/// **What the wallet itself knows about the chain it signs for** (from its own params, never from the node).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MisakaWalletExpectationsV1 {
    /// Refuse a node on another genesis (hex). `None`: not checked (the wallet then owns that risk).
    pub expected_genesis_hash: Option<String>,
    /// The DAA the wallet's params schedule DNS retirement at (`Params::palw_dns_retirement`), if any.
    pub retirement_daa: Option<u64>,
}

impl MisakaWalletExpectationsV1 {
    /// From the wallet's own chain params (`Params::from(network)`, or a drill's).
    pub fn for_params(params: &Params) -> Self {
        let retirement_daa = params
            .palw_dns_retirement
            .as_ref()
            .filter(|r| r.activation != kaspa_consensus_core::config::params::ForkActivation::never())
            .map(|r| r.activation.daa_score());
        Self { expected_genesis_hash: Some(params.genesis.hash.to_string()), retirement_daa }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "code")]
pub enum MisakaPreflightRefusalV1 {
    InvalidCapabilities { reason: String },
    GenesisMismatch { expected: String, reported: Option<String> },
    ImportStatusMissing,
    ImportIncomplete { phase: String, point: String },
    FinalityAuthorityUnknown { read_errors: Vec<String> },
    RetiredKind { subnetwork_id: String, retired_at: Option<u64>, scheduled_at: Option<u64> },
}

impl std::fmt::Display for MisakaPreflightRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCapabilities { reason } => write!(f, "invalid wallet capability snapshot: {reason}"),
            Self::GenesisMismatch { expected, reported } => {
                write!(f, "the node runs genesis {reported:?}, not the expected {expected}; nothing is signed for it")
            }
            Self::ImportStatusMissing => {
                write!(f, "the node does not report the durable import status required for native settlement")
            }
            Self::ImportIncomplete { phase, point } => write!(
                f,
                "the node is importing its pruning snapshot (phase {phase}, point {point}); wait before choosing funds or signing"
            ),
            Self::FinalityAuthorityUnknown { read_errors } => {
                write!(f, "the node's finality authority is unknown (settlement and DNS reads failed: {read_errors:?})")
            }
            Self::RetiredKind { subnetwork_id, retired_at, scheduled_at } => write!(
                f,
                "subnetwork {subnetwork_id} creates retired DNS participation (retired at {retired_at:?}, scheduled at \
                 {scheduled_at:?}); bond exit/unbond remain available"
            ),
        }
    }
}

/// A passing preflight: the authority the wallet may show next to its balance, and the capability it was judged on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MisakaPreflightOkV1 {
    pub finality_authority: MisakaFinalityAuthorityV1,
    pub capabilities: MisakaNodeCapabilitiesV1,
}

/// **The capability from the three node reads**, each `Err(reason)` when it failed.
pub fn capabilities_from_reads_v1(
    status: Result<&GetPalwNodeStatusResponse, String>,
    settlement: Result<&GetPalwSettlementResponse, String>,
    dns: Result<&GetDnsConfirmationResponse, String>,
) -> MisakaNodeCapabilitiesV1 {
    let mut read_errors = Vec::new();
    let nonempty = |s: &str| (!s.is_empty()).then(|| s.to_owned());
    let (genesis_hash, drill_salt_id, consensus_params_id, import) = match status {
        Ok(s) => (
            nonempty(&s.genesis_hash),
            nonempty(&s.drill_salt_id),
            nonempty(&s.consensus_params_id),
            Some((
                s.import_session_available,
                s.pruning_import_stable,
                s.import_session_phase.clone(),
                s.import_session_point.clone(),
            )),
        ),
        Err(e) => {
            read_errors.push(format!("getPalwNodeStatus: {e}"));
            (None, None, None, None)
        }
    };
    let (import_status_reported, import_stable, import_phase, import_point) =
        import.unwrap_or((false, false, String::new(), String::new()));
    let (dns_retired_at, native_settlement_available, sink_daa) = match settlement {
        Ok(r) => (r.dns_retired_at, r.native_settlement.is_some(), Some(r.sink_daa)),
        Err(e) => {
            read_errors.push(format!("getPalwSettlement: {e}"));
            (None, false, None)
        }
    };
    let dns_authority_available = match dns {
        Ok(d) => d.available,
        Err(e) => {
            read_errors.push(format!("getDnsConfirmation: {e}"));
            false
        }
    };
    let finality_authority = if dns_retired_at.is_some() {
        MisakaFinalityAuthorityV1::Native
    } else if dns_authority_available {
        MisakaFinalityAuthorityV1::Dns
    } else {
        MisakaFinalityAuthorityV1::Unknown
    };
    MisakaNodeCapabilitiesV1 {
        schema: MISAKA_WALLET_CAPABILITY_SCHEMA_V1.to_owned(),
        version: MISAKA_WALLET_CAPABILITY_VERSION_V1,
        genesis_hash,
        drill_salt_id,
        consensus_params_id,
        import_status_reported,
        import_stable,
        import_phase,
        import_point,
        dns_retired_at,
        native_settlement_available,
        dns_authority_available,
        finality_authority,
        sink_daa,
        read_errors,
    }
}

/// Reads the three node facts through any `RpcApi` (wRPC, gRPC or in-process).
pub async fn fetch_capabilities_v1(rpc: &dyn RpcApi) -> MisakaNodeCapabilitiesV1 {
    let status = rpc.get_palw_node_status().await.map_err(|e| e.to_string());
    let settlement = rpc.get_palw_settlement(0).await.map_err(|e| e.to_string());
    let dns = rpc.get_dns_confirmation().await.map_err(|e| e.to_string());
    capabilities_from_reads_v1(
        status.as_ref().map_err(Clone::clone),
        settlement.as_ref().map_err(Clone::clone),
        dns.as_ref().map_err(Clone::clone),
    )
}

/// **The verdict.** `subnetworks` are those of every transaction about to be signed.
pub fn evaluate_preflight_v1(
    capabilities: &MisakaNodeCapabilitiesV1,
    expectations: &MisakaWalletExpectationsV1,
    subnetworks: &[SubnetworkId],
) -> Result<MisakaPreflightOkV1, MisakaPreflightRefusalV1> {
    // Native callers and JSON/WASM callers must enforce the same version and
    // facts. The serialized display enum is not a fourth authority read.
    if capabilities.schema != MISAKA_WALLET_CAPABILITY_SCHEMA_V1 || capabilities.version != MISAKA_WALLET_CAPABILITY_VERSION_V1 {
        return Err(MisakaPreflightRefusalV1::InvalidCapabilities { reason: "unknown schema/version".into() });
    }
    let authority = if capabilities.dns_retired_at.is_some() {
        MisakaFinalityAuthorityV1::Native
    } else if capabilities.dns_authority_available {
        MisakaFinalityAuthorityV1::Dns
    } else {
        MisakaFinalityAuthorityV1::Unknown
    };
    if capabilities.finality_authority != authority {
        return Err(MisakaPreflightRefusalV1::InvalidCapabilities {
            reason: "reported finality authority contradicts the capability facts".into(),
        });
    }
    if let Some(expected) = &expectations.expected_genesis_hash
        && capabilities.genesis_hash.as_deref().is_none_or(|reported| !reported.eq_ignore_ascii_case(expected))
    {
        return Err(MisakaPreflightRefusalV1::GenesisMismatch {
            expected: expected.clone(),
            reported: capabilities.genesis_hash.clone(),
        });
    }
    let native_chain = expectations.retirement_daa.is_some() || capabilities.dns_retired_at.is_some();
    if !capabilities.import_status_reported {
        if native_chain {
            return Err(MisakaPreflightRefusalV1::ImportStatusMissing);
        }
    } else if !capabilities.import_stable
        || !capabilities.import_point.is_empty()
        || !matches!(capabilities.import_phase.as_str(), "idle" | "legacy-stable")
    {
        return Err(MisakaPreflightRefusalV1::ImportIncomplete {
            phase: capabilities.import_phase.clone(),
            point: capabilities.import_point.clone(),
        });
    }
    if capabilities.finality_authority == MisakaFinalityAuthorityV1::Unknown {
        return Err(MisakaPreflightRefusalV1::FinalityAuthorityUnknown { read_errors: capabilities.read_errors.clone() });
    }
    let retired_now = capabilities.dns_retired_at.is_some();
    let retiring_soon = match (expectations.retirement_daa, capabilities.sink_daa) {
        (Some(at), Some(sink)) => sink.saturating_add(MISAKA_RETIREMENT_SIGNING_MARGIN_DAA_V1) >= at,
        // Scheduled but the node did not say where it is: treat as imminent.
        (Some(_), None) => true,
        (None, _) => false,
    };
    if (retired_now || retiring_soon)
        && let Some(id) = subnetworks.iter().find(|id| kaspa_consensus_core::palw_native_settlement_v1::creates_dns_participation(id))
    {
        return Err(MisakaPreflightRefusalV1::RetiredKind {
            subnetwork_id: id.to_string(),
            retired_at: capabilities.dns_retired_at,
            scheduled_at: expectations.retirement_daa,
        });
    }
    Ok(MisakaPreflightOkV1 { finality_authority: capabilities.finality_authority, capabilities: capabilities.clone() })
}

/// **The one call an external wallet makes before signing** `transactions`.
pub async fn preflight(
    rpc: &dyn RpcApi,
    expectations: &MisakaWalletExpectationsV1,
    transactions: &[Transaction],
) -> Result<MisakaPreflightOkV1, MisakaPreflightRefusalV1> {
    let before = rpc.get_palw_node_status().await.map_err(|e| e.to_string());
    let settlement = rpc.get_palw_settlement(0).await.map_err(|e| e.to_string());
    let dns = rpc.get_dns_confirmation().await.map_err(|e| e.to_string());
    let after = rpc.get_palw_node_status().await.map_err(|e| e.to_string());
    check_read_bracket_v1(&before, &after)?;
    let capabilities = capabilities_from_reads_v1(
        before.as_ref().map_err(Clone::clone),
        settlement.as_ref().map_err(Clone::clone),
        dns.as_ref().map_err(Clone::clone),
    );
    let subnetworks: Vec<SubnetworkId> = transactions.iter().map(|tx| tx.subnetwork_id.clone()).collect();
    evaluate_preflight_v1(&capabilities, expectations, &subnetworks)
}

fn check_read_bracket_v1(
    before: &Result<GetPalwNodeStatusResponse, String>,
    after: &Result<GetPalwNodeStatusResponse, String>,
) -> Result<(), MisakaPreflightRefusalV1> {
    let stable = match (before, after) {
        (Ok(a), Ok(b)) => {
            a.genesis_hash == b.genesis_hash
                && a.drill_salt_id == b.drill_salt_id
                && a.consensus_params_id == b.consensus_params_id
                && a.consensus_schedule_id == b.consensus_schedule_id
                && a.import_session_available == b.import_session_available
                && a.import_session_epoch == b.import_session_epoch
                && a.import_session_point == b.import_session_point
                && a.import_session_phase == b.import_session_phase
                && a.pruning_import_stable == b.pruning_import_stable
        }
        // Preserve the dormant legacy capability rule. The normal evaluator
        // still rejects missing genesis/import status whenever the caller's
        // expectations require it; no native import is certified by this arm.
        (Err(_), Err(_)) => true,
        _ => false,
    };
    if stable {
        Ok(())
    } else {
        Err(MisakaPreflightRefusalV1::InvalidCapabilities {
            reason: "node identity or durable import epoch changed during the capability reads; retry before signing".into(),
        })
    }
}

/// JSON form for bindings (wasm, FFI): `{"ok":true,...}` or `{"ok":false,"refusal":{"code":...}}`. Malformed input refuses.
pub fn evaluate_preflight_json_v1(capabilities: &str, expectations: &str, subnetwork_ids: &str) -> String {
    let parsed = (|| -> Result<_, String> {
        let capabilities: MisakaNodeCapabilitiesV1 = serde_json::from_str(capabilities).map_err(|e| format!("capabilities: {e}"))?;
        if capabilities.schema != MISAKA_WALLET_CAPABILITY_SCHEMA_V1 || capabilities.version != MISAKA_WALLET_CAPABILITY_VERSION_V1 {
            return Err("capabilities: unknown schema/version".into());
        }
        let expectations: MisakaWalletExpectationsV1 = serde_json::from_str(expectations).map_err(|e| format!("expectations: {e}"))?;
        let ids: Vec<String> = serde_json::from_str(subnetwork_ids).map_err(|e| format!("subnetworkIds: {e}"))?;
        let ids = ids
            .iter()
            .map(|id| id.parse::<SubnetworkId>().map_err(|e| format!("subnetworkIds: {id}: {e}")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((capabilities, expectations, ids))
    })();
    let value = match parsed {
        Err(error) => serde_json::json!({"ok": false, "error": error}),
        Ok((capabilities, expectations, ids)) => match evaluate_preflight_v1(&capabilities, &expectations, &ids) {
            Ok(ok) => serde_json::json!({"ok": true, "result": ok}),
            Err(refusal) => serde_json::json!({"ok": false, "refusal": refusal, "reason": refusal.to_string()}),
        },
    };
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(phase: &str, stable: bool) -> GetPalwNodeStatusResponse {
        GetPalwNodeStatusResponse {
            genesis_hash: "aa".repeat(64),
            import_session_available: true,
            import_session_phase: phase.into(),
            pruning_import_stable: stable,
            ..Default::default()
        }
    }
    fn settlement(retired: Option<u64>) -> GetPalwSettlementResponse {
        GetPalwSettlementResponse { dns_retired_at: retired, sink_daa: 5_000, available: true, ..Default::default() }
    }
    fn expect(retirement: Option<u64>) -> MisakaWalletExpectationsV1 {
        MisakaWalletExpectationsV1 { expected_genesis_hash: Some("AA".repeat(64)), retirement_daa: retirement }
    }

    #[test]
    fn a_completed_import_between_stable_reads_is_not_the_same_signing_view() {
        let mut before = status("idle", true);
        before.import_session_epoch = 7;
        let mut after = before.clone();
        after.produced_blocks += 1;
        assert!(check_read_bracket_v1(&Ok(before.clone()), &Ok(after.clone())).is_ok(), "ordinary production can advance");
        after.import_session_epoch += 1;
        assert!(check_read_bracket_v1(&Ok(before.clone()), &Ok(after)).is_err(), "stable -> import -> stable is still a new epoch");
        assert!(check_read_bracket_v1(&Ok(before), &Err("restarted".into())).is_err());
    }

    #[test]
    fn a_serialized_authority_cannot_override_failed_reads_or_another_capability_version() {
        let mut caps = capabilities_from_reads_v1(Ok(&status("idle", true)), Err("down".into()), Err("down".into()));
        caps.finality_authority = MisakaFinalityAuthorityV1::Dns;
        assert!(matches!(evaluate_preflight_v1(&caps, &expect(None), &[]), Err(MisakaPreflightRefusalV1::InvalidCapabilities { .. })));
        let value: serde_json::Value = serde_json::from_str(&evaluate_preflight_json_v1(
            &serde_json::to_string(&caps).unwrap(),
            &serde_json::to_string(&expect(None)).unwrap(),
            "[]",
        ))
        .unwrap();
        assert_eq!(value["refusal"]["code"], "invalidCapabilities");
        caps.finality_authority = MisakaFinalityAuthorityV1::Unknown;
        caps.version += 1;
        assert!(matches!(evaluate_preflight_v1(&caps, &expect(None), &[]), Err(MisakaPreflightRefusalV1::InvalidCapabilities { .. })));
    }

    #[test]
    fn a_retired_native_node_passes_ordinary_transfers_and_refuses_retired_kinds() {
        let caps = capabilities_from_reads_v1(Ok(&status("idle", true)), Ok(&settlement(Some(4_000))), Err("no overlay".into()));
        assert_eq!(caps.finality_authority, MisakaFinalityAuthorityV1::Native);
        let ok = evaluate_preflight_v1(&caps, &expect(Some(4_000)), &[kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE]).unwrap();
        assert_eq!(ok.finality_authority, MisakaFinalityAuthorityV1::Native);
        for retired in [
            kaspa_consensus_core::subnets::SUBNETWORK_ID_STAKE_BOND,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_STAKE_ATTESTATION_SHARD,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_STAKE_PRECOMMIT,
        ] {
            assert!(matches!(
                evaluate_preflight_v1(&caps, &expect(Some(4_000)), &[retired]),
                Err(MisakaPreflightRefusalV1::RetiredKind { .. })
            ));
        }
    }

    #[test]
    fn import_genesis_and_unknown_authority_refuse_before_any_signature() {
        let settled = settlement(Some(4_000));
        let importing = capabilities_from_reads_v1(Ok(&status("installing", false)), Ok(&settled), Err("x".into()));
        assert!(matches!(
            evaluate_preflight_v1(&importing, &expect(Some(4_000)), &[]),
            Err(MisakaPreflightRefusalV1::ImportIncomplete { .. })
        ));
        let mut legacy = status("idle", true);
        legacy.import_session_available = false;
        let missing = capabilities_from_reads_v1(Ok(&legacy), Ok(&settled), Err("x".into()));
        assert_eq!(evaluate_preflight_v1(&missing, &expect(Some(4_000)), &[]), Err(MisakaPreflightRefusalV1::ImportStatusMissing));
        let unknown = capabilities_from_reads_v1(Ok(&status("idle", true)), Err("down".into()), Err("down".into()));
        assert!(matches!(
            evaluate_preflight_v1(&unknown, &expect(None), &[]),
            Err(MisakaPreflightRefusalV1::FinalityAuthorityUnknown { .. })
        ));
        let other = MisakaWalletExpectationsV1 { expected_genesis_hash: Some("bb".repeat(64)), retirement_daa: None };
        let caps = capabilities_from_reads_v1(Ok(&status("idle", true)), Ok(&settled), Err("x".into()));
        assert!(matches!(evaluate_preflight_v1(&caps, &other, &[]), Err(MisakaPreflightRefusalV1::GenesisMismatch { .. })));
    }

    #[test]
    fn a_scheduled_retirement_refuses_retired_kinds_inside_the_margin_and_the_json_form_round_trips() {
        let dns = GetDnsConfirmationResponse { available: true, ..Default::default() };
        let caps = capabilities_from_reads_v1(Ok(&status("idle", true)), Ok(&settlement(None)), Ok(&dns));
        assert_eq!(caps.finality_authority, MisakaFinalityAuthorityV1::Dns);
        let bond = kaspa_consensus_core::subnets::SUBNETWORK_ID_STAKE_BOND;
        assert!(
            evaluate_preflight_v1(&caps, &expect(Some(5_000 + MISAKA_RETIREMENT_SIGNING_MARGIN_DAA_V1)), &[bond.clone()]).is_err()
        );
        assert!(evaluate_preflight_v1(&caps, &expect(Some(5_001 + MISAKA_RETIREMENT_SIGNING_MARGIN_DAA_V1)), &[bond.clone()]).is_ok());
        let json = evaluate_preflight_json_v1(
            &serde_json::to_string(&caps).unwrap(),
            &serde_json::to_string(&expect(Some(5_000))).unwrap(),
            &serde_json::to_string(&vec![bond.to_string()]).unwrap(),
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["refusal"]["code"], "retiredKind");
        assert!(evaluate_preflight_json_v1("{}", "{}", "[]").contains("\"ok\":false"));
    }
}
