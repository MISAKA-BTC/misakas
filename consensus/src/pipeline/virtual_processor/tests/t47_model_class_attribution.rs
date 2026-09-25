//! **ADR-0152 v3.1 F1-M, Tier C (addendum §4-bis, "T18e on model classes becomes a conviction"): a
//! model class's claim is held to `CoreV1`'s job, and kind 4 convicts every way out of it.**
//!
//! T46's harness, doors and assertions — this module is that suite's child, so nothing here is a
//! second copy of them: testnet-12's shipped ruleset with harness keys on its eight cards, every
//! offence through the gate, the acceptance walk and the fold, weighed in one 0x4b carrier.
//!
//! **The classes are the held fixtures** — the A16 graph-v7 row at `n_ctx` 128 (the formula's
//! `(15, 2)`) and the Qwen3.6 graph-v7 row at `n_ctx` 32 (`(3, 2)`) — each backend built as the
//! SDK builds it on testnet-12: its registered profile, the network's prompt form and ladder, and
//! `CoreV1` from `palw_attempt_rules_of_params_v1`. testnet-12's genesis registers its model rows at
//! 2M and 512 and the registry holds both, so no block of this harness can register and admit a
//! fixture; the class is seeded through the carriage (the spec's named fallback), copying the
//! genesis model row with the class admitting and the floor's pricing — and nothing else. **The
//! claim itself is the fold's**: the template this node builds, its anchor
//! (`execution_anchor_v3` under the model class), the backend's `job_for_anchor` of that anchor,
//! the producer's run, the attempt signed by card 0 and folded as the block's own work, which
//! records the anchor (J-1) exactly as it does for the floor.
//!
//! What each test proves, on both families:
//!
//! * **T18e (model)**: the relabel of another anchor's prompt under this anchor (J1, J3 hold) is
//!   `IdentityMismatch` J5b, convicted by kind 4 before licence; the honest twin is refused
//!   `IdentityHolds` and the walk drops it; a seat's own rule refuses the relabel too.
//! * **T18s**: a claim committing an `output_root` its generated ids do not render to is
//!   `OutputMismatch` (10) through the tiled pin; forfeiture is by claim — a borrower of an honest
//!   claim's roots under a ground output root is voided and the lender's claim, reservation and root
//!   stand.
//! * **T18t**: J6 (a non-canonical activation leg), J7 (a non-canonical checkpoint profile), and J5a
//!   twice — a short prefill (the formula's job with one prompt id fewer) and the `Legacy` rule's
//!   context (an instance's own fields in the job) — each re-run or re-committed by the producer,
//!   each convicted by kind 4.

use super::*;
use kaspa_consensus_core::palw_attempt_rules_v1::{PalwAttemptRulesV1, palw_attempt_rules_of_params_v1};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_step_refute::{PalwDecodeTokenPinV1, PalwTiledDecodeTokensV1, tiled_logits_rows_root_v1};

/// One model class: its registered profile, its artifact's root, and its backend under the
/// network's rule (`CoreV1` on testnet-12) and under `Legacy` (the rule an instance ran before the
/// fence — J5a's `LegacyContextField`).
struct ModelClass {
    label: &'static str,
    profile: PalwShapeProfileV3,
    artifact_root: Hash64,
    backend: Box<dyn PalwExecutionBackendV1>,
    legacy: Box<dyn PalwExecutionBackendV1>,
}

impl ModelClass {
    fn class_id(&self) -> Hash64 {
        self.profile.shape_profile_id()
    }
}

/// The held A16 graph-v7 fixture (`base0/tests/common`'s), at `n_ctx` 128 and a 128-id vocabulary.
fn a16_fixture(h: &H) -> ModelClass {
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_held_canonical_v1, qwen25_a16_profile_v7};
    use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;
    let g = PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 32,
        ffn_dim: 64,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 8,
        vocab_size: 128,
        n_ctx: 128,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    };
    let shape = Base0ShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        n_kv_heads: g.attn_kv_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        max_position: g.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: g.rms_eps_q,
    };
    let artifact = Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
            .expect("the derived store is sorted and unique"),
    );
    let profile = qwen25_a16_profile_v7(g).expect("the held graph-v7 row projects");
    let canonical = qwen25_a16_held_canonical_v1(profile.n_ctx);
    let build = |rules: PalwAttemptRulesV1| {
        Qwen25A16Backend::new(artifact.clone(), h.config.params.net.to_string().into_bytes(), profile.clone(), canonical)
            .expect("the fixture's declaration is this engine's program")
            .with_step_ladder_cap(h.bundle.court.max_step_leaf_count())
            .with_prompt_ids_form(h.form())
            .with_attempt_rules(rules)
    };
    let backend = build(palw_attempt_rules_of_params_v1(&h.config.params));
    let artifact_root = backend.artifact_root_and_leaf_count().expect("the fixture's inventory roots").0;
    let legacy = Box::new(build(PalwAttemptRulesV1::Legacy));
    ModelClass { label: "A16 held v7", profile, artifact_root, backend: Box::new(backend), legacy }
}

/// The held Qwen3.6 graph-v7 fixture (`fuzz_qwen36`'s tiny class) at `n_ctx` 32.
fn qwen36_fixture(h: &H) -> ModelClass {
    use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_held_canonical_v1, qwen36_profile_v7};
    use misaka_palw_base0::qwen36_backend::Qwen36Backend;
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
        n_ctx: 32,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 512,
    };
    let artifact = Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8));
    let profile = qwen36_profile_v7(geometry).expect("the held graph-v7 projection");
    let canonical = qwen36_held_canonical_v1(profile.n_ctx);
    assert_eq!(canonical, (3, 2), "the formula at 32");
    let build = |rules: PalwAttemptRulesV1| {
        Qwen36Backend::from_registered_profile(
            artifact.clone(),
            h.config.params.net.to_string().into_bytes(),
            profile.clone(),
            canonical,
        )
        .expect("servable")
        .with_step_ladder_cap(h.bundle.court.max_step_leaf_count())
        .with_prompt_ids_form(h.form())
        .with_attempt_rules(rules)
    };
    let backend = build(palw_attempt_rules_of_params_v1(&h.config.params));
    let artifact_root = backend.artifact_root_and_leaf_count().expect("the fixture's inventory roots").0;
    let legacy = Box::new(build(PalwAttemptRulesV1::Legacy));
    ModelClass { label: "Qwen3.6 held v7", profile, artifact_root, backend: Box::new(backend), legacy }
}

/// **The class, seeded through the carriage**: its record (the floor's pricing, its own artifact
/// root and fused-attention answer), the floor's target, a share donated from the table (the loader
/// holds every active class to one and the table to 1000‰), and testnet-12's genesis model row with
/// the class admitting — the one thing no block of this harness can write.
fn seed(h: &H, walk: &mut Walk, m: &ModelClass) {
    seed_profile(h, walk, m.label, &m.profile, m.artifact_root);
}

