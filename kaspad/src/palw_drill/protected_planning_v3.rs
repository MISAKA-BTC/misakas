//! Command-line-only private planning declaration. Read once; the same parsed
//! value reaches validation, identities, datadir protection and keyring export.
use kaspa_consensus_core::config::params::{ForkActivation, Params, ProtectedPlanningFenceV3};
use std::{fs::File, io::Read};

const MAX_POLICY_BYTES: usize = 16 * 1024;
const FLAG: &str = "--palw-drill-protected-planning";

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclarationV3 {
    version: u16,
    policy: ProtectedPlanningFenceV3,
}

fn decode_v3(bytes: &[u8]) -> Result<ProtectedPlanningFenceV3, String> {
    if bytes.len() > MAX_POLICY_BYTES {
        return Err(format!("{FLAG}: declaration exceeds {MAX_POLICY_BYTES} bytes"));
    }
    let value: DeclarationV3 = serde_json::from_slice(bytes).map_err(|e| format!("{FLAG}: {e}"))?;
    if value.version != 3 || value.policy.activation == ForkActivation::never() {
        return Err(format!("{FLAG}: requires version 3 and an explicit finite activation"));
    }
    value.policy.declaration.validate_v3().map_err(|e| format!("{FLAG}: {e}"))?;
    Ok(value.policy)
}

