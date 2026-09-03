//! **The composition nothing in the tree had: a real bisection CONVERGES on the real divergent
//! leaf, and the close at that leaf convicts** — the two halves of a court, run against one
//! another through real backend material rather than each in its own fixture.
//!
//! Every conviction test before this called the close with the ladder bypassed, and the one test
//! that drove real rungs to a terminal asserted only refusals; `kaspad/src/palw_panel.rs` says in
//! so many words that the terminal move was "STILL NOT COVERED BY A TEST". The chain-state
//! plumbing around these calls (session acceptance, signatures, deadlines, settlement) has its
//! own coverage in `palw_state_v2`; what was never composed is the part below it — the part that
//! decides who wins.
//!
//! The cast: one A16 (corrected class) job, executed honestly and executed with one injected
//! fault. The producer answers rungs from the TAMPERED material it committed; the challenger
//! verdicts from its own honest re-execution. The ladder must converge on exactly the tampered
//! leaf, the cost gate must admit the close, the operands must prove against the class's
//! inventory root, and the arithmetic must convict — then the honest direction: the same close
//! machinery over the honest material reads `NoFaultFound`, which the court maps to
//! `ChallengerDefeated`.

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_bisect::{
    PALW_BISECT_OBJECT_VERSION_V1, PalwBisectDisclosureV1, PalwBisectLadderV1, PalwBisectSpaceV1, PalwBisectTurnV1,
    PalwBisectVerdictV1,
};
use kaspa_consensus_core::palw_court_v2::{
    PalwCourtVerdictProofV2, check_arithmetic_close_binding, check_close_cost_v2, map_refutation_outcome,
};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v2};
use kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2;
use kaspa_consensus_core::palw_step_refute::check_execution_step_refutation_v1;
use kaspa_hashes::Hash64;
use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
use misaka_palw_base0::engine_a16::derived_a16_store;
use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;

const NETWORK: &[u8] = b"misaka-palw-rc";

/// The corrected A16 class at a unit-test geometry — the same construction the backend's own
/// sweep uses, from ONE geometry so the artifact and the profile cannot describe different
/// models.
const GEOMETRY: PalwQwen25GeometryV1 = PalwQwen25GeometryV1 {
    layer_count: 2,
    hidden_dim: 8,
    ffn_dim: 8,
    attn_heads: 2,
    attn_kv_heads: 2,
    attn_head_dim: 4,
    vocab_size: 64,
    n_ctx: 32,
    n_threads: 1,
    rms_eps_q: 1,
    tile_len: 4,
};

fn class() -> (std::sync::Arc<Base0ArtifactV1>, kaspa_consensus_core::palw_step::PalwShapeProfileV3) {
    let geometry = GEOMETRY;
    let profile = qwen25_a16_profile_v2(geometry).expect("the corrected A16 profile projects");
    let shape = Base0ShapeV1 {
        n_layers: geometry.layer_count as usize,
        n_heads: geometry.attn_heads as usize,
        n_kv_heads: geometry.attn_kv_heads as usize,
        d_head: geometry.attn_head_dim as usize,
        d_ff: geometry.ffn_dim as usize,
        vocab: geometry.vocab_size as usize,
        max_position: geometry.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: 1,
    };
    let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
        .expect("a valid shape")
        .with_a16_params(derived_a16_store(&shape))
        .expect("the derived store is sorted and unique");
    (std::sync::Arc::new(artifact), profile)
}

/// The same class as [`class`], projected as ADR-0082's graph v5: one fused attention node per
/// layer instead of four, over the SAME artifact — which is the point of Decision 1's "the
/// artifact is UNCHANGED".
fn class_v5() -> (std::sync::Arc<Base0ArtifactV1>, kaspa_consensus_core::palw_step::PalwShapeProfileV3) {
    let (artifact, v2) = class();
    let v5 = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v5(GEOMETRY).expect("the v5 row projects");
    assert_ne!(v2.shape_profile_id(), v5.shape_profile_id(), "a different graph is a different class");
    (artifact, v5)
}

