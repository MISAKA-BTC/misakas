//! Actual file/signature/root controls; the seeded typed program is a code
//! fixture, not a trained model or multi-process useful-work acceptance.
use kaspa_consensus_core::{
    palw_kernel_route_v1::{
        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, PalwKernelRouteStateV1, palw_kernel_route_message_v1, palw_kernel_route_policy_v1,
    },
    palw_onboarding_v1::{
        ArtifactBindingRowV1, KernelBindingRowV1, PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1, PALW_ONBOARDING_MLDSA87_CONTEXT_V1,
        PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1, palw_onboarding_message_v1,
    },
    palw_opv_bootstrap_v1::{PALW_COMPLETE_CHECK_FEE_SOMPI_V1, palw_onboarding_complete_check_policy_v1},
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    palw_tir_admission_v1::palw_tir_post_genesis_registration_v1,
    palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1},
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1},
    tx::{TransactionId, TransactionOutpoint},
};
use kaspa_hashes::Hash64;
use misaka_palw_challenge::{ConformanceCommitmentV1, RootV1, SubjectKindV1};
use misaka_palw_kernel::{
    descriptor::k2_tir_v2_descriptor,
    public::program_root_v1,
    route::KernelRouteObjectV1,
    rows::{LedgerScalarsV1, TABLE_CLASSES_V1},
    state::ClassRecordV1,
};
use misaka_palw_sdk::{
    bootstrap_check::{BootstrapSnapshotPinsV1, CompleteBootstrapArtifactV1, CompleteBootstrapCostsV1},
    kernel_rows::KernelRowsSnapshotV1,
    onboarding_chain::EnvelopeSigner,
};
use std::{cell::Cell, path::PathBuf};

