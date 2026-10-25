//! Tiny native TypedKernelV4 ingress/retention mechanism, not K3 or a campaign.
use super::*;
#[path = "material_typed_pruning_v4.rs"]
mod pruning;
#[path = "material_typed_reorg_v4.rs"]
mod reorg;
#[path = "material_typed_terminal_v4.rs"]
mod terminal;
#[path = "material_typed_source_manifest_v4.rs"]
mod source_manifest;
#[path = "signed_provider_challenge_v4.rs"]
mod signed_provider;
use kaspa_consensus_core::{
    api::ConsensusApi,
    palw_kernel_material_context_v4::PalwKernelMaterialContextLimitsV4,
    palw_material_capacity_v1::{PalwMaterialCodecV1, PalwMonetaryMaterialDemandV1},
    palw_material_release_recovery_v2::{PalwMaterialRecoveryActionV2 as Recovery, material_recovery_object_v2},
    palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as Monetary,
    palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1},
    palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4,
};
use misaka_palw_remote::monetary_material::DurableMonetaryMaterialProviderV1 as Provider;
use misaka_palw_sdk::claim_paid_v2::{
    PaidClaimMaterialWireV2,
    provider_handoff_v4::{ProviderHandoffLimitsV4, build_kernel_provider_handoff_from_candidate_v4},
};

// Reuse the explicit private policy values of the legacy mechanism control,
// but activate before work: this fixture must satisfy prospective admission.
fn config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    if signed_provider::enabled() {
        // Same pre-derivation mechanics-fixture eligibility as material_gc.
        // This class seam supplies no selected-verifier receipt or approval.
        opv_test_eligible(&opv_admitted());
        return signed_provider::config();
    }
    let (c, _, premine, floats) = super::material_gc_v4::config();
    let mut p = c.params.clone();
    p.palw_provider_court_v1 = Some(ForkActivation::new(1));
    p.palw_reporter_income_v2 = Some(ForkActivation::new(1));
    p.palw_material_income_v3 = Some(ForkActivation::new(1));
    // Active provider court refuses clear v1/v2 model claims. This private
    // prospective fixture uses the existing commitment-only court rule, as the
    // paid-source native fixtures do; public presets remain dormant.
    p.palw_court_commitment_scope_v1 = Some(ForkActivation::new(1));
    p.palw_monetary_material_v1.as_mut().unwrap().activation = ForkActivation::new(1);
    p.palw_material_release_recovery_v2.as_mut().unwrap().activation = ForkActivation::new(1);
    p.sync_palw_bond_budget_v1();
    assert!(p.validate_palw_monetary_material_v1().is_err());
    assert!(p.validate_palw_material_release_recovery_v2().is_err());
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { unreachable!() };
    let bundle = bundle.clone();
    let config = Config::new(p);
    let (config, premine) = if std::env::var("MISAKA_TYPED_PRUNING_CHILD_V4").as_deref() == Ok("1") {
        pruning::private_funding_config(config, premine)
    } else {
        (config, premine)
    };
    (config, bundle, premine, floats)
}
struct ProducerSigner(Vec<u8>);
impl kaspa_pq_validator_core::MessageSigner for ProducerSigner {
    fn public_key(&self) -> &[u8] {
        &self.0
    }
    fn sign_message(&self, msg: &[u8], domain: &[u8]) -> Result<Vec<u8>, String> {
        let key = TestConsensus::palw_v2_registry_keypair(0);
        libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, msg, domain, [0xB8; 32])
            .map(|s| s.as_ref().to_vec())
            .map_err(|e| format!("{e:?}"))
    }
}
fn material(fx: &Fixture, trace: &TraceV1, tokens: Vec<u32>) -> PaidClaimMaterialWireV2 {
    let mut m = PaidClaimMaterialWireV2 { tokens, ..Default::default() };
    for (p, position) in trace.values.iter().enumerate() {
        for (o, occurrence) in position.iter().enumerate() {
            for (n, value) in occurrence.iter().enumerate() {
                m.nodes.insert((0, p as u32, o as u16, n as u16), TensorWireV1::of(value));
            }
        }
    }
    for (p, inputs) in trace.inputs.iter().enumerate() {
        for (i, value) in inputs.iter().enumerate() {
            m.inputs.insert((0, p as u32, i as u16), TensorWireV1::of(value));
        }
    }
    for ((i, layer), tensor) in &fx.params.tensors {
        m.params.insert((0, *i, *layer), TensorWireV1::of(tensor));
    }
    m
}
fn signed_capsule(object: &Obj) -> &Recovery {
    let Obj::MonetaryMaterialActionV1 { action, .. } = object else { panic!("actual signed monetary carrier") };
    let Monetary::RecoveryV2(capsule) = action.as_ref() else { panic!("actual signed recovery carrier") };
    capsule
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct ReaderInput {
    request: kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialRequestV4,
    expected_tip: Hash64,
    simulated_time: u64,
    nonce: u64,
    current_read: bool,
    replacement: bool,
    rollback: bool,
}
#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct SignedAdmission {
    request: Hash64,
    consent: Hash64,
    object: Obj,
}
#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct SignedReplacement {
    intent: kaspa_consensus_core::palw_kernel_material_replacement_v4::PalwKernelMaterialReplacementIntentV4,
    consent: Hash64,
    object: Obj,
}
fn child_name(name: &str) -> String {
    format!("{}::{name}", module_path!().split_once("::").unwrap().1)
}

