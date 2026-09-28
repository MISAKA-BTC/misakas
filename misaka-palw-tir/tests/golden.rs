//! **The golden vectors, regenerated and byte-compared** (`consensus-vectors/tir-v1/`).
//!
//! Three kinds of file, formats in spec 04b §12:
//!
//! * `primitives/NN-Name.json` — every stateless primitive on hand-picked cases (range extremes,
//!   every rounding-rule edge: exact halves, negative halves, `i32::MIN`/`i64::MIN`/`i128::MIN`,
//!   empty-looking and broadcast shapes, every error class) plus seeded random cases;
//! * `programs/name.json` — whole programs (canonical bytes, params, tokens) with the logits and
//!   every commit point of every position, and cone-evaluation cases; the state primitives
//!   (`StateWrite`, `HistAppend`) are pinned here, where they have a meaning;
//! * `encoding.json` — byte strings `TirProgramV1::decode_canonical` must accept or refuse.
//!
//! The test regenerates every file in memory and requires the bytes on disk to be identical.
//! `TIR_BLESS=1 cargo test -p misaka-palw-tir --test golden` rewrites them — which is a change of
//! the vectors and must be reviewed as a change of the semantics.

mod common;

use std::path::PathBuf;

use common::Lcg;
use common::models::*;
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::eval::eval_primitive;
use misaka_palw_tir::interp::CommitRecord;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_POS, INPUT_TOKEN};
use misaka_palw_tir::{
    Cmp, ConeEnv, DType, Dim, Interpreter, MapParams, Prim, Ref, Rounding, RunState, Tensor, TensorType, TirProgramV1,
};
use serde::Serialize;

const FORMAT_PRIM: &str = "palw-tir-v1/primitive-vectors/1";
const FORMAT_PROGRAM: &str = "palw-tir-v1/program-vectors/3";
/// The key of `graph_ir_root` (spec 04b §3.6), the one `palw_tir_class_v1` hashes under.
const GRAPH_IR_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/tir/graph-ir-root/v1";
const FORMAT_ENCODING: &str = "palw-tir-v1/encoding-vectors/1";
const FORMAT_ADMISSION: &str = "palw-tir-v1/admission-vectors/1";
const SPEC: &str = "docs/spec/palw/04b-tensor-ir.md";

// ---- JSON shapes (struct field order is the file's key order) ---------------------------------

#[derive(Serialize)]
struct TensorJson {
    dtype: String,
    shape: Vec<u64>,
    data: Vec<String>,
}

#[derive(Serialize)]
struct TypeJson {
    dtype: String,
    shape: Vec<u64>,
}

#[derive(Serialize)]
struct PrimJson {
    name: String,
    attrs: Vec<(String, String)>,
    borsh_hex: String,
}

#[derive(Serialize)]
struct CaseJson {
    name: String,
    prim: PrimJson,
    inputs: Vec<TensorJson>,
    out: TypeJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    expect: Option<TensorJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expect_error: Option<String>,
}

#[derive(Serialize)]
struct PrimFileJson {
    format: String,
    spec: String,
    primitive: String,
    tag: u8,
    cases: Vec<CaseJson>,
}

#[derive(Serialize)]
struct ParamJson {
    param: u16,
    layer: Option<u16>,
    le_hex: String,
}

#[derive(Serialize)]
struct CommitJson {
    slot: u32,
    block: u8,
    layer: Option<u16>,
    node: u16,
    value: TensorJson,
}

#[derive(Serialize)]
struct StepJson {
    pos: u32,
    token: u32,
    logits: TensorJson,
    commits: Vec<CommitJson>,
}

#[derive(Serialize)]
struct NamedTensorJson {
    index: u16,
    value: TensorJson,
}

#[derive(Serialize)]
struct HistJson {
    state: u16,
    rows: Vec<TensorJson>,
}

#[derive(Serialize)]
struct ConeJson {
    block: u8,
    layer: Option<u16>,
    target: u16,
    token: u32,
    pos: u32,
    carry_in: Vec<NamedTensorJson>,
    fixed: Vec<NamedTensorJson>,
    hist_prior: Vec<HistJson>,
    supplied: Vec<NamedTensorJson>,
    expect: TensorJson,
}

/// A cone the evaluator must REFUSE (spec 04b §9.2): an honest environment with one defect, and
/// the class of the refusal (§9.3's table).
#[derive(Serialize)]
struct RefusalJson {
    what: String,
    block: u8,
    layer: Option<u16>,
    target: u16,
    token: Option<u32>,
    pos: u32,
    carry_in: Vec<NamedTensorJson>,
    fixed: Vec<NamedTensorJson>,
    hist_prior: Vec<HistJson>,
    supplied: Vec<NamedTensorJson>,
    expect_error: String,
}

#[derive(Serialize)]
struct ProgramFileJson {
    format: String,
    spec: String,
    name: String,
    description: String,
    program_borsh_hex: String,
    graph_ir_root_hex: String,
    params: Vec<ParamJson>,
    steps: Vec<StepJson>,
    cones: Vec<ConeJson>,
    refusals: Vec<RefusalJson>,
}

#[derive(Serialize)]
struct EncodingCaseJson {
    name: String,
    hex: String,
    expect: String,
}

#[derive(Serialize)]
struct EncodingFileJson {
    format: String,
    spec: String,
    cases: Vec<EncodingCaseJson>,
}

/// `graph_ir_root` (spec 04b §3.6): BLAKE2b-512 keyed by [`GRAPH_IR_ROOT_DOMAIN_V1`] over the
/// canonical bytes. The crate itself never hashes (identity stays with the caller); the vectors pin it.
fn graph_ir_root(bytes: &[u8]) -> blake2b_simd::Hash {
    blake2b_simd::Params::new().hash_length(64).key(GRAPH_IR_ROOT_DOMAIN_V1).hash(bytes)
}

// ---- admission vectors (spec 04b §10.3, §12) ------------------------------------------------------

#[derive(Serialize)]
struct CostJson {
    macs: String,
    elementwise: String,
    transcendentals: String,
    bytes_read: String,
    bytes_written: String,
}

fn cost_json(c: &misaka_palw_tir::admit::CostV1) -> CostJson {
    CostJson {
        macs: c.macs.to_string(),
        elementwise: c.elementwise.to_string(),
        transcendentals: c.transcendentals.to_string(),
        bytes_read: c.bytes_read.to_string(),
        bytes_written: c.bytes_written.to_string(),
    }
}

#[derive(Serialize)]
struct CeilingsJson {
    max_tile_macs: String,
    max_tile_transcendentals: String,
    max_tile_opened_bytes: String,
    max_tile_operands: String,
    max_position_macs: String,
    max_position_transcendentals: String,
    max_state_bytes: String,
    max_step_leaves: String,
    max_checkpoint_interval: String,
    max_cone_work: String,
}

#[derive(Serialize)]
struct AdmitInputsJson {
    tile_len: String,
    h_chunk: String,
    ceilings: CeilingsJson,
}

#[derive(Serialize)]
struct PositionJson {
    cost: CostJson,
    state_bytes: String,
    peak_live_bytes: String,
    commit_lanes: String,
    step_leaves: String,
}

#[derive(Serialize)]
struct StateCkptJson {
    state: String,
    closure: Vec<String>,
    groups: String,
    per_position: CostJson,
    interval: String,
}

#[derive(Serialize)]
struct ConeCaseJson {
    block: String,
    node: String,
    nodes: Vec<String>,
    leaves: Vec<String>,
    whole: CostJson,
    tiles: String,
    tile: CostJson,
    tile_opened_bytes: String,
    operands: String,
    h_reductions: Vec<String>,
    chunk: Option<CostJson>,
    chunk_opened_bytes: Option<String>,
}

#[derive(Serialize)]
struct AdmissionJson {
    checkpoint_interval: String,
    cone_work: String,
    position: PositionJson,
    states: Vec<StateCkptJson>,
    cones: Vec<ConeCaseJson>,
    node_costs: Vec<Vec<CostJson>>,
    intervals: Vec<Vec<[String; 2]>>,
}

#[derive(Serialize)]
struct RefusalCaseJson {
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cap: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    class: Option<String>,
}

#[derive(Serialize)]
struct AdmissionCaseJson {
    name: String,
    description: String,
    program_borsh_hex: String,
    inputs: AdmitInputsJson,
    expect: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    refusal: Option<RefusalCaseJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    admission: Option<AdmissionJson>,
}

#[derive(Serialize)]
struct AdmissionFileJson {
    format: String,
    spec: String,
    cases: Vec<AdmissionCaseJson>,
}

fn leaf_text(l: &misaka_palw_tir::admit::LeafV1) -> String {
    use misaka_palw_tir::admit::LeafV1 as L;
    match l {
        L::Commit(n) => format!("commit:{n}"),
        L::CarryIn(k) => format!("carry_in:{k}"),
        L::State(s) => format!("state:{s}"),
        L::History(s) => format!("history:{s}"),
        L::Param(j) => format!("param:{j}"),
        L::Const(j) => format!("const:{j}"),
        L::Input(j) => format!("input:{j}"),
    }
}

