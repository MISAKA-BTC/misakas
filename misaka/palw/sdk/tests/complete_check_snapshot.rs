//! Public-snapshot/file/signature controls. The tiny typed program is a test
//! fixture, not a trained-model, real-node, independence or release result.
use kaspa_consensus_core::{
    palw_artifact::{artifact_leaf_v1, artifact_root_v1},
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_route_policy_v1},
    palw_onboarding_v1::{
        ConformanceAttemptRowV1, ConformanceRowV1, KernelBindingRowV1, PALW_ONBOARDING_MLDSA87_CONTEXT_V1,
        PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1, PALW_ONBOARDING_TABLE_CONFORMANCE_V1, PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1,
        palw_onboarding_message_v1,
    },
    palw_opv_bootstrap_v1::palw_onboarding_complete_check_policy_v1,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    palw_tir_artifact_v1::palw_tir_inventory_operands_v1,
    tx::{TransactionId, TransactionOutpoint},
};
use kaspa_hashes::Hash64;
use misaka_palw_challenge::{ConformanceCommitmentV1, OnboardingRecordV1, OnboardingStateV1, RootV1, SubjectKindV1};
use misaka_palw_kernel::{
    descriptor::k2_tir_v2_descriptor,
    plan::plan_for_tir_program_v1,
    public::program_root_v1,
    rows::{LedgerScalarsV1, TABLE_CLASSES_V1},
    state::ClassRecordV1,
    trace::ParamCommitmentsV1,
};
use misaka_palw_sdk::{complete_check::VerifiedCompleteCheckV1, kernel_rows::KernelRowsSnapshotV1, onboarding_chain::EnvelopeSigner};
use std::{collections::BTreeMap, path::PathBuf};

