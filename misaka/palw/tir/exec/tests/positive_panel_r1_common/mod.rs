#![allow(dead_code)]
#[path = "../node_common/mod.rs"]
mod node_common;

use kaspa_consensus_core::{
    Hash64, palw_attempt_rules_v1::palw_attempt_output_root_v1, palw_decode_select_v2::PalwDecodeSamplingV2,
    palw_prompt_ids_v1::PalwPromptIdsFormV1, palw_tir_class_v1::PalwTirClassV1, palw_tir_step_v1::PalwTirStepSpaceV1,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION, identity::*, messages::*, ports::require_next, tir, wire::EvidenceFrame,
};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirClassRunnerV1, TirHeldInventoryV1, TirParamsSourceV1,
        positive_check::{CheckLimits, CoveragePlan, ExactChecker},
        positive_panel::{PanelAdmission, PanelLimits},
    },
};

pub const PROMPT: &[u32] = &[0];
// Row 0 [1,9,3,2] selects 1. Row 1 [5,5,1,-2] selects 0 by the
// lowest-index tie rule. Row 0 then selects 1. No executor computes this oracle.
pub const EXPECTED: &[u32] = &[1, 0, 1];
pub const FRAME_CAP: u64 = 1 << 20;
pub fn d(n: u8) -> Digest {
    Digest([n; 64])
}
pub fn digest(hash: Hash64) -> Digest {
    Digest(hash.as_bytes())
}
pub fn check_limits() -> CheckLimits {
    CheckLimits {
        max_leaves: 1 << 12,
        max_part_bytes: tir::raw_part_budget(FRAME_CAP).unwrap(),
        max_part_heap: 1 << 24,
        max_total_bytes: 1 << 36,
        max_elements_per_leaf: 1 << 16,
        max_terms_per_leaf: 1 << 18,
        max_total_elements: 1 << 32,
        max_total_terms: 1 << 36,
    }
}
pub fn panel_limits() -> PanelLimits {
    PanelLimits { check: check_limits(), max_frame_bytes: FRAME_CAP, max_total_frame_bytes: 1 << 36, max_capture_bytes: 1 << 27 }
}

pub struct Fixture {
    pub class: PalwTirClassV1,
    pub job: PalwJobContextV2,
    pub artifact: Hash64,
    pub result: MinerResultCommit,
    pub rooted: AdmissionContext,
    pub key: [u8; PUBKEY_BYTES],
    pub signature: [u8; SIGNATURE_BYTES],
}
impl Fixture {
    pub fn admit(&self) -> Result<PanelAdmission, String> {
        PanelAdmission::authenticate(
            &self.result,
            self.rooted,
            &self.class,
            &self.job,
            PROMPT,
            PalwPromptIdsFormV1::Flat,
            &self.key,
            &self.signature,
            panel_limits(),
        )
    }
    pub fn checker(&self, binding: kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1, generated: Vec<u32>) -> ExactChecker {
        let plan = CoveragePlan::derive(&self.class, self.artifact, &self.job, PalwPromptIdsFormV1::Flat, check_limits()).unwrap();
        ExactChecker::begin_greedy(plan, binding, generated, Hash64::from_bytes(self.result.output.0), PalwDecodeSamplingV2::GREEDY)
            .unwrap()
    }
}

