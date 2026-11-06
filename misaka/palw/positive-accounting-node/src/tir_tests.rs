// Actual A1 arithmetic/checker integration. The tiny execution fixture follows
// A1's public-port fixture; economic rooted-state answers below are explicitly
// private test fixtures, never a native admission or drawn approval adapter.
use super::*;
use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1},
    palw_tir_court_v1::{PalwTirCourtRulesV1, tir_logits_event_disclosure_v1},
    palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirLeafKindV1, PalwTirStepBindingV1, PalwTirStepSpaceV1},
};
use kaspa_consensus_core::{
    palw_prompt_ids_v1::prompt_token_ids_commitment_v1,
    palw_step_refute::{PALW_LOGITS_TILE_LANES, tiled_logits_scheme_id_v1},
    palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_accounting::credit::{
    ALL_CREDIT_USES, CreditLimits, EconomicReadiness, FraudState, GateCApproval, PositiveCreditGate, consumers::CreditConsumers,
};
use misaka_palw_positive_contract::{
    identity::*,
    messages::*,
    ports::{CreditGate, CreditUse, NodeChecker},
    tir::ExactPart,
    wire::*,
};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, demand::DemandLimits};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirClassRunnerV1, TirEvidenceV1, TirHeldInventoryV1, TirParamsSourceV1,
        positive_check::{CheckLimits, CheckOutcome},
        positive_contract::{AdapterError, CheckContext, ContractProgress, PreparedCheck, TirNodeChecker},
    },
};
use std::cell::Cell;