fn admission_case(
    name: &str,
    description: &str,
    program: &TirProgramV1,
    inputs: misaka_palw_tir::admit::TirAdmitInputsV1,
) -> AdmissionCaseJson {
    use misaka_palw_tir::admit::{TirAdmitError, tir_admit_v1};
    let bytes = program.encode();
    let c = &inputs.ceilings;
    let inputs_json = AdmitInputsJson {
        tile_len: inputs.tile_len.to_string(),
        h_chunk: inputs.h_chunk.to_string(),
        ceilings: CeilingsJson {
            max_tile_macs: c.max_tile_macs.to_string(),
            max_tile_transcendentals: c.max_tile_transcendentals.to_string(),
            max_tile_opened_bytes: c.max_tile_opened_bytes.to_string(),
            max_tile_operands: c.max_tile_operands.to_string(),
            max_position_macs: c.max_position_macs.to_string(),
            max_position_transcendentals: c.max_position_transcendentals.to_string(),
            max_state_bytes: c.max_state_bytes.to_string(),
            max_step_leaves: c.max_step_leaves.to_string(),
            max_checkpoint_interval: c.max_checkpoint_interval.to_string(),
            max_cone_work: c.max_cone_work.to_string(),
        },
    };
    let (expect, refusal, admission) = match tir_admit_v1(&bytes, &inputs) {
        Ok(a) => {
            let admission = AdmissionJson {
                checkpoint_interval: a.checkpoint_interval.to_string(),
                cone_work: a.cone_work.to_string(),
                position: PositionJson {
                    cost: cost_json(&a.position.cost),
                    state_bytes: a.position.state_bytes.to_string(),
                    peak_live_bytes: a.position.peak_live_bytes.to_string(),
                    commit_lanes: a.position.commit_lanes.to_string(),
                    step_leaves: a.position.step_leaves.to_string(),
                },
                states: a
                    .states
                    .iter()
                    .map(|s| StateCkptJson {
                        state: s.state.to_string(),
                        closure: s.closure.iter().map(|t| t.to_string()).collect(),
                        groups: s.groups.to_string(),
                        per_position: cost_json(&s.per_position),
                        interval: s.interval.to_string(),
                    })
                    .collect(),
                cones: a
                    .cones
                    .iter()
                    .map(|k| ConeCaseJson {
                        block: k.block.to_string(),
                        node: k.node.to_string(),
                        nodes: k.nodes.iter().map(|n| n.to_string()).collect(),
                        leaves: k.leaves.iter().map(leaf_text).collect(),
                        whole: cost_json(&k.whole),
                        tiles: k.tiles.to_string(),
                        tile: cost_json(&k.tile),
                        tile_opened_bytes: k.tile_opened_bytes.to_string(),
                        operands: k.operands.to_string(),
                        h_reductions: k.h_reductions.iter().map(|n| n.to_string()).collect(),
                        chunk: k.chunk.as_ref().map(cost_json),
                        chunk_opened_bytes: k.chunk_opened_bytes.map(|b| b.to_string()),
                    })
                    .collect(),
                node_costs: a.node_costs.iter().map(|b| b.iter().map(cost_json).collect()).collect(),
                intervals: a.intervals.iter().map(|b| b.iter().map(|i| [i.lo.to_string(), i.hi.to_string()]).collect()).collect(),
            };
            ("admitted".to_string(), None, Some(admission))
        }
        Err(TirAdmitError::Exceeds { limit, value, cap, .. }) => (
            "refused".to_string(),
            Some(RefusalCaseJson {
                kind: "exceeds".into(),
                limit: Some(limit.to_string()),
                value: Some(value.to_string()),
                cap: Some(cap.to_string()),
                class: None,
            }),
            None,
        ),
        Err(TirAdmitError::Program(e)) => (
            "refused".to_string(),
            Some(RefusalCaseJson {
                kind: "program".into(),
                limit: None,
                value: None,
                cap: None,
                class: Some(format!("{:?}", e.kind)),
            }),
            None,
        ),
        Err(TirAdmitError::Inputs(_)) => (
            "refused".to_string(),
            Some(RefusalCaseJson { kind: "inputs".into(), limit: None, value: None, cap: None, class: None }),
            None,
        ),
    };
    AdmissionCaseJson {
        name: name.into(),
        description: description.into(),
        program_borsh_hex: hex(&bytes),
        inputs: inputs_json,
        expect,
        refusal,
        admission,
    }
}

fn admission_file() -> (String, String) {
    use common::admission::*;
    use misaka_palw_tir::admit::{TirAdmitInputsV1, TirCeilingsV1};
    let base = TirAdmitInputsV1 { tile_len: 64, h_chunk: 16, ceilings: TirCeilingsV1::legacy_court_v1() };
    let with = |f: &dyn Fn(&mut TirCeilingsV1)| {
        let mut c = base.ceilings;
        f(&mut c);
        TirAdmitInputsV1 { ceilings: c, ..base }
    };
    let dense2 = dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]).0;
    let cases = vec![
        admission_case("dense-gqa-2layer", "the corpus's dense GQA decoder, two global layers", &dense2, base),
        admission_case("sliding-global", "sliding + global attention", &dense(&[3, HISTORY_BOUND_V1_SMALL, 3]).0, base),
        admission_case(
            "gdn-k2-v4-grouped",
            "GDN, 2 key / 4 value heads, grouping; the conv window replays with S (G = 1)",
            &gdn_program(true).0,
            base,
        ),
        admission_case("mamba2", "Mamba-2's selective scan", &mamba2_program().0, base),
        admission_case("moe-top2-shared", "top-2 MoE with a shared expert", &moe_program().0, base),
        admission_case(
            "head-local-delta-rule",
            "a delta rule whose per-position operands are committed: S splits per head",
            &head_local_delta_rule(8, 16, 16),
            base,
        ),
        admission_case(
            "a1-free-update",
            "S <- Clamp(P): every node free, four groups, each paying the free nodes whole",
            &free_update(),
            base,
        ),
        admission_case(
            "a1-free-node-in-an-aligned-update",
            "S <- S + Clamp(P): ceil(aligned / 4) + free",
            &free_node_in_an_aligned_update(),
            base,
        ),
        admission_case(
            "a1-a-committed-member-write-is-a-free-leaf",
            "S2 <- S2 + S1 + Reshape(Transpose(w1)) with w1 = S1's committed StateWrite: a free leaf, so S2 splits",
            &a_committed_member_write_is_a_free_leaf(),
            base,
        ),
        admission_case(
            "a1-a-reduction-across-groups",
            "S <- Broadcast(ReduceSum(S, 0)): mixed, G = 1",
            &a_reduction_across_groups(),
            base,
        ),
        admission_case(
            "a1-a-member-of-another-width-a7-a-state-nobody-writes",
            "S's closure holds T[2, 8] (G = 1); T is never written: no C_j",
            &a_member_of_another_width_and_a_state_nobody_writes(),
            base,
        ),
        admission_case(
            "a1-free-nodes-are-paid-whole",
            "S <- Clamp(P1 . P2): the free product, 512 MACs whole in every group, against 256: refused",
            &a_free_product_update(),
            with(&|c| c.max_tile_macs = 256),
        ),
        admission_case(
            "a1-the-commit-point-rule-decides-the-split",
            "S2's closure {S1, S2}: 512 aligned MACs, 128 a group because w1 is a free leaf, against 128 (16-lane tiles): C_j = 1",
            &a_committed_member_write_decides_the_split(),
            TirAdmitInputsV1 { tile_len: 16, ..with(&|c| c.max_tile_macs = 128) },
        ),
        admission_case(
            "a3-a-replay-at-the-mac-ceiling",
            "64 MACs a group against a cap of 64: C_j = 1",
            &a_replay_of_matmuls(),
            with(&|c| c.max_tile_macs = 64),
        ),
        admission_case(
            "a3-a-replay-past-the-mac-ceiling",
            "64 MACs a group against 63: max_tile_macs",
            &a_replay_of_matmuls(),
            with(&|c| c.max_tile_macs = 63),
        ),
        admission_case(
            "a3-a-replay-past-the-transcendental-ceiling",
            "4 transcendentals a group against 3 (no MACs): max_tile_transcendentals",
            &a_replay_of_transcendentals(),
            with(&|c| c.max_tile_transcendentals = 3),
        ),
        admission_case(
            "a3-a-zero-interval-cap",
            "max_checkpoint_interval = 0 is out of range",
            &free_update(),
            with(&|c| c.max_checkpoint_interval = 0),
        ),
        admission_case("a2-max-tile-macs", "the dense LM head tile past 1,000 MACs", &dense2, with(&|c| c.max_tile_macs = 1_000)),
        admission_case("a2-max-cone-work", "admission's own work past 10", &dense2, with(&|c| c.max_cone_work = 10)),
        admission_case("a2-max-step-leaves", "one position's commit tiles past 2", &dense2, with(&|c| c.max_step_leaves = 2)),
        admission_case("a2-max-position-macs", "one position's MACs past 1,000", &dense2, with(&|c| c.max_position_macs = 1_000)),
        admission_case(
            "a2-max-position-transcendentals",
            "one position's transcendentals past 100",
            &dense2,
            with(&|c| c.max_position_transcendentals = 100),
        ),
        admission_case("a2-max-state-bytes", "the run's state past 1,000 bytes", &dense2, with(&|c| c.max_state_bytes = 1_000)),
        admission_case(
            "a2-max-tile-transcendentals",
            "a softmax tile's exponentials past 10",
            &dense2,
            with(&|c| c.max_tile_transcendentals = 10),
        ),
        admission_case("a2-max-tile-opened-bytes", "a tile opening past 100 bytes", &dense2, with(&|c| c.max_tile_opened_bytes = 100)),
        admission_case(
            "a2-max-tile-operands",
            "a cone reading more than one committed operand",
            &dense2,
            with(&|c| c.max_tile_operands = 1),
        ),
        admission_case("a6-overflow", "i32 x i32 into i32", &an_overflow(), base),
        admission_case("a6-index", "a 4-row table gathered by the position", &an_index_out_of_range(), base),
        admission_case("a6-divisor", "a divisor interval reaching below 1", &a_divisor_below_one(), base),
    ];
    let file = AdmissionFileJson { format: FORMAT_ADMISSION.into(), spec: SPEC.into(), cases };
    ("admission.json".into(), serde_json::to_string_pretty(&file).unwrap() + "\n")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn tj(t: &Tensor) -> TensorJson {
    TensorJson {
        dtype: t.dtype.name().into(),
        shape: t.shape.iter().map(|d| *d as u64).collect(),
        data: t.data.iter().map(|v| v.to_string()).collect(),
    }
}

