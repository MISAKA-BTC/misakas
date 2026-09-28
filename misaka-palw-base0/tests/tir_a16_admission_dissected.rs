//! **RFC-0002 Phase F, D-F1's admission gate (F7): the Qwen2.5-1.5B A16 decoder as an IR class is
//! admitted by admission v10 under testnet-12's ceilings, its history cones dissected.**
//!
//! `a16_mirror_program` commits two tensors whose cones reduce over the history in every layer: the
//! attention probabilities (the softmax's maximum and exponent sum over `H`) and the attention context
//! (the value contraction over `H`). Whole, at `H = W = 2^18`, neither can be closed — the context's tile
//! costs 33.5 M MACs against testnet-12's 16 Mi terminal, the probabilities' reads a whole row of
//! scores, a close no carrier holds — so without the k-ary court the class is refused. Under the court (testnet-12 arms it) each is dissected: its terminal is one
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

/// The logits tile the t12 dense row's IR class declares: 1,024 lanes. As CARRIED, a logits tile's
/// close holds one inventory leaf per output row — the row's 1,536 bytes, its 20-sibling path (1,280
/// bytes) and a header, ≈ 2,860 bytes a lane (Phase F's measurement) — so 1,024 lanes are ≈ 2.93 MB
/// with the frame, inside testnet-12's 3.2 MB, and 2,048 (≈ 5.9 MB) are not. Admission's measure is
/// element-granular until it prices carried units (`TirCloseDemandV1`), so it would still pass 2,048;
/// the class declares the tile that is carriable by the exact count.
const LOGITS_TILE: u32 = 1024;

/// The class at `h_tile`: 128-lane commit tiles (the FFN gate's at `gate`), the logits at `logits`,
/// a history's rows whole.
fn class_with(program: &TirProgramV1, h_tile: u32, logits: u32, gate: u32) -> PalwTirClassV1 {
    let ff = QWEN25_1_5B.ffn_dim as u64;
    let mut commit_tiles = Vec::new();
    let mut gate_seen = false;
    for (bi, block) in program.blocks.iter().enumerate() {
        for (ni, node) in block.nodes.iter().enumerate() {
            if node.commit {
                let is_logits = bi == program.schedule.post as usize && ni == program.logits as usize;
                // The first committed `[d_ff]` node of the layer is the FFN gate.
                let is_gate = !gate_seen && node.out.elements_at(1) == ff;
                gate_seen |= is_gate;
                commit_tiles.push(if is_logits {
                    logits
                } else if is_gate {
                    gate
                } else {
                    128
                });
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

fn class(program: &TirProgramV1, h_tile: u32) -> PalwTirClassV1 {
    class_with(program, h_tile, LOGITS_TILE, 128)
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

    // Without the k-ary court nothing dissects, and the history cones cannot be closed whole: the
    // probabilities' tile reads its whole row of scores over `H` (a close no carrier holds), the
    // context's costs 33.5 M MACs against the 16 Mi terminal.
    let mut none = r;
    none.court = None;
    let whole = verify_class_admission_v10(&b, &none, &registration(class(&program, 64)), &[], &[]).map(|_| ());
    assert!(
        matches!(
            whole,
            Err(E::TirNeedsDissection { .. } | E::CourtCostExceedsCeiling { what: "IR terminal close bytes as carried", .. })
        ),
        "{whole:?}"
    );
}

/// **Decision (1) of 2026-09-28: every terminal close an executor can be clocked for is carriable**
/// (spec 04b §10.3) — at most the fold's chunks (testnet-12: `min(202, 32)`) of one carrier each,
/// 3,200,000 bytes. The class's executor is clocked at every terminal leaf (it has dissected points),
/// so a close past that is refused by name with its value and the cap:
/// * the tiled scheme's full 4,096-lane logits tile reads 4,096 weight rows (6.3 MB) and is refused;
///   the D-F1 layout's 1,024-lane divisor is admitted;
/// * the boundary: the FFN gate's tile (a row of 1,536 weight bytes a lane) is admitted at `L*`
///   lanes and refused at `L* + 1`.
#[test]
fn every_terminal_close_of_the_1_5b_class_is_carriable() {
    use kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1;
    let p = params();
    let b = bundle(&p);
    let r = PalwTirAdmissionRulesV1::at(&p, AT).expect("the fence is in force");
    let cap = palw_tir_carriable_close_bytes_v1(&b.court);
    assert_eq!(cap, 32 * 100_000, "testnet-12 carries a close in the fold's 32 chunks, not its ruleset's 202");
    let program = program();
    let admit = |logits: u32, gate: u32| {
        verify_class_admission_v10(&b, &r, &registration(class_with(&program, 64, logits, gate)), &[], &[]).map(|_| ())
    };

    let full = admit(4096, 128);
    assert!(
        matches!(full, Err(E::CourtCostExceedsCeiling { what: "IR terminal close bytes as carried", got, ceiling }) if got > ceiling && ceiling == cap),
        "the 4,096-lane logits close is not carriable: {full:?}"
    );
    assert_eq!(admit(LOGITS_TILE, 128), Ok(()), "the 1,024-lane logits tile is");

    // The boundary, on the FFN gate's tile.
    let ff = QWEN25_1_5B.ffn_dim;
    let lanes = {
        let (mut lo, mut hi) = (128u32, ff);
        assert!(admit(LOGITS_TILE, lo).is_ok() && admit(LOGITS_TILE, hi).is_err());
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if admit(LOGITS_TILE, mid).is_ok() { lo = mid } else { hi = mid }
        }
        lo
    };
    let over = admit(LOGITS_TILE, lanes + 1);
    eprintln!("the FFN gate's tile: {lanes} lanes admitted, {} refused: {over:?}", lanes + 1);
    assert!(
        matches!(over, Err(E::CourtCostExceedsCeiling { what: "IR terminal close bytes as carried", got, ceiling }) if got > ceiling && ceiling == cap),
        "one lane over is refused by name: {over:?}"
    );
    assert_eq!(admit(LOGITS_TILE, lanes), Ok(()), "one lane under is admitted");
    assert!((2000..2100).contains(&lanes), "about 3.2 MB of 1,536-byte rows: {lanes}");
}
