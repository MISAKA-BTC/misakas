//! **ADR-0067 Decision 5's fuzz gate: the profile space, driven through the interpreter.**
//!
//! The fence's arming condition, in the ADR's words: arbitrary GATE-ACCEPTED profiles through the
//! interpreter, to saturation, with zero panics and zero non-determinism. This module is that
//! harness — deliberately deterministic (a seeded xorshift, no clocks, no thread randomness), so
//! a failing seed is a repro command and not a war story.
//!
//! What one iteration does:
//!
//! 1. mutate the corrected A16 profile (node swaps, deletions, duplications, input-ref rewrites,
//!    kernel/operand/width/dtype edits — including FOREIGN kernels and names, because the gate
//!    must be allowed to accept things the planner refuses);
//! 2. run the ADMISSION gate (`verify_class_admission_v2`, the same function the chain runs, over
//!    the same probe object the SDK's preflight builds). Gate-refused → the mutation is outside
//!    the space this ADR promises anything about; counted and skipped.
//! 3. gate-accepted → compile the plan. A refusal here is LEGAL (a registrable class this build
//!    cannot serve is economically dead, not unsound — Decision 6 makes possession opt-in), but
//!    it is counted, because a wide gap between the two gates is a usability defect worth seeing.
//! 4. gate-accepted → derive the COURT cost (`derive_court_cost_v1`, the gate's own function) and
//!    measure the worst close against `PALW_RC_COURT_MAX_CLOSE_BYTES`. A close over the ceiling
//!    is a finding: an admitted class whose disputes cannot be carried executes, certifies, and
//!    can never be policed — which is a defect of the adjudication half, invisible to any amount
//!    of executing.
//! 5. plan-compiled → execute a caller's prompt TWICE, under `catch_unwind`. A panic is a
//!    finding. A bit-difference between the two runs is a finding. Both are the whole point.
//!
//! `fuzz_a16_profiles_v1` returns the tally; the CI test bounds it small, and the
//! `palw-a16-profile-fuzz` binary runs it to saturation with the seed printed.

use crate::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
use crate::engine_a16::{A16Engine, derived_a16_store};
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v2};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, PalwStepNodeV1, PalwStepOutLenV1, kernel_semantics_id_v1};

/// Deterministic xorshift64*. Not cryptographic, not shared with anything consensus — a fuzz
/// schedule generator whose whole virtue is that a seed is a repro.
pub struct FuzzRng(u64);

impl FuzzRng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    pub(crate) fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// One saturation run's honest arithmetic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FuzzTallyV1 {
    pub iterations: u64,
    pub gate_refused: u64,
    pub plan_refused_after_gate: u64,
    pub executed: u64,
    /// Profiles whose court cost was derived and measured against the ceiling. Every executed
    /// profile is also costed, so this tracks `executed` unless the derivation itself refused.
    pub court_costed: u64,
    /// The findings. A non-zero in ANY of these is the fence staying down.
    pub panics: u64,
    pub nondeterminism: u64,
    /// **A profile the admission gate ACCEPTED whose worst close does not fit the carrier.**
    /// Decision 5 names this as its own arming clause, and for a reason the audit found an
    /// instance of: an admitted class whose disputes cannot be carried is a class that executes
    /// and certifies and can never be policed. `derive_court_cost_v1` is the same function the
    /// gate itself calls, so a non-zero here means the gate and the ceiling disagree.
    pub closes_over_ceiling: u64,
    /// The largest close any accepted profile in this run would cost, in bytes — reported so a
    /// zero in `closes_over_ceiling` is a measurement rather than an absence of measurement.
    pub max_close_bytes_seen: u64,
    /// **The corpus digest: every executed run's every committed row, folded in order.**
    ///
    /// Two runs in one process prove the interpreter is not reading uninitialised memory; they
    /// prove nothing about two MACHINES, and cross-architecture determinism is the property this
    /// family's whole claim rests on. A tally cannot carry that — it is the same on any machine
    /// that panics identically — but a digest over the actual bits can: one seed, one corpus, one
    /// number, comparable anywhere. `the_fuzz_corpus_digest_is_the_same_on_every_machine` pins it,
    /// so CI's ubuntu/macOS/Windows runners each assert the same value and the clause is met by
    /// the suite rather than by a promise.
    pub corpus_digest: [u8; 32],
}

