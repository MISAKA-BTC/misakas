//! A7's fixture and scalar oracle. Expected arithmetic never calls a TIR evaluator.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_positive_court_v2::*,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_court_v1::PalwTirCourtRulesV1,
    palw_tir_step_v1::PalwTirStepSpaceV1,
};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, demand::DemandLimits};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirClassRunnerV1,
        inventory::{TirHeldInventoryV1, TirParamsSourceV1},
        positive_court::*,
    },
};

pub const ROWS: [[i128; 4]; 4] = [[4, 2, -3, 1], [-1, 7, 3, 2], [6, 6, 1, -2], [-8, -2, -3, -1]];
pub fn scales(wide: bool, ties: bool) -> [i128; 4] {
    if ties {
        [1; 4]
    } else if wide {
        [1 << 34, -(1 << 33), 1 << 32, 1 << 31]
    } else {
        [3, -2, 5, 1]
    }
}
pub fn oracle(prompt: u32, wide: bool, ties: bool) -> (Vec<u32>, Vec<[i128; 4]>) {
    let mut token = prompt as usize;
    let mut ids = Vec::new();
    let mut rows = Vec::new();
    for _ in 0..3 {
        let row = std::array::from_fn(|i| ROWS[token][i] * scales(wide, ties)[i]);
        // Strict comparison specifies the first-lane tie break independently.
        token = (1..4).fold(0, |best, i| if row[i] > row[best] { i } else { best });
        ids.push(token as u32);
        rows.push(row);
    }
    (ids, rows)
}
pub fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
pub fn policy() -> ExactPolicyV2 {
    ExactPolicyV2 {
        max_leaves: 4096,
        max_part_bytes: 1 << 20,
        max_part_heap: 8 << 20,
        max_total_bytes: 64 << 20,
        max_elements_per_leaf: 1 << 18,
        max_terms_per_leaf: 1 << 18,
        max_total_elements: 1 << 32,
        max_total_terms: 1 << 32,
        max_commit_bytes: 1 << 20,
        deadline: 100,
    }
}
pub fn limits() -> RoleLimitsV2 {
    RoleLimitsV2 { max_prefill: 4, max_decode: 4, max_leaves: 4096, max_capture_bytes: 16 << 20 }
}
pub fn program(wide: bool, ties: bool) -> misaka_palw_tir::TirProgramV1 {
    let mut b = ProgramBuilder::new(4, 1 << 18);
    let emb = b.param("a7-embedding", DType::I8, &[4, 4], false);
    let scale = b.konst(if wide { DType::I64 } else { DType::I32 }, &[4], &scales(wide, ties));
    let mut pre = b.block("pre", vec![]);
    let x = pre.gather(emb, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[x]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let x = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(x);
    let x = if wide { post.cast(x, DType::I64) } else { x };
    let y = post.mul(x, scale, if wide { DType::I64 } else { DType::I32 });
    post.commit(y);
    let Ref::Node(logits) = y else { unreachable!() };
    let post = post.finish(&[]);
    let mut p = b.finish(pre, vec![], post, logits);
    p.logits_scheme_id = if wide {
        misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2
    } else {
        kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_bytes()
    };
    p
}
pub fn params(poison: bool) -> MapParams {
    let mut values: Vec<_> = ROWS.into_iter().flatten().collect();
    if poison {
        values[4] = 99;
    }
    let mut p = MapParams::default();
    p.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 4], values).unwrap());
    p
}
pub fn setup(wide: bool, ties: bool, prompt: u32) -> (PalwTirClassV1, MinerResultV2) {
    let p = program(wide, ties);
    let plan = TirPlan::compile(&p).unwrap();
    let map = params(false);
    let params = TirParams::from_map(&plan, &map).unwrap();
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
    let class_id = class.class_id(&artifact);
    let mut job = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class_id, (1, 3)).unwrap();
    job.tokenizer_id = class.tokenizer_id;
    job.prompt_token_ids_hash =
        kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &[prompt]).unwrap();
    (
        class,
        MinerResultV2 {
            version: 2,
            network: h(1),
            branch: h(2),
            ruleset: h(3),
            claim: h(4),
            job,
            artifact,
            output: h(0),
            sampling: kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY,
            service_policy: h(5),
            dependencies: h(6),
        },
    )
}
pub fn sign(seed: u8, id: Hash64, context: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let k = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32]);
    let s = libcrux_ml_dsa::ml_dsa_87::sign(&k.signing_key, id.as_byte_slice(), context, [seed + 1; 32]).unwrap();
    (k.verification_key.as_ref().to_vec(), s.as_ref().to_vec())
}
#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct Packet {
    pub class: PalwTirClassV1,
    pub result: MinerResultV2,
    pub witness: PanelWitnessV2,
    pub miner_key: Vec<u8>,
    pub miner_sig: Vec<u8>,
    pub panel_key: Vec<u8>,
    pub panel_sig: Vec<u8>,
    pub policy: ExactPolicyV2,
    pub parts: Vec<(Vec<u8>, Option<Vec<u8>>)>,
}
impl Packet {
    pub fn scope(&self) -> AdmissionScopeV2 {
        AdmissionScopeV2 {
            network: h(1),
            branch: h(2),
            ruleset: h(3),
            required_form: gate::ClaimFormV2::MinerResultV2,
            registered_class_id: self.class.class_id(&self.result.artifact),
        }
    }
    pub fn resign(&mut self) {
        self.witness.result = self.result.id();
        (self.miner_key, self.miner_sig) = sign(51, self.result.id(), RESULT_CONTEXT);
        (self.panel_key, self.panel_sig) = sign(61, self.witness.id(), WITNESS_CONTEXT);
    }
    pub fn admit(&self) -> Result<ExactCaseV2, ErrorV2> {
        ExactCaseV2::admit(
            self.result.clone(),
            &SignatureV2(self.miner_sig.clone()),
            &self.miner_key,
            self.witness.clone(),
            &SignatureV2(self.panel_sig.clone()),
            &self.panel_key,
            &self.class,
            self.policy,
            PalwPromptIdsFormV1::Flat,
            1,
            self.scope(),
        )
    }
    pub fn submit_all(&self, case: &mut ExactCaseV2) -> Result<(), ErrorV2> {
        for (c, t) in &self.parts {
            case.submit_encoded(c, t.as_deref(), 2)?;
        }
        Ok(())
    }
}
pub fn miner(wide: bool, ties: bool, prompt: u32) -> MinerResultV2 {
    let (class, result) = setup(wide, ties, prompt);
    let plan = TirPlan::compile(&program(wide, ties)).unwrap();
    let map = params(false);
    let params = TirParams::from_map(&plan, &map).unwrap();
    let (r, ids) = miner_result_v2(result, &class, &plan, &params, &[prompt], PalwPromptIdsFormV1::Flat, limits()).unwrap();
    assert_eq!(ids, oracle(prompt, wide, ties).0);
    r
}
pub fn panel(wide: bool, ties: bool, prompt: u32, result: MinerResultV2, poison: bool) -> Packet {
    let (class, _) = setup(wide, ties, prompt);
    let plan = TirPlan::compile(&program(wide, ties)).unwrap();
    let honest_map = params(false);
    let honest_params = TirParams::from_map(&plan, &honest_map).unwrap();
    let honest_source = TirParamsSourceV1(&honest_params);
    let inventory = TirHeldInventoryV1::build(&plan.program, &honest_source).unwrap();
    let map = params(poison);
    let params = TirParams::from_map(&plan, &map).unwrap();
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let runner = if wide {
        TirClassRunnerV1::new_wide_q24_v2(&space, &plan, &params, result.job.shape_profile_id)
    } else {
        TirClassRunnerV1::new(&space, &plan, &params, result.job.shape_profile_id)
    }
    .unwrap();
    let capture =
        PanelCaptureV2::replay(&runner, &class, result.artifact, &result.job, &[prompt], PalwPromptIdsFormV1::Flat, limits()).unwrap();
    let policy = policy();
    let witness = capture.witness(&result, policy, 0).unwrap();
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: policy.max_leaves,
        prompt_form: PalwPromptIdsFormV1::Flat,
        limits: DemandLimits { max_elements: policy.max_elements_per_leaf, max_terms: policy.max_terms_per_leaf },
    };
    let mut parts = Vec::new();
    capture
        .stream_parts(&runner, &inventory, &rules, policy.max_part_bytes, &mut |_, part| {
            parts.push((borsh::to_vec(&part.cone).unwrap(), part.trace.map(|t| borsh::to_vec(&t).unwrap())));
            Ok(())
        })
        .unwrap();
    let mut packet =
        Packet { class, result, witness, policy, parts, miner_key: vec![], miner_sig: vec![], panel_key: vec![], panel_sig: vec![] };
    packet.resign();
    packet
}
pub fn fixture(wide: bool) -> Packet {
    panel(wide, false, 1, miner(wide, false, 1), false)
}