/// [`seed`] for a class known by its profile alone (the 2M-sized row, whose producer no test runs).
fn seed_profile(h: &H, walk: &mut Walk, label: &str, profile: &PalwShapeProfileV3, artifact_root: Hash64) {
    use kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1;
    use kaspa_consensus_core::palw_state_v2::PalwClassStateV2;
    assert_eq!(
        palw_attempt_rules_of_params_v1(&h.config.params),
        PalwAttemptRulesV1::CoreV1,
        "testnet-12's producers run CoreV1 from genesis"
    );
    let (floor, class_id) = (h.floor(), profile.shape_profile_id());
    let fused = kaspa_consensus_core::palw_class_admission_v2::palw_profile_has_fused_attention_v1(profile);
    walk.state = h.rebuilt(&walk.state, |c| {
        assert!(!c.classes.contains_key(&class_id), "{label}: the fixture is not a genesis class");
        let record = PalwClassStateV2 { artifact_root, fused_attention: fused, ..c.classes[&floor].clone() };
        c.classes.insert(class_id, record);
        let target = c.class_targets[&floor].clone();
        c.class_targets.insert(class_id, target);
        // An active class holds a share, and the table conserves 1000‰: half of the largest model
        // share is donated (the floor's, one permille, if no model class holds any).
        let donor = c
            .class_shares
            .iter()
            .filter(|(id, share)| **id != floor && **share >= 2)
            .max_by_key(|(_, share)| **share)
            .map(|(id, _)| *id)
            .unwrap_or(floor);
        let given = if donor == floor { 1 } else { c.class_shares[&donor] / 2 };
        *c.class_shares.get_mut(&donor).unwrap() -= given;
        c.class_shares.insert(class_id, given);
        // testnet-12's 8k row, named: the model row with the shortest derived verification deadline
        // (`D` 15). Not "the first non-floor row" — the map is keyed by class id and the 2M row
        // (`0x74…`) sorts before the 8k one (`0xeb…`), and since ADR-0152 §4-quater the 2M row's
        // derived 13,995-DAA deadline is unmeasured at launch (U-D1), so a class copying it is refused
        // `ClassDeadlineUnmeasured` before this test's attribution is ever asked.
        let (_, row) = c
            .model_lifecycles
            .iter()
            .filter(|(id, _)| **id != floor)
            .min_by_key(|(_, row)| row.work.verification_ccu)
            .expect("testnet-12 registers model rows");
        assert!(
            !kaspa_consensus_core::palw_state_v2::palw_class_row_is_long_d_v1(row),
            "{label}: the 8k row, a short-D class open at launch"
        );
        let mut row = row.clone();
        row.state = PalwModelLifecycleV1::Active;
        row.profile.max_inflight_claims = row.profile.max_inflight_claims.max(8);
        c.model_lifecycles.insert(class_id, row);
    });
}

/// How the producer of a model claim committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelFault {
    Honest,
    /// Another anchor's prompt, run with this anchor written as the job id and seed (J5b).
    Relabel,
    /// The honest run under an `output_root` its ids do not render to (10).
    OutputRoot,
    /// The honest binding with its activation leg moved, re-committed (J6).
    ActivationLeg,
    /// The honest binding with its checkpoint profile moved, re-committed (J7).
    CheckpointProfile,
    /// The formula's job with one prompt id fewer, run (J5a).
    ShortPrefill,
    /// The `Legacy` rule's job for this anchor — an instance's fields in the context — run (J5a).
    LegacyContext,
    /// **F1c R1**: the honest step tree, logits row 0's interior lane bent without moving its argmax,
    /// the trace root re-committed (12).
    BendLogits,
    /// **F1c T4**: the honest rows, the first token replaced by its row's runner-up, re-committed
    /// with the output root its ids render to (11 `NotSelected`).
    TokenNotSelected,
    /// **F1c T5**: the first token replaced by an id past the vocabulary (11 `OutOfVocab`).
    TokenOutOfVocab,
}

struct ModelClaim {
    claim_id: Hash64,
    anchor: Hash64,
    envelope: PalwAttemptEnvelopeV2,
    binding: PalwStepBindingV2,
    material: Vec<u8>,
    /// The tiled pin of the generated ids the claim's binding commits.
    pin: PalwDecodeTokenPinV1,
    contradiction: Option<C>,
}

impl ModelClaim {
    fn contradiction(&self) -> C {
        self.contradiction.clone().expect("a faulted claim carries its proof")
    }

    fn roots(&self) -> PalwClaimRootsV1 {
        let a = &self.envelope.attempt;
        PalwClaimRootsV1 {
            execution_root: a.execution_root,
            trace_root: a.trace_root,
            anchor: self.anchor,
            attempt_draw: Some(true),
            output_root: Some(a.output_root),
            job_pin: None,
        }
    }
}

fn roots_of(o: &PalwExecutionOutcomeV1) -> AttemptRoots {
    AttemptRoots {
        trace: o.trace_root,
        output: o.output_root,
        execution: o.execution_root,
        manifest: o.trace_manifest_root,
        chunks: o.trace_chunk_count,
    }
}

/// The capture's binding and the tiled pin of its generated ids.
fn decoded(material: &[u8]) -> (PalwStepBindingV2, PalwDecodeTokenPinV1) {
    let retention = misaka_palw_base0::produce::base0_material_decode_any_v1(material).expect("the capture decodes");
    let binding = retention.binding().clone();
    let rows_root = tiled_logits_rows_root_v1(&binding.job_context, retention.logits_rows()).expect("the rows build a tree");
    let pin = PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 {
        rows_root,
        generated_token_ids: retention.generated_token_ids().to_vec(),
    });
    (binding, pin)
}