pub fn with_fixture(wrong_result: bool, body: impl FnOnce(&Fixture, &TirClassRunnerV1<'_>, &TirHeldInventoryV1<'_>)) {
    let mut builder = ProgramBuilder::new(4, 1 << 18);
    let embedding = builder.param("embedding", DType::I8, &[4, 4], false);
    let mut pre = builder.block("pre", vec![]);
    let x = pre.gather(embedding, Ref::Input(0), 0, 0);
    pre.commit(x);
    let pre = pre.finish(&[x]);
    let mut post = builder.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let logits = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(logits);
    let Ref::Node(logits_node) = logits else { unreachable!() };
    let post = post.finish(&[]);
    // Commit flags are part of this NEW program before registration.
    let program = node_common::tiled(builder.finish(pre, vec![], post, logits_node));
    assert!(program.blocks.iter().flat_map(|b| &b.nodes).all(|n| n.commit));
    let mut map = MapParams::default();
    map.tensors
        .insert((0, None), Tensor::new(DType::I8, vec![4, 4], vec![1, 9, 3, 2, 5, 5, 1, -2, -3, -2, -1, 0, 0, 7, 10, 2]).unwrap());
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let artifact = inventory.tree.root();
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        layout: node_common::layout(&program, 3, 2, 2, 16),
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let registered_class = class.class_id(&artifact);
    let mut job = node_common::job(&program, 1, 3, registered_class, PROMPT, PalwPromptIdsFormV1::Flat);
    job.job_id = Hash64::from_u64_word(4);
    job.cu_ruleset_id = Hash64::from_u64_word(9);
    let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([61; 32]);
    let key: [u8; PUBKEY_BYTES] = *pair.verification_key.as_ref();
    let rooted = AdmissionContext {
        scope: ChainScope { network: d(1), branch: d(2), ruleset: d(3) },
        expected_job: digest(job.job_id),
        expected_execution: digest(job.context_hash()),
        expected_miner: principal(&key),
        expected_input: digest(job.prompt_token_ids_hash),
        registered_class: digest(registered_class),
        artifact: digest(artifact),
        program: digest(class.graph_ir_root()),
        // The actual job context retains its tokenizer marker independently
        // from the nonzero registration tokenizer pinned inside the class ID.
        tokenizer: digest(job.tokenizer_id),
        execution_rules: digest(job.cu_ruleset_id),
        selector: tir::greedy_selector_id(),
        required_form: ClaimForm::PositiveResultV2,
    };
    let result = MinerResultCommit {
        version: CONTRACT_VERSION,
        revision: CONTRACT_REVISION,
        scope: rooted.scope,
        job: rooted.expected_job,
        execution: rooted.expected_execution,
        miner: rooted.expected_miner,
        registered_class: rooted.registered_class,
        artifact: rooted.artifact,
        program: rooted.program,
        input: rooted.expected_input,
        tokenizer: rooted.tokenizer,
        execution_rules: rooted.execution_rules,
        selector: rooted.selector,
        generation_seed: job.execution_seed,
        temperature_q24: 0,
        prefill: 1,
        decode: 3,
        max_context: job.max_context_tokens,
        output: digest(palw_attempt_output_root_v1(&job, if wrong_result { &[1, 1, 1] } else { EXPECTED })),
        dependencies: d(13),
        service_contract: d(14),
    };
    let signature =
        *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    let fixture = Fixture { class, job, artifact, result, rooted, key, signature };
    let space = PalwTirStepSpaceV1::new(&fixture.class).unwrap();
    let runner = TirClassRunnerV1::new(&space, &plan, &params, registered_class).unwrap();
    body(&fixture, &runner, &inventory);
}

pub fn witness_author() -> PrincipalId {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([62; 32]);
    principal(key.verification_key.as_ref())
}
pub fn authenticate_witness(witness: &PanelWitnessCommit, result: &MinerResultCommit) {
    let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([62; 32]);
    let signature =
        *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &witness.id().0.0, WITNESS_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    witness.authenticate(result, pair.verification_key.as_ref(), &signature).unwrap();
}
pub fn check_frame(
    checker: &mut ExactChecker,
    descriptor: &CoverageDescriptor,
    witness: &PanelWitnessCommit,
    cursor: &mut CoverageCursor,
    bytes: &[u8],
) -> Result<(), String> {
    let frame = EvidenceFrame::decode(bytes, descriptor.max_part_bytes).map_err(|e| format!("frame:{e:?}"))?;
    let next = require_next(descriptor, *cursor, witness, &frame).map_err(|e| format!("scope:{e:?}"))?;
    let part = tir::decode_frame(&frame, descriptor.max_part_bytes).map_err(|e| format!("part:{e:?}"))?;
    checker.submit_encoded(part.cone(), part.trace()).map_err(|e| format!("check:{e:?}"))?;
    cursor.next_relation = next;
    Ok(())
}
