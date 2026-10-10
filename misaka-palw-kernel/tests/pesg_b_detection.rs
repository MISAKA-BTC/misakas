//! **PESG lane B — the worst-case placement of faults and the exact detection probability, in small geometry**
//! (`docs/design/palw/probabilistic-economic-security-gate.md` §4 B, T2; the report is `docs/design/palw/pesg-b-detection-bounds.md`).
//!
//! Every fault the gate lists is placed in every position of a real five-position claim (the dense + MoE + history fixture: MatMul,
//! quantization, the integer nonlinears, TopK routing, `Gather`, `Fixed` and `Hist` state), against every sample choice a verifier can
//! make, with the REAL verifiers and courts of this crate:
//!
//! * K2-TIR-v1 / v2 single-program claims: `verify_scope_v1` on `WholeClaim`, on every `Segments` set and on every `AuditOnly` subset of
//!   positions, each found fault filed to `verify_fault_proof_v1`;
//! * K2-TIR-v4 segmented claims: route A (`check_positions_v1` on sampled positions) and route B (`check_claim_by_reexecution_v1`),
//!   each found fault filed to `verify_seg_fault_v1`.
//!
//! A fault is placed two ways: **isolated** (one committed value changed, the rest honest: two relations are false, the lied value's
//! and its consumers') and **self-consistent** (the producer computes everything after the lie from the lie: exactly ONE relation is
//! false). The second is the adversary's best placement. For each placement the set `D` of single positions whose check detects it is
//! measured, then the miss probability of every sample size is computed by enumerating the sample space and compared with the closed
//! form `C(P − |D|, s) / C(P, s)`; that a scope's detection is the union of its positions' is checked on every subset by the real
//! verifier for one placement of every family.
//!
//! The algebraic residual is enumerated exhaustively in toy Mersenne fields with this crate's own field arithmetic and sampler mapping
//! (GF(7), GF(31)): every error pattern of the given shapes against every challenge vector. Grinding, retries/vetoes and correlated
//! watchers are enumerated over their joint sample spaces. Nothing here is a Monte Carlo estimate of a bound; the one frequency check
//! (the challenge crate's subset sampler) is a sanity check of support, labelled as such.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use misaka_palw_kernel::challenge::{ChallengeLabelV1, ChallengeStreamV1 as KernelStream};
use misaka_palw_kernel::descriptor::k2_tir_v2_descriptor;
use misaka_palw_kernel::element::{SegClaimContextV1, SegFindingV1, SegMaterialV1, check_positions_v1, verify_seg_fault_v1};
use misaka_palw_kernel::family::{ConstraintFamilyV1, family_of_prim};
use misaka_palw_kernel::field::{FieldElemV1, Mersenne};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::DecodeRuleV1;
use misaka_palw_kernel::seg::{SegmentedCommitmentsV1, prompt_root_of_ids_v1, seg_commitments_of_trace_v1};
use misaka_palw_kernel::seg_detect::{check_claim_by_reexecution_v1, first_decode_mismatch_v1};
use misaka_palw_kernel::trace::{ParamCommitmentsV1, SourceV1, TraceV1, WiringV1, eval_node, trace_v1};
use misaka_palw_kernel::verify::{MaterialV1, ScopeV1, ScopeVerdictV1, TraceMaterialV1};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{DType, MapParams, Tensor};
use misaka_palw_tir_sketch::fixture::dense_moe_v1;

const P: u32 = TOKENS.len() as u32;

// ── exact combinatorics ─────────────────────────────────────────────────────────────────────────────────────────────────────────

fn choose(n: u32, k: u32) -> u128 {
    if k > n {
        return 0;
    }
    let mut r: u128 = 1;
    for i in 0..k as u128 {
        r = r * (n as u128 - i) / (i + 1);
    }
    r
}

/// Every subset of `0..n` of size `k`, as a bit mask.
fn subsets(n: u32, k: u32) -> Vec<u32> {
    (0u32..(1 << n)).filter(|m| m.count_ones() == k).collect()
}

fn positions_of(mask: u32) -> Vec<u32> {
    (0..32).filter(|i| mask & (1 << i) != 0).collect()
}

// ── the adversary's tracer: an honest trace, or one computed onward from a lie ────────────────────────────────────────────────────

type At = (u32, u16, u16);

/// The producer's values for `tokens`, with element `e` of value `at` moved by one inside its dtype and **everything after it
/// computed from the lie** (a self-consistent fake trace: only the lied relation is false). `None` when no consistent continuation
/// exists (a later primitive refuses the lie, e.g. an index out of bounds).
fn trace_with_lie(program: &TirProgramV1, params: &MapParams, tokens: &[u32], lie: Option<(At, usize)>) -> Option<TraceV1> {
    let w = WiringV1::new(program).ok()?;
    let mut values: Vec<Vec<Vec<Tensor>>> = Vec::with_capacity(tokens.len());
    for p in 0..tokens.len() as u32 {
        let mut pos_vals: Vec<Vec<Tensor>> = Vec::with_capacity(w.occurrences.len());
        for s in 0..w.occurrences.len() as u16 {
            let b = w.occurrences[s as usize].0 as usize;
            let mut occ_vals: Vec<Tensor> = Vec::with_capacity(program.blocks[b].nodes.len());
            for n in 0..program.blocks[b].nodes.len() as u16 {
                let resolve = |src: SourceV1, occ_vals: &Vec<Tensor>, pos_vals: &Vec<Vec<Tensor>>| -> Option<Tensor> {
                    match src {
                        SourceV1::Node { position, occurrence, node } if position == p && occurrence == s => {
                            occ_vals.get(node as usize).cloned()
                        }
                        SourceV1::Node { position, occurrence, node } if position == p => {
                            pos_vals.get(occurrence as usize)?.get(node as usize).cloned()
                        }
                        SourceV1::Node { position, occurrence, node } => {
                            values.get(position as usize)?.get(occurrence as usize)?.get(node as usize).cloned()
                        }
                        SourceV1::Param { index, layer } => params.tensors.get(&(index, layer)).cloned(),
                        SourceV1::Const(j) => {
                            let c = &program.consts[j as usize];
                            let shape: Vec<usize> = c.shape.iter().map(|d| *d as usize).collect();
                            Tensor::from_le_bytes(c.dtype, &shape, &c.data).ok()
                        }
                        SourceV1::Zeros { dtype, shape } => Some(Tensor::zeros(dtype, &shape)),
                        SourceV1::Public(v) => Tensor::scalar(DType::Idx, v as i128).ok(),
                        SourceV1::Input { .. } => None,
                    }
                };
                let node = w.node(s, n);
                let inputs = (0..node.inputs.len())
                    .map(|i| resolve(w.input_source(tokens, p, s, n, i).ok()?, &occ_vals, &pos_vals))
                    .collect::<Option<Vec<_>>>()?;
                let prior = w
                    .hist_prior_sources(tokens, p, s, n)
                    .ok()?
                    .into_iter()
                    .map(|src| resolve(src, &occ_vals, &pos_vals))
                    .collect::<Option<Vec<_>>>()?;
                let mut v = eval_node(program, node, &inputs, &prior, w.h(s, p)).ok()?;
                if let Some((at, e)) = lie
                    && at == (p, s, n)
                {
                    bump(&mut v, e);
                }
                occ_vals.push(v);
            }
            pos_vals.push(occ_vals);
        }
        values.push(pos_vals);
    }
    Some(TraceV1 { values, inputs: Vec::new() })
}

