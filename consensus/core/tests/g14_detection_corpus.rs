//! **G14 — the detection policy's fault corpus** (`docs/design/palw/g14-detection-policy-2026-10-17.md` §6; audit 2026-10-17 §7 P1).
//!
//! A deterministic integer model with a persistent cache (an 8-wide running sum of token embeddings that every later row reads, as
//! a KV cache is), a quantised hidden layer (round-half-up shift, saturating i16) and 16 logits — the shape of a quantised decoder,
//! not a real genesis model — is run by a producer that commits a checkpointed trace (a Merkle root over
//! per-interval records of the checkpoint state and every row), and checked by a seat that implements the REAL seam
//! (`PalwExecutionBackendV1::verify_fp_interval_opening`) the way ADR-0082 Decision 9 asks: it recomputes the checkpoint state from the
//! prompt and the committed ids on its own kernels, compares it with the committed one, then replays every row of the interval and the
//! decode relation exactly. The draw is the policy's v2 draw, `k` is the policy's plan, and the inspection loop is
//! `palw_detection_inspect_v2`.
//!
//! For every fault class the test measures the ground-truth `D` (the intervals whose inspection alone finds it), runs many
//! independent post-commit anchors, and compares the empirical detection with the exact hypergeometric value; where `D ≥ D_min` it
//! asserts the policy's target (99%). Honest claims are never flagged by a correct seat or an equivalent implementation; a seat with a
//! different (wrong) rounding does flag honest claims locally, and the reference adjudication of the localised leaf dismisses every
//! one of those — a verifier's implementation difference never becomes the producer's fraud.
//!
//! This is the policy's arithmetic and the seat contract on a fixture model. It is NOT a measurement of any genesis family's real
//! kernels, bytes or CPU (the design note's §7 lists those as unmeasured).

use std::collections::BTreeSet;

use kaspa_consensus_core::palw_backend::{
    PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwFpIntervalVerdictV1, PalwMaterialVerdictV1,
};
use kaspa_consensus_core::palw_detection_policy_v1::*;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

const HID: usize = 8;
const VOC: usize = 16;
const WIDTH: u32 = 4;
const PROMPT: usize = 40;
const DECODE: usize = 217;
/// Steps `P + T − 1 = 256`, so `N = 64` intervals of 4 under the held rule.
const STEPS: usize = PROMPT + DECODE - 1;
const N: u32 = 64;
const TRIALS: u64 = 600;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn small(state: &mut u64, half: i64) -> i32 {
    (splitmix(state) % (2 * half as u64 + 1)) as i64 as i32 - half as i32
}

#[derive(Clone)]
struct Model {
    e: [[i32; HID]; VOC],
    ws: [[i32; HID]; HID],
    wo: [[i32; HID]; VOC],
}