#[test]
fn rfc08_typed_provider_exact_publication_native_acceptance_and_cold_restore() {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &child_name("typed_writer_child"), "--nocapture", "--test-threads=1"])
        .env("MISAKA_TYPED_WRITER_CHILD_V4", "1")
        .status()
        .unwrap();
    assert!(status.success(), "native TypedKernelV4 role fixture failed: {status}");
}

#[test]
fn rfc08_typed_replacement_exact_request_native_acceptance_and_historical_cold_read() {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &child_name("typed_writer_child"), "--nocapture", "--test-threads=1"])
        .env("MISAKA_TYPED_WRITER_CHILD_V4", "1")
        .env("MISAKA_TYPED_REPLACEMENT_CHILD_V4", "1")
        .status()
        .unwrap();
    assert!(status.success(), "native TypedKernelV4 replacement role fixture failed: {status}");
}

// The writer deliberately has no native resource limit configured. Configuring
// global native DB admission on macOS correctly refuses unsupported hard quota.
// Acquisition-only readers declare local metadata admission in OTHER processes;
// no native state installation occurs there after admission is armed.
fn reader_roundtrip(
    w: World,
    db: std::sync::Arc<kaspa_database::prelude::DB>,
    dir: &std::path::Path,
    request: &kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialRequestV4,
    current_read: bool,
    replacement: bool,
    rollback: bool,
) -> (World, std::sync::Arc<kaspa_database::prelude::DB>) {
    use kaspa_database::prelude::ConnBuilder;
    assert!(kaspa_consensus_core::palw_native_resource_v1::native_declared_logical_limits_v1().is_none());
    let World { net, fx, class, jobs } = w;
    let Net { chain, config, bundle, premine, floats, funding, domain, rnd, mut _keep } = net;
    let input = ReaderInput {
        request: request.clone(),
        expected_tip: chain.tip_state().0,
        simulated_time: chain.ctx.simulated_time,
        nonce: chain.nonce_for_reopen(),
        current_read,
        replacement,
        rollback,
    };
    std::fs::write(dir.join("reader-input"), borsh::to_vec(&input).unwrap()).unwrap();
    let db_path = db.path().to_path_buf();
    let weak = std::sync::Arc::downgrade(&db);
    drop(chain); // joins all native workers before handing the DB to another process
    drop(db);
    assert_eq!(weak.strong_count(), 0, "writer must release every actual DB handle before reader opens it");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &child_name("typed_reader_child"), "--nocapture", "--test-threads=1"])
        .env("MISAKA_TYPED_READER_DIR_V4", dir)
        .env("MISAKA_TYPED_READER_DB_V4", &db_path)
        .status()
        .unwrap();
    assert!(status.success(), "acquisition-only native reader failed: {status}");
    // Successful child exit means its actual RocksDB handle is closed before
    // this writer reopens it. RocksDB's own exclusive lock also enforces this.
    let db = ConnBuilder::default().with_files_limit(10).with_db_path(db_path).with_create_if_missing(false).build().unwrap();
    let mut resumed = config.clone();
    resumed.process_genesis = false;
    let (sender, receiver) = async_channel::unbounded();
    let node = TestConsensus::with_db(db.clone(), &resumed, sender);
    let chain = t12_reopened_chain(node, &resumed, &bundle, input.simulated_time, input.nonce);
    assert_eq!(chain.tip_state().0, input.expected_tip, "read-only phase cannot move native state");
    _keep.push(Box::new(receiver));
    let net = Net { chain, config, bundle, premine, floats, funding, domain, rnd, _keep };
    (World { net, fx, class, jobs }, db)
}

