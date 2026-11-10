#[allow(dead_code)]
mod tokenizer_native_bundle {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/tokenizer-registration-probe/src/native_bundle.rs"));
}

// Explicitly seeded accepted-parent measurements. This fixture times native
// admission and replay; it does not run UTXO script verification or a node.
#[test]
#[ignore = "opt-in native resource measurement"]
fn measure_tokenizer_registration_native_workload() {
    std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(run_tokenizer_registration_measurement).unwrap().join().unwrap();
}

fn run_tokenizer_registration_measurement() {
    use crate::{
        palw_class_semantics_v1::PalwClassSemanticsV1,
        palw_fp_constraint_job_v1::palw_token_table_file_decode_v1,
        palw_native_resource_v1::restore_native_carriage_v1,
        palw_state_v2::{
            PalwBlockWorkV3, PalwStateCarriageV2, TokenizerRegistrationFeeWorkingStateV1, apply_delta_v2,
            apply_palw_transition_retained_v7, palw_tokenizer_registration_status_v1, revert_delta_v2,
        },
        palw_tir_admission_v1::palw_tir_class_record_v1,
        palw_tir_attempt_v1::{palw_tir_attempt_canonical_v1, palw_tir_canonical_context_v1},
        palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1},
        palw_tir_container_v1::{check_tir_container_binding_v1, test_binding_v1},
        palw_tokenizer_artifact_inventory_v1::*,
        palw_tokenizer_finish_derivation_cache_v1::TokenizerFinishDerivationCacheV1,
        palw_tokenizer_registration_carriage_v1::*,
        palw_tokenizer_registration_state_v1::TokenizerRegistrationStatusV1,
        palw_tokenizer_registration_v1::*,
        palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1,
        palw_tokenizer_rendering_v2::{PalwTfp1TokenizerIntakePinsV2, PalwTokenizerRenderingStatementV2, ROOTED_TOKENIZER_LIMITS_V1},
        tx::{
            PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
            ValidatedTransaction,
        },
    };
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
        time::Instant,
    };
    use tokenizer_native_bundle::{Action, MANIFEST_FILE, MAX_ACTIONS, Manifest, STATE_FILE, VERSION, action_file};

    let (fx, synthetic_semantics, _, mut synthetic_raw, mut synthetic_table) =
        crate::palw_tir_public_claim_v1::palw_tir_public_tokenizer_frame_fixture_v1();
    let (_, params, mut extras) = world(&fx, &fx.frame);
    let cap = ROOTED_TOKENIZER_LIMITS_V1;
    let which = std::env::var("TKR_MEASUREMENT_CASE").unwrap_or_else(|_| "maximum_raw".into());
    let (class, artifact_root, raw, table) = match which.as_str() {
        "maximum_raw" => {
            synthetic_raw.resize(cap.raw_bytes as usize, b' ');
            synthetic_table.tokenizer_id = palw_tokenizer_raw_commitment_v1(&synthetic_raw);
            let mut layout = fx.layout.clone();
            layout.max_context = 64;
            let class = PalwTirClassV1 {
                version: PALW_TIR_CLASS_VERSION_V1,
                program: fx.record.program.as_ref().clone(),
                layout,
                tokenizer_id: synthetic_table.tokenizer_id,
            };
            let root = synthetic_semantics.artifact_root;
            (class, root, synthetic_raw, synthetic_table)
        }
        "actual" => {
            let read = |name| std::fs::read(std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))).unwrap();
            let raw = read("TKR_MEASUREMENT_RAW");
            let table = palw_token_table_file_decode_v1(&read("TKR_MEASUREMENT_TABLE")).unwrap();
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../src/pipeline/virtual_processor/tests/fixtures/g14/shipped/smollm2-1.7b.json");
            let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            let field = |key: &str| {
                let encoded = fixture[key].as_str().unwrap();
                let mut bytes = vec![0; encoded.len() / 2];
                faster_hex::hex_decode(encoded.as_bytes(), &mut bytes).unwrap();
                bytes
            };
            let hash = |key| Hash64::from_bytes(field(key).try_into().unwrap());
            let class = PalwTirClassV1 {
                version: PALW_TIR_CLASS_VERSION_V1,
                program: field("program_borsh_hex"),
                layout: borsh::from_slice(&field("layout_borsh_hex")).unwrap(),
                tokenizer_id: hash("tokenizer_id_hex"),
            };
            let root = hash("artifact_root_hex");
            let class_id = class.class_id(&root);
            assert_eq!(class_id, hash("class_id_hex"));
            (class, root, raw, table)
        }
        _ => panic!("unknown TKR_MEASUREMENT_CASE {which}"),
    };
    assert_eq!(class.tokenizer_id, table.tokenizer_id);
    assert_eq!(table.tokenizer_id, palw_tokenizer_raw_commitment_v1(&raw));
    let class_id = class.class_id(&artifact_root);
    let mut canonical = palw_tir_canonical_context_v1(&class, class_id, palw_tir_attempt_canonical_v1(&class).unwrap()).unwrap();
    canonical.tokenizer_id = class.tokenizer_id;
    let mut semantics = PalwClassSemanticsV1::of_registration(class_id, artifact_root, &canonical, 0);
    let (record, _) = palw_tir_class_record_v1(&class, &artifact_root).unwrap();
    assert_eq!((record.logits_vocab, record.layout_digest), (table.vocab(), class.layout_digest()));
    record.check_program_v1().unwrap();
    let binding = test_binding_v1(&record, artifact_root, b"tkr-native-measurement");
    let container = check_tir_container_binding_v1(&binding, &class_id, &artifact_root, &record, 1).unwrap();
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([166; 32]);
    let sign = |digest: Hash64, domain: &[u8]| {
        *libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, digest.as_byte_slice(), domain, [167; 32]).unwrap().as_ref()
    };
    let mut offer = PalwTokenizerArtifactInventoryOfferV1 {
        version: 1,
        artifact_hash: class.tokenizer_id,
        opener: outsider(),
        maximum_bytes: raw.len() as u64,
        accept_until_daa: 100,
        signature: Vec::new(),
    };
    offer.signature = sign(offer.id_v1(), PALW_ARTIFACT_INVENTORY_CONTEXT_V1).to_vec();
    let mut inventory = PalwTokenizerArtifactInventoryV1::new_v1(offer, 1, fx.rules.config).unwrap();
    for (index, bytes) in raw.chunks(PALW_ARTIFACT_INVENTORY_PART_BYTES_V1).enumerate() {
        let offset = (index * PALW_ARTIFACT_INVENTORY_PART_BYTES_V1) as u64;
        inventory.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Part { offset, bytes: bytes.to_vec() }, 2).unwrap();
    }
    let eof = inventory.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Finish, 3).unwrap().unwrap();
    let (_, mut body) = crate::palw_tokenizer_registration_v1::tests::fixture();
    let declared_events = eof.key_event_upper_bound_v1();
    semantics.token_table_root = Some(table.root());
    semantics.rendering_rule = Some(crate::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1);
    semantics.raw_tokenizer_bytes = Some(eof.actual_bytes);
    semantics.tokenizer_key_events = Some(declared_events);
    let pins = PalwTfp1TokenizerIntakePinsV2 {
        class_id,
        container_root: container.container_root,
        semantics_id: semantics.commitment(),
        inventory_id: eof.inventory_id,
    };
    let (mut policy, _) = crate::palw_tokenizer_registration_v1::tests::fixture();
    // Local measurement envelope with two independently paid sessions. A
    // source takes at most one indexed Part in this run.
    policy.max_raw_bytes = cap.raw_bytes;
    policy.max_vocab = cap.vocab;
    policy.max_key_events = cap.key_events;
    policy.max_eog_ids = cap.eog_ids;
    policy.max_rendering_bytes = cap.total_rendering_bytes;
    policy.max_parser_scratch_bytes = 2 * 1024 * 1024 * 1024;
    policy.max_pending_total = 2;
    policy.max_pending_per_opener = 2;
    policy.max_session_daa = 512;
    policy.max_retained_bytes = 64 * 1024 * 1024;
    policy.max_retained_total_bytes = 2 * policy.max_retained_bytes;
    policy.metadata_bytes = policy.metadata_floor_v1().unwrap();
    policy.max_work = 10_000_000_000;
    policy.max_registration_charge = 100_000_000_000;
    policy.service_budget.bytes = 128 * 1024;
    policy.service_budget.io = 128 * 1024 * 1024;
    policy.service_budget.work = policy.bookkeeping_work_v1().unwrap().sweep + policy.max_work;
    policy.service_budget.controls = 1;
    policy.check_source_budget_v1().unwrap();
    extras.class_semantics_active = true;
    extras.tir_container_identity_active = true;
    extras.tir_public_conviction_active = true;
    extras.staged_public_court = Some(fx.rules.config);
    extras.tokenizer_registration_policy_v1 = Some(policy);
    extras.tokenizer_registration_network_domain_v1 = Some(body.network);

    let mut state = PalwChainStateV2::genesis();
    let mut bond = bond_state(10);
    bond.pubkey = key.verification_key.as_ref().to_vec();
    state.bonds.insert(outsider(), bond);
    state.set_class_for_tests(
        class_id,
        PalwClassStateV2 {
            artifact_root,
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(10),
            status: PalwClassStatusV2::Active,
            registered_daa: 0,
            registrant_bond: None,
            fused_attention: false,
        },
    );
    state.class_shares.insert(class_id, 1000);
    state.tir_classes.insert(class_id, record);
    state.class_semantics.insert(class_id, semantics);
    let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    builder.write_staged_row_v1(Key::TirLayout(class_id), Some(Row::TirLayout(Box::new(class.layout)))).unwrap();
    builder.write_staged_row_v1(Key::TirContainer(class_id), Some(Row::TirContainer(Box::new(container)))).unwrap();
    builder.write_staged_row_v1(Key::Inventory(eof.artifact_hash), Some(Row::ArtifactInventory(eof))).unwrap();
    let point = |blue, daa| PalwBlockContextV2 { block: h(80_000 + blue), daa_score: daa, blue_score: blue, subsidy: 0 };
    builder.state.last_point = Some(point(1, 1_000));
    state = builder.state;
    body.policy = policy.id_v1().unwrap();
    body.pins = pins;
    body.artifact_root = artifact_root;
    body.statement = PalwTokenizerRenderingStatementV2 {
        tokenizer_id: table.tokenizer_id,
        exact_raw_bytes: raw.len() as u64,
        table_root: table.root(),
        vocab: table.vocab(),
        rule: crate::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
    };
    let eog = table.eog_opening().unwrap();
    body.eog_digest = TokenizerRegistrationFinishV1::eog_digest_v1(&eog).unwrap();
    body.opener = outsider();
    body.accept_until_daa = 1_010;
    body.finish_until_daa = 1_500;
    body.material_until_daa = body.finish_until_daa + policy.retention_daa;
    body.max_registration_charge = policy.max_registration_charge;
    drop(table);
    let first_id = body.id_v1().unwrap();
    let second = TokenizerRegistrationOpenBodyV1 { nonce: h(909), ..body };
    let open_id = second.id_v1().unwrap();
    let quote = policy.quote_v1(&body, 1_001, declared_events).unwrap();
    let mut export = std::env::var_os("TKR_MEASUREMENT_EXPORT").map(|dir| {
        let dir = PathBuf::from(dir);
        std::fs::create_dir(&dir).expect("measurement export requires a new directory");
        std::fs::write(dir.join(STATE_FILE), borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).unwrap()).unwrap();
        let manifest = Manifest {
            version: VERSION,
            case: which.clone(),
            params: params.clone(),
            policy,
            network: body.network,
            allocation_network: extras.tir_public_network_domain_v1.unwrap(),
            allocation: extras.tir_public_allocation_rules_v1.unwrap().into(),
            initial: *state.last_point().unwrap(),
            initial_root: state.state_root(),
            open_ids: [first_id, open_id],
            snapshot_before: 0,
            snapshot_bytes: 0,
            actions: Vec::new(),
        };
        (dir, manifest)
    });
    let mut metrics = BTreeMap::<&str, u128>::new();
    let mut run = |state: &PalwChainStateV2, at: PalwBlockContextV2, wire: TokenizerRegistrationWireV1, paid, label: &'static str| {
        let start = Instant::now();
        let parent_root = state.state_root();
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(at.block, 0), vec![], 0, 0)],
            vec![TransactionOutput::new(10, ScriptPublicKey::default())],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            encode_tokenizer_registration_payload_v1(&wire).unwrap(),
        );
        let populated = PopulatedTransaction::new(&tx, vec![UtxoEntry::new(paid + 10, ScriptPublicKey::default(), 0, false)]);
        let carrier = ValidatedTokenizerRegistrationCarrierV1::prepare_shared_v1(
            &ValidatedTransaction::new(populated, paid),
            TokenizerRegistrationCarrierContextV1 { parent_root, at, accepted_ordinal: 0, network: body.network, policy: body.policy },
            &policy,
            0,
        )
        .unwrap()
        .unwrap();
        *metrics.entry("handoff_ns").or_default() += start.elapsed().as_nanos();
        let start = Instant::now();
        let memo = TokenizerFinishDerivationCacheV1::new(state, at, &policy).unwrap();
        let mut scratch = TokenizerRegistrationFeeWorkingStateV1::new(state, &params, &at, &extras).unwrap();
        scratch.begin_body_v1(&params, &extras).unwrap();
        *metrics.entry("fee_base_ns").or_default() += start.elapsed().as_nanos();
        let start = Instant::now();
        let (fee, refusal) = memo.scope_v1(|| scratch.apply_v1(state, parent_root, &params, &at, &carrier, &extras)).unwrap();
        assert_eq!(refusal, None);
        let rehearsal_label =
            if label == "finish_cached_fold_ns" { "finish_cold_fee_rehearsal_ns" } else { "open_part_fee_rehearsal_ns" };
        *metrics.entry(rehearsal_label).or_default() += start.elapsed().as_nanos();
        let start = Instant::now();
        scratch.begin_body_v1(&params, &extras).unwrap();
        *metrics.entry("fee_rollback_ns").or_default() += start.elapsed().as_nanos();
        drop(scratch);
        let mut authoritative = extras.clone();
        authoritative.tokenizer_registration_expected_fees_v1.insert(carrier.carrier_v1(), fee);
        authoritative.tokenizer_registration_carriers_v1.push((0, carrier));
        let start = Instant::now();
        let (next, delta, _) = memo
            .scope_v1(|| {
                apply_palw_transition_retained_v7(
                    state,
                    &params,
                    None,
                    &at,
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
        *metrics.entry(label).or_default() += start.elapsed().as_nanos();
        let delta_bytes = borsh::object_length(&*delta).unwrap() as u64;
        *metrics.entry("journal_bytes").or_default() += u128::from(delta_bytes);
        let start = Instant::now();
        let undo = revert_delta_v2(&next, &delta, &params).unwrap();
        let redo = apply_delta_v2(&undo, &delta, &params).unwrap();
        *metrics.entry("journal_undo_redo_ns").or_default() += start.elapsed().as_nanos();
        assert_eq!(undo.state_root(), parent_root);
        assert_eq!(redo.state_root(), next.state_root());
        if let Some((dir, manifest)) = &mut export {
            let index = manifest.actions.len();
            assert!(index < MAX_ACTIONS);
            std::fs::write(dir.join(action_file(index)), &tx.payload).unwrap();
            manifest.actions.push(Action {
                at,
                txid: tx.id(),
                paid,
                burn: fee.registration_burn,
                collectible: fee.collectible_fee,
                charge_id: fee.charge_id,
                parent_root,
                next_root: next.state_root(),
                delta_bytes,
            });
        }
        (*next).clone()
    };

    for (index, open) in [body, second].into_iter().enumerate() {
        let wire = TokenizerRegistrationWireV1::Open(TokenizerRegistrationOpenV1 {
            body: open,
            signature: sign(open.id_v1().unwrap(), TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1),
        });
        state = run(&state, point(2 + index as u64, 1_001), wire, quote.charge + 7, "open_fold_ns");
    }
    let first = palw_tokenizer_registration_status_v1(&state, first_id).unwrap().unwrap();
    let last = palw_tokenizer_registration_status_v1(&state, open_id).unwrap().unwrap();
    let finish_slot = last.finishes[0].slot_daa;
    assert!(last.parts.iter().all(|part| first.finishes[1].slot_daa < part.slot_daa));
    assert!(finish_slot < body.finish_until_daa);
    for status in [first, last] {
        assert_eq!(status.parts.len(), raw.len().div_ceil(TOKENIZER_REGISTRATION_PART_BYTES_V1));
        // The scheduler orders independent packets by canonical packet ID,
        // so index order does not imply increasing assigned DAA.
        let mut scheduled = status.parts.clone();
        scheduled.sort_unstable_by_key(|part| part.slot_daa);
        for part_status in scheduled {
            let offset = usize::try_from(part_status.offset).unwrap();
            let bytes = &raw[offset..offset + part_status.bytes as usize];
            let mut part = TokenizerRegistrationPartV1 {
                body: TokenizerRegistrationPartBodyV1 {
                    version: body.version,
                    network: body.network,
                    policy: body.policy,
                    open_id: status.open_id,
                    opener: body.opener,
                    index: part_status.index,
                    offset: part_status.offset,
                    payload_digest: TokenizerRegistrationPartV1::payload_digest_v1(bytes).unwrap(),
                },
                signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
                bytes: bytes.to_vec(),
            };
            part.signature = sign(part.signing_message_v1().unwrap(), TOKENIZER_REGISTRATION_PART_CONTEXT_V1);
            let at = point(state.last_point().unwrap().blue_score + 1, part_status.slot_daa);
            state = run(&state, at, TokenizerRegistrationWireV1::Part(part), 7, "part_fold_ns");
        }
    }
    for id in [first_id, open_id] {
        let status = palw_tokenizer_registration_status_v1(&state, id).unwrap().unwrap();
        assert_eq!(status.status, TokenizerRegistrationStatusV1::Pending);
        assert!(status.parts.iter().all(|part| part.accepted.is_some()));
    }
    let start = Instant::now();
    let snapshot = borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).unwrap();
    let snapshot_encode_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let parent = restore_native_carriage_v1(
        state.last_point().unwrap().block,
        borsh::from_slice(&snapshot).unwrap(),
        &params,
        Some(state.state_root()),
        false,
        None,
    )
    .unwrap();
    let snapshot_restore_ns = start.elapsed().as_nanos();
    assert_eq!(parent.state_root(), state.state_root());
    let mut finish = TokenizerRegistrationFinishV1 {
        body: TokenizerRegistrationFinishBodyV1 {
            version: body.version,
            network: body.network,
            policy: body.policy,
            open_id,
            opener: body.opener,
            eog_digest: body.eog_digest,
            finish_until_daa: body.finish_until_daa,
        },
        signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        eog,
    };
    finish.signature = sign(finish.signing_message_v1().unwrap(), TOKENIZER_REGISTRATION_FINISH_CONTEXT_V1);
    state = run(
        &parent,
        point(parent.last_point().unwrap().blue_score + 1, finish_slot),
        TokenizerRegistrationWireV1::Finish(finish),
        7,
        "finish_cached_fold_ns",
    );
    let complete = palw_tokenizer_registration_status_v1(&state, open_id).unwrap().unwrap();
    assert!(matches!(complete.status, TokenizerRegistrationStatusV1::Complete { .. }));
    let superseded = palw_tokenizer_registration_status_v1(&state, first_id).unwrap().unwrap();
    assert!(matches!(superseded.status, TokenizerRegistrationStatusV1::Superseded { .. }));
    assert!(superseded.finishes.iter().all(|finish| !finish.attempted));
    if let Some((dir, mut manifest)) = export {
        manifest.snapshot_before = u32::try_from(manifest.actions.len() - 1).unwrap();
        manifest.snapshot_bytes = snapshot.len() as u64;
        std::fs::write(dir.join(MANIFEST_FILE), borsh::to_vec(&manifest).unwrap()).unwrap();
    }

    println!(
        "{}",
        serde_json::json!({
            "case": which, "scope": "seeded native accepted-parent fixture",
            "classId": class_id.to_string(), "artifactRoot": artifact_root.to_string(), "containerRoot": pins.container_root.to_string(),
            "tokenizerId": body.statement.tokenizer_id.to_string(), "tableRoot": body.statement.table_root.to_string(),
            "semanticsId": pins.semantics_id.to_string(), "inventoryId": pins.inventory_id.to_string(),
            "policyId": body.policy.to_string(), "policyBorshHex": faster_hex::hex_string(&borsh::to_vec(&policy).unwrap()),
            "openIds": [first_id.to_string(), open_id.to_string()],
            "parts": quote.geometry.part_count, "rawBytes": raw.len(), "vocab": body.statement.vocab,
            "concurrentOpens": 2, "retainedRawBytes": 2 * raw.len(), "snapshotBytes": snapshot.len(), "snapshotEncodeNs": snapshot_encode_ns,
            "snapshotRestoreNs": snapshot_restore_ns, "chargedWork": quote.work, "metrics": metrics,
        })
    );
}
