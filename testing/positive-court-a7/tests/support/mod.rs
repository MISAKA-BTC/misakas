use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1},
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_court_v1::*,
    palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirStepBindingV1, PalwTirStepSpaceV1},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::{identity::*, messages::*, ports::NodeChecker, tir, wire::*};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, demand::DemandLimits};
use misaka_palw_tir_exec::{
    NoSink, Slice, TirExecutor, TirParams, TirPlan,
    node::{
        TirClassRunnerV1, TirEvidenceV1, TirHeldInventoryV1, TirParamsSourceV1,
        positive_check::CheckLimits,
        positive_contract::{AdapterError, CheckContext, PreparedCheck},
    },
};

pub const ROWS: [[i128; 4]; 4] = [[4, 2, -3, 1], [-1, 7, 3, 2], [5, 6, 3, -2], [-8, -2, -3, -1]];
pub fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
pub fn d(h: Hash64) -> Digest {
    Digest(h.as_bytes())
}
pub fn rows(alternate: bool) -> [[i128; 4]; 4] {
    let mut r = ROWS;
    if alternate {
        r[1][0] = 99;
    }
    r
}
/// Arithmetic expectation independent of TIR, the Panel builder and Node checker.
pub fn oracle(alternate: bool, prompt: u32) -> (Vec<u32>, Vec<[i128; 4]>) {
    let rows = rows(alternate);
    let mut token = prompt as usize;
    let mut ids = Vec::new();
    let mut logits = Vec::new();
    for _ in 0..3 {
        let row = std::array::from_fn(|i| rows[token][i] * [3, -2, 5, 1][i]);
        token = (1..4).fold(0, |best, i| if row[i] > row[best] { i } else { best });
        ids.push(token as u32);
        logits.push(row);
    }
    (ids, logits)
}
pub fn limits() -> CheckLimits {
    CheckLimits {
        max_leaves: 4096,
        max_part_bytes: 1 << 20,
        max_part_heap: 8 << 20,
        max_total_bytes: 1 << 32,
        max_elements_per_leaf: 1 << 18,
        max_terms_per_leaf: 1 << 18,
        max_total_elements: 1 << 32,
        max_total_terms: 1 << 32,
    }
}
pub fn program() -> misaka_palw_tir::TirProgramV1 {
    let mut b = ProgramBuilder::new(4, 1 << 18);
    let emb = b.param("a7-embedding", DType::I8, &[4, 4], false);
    let scale = b.konst(DType::I32, &[4], &[3, -2, 5, 1]);
    let mut pre = b.block("pre", vec![]);
    let x = pre.gather(emb, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[x]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let x = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(x);
    let y = post.mul(x, scale, DType::I32);
    post.commit(y);
    let Ref::Node(logits) = y else { unreachable!() };
    let post = post.finish(&[]);
    let mut p = b.finish(pre, vec![], post, logits);
    // This defines a NEW registered program, never changes an existing class.
    for block in &mut p.blocks {
        for node in &mut block.nodes {
            node.commit = true;
        }
    }
    p.logits_scheme_id = kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_bytes();
    p
}
pub fn map(alternate: bool) -> MapParams {
    let mut m = MapParams::default();
    m.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 4], rows(alternate).into_iter().flatten().collect()).unwrap());
    m
}
pub fn sign(seed: u8, id: Digest, context: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let k = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32]);
    let s = libcrux_ml_dsa::ml_dsa_87::sign(&k.signing_key, &id.0, context, [seed + 1; 32]).unwrap();
    (k.verification_key.as_ref().to_vec(), s.as_ref().to_vec())
}
pub fn miner_identity() -> PrincipalId {
    let k = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([51; 32]);
    principal(k.verification_key.as_ref().try_into().unwrap())
}

