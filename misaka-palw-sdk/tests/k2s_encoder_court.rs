//! **Encoders and heads under K2-TIR-v5's element courts** (lane K2S, `docs/design/palw/k2-real-scale.md` §12; the finding is HFX m3:
//! with testnet-12's ceilings the generative route admits no encoder or head class).
//!
//! A bidirectional encoder is lowered as ONE position over a padded token axis (`lower::bidir`): every value is a `[L, …]` tensor of all
//! rows at once. The generative route judges that position as one unit (a Panel replays it, a close walks its cone), so the whole
//! forward pass is the unit of every bound. Under K2-TIR-v5 the unit of a court is one ELEMENT of one committed value, opened through
//! the tiled dual-root commitments, and the ids are the job's input (`misaka-palw-kernel::seg_encoder`).
//!
//! **Bounds that depend on geometry only** (the first two tests): real configurations (`config.json` values as published; no weight is
//! read — a shape is the configuration's) lowered by `lower::bidir`, lifted to the version-2 program (`encoder::bidir_v2`) and judged
//! as its version-1 view (the ids and the count trail the params, the v5 binding) under the v5 plan, `check_plan_v1`, the
//! per-prosecution gate and the node's carrier; then each axis pushed to the largest value the ceilings still admit.
//!
//! **K2-TIR-v5 at the ledger level** (the third test): a real tiny BERT encoder and a real tiny XLM-R reranker head (the lowering's HF
//! fixtures, WITH their weights) registered under OPV; every honest element of every value filed and dismissed, each filing within its
//! priced bound; lies in the embedding lookup (which reads the job's ids through the prompt tile), a projection, the key mask (which
//! reads the job's count), the attention and the result each localized to one element and convicted from public material.
//!
//! Run: `cargo test --offline -p misaka-palw-sdk --test k2s_encoder_court -- --nocapture --test-threads=1`

use misaka_palw_kernel::descriptor::{KernelScheduleV1, KernelStatusV1, k2_tir_v4_descriptor};
use misaka_palw_kernel::gate::public_prosecution_complete_v4;
use misaka_palw_kernel::ledger::carrier_fit_v1;
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::public::{ProfileMaterialV1, program_root_v1};
use misaka_palw_tir::Prim;
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir_lower::lower::bidir::{self, BidirCfg, Pooling};

/// `BAAI/bge-base-en-v1.5` (`BertModel`).
const BGE_BASE: &str = r#"{"architectures":["BertModel"],"attention_probs_dropout_prob":0.1,"hidden_act":"gelu","hidden_dropout_prob":0.1,
"hidden_size":768,"initializer_range":0.02,"intermediate_size":3072,"layer_norm_eps":1e-12,"max_position_embeddings":512,
"model_type":"bert","num_attention_heads":12,"num_hidden_layers":12,"pad_token_id":0,"position_embedding_type":"absolute",
"type_vocab_size":2,"use_cache":true,"vocab_size":30522}"#;

/// `BAAI/bge-large-en-v1.5` (`BertModel`).
const BGE_LARGE: &str = r#"{"architectures":["BertModel"],"attention_probs_dropout_prob":0.1,"hidden_act":"gelu","hidden_dropout_prob":0.1,
"hidden_size":1024,"initializer_range":0.02,"intermediate_size":4096,"layer_norm_eps":1e-12,"max_position_embeddings":512,
"model_type":"bert","num_attention_heads":16,"num_hidden_layers":24,"pad_token_id":0,"position_embedding_type":"absolute",
"type_vocab_size":2,"use_cache":true,"vocab_size":30522}"#;

/// A BERT-base extractive QA checkpoint (`deepset/bert-base-cased-squad2`: `BertForQuestionAnswering`, cased vocabulary).
const BERT_BASE_QA: &str = r#"{"architectures":["BertForQuestionAnswering"],"attention_probs_dropout_prob":0.1,"hidden_act":"gelu",
"hidden_dropout_prob":0.1,"hidden_size":768,"initializer_range":0.02,"intermediate_size":3072,"layer_norm_eps":1e-12,
"max_position_embeddings":512,"model_type":"bert","num_attention_heads":12,"num_hidden_layers":12,"pad_token_id":0,
"position_embedding_type":"absolute","type_vocab_size":2,"vocab_size":28996}"#;

/// `BAAI/bge-reranker-large` (`XLMRobertaForSequenceClassification`, one label).
const BGE_RERANKER_LARGE: &str = r#"{"architectures":["XLMRobertaForSequenceClassification"],"attention_probs_dropout_prob":0.1,
"bos_token_id":0,"classifier_dropout":null,"eos_token_id":2,"hidden_act":"gelu","hidden_dropout_prob":0.1,"hidden_size":1024,
"id2label":{"0":"LABEL_0"},"initializer_range":0.02,"intermediate_size":4096,"label2id":{"LABEL_0":0},"layer_norm_eps":1e-05,
"max_position_embeddings":514,"model_type":"xlm-roberta","num_attention_heads":16,"num_hidden_layers":24,"pad_token_id":1,
"position_embedding_type":"absolute","type_vocab_size":1,"use_cache":true,"vocab_size":250002}"#;

/// `BAAI/bge-m3` (`XLMRobertaModel`, 8,192 tokens).
const BGE_M3: &str = r#"{"architectures":["XLMRobertaModel"],"attention_probs_dropout_prob":0.1,"bos_token_id":0,"eos_token_id":2,
"hidden_act":"gelu","hidden_dropout_prob":0.1,"hidden_size":1024,"initializer_range":0.02,"intermediate_size":4096,"layer_norm_eps":1e-05,
"max_position_embeddings":8194,"model_type":"xlm-roberta","num_attention_heads":16,"num_hidden_layers":24,"output_past":true,
"pad_token_id":1,"position_embedding_type":"absolute","type_vocab_size":1,"use_cache":true,"vocab_size":250002}"#;

