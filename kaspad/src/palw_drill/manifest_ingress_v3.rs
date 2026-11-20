//! Explicit private Manifest ingress configuration, not a capacity calibration.
//! Read once at argument parsing so validation, Config, key export and the
//! datadir marker cannot observe different versions of a changing file.
use kaspa_consensus_core::config::params::{ForkActivation, ManifestIngressFenceV3, Params};
use std::{fs::File, io::Read};

const MAX_POLICY_BYTES: usize = 16 * 1024;
const FLAG: &str = "--palw-drill-manifest-ingress";

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclarationV3 {
    version: u16,
    policy: ManifestIngressFenceV3,
}

fn decode_v3(bytes: &[u8]) -> Result<ManifestIngressFenceV3, String> {
    if bytes.len() > MAX_POLICY_BYTES {
        return Err(format!("{FLAG}: policy exceeds {MAX_POLICY_BYTES} bytes"));
    }
    let declaration: DeclarationV3 = serde_json::from_slice(bytes).map_err(|e| format!("{FLAG}: {e}"))?;
    if declaration.version != 3 || declaration.policy.activation == ForkActivation::never() {
        return Err(format!("{FLAG}: requires version 3 and an explicit finite activation"));
    }
    declaration.policy.validate_v3().map_err(|e| format!("{FLAG}: {e}"))?;
    Ok(declaration.policy)
}

