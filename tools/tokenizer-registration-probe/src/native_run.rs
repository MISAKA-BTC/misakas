use crate::{
    native_bundle::{Action, MANIFEST_FILE, MAX_ACTIONS, Manifest, STATE_FILE, VERSION, action_file},
    native_candidate,
};
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{
        NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, native_declared_logical_limits_v1,
        native_logical_admission_enabled_v1, native_resource_admission_enabled_v1, restore_native_carriage_v1,
    },
    palw_state_v2::{
        PalwBlockWorkV3, PalwStateCarriageV2, TokenizerRegistrationFeeWorkingStateV1, apply_delta_v2,
        apply_palw_transition_retained_v7, palw_tokenizer_registration_status_v1, palw_v2_apply_one_tokenizer_registration_owned_v1,
        palw_v2_pre_object_base_retained_v1, revert_delta_v2,
    },
    palw_tokenizer_finish_derivation_cache_v1::TokenizerFinishDerivationCacheV1,
    palw_tokenizer_registration_carriage_v1::{
        TokenizerRegistrationCarrierContextV1, ValidatedTokenizerRegistrationCarrierV1, tokenizer_registration_parent_quote_v1,
    },
    palw_tokenizer_registration_state_v1::TokenizerRegistrationStatusV1,
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
    tx::{
        PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
        ValidatedTransaction,
    },
};
use serde_json::json;
use std::{collections::BTreeMap, fs::File, io::Read, path::Path, time::Instant};

pub(crate) fn read_bounded(path: &Path, limit: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    File::open(path).unwrap_or_else(|error| panic!("{}: {error}", path.display())).take(limit + 1).read_to_end(&mut bytes).unwrap();
    assert!(bytes.len() as u64 <= limit, "{} exceeds the measurement input bound", path.display());
    bytes
}

pub(crate) fn measurement_transaction(action: &Action, input_index: u32, payload: Vec<u8>) -> Transaction {
    Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint::new(action.at.block, input_index), vec![], 0, 0)],
        vec![TransactionOutput::new(10, ScriptPublicKey::default())],
        0,
        SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    )
}

