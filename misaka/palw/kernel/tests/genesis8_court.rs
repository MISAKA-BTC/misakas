//! **Genesis 8, per architecture: convert → TIR → execution → fault injection → localization → court verdict.**
//!
//! Each genesis architecture that is adjudicated on the TIR route, at toy scale (tiny synthetic checkpoints with the real
//! structure), is lowered by the same frontend the converter uses, quantised from a calibration run and materialised to
//! integer params; the reference evaluator produces the honest trace; under K2-TIR (the multi-modulus dense family, v2):
//!
//! * the honest claim passes with its derived bound;
//! * a lie at one committed value of EVERY constraint family the program uses (dense matrix, nonlinear, selection, quant
//!   range, exact arithmetic, recurrent state) is localized to that primitive instance and convicted by a court that holds
//!   only public material;
//! * a false accusation against the honest claim is dismissed;
//! * every family the program uses has a checker and a court in K2-TIR-v4 (real scale), and every state has state continuity.
//!
//! | genesis | architecture | toy checkpoint |
//! |---|---|---|
//! | #1 (TIR route of its arch; #1 itself ships A16) | Qwen2 dense (sliding window) | `tests/fixtures/hf/qwen2_sliding` |
//! | #2 #3 #6 | Qwen3.5 / 3.8 Gated-DeltaNet hybrid (default and `gdn_core_wide`) | `tests/fixtures/hf/qwen3_5` |
//! | #8 (TIR route of its arch; #8 ships PALWQ361) | Qwen3.5-MoE GDN hybrid + MoE | `tests/fixtures/hf/qwen3_5_moe` |
//! | #4 | Gemma-4 Unified (sliding/full, k = v, soft-cap, sandwich norms) | `tests/fixtures/genesis8-tiny/gemma4_unified` |
//! | #5 | gpt-oss (MoE, sinks, alternating sliding window, YaRN, clamped SwiGLU) | `tests/fixtures/hf/gpt_oss` |
//! | #10 (not required e2e) | Llama 3 dense (llama3 rope) | `tests/fixtures/hf/llama` |
//!
//! Correctness is decided by deterministic recomputation and spot checks (Freivalds for products), never by a cryptographic
//! proof of computation (PRINCIPLES §8). Written, not run here: the validation lane compiles and runs it.

mod common;

use common::*;
use misaka_palw_kernel::check::registration_outcome_v1;
use misaka_palw_kernel::descriptor::{k2_tir_v2_descriptor, k2_tir_v4_descriptor};
use misaka_palw_kernel::family::{CheckerIdV1, ConstraintFamilyV1, family_of_prim};
use misaka_palw_kernel::merkle::TensorOpeningV1;
use misaka_palw_kernel::outcome::RegistrationOutcomeV1;
use misaka_palw_kernel::trace::TraceV1;
use misaka_palw_kernel::verify::{ClaimVerdictV1, DismissalV1, TraceMaterialV1};
use misaka_palw_tir::{MapParams, ParamSource, Prim};
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_sketch::fixture::TirSketchFixtureV1;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct Arch {
    genesis: &'static str,
    dir: PathBuf,
    opts: LowerOpts,
}

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tir/lower/tests/fixtures").join(rel)
}

fn archs() -> Vec<Arch> {
    let wide = LowerOpts { gdn_core_wide: true, ..LowerOpts::default() };
    vec![
        Arch { genesis: "#1 qwen2 dense", dir: fixture("hf/qwen2_sliding"), opts: LowerOpts::default() },
        Arch { genesis: "#2/#3/#6 qwen3.5 GDN hybrid", dir: fixture("hf/qwen3_5"), opts: LowerOpts::default() },
        Arch { genesis: "#2/#3/#6 qwen3.5 GDN hybrid (gdn_core_wide)", dir: fixture("hf/qwen3_5"), opts: wide },
        Arch { genesis: "#8 qwen3.5-moe GDN hybrid MoE", dir: fixture("hf/qwen3_5_moe"), opts: LowerOpts::default() },
        Arch { genesis: "#4 gemma-4 unified", dir: fixture("genesis8-tiny/gemma4_unified"), opts: LowerOpts::default() },
        Arch { genesis: "#5 gpt-oss MoE", dir: fixture("hf/gpt_oss"), opts: LowerOpts::default() },
        Arch { genesis: "#10 llama 3", dir: fixture("hf/llama"), opts: LowerOpts::default() },
    ]
}