struct Tensors(BTreeMap<(u16, Option<u16>), Vec<u8>>);
impl kaspa_consensus_core::palw_tir_artifact_v1::PalwTirTensorSourceV1 for Tensors {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<std::borrow::Cow<'_, [u8]>> {
        self.0.get(&(param, layer)).map(|b| std::borrow::Cow::Borrowed(b.as_slice()))
    }
}
struct Key(kaspa_pq_validator_core::ValidatorKey);
impl EnvelopeSigner for Key {
    fn public_key(&self) -> Vec<u8> {
        self.0.public_key().to_vec()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.0.sign_with_context(message, context).to_vec())
    }
}
struct Fixture {
    route: PalwKernelRouteStateV1,
    class: Hash64,
    genesis: Hash64,
    network: Hash64,
    bond: PalwBondKeyV2,
    artifact: PathBuf,
    program: misaka_palw_tir::TirProgramV1,
    params: misaka_palw_tir::MapParams,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.artifact);
    }
}
impl Fixture {
    fn new() -> Self {
        let fx = misaka_palw_tir_sketch::fixture::wide128_v1(31);
        let descriptor = k2_tir_v2_descriptor();
        let plan = plan_for_tir_program_v1(&descriptor, &fx.program, program_root_v1(&fx.program.encode()), 64).unwrap();
        let pc = ParamCommitmentsV1::of(&fx.params);
        let tensors = Tensors(fx.params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect());
        let operands = palw_tir_inventory_operands_v1(&fx.program, &tensors).unwrap();
        let root = artifact_root_v1(&operands.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
        let (class, kernel, genesis, network) =
            (Hash64::from_u64_word(10), Hash64::from_u64_word(11), Hash64::from_u64_word(12), Hash64::from_u64_word(13));
        let bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(20), 0));
        let policy = palw_kernel_route_policy_v1(network, Hash64::from_u64_word(21));
        let mut route = PalwKernelRouteStateV1::new(policy, None, LedgerScalarsV1 { daa: 5, burned: 0 });
        let c = ConformanceCommitmentV1 {
            version: 1,
            chain_genesis: genesis.as_bytes(),
            ruleset_id: policy.ruleset_digest,
            subject_kind: SubjectKindV1::ModelConformance,
            candidate_id: class.as_bytes(),
            kernel_descriptor_id: descriptor.digest(),
            challenge_policy_id: palw_onboarding_complete_check_policy_v1().id(),
            artifact_root: root.as_bytes(),
            program_root: program_root_v1(&fx.program.encode()),
            source_root: RootV1::Absent,
            tokenizer_or_input_schema_root: RootV1::Absent,
            layout_root: [1; 64],
            verification_plan_root: plan.root(),
            constraint_root: RootV1::Absent,
            implementation_set_root: [2; 64],
            test_scope_root: [3; 64],
            calibration_id: RootV1::Absent,
            input_and_state_binding_root: RootV1::Absent,
            resource_profile_id: [4; 64],
            commitment_object_id: None,
            canonical_commitment_position: None,
        };
        let mut record = OnboardingRecordV1::new(4);
        record.state = OnboardingStateV1::ChallengePending;
        let attempt = ConformanceAttemptRowV1 {
            record,
            commitment: c.clone(),
            committed_daa: 5,
            challenge_epoch: 0,
            eligible_profiles: vec![],
            excluded_profiles: vec![class, kernel],
            evidence: None,
            last_end: None,
            refuters_judged: vec![],
        };
        route.rows.insert(
            (TABLE_CLASSES_V1, borsh::to_vec(&kernel.as_bytes()).unwrap()),
            borsh::to_vec(&ClassRecordV1 {
                descriptor: descriptor.digest(),
                program_bytes: fx.program.encode(),
                plan: plan.clone(),
                param_commitments: pc.clone(),
            })
            .unwrap(),
        );
        route.aux.insert(
            (PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1, borsh::to_vec(&class).unwrap()),
            borsh::to_vec(&KernelBindingRowV1 {
                kernel_class: kernel,
                kernel_param_root: Hash64::from_bytes(pc.root()),
                plan_root: Hash64::from_bytes(plan.root()),
                challenge_policy_id: Hash64::from_bytes(c.challenge_policy_id),
                binder: bond,
                bound_daa: 4,
            })
            .unwrap(),
        );
        route.aux.insert(
            (PALW_ONBOARDING_TABLE_CONFORMANCE_V1, borsh::to_vec(&(class, root)).unwrap()),
            borsh::to_vec(&ConformanceRowV1 {
                statement_root: Hash64::from_bytes(c.statement_root()),
                signer: bond,
                committed_daa: 5,
            })
            .unwrap(),
        );
        route
            .aux
            .insert((PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1, borsh::to_vec(&class).unwrap()), borsh::to_vec(&attempt).unwrap());
        let artifact = std::env::temp_dir().join(format!(
            "rfc12-complete-check-{}-{}.palwtir",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        misaka_palw_tir_artifact::write_container_v1(&artifact, &fx.program, vec![], [0; 64], "test fixture".into(), &mut |j, l| {
            Ok(fx.params.tensors[&(j, l)].to_le_bytes())
        })
        .unwrap();
        Self { route, class, genesis, network, bond, artifact, program: fx.program, params: fx.params }
    }
    #[allow(clippy::type_complexity)] // the tuple is the record; naming it adds a type nobody else uses
    fn reads(&self, daa: u64) -> (KernelRowsSnapshotV1, Vec<(u8, Vec<u8>, Vec<u8>)>) {
        let p = self.route.rows_page_v1(None, usize::MAX);
        (
            KernelRowsSnapshotV1 {
                tip_daa: daa,
                ledger_root: self.route.ledger_root(),
                aux_root: self.route.aux_root(),
                header: borsh::to_vec(&self.route.header).unwrap(),
                total_rows: p.total_rows,
            },
            p.rows,
        )
    }
    fn verified(&self, daa: u64) -> VerifiedCompleteCheckV1 {
        let (s, r) = self.reads(daa);
        VerifiedCompleteCheckV1::from_snapshot(&s, r, self.class, self.genesis, self.network, daa).unwrap()
    }
}

#[test]
fn rfc12_complete_check_builds_signed_post_from_rooted_public_file_and_refuses_replacements() {
    let f = Fixture::new();
    let verified = f.verified(6);
    let report = verified.verify_file(&f.artifact).unwrap();
    assert!(report.honest_passes && report.agrees && !report.chain_says_passed);
    let post = verified.build_post_file(&f.artifact).unwrap();
    assert_eq!(post.id(), report.honest_post_id);
    let key = Key(kaspa_pq_validator_core::ValidatorKey::from_seed([0x63; 32]));
    let o = verified.signed_post_file(&f.artifact, f.bond, &key).unwrap();
    let PalwConsensusObjectV2::ConformanceEvidenceV1 { v2_class, action, signer, signature } = o else { panic!("tag109") };
    let payload = borsh::to_vec(&(v2_class, &action)).unwrap();
    let message = palw_onboarding_message_v1(f.network, 109, &signer, &payload);
    assert!(
        kaspa_txscript::verify_mldsa87_with_context(
            &key.public_key(),
            message.as_byte_slice(),
            &signature,
            PALW_ONBOARDING_MLDSA87_CONTEXT_V1
        )
        .unwrap()
    );
    assert!(
        verified
            .signed_post_file(&f.artifact, PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(99), 0)), &key)
            .is_err()
    );
    assert!(verified.check_rpc_reads(&[], &f.program.encode()).is_err());
    let (snapshot, mut rows) = f.reads(6);
    rows[0].2[0] ^= 1;
    assert!(VerifiedCompleteCheckV1::from_snapshot(&snapshot, rows, f.class, f.genesis, f.network, 6).is_err());
    let (snapshot, rows) = f.reads(6);
    assert!(VerifiedCompleteCheckV1::from_snapshot(&snapshot, rows, f.class, Hash64::default(), f.network, 6).is_err());
    assert!(f.verified(66).build_post_file(&f.artifact).is_err(), "expired open row cannot produce a post");
    misaka_palw_tir_artifact::write_container_v1(&f.artifact, &f.program, vec![], [0; 64], "substitution".into(), &mut |j, l| {
        let mut b = f.params.tensors[&(j, l)].to_le_bytes();
        b[0] ^= 1;
        Ok(b)
    })
    .unwrap();
    assert!(verified.verify_file(&f.artifact).is_err(), "another artifact cannot verify this rooted class");
}