/// **A model claim opened from the producer's own work** on the template carrying `nonce`, folded
/// as the block's own attempt by T46's `fold_own_attempt` (which asserts the claim records its
/// anchor).
fn open_model_claim(h: &H, walk: &mut Walk, m: &ModelClass, fault: ModelFault, nonce: u64) -> ModelClaim {
    use kaspa_consensus_core::hashing::header::pre_pow_hash_64;
    let template = h.ctx.build_block_template_keeping_time(nonce);
    let header: Header = template.block.header.clone();
    let bond = h.cards[EXECUTOR];
    // The floor's facts for card 0, restated for the seeded class: the class copies the floor's
    // pricing, so its pwu and retention are the floor's.
    let mut facts = h.ctx.consensus.palw_producer_facts_v2(h.floor(), Some(bond.0)).expect("testnet-12 answers for card 0");
    facts.class_id = m.class_id();
    facts.artifact_root = m.artifact_root;
    let pre_pow = pre_pow_hash_64(&header);
    let anchor = execution_anchor_v3(h.domain, pre_pow, m.class_id(), &bond.0, header.nonce);
    let (canonical, prompt) = m.backend.job_for_anchor(anchor).expect("the class implies a job");
    let job = palw_attempt_job_v1(canonical, true);
    let honest = m.backend.execute(&job, &prompt).expect("the class runs its own job");
    let (honest_binding, honest_pin) = decoded(&honest.material);
    assert_eq!(honest_binding.committed_execution_root, honest.execution_root);
    let recommitted = |edit: &dyn Fn(&mut PalwStepBindingV2)| {
        let mut b = honest_binding.clone();
        edit(&mut b);
        rebind(&mut b);
        verify_binding_v1(&b).expect("the re-committed binding is well-formed");
        let roots = AttemptRoots { execution: b.committed_execution_root, ..roots_of(&honest) };
        (roots, b.clone(), honest.material.clone(), honest_pin.clone(), Some(C::IdentityMismatch { binding: b }))
    };
    let run = |backend: &dyn PalwExecutionBackendV1, job: &PalwJobContextV2, prompt: &[usize]| {
        let run = backend.execute(job, prompt).expect("the producer's job runs");
        let (binding, pin) = decoded(&run.material);
        (roots_of(&run), binding.clone(), run.material.clone(), pin, Some(C::IdentityMismatch { binding }))
    };
    let (roots, binding, material, pin, contradiction) = match fault {
        ModelFault::Honest => (roots_of(&honest), honest_binding.clone(), honest.material.clone(), honest_pin.clone(), None),
        ModelFault::Relabel => {
            let other = Hash64::from_u64_word(0x4E1A_BE47_0000_0000 ^ nonce);
            let (their, their_prompt) = m.backend.job_for_anchor(other).expect("another anchor's job");
            assert_ne!(their_prompt, prompt, "another anchor names another prompt");
            let mut relabelled = palw_attempt_job_v1(their, true);
            relabelled.job_id = anchor;
            relabelled.execution_seed = anchor.as_byte_slice()[..32].try_into().unwrap();
            run(m.backend.as_ref(), &relabelled, &their_prompt)
        }
        ModelFault::OutputRoot => {
            let ids = match &honest_pin {
                PalwDecodeTokenPinV1::TiledV1(t) => t.generated_token_ids.clone(),
                _ => unreachable!("a model class pins tiled"),
            };
            let ground = kaspa_consensus_core::palw_v2::output_commitment_v2(
                &honest_binding.job_context.context_hash(),
                &ids,
                &kaspa_consensus_core::palw_v2::rendered_output_hash_v2(b"a rendering the class never makes"),
            );
            assert_ne!(ground, honest.output_root, "the ground root is not the run's");
            let contradiction = C::OutputMismatch { binding: honest_binding.clone(), pin: honest_pin.clone() };
            (
                AttemptRoots { output: ground, ..roots_of(&honest) },
                honest_binding.clone(),
                honest.material.clone(),
                honest_pin.clone(),
                Some(contradiction),
            )
        }
        ModelFault::ActivationLeg => recommitted(&|b| b.activation_leg_root = Hash64::from_u64_word(0xAC71_0476)),
        ModelFault::CheckpointProfile => recommitted(&|b| b.checkpoint_profile.checkpoint_interval += 1),
        ModelFault::ShortPrefill => {
            let short: Vec<usize> = prompt[..prompt.len() - 1].to_vec();
            let ids: Vec<u32> = short.iter().map(|t| *t as u32).collect();
            let mut job = job.clone();
            job.declared_prefill_tokens -= 1;
            job.prompt_token_ids_hash = kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(h.form(), &ids)
                .expect("the short prompt commits");
            run(m.backend.as_ref(), &job, &short)
        }
        ModelFault::LegacyContext => {
            let (legacy, legacy_prompt) = m.legacy.job_for_anchor(anchor).expect("the Legacy rule implies a job");
            let legacy = palw_attempt_job_v1(legacy, true);
            assert_ne!(legacy.context_hash(), job.context_hash(), "an instance's fields enter the Legacy context");
            run(m.legacy.as_ref(), &legacy, &legacy_prompt)
        }
        ModelFault::BendLogits | ModelFault::TokenNotSelected | ModelFault::TokenOutOfVocab => {
            f1c_commit(m, fault, &honest, &honest_binding)
        }
    };
    assert_eq!(binding.committed_execution_root, roots.execution, "the claim's root is its binding's");
    let (claim_id, envelope, _header) = h.fold_own_attempt(walk, header, anchor, pre_pow, &facts, roots);
    assert_eq!(walk.state.claim(&claim_id).unwrap().class_id, m.class_id(), "the claim is the model class's");
    ModelClaim { claim_id, anchor, envelope, binding, material, pin, contradiction }
}