/// The toy checkpoint lowered, calibrated and materialised exactly as `palw-tir-convert` does it (minus the container).
fn lowered(name: &str, dir: &Path, opts: &LowerOpts) -> TirSketchFixtureV1 {
    let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap_or_else(|e| panic!("{name}: {e}"));
    let prep = fidelity::prepare(&cfg, opts).unwrap_or_else(|e| panic!("{name}: the frontend refuses the architecture: {e}"));
    let ck = Checkpoint::open(dir).unwrap_or_else(|e| panic!("{name}: {e}"));
    let (store, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap_or_else(|e| panic!("{name}: {e}"));
    let loader = Resident(Arc::new(store));
    let calib = fidelity::random_sequences(prep.hl.vocab, 4, 16, 7);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).unwrap_or_else(|e| panic!("{name}: calibrate: {e}"));
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet)
        .unwrap_or_else(|e| panic!("{name}: materialise: {e}"));
    let mut params = MapParams::default();
    for k in mat.params.tensors.keys() {
        params.tensors.insert(*k, mat.params.param(k.0, k.1).expect("an instance the materialisation made"));
    }
    TirSketchFixtureV1 { program: prep.lowered.program, params }
}

/// The first `(position, occurrence, node)` from position `from` whose primitive is of `family` and whose value has an element.
fn find(c: &Claim, from: u32, family: ConstraintFamilyV1) -> Option<(u32, u16, u16)> {
    let occ = c.program.occurrences();
    for p in from..TOKENS.len() as u32 {
        for (s, (b, _)) in occ.iter().enumerate() {
            for (n, node) in c.program.blocks[*b as usize].nodes.iter().enumerate() {
                if family_of_prim(&node.prim) == family && !c.trace.values[p as usize][s][n].data.is_empty() {
                    return Some((p, s as u16, n as u16));
                }
            }
        }
    }
    None
}

fn lie_at(c: &Claim, at: (u32, u16, u16)) -> TraceV1 {
    let mut t = c.trace.clone();
    bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], 0);
    t
}

const FAMILIES: [ConstraintFamilyV1; 6] = [
    ConstraintFamilyV1::DenseMatrix,
    ConstraintFamilyV1::Nonlinear,
    ConstraintFamilyV1::Selection,
    ConstraintFamilyV1::QuantRange,
    ConstraintFamilyV1::ExactArithmetic,
    ConstraintFamilyV1::RecurrentState,
];

#[test]
fn every_family_a_genesis_program_uses_has_a_checker_and_a_court_in_k2_tir_v4() {
    let d = k2_tir_v4_descriptor();
    for a in archs() {
        let fx = lowered(a.genesis, &a.dir, &a.opts);
        let mut used: BTreeMap<&'static str, usize> = BTreeMap::new();
        for block in &fx.program.blocks {
            for node in &block.nodes {
                let f = family_of_prim(&node.prim);
                *used.entry(f.name()).or_default() += 1;
                assert!(d.support(f).is_some(), "{}: family {} has no checker in K2-TIR-v4", a.genesis, f.name());
            }
        }
        if !fx.program.states.is_empty() {
            assert_eq!(
                d.support(ConstraintFamilyV1::RecurrentState).map(|s| s.checker),
                Some(CheckerIdV1::StateContinuity),
                "{}: its states need state continuity",
                a.genesis
            );
        }
        eprintln!("{}: {} blocks, {} states, families {used:?}", a.genesis, fx.program.blocks.len(), fx.program.states.len());
    }
}