struct Signer {
    key: kaspa_pq_validator_core::ValidatorKey,
    calls: Cell<u32>,
}
impl Signer {
    fn new() -> Self {
        Self { key: kaspa_pq_validator_core::ValidatorKey::from_seed([0x67; 32]), calls: Cell::new(0) }
    }
}
impl EnvelopeSigner for Signer {
    fn public_key(&self) -> Vec<u8> {
        self.key.public_key().to_vec()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.key.sign_with_context(message, context).to_vec())
    }
}
struct Fixture {
    path: PathBuf,
    registration: PalwConsensusObjectV2,
    route: PalwKernelRouteStateV1,
    bond: PalwBondKeyV2,
    network: Hash64,
    genesis: Hash64,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn costs() -> CompleteBootstrapCostsV1 {
    CompleteBootstrapCostsV1 {
        maximum_binding_reservation_sompi: PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
        maximum_check_fee_sompi: PALW_COMPLETE_CHECK_FEE_SOMPI_V1,
    }
}
impl Fixture {
    fn new() -> Self {
        let fx = misaka_palw_tir_sketch::fixture::wide128_v1(37);
        let native = PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: fx.program.encode(),
            tokenizer_id: Hash64::default(),
            layout: PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: 64,
                checkpoint_interval: 2,
                h_tile: 2,
                commit_tiles: fx.program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).map(|_| 4).collect(),
                state_tiles: vec![],
            },
        };
        let path = std::env::temp_dir().join(format!(
            "rfc12-bootstrap-{}-{}.palwtir",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        misaka_palw_tir_artifact::write_container_v1(
            &path,
            &fx.program,
            borsh::to_vec(&native.layout).unwrap(),
            [0; 64],
            "seeded code fixture; no model provenance claim".into(),
            &mut |j, l| Ok(fx.params.tensors[&(j, l)].to_le_bytes()),
        )
        .unwrap();
        let root = misaka_palw_sdk::tir_manifest::PalwTirManifestV1::derive_streamed(&path).unwrap().inventory_root;
        let bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(99), 0));
        let facts = PalwTirJobFactsV1::of(&native, &fx.program, native.class_id(&root));
        let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&native).unwrap());
        let registration = palw_tir_post_genesis_registration_v1(native, canonical, root, 0, 1, 1, 0, bond, vec![], u64::MAX).unwrap();
        let (network, genesis) = (Hash64::from_u64_word(101), Hash64::from_u64_word(102));
        let route = PalwKernelRouteStateV1::new(
            palw_kernel_route_policy_v1(network, Hash64::from_u64_word(103)),
            None,
            LedgerScalarsV1 { daa: 5, burned: 0 },
        );
        Self { path, registration, route, bond, network, genesis }
    }
    fn artifact(&self) -> CompleteBootstrapArtifactV1 {
        CompleteBootstrapArtifactV1::from_registration_file(&self.registration, &self.path, 64, costs()).unwrap()
    }
    fn binding(&mut self, artifact: &CompleteBootstrapArtifactV1) {
        self.route.aux.insert(
            (PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, borsh::to_vec(&(artifact.class(), artifact.kernel_param_root())).unwrap()),
            borsh::to_vec(&ArtifactBindingRowV1 {
                binder: self.bond,
                bound_daa: 1,
                matures_daa: 2,
                final_daa: 100,
                reserved: PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
                refuted: false,
            })
            .unwrap(),
        );
    }
    fn reads(&self) -> (KernelRowsSnapshotV1, Vec<(u8, Vec<u8>, Vec<u8>)>, BootstrapSnapshotPinsV1) {
        let page = self.route.rows_page_v1(None, usize::MAX);
        let pins = BootstrapSnapshotPinsV1 { ledger_root: self.route.ledger_root(), aux_root: self.route.aux_root(), tip_daa: 6 };
        (
            KernelRowsSnapshotV1 {
                tip_daa: 6,
                ledger_root: pins.ledger_root,
                aux_root: pins.aux_root,
                header: borsh::to_vec(&self.route.header).unwrap(),
                total_rows: page.total_rows,
            },
            page.rows,
            pins,
        )
    }
    fn kernel_record(&mut self, artifact: &CompleteBootstrapArtifactV1, register: &PalwConsensusObjectV2) {
        let PalwConsensusObjectV2::KernelRouteV1 { bytes, .. } = register else { panic!("tag110") };
        let KernelRouteObjectV1::RegisterClass { descriptor, program_bytes, plan, param_commitments } =
            KernelRouteObjectV1::decode(bytes).unwrap()
        else {
            panic!("inner1")
        };
        self.route.rows.insert(
            (TABLE_CLASSES_V1, borsh::to_vec(&artifact.kernel_class().as_bytes()).unwrap()),
            borsh::to_vec(&ClassRecordV1 { descriptor, program_bytes, plan, param_commitments }).unwrap(),
        );
    }
    fn kernel_binding(&mut self, artifact: &CompleteBootstrapArtifactV1) -> ConformanceCommitmentV1 {
        let record = self.route.kernel_class_record_v1(&artifact.kernel_class()).unwrap();
        let policy = palw_onboarding_complete_check_policy_v1().id();
        self.route.aux.insert(
            (PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1, borsh::to_vec(&artifact.class()).unwrap()),
            borsh::to_vec(&KernelBindingRowV1 {
                kernel_class: artifact.kernel_class(),
                kernel_param_root: artifact.kernel_param_root(),
                plan_root: Hash64::from_bytes(record.plan.root()),
                challenge_policy_id: Hash64::from_bytes(policy),
                binder: self.bond,
                bound_daa: 5,
            })
            .unwrap(),
        );
        let PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } = &self.registration else { panic!() };
        ConformanceCommitmentV1 {
            version: 1,
            chain_genesis: self.genesis.as_bytes(),
            ruleset_id: self.route.header.policy.ruleset_digest,
            subject_kind: SubjectKindV1::ModelConformance,
            candidate_id: artifact.class().as_bytes(),
            kernel_descriptor_id: k2_tir_v2_descriptor().digest(),
            challenge_policy_id: policy,
            artifact_root: artifact.artifact_root().as_bytes(),
            program_root: program_root_v1(&record.program_bytes),
            source_root: RootV1::Absent,
            tokenizer_or_input_schema_root: RootV1::Absent,
            layout_root: admission.class.layout_digest().as_bytes(),
            verification_plan_root: record.plan.root(),
            constraint_root: RootV1::Absent,
            implementation_set_root: [1; 64],
            test_scope_root: [2; 64],
            calibration_id: RootV1::Absent,
            input_and_state_binding_root: RootV1::Absent,
            resource_profile_id: [3; 64],
            commitment_object_id: None,
            canonical_commitment_position: None,
        }
    }
}
fn verify_signature(o: &PalwConsensusObjectV2, network: Hash64, key: &Signer) {
    let (message, signature, context) = match o {
        PalwConsensusObjectV2::ArtifactBoundV1 { v2_class, kernel_param_root, signer, signature } => (
            palw_onboarding_message_v1(network, 104, signer, &borsh::to_vec(&(v2_class, kernel_param_root)).unwrap()),
            signature,
            PALW_ONBOARDING_MLDSA87_CONTEXT_V1,
        ),
        PalwConsensusObjectV2::KernelBoundV1 { v2_class, kernel_class, challenge_policy_id, signer, signature } => (
            palw_onboarding_message_v1(network, 106, signer, &borsh::to_vec(&(v2_class, kernel_class, challenge_policy_id)).unwrap()),
            signature,
            PALW_ONBOARDING_MLDSA87_CONTEXT_V1,
        ),
        PalwConsensusObjectV2::ConformanceCommittedV1 { commitment, signer, signature } => (
            palw_onboarding_message_v1(network, 107, signer, &borsh::to_vec(commitment).unwrap()),
            signature,
            PALW_ONBOARDING_MLDSA87_CONTEXT_V1,
        ),
        PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, signature } => {
            (palw_kernel_route_message_v1(network, signer, bytes), signature, PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1)
        }
        _ => panic!("bootstrap object"),
    };
    assert!(kaspa_txscript::verify_mldsa87_with_context(&key.public_key(), message.as_byte_slice(), signature, context).unwrap());
    assert!(
        !kaspa_txscript::verify_mldsa87_with_context(
            &key.public_key(),
            palw_onboarding_message_v1(
                Hash64::from_u64_word(200),
                104,
                &PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(99), 0)),
                b"other"
            )
            .as_byte_slice(),
            signature,
            context
        )
        .unwrap()
    );
}