/// Simulated rooted registration and canonical job, serialized SEPARATELY from
/// candidate bytes by the trusted fixture initializer before any packet arrives.
#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct Trusted {
    pub class: PalwTirClassV1,
    pub artifact: Hash64,
    pub job: PalwJobContextV2,
    pub scope: ChainScope,
    pub miner: PrincipalId,
}
impl Trusted {
    pub fn admission(&self) -> AdmissionContext {
        AdmissionContext {
            scope: self.scope,
            expected_job: d(self.job.job_id),
            expected_execution: d(self.job.context_hash()),
            expected_miner: self.miner,
            expected_input: d(self.job.prompt_token_ids_hash),
            registered_class: d(self.class.class_id(&self.artifact)),
            artifact: d(self.artifact),
            program: d(self.class.graph_ir_root()),
            tokenizer: d(self.class.tokenizer_id),
            execution_rules: d(self.job.cu_ruleset_id),
            selector: tir::greedy_selector_id(),
            required_form: ClaimForm::PositiveResultV2,
        }
    }
}
pub fn registration(alternate: bool, prompt: u32, job_tag: u64) -> Trusted {
    let p = program();
    let plan = TirPlan::compile(&p).unwrap();
    let m = map(alternate);
    let params = TirParams::from_map(&plan, &m).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&p, &source).unwrap();
    let artifact = inventory.tree.root();
    let class = PalwTirClassV1 {
        version: 1,
        program: p.encode(),
        tokenizer_id: h(18),
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: 16,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: vec![4, 4, 4096],
            state_tiles: vec![],
        },
    };
    let mut job =
        kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class.class_id(&artifact), (1, 3)).unwrap();
    job.tokenizer_id = class.tokenizer_id;
    job.job_id = h(job_tag);
    job.prompt_token_ids_hash = prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &[prompt]).unwrap();
    Trusted {
        class,
        artifact,
        job,
        scope: ChainScope { network: d(h(1)), branch: d(h(2)), ruleset: d(h(3)) },
        miner: miner_identity(),
    }
}
pub fn infer(t: &Trusted, alternate: bool, prompt: u32) -> Vec<u32> {
    let plan = TirPlan::compile(&program()).unwrap();
    let m = map(alternate);
    let params = TirParams::from_map(&plan, &m).unwrap();
    let mut e = TirExecutor::new(&plan, &params).unwrap();
    let mut token = prompt;
    let mut ids = Vec::new();
    for _ in 0..3 {
        e.step_opt(token, &mut NoSink, true).unwrap();
        let Slice::I32(row) = e.logits().1 else { unreachable!() };
        token = (1..row.len()).fold(0, |best, i| if row[i] > row[best] { i } else { best }) as u32;
        ids.push(token);
    }
    assert_eq!(ids, oracle(alternate, prompt).0);
    assert_eq!(t.job.exact_decode_tokens, 3);
    ids
}
pub fn result(t: &Trusted, alternate: bool, prompt: u32) -> MinerResultCommit {
    let ids = infer(t, alternate, prompt);
    MinerResultCommit {
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
        selector: tir::greedy_selector_id(),
        generation_seed: t.job.execution_seed,
        temperature_q24: 0,
        prefill: 1,
        decode: 3,
        max_context: t.job.max_context_tokens,
        output: d(palw_attempt_output_root_v1(&t.job, &ids)),
        dependencies: d(h(5)),
        service_contract: d(h(6)),
    }
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct Packet {
    pub result: MinerResultCommit,
    pub witness: PanelWitnessCommit,
    pub binding: PalwTirStepBindingV1,
    pub generated: Vec<u32>,
    pub miner_key: Vec<u8>,
    pub miner_sig: Vec<u8>,
    pub panel_key: Vec<u8>,
    pub panel_sig: Vec<u8>,
    pub frames: Vec<Vec<u8>>,
    pub frame_signatures: Vec<Vec<u8>>,
}
impl Packet {
    /// A4-style delivery preflight for this fixture. Draw membership, current
    /// duty, funding and native admission are deliberately not claimed here.
    pub fn authenticated<'a>(
        &self,
        bytes: &'a [u8],
    ) -> Result<AuthenticatedEvidenceFrame<'a>, misaka_palw_positive_contract::ContractError> {
        use misaka_palw_positive_contract::ContractError;
        let frame = EvidenceFrame::decode(bytes, limits().max_part_bytes)?;
        let index = usize::try_from(frame.header().first_relation).map_err(|_| ContractError::Range)?;
        let signature = self.frame_signatures.get(index).ok_or(ContractError::Signature)?;
        frame.authenticate(
            &self.witness,
            self.panel_key.as_slice().try_into().map_err(|_| ContractError::Identity)?,
            signature.as_slice().try_into().map_err(|_| ContractError::Signature)?,
            limits().max_part_bytes + SIGNATURE_BYTES as u64,
        )
    }
    pub fn resign_frame(&mut self, index: usize) {
        let frame = EvidenceFrame::decode(&self.frames[index], limits().max_part_bytes).unwrap();
        self.frame_signatures[index] = sign(61, frame.id().0, EVIDENCE_SIGN_CONTEXT).1;
    }
    pub fn authenticate(&self, t: &Trusted) -> Result<CheckContext, AdapterError> {
        self.authenticate_limits(t, limits())
    }
    pub fn authenticate_limits(&self, t: &Trusted, policy: CheckLimits) -> Result<CheckContext, AdapterError> {
        use misaka_palw_positive_contract::ContractError;
        // These vectors originate in a decoded candidate, unlike the fixed
        // trusted registration. Malformed actor credentials are an admission
        // refusal, not a Node process panic or a computation verdict.
        let miner_key = self.miner_key.as_slice().try_into().map_err(|_| ContractError::Identity)?;
        let miner_sig = self.miner_sig.as_slice().try_into().map_err(|_| ContractError::Signature)?;
        let panel_key = self.panel_key.as_slice().try_into().map_err(|_| ContractError::Identity)?;
        let panel_sig = self.panel_sig.as_slice().try_into().map_err(|_| ContractError::Signature)?;
        let prepared = PreparedCheck::authenticate(
            &t.admission(),
            &t.class,
            t.artifact,
            &t.job,
            PalwPromptIdsFormV1::Flat,
            policy,
            self.result.clone(),
            miner_key,
            miner_sig,
        )?;
        prepared.authenticate_witness(self.witness.clone(), self.binding.clone(), self.generated.clone(), panel_key, panel_sig)
    }
    pub fn resign_and_reframe(&mut self) {
        let payloads: Vec<_> =
            self.frames.iter().map(|f| EvidenceFrame::decode(f, limits().max_part_bytes).unwrap().payload().to_vec()).collect();
        self.witness.case = self.result.case_id();
        self.witness.result = self.result.id();
        self.witness.plan =
            coverage_plan_id(&self.result, CheckProfile::ExactStreamV1, tir::exact_derivation_id(), self.binding.step_leaf_count)
                .unwrap();
        self.witness.provenance = tir::provenance_id(&self.result);
        (self.miner_key, self.miner_sig) = sign(51, self.result.id().0, RESULT_SIGN_CONTEXT);
        (self.panel_key, self.panel_sig) = sign(61, self.witness.id().0, WITNESS_SIGN_CONTEXT);
        self.frames = payloads.into_iter().enumerate().map(|(i, p)| frame(&self.result, &self.witness, i as u64, &p)).collect();
        self.frame_signatures = vec![vec![]; self.frames.len()];
        for i in 0..self.frames.len() {
            self.resign_frame(i);
        }
    }
}
pub fn frame(r: &MinerResultCommit, w: &PanelWitnessCommit, i: u64, payload: &[u8]) -> Vec<u8> {
    EvidenceFrame::new(
        EvidenceHeader {
            case: r.case_id(),
            result: r.id(),
            witness: w.id(),
            plan: w.plan,
            first_relation: i,
            relation_count: 1,
            payload_bytes: payload.len() as u32,
        },
        payload,
        limits().max_part_bytes,
    )
    .unwrap()
    .encode(limits().max_part_bytes)
    .unwrap()
}
pub fn panel(t: &Trusted, r: MinerResultCommit, alternate: bool, prompt: u32, forged_eval: bool) -> Packet {
    if !forged_eval {
        return panel_a3(t, r, alternate, prompt);
    }
    // The malicious candidate is assembled independently from A3, so A3 and
    // A1 cannot share a generator bug that silently makes this attack honest.
    let p = program();
    let plan = TirPlan::compile(&p).unwrap();
    let honest_map = map(alternate);
    let honest_params = TirParams::from_map(&plan, &honest_map).unwrap();
    let source = TirParamsSourceV1(&honest_params);
    let inventory = TirHeldInventoryV1::build(&p, &source).unwrap();
    let eval_map = map(if forged_eval { !alternate } else { alternate });
    let params = TirParams::from_map(&plan, &eval_map).unwrap();
    let space = PalwTirStepSpaceV1::new(&t.class).unwrap();
    let runner = TirClassRunnerV1::new(&space, &plan, &params, t.class.class_id(&t.artifact)).unwrap();
    let mut leaves = Vec::new();
    let run = runner.run(&t.job, &[prompt], limits().max_leaves, true, &mut |l| leaves.push(l.preimage.clone())).unwrap();
    let binding = PalwTirStepBindingV1 {
        version: 1,
        job_context: t.job.clone(),
        class: t.class.clone(),
        artifact_root: t.artifact,
        full_logits_trace_root: run.trace_root,
        step_leaf_count: run.leaf_count,
        step_merkle_root: run.step_merkle_root,
        committed_execution_root: run.execution_root,
    };
    let plan_id = coverage_plan_id(&r, CheckProfile::ExactStreamV1, tir::exact_derivation_id(), binding.step_leaf_count).unwrap();
    let author = principal(sign(61, Digest([0; 64]), WITNESS_SIGN_CONTEXT).0.as_slice().try_into().unwrap());
    let witness = PanelWitnessCommit {
        version: 2,
        revision: 1,
        scope: r.scope,
        case: r.case_id(),
        result: r.id(),
        author,
        attempt: 0,
        profile: CheckProfile::ExactStreamV1,
        plan: plan_id,
        execution_record: d(binding.committed_execution_root),
        provenance: tir::provenance_id(&r),
        candidate_output: d(palw_attempt_output_root_v1(&t.job, &run.generated)),
    };
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: limits().max_leaves,
        prompt_form: PalwPromptIdsFormV1::Flat,
        limits: DemandLimits { max_elements: limits().max_elements_per_leaf, max_terms: limits().max_terms_per_leaf },
    };
    let prompt_ids = [prompt];
    let store = TirEvidenceV1::dense(
        &runner,
        &binding,
        &prompt_ids,
        &run.logits_rows,
        &run.generated,
        &leaves,
        &inventory,
        PalwPromptIdsFormV1::Flat,
        limits().max_leaves,
    )
    .unwrap();
    let frames = (0..binding.step_leaf_count)
        .map(|i| {
            let cone = store.cone_refutation(i, &rules).unwrap();
            let leaf = space.leaf_at(&t.job, i).unwrap();
            let trace = match leaf.kind {
                PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
                    if occurrence as usize == space.occurrences().len() - 1 && node == p.logits =>
                {
                    Some(
                        tir_logits_event_disclosure_v1(
                            &binding,
                            &run.logits_rows,
                            &run.generated,
                            leaf.position,
                            (first_element / 4096) as u8,
                        )
                        .unwrap(),
                    )
                }
                _ => None,
            };
            let cone = borsh::to_vec(&cone).unwrap();
            let trace = trace.map(|x| borsh::to_vec(&x).unwrap());
            let payload = tir::ExactPart::new(&cone, trace.as_deref(), limits().max_part_bytes - 284).unwrap().encode();
            frame(&r, &witness, i, &payload)
        })
        .collect();
    let mut packet = Packet {
        result: r,
        witness,
        binding,
        generated: run.generated,
        miner_key: vec![],
        miner_sig: vec![],
        panel_key: vec![],
        panel_sig: vec![],
        frames,
        frame_signatures: vec![],
    };
    packet.resign_and_reframe();
    packet
}