fn tiled(mut p: TirProgramV1) -> TirProgramV1 {
    p.logits_scheme_id = tiled_logits_scheme_id_v1().as_bytes();
    p
}

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn native(hash: Hash64) -> Digest {
    Digest(hash.as_bytes())
}
fn limits() -> CheckLimits {
    CheckLimits {
        max_leaves: 1 << 20,
        max_part_bytes: 1 << 20,
        max_part_heap: 1 << 24,
        max_total_bytes: 1 << 40,
        max_elements_per_leaf: 1 << 20,
        max_terms_per_leaf: 1 << 24,
        max_total_elements: 1 << 40,
        max_total_terms: 1 << 44,
    }
}
fn miner_key() -> [u8; PUBKEY_BYTES] {
    *libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]).verification_key.as_ref()
}
fn panel_key() -> [u8; PUBKEY_BYTES] {
    *libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]).verification_key.as_ref()
}
fn sign_result(result: &MinerResultCommit) -> [u8; SIGNATURE_BYTES] {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
    *libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref()
}
fn sign_witness(witness: &PanelWitnessCommit) -> [u8; SIGNATURE_BYTES] {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
    *libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, &witness.id().0.0, WITNESS_SIGN_CONTEXT, [0; 32]).unwrap().as_ref()
}
struct Fixture {
    binding: PalwTirStepBindingV1,
    generated: Vec<u32>,
    parts: Vec<(Vec<u8>, Option<Vec<u8>>)>,
}
fn fixture() -> Fixture {
    // Independent tiny arithmetic: logits = [7,13,2,13] + scalar_embed[token].
    // The prompt's final token and all generated inputs are 1, whose scalar is
    // 1, so both checked rows are [8,14,3,14] and lowest-index greedy is 1.
    let mut pb = ProgramBuilder::new(4, 1 << 18);
    let embed = pb.param("scalar_embed", DType::I8, &[4, 1], false);
    let mut pre = pb.block("pre", vec![]);
    let x = pre.gather(embed, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[x]);
    let bias = pb.konst(DType::I32, &[4], &[7, 13, 2, 13]);
    let mut post = pb.block("post", vec![TensorType::fixed(DType::I8, &[1])]);
    let scalar = post.cast(Ref::CarryIn(0), DType::I32);
    let logits = post.add(bias, scalar, DType::I32);
    let Ref::Node(logits_node) = logits else { unreachable!() };
    post.commit(logits);
    let post = post.finish(&[]);
    let mut program = tiled(pb.finish(pre, vec![], post, logits_node));
    for block in &mut program.blocks {
        for node in &mut block.nodes {
            node.commit = true;
        }
    }
    let mut map = MapParams::default();
    map.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 1], vec![0, 1, 2, 3]).unwrap());
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let artifact = inventory.tree.root();
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: layout(&program, 3, 2, 2, 64),
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let class_id = class.class_id(&artifact);
    let prompt = vec![0, 1];
    let mut job = job(&program, 2, 2, class_id, &prompt, PalwPromptIdsFormV1::Flat);
    job.tokenizer_id = class.tokenizer_id;
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let runner = TirClassRunnerV1::new(&space, &plan, &params, class_id).unwrap();
    let mut leaves = Vec::new();
    let run = runner.run(&job, &prompt, limits().max_leaves, true, &mut |l| leaves.push(l.preimage.clone())).unwrap();
    assert_eq!(run.generated, vec![1, 1]);
    assert_eq!(run.logits_rows, vec![vec![8, 14, 3, 14], vec![8, 14, 3, 14]]);
    let binding = PalwTirStepBindingV1 {
        version: PALW_TIR_STEP_BINDING_VERSION_V1,
        job_context: job,
        class,
        artifact_root: artifact,
        full_logits_trace_root: run.trace_root,
        step_leaf_count: run.leaf_count,
        step_merkle_root: run.step_merkle_root,
        committed_execution_root: run.execution_root,
    };
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: limits().max_leaves,
        prompt_form: PalwPromptIdsFormV1::Flat,
        limits: DemandLimits { max_elements: limits().max_elements_per_leaf, max_terms: limits().max_terms_per_leaf },
    };
    let store = TirEvidenceV1::dense(
        &runner,
        &binding,
        &prompt,
        &run.logits_rows,
        &run.generated,
        &leaves,
        &inventory,
        PalwPromptIdsFormV1::Flat,
        limits().max_leaves,
    )
    .unwrap();
    let parts = (0..binding.step_leaf_count)
        .map(|i| {
            let cone = store.cone_refutation(i, &rules).unwrap();
            let leaf = space.leaf_at(&binding.job_context, i).unwrap();
            let trace = match leaf.kind {
                PalwTirLeafKindV1::Commit { occurrence, node, .. }
                    if occurrence as usize == space.occurrences().len() - 1 && node == program.logits =>
                {
                    Some(
                        tir_logits_event_disclosure_v1(
                            &binding,
                            &run.logits_rows,
                            &run.generated,
                            leaf.position + 1 - binding.job_context.declared_prefill_tokens,
                            0,
                        )
                        .unwrap(),
                    )
                }
                _ => None,
            };
            (borsh::to_vec(&cone).unwrap(), trace.map(|t| borsh::to_vec(&t).unwrap()))
        })
        .collect();
    Fixture { binding, generated: run.generated, parts }
}
impl Fixture {
    fn result(&self, output: Digest) -> MinerResultCommit {
        let job = &self.binding.job_context;
        MinerResultCommit {
            version: 2,
            revision: 1,
            scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
            job: native(job.job_id),
            execution: native(job.context_hash()),
            miner: principal(&miner_key()),
            registered_class: native(self.binding.class.class_id(&self.binding.artifact_root)),
            artifact: native(self.binding.artifact_root),
            program: native(self.binding.class.graph_ir_root()),
            input: native(job.prompt_token_ids_hash),
            tokenizer: native(job.tokenizer_id),
            execution_rules: native(job.cu_ruleset_id),
            selector: misaka_palw_positive_contract::tir::greedy_selector_id(),
            generation_seed: job.execution_seed,
            temperature_q24: 0,
            prefill: job.declared_prefill_tokens,
            decode: job.exact_decode_tokens,
            max_context: job.max_context_tokens,
            output,
            dependencies: d(21),
            service_contract: d(22),
        }
    }
    fn correct_result(&self) -> MinerResultCommit {
        self.result(native(palw_attempt_output_root_v1(&self.binding.job_context, &self.generated)))
    }
    fn prepare(&self, result: &MinerResultCommit, limits: CheckLimits) -> PreparedCheck {
        PreparedCheck::authenticate(
            &rooted(result),
            &self.binding.class,
            self.binding.artifact_root,
            &self.binding.job_context,
            PalwPromptIdsFormV1::Flat,
            limits,
            result.clone(),
            &miner_key(),
            &sign_result(result),
        )
        .unwrap()
    }
    fn witness(&self, result: &MinerResultCommit, plan: CoverageDescriptor) -> PanelWitnessCommit {
        PanelWitnessCommit {
            version: 2,
            revision: 1,
            scope: result.scope,
            case: result.case_id(),
            result: result.id(),
            author: principal(&panel_key()),
            attempt: 0,
            profile: CheckProfile::ExactStreamV1,
            plan: plan.plan,
            execution_record: native(self.binding.committed_execution_root),
            provenance: misaka_palw_positive_contract::tir::provenance_id(result),
            candidate_output: native(palw_attempt_output_root_v1(&self.binding.job_context, &self.generated)),
        }
    }
    fn context(&self, result: &MinerResultCommit, limits: CheckLimits) -> (CheckContext, PanelWitnessCommit) {
        let prepared = self.prepare(result, limits);
        let witness = self.witness(result, prepared.descriptor());
        let context = prepared
            .authenticate_witness(witness.clone(), self.binding.clone(), self.generated.clone(), &panel_key(), &sign_witness(&witness))
            .unwrap();
        (context, witness)
    }
    fn frames(&self, witness: &PanelWitnessCommit, descriptor: CoverageDescriptor) -> Vec<Vec<u8>> {
        self.parts
            .iter()
            .enumerate()
            .map(|(index, (cone, trace))| {
                let payload =
                    ExactPart::new(cone, trace.as_deref(), descriptor.max_part_bytes - EVIDENCE_HEADER_BYTES as u64).unwrap().encode();
                frame_bytes(witness, descriptor, index as u64, &payload)
            })
            .collect()
    }
}
fn rooted(result: &MinerResultCommit) -> AdmissionContext {
    AdmissionContext {
        scope: result.scope,
        expected_job: result.job,
        expected_execution: result.execution,
        expected_miner: result.miner,
        expected_input: result.input,
        registered_class: result.registered_class,
        artifact: result.artifact,
        program: result.program,
        tokenizer: result.tokenizer,
        execution_rules: result.execution_rules,
        selector: result.selector,
        required_form: ClaimForm::PositiveResultV2,
    }
}
fn frame_bytes(witness: &PanelWitnessCommit, descriptor: CoverageDescriptor, index: u64, payload: &[u8]) -> Vec<u8> {
    EvidenceFrame::new(
        EvidenceHeader {
            case: witness.case,
            result: witness.result,
            witness: witness.id(),
            plan: descriptor.plan,
            first_relation: index,
            relation_count: 1,
            payload_bytes: payload.len() as u32,
        },
        payload,
        descriptor.max_part_bytes,
    )
    .unwrap()
    .encode(descriptor.max_part_bytes)
    .unwrap()
}
fn submit(context: &CheckContext, progress: &mut ContractProgress, bytes: &[u8]) -> Result<(), AdapterError> {
    let descriptor = TirNodeChecker.derive_plan(context)?;
    TirNodeChecker.check_part(context, progress, EvidenceFrame::decode(bytes, descriptor.max_part_bytes)?)
}