#[test]
fn a_real_bisection_converges_on_the_tampered_leaf_and_the_close_convicts() {
    let (artifact, profile) = class();
    let backend = Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), (4, 3))
        .expect("the fixture's declaration is this engine's program");
    let anchor = Hash64::from_u64_word(0xC0117E2E);
    let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");

    let honest = backend.execute(&job, &prompt).expect("the honest run");
    let (binding, _, _, _, _) =
        misaka_palw_base0::produce::base0_material_decode_v1(&honest.material).expect("our own material decodes");
    let leaf_count = binding.step_leaf_count;

    // The fault, at a leaf deep enough that the ladder has real narrowing to do.
    let fault_at = leaf_count / 2 + 3;
    let guilty = backend.execute_with_injected_fault(&job, &prompt, fault_at).expect("a tampered capture still commits");
    assert_ne!(guilty.execution_root, honest.execution_root, "the tamper moved the commitment");
    // The tamper is in a step tile, not in the logits retention, so the claim's TRACE root is the
    // honest one — which is exactly the shape of a producer lying about arithmetic it never did.
    assert_eq!(guilty.trace_root, honest.trace_root);

    // --- the ladder, driven by the two parties' own materials -----------------------------------
    //
    // Seeded the way the V2 transition seeds it: `open(claim_id, claim.trace_root, …)`.
    let claim_id = Hash64::from_u64_word(0xC7A1);
    let challenger = Hash64::from_u64_word(0xC1);
    let responder = Hash64::from_u64_word(0xE2);
    let mut ladder = PalwBisectLadderV1::open(
        &claim_id,
        &guilty.trace_root,
        &challenger,
        &responder,
        PalwBisectSpaceV1::StepLeaves,
        leaf_count,
        100,
        200,
    )
    .expect("the dispute opens over the whole step space");

    let mut rungs = 0u32;
    while ladder.turn() != PalwBisectTurnV1::Terminal {
        let midpoint = ladder.expected_midpoint().expect("a non-terminal ladder awaits a disclosure");
        // The RESPONDER answers from the material it committed — the tampered one.
        let mid_state = backend.bisect_prefix_state(&guilty.material, midpoint).expect("the responder states its prefix");
        ladder
            .apply_disclosure(
                &PalwBisectDisclosureV1 {
                    version: PALW_BISECT_OBJECT_VERSION_V1,
                    session_id: ladder.session_id(),
                    round: rungs,
                    midpoint,
                    mid_state,
                },
                100 + u64::from(rungs) * 2 + 1,
                50,
            )
            .expect("a canonical disclosure is a move");
        // The CHALLENGER verdicts from its own honest execution of the same job.
        let own = backend.bisect_prefix_state(&honest.material, midpoint).expect("the challenger states its prefix");
        ladder
            .apply_verdict(
                &PalwBisectVerdictV1 {
                    version: PALW_BISECT_OBJECT_VERSION_V1,
                    session_id: ladder.session_id(),
                    round: rungs,
                    agree: own == mid_state,
                },
                100 + u64::from(rungs) * 2 + 2,
                50,
            )
            .expect("a canonical verdict narrows");
        rungs += 1;
    }

    // Convergence: the first index the two executions disagree on IS the tampered leaf, and the
    // rung count is the logarithm the ladder promises, not the space.
    let narrowed = ladder.terminal_index().expect("a terminal ladder names its step");
    assert_eq!(narrowed, fault_at, "the bisection must converge on the tampered leaf");
    assert!(u64::from(rungs) <= 64, "narrowing is logarithmic; {rungs} rungs for {leaf_count} leaves");

    // --- the terminal move: the close, at the narrowed step -------------------------------------
    //
    // Assembled from the RESPONDER's own served material — both sides run the same prover — and
    // checked exactly as `adjudicate_close_proof_v2`'s arithmetic arm checks it: the cost gate,
    // the two binding pins, the proven operands, the recomputation. (The state-carrying wrapper
    // adds the session lookup and the narrowed-step guard, which `palw_state_v2`'s own tests
    // cover; the class record's `artifact_root` is the inventory root passed here.)
    let refutation = backend.refutation_for_index(&guilty.material, narrowed).expect("the narrowed leaf opens");
    let openings = backend.operand_openings_for(&refutation).expect("the prover opens what the court resolves");
    let inventory = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile).expect("the class yields an inventory");

    let court = PalwCourtParamsV2::new(leaf_count, 50, 4).expect("court params");
    let proof = PalwCourtVerdictProofV2::Arithmetic { refutation: refutation.clone(), operand_openings: openings.clone() };
    check_close_cost_v2(&proof, &court).expect("the close rides the carrier");
    check_arithmetic_close_binding(guilty.trace_root, refutation.binding.full_logits_trace_root)
        .expect("the refutation binds the claim's own trace root");
    assert_eq!(refutation.binding.committed_execution_root, guilty.execution_root, "and the claim's execution root");
    let operands = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
        .expect("every carried operand proves against the class's registered root");
    let verdict =
        map_refutation_outcome(check_execution_step_refutation_v1(&refutation, &operands)).expect("the narrowed step adjudicates");
    assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty, "a tampered leaf at the narrowed step convicts");

    // --- the honest direction through the same machinery ----------------------------------------
    //
    // An honest producer closing its own case assembles the identical object from its honest
    // material; the court reads `NoFaultFound` and maps it to the challenger's defeat.
    let honest_refutation = backend.refutation_for_index(&honest.material, narrowed).expect("the honest capture opens too");
    let honest_openings = backend.operand_openings_for(&honest_refutation).expect("and proves its operands");
    let honest_proof =
        PalwCourtVerdictProofV2::Arithmetic { refutation: honest_refutation.clone(), operand_openings: honest_openings.clone() };
    check_close_cost_v2(&honest_proof, &court).expect("the honest close rides the same carrier");
    let honest_operands =
        kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&honest_openings, inventory.root())
            .expect("proves");
    let verdict =
        map_refutation_outcome(check_execution_step_refutation_v1(&honest_refutation, &honest_operands)).expect("adjudicates");
    assert_eq!(verdict, PalwCourtVerdictV2::ChallengerDefeated, "an honest execution clears itself at the same step");
}

