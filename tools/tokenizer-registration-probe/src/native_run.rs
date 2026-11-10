use crate::native_bundle::{MANIFEST_FILE, MAX_ACTIONS, Manifest, STATE_FILE, VERSION, action_file};
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{
        NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, native_declared_logical_limits_v1,
        native_logical_admission_enabled_v1, native_resource_admission_enabled_v1, restore_native_carriage_v1,
    },
    palw_state_v2::{
        PalwBlockWorkV3, PalwStateCarriageV2, TokenizerRegistrationFeeWorkingStateV1, apply_delta_v2,
        apply_palw_transition_retained_v7, palw_tokenizer_registration_status_v1, revert_delta_v2,
    },
    palw_tokenizer_finish_derivation_cache_v1::TokenizerFinishDerivationCacheV1,
    palw_tokenizer_registration_carriage_v1::{TokenizerRegistrationCarrierContextV1, ValidatedTokenizerRegistrationCarrierV1},
    palw_tokenizer_registration_state_v1::TokenizerRegistrationStatusV1,
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
    tx::{
        PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
        ValidatedTransaction,
    },
};
use serde_json::json;
use std::{collections::BTreeMap, fs::File, io::Read, path::Path, time::Instant};

fn read_bounded(path: &Path, limit: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    File::open(path).unwrap_or_else(|error| panic!("{}: {error}", path.display())).take(limit + 1).read_to_end(&mut bytes).unwrap();
    assert!(bytes.len() as u64 <= limit, "{} exceeds the measurement input bound", path.display());
    bytes
}