fn prim_json(p: &Prim) -> PrimJson {
    let attrs: Vec<(String, String)> = match p {
        Prim::Transpose { perm } => vec![("perm".into(), format!("{perm:?}"))],
        Prim::Slice { axis, start } => vec![("axis".into(), axis.to_string()), ("start".into(), start.to_string())],
        Prim::Concat { axis } | Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => vec![("axis".into(), axis.to_string())],
        Prim::Iota { axis, start, step } => {
            vec![("axis".into(), axis.to_string()), ("start".into(), start.to_string()), ("step".into(), step.to_string())]
        }
        Prim::Gather { axis, batch_dims } => vec![("axis".into(), axis.to_string()), ("batch_dims".into(), batch_dims.to_string())],
        Prim::Div { rule } => vec![("rule".into(), rule.name().into())],
        Prim::Clamp { lo, hi } => vec![("lo".into(), lo.to_string()), ("hi".into(), hi.to_string())],
        Prim::Compare { cmp } => vec![("cmp".into(), cmp.name().into())],
        Prim::TopK { axis, k } => vec![("axis".into(), axis.to_string()), ("k".into(), k.to_string())],
        Prim::StateWrite { state } | Prim::HistAppend { state } => vec![("state".into(), state.to_string())],
        _ => vec![],
    };
    PrimJson { name: p.name().into(), attrs, borsh_hex: hex(&borsh::to_vec(p).unwrap()) }
}

fn t(dtype: DType, shape: &[usize], data: &[i128]) -> Tensor {
    Tensor::new(dtype, shape.to_vec(), data.to_vec()).expect("a vector's operand is a tensor of its dtype")
}

struct Case {
    name: &'static str,
    prim: Prim,
    inputs: Vec<Tensor>,
    out: (DType, Vec<usize>),
}

fn case(name: &'static str, prim: Prim, inputs: Vec<Tensor>, dtype: DType, shape: &[usize]) -> Case {
    Case { name, prim, inputs, out: (dtype, shape.to_vec()) }
}

fn run_case(c: &Case) -> CaseJson {
    let r = eval_primitive(&c.prim, &c.inputs, c.out.0, &c.out.1);
    CaseJson {
        name: c.name.into(),
        prim: prim_json(&c.prim),
        inputs: c.inputs.iter().map(tj).collect(),
        out: TypeJson { dtype: c.out.0.name().into(), shape: c.out.1.iter().map(|d| *d as u64).collect() },
        expect: r.as_ref().ok().map(tj),
        expect_error: r.err().map(|e| format!("{:?}", e.kind)),
    }
}

// ---- the per-primitive cases ------------------------------------------------------------------

const I32MIN: i128 = i32::MIN as i128;
const I32MAX: i128 = i32::MAX as i128;
const I64MIN: i128 = i64::MIN as i128;
const I64MAX: i128 = i64::MAX as i128;

fn random_i(rng: &mut Lcg, d: DType, n: usize) -> Vec<i128> {
    let (lo, hi) = (d.min_value().max(-(1i128 << 100)), d.max_value().min(1i128 << 100));
    (0..n).map(|_| rng.range(lo, hi)).collect()
}