/// A BERT encoder of a chosen geometry (`head_dim` 64), for the ceiling sweeps.
fn bert(d: u64, ff: u64, layers: u64, vocab: u64, max_pos: u64) -> String {
    format!(
        r#"{{"architectures":["BertModel"],"attention_probs_dropout_prob":0.1,"hidden_act":"gelu","hidden_dropout_prob":0.1,
"hidden_size":{d},"initializer_range":0.02,"intermediate_size":{ff},"layer_norm_eps":1e-12,"max_position_embeddings":{max_pos},
"model_type":"bert","num_attention_heads":{},"num_hidden_layers":{layers},"pad_token_id":0,"position_embedding_type":"absolute",
"type_vocab_size":2,"use_cache":true,"vocab_size":{vocab}}}"#,
        d / 64
    )
}

/// The node's carrier (`PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1`).
const CARRIER: usize = 100_000 * 16 - (16 << 10);

/// The interim route policy's prosecution ceilings (`palw_kernel_route_policy_v1`).
const ROUTE: misaka_palw_kernel::gate::ProsecutionPolicyV1 = misaka_palw_kernel::gate::ProsecutionPolicyV1 {
    court_deadline_daa: 20,
    max_sessions_per_claim: 1 << 10,
    max_public_bytes: 1 << 40,
    max_verifier_ram: 1 << 36,
    max_retained_state: 1 << 32,
};

/// Multiply-accumulates of the program's one position (a decoder's: of one position): every `MatMul` node's output elements times its
/// contracted extent — what re-executing the position costs in products.
fn position_macs(p: &TirProgramV1, h: usize) -> u128 {
    let mut macs = 0u128;
    for (b, _) in p.occurrences() {
        let block = &p.blocks[b as usize];
        for node in &block.nodes {
            if !matches!(node.prim, Prim::MatMul) {
                continue;
            }
            let elements: u128 = node.out.resolve(h).iter().map(|d| *d as u128).product();
            let k = match node.inputs.first() {
                Some(misaka_palw_tir::Ref::Node(i)) => block.nodes[*i as usize].out.resolve(h).last().copied(),
                Some(misaka_palw_tir::Ref::CarryIn(i)) => block.carry_in[*i as usize].resolve(h).last().copied(),
                Some(misaka_palw_tir::Ref::Param(j)) => p.params[*j as usize].shape.last().map(|d| *d as usize),
                _ => None,
            }
            .unwrap_or(1);
            macs += elements * k as u128;
        }
    }
    macs
}

/// **An encoder class's program from its configuration alone**: `lower_bidir` (shape-only: no weight is read; the scales the
/// calibration sets do not change a shape), lifted to the version-2 program and taken as its version-1 view, whose last two params are
/// the job's ids and count (checked: the v5 binding).
fn encoder_view(config: &str, lmax: u32, pooling: Pooling) -> Result<TirProgramV1, String> {
    let json: serde_json::Value = serde_json::from_str(config).map_err(|e| e.to_string())?;
    let spec = misaka_palw_tir_lower::hf_schema::read_model(&json, None, &misaka_palw_tir_lower::hf_schema::ReadOptions::default())
        .map_err(|f| format!("not read: {}", f.error))?
        .spec;
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).map_err(|e| format!("HL: {e}"))?;
    let lw = bidir::lower_bidir(&hl, &spec, &BidirCfg { lmax, pooling, normalize: false }).map_err(|e| format!("not lowered: {e}"))?;
    let p2 = misaka_palw_tir_lower::encoder::bidir_v2(&lw, spec.vocab_size as u32, lmax).map_err(|e| format!("version 2: {e}"))?;
    let view = p2.v1_view();
    misaka_palw_kernel::seg_encoder::encoder_binding_v1(&view).map_err(|e| format!("not a v5 encoder: {e}"))?;
    Ok(view)
}

/// Everything §12 reports about one class at one geometry.
struct Judged {
    relations: usize,
    eps_bits: u32,
    macs: u128,
    values: u64,
    material: u128,
    parts: u32,
    artifact: u128,
    public: u128,
    ram: u128,
    court_bytes: u64,
    court_work: u64,
    court_at: String,
    filing: u64,
    retained: u128,
}