fn model(seed: u64) -> Model {
    let mut s = seed;
    let mut m = Model { e: [[0; HID]; VOC], ws: [[0; HID]; HID], wo: [[0; HID]; VOC] };
    for row in m.e.iter_mut() {
        for x in row.iter_mut() {
            *x = small(&mut s, 32_000);
        }
    }
    for row in m.ws.iter_mut() {
        for x in row.iter_mut() {
            *x = small(&mut s, 1);
        }
    }
    for row in m.wo.iter_mut() {
        for x in row.iter_mut() {
            *x = small(&mut s, 2);
        }
    }
    m
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kernel {
    /// Round half up, saturate to i16 — the class's pinned semantics.
    Reference,
    /// The same semantics written differently (reverse order, i128 accumulator).
    Equivalent,
    /// Round toward −∞: a WRONG implementation.
    Truncating,
    /// Round half to even: a WRONG implementation.
    HalfEven,
    /// Wrap instead of saturating: a WRONG implementation.
    Wrapping,
}

type State = [i32; HID];
type Row = [i32; VOC];

/// One step: the cache absorbs the token (`kv += E[token]`, the state every later step reads), the hidden layer reads the cache
/// and the token through the class's quantisation, the logits read the hidden layer. The cache update is kernel-independent; the
/// quantisation is where implementations can differ.
fn step(m: &Model, s: &State, token: u32, kernel: Kernel) -> (State, Row) {
    let mut next = *s;
    for i in 0..HID {
        next[i] = next[i].wrapping_add(m.e[token as usize][i]);
    }
    let mut x = [0i32; HID];
    for j in 0..HID {
        let acc: i64 = if kernel == Kernel::Equivalent {
            let mut a: i128 = i128::from(m.e[token as usize][j]) * 16;
            for i in (0..HID).rev() {
                a += i128::from(m.ws[j][i]) * i128::from(next[i] >> 4);
            }
            a as i64
        } else {
            let mut a: i64 = 0;
            for i in 0..HID {
                a += i64::from(m.ws[j][i]) * i64::from(next[i] >> 4);
            }
            a + i64::from(m.e[token as usize][j]) * 16
        };
        let shifted = match kernel {
            Kernel::Truncating => acc >> 4,
            Kernel::HalfEven => {
                let (q, r) = (acc >> 4, acc & 15);
                if r > 8 || (r == 8 && q & 1 == 1) { q + 1 } else { q }
            }
            _ => (acc + 8) >> 4,
        };
        x[j] = if kernel == Kernel::Wrapping { shifted as i16 as i32 } else { shifted.clamp(-32_768, 32_767) as i32 };
    }
    let mut row = [0i32; VOC];
    for v in 0..VOC {
        let mut a: i64 = 0;
        for j in 0..HID {
            a += i64::from(m.wo[v][j]) * i64::from(x[j]);
        }
        row[v] = a as i32;
    }
    (next, row)
}

fn argmax(row: &Row) -> u32 {
    let mut best = 0;
    for v in 1..VOC {
        if row[v] > row[best] {
            best = v;
        }
    }
    best as u32
}

fn prompt(seed: u64) -> Vec<u32> {
    let mut s = seed ^ 0xABCD;
    (0..PROMPT).map(|_| (splitmix(&mut s) % VOC as u64) as u32).collect()
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The producer and its commitment
// ---------------------------------------------------------------------------------------------------------------------------------

/// A committed trace: the claim's prompt, its answer, the checkpoint states and every row.
#[derive(Clone)]
struct Trace {
    prompt: Vec<u32>,
    outputs: Vec<u32>,
    checkpoints: Vec<State>,
    rows: Vec<Row>,
}

/// How a producer departs from honest execution.
#[derive(Clone, Default)]
struct Deviation {
    kernel_at: Option<fn(usize) -> Kernel>,
    row_lie: Vec<usize>,
    select_lie: Vec<usize>,
    state_lie: Vec<usize>,
    other_model_interval: Option<(u32, u64)>,
    rows_from_other_job_interval: Option<(u32, u64)>,
    computed_on_prompt: Option<u64>,
    full_forgery: bool,
}

fn consumed(prompt: &[u32], outputs: &[u32], i: usize) -> u32 {
    if i < PROMPT { prompt[i] } else { outputs[i - PROMPT] }
}

fn run(m: &Model, claim_prompt: &[u32], dev: &Deviation) -> Trace {
    let compute_prompt = dev.computed_on_prompt.map(prompt).unwrap_or_else(|| claim_prompt.to_vec());
    let other_job = dev.rows_from_other_job_interval.map(|(j, seed)| (j, run(m, &prompt(seed), &Deviation::default())));
    let mut s: State = [0; HID];
    let (mut outputs, mut checkpoints, mut rows) = (Vec::new(), Vec::new(), Vec::new());
    let mut rng = 0x5EED_u64;
    for i in 0..STEPS {
        if i % WIDTH as usize == 0 {
            checkpoints.push(s);
        }
        let kernel = dev.kernel_at.map(|f| f(i)).unwrap_or(Kernel::Reference);
        let interval = (i / WIDTH as usize) as u32;
        let token = consumed(&compute_prompt, &outputs, i);
        let (mut next, mut row) = step(m, &s, token, kernel);
        if let Some((j, seed)) = dev.other_model_interval
            && j == interval
        {
            // Another model's rows over this job's cache, for one interval.
            row = step(&model(seed), &s, token, kernel).1;
        }
        if dev.row_lie.contains(&i) {
            // A non-selecting logit moves: the answer is unchanged, only the row lies.
            let top = argmax(&row) as usize;
            let v = (top + 1) % VOC;
            row[v] = row[v].wrapping_sub(1);
        }
        if let Some((j, ref other)) = other_job
            && j == interval
        {
            row = other.rows[i];
        }
        if dev.full_forgery {
            for x in row.iter_mut() {
                *x = splitmix(&mut rng) as i32;
            }
            for x in next.iter_mut() {
                *x = (splitmix(&mut rng) % 1000) as i32;
            }
        }
        if i + 1 >= PROMPT {
            let mut out = argmax(&row);
            if dev.select_lie.contains(&i) {
                out = (out + 1) % VOC as u32;
            }
            outputs.push(out);
        }
        rows.push(row);
        if dev.state_lie.contains(&i) {
            // A cache lie: it persists into every later row, as a corrupted K/V row does.
            next[0] = next[0].wrapping_add(4_096);
        }
        s = next;
    }
    assert_eq!(outputs.len(), DECODE);
    Trace { prompt: claim_prompt.to_vec(), outputs, checkpoints, rows }
}

fn digest(parts: &[&[u8]]) -> [u8; 64] {
    let mut st = blake2b_simd::Params::new().hash_length(64).to_state();
    for p in parts {
        st.update(p);
    }
    st.finalize().as_bytes().try_into().unwrap()
}

fn state_bytes(s: &State) -> Vec<u8> {
    s.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn rows_bytes(rows: &[Row]) -> Vec<u8> {
    rows.iter().flat_map(|r| r.iter().flat_map(|x| x.to_le_bytes())).collect()
}

fn record(j: u32, checkpoint: &State, rows: &[Row]) -> [u8; 64] {
    digest(&[b"toy-record", &j.to_le_bytes(), &state_bytes(checkpoint), &rows_bytes(rows)])
}

fn interval_rows(t: &Trace, j: u32) -> &[Row] {
    let a = (j * WIDTH) as usize;
    &t.rows[a..(a + WIDTH as usize).min(STEPS)]
}

fn merkle_levels(leaves: Vec<[u8; 64]>) -> Vec<Vec<[u8; 64]>> {
    let mut levels = vec![leaves];
    while levels.last().unwrap().len() > 1 {
        let prev = levels.last().unwrap();
        let next: Vec<[u8; 64]> = prev.chunks(2).map(|c| digest(&[b"node", &c[0], c.get(1).unwrap_or(&c[0])])).collect();
        levels.push(next);
    }
    levels
}

/// The claim's execution root and every interval's opening.
fn commit(t: &Trace) -> (Hash64, Vec<Vec<u8>>) {
    let leaves: Vec<[u8; 64]> = (0..N).map(|j| record(j, &t.checkpoints[j as usize], interval_rows(t, j))).collect();
    let levels = merkle_levels(leaves);
    let root = Hash64::from_bytes(levels.last().unwrap()[0]);
    let openings = (0..N)
        .map(|j| {
            let mut b = Vec::new();
            b.extend_from_slice(&j.to_le_bytes());
            b.extend_from_slice(&state_bytes(&t.checkpoints[j as usize]));
            let rows = interval_rows(t, j);
            b.extend_from_slice(&(rows.len() as u32).to_le_bytes());
            b.extend_from_slice(&rows_bytes(rows));
            let mut idx = j as usize;
            for level in &levels[..levels.len() - 1] {
                let sib = level.get(idx ^ 1).unwrap_or(&level[idx]);
                b.extend_from_slice(sib);
                idx /= 2;
            }
            b
        })
        .collect();
    (root, openings)
}

struct Opening {
    j: u32,
    checkpoint: State,
    rows: Vec<Row>,
    path: Vec<[u8; 64]>,
}

fn decode_opening(b: &[u8]) -> Option<Opening> {
    let rd = |o: usize| b.get(o..o + 4).map(|x| i32::from_le_bytes(x.try_into().unwrap()));
    let j = rd(0)? as u32;
    let mut checkpoint = [0i32; HID];
    for (k, x) in checkpoint.iter_mut().enumerate() {
        *x = rd(4 + 4 * k)?;
    }
    let mut o = 4 + 4 * HID;
    let count = rd(o)? as usize;
    o += 4;
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count.min(64) {
        let mut r = [0i32; VOC];
        for x in r.iter_mut() {
            *x = rd(o)?;
            o += 4;
        }
        rows.push(r);
    }
    let path = b.get(o..)?.chunks(64).filter(|c| c.len() == 64).map(|c| c.try_into().unwrap()).collect();
    Some(Opening { j, checkpoint, rows, path })
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The seat: the real seam, a fixture family
// ---------------------------------------------------------------------------------------------------------------------------------

struct ToySeat {
    model: Model,
    kernel: Kernel,
    prompt: Vec<u32>,
    outputs: Vec<u32>,
    /// The state before every step, recomputed on THIS seat's kernels from the prompt and the committed ids (teacher-forced).
    own_states: Vec<State>,
}

impl ToySeat {
    fn new(m: &Model, kernel: Kernel, prompt: &[u32], outputs: &[u32]) -> Self {
        let mut own_states = Vec::with_capacity(STEPS + 1);
        let mut s: State = [0; HID];
        for i in 0..STEPS {
            own_states.push(s);
            s = step(m, &s, consumed(prompt, outputs, i), kernel).0;
        }
        own_states.push(s);
        Self { model: m.clone(), kernel, prompt: prompt.to_vec(), outputs: outputs.to_vec(), own_states }
    }
}

impl PalwExecutionBackendV1 for ToySeat {
    fn model_id(&self) -> &str {
        "toy-integer-recurrent"
    }
    fn job_for_anchor(&self, _anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
        Err("not used".into())
    }
    fn execute(&self, _job: &PalwJobContextV2, _prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        Err("not used".into())
    }
    fn verify_material(&self, _material: &[u8], _claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        PalwMaterialVerdictV1::Unverifiable
    }
    fn fp_interval_count_for(&self, prompt_tokens: u32, decode_tokens_executed: u32) -> Option<u32> {
        Some((prompt_tokens + decode_tokens_executed - 1).div_ceil(WIDTH).max(1))
    }
    fn verify_fp_interval_opening(
        &self,
        opening: &[u8],
        claim: PalwClaimRootsV1,
        index: u32,
        _prompt_token_ids: &[u32],
        _work_leaves: u64,
    ) -> PalwFpIntervalVerdictV1 {
        let Some(o) = decode_opening(opening) else { return PalwFpIntervalVerdictV1::Unverifiable };
        if o.j != index || index >= N {
            return PalwFpIntervalVerdictV1::Mismatch;
        }
        // Binding: the record opens to the claim's execution root, or it is not this claim's evidence.
        let mut acc = record(o.j, &o.checkpoint, &o.rows);
        let mut idx = o.j as usize;
        for sib in &o.path {
            acc = if idx % 2 == 0 { digest(&[b"node", &acc, sib]) } else { digest(&[b"node", sib, &acc]) };
            idx /= 2;
        }
        if Hash64::from_bytes(acc) != claim.execution_root {
            return PalwFpIntervalVerdictV1::Mismatch;
        }
        let first = (index * WIDTH) as usize;
        let len = WIDTH.min((STEPS - first) as u32) as usize;
        if o.rows.len() != len {
            return PalwFpIntervalVerdictV1::Fault { leaf_index: first as u64 };
        }
        // ADR-0082 Decision 9: the checkpoint is recomputed, never fetched — a cache-altering lie anywhere before lands here.
        if self.own_states[first] != o.checkpoint {
            return PalwFpIntervalVerdictV1::Fault { leaf_index: first as u64 };
        }
        let mut s = self.own_states[first];
        for (k, committed) in o.rows.iter().enumerate() {
            let i = first + k;
            let (next, row) = step(&self.model, &s, consumed(&self.prompt, &self.outputs, i), self.kernel);
            if row != *committed {
                return PalwFpIntervalVerdictV1::Fault { leaf_index: i as u64 };
            }
            if i + 1 >= PROMPT && argmax(&row) != self.outputs[i + 1 - PROMPT] {
                return PalwFpIntervalVerdictV1::Fault { leaf_index: i as u64 };
            }
            s = next;
        }
        PalwFpIntervalVerdictV1::Valid
    }
}

/// **The reference adjudication of one localised leaf** — the court's question, decided by the class's pinned semantics from the
/// claim's own commitments: is the committed row (and its decode relation, or the checkpoint) what the reference computes?
/// `true` = the producer's value stands (dismissed).
fn court_dismisses(m: &Model, t: &Trace, leaf: u64) -> bool {
    let reference = ToySeat::new(m, Kernel::Reference, &t.prompt, &t.outputs);
    let i = leaf as usize;
    if i % WIDTH as usize == 0 && reference.own_states[i] != t.checkpoints[i / WIDTH as usize] {
        return false;
    }
    let (_, row) = step(m, &reference.own_states[i], consumed(&t.prompt, &t.outputs, i), Kernel::Reference);
    row == t.rows[i] && (i + 1 < PROMPT || argmax(&row) == t.outputs[i + 1 - PROMPT])
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The harness
// ---------------------------------------------------------------------------------------------------------------------------------

fn policy() -> PalwDetectionPolicyV1 {
    palw_detection_genesis_design_v1(h(0x70E), PalwDetectionRouteV1::Base0A16, PalwIntervalCountRuleV1::HeldPositions { width: WIDTH })
}

fn plan(p: &PalwDetectionPolicyV1) -> PalwDetectionPlanV1 {
    p.plan_v1(&PalwDetectionClaimFactsV1 {
        attempt_lane: false,
        work_leaves: 0,
        context_priced_leaves: None,
        prompt_tokens: PROMPT as u32,
        decode_tokens_executed: DECODE as u32,
        collateral_sompi: u128::MAX,
    })
    .expect("the corpus claim is plannable")
}

fn binom(n: u64, k: u64) -> u128 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut r: u128 = 1;
    for i in 0..k {
        r = r * u128::from(n - i) / u128::from(i + 1);
    }
    r
}

fn analytic(n: u32, d: u32, k: u32) -> f64 {
    1.0 - binom(u64::from(n - d), u64::from(k)) as f64 / binom(u64::from(n), u64::from(k)) as f64
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Event {
    Fault,
    NotCertified,
}

struct Claim {
    trace: Trace,
    root: Hash64,
    served: Vec<Vec<Vec<u8>>>,
}

fn claim(m: &Model, prompt_seed: u64, dev: &Deviation) -> Claim {
    let trace = run(m, &prompt(prompt_seed), dev);
    let (root, openings) = commit(&trace);
    Claim { trace, root, served: openings.into_iter().map(|o| vec![o]).collect() }
}

fn roots(root: Hash64) -> PalwClaimRootsV1 {
    PalwClaimRootsV1 { execution_root: root, trace_root: h(5), anchor: h(9), attempt_draw: None, output_root: None, job_pin: None }
}

fn inspect(seat: &ToySeat, c: &Claim, drawn: &[u32]) -> PalwDetectionOutcomeV2 {
    let served = |j: u32| c.served.get(j as usize).cloned().unwrap_or_default();
    palw_detection_inspect_v2(seat, roots(c.root), &c.trace.prompt, 0, drawn, &served, 1 << 20)
}

fn event_of(o: &PalwDetectionOutcomeV2) -> Option<Event> {
    match o {
        PalwDetectionOutcomeV2::Certified => None,
        PalwDetectionOutcomeV2::Fault { .. } => Some(Event::Fault),
        PalwDetectionOutcomeV2::NotVerified { .. } => Some(Event::NotCertified),
    }
}

fn binding(claim_seed: u64, seat: u8) -> PalwDetectionDrawBindingV2 {
    PalwDetectionDrawBindingV2 {
        network_domain: h(0x11),
        claim_id: h(claim_seed),
        class_id: policy().class_id,
        job_id: h(claim_seed ^ 0xF00D),
        policy_id: policy().id(),
        checker: PalwDetectionCheckerV1::Seat(seat),
    }
}

/// Ground truth `D` for an event: the intervals whose inspection alone yields it.
fn ground_truth(seat: &ToySeat, c: &Claim, want: Event) -> u32 {
    (0..N).filter(|j| event_of(&inspect(seat, c, &[*j])) == Some(want)).count() as u32
}

struct Measured {
    d: u32,
    empirical: f64,
    analytic: f64,
}

/// `TRIALS` independent post-commit anchors; each draws `k` with the v2 draw and inspects.
fn measure(m: &Model, c: &Claim, want: Event, k: u32, claim_seed: u64) -> Measured {
    let seat = ToySeat::new(m, Kernel::Reference, &c.trace.prompt, &c.trace.outputs);
    let d = ground_truth(&seat, c, want);
    let mut hits = 0u64;
    for t in 0..TRIALS {
        let drawn = palw_detection_draw_round_v2(&binding(claim_seed, 0), 0, &h(1_000_000 + t), k, N, &BTreeSet::new()).unwrap();
        let outcome = inspect(&seat, c, &drawn);
        if let PalwDetectionOutcomeV2::Fault { interval_index, first_leaf_index, .. } = outcome {
            // The localisation is inside the drawn interval and is a REAL fault: the reference adjudication does not dismiss it.
            assert!(drawn.contains(&interval_index));
            assert!(!court_dismisses(m, &c.trace, first_leaf_index), "a reference seat's localised leaf is a real fault");
        }
        if event_of(&outcome) == Some(want) {
            hits += 1;
        }
    }
    Measured { d, empirical: hits as f64 / TRIALS as f64, analytic: analytic(N, d, k) }
}

fn check(name: &str, r: &Measured, d_min: u32) {
    let p = r.analytic;
    let sigma = (p * (1.0 - p) / TRIALS as f64).sqrt();
    let tol = 4.0 * sigma + 1.0 / TRIALS as f64;
    eprintln!(
        "corpus {name:<28} D={:>2}/{N}  empirical={:.4}  analytic={:.4}  {}",
        r.d,
        r.empirical,
        p,
        if r.d >= d_min { "(in the fault set: target 0.99)" } else { "(below D_min: deterrence / full re-execution cover)" }
    );
    assert!((r.empirical - p).abs() <= tol, "{name}: empirical {} vs analytic {p} (tol {tol})", r.empirical);
    if r.d >= d_min {
        assert!(p >= 0.99, "{name}: the policy's k does not reach the target at D={}", r.d);
        assert!(r.empirical >= 0.99 - tol, "{name}: empirical {} below the target", r.empirical);
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------------------------------------------------------------

#[test]
fn the_backend_geometry_is_the_policys_and_the_plan_is_what_the_corpus_uses() {
    let p = policy();
    let pl = plan(&p);
    let seat = ToySeat::new(&model(1), Kernel::Reference, &prompt(1), &[0; DECODE]);
    assert_eq!(seat.fp_interval_count_for(PROMPT as u32, DECODE as u32), Some(pl.n));
    assert_eq!(pl.n, N);
    assert_eq!(pl.d_min, 7, "10% of 64, rounded up");
    eprintln!("corpus plan: N={} D_min={} k={} miss@D_min≤{} ppm", pl.n, pl.d_min, pl.k_round, pl.miss_at_d_min_ppm_ceil);
}

#[test]
fn every_fault_class_is_detected_at_its_analytic_rate_and_the_fault_set_at_the_target() {
    let p = policy();
    let pl = plan(&p);
    let m = model(0xC0FFEE);
    let last = STEPS - 1;
    let fault_classes: Vec<(&str, Deviation, Event, Option<(u32, u32)>)> = vec![
        ("full_forgery", Deviation { full_forgery: true, ..Default::default() }, Event::Fault, Some((N, N))),
        ("single_row_mid", Deviation { row_lie: vec![130], ..Default::default() }, Event::Fault, Some((1, 1))),
        ("selection_lie_mid", Deviation { select_lie: vec![150], ..Default::default() }, Event::Fault, Some((1, 1))),
        ("interval_head_row", Deviation { row_lie: vec![0], ..Default::default() }, Event::Fault, Some((1, 1))),
        ("interval_tail_row", Deviation { row_lie: vec![last], ..Default::default() }, Event::Fault, Some((1, 1))),
        ("head_state", Deviation { state_lie: vec![0], ..Default::default() }, Event::Fault, Some((N - 2, N))),
        ("tail_state", Deviation { state_lie: vec![last - 1], ..Default::default() }, Event::Fault, Some((1, 1))),
        ("prefill_last_row", Deviation { row_lie: vec![PROMPT - 2], ..Default::default() }, Event::Fault, Some((1, 1))),
        ("decode_first_selection", Deviation { select_lie: vec![PROMPT - 1], ..Default::default() }, Event::Fault, Some((1, 1))),
        ("kv_state_mid", Deviation { state_lie: vec![101], ..Default::default() }, Event::Fault, Some((37, 39))),
        ("quant_half_even_all", Deviation { kernel_at: Some(|_| Kernel::HalfEven), ..Default::default() }, Event::Fault, None),
        ("quant_truncating_all", Deviation { kernel_at: Some(|_| Kernel::Truncating), ..Default::default() }, Event::Fault, None),
        ("overflow_wrapping_all", Deviation { kernel_at: Some(|_| Kernel::Wrapping), ..Default::default() }, Event::Fault, None),
        (
            "quant_half_even_last_interval",
            Deviation { kernel_at: Some(|i| if i >= STEPS - 4 { Kernel::HalfEven } else { Kernel::Reference }), ..Default::default() },
            Event::Fault,
            Some((0, 1)),
        ),
        ("other_job_rows_one_interval", Deviation { rows_from_other_job_interval: Some((20, 0xBEEF)), ..Default::default() }, Event::Fault, Some((1, 1))),
        ("other_model_one_interval", Deviation { other_model_interval: Some((20, 0xD00D)), ..Default::default() }, Event::Fault, Some((1, 1))),
        ("consistent_borrowed_trace", Deviation { computed_on_prompt: Some(0x0B0E), ..Default::default() }, Event::Fault, Some((N - 1, N))),
    ];
    for (i, (name, dev, want, d_bounds)) in fault_classes.iter().enumerate() {
        let seed = 0x1000 + i as u64;
        let c = claim(&m, seed, dev);
        let r = measure(&m, &c, *want, pl.k_round, seed);
        if let Some((lo, hi)) = d_bounds {
            assert!((*lo..=*hi).contains(&r.d), "{name}: D={} outside [{lo}, {hi}]", r.d);
        } else {
            assert!(r.d >= 1, "{name}: the deviation must change something, or the row proves nothing (D={})", r.d);
        }
        check(name, &r, pl.d_min);
    }
}

/// **Withholding and served-but-foreign openings** are not faults: the claim is NOT certified (the DA/default path), at the same
/// hypergeometric rate over the withheld set.
#[test]
fn withholding_and_foreign_openings_are_never_certified_at_their_rate() {
    let pl = plan(&policy());
    let m = model(0xC0FFEE);
    // 8 of 64 intervals withheld.
    let mut c = claim(&m, 0x2001, &Deviation::default());
    for j in (0..N).step_by(8) {
        c.served[j as usize].clear();
    }
    check("withholding_8_of_64", &measure(&m, &c, Event::NotCertified, pl.k_round, 0x2001), pl.d_min);
    // Everything withheld.
    let mut all = claim(&m, 0x2002, &Deviation::default());
    all.served.iter_mut().for_each(|s| s.clear());
    let r = measure(&m, &all, Event::NotCertified, pl.k_round, 0x2002);
    assert_eq!(r.d, N);
    check("withholding_all", &r, pl.d_min);
    // Openings authenticated under ANOTHER claim's root served for 8 intervals: they bind to nothing here.
    let mut mixed = claim(&m, 0x2003, &Deviation::default());
    let other = claim(&m, 0x2004, &Deviation::default());
    for j in (3..N).step_by(8) {
        mixed.served[j as usize] = other.served[j as usize].clone();
    }
    let r = measure(&m, &mixed, Event::NotCertified, pl.k_round, 0x2003);
    assert_eq!(r.d, 8);
    check("foreign_openings_8_of_64", &r, pl.d_min);
}

/// **Coverage across rounds**: four rounds of 8 inspect 32 DISTINCT intervals (miss C(63,32)/C(64,32) = 1/2 for one bad interval);
/// re-running round 0 four times — the legacy retry — inspects the same 8 (miss 7/8).
#[test]
fn distinct_rounds_add_coverage_and_a_repeated_draw_adds_none() {
    let m = model(0xC0FFEE);
    let c = claim(&m, 0x3001, &Deviation { row_lie: vec![77], ..Default::default() });
    let seat = ToySeat::new(&m, Kernel::Reference, &c.trace.prompt, &c.trace.outputs);
    assert_eq!(ground_truth(&seat, &c, Event::Fault), 1);
    let (mut distinct_hits, mut repeat_hits) = (0u64, 0u64);
    for t in 0..TRIALS {
        let anchors: Vec<PalwDetectionRoundAnchorV1> =
            (0..4).map(|r| PalwDetectionRoundAnchorV1 { block: h(5_000_000 + t * 8 + r), daa: 100 + r * 10 }).collect();
        let (rounds, union) = palw_detection_draw_rounds_v2(&binding(0x3001, 1), 50, &anchors, 8, N).unwrap();
        assert_eq!(union.len(), 32);
        if rounds.iter().any(|r| event_of(&inspect(&seat, &c, r)) == Some(Event::Fault)) {
            distinct_hits += 1;
        }
        let same = palw_detection_draw_round_v2(&binding(0x3001, 1), 0, &anchors[0].block, 8, N, &BTreeSet::new()).unwrap();
        if (0..4).any(|_| event_of(&inspect(&seat, &c, &same)) == Some(Event::Fault)) {
            repeat_hits += 1;
        }
    }
    let (d, r) = (distinct_hits as f64 / TRIALS as f64, repeat_hits as f64 / TRIALS as f64);
    eprintln!("corpus rounds: 4×8 distinct → {d:.4} (analytic 0.5000); 4× the same 8 → {r:.4} (analytic 0.1250)");
    assert!((d - 0.5).abs() <= 4.0 * (0.25f64 / TRIALS as f64).sqrt() + 1.0 / TRIALS as f64);
    assert!((r - 0.125).abs() <= 4.0 * (0.125f64 * 0.875 / TRIALS as f64).sqrt() + 1.0 / TRIALS as f64);
}

/// **An honest claim is never flagged** by a correct seat or by an equivalent implementation, over many claims, models and every
/// interval. A seat with a wrong rounding DOES flag honest claims locally — and the reference adjudication of every such localised
/// leaf dismisses it: a verifier's implementation difference never becomes the producer's fraud.
#[test]
fn honest_claims_are_never_flagged_and_a_wrong_verifier_never_convicts() {
    let all: Vec<u32> = (0..N).collect();
    let (mut wrong_flags, mut checked) = (0u32, 0u32);
    for mseed in 0..4u64 {
        let m = model(0xA000 + mseed);
        for pseed in 0..25u64 {
            let c = claim(&m, 0x4000 + mseed * 100 + pseed, &Deviation::default());
            for kernel in [Kernel::Reference, Kernel::Equivalent] {
                let seat = ToySeat::new(&m, kernel, &c.trace.prompt, &c.trace.outputs);
                assert_eq!(inspect(&seat, &c, &all), PalwDetectionOutcomeV2::Certified, "honest claim flagged by {kernel:?}");
            }
            for kernel in [Kernel::Truncating, Kernel::HalfEven, Kernel::Wrapping] {
                let seat = ToySeat::new(&m, kernel, &c.trace.prompt, &c.trace.outputs);
                for j in 0..N {
                    if let PalwDetectionOutcomeV2::Fault { first_leaf_index, .. } = inspect(&seat, &c, &[j]) {
                        wrong_flags += 1;
                        assert!(court_dismisses(&m, &c.trace, first_leaf_index), "a wrong verifier's finding convicted an honest claim");
                    }
                }
            }
            checked += 1;
        }
    }
    eprintln!("corpus honest: {checked} claims × all intervals clean under 2 correct seats; {wrong_flags} local flags by wrong seats, all dismissed");
    assert!(wrong_flags > 0, "the wrong verifiers must actually disagree somewhere, or this test proves nothing");
}