fn cases_for(tag: u8) -> Vec<Case> {
    use DType::*;
    let mut rng = Lcg(0x071b_0000 + tag as u64);
    let r = &mut rng;
    match tag {
        0 => vec![
            case("matrix_to_transpose_shape", Prim::Reshape, vec![t(I32, &[2, 3], &[1, 2, 3, 4, 5, 6])], I32, &[3, 2]),
            case("scalar_to_one", Prim::Reshape, vec![t(I8, &[], &[-128])], I8, &[1]),
            case("flat_to_rank3", Prim::Reshape, vec![t(I16, &[6], &[-32768, 1, 2, 3, 4, 32767])], I16, &[1, 2, 3]),
            case("rank4", Prim::Reshape, vec![t(I32, &[2, 2, 2, 1], &[1, 2, 3, 4, 5, 6, 7, 8])], I32, &[4, 2]),
            case("error_element_count", Prim::Reshape, vec![t(I32, &[2, 3], &[1, 2, 3, 4, 5, 6])], I32, &[4]),
            case("error_dtype_change", Prim::Reshape, vec![t(I32, &[2], &[1, 2])], I64, &[2]),
        ],
        1 => vec![
            case("2d", Prim::Transpose { perm: vec![1, 0] }, vec![t(I32, &[2, 3], &[1, 2, 3, 4, 5, 6])], I32, &[3, 2]),
            case(
                "3d_rotate",
                Prim::Transpose { perm: vec![2, 0, 1] },
                vec![t(I16, &[2, 3, 2], &(0..12).collect::<Vec<_>>())],
                I16,
                &[2, 2, 3],
            ),
            case(
                "4d",
                Prim::Transpose { perm: vec![3, 1, 0, 2] },
                vec![t(I8, &[2, 1, 3, 2], &(0..12).map(|v| v - 6).collect::<Vec<_>>())],
                I8,
                &[2, 1, 2, 3],
            ),
            case("identity", Prim::Transpose { perm: vec![0, 1] }, vec![t(Idx, &[1, 2], &[0, 4294967295])], Idx, &[1, 2]),
            case("error_not_a_permutation", Prim::Transpose { perm: vec![0, 0] }, vec![t(I32, &[2, 2], &[1, 2, 3, 4])], I32, &[2, 2]),
            case("error_wrong_shape", Prim::Transpose { perm: vec![1, 0] }, vec![t(I32, &[2, 3], &[1, 2, 3, 4, 5, 6])], I32, &[2, 3]),
        ],
        2 => vec![
            case("middle", Prim::Slice { axis: 0, start: 1 }, vec![t(I32, &[5], &[10, 11, 12, 13, 14])], I32, &[3]),
            case("inner_axis", Prim::Slice { axis: 1, start: 2 }, vec![t(I32, &[2, 4], &(0..8).collect::<Vec<_>>())], I32, &[2, 2]),
            case("whole", Prim::Slice { axis: 0, start: 0 }, vec![t(I8, &[3], &[-128, 0, 127])], I8, &[3]),
            case("last_element", Prim::Slice { axis: 0, start: 4 }, vec![t(I64, &[5], &[I64MIN, 0, 0, 0, I64MAX])], I64, &[1]),
            case("error_past_the_end", Prim::Slice { axis: 0, start: 3 }, vec![t(I32, &[5], &[1, 2, 3, 4, 5])], I32, &[3]),
        ],
        3 => vec![
            case("two_axis0", Prim::Concat { axis: 0 }, vec![t(I32, &[2], &[1, 2]), t(I32, &[3], &[3, 4, 5])], I32, &[5]),
            case(
                "three_axis1",
                Prim::Concat { axis: 1 },
                vec![t(I16, &[2, 1], &[1, 2]), t(I16, &[2, 2], &[3, 4, 5, 6]), t(I16, &[2, 1], &[7, 8])],
                I16,
                &[2, 4],
            ),
            case("error_dtype_mismatch", Prim::Concat { axis: 0 }, vec![t(I32, &[1], &[1]), t(I16, &[1], &[2])], I32, &[2]),
            case(
                "error_other_axis_differs",
                Prim::Concat { axis: 0 },
                vec![t(I32, &[1, 2], &[1, 2]), t(I32, &[1, 3], &[3, 4, 5])],
                I32,
                &[2, 2],
            ),
            // Nine operands: past the eight a node may have, and alone that is `Shape` (§9.3).
            case("error_arity_9", Prim::Concat { axis: 0 }, (1..=9).map(|v| t(I32, &[1], &[v])).collect(), I32, &[9]),
        ],
        4 => vec![
            case("row_to_matrix", Prim::Broadcast, vec![t(I32, &[3], &[1, 2, 3])], I32, &[2, 3]),
            case("column_expand", Prim::Broadcast, vec![t(I32, &[2, 1], &[7, -7])], I32, &[2, 4]),
            case("scalar", Prim::Broadcast, vec![t(I8, &[], &[-128])], I8, &[3]),
            case("head_grouping", Prim::Broadcast, vec![t(I16, &[2, 1, 2], &[1, 2, 3, 4])], I16, &[2, 3, 2]),
            case("error_not_size_one", Prim::Broadcast, vec![t(I32, &[2], &[1, 2])], I32, &[3]),
        ],
        5 => vec![
            case("ascending", Prim::Iota { axis: 0, start: -2, step: 3 }, vec![], I32, &[4]),
            case("inner_axis", Prim::Iota { axis: 1, start: 5, step: -1 }, vec![], I64, &[2, 3]),
            case("fill_step_zero", Prim::Iota { axis: 0, start: 9, step: 0 }, vec![], I16, &[3]),
            case("idx_positions", Prim::Iota { axis: 0, start: 0, step: 1 }, vec![], Idx, &[5]),
            case("error_overflow_i8", Prim::Iota { axis: 0, start: 120, step: 5 }, vec![], I8, &[4]),
            case("error_negative_idx", Prim::Iota { axis: 0, start: 1, step: -1 }, vec![], Idx, &[3]),
        ],
        6 => vec![
            case(
                "embedding_row",
                Prim::Gather { axis: 0, batch_dims: 0 },
                vec![t(I8, &[4, 3], &(0..12).map(|v| v * 10 - 60).collect::<Vec<_>>()), t(Idx, &[], &[2])],
                I8,
                &[3],
            ),
            case(
                "two_rows",
                Prim::Gather { axis: 0, batch_dims: 0 },
                vec![t(I32, &[3, 2], &[1, 2, 3, 4, 5, 6]), t(I32, &[2], &[2, 0])],
                I32,
                &[2, 2],
            ),
            case(
                "inner_axis",
                Prim::Gather { axis: 1, batch_dims: 0 },
                vec![t(I32, &[2, 3], &[1, 2, 3, 4, 5, 6]), t(Idx, &[2], &[2, 2])],
                I32,
                &[2, 2],
            ),
            case(
                "batch_dims_1",
                Prim::Gather { axis: 1, batch_dims: 1 },
                vec![t(I32, &[2, 3], &[1, 2, 3, 4, 5, 6]), t(I64, &[2, 2], &[0, 2, 1, 1])],
                I32,
                &[2, 2],
            ),
            case(
                "table_lookup",
                Prim::Gather { axis: 0, batch_dims: 0 },
                vec![t(I64, &[4], &[1, 2, 4, 8]), t(I8, &[2, 2], &[3, 0, 1, 2])],
                I64,
                &[2, 2],
            ),
            case(
                "error_index_past_the_end",
                Prim::Gather { axis: 0, batch_dims: 0 },
                vec![t(I32, &[3], &[1, 2, 3]), t(I32, &[1], &[3])],
                I32,
                &[1],
            ),
            case(
                "error_negative_index",
                Prim::Gather { axis: 0, batch_dims: 0 },
                vec![t(I32, &[3], &[1, 2, 3]), t(I32, &[1], &[-1])],
                I32,
                &[1],
            ),
        ],
        7 => vec![
            case("narrow_fits", Prim::Cast, vec![t(I32, &[3], &[-128, 0, 127])], I8, &[3]),
            case("widen", Prim::Cast, vec![t(I8, &[2], &[-128, 127])], I128, &[2]),
            case("to_idx", Prim::Cast, vec![t(I64, &[2], &[0, 4294967295])], Idx, &[2]),
            case("error_narrow_overflow", Prim::Cast, vec![t(I32, &[2], &[127, 128])], I8, &[2]),
            case("error_negative_to_idx", Prim::Cast, vec![t(I32, &[1], &[-1])], Idx, &[1]),
        ],
        8..=10 => {
            let p = [Prim::Add, Prim::Sub, Prim::Mul][tag as usize - 8].clone();
            let mut v = vec![
                case(
                    "broadcast_row",
                    p.clone(),
                    vec![t(I32, &[2, 3], &[1, -2, 3, -4, 5, -6]), t(I32, &[3], &[10, 20, 30])],
                    I64,
                    &[2, 3],
                ),
                case(
                    "i32_extremes_in_i64",
                    p.clone(),
                    vec![t(I32, &[4], &[I32MIN, I32MIN, I32MAX, 0]), t(I32, &[4], &[I32MIN, -1, I32MAX, I32MIN])],
                    I64,
                    &[4],
                ),
                case(
                    "i64_extremes_in_i128",
                    p.clone(),
                    vec![t(I64, &[3], &[I64MIN, I64MAX, I64MIN]), t(I64, &[3], &[I64MIN, I64MAX, I64MAX])],
                    I128,
                    &[3],
                ),
                case(
                    "error_overflow_i32",
                    p.clone(),
                    vec![t(I32, &[2], &[I32MIN, I32MAX]), t(I32, &[2], &[I32MIN, I32MAX])],
                    I32,
                    &[2],
                ),
                case("error_overflow_i128", p.clone(), vec![t(I128, &[1], &[i128::MAX]), t(I128, &[1], &[i128::MAX])], I128, &[1]),
                case("error_no_broadcast", p.clone(), vec![t(I32, &[2], &[1, 2]), t(I32, &[3], &[1, 2, 3])], I32, &[3]),
                // Outside a program there is no normal form: a wrong operand count is the type
                // rule's failure (`Shape`, spec 04b §9.3), not NF-14's.
                case("error_arity_1", p.clone(), vec![t(I32, &[2], &[1, 2])], I32, &[2]),
            ];
            let a = random_i(r, I32, 32);
            let b = random_i(r, I32, 32);
            v.push(case("random_i32_into_i64", p.clone(), vec![t(I32, &[32], &a), t(I32, &[32], &b)], I64, &[32]));
            let a = random_i(r, I64, 16);
            let b = random_i(r, I64, 16);
            v.push(case("random_i64_into_i128", p, vec![t(I64, &[16], &a), t(I64, &[16], &b)], I128, &[16]));
            v
        }
        11 => {
            let mut v = vec![
                case(
                    "small",
                    Prim::MatMul,
                    vec![t(I8, &[2, 3], &[1, 2, 3, 4, 5, 6]), t(I16, &[3, 2], &[1, -1, 2, -2, 3, -3])],
                    I32,
                    &[2, 2],
                ),
                case(
                    "batch_broadcast",
                    Prim::MatMul,
                    vec![t(I8, &[2, 2, 3], &(0..12).map(|v| v - 6).collect::<Vec<_>>()), t(I16, &[3, 1], &[100, -200, 300])],
                    I64,
                    &[2, 2, 1],
                ),
                case(
                    "i8_extremes",
                    Prim::MatMul,
                    vec![t(I8, &[1, 4], &[-128, -128, 127, -128]), t(I8, &[4, 1], &[-128, -128, 127, -128])],
                    I32,
                    &[1, 1],
                ),
                // The order-free rule: the total fits i32, but the sum of the positive terms does
                // not — every partial sum in every order must fit.
                case(
                    "error_positive_terms_overflow_although_the_total_fits",
                    Prim::MatMul,
                    vec![t(I32, &[1, 3], &[1 << 30, 1 << 30, -(1 << 30)]), t(I32, &[3, 1], &[1, 1, 1])],
                    I32,
                    &[1, 1],
                ),
                case(
                    "error_negative_terms_overflow",
                    Prim::MatMul,
                    vec![t(I32, &[1, 3], &[I32MIN, -1, 5]), t(I32, &[3, 1], &[1, 1, 1])],
                    I32,
                    &[1, 1],
                ),
                case("error_contraction_mismatch", Prim::MatMul, vec![t(I8, &[2, 3], &[0; 6]), t(I8, &[2, 2], &[0; 4])], I32, &[2, 2]),
            ];
            let a = random_i(r, I8, 4 * 8);
            let b = random_i(r, I16, 8 * 3);
            v.push(case("random_i8_i16", Prim::MatMul, vec![t(I8, &[4, 8], &a), t(I16, &[8, 3], &b)], I64, &[4, 3]));
            v
        }
        12 => vec![
            case("axis0", Prim::ReduceSum { axis: 0 }, vec![t(I32, &[2, 3], &[1, 2, 3, 4, 5, 6])], I64, &[1, 3]),
            case("axis1", Prim::ReduceSum { axis: 1 }, vec![t(I32, &[2, 3], &[1, 2, 3, 4, 5, 6])], I64, &[2, 1]),
            case("exact_i8_fit", Prim::ReduceSum { axis: 0 }, vec![t(I8, &[2], &[127, -128])], I8, &[1]),
            case("error_positive_partial_sum", Prim::ReduceSum { axis: 0 }, vec![t(I8, &[3], &[100, 100, -100])], I8, &[1]),
            case("i128_wide", Prim::ReduceSum { axis: 0 }, vec![t(I64, &[3], &[I64MAX, I64MAX, I64MIN])], I128, &[1]),
        ],
        13 => vec![
            case("axis1", Prim::ReduceMax { axis: 1 }, vec![t(I32, &[2, 3], &[1, 9, 3, -4, -5, -6])], I32, &[2, 1]),
            case("ties_and_extremes", Prim::ReduceMax { axis: 0 }, vec![t(I64, &[3], &[I64MIN, I64MIN, I64MIN])], I64, &[1]),
            case("error_dtype_change", Prim::ReduceMax { axis: 0 }, vec![t(I32, &[2], &[1, 2])], I64, &[1]),
        ],
        14 => {
            let mut v = Vec::new();
            let xs: Vec<i128> = vec![-7, -6, -5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 7, -64, -63, 64, 63];
            for (rule, rn) in [(Rounding::Floor, "floor"), (Rounding::HalfUp, "half_up"), (Rounding::HalfAwayFromZero, "half_away")] {
                let name: &'static str = Box::leak(format!("{rn}_small_by_2").into_boxed_str());
                v.push(case(name, Prim::Div { rule }, vec![t(I32, &[xs.len()], &xs), t(I32, &[], &[2])], I32, &[xs.len()]));
                let name: &'static str = Box::leak(format!("{rn}_small_by_4").into_boxed_str());
                v.push(case(name, Prim::Div { rule }, vec![t(I32, &[xs.len()], &xs), t(I32, &[], &[4])], I32, &[xs.len()]));
                let name: &'static str = Box::leak(format!("{rn}_by_3_no_ties").into_boxed_str());
                v.push(case(name, Prim::Div { rule }, vec![t(I32, &[xs.len()], &xs), t(I32, &[], &[3])], I32, &[xs.len()]));
                let name: &'static str = Box::leak(format!("{rn}_extremes").into_boxed_str());
                v.push(case(
                    name,
                    Prim::Div { rule },
                    vec![
                        t(I128, &[6], &[I32MIN, I64MIN, i128::MIN, i128::MIN, i128::MAX, I64MAX]),
                        t(I128, &[6], &[1 << 31, 1 << 62, 1, 2, i128::MAX, 1 << 1]),
                    ],
                    I128,
                    &[6],
                ));
                let name: &'static str = Box::leak(format!("{rn}_srdhm_halves").into_boxed_str());
                // a·b with b = 2^30: the exact halves of SRDHM's `/ 2^31`.
                let p: Vec<i128> = (-5..=5).map(|a| a * (1 << 30)).collect();
                v.push(case(name, Prim::Div { rule }, vec![t(I64, &[p.len()], &p), t(I64, &[], &[1 << 31])], I64, &[p.len()]));
            }
            v.push(case(
                "error_divisor_zero",
                Prim::Div { rule: Rounding::Floor },
                vec![t(I32, &[1], &[5]), t(I32, &[1], &[0])],
                I32,
                &[1],
            ));
            v.push(case(
                "error_divisor_negative",
                Prim::Div { rule: Rounding::HalfUp },
                vec![t(I32, &[1], &[5]), t(I32, &[1], &[-2])],
                I32,
                &[1],
            ));
            v.push(case(
                "error_result_does_not_fit",
                Prim::Div { rule: Rounding::Floor },
                vec![t(I64, &[1], &[1 << 40]), t(I64, &[1], &[2])],
                I32,
                &[1],
            ));
            let a = random_i(r, I64, 24);
            let d: Vec<i128> = (0..24).map(|i| 1i128 << (i % 40)).collect();
            v.push(case(
                "random_by_powers_of_two_half_away",
                Prim::Div { rule: Rounding::HalfAwayFromZero },
                vec![t(I64, &[24], &a), t(I64, &[24], &d)],
                I64,
                &[24],
            ));
            v
        }
        15 => vec![
            case("to_i8", Prim::Clamp { lo: -128, hi: 127 }, vec![t(I32, &[5], &[-1000, -128, 0, 127, 1000])], I8, &[5]),
            case("symmetric_a16", Prim::Clamp { lo: -32767, hi: 32767 }, vec![t(I64, &[3], &[I64MIN, -32768, 40000])], I16, &[3]),
            case(
                "i128_to_i64",
                Prim::Clamp { lo: i64::MIN, hi: i64::MAX },
                vec![t(I128, &[3], &[i128::MIN, 5, i128::MAX])],
                I64,
                &[3],
            ),
            case("point", Prim::Clamp { lo: 3, hi: 3 }, vec![t(I32, &[2], &[-5, 99])], I32, &[2]),
            case("error_bounds_outside_out_dtype", Prim::Clamp { lo: -200, hi: 127 }, vec![t(I32, &[1], &[0])], I8, &[1]),
            case("error_lo_above_hi", Prim::Clamp { lo: 1, hi: 0 }, vec![t(I32, &[1], &[0])], I32, &[1]),
        ],
        16 => vec![case(
            "edges",
            Prim::Log2Floor,
            vec![t(I128, &[10], &[i128::MIN, -5, 0, 1, 2, 3, 4, (1 << 62) - 1, 1 << 62, i128::MAX])],
            I8,
            &[10],
        )],
        17 => {
            let ln2 = 11_629_080i128;
            let mut xs: Vec<i128> = vec![1_000_000, 1, 0, -1, -2, -ln2, -ln2 + 1, -ln2 - 1, -2 * ln2, -10 * ln2, I32MIN, I64MIN];
            for z in [30i128, 31] {
                xs.extend([-z * ln2 - 1, -z * ln2, -z * ln2 + 1]);
            }
            xs.extend(random_i(r, I32, 12).into_iter().map(|v| -(v.abs() % (32 * ln2))));
            vec![
                case("bucket_edges", Prim::IntExp, vec![t(I64, &[xs.len()], &xs)], I32, &[xs.len()]),
                case("error_out_dtype_too_narrow", Prim::IntExp, vec![t(I32, &[1], &[0])], I16, &[1]),
                case("error_i128_input", Prim::IntExp, vec![t(I128, &[1], &[0])], I32, &[1]),
            ]
        }
        18 => {
            let one = 1i128 << 24;
            let vs: Vec<i128> = vec![
                I64MIN,
                -1,
                0,
                1,
                2,
                3,
                4,
                5,
                511,
                one - 1,
                one,
                one + 1,
                3 * one,
                4 * one - 1,
                4 * one,
                1 << 50,
                1 << 62,
                I64MAX,
            ];
            vec![case("edges", Prim::IntRsqrt, vec![t(I64, &[vs.len()], &vs)], I64, &[vs.len()])]
        }
        19 => {
            let one = 1i128 << 24;
            let mut vs: Vec<i128> =
                vec![I64MIN, -1, 0, 1, 2, 3, one / 1000, one / 2, one - 1, one, one + 1, 2 * one - 1, 2 * one, 100 * one, I64MAX];
            vs.extend(random_i(r, I64, 8).into_iter().map(|v| v.abs()));
            vec![case("edges", Prim::IntLn, vec![t(I64, &[vs.len()], &vs)], I32, &[vs.len()])]
        }
        20 => {
            let a = t(I32, &[6], &[-2, -1, 0, 0, 1, I32MIN]);
            let b = t(I64, &[6], &[-2, 0, 0, -1, 2, I64MAX]);
            let mut v: Vec<Case> = Cmp::ALL
                .iter()
                .map(|c| {
                    let name: &'static str = Box::leak(format!("{}_mixed", c.name()).into_boxed_str());
                    case(name, Prim::Compare { cmp: *c }, vec![a.clone(), b.clone()], I8, &[6])
                })
                .collect();
            v.push(case(
                "broadcast_scalar",
                Prim::Compare { cmp: Cmp::Le },
                vec![t(I32, &[2, 2], &[1, 2, 3, 4]), t(I32, &[], &[2])],
                I8,
                &[2, 2],
            ));
            v.push(case("error_out_not_i8", Prim::Compare { cmp: Cmp::Eq }, vec![t(I32, &[1], &[1]), t(I32, &[1], &[1])], I32, &[1]));
            v
        }
        21 => vec![
            case(
                "nonzero_is_true",
                Prim::Select,
                vec![t(I8, &[4], &[0, 1, -1, 2]), t(I32, &[4], &[10, 11, 12, 13]), t(I64, &[4], &[20, 21, 22, 23])],
                I64,
                &[4],
            ),
            case(
                "broadcast",
                Prim::Select,
                vec![t(I8, &[2, 1], &[1, 0]), t(I32, &[], &[7]), t(I32, &[2, 3], &[1, 2, 3, 4, 5, 6])],
                I32,
                &[2, 3],
            ),
            case(
                "error_selected_value_does_not_fit",
                Prim::Select,
                vec![t(I8, &[2], &[1, 0]), t(I32, &[2], &[1000, 1000]), t(I32, &[2], &[1, 1])],
                I8,
                &[2],
            ),
        ],
        22 => vec![
            case("ties_to_the_lowest_index", Prim::TopK { axis: 0, k: 2 }, vec![t(I32, &[6], &[5, 9, 9, 1, 9, 5])], Idx, &[2]),
            case("ties_past_the_first_value", Prim::TopK { axis: 0, k: 4 }, vec![t(I32, &[6], &[5, 9, 9, 1, 9, 5])], Idx, &[4]),
            case("all_equal", Prim::TopK { axis: 0, k: 3 }, vec![t(I64, &[5], &[0; 5])], Idx, &[3]),
            case("k_is_n", Prim::TopK { axis: 0, k: 3 }, vec![t(I8, &[3], &[-128, 127, 0])], Idx, &[3]),
            case("per_row", Prim::TopK { axis: 1, k: 2 }, vec![t(I32, &[2, 4], &[1, 4, 4, 2, -1, -1, -1, 0])], Idx, &[2, 2]),
            case("along_axis0", Prim::TopK { axis: 0, k: 1 }, vec![t(I32, &[3, 2], &[1, 6, 5, 6, 5, 0])], Idx, &[1, 2]),
            case("error_k_zero", Prim::TopK { axis: 0, k: 0 }, vec![t(I32, &[3], &[1, 2, 3])], Idx, &[0]),
            case("error_k_past_n", Prim::TopK { axis: 0, k: 4 }, vec![t(I32, &[3], &[1, 2, 3])], Idx, &[4]),
            case("error_out_not_idx", Prim::TopK { axis: 0, k: 1 }, vec![t(I32, &[3], &[1, 2, 3])], I32, &[1]),
        ],
        _ => unreachable!(),
    }
}