/// **Plan, check, gate and fit one program** under `descriptor` at `positions` (`Err`: the first refusal, by name).
fn judge(
    descriptor: &misaka_palw_kernel::descriptor::KernelDescriptorV1,
    program: &TirProgramV1,
    positions: u32,
) -> Result<Judged, String> {
    let root = program_root_v1(&program.encode());
    let plan = plan_for_tir_program_v1(descriptor, program, root, positions)
        .map_err(|(family, why)| format!("KERNEL_EXTENSION_REQUIRED ({}: {why})", family.name()))?;
    let armed = KernelScheduleV1::default().with(descriptor.digest(), KernelStatusV1::Active { since_daa: 0 });
    // A K2-TIR-v5 class's ranges are proven with its inputs' intervals, as the ledger's registration does.
    let rule = if misaka_palw_kernel::descriptor::is_encoder_v1(descriptor) {
        let e = misaka_palw_kernel::seg_encoder::encoder_binding_v1(program)?;
        misaka_palw_kernel::seg_encoder::prove_encoder_ranges_v1(program, &e)?;
        misaka_palw_kernel::check::RangeRuleV1::ProvenByV2
    } else {
        misaka_palw_kernel::check::RangeRuleV1::TirV1
    };
    let accepted = misaka_palw_kernel::check::check_plan_with_v1(&armed, descriptor, program, root, &plan, 0, rule)
        .map_err(|o| format!("check_plan_v1 {}: {}", o.code(), o.to_string().chars().take(200).collect::<String>()))?;
    let (bounds, seg) = public_prosecution_complete_v4(descriptor, &plan, program, &ProfileMaterialV1::kernel_route(true), &ROUTE)
        .map_err(|g| format!("the gate refuses {g:?}"))?;
    carrier_fit_v1(&bounds, CARRIER, CARRIER, CARRIER).map_err(|e| format!("carrier: {e}"))?;
    // The court prices of a K2-TIR-v5 class read its job-bound inputs as the job's prompt tile (as the plan's own budgets do).
    let enc = if misaka_palw_kernel::descriptor::is_encoder_v1(descriptor) {
        misaka_palw_kernel::seg_encoder::encoder_binding_v1(program).ok()
    } else {
        None
    };
    let cost = |r: &misaka_palw_kernel::plan::PlanRelationV1| {
        misaka_palw_kernel::element::element_court_cost_in_v1(
            program,
            r.block as usize,
            r.node as usize,
            seg.node_count,
            positions,
            enc.as_ref(),
        )
    };
    let mut worst = plan
        .relations
        .iter()
        .map(|r| {
            let (b, w) = cost(r);
            (b, w, format!("block {} node {}, {}", r.block, r.node, r.family.name()))
        })
        .max_by_key(|x| (x.0, x.1))
        .expect("a relation");
    // The plan's worst court is the gate's; when it is above every element court it is the decode court's.
    if plan.budgets.worst_court_bytes > worst.0 {
        worst = (plan.budgets.worst_court_bytes, worst.1, "the decode court".to_string());
    }
    let worst_work = plan.budgets.worst_court_work;
    let h = positions.saturating_sub(1) as usize;
    Ok(Judged {
        relations: plan.relations.len(),
        eps_bits: accepted.error_bits as u32,
        macs: position_macs(program, h),
        values: seg.node_count,
        material: seg.position_material_bytes,
        parts: seg.parts_per_position,
        artifact: plan.budgets.artifact_bytes,
        public: bounds.max_public_bytes,
        ram: bounds.max_verifier_ram,
        court_bytes: worst.0,
        court_work: worst_work,
        court_at: worst.2,
        filing: bounds.max_filing_bytes,
        retained: bounds.max_retained_state,
    })
}

fn print(name: &str, j: &Judged) {
    println!(
        "[k2s-enc] {name}: PASS — {} relations, eps <= 2^-{}; MACs/position {}; values {}; position material {} B in {} parts; artifact {} B; \
         per prosecution: public {} B, verifier RAM {} B; worst element court {} B ({}), worst court work {}; filing {} B; \
         on chain {} B; carrier fit OK",
        j.relations,
        j.eps_bits,
        j.macs,
        j.values,
        j.material,
        j.parts,
        j.artifact,
        j.public,
        j.ram,
        j.court_bytes,
        j.court_at,
        j.court_work,
        j.filing,
        j.retained
    );
}

fn v5() -> misaka_palw_kernel::descriptor::KernelDescriptorV1 {
    misaka_palw_kernel::descriptor::k2_tir_v5_descriptor()
}

/// **The encoders HFX m3 names, BGE-M3 and Huihui-9B, from their configurations alone**: bge-base, bge-large, the QA checkpoint's
/// encoder and bge-reranker-large at 512 tokens pass K2-TIR-v5's plan, check, per-prosecution gate and the node's carrier, where the
/// generative route's position-sized bounds refused every one. BGE-M3 at its 8,192 tokens is refused by name (the reasons printed),
/// and at the widest axis v5 reads (4,096) it is judged. Huihui-9B at 8,192 positions is the decoder reference under K2-TIR-v4.
#[test]
fn k2s_v5_bounds_from_real_configurations() {
    for (name, config, pooling) in [
        ("bge-base-en-v1.5 @512", BGE_BASE, Pooling::Cls),
        ("bge-large-en-v1.5 @512", BGE_LARGE, Pooling::Cls),
        ("bge-reranker-large @512", BGE_RERANKER_LARGE, Pooling::Cls),
    ] {
        let program = encoder_view(config, 512, pooling).unwrap_or_else(|e| panic!("{name}: {e}"));
        let j = judge(&v5(), &program, 1).unwrap_or_else(|e| panic!("{name}: {e}"));
        print(name, &j);
        assert!(j.filing < CARRIER as u64 && j.retained < 8192);
    }
    // The QA checkpoint: its own architecture where this branch reads it (the span head is HFX's), else its encoder.
    let qa = encoder_view(BERT_BASE_QA, 512, Pooling::Cls).or_else(|e| {
        println!("[k2s-enc] bert-base-cased-squad2 @512: {e} — judged as its encoder");
        encoder_view(&BERT_BASE_QA.replace("BertForQuestionAnswering", "BertModel"), 512, Pooling::Cls)
    });
    let j = judge(&v5(), &qa.expect("the QA checkpoint's encoder"), 1).expect("bert-base-cased-squad2 (encoder) @512");
    print("bert-base-cased-squad2 (encoder) @512", &j);
    // BGE-M3: its own 8,192 tokens, then the widest axis K2-TIR-v5 reads.
    match encoder_view(BGE_M3, 8192, Pooling::Cls).and_then(|p| judge(&v5(), &p, 1)) {
        Ok(j) => print("bge-m3 @8192", &j),
        Err(e) => println!("[k2s-enc] bge-m3 @8192: REFUSED — {e}"),
    }
    match encoder_view(BGE_M3, 4096, Pooling::Cls).and_then(|p| judge(&v5(), &p, 1)) {
        Ok(j) => print("bge-m3 @4096", &j),
        Err(e) => println!("[k2s-enc] bge-m3 @4096: REFUSED — {e}"),
    }
    // Huihui-Qwen3.5-9B at 8,192 positions: the shipped fixture's program (lane D's), under K2-TIR-v4.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../consensus/src/pipeline/virtual_processor/tests/fixtures/g14/shipped/huihui-qwen3.5-9b-8k.json");
    if let Ok(text) = std::fs::read_to_string(&path) {
        let v: serde_json::Value = serde_json::from_str(&text).expect("the fixture");
        let hex = v["program_borsh_hex"].as_str().expect("the program");
        let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
        let program = TirProgramV1::decode_canonical(&bytes).expect("the program decodes");
        let j = judge(&k2_tir_v4_descriptor(), &program, 8192).expect("Huihui-9B @8192 under K2-TIR-v4");
        print("huihui-qwen3.5-9b @8192 (K2-TIR-v4)", &j);
    } else {
        println!("[k2s-enc] the Huihui fixture is not present");
    }
}