pub fn layout(p: &TirProgramV1, seed: u32, c: u32, h: u32, max_context: u32) -> PalwTirLayoutV1 {
    let tiled = Hash64::from_bytes(p.logits_scheme_id) == tiled_logits_scheme_id_v1();
    let mut commit_tiles = Vec::new();
    let mut k = 0u32;
    for (bi, b) in p.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if !n.commit {
                continue;
            }
            let logits = bi == p.schedule.post as usize && ni == p.logits as usize;
            commit_tiles.push(if logits && tiled { PALW_LOGITS_TILE_LANES as u32 } else { 4 + (k.wrapping_mul(seed) % 6) });
            k += 1;
        }
    }
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context,
        checkpoint_interval: c,
        h_tile: h,
        commit_tiles,
        state_tiles: p.states.iter().enumerate().map(|(j, _)| 4 + ((j as u32 * seed) % 3)).collect(),
    }
}

/// A well-formed job context for `p`'s class: the trace scheme its logits scheme pins (the flat
/// scheme runs under the v2 trace id, as the integer families' contexts do), and the prompt
/// committed under `form`.
pub fn job(
    p: &TirProgramV1,
    prefill: u32,
    decode: u32,
    class_id: Hash64,
    prompt: &[u32],
    form: PalwPromptIdsFormV1,
) -> PalwJobContextV2 {
    let z = Hash64::from_bytes([0u8; 64]);
    let tiled = Hash64::from_bytes(p.logits_scheme_id) == tiled_logits_scheme_id_v1();
    PalwJobContextV2 {
        version: 2,
        network_id: b"testnet-12".to_vec(),
        job_id: Hash64::from_bytes([prefill as u8 ^ 0x5A; 64]),
        job_nullifier: z,
        assignment_id: z,
        execution_seed: [decode as u8; 32],
        model_profile_id: z,
        runtime_manifest_hash: z,
        runtime_class_id: z,
        shape_profile_id: class_id,
        trace_scheme_id: if tiled { tiled_logits_scheme_id_v1() } else { kaspa_consensus_core::palw_v2::trace_scheme_id_v2() },
        cu_ruleset_id: z,
        tokenizer_id: z,
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(form, prompt).expect("a short prompt commits"),
        declared_prefill_tokens: prefill,
        exact_decode_tokens: decode,
        max_context_tokens: 1024,
    }
}

