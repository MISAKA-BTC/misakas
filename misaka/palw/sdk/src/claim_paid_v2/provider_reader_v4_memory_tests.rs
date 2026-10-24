//! Actual typed producer/provider/checker controls. Local bootstrap authority is
//! test input only; no selected seat, accepted receipt or activation is created.
use super::*;
use crate::claim_paid_v2::{
    all_target_bootstrap::{NativePaidProducerInputsV3, produce_native_work_v3},
    private_bootstrap::ProgramPaidBootstrapWorkV3,
};
use kaspa_consensus_core::{
    palw_kernel_parameter_material_v4::kernel_parameter_at_v4,
    palw_kernel_route_v1::PalwKernelRouteStateV1,
    palw_prospective_kernel_material_v4::{commit_identity_v4, typed_kernel_unit_ranges_v4},
};
use misaka_palw_kernel::{
    claim_paid_v2::*,
    descriptor::k2_tir_v2_descriptor,
    ledger::{AuthV1, KernelRouteObjectV1 as O, LedgerBlockV1, LedgerEventV1, LedgerTxV1, claim_seal_v2},
    mode::VerificationModeV1,
    plan::plan_for_tir_program_v1,
    public::{TensorWireV1, program_root_v1},
    spec::{
        ComputationSpecV1, SpecClassKindV1, SpecJobV1, SpecObjectV1, TypedRootV1, WeightsRootV1, k2_tr_v1_descriptor,
        memory::{MemoryJobV1, MemoryRootV1, MemorySlotV1, memory_root_v1, slot_commitments_v1},
    },
    trace::ParamCommitmentsV1,
};
use std::collections::BTreeMap;