/// The largest value of an axis the ceilings admit: from `ok` (admitted), doubling to the first refusal or `cap`, then bisecting on
/// multiples of `step`. Returns the value, its judgement and what refused the next value up.
fn frontier(ok: u64, step: u64, cap: u64, try_at: &dyn Fn(u64) -> Result<Judged, String>) -> Result<(u64, Judged, String), String> {
    let mut ok = ok;
    let mut best = try_at(ok).map_err(|e| format!("the start {ok} is refused: {e}"))?;
    let mut refused: Option<(u64, String)> = None;
    while refused.is_none() && ok < cap {
        let next = (ok * 2).min(cap);
        match try_at(next) {
            Ok(j) => {
                ok = next;
                best = j;
            }
            Err(e) => refused = Some((next, e)),
        }
    }
    let Some((mut hi, mut why)) = refused else { return Ok((ok, best, format!("the axis's own cap {cap}"))) };
    while hi - ok > step {
        let mid = ok + ((hi - ok) / 2 / step).max(1) * step;
        match try_at(mid) {
            Ok(j) => {
                ok = mid;
                best = j;
            }
            Err(e) => {
                hi = mid;
                why = e;
            }
        }
    }
    Ok((ok, best, format!("{hi} refused: {}", why.chars().take(240).collect::<String>())))
}

/// **The worst geometry the ceilings admit.** From bge-large's geometry (one layer where the axis is per layer), each axis is pushed
/// to the largest value TIR's shape caps, K2-TIR-v5's one-tile input, the descriptor's court ceiling, the route's per-prosecution
/// ceilings and the node's carrier still admit. At every admitted geometry the worst filing fits the carrier — the gate refuses any
/// class whose priced court does not — so these are the worst court bytes, court work and retained bytes a v5 class can have.
#[test]
fn k2s_v5_the_worst_geometry_the_ceilings_admit() {
    let at = |d: u64, ff: u64, layers: u64, vocab: u64, l: u64| -> Result<Judged, String> {
        let program = encoder_view(&bert(d, ff, layers, vocab, l.max(512)), l as u32, Pooling::Cls)?;
        judge(&v5(), &program, 1)
    };
    let axes: [(&str, u64, u64, u64, &dyn Fn(u64) -> Result<Judged, String>); 6] = [
        ("tokens L (d 1024, ff 4096, 1 layer)", 512, 64, 4096, &|l| at(1024, 4096, 1, 30522, l)),
        ("hidden d (L 512, ff 4·d, 1 layer)", 1024, 64, 1 << 20, &|d| at(d, 4 * d, 1, 30522, 512)),
        ("FFN width ff (L 512, d 1024, 1 layer)", 4096, 64, 1 << 24, &|ff| at(1024, ff, 1, 30522, 512)),
        ("vocabulary (L 512, d 1024, 1 layer)", 30522, 1024, 1 << 24, &|v| at(1024, 4096, 1, v, 512)),
        ("layers at L 512 (bge-large)", 24, 1, 4096, &|n| at(1024, 4096, n, 30522, 512)),
        ("layers at L 4096 (bge-large)", 1, 1, 4096, &|n| at(1024, 4096, n, 30522, 4096)),
    ];
    for (axis, start, step, cap, f) in axes {
        let (max, j, refused) = match frontier(start, step, cap, f) {
            Ok(x) => x,
            Err(e) => {
                println!("[k2s-enc] frontier — {axis}: {e}");
                continue;
            }
        };
        println!("[k2s-enc] frontier — {axis}: largest admitted {max}; next: {refused}");
        print(&format!("  at {axis} = {max}"), &j);
        assert!(j.filing < CARRIER as u64 && j.court_bytes <= 1 << 24, "{axis}");
        assert!(j.retained < 8192, "{axis}: one segment root on chain");
    }
}

// ---- K2-TIR-v5 at the ledger level: real tiny encoders, lowered with their weights ---------------------------------------------------

use misaka_palw_kernel::descriptor::{KernelDescriptorV1, k2_tir_v1_descriptor, k2_tir_v2_descriptor, k2_tir_v5_descriptor};
use misaka_palw_kernel::element::{SegFaultV1, SegFindingV1, SegMaterialV1, check_positions_v1, prove_element_v1};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::{DecodeRuleV1, KernelClaimV1};
use misaka_palw_kernel::ledger::{
    AuthV1, KernelLedgerV1, KernelRouteObjectV1 as O, LedgerBlockV1, LedgerEventV1 as E, LedgerPolicyV1, LedgerTxV1, ProsecutionV1,
    claim_seal_v1, single_class_id_v1,
};
use misaka_palw_kernel::mode::VerificationModeV1;
use misaka_palw_kernel::opv::{CarrierCapsV1, OpvBudgetsV1, OpvEconomicsV1, OpvPolicyV1, OpvWindowV1};
use misaka_palw_kernel::plan::VerificationPlanV1;
use misaka_palw_kernel::seg::{
    PromptTileOpeningV1, SegmentedCommitmentsV1, SegmentedEvidenceV2, TiledJobV1, build_segmented_evidence_v1, prompt_root_of_ids_v1,
    seg_commitments_of_trace_v1,
};
use misaka_palw_kernel::seg_detect::check_claim_by_reexecution_v1;
use misaka_palw_kernel::seg_encoder::{EncoderBindingV1, encoder_binding_v1, trace_encoder_v1};
use misaka_palw_kernel::trace::{ParamCommitmentsV1, TraceV1};
use misaka_palw_tir::{MapParams, ParamSource, Tensor};

