//! **RFC-0002 Phase F, step F7: admitting a class whose cones reduce over the history** (spec 04b
//! §9.5.6), on testnet-12's ruleset with `palw_tir_v1` armed (its k-ary court: arity 4, a 3,000-DAA
//! court window, 42-DAA rungs).
//!
//! * **The window.** A dissected cone is admitted only if the whole exchange — the root claim, the
//!   rounds and choices over `max_context` positions in `h_tile` tiles, the bottom's assembly — fits
//!   strictly inside the court window at the court's arity (O-5). The dense GQA model's need grows
//!   with its context, fits testnet-12's window up to the longest context admission v10 takes (a 4,096-id canonical prompt), and a
//!   window of exactly what the exchange takes is refused by name.
//! * **Without the k-ary court nothing dissects**, so the same cone must fit the court whole at
//!   `H = W`: under a terminal ceiling that one history tile fits, the class is admitted with the
//!   court and refused without it — and whole, at any ceiling, its close reads the whole history,
//!   which no carrier holds (PALW-TIR-38).
//! * **The obligations** (O-1, O-2): more than sixteen reductions over `H` in a cone, and a read of a
//!   reduction's output chosen by a history-varying value (a `Gather`'s indices, a `Select`'s
//!   condition), are refused by name; the same cone without the defect is admitted.

#[path = "palw_tir_fixture_common.rs"]
#[allow(unused)]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, artifact_root_v1};
use kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError as E;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2, PalwCourtParamsV2};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1;
use kaspa_consensus_core::palw_tir_admission_v1::{
    PalwTirAdmissionRulesV1, PalwTirClassRecordV1, palw_tir_post_genesis_registration_v1, verify_class_admission_v10,
};
use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_operands_v1;
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_dissected_commit_points_v1;
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{Cmp, DType, Dim, MapParams, Ref, Tensor, TensorType, TirProgramV1};

const AT: u64 = 1_000;

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

fn rules() -> PalwTirAdmissionRulesV1 {
    PalwTirAdmissionRulesV1::at(&params(), AT).expect("the fence is in force")
}

/// `bundle` with its court's terminal ceiling (and, through it, the IR court's work limits) at `macs`.
fn with_terminal_macs(bundle: &PalwConsensusParamsV2, macs: u64) -> PalwConsensusParamsV2 {
    let c = &bundle.court;
    let mut b = bundle.clone();
    b.court = PalwCourtParamsV2::with_cost_ceilings(
        c.max_step_leaf_count(),
        c.turn_deadline_daa(),
        2,
        c.max_close_bytes(),
        macs,
        c.max_operand_count(),
    )
    .expect("a court");
    b
}

fn root_of(program: &TirProgramV1, params: &MapParams) -> Hash64 {
    let ops = palw_tir_inventory_operands_v1(program, &TensorSrc(params)).expect("the inventory");
    artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("a root")
}

fn registration(program: &TirProgramV1, root: Hash64, context: u32) -> PalwConsensusObjectV2 {
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: layout(program, context),
        tokenizer_id: Hash64::from_bytes([0x70; 64]),
    };
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

fn admit(b: &PalwConsensusParamsV2, r: &PalwTirAdmissionRulesV1, object: &PalwConsensusObjectV2) -> Result<PalwTirClassRecordV1, E> {
    verify_class_admission_v10(b, r, object, &[], &[]).map(|(_, record)| record)
}

fn dense() -> (TirProgramV1, Hash64) {
    let (_, mut program, params, _) = programs().into_iter().find(|(n, ..)| n == "dense-gqa-2layer").expect("the dense model");
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let root = root_of(&program, &params);
    (program, root)
}