#[test]
fn every_genesis_architecture_is_eligible_under_an_armed_k2_tir_v2() {
    let d = k2_tir_v2_descriptor();
    for a in archs() {
        let fx = lowered(a.genesis, &a.dir, &a.opts);
        let o = registration_outcome_v1(&active_for(&d), &d, &fx.program, root_of(&fx.program), MAX_POSITIONS, 1_000);
        let RegistrationOutcomeV1::EligibleAt { error_bits, .. } = o else { panic!("{}: {o}", a.genesis) };
        assert!(error_bits >= 128, "{}: 2^-{error_bits}", a.genesis);
    }
}

#[test]
fn an_honest_genesis_claim_passes_and_a_lie_in_every_family_is_localized_and_convicted() {
    for a in archs() {
        let c = Claim::of(lowered(a.genesis, &a.dir, &a.opts), k2_tir_v2_descriptor());
        let honest = TraceMaterialV1 { trace: &c.trace, params: &c.params };
        let (v, honest_court) = c.verify_with(&c.trace, &honest);
        assert!(matches!(v, ClaimVerdictV1::Pass { .. }), "{}: the honest claim: {v:?}", a.genesis);
        let mut convicted = Vec::new();
        for family in FAMILIES {
            let Some(at) = find(&c, 1, family) else { continue };
            let lie = lie_at(&c, at);
            let material = TraceMaterialV1 { trace: &lie, params: &c.params };
            let (v, court) = c.verify_with(&lie, &material);
            let ClaimVerdictV1::Fault(proof) = v else { panic!("{} / {}: a lie at {at:?} passed: {v:?}", a.genesis, family.name()) };
            assert_eq!((proof.position, proof.occurrence, proof.node), at, "{} / {}: localized elsewhere", a.genesis, family.name());
            let conviction =
                court(&proof).unwrap_or_else(|e| panic!("{} / {}: the public court dismissed a true fault: {e:?}", a.genesis, family.name()));
            assert_eq!((conviction.position, conviction.occurrence, conviction.node), at);
            // The same accusation against the honest claim is dismissed (a dense-matrix scalar recomputes; any other relation
            // recomputes from the honest openings).
            if family == ConstraintFamilyV1::DenseMatrix {
                let mut false_accusation = (*proof).clone();
                let honest_y = &c.trace.values[at.0 as usize][at.1 as usize][at.2 as usize];
                let o = false_accusation.scalar.as_mut().expect("a MatMul scalar is proved by its openings");
                o.y = TensorOpeningV1::row(honest_y, o.y.index).unwrap();
                assert_eq!(honest_court(&false_accusation).unwrap_err(), DismissalV1::NoFault, "{}", a.genesis);
            }
            convicted.push(family.name());
        }
        assert!(convicted.contains(&"dense-matrix"), "{}: no product was tried", a.genesis);
        eprintln!("{}: convicted lies in {convicted:?}", a.genesis);
    }
}

#[test]
fn the_gdn_hybrid_carries_recurrent_state_and_its_lie_is_convicted() {
    // The Gated-DeltaNet recurrence is the genesis set's one fixed-size state (#2 #3 #6 #8): it must be a committed state
    // whose continuity the kernel checks, not an uncommitted carry.
    for a in archs().into_iter().filter(|a| a.genesis.contains("GDN")) {
        let fx = lowered(a.genesis, &a.dir, &a.opts);
        assert!(!fx.program.states.is_empty(), "{}: the recurrence lowers to a state", a.genesis);
        let c = Claim::of(fx, k2_tir_v2_descriptor());
        let at = find(&c, 1, ConstraintFamilyV1::RecurrentState).unwrap_or_else(|| panic!("{}: no state write", a.genesis));
        let lie = lie_at(&c, at);
        let material = TraceMaterialV1 { trace: &lie, params: &c.params };
        let (v, court) = c.verify_with(&lie, &material);
        let ClaimVerdictV1::Fault(proof) = v else { panic!("{}: {v:?}", a.genesis) };
        court(&proof).unwrap_or_else(|e| panic!("{}: {e:?}", a.genesis));
        assert!(matches!(c.program.blocks[c.program.occurrences()[at.1 as usize].0 as usize].nodes[at.2 as usize].prim,
            Prim::StateWrite { .. } | Prim::HistAppend { .. }));
    }
}