struct State {
    scope: Cell<ChainScope>,
    facts: Cell<RootedCreditFacts>,
    entitlements: Cell<FixtureEntitlements>,
}
#[derive(Clone, Copy)]
enum FixtureEntitlements {
    All,
    Only(&'static [CreditUse]),
}
impl RootedCreditView for State {
    fn scope(&self) -> ChainScope {
        self.scope.get()
    }
    fn claim(&self, claim: ClaimId) -> Result<RootedCreditFacts, CreditDenial> {
        let facts = self.facts.get();
        if facts.binding.claim == claim { Ok(facts) } else { Err(CreditDenial::MissingClaim(claim)) }
    }
    fn checked_dependencies(&self, _: ClaimId) -> Result<&[CreditBinding], CreditDenial> {
        // This fixture explicitly resolves its configured empty dependency list.
        // The real A4 adapter must authenticate the result's dependency digest.
        Ok(&[])
    }
    fn authorize_entitlement(&self, binding: &CreditBinding, usage: CreditUse) -> Result<(), CreditDenial> {
        if self.claim(binding.claim)?.binding != *binding {
            return Err(CreditDenial::Binding(binding.claim));
        }
        match self.entitlements.get() {
            FixtureEntitlements::All => Ok(()),
            FixtureEntitlements::Only(uses) if uses.contains(&usage) => Ok(()),
            FixtureEntitlements::Only(_) => Err(CreditDenial::Entitlement { claim: binding.claim, usage }),
        }
    }
}
fn economic_fixture(result: &MinerResultCommit, descriptor: CoverageDescriptor) -> State {
    let binding = CreditBinding {
        scope: result.scope,
        claim: result.claim_id(),
        case: result.case_id(),
        result: result.id(),
        output: result.output,
        dependencies: result.dependencies,
    };
    State {
        scope: Cell::new(result.scope),
        entitlements: Cell::new(FixtureEntitlements::All),
        facts: Cell::new(RootedCreditFacts {
            binding,
            plan: descriptor.plan,
            mandatory_relations: descriptor.mandatory_relations,
            status: ClaimStatus::PositivePassed,
            gate_c: GateCApproval::Approved(binding),
            fraud: FraudState::Clear,
            maturity_and_liability: EconomicReadiness::Ready,
            carrier_and_funding: EconomicReadiness::Ready,
        }),
    }
}
fn complete(f: &Fixture, result: &MinerResultCommit) -> (CoverageDescriptor, CheckedTerminal) {
    let (context, witness) = f.context(result, limits());
    let descriptor = TirNodeChecker.derive_plan(&context).unwrap();
    let mut progress = context.start().unwrap();
    for frame in f.frames(&witness, descriptor) {
        submit(&context, &mut progress, &frame).unwrap();
    }
    (descriptor, TirNodeChecker.finish(&context, progress).unwrap())
}
fn gate<'a>(view: &'a TirCreditView<'a, State>) -> PositiveCreditGate<'a, TirCreditView<'a, State>> {
    PositiveCreditGate::new(view, CreditLimits { max_claims: 10, max_edges: 10 }).unwrap()
}
fn denied_all(view: &TirCreditView<'_, State>, claim: ClaimId, expected: CreditDenial) {
    for usage in ALL_CREDIT_USES {
        assert_eq!(gate(view).authorize(&claim, usage).err(), Some(expected));
    }
    let g = gate(view);
    let uses = CreditConsumers::new(&g);
    assert!(uses.inference_reward(&claim).is_err());
    assert!(uses.worker_payment(&claim).is_err());
    assert!(uses.work_receipt(&claim).is_err());
    assert_eq!(uses.dag_weight(&claim, 17), 0);
    assert_eq!(uses.rule_e_capital(&claim, 19), 0);
    assert!(uses.settlement(&claim).is_err());
    assert!(uses.prefix_reuse(&claim).is_err());
    assert!(uses.dependent_claim(&claim).is_err());
}

