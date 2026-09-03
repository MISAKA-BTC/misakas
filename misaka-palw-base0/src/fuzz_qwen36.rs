//! **ADR-0067 Decision 5's fuzz gate for the mmap (Qwen3.6) container.**
//!
//! The same harness shape as `fuzz_a16`, because the clause is the same: arbitrary GATE-ACCEPTED
//! profiles through the interpreter, to saturation, with zero panics, zero non-determinism and
//! zero closes over the ceiling — deliberately deterministic (a seeded xorshift, no clocks), so a
//! failing seed is a repro command. What differs is the space: this family has FOUR node tables
//! (two of them layer arms selected by kind), cache-write roles on declared nodes, a recurrence
//! and a convolution with per-layer runtime state, routed-expert fusion nodes that read a
//! committed routing row, and per-head slicing everywhere — each its own way for a stranger's
//! gate-accepted declaration to reach arithmetic the compiled engine never runs.
//!
//! The design pass for this harness already paid for itself once: walking the mutation space on
//! paper found two panics the differentials could never see — a convolution or a recurrence
//! declared in the ATTENTION table lands on a cache whose window/states the artifact's layer
//! kinds never pre-filled (`remove(0)` on an empty window; indexing absent states). Both are now
//! declaration-faithful zero-state initializations in the walk, and this harness is what keeps
//! that class closed.
//!
//! One iteration: mutate the corrected profile (structural, nominal, and ROLE edits — this
//! family's cache writes are declared per node, so a mutated role is a mutated program) → the
//! chain's own admission gate → the plan → the court cost against the carrier ceiling → a
//! caller's prompt executed twice under `catch_unwind`. `fuzz_qwen36_profiles_v1` returns the
//! same tally type the dense harness uses, corpus digest included, so the cross-architecture
//! clause is met the same way: one seed, one corpus, one number, asserted wherever the suite
//! runs.

use crate::fuzz_a16::{FuzzRng, FuzzTallyV1};
use crate::qwen36::{Qwen36ArtifactV1, Qwen36Cache, Qwen36Engine, qwen36_dev_fixture};
use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_profile_v2};
use kaspa_consensus_core::palw_step::{
    PalwShapeProfileV3, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOutLenV1, kernel_semantics_id_v1,
};

/// The fixed base class every mutation starts from: the dev fixture's artifact and the corrected
/// (graph-v3) tables at its geometry — the same pairing the interpreter's differentials pin, and
/// the same epsilon rule the disposition took (the geometry declares what the artifact executes).
fn tiny_class() -> (Qwen36ArtifactV1, PalwShapeProfileV3) {
    let artifact = qwen36_dev_fixture(4, 8);
    let geometry = PalwQwen36GeometryV1 {
        layer_count: 4,
        full_attention_interval: 4,
        hidden_dim: 32,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 16,
        rope_dims: 4,
        rope_freq_base_bits: 0x4B18_9680,
        gdn_k_heads: 2,
        gdn_v_heads: 4,
        gdn_head_dim: 8,
        gdn_conv_kernel: 4,
        n_experts: 8,
        experts_per_token: 4,
        moe_dim: 16,
        shared_dim: 16,
        attn_output_gate: 1,
        vocab_size: 64,
        n_ctx: 8,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 512,
    };
    let profile = qwen36_profile_v2(geometry).expect("the corrected tables project at the fixture geometry");
    (artifact, profile)
}

/// The SAME artifact and geometry as [`tiny_class`], projected as ADR-0082's graph v5: one fused
/// attention node per attention layer instead of four. A second GRAPH over one model, which is
/// what makes it worth fuzzing — the mutations reach a node whose arity, out width and operand
/// naming rule differ from every other node in the table.
#[cfg(test)]
pub(crate) fn tiny_class_v5_for_tests() -> (Qwen36ArtifactV1, PalwShapeProfileV3) {
    tiny_class_v5()
}