const PROD: Digest = [0xA1; 64];
const OUT: Digest = [0x0B; 64];
const ANY: Digest = [0x77; 64];
const OPV: VerificationModeV1 = VerificationModeV1::OptimisticPublicVerification;

fn ledger_policy() -> LedgerPolicyV1 {
    LedgerPolicyV1 {
        network_domain: [9; 64],
        ruleset_digest: [3; 64],
        challenge_policy_id: [5; 64],
        claim_collateral: 1000,
        demand_bond: 10,
        check_window_daa: 100,
        challenge_window_daa: 50,
        court_deadline_daa: 20,
        proof_grace_daa: 10,
        liability_daa: 200,
        exit_delay_daa: 30,
        dismissed_proof_fee: 5,
        accuser_reward_permille: 500,
        default_penalty: 100,
        claim_reward: 7,
        job_fee: 2,
        job_escrow_ttl_daa: 300,
        max_adjudications_per_block: 64,
        prosecution_reserve_permille: 500,
        max_court_work_per_block: u64::MAX,
        claim_seal_delay_daa: 1,
        seal_ttl_daa: 100,
        seal_deposit: 1,
        prosecution: ROUTE,
    }
}

/// An example OPV policy (tests only): a 50-DAA window, budgets that fit it, a 1,000 reservation a claim.
fn opv_policy() -> OpvPolicyV1 {
    OpvPolicyV1 {
        activation_daa: Some(0),
        window: OpvWindowV1 { base_challenge_window_daa: 40, verification_horizon_daa: 10 },
        budgets: OpvBudgetsV1 {
            cold_material_daa: 10,
            check_daa: 10,
            localize_daa: 2,
            disclose_daa: 8,
            court_daa: 3,
            carrier_daa: 2,
            reorg_slack_daa: 2,
        },
        economics: OpvEconomicsV1 {
            reservation_per_claim: 1000,
            work_credit_per_claim: 13,
            external_gain_bound: 80,
            assumed_detection_permille: 500,
            fresh_producer_slots: 2,
            admission_fee: 3,
            max_live_claims_per_producer: 3,
            max_live_claims_total: 5,
            default_burn_permille: 100,
        },
        carrier: CarrierCapsV1 { filing_cap: 1 << 26, response_cap: 1 << 27, commit_cap: 1 << 27 },
    }
}

/// A tiny encoder's K2-TIR-v5 class pieces: the version-1 view of its version-2 program (the ids and count trail the params), its
/// integer params in that order, and the template ids of its fixture.
struct Encoder {
    program: TirProgramV1,
    params: MapParams,
    binding: EncoderBindingV1,
    cls: u32,
    sep: u32,
    vocab: u32,
}

/// **Lower a tiny HF fixture with its weights**: calibrate the float reference on templated sequences, lower, materialise, lift the ids
/// and the count into the version-2 program's inputs (`encoder::bidir_v2`), and take its version-1 view.
fn tiny_encoder(fixture: &str, lmax: u32) -> Encoder {
    use misaka_palw_tir_lower::float_ref::{ParamStore, stream::Resident};
    use misaka_palw_tir_lower::lower::bidir::{Padded, float_forward};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures").join(fixture);
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).unwrap()).unwrap();
    let spec = misaka_palw_tir_lower::hf_schema::read_model(&json, None, &misaka_palw_tir_lower::hf_schema::ReadOptions::default())
        .unwrap_or_else(|f| panic!("{fixture}: {}", f.error))
        .spec;
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).expect("the HL program");
    let bind = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).expect("the binding");
    let ck = misaka_palw_tir_lower::weights::Checkpoint::open(&dir).expect("the checkpoint");
    let (params_f, _) = ParamStore::from_source(&hl, &bind, &ck).expect("the float params");
    // The fixture's own template: its first sequence's first id and its last real id, and its pad.
    let out: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).unwrap()).unwrap();
    let first = &out["sequences"][0];
    let padded: Vec<usize> = first["padded"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let count = first["count"].as_u64().unwrap() as usize;
    let (cls, sep, pad) = (padded[0], padded[count - 1], *padded.last().unwrap());
    let cfg = BidirCfg { lmax, pooling: Pooling::Cls, normalize: false };
    let mut stats = std::collections::BTreeMap::new();
    for (i, body) in
        misaka_palw_tir_lower::fidelity::random_sequences(spec.vocab_size, 8, lmax as usize - 2, 11).into_iter().enumerate()
    {
        let n = 1 + (i % (lmax as usize - 2));
        let mut ids: Vec<usize> = std::iter::once(cls).chain(body.into_iter().take(n)).chain(std::iter::once(sep)).collect();
        let count = ids.len().min(lmax as usize);
        ids.truncate(lmax as usize);
        ids.resize(lmax as usize, pad);
        float_forward(&hl, &spec, &cfg, &params_f, &Padded { ids, count }, Some(&mut stats)).expect("calibration");
    }
    let lw = bidir::lower_bidir(&hl, &spec, &cfg).expect("lower");
    let quiet = |_: usize, _: usize| {};
    let mat = misaka_palw_tir_lower::lower::materialise(
        &lw,
        &hl,
        &Resident(std::sync::Arc::new(params_f)),
        &stats,
        &misaka_palw_tir_lower::quant::QuantPolicy::default(),
        &quiet,
    )
    .expect("materialise");
    let vocab = spec.vocab_size as u32;
    let p2 = misaka_palw_tir_lower::encoder::bidir_v2(&lw, vocab, lmax).expect("the version-2 program");
    let ints = misaka_palw_tir_lower::encoder::lifted_params(&lw.program, &[bidir::IDS_PARAM, bidir::COUNT_PARAM], &mat.params);
    let program = p2.v1_view();
    let params = MapParams { tensors: ints.tensors.keys().map(|k| (*k, ints.param(k.0, k.1).expect("a param"))).collect() };
    let binding = encoder_binding_v1(&program).unwrap_or_else(|e| panic!("{fixture}: the version-1 view is an encoder program: {e}"));
    assert_eq!(binding.l, lmax);
    Encoder { program, params, binding, cls: cls as u32, sep: sep as u32, vocab }
}