#[test]
fn actual_a1_checked_terminal_reaches_all_consumers_only_with_all_existing_conditions() {
    let f = fixture();
    let result = f.correct_result();
    let (descriptor, terminal) = complete(&f, &result);
    assert_eq!(terminal.outcome(), CheckOutcome::Matched);
    let state = economic_fixture(&result, descriptor);
    let mut view = TirCreditView::new(&state, 1).unwrap();
    denied_all(&view, result.claim_id(), CreditDenial::MissingLocalVerification(result.claim_id()));
    let signature = sign_result(&result);
    let key = miner_key();
    let authenticated = result.authenticate(&key, &signature).unwrap();
    assert_eq!(view.record_checked(&authenticated, terminal), Ok(TerminalEffect::Recorded));
    assert_eq!(view.recorded_terminals(), 1);
    let g = gate(&view);
    let uses = CreditConsumers::new(&g);
    assert_eq!(uses.inference_reward(&result.claim_id()).unwrap().usage(), CreditUse::Reward);
    assert_eq!(uses.worker_payment(&result.claim_id()).unwrap().usage(), CreditUse::WorkerPayment);
    assert_eq!(uses.work_receipt(&result.claim_id()).unwrap().usage(), CreditUse::WorkReceipt);
    assert_eq!(uses.dag_weight(&result.claim_id(), 17), 17);
    assert_eq!(uses.rule_e_capital(&result.claim_id(), 19), 19);
    assert_eq!(uses.settlement(&result.claim_id()).unwrap().usage(), CreditUse::Settlement);
    assert_eq!(uses.prefix_reuse(&result.claim_id()).unwrap().usage(), CreditUse::PrefixReuse);
    assert_eq!(uses.dependent_claim(&result.claim_id()).unwrap().usage(), CreditUse::Dependency);
    let mut facts = state.facts.get();
    facts.gate_c = GateCApproval::Pending;
    state.facts.set(facts);
    denied_all(&view, result.claim_id(), CreditDenial::GateC(result.claim_id()));
    facts.gate_c = GateCApproval::Approved(facts.binding);
    facts.fraud = FraudState::AcceptedPending;
    state.facts.set(facts);
    denied_all(&view, result.claim_id(), CreditDenial::Fraud(result.claim_id()));
}