/// One fault placement: a value, an element of it, its family.
#[derive(Clone, Debug)]
struct Placement {
    at: At,
    elem: usize,
    family: ConstraintFamilyV1,
    derived: bool,
}

/// Every committed value of every position, at its first and its last element.
fn placements(program: &TirProgramV1, honest: &TraceV1) -> Vec<Placement> {
    let w = WiringV1::new(program).unwrap();
    let mut out = Vec::new();
    for p in 0..honest.values.len() as u32 {
        for s in 0..w.occurrences.len() as u16 {
            for n in 0..honest.values[p as usize][s as usize].len() as u16 {
                let len = honest.values[p as usize][s as usize][n as usize].len();
                if len == 0 {
                    continue;
                }
                let family = family_of_prim(&w.node(s, n).prim);
                let mut elems = vec![0, len - 1];
                elems.dedup();
                for elem in elems {
                    out.push(Placement { at: (p, s, n), elem, family, derived: w.is_derived(s, n) });
                }
            }
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Isolated,
    SelfConsistent,
}

fn fake(c: &Claim, honest: &TraceV1, pl: &Placement, mode: Mode) -> Option<TraceV1> {
    match mode {
        Mode::Isolated => {
            let mut t = honest.clone();
            bump(&mut t.values[pl.at.0 as usize][pl.at.1 as usize][pl.at.2 as usize], pl.elem);
            Some(t)
        }
        Mode::SelfConsistent => trace_with_lie(&c.program, &c.params, &TOKENS, Some((pl.at, pl.elem))),
    }
}

// ── K2-TIR-v1 / v2: what one scope sees ─────────────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seen {
    /// A fault found, filed, and convicted by the court from public material.
    Convicted,
    Pass,
    /// The DA path: something read was not served or does not open to its commitment.
    Unavailable,
    /// The evidence object is not a claim of this program: refused at inclusion.
    Refused,
}

fn seen(c: &Claim, committed: &TraceV1, material: &dyn MaterialV1, scope: &ScopeV1) -> Seen {
    let (v, court) = c.verify_scope(committed, material, scope);
    match v {
        ScopeVerdictV1::Pass { .. } => Seen::Pass,
        ScopeVerdictV1::Fault(proof) => {
            court(&proof).unwrap_or_else(|e| panic!("a found fault must convict from public material: {e:?} ({proof:?})"));
            Seen::Convicted
        }
        ScopeVerdictV1::Unavailable { .. } => Seen::Unavailable,
        ScopeVerdictV1::EvidenceMalformed { .. } => Seen::Refused,
        ScopeVerdictV1::Inconsistent { why } => panic!("verifier inconsistency: {why}"),
    }
}

fn detected(x: Seen) -> bool {
    matches!(x, Seen::Convicted | Seen::Refused)
}

/// The single positions whose `AuditOnly` check detects the committed trace `t` (bit mask).
fn detection_mask(c: &Claim, t: &TraceV1) -> u32 {
    let m = TraceMaterialV1 { trace: t, params: &c.params };
    (0..P).filter(|q| detected(seen(c, t, &m, &ScopeV1::AuditOnly(vec![*q])))).fold(0, |a, q| a | (1 << q))
}

/// The exact miss probability of a uniform `s`-of-`P` sample against detection set `d`, by enumerating the sample space.
fn miss_enumerated(d: u32, s: u32) -> (u128, u128) {
    let all = subsets(P, s);
    (all.iter().filter(|m| *m & d == 0).count() as u128, all.len() as u128)
}

#[derive(Default)]
struct FamilyStats {
    placements: u32,
    min_d: u32,
    max_d: u32,
    unpropagatable: u32,
}

/// **The placement search for one descriptor**: every placement × both modes; `WholeClaim` always detects and convicts; every
/// sample size's enumerated miss equals the closed form; the segment scopes; the worst placement per family.
fn placement_search(c: &Claim, only: Option<ConstraintFamilyV1>, label: &str) -> BTreeMap<(String, &'static str), FamilyStats> {
    let honest = trace_v1(&c.program, &c.params, &TOKENS).unwrap();
    assert_eq!(
        trace_with_lie(&c.program, &c.params, &TOKENS, None).as_ref(),
        Some(&honest),
        "the adversary's tracer is the honest one"
    );
    let hm = TraceMaterialV1 { trace: &honest, params: &c.params };
    assert_eq!(seen(c, &honest, &hm, &ScopeV1::WholeClaim), Seen::Pass, "an honest claim passes");
    let ev = c.evidence_of(&honest);
    let segs = ev.segments.len() as u32;
    let mut stats: BTreeMap<(String, &'static str), FamilyStats> = BTreeMap::new();
    let mut worst_overall = 0u32;
    for pl in placements(&c.program, &honest) {
        if only.is_some_and(|f| f != pl.family) {
            continue;
        }
        for mode in [Mode::Isolated, Mode::SelfConsistent] {
            let key = (format!("{mode:?}"), pl.family.name());
            let st = stats.entry(key).or_insert(FamilyStats { min_d: u32::MAX, ..Default::default() });
            let Some(t) = fake(c, &honest, &pl, mode) else {
                st.unpropagatable += 1;
                continue;
            };
            if t == honest {
                continue;
            }
            let m = TraceMaterialV1 { trace: &t, params: &c.params };
            // Complete coverage: the whole-claim check finds every placement, and its filing convicts.
            let whole = seen(c, &t, &m, &ScopeV1::WholeClaim);
            assert!(detected(whole), "{label} {mode:?} {pl:?}: the whole-claim check missed ({whole:?})");
            let d = detection_mask(c, &t);
            assert_ne!(d, 0, "{label} {mode:?} {pl:?}: no single position detects a lie the whole claim detects");
            assert!(d & (1 << pl.at.0) != 0, "{label} {mode:?} {pl:?}: the lied position's own check misses it (D = {d:#b})");
            if mode == Mode::SelfConsistent && !pl.derived {
                assert_eq!(d, 1 << pl.at.0, "{label} {pl:?}: a self-consistent lie is false at its own position only");
            }
            // The exact miss of every sample size equals the closed form.
            let k = d.count_ones();
            for s in 1..=P {
                let (miss, all) = miss_enumerated(d, s);
                assert_eq!((miss, all), (choose(P - k, s), choose(P, s)), "{label} {pl:?} s = {s}");
            }
            // Segment scopes (a Panel's partial seat checks one): detection is the real verifier's, per segment.
            for i in 0..segs {
                let sx = seen(c, &t, &m, &ScopeV1::Segments(vec![i]));
                let seg = ev.segment(i).unwrap();
                let covers = (seg.first..seg.end).any(|q| d & (1 << q) != 0);
                assert_eq!(detected(sx), covers, "{label} {mode:?} {pl:?}: segment {i} ({sx:?}) against D = {d:#b}");
            }
            st.placements += 1;
            st.min_d = st.min_d.min(k);
            st.max_d = st.max_d.max(k);
            worst_overall = worst_overall.max(P - k);
        }
    }
    assert_eq!(worst_overall, P - 1, "{label}: the worst placement is detected by exactly one position");
    stats
}

fn report(label: &str, stats: &BTreeMap<(String, &'static str), FamilyStats>) {
    for ((mode, fam), st) in stats {
        if st.placements == 0 {
            continue;
        }
        let worst: Vec<String> = (1..=P)
            .map(|s| {
                let k = st.min_d;
                format!("s={s}: {}/{}", choose(P - k, s), choose(P, s))
            })
            .collect();
        eprintln!(
            "[pesg-b] {label} {mode:<14} {fam:<16} placements {:>4}  |D| min {} max {}  unpropagatable {:>3}  worst miss {}",
            st.placements,
            st.min_d,
            st.max_d,
            st.unpropagatable,
            worst.join(", ")
        );
    }
}

/// **K2-TIR-v1 (one modulus): every fault in every position.** The whole-claim check detects and convicts all of them; the worst
/// placement (a self-consistent lie) is seen by exactly one position, so an `s`-of-`P` position sample misses it with exactly
/// `1 − s/P` — whatever the family (MatMul, quantization, nonlinear, routing, embedding, state).
#[test]
#[ignore = "exhaustive (minutes in release, hours in debug): cargo test --release -p misaka-palw-kernel --test pesg_b_detection -- --ignored"]
fn pesg_b_v1_every_fault_in_every_position_against_every_sample() {
    let c = Claim::honest();
    let stats = placement_search(&c, None, "K2-TIR-v1");
    report("K2-TIR-v1", &stats);
    for fam in ["dense-matrix", "quant-range", "nonlinear", "selection", "recurrent-state", "exact-arithmetic", "structure"] {
        let st = &stats[&("SelfConsistent".to_string(), fam)];
        assert!(st.placements > 0, "the fixture has {fam} values");
        assert_eq!(st.min_d, 1, "{fam}: some self-consistent lie is seen by one position only");
    }
}

/// **K2-TIR-v2 (multi-modulus CRT dense relation)**: the same search over the MatMul values (the only family whose checker differs).
#[test]
#[ignore = "exhaustive (minutes in release, hours in debug): cargo test --release -p misaka-palw-kernel --test pesg_b_detection -- --ignored"]
fn pesg_b_v2_every_matmul_fault_in_every_position() {
    let c = Claim::of(dense_moe_v1(7), k2_tir_v2_descriptor());
    let stats = placement_search(&c, Some(ConstraintFamilyV1::DenseMatrix), "K2-TIR-v2");
    report("K2-TIR-v2", &stats);
}

/// A scope's detection is the union of its positions' detection — checked with the real verifier on EVERY subset of positions, for
/// one placement of every family in both modes (so the closed form over single-position detection sets is the real verifier's).
#[test]
#[ignore = "exhaustive (minutes in release, hours in debug): cargo test --release -p misaka-palw-kernel --test pesg_b_detection -- --ignored"]
fn pesg_b_a_scope_detects_exactly_when_one_of_its_positions_does() {
    let c = Claim::honest();
    let honest = trace_v1(&c.program, &c.params, &TOKENS).unwrap();
    let mut seen_family: BTreeSet<(&'static str, bool)> = BTreeSet::new();
    let mut checked = 0;
    for pl in placements(&c.program, &honest) {
        if pl.at.0 != 2 || !seen_family.insert((pl.family.name(), pl.derived)) {
            continue;
        }
        for mode in [Mode::Isolated, Mode::SelfConsistent] {
            let Some(t) = fake(&c, &honest, &pl, mode) else { continue };
            let m = TraceMaterialV1 { trace: &t, params: &c.params };
            let d = detection_mask(&c, &t);
            for mask in 1u32..(1 << P) {
                let x = seen(&c, &t, &m, &ScopeV1::AuditOnly(positions_of(mask)));
                assert_eq!(detected(x), mask & d != 0, "{mode:?} {pl:?}: scope {mask:#b} against D = {d:#b} ({x:?})");
                checked += 1;
            }
        }
    }
    eprintln!("[pesg-b] union property: {checked} scope checks over every subset of {P} positions");
    assert!(checked >= 7 * 31);
}

/// **Structure and forgery faults.** A substituted weight used at every position (an operand fault of a whole relation) is seen by
/// every position: any one sample detects it. A borrowed trace (the honest trace of another job) that shares our prompt except its
/// LAST token is seen by the last position only — the worst placement — and a borrowed trace of an unrelated job by every position.
#[test]
fn pesg_b_operand_and_borrowed_trace_placements() {
    let c = Claim::honest();
    let honest = trace_v1(&c.program, &c.params, &TOKENS).unwrap();
    // The query projection's weight: a param every position's MatMul reads.
    let w = WiringV1::new(&c.program).unwrap();
    let mut whole_relation = 0;
    for (&(index, layer), t) in &c.params.tensors {
        let used_everywhere = (0..w.occurrences.len() as u16).any(|s| {
            let b = w.occurrences[s as usize].0 as usize;
            (0..c.program.blocks[b].nodes.len() as u16).any(|n| {
                matches!(w.node(s, n).prim, misaka_palw_tir::Prim::MatMul)
                    && (0..w.node(s, n).inputs.len()).any(|i| {
                        matches!(w.input_source(&TOKENS, 0, s, n, i), Ok(SourceV1::Param { index: j, layer: l }) if (j, l) == (index, layer))
                    })
            })
        });
        if !used_everywhere || t.len() < 2 {
            continue;
        }
        let mut substituted = c.params.clone();
        bump(substituted.tensors.get_mut(&(index, layer)).unwrap(), 0);
        let Ok(lie) = trace_v1(&c.program, &substituted, &TOKENS) else { continue };
        if lie == honest {
            continue;
        }
        // The producer serves the class's REGISTERED weight (anything else does not open to the commitment: unavailable, never a pass).
        let d = detection_mask(&c, &lie);
        assert_eq!(d, (1 << P) - 1, "a substituted weight {index} used at every position is seen by every position");
        whole_relation += 1;
    }
    assert!(whole_relation > 0, "the fixture has a weight every position multiplies by");
    // A borrowed trace: another job's honest trace, committed as ours.
    for (other, want) in [([3u32, 17, 9, 30, 2], 1u32 << (P - 1)), ([4, 18, 10, 31, 2], (1 << P) - 1)] {
        let borrowed = trace_v1(&c.program, &c.params, &other).unwrap();
        let d = detection_mask(&c, &borrowed);
        assert_eq!(d, want, "a borrowed trace of {other:?} is seen by {want:#b}, got {d:#b}");
        let m = TraceMaterialV1 { trace: &borrowed, params: &c.params };
        assert!(detected(seen(&c, &borrowed, &m, &ScopeV1::WholeClaim)));
    }
    eprintln!("[pesg-b] {whole_relation} substituted weights each seen by all {P} positions; a last-token borrowed trace by one");
}

// ── DA: withholding and partial or false disclosure ─────────────────────────────────────────────────────────────────────────────

/// Serves the committed trace except position `hide` (withheld), or except value `wrong` (served with one wrong element: a false
/// disclosure).
struct Disclosed<'a> {
    inner: TraceMaterialV1<'a>,
    hide: Option<u32>,
    wrong: Option<At>,
}

impl MaterialV1 for Disclosed<'_> {
    fn node_value(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
        if self.hide == Some(p) {
            return None;
        }
        let mut v = self.inner.node_value(p, s, n)?;
        if self.wrong == Some((p, s, n)) && !v.data.is_empty() {
            bump(&mut v, 0);
        }
        Some(v)
    }
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        self.inner.param(index, layer)
    }
}

/// **Withholding** a position is seen (as the DA path, never a pass and never a conviction of an honest claim) by exactly the scopes
/// that read it; withholding the last position is seen by one position only, so a sampled check misses it with `1 − s/P`. A **false
/// disclosure** (served bytes that do not open to the commitment) is unavailable, never a pass, never a conviction.
#[test]
#[ignore = "exhaustive (minutes in release, hours in debug): cargo test --release -p misaka-palw-kernel --test pesg_b_detection -- --ignored"]
fn pesg_b_withholding_and_false_disclosure_are_the_da_path_and_seen_only_where_read() {
    let c = Claim::honest();
    let honest = trace_v1(&c.program, &c.params, &TOKENS).unwrap();
    let mut min_d = u32::MAX;
    for hide in 0..P {
        let m = Disclosed { inner: TraceMaterialV1 { trace: &honest, params: &c.params }, hide: Some(hide), wrong: None };
        assert_eq!(seen(&c, &honest, &m, &ScopeV1::WholeClaim), Seen::Unavailable, "withholding is never a pass");
        let mut d = 0u32;
        for q in 0..P {
            match seen(&c, &honest, &m, &ScopeV1::AuditOnly(vec![q])) {
                Seen::Unavailable => d |= 1 << q,
                Seen::Pass => {}
                x => panic!("withholding position {hide} gave {x:?} at {q}: an honest claim is never convicted"),
            }
        }
        assert!(d & (1 << hide) != 0);
        eprintln!("[pesg-b] withholding position {hide}: seen by positions {:?}", positions_of(d));
        min_d = min_d.min(d.count_ones());
    }
    assert_eq!(min_d, 1, "the worst withholding is seen by one position");
    let w = WiringV1::new(&c.program).unwrap();
    let mut false_disclosures = 0;
    for s in 0..w.occurrences.len() as u16 {
        let b = w.occurrences[s as usize].0 as usize;
        for n in 0..c.program.blocks[b].nodes.len() as u16 {
            if w.is_derived(s, n) {
                continue; // never served: rebuilt from the rows
            }
            let m = Disclosed { inner: TraceMaterialV1 { trace: &honest, params: &c.params }, hide: None, wrong: Some((2, s, n)) };
            let x = seen(&c, &honest, &m, &ScopeV1::WholeClaim);
            assert_eq!(x, Seen::Unavailable, "a false disclosure of (2, {s}, {n}) is the DA path, got {x:?}");
            false_disclosures += 1;
        }
    }
    assert!(false_disclosures > 0);
}

// ── K2-TIR-v4 segmented claims: route A (sample positions) and route B (re-execute, compare roots) ───────────────────────────────

struct SegServed {
    values: Vec<Vec<Vec<Tensor>>>,
    c: SegmentedCommitmentsV1,
}

impl SegServed {
    fn of(values: Vec<Vec<Vec<Tensor>>>) -> Self {
        let c = seg_commitments_of_trace_v1(&TraceV1 { values: values.clone(), inputs: Vec::new() });
        SegServed { values, c }
    }
}

impl SegMaterialV1 for SegServed {
    fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>> {
        self.values.get(p as usize).cloned()
    }
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
        (p < self.c.positions()).then(|| self.c.position_path(p).1)
    }
}

