//! **RFC-0004 §6.3 at a real line's size: what admitting a LoRA candidate of a SmolLM2-1.7B-shaped
//! parent costs, and whether it fits admission's work cap.**
//!
//! Phase H registers SmolLM2-1.7B-Instruct as an IR class at testnet-12's IR flag day; it is the
//! first line a governed epoch would improve. Admission of a composite sizes every terminal close of
//! the CANDIDATE — the parent's commit points and the adapter's — across the parent and adapter
//! roots (`TirCloseDemandV1`), inside the same `2^26`-step cap v10 applies to a registration. Measured
//! here on the lowered programs alone (a program and a layout are all the sizing reads): the parent as
//! v10 sizes it and each candidate (rank 16 on q/k/v/o, and on every linear) as its composite admission
//! does, at 2,048 and 8,192 positions, with the logits in 512-lane tiles; the work is printed against
//! the cap, and every close must be carriable.

use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_improve_composite_v1::palw_tir_composite_rule_v1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1;
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_consensus_core::palw_tir_close_size_v1::{
    PALW_TIR_CLOSE_SIZING_WORK_CAP_V1, PalwTirCloseSizingV1, PalwTirParamFormV1, palw_tir_worst_closes_work_v1,
};
use kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1;
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_hashes::Hash64;
use misaka_palw_sdk::tir_layout::{TirLayoutChoiceV1, tir_default_layout_v1, tir_program_with_scheme_v1};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_lower::lower::{self, LowerOpts};
use misaka_palw_tir_lower::{hl, lora};

/// SmolLM2-1.7B's shape (a Llama: 24 layers, 2,048 wide, 32 heads with 32 KV heads, 8,192 MLP,
/// 49,152 tokens, tied embeddings).
const SMOLLM2_1_7B: &str = r#"{
  "architectures": ["LlamaForCausalLM"], "model_type": "llama", "hidden_act": "silu",
  "hidden_size": 2048, "intermediate_size": 8192, "num_attention_heads": 32, "num_key_value_heads": 32,
  "num_hidden_layers": 24, "vocab_size": 49152, "max_position_embeddings": 8192, "rms_norm_eps": 1e-05,
  "rope_theta": 130000, "tie_word_embeddings": true, "attention_bias": false, "mlp_bias": false,
  "bos_token_id": 1, "eos_token_id": 2
}"#;

fn params() -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(1_000)));
    p.sync_palw_tir_v1();
    p
}

fn lowered(cfg: &str, adapter: Option<&str>) -> (TirProgramV1, usize) {
    let mut spec = misaka_palw_tir_lower::parse_config_str(cfg).expect("the spec");
    if let Some(a) = adapter {
        lora::attach(&mut spec, a).expect("the adapter attaches");
    }
    let hl = hl::build_program(&spec).expect("the HL program");
    let mut lw = lower::lower(&hl, &LowerOpts::default()).expect("lowered");
    let p =
        if adapter.is_some() { lower::adapter_params_last(&mut lw).expect("adapter params last") } else { lw.program.params.len() };
    (tir_program_with_scheme_v1(&lw.program, None).expect("under a logits scheme"), p)
}

/// The sizing admission runs, uncapped so the work it needs is measured: `(work, largest close, time)`.
fn sized(net: &Params, program: &TirProgramV1, context: u32, form: PalwTirParamFormV1) -> (u64, u64, std::time::Duration) {
    let choice = TirLayoutChoiceV1 { max_context: Some(context), logits_tile: Some(512), ..TirLayoutChoiceV1::default() };
    let layout = tir_default_layout_v1(net, program, &choice).expect("a layout");
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout,
        tokenizer_id: Hash64::from_bytes([0x5a; 64]),
    };
    let id = Hash64::from_bytes([0x22; 64]);
    let space = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(&class).expect("a step space");
    let longest =
        kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, id, (1, context)).expect("the longest job");
    let inventory = PalwTirInventoryIndexV1::new(&space.program).expect("an inventory");
    let sizing = PalwTirCloseSizingV1 { form, court: true, cap: 1 << 40, stop_above: None };
    let started = std::time::Instant::now();
    let (bounds, work) = palw_tir_worst_closes_work_v1(&space, &inventory, &longest, &sizing).expect("sized");
    (work, bounds.iter().map(|b| b.close_bytes).max().unwrap_or(0), started.elapsed())
}

#[test]
fn what_admitting_a_smollm2_1_7b_lora_candidate_costs() {
    let net = params();
    let PalwConsensusMode::ConsensusV2(bundle) = &net.palw_consensus_mode else { panic!("testnet-12 is a V2 network") };
    let carriable = palw_tir_carriable_close_bytes_v1(&bundle.court);
    let cap = PALW_TIR_CLOSE_SIZING_WORK_CAP_V1;
    let (parent, _) = lowered(SMOLLM2_1_7B, None);
    let report = |what: &str, (work, worst, t): (u64, u64, std::time::Duration)| {
        eprintln!(
            "{what}: {work} steps ({:.0}% of the 2^26 cap{}) in {t:?}; largest close {worst} B of {carriable}",
            100.0 * work as f64 / cap as f64,
            if work > cap { ", PAST IT" } else { "" }
        );
        assert!(worst <= carriable, "{what}: every close carriable");
    };
    for context in [2048u32, 8192] {
        report(&format!("SmolLM2-1.7B @{context}"), sized(&net, &parent, context, PalwTirParamFormV1::Multiproof));
        for (what, targets) in [("q/k/v/o", r#"["q_proj","k_proj","v_proj","o_proj"]"#), ("all-linear", r#""all-linear""#)] {
            let adapter = format!(r#"{{"peft_type":"LORA","r":16,"lora_alpha":32,"target_modules":{targets}}}"#);
            let (candidate, p) = lowered(SMOLLM2_1_7B, Some(&adapter));
            palw_tir_composite_rule_v1(&parent, &candidate, p as u32).unwrap_or_else(|e| panic!("{what}: {e}"));
            report(
                &format!("  + LoRA r16 {what} @{context} ({} adapter params)", candidate.params.len() - p),
                sized(&net, &candidate, context, PalwTirParamFormV1::Composite { p: p as u32 }),
            );
        }
    }
}
