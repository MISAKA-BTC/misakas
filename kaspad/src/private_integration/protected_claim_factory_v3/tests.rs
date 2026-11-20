use super::*;
use crate::{
    args::Args,
    private_integration::{PRIVATE_PROBE_MARKER_V1, prepare_private_integration_probe, private_probe_manifest},
};
use kaspa_consensus_core::{config::Config, palw_court_service_v1::ServiceCost};

fn salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x53; 32]).unwrap()
}

// Schema/startup fixture only. No Studio capacity, admission or approval is
// inferred from these values. Preserve the existing tokenizer fixture's source
// allowance when splitting it over the actual configured mergeset bound.
fn profile() -> PrivateProtectedClaimProfileV3 {
    let baseline = palw_chain_params_v1(palw_drill_network_v1(), Some(&salt())).unwrap();
    let mut tokenizer = TokenizerRegistrationPolicyV1 {
        version: 1,
        max_raw_bytes: 131_072,
        max_vocab: 1024,
        max_key_events: 200_000,
        max_eog_ids: 8,
        max_rendering_bytes: 131_072,
        max_parser_scratch_bytes: 32_000_000,
        max_carrier_bytes: 80_000,
        max_retained_bytes: 1_000_000,
        max_retained_total_bytes: 8_000_000,
        service_budget: ServiceCost { bytes: 100_000, work: 84_000_000, io: 1_000_000, controls: 2 },
        maximum_source_blocks: 1,
        max_work: 70_000_000,
        max_pending_per_opener: 2,
        max_pending_total: 8,
        max_session_daa: 100,
        retention_daa: 20,
        carrier_overhead_bytes: 5,
        retention_copies: 3,
        metadata_bytes: 64_000,
        work_rates: [1; 7],
        bookkeeping_work_per_step: 1,
        rent_per_carrier_byte: 1,
        rent_per_retained_byte_daa: 1,
        rent_per_work: 1,
        max_registration_charge: 100_000_000,
    };
    let sweep = tokenizer.bookkeeping_work_v1().unwrap().sweep;
    let sources = baseline.tokenizer_registration_source_blocks_bound_v1().unwrap();
    tokenizer.maximum_source_blocks = sources;
    tokenizer.service_budget.bytes *= sources;
    tokenizer.service_budget.work = sweep + (tokenizer.service_budget.work - sweep) * sources;
    tokenizer.service_budget.io *= sources;
    tokenizer.service_budget.controls *= sources;
    let activation = ForkActivation::new(20_000);
    PrivateProtectedClaimProfileV3 {
        planning: ProtectedPlanningFenceV3 {
            activation,
            declaration: kaspa_consensus_core::palw_positive_contract_v2::ProtectedPlanningDeclarationV3::decode_json_v3(
                include_bytes!("../../../../consensus/core/tests/fixtures/protected-planning-schema-v3.json"),
            )
            .unwrap(),
        },
        manifest: ManifestIngressFenceV3 {
            activation,
            maximum_source_blocks: 1,
            maximum_transactions: 1,
            maximum_requests: 1,
            maximum_request_bytes: 1,
            maximum_archive_heap: 1,
            maximum_archive_work: 1,
            replay_passes: 3,
            replay_visits: 1,
            byte_sompi: 1,
            work_sompi: 1,
            io_sompi: 1,
            control_sompi: 1,
            retained_byte_sompi: 1,
            archive_work_sompi: 1,
        },
        tokenizer,
    }
}

fn with_stack(body: fn()) {
    if let Err(panic) = std::thread::Builder::new().stack_size(64 << 20).spawn(body).unwrap().join() {
        std::panic::resume_unwind(panic);
    }
}