struct SegWorld {
    program: TirProgramV1,
    params: MapParams,
    pc: ParamCommitmentsV1,
    prompt: Vec<u32>,
}

impl SegWorld {
    fn new() -> Self {
        let f = dense_moe_v1(7);
        let prompt = TOKENS[..TOKENS.len() - 1].to_vec();
        SegWorld { pc: ParamCommitmentsV1::of_v3(&f.params), program: f.program, params: f.params, prompt }
    }

    fn greedy(&self, t: &TraceV1, p: usize) -> u32 {
        let post = self.program.occurrences().len() - 1;
        DecodeRuleV1::Greedy.select(&t.values[p][post][self.program.logits as usize]).unwrap()
    }

    /// A claim: its fed ids, delivered ids and values. With `lie`, the producer is self-consistent when `consistent` (the fed and
    /// delivered ids follow its own lying logits), else only the value is changed.
    fn claim(&self, lie: Option<(At, usize)>, consistent: bool) -> Option<(Vec<u32>, Vec<u32>, TraceV1)> {
        let pre = trace_with_lie(&self.program, &self.params, &self.prompt, if consistent { lie } else { None })?;
        let g0 = self.greedy(&pre, self.prompt.len() - 1);
        let mut tokens = self.prompt.clone();
        tokens.push(g0);
        let honest_tail = trace_v1(&self.program, &self.params, &tokens).ok()?;
        let t = if consistent {
            trace_with_lie(&self.program, &self.params, &tokens, lie)?
        } else {
            let mut t = honest_tail.clone();
            if let Some((at, e)) = lie {
                bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], e);
            }
            t
        };
        let g1 = self.greedy(if consistent { &t } else { &honest_tail }, tokens.len() - 1);
        Some((tokens, vec![g0, g1], t))
    }

    fn ctx<'a>(&'a self, roots: &'a [Digest], generated: &'a [u32]) -> SegClaimContextV1<'a> {
        SegClaimContextV1 {
            program: &self.program,
            params: &self.pc,
            segment_roots: roots,
            positions: P,
            prompt_len: self.prompt.len() as u32,
            prompt_root: prompt_root_of_ids_v1(&self.prompt),
            inline_prompt: None,
            generated,
            decode: DecodeRuleV1::Greedy,
            encoder: None,
        }
    }
}