#[test]
fn rfc12_bootstrap_prepares_actual_signed_objects_from_complete_file_and_rooted_bindings() {
    let mut f = Fixture::new();
    let a = f.artifact();
    let key = Signer::new();
    let bind = a.signed_artifact_binding(f.network, f.bond, &key).unwrap();
    verify_signature(&bind, f.network, &key);
    f.binding(&a);
    let (s, r, p) = f.reads();
    let register = a.signed_kernel_registration(&s, r, p, f.network, f.bond, &key).unwrap();
    verify_signature(&register, f.network, &key);
    f.kernel_record(&a, &register);
    let (s, r, p) = f.reads();
    let bind = a.signed_kernel_binding(&s, r, p, f.network, f.bond, &key).unwrap();
    verify_signature(&bind, f.network, &key);
    let commitment = f.kernel_binding(&a);
    let (s, r, p) = f.reads();
    let committed = a.signed_conformance_commitment(&s, r, p, f.genesis, f.network, commitment, f.bond, &key).unwrap();
    verify_signature(&committed, f.network, &key);
    assert_eq!(key.calls.get(), 4);
}

#[test]
fn rfc12_bootstrap_refuses_cost_file_registration_root_clock_and_refuted_binding_before_signing() {
    let mut f = Fixture::new();
    let a = f.artifact();
    let key = Signer::new();
    let mut cap = costs();
    cap.maximum_check_fee_sompi = PALW_COMPLETE_CHECK_FEE_SOMPI_V1 - 1;
    assert_eq!(
        CompleteBootstrapArtifactV1::from_registration_file(&f.registration, &f.path.with_extension("missing"), 64, cap)
            .err()
            .unwrap()
            .code,
        "COST_LIMIT"
    );
    cap = costs();
    cap.maximum_binding_reservation_sompi -= 1;
    assert!(cap.check().is_err());
    assert!(CompleteBootstrapArtifactV1::from_registration_file(&f.registration, &f.path, 0, costs()).is_err());
    assert!(CompleteBootstrapArtifactV1::from_registration_file(&f.registration, &f.path, 65, costs()).is_err());
    let mut registration = f.registration.clone();
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, .. } = &mut registration else { panic!() };
    *class_id = Hash64::default();
    assert!(CompleteBootstrapArtifactV1::from_registration_file(&registration, &f.path, 64, costs()).is_err());
    f.binding(&a);
    let (s, r, mut p) = f.reads();
    p.tip_daa = 7;
    assert!(a.signed_kernel_registration(&s, r, p, f.network, f.bond, &key).is_err());
    let (s, r, mut p) = f.reads();
    p.aux_root = Hash64::default();
    assert!(a.signed_kernel_registration(&s, r, p, f.network, f.bond, &key).is_err());
    let binding_key = (PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, borsh::to_vec(&(a.class(), a.kernel_param_root())).unwrap());
    let mut binding: ArtifactBindingRowV1 = borsh::from_slice(&f.route.aux[&binding_key]).unwrap();
    binding.refuted = true;
    f.route.aux.insert(binding_key, borsh::to_vec(&binding).unwrap());
    let (s, r, p) = f.reads();
    assert!(a.signed_kernel_registration(&s, r, p, f.network, f.bond, &key).is_err());
    assert_eq!(key.calls.get(), 0, "no refused preparation may invoke the signer");
    std::fs::OpenOptions::new().write(true).open(&f.path).unwrap().set_len(2 * 1024 * 1024 + 1).unwrap();
    assert!(CompleteBootstrapArtifactV1::from_registration_file(&f.registration, &f.path, 64, costs()).is_err());
}

#[test]
fn rfc12_bootstrap_refuses_caller_statement_semantic_substitution_before_signing() {
    let mut f = Fixture::new();
    let a = f.artifact();
    let key = Signer::new();
    f.binding(&a);
    let (s, r, p) = f.reads();
    let register = a.signed_kernel_registration(&s, r, p, f.network, f.bond, &key).unwrap();
    f.kernel_record(&a, &register);
    let c = f.kernel_binding(&a);
    key.calls.set(0);
    for field in 0..7 {
        let mut wrong = c.clone();
        match field {
            0 => wrong.chain_genesis = [0; 64],
            1 => wrong.artifact_root = [0; 64],
            2 => wrong.layout_root = [0; 64],
            3 => wrong.ruleset_id = [0; 64],
            4 => wrong.verification_plan_root = [0; 64],
            5 => wrong.candidate_id = [0; 64],
            _ => wrong.challenge_policy_id = [0; 64],
        };
        let (s, r, p) = f.reads();
        assert!(a.signed_conformance_commitment(&s, r, p, f.genesis, f.network, wrong, f.bond, &key).is_err());
    }
    let (s, r, p) = f.reads();
    assert!(a.signed_kernel_binding(&s, r, p, f.network, f.bond, &key).is_err(), "an existing binding cannot be replaced");
    assert_eq!(key.calls.get(), 0);
}