#[test]
fn checked_terminal_delegates_paid_worker_rights_without_promoting_them_to_primary_reward() {
    let f = fixture();
    let result = f.correct_result();
    let (descriptor, terminal) = complete(&f, &result);
    let state = economic_fixture(&result, descriptor);
    state.entitlements.set(FixtureEntitlements::Only(&[CreditUse::WorkerPayment, CreditUse::Settlement]));
    let mut view = TirCreditView::new(&state, 1).unwrap();
    let key = miner_key();
    let signature = sign_result(&result);
    view.record_checked(&result.authenticate(&key, &signature).unwrap(), terminal).unwrap();
    assert_eq!(view.recorded_terminals(), 1);
    assert!(view.checked_coverage(result.claim_id()).is_ok());
    for usage in ALL_CREDIT_USES {
        let expected = state.authorize_entitlement(&state.facts.get().binding, usage);
        assert_eq!(view.authorize_entitlement(&state.facts.get().binding, usage), expected);
        assert_eq!(gate(&view).authorize(&result.claim_id(), usage).err(), expected.err());
    }
    let g = gate(&view);
    let uses = CreditConsumers::new(&g);
    assert_eq!(uses.dag_weight(&result.claim_id(), 17), 0);
    assert_eq!(uses.rule_e_capital(&result.claim_id(), 19), 0);
    assert!(uses.inference_reward(&result.claim_id()).is_err());
    assert!(uses.work_receipt(&result.claim_id()).is_err());
    assert_eq!(uses.worker_payment(&result.claim_id()).unwrap().usage(), CreditUse::WorkerPayment);
    assert!(uses.settlement(&result.claim_id()).is_ok());
    // Changes in the current rooted entitlement cannot be hidden by the saved
    // terminal or a previously successful settlement authorization.
    state.entitlements.set(FixtureEntitlements::Only(&[]));
    for usage in ALL_CREDIT_USES {
        assert_eq!(
            gate(&view).authorize(&result.claim_id(), usage).err(),
            Some(CreditDenial::Entitlement { claim: result.claim_id(), usage })
        );
    }
}

struct MissingEntitlement<'a>(&'a State);
impl RootedCreditView for MissingEntitlement<'_> {
    fn scope(&self) -> ChainScope {
        self.0.scope()
    }
    fn claim(&self, claim: ClaimId) -> Result<RootedCreditFacts, CreditDenial> {
        self.0.claim(claim)
    }
    fn checked_dependencies(&self, claim: ClaimId) -> Result<&[CreditBinding], CreditDenial> {
        self.0.checked_dependencies(claim)
    }
    // Deliberately no authorize_entitlement: exercise the closed trait default.
}

#[test]
fn checked_terminal_cannot_replace_a_missing_rooted_entitlement_port_for_any_of_eight_uses() {
    let f = fixture();
    let result = f.correct_result();
    let (descriptor, terminal) = complete(&f, &result);
    let state = economic_fixture(&result, descriptor);
    let missing = MissingEntitlement(&state);
    let mut view = TirCreditView::new(&missing, 1).unwrap();
    let key = miner_key();
    let signature = sign_result(&result);
    view.record_checked(&result.authenticate(&key, &signature).unwrap(), terminal).unwrap();
    assert!(view.checked_coverage(result.claim_id()).is_ok());
    let gate = PositiveCreditGate::new(&view, CreditLimits { max_claims: 10, max_edges: 10 }).unwrap();
    assert_eq!(ALL_CREDIT_USES.len(), 8);
    for usage in ALL_CREDIT_USES {
        let denied = CreditDenial::Entitlement { claim: result.claim_id(), usage };
        assert_eq!(view.authorize_entitlement(&state.facts.get().binding, usage), Err(denied));
        assert_eq!(gate.authorize(&result.claim_id(), usage).err(), Some(denied));
    }
}