const ACTOR: Digest = [12; 64];
const PRODUCER: Digest = [161; 64];
fn object(object: O, signer: Digest) -> LedgerTxV1 {
    LedgerTxV1::Object { auth: AuthV1 { signer_bond: signer }, object }
}
fn block(ledger: &mut KernelLedgerV1, txs: Vec<LedgerTxV1>) -> Vec<LedgerEventV1> {
    ledger.apply_block(&LedgerBlockV1 { daa: ledger.daa + 1, txs })
}
fn route(ledger: &KernelLedgerV1) -> PalwKernelRouteStateV1 {
    let mut route = PalwKernelRouteStateV1::new_typed(ledger.policy, ledger.opv_policy().copied(), Some(0), ledger.scalars());
    route.rows = ledger.to_rows();
    assert_eq!(route.ledger().unwrap().root(), ledger.root());
    route
}
struct MemoryProvider {
    ledger: KernelLedgerV1,
    work: ProgramPaidBootstrapWorkV3,
    claim: Digest,
    class: Digest,
    check: PaidClaimCheckAuthorityV2,
    units: BTreeMap<(u8, u32), Vec<u8>>,
}
impl MemoryProvider {
    fn new() -> Self {
        Self::with_step_fault(None)
    }
    fn with_step_fault(fault: Option<usize>) -> Self {
        let fx = misaka_palw_tir_sketch::fixture::memory_v1(3);
        let descriptor = k2_tir_v2_descriptor();
        let weights = WeightsRootV1 {
            descriptor: descriptor.digest(),
            program_bytes: fx.program.encode(),
            plan: plan_for_tir_program_v1(&descriptor, &fx.program, program_root_v1(&fx.program.encode()), 64).unwrap(),
            param_commitments: ParamCommitmentsV1::of(&fx.params),
        };
        let slots = vec![MemorySlotV1 { param: (1, Some(0)), state: (0, Some(0)) }];
        let m0 = slot_commitments_v1(&slots, &weights.param_commitments).unwrap();
        let root = MemoryRootV1 {
            extension: k2_tr_v1_descriptor().digest(),
            line: [17; 64],
            initial_root: memory_root_v1(&slots, &m0),
            slots,
            max_steps: 8,
        };
        let spec = ComputationSpecV1 {
            version: 1,
            mode: VerificationModeV1::OptimisticPublicVerification,
            roots: vec![TypedRootV1::WeightsV1(weights.clone()), TypedRootV1::MemoryV1(root)],
        };
        let class = spec.class_id().unwrap();
        let mut ledger = PalwKernelRouteStateV1::new_typed(
            crate::provider_reader_fixture::ledger_world::policy(),
            Some(crate::provider_reader_fixture::opv_world::opv_example()),
            Some(0),
            Default::default(),
        )
        .template();
        block(&mut ledger, [ACTOR, PRODUCER].into_iter().map(|bond| LedgerTxV1::SyncBond { bond, collateral: 1_000_000 }).collect());
        let events = block(
            &mut ledger,
            vec![
                LedgerTxV1::AttestArtifact { artifact_root: weights.param_commitments.root() },
                LedgerTxV1::AdmitOptimisticClass { class },
                object(O::Spec { object: SpecObjectV1::RegisterClass { spec } }, ACTOR),
            ],
        );
        assert!(events.contains(&LedgerEventV1::ClassRegistered { class }), "{events:?}");
        let job = SpecJobV1::Memory(MemoryJobV1 {
            class,
            pre_root: ledger.typed.lines[&class].head_root,
            chunks: vec![vec![1, 2], vec![3]],
            nonce: [1; 64],
        });
        let job_id = job.id();
        let events = block(&mut ledger, vec![object(O::Spec { object: SpecObjectV1::PostJob { job } }, ACTOR)]);
        assert!(events.contains(&LedgerEventV1::JobPosted { job: job_id }), "{events:?}");
        let inputs = NativePaidProducerInputsV3 {
            parameters: fx
                .params
                .tensors
                .iter()
                .map(|((parameter, layer), tensor)| ((0, *parameter, *layer), TensorWireV1::of(tensor)))
                .collect(),
            ..Default::default()
        };
        let mut work = produce_native_work_v3(&route(&ledger), job_id, PRODUCER, inputs).unwrap();
        if let Some(fault_step) = fault {
            let SpecClassKindV1::Memory { rule, root, writers } = &ledger.typed.classes[&class].kind else { unreachable!() };
            let SpecJobV1::Memory(job) = &ledger.typed.jobs[&job_id] else { unreachable!() };
            let pre: Vec<_> = root.slots.iter().map(|slot| fx.params.tensors[&slot.param].clone()).collect();
            let got = misaka_palw_kernel::spec::produce::produce_memory_v1(
                &class,
                rule,
                root,
                writers,
                job,
                &PRODUCER,
                &fx.params,
                &pre,
                2,
                |step, trace| {
                    if step == fault_step {
                        let (occurrence, node) = rule
                            .program
                            .occurrences()
                            .iter()
                            .enumerate()
                            .find_map(|(o, (b, _))| {
                                rule.program.blocks[*b as usize]
                                    .nodes
                                    .iter()
                                    .position(|n| matches!(n.prim, misaka_palw_tir::Prim::MatMul))
                                    .map(|n| (o, n))
                            })
                            .unwrap();
                        let value = &mut trace.values[0][occurrence][node];
                        let old = value.data[0];
                        value.data[0] = if value.dtype.contains(old + 1) { old + 1 } else { old - 1 };
                    }
                },
            )
            .unwrap();
            // A malicious producer commits its altered trace, so all fetched
            // units remain authentic. Only the direct arithmetic check exposes it.
            let mut offset = 0;
            for trace in &got.traces {
                for (p, occurrences) in trace.values.iter().enumerate() {
                    for (o, nodes) in occurrences.iter().enumerate() {
                        for (n, value) in nodes.iter().enumerate() {
                            work.material.nodes.insert((0, offset + p as u32, o as u16, n as u16), TensorWireV1::of(value));
                        }
                    }
                }
                offset += trace.values.len() as u32;
            }
            work.commit =
                misaka_palw_kernel::route::SaltedCommitV1::Spec { claim: misaka_palw_kernel::spec::SpecClaimV1::Memory(got.claim) };
        }
        let claim = commit_identity_v4(&work.commit).0;
        assert!(matches!(&work.commit, misaka_palw_kernel::route::SaltedCommitV1::Spec { .. }));
        let salt = [45; 64];
        block(
            &mut ledger,
            vec![object(O::SealClaim { producer: PRODUCER, job: job_id, seal: claim_seal_v2(&claim, &salt) }, PRODUCER)],
        );
        let events = block(&mut ledger, vec![object(O::CommitClaimSalted { salt, commit: work.commit.clone() }, PRODUCER)]);
        assert!(ledger.claims.contains_key(&claim), "{events:?}");
        let subject = ledger.claim_challenge_subject(&claim).unwrap();
        let paid_class = PaidClaimClassV2 {
            version: 2,
            registrant: ACTOR,
            base_class: class,
            target: PaidClaimTargetV2::TypedSpec,
            checking: PaidClaimCheckingV2::ProspectiveOpv,
            checker_contract: checker_contract_v2(),
            source_policy: misaka_palw_challenge::paid_opt_in_source_policy_v5(1, 1, 10, 1, 2),
            check_window_daa: 100,
            challenge_window_daa: 50,
            court_and_grace_daa: 20,
            liability_daa: 200,
            maximum_stream_blocks: 1 << 20,
            maximum_rounds: 1,
            maximum_commit_delay_daa: 1,
            review_scope: None,
            interactive_plan: None,
        };
        let enrollment = PaidClaimEnrollmentV2 {
            version: 2,
            class: paid_class.id(),
            job: [31; 64],
            base_claim: claim,
            producer: PRODUCER,
            commit_root: subject.commitment_root,
            lifetime: PaidClaimLifetimeV2::of(&paid_class, ledger.daa).unwrap(),
            accepted_until_daa: ledger.daa + 1,
            material_consent: [32; 64],
            round: 0,
            prior_transcript: [0; 64],
        };
        let checker_snapshot = [33; 64];
        let paid_subject = paid_subject_with_checker_snapshot_v2(&subject, &paid_class, &enrollment, checker_snapshot).unwrap();
        let context = paid_context_v2(&paid_subject, &paid_class, &enrollment, Default::default(), ledger.daa);
        let check = PaidClaimCheckAuthorityV2::verify_bootstrap_v2(
            &paid_class,
            &enrollment,
            &subject,
            &context,
            ledger.daa,
            checker_snapshot,
            [34; 64],
        )
        .unwrap();
        let mut units = BTreeMap::new();
        typed_kernel_unit_ranges_v4(&ledger, &claim, |stage, count| {
            for position in 0..count {
                let bytes = if stage == PALW_KERNEL_PARAMETER_STAGE_V4 {
                    let (spec, row) = kernel_parameter_at_v4(&ledger, &claim, position).unwrap();
                    assert_ne!((spec.parameter, spec.layer), (1, Some(0)), "mutable slot is not an immutable parameter");
                    let tensor = &fx.params.tensors[&(spec.parameter, spec.layer)];
                    let opening =
                        PalwKernelParameterOpeningV4::RowV2(misaka_palw_kernel::merkle::TensorOpeningV1::row(tensor, row).unwrap());
                    borsh::to_vec(&PalwKernelParameterRowV4 {
                        program: spec.program,
                        parameter: spec.parameter,
                        layer: spec.layer,
                        row,
                        opening,
                    })
                    .unwrap()
                } else {
                    let row = &ledger.claims[&claim];
                    let (values, inputs) = row.body.position(stage, position).unwrap();
                    let derived = ledger.derived_mask(row, stage);
                    let values = values
                        .iter()
                        .enumerate()
                        .map(|(o, nodes)| {
                            nodes
                                .iter()
                                .enumerate()
                                .map(|(n, _)| {
                                    if derived.get(o).and_then(|v| v.get(n)).copied().unwrap_or(false) {
                                        MaterialResponseV1::Omitted
                                    } else {
                                        MaterialResponseV1::Whole(work.material.nodes[&(stage, position, o as u16, n as u16)].clone())
                                    }
                                })
                                .collect()
                        })
                        .collect();
                    let inputs = inputs
                        .iter()
                        .enumerate()
                        .map(|(i, _)| MaterialResponseV1::Whole(work.material.inputs[&(stage, position, i as u16)].clone()))
                        .collect();
                    borsh::to_vec(&PositionResponseV1 { values, inputs }).unwrap()
                };
                typed_kernel_classify_unit_v4(&ledger, &claim, stage, position, &bytes).unwrap();
                units.insert((stage, position), bytes);
            }
            Ok(())
        })
        .unwrap();
        Self { ledger, work, claim, class, check, units }
    }
    fn limits() -> ProviderReaderLimitsV4 {
        ProviderReaderLimitsV4 {
            max_unit_bytes: PALW_KERNEL_MATERIAL_UNIT_BYTES_V4,
            max_total_fetch_bytes: 256 << 20,
            max_workspace_bytes: 1 << 30,
            max_units: 100_000,
        }
    }
    fn run(&mut self) -> Result<ProviderCheckedResultV4, Refusal> {
        let units = &self.units;
        let mut native_ledger = route(&self.ledger).ledger().unwrap();
        check_provider_material_v4(
            &mut native_ledger,
            self.claim,
            PaidClaimTargetV2::TypedSpec,
            &self.work.material,
            &self.check,
            Self::limits(),
            |s, p| {
                NativeRetainedV1::try_from_clone_v1(units.get(&(s, p)).ok_or_else(|| format!("provider unit absent {s}:{p}"))?)
                    .map_err(|e| e.to_string())
            },
        )
    }
}