fn primitive_files() -> Vec<(String, String)> {
    (0u8..=22)
        .map(|tag| {
            let cases = cases_for(tag);
            let name = misaka_palw_tir::prim::PRIM_NAMES_V1[tag as usize];
            let file = PrimFileJson {
                format: FORMAT_PRIM.into(),
                spec: SPEC.into(),
                primitive: name.into(),
                tag,
                cases: cases.iter().map(run_case).collect(),
            };
            (format!("primitives/{tag:02}-{name}.json"), serde_json::to_string_pretty(&file).unwrap() + "\n")
        })
        .collect()
}

// ---- programs ------------------------------------------------------------------------------------

/// `S ← sat(S + w(token))` per layer, two layers: `StateWrite` saturation and per-layer state.
fn fixed_state_program() -> (TirProgramV1, MapParams) {
    let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let s = pb.fixed_state("acc", DType::I16, &[2], -100, 100, true);
    let carry = vec![TensorType::fixed(DType::I32, &[1])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let tk = b.cast(Ref::Input(INPUT_TOKEN), DType::I32);
        let tk = b.reshape_fixed(tk, &[1]);
        b.finish(&[tk])
    };
    let layer = {
        let mut b = pb.block("acc", carry.clone());
        let k = b.pb.konst(DType::I32, &[2], &[37, -23]);
        let w = b.mul(Ref::CarryIn(0), k, DType::I32);
        let off = b.c(DType::I32, -50);
        let w = b.add(w, off, DType::I32);
        let n = b.add(Ref::State(s), w, DType::I32);
        let n = b.state_write(s, n);
        let out = b.reduce_sum(n, 0, DType::I32);
        b.finish(&[out])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        b.commit(l);
        b.finish(&[])
    };
    (pb.finish(pre, vec![layer, layer], post, 0), MapParams::default())
}

