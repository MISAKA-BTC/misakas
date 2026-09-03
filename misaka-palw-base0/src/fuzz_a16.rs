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

/// The SAME artifact and the same geometry as [`tiny_class`], projected as ADR-0082's graph v5:
/// one fused attention node per layer instead of four. The artifact is unchanged, which is what
/// makes this a second GRAPH over one model rather than a second fixture.
fn tiny_class_v5() -> (Base0ArtifactV1, PalwShapeProfileV3) {
    let (artifact, v2) = tiny_class();
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
    let v5 = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v5(geometry).expect("the v5 profile builds");
    assert_ne!(v2.shape_profile_id(), v5.shape_profile_id(), "a different graph is a different class");
    (artifact, v5)
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
    // **A fused profile is judged under the court that can try it** (ADR-0082 Decision 6): the
    // admission gate refuses a graph-v5 row by name unless the k-ary court is in force, so the
    // corpus is gated the way an ARMED ruleset gates it — the RC's derived arity, the Merkle prompt
    // ids the rows arm with, and the RC's court window. A profile with no fused site is gated
    // exactly as before (`None`), so the v2 corpus reads unchanged.
    // The court is the bundle's own (`fuzz_*_profiles_from_v1` arms the dissection arity on a fused
    // base), the prompt ids are the Merkle form graph-v5 rows arm with, and the window is the RC's.
    // A fused row is also PRICED for that court: the ladder rules carry the cost shape the gate
    // compares against the arity, and a base too small for the long-form rules is priced by the
    // anchored shape with the same dissection — `PricedForADifferentCourt` is the gate's answer
    // when a registrant prices a row for a court the ruleset does not play, not this harness's.
    let fused = kaspa_consensus_core::palw_class_admission_v2::palw_profile_has_fused_attention_v1(profile);
    let arity = bundle.court.dissection_arity();
    let court = fused.then_some(kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
        dissection_arity: arity,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
        window_court_daa: kaspa_consensus_core::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1.window_court,
    });
    let ladder = fused.then(|| {
        kaspa_consensus_core::palw_context_ladder::palw_class_ladder_rules_for_court_v1(profile, court).unwrap_or_else(|| {
            kaspa_consensus_core::palw_class_admission_v2::PalwClassLadderRulesV1 {
                ladder: bundle.court.max_step_leaf_count(),
                cost_shape: kaspa_consensus_core::palw_class_admission_v2::PalwCourtCostShapeV1::genesis_anchored_v1(profile)
                    .with_dissection_v1(arity),
                canonical_footprint_floor: 0,
            }
        })
    });
    kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v5(bundle, profile, &canonical, &probe, &[], &[], ladder, court)
        .is_ok()
}

/// Drive `iterations` mutated profiles through gate → plan → double execution. See the module
/// doc for what each tally column means; the two FINDING columns must be zero for the ADR's
/// fence to arm.
pub fn fuzz_a16_profiles_v1(seed: u64, iterations: u64) -> FuzzTallyV1 {
    let (artifact, base) = tiny_class();
    fuzz_a16_profiles_from_v1(seed, iterations, &artifact, &base)
}