#[test]
fn initial_memory_provider_falls_back_to_authenticated_entry_state_and_completes_every_step() {
    let mut fixture = MemoryProvider::new();
    assert!(matches!(&fixture.ledger.claims[&fixture.claim].body, misaka_palw_kernel::ledger::ClaimBodyV1::Spec(body)
        if body.pre_source.is_none()));
    let checked = fixture.run().unwrap();
    assert_eq!(checked.finding(), &OutsiderFindingV1::Clean);
    let complete = checked.result.complete.as_ref().expect("the real direct checker completed both memory steps");
    let receipt = complete.receipt([41; 64], [42; 64], fixture.ledger.daa);
    assert_eq!(receipt.enrollment, fixture.check.enrollment());
    assert!(receipt.complete);
}

#[test]
fn memory_entry_state_and_immutable_parameter_failures_never_complete() {
    for failure in 0..3 {
        let mut fixture = MemoryProvider::new();
        if failure < 2 {
            let key = (misaka_palw_kernel::spec::MEMORY_PRE_STATE_STAGE_V1, 0);
            if failure == 0 {
                fixture.units.remove(&key);
            } else {
                *fixture.units.get_mut(&key).unwrap() = vec![0];
            }
        } else {
            let key = *fixture.units.keys().find(|(stage, _)| *stage == PALW_KERNEL_PARAMETER_STAGE_V4).unwrap();
            fixture.units.remove(&key);
        }
        assert!(fixture.run().is_err(), "missing/malformed state or immutable artifact cannot produce a complete check");
    }
}

