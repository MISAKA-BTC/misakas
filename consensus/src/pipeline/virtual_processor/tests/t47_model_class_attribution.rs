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
    use kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1;
    use kaspa_consensus_core::palw_state_v2::PalwClassStateV2;
    assert_eq!(
        palw_attempt_rules_of_params_v1(&h.config.params),
        PalwAttemptRulesV1::CoreV1,
        "testnet-12's producers run CoreV1 from genesis"
    );
    let (floor, class_id) = (h.floor(), m.class_id());
    let fused = kaspa_consensus_core::palw_class_admission_v2::palw_profile_has_fused_attention_v1(&m.profile);
    walk.state = h.rebuilt(&walk.state, |c| {
        assert!(!c.classes.contains_key(&class_id), "{}: the fixture is not a genesis class", m.label);
        let record = PalwClassStateV2 { artifact_root: m.artifact_root, fused_attention: fused, ..c.classes[&floor].clone() };
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
        let (_, row) = c.model_lifecycles.iter().find(|(id, _)| **id != floor).expect("testnet-12 registers model rows");
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
    };
    assert_eq!(binding.committed_execution_root, roots.execution, "the claim's root is its binding's");
    let (claim_id, envelope, _header) = h.fold_own_attempt(walk, header, anchor, pre_pow, &facts, roots);
    assert_eq!(walk.state.claim(&claim_id).unwrap().class_id, m.class_id(), "the claim is the model class's");
    ModelClaim { claim_id, anchor, envelope, binding, material, pin, contradiction }
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