fn write_profile(dir: &Path, profile: PrivateProtectedClaimProfileV3) -> (PathBuf, PathBuf, serde_json::Value) {
    let tokenizer_path = dir.join("tokenizer.borsh");
    std::fs::write(&tokenizer_path, borsh::to_vec(&profile.tokenizer).unwrap()).unwrap();
    let value = serde_json::json!({"version": 3, "planning": profile.planning, "manifest": profile.manifest,
        "tokenizer_policy_path": tokenizer_path});
    let path = dir.join("protected.json");
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    (path, tokenizer_path, value)
}

#[test]
fn full_startup_uses_parsed_policy_and_exact_owned_directory() {
    with_stack(|| {
        let expected = profile();
        let files = tempfile::tempdir().unwrap();
        let (path, tokenizer_path, _) = write_profile(files.path(), expected);
        let parsed = PrivateProtectedClaimProfileV3::read(&path).unwrap();
        assert_eq!(parsed, expected);
        std::fs::write(path, b"changed after parse").unwrap();
        std::fs::write(tokenizer_path, b"changed after parse").unwrap();
        let params = parsed.params_v3(&salt()).unwrap();
        let base = palw_chain_params_v1(palw_drill_network_v1(), Some(&salt())).unwrap();
        assert_eq!(params.genesis.hash, base.genesis.hash);
        assert!(!params.skip_proof_of_work);
        assert!(params.palw_court_family_supplier_v1.is_none());
        assert!(params.palw_settlement_once_v1.is_none());
        assert!(params.palw_reporter_income_v2.is_none());
        assert!(params.palw_material_income_v3.is_none());
        assert!(base.palw_class_semantics_v1_fence().is_none());
        assert!(!params.palw_class_semantics_v1_active_at(19_999));
        assert!(params.palw_class_semantics_v1_active_at(20_000));
        let mut missing_semantics = params.clone();
        missing_semantics.palw_class_semantics_v1 = None;
        assert!(missing_semantics.validate_palw_tokenizer_registration_v1().is_err());
        let mut restored = params.clone();
        restored.palw_class_semantics_v1 = base.palw_class_semantics_v1;
        restored.palw_batch_court_v1 = base.palw_batch_court_v1;
        restored.palw_tokenizer_registration_v1 = base.palw_tokenizer_registration_v1;
        restored.palw_model_panel_one_retention_v1 = base.palw_model_panel_one_retention_v1;
        restored.palw_manifest_ingress_v3 = base.palw_manifest_ingress_v3;
        restored.palw_protected_planning_v3 = base.palw_protected_planning_v3;
        restored.sync_palw_g14_gates_v1();
        assert_eq!(
            restored.consensus_params_id(),
            base.consensus_params_id(),
            "only the six native inputs depart from salted shipped rules"
        );
        params.validate_palw_v2().unwrap();
        assert!(params.palw_protected_planning_v3_at(19_999).is_none());
        assert_eq!(params.palw_protected_planning_v3_at(20_000), Some(expected.planning));
        let mut config = Config::new(params);
        config.palw_drill_genesis_salt = Some(salt());
        let data = tempfile::tempdir().unwrap();
        let args = Args {
            testnet: true,
            testnet_suffix: 12,
            disable_dns_seeding: true,
            outbound_target: 0,
            disable_upnp: true,
            listen: Some("127.0.0.1:53001".parse().unwrap()),
            rpclisten: Some("127.0.0.1:53002".parse().unwrap()),
            appdir: Some(data.path().display().to_string()),
            ..Args::default()
        };
        prepare_private_integration_probe(&args, &config).unwrap();
        prepare_private_integration_probe(&args, &config).unwrap();
        let manifest = private_probe_manifest(&config);
        assert_eq!(manifest["releaseApproval"], false);
        assert_eq!(manifest["protectedClaimPrivatePolicyV3"]["actualClaimCompletion"], "UNKNOWN");
        assert!(manifest.get("g14Drill").is_none());
        let marker = data.path().join(PRIVATE_PROBE_MARKER_V1);
        let before = std::fs::read(&marker).unwrap();
        let mut changed = config.clone();
        changed.params.blockrate.target_time_per_block += 1;
        assert!(prepare_private_integration_probe(&args, &changed).unwrap_err().contains("exact salted factory"));
        let mut changed_policy = parsed;
        changed_policy.manifest.byte_sompi += 1;
        let mut changed = Config::new(changed_policy.params_v3(&salt()).unwrap());
        changed.palw_drill_genesis_salt = Some(salt());
        assert!(prepare_private_integration_probe(&args, &changed).unwrap_err().contains("pinned identity"));
        assert_eq!(std::fs::read(marker).unwrap(), before);
    });
}