fn panel_a3(t: &Trusted, r: MinerResultCommit, alternate: bool, prompt: u32) -> Packet {
    use misaka_palw_positive_contract::ports::PanelBuilder;
    use misaka_palw_tir_exec::node::positive_panel::{PanelAdmission, PanelLimits, TirPanelBuilder};
    let p = program();
    let plan = TirPlan::compile(&p).unwrap();
    let m = map(alternate);
    let params = TirParams::from_map(&plan, &m).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&p, &source).unwrap();
    let space = PalwTirStepSpaceV1::new(&t.class).unwrap();
    let runner = TirClassRunnerV1::new(&space, &plan, &params, t.class.class_id(&t.artifact)).unwrap();
    let (miner_key, miner_sig) = sign(51, r.id().0, RESULT_SIGN_CONTEXT);
    let admission = PanelAdmission::authenticate(
        &r,
        t.admission(),
        &t.class,
        &t.job,
        &[prompt],
        PalwPromptIdsFormV1::Flat,
        miner_key.as_slice().try_into().unwrap(),
        miner_sig.as_slice().try_into().unwrap(),
        PanelLimits {
            check: limits(),
            max_frame_bytes: limits().max_part_bytes,
            max_total_frame_bytes: limits().max_total_bytes,
            max_capture_bytes: 16 << 20,
        },
    )
    .unwrap();
    let builder = TirPanelBuilder::new(&runner, &inventory);
    let capture = builder.replay_and_capture(&admission).unwrap();
    let descriptor = admission.plan();
    let panel_key = sign(61, Digest([0; 64]), WITNESS_SIGN_CONTEXT).0;
    let author = principal(panel_key.as_slice().try_into().unwrap());
    let witness = builder.commit(&capture, &descriptor, author, 0).unwrap();
    let prepared = builder.prepare_parts(&capture).unwrap();
    let frames = (0..descriptor.mandatory_relations)
        .map(|next_relation| {
            let part = prepared.build_part(CoverageCursor { plan: descriptor.plan, next_relation }).unwrap();
            part.frame(&witness, &descriptor).unwrap().encode(descriptor.max_part_bytes).unwrap()
        })
        .collect();
    let panel_sig = sign(61, witness.id().0, WITNESS_SIGN_CONTEXT).1;
    let mut packet = Packet {
        result: r,
        witness,
        binding: capture.binding().clone(),
        generated: capture.generated().to_vec(),
        miner_key,
        miner_sig,
        panel_key,
        panel_sig,
        frames,
        frame_signatures: vec![],
    };
    // Sign A3's exact bytes as-is. Rebuilding a frame or rewriting the witness
    // here could hide an A3 interchange bug from the independent receiver.
    packet.frame_signatures = vec![vec![]; packet.frames.len()];
    for i in 0..packet.frames.len() {
        packet.resign_frame(i);
    }
    packet
}
pub fn fixture(alternate: bool, prompt: u32, job: u64) -> (Trusted, Packet) {
    let t = registration(alternate, prompt, job);
    let r = result(&t, alternate, prompt);
    let p = panel(&t, r, alternate, prompt, false);
    (t, p)
}
pub fn submit(p: &Packet, t: &Trusted) -> Result<misaka_palw_tir_exec::node::positive_contract::CheckedTerminal, AdapterError> {
    let ctx = p.authenticate(t)?;
    let mut progress = ctx.start()?;
    let checker = misaka_palw_tir_exec::node::positive_contract::TirNodeChecker;
    for bytes in &p.frames {
        checker.check_authenticated_part(&ctx, &mut progress, p.authenticated(bytes)?)?;
    }
    checker.finish(&ctx, progress)
}
