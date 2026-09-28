//! **RFC-0002 Phase F, D-F1's admission gate (F7): the Qwen2.5-1.5B A16 decoder as an IR class is
//! admitted by admission v10 under testnet-12's ceilings, its history cones dissected.**
//!
//! `a16_mirror_program` commits two tensors whose cones reduce over the history in every layer: the
//! attention probabilities (the softmax's maximum and exponent sum over `H`) and the attention context
//! (the value contraction over `H`). Whole, at `H = W = 2^18`, each costs 33.5 M MACs a tile against
//! testnet-12's 16 Mi terminal, so without the k-ary court the class is refused
//! (`TirNeedsDissection`). Under the court (testnet-12 arms it) each is dissected: its terminal is one
//! `h_tile` chunk, its claims and rounds fit one carrier, and the exchange fits the court window —
//! the class is admitted, and its record names both dissected commit points.
//!
//! The class is the t12 dense row's shape (`graph-v7@8192`): an 8,192-position context, 128-lane
//! commit tiles, the logits in 4,096-lane tiles, whole-row history sub-rows.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError as E;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1;
use kaspa_consensus_core::palw_tir_admission_v1::{
    PalwTirAdmissionRulesV1, palw_tir_post_genesis_registration_v1, verify_class_admission_v10,
};
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_dissected_commit_points_v1;
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use misaka_palw_base0::artifact::Base0ShapeV1;
use misaka_palw_base0::tir_a16::a16_mirror_program;
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, StateKind};

const AT: u64 = 1_000;
const CONTEXT: u32 = 8_192;

fn params() -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
    p.sync_palw_tir_v1();
    p
}

fn bundle(p: &Params) -> PalwConsensusParamsV2 {
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is a V2 network") };
    bundle.clone()
}

fn shape(g: PalwQwen25GeometryV1) -> Base0ShapeV1 {
    Base0ShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        n_kv_heads: g.attn_kv_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        max_position: g.n_ctx as usize,
        ln_theta_gen_q: 0,
        eps_q: g.rms_eps_q,
    }
}

fn program() -> TirProgramV1 {
    let mut program = a16_mirror_program(&shape(QWEN25_1_5B), HISTORY_BOUND_V1_SMALL).expect("the A16 program");
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    program
}

/// The class at `h_tile`: 128-lane commit tiles, the logits at 4,096, a history's rows whole.
fn class(program: &TirProgramV1, h_tile: u32) -> PalwTirClassV1 {
    let mut commit_tiles = Vec::new();
    for (bi, block) in program.blocks.iter().enumerate() {
        for (ni, node) in block.nodes.iter().enumerate() {
            if node.commit {
                let is_logits = bi == program.schedule.post as usize && ni == program.logits as usize;
                commit_tiles.push(if is_logits { 4096 } else { 128 });
            }
        }
    }
    let state_tiles = program
        .states
        .iter()
        .map(|s| match s.kind {
            StateKind::Hist { .. } => s.shape.iter().product::<u32>(),
            StateKind::Fixed { .. } => 128,
        })
        .collect();
    PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: CONTEXT,
            checkpoint_interval: 16,
            h_tile,
            commit_tiles,
            state_tiles,
        },
        tokenizer_id: Hash64::from_bytes([0x70; 64]),
    }
}

fn registration(class: PalwTirClassV1) -> PalwConsensusObjectV2 {
    let root = Hash64::from_bytes([0xA1; 64]);
    let class_id = class.class_id(&root);
    let facts = PalwTirJobFactsV1::of_class(&class, class_id).expect("decodes");
    let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&class).expect("wide enough"));
    palw_tir_post_genesis_registration_v1(
        class,
        canonical,
        root,
        0,
        1 << 100,
        1,
        AT + 10,
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
            transaction_id: kaspa_consensus_core::tx::TransactionId::from_bytes([7; 64]),
            index: 0,
        }),
        vec![9; 16],
        bundle(&params()).court.max_step_leaf_count(),
    )
    .expect("the builder counts the canonical job")
}

#[test]
fn the_qwen25_1_5b_a16_program_is_admitted_with_its_history_cones_dissected() {
    let p = params();
    let b = bundle(&p);
    let r = PalwTirAdmissionRulesV1::at(&p, AT).expect("the fence is in force");
    assert!(r.court.is_some(), "testnet-12 arms the k-ary court");
    let program = program();
    let dissected = palw_tir_dissected_commit_points_v1(&program);
    assert_eq!(dissected.len(), 2, "the probabilities and the context reduce over H");

    let mut admitted = Vec::new();
    for h_tile in [16u32, 32, 64, 128, 256] {
        let object = registration(class(&program, h_tile));
        let started = std::time::Instant::now();
        let verdict = verify_class_admission_v10(&b, &r, &object, &[], &[]);
        eprintln!(
            "h_tile {h_tile:>3}: {:?} in {:?}",
            verdict.as_ref().map(|(e, rec)| (e.court_cost.max_close_bytes, e.court_cost.max_terminal_macs, rec.dissected.len())),
            started.elapsed()
        );
        if let Ok((entry, record)) = verdict {
            assert_eq!(record.dissected, dissected, "the record names both dissected commit points");
            assert!(entry.court_cost.max_terminal_macs <= b.court.max_terminal_macs(), "every terminal fits the court");
            admitted.push(h_tile);
        }
    }
    assert!(!admitted.is_empty(), "some history tile admits the 1.5B decoder under testnet-12's ceilings");
    assert!(admitted.contains(&64), "the 64-position history tile does: {admitted:?}");

    // Without the k-ary court nothing dissects, and the whole cone does not fit the terminal.
    let mut none = r;
    none.court = None;
    let whole = verify_class_admission_v10(&b, &none, &registration(class(&program, 64)), &[], &[]).map(|_| ());
    assert!(matches!(whole, Err(E::TirNeedsDissection { .. })), "{whole:?}");
}
