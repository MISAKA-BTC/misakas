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
//! 4. plan-compiled → execute a caller's prompt TWICE, under `catch_unwind`. A panic is a
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
    fn below(&mut self, n: u64) -> u64 {
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
    /// The findings. A non-zero here is the fence staying down.
    pub panics: u64,
    pub nondeterminism: u64,
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
fn gate_accepts(bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2, profile: &PalwShapeProfileV3, root: kaspa_hashes::Hash64) -> bool {
    let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, 4, 2);
    let Ok(probe) = kaspa_consensus_core::palw_class_admission_v2::palw_post_genesis_registration_v1(
        profile.clone(),
        canonical.clone(),
        root,
        1,
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
    kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v2(bundle, profile, &canonical, &probe).is_ok()
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
                }
            }
            _ => tally.panics += 1,
        }
    }
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
        assert!(tally.executed > 0, "the corpus must actually reach execution, or this test gates nothing");
    }
}
