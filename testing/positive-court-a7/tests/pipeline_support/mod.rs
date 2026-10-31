//! A7 integration of the committed A2/A3/A1/A5 APIs. The economic view below
//! is explicitly a private fixture, not native admission or drawn approval.
#![allow(dead_code)]

use crate::support::{self, Packet, Trusted, d, h, limits};
use kaspa_consensus_core::{
    palw_attempt_rules_v1::palw_attempt_output_root_v1, palw_prompt_ids_v1::PalwPromptIdsFormV1, palw_tir_step_v1::PalwTirStepSpaceV1,
};
use misaka_palw_positive_accounting::credit::{
    CreditBinding, CreditDenial, CreditLimits, EconomicReadiness, FraudState, GateCApproval, PositiveCreditGate, RootedCreditFacts,
    RootedCreditView, consumers::CreditConsumers,
};
use misaka_palw_positive_accounting_node::tir::TirCreditView;
use misaka_palw_positive_contract::{
    identity::*,
    messages::*,
    ports::{NodeChecker, PanelBuilder},
};
use misaka_palw_remote::miner::positive_v2::{
    InferenceLimitsV2,
    inference::{AuthenticatedTirParamsV2, ParameterIdentityLimitsV2, TirMinerJobV2, infer_and_sign_tir_result_v2},
    sign_result_v2,
};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirClassRunnerV1, TirHeldInventoryV1, TirParamsSourceV1,
        positive_contract::{CheckedTerminal, TirNodeChecker},
        positive_panel::{PanelAdmission, PanelLimits, TirPanelBuilder},
    },
};
use std::cell::Cell;

pub struct SignedA2 {
    pub result: MinerResultCommit,
    pub service: MinerServiceContract,
    pub generated: Vec<u32>,
    pub key: [u8; PUBKEY_BYTES],
    pub signature: [u8; SIGNATURE_BYTES],
}

pub fn parameter_limits() -> ParameterIdentityLimitsV2 {
    ParameterIdentityLimitsV2 { max_bytes: 1 << 20, max_leaves: 4096, max_compiled_nodes: 4096, max_parameter_instances: 1024 }
}

/// Use the accepted A2 bound inference API. Parameter identity is authenticated
/// by its streaming loader; the Miner path does not build a full opening tree.
/// Durable source journals are separate and are not executed by this helper.
pub fn a2_result(t: &Trusted, alternate: bool, prompt: u32, wrong_output: bool) -> SignedA2 {
    let program = support::program();
    let plan = TirPlan::compile(&program).unwrap();
    let map = support::map(alternate);
    let params = TirParams::from_map(&plan, &map).unwrap();
    assert_eq!(t.class.program, program.encode());
    let held = AuthenticatedTirParamsV2::authenticate(&plan, &params, d(t.artifact), parameter_limits()).unwrap();
    assert_eq!(held.bytes_hashed(), 16, "the 4x4 i8 parameter identity is loaded once");
    let mut result = MinerResultCommit {
        version: 2,
        revision: 1,
        scope: t.scope,
        job: d(t.job.job_id),
        execution: d(t.job.context_hash()),
        miner: t.miner,
        registered_class: d(t.class.class_id(&t.artifact)),
        artifact: d(t.artifact),
        program: d(t.class.graph_ir_root()),
        input: d(t.job.prompt_token_ids_hash),
        tokenizer: d(t.class.tokenizer_id),
        execution_rules: d(t.job.cu_ruleset_id),
        selector: misaka_palw_positive_contract::tir::greedy_selector_id(),
        generation_seed: t.job.execution_seed,
        temperature_q24: 0,
        prefill: t.job.declared_prefill_tokens,
        decode: t.job.exact_decode_tokens,
        max_context: t.job.max_context_tokens,
        // A2 must replace a placeholder with its own inference result.
        output: d(h(0)),
        // The private economic fixture resolves exactly this configured empty
        // dependency declaration. It is not a proposed native preimage/hash.
        dependencies: d(h(5)),
        service_contract: d(h(0)),
    };
    let service = MinerServiceContract {
        version: 2,
        revision: 1,
        case: result.case_id(),
        source_catalog: d(h(70)),
        provider_delegations: d(h(71)),
        budget: AggregateBudget {
            rounds: 4,
            unique_source_bytes: 1 << 20,
            transmitted_bytes: 4 << 20,
            additional_work: 1 << 20,
            retained_bytes: 4 << 20,
            retention_until: 120,
            fee: 100,
            witness_attempts: 2,
        },
        // A test identity only. No UTXO, payer consent or real funding is
        // manufactured by this synthetic service fixture.
        funding_consent: d(h(72)),
        deadline: 100,
    };
    result.service_contract = service.id();
    assert_eq!(service.case, result.case_id());
    let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([51; 32]);
    let key = *pair.verification_key.as_ref();
    let (mut result, mut signature, generated) = infer_and_sign_tir_result_v2(
        &t.admission(),
        result,
        &service,
        &held,
        TirMinerJobV2 { class: &t.class, context: &t.job, prompt: &[prompt], prompt_form: PalwPromptIdsFormV1::Flat },
        InferenceLimitsV2 { max_prefill: 4, max_decode: 4, max_positions: 8 },
        1,
        &key,
        |id, context| {
            assert_eq!(context, RESULT_SIGN_CONTEXT);
            libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, id, context, [52; 32])
                .map(|signature| *signature.as_ref())
                .map_err(|error| format!("{error:?}"))
        },
    )
    .unwrap();
    let expected = support::oracle(alternate, prompt).0;
    assert_eq!(generated, expected);
    assert_eq!(result.output, d(palw_attempt_output_root_v1(&t.job, &expected)));
    if wrong_output {
        // A malicious key holder deliberately re-signs a lie AFTER correct
        // bound inference. Do not label that lie an output of the bound API.
        result.output = d(h(999));
        (result, signature) = sign_result_v2(&t.admission(), result, &service, 1, &key, |id, context| {
            assert_eq!(context, RESULT_SIGN_CONTEXT);
            libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, id, context, [52; 32])
                .map(|signature| *signature.as_ref())
                .map_err(|error| format!("{error:?}"))
        })
        .unwrap();
    }
    result.authenticate(&key, &signature).unwrap();
    SignedA2 { result, service, generated, key, signature }
}