fn tiny_class() -> (Base0ArtifactV1, PalwShapeProfileV3) {
    let shape = Base0ShapeV1 {
        n_layers: 2,
        n_heads: 4,
        n_kv_heads: 2,
        d_head: 4,
        d_ff: 12,
        vocab: 64,
        max_position: 32,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: 1,
    };
    let artifact = Base0ArtifactV1::derive_deterministic(shape, 0xF022)
        .expect("a valid shape")
        .with_a16_params(derived_a16_store(&shape))
        .expect("sorted and unique");
    let geometry = PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 16,
        ffn_dim: 12,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 4,
        vocab_size: 64,
        n_ctx: 16,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    };
    (artifact, qwen25_a16_profile_v2(geometry).expect("the corrected profile builds"))
}

/// One random edit. The vocabulary of edits is the vocabulary of ways a stranger's registration
/// can differ from the family shape — structural (order, arity, count) and nominal (kernels,
/// operands, widths, dtypes) — plus a "no edit" arm so the unmutated profile stays in the corpus.
fn mutate(rng: &mut FuzzRng, profile: &mut PalwShapeProfileV3) {
    let foreign_kernels = ["a16/some-future-kernel/v9", "q99/attention-you-never-met/v1", ""];
    let foreign_names = ["blk.{layer}.someone_elses.a16", "totally.unbound", "blk.{layer}.attn_q.weight.evil"];
    let table = |rng: &mut FuzzRng, p: &mut PalwShapeProfileV3| -> *mut Vec<PalwStepNodeV1> {
        match rng.below(3) {
            0 => &mut p.pre_nodes,
            1 => &mut p.attn_nodes,
            _ => &mut p.post_nodes,
        }
    };
    for _ in 0..=rng.below(3) {
        // SAFETY-free: the pointer dance above is only to pick a table; re-borrow immediately.
        let t: &mut Vec<PalwStepNodeV1> = unsafe { &mut *table(rng, profile) };
        if t.is_empty() {
            continue;
        }
        let i = rng.below(t.len() as u64) as usize;
        match rng.below(8) {
            0 => {
                let j = rng.below(t.len() as u64) as usize;
                t.swap(i, j);
            }
            1 => {
                if t.len() > 1 {
                    t.remove(i);
                }
            }
            2 => {
                let clone = t[i].clone();
                t.insert(i, clone);
            }
            3 => {
                let k = foreign_kernels[rng.below(foreign_kernels.len() as u64) as usize];
                t[i].kernel_semantics_id = kernel_semantics_id_v1(k);
            }
            4 => {
                t[i].weight_name = foreign_names[rng.below(foreign_names.len() as u64) as usize].to_string();
            }
            5 => {
                t[i].out_len = match rng.below(3) {
                    0 => PalwStepOutLenV1::Fixed { elements: rng.below(4096) as u32 },
                    1 => PalwStepOutLenV1::KvScaled { multiplier: rng.below(16) as u32 },
                    _ => PalwStepOutLenV1::Fixed { elements: 0 },
                };
            }
            6 => {
                t[i].input_refs = match rng.below(4) {
                    0 => vec![],
                    1 => vec![rng.below(64) as u16],
                    2 => vec![0xFFFF],
                    _ => vec![rng.below(8) as u16, rng.below(0x1_0000) as u16],
                };
            }
            _ => {
                if !t[i].weight_dtypes.is_empty() {
                    let d = rng.below(t[i].weight_dtypes.len() as u64) as usize;
                    t[i].weight_dtypes[d] = rng.below(40) as u8;
                }
            }
        }
    }
}