#[test]
fn policy_loading_and_native_dependencies_remain_strict() {
    with_stack(|| {
        let profile = profile();
        let files = tempfile::tempdir().unwrap();
        let (path, tokenizer_path, value) = write_profile(files.path(), profile);
        for field in ["version", "planning", "manifest", "tokenizer_policy_path"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            std::fs::write(&path, serde_json::to_vec(&missing).unwrap()).unwrap();
            assert!(PrivateProtectedClaimProfileV3::read(&path).is_err(), "{field}");
        }
        let mut unknown = value.clone();
        unknown["unexpected"] = serde_json::json!(1);
        std::fs::write(&path, serde_json::to_vec(&unknown).unwrap()).unwrap();
        assert!(PrivateProtectedClaimProfileV3::read(&path).is_err());
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let mut trailing = borsh::to_vec(&profile.tokenizer).unwrap();
        trailing.push(0);
        std::fs::write(&tokenizer_path, trailing).unwrap();
        assert!(PrivateProtectedClaimProfileV3::read(&path).is_err());
        std::fs::write(&tokenizer_path, vec![0; MAX_TOKENIZER_POLICY_BYTES + 1]).unwrap();
        assert!(PrivateProtectedClaimProfileV3::read(&path).is_err());
        assert!(PrivateProtectedClaimProfileV3::read(Path::new("relative.json")).is_err());
        let mut premature = profile;
        premature.planning.activation = ForkActivation::new(1);
        premature.manifest.activation = ForkActivation::new(1);
        assert!(premature.params_v3(&salt()).is_err(), "cannot move shipped prerequisites to fit a declaration");
        let mut mismatch = profile;
        mismatch.manifest.activation = ForkActivation::new(20_001);
        assert!(mismatch.params_v3(&salt()).is_err());
        let mut too_few_sources = profile;
        too_few_sources.tokenizer.maximum_source_blocks = 1;
        assert!(too_few_sources.params_v3(&salt()).is_err(), "tokenizer must cover the real source geometry");
    });
}

#[test]
fn protected_bootstrap_preserves_exact_policy_keys_and_genesis_outpoints() {
    with_stack(|| {
        use kaspa_consensus_core::config::drill::{palw_t12_drill_cards_v1, palw_t12_drill_main_key_v1};
        let profile = profile();
        let mut config = Config::new(profile.params_v3(&salt()).unwrap());
        config.palw_drill_genesis_salt = Some(salt());
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("keys");
        let report = export_private_protected_bootstrap_v3(&config, &output).unwrap();
        assert_eq!(report["profile"], private_probe_manifest(&config));
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(output.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(saved, report);
        let exported = PrivateProtectedClaimProfileV3::read(Path::new(report["policyFile"].as_str().unwrap())).unwrap();
        assert_eq!(exported, profile);
        let restored = exported.params_v3(&salt()).unwrap();
        assert_eq!(restored.consensus_identity_id(), config.params.consensus_identity_id());
        assert_eq!(restored.consensus_schedule_id(), config.params.consensus_schedule_id());
        assert_eq!(report["claimAdmitted"], false);
        assert_eq!(report["registrationSubmitted"], false);
        assert_eq!(report["seats"].as_array().unwrap().len(), palw_t12_drill_cards_v1(&salt()).len());
        let main = palw_t12_drill_main_key_v1(&salt());
        assert_eq!(std::fs::read_to_string(output.join("main-0.seed")).unwrap(), faster_hex::hex_string(&main.seed));
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(!serialized.contains(&faster_hex::hex_string(&main.seed)));
        for card in palw_t12_drill_cards_v1(&salt()) {
            let row = &report["seats"][card.bond.n as usize];
            for (field, key) in [("bond", card.bond), ("operator", card.operator)] {
                assert_eq!(row[field]["publicKey"], faster_hex::hex_string(&key.pubkey));
                let path = output.join(row[field]["seedFile"].as_str().unwrap());
                assert_eq!(std::fs::read_to_string(path).unwrap(), faster_hex::hex_string(&key.seed));
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&output).unwrap().permissions().mode() & 0o777, 0o700);
            for entry in std::fs::read_dir(&output).unwrap() {
                assert_eq!(entry.unwrap().metadata().unwrap().permissions().mode() & 0o777, 0o600);
            }
        }
        assert!(export_private_protected_bootstrap_v3(&config, &output).is_err());
        assert_eq!(std::fs::read_to_string(output.join("main-0.seed")).unwrap(), faster_hex::hex_string(&main.seed));
    });
}