/// A window-3 history of `[token, pos]` rows: `HistAppend`, `H`, and an `Iota` over `H`.
fn hist_window_program() -> (TirProgramV1, MapParams) {
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let hs = pb.hist_state("rows", DType::I16, &[2], 3, true);
    let carry = vec![TensorType::fixed(DType::I32, &[4])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let z = b.iota(DType::I32, &[Dim::Fixed(4)], 0, 0, 0);
        let z = b.commit(z);
        b.finish(&[z])
    };
    let layer = {
        let mut b = pb.block("window", carry.clone());
        let tk = b.cast(Ref::Input(INPUT_TOKEN), DType::I16);
        let ps = b.cast(Ref::Input(INPUT_POS), DType::I16);
        let tk = b.reshape_fixed(tk, &[1]);
        let ps = b.reshape_fixed(ps, &[1]);
        let row = b.concat(&[tk, ps], 0);
        let win = b.hist_append(hs, row);
        let plain = b.reduce_sum(win, 0, DType::I32);
        let w = b.iota(DType::I32, &[Dim::H, Dim::Fixed(1)], 0, 1, 1);
        let weighted = b.mul(win, w, DType::I32);
        let weighted = b.reduce_sum(weighted, 0, DType::I32);
        let both = b.concat(&[plain, weighted], 1);
        let both = b.reshape_fixed(both, &[4]);
        let out = b.add(both, Ref::CarryIn(0), DType::I32);
        b.finish(&[out])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[4]);
        b.commit(l);
        b.finish(&[])
    };
    (pb.finish(pre, vec![layer, layer], post, 0), MapParams::default())
}

fn pj(program: &TirProgramV1, params: &MapParams) -> Vec<ParamJson> {
    let _ = program;
    params.tensors.iter().map(|((j, l), t)| ParamJson { param: *j, layer: *l, le_hex: hex(&t.to_le_bytes()) }).collect()
}

fn named(m: &std::collections::BTreeMap<impl Copy + Into<u16>, Tensor>) -> Vec<NamedTensorJson> {
    m.iter().map(|(k, v)| NamedTensorJson { index: (*k).into(), value: tj(v) }).collect()
}

fn commit_json(c: &CommitRecord) -> CommitJson {
    CommitJson { slot: c.slot, block: c.block, layer: c.layer, node: c.node, value: tj(&c.value) }
}

/// The honest environment of the first layer occurrence at one position, and its commit points.
struct HonestEnv {
    block: u8,
    layer: Option<u16>,
    env: ConeEnv,
    commits: std::collections::BTreeMap<u16, Tensor>,
}

fn honest_env(program: &TirProgramV1, before: &RunState, out: &misaka_palw_tir::StepOutput, token: u32) -> HonestEnv {
    let bases = program.occurrence_slot_bases();
    let (block, layer) = (program.schedule.layers[0], Some(0u16));
    let carry_in: std::collections::BTreeMap<u8, Tensor> = program.blocks[program.schedule.pre as usize]
        .carry_out
        .iter()
        .enumerate()
        .map(|(k, n)| (k as u8, out.commits.iter().find(|c| c.block == program.schedule.pre && c.node == *n).unwrap().value.clone()))
        .collect();
    let commits: std::collections::BTreeMap<u16, Tensor> = out
        .commits
        .iter()
        .filter(|c| c.block == block && c.layer == layer && c.slot >= bases[1])
        .map(|c| (c.node, c.value.clone()))
        .collect();
    let occurrence_states = |j: u16| {
        program.blocks[block as usize].nodes.iter().any(|n| {
            n.inputs.contains(&Ref::State(j))
                || matches!(n.prim, Prim::StateWrite { state } | Prim::HistAppend { state } if state == j)
        })
    };
    // Every state instance the occurrence touches is supplied — a run state may omit a never-written
    // one, a cone's environment may not (§9.1, §9.2).
    let mut fixed = std::collections::BTreeMap::new();
    let mut hist_prior = std::collections::BTreeMap::new();
    for (j, s) in program.states.iter().enumerate() {
        let j = j as u16;
        if !s.per_layer || !occurrence_states(j) {
            continue;
        }
        match s.kind {
            misaka_palw_tir::StateKind::Fixed { .. } => {
                let v = before
                    .fixed
                    .get(&(j, layer))
                    .cloned()
                    .unwrap_or_else(|| Tensor::zeros(s.dtype, &s.shape.iter().map(|d| *d as usize).collect::<Vec<_>>()));
                fixed.insert(j, v);
            }
            misaka_palw_tir::StateKind::Hist { .. } => {
                hist_prior.insert(j, before.hist.get(&(j, layer)).map(|r| r.iter().cloned().collect()).unwrap_or_default());
            }
        }
    }
    let env = ConeEnv { token: Some(token), pos: before.pos, carry_in, fixed, hist_prior, supplied: Default::default() };
    HonestEnv { block, layer, env, commits }
}

fn refusal_json(what: &str, h: &HonestEnv, target: u16, env: &ConeEnv, class: misaka_palw_tir::TirErrorKind) -> RefusalJson {
    RefusalJson {
        what: what.into(),
        block: h.block,
        layer: h.layer,
        target,
        token: env.token,
        pos: env.pos,
        carry_in: env.carry_in.iter().map(|(k, v)| NamedTensorJson { index: *k as u16, value: tj(v) }).collect(),
        fixed: named(&env.fixed),
        hist_prior: env.hist_prior.iter().map(|(j, rows)| HistJson { state: *j, rows: rows.iter().map(tj).collect() }).collect(),
        supplied: named(&env.supplied),
        expect_error: format!("{class:?}"),
    }
}