/// **The F1c producers** over the honest run: its step tree, with rows or ids the class never
/// produced committed beside it — every root re-derived as the producer would (the tiled trace root,
/// the execution root, the manifest over the new trace root, the one rendered output root over the
/// committed ids) — and the proof a filer assembles: the event from the committed rows and the
/// class's own opening of the head leaf (12), or the tiled decode pin (11).
fn f1c_commit(
    m: &ModelClass,
    fault: ModelFault,
    honest: &PalwExecutionOutcomeV1,
    honest_binding: &PalwStepBindingV2,
) -> (AttemptRoots, PalwStepBindingV2, Vec<u8>, PalwDecodeTokenPinV1, Option<C>) {
    use kaspa_consensus_core::palw_offence_v1::PalwForgedOutputTiledProofV1 as P;
    use kaspa_consensus_core::palw_step::{canonical_step_leaf_index, palw_logits_head_coordinate_v1, palw_logits_head_v1};
    use kaspa_consensus_core::palw_step_refute::{
        base0_decode_token_select_v1, logits_event_disclosure_v1, tiled_decode_pin_v1, tiled_logits_trace_root_v1,
    };
    let retention = misaka_palw_base0::produce::base0_material_decode_any_v1(&honest.material).expect("the capture decodes");
    let (mut rows, mut ids) = (retention.logits_rows().to_vec(), retention.generated_token_ids().to_vec());
    let ctx = honest_binding.job_context.clone();
    let vocab = honest_binding.shape_profile.vocab_size;
    let top = base0_decode_token_select_v1(&rows[0]) as u32;
    let runner_up = (0..vocab).filter(|l| *l != top).max_by_key(|l| (rows[0][*l as usize], std::cmp::Reverse(*l))).unwrap();
    let bent_lane = match fault {
        ModelFault::BendLogits => {
            let lane =
                (1..vocab as usize - 1).find(|l| *l as u32 != top && rows[0][*l] + 1 < rows[0][top as usize]).expect("a lane below");
            rows[0][lane] += 1;
            assert_eq!(base0_decode_token_select_v1(&rows[0]) as u32, top, "the argmax does not move");
            Some(lane as u32)
        }
        ModelFault::TokenNotSelected => {
            ids[0] = runner_up;
            None
        }
        ModelFault::TokenOutOfVocab => {
            ids[0] = vocab + 3;
            None
        }
        _ => unreachable!("an F1c fault"),
    };
    let mut binding = honest_binding.clone();
    binding.full_logits_trace_root = tiled_logits_trace_root_v1(&ctx, &rows, &ids).expect("rows build a tree");
    binding.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(&binding);
    verify_binding_v1(&binding).expect("the re-committed binding verifies");
    let rows_root = tiled_logits_rows_root_v1(&ctx, &rows).expect("rows build a tree");
    let pin = PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: ids.clone() });
    let roots = AttemptRoots {
        trace: binding.full_logits_trace_root,
        output: kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(&ctx, &ids),
        execution: binding.committed_execution_root,
        manifest: attempt_trace_manifest_root_v1(binding.full_logits_trace_root, honest.trace_chunk_count),
        chunks: honest.trace_chunk_count,
    };
    let contradiction = match (fault, bent_lane) {
        (ModelFault::BendLogits, Some(lane)) => {
            let head = palw_logits_head_v1(&binding.shape_profile).expect("the class's head is provable");
            let tile = lane / head.tile_len;
            let logits_tile = (lane as usize / kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES) as u8;
            let event = logits_event_disclosure_v1(&binding, &rows, &ids, 0, logits_tile).expect("the tiled event opens");
            let coord = palw_logits_head_coordinate_v1(&head, &ctx, 0, tile).expect("a coordinate");
            let index = canonical_step_leaf_index(&binding.shape_profile, &ctx, &coord).expect("a leaf");
            let head_opening =
                m.backend.refutation_for_index(&honest.material, index).expect("the class opens its head").output_opening;
            C::LogitsNotStepOutput { event, row: 0, head_tile: tile, head_opening }
        }
        (ModelFault::TokenNotSelected, _) => C::ForgedOutputTiled {
            binding: binding.clone(),
            proof: P::NotSelected { pin: tiled_decode_pin_v1(&ctx, &rows, &ids, 0, top).expect("the pin opens") },
        },
        (ModelFault::TokenOutOfVocab, _) => C::ForgedOutputTiled {
            binding: binding.clone(),
            proof: P::OutOfVocab { position: 0, tokens: PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: ids.clone() } },
        },
        _ => unreachable!("an F1c fault"),
    };
    (roots, binding, honest.material.clone(), pin, Some(contradiction))
}

/// A borrower of `lender`'s trace and execution roots on another template, under a ground output
/// root — T18s's by-claim forfeiture.
fn open_ground_borrower(h: &H, walk: &mut Walk, m: &ModelClass, nonce: u64, lender: &ModelClaim) -> ModelClaim {
    use kaspa_consensus_core::hashing::header::pre_pow_hash_64;
    let template = h.ctx.build_block_template_keeping_time(nonce);
    let header: Header = template.block.header.clone();
    let bond = h.cards[EXECUTOR];
    let mut facts = h.ctx.consensus.palw_producer_facts_v2(h.floor(), Some(bond.0)).expect("testnet-12 answers for card 0");
    facts.class_id = m.class_id();
    facts.artifact_root = m.artifact_root;
    let pre_pow = pre_pow_hash_64(&header);
    let anchor = execution_anchor_v3(h.domain, pre_pow, m.class_id(), &bond.0, header.nonce);
    assert_ne!(anchor, lender.anchor);
    let lent = &lender.envelope.attempt;
    let ground = Hash64::from_u64_word(0x6A0D_0047 ^ nonce);
    let roots = AttemptRoots {
        trace: lent.trace_root,
        output: ground,
        execution: lent.execution_root,
        manifest: lent.trace_manifest_root,
        chunks: lent.trace_chunk_count,
    };
    let (claim_id, envelope, _header) = h.fold_own_attempt(walk, header, anchor, pre_pow, &facts, roots);
    ModelClaim {
        claim_id,
        anchor,
        envelope,
        binding: lender.binding.clone(),
        material: lender.material.clone(),
        pin: lender.pin.clone(),
        contradiction: Some(C::OutputMismatch { binding: lender.binding.clone(), pin: lender.pin.clone() }),
    }
}

fn families(h: &H) -> Vec<ModelClass> {
    vec![a16_fixture(h), qwen36_fixture(h)]
}

/// **T18e on the model classes**: J5b convicts the relabel through kind 4 before any licence; the
/// honest twin is refused `IdentityHolds` in the gate and the fold and the walk drops it; the seat's
/// own rule refuses the relabel and licenses the twin.
#[tokio::test]
async fn t47a_the_model_relabel_is_refuted_by_j5b() {
    let h = harness(true);
    for m in families(&h) {
        let mut walk = h.genesis_walk();
        seed(&h, &mut walk, &m);
        let honest = open_model_claim(&h, &mut walk, &m, ModelFault::Honest, bucket(1));
        let relabel = open_model_claim(&h, &mut walk, &m, ModelFault::Relabel, bucket(2));
        let target = palw_offence_target_v1(&walk.state, &relabel.claim_id).unwrap();
        let binding = &relabel.binding;
        assert_eq!(binding.job_context.job_id, relabel.anchor, "{}: J1 holds", m.label);
        assert_eq!(&binding.job_context.execution_seed[..], &relabel.anchor.as_byte_slice()[..32], "{}: J3 holds", m.label);
        assert_eq!(
            palw_binding_identity_fault_v1(&target, binding, h.rules(), true),
            Ok(Some(PalwIdentityFaultV1::PromptNotTheAnchors)),
            "{}: J5b names the relabel",
            m.label
        );
        assert_eq!(
            m.backend.verify_material(&relabel.material, relabel.roots()),
            PalwMaterialVerdictV1::Mismatch,
            "{}: a seat refuses it",
            m.label
        );
        assert_eq!(
            m.backend.verify_material(&honest.material, honest.roots()),
            PalwMaterialVerdictV1::Matches,
            "{}: and licenses the twin",
            m.label
        );

        // The honest twin: refused by name at both doors, dropped by the walk.
        let twin = h.refuted(honest.claim_id, C::IdentityMismatch { binding: honest.binding.clone() });
        h.refused(&walk, &twin, &E::IdentityHolds.to_string());
        assert_eq!(h.fold_refusal(&walk, &twin), E::IdentityHolds.to_string());
        let twin_output = h.refuted(honest.claim_id, C::OutputMismatch { binding: honest.binding.clone(), pin: honest.pin.clone() });
        h.refused(&walk, &twin_output, &E::OutputHolds.to_string());

        let finding = h.judge_refuted(&walk.state, relabel.claim_id, relabel.contradiction()).expect("J5b convicts");
        assert_eq!(
            (finding.forfeit, finding.identity_fault),
            (PalwForfeitScopeV1::ByClaim, Some(PalwIdentityFaultV1::PromptNotTheAnchors)),
            "{}",
            m.label
        );
        let (before, _) = h.carry(&mut walk, vec![h.refuted(relabel.claim_id, relabel.contradiction())]);
        assert_refuted_before_final(&h, &before, &walk.state, relabel.claim_id, walk.daa, Hash64::default());
        assert!(
            matches!(walk.state.claim(&honest.claim_id).unwrap().phase, PalwClaimPhaseV2::Provisional),
            "{}: the twin is untouched",
            m.label
        );
        h.reloads(&walk.state);
    }
}