#[tokio::test]
async fn typed_writer_child() {
    if std::env::var("MISAKA_TYPED_WRITER_CHILD_V4").as_deref() != Ok("1") {
        return;
    }
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    assert!(kaspa_consensus_core::palw_native_resource_v1::native_declared_logical_limits_v1().is_none());
    let (_life, mut db) = if std::env::var("MISAKA_TYPED_PRUNING_CHILD_V4").as_deref() == Ok("1") {
        // Retain this expensive long-run source for cold diagnosis and a later
        // real pruned-import role. The ordinary quick fixtures keep temp DBs.
        let parent = std::env::temp_dir().join("rusty-kaspa");
        std::fs::create_dir_all(&parent).unwrap();
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = parent.join(format!("typed-pruning-retained-{}-{unique}", std::process::id()));
        eprintln!("TYPED_PRUNING_V4 retained_database={path:?} cleanup=explicit");
        kaspa_database::create_permanent_db!(path, ConnBuilder::default().with_files_limit(10))
    } else {
        create_temp_db!(ConnBuilder::default().with_files_limit(10))
    };
    let (tx, rx) = async_channel::unbounded();
    let mut net = Net::over_cfg(config(), |c| TestConsensus::with_db(db.clone(), c, tx));
    net._keep.push(Box::new(rx));
    if std::env::var("MISAKA_TYPED_PRUNING_CHILD_V4").as_deref() == Ok("1") {
        pruning::started(&net, db.path());
    }
    let mut w = World::on_opv(net).await;
    let job = w.job().await;
    let paid = if signed_provider::enabled() { Some(signed_provider::paid_job(&mut w, &job).await) } else { None };
    let ledger = w.net.ledger();
    assert_eq!(ledger.commitment_scope_from_daa, Some(1), "native route persisted the explicit private commitment-only court fence");
    let mut scope_control = ledger.clone();
    scope_control.court_scope_policy = Some(misaka_palw_kernel::court_scope::CourtScopePolicyV1 { activation_daa: 1 });
    scope_control.class_court_scope_v1(&w.class).expect("actual commitment-only class scope stands under active provider court");
    scope_control.commitment_scope_from_daa = None;
    let refused_scope = scope_control.class_court_scope_v1(&w.class).unwrap_err();
    assert!(refused_scope.starts_with("COURT_SCOPE_INCOMPLETE"));
    eprintln!("TYPED_INGRESS_V4 without_commitment_scope={refused_scope}");
    drop(scope_control);
    let generated = greedy(&w.fx, &ledger, &w.class, &job.prompt, job.max_new_tokens as usize);
    let produced = produce(&w.fx, &ledger, &w.class, &job, w.net.kid(0), generated, |_| {});
    let root = Hash64::from_bytes(produced.claim.id());
    let (seal, reveal) = seal_and_reveal(&ledger, w.net.kid(0), &produced.object);
    let K::CommitClaimSalted { salt, commit } = &reveal else { panic!("selected verifier fence requires salted claim") };
    let object = w.net.route(0, &seal);
    w.net.send(vec![(0, object)]).await;
    assert!(!w.net.ledger().claims.contains_key(&root.as_bytes()));
    let checkpoint = w.net.chain.tip_state().0;
    let commit_daa = w.net.daa() + 20;
    let expires = commit_daa + 60;
    let native: &dyn ConsensusApi = w.net.chain.ctx.consensus.as_ref();
    let candidate = native.palw_kernel_producer_candidate_v4(checkpoint, w.net.bond(0), commit, salt, commit_daa, expires).unwrap();
    let signer = ProducerSigner(candidate.producer_pubkey().to_vec());
    let mut providers = vec![w.net.bond(2), w.net.bond(3)];
    providers.sort();
    let tokens = job.prompt.iter().chain(produced.claim.generated.iter().take(produced.claim.generated.len() - 1)).copied().collect();
    let public = material(&w.fx, &produced.trace, tokens);
    let handoff = build_kernel_provider_handoff_from_candidate_v4(
        candidate,
        providers,
        commit.clone(),
        *salt,
        &public,
        ProviderHandoffLimitsV4 { max_material_bytes: 4 << 20, max_workspace_bytes: 128 << 20, max_units: 65536 },
        &signer,
    )
    .unwrap();
    let request = handoff.request();
    assert!(matches!(request.bundle, PalwProspectiveKernelBundleV4::TypedKernelV4(_)));
    let dir = std::env::temp_dir().join(format!("misaka-native-typed-{}-{root}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    (w, db) = reader_roundtrip(w, db, &dir, request, false, false, false);
    let admission: SignedAdmission = borsh::from_slice(&std::fs::read(dir.join("signed-admission")).unwrap()).unwrap();
    assert_eq!(admission.request, request.id());
    let capsule = signed_capsule(&admission.object);
    let Recovery::FundedEnroll { monetary, .. } = capsule else { unreachable!() };
    assert_eq!(monetary.id(), admission.consent);
    let native: &dyn ConsensusApi = w.net.chain.ctx.consensus.as_ref();
    assert!(
        !native.palw_kernel_material_consent_accepted_v4(root, admission.consent).unwrap(),
        "provider preparation does not write native acceptance"
    );
    w.net.send(vec![(0, admission.object.clone())]).await;
    let native: &dyn ConsensusApi = w.net.chain.ctx.consensus.as_ref();
    assert!(
        native.palw_kernel_material_consent_accepted_v4(root, admission.consent).unwrap(),
        "actual signed FundedEnroll accepted before claim"
    );
    assert!(native.palw_kernel_material_admission_accepted_v4(request, capsule).unwrap());
    assert!(!w.net.ledger().claims.contains_key(&root.as_bytes()), "funding does not commit work");
    w.net.beat_to(commit_daa).await;
    // Diagnostics only: these owners never replace the original signed work,
    // request, capsule or carrier. Native acceptance below remains mandatory.
    let native: &dyn ConsensusApi = w.net.chain.ctx.consensus.as_ref();
    let current_checkpoint = w.net.chain.tip_state().0;
    let virtual_daa = native.get_virtual_daa_score();
    let quote_daa = virtual_daa.checked_add(1).unwrap();
    let quote = native.palw_kernel_producer_candidate_v4(current_checkpoint, w.net.bond(0), commit, salt, quote_daa, expires);
    eprintln!(
        "TYPED_INGRESS_V4 request_commit_daa={commit_daa} sink_daa={} virtual_daa={virtual_daa} checkpoint={current_checkpoint} quote_daa={quote_daa} native_quote={:?}",
        w.net.daa(),
        quote.as_ref().map(|_| ()).map_err(|why| why.as_str()),
    );
    if let Ok(candidate) = &quote {
        let row = &candidate.ledger().claims[&root.as_bytes()];
        w.net
            .chain
            .tip_state()
            .1
            .require_kernel_monetary_material_scope_v1(&w.net.bundle.state, root, row, quote_daa)
            .expect("the actual funded signed TypedKernelV4 manifest binds this prospective claim body");
    }
    drop(quote);
    let mut observed = w.net.ledger();
    eprintln!(
        "TYPED_INGRESS_V4 ledger_daa={} seal={:?} producer={:?} native_consent={:?} native_admission={:?}",
        observed.daa,
        observed.seals.get(&(job.id(), w.net.kid(0))),
        observed.bonds.get(&w.net.kid(0)),
        native.palw_kernel_material_consent_accepted_v4(root, admission.consent),
        native.palw_kernel_material_admission_accepted_v4(request, capsule),
    );
    observed.begin_block(quote_daa).unwrap();
    let diagnostic_replay = observed.apply_object(&reveal, &misaka_palw_kernel::ledger::AuthV1 { signer_bond: w.net.kid(0) });
    eprintln!("TYPED_INGRESS_V4 observed_ledger_replay={:?}", diagnostic_replay.as_ref().map(|events| events.len()));
    drop(observed);
    let committing = match paid { Some((class, job)) => signed_provider::paid_commit(&w, class, job, commit, *salt, admission.consent), None => reveal.clone() };
    let object = w.net.route(0, &committing);
    let folded = w.net.send(vec![(0, object)]).await;
    let native: &dyn ConsensusApi = w.net.chain.ctx.consensus.as_ref();
    let carried = native.get_ghostdag_data(folded.hash()).unwrap().selected_parent;
    eprintln!(
        "TYPED_INGRESS_V4 carrying_block={carried} carrying_header_daa={} folded_block={} folded_header_daa={} actual_sink={} actual_virtual_daa={}",
        native.get_header(carried).unwrap().daa_score,
        folded.hash(),
        folded.header.daa_score,
        native.get_sink(),
        native.get_virtual_daa_score(),
    );
    assert!(
        w.net.ledger().claims.contains_key(&root.as_bytes()),
        "actual claim passes active material gate after funded TypedKernelV4 acceptance"
    );
    if signed_provider::enabled() {
        signed_provider::finish(w, db, &dir, &committing, &public, commit.clone(), root, admission.consent).await;
        return;
    }
    (w, db) = reader_roundtrip(w, db, &dir, request, true, false, false);
    assert!(w.net.chain.ctx.consensus.palw_kernel_material_consent_accepted_v4(root, admission.consent).unwrap());
    if std::env::var("MISAKA_TYPED_REPLACEMENT_CHILD_V4").as_deref() == Ok("1") {
        (w, db) = reader_roundtrip(w, db, &dir, request, false, true, false);
        let replacement: SignedReplacement = borsh::from_slice(&std::fs::read(dir.join("signed-replacement")).unwrap()).unwrap();
        assert_eq!(replacement.intent.context_request, request.id());
        assert_eq!(replacement.intent.previous, admission.consent);
        assert_eq!(replacement.intent.sequence, 1);
        let Recovery::ReplaceBeforeV4 { valid_until_daa, .. } = signed_capsule(&replacement.object) else { unreachable!() };
        assert_eq!(*valid_until_daa, replacement.intent.valid_until_daa);
        assert_eq!(signed_capsule(&replacement.object).bounded_replacement_consent_v4(), Some(replacement.consent));
        assert!(!w.net.chain.ctx.consensus.palw_kernel_material_consent_accepted_v4(root, replacement.consent).unwrap());
        let saved = if std::env::var("MISAKA_TYPED_REORG_CHILD_V4").as_deref() == Ok("1") {
            let fork = w.net.chain.sink();
            let carrier = w.net.carrier(0, &replacement.object);
            let bytes = borsh::to_vec(&carrier).unwrap();
            reorg::send_saved_carrier(&mut w.net, &carrier).await;
            Some((fork, carrier, bytes))
        } else {
            w.net.send(vec![(0, replacement.object)]).await;
            None
        };
        let current = w.net.chain.tip_state().1.monetary_material_row_v1(root).expect("actual accepted replacement row");
        assert_eq!(current.sequence, 1);
        assert_eq!(current.consent, replacement.consent);
        assert!(
            misaka_palw_remote::monetary_material::replacement_generation_observed_v4(
                &replacement.intent,
                Some(replacement.consent),
                &current,
            )
            .unwrap()
        );
        assert!(w.net.chain.ctx.consensus.palw_kernel_material_consent_accepted_v4(root, replacement.consent).unwrap());
        assert!(
            !w.net.chain.ctx.consensus.palw_kernel_material_consent_accepted_v4(root, admission.consent).unwrap(),
            "the old consent is historical, not the current monetary generation"
        );
        (w, db) = reader_roundtrip(w, db, &dir, request, true, true, false);
        assert_eq!(w.net.chain.tip_state().1.monetary_material_row_v1(root).unwrap().consent, replacement.consent);
        if let Some((fork, carrier, bytes)) = saved {
            (w, db) =
                reorg::rollback_and_restore(w, db, &dir, request, admission.consent, replacement.consent, fork, carrier, &bytes).await;
        }
    }
    if std::env::var("MISAKA_TYPED_TERMINAL_CHILD_V4").as_deref() == Ok("1") {
        (w, db) = terminal::release_and_collect(w, db, &dir, request).await;
    }
    if std::env::var("MISAKA_TYPED_PRUNING_CHILD_V4").as_deref() == Ok("1") {
        source_manifest::retain(&w, db.path(), &dir);
    }
    drop(w);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn typed_reader_child() {
    let Some(dir) = std::env::var_os("MISAKA_TYPED_READER_DIR_V4").map(std::path::PathBuf::from) else {
        return;
    };
    use kaspa_database::prelude::ConnBuilder;
    kaspa_core::log::try_init_logger("warn");
    assert!(kaspa_consensus_core::palw_native_resource_v1::native_declared_logical_limits_v1().is_none());
    let input: ReaderInput = borsh::from_slice(&std::fs::read(dir.join("reader-input")).unwrap()).unwrap();
    let db_path = std::path::PathBuf::from(std::env::var_os("MISAKA_TYPED_READER_DB_V4").unwrap());
    let db = ConnBuilder::default().with_files_limit(10).with_db_path(db_path).with_create_if_missing(false).build().unwrap();
    let weak = std::sync::Arc::downgrade(&db);
    let (mut config, bundle, _, _) = config();
    config.process_genesis = false;
    let (tx, rx) = async_channel::unbounded();
    let node = TestConsensus::with_db(db.clone(), &config, tx);
    let chain = t12_reopened_chain(node, &config, &bundle, input.simulated_time, input.nonce);
    let native: &dyn ConsensusApi = chain.ctx.consensus.as_ref();
    let root = input.request.intent.manifest.root;
    assert_eq!(chain.tip_state().0, input.expected_tip);
    assert_eq!(native.get_sink(), input.expected_tip);
    let before_daa = native.get_virtual_daa_score();
    // Warm the own accepted state while all native workers are ready, before
    // acquisition-only logical admission. This child submits NO block/object.
    let was_accepted = native.palw_kernel_material_consent_accepted_v4(root, Hash64::default()).unwrap();
    assert!(!was_accepted);
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 4 << 30, database_bytes: 4 << 30 }));
    let limits = PalwKernelMaterialContextLimitsV4 { max_encoded_bytes: 64 << 20, max_heap_bytes: 128 << 20 };
    if dir.join("terminal-input").exists() {
        terminal::read_and_collect(native, &input, &dir, limits);
    } else if input.replacement && !input.current_read {
        prepare_replacement_reader(&chain, &input, &dir, limits);
    } else if input.current_read {
        let admission: SignedAdmission = borsh::from_slice(&std::fs::read(dir.join("signed-admission")).unwrap()).unwrap();
        assert_eq!(admission.request, input.request.id());
        let replacement = input
            .replacement
            .then(|| borsh::from_slice::<SignedReplacement>(&std::fs::read(dir.join("signed-replacement")).unwrap()).unwrap());
        if let Some(replacement) = &replacement {
            assert_eq!(native.palw_kernel_material_consent_accepted_v4(root, replacement.consent).unwrap(), !input.rollback);
            assert_eq!(native.palw_kernel_material_consent_accepted_v4(root, admission.consent).unwrap(), input.rollback);
        } else {
            assert!(native.palw_kernel_material_consent_accepted_v4(root, admission.consent).unwrap());
        }
        for card in [2, 3, 4, 5].into_iter().filter(|card| *card < 4 || replacement.is_some()) {
            let file = dir.join(format!("provider-{card}"));
            let mut store = Provider::open_with_kernel_contexts_v4(&file, 4 << 20, 16 << 20, |_| None, limits).unwrap();
            let provider = chain.bonds[card];
            let key = TestConsensus::palw_v2_registry_keypair(card as u64);
            let exact_consent = if card < 4 { admission.consent } else { replacement.as_ref().unwrap().consent };
            if input.rollback {
                reorg::rolled_back_provider_read(
                    &mut store,
                    native,
                    &input,
                    provider,
                    key.verification_key.as_ref(),
                    exact_consent,
                    card >= 4,
                );
            } else {
                store
                    .answer_material_exact_current_v4(
                        exact_consent,
                        root,
                        root,
                        provider,
                        key.verification_key.as_ref(),
                        PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 },
                        native,
                    )
                    .unwrap();
            }
        }
    } else {
        let ingress = native.palw_kernel_material_ingress_v4(input.request.intent.checkpoint, &input.request).unwrap();
        let Recovery::FundedEnroll { monetary, .. } = &*ingress.enrollment else { unreachable!() };
        assert!(
            monetary.terms.material_manifest.as_ref().unwrap().scopes.iter().all(|s| s.codec == PalwMaterialCodecV1::TypedKernelV4)
        );
        let consent = monetary.id();
        let mut stores = Vec::new();
        for card in [2, 3] {
            let file = dir.join(format!("provider-{card}"));
            let mut store = Provider::open_with_kernel_contexts_v4(&file, 4 << 20, 16 << 20, |_| None, limits).unwrap();
            let provider = chain.bonds[card];
            if card == 2 {
                let before = (std::fs::read(&file).ok(), std::fs::read(file.with_extension("kernel-context-v4")).ok());
                let mut wrong_context = (*ingress.context).clone();
                wrong_context.request = Hash64::from_bytes([0xFF; 64]);
                assert!(
                    store
                        .prepare_prospective_kernel_v4(
                            provider,
                            monetary,
                            &ingress.verified,
                            &input.request.bundle,
                            &wrong_context,
                            &ingress.enrollment
                        )
                        .is_err()
                );
                let mut wrong_manifest = (*ingress.context).clone();
                wrong_manifest.intent.manifest.root = Hash64::from_bytes([0xFE; 64]);
                assert_eq!(wrong_manifest.request, ingress.verified.request_id());
                assert_eq!(
                    store
                        .prepare_prospective_kernel_v4(
                            provider,
                            monetary,
                            &ingress.verified,
                            &input.request.bundle,
                            &wrong_manifest,
                            &ingress.enrollment
                        )
                        .unwrap_err(),
                    "prospective publication substitutes native request/context/financial consent"
                );
                let Recovery::FundedEnroll { terms, providers, .. } = &*ingress.enrollment else { unreachable!() };
                let wrong_kind = Recovery::Enroll { terms: terms.clone(), providers: providers.clone() };
                assert!(
                    store
                        .prepare_prospective_kernel_v4(
                            provider,
                            monetary,
                            &ingress.verified,
                            &input.request.bundle,
                            &ingress.context,
                            &wrong_kind
                        )
                        .is_err()
                );
                let mut wrong_capsule = (*ingress.enrollment).clone();
                let Recovery::FundedEnroll { monetary: wrong, .. } = &mut wrong_capsule else { unreachable!() };
                wrong.offers[0].prepaid_fee_sompi += 1;
                assert!(
                    store
                        .prepare_prospective_kernel_v4(
                            provider,
                            monetary,
                            &ingress.verified,
                            &input.request.bundle,
                            &ingress.context,
                            &wrong_capsule
                        )
                        .is_err()
                );
                assert!(store.held_contracts().is_empty());
                assert_eq!(before, (std::fs::read(&file).ok(), std::fs::read(file.with_extension("kernel-context-v4")).ok()));
            }
            assert_eq!(
                store
                    .prepare_prospective_kernel_v4(
                        provider,
                        monetary,
                        &ingress.verified,
                        &input.request.bundle,
                        &ingress.context,
                        &ingress.enrollment
                    )
                    .unwrap(),
                consent
            );
            let key = TestConsensus::palw_v2_registry_keypair(card as u64);
            assert!(
                store
                    .answer_material_exact_current_v4(
                        consent,
                        root,
                        root,
                        provider,
                        key.verification_key.as_ref(),
                        PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 },
                        native
                    )
                    .is_err()
            );
            stores.push(store);
        }
        // Every provider has completed actual durable preparation before ANY
        // provider signature is generated. This is not a mocked receipt.
        let network = ingress.network;
        let object = material_recovery_object_v2(network, (*ingress.enrollment).clone(), chain.bonds[0], |bond, msg, domain| {
            let card = chain.bonds.iter().position(|b| b == bond)?;
            let key = TestConsensus::palw_v2_registry_keypair(card as u64);
            libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, msg, domain, [0xB8; 32]).ok().map(|s| s.as_ref().to_vec())
        })
        .unwrap();
        for store in &mut stores {
            store.persist_kernel_signatures_v4(&ingress.context, signed_capsule(&object)).unwrap();
        }
        std::fs::write(
            dir.join("signed-admission"),
            borsh::to_vec(&SignedAdmission { request: input.request.id(), consent, object }).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(native.get_virtual_daa_score(), before_daa);
    assert_eq!(native.get_sink(), input.expected_tip, "acquisition-only reader cannot advance accepted sink");
    {
        let (tip, _retained_state) =
            chain.vp().palw_state_v2_store.read().load_tip_retained_v1(&bundle.state).unwrap().expect("own accepted tip");
        assert_eq!(tip, input.expected_tip, "read-only phase retains the actual state owner/ticket");
    }
    drop(chain);
    drop(db);
    drop(rx);
    assert_eq!(weak.strong_count(), 0, "reader releases native workers and every DB handle before process exit");
}