/// The gate the chain runs, over the probe object the SDK's preflight builds — placeholder
/// economics, which the gate does not read.
fn gate_accepts(
    bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    root: kaspa_hashes::Hash64,
) -> bool {
    let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, 4, 2);
    // **Probed WEIGHTLESS** (ADR-0069 Decision 5). What this fuzzer asks is whether a mutated
    // profile is a well-formed, adjudicable SHAPE — ids, coverage, ladder depth, court cost. Who
    // may hold cadence is a different question with a different input (the build's certified family
    // set), and asking it here would make every mutation fail for a reason that has nothing to do
    // with the mutation.
    let Ok(probe) = kaspa_consensus_core::palw_class_admission_v2::palw_post_genesis_registration_v1(
        profile.clone(),
        canonical.clone(),
        root,
        0,
        1,
        1,
        0,
        kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
            kaspa_consensus_core::tx::TransactionId::default(),
            0,
        )),
        Vec::new(),
    ) else {
        return false;
    };
    kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v2(bundle, profile, &canonical, &probe, &[]).is_ok()
}

/// Drive `iterations` mutated profiles through gate → plan → double execution. See the module
/// doc for what each tally column means; the two FINDING columns must be zero for the ADR's
/// fence to arm.
pub fn fuzz_a16_profiles_v1(seed: u64, iterations: u64) -> FuzzTallyV1 {
    let (artifact, base) = tiny_class();
    let engine = A16Engine::new(&artifact).expect("the store resolves");
    let bundle = kaspa_consensus_core::palw_fp_devnet_v3::palw_fp_devnet_bundle_v3(
        base.shape_profile_id(),
        kaspa_hashes::Hash64::from_u64_word(0xCA7),
        kaspa_hashes::Hash64::from_u64_word(0xC0757),
        4_096,
        kaspa_hashes::Hash64::from_u64_word(0xA7),
        kaspa_consensus_core::palw_fp_devnet_v3::palw_devnet_bond_registry_v1(
            kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1(),
        ),
    )
    .expect("the devnet bundle assembles");
    let root = artifact.artifact_digest();

    let mut rng = FuzzRng::new(seed);
    let mut tally = FuzzTallyV1 { iterations, ..Default::default() };
    // Folded over every executed run, in corpus order — see `corpus_digest`.
    let mut corpus = blake2b_simd::Params::new().hash_length(32).key(b"misaka-palw/fuzz-corpus/v1").to_state();
    for _ in 0..iterations {
        let mut profile = base.clone();
        mutate(&mut rng, &mut profile);
        if !gate_accepts(&bundle, &profile, root) {
            tally.gate_refused += 1;
            continue;
        }
        let plan = match engine.plan_from_profile(&profile) {
            Ok(plan) => plan,
            Err(_) => {
                tally.plan_refused_after_gate += 1;
                continue;
            }
        };
        // **The court's side of the same profile** (Decision 5's second clause). A class the gate
        // admits must also be one whose worst dispute fits the carrier the network has — the
        // ceiling is a fact about adjudicability, not about execution, so a fuzzer that only
        // executed would report a clean run over classes nobody could ever police.
        match kaspa_consensus_core::palw_class_admission_v2::derive_court_cost_v1(&profile) {
            Ok(cost) => {
                tally.court_costed += 1;
                tally.max_close_bytes_seen = tally.max_close_bytes_seen.max(cost.max_close_bytes);
                if cost.max_close_bytes > kaspa_consensus_core::palw_class_admission_v2::PALW_RC_COURT_MAX_CLOSE_BYTES {
                    tally.closes_over_ceiling += 1;
                }
            }
            // A cost the derivation refuses is a profile the gate would have refused too; it is
            // not a finding, and it is not silently counted as costed either.
            Err(_) => {}
        }

        // A caller's prompt, twice; the bits must not care which run it was.
        let prompt: Vec<usize> = (0..(1 + rng.below(4) as usize)).map(|_| rng.below(64) as usize).collect();
        let run = |()| -> Result<Vec<(Vec<i32>, crate::engine_a16::A16TraceV1)>, ()> {
            let mut cache = crate::engine_a16::A16Cache::new(artifact.shape.n_layers);
            let mut out = Vec::new();
            for (position, token) in prompt.iter().enumerate() {
                match engine.forward_token_planned(&plan, &mut cache, *token, position) {
                    Ok(pair) => out.push(pair),
                    Err(_) => return Err(()),
                }
            }
            Ok(out)
        };
        let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(())));
        let second = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(())));
        match (first, second) {
            (Ok(a), Ok(b)) => {
                if a != b {
                    tally.nondeterminism += 1;
                } else {
                    tally.executed += 1;
                    // The BITS, not the count: logits and every committed row of every table, in
                    // the order the walk produced them. A machine that computes anything
                    // differently lands on a different digest, whatever its tally says.
                    if let Ok(rows) = &a {
                        for (logits, trace) in rows {
                            for v in logits {
                                corpus.update(&v.to_le_bytes());
                            }
                            let mut fold = |row: &Vec<i32>| {
                                corpus.update(&(row.len() as u64).to_le_bytes());
                                for v in row {
                                    corpus.update(&v.to_le_bytes());
                                }
                            };
                            trace.pre.iter().for_each(&mut fold);
                            trace.attn.iter().flatten().for_each(&mut fold);
                            trace.post.iter().for_each(&mut fold);
                        }
                    }
                }
            }
            _ => tally.panics += 1,
        }
    }
    tally.corpus_digest.copy_from_slice(corpus.finalize().as_bytes());
    tally
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CI-sized slice of the saturation run: enough iterations to exercise every mutation
    /// arm, small enough to live in the suite. Zero findings is the assertion; the counters are
    /// printed so a drift in the gate/plan gap is visible in the log.
    #[test]
    fn a_bounded_fuzz_run_finds_no_panic_and_no_nondeterminism() {
        let tally = fuzz_a16_profiles_v1(0x0067_2026_08_31, 400);
        println!("fuzz tally: {tally:?}");
        assert_eq!(tally.panics, 0, "a panic inside the interpreter is the fence staying down");
        assert_eq!(tally.nondeterminism, 0, "two runs of one plan must be one bitstream");
        assert_eq!(
            tally.closes_over_ceiling, 0,
            "the gate admitted a class whose worst dispute does not fit the carrier — it would execute, certify, and \
             never be policeable"
        );
        assert!(tally.executed > 0, "the corpus must actually reach execution, or this test gates nothing");
        assert!(tally.court_costed > 0, "and the court cost must actually be derived, or the ceiling column proves nothing");
        assert!(tally.max_close_bytes_seen > 0, "a zero here would mean the ceiling was never compared against anything");
    }

    /// **ADR-0067 Decision 5's cross-architecture clause, met by the suite rather than promised.**
    ///
    /// The two-runs-in-one-process check cannot see a machine that computes differently — it
    /// would agree with itself and report zero. This pins the DIGEST of the corpus's actual bits
    /// (every executed run's logits and every committed row of every table, folded in order), so
    /// the property "one seed, one corpus, one number, on any machine" is asserted wherever the
    /// suite runs. CI runs it on ubuntu, macOS and Windows; this repo's own arm64 development
    /// machines run it too, and the manual arm64/x86-64 comparison that preceded this test agreed
    /// on the one real class it could carry. A failure here is not a flake: it is two machines
    /// disagreeing about integer arithmetic, which is the one thing this family may not do.
    ///
    /// If a deliberate change to the mutator, the schedule or the interpreter moves this value,
    /// re-pin it in ONE commit that says which change moved it and why that change was intended —
    /// never alongside other work, because a silently re-pinned determinism digest is the same as
    /// not having one.
    #[test]
    fn the_fuzz_corpus_digest_is_the_same_on_every_machine() {
        let tally = fuzz_a16_profiles_v1(0x0067_2026_08_31, 400);
        assert_eq!(
            faster_hex::hex_string(&tally.corpus_digest),
            CORPUS_DIGEST_400,
            "this machine computed a different corpus than the pinned one — see the doc above before touching the pin"
        );
    }

    /// Seed `0x0067_2026_08_31`, 400 iterations. See the test above.
    const CORPUS_DIGEST_400: &str = "90939894923247d3e1eb18478b0495744e9ff0416bb9a20d16d01f0c411ff5eb";
}