/// **T18s: `OutputMismatch` on model attempts, forfeiture by claim.** The ground output root
/// convicts through the tiled pin; a borrower of an honest claim's execution root under a ground
/// output root is voided alone — the lender's claim, reservation and root stand.
#[tokio::test]
async fn t47b_the_model_output_rule_convicts_by_claim() {
    let h = harness(true);
    for m in families(&h) {
        let mut walk = h.genesis_walk();
        seed(&h, &mut walk, &m);
        let ground = open_model_claim(&h, &mut walk, &m, ModelFault::OutputRoot, bucket(1));
        let finding = h.judge_refuted(&walk.state, ground.claim_id, ground.contradiction()).expect("the ground root convicts");
        assert_eq!((finding.forfeit, finding.identity_fault), (PalwForfeitScopeV1::ByClaim, None), "{}", m.label);
        let (before, _) = h.carry(&mut walk, vec![h.refuted(ground.claim_id, ground.contradiction())]);
        assert_refuted_before_final(&h, &before, &walk.state, ground.claim_id, walk.daa, Hash64::default());

        let lender = open_model_claim(&h, &mut walk, &m, ModelFault::Honest, bucket(2));
        let borrower = open_ground_borrower(&h, &mut walk, &m, bucket(3), &lender);
        let lender_row = walk.state.claim(&lender.claim_id).unwrap().clone();
        let (before, _) = h.carry(&mut walk, vec![h.refuted(borrower.claim_id, borrower.contradiction())]);
        assert_refuted_before_final(&h, &before, &walk.state, borrower.claim_id, walk.daa, Hash64::default());
        assert_eq!(walk.state.claim(&lender.claim_id), Some(&lender_row), "{}: the lender's claim, as it was", m.label);
        assert!(
            !walk.state.palw_execution_root_is_forfeited_v1(&lender.envelope.attempt.execution_root),
            "{}: the root is the lender's too — not forfeit",
            m.label
        );
        h.reloads(&walk.state);
    }
}

/// **T18t: J6, J7 and J5a on the model classes** — each producer-side move convicted by kind 4,
/// with the fault the identity rule names.
#[tokio::test]
async fn t47c_the_moved_legs_and_the_non_formula_contexts_are_refuted() {
    let h = harness(true);
    for m in families(&h) {
        let mut walk = h.genesis_walk();
        seed(&h, &mut walk, &m);
        for (n, (fault, named)) in [
            (ModelFault::ActivationLeg, PalwIdentityFaultV1::ActivationLegNotCanonical),
            (ModelFault::CheckpointProfile, PalwIdentityFaultV1::CheckpointProfileNotCanonical),
            (ModelFault::ShortPrefill, PalwIdentityFaultV1::ContextNotCanonical),
            (ModelFault::LegacyContext, PalwIdentityFaultV1::ContextNotCanonical),
        ]
        .into_iter()
        .enumerate()
        {
            let claim = open_model_claim(&h, &mut walk, &m, fault, bucket(1 + n as u64));
            let target = palw_offence_target_v1(&walk.state, &claim.claim_id).unwrap();
            assert_eq!(
                palw_binding_identity_fault_v1(&target, &claim.binding, h.rules(), true),
                Ok(Some(named)),
                "{} {fault:?}: the identity rule names it",
                m.label
            );
            assert_eq!(
                palw_binding_identity_fault_v1(&target, &claim.binding, h.rules(), false),
                Ok(Some(named)),
                "{} {fault:?}: and so do the DA answers (it is not J5b)",
                m.label
            );
            let (before, _) = h.carry(&mut walk, vec![h.refuted(claim.claim_id, claim.contradiction())]);
            assert_refuted_before_final(&h, &before, &walk.state, claim.claim_id, walk.daa, Hash64::default());
        }
        h.reloads(&walk.state);
    }
}

/// **T18q/T18r on the chain (F1c): kind 4 convicts a model class's logits fault and forged tokens.**
/// On both families: `LogitsNotStepOutput` (12) on bent logits over the honest step tree, and
/// `ForgedOutputTiled` (11) on a token not its row's selection (`NotSelected`) and one past the
/// vocabulary (`OutOfVocab`) — each through the gate, the walk and the fold, the claim voided
/// `CourtFraud`, its executor charged, and — the execution being proven false — its root recorded and
/// forfeit.
#[tokio::test]
async fn t47d_a_models_logits_and_tokens_are_refuted_by_root() {
    let h = harness(true);
    for m in families(&h) {
        let mut walk = h.genesis_walk();
        seed(&h, &mut walk, &m);
        for (n, fault) in [ModelFault::BendLogits, ModelFault::TokenNotSelected, ModelFault::TokenOutOfVocab].into_iter().enumerate() {
            let claim = open_model_claim(&h, &mut walk, &m, fault, bucket(1 + n as u64));
            let finding = h.judge_refuted(&walk.state, claim.claim_id, claim.contradiction()).expect("the proof convicts");
            assert_eq!(finding.forfeit, PalwForfeitScopeV1::ByRoot, "{} {fault:?}", m.label);
            let (before, _) = h.carry(&mut walk, vec![h.refuted(claim.claim_id, claim.contradiction())]);
            let root = claim.envelope.attempt.execution_root;
            assert_refuted_before_final(&h, &before, &walk.state, claim.claim_id, walk.daa, root);
            assert!(walk.state.palw_execution_root_is_forfeited_v1(&root), "{} {fault:?}: a proven-false root is forfeit", m.label);
        }
        h.reloads(&walk.state);
    }
}

// ---------------------------------------------------------------------------------------------
// F1-M 13 on the 2M-sized row: PromptNotAnchored, AnyValid, the heavy budget
// ---------------------------------------------------------------------------------------------