/// The smallest `x` in `[lo, hi]` for which `ok(x)` holds, `ok` being monotone.
fn least(mut lo: u64, mut hi: u64, ok: impl Fn(u64) -> bool) -> u64 {
    assert!(ok(hi), "the upper end holds");
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if ok(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo
}

#[test]
fn the_window_decides_a_dissected_cone_s_admission() {
    let (p, r) = (params(), rules());
    let b = bundle(&p);
    let court = r.court.expect("testnet-12 arms the k-ary court");
    let (program, root) = dense();
    let dissected = palw_tir_dissected_commit_points_v1(&program);
    assert!(!dissected.is_empty(), "the attention output reduces over the history");
    let mut previous = 0;
    for context in [64u32, 4096, 32_768] {
        let object = registration(&program, root, context);
        let record = admit(&b, &r, &object).unwrap_or_else(|e| panic!("{context} positions: {e}"));
        assert_eq!(record.dissected, dissected, "the record names the class's dissected commit points");
        // The DAA the exchange takes at this context: the window must exceed it (strictly, as every
        // court window does), and a window of exactly that is refused, by name.
        let at_window = |w: u64| {
            let mut short = r;
            short.court = Some(kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 { window_court_daa: w, ..court });
            admit(&b, &short, &object)
        };
        let needed = least(1, court.window_court_daa, |w| at_window(w).is_ok()) - 1;
        assert_eq!(
            at_window(needed).map(|_| ()),
            Err(E::CourtWindowTooShort { needed, window: needed }),
            "{context} positions: a window of exactly the exchange"
        );
        assert!(needed > previous, "{context} positions: a longer history needs more rounds");
        assert!(needed < court.window_court_daa, "testnet-12's window fits the longest context v10 admits");
        eprintln!("{context:>7} positions: the exchange needs {needed} of {} DAA", court.window_court_daa);
        previous = needed;
    }
}

#[test]
fn without_the_court_a_history_cone_must_fit_whole() {
    let (p, r) = (params(), rules());
    let b = bundle(&p);
    let (program, root) = dense();
    let long = registration(&program, root, 32_768);
    let mut none = r;
    none.court = None;
    // The least terminal ceiling the court admits the long class under: one history tile.
    let dissected = least(1, b.court.max_terminal_macs(), |m| admit(&with_terminal_macs(&b, m), &r, &long).is_ok());
    eprintln!("the long class needs a {dissected}-MAC terminal dissected");
    let tight = with_terminal_macs(&b, dissected);
    assert!(admit(&tight, &r, &long).is_ok(), "dissected under the court");
    let refused = admit(&tight, &none, &long);
    assert!(
        matches!(
            refused,
            Err(E::TirNeedsDissection { .. } | E::CourtCostExceedsCeiling { what: "IR terminal close bytes as carried", .. })
        ),
        "without the court the history cone does not fit whole: {refused:?}"
    );
    // At ANY terminal ceiling: whole, the cone reads the whole history (costed at the program's own
    // window `W`, spec 04b §10.3), a close no carrier holds (PALW-TIR-38).
    let unbounded = admit(&with_terminal_macs(&b, u64::MAX >> 8), &none, &long);
    assert!(
        matches!(unbounded, Err(E::CourtCostExceedsCeiling { what: "IR terminal close bytes as carried", got, ceiling }) if got > ceiling),
        "{unbounded:?}"
    );
}

// -------------------------------------------------------------------------------------------------
// The obligations, on a one-layer program whose layer appends a clamped row to a history and commits
// a cone built over it by `body`.
// -------------------------------------------------------------------------------------------------

fn with_body(body: impl FnOnce(&mut BlockBuilder<'_>, Ref) -> Ref) -> (TirProgramV1, MapParams) {
    let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
    let embed = pb.param("embed", DType::I8, &[8, 4], false);
    let head = pb.param("head", DType::I8, &[8, 4], false);
    let hist = pb.hist_state("rows", DType::I32, &[4], 1 << 16, true);
    let carry = vec![TensorType::fixed(DType::I32, &[4])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(embed, Ref::Input(0), 0, 0);
        let x = b.cast(x, DType::I32);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let row = b.clamp(Ref::CarryIn(0), -64, 64, DType::I32);
        let rows = b.hist_append(hist, row);
        let out = body(&mut b, rows);
        let out = b.reshape_fixed(out, &[4]);
        let out = b.clamp(out, -1000, 1000, DType::I32);
        b.finish(&[out])
    };
    let (post, logits) = {
        let mut b = pb.block("post", carry);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let l = b.matmul(head, x, DType::I64);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let l = b.reshape_fixed(l, &[8]);
        let l = b.commit(l);
        let Ref::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let mut program = pb.finish(pre, vec![layer], post, logits);
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let mut params = MapParams::default();
    let fill = |n: usize, k: i128| (0..n as i128).map(|i| ((i * 37 + k) % 255) - 127).collect::<Vec<_>>();
    params.tensors.insert((0, None), Tensor::new(DType::I8, vec![8, 4], fill(32, 5)).unwrap());
    params.tensors.insert((1, None), Tensor::new(DType::I8, vec![8, 4], fill(32, 17)).unwrap());
    (program, params)
}

fn admit_body(body: impl FnOnce(&mut BlockBuilder<'_>, Ref) -> Ref) -> Result<PalwTirClassRecordV1, E> {
    let (program, tensors) = with_body(body);
    let root = root_of(&program, &tensors);
    admit(&bundle(&params()), &rules(), &registration(&program, root, 64))
}

/// `Σ_t rows[t]` — one reduction over `H`, `[1, 4]`.
fn sum(b: &mut BlockBuilder<'_>, rows: Ref) -> Ref {
    b.reduce_sum(rows, 0, DType::I32)
}

#[test]
fn the_obligations_are_refused_by_name() {
    // The control: two reductions (a maximum, then a sum against it), dissected and admitted.
    let control = admit_body(|b, rows| {
        let m = b.reduce_max(rows, 0);
        let d = b.sub(rows, m, DType::I32);
        b.reduce_sum(d, 0, DType::I32)
    })
    .expect("a max-then-sum cone is dissectable");
    assert_eq!(control.dissected.len(), 1, "its commit point is dissected");

    // O-1: seventeen reductions over H in one cone.
    let refused = admit_body(|b, rows| {
        let mut acc = sum(b, rows);
        for k in 1..17 {
            let c = b.c(DType::I32, k);
            let shifted = b.add(rows, c, DType::I32);
            let s = b.reduce_sum(shifted, 0, DType::I32);
            acc = b.add(acc, s, DType::I32);
        }
        acc
    });
    assert!(matches!(&refused, Err(E::TirDissection { why, .. }) if why.contains("at most 16")), "{refused:?}");
    // Sixteen are admitted.
    let sixteen = admit_body(|b, rows| {
        let mut acc = sum(b, rows);
        for k in 1..16 {
            let c = b.c(DType::I32, k);
            let shifted = b.add(rows, c, DType::I32);
            let s = b.reduce_sum(shifted, 0, DType::I32);
            acc = b.add(acc, s, DType::I32);
        }
        acc
    });
    assert!(sixteen.is_ok(), "{sixteen:?}");

    // O-2: a Gather whose history-carrying indices read a reduction's output.
    let refused = admit_body(|b, rows| {
        let m = b.reduce_max(rows, 0); // [1, 4]
        let idx = b.clamp(rows, 0, 3, DType::Idx); // [H, 4]
        let g = b.gather(m, idx, 1, 0); // [1, H, 4]: m[0, rows[t, j]]
        b.reduce_sum(g, 1, DType::I32)
    });
    assert!(matches!(&refused, Err(E::TirDissection { why, .. }) if why.contains("Gather")), "{refused:?}");
    // …and the same Gather over a history-free table is admitted.
    let admitted = admit_body(|b, rows| {
        let table = b.pb.konst(DType::I32, &[1, 4], &[1, -2, 3, -4]);
        let idx = b.clamp(rows, 0, 3, DType::Idx);
        let g = b.gather(table, idx, 1, 0);
        let s = b.reduce_sum(g, 1, DType::I32);
        let m = b.reduce_max(rows, 0);
        let m = b.reshape(m, &[Dim::Fixed(1), Dim::Fixed(1), Dim::Fixed(4)]);
        b.add(s, m, DType::I32)
    });
    assert!(admitted.is_ok(), "{admitted:?}");

    // O-2: a Select whose history-carrying condition chooses a reduction's output.
    let refused = admit_body(|b, rows| {
        let m = b.reduce_max(rows, 0); // [1, 4]
        let zero = b.c(DType::I32, 0);
        let c = b.compare(rows, zero, Cmp::Gt); // [H, 4]
        let v = b.select(c, m, rows, DType::I32); // [H, 4]
        b.reduce_sum(v, 0, DType::I32)
    });
    assert!(matches!(&refused, Err(E::TirDissection { why, .. }) if why.contains("Select")), "{refused:?}");
    // …but a Select whose condition reads that operand itself, at the same element, reads what its
    // condition reads whichever it chooses — a shifted softmax's clamp-by-select is admitted.
    let admitted = admit_body(|b, rows| {
        let m = b.reduce_max(rows, 0); // [1, 4]
        let d = b.sub(rows, m, DType::I32); // [H, 4]
        let floor = b.c(DType::I32, -100);
        let c = b.compare(d, floor, Cmp::Lt); // [H, 4], reads d
        let v = b.select(c, floor, d, DType::I32);
        b.reduce_sum(v, 0, DType::I32)
    });
    assert!(admitted.is_ok(), "{admitted:?}");
}