#[test]
fn real_output_mismatch_is_not_credit_even_if_a_saved_row_falsely_says_passed() {
    let f = fixture();
    let result = f.result(d(99));
    let (descriptor, terminal) = complete(&f, &result);
    assert_eq!(terminal.outcome(), CheckOutcome::OutputMismatch);
    let state = economic_fixture(&result, descriptor);
    let mut view = TirCreditView::new(&state, 1).unwrap();
    let signature = sign_result(&result);
    let key = miner_key();
    view.record_checked(&result.authenticate(&key, &signature).unwrap(), terminal).unwrap();
    denied_all(&view, result.claim_id(), CreditDenial::PositiveNotPassed(result.claim_id()));
}

#[test]
fn a_valid_terminal_cannot_be_rebound_to_another_signed_result() {
    let f = fixture();
    let result = f.correct_result();
    let (descriptor, terminal) = complete(&f, &result);
    let mut other = result.clone();
    other.output = d(99);
    let mut other_descriptor = descriptor;
    other_descriptor.result = other.id();
    let state = economic_fixture(&other, other_descriptor);
    let mut view = TirCreditView::new(&state, 1).unwrap();
    let signature = sign_result(&other);
    let key = miner_key();
    assert_eq!(
        view.record_checked(&other.authenticate(&key, &signature).unwrap(), terminal),
        Err(CreditDenial::Binding(other.claim_id()))
    );
    assert_eq!(view.recorded_terminals(), 0);
}

#[test]
fn reorg_or_new_process_requires_current_scope_and_actual_rechecked_coverage() {
    let f = fixture();
    let result = f.correct_result();
    let (descriptor, terminal) = complete(&f, &result);
    let state = economic_fixture(&result, descriptor);
    let mut view = TirCreditView::new(&state, 1).unwrap();
    let signature = sign_result(&result);
    let key = miner_key();
    view.record_checked(&result.authenticate(&key, &signature).unwrap(), terminal).unwrap();
    state.scope.set(ChainScope { branch: d(77), ..result.scope });
    denied_all(&view, result.claim_id(), CreditDenial::Scope(result.claim_id()));
    state.scope.set(result.scope);
    drop(view);
    // Restoring only the state flag does not recreate the actual A1 terminal.
    let restarted = TirCreditView::new(&state, 1).unwrap();
    denied_all(&restarted, result.claim_id(), CreditDenial::MissingLocalVerification(result.claim_id()));
}

#[test]
fn private_result_witness_and_request_frames_do_not_enter_legacy_native_ingress() {
    use kaspa_consensus_core::{
        palw_attempt_v2::PalwAttemptEnvelopeV2, palw_native_resource_v1::preflight_native_value_v1,
        palw_state_v2::PalwConsensusObjectV2,
    };
    use misaka_palw_kernel::route::KernelRouteObjectV1;
    let f = fixture();
    let result = f.correct_result();
    let plan = f.prepare(&result, limits()).descriptor();
    let witness = f.witness(&result, plan);
    let request = SourceRequest {
        version: 2,
        revision: 1,
        case: result.case_id(),
        contract: result.service_contract,
        source: result.artifact,
        kind: SourceKind::ArtifactInventory,
        first: 0,
        end: 1,
    };
    for bytes in [encode_result(&result).unwrap(), encode_witness(&witness).unwrap(), encode_request(&request).unwrap()] {
        assert!(PalwAttemptEnvelopeV2::decode_wire(&bytes).is_err());
        assert!(KernelRouteObjectV1::decode(&bytes).is_err());
        assert!(preflight_native_value_v1::<PalwConsensusObjectV2>(&bytes[..], bytes.len() as u64).is_err());
    }
}

#[test]
fn repeated_real_terminal_does_not_create_another_credit_record_or_skip_rooted_plan_binding() {
    let f = fixture();
    let result = f.correct_result();
    let (descriptor, terminal) = complete(&f, &result);
    let (_, replayed_terminal) = complete(&f, &result);
    let state = economic_fixture(&result, descriptor);
    let mut view = TirCreditView::new(&state, 1).unwrap();
    let signature = sign_result(&result);
    let key = miner_key();
    let authenticated = result.authenticate(&key, &signature).unwrap();
    assert_eq!(view.record_checked(&authenticated, terminal), Ok(TerminalEffect::Recorded));
    assert_eq!(view.record_checked(&authenticated, replayed_terminal), Ok(TerminalEffect::AlreadyRecorded));
    assert_eq!(view.recorded_terminals(), 1);
    assert_eq!(TirCreditView::new(&state, 0).err(), Some(CreditDenial::Budget));
    let (_, third_terminal) = complete(&f, &result);
    let mut facts = state.facts.get();
    facts.plan = PlanId(d(111));
    state.facts.set(facts);
    assert_eq!(view.record_checked(&authenticated, third_terminal), Err(CreditDenial::Binding(result.claim_id())));
    assert_eq!(view.recorded_terminals(), 1);
}