#[cfg(test)]
fn tiny_class_v5() -> (Qwen36ArtifactV1, PalwShapeProfileV3) {
    let (artifact, v2) = tiny_class();
    let geometry = PalwQwen36GeometryV1 {
        layer_count: 4,
        full_attention_interval: 4,
        hidden_dim: 32,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 16,
        rope_dims: 4,
        rope_freq_base_bits: 0x4B18_9680,
        gdn_k_heads: 2,
        gdn_v_heads: 4,
        gdn_head_dim: 8,
        gdn_conv_kernel: 4,
        n_experts: 8,
        experts_per_token: 4,
        moe_dim: 16,
        shared_dim: 16,
        attn_output_gate: 1,
        vocab_size: 64,
        n_ctx: 8,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 512,
    };
    let v5 = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v5(geometry).expect("the v5 tables project");
    assert_ne!(v2.shape_profile_id(), v5.shape_profile_id(), "a different graph is a different class");
    (artifact, v5)
}

/// One random edit. The dense harness's vocabulary — structural (order, arity, count) and nominal
/// (kernels, operands, widths, dtypes) — plus two this family adds: ROLE edits (cache writes are
/// declared per node here, so a moved role is a moved program) and REAL operand names landing on
/// wrong nodes (a plannable-but-misbound declaration stresses the width checks harder than a
/// foreign name the planner refuses on sight).
fn mutate(rng: &mut FuzzRng, profile: &mut PalwShapeProfileV3) {
    let foreign_kernels = ["a16/some-future-kernel/v9", "q99/attention-you-never-met/v1", ""];
    let names = [
        "blk.{layer}.someone_elses.a16",
        "totally.unbound",
        "blk.{layer}.ffn_router_up.a16",
        "blk.{layer}.attn_v.weight",
        "blk.{layer}.linear_conv.weight",
        "blk.{layer}.ffn_shared_gate.weight",
        "blk.{layer}.attn_gated.a16",
    ];
    let table = |rng: &mut FuzzRng, p: &mut PalwShapeProfileV3| -> *mut Vec<PalwStepNodeV1> {
        match rng.below(4) {
            0 => &mut p.pre_nodes,
            1 => &mut p.gdn_nodes,
            2 => &mut p.attn_nodes,
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
        match rng.below(9) {
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
                t[i].weight_name = names[rng.below(names.len() as u64) as usize].to_string();
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
            7 => {
                t[i].role = match rng.below(3) {
                    0 => PalwStepNodeRoleV1::Plain,
                    1 => PalwStepNodeRoleV1::KCacheWrite,
                    _ => PalwStepNodeRoleV1::VCacheWrite,
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
/// economics, which the gate does not read. The same construction as the dense harness's.
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
    //
    // **The arity is the one an ARMED CHAIN would play, not one written into the bundle** (audit D
    // M-4). This read `bundle.court.dissection_arity()` while `fuzz_qwen36_profiles_from_v1` wrote
    // the derived value into the bundle first — a configuration no chain has, because no genesis
    // builder writes a derived arity in, and precisely the one in which H-1's stored-versus-derived
    // mismatch is invisible. `palw_court_params_at_v2(bundle, true)` is what the court itself
    // reads at activation, so the harness and the chain now answer the same question.
    let fused = kaspa_consensus_core::palw_class_admission_v2::palw_profile_has_fused_attention_v1(profile);
    let arity = kaspa_consensus_core::palw_court_v2::palw_court_params_at_v2(bundle, fused)
        .map(|c| c.dissection_arity())
        .unwrap_or_else(|_| bundle.court.dissection_arity());
    let court = fused.then_some(kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
        dissection_arity: arity,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
        window_court_daa: kaspa_consensus_core::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1.window_court,
    });
    let ladder = fused.then(|| {
        kaspa_consensus_core::palw_context_ladder::palw_class_ladder_rules_for_court_v1(
            profile,
            court,
            bundle.court.max_step_leaf_count(),
        )
        .unwrap_or_else(|| kaspa_consensus_core::palw_class_admission_v2::PalwClassLadderRulesV1 {
            ladder: bundle.court.max_step_leaf_count(),
            cost_shape: kaspa_consensus_core::palw_class_admission_v2::PalwCourtCostShapeV1::genesis_anchored_v1(
                profile,
                bundle.court.max_step_leaf_count(),
            )
            .with_dissection_v1(arity),
            canonical_footprint_floor: 0,
        })
    });
    kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v5(
        bundle,
        profile,
        &canonical,
        &probe,
        &[],
        &[],
        ladder,
        court,
    )
    .is_ok()
}

/// Drive `iterations` mutated profiles through gate → plan → court cost → double execution. The
/// tally's meaning is `fuzz_a16`'s, column for column; the two FINDING columns plus the ceiling
/// column must be zero for the ADR's fence to arm for this container.
pub fn fuzz_qwen36_profiles_v1(seed: u64, iterations: u64) -> FuzzTallyV1 {
    let (artifact, base) = tiny_class();
    fuzz_qwen36_profiles_from_v1(seed, iterations, &artifact, &base)
}

/// [`fuzz_qwen36_profiles_v1`] over a caller's BASE profile — same schedule, same mutations, same
/// findings, driven from a different graph. A parameter rather than a second harness, for the
/// reason `fuzz_a16_profiles_from_v1` gives: graph v5 is a new graph over the same model, and a
/// gate that only ever saw the four-node attention site says nothing about the one-node one.
pub fn fuzz_qwen36_profiles_from_v1(
    seed: u64,
    iterations: u64,
    artifact: &Qwen36ArtifactV1,
    base: &PalwShapeProfileV3,
) -> FuzzTallyV1 {
    let engine = Qwen36Engine::new(artifact);
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
    // **The bundle is the devnet's, unmutated** (audit D M-4). This wrote the derived arity into
    // `bundle.court` for a fused base so that `gate_accepts` would read 4 back out of it. No chain
    // bundle carries a derived arity, so the harness was measuring a gate no network runs — and it
    // was exactly the configuration in which H-1's stored-versus-derived mismatch cannot be seen.
    let root = artifact.artifact_root();

    let mut rng = FuzzRng::new(seed);
    let mut tally = FuzzTallyV1 { iterations, ..Default::default() };
    // Folded over every executed run, in corpus order — the family's own domain, so a dense
    // corpus and an mmap corpus can never be mistaken for one another.
    let mut corpus = blake2b_simd::Params::new().hash_length(32).key(b"misaka-palw/fuzz-corpus-qwen36/v1").to_state();
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
        // The court's side of the same profile: an admitted class whose worst dispute does not
        // fit the carrier executes, certifies, and can never be policed.
        // A cost the derivation refuses is a profile the gate would have refused too, so there is
        // deliberately no `else` here.
        if let Ok(cost) = kaspa_consensus_core::palw_class_admission_v2::derive_court_cost_v1(&profile) {
            tally.court_costed += 1;
            tally.max_close_bytes_seen = tally.max_close_bytes_seen.max(cost.max_close_bytes);
            if cost.max_close_bytes > kaspa_consensus_core::palw_class_admission_v2::PALW_RC_COURT_MAX_CLOSE_BYTES {
                tally.closes_over_ceiling += 1;
            }
        }

        // A caller's prompt, twice; the bits must not care which run it was. The recurrent state
        // and both caches ride inside the run, so a nondeterminism in EITHER shows in the rows.
        let prompt: Vec<usize> = (0..(1 + rng.below(4) as usize)).map(|_| rng.below(64) as usize).collect();
        let run = |()| -> Result<Vec<(Vec<i32>, crate::qwen36_plan::Qwen36PlanTraceV1)>, ()> {
            let mut cache = Qwen36Cache::new(&artifact.shape);
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
                    // the order the walk produced them.
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
                            trace.layers.iter().flatten().for_each(&mut fold);
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

    /// The CI-sized slice of the saturation run — the same assertion set as the dense harness's:
    /// zero findings, and every measurement column actually measured something.
    #[test]
    fn a_bounded_fuzz_run_finds_no_panic_and_no_nondeterminism() {
        let tally = fuzz_qwen36_profiles_v1(0x0067_2026_0901, 400);
        println!("qwen36 fuzz tally: {tally:?}");
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

    /// **The same gate over ADR-0082's graph v5 on the hybrid** — the fused attention site driven
    /// through the same mutate → admit → plan → execute-twice schedule. The GDN table is
    /// untouched by the fusion and still in the corpus, so this is the v2 space with the attention
    /// site replaced rather than a smaller space.
    ///
    /// The corpus digest for THIS base is pinned separately, by
    /// `the_v5_fuzz_corpus_digest_is_the_same_on_every_machine` below.
    #[test]
    fn a_bounded_fuzz_run_over_the_v5_graph_finds_no_panic_and_no_nondeterminism() {
        let (artifact, base) = tiny_class_v5();
        let tally = fuzz_qwen36_profiles_from_v1(0x0082_2026_0903, 400, &artifact, &base);
        println!("qwen36 v5 fuzz tally: {tally:?}");
        assert_eq!(tally.panics, 0, "a panic inside the interpreter is the fence staying down");
        assert_eq!(tally.nondeterminism, 0, "two runs of one plan must be one bitstream");
        assert!(tally.executed > 0, "the v5 corpus must actually reach execution, or this test gates nothing");
        assert!(tally.court_costed > 0, "and the court cost must actually derive for a v5 row");
        assert!(tally.max_close_bytes_seen > 0, "a zero here would mean the ceiling was never compared against anything");
        // Reported, not asserted zero: pricing the fused site down from its whole-row close to the
        // dissection's bounded one is ADR-0082 Decision 2 / the cost stream's derivation.
        println!("qwen36 v5 closes over the shipped ceiling: {}", tally.closes_over_ceiling);
    }

    /// **The cross-architecture clause for the mmap container, met by the suite** — one seed, one
    /// corpus, one number, asserted wherever the suite runs. The dense harness's re-pin
    /// discipline applies verbatim: a deliberate change that moves this value re-pins it in ONE
    /// commit that says which change moved it, never alongside other work.
    #[test]
    fn the_fuzz_corpus_digest_is_the_same_on_every_machine() {
        let tally = fuzz_qwen36_profiles_v1(0x0067_2026_0901, 400);
        assert_eq!(
            faster_hex::hex_string(&tally.corpus_digest),
            CORPUS_DIGEST_400,
            "this machine computed a different corpus than the pinned one — see the doc above before touching the pin"
        );
    }

    /// Seed `0x0067_2026_09_01`, 400 iterations. See the test above.
    const CORPUS_DIGEST_400: &str = "5b9be0a7479f824a1d87fdd33785a226c551130a4768730855d068a4e3acf8e1";

    /// **The same clause over the FUSED base on the hybrid** (ADR-0082 audit E, M-2).
    ///
    /// The pin above folds the graph-v2 space; the fused attention site is new arithmetic on the
    /// executor's hot path and the two-runs-in-one-process gate beside it cannot see a machine
    /// that computes it differently. A second base gets a second pin rather than the first one
    /// being widened, so neither hides the other. Re-pin discipline is the v2 pin's, verbatim.
    #[test]
    fn the_v5_fuzz_corpus_digest_is_the_same_on_every_machine() {
        let (artifact, base) = tiny_class_v5();
        let tally = fuzz_qwen36_profiles_from_v1(0x0082_2026_0903, 400, &artifact, &base);
        assert_eq!(
            faster_hex::hex_string(&tally.corpus_digest),
            V5_CORPUS_DIGEST_400,
            "this machine computed a different fused corpus than the pinned one — see the doc above before touching the pin"
        );
        assert_ne!(V5_CORPUS_DIGEST_400, CORPUS_DIGEST_400, "two bases must not fold to one number, or one of them is unpinned");
    }

    /// Seed `0x0082_2026_0903`, 400 iterations, over the graph-v5 hybrid base. See the test above.
    const V5_CORPUS_DIGEST_400: &str = "7e5f14c2b0d66fc53ef1057188b29ec26030f80cb73f666244a22ac33854b5fd";

    /// **The panic the design pass found stays closed.** A convolution declared in the ATTENTION
    /// table lands on a window the artifact's layer kinds never pre-filled, and the walk's
    /// `remove(0)` ran BEFORE the tap tensor was read — so the panic fired ahead of the refusal
    /// that would otherwise have covered it. The profile is built by hand rather than hoped for
    /// from the schedule: a bespoke attention table whose convolution is fed from the shared
    /// expert's projections (the one attention-layer source at the recurrence's k width), so the
    /// plan compiles and the walk actually reaches the window. Post-fix, the window fills with
    /// the zero rows a fresh sequence has, and the walk then REFUSES at the tap tensor an
    /// attention layer does not carry — a decision with a name, never an unwind.
    #[test]
    fn a_convolution_declared_in_the_attention_table_refuses_instead_of_panicking() {
        use kaspa_consensus_core::palw_step::{PALW_STEP_INPUT_LAYER_IN, PalwStepNodeV1};
        use kaspa_consensus_core::palw_step_refute::{
            KDESC_A16_REQUANTIZE, KDESC_A16_RMS_NORM, KDESC_Q36_MATMUL_GROUPED, KDESC_Q36_MATMUL_GROUPED_WIDE, KDESC_Q36_SSM_CONV,
        };
        let (artifact, base) = tiny_class();
        let engine = Qwen36Engine::new(&artifact);
        let node = |kernel: &str, role, weight: &str, elements: u32, refs: Vec<u16>, op| PalwStepNodeV1 {
            op_kind: op,
            role,
            weight_name: weight.to_string(),
            weight_dtypes: if weight.is_empty() { Vec::new() } else { vec![24; 1] },
            out_len: PalwStepOutLenV1::Fixed { elements },
            tile_len: 16,
            kernel_semantics_id: kernel_semantics_id_v1(kernel),
            input_refs: refs,
        };
        use kaspa_consensus_core::palw_step::PalwStepOpKindV1 as Op;
        let plain = PalwStepNodeRoleV1::Plain;
        let mut profile = base.clone();
        profile.attn_nodes = vec![
            node(KDESC_A16_RMS_NORM, plain, "", 32, vec![PALW_STEP_INPUT_LAYER_IN], Op::RmsNorm),
            node(KDESC_A16_REQUANTIZE, plain, "blk.{layer}.attn_norm.a16", 32, vec![0], Op::MulElem),
            node(KDESC_Q36_MATMUL_GROUPED, plain, "blk.{layer}.ffn_shared_expert_gate.weight", 16, vec![1], Op::MatMulQuant),
            node(KDESC_Q36_MATMUL_GROUPED_WIDE, plain, "blk.{layer}.ffn_shared_expert_up.weight", 16, vec![1], Op::MatMulQuant),
            // The convolution: k-width, k-width, v-width — (16, 16, 32) in this geometry — and
            // the declared conv row 2·16 + 32.
            node(KDESC_Q36_SSM_CONV, plain, "blk.{layer}.linear_conv.weight", 64, vec![2, 3, 1], Op::SsmConv),
            node(KDESC_A16_REQUANTIZE, plain, "blk.{layer}.attn_align.a16", 32, vec![PALW_STEP_INPUT_LAYER_IN], Op::MulElem),
            node(KDESC_A16_REQUANTIZE, plain, "blk.{layer}.attn_residual.a16", 32, vec![5], Op::MulElem),
        ];
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let plan =
                engine.plan_from_profile(&profile).expect("the bespoke table is plannable — that is what makes the site reachable");
            let mut cache = Qwen36Cache::new(&artifact.shape);
            let mut last = Ok(Vec::new());
            for (position, token) in [3usize, 10, 17].into_iter().enumerate() {
                last = engine.forward_token_planned(&plan, &mut cache, token, position).map(|(logits, _)| logits);
            }
            last
        }));
        let decision = outcome.expect("a plannable declaration must never unwind the walk");
        let err = decision.expect_err("an attention layer carries no convolution taps, so the walk refuses");
        assert!(
            err.to_string().contains("linear_conv.weight"),
            "the refusal names the tap tensor — proof the walk got PAST the window it used to panic on: {err}"
        );
    }
}