/// **The 2M-sized class** — the floor's graph at `n_ctx` 2,097,152, registered as a model class of its
/// own: the formula's canonical job is `(262,143, 2)`, past J5b's inline bound, so a relabelled prompt
/// is `PromptNotAnchored`'s (13) to prove. No producer of this width runs in a test, and none needs
/// to: 13 is the route for a prompt root the producer committed WITHOUT a run behind it (R-M1).
fn wide_profile() -> PalwShapeProfileV3 {
    use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    let mut profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor's graph");
    profile.n_ctx = 2_097_152;
    profile
}

const WIDE_ARTIFACT_ROOT: u64 = 0x2A11_0000;

struct WideClaim {
    claim_id: Hash64,
    binding: PalwStepBindingV2,
    /// The anchor whose prompt the binding commits (the claim's own for the honest twin).
    prompt_anchor: Hash64,
}

/// **A 2M-sized claim as its producer commits it**: `CoreV1`'s canonical context for this block's
/// anchor (J1, J3 and J5a hold) under the prompt root of `prompt_anchor` — another anchor's for the
/// relabel, the claim's own (`None`) for the honest twin — the legs committed as `verify_binding`
/// recomputes them, folded as the template block's own attempt.
fn open_wide_claim(h: &H, walk: &mut Walk, profile: &PalwShapeProfileV3, prompt_anchor: Option<Hash64>, nonce: u64) -> WideClaim {
    use kaspa_consensus_core::hashing::header::pre_pow_hash_64;
    use kaspa_consensus_core::palw_attempt_rules_v1::{
        palw_attempt_canonical_v1, palw_attempt_context_v1, palw_attempt_prompt_root_v1, palw_canonical_checkpoint_profile_v1,
        palw_int_activation_leg_root_v1,
    };
    let header: Header = h.ctx.build_block_template_keeping_time(nonce).block.header.clone();
    let bond = h.cards[EXECUTOR];
    let mut facts = h.ctx.consensus.palw_producer_facts_v2(h.floor(), Some(bond.0)).expect("testnet-12 answers for card 0");
    facts.class_id = profile.shape_profile_id();
    facts.artifact_root = Hash64::from_u64_word(WIDE_ARTIFACT_ROOT);
    let pre_pow = pre_pow_hash_64(&header);
    let anchor = execution_anchor_v3(h.domain, pre_pow, facts.class_id, &bond.0, header.nonce);
    let prompt_anchor = prompt_anchor.unwrap_or(anchor);
    let canonical = palw_attempt_canonical_v1(profile, false).expect("the formula");
    assert_eq!(canonical, (262_143, 2));
    let root = palw_attempt_prompt_root_v1(profile, &prompt_anchor, canonical.0, h.form()).expect("the prompt commits");
    let job_context = palw_attempt_context_v1(profile, &anchor, canonical, root);
    let mut binding = PalwStepBindingV2 {
        version: kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
        state_chunk_map_id: profile.state_chunk_map_id,
        checkpoint_profile: palw_canonical_checkpoint_profile_v1(profile),
        activation_leg_root: palw_int_activation_leg_root_v1(&job_context),
        job_context,
        shape_profile: profile.clone(),
        full_logits_trace_root: Hash64::from_u64_word(0x7ACE ^ nonce),
        step_leaf_count: 64,
        step_merkle_root: Hash64::from_u64_word(0x57E9 ^ nonce),
        checkpoint_count: 0,
        checkpoint_merkle_root: Hash64::default(),
        committed_execution_root: Hash64::default(),
    };
    binding.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(&binding);
    verify_binding_v1(&binding).expect("the binding verifies");
    let roots = AttemptRoots {
        trace: binding.full_logits_trace_root,
        output: Hash64::from_u64_word(0x0A7 ^ nonce),
        execution: binding.committed_execution_root,
        manifest: attempt_trace_manifest_root_v1(binding.full_logits_trace_root, 1),
        chunks: 1,
    };
    let (claim_id, _envelope, _header) = h.fold_own_attempt(walk, header, anchor, pre_pow, &facts, roots);
    assert_eq!(walk.state.claim(&claim_id).unwrap().job_identity, anchor, "the claim records its anchor");
    WideClaim { claim_id, binding, prompt_anchor }
}

impl WideClaim {
    fn tile(&self, position: u32) -> C {
        let profile = &self.binding.shape_profile;
        let ids = kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_prompt_ids_v1(
            &self.prompt_anchor,
            u64::from(profile.vocab_size),
            self.binding.job_context.declared_prefill_tokens,
        );
        let opening = kaspa_consensus_core::palw_prompt_ids_v1::prompt_ids_opening_v1(&ids, position).expect("an opening");
        C::PromptNotAnchored {
            binding: self.binding.clone(),
            proof: kaspa_consensus_core::palw_offence_v1::PalwPromptProofV1::Tile(opening),
        }
    }

    fn whole(&self) -> C {
        C::PromptNotAnchored { binding: self.binding.clone(), proof: kaspa_consensus_core::palw_offence_v1::PalwPromptProofV1::Whole }
    }
}

/// **T18e on the 2M-sized row (13)**: a relabelled prompt root — another anchor's, committed under
/// this anchor's canonical context — is invisible to 9 (J5b is not inline at 262,143 ids:
/// `IdentityHolds`), and `PromptNotAnchored` convicts it by a `Tile` and by `Whole`, through kind 4,
/// by claim; the honest twin (its own anchor's prompt) is `PromptHolds` both ways, refused at the gate
/// and in the fold.
#[tokio::test]
async fn t47e_the_2m_relabel_is_refuted_by_prompt_not_anchored() {
    let h = harness(true);
    let profile = wide_profile();
    for proof in ["tile", "whole"] {
        let mut walk = h.genesis_walk();
        seed_profile(&h, &mut walk, "2M-sized", &profile, Hash64::from_u64_word(WIDE_ARTIFACT_ROOT));
        let honest = open_wide_claim(&h, &mut walk, &profile, None, bucket(1));
        let relabel = open_wide_claim(&h, &mut walk, &profile, Some(Hash64::from_u64_word(0x13_0DE1)), bucket(2));
        h.refused(
            &walk,
            &h.refuted(relabel.claim_id, C::IdentityMismatch { binding: relabel.binding.clone() }),
            &E::IdentityHolds.to_string(),
        );
        for twin in [honest.tile(0), honest.tile(262_142), honest.whole()] {
            let object = h.refuted(honest.claim_id, twin);
            h.refused(&walk, &object, &E::PromptHolds.to_string());
            assert_eq!(h.fold_refusal(&walk, &object), E::PromptHolds.to_string());
        }
        let contradiction = if proof == "tile" { relabel.tile(131_072) } else { relabel.whole() };
        let finding = h.judge_refuted(&walk.state, relabel.claim_id, contradiction.clone()).expect("13 convicts");
        assert_eq!((finding.forfeit, finding.identity_fault), (PalwForfeitScopeV1::ByClaim, None), "{proof}");
        let (before, _) = h.carry(&mut walk, vec![h.refuted(relabel.claim_id, contradiction)]);
        assert_refuted_before_final(&h, &before, &walk.state, relabel.claim_id, walk.daa, Hash64::default());
        assert!(matches!(walk.state.claim(&honest.claim_id).unwrap().phase, PalwClaimPhaseV2::Provisional), "the twin is untouched");
        h.reloads(&walk.state);
    }
}