fn seg_found(ctx: &SegClaimContextV1<'_>, f: SegFindingV1) -> bool {
    match f {
        SegFindingV1::Clean => false,
        SegFindingV1::Fault(fault) => {
            verify_seg_fault_v1(ctx, &fault).unwrap_or_else(|e| panic!("a found fault must convict: {e:?}"));
            true
        }
        SegFindingV1::Demand(d) => panic!("every position is served, yet {d:?} is demanded"),
        SegFindingV1::Inconsistent(why) => panic!("verifier inconsistency: {why}"),
    }
}

/// **K2-TIR-v4: route A against route B, every fault in every position, and the last delivered token.** Route A (check sampled
/// positions, element courts) sees a self-consistent lie at one position only: miss `1 − m/P` for `m` sampled positions. Route B
/// (re-execute the claim's fed ids and compare roots) finds every placement with certainty and convicts it.
#[test]
#[ignore = "exhaustive (minutes in release, hours in debug): cargo test --release -p misaka-palw-kernel --test pesg_b_detection -- --ignored"]
fn pesg_b_v4_route_a_samples_and_route_b_reexecution() {
    let w = SegWorld::new();
    let (tokens, generated, honest) = w.claim(None, false).unwrap();
    let art = |j: u16, l: Option<u16>| w.params.tensors.get(&(j, l)).cloned();
    let own_roots = seg_commitments_of_trace_v1(&honest).position_roots;
    let mut stats: BTreeMap<(&'static str, &'static str), (u32, u32)> = BTreeMap::new();
    let mut worst = 0u32;
    for pl in placements(&w.program, &honest) {
        for consistent in [false, true] {
            let Some((toks, gen_ids, t)) = w.claim(Some((pl.at, pl.elem)), consistent) else { continue };
            if t == honest && gen_ids == generated {
                continue;
            }
            let served = SegServed::of(t.values.clone());
            let roots = served.c.segment_roots();
            let ctx = w.ctx(&roots, &gen_ids);
            let mut d = 0u32;
            for q in 0..P {
                if seg_found(&ctx, check_positions_v1(&ctx, &served, &art, &toks, &[q])) {
                    d |= 1 << q;
                }
            }
            assert!(d != 0, "{pl:?} consistent={consistent}: route A over every position misses");
            // Route B: the verifier's own re-execution of the claim's fed ids.
            let own_trace = trace_v1(&w.program, &w.params, &toks).unwrap();
            let own = if toks == tokens { own_roots.clone() } else { seg_commitments_of_trace_v1(&own_trace).position_roots };
            let post = w.program.occurrences().len() - 1;
            let logits = |p: u32| own_trace.values.get(p as usize).map(|v| v[post][w.program.logits as usize].clone());
            let mismatch = first_decode_mismatch_v1(&ctx, &logits);
            let r = check_claim_by_reexecution_v1(&ctx, &own, mismatch, &served, &art, &toks);
            assert!(seg_found(&ctx, r.finding.clone()), "{pl:?} consistent={consistent}: route B missed ({r:?})");
            let key = (if consistent { "SelfConsistent" } else { "Isolated" }, pl.family.name());
            let e = stats.entry(key).or_insert((0, u32::MAX));
            e.0 += 1;
            e.1 = e.1.min(d.count_ones());
            worst = worst.max(P - d.count_ones());
        }
    }
    for ((mode, fam), (n, min_d)) in &stats {
        eprintln!(
            "[pesg-b] K2-TIR-v4 route A {mode:<14} {fam:<16} placements {n:>4}  |D| min {min_d}  worst miss of m=1: {}/{P}",
            P - min_d
        );
    }
    assert_eq!(worst, P - 1, "route A's worst placement is seen by one position");
    // The last delivered token: a decode lie over honest values is seen only where it is selected (the last position).
    let mut cheat = generated.clone();
    cheat[1] = (cheat[1] + 1) % w.program.token_bound;
    let served = SegServed::of(honest.values.clone());
    let roots = served.c.segment_roots();
    let ctx = w.ctx(&roots, &cheat);
    let d: Vec<u32> = (0..P).filter(|q| seg_found(&ctx, check_positions_v1(&ctx, &served, &art, &tokens, &[*q]))).collect();
    assert_eq!(d, vec![P - 1], "a last-token decode lie is seen by the last position only");
    let post = w.program.occurrences().len() - 1;
    let logits = |p: u32| honest.values.get(p as usize).map(|v| v[post][w.program.logits as usize].clone());
    let mismatch = first_decode_mismatch_v1(&ctx, &logits);
    assert_eq!(mismatch, Some(P - 1));
    let r = check_claim_by_reexecution_v1(&ctx, &own_roots, mismatch, &served, &art, &tokens);
    assert!(seg_found(&ctx, r.finding));
}