fn prepare_replacement_reader(
    chain: &T12Chain,
    input: &ReaderInput,
    dir: &std::path::Path,
    limits: PalwKernelMaterialContextLimitsV4,
) {
    use kaspa_consensus_core::{
        palw_kernel_material_enrollment_v4::PalwMaterialGenerationAuthorityErrorV4,
        palw_kernel_material_replacement_v4::PalwKernelMaterialReplacementIntentV4,
    };
    use misaka_palw_remote::kernel_material_context_v4::KernelMaterialContextArchiveV4;
    let native: &dyn ConsensusApi = chain.ctx.consensus.as_ref();
    let initial: SignedAdmission = borsh::from_slice(&std::fs::read(dir.join("signed-admission")).unwrap()).unwrap();
    assert_eq!(initial.request, input.request.id());
    let old_file = dir.join("provider-2").with_extension("kernel-context-v4");
    let archive = KernelMaterialContextArchiveV4::open(&old_file, limits).unwrap();
    let original = archive.get(initial.consent).unwrap().expect("real persisted initial signed context");
    assert_eq!(original.context.request, input.request.id());
    let mut providers = vec![chain.bonds[4], chain.bonds[5]];
    providers.sort();
    let intent = PalwKernelMaterialReplacementIntentV4 {
        version: 4,
        root: input.request.intent.manifest.root,
        context_request: original.context.request,
        previous: initial.consent,
        sequence: 1,
        providers,
        valid_until_daa: native.get_virtual_daa_score().checked_add(30).unwrap(),
    };
    let grant = native.palw_kernel_material_replacement_v4(&original.context, &input.request.bundle, &intent).unwrap();
    assert_eq!(grant.context_request(), original.context.request);
    let consent = grant.proposal.bounded_replacement_consent_v4().expect("real native bounded replacement grant");
    let mut stores = Vec::new();
    for card in [4, 5] {
        let provider = chain.bonds[card];
        let file = dir.join(format!("provider-{card}"));
        let mut store = Provider::open_with_kernel_contexts_v4(&file, 4 << 20, 16 << 20, |_| None, limits).unwrap();
        if card == 4 {
            let before = (std::fs::read(&file).ok(), std::fs::read(file.with_extension("kernel-context-v4")).ok());
            let mut different_request = original.context.clone();
            different_request.request = Hash64::from_bytes([0xFD; 64]);
            assert_eq!(different_request.intent.manifest, *grant.manifest(), "negative keeps exactly the same material manifest");
            assert_eq!(
                store.prepare_replacement_kernel_v4(provider, &grant, &input.request.bundle, &different_request).unwrap_err(),
                "replacement publication substitutes own native context request",
            );
            assert!(store.held_contracts().is_empty());
            assert_eq!(
                before,
                (std::fs::read(&file).ok(), std::fs::read(file.with_extension("kernel-context-v4")).ok()),
                "wrong original request refuses before either replacement publication"
            );
        }
        assert_eq!(store.prepare_replacement_kernel_v4(provider, &grant, &input.request.bundle, &original.context).unwrap(), consent);
        let scope = grant.manifest().scope(intent.root).unwrap();
        assert!(
            matches!(
                native.palw_material_generation_authority_checked_v4(intent.root, consent, provider, scope),
                Err(PalwMaterialGenerationAuthorityErrorV4::Denied(_))
            ),
            "new signed generation has no native authority before acceptance"
        );
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        assert!(
            store
                .answer_material_exact_current_v4(
                    consent,
                    intent.root,
                    intent.root,
                    provider,
                    key.verification_key.as_ref(),
                    PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 },
                    native,
                )
                .is_err()
        );
        stores.push(store);
    }
    // Real publication by BOTH newly liable actors precedes ordinary 241 and
    // recovery 242 signatures. The native grant's original request is immutable.
    let object = material_recovery_object_v2(grant.network, (*grant.proposal).clone(), chain.bonds[0], |bond, msg, domain| {
        let card = chain.bonds.iter().position(|b| b == bond)?;
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, msg, domain, [0xB9; 32]).ok().map(|s| s.as_ref().to_vec())
    })
    .unwrap();
    assert_eq!(signed_capsule(&object).bounded_replacement_consent_v4(), Some(consent));
    for store in &mut stores {
        store.persist_kernel_signatures_v4(&original.context, signed_capsule(&object)).unwrap();
    }
    std::fs::write(dir.join("signed-replacement"), borsh::to_vec(&SignedReplacement { intent, consent, object }).unwrap()).unwrap();
}