pub(crate) fn parse_file_v3(path: &str) -> Result<ProtectedPlanningFenceV3, String> {
    let file = File::open(path).map_err(|e| format!("{FLAG}: cannot open {path}: {e}"))?;
    if !file.metadata().map_err(|e| format!("{FLAG}: {e}"))?.is_file() {
        return Err(format!("{FLAG}: declaration must be a regular file"));
    }
    let mut bytes = Vec::with_capacity(MAX_POLICY_BYTES + 1);
    file.take((MAX_POLICY_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|e| format!("{FLAG}: {e}"))?;
    decode_v3(&bytes)
}

pub(super) fn install_v3(params: &mut Params, policy: ProtectedPlanningFenceV3) -> Result<(), String> {
    if policy.activation == ForkActivation::never() {
        return Err(format!("{FLAG}: activation must be finite"));
    }
    let mut candidate = params.clone();
    candidate.palw_protected_planning_v3 = Some(policy);
    candidate.validate_palw_protected_planning_v3().map_err(|e| format!("{FLAG}: {e}"))?;
    params.palw_protected_planning_v3 = Some(policy);
    Ok(())
}

pub(super) fn marker_v3(policy: Option<ProtectedPlanningFenceV3>) -> String {
    policy.map_or_else(|| "none".into(), |p| format!("v3:{}:{}", p.activation.daa_score(), p.declaration.fingerprint_v3()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{args::Args, palw_drill::PalwDrillExtraFencesV1};
    use kaspa_consensus_core::config::{drill::PalwDrillSaltV1, params::TESTNET_PARAMS};

    // Schema-only fixture. It is never used to admit a native claim.
    fn policy() -> ProtectedPlanningFenceV3 {
        ProtectedPlanningFenceV3 {
            activation: ForkActivation::new(100),
            declaration: kaspa_consensus_core::palw_positive_contract_v2::ProtectedPlanningDeclarationV3::decode_json_v3(
                include_bytes!("../../../consensus/core/tests/fixtures/protected-planning-schema-v3.json"),
            )
            .unwrap(),
        }
    }

    #[test]
    fn strict_cli_reads_once_and_does_not_inherit_from_toml() {
        let declaration = serde_json::json!({"version":3,"policy":policy()});
        let bytes = serde_json::to_vec(&declaration).unwrap();
        assert_eq!(decode_v3(&bytes).unwrap(), policy());
        assert!(decode_v3(&vec![b' '; MAX_POLICY_BYTES + 1]).is_err());
        for pointer in ["/version", "/policy/activation", "/policy/declaration/version"] {
            let mut wrong = declaration.clone();
            *wrong.pointer_mut(pointer).unwrap() = serde_json::json!(u64::MAX);
            assert!(decode_v3(&serde_json::to_vec(&wrong).unwrap()).is_err());
        }
        let mut wrong = declaration.clone();
        wrong["policy"]["unexpected"] = serde_json::json!(1);
        assert!(decode_v3(&serde_json::to_vec(&wrong).unwrap()).is_err());
        let mut wrong = declaration;
        wrong["policy"]["declaration"].as_object_mut().unwrap().remove("material");
        assert!(decode_v3(&serde_json::to_vec(&wrong).unwrap()).is_err());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("planning.json");
        std::fs::write(&path, bytes).unwrap();
        let flag = format!("{FLAG}={}", path.display());
        let args = Args::parse(["kaspad", flag.as_str()]).unwrap();
        std::fs::write(&path, b"changed after parsing").unwrap();
        assert_eq!(args.palw_drill_protected_planning, Some(policy()));
        assert_eq!(PalwDrillExtraFencesV1::of(&args).protected_planning, Some(policy()));
        assert!(super::super::palw_drill_validate_args_v1(&args).unwrap_err().to_string().contains("genesis-salt"));
        assert!(parse_file_v3(path.to_str().unwrap()).is_err());
        assert!(Args::default().palw_drill_protected_planning.is_none());
        assert!(
            serde_json::from_value::<Args>(serde_json::json!({
                "palw-drill-protected-planning": policy()
            }))
            .is_err()
        );
        assert!(serde_json::from_value::<Args>(serde_json::json!({})).unwrap().palw_drill_protected_planning.is_none());
    }

    #[test]
    fn refusal_preserves_public_and_incomplete_private_parameters() {
        let salt = PalwDrillSaltV1::from_hex(&"53".repeat(32)).unwrap();
        for mut params in [TESTNET_PARAMS.clone(), kaspa_consensus_core::config::params::palw_t12_drill_params_v1(&salt)] {
            let before = params.consensus_params_id();
            assert!(install_v3(&mut params, policy()).is_err());
            assert_eq!(params.consensus_params_id(), before);
            assert!(params.palw_protected_planning_v3.is_none());
        }
    }

    fn scalar_paths(value: &serde_json::Value, prefix: String, out: &mut Vec<String>) {
        if let Some(fields) = value.as_object() {
            for (key, value) in fields {
                scalar_paths(value, format!("{prefix}/{key}"), out);
            }
        } else {
            assert!(value.is_u64());
            out.push(prefix);
        }
    }

    #[test]
    fn every_declared_scalar_changes_schedule_params_and_datadir_identity() {
        let mut params = TESTNET_PARAMS.clone();
        params.palw_protected_planning_v3 = Some(policy());
        let original = serde_json::to_value(policy()).unwrap();
        let mut paths = Vec::new();
        scalar_paths(&original, String::new(), &mut paths);
        for path in paths {
            let mut value = original.clone();
            let field = value.pointer_mut(&path).unwrap();
            *field = serde_json::json!(field.as_u64().unwrap() + 1);
            let changed: ProtectedPlanningFenceV3 = serde_json::from_value(value).unwrap();
            let mut next = params.clone();
            next.palw_protected_planning_v3 = Some(changed);
            assert_ne!(next.consensus_params_id(), params.consensus_params_id(), "{path}");
            assert_ne!(next.consensus_schedule_id(), params.consensus_schedule_id(), "{path}");
            assert_ne!(marker_v3(Some(changed)), marker_v3(Some(policy())), "{path}");
        }
    }

    #[test]
    fn stored_chain_refuses_changed_or_removed_planning_declaration() {
        let salt = PalwDrillSaltV1::from_hex(&"53".repeat(32)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let extra = PalwDrillExtraFencesV1 { protected_planning: Some(policy()), ..Default::default() };
        let guard =
            |extra| super::super::palw_drill_datadir_guard_v4(dir.path(), Some(&salt), "test-genesis", None, None, None, None, extra);
        guard(extra).unwrap();
        std::fs::create_dir(dir.path().join("datadir")).unwrap();
        guard(extra).unwrap();
        let marker = dir.path().join(super::super::PALW_DRILL_DATADIR_MARKER_V1);
        let before = std::fs::read(&marker).unwrap();
        let mut changed = extra;
        changed.protected_planning.as_mut().unwrap().activation = ForkActivation::new(101);
        assert!(guard(changed).unwrap_err().contains(FLAG));
        let mut value = serde_json::to_value(policy()).unwrap();
        value["declaration"]["material"]["sompi_per_byte"] = serde_json::json!(2);
        changed.protected_planning = Some(serde_json::from_value(value).unwrap());
        assert!(guard(changed).is_err());
        assert!(guard(PalwDrillExtraFencesV1::default()).is_err());
        assert_eq!(std::fs::read(&marker).unwrap(), before);
    }
}