fn obj(signer: Digest, object: O) -> LedgerTxV1 {
    LedgerTxV1::Object { auth: AuthV1 { signer_bond: signer }, object }
}

fn refusal(ev: &[E]) -> Option<String> {
    ev.iter().find_map(|e| if let E::Refused { why, .. } = e { Some(why.clone()) } else { None })
}

/// A ledger with the OPV policy and the encoder's K2-TIR-v5 class registered under OPV.
struct World {
    l: KernelLedgerV1,
    daa: u64,
    d: KernelDescriptorV1,
    enc: Encoder,
    class: Digest,
    /// The registered plan's worst court (what the gate holds against the carrier).
    plan_worst: u64,
}

impl World {
    fn new(enc: Encoder) -> World {
        let d = k2_tir_v5_descriptor();
        let schedule = KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 });
        let l = KernelLedgerV1::genesis(ledger_policy(), schedule, vec![k2_tir_v1_descriptor(), k2_tir_v2_descriptor(), d.clone()])
            .unwrap()
            .with_opv_policy(opv_policy())
            .unwrap();
        let bytes = enc.program.encode();
        let plan: VerificationPlanV1 = plan_for_tir_program_v1(&d, &enc.program, program_root_v1(&bytes), 1).expect("a v5 plan");
        let pc = ParamCommitmentsV1::of_v3(&enc.params);
        let class = single_class_id_v1(d.digest(), &bytes, &plan, &pc, OPV);
        let plan_worst = plan.budgets.worst_court_bytes;
        let mut w = World { l, daa: 0, d: d.clone(), enc, class, plan_worst };
        let ev = w.block(vec![
            LedgerTxV1::SyncBond { bond: PROD, collateral: 1_000_000 },
            LedgerTxV1::SyncBond { bond: OUT, collateral: 1_000_000 },
            LedgerTxV1::SyncBond { bond: ANY, collateral: 1_000_000 },
            LedgerTxV1::AttestArtifact { artifact_root: pc.root() },
        ]);
        assert!(refusal(&ev).is_none(), "{ev:?}");
        // A plan of more than one position is refused by name (its class admitted, so the refusal is the encoder rule's).
        let wide = plan_for_tir_program_v1(&d, &w.enc.program, program_root_v1(&bytes), 2).unwrap();
        let wide_class = single_class_id_v1(d.digest(), &bytes, &wide, &pc, OPV);
        let ev = w.block(vec![
            LedgerTxV1::AdmitOptimisticClass { class: wide_class },
            obj(
                PROD,
                O::RegisterClassV2 {
                    mode: OPV,
                    descriptor: d.digest(),
                    program_bytes: bytes.clone(),
                    plan: wide,
                    param_commitments: pc.clone(),
                },
            ),
        ]);
        assert!(refusal(&ev).is_some_and(|r| r.contains("one position")), "a v5 plan of two positions: {ev:?}");
        let ev = w.block(vec![
            LedgerTxV1::AdmitOptimisticClass { class },
            obj(PROD, O::RegisterClassV2 { mode: OPV, descriptor: d.digest(), program_bytes: bytes, plan, param_commitments: pc }),
        ]);
        assert!(ev.iter().any(|e| matches!(e, E::ClassRegistered { class: c } if *c == class)), "{ev:?}");
        w
    }

    fn block(&mut self, txs: Vec<LedgerTxV1>) -> Vec<E> {
        self.daa += 1;
        self.l.apply_block(&LedgerBlockV1 { daa: self.daa, txs })
    }

    fn beat_to(&mut self, daa: u64) -> Vec<E> {
        let mut all = Vec::new();
        while self.daa < daa {
            all.extend(self.block(Vec::new()));
        }
        all
    }

    /// A tiled encoder job of `prompt` (no generated id) and its one tile.
    fn job(&mut self, prompt: &[u32]) -> Digest {
        let job = TiledJobV1 {
            class_binding_id: self.class,
            prompt_len: prompt.len() as u32,
            prompt_root: prompt_root_of_ids_v1(prompt),
            max_new_tokens: 0,
            decode: DecodeRuleV1::Greedy,
            nonce: [self.daa as u8; 64],
        };
        let id = job.id();
        let ev = self.block(vec![obj(ANY, O::PostTiledJob { job })]);
        assert!(ev.iter().any(|e| matches!(e, E::JobPosted { .. })), "{ev:?}");
        let ev = self.block(vec![obj(ANY, O::PostPromptTile { job: id, tile: PromptTileOpeningV1::of(prompt, 0).unwrap() })]);
        assert!(refusal(&ev).is_none(), "{ev:?}");
        id
    }
}

struct Produced {
    values: Vec<Vec<Vec<Tensor>>>,
    c: SegmentedCommitmentsV1,
    claim: KernelClaimV1,
    evidence: SegmentedEvidenceV2,
}

impl SegMaterialV1 for Produced {
    fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>> {
        self.values.get(p as usize).cloned()
    }
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
        (p < self.c.positions()).then(|| self.c.position_path(p).1)
    }
}