pub fn run(dir: &Path, multipass: bool, candidate: bool) {
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
    if candidate {
        assert_eq!(manifest.policy.maximum_source_blocks, native_candidate::SOURCES);
    }
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
    let source = manifest.policy.source_budget_v1().unwrap();
    let mut maximum_equal_cost_finish_slots = None;
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
        if kind == 2 {
            let quote = tokenizer_registration_parent_quote_v1(&state, manifest.open_ids[1]).expect("authenticated Finish quote");
            let cost = quote.finish_cost;
            let upper_bound = [
                source.bytes / cost.bytes,
                source.work / cost.work,
                source.io / cost.io,
                source.controls / cost.controls,
                u64::from(manifest.policy.max_pending_total),
            ]
            .into_iter()
            .min()
            .unwrap();
            assert_eq!(source.controls, 1);
            assert_eq!(upper_bound, 1);
            maximum_equal_cost_finish_slots = Some(upper_bound);
        }
        let start = Instant::now();
        let memo = TokenizerFinishDerivationCacheV1::new(&state, action.at, &manifest.policy).unwrap();
        assert_eq!(memo.parent_root_v1(), action.parent_root);
        *metrics.entry("finish_cache_scope_setup_ns").or_default() += start.elapsed().as_nanos();
        let expanded = candidate && kind == 2;
        let authoritative = if expanded {
            native_candidate::prepare(&manifest, action, &state, &payload, &extras, &memo, &mut metrics)
        } else {
            let start = Instant::now();
            let tx = measurement_transaction(action, 0, payload);
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
            let mut scratch = TokenizerRegistrationFeeWorkingStateV1::new(&state, &manifest.params, &action.at, &extras).unwrap();
            scratch.begin_body_v1(&manifest.params, &extras).unwrap();
            *metrics.entry("fee_base_ns").or_default() += start.elapsed().as_nanos();
            let start = Instant::now();
            let (fee, refusal) = memo
                .scope_v1(|| scratch.apply_v1(&state, action.parent_root, &manifest.params, &action.at, &carrier, &extras))
                .unwrap();
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
            if multipass {
                let start = Instant::now();
                let mut scratch = TokenizerRegistrationFeeWorkingStateV1::new(&state, &manifest.params, &action.at, &extras).unwrap();
                scratch.begin_body_v1(&manifest.params, &extras).unwrap();
                *metrics.entry("fee_replay_base_ns").or_default() += start.elapsed().as_nanos();
                let start = Instant::now();
                let (repeat_fee, repeat_refusal) = memo
                    .scope_v1(|| scratch.apply_v1(&state, action.parent_root, &manifest.params, &action.at, &carrier, &extras))
                    .unwrap();
                assert_eq!((repeat_fee, repeat_refusal), (fee, None));
                let phase = if kind == 2 { "finish_cached_fee_replay_ns" } else { "open_part_fee_replay_ns" };
                *metrics.entry(phase).or_default() += start.elapsed().as_nanos();
                let start = Instant::now();
                scratch.begin_body_v1(&manifest.params, &extras).unwrap();
                *metrics.entry("fee_replay_rollback_ns").or_default() += start.elapsed().as_nanos();
            }

            if multipass {
                let mut observed_roots = [Hash64::default(); 2];
                let phases = if kind == 2 {
                    ["finish_cached_observed_rehearsal_ns", "finish_cached_observed_replay_ns"]
                } else {
                    ["open_part_observed_rehearsal_ns", "open_part_observed_replay_ns"]
                };
                for (root, phase) in observed_roots.iter_mut().zip(phases) {
                    let start = Instant::now();
                    let base =
                        palw_v2_pre_object_base_retained_v1(&state, &manifest.params, &action.at, false, false, false, false, &extras)
                            .unwrap();
                    *metrics.entry("observed_pre_object_base_ns").or_default() += start.elapsed().as_nanos();
                    let start = Instant::now();
                    let (observed, observed_fee, refusal) = memo
                        .scope_v1(|| {
                            palw_v2_apply_one_tokenizer_registration_owned_v1(
                                base,
                                &state,
                                action.parent_root,
                                &manifest.params,
                                &action.at,
                                &carrier,
                                false,
                                false,
                                false,
                                false,
                                &extras,
                            )
                        })
                        .unwrap();
                    *metrics.entry(phase).or_default() += start.elapsed().as_nanos();
                    assert_eq!((observed_fee, refusal), (fee, None));
                    *root = observed.state_root();
                }
                // The rehearsal prefix is not a settled block. Compare the two
                // prefixes here and verify the authoritative block below.
                assert_eq!(observed_roots[0], observed_roots[1]);
            }

            let mut authoritative = extras.clone();
            authoritative.tokenizer_registration_expected_fees_v1.insert(carrier.carrier_v1(), fee);
            authoritative.tokenizer_registration_carriers_v1.push((0, carrier));
            authoritative
        };
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
            2 if expanded => "candidate_authoritative_fold_ns",
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
    let status = |id| palw_tokenizer_registration_status_v1(&state, id).unwrap().unwrap();
    assert!(matches!(status(manifest.open_ids[0]).status, TokenizerRegistrationStatusV1::Superseded { .. }));
    let completed = status(manifest.open_ids[1]);
    let finish_at = manifest.actions.last().unwrap().at;
    assert!(matches!(completed.status, TokenizerRegistrationStatusV1::Complete { point, .. } if point == finish_at));
    assert_eq!(completed.released_at, Some(finish_at));
    assert_eq!(completed.finishes.iter().filter(|slot| slot.attempted).count(), 1);
    assert!(completed.finishes.iter().any(|slot| slot.attempted && slot.slot_daa == finish_at.daa_score));
    println!(
        "{}",
        json!({
            "case": manifest.case, "scope": "production native APIs on seeded accepted-parent inputs",
            "policyId": policy_id.to_string(), "initialRoot": manifest.initial_root.to_string(),
            "mode": if candidate { "modeled_native_candidate" } else if multipass { "single_carrier_multipass" } else { "single_carrier_replay" },
            "nativePassesPerAction": if multipass { 5 } else { 2 },
            "feeRollbacksPerAction": if multipass { 2 } else { 1 },
            "finishCandidate": candidate.then(|| json!({
                "physicalCarriers": native_candidate::SOURCES,
                "nativePassesPerCarrier": 5, "feeRollbacksPerCarrier": 2,
                "completedFinishes": 1, "terminalNoOpsInAuthoritativeFold": native_candidate::SOURCES - 1,
                "authoritativeFoldCount": 1, "consumedFinishSlots": 1,
                "finishDerivationKeys": 1,
            })),
            "finishCacheSharedWithinAction": true,
            "maximumSourceBlocksPolicy": manifest.policy.maximum_source_blocks,
            "sourceControls": source.controls,
            "maximumEqualCostFinishSlotsPerDaaUpperBound": maximum_equal_cost_finish_slots.unwrap(),
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