#[test]
fn protected_bootstrap_rejects_mismatched_factory_before_writing() {
    with_stack(|| {
        let profile = profile();
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("keys");
        for mutation in 0..4 {
            let mut config = Config::new(profile.params_v3(&salt()).unwrap());
            config.palw_drill_genesis_salt = Some(salt());
            match mutation {
                0 => config.palw_drill_genesis_salt = None,
                1 => config.params.skip_proof_of_work = true,
                2 => config.palw_drill_genesis_salt = Some(PalwDrillSaltV1::from_bytes([0x54; 32]).unwrap()),
                3 => config.params.palw_manifest_ingress_v3 = None,
                _ => unreachable!(),
            }
            assert!(export_private_protected_bootstrap_v3(&config, &output).is_err(), "mutation {mutation}");
            assert!(!output.exists());
        }
        let mut config = Config::new(profile.params_v3(&salt()).unwrap());
        config.palw_drill_genesis_salt = Some(salt());
        assert!(export_private_protected_bootstrap_v3(&config, Path::new("relative-keys")).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(directory.path(), &output).unwrap();
            assert!(export_private_protected_bootstrap_v3(&config, &output).is_err());
            assert!(!directory.path().join("main-0.seed").exists());
        }
    });
}

#[cfg(feature = "private-integration-clock")]
#[test]
fn protected_lifecycle_uses_an_isolated_genesis_and_registers_at_200() {
    with_stack(|| {
        let mut bytes = *salt().as_bytes();
        let prefix = b"MISAKA/P200/V1/";
        bytes[..prefix.len()].copy_from_slice(prefix);
        let lifecycle_salt = PalwDrillSaltV1::from_bytes(bytes).unwrap();
        let mut policy = profile();
        let at = ForkActivation::new(200);
        policy.planning.activation = at;
        policy.manifest.activation = at;
        assert!(policy.params_v3(&salt()).is_err());

        let params = policy.params_v3(&lifecycle_salt).unwrap();
        assert!(params.has_protected_lifecycle_drill_genesis_v1());
        assert_eq!(params.genesis.hash, palw_chain_params_v1(palw_drill_network_v1(), Some(&lifecycle_salt)).unwrap().genesis.hash);
        assert!(!params.skip_proof_of_work);
        assert!(params.palw_model_eligibility_v1.is_none());
        assert!(params.palw_protected_planning_v3_at(199).is_none());
        assert_eq!(params.palw_protected_planning_v3_at(200), Some(policy.planning));
        let expected_mirror = params.palw_rfc0018_expected_mirror_v1().unwrap();
        assert_eq!(expected_mirror.gate_c_from_daa, Some(80));
        assert_eq!(expected_mirror.certification.verifier_maturity_daa, 80);
        assert_eq!(expected_mirror.certification.prosecution_window_daa, 70);
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("expected private ConsensusV2 bundle");
        };
        assert_eq!(bundle.freeprompt.receipt_maturity_daa(), 40);
        assert_eq!(bundle.reorg_margin_daa, 24);
        let normal = palw_chain_params_v1(palw_drill_network_v1(), Some(&salt())).unwrap();
        let normal_certification = normal.palw_rfc0018_expected_mirror_v1().unwrap().certification;
        assert_eq!(normal_certification.verifier_maturity_daa, 5_040);
        assert_eq!(
            (expected_mirror.certification.m, expected_mirror.certification.k),
            (normal_certification.m, normal_certification.k)
        );

        let mut config = Config::new(params);
        config.palw_drill_genesis_salt = Some(lifecycle_salt);
        policy.check_config_v3(&config).unwrap();
    });
}