#[test]
fn signed_evidence_packets_authenticate_before_checking_and_charge_the_complete_signed_frame() {
    use misaka_palw_positive_contract::ContractError;
    let f = fixture();
    let result = f.correct_result();
    let (context, witness) = f.context(&result, limits());
    let descriptor = TirNodeChecker.derive_plan(&context).unwrap();
    let frames = f.frames(&witness, descriptor);
    let signer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
    let public_key = panel_key();
    let mut progress = context.start().unwrap();
    let mut signed_bytes = 0u64;
    let quote = context.signed_transport_quote().unwrap();
    assert_eq!(quote.max_packet_bytes, descriptor.max_part_bytes + SIGNATURE_BYTES as u64);

    for bytes in &frames {
        let frame = EvidenceFrame::decode(bytes, descriptor.max_part_bytes).unwrap();
        let signed_size = frame.signed_wire_bytes();
        assert_eq!(signed_size, frame.wire_bytes() + SIGNATURE_BYTES as u64);
        let signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&signer.signing_key, &frame.id().0.0, EVIDENCE_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
        let before = progress.snapshot();
        let before_wire = progress.charged_wire_bytes();
        let before_signature = progress.charged_signature_bytes();
        let before_transport = progress.charged_transport_bytes().unwrap();

        let mut bad_signature = signature;
        bad_signature[0] ^= 1;
        let rejected = frame
            .authenticate(&witness, &public_key, &bad_signature, signed_size)
            .map(|authenticated| TirNodeChecker.check_authenticated_part(&context, &mut progress, authenticated));
        assert!(matches!(rejected, Err(ContractError::Signature)));
        assert_eq!(progress.snapshot(), before);
        assert_eq!(progress.charged_wire_bytes(), before_wire);
        assert_eq!(progress.charged_signature_bytes(), before_signature);
        assert_eq!(progress.charged_transport_bytes().unwrap(), before_transport);

        let over_budget = frame
            .authenticate(&witness, &public_key, &signature, signed_size - 1)
            .map(|authenticated| TirNodeChecker.check_authenticated_part(&context, &mut progress, authenticated));
        assert!(matches!(over_budget, Err(ContractError::Budget)));
        assert_eq!(progress.snapshot(), before);
        assert_eq!(progress.charged_wire_bytes(), before_wire);
        assert_eq!(progress.charged_signature_bytes(), before_signature);
        assert_eq!(progress.charged_transport_bytes().unwrap(), before_transport);

        let authenticated = frame.authenticate(&witness, &public_key, &signature, signed_size).unwrap();
        assert_eq!(authenticated.author(), witness.author);
        assert_eq!(authenticated.id(), frame.id());
        assert_eq!(authenticated.frame().header(), frame.header());
        TirNodeChecker.check_authenticated_part(&context, &mut progress, authenticated).unwrap();
        signed_bytes += signed_size;
    }
    assert_eq!(progress.snapshot().next_leaf, descriptor.mandatory_relations);
    assert_eq!(progress.charged_signature_bytes(), frames.len() as u64 * SIGNATURE_BYTES as u64);
    assert_eq!(signed_bytes, progress.charged_transport_bytes().unwrap());
    assert!(progress.charged_transport_bytes().unwrap() <= quote.total_packet_bytes);
    assert_eq!(TirNodeChecker.finish(&context, progress).unwrap().outcome(), CheckOutcome::Matched);
}