/// **13 is `AnyValid`** (the user's decision of 2026-09-24): on the licensed 2M relabel every `Valid`
/// signer — the full seat and each partial-mask seat, whose SEAT-S4 opening checked the prompt it
/// resumed from — is convicted by kind 3; a partial seat presenting an `Incapable` receipt is refused.
#[tokio::test]
async fn t47f_a_prompt_fault_convicts_every_valid_signer() {
    let h = harness(true);
    let profile = wide_profile();
    let mut walk = h.genesis_walk();
    seed_profile(&h, &mut walk, "2M-sized", &profile, Hash64::from_u64_word(WIDE_ARTIFACT_ROOT));
    let relabel = open_wide_claim(&h, &mut walk, &profile, Some(Hash64::from_u64_word(0x13_0DE2)), bucket(1));
    h.bind(&mut walk, relabel.claim_id);
    let licence = h.license_v2(&mut walk, relabel.claim_id);
    let thirteen = relabel.tile(7);
    let partial = licence.partials()[0];
    let receipt = licence.receipt(partial);
    let incapable =
        h.v3_verdict(partial, relabel.claim_id, PalwReceiptVerdictV2::Incapable, receipt.receipt.signed_daa, receipt.segments);
    h.refused(&walk, &h.v2(partial, relabel.claim_id, incapable, thirteen.clone()), &E::PanelFalseValidNotValidVerdict.to_string());
    let every: Vec<usize> = std::iter::once(licence.full_card()).chain(licence.partials()).collect();
    let objects: Vec<Obj> =
        every.iter().map(|&card| h.v2(card, relabel.claim_id, licence.segmented(card), thirteen.clone())).collect();
    let (licensed, _) = h.carry(&mut walk, objects);
    assert_seats_convicted_by_claim(&h, &licensed, &walk.state, relabel.claim_id, &every, walk.daa);
    h.reloads(&walk.state);
}