fn produce(w: &World, job: Digest, prompt: &[u32], lie: Option<(u16, u16)>) -> Produced {
    let trace = trace_encoder_v1(&w.enc.program, &w.enc.params, &w.enc.binding, prompt).expect("the encoder's trace");
    let mut values = trace.values;
    if let Some((s, n)) = lie {
        let t = &mut values[0][s as usize][n as usize];
        let v = t.data[0];
        t.data[0] = if t.dtype.contains(v + 1) { v + 1 } else { v - 1 };
    }
    let c = seg_commitments_of_trace_v1(&TraceV1 { values: values.clone(), inputs: Vec::new() });
    let class = &w.l.classes[&w.class];
    let evidence =
        build_segmented_evidence_v1(class.header(w.class), &w.d, prompt.len() as u32, &prompt_root_of_ids_v1(prompt), &[], &c);
    let claim = KernelClaimV1 { job_id: job, producer_bond: PROD, generated: Vec::new(), evidence_root: evidence.root() };
    Produced { values, c, claim, evidence }
}

fn commit(w: &mut World, p: &Produced) -> Vec<E> {
    // Past `palw_panel_free_v1` (the OPV policy's activation) the seal is claim seal v2 and the reveal carries its salt.
    let id = p.claim.id();
    let salted = w.l.salted_seals_from().is_some_and(|at| w.l.daa.saturating_add(1) >= at);
    let salt = misaka_palw_kernel::hash::id(b"misaka-palw/test/claim-salt", &id);
    let seal = if salted { misaka_palw_kernel::ledger::claim_seal_v2(&id, &salt) } else { claim_seal_v1(&id) };
    let ev = w.block(vec![obj(PROD, O::SealClaim { producer: PROD, job: p.claim.job_id, seal })]);
    assert!(refusal(&ev).is_none(), "{ev:?}");
    let (claim, evidence, segment_roots) = (p.claim.clone(), p.evidence.clone(), p.c.segment_roots());
    let reveal = if salted {
        O::CommitClaimSalted { salt, commit: misaka_palw_kernel::ledger::SaltedCommitV1::Segmented { claim, evidence, segment_roots } }
    } else {
        O::CommitSegmentedClaim { claim, evidence, segment_roots }
    };
    w.block(vec![obj(PROD, reveal)])
}

/// The nodes a lie is put in: the first node that reads the job's ids (the embedding lookup: its court opens the prompt tile), the
/// first `MatMul` of the first layer (a projection), the first `MatMul` of two committed values in the first layer (the attention's
/// scores or its mix), and the program's output (the job's result).
fn targets(e: &Encoder) -> Vec<(&'static str, u16, u16)> {
    use misaka_palw_tir::Ref;
    let p = &e.program;
    let occ = p.occurrences();
    let first = |s: usize, f: &dyn Fn(&misaka_palw_tir::Node) -> bool| -> Option<u16> {
        p.blocks[occ[s].0 as usize].nodes.iter().position(f).map(|n| n as u16)
    };
    let ids = e.binding.first_input;
    let lookup = first(0, &|n| n.inputs.iter().any(|r| matches!(r, Ref::Param(j) if *j == ids))).expect("a node reads the ids");
    let projection = first(1, &|n| matches!(n.prim, Prim::MatMul)).expect("a projection");
    let attention =
        first(1, &|n| matches!(n.prim, Prim::MatMul) && n.inputs.iter().all(|r| matches!(r, Ref::Node(_) | Ref::CarryIn(_))))
            .expect("an attention product");
    // The first node of any occurrence that reads the count (the key mask, or the pooling's length): no tile, the job states it.
    let count = (0..occ.len())
        .find_map(|s| first(s, &|n| n.inputs.iter().any(|r| matches!(r, Ref::Param(j) if *j == ids + 1))).map(|n| (s as u16, n)))
        .expect("a node reads the count");
    let post = (occ.len() - 1) as u16;
    vec![
        ("the embedding lookup (reads the job's ids)", 0, lookup),
        ("a projection", 1, projection),
        ("the key mask (reads the job's count)", count.0, count.1),
        ("an attention product", 1, attention),
        ("the result", post, p.logits),
    ]
}