// ── the algebraic residual, exhaustively, in toy Mersenne fields ────────────────────────────────────────────────────────────────

fn f<const E: u32>(v: i64) -> Mersenne<E> {
    Mersenne::<E>::from_i128(v as i128)
}

fn finv<const E: u32>(a: Mersenne<E>) -> Mersenne<E> {
    // a^(p − 2)
    let mut e = Mersenne::<E>::MODULUS - 2;
    let (mut base, mut acc) = (a, Mersenne::<E>::ONE);
    while e > 0 {
        if e & 1 == 1 {
            acc = acc.fmul(base);
        }
        base = base.fmul(base);
        e >>= 1;
    }
    acc
}

/// The rank of an integer matrix reduced modulo `2^E − 1`.
fn rank_mod<const E: u32>(m: &[Vec<i64>]) -> usize {
    let mut a: Vec<Vec<Mersenne<E>>> = m.iter().map(|r| r.iter().map(|v| f::<E>(*v)).collect()).collect();
    let cols = a.first().map_or(0, Vec::len);
    let mut rank = 0;
    for col in 0..cols {
        let Some(piv) = (rank..a.len()).find(|&i| a[i][col] != Mersenne::<E>::ZERO) else { continue };
        a.swap(rank, piv);
        let inv = finv(a[rank][col]);
        for i in 0..a.len() {
            if i != rank && a[i][col] != Mersenne::<E>::ZERO {
                let factor = a[i][col].fmul(inv);
                for j in 0..cols {
                    let t = a[rank][j].fmul(factor);
                    a[i][j] = a[i][j].fsub(t);
                }
            }
        }
        rank += 1;
    }
    rank
}