/// The refusals of spec 04b §9.2, each an honest environment with one defect (ref2 F1–F4, F7,
/// F10), at the position the defect needs: every one must fail, with the class §9.3 names.
fn refusals(program: &TirProgramV1, interp: &Interpreter<'_>, params: &MapParams, envs: &[HonestEnv]) -> Vec<RefusalJson> {
    use misaka_palw_tir::TirErrorKind as K;
    let mut out = Vec::new();
    let mut expect = |what: &str, h: &HonestEnv, target: u16, env: ConeEnv, class: K| {
        let e = interp.eval_cone(h.block, h.layer, target, params, &env).expect_err(what);
        assert_eq!(e.kind, class, "{what}: {e}");
        out.push(refusal_json(what, h, target, &env, class));
    };
    let reads = |target: u16, h: &HonestEnv, pred: &dyn Fn(&misaka_palw_tir::Node) -> bool| -> bool {
        // Does the closure of `target` (stopping at the other commit points) contain a node with `pred`?
        let b = &program.blocks[h.block as usize];
        let mut seen = vec![false; b.nodes.len()];
        let mut stack = vec![target as usize];
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut seen[i], true) {
                continue;
            }
            if pred(&b.nodes[i]) {
                return true;
            }
            if i != target as usize && h.commits.contains_key(&(i as u16)) {
                continue;
            }
            for r in &b.nodes[i].inputs {
                if let Ref::Node(j) = r {
                    stack.push(*j as usize);
                }
            }
        }
        false
    };
    let (first, last) = (&envs[0], envs.last().unwrap());
    let target = *last.commits.keys().max().unwrap();
    let honest = |h: &HonestEnv, target: u16| {
        let mut env = h.env.clone();
        env.supplied = h.commits.clone();
        env.supplied.remove(&target);
        env
    };
    // F2: the target's own value is supplied.
    let mut env = honest(last, target);
    env.supplied.insert(target, last.commits[&target].clone());
    expect("the target is supplied (F2)", last, target, env, K::Malformed);
    // F4: a supplied index that is no node of the block.
    let mut env = honest(last, target);
    let n = program.blocks[last.block as usize].nodes.len() as u16;
    env.supplied.insert(n, last.commits[&target].clone());
    expect("a supplied index that is no node (F4)", last, target, env, K::Malformed);
    // F7: the position bound applies to a cone.
    let mut env = honest(last, target);
    env.pos = program.history_bound;
    expect("pos = history_bound (F7)", last, target, env, K::Position);
    // F1 and F3: a state value or a history the closure reads, absent — at the last position, and
    // for a history also at position 0, where zero rows are needed.
    for (j, _) in program.states.iter().enumerate() {
        let j = j as u16;
        for (h, when) in [(last, "last"), (first, "position 0")] {
            let Some(t) = h.commits.keys().copied().filter(|t| reads(*t, h, &|n| n.inputs.contains(&Ref::State(j)))).max() else {
                continue;
            };
            if when == "last" && h.env.fixed.contains_key(&j) {
                let mut env = honest(h, t);
                env.fixed.remove(&j);
                expect(&format!("state {j}'s value is absent (F1)"), h, t, env, K::Missing);
            }
        }
        for (h, when) in [(last, "the last position"), (first, "position 0, zero rows needed")] {
            let Some(t) = h
                .commits
                .keys()
                .copied()
                .filter(|t| reads(*t, h, &|n| matches!(n.prim, Prim::HistAppend { state } if state == j)))
                .max()
            else {
                continue;
            };
            let mut env = honest(h, t);
            env.hist_prior.remove(&j);
            expect(&format!("history {j} is absent at {when} (F3)"), h, t, env, K::Missing);
        }
    }
    // F10: the token, when the closure reads it — absent, then out of bound.
    if let Some(t) = last.commits.keys().copied().filter(|t| reads(*t, last, &|n| n.inputs.contains(&Ref::Input(INPUT_TOKEN)))).max() {
        let mut env = honest(last, t);
        env.token = None;
        expect("the token is absent and the closure reads it (F10)", last, t, env, K::Missing);
        let mut env = honest(last, t);
        env.token = Some(program.token_bound);
        expect("the token is at token_bound (F10)", last, t, env, K::Operand);
    }
    out
}

fn program_file(name: &str, description: &str, program: &TirProgramV1, params: &MapParams, tokens: &[u32]) -> (String, String) {
    let interp = Interpreter::new(program).expect("valid");
    let mut state = RunState::default();
    let mut steps = Vec::new();
    let mut cones = Vec::new();
    let mut envs: Vec<HonestEnv> = Vec::new();
    for (i, tk) in tokens.iter().enumerate() {
        let before = state.clone();
        let out = interp.step(params, &mut state, *tk).expect("step");
        if !program.schedule.layers.is_empty() && (i == 0 || i + 1 == tokens.len()) {
            envs.push(honest_env(program, &before, &out, *tk));
        }
        // Cones at the LAST position: every commit point of the first layer occurrence, from the
        // others (at most 3 cases, to keep the file small).
        if i + 1 == tokens.len() && !program.schedule.layers.is_empty() {
            let bases = program.occurrence_slot_bases();
            let (block, layer) = (program.schedule.layers[0], Some(0u16));
            let pre_carry: std::collections::BTreeMap<u8, Tensor> = program.blocks[program.schedule.pre as usize]
                .carry_out
                .iter()
                .enumerate()
                .map(|(k, n)| {
                    (k as u8, out.commits.iter().find(|c| c.block == program.schedule.pre && c.node == *n).unwrap().value.clone())
                })
                .collect();
            let commits: std::collections::BTreeMap<u16, Tensor> = out
                .commits
                .iter()
                .filter(|c| c.block == block && c.layer == layer && c.slot >= bases[1])
                .map(|c| (c.node, c.value.clone()))
                .collect();
            let h = envs.last().expect("the last position's environment");
            let (fixed, hist) = (h.env.fixed.clone(), h.env.hist_prior.clone());
            let targets: Vec<u16> = {
                let mut v: Vec<u16> = commits.keys().copied().collect();
                v.sort_unstable_by(|a, b| b.cmp(a));
                v.truncate(3);
                v
            };
            for target in targets {
                let mut supplied = commits.clone();
                supplied.remove(&target);
                let env = ConeEnv {
                    token: Some(*tk),
                    pos: before.pos,
                    carry_in: pre_carry.clone(),
                    fixed: fixed.clone(),
                    hist_prior: hist.clone(),
                    supplied: supplied.clone(),
                };
                let got = interp.eval_cone(block, layer, target, params, &env).expect("cone");
                assert_eq!(&got, &commits[&target]);
                cones.push(ConeJson {
                    block,
                    layer,
                    target,
                    token: *tk,
                    pos: before.pos,
                    carry_in: pre_carry.iter().map(|(k, v)| NamedTensorJson { index: *k as u16, value: tj(v) }).collect(),
                    fixed: named(&fixed),
                    hist_prior: hist.iter().map(|(j, rows)| HistJson { state: *j, rows: rows.iter().map(tj).collect() }).collect(),
                    supplied: named(&supplied),
                    expect: tj(&got),
                });
            }
        }
        steps.push(StepJson {
            pos: out.pos,
            token: *tk,
            logits: tj(&out.logits),
            commits: out.commits.iter().map(commit_json).collect(),
        });
    }
    let file = ProgramFileJson {
        format: FORMAT_PROGRAM.into(),
        spec: SPEC.into(),
        name: name.into(),
        description: description.into(),
        program_borsh_hex: hex(&program.encode()),
        graph_ir_root_hex: hex(graph_ir_root(&program.encode()).as_bytes()),
        params: pj(program, params),
        steps,
        cones,
        refusals: if envs.is_empty() { Vec::new() } else { refusals(program, &interp, params, &envs) },
    };
    (format!("programs/{name}.json"), serde_json::to_string_pretty(&file).unwrap() + "\n")
}

fn program_files() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let (p, params) = fixed_state_program();
    out.push(program_file(
        "fixed-state-saturation",
        "StateWrite saturating to [-100, 100], one state instance per layer",
        &p,
        &params,
        &[0, 3, 1, 2, 3, 3, 0, 1],
    ));
    let (p, params) = hist_window_program();
    out.push(program_file(
        "hist-window",
        "HistAppend with window 3: H = min(pos + 1, 3), an Iota over H, per-layer histories",
        &p,
        &params,
        &[5, 1, 12, 7, 7, 15],
    ));
    let (p, gens) = dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]);
    out.push(program_file(
        "dense-gqa-2layer",
        "A16 dense decoder: RMSNorm, GQA with two-level RoPE, two-pass softmax, SwiGLU",
        &p,
        &materialize(&p, &gens, 101),
        &[3, 17, 0, 23],
    ));
    let (p, gens) = dense(&[3, HISTORY_BOUND_V1_SMALL, 3]);
    out.push(program_file(
        "sliding-global",
        "window-3 local layers around a global layer",
        &p,
        &materialize(&p, &gens, 202),
        &[5, 1, 12, 7, 7],
    ));
    let (p, gens) = gdn_program(true);
    out.push(program_file(
        "gdn-k2-v4-grouped",
        "gated delta rule, 2 key heads and 4 value heads, repeat_interleave mapping",
        &p,
        &materialize(&p, &gens, 303),
        &[1, 2, 3, 5],
    ));
    let (p, gens) = mamba2_program();
    out.push(program_file(
        "mamba2",
        "Mamba2 mixer: grouped B/C, softplus dt, exp(dt·A), gate before the norm",
        &p,
        &materialize(&p, &gens, 404),
        &[7, 7, 11],
    ));
    let (p, gens) = moe_program();
    out.push(program_file(
        "moe-top2-shared",
        "top-2 of 4 experts gathered by a committed TopK, one combine accumulator, a sigmoid-gated shared expert",
        &p,
        &materialize(&p, &gens, 505),
        &[0, 1, 2, 3],
    ));
    out
}