#[cfg(feature = "private-integration-clock")]
#[test]
fn protected_historical_clock_runs_only_in_an_isolated_child_and_restores_time() {
    const CHILD: &str = "MISAKA_PROTECTED_CLOCK_TEST_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "private_integration::protected_claim_factory_v3::tests::protected_historical_clock_runs_only_in_an_isolated_child_and_restores_time",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("KASPA_TEST_CLOCK_ISOLATED", "1")
            .output()
            .unwrap();
        assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed"));
        return;
    }
    with_stack(|| {
        use crate::private_integration::PrivateHistoricalClockV1;
        use kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS;
        use std::time::{Duration, Instant};
        let mut config = Config::new(profile().params_v3(&salt()).unwrap());
        config.palw_drill_genesis_salt = Some(salt());
        let data = tempfile::tempdir().unwrap();
        let args = Args {
            testnet: true,
            testnet_suffix: 12,
            disable_dns_seeding: true,
            outbound_target: 0,
            disable_upnp: true,
            listen: Some("127.0.0.1:53001".parse().unwrap()),
            rpclisten: Some("127.0.0.1:53002".parse().unwrap()),
            appdir: Some(data.path().join("node").display().to_string()),
            ..Args::default()
        };
        let command = data.path().join("clock.json");
        let initial = config.params.genesis.timestamp;
        let write = |timestamp_ms| {
            let bytes = serde_json::to_vec(&serde_json::json!({
                "version": 1, "genesis": config.params.genesis.hash.to_string(), "timestampMs": timestamp_ms,
            }))
            .unwrap();
            let temporary = data.path().join("clock.next");
            std::fs::write(&temporary, bytes).unwrap();
            std::fs::rename(temporary, &command).unwrap();
        };
        write(initial);
        let real_before = kaspa_core::time::unix_now();
        assert!(real_before > initial + 2 * HEARTBEAT_RECOVERY_INTERVAL_MS);
        let mut wrong = config.clone();
        wrong.params.skip_proof_of_work = true;
        assert!(PrivateHistoricalClockV1::start(&args, &wrong, &command).is_err());
        assert!(!kaspa_core::time::test_clock::active());
        let scope = PrivateHistoricalClockV1::start(&args, &config, &command).unwrap();
        assert!(kaspa_core::time::test_clock::active());
        assert!(kaspa_core::time::unix_now().abs_diff(initial) < 5_000);
        assert!(PrivateHistoricalClockV1::start(&args, &config, &command).is_err());
        let next = initial + HEARTBEAT_RECOVERY_INTERVAL_MS;
        write(next);
        let deadline = Instant::now() + Duration::from_secs(5);
        while kaspa_core::time::unix_now() < next && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!((next..next + 5_000).contains(&kaspa_core::time::unix_now()));
        drop(scope);
        assert!(!kaspa_core::time::test_clock::active());
        assert!(kaspa_core::time::unix_now() >= real_before);
        assert_eq!(config.params.consensus_params_id(), profile().params_v3(&salt()).unwrap().consensus_params_id());
        assert!(!config.params.skip_proof_of_work);
    });
}