#[test]
fn rfc12_complete_check_refuses_rooted_plan_substitution_and_oversized_file() {
    let mut f = Fixture::new();
    let key = (PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1, borsh::to_vec(&f.class).unwrap());
    let mut binding: KernelBindingRowV1 = borsh::from_slice(&f.route.aux[&key]).unwrap();
    binding.plan_root = Hash64::default();
    f.route.aux.insert(key, borsh::to_vec(&binding).unwrap());
    let (snapshot, rows) = f.reads(6);
    assert!(VerifiedCompleteCheckV1::from_snapshot(&snapshot, rows, f.class, f.genesis, f.network, 6).is_err());
    let f = Fixture::new();
    let verified = f.verified(6);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&f.artifact)
        .unwrap()
        .set_len(misaka_palw_sdk::complete_check::COMPLETE_CHECK_MAX_CONTAINER_BYTES_V1 + 1)
        .unwrap();
    assert!(verified.verify_file(&f.artifact).is_err());
}

#[test]
fn rfc12_complete_check_authenticates_expiry_clock_independently_of_rpc_and_idle_route_clock() {
    let f = Fixture::new();
    // A valid idle route clock is behind the current chain DAA. It must not
    // authenticate expiry, or a node can indefinitely reuse an old open row.
    assert_eq!(f.route.header.scalars.daa, 5);
    let (snapshot, rows) = f.reads(6);
    assert!(VerifiedCompleteCheckV1::from_snapshot(&snapshot, rows, f.class, f.genesis, f.network, 66).is_err());
    let current = f.verified(66);
    assert!(current.build_post_file(&f.artifact).is_err());
    let (snapshot, rows) = f.reads(4);
    assert!(VerifiedCompleteCheckV1::from_snapshot(&snapshot, rows, f.class, f.genesis, f.network, 4).is_err());
}