// ---- encodings -----------------------------------------------------------------------------------

/// A global state (`Fixed` of `[1]`, or a `Hist` of window 3) written by pre, by post, or by both.
fn global_state_program(pre_writes: bool, post_writes: bool, hist: bool) -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let s =
        if hist { pb.hist_state("g", DType::I32, &[1], 3, false) } else { pb.fixed_state("g", DType::I32, &[1], -100, 100, false) };
    let carry = vec![TensorType::fixed(DType::I32, &[1])];
    let write = |b: &mut misaka_palw_tir::builder::BlockBuilder<'_>, x: Ref| {
        if hist {
            b.commit(x);
            b.hist_append(s, x)
        } else {
            b.state_write(s, x)
        }
    };
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let tk = b.cast(Ref::Input(INPUT_TOKEN), DType::I32);
        let tk = b.reshape_fixed(tk, &[1]);
        let tk = b.commit(tk);
        if pre_writes {
            write(&mut b, tk);
        }
        b.finish(&[tk])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        b.commit(l);
        if post_writes {
            write(&mut b, l);
        }
        b.finish(&[])
    };
    pb.finish(pre, vec![], post, 0)
}

fn encoding_file() -> (String, String) {
    let (p, _) = fixed_state_program();
    let good = p.encode();
    let mut cases = vec![EncodingCaseJson { name: "valid".into(), hex: hex(&good), expect: "ok".into() }];
    let mut push = |name: &str, bytes: Vec<u8>| {
        let expect = match TirProgramV1::decode_canonical(&bytes) {
            Ok(_) => "ok".to_string(),
            Err(e) => format!("{:?}", e.kind),
        };
        cases.push(EncodingCaseJson { name: name.into(), hex: hex(&bytes), expect });
    };
    let mut trailing = good.clone();
    trailing.push(0);
    push("trailing_byte", trailing);
    push("truncated", good[..good.len() - 1].to_vec());
    let mut v2 = good.clone();
    v2[0] = 2;
    push("version_2", v2);
    // A commit flag that is neither 0 nor 1: the logits node's flag, found by flipping it.
    let flag_at = {
        let mut q = p.clone();
        let post = q.schedule.post as usize;
        let logits = q.logits as usize;
        q.blocks[post].nodes[logits].commit = false;
        good.iter().zip(q.encode()).position(|(a, b)| *a != b).unwrap()
    };
    let mut bad_bool = good.clone();
    assert_eq!(bad_bool[flag_at], 1);
    bad_bool[flag_at] = 2;
    push("bool_not_0_or_1", bad_bool);
    let mut not_committed = good.clone();
    not_committed[flag_at] = 0;
    push("logits_not_committed", not_committed);
    // history_bound 2^20 is not one of the two allowed values.
    let mut hb = p.clone();
    hb.history_bound = 1 << 20;
    push("history_bound_not_allowed", hb.encode());
    let mut dead = p.clone();
    let extra = misaka_palw_tir::Node {
        prim: Prim::Cast,
        inputs: vec![Ref::CarryIn(0)],
        out: TensorType::fixed(DType::I64, &[1]),
        commit: false,
    };
    dead.blocks[1].nodes.push(extra);
    push("dead_node", dead.encode());
    let mut fwd = p.clone();
    fwd.blocks[1].nodes[1].inputs[0] = Ref::Node(5);
    push("forward_reference", fwd.encode());
    let mut unknown_prim = good.clone();
    // The first node of the pre block starts with its prim tag; find it by re-encoding.
    let prefix = {
        let mut q = p.clone();
        q.blocks[0].nodes.clear();
        q.encode()
    };
    let first_diff = good.iter().zip(&prefix).position(|(a, b)| a != b).unwrap();
    let tag_at = first_diff + 4;
    assert_eq!(unknown_prim[tag_at], Prim::Cast.tag());
    unknown_prim[tag_at] = 25;
    push("unknown_primitive_tag", unknown_prim);
    let mut wrong_shape = p.clone();
    wrong_shape.blocks[1].nodes[2].out = TensorType::fixed(DType::I32, &[3]);
    push("declared_shape_is_not_the_inferred_one", wrong_shape.encode());
    let mut per_layer_in_pre = p.clone();
    per_layer_in_pre.states[0].per_layer = false;
    push("per_layer_state_flag_mismatch", per_layer_in_pre.encode());
    // NF-1: the prim set is PALW-TIR v1's (ref2 F14).
    let mut other_set = p.clone();
    other_set.prim_set_id = [0u8; 64];
    push("prim_set_id_not_v1", other_set.encode());
    // NF-2: two blocks at least — pre and post are distinct (ref2 F13).
    let mut one = p.clone();
    one.blocks.truncate(1);
    one.schedule = misaka_palw_tir::Schedule { pre: 0, layers: vec![], post: 0 };
    push("one_block", one.encode());
    // NF-19: no two nodes of one step write one state instance; post writes nothing (ref2 F5/F6).
    push("valid_pre_writes_a_global_state", global_state_program(true, false, false).encode());
    push("post_writes_a_global_state", global_state_program(false, true, false).encode());
    push("pre_and_post_write_one_global_state", global_state_program(true, true, false).encode());
    push("pre_and_post_append_one_global_history", global_state_program(true, true, true).encode());
    for (name, want) in [
        ("valid", "ok"),
        ("prim_set_id_not_v1", "NormalForm"),
        ("one_block", "NormalForm"),
        ("valid_pre_writes_a_global_state", "ok"),
        ("post_writes_a_global_state", "NormalForm"),
        ("pre_and_post_write_one_global_state", "NormalForm"),
        ("pre_and_post_append_one_global_history", "NormalForm"),
    ] {
        let c = cases.iter().find(|c| c.name == name).expect(name);
        assert_eq!(c.expect, want, "{name}");
    }
    let file = EncodingFileJson { format: FORMAT_ENCODING.into(), spec: SPEC.into(), cases };
    ("encoding.json".into(), serde_json::to_string_pretty(&file).unwrap() + "\n")
}

// ---- the test ----------------------------------------------------------------------------------

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1")
}

#[test]
fn the_golden_vectors_are_reproduced_byte_for_byte() {
    let mut files = primitive_files();
    files.extend(program_files());
    files.push(encoding_file());
    files.push(admission_file());
    let bless = std::env::var("TIR_BLESS").is_ok_and(|v| v == "1");
    let mut mismatched = Vec::new();
    for (rel, content) in &files {
        let path = root().join(rel);
        if bless {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
            continue;
        }
        match std::fs::read(&path) {
            Ok(bytes) if bytes == content.as_bytes() => {}
            _ => mismatched.push(rel.clone()),
        }
    }
    assert!(
        mismatched.is_empty(),
        "golden vectors differ from what this implementation produces: {mismatched:?} (TIR_BLESS=1 rewrites them)"
    );
    // No stray file: every file on disk is one this generator produces.
    let mut on_disk = Vec::new();
    for sub in ["primitives", "programs"] {
        for e in std::fs::read_dir(root().join(sub)).unwrap() {
            on_disk.push(format!("{sub}/{}", e.unwrap().file_name().to_string_lossy()));
        }
    }
    on_disk.push("encoding.json".into());
    on_disk.push("admission.json".into());
    for f in on_disk {
        assert!(files.iter().any(|(rel, _)| *rel == f), "{f} is not generated by this test");
    }
}

/// Every primitive of the set has vectors, and every vector file has both outcomes where the
/// primitive can fail.
#[test]
fn every_primitive_has_vectors_including_its_edges() {
    let files = primitive_files();
    assert_eq!(files.len(), 23, "the 23 stateless primitives; StateWrite and HistAppend are in programs/");
    for tag in 0u8..=22 {
        let cases = cases_for(tag);
        assert!(!cases.is_empty());
        let errors = cases.iter().filter(|c| eval_primitive(&c.prim, &c.inputs, c.out.0, &c.out.1).is_err()).count();
        // Only these cannot fail on well-typed operands.
        if ![16u8, 18, 19].contains(&tag) {
            assert!(errors >= 1, "primitive {tag} has no error vector");
        }
        assert!(errors < cases.len(), "primitive {tag} has no success vector");
    }
}

/// `prim_set_id` is the keyed BLAKE2b-512 of the descriptor, under the chain's `Hash64` id
/// discipline (spec 04b §6.0) — recomputed here, since the crate itself hashes nothing.
#[test]
fn prim_set_id_is_the_keyed_hash_of_the_descriptor() {
    use misaka_palw_tir::prim::{PRIM_SET_ID_DOMAIN_V1, PRIM_SET_ID_V1, prim_set_descriptor_v1};
    let h = blake2b_simd::Params::new().hash_length(64).key(PRIM_SET_ID_DOMAIN_V1).hash(&prim_set_descriptor_v1());
    assert_eq!(h.as_bytes(), &PRIM_SET_ID_V1[..]);
    assert!(prim_set_descriptor_v1().starts_with(b"palw-tir/v1/spec=04b-tensor-ir/rev2/q=24/prims=0:Reshape,1:Transpose,"));
}