/// Every vector of `GF(2^E − 1)^n`.
fn every_vector<const E: u32>(n: usize) -> Vec<Vec<Mersenne<E>>> {
    let p = Mersenne::<E>::MODULUS as u64;
    (0..p.pow(n as u32))
        .map(|mut i| {
            (0..n)
                .map(|_| {
                    let v = i % p;
                    i /= p;
                    Mersenne::<E>::from_canonical(v as u128).unwrap()
                })
                .collect()
        })
        .collect()
}

fn matmul(x: &[Vec<i64>], w: &[Vec<i64>]) -> Vec<Vec<i64>> {
    x.iter().map(|row| (0..w[0].len()).map(|j| row.iter().zip(w).map(|(a, wr)| a * wr[j]).sum()).collect()).collect()
}

/// The right-projection check `X (W r) = Y r`, every row compared (the kernel's `first_bad_row` per row).
fn right_passes<const E: u32>(x: &[Vec<i64>], w: &[Vec<i64>], y: &[Vec<i64>], r: &[Mersenne<E>]) -> bool {
    let wr: Vec<Mersenne<E>> = w.iter().map(|row| Mersenne::<E>::dot(row.iter().map(|v| f::<E>(*v)), r.iter().copied())).collect();
    x.iter().zip(y).all(|(xr, yr)| {
        Mersenne::<E>::dot(xr.iter().map(|v| f::<E>(*v)), wr.iter().copied())
            == Mersenne::<E>::dot(yr.iter().map(|v| f::<E>(*v)), r.iter().copied())
    })
}

/// The left-projection check `(rᵀ W) X = rᵀ Y` for `Y = W X` (the kernel's GEMV lowering, every column compared).
fn left_passes<const E: u32>(w: &[Vec<i64>], x: &[Vec<i64>], y: &[Vec<i64>], r: &[Mersenne<E>]) -> bool {
    let rw: Vec<Mersenne<E>> =
        (0..w[0].len()).map(|k| Mersenne::<E>::dot(r.iter().copied(), w.iter().map(|row| f::<E>(row[k])))).collect();
    (0..y[0].len()).all(|j| {
        Mersenne::<E>::dot(rw.iter().copied(), x.iter().map(|row| f::<E>(row[j])))
            == Mersenne::<E>::dot(r.iter().copied(), y.iter().map(|row| f::<E>(row[j])))
    })
}

/// Every error matrix of `rows × cols` with entries in `alphabet`, nonzero.
fn every_error(rows: usize, cols: usize, alphabet: &[i64]) -> Vec<Vec<Vec<i64>>> {
    let a = alphabet.len();
    (0..a.pow((rows * cols) as u32))
        .filter_map(|mut i| {
            let e: Vec<Vec<i64>> = (0..rows)
                .map(|_| {
                    (0..cols)
                        .map(|_| {
                            let v = alphabet[i % a];
                            i /= a;
                            v
                        })
                        .collect()
                })
                .collect();
            e.iter().flatten().any(|v| *v != 0).then_some(e)
        })
        .collect()
}

fn add(a: &[Vec<i64>], e: &[Vec<i64>]) -> Vec<Vec<i64>> {
    a.iter().zip(e).map(|(r, er)| r.iter().zip(er).map(|(x, y)| x + y).collect()).collect()
}