pub fn run(dir: &Path) {
    match (std::env::var_os("TKR_MEASUREMENT_LOGICAL_RAM_BYTES"), std::env::var_os("TKR_MEASUREMENT_LOGICAL_DB_BYTES")) {
        (None, None) => {}
        (Some(memory), Some(database)) => {
            let finite = |raw: std::ffi::OsString| {
                let bytes: u64 = raw.into_string().unwrap().parse().unwrap();
                assert!(bytes > 0 && bytes < u64::MAX);
                bytes
            };
            arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 {
                memory_bytes: finite(memory),
                database_bytes: finite(database),
            }));
        }
        _ => panic!("set both logical RAM and DB byte limits"),
    }
    // An offline bundle made by the seeded unit fixture, not a chain snapshot
    // authority or P2P input. Its expected roots are checked as test oracles.
    let manifest: Manifest = borsh::from_slice(&read_bounded(&dir.join(MANIFEST_FILE), 256 * 1024)).unwrap();
    assert_eq!(manifest.version, VERSION);
    assert!((4..=MAX_ACTIONS).contains(&manifest.actions.len()));
    let snapshot_before = usize::try_from(manifest.snapshot_before).unwrap();
    assert_eq!(snapshot_before + 1, manifest.actions.len());
    let initial: PalwStateCarriageV2 = borsh::from_slice(&read_bounded(&dir.join(STATE_FILE), 32 * 1024 * 1024)).unwrap();
    let mut state =
        restore_native_carriage_v1(manifest.initial.block, initial, &manifest.params, Some(manifest.initial_root), false, None)
            .unwrap();
    assert_eq!(state.last_point(), Some(&manifest.initial));
    assert_eq!(state.state_root(), manifest.initial_root);
    let policy_id = manifest.policy.id_v1().unwrap();
    let extras = manifest.extras();
    let mut metrics = BTreeMap::<&str, u128>::new();
    for (index, action) in manifest.actions.iter().enumerate() {
        assert_eq!(state.state_root(), action.parent_root);
        if index == snapshot_before {
            let start = Instant::now();
            let snapshot = borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).unwrap();
            metrics.insert("snapshot_encode_ns", start.elapsed().as_nanos());
            assert_eq!(snapshot.len() as u64, manifest.snapshot_bytes);
            let start = Instant::now();
            state = restore_native_carriage_v1(
                state.last_point().unwrap().block,
                borsh::from_slice(&snapshot).unwrap(),
                &manifest.params,
                Some(action.parent_root),
                false,
                None,
            )
            .unwrap();
            metrics.insert("snapshot_restore_ns", start.elapsed().as_nanos());
            assert_eq!(state.state_root(), action.parent_root);
        }

        let payload = read_bounded(&dir.join(action_file(index)), 96 * 1024);
        let kind = *payload.get(4).expect("carrier kind");
        assert_eq!(kind == 2, index == snapshot_before, "the final action is the single Finish");
        let start = Instant::now();
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(action.at.block, 0), vec![], 0, 0)],
            vec![TransactionOutput::new(10, ScriptPublicKey::default())],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        assert_eq!(tx.id(), action.txid);
        let populated = PopulatedTransaction::new(
            &tx,
            vec![UtxoEntry::new(action.paid.checked_add(10).unwrap(), ScriptPublicKey::default(), 0, false)],
        );
        let carrier = ValidatedTokenizerRegistrationCarrierV1::prepare_shared_v1(
            &ValidatedTransaction::new(populated, action.paid),
            TokenizerRegistrationCarrierContextV1 {
                parent_root: action.parent_root,
                at: action.at,
                accepted_ordinal: 0,
                network: manifest.network,
                policy: policy_id,
            },
            &manifest.policy,
            0,
        )
        .unwrap()
        .unwrap();
        *metrics.entry("handoff_ns").or_default() += start.elapsed().as_nanos();

        let start = Instant::now();
        let memo = TokenizerFinishDerivationCacheV1::new(&state, action.at, &manifest.policy).unwrap();
        assert_eq!(memo.parent_root_v1(), action.parent_root);
        let mut scratch = TokenizerRegistrationFeeWorkingStateV1::new(&state, &manifest.params, &action.at, &extras).unwrap();
        scratch.begin_body_v1(&manifest.params, &extras).unwrap();
        *metrics.entry("fee_base_ns").or_default() += start.elapsed().as_nanos();
        let start = Instant::now();
        let (fee, refusal) =
            memo.scope_v1(|| scratch.apply_v1(&state, action.parent_root, &manifest.params, &action.at, &carrier, &extras)).unwrap();
        assert_eq!(refusal, None);
        assert_eq!(
            (fee.actual_fee, fee.registration_burn, fee.collectible_fee, fee.charge_id),
            (action.paid, action.burn, action.collectible, action.charge_id)
        );
        let phase = if kind == 2 { "finish_cold_fee_rehearsal_ns" } else { "open_part_fee_rehearsal_ns" };
        *metrics.entry(phase).or_default() += start.elapsed().as_nanos();
        let start = Instant::now();
        scratch.begin_body_v1(&manifest.params, &extras).unwrap();
        *metrics.entry("fee_rollback_ns").or_default() += start.elapsed().as_nanos();
        drop(scratch);

        let mut authoritative = extras.clone();
        authoritative.tokenizer_registration_expected_fees_v1.insert(carrier.carrier_v1(), fee);
        authoritative.tokenizer_registration_carriers_v1.push((0, carrier));
        let start = Instant::now();
        let (next, delta, _) = memo
            .scope_v1(|| {
                apply_palw_transition_retained_v7(
                    &state,
                    &manifest.params,
                    None,
                    &action.at,
                    &[],
                    PalwBlockWorkV3::None,
                    &[],
                    Hash64::default(),
                    false,
                    false,
                    false,
                    false,
                    &authoritative,
                )
            })
            .unwrap();
        let phase = match kind {
            0 => "open_fold_ns",
            1 => "part_fold_ns",
            2 => "finish_cached_fold_ns",
            _ => panic!("unknown TKR1 action"),
        };
        *metrics.entry(phase).or_default() += start.elapsed().as_nanos();
        assert_eq!(next.state_root(), action.next_root);
        let bytes = borsh::object_length(&*delta).unwrap() as u64;
        assert_eq!(bytes, action.delta_bytes);
        *metrics.entry("journal_bytes").or_default() += u128::from(bytes);
        let start = Instant::now();
        let undo = revert_delta_v2(&next, &delta, &manifest.params).unwrap();
        let redo = apply_delta_v2(&undo, &delta, &manifest.params).unwrap();
        *metrics.entry("journal_undo_redo_ns").or_default() += start.elapsed().as_nanos();
        assert_eq!(undo.state_root(), action.parent_root);
        assert_eq!(redo.state_root(), action.next_root);
        drop(memo);
        state = next;
    }
    let status = |id| palw_tokenizer_registration_status_v1(&state, id).unwrap().unwrap().status.clone();
    assert!(matches!(status(manifest.open_ids[0]), TokenizerRegistrationStatusV1::Superseded { .. }));
    assert!(matches!(status(manifest.open_ids[1]), TokenizerRegistrationStatusV1::Complete { .. }));
    println!(
        "{}",
        json!({
            "case": manifest.case, "scope": "production native APIs on seeded accepted-parent inputs",
            "policyId": policy_id.to_string(), "initialRoot": manifest.initial_root.to_string(),
            "finalRoot": state.state_root().to_string(), "actions": manifest.actions.len(),
            "snapshotBytes": manifest.snapshot_bytes, "metrics": metrics,
            "nativeLogicalAdmissionEnabled": native_logical_admission_enabled_v1(),
            "nativeResourceAdmissionEnabled": native_resource_admission_enabled_v1(),
            "declaredLogicalLimits": native_declared_logical_limits_v1().map(|limits| json!({
                "memoryBytes": limits.memory_bytes, "databaseBytes": limits.database_bytes,
            })),

        })
    );
}