#[test]
fn only_the_exact_registered_mutable_slot_is_an_optional_artifact_miss() {
    let fixture = MemoryProvider::new();
    for (program, layer) in [(1, Some(0)), (0, None)] {
        let reader = Reader {
            ledger: &fixture.ledger,
            claim: fixture.claim,
            fetch: RefCell::new(|_, _| -> Result<NativeRetainedV1<Vec<u8>>, String> {
                panic!("optional/excluded parameter must not query provider")
            }),
            limits: MemoryProvider::limits(),
            calls: Cell::new(0),
            bytes: Cell::new(0),
            charged: Cell::new(0),
            lease: RefCell::new(None),
            error: RefCell::new(None),
        };
        assert!(reader.param(0, 1, Some(0)).is_none());
        assert!(reader.error.borrow().is_none());
        assert!(reader.param(program, 1, layer).is_none());
        assert!(reader.error.borrow().is_some(), "another program or layer must not become an optional miss");
    }
    assert!(matches!(&fixture.ledger.typed.classes[&fixture.class].kind, SpecClassKindV1::Memory { .. }));
}

#[test]
fn authentic_lie_in_each_memory_step_is_localized_and_convicted_without_complete_receipt() {
    for step in 0..2 {
        let mut fixture = MemoryProvider::with_step_fault(Some(step));
        let checked = fixture.run().unwrap();
        assert!(checked.result.complete.is_none(), "a detected arithmetic fault cannot authenticate a complete receipt");
        let OutsiderFindingV1::Prosecute(proof @ misaka_palw_kernel::route::ProsecutionV1::Spec(bytes)) = checked.finding() else {
            panic!("the authenticated provider trace must expose its arithmetic fault: {:?}", checked.finding())
        };
        let fault: misaka_palw_kernel::spec::SpecFaultV1 = borsh::from_slice(bytes).unwrap();
        assert!(matches!(fault, misaka_palw_kernel::spec::SpecFaultV1::MemoryStep { step: found, .. } if found == step as u32));
        let proof = proof.clone();
        let events = block(&mut fixture.ledger, vec![object(O::FileProof { accuser: ACTOR, claim: fixture.claim, proof }, ACTOR)]);
        assert!(
            fixture.ledger.claims[&fixture.claim].convicted,
            "the actual bounded kernel court must accept the localized filing: {events:?}"
        );
    }
}