/// The result, rather than the path, is stored in Args. No defaults, environment
/// variables, TOML inheritance or later file reload can select this draft.
pub(crate) fn parse_file_v3(path: &str) -> Result<ManifestIngressFenceV3, String> {
    let file = File::open(path).map_err(|e| format!("{FLAG}: cannot open {path}: {e}"))?;
    if !file.metadata().map_err(|e| format!("{FLAG}: {e}"))?.is_file() {
        return Err(format!("{FLAG}: policy must be a regular file"));
    }
    // Bound the read itself, including a file that grows after metadata.
    let mut bytes = Vec::with_capacity(MAX_POLICY_BYTES + 1);
    file.take((MAX_POLICY_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|e| format!("{FLAG}: {e}"))?;
    decode_v3(&bytes)
}

pub(super) fn install_v3(params: &mut Params, policy: ManifestIngressFenceV3) -> Result<(), String> {
    if policy.activation == ForkActivation::never() {
        return Err(format!("{FLAG}: activation must be finite"));
    }
    // Leave the original Params intact on refusal. This operation never arms
    // prerequisite fences or weakens their validators to make the file work.
    let mut candidate = params.clone();
    candidate.palw_manifest_ingress_v3 = Some(policy);
    candidate.validate_palw_manifest_ingress_v3().map_err(|e| format!("{FLAG}: {e}"))?;
    params.palw_manifest_ingress_v3 = Some(policy);
    Ok(())
}

pub(super) fn marker_v3(policy: Option<ManifestIngressFenceV3>) -> String {
    let Some(policy) = policy else { return "none".into() };
    let mut text = format!("v3:{}", policy.activation.daa_score());
    for dimension in policy.dimensions_v3() {
        use std::fmt::Write;
        write!(text, ":{dimension}").expect("String writer");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{args::Args, palw_drill::PalwDrillExtraFencesV1};
    use kaspa_consensus_core::config::{drill::PalwDrillSaltV1, params::TESTNET_PARAMS};

    // Schema/identity inputs only. These values are neither measured nor a
    // complete original-service profile, and no test claims native admission.
    fn policy() -> ManifestIngressFenceV3 {
        ManifestIngressFenceV3 {
            activation: ForkActivation::new(100),
            maximum_source_blocks: 4,
            maximum_transactions: 16,
            maximum_requests: 8,
            maximum_request_bytes: 10_000,
            maximum_archive_heap: 1_000_000,
            maximum_archive_work: 100_000,
            replay_passes: 3,
            replay_visits: 32,
            byte_sompi: 1,
            work_sompi: 2,
            io_sompi: 3,
            control_sompi: 4,
            retained_byte_sompi: 5,
            archive_work_sompi: 6,
        }
    }

    fn declaration() -> serde_json::Value {
        serde_json::json!({"version": 3, "policy": policy()})
    }

    #[test]
    fn strict_bounded_declaration_and_single_cli_read() {
        let bytes = serde_json::to_vec(&declaration()).unwrap();
        assert_eq!(decode_v3(&bytes).unwrap(), policy());
        assert!(decode_v3(&vec![b' '; MAX_POLICY_BYTES + 1]).is_err());
        for (key, value) in [("version", serde_json::json!(4)), ("unknown", serde_json::json!(1))] {
            let mut wrong = declaration();
            wrong[key] = value;
            assert!(decode_v3(&serde_json::to_vec(&wrong).unwrap()).is_err());
        }
        for (key, value) in
            [("activation", u64::MAX), ("byte_sompi", 0), ("maximum_requests", 17), ("replay_passes", 2), ("unexpected", 1)]
        {
            let mut wrong = declaration();
            wrong["policy"][key] = serde_json::json!(value);
            assert!(decode_v3(&serde_json::to_vec(&wrong).unwrap()).is_err(), "{key}");
        }
        let mut missing = declaration();
        missing["policy"].as_object_mut().unwrap().remove("io_sompi");
        assert!(decode_v3(&serde_json::to_vec(&missing).unwrap()).is_err());
        let duplicate = String::from_utf8(bytes.clone()).unwrap().replace("\"version\":3", "\"version\":3,\"version\":3");
        assert!(decode_v3(duplicate.as_bytes()).is_err());

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("policy.json");
        std::fs::write(&path, bytes).unwrap();
        let arg = format!("{FLAG}={}", path.display());
        let args = Args::parse(["kaspad", arg.as_str()]).unwrap();
        std::fs::write(&path, b"changed after parsing").unwrap();
        assert_eq!(args.palw_drill_manifest_ingress, Some(policy()));
        assert_eq!(PalwDrillExtraFencesV1::of(&args).manifest_ingress, Some(policy()));
        assert!(super::super::palw_drill_validate_args_v1(&args).unwrap_err().to_string().contains("genesis-salt"));
        assert!(parse_file_v3(path.to_str().unwrap()).is_err());
        assert!(Args::default().palw_drill_manifest_ingress.is_none());
    }

    #[test]
    fn public_or_incomplete_parameters_are_not_mutated() {
        let mut public = TESTNET_PARAMS.clone();
        let before = public.consensus_params_id();
        assert!(install_v3(&mut public, policy()).is_err());
        assert_eq!(public.consensus_params_id(), before);
        assert!(public.palw_manifest_ingress_v3.is_none());
        let salt = PalwDrillSaltV1::from_hex(&"53".repeat(32)).unwrap();
        let mut private = kaspa_consensus_core::config::params::palw_t12_drill_params_v1(&salt);
        let before = private.consensus_params_id();
        assert!(install_v3(&mut private, policy()).unwrap_err().contains("requires native court"));
        assert_eq!(private.consensus_params_id(), before);
        assert!(private.palw_manifest_ingress_v3.is_none());
    }

    #[test]
    fn every_declared_dimension_changes_schedule_and_datadir_identity() {
        let original = policy();
        let mut declared = TESTNET_PARAMS.clone();
        // Hash coverage only; public validation still refuses this declaration.
        declared.palw_manifest_ingress_v3 = Some(original);
        let params_id = declared.consensus_params_id();
        let schedule_id = declared.consensus_schedule_id();
        let marker = marker_v3(Some(original));
        let object = serde_json::to_value(original).unwrap();
        for key in object.as_object().unwrap().keys() {
            let mut changed = object.clone();
            changed[key] = serde_json::json!(changed[key].as_u64().unwrap() + 1);
            let policy: ManifestIngressFenceV3 = serde_json::from_value(changed).unwrap();
            let mut candidate = declared.clone();
            candidate.palw_manifest_ingress_v3 = Some(policy);
            assert_ne!(candidate.consensus_params_id(), params_id, "{key}");
            assert_ne!(candidate.consensus_schedule_id(), schedule_id, "{key}");
            assert_ne!(marker_v3(Some(policy)), marker, "{key}");
        }
    }

    #[test]
    fn stored_chain_refuses_changed_or_removed_prices_and_bounds() {
        let salt = PalwDrillSaltV1::from_hex(&"53".repeat(32)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let extra = PalwDrillExtraFencesV1 { manifest_ingress: Some(policy()), ..Default::default() };
        let guard =
            |extra| super::super::palw_drill_datadir_guard_v4(dir.path(), Some(&salt), "test-genesis", None, None, None, None, extra);
        guard(extra).unwrap();
        std::fs::create_dir(dir.path().join("datadir")).unwrap();
        guard(extra).unwrap();
        let marker = dir.path().join(super::super::PALW_DRILL_DATADIR_MARKER_V1);
        let before = std::fs::read(&marker).unwrap();
        let mut changed = extra;
        changed.manifest_ingress.as_mut().unwrap().byte_sompi += 1;
        assert!(guard(changed).unwrap_err().contains(FLAG));
        changed = extra;
        changed.manifest_ingress.as_mut().unwrap().replay_visits += 1;
        assert!(guard(changed).is_err());
        assert!(guard(PalwDrillExtraFencesV1::default()).is_err());
        assert_eq!(std::fs::read(&marker).unwrap(), before);
    }
}