/// **ADR-0082 Decision 1, unit U-02, adjudicated end to end: the court's whole-row arm reproduces
/// the engine at a FUSED attention leaf, and convicts a tampered one.**
///
/// The one composition that says the fused site is real. Everything else about graph v5 is a
/// projection or a kernel identity; this drives a registered v5 row through the actual producer
/// (`from_registered_profile`, so the plan is the declared graph and not the compiled engine's own
/// table), targets the leaf the FUSED node commits, and puts both directions through the shipped
/// close:
///
/// * honest → `NoFaultFound` → `ChallengerDefeated`. This is the assertion that matters most,
///   because the arm opens the whole K and V history and recomputes the four kernels composed: a
///   court that read the series differently, resolved a different registered triple, or clamped
///   the softmax's widening byte differently would convict an honest producer here.
/// * one tampered value in that same row → `ExecutorGuilty`.
///
/// The inventory is the class's own (`a16_inventory_v1` over the v5 profile), so the four operands
/// the arm resolves are proven against the same artifact root a chain would register — which is
/// also the statement that the artifact is UNCHANGED by the fusion.
#[test]
fn a_fused_attention_leaf_adjudicates_both_ways() {
    use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepOpKindV1, canonical_step_leaf_index};
    use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;

    let (artifact, v5) = class_v5();
    // Z0's first half, on the row this test actually executes.
    assert!(
        v5.attn_nodes.iter().all(|n| !matches!(n.out_len, kaspa_consensus_core::palw_step::PalwStepOutLenV1::KvScaled { .. })),
        "a v5 layer table still commits a context-shaped row"
    );
    let backend = Qwen25A16Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), v5.clone(), (4, 3))
        .expect("the v5 row is servable by this build");
    let anchor = Hash64::from_u64_word(0x0000_082F_05ED);
    let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
    let honest = backend.execute(&job, &prompt).expect("the honest run");
    let (binding, _, _, _, _) =
        misaka_palw_base0::produce::base0_material_decode_v1(&honest.material).expect("our own material decodes");

    // The FUSED node's own leaf, named through the profile rather than by counting: the first
    // global slot whose node is `AttnFused`, at a prefill position with real history behind it.
    let fused_slot = (0..v5.global_node_count())
        .find(|slot| v5.resolve_node_slot(*slot).is_some_and(|(n, _)| n.op_kind == PalwStepOpKindV1::AttnFused))
        .expect("a v5 row has a fused site");
    let position = binding.job_context.declared_prefill_tokens.saturating_sub(2).max(1);
    let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: fused_slot, position, tile_index: 0 };
    let fused_leaf =
        canonical_step_leaf_index(&v5, &binding.job_context, &coord).expect("the fused site's leaf is in the enumeration");

    let inventory = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &v5).expect("the v5 class yields an inventory");

    // --- honest: the arm reproduces the engine --------------------------------------------------
    let honest_refutation = backend.refutation_for_index(&honest.material, fused_leaf).expect("the fused leaf opens");
    assert_eq!(honest_refutation.output_preimage.coord.node_slot, fused_slot, "the leaf we opened is the fused site's");
    let honest_openings = backend.operand_openings_for(&honest_refutation).expect("the prover opens what the court resolves");
    let honest_operands =
        kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&honest_openings, inventory.root())
            .expect("every operand the fused arm resolves proves against the class's registered root");
    let court = PalwCourtParamsV2::new(binding.step_leaf_count, 50, 4).expect("court params");
    let honest_proof =
        PalwCourtVerdictProofV2::Arithmetic { refutation: honest_refutation.clone(), operand_openings: honest_openings.clone() };
    // The whole-row route opens the K and V history, which is the price ADR-0082 Decision 2's
    // dissection exists to replace — so the cost is REPORTED here rather than asserted small.
    println!("fused close bytes: {:?}", kaspa_consensus_core::palw_court_v2::arithmetic_close_bytes_v2(&honest_proof));
    let _ = check_close_cost_v2(&honest_proof, &court);
    let verdict = check_execution_step_refutation_v1(&honest_refutation, &honest_operands);
    assert!(
        matches!(verdict, Err(PalwStepRefuteError::NoFaultFound)),
        "the court's fused arm did not reproduce the engine's own row: {verdict:?}"
    );
    assert_eq!(
        map_refutation_outcome(verdict).expect("adjudicates"),
        PalwCourtVerdictV2::ChallengerDefeated,
        "an honest fused site clears itself"
    );

    // --- tampered: one value of that row convicts -----------------------------------------------
    let guilty = backend.execute_with_injected_fault(&job, &prompt, fused_leaf).expect("a tampered capture still commits");
    assert_ne!(guilty.execution_root, honest.execution_root, "the tamper moved the commitment");
    let refutation = backend.refutation_for_index(&guilty.material, fused_leaf).expect("the tampered fused leaf opens");
    let openings = backend.operand_openings_for(&refutation).expect("and proves its operands");
    let operands = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
        .expect("proves against the same root");
    let verdict = map_refutation_outcome(check_execution_step_refutation_v1(&refutation, &operands)).expect("adjudicates");
    assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty, "a tampered fused row must convict");
}