/// For every error pattern `E` (every placement: one element, a row, a column, several positions stacked, rank 2) and every
/// challenge vector, the number of passing vectors is exactly `p^(n − rank_p(E))`: the miss is `p^-rank`, at most `1/p`, and the
/// worst placement is any rank-1 error (one element, one row, or one column across any number of stacked positions). An error that
/// is a multiple of `p` (an integer error the field aliases away) passes every vector — the reason admission refuses any span ≥ p.
fn exhaustive_freivalds<const E: u32>(rows: usize, k: usize, n: usize, alphabet: &[i64]) -> (usize, u128) {
    let p = Mersenne::<E>::MODULUS;
    let x: Vec<Vec<i64>> = (0..rows).map(|i| (0..k).map(|j| (i * 3 + j * 5 + 1) as i64 % 9 - 4).collect()).collect();
    let w: Vec<Vec<i64>> = (0..k).map(|i| (0..n).map(|j| (i * 7 + j * 2 + 3) as i64 % 11 - 5).collect()).collect();
    let y = matmul(&x, &w);
    let vectors = every_vector::<E>(n);
    assert!(vectors.iter().all(|r| right_passes(&x, &w, &y, r)), "completeness: the honest product passes every vector");
    let mut worst: u128 = 0;
    let errors = every_error(rows, n, alphabet);
    for e in &errors {
        let lie = add(&y, e);
        let passes = vectors.iter().filter(|r| right_passes(&x, &w, &lie, r)).count() as u128;
        let rank = rank_mod::<E>(e) as u32;
        assert_eq!(passes, p.pow(n as u32 - rank), "GF({p}) E = {e:?}: rank {rank}");
        worst = worst.max(passes);
    }
    // The left (GEMV) variant over `Y = W X`: the challenge is on the rows of W.
    let wl: Vec<Vec<i64>> = (0..rows).map(|i| (0..k).map(|j| (i * 5 + j * 3 + 2) as i64 % 7 - 3).collect()).collect();
    let xl: Vec<Vec<i64>> = (0..k).map(|i| (0..n).map(|j| (i * 2 + j * 7 + 1) as i64 % 9 - 4).collect()).collect();
    let yl = matmul(&wl, &xl);
    let lvec = every_vector::<E>(rows);
    for e in &errors {
        let lie = add(&yl, e);
        let passes = lvec.iter().filter(|r| left_passes(&wl, &xl, &lie, r)).count() as u128;
        let rank = rank_mod::<E>(e) as u32;
        assert_eq!(passes, p.pow(rows as u32 - rank), "left GF({p}) E = {e:?}");
    }
    (errors.len(), worst)
}

#[test]
#[ignore = "exhaustive (minutes in release, hours in debug): cargo test --release -p misaka-palw-kernel --test pesg_b_detection -- --ignored"]
fn pesg_b_freivalds_miss_is_exactly_p_to_the_minus_rank_for_every_error_placement() {
    // GF(7): 3 stacked rows (three positions of one batched weight product) × 2 columns, entries in {−1, 0, 1, 7}: 4^6 − 1 patterns.
    let (n7, worst7) = exhaustive_freivalds::<3>(3, 2, 2, &[0, 1, -1, 7]);
    // GF(31): 2 × 3, entries in {0, 1, −2, 31}.
    let (n31, worst31) = exhaustive_freivalds::<5>(2, 2, 3, &[0, 1, -2, 31]);
    eprintln!(
        "[pesg-b] Freivalds exhaustive: GF(7) {n7} error patterns × 49 vectors (worst non-alias miss 1/7), GF(31) {n31} × 29,791 \
         (worst non-alias miss 1/31); aliasing errors (multiples of p) pass every vector: max passes {worst7}, {worst31}"
    );
    assert_eq!(worst7, 49, "an error ≡ 0 mod 7 passes every vector (alias)");
    assert_eq!(worst31, 31u128.pow(3));
    // Without aliasing entries, the worst is exactly one rank: miss 1/p.
    let (_, w7) = exhaustive_freivalds::<3>(3, 2, 2, &[0, 1, -1]);
    assert_eq!(w7, 7, "the worst non-aliasing placement misses with exactly 1/7");
}

/// **The CRT composition** (K2-TIR-v2, toy moduli 7 and 31, product 217), exhaustively over both moduli's challenges for a single
/// element error `e`: the miss is `Π_q (q | e ? 1 : 1/q)`, at most `1/7` (the smallest modulus) for every `0 < |e| < 217`, and 1
/// at `e = 217` — the hole admission's `span < Π q` rule closes.
#[test]
fn pesg_b_crt_miss_is_the_smallest_moduluss_below_the_product() {
    let mut worst = 0u32;
    for e in -434i64..=434 {
        if e == 0 {
            continue;
        }
        let mut passes = 0u32;
        for r7 in 0..7u128 {
            for r31 in 0..31u128 {
                let a = f::<3>(e).fmul(Mersenne::<3>::from_canonical(r7).unwrap()) == Mersenne::<3>::ZERO;
                let b = f::<5>(e).fmul(Mersenne::<5>::from_canonical(r31).unwrap()) == Mersenne::<5>::ZERO;
                passes += u32::from(a && b);
            }
        }
        let want = (if e % 7 == 0 { 7 } else { 1 }) * (if e % 31 == 0 { 31 } else { 1 });
        assert_eq!(passes, want, "e = {e}");
        if e.abs() < 217 {
            worst = worst.max(passes);
        }
    }
    assert_eq!(worst, 31, "below the product the worst miss is 31/217 = 1/7");
}

// ── the samplers ────────────────────────────────────────────────────────────────────────────────────────────────────────────────

/// The kernel's field sampler (`FieldElemV1::of_word`, used by `ChallengeStreamV1::next_in`) maps the low `E` bits of a word to
/// `GF(2^E − 1)` and rejects exactly one pattern: exhaustively over every low pattern, under three high-bit patterns, each field
/// element has exactly one preimage — exact uniformity given uniform words.
fn of_word_exact<const E: u32>() {
    let p = Mersenne::<E>::MODULUS;
    for high in [0u128, 1u128 << E, u128::MAX & !p] {
        let mut hits = vec![0u32; p as usize];
        let mut rejected = 0;
        for low in 0..=p {
            match <Mersenne<E> as FieldElemV1>::of_word(high | low) {
                Some(v) => hits[v.value() as usize] += 1,
                None => rejected += 1,
            }
        }
        assert!(hits.iter().all(|h| *h == 1), "E = {E}: every element exactly once");
        assert_eq!(rejected, 1, "E = {E}: only p is rejected");
    }
}