/// A3 authenticates the actual signature returned by A2. Do not regenerate a
/// Miner signature in the Panel fixture, which could mask a broken A2 handoff.
pub fn a3_packet(t: &Trusted, a2: &SignedA2, alternate: bool, prompt: u32) -> Packet {
    let program = support::program();
    let plan = TirPlan::compile(&program).unwrap();
    let map = support::map(alternate);
    let params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let space = PalwTirStepSpaceV1::new(&t.class).unwrap();
    let runner = TirClassRunnerV1::new(&space, &plan, &params, t.class.class_id(&t.artifact)).unwrap();
    let admitted = PanelAdmission::authenticate(
        &a2.result,
        t.admission(),
        &t.class,
        &t.job,
        &[prompt],
        PalwPromptIdsFormV1::Flat,
        &a2.key,
        &a2.signature,
        PanelLimits {
            check: limits(),
            max_frame_bytes: limits().max_part_bytes,
            max_total_frame_bytes: limits().max_total_bytes,
            max_capture_bytes: 16 << 20,
        },
    )
    .unwrap();
    let builder = TirPanelBuilder::new(&runner, &inventory);
    let capture = builder.replay_and_capture(&admitted).unwrap();
    assert_eq!(capture.generated(), support::oracle(alternate, prompt).0);
    let descriptor = admitted.plan();
    let panel_key = support::sign(61, Digest([0; 64]), WITNESS_SIGN_CONTEXT).0;
    let author = principal(panel_key.as_slice().try_into().unwrap());
    let witness = builder.commit(&capture, &descriptor, author, 0).unwrap();
    let prepared = builder.prepare_parts(&capture).unwrap();
    let frames = (0..descriptor.mandatory_relations)
        .map(|next_relation| {
            let part = prepared.build_part(CoverageCursor { plan: descriptor.plan, next_relation }).unwrap();
            part.frame(&witness, &descriptor).unwrap().encode(descriptor.max_part_bytes).unwrap()
        })
        .collect();
    let panel_sig = support::sign(61, witness.id().0, WITNESS_SIGN_CONTEXT).1;
    let mut packet = Packet {
        result: a2.result.clone(),
        witness,
        binding: capture.binding().clone(),
        generated: capture.generated().to_vec(),
        miner_key: a2.key.to_vec(),
        miner_sig: a2.signature.to_vec(),
        panel_key,
        panel_sig,
        frames,
        frame_signatures: vec![],
    };
    packet.frame_signatures = vec![vec![]; packet.frames.len()];
    for index in 0..packet.frames.len() {
        packet.resign_frame(index);
    }
    packet
}