/// **K2-TIR-v5 on a real tiny encoder and a real tiny reranker head**: registered under OPV as ONE position whose ids are the job's;
/// an honest claim checks clean (by the element courts and by re-execution) and an honest element filed against it is dismissed; a lie in
/// each target value is localized to one element of that value, from public material (the claim's one segment root, the job's prompt
/// tile, the public artifact), and convicted; an encoder job that generates, or a claim that delivers ids, is refused by name.
#[test]
fn k2s_v5_encoders_and_heads_are_judged_one_element_at_a_time_from_public_material() {
    for fixture in ["hf-enc/bert", "hf-cls/xlmr_rerank"] {
        let enc = tiny_encoder(fixture, 12);
        let prompt: Vec<u32> = [enc.cls, 11 % enc.vocab, 25 % enc.vocab, 7 % enc.vocab, enc.sep].to_vec();
        let artifact_params = enc.params.clone();
        let art = move |j: u16, l: Option<u16>| artifact_params.tensors.get(&(j, l)).cloned();
        let targets = targets(&enc);
        let mut w = World::new(enc);
        // Refused by name: an encoder job that generates, and one longer than the axis.
        let mut bad = TiledJobV1 {
            class_binding_id: w.class,
            prompt_len: 5,
            prompt_root: prompt_root_of_ids_v1(&prompt),
            max_new_tokens: 1,
            decode: DecodeRuleV1::Greedy,
            nonce: [0xEE; 64],
        };
        let ev = w.block(vec![obj(ANY, O::PostTiledJob { job: bad.clone() })]);
        assert!(refusal(&ev).is_some_and(|r| r.contains("generates nothing")), "{ev:?}");
        bad.max_new_tokens = 0;
        bad.prompt_len = 13;
        let ev = w.block(vec![obj(ANY, O::PostTiledJob { job: bad })]);
        assert!(refusal(&ev).is_some_and(|r| r.contains("generates nothing")), "{ev:?}");

        // The honest claim.
        let job = w.job(&prompt);
        let honest = produce(&w, job, &prompt, None);
        let mut delivering = produce(&w, job, &prompt, None);
        delivering.claim.generated = vec![1];
        let ev = commit(&mut w, &delivering);
        assert!(refusal(&ev).is_some_and(|r| r.contains("delivers no id")), "{ev:?}");
        let ev = commit(&mut w, &honest);
        assert!(ev.iter().any(|e| matches!(e, E::ClaimCommitted { .. })), "{ev:?}");
        let id = honest.claim.id();
        let view = w.l.seg_claim_view_v1(&id).unwrap();
        assert_eq!((view.positions, view.segment_roots.len(), view.encoder), (1, 1, Some(w.enc.binding)));
        let ctx = view.context();
        assert_eq!(check_positions_v1(&ctx, &honest, &art, &prompt, &[0]), SegFindingV1::Clean, "{fixture}: honest");
        let own = trace_encoder_v1(&w.enc.program, &w.enc.params, &w.enc.binding, &prompt).unwrap();
        let roots = seg_commitments_of_trace_v1(&own).position_roots;
        assert_eq!(check_claim_by_reexecution_v1(&ctx, &roots, None, &honest, &art, &prompt).finding, SegFindingV1::Clean);
        // Every honest element filed (the first, a middle and the last of every value) is dismissed, and every filing is within the
        // court's priced bound for its relation: the price the gate holds against the carrier is an upper bound of what is filed.
        let occ = w.enc.program.occurrences();
        let node_count: u64 = occ.iter().map(|(b, _)| w.enc.program.blocks[*b as usize].nodes.len() as u64).sum();
        let (mut filed, mut worst_ratio) = (0usize, 0f64);
        for (s, (b, _)) in occ.iter().enumerate() {
            for n in 0..w.enc.program.blocks[*b as usize].nodes.len() {
                let len = honest.values[0][s][n].len() as u64;
                let priced = misaka_palw_kernel::element::element_court_cost_in_v1(
                    &w.enc.program,
                    *b as usize,
                    n,
                    node_count,
                    1,
                    Some(&w.enc.binding),
                )
                .0;
                assert!(priced <= w.plan_worst, "{fixture}: ({s}, {n}): a relation's price is within the plan's worst court");
                for e in [0, len / 2, len.saturating_sub(1)] {
                    let filing = prove_element_v1(&ctx, &honest, &art, &prompt, (0, s as u16, n as u16), e)
                        .unwrap_or_else(|why| panic!("{fixture}: ({s}, {n}) element {e}: {why}"));
                    let fault = SegFaultV1::Element(filing);
                    let bytes = fault.to_bytes().len() as u64;
                    assert!(bytes <= priced, "{fixture}: ({s}, {n}) element {e}: filed {bytes} B above its priced {priced} B");
                    worst_ratio = worst_ratio.max(bytes as f64 / priced as f64);
                    assert!(
                        matches!(
                            misaka_palw_kernel::element::verify_seg_fault_v1(&ctx, &fault),
                            Err(misaka_palw_kernel::verify::DismissalV1::NoFault)
                        ),
                        "{fixture}: ({s}, {n}) element {e}: an honest element is dismissed"
                    );
                    filed += 1;
                }
            }
        }
        println!(
            "[k2s-enc] v5 {fixture}: {filed} honest element filings dismissed, each within its priced bound (largest ratio {worst_ratio:.3})"
        );
        let (_, s, n) = targets[3];
        let filing = prove_element_v1(&ctx, &honest, &art, &prompt, (0, s, n), 0).unwrap();
        let ev = w.block(vec![obj(
            OUT,
            O::FileProof { accuser: OUT, claim: id, proof: ProsecutionV1::Segmented(SegFaultV1::Element(filing).to_bytes()) },
        )]);
        assert!(ev.iter().any(|e| matches!(e, E::ProofDismissed { .. })), "{fixture}: an honest element is dismissed: {ev:?}");

        // A lie in each target value.
        for (what, s, n) in targets.iter().copied() {
            let job = w.job(&prompt);
            let liar = produce(&w, job, &prompt, Some((s, n)));
            let ev = commit(&mut w, &liar);
            assert!(ev.iter().any(|e| matches!(e, E::ClaimCommitted { .. })), "{ev:?}");
            let id = liar.claim.id();
            let view = w.l.seg_claim_view_v1(&id).unwrap();
            let ctx = view.context();
            let SegFindingV1::Fault(fault) = check_positions_v1(&ctx, &liar, &art, &prompt, &[0]) else {
                panic!("{fixture}: the lie in {what} is not found")
            };
            let e = &fault;
            assert_eq!(e.at(), Some((0, s, n)), "{fixture}: {what}: localized to the lying value");
            assert_eq!(e.token().is_some(), what.starts_with("the embedding lookup"), "{fixture}: {what}: the prompt tile");
            let r = check_claim_by_reexecution_v1(&ctx, &roots, None, &liar, &art, &prompt);
            assert_eq!((r.divergent, r.probes), (Some(0), 0), "{fixture}: {what}: one position, no probe");
            assert!(matches!(r.finding, SegFindingV1::Fault(_)), "{fixture}: {what}: re-execution finds it too");
            let bytes = fault.to_bytes();
            println!("[k2s-enc] v5 {fixture}: a lie in {what} ({s}, {n}) is filed in {} B", bytes.len());
            assert!(bytes.len() < CARRIER);
            let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: id, proof: ProsecutionV1::Segmented(bytes) })]);
            assert!(ev.iter().any(|e| matches!(e, E::Convicted { claim, .. } if *claim == id)), "{fixture}: {what}: {ev:?}");
        }
        // The honest claim finalizes at the window's end.
        let ev = w.beat_to(w.daa + 80);
        assert!(ev.iter().any(|e| matches!(e, E::Final { claim, .. } if *claim == honest.claim.id())), "{fixture}: {ev:?}");
    }
}