/// [`fuzz_a16_profiles_v1`] over a caller's BASE profile — the same schedule, the same mutations,
/// the same findings, driven from a different graph.
///
/// A parameter rather than a second harness because ADR-0082's graph v5 is a new graph over the
/// same model, and a fuzz gate that only ever saw the four-node attention site would say nothing
/// about the one-node one. `fuzz_a16_profiles_v1` is the shipped base and its corpus digest is
/// pinned; this is what lets a second base be driven without moving that pin.
pub fn fuzz_a16_profiles_from_v1(seed: u64, iterations: u64, artifact: &Base0ArtifactV1, base: &PalwShapeProfileV3) -> FuzzTallyV1 {
    let engine = A16Engine::new(artifact).expect("the store resolves");
    let mut bundle = kaspa_consensus_core::palw_fp_devnet_v3::palw_fp_devnet_bundle_v3(
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
    // **A fused base is fuzzed under the court that can try it** (ADR-0082 Decision 6): the devnet
    // bundle plays the binary court, and the gate refuses every graph-v5 row by name under it, so
    // the corpus would never reach execution. The RC derives 4 (`palw_court_arity_v1`, the
    // smallest legal arity inside the window); a v2 base leaves the bundle byte for byte.
    if kaspa_consensus_core::palw_class_admission_v2::palw_profile_has_fused_attention_v1(base) {
        bundle.court = bundle.court.with_dissection_arity(4).expect("4 is a legal dissection arity");
    }
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
        // A cost the derivation refuses is a profile the gate would have refused too; it is not a
        // finding, and it is not silently counted as costed either -- hence no `else`.
        if let Ok(cost) = kaspa_consensus_core::palw_class_admission_v2::derive_court_cost_v1(&profile) {
            tally.court_costed += 1;
            tally.max_close_bytes_seen = tally.max_close_bytes_seen.max(cost.max_close_bytes);
            if cost.max_close_bytes > kaspa_consensus_core::palw_class_admission_v2::PALW_RC_COURT_MAX_CLOSE_BYTES {
                tally.closes_over_ceiling += 1;
            }
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

/// **ADR-0067 security amendment SA-1's second corpus: profiles built to EXHAUST MEMORY and to
/// RECURSE.**
///
/// The saturation run above mutates a real profile the way a stranger's registration might
/// differ from it — order, arity, kernels, names, widths — and that is the right corpus for
/// "does the interpreter compute the wrong thing". It is the wrong corpus for "does the
/// interpreter survive a program written to kill it", because its width edits draw from a small
/// range and its structural edits keep the tables small. A chain-registered profile is a
/// stranger's PROGRAM, and the two things a stranger's program does to a host are allocate and
/// recurse.
///
/// So this is a separate generator with a separate tally, deliberately NOT folded into
/// [`fuzz_a16_profiles_v1`]: that run's corpus digest is a cross-architecture pin, and a pin that
/// moves whenever someone adds a mutation is a pin nobody trusts. Two corpora, two questions.
///
/// The edits, and what each is trying to do:
///
/// * **exhaust** — maximal `Fixed` row widths, maximal kv-scaled multipliers, tables grown to the
///   per-table cap, `tile_len` pushed to its maximum (which is what lets a huge row still fit the
///   leaf bound and reach the planner at all), and geometry fields raised toward their own caps;
/// * **recurse** — self references, forward references, chains that thread every node of a table
///   in sequence, and maximal fan-in. The graph rules refuse most of these at the GATE, which is
///   the correct outcome and is exactly why they belong in a corpus: a rule that refuses them is
///   a rule worth running against them.
///
/// The plan step runs under the caller's `ceiling_bytes` so the memory ceiling can be shown to
/// BIND rather than merely to exist, and both the plan and the walk run under `catch_unwind`,
/// because "the planner panicked" is a finding the previous corpus could not have reported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AdversarialTallyV1 {
    pub iterations: u64,
    pub gate_refused: u64,
    /// Refused by the planner for an ordinary reason (an unserved node, a width, an arity).
    pub plan_refused: u64,
    /// **Refused by the memory ceiling** — the column that makes the ceiling a measurement.
    pub refused_by_memory_ceiling: u64,
    pub executed: u64,
    /// The findings. Any non-zero is the fence staying down.
    pub panics: u64,
    pub nondeterminism: u64,
    /// The largest one-token trace any GATE-ACCEPTED profile in this run would have committed —
    /// reported so a non-zero `refused_by_memory_ceiling` is a number and not a hope.
    pub max_trace_bytes_seen: u64,
}

/// A tile width for an inflated row. **The tile is what decides whether the gate ever sees the
/// row**, and it pulls two ways: the leaf bound counts TILES, so a wide row needs a wide tile to
/// stay under it, while the court-cost ceiling bounds a close — which carries a tile — so a tile
/// that is too wide is refused for the opposite reason. An attacker looking for the largest
/// declaration the gate will admit searches exactly this interval, so the corpus does too. The
/// maximum is kept in the set because a shape refused for being over the court ceiling is also
/// worth generating; it is just refused by a different rule.
fn wide_tile(rng: &mut FuzzRng) -> u32 {
    const CANDIDATES: [u32; 5] = [1_024, 4_096, 8_192, 16_384, kaspa_consensus_core::palw_step::PALW_STEP_MAX_TILE_LEN];
    CANDIDATES[rng.below(CANDIDATES.len() as u64) as usize]
}

/// One adversarial edit. Returns nothing: the profile is mutated in place, and a mutation the
/// gate then refuses is a result, not a failure.
fn mutate_adversarially(rng: &mut FuzzRng, profile: &mut PalwShapeProfileV3) {
    let pick = |rng: &mut FuzzRng, p: &mut PalwShapeProfileV3| -> *mut Vec<PalwStepNodeV1> {
        match rng.below(3) {
            0 => &mut p.pre_nodes,
            1 => &mut p.attn_nodes,
            _ => &mut p.post_nodes,
        }
    };
    match rng.below(10) {
        // ---- exhaust -----------------------------------------------------------------------
        0 => {
            // A maximal fixed row. `tile_len` goes with it: the leaf bound counts TILES, so a
            // wide row only reaches the planner if its tiles are wide too — which is the shape an
            // attacker would find, so it is the shape the corpus must carry.
            let t: &mut Vec<PalwStepNodeV1> = unsafe { &mut *pick(rng, profile) };
            if t.is_empty() {
                return;
            }
            let i = rng.below(t.len() as u64) as usize;
            t[i].out_len = PalwStepOutLenV1::Fixed { elements: u32::MAX - rng.below(4) as u32 };
            t[i].tile_len = wide_tile(rng);
        }
        1 => {
            let t: &mut Vec<PalwStepNodeV1> = unsafe { &mut *pick(rng, profile) };
            if t.is_empty() {
                return;
            }
            let i = rng.below(t.len() as u64) as usize;
            t[i].out_len = PalwStepOutLenV1::KvScaled { multiplier: u32::MAX - rng.below(4) as u32 };
            t[i].tile_len = wide_tile(rng);
        }
        2 => {
            // Grow a table to its cap by duplication: N times the rows, N times the allocation,
            // with every node individually legal.
            let t: &mut Vec<PalwStepNodeV1> = unsafe { &mut *pick(rng, profile) };
            let cap = kaspa_consensus_core::palw_step::PALW_STEP_MAX_NODES_PER_TABLE;
            while !t.is_empty() && t.len() < cap {
                let clone = t[t.len() - 1].clone();
                t.push(clone);
            }
        }
        3 => {
            // The layer table is walked once per layer, so the layer count multiplies whatever
            // the table costs. Raised toward the profile cap rather than to it, because the
            // enumeration bound couples it to `n_ctx`.
            profile.layer_count = 1 + rng.below(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LAYERS as u64) as u16;
        }
        4 => {
            profile.n_ctx = 1 + rng.below(1 << 20) as u32;
        }
        5 => {
            // Every table at once — the honest worst case, since a registrant writes all three.
            for t in [&mut profile.pre_nodes, &mut profile.attn_nodes, &mut profile.post_nodes] {
                for n in t.iter_mut() {
                    n.out_len = PalwStepOutLenV1::Fixed { elements: 1 << (16 + rng.below(14) as u32).min(31) };
                    n.tile_len = wide_tile(rng);
                }
            }
        }
        // ---- recurse -----------------------------------------------------------------------
        6 => {
            // A node that reads itself. The graph rule refuses it; the corpus proves the rule
            // runs before anything walks the reference.
            let t: &mut Vec<PalwStepNodeV1> = unsafe { &mut *pick(rng, profile) };
            if t.is_empty() {
                return;
            }
            let i = rng.below(t.len() as u64) as usize;
            t[i].input_refs = vec![i as u16];
        }
        7 => {
            // A forward reference: a committed input defined by the output that explains it.
            let t: &mut Vec<PalwStepNodeV1> = unsafe { &mut *pick(rng, profile) };
            if t.len() < 2 {
                return;
            }
            let i = rng.below((t.len() - 1) as u64) as usize;
            t[i].input_refs = vec![(i + 1) as u16];
        }
        8 => {
            // A chain threading every node of a table in sequence — legal, and the deepest
            // dependency this space can express. What it hunts is a resolver that recurses per
            // edge instead of walking the table once.
            let t: &mut Vec<PalwStepNodeV1> = unsafe { &mut *pick(rng, profile) };
            for (i, node) in t.iter_mut().enumerate().skip(1) {
                node.input_refs = vec![(i - 1) as u16];
            }
        }
        _ => {
            // Maximal fan-in, all onto one earlier row.
            let t: &mut Vec<PalwStepNodeV1> = unsafe { &mut *pick(rng, profile) };
            if t.len() < 2 {
                return;
            }
            let i = 1 + rng.below((t.len() - 1) as u64) as usize;
            t[i].input_refs = vec![0u16; 8];
        }
    }
}

/// Drive `iterations` adversarially-shaped profiles through gate → plan (under `ceiling_bytes`) →
/// double execution. See [`AdversarialTallyV1`] for what each column means.
pub fn fuzz_a16_adversarial_profiles_v1(seed: u64, iterations: u64, ceiling_bytes: u64) -> AdversarialTallyV1 {
    let (artifact, base) = tiny_class();
    let engine = A16Engine::new(&artifact).expect("the store resolves");
    let mut bundle = kaspa_consensus_core::palw_fp_devnet_v3::palw_fp_devnet_bundle_v3(
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
    // **A fused base is fuzzed under the court that can try it** (ADR-0082 Decision 6): the devnet
    // bundle plays the binary court, and the gate refuses every graph-v5 row by name under it, so
    // the corpus would never reach execution. The RC derives 4 (`palw_court_arity_v1`, the
    // smallest legal arity inside the window); a v2 base leaves the bundle byte for byte.
    if kaspa_consensus_core::palw_class_admission_v2::palw_profile_has_fused_attention_v1(&base) {
        bundle.court = bundle.court.with_dissection_arity(4).expect("4 is a legal dissection arity");
    }
    let root = artifact.artifact_digest();

    let mut rng = FuzzRng::new(seed);
    let mut tally = AdversarialTallyV1 { iterations, ..Default::default() };
    for _ in 0..iterations {
        let mut profile = base.clone();
        // One to three edits, so an exhausting shape can also be a recursing one.
        for _ in 0..=rng.below(3) {
            mutate_adversarially(&mut rng, &mut profile);
        }
        if !gate_accepts(&bundle, &profile, root) {
            tally.gate_refused += 1;
            continue;
        }
        tally.max_trace_bytes_seen = tally
            .max_trace_bytes_seen
            .max(crate::engine_a16::interpreted_trace_bytes_v1(&profile, artifact.shape.max_position as u64));
        // **The planner runs under `catch_unwind` too.** The other corpus wraps only the walk,
        // which cannot see a planner that panics on a shape it never had to consider.
        let planned =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.plan_from_profile_within(&profile, ceiling_bytes)));
        let plan = match planned {
            Err(_) => {
                tally.panics += 1;
                continue;
            }
            Ok(Err(crate::engine_a16::A16PlanErrorV1::OverMemoryCeiling { .. })) => {
                tally.refused_by_memory_ceiling += 1;
                continue;
            }
            Ok(Err(_)) => {
                tally.plan_refused += 1;
                continue;
            }
            Ok(Ok(plan)) => plan,
        };
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
    use crate::engine_a16::PALW_INTERPRETER_TRACE_BYTES_CEILING_V1;

    /// The CI-sized slice of the saturation run: enough iterations to exercise every mutation
    /// arm, small enough to live in the suite. Zero findings is the assertion; the counters are
    /// printed so a drift in the gate/plan gap is visible in the log.
    #[test]
    fn a_bounded_fuzz_run_finds_no_panic_and_no_nondeterminism() {
        let tally = fuzz_a16_profiles_v1(0x0067_2026_0831, 400);
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

    /// **The same gate over ADR-0082's graph v5** — the fused attention site, driven through the
    /// same mutate → admit → plan → execute-twice schedule.
    ///
    /// The point is not a second tally. It is that the fence Decision 5 arms is a fence about the
    /// PROFILE SPACE, and graph v5 changes the shape of that space at exactly the site the
    /// mutations reach: a fused node has three inputs where every other node has one or two, its
    /// out width is fixed where the site's used to be job-scaled, and its operand name is the ONE
    /// the other three derive from. A mutation that rewrites that name, that width or those refs
    /// must be refused or planned — never panicked on, and never nondeterministic.
    ///
    /// No corpus digest is pinned for this base: the digest's whole discipline is that it moves
    /// only in a commit that says why, and a v5 row is not registered anywhere yet.
    #[test]
    fn a_bounded_fuzz_run_over_the_v5_graph_finds_no_panic_and_no_nondeterminism() {
        let (artifact, base) = tiny_class_v5();
        let tally = fuzz_a16_profiles_from_v1(0x0082_2026_09_03, 400, &artifact, &base);
        println!("v5 fuzz tally: {tally:?}");
        assert_eq!(tally.panics, 0, "a panic inside the interpreter is the fence staying down");
        assert_eq!(tally.nondeterminism, 0, "two runs of one plan must be one bitstream");
        assert!(tally.executed > 0, "the v5 corpus must actually reach execution, or this test gates nothing");
        assert!(tally.court_costed > 0, "and the court cost must actually derive for a v5 row");
        assert!(tally.max_close_bytes_seen > 0, "a zero here would mean the ceiling was never compared against anything");
        // `closes_over_ceiling` is REPORTED and not asserted zero: the whole-row route of ADR-0082
        // Decision 1 opens the K and V history, and pricing it down to the dissection's bounded
        // close is Decision 2 / stream F's derivation, not this stream's. Asserting zero here
        // would be asserting a bound this ADR has not yet derived.
        println!("v5 closes over the shipped ceiling: {}", tally.closes_over_ceiling);
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
        let tally = fuzz_a16_profiles_v1(0x0067_2026_0831, 400);
        assert_eq!(
            faster_hex::hex_string(&tally.corpus_digest),
            CORPUS_DIGEST_400,
            "this machine computed a different corpus than the pinned one — see the doc above before touching the pin"
        );
    }

    /// Seed `0x0067_2026_08_31`, 400 iterations. See the test above.
    const CORPUS_DIGEST_400: &str = "90939894923247d3e1eb18478b0495744e9ff0416bb9a20d16d01f0c411ff5eb";

    /// **ADR-0067 SA-1, first half: the corpus contains profiles built to exhaust memory and to
    /// recurse, and driving them finds no panic.**
    ///
    /// The columns are printed because the interesting failure of an adversarial corpus is not a
    /// finding — it is a corpus that stopped reaching anything. `gate_refused` climbing to the
    /// iteration count would mean the generator only ever writes shapes the gate throws out, which
    /// asserts about the gate and nothing about the interpreter.
    #[test]
    fn the_adversarial_corpus_reaches_the_interpreter_without_a_panic() {
        let tally = fuzz_a16_adversarial_profiles_v1(0x0067_5A01, 400, PALW_INTERPRETER_TRACE_BYTES_CEILING_V1);
        println!("adversarial tally: {tally:?}");
        assert_eq!(tally.panics, 0, "a panic on a hostile profile is the fence staying down — planner or walk");
        assert_eq!(tally.nondeterminism, 0, "two runs of one plan must be one bitstream, hostile shape or not");
        assert!(
            tally.gate_refused < tally.iterations,
            "every generated profile was refused at the gate: this corpus would then assert nothing about the \
             interpreter, which is the failure mode an adversarial corpus has"
        );
    }

    /// **ADR-0067 SA-1, second half: the memory ceiling is PROVEN TO BIND.**
    ///
    /// The amendment's own wording is the reason this test exists in this shape: a run under the
    /// shipped ceiling that reports zero panics shows that nothing crashed, which is not evidence
    /// that anything would have been stopped. So the corpus is run against a ceiling the generated
    /// shapes actually cross, and the assertion is that the ceiling REFUSED them — by name, at plan
    /// time, before an allocation.
    ///
    /// **What the run measures, stated because it is smaller than it sounds and the honest number
    /// is the useful one.** At 400 iterations of seed `0x0067_5A01` the admission GATE refuses 372
    /// of the generated shapes, and the largest one-token trace any gate-ACCEPTED profile would
    /// have committed is 26,624 bytes — not gigabytes. The wide-row arms never reach the planner
    /// at all: the leaf bound multiplies a node's tiles by positions and layers, so a `u32::MAX`
    /// row blows `PALW_STEP_MAX_LEAVES` whatever tile width it picks. That is the gate doing
    /// exactly its job, and it means the ceiling is a SECOND line rather than the only one.
    ///
    /// Which is why the mechanism is also pinned directly, one byte under an honest profile's own
    /// cost, below: "the gate happens to refuse everything big" is a fact about today's gate, and
    /// the ceiling has to bind on its own terms or it is not a ceiling.
    #[test]
    fn the_interpreter_memory_ceiling_actually_stops_a_hostile_profile() {
        // One kibibyte of committed trace: far below anything the generator's inflated rows cost,
        // and the tiny fixture's own honest profile is comfortably under it, so a refusal here is
        // the ceiling and not the fixture.
        let bound = fuzz_a16_adversarial_profiles_v1(0x0067_5A01, 400, 1 << 10);
        println!("ceiling-bound tally: {bound:?}");
        assert_eq!(bound.panics, 0);
        assert!(
            bound.refused_by_memory_ceiling > 0,
            "the ceiling refused nothing, so nothing here shows it binds — either the generator stopped producing \
             large declarations or the check stopped running"
        );
        assert!(bound.max_trace_bytes_seen > (1 << 10), "…and the corpus really did ask for more than the ceiling allows");

        // The refusal is a named plan error carrying both numbers, not a generic one: an operator
        // reading it must be able to tell "this class is too big for the rule" from "this build
        // cannot serve this graph", because only one of those is fixed by raising a bound.
        let (artifact, base) = tiny_class();
        let engine = A16Engine::new(&artifact).expect("the store resolves");
        let honest = crate::engine_a16::interpreted_trace_bytes_v1(&base, artifact.shape.max_position as u64);
        assert!(honest > 0, "the honest profile must cost something or the ceiling compares against nothing");
        match engine.plan_from_profile_within(&base, honest - 1) {
            Err(crate::engine_a16::A16PlanErrorV1::OverMemoryCeiling { bytes, ceiling }) => {
                assert_eq!((bytes, ceiling), (honest, honest - 1), "the refusal reports what it measured and what it allowed");
            }
            other => panic!("a ceiling one byte under the cost must refuse, got {other:?}"),
        }

        // …and the SHIPPED ceiling does not bind on an honest class, which is the other half of
        // calibration: a ceiling that refuses real work is an outage, not a defence.
        assert!(honest < PALW_INTERPRETER_TRACE_BYTES_CEILING_V1);
        assert!(engine.plan_from_profile(&base).is_ok(), "the corrected profile must plan under the shipped ceiling");
    }

    /// **Round-3 defect I-3: the consensus shape caps admit more than this build will
    /// materialise, and the operator's refusal has to say which of those two happened.**
    ///
    /// The band the derivation test pins is measured over the three classes THIS BUILD compiles,
    /// and classes are permissionless (ADR-0054) — so the population the ceiling actually meets is
    /// not the population it was calibrated against. That gap is not a suspicion: this constructs a
    /// declaration the consensus shape caps accept — `validate_shape` bounds a node's tile and
    /// requires a non-zero width, and bounds the width nowhere — whose worst job is far inside the
    /// leaf cap and whose one-token trace is over the ceiling.
    ///
    /// So the ceiling is this NODE's capacity, not a bound the gate implies. Deriving it from the
    /// caps would put it at `PALW_STEP_MAX_LEAVES × PALW_STEP_MAX_TILE_LEN × 4` — a terabyte, which
    /// is the "number nothing chose" problem again. What is fixed instead is the thing an operator
    /// can act on: the refusal must not read as "this build cannot serve the registered graph",
    /// because the graph is servable and the chain admitted it; a node with a larger ceiling runs
    /// it, and the divergence is node-local servability, never block validity.
    ///
    /// This also binds the VALUE from the other side: restore the 1 GiB ceiling and the witness no
    /// longer crosses it, so whoever moves the constant has to come here and say what the new claim
    /// is.
    #[test]
    fn the_consensus_shape_caps_admit_more_than_this_build_will_materialise() {
        use kaspa_consensus_core::palw_step::{PALW_STEP_MAX_LEAVES, PALW_STEP_MAX_TILE_LEN, worst_case_step_leaf_count_v1};
        let (artifact, base) = tiny_class();
        let max_position = artifact.shape.max_position as u64;

        // One extra declared row, of a width no consensus cap bounds, tiled at the widest tile the
        // court admits — so it costs 306 leaves per position and 80 MB of committed trace.
        let mut witness = base.clone();
        let mut wide = witness.pre_nodes.last().cloned().expect("the pre table is non-empty");
        wide.out_len = PalwStepOutLenV1::Fixed { elements: 20_000_000 };
        wide.tile_len = PALW_STEP_MAX_TILE_LEN;
        wide.input_refs = vec![(witness.pre_nodes.len() - 1) as u16];
        wide.weight_name = String::new();
        wide.weight_dtypes = Vec::new();
        witness.pre_nodes.push(wide);

        witness.validate_shape().expect("the consensus shape caps admit this declaration");
        let leaves = worst_case_step_leaf_count_v1(&witness).expect("…and its worst job is inside the leaf cap");
        let trace = crate::engine_a16::interpreted_trace_bytes_v1(&witness, max_position);
        println!("witness: {leaves} worst-case leaves (cap {PALW_STEP_MAX_LEAVES}), {trace} bytes of committed trace");
        assert!(
            leaves * 2 < PALW_STEP_MAX_LEAVES,
            "the witness sits near the leaf cap ({leaves}), so the gate — not the ceiling — is what stops it and this \
             test would be proving the opposite of what it says"
        );
        assert!(
            trace > PALW_INTERPRETER_TRACE_BYTES_CEILING_V1,
            "a shape the consensus caps admit costs {trace} bytes and this build's ceiling is \
             {PALW_INTERPRETER_TRACE_BYTES_CEILING_V1}: if that is no longer true, the ceiling now covers everything \
             the caps allow — say so here, and in the constant's doc, rather than leaving both claims standing"
        );

        // The plan refuses it by name, before an allocation…
        {
            let engine = A16Engine::new(&artifact).expect("the store resolves");
            match engine.plan_from_profile(&witness) {
                Err(crate::engine_a16::A16PlanErrorV1::OverMemoryCeiling { bytes, ceiling }) => {
                    assert_eq!((bytes, ceiling), (trace, PALW_INTERPRETER_TRACE_BYTES_CEILING_V1));
                }
                other => panic!("the ceiling must refuse the witness, got {other:?}"),
            }
        }

        // …and the sentence a node's operator reads says whose limit it was.
        let refusal = match crate::qwen25_a16_backend::Qwen25A16Backend::from_registered_profile(
            std::sync::Arc::new(artifact),
            b"misaka-palw-rc".to_vec(),
            witness,
            (1, 1),
        ) {
            Err(why) => why,
            Ok(_) => panic!("this build must not materialise the witness"),
        };
        println!("operator sees: {refusal}");
        assert!(refusal.contains("capacity"), "a capacity refusal must say so: {refusal}");
        assert!(
            !refusal.contains("cannot serve the registered graph"),
            "the chain admitted this class and another build serves it — telling the operator this build cannot serve \
             the graph sends them looking for software that is not missing: {refusal}"
        );
    }

    /// **ADR-0067 SA-1: the shipped ceiling is answerable to a measurement, in both directions.**
    ///
    /// The mechanism is proven by the test above — a ceiling one byte under a profile's own cost
    /// refuses it by name, before an allocation. That says nothing about the VALUE, and the first
    /// value shipped said nothing either: 1 GiB was 60x the largest class this build serves and
    /// 40,000x the largest gate-accepted profile the adversarial corpus produces, which is a number
    /// no evidence chose. A ceiling nobody can reach is not a ceiling.
    ///
    /// So the constant is pinned to what this build actually runs. Both bounds are load-bearing:
    ///
    /// * **Below** — the ceiling must clear the biggest class by a real margin, or a legitimate
    ///   registration becomes an outage. Measured at each class's registered context AND at a
    ///   4,096-position stress context far past anything the admission court accepts.
    /// * **Above** — the ceiling must NOT be an arbitrary distance above them, or it is back to
    ///   being a number with nothing behind it. This half is what fails if someone restores 1 GiB.
    ///
    /// Raising a class past the band is a legitimate reason to move the constant. Doing it without
    /// touching this test is not possible, which is the point.
    #[test]
    fn the_interpreter_ceiling_is_derived_from_what_this_build_actually_serves() {
        // Fully qualified rather than imported: a `use` list's internal order is a rustfmt style
        // question this repo has been burned by across tool versions, and this test has no need of
        // one.
        let b0g = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY;
        let a16g = kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B_A16;
        let g36 = kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(
            kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B,
        );

        let bytes = crate::engine_a16::interpreted_trace_bytes_v1;
        let base0 = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(b0g).expect("BASE-0 projects");
        let a16 = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2(a16g).expect("the A16 geometry projects");
        let q36 = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(g36).expect("the Qwen3.6 geometry projects");

        let registered = [
            ("BASE-0", bytes(&base0, b0g.n_ctx as u64)),
            ("QWEN25-A16", bytes(&a16, a16g.n_ctx as u64)),
            ("QWEN36", bytes(&q36, g36.n_ctx as u64)),
        ];
        for (name, cost) in registered {
            println!("{name}: one token's committed trace = {cost} bytes");
            assert!(cost > 0, "{name}: a class that costs nothing is a measurement error, not a cheap class");
        }
        let largest = registered.iter().map(|(_, c)| *c).max().expect("three classes");

        // Below: real headroom over the largest registered class, and over the same graph at a
        // context no court admits — so the margin is known to cover growth, not just today.
        let stress = bytes(&q36, 4_096);
        assert!(
            stress > largest && stress < PALW_INTERPRETER_TRACE_BYTES_CEILING_V1,
            "the largest class stretched to a 4,096-position context costs {stress} and must still fit under \
             {PALW_INTERPRETER_TRACE_BYTES_CEILING_V1}: a ceiling a legitimate class can cross is an outage"
        );
        assert!(
            PALW_INTERPRETER_TRACE_BYTES_CEILING_V1 >= largest.saturating_mul(2),
            "the ceiling ({PALW_INTERPRETER_TRACE_BYTES_CEILING_V1}) leaves under 2x over the largest class this \
             build serves ({largest}) — too tight to be a second line"
        );
        // Above: the constant stays tied to the measurement. 1 GiB fails here, which is the whole
        // reason this half exists.
        assert!(
            PALW_INTERPRETER_TRACE_BYTES_CEILING_V1 <= largest.saturating_mul(8),
            "the ceiling ({PALW_INTERPRETER_TRACE_BYTES_CEILING_V1}) is more than 8x the largest class this build \
             serves ({largest}), so it is a number nothing measured chose — derive it or say why the band moved"
        );
    }
}