pub fn a1_complete(packet: &Packet, trusted: &Trusted) -> (CoverageDescriptor, CheckedTerminal) {
    let context = packet.authenticate(trusted).unwrap();
    let descriptor = TirNodeChecker.derive_plan(&context).unwrap();
    let mut progress = context.start().unwrap();
    for bytes in &packet.frames {
        TirNodeChecker.check_authenticated_part(&context, &mut progress, packet.authenticated(bytes).unwrap()).unwrap();
    }
    assert_eq!(progress.charged_wire_bytes(), packet.frames.iter().map(|frame| frame.len() as u64).sum::<u64>());
    assert_eq!(progress.charged_signature_bytes(), packet.frame_signatures.iter().map(|sig| sig.len() as u64).sum::<u64>());
    assert_eq!(
        progress.charged_transport_bytes().unwrap(),
        packet.frames.iter().map(|frame| frame.len() as u64).sum::<u64>()
            + packet.frame_signatures.iter().map(|sig| sig.len() as u64).sum::<u64>()
    );
    (descriptor, TirNodeChecker.finish(&context, progress).unwrap())
}

/// A configured test owner of simulated A4/Gate C/economic facts. Native
/// registry, drawn approval and funding adapters are absent and stay absent.
pub struct PrivateEconomicFixture {
    pub scope: Cell<ChainScope>,
    pub facts: Cell<RootedCreditFacts>,
    declared_empty_dependencies: Digest,
}
impl PrivateEconomicFixture {
    pub fn new(result: &MinerResultCommit, descriptor: CoverageDescriptor) -> Self {
        assert_eq!(result.dependencies, d(h(5)));
        let binding = CreditBinding {
            scope: result.scope,
            claim: result.claim_id(),
            case: result.case_id(),
            result: result.id(),
            output: result.output,
            dependencies: result.dependencies,
        };
        Self {
            scope: Cell::new(result.scope),
            facts: Cell::new(RootedCreditFacts {
                binding,
                plan: descriptor.plan,
                mandatory_relations: descriptor.mandatory_relations,
                status: ClaimStatus::PositivePassed,
                gate_c: GateCApproval::Pending,
                fraud: FraudState::Clear,
                maturity_and_liability: EconomicReadiness::Held,
                carrier_and_funding: EconomicReadiness::Held,
            }),
            declared_empty_dependencies: result.dependencies,
        }
    }
    pub fn set_fixture_ready(&self) {
        let mut facts = self.facts.get();
        facts.gate_c = GateCApproval::Approved(facts.binding);
        facts.maturity_and_liability = EconomicReadiness::Ready;
        facts.carrier_and_funding = EconomicReadiness::Ready;
        self.facts.set(facts);
    }
}
impl RootedCreditView for PrivateEconomicFixture {
    fn scope(&self) -> ChainScope {
        self.scope.get()
    }
    fn claim(&self, claim: ClaimId) -> Result<RootedCreditFacts, CreditDenial> {
        let facts = self.facts.get();
        if facts.binding.claim == claim { Ok(facts) } else { Err(CreditDenial::MissingClaim(claim)) }
    }
    fn checked_dependencies(&self, claim: ClaimId) -> Result<&[CreditBinding], CreditDenial> {
        let facts = self.claim(claim)?;
        if facts.binding.dependencies != self.declared_empty_dependencies {
            return Err(CreditDenial::MissingDependencyBinding(claim));
        }
        Ok(&[])
    }
}

pub fn assert_private_consumers(view: &TirCreditView<'_, PrivateEconomicFixture>, claim: ClaimId, allowed: bool) {
    let gate = PositiveCreditGate::new(view, CreditLimits { max_claims: 8, max_edges: 8 }).unwrap();
    let consumers = CreditConsumers::new(&gate);
    assert_eq!(consumers.inference_reward(&claim).is_ok(), allowed);
    assert_eq!(consumers.work_receipt(&claim).is_ok(), allowed);
    assert_eq!(consumers.dag_weight(&claim, 31), if allowed { 31 } else { 0 });
    assert_eq!(consumers.rule_e_capital(&claim, 47), if allowed { 47 } else { 0 });
    assert_eq!(consumers.settlement(&claim).is_ok(), allowed);
    assert_eq!(consumers.prefix_reuse(&claim).is_ok(), allowed);
    assert_eq!(consumers.dependent_claim(&claim).is_ok(), allowed);
}