/// **T18u: the heavy budget** (addendum §4-bis.3): a block holds one whole-prompt recomputation at the
/// 2M width. Two `Whole` 13s in one block: the acceptance walk drops the second before any node
/// computes it, the block standing on the rest (a `Tile` in the same block, cheap, is taken), and the
/// fold — the second lock — refuses a block that carries both. The charge is the object's, whatever
/// its kind: a kind-3 `Whole` after a kind-4 `Whole` is dropped the same way.
#[tokio::test]
async fn t18u_a_block_recomputes_one_whole_2m_prompt() {
    let h = harness(true);
    let profile = wide_profile();
    let mut walk = h.genesis_walk();
    seed_profile(&h, &mut walk, "2M-sized", &profile, Hash64::from_u64_word(WIDE_ARTIFACT_ROOT));
    let c1 = open_wide_claim(&h, &mut walk, &profile, Some(Hash64::from_u64_word(0x18_0001)), bucket(1));
    let c2 = open_wide_claim(&h, &mut walk, &profile, Some(Hash64::from_u64_word(0x18_0002)), bucket(2));
    let (w1, w2, t2) = (h.refuted(c1.claim_id, c1.whole()), h.refuted(c2.claim_id, c2.whole()), h.refuted(c2.claim_id, c2.tile(3)));
    for object in [&w1, &w2, &t2] {
        h.validate(&walk.state, &walk.next(), object).expect("each alone clears the gate");
    }
    let point = walk.next();
    assert_eq!(
        h.accepted(&walk.state, &point, &[w1.clone(), w2.clone(), t2.clone()]),
        vec![w1.clone(), t2.clone()],
        "the second Whole is dropped"
    );
    match h.fold(&walk.state, &point, &[w1.clone(), w2.clone()]) {
        Err(PalwStateV2Error::ObjectiveOffenceRefused(_, why)) => assert_eq!(why, E::HeavyBudgetExhausted.to_string()),
        other => panic!("the fold's second lock refuses a block with two: {other:?}"),
    }
    let (before, _) = h.carry(&mut walk, vec![w1, t2]);
    // Both convict in the one block; the executor pays each claim's reservation, escrow and rights,
    // and past `palw_rcore_plus` S2's `min(10% · C₀, 3 G)` (S-4) — C₀ its collateral before THAT
    // conviction's first debit, so the second claim is priced after the first's debit.
    let mut c0 = before.bond(&h.cards[EXECUTOR]).unwrap().collateral;
    let mut owed = 0u128;
    for id in [c1.claim_id, c2.claim_id] {
        assert!(
            matches!(walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
            "claim {id} is voided CourtFraud"
        );
        let row = walk.state.consumed_offence(&palw_executor_refuted_offence_id_v1(&h.cards[EXECUTOR].0, &id)).expect("a kind-4 row");
        assert_eq!((row.claim_id, row.execution_root), (id, Hash64::default()), "by claim");
        let claim = before.claim(&id).unwrap();
        let forfeit = claim.reserved + h.sp().claim_escrow_reservation_v1(claim.accepted_daa, claim.escrowed_reward) + claim.rights_reserved;
        let nominal = if h.sp().rcore_plus_active_at(point.daa_score) {
            forfeit + kaspa_consensus_core::palw_state_v2::palw_rcore_s1s2_action_v1(c0, g_of(&walk.state, id))
        } else {
            forfeit
        };
        let debit = nominal.min(u128::from(c0));
        owed += debit;
        c0 -= u64::try_from(debit).unwrap();
    }
    let paid = before.bond(&h.cards[EXECUTOR]).unwrap().collateral - walk.state.bond(&h.cards[EXECUTOR]).unwrap().collateral;
    assert_eq!(u128::from(paid), owed, "the executor pays both claims");

    // Kind 3 is charged the same.
    let c3 = open_wide_claim(&h, &mut walk, &profile, Some(Hash64::from_u64_word(0x18_0003)), bucket(3));
    let c4 = open_wide_claim(&h, &mut walk, &profile, Some(Hash64::from_u64_word(0x18_0004)), bucket(4));
    h.bind(&mut walk, c4.claim_id);
    let licence = h.license_v2(&mut walk, c4.claim_id);
    let full = licence.full_card();
    let (w3, w4) = (h.refuted(c3.claim_id, c3.whole()), h.v2(full, c4.claim_id, licence.segmented(full), c4.whole()));
    let point = walk.next();
    assert_eq!(h.accepted(&walk.state, &point, &[w3.clone(), w4]), vec![w3], "a kind-3 Whole after a kind-4 one is dropped");
}

/// **T18u′ (the Phase 3 review's heavy-budget finding): holding the slot costs what it consumes.**
/// On 2M-sized claims — `X` a relabel an honest filer convicts, `Y` an honest claim:
///
/// * **Same claim.** An attacker's Whole on `X` that reaches the recompute and then fails (a kind-3
///   `Valid` receipt of a bond the panel does not seat) precedes the honest kind-4 Whole on `X`: the
///   claim was charged once, the root is remembered, and the honest conviction lands in the same
///   block.
/// * **Junk costs nothing.** A Whole naming a claim the chain does not have, or accusing a bond that
///   is not the claim's executor, fails before the recompute and is never charged: the honest Whole
///   after it lands.
/// * **Another claim pays.** An attacker's Whole on `Y` (its prompt is honest: `PromptHolds`) is
///   priced at its prompt's carriage (`palw_object_rent_ceiling_v2`): underpaid, the walk drops it and
///   the honest Whole on `X` lands; paid in full, it holds the slot — and the attacker paid the relay
///   rate for 4 bytes a prompt id, 1,048,572 mass at 2M, burned.
#[tokio::test]
async fn t18u_holding_the_heavy_slot_costs_what_it_consumes() {
    use kaspa_consensus_core::palw_state_v2::{palw_object_rent_ceiling_v1, palw_object_rent_ceiling_v2, palw_relay_fee_for_mass_v1};
    let h = harness(true);
    let profile = wide_profile();
    let mut walk = h.genesis_walk();
    seed_profile(&h, &mut walk, "2M-sized", &profile, Hash64::from_u64_word(WIDE_ARTIFACT_ROOT));
    let x = open_wide_claim(&h, &mut walk, &profile, Some(Hash64::from_u64_word(0x18_0A11)), bucket(1));
    let y = open_wide_claim(&h, &mut walk, &profile, None, bucket(2));
    let honest = h.refuted(x.claim_id, x.whole());

    // Same claim: a stranger's kind-3 Whole on X reaches the recompute (it is well-formed and names
    // X), then fails; the honest kind-4 on X is not charged again.
    let mask = PalwSegmentMaskV2::full(4);
    let stranger =
        h.v2(BYSTANDER, x.claim_id, h.v3_verdict(BYSTANDER, x.claim_id, PalwReceiptVerdictV2::Valid, walk.daa, mask), x.whole());
    assert!(h.validate(&walk.state, &walk.next(), &stranger).is_err(), "the stranger's Whole fails");
    let point = walk.next();
    assert_eq!(h.accepted(&walk.state, &point, &[stranger.clone(), honest.clone()]), vec![honest.clone()], "the honest Whole lands");

    // Junk costs nothing: a claim the chain does not have, an accused that is not the executor.
    let mut unknown_payload = h.refuted_payload(x.claim_id, x.whole());
    unknown_payload.claim_id = Hash64::from_u64_word(0xDEAD_C1A1);
    let unknown = offence(PalwOffenceKindV1::ExecutorRefuted, h.cards[EXECUTOR], borsh::to_vec(&unknown_payload).unwrap());
    let wrong_accused = h.refuted_by(BYSTANDER, y.claim_id, y.whole());
    let point = walk.next();
    assert_eq!(
        h.accepted(&walk.state, &point, &[unknown, wrong_accused, honest.clone()]),
        vec![honest.clone()],
        "junk that fails before the recompute is never charged"
    );

    // Another claim pays for the slot it takes.
    let attacker = h.refuted(y.claim_id, y.whole());
    h.refused(&walk, &attacker, &E::PromptHolds.to_string());
    let rent = palw_object_rent_ceiling_v2(&attacker, true);
    assert_eq!(
        rent,
        palw_relay_fee_for_mass_v1(262_143 * kaspa_consensus_core::palw_attempt_rules_v1::PALW_WHOLE_PROMPT_MASS_PER_ID_V1)
    );
    assert_eq!(rent, palw_object_rent_ceiling_v2(&honest, true), "the honest filer pays the same carriage");
    let priced = |objects: Vec<(Obj, u64)>| {
        let point = walk.next();
        h.vp().palw_v2_accepted_priced_objects_for_tests(&walk.state, h.sp(), &point, objects, point.block).0
    };
    assert_eq!(priced(vec![(attacker.clone(), rent - 1), (honest.clone(), rent)]), vec![honest.clone()], "underpaid: dropped");
    assert_eq!(priced(vec![(attacker.clone(), rent), (honest.clone(), rent)]), Vec::<Obj>::new(), "paid in full, it holds the slot");
    assert_eq!(priced(vec![(honest.clone(), rent - 1)]), Vec::<Obj>::new(), "and the honest filer pays it too");

    // Fence-off parity: below `palw_offence_attribution` the rent is the v1 rent (nothing).
    assert_eq!(palw_object_rent_ceiling_v2(&attacker, false), palw_object_rent_ceiling_v1(&attacker));
    assert_eq!(palw_object_rent_ceiling_v1(&attacker), 0);
    let tile = h.refuted(x.claim_id, x.tile(3));
    assert_eq!(palw_object_rent_ceiling_v2(&tile, true), 0, "a Tile recomputes nothing");

    // And the honest conviction folds.
    let (before, _) = h.carry(&mut walk, vec![honest]);
    assert_refuted_before_final(&h, &before, &walk.state, x.claim_id, walk.daa, Hash64::default());

    // The walk and the fold charge alike: two Wholes on ONE licensed claim — the executor refuted by
    // kind 4 and the full seat by kind 3 — both ride one block and both fold (the claim is charged
    // once), where two Wholes on two claims do not (`t18u_a_block_recomputes_one_whole_2m_prompt`).
    let z = open_wide_claim(&h, &mut walk, &profile, Some(Hash64::from_u64_word(0x18_0A12)), bucket(3));
    h.bind(&mut walk, z.claim_id);
    let licence = h.license_v2(&mut walk, z.claim_id);
    let full = licence.full_card();
    let (licensed, _) =
        h.carry(&mut walk, vec![h.v2(full, z.claim_id, licence.segmented(full), z.whole()), h.refuted(z.claim_id, z.whole())]);
    assert!(walk.state.slashable_lock(h.cards[full], z.claim_id).is_none(), "the full seat is convicted");
    assert!(
        walk.state.consumed_offence(&palw_executor_refuted_offence_id_v1(&h.cards[EXECUTOR].0, &z.claim_id)).is_some(),
        "and the executor refuted, in the same block"
    );
    assert!(licensed.bond(&h.cards[full]).unwrap().collateral > walk.state.bond(&h.cards[full]).unwrap().collateral);
}