#[test]
fn pesg_b_the_field_sampler_is_exactly_uniform_and_the_streams_reach_every_element() {
    of_word_exact::<2>();
    of_word_exact::<3>();
    of_word_exact::<5>();
    of_word_exact::<7>();
    // The real stream reaches every element of GF(7) and GF(31) (support), and two labels never share a stream.
    let seed = [0x5A; 64];
    let label = ChallengeLabelV1 { kind: 0, position: 0, occurrence: 0, node: 0, repetition: 0, slice: 0 };
    let v7: BTreeSet<u128> = KernelStream::new(seed, label).vector_in::<Mersenne<3>>(400).iter().map(|x| x.value()).collect();
    let v31: BTreeSet<u128> = KernelStream::new(seed, label).vector_in::<Mersenne<5>>(2_000).iter().map(|x| x.value()).collect();
    assert_eq!((v7.len(), v31.len()), (7, 31));
    let a = KernelStream::new(seed, label).vector_in::<Mersenne<5>>(64);
    let b = KernelStream::new(seed, ChallengeLabelV1 { repetition: 1, ..label }).vector_in::<Mersenne<5>>(64);
    assert_ne!(a, b);
}

/// The challenge contract's subset sampler (`ChallengeStreamV1::distinct_indices`, Floyd over the rejection sampler `index_below`;
/// exactly uniform by construction, dossier S6.4) reaches **every** `s`-subset of `P` positions — no position or subset is a blind
/// spot a producer could place a fault in. The frequencies are a sanity check of support only, never a bound.
#[test]
fn pesg_b_the_subset_sampler_reaches_every_subset() {
    use misaka_palw_challenge::{ChallengeStreamV1, StreamKindV1, StreamLabelV1};
    for s in 1..=P {
        let mut counts: BTreeMap<Vec<u64>, u32> = BTreeMap::new();
        let trials = 4_000u32;
        for i in 0..trials {
            let mut seed = [0u8; 64];
            seed[..4].copy_from_slice(&i.to_le_bytes());
            let label = StreamLabelV1 { kind: StreamKindV1::Query, scope_id: [7; 64], relation: 0, repetition: 0 };
            let mut st = ChallengeStreamV1::new(&seed, &label);
            let pick = st.distinct_indices(P as u64, s as u64).unwrap();
            assert!(pick.windows(2).all(|w| w[0] < w[1]), "sorted and distinct");
            *counts.entry(pick).or_default() += 1;
        }
        assert_eq!(counts.len() as u128, choose(P, s), "s = {s}: every subset is reachable");
        let expect = trials as f64 / choose(P, s) as f64;
        assert!(counts.values().all(|c| (*c as f64 - expect).abs() < 0.25 * expect + 30.0), "s = {s}: {counts:?}");
    }
}

// ── adaptive retries, grinding, correlated watchers and claim-draw vetoes, over their joint sample spaces ───────────────────────

/// **Grinding a sampled interval check is catastrophic; grinding an algebraic check is linear.** An adversary that can take the best
/// of `G` independent samples (seeds it grinds, retries, vetoes, reorg re-draws) escapes a one-position lie unless every sample hits
/// it: `1 − (s/P)^G` (enumerated over all `C(P,s)^G` sample tuples). Against a rank-1 Freivalds error over GF(7) it escapes with
/// `1 − (1 − 1/7)^G ≤ G/7` (enumerated over all `7^G` challenges).
#[test]
fn pesg_b_grinding_retries_correlation_and_vetoes_enumerated() {
    let s = 2;
    let samples = subsets(P, s);
    let d = 1u32 << (P - 1);
    for g in 1..=3u32 {
        let mut escape = 0u128;
        let total = (samples.len() as u128).pow(g);
        for t in 0..total {
            let mut idx = t;
            let mut any_miss = false;
            for _ in 0..g {
                any_miss |= samples[(idx % samples.len() as u128) as usize] & d == 0;
                idx /= samples.len() as u128;
            }
            escape += u128::from(any_miss);
        }
        let hit = choose(P - 1, s - 1); // samples containing the lied position
        assert_eq!(escape, total - hit.pow(g), "G = {g}: 1 − (s/P)^G");
        let mut fe = 0u128;
        for t in 0..7u128.pow(g) {
            let mut idx = t;
            let mut any_pass = false;
            for _ in 0..g {
                any_pass |= idx % 7 == 0; // the rank-1 error's coordinate drawn as 0: the check passes
                idx /= 7;
            }
            fe += u128::from(any_pass);
        }
        assert_eq!(fe, 7u128.pow(g) - 6u128.pow(g), "G = {g}: 1 − (6/7)^G");
        assert!(fe * 7 <= g as u128 * 7u128.pow(g), "≤ G/p");
    }
    // Correlation: two watchers drawing from ONE seed draw the same sample (joint miss = one miss); two private salts are independent.
    let miss_one = samples.iter().filter(|m| *m & d == 0).count() as u128;
    let shared = samples.iter().filter(|m| *m & d == 0).count() as u128; // the diagonal of the joint space
    let independent = (0..samples.len()).flat_map(|i| (0..samples.len()).map(move |j| (i, j)));
    let indep_miss = independent.filter(|(i, j)| samples[*i] & d == 0 && samples[*j] & d == 0).count() as u128;
    assert_eq!((shared, samples.len() as u128), (miss_one, choose(P, s)), "a shared seed: the second watcher adds nothing");
    assert_eq!(indep_miss, miss_one * miss_one, "independent salts: the misses multiply");
    // A post-commit claim draw (checked with probability 1/4: `index_below(4) == 0`) that the producer may veto R times (each veto a
    // re-draw): it escapes unless all R + 1 draws say "checked" — detection `q^(R+1)`.
    for r in 0..=3u32 {
        let total = 4u128.pow(r + 1);
        let caught = (0..total)
            .filter(|t| {
                let mut idx = *t;
                (0..=r).all(|_| {
                    let ok = idx % 4 == 0;
                    idx /= 4;
                    ok
                })
            })
            .count() as u128;
        assert_eq!(caught, 1, "R = {r}: detection 4^-(R+1)");
    }
}
