//! **P0a exit test (e): the GDN key-head fix on the processor** (`docs/design/palw/tir/phase-f-
//! integration.md` §3.3, on `tir/phase-f`). A child of T47, so the harness, the three doors, the
//! seeding and the model claims are T46's and T47's rather than a second copy of them.
//!
//! The class is graph-v8 — graph-v7's tables at profile version 3 on the held composition v5 — over
//! the 16/32 fixture (`qwen36_dev_fixture_heads`: 16 key and 32 value heads of 4 lanes over 64) at
//! `n_ctx` 256, whose attempt (the formula's 31 prompt ids under the prefill draw) passes the
//! checkpoint that carries the recurrence (16 positions, one history tile). Graph-v7 over the same
//! weights cannot capture that checkpoint at all: its recurrence map gathers over one head count.
//!
//! * **(e1) the registration, across `palw_gdn_key_heads`**, on testnet-12's twin with
//!   `palw_offence_attribution` unset — the regime in which a held hybrid can register at all.
//!   Below the fence the registration is refused by name (`GdnKeyHeadsNeedsItsFence`) at the gate,
//!   the walk drops it and its block folds without it; past it the gate, the walk and the fold admit
//!   it. The hygiene half: past the fence graph-v7 at 16/32 is `GdnMapAssumesEqualHeads`; below it
//!   the gate admits it, as it always has.
//! * **(e2) testnet-12 as shipped**: `palw_offence_attribution` is armed at 0, and past it C5 refuses
//!   every held class with a recurrent layer (`HeldClassUnanswerable`). A graph-v8 registration is
//!   therefore refused below the fence by the fence and past it by C5: P0a alone does not make a
//!   Qwen3.6 held class registrable on testnet-12. Pinned here rather than papered over.
//! * **(e3) produced, disputed and closed**, on testnet-12 with the fence armed and the class seeded
//!   through the carriage (T47's route for the held classes testnet-12 cannot register): the honest
//!   claim is produced and folded across the recurrence's checkpoint; a one-lane fault at value
//!   head 20's GdnStep tile (key head 4) is convicted by kind 4 before `Final` through the gate, the
//!   walk and the fold; the honest claim's refutation at the same coordinate is refused and dropped.
//!   The disputed tile sits at prompt position 3: kind 4 carries its evidence in ONE carrier
//!   (100,000 bytes), and the held composition's GdnStep arm is the genesis replay (five opened rows
//!   per position from the prompt's start), so a tile deep in the job is the court's to try, not
//!   one carrier's.

use super::*;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_class_admission_v2::{PalwClassAdmissionError, PalwHeldUnanswerableV1};

/// The fixture's context width: the held formula's job is `(31, 2)`, and the attempt (the prefill
/// draw) runs its 31 prompt positions — past the recurrence's spacing of 16.
const P0A_N_CTX: u32 = 256;
/// Where e1 and e2 arm the fence: a few blocks past testnet-12's genesis point.
const P0A_FENCE: u64 = 6;
/// The card that registers in e1/e2 (no claim of this suite is its).
const REGISTRANT: usize = 7;

/// **testnet-12 with harness cards, `palw_gdn_key_heads` at `fence`**, and with `attribution =
/// false` T46's attribution-off twin (R-core+ off with it, as `harness(false)` takes it). The bundle
/// is the rebuilt config's own, so the fold this suite drives and the processor read one ruleset.
fn p0a_harness(attribution: bool, fence: Option<u64>) -> H {
    use misaka_palw_base0::classes::resolve_class_v1;
    kaspa_core::log::try_init_logger("warn");
    let (config, _bundle, _premine, floats) = super::super::super::t12_round_lane_e2e::t12_with_harness_cards();
    assert!(config.params.palw_offence_attribution.is_some_and(|f| f.is_active(0)), "testnet-12 arms the fence from genesis");
    assert_eq!(config.params.palw_gdn_key_heads, None, "testnet-12 ships the P0a fence dormant (its height is the user's)");
    let mut params = config.params.clone();
    if !attribution {
        params.palw_offence_attribution = None;
        params.palw_rcore_plus = None;
        params.palw_rcore_conservative_classes = &[];
        params.sync_palw_rcore_plus();
    }
    params.palw_gdn_key_heads = fence.map(ForkActivation::new);
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else {
        unreachable!("testnet-12 is ConsensusV2")
    };
    let bundle = bundle.clone();
    config.params.validate_palw_v2().expect("the fixture is a runnable ruleset");
    let ctx = TestContext::new(TestConsensus::new(&config));
    let (_, genesis) =
        ctx.consensus.virtual_processor().palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the genesis tip loads");
    let cards: Vec<PalwBondKeyV2> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            Obj::BondRegistered { bond, .. } => Some(*bond),
            _ => None,
        })
        .collect();
    assert_eq!(cards.len(), 8, "testnet-12 registers eight cards");
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let artifact_root = genesis.class(&bundle.base_class_id).expect("the floor is registered").artifact_root;
    let backend = Base0Backend::new(
        resolve_class_v1(&bundle.court, bundle.base_class_id, artifact_root, &[])
            .expect("the floor resolves from its registered root"),
    )
    .with_step_ladder_cap(bundle.court.max_step_leaf_count())
    .with_prompt_ids_form(config.params.palw_prompt_ids_form_v1());
    H { ctx, config, bundle, domain, cards, genesis, backend, artifact_root, floats }
}

/// The 16/32 fixture's class at [`P0A_N_CTX`]: graph-v8 (`v8`) or graph-v7 over the same weights —
/// each backend built as the SDK builds it on testnet-12 (T47's `qwen36_fixture`).
fn p0a_class(h: &H, v8: bool) -> ModelClass {
    use kaspa_consensus_core::palw_qwen36_profile::{
        PalwQwen36GeometryV1, qwen36_held_canonical_v1, qwen36_profile_v7, qwen36_profile_v8,
    };
    use misaka_palw_base0::qwen36_backend::Qwen36Backend;
    let geometry = PalwQwen36GeometryV1 {
        layer_count: 4,
        full_attention_interval: 4,
        hidden_dim: 64,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 16,
        rope_dims: 4,
        rope_freq_base_bits: 0x4B18_9680,
        gdn_k_heads: 16,
        gdn_v_heads: 32,
        gdn_head_dim: 4,
        gdn_conv_kernel: 4,
        n_experts: 8,
        experts_per_token: 4,
        moe_dim: 16,
        shared_dim: 16,
        attn_output_gate: 1,
        vocab_size: 64,
        n_ctx: P0A_N_CTX,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 512,
    };
    let artifact = Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture_heads(4, 8, 16, 32, 4, 64));
    let profile = if v8 { qwen36_profile_v8(geometry) } else { qwen36_profile_v7(geometry) }.expect("the held projection");
    let canonical = qwen36_held_canonical_v1(profile.n_ctx);
    assert_eq!(canonical, (31, 2), "the formula at 256");
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
    let label = if v8 { "Qwen3.6 16/32 graph-v8" } else { "Qwen3.6 16/32 graph-v7" };
    ModelClass { label, profile, artifact_root, backend: Box::new(backend), legacy }
}

/// **A signed `ClassRegistered` of `m` by card `card`**, built as a registrant builds it: the held
/// formula's canonical job, the derived pwu rule, the base class's pricing and target, and the share
/// the gate itself derives at `daa` (the minimum grantable where a certified family covers the
/// class and admission independence is dormant, 0 otherwise).
fn p0a_registration(h: &H, state: &PalwChainStateV2, daa: u64, m: &ModelClass, card: usize) -> Obj {
    use kaspa_consensus_core::palw_state_v2::{
        PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT, PalwCertifiedLaneV1, PalwClassAdmissionCarriageV2, PalwPwuRuleV2,
        palw_class_registration_message_v2,
    };
    let base = h.floor();
    let target = state.class_target(&base).expect("the base target").target;
    let slash = state.class(&base).expect("the base class").slash_value_per_pwu;
    let (prefill, decode) = kaspa_consensus_core::palw_qwen36_profile::qwen36_held_canonical_v1(m.profile.n_ctx);
    let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(&m.profile, prefill, decode);
    let counted =
        kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&m.profile, &canonical, h.bundle.court.max_step_leaf_count())
            .expect("the canonical job counts");
    let rule = PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted };
    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    let chain_certified = state.chain_certified_families(PalwCertifiedLaneV1::Attempt);
    let prosecutable = kaspa_consensus_core::palw_e2e_adjudicability::family_certified_for_weight_v2(
        h.bundle.court_e2e_root,
        &certified,
        &chain_certified,
        &kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&m.profile),
    )
    .expect("the certified set is the network's")
    .is_some();
    let share = if prosecutable && !h.vp().palw_admission_independence_at(daa) { h.sp().min_grantable_share_permille() } else { 0 };
    let (bond, class_id) = (h.cards[card], m.class_id());
    let message =
        palw_class_registration_message_v2(h.domain, class_id, share, 0, &bond, m.artifact_root, slash, target, &rule, &canonical);
    let signature = sign(card, message.as_byte_slice(), PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT);
    Obj::ClassRegistered {
        class_id,
        artifact_root: m.artifact_root,
        slash_value_per_pwu: slash,
        pwu_rule: rule,
        initial_target: target,
        share_permille: share,
        activation_daa: 0,
        admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
            profile: m.profile.clone(),
            canonical,
            registrant_bond: bond,
            signature,
        })),
    }
}

/// The gate's refusal of a class registration, as the processor words it.
fn not_admissible(m: &ModelClass, why: PalwClassAdmissionError) -> String {
    format!("class {} is not admissible: {why}", m.class_id())
}

/// Empty blocks until the next block's point is at `daa`.
fn p0a_empty_blocks_to(h: &H, walk: &mut Walk, daa: u64) {
    while walk.daa + 1 < daa {
        let point = walk.next();
        let next = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
        walk.advance(&point, next);
    }
}

/// **A block that carries `object` and folds without it**: the gate refuses it by `want`, the walk
/// drops it, and the block folds exactly what the walk accepted — it stands.
fn p0a_refused_and_the_block_stands(h: &H, walk: &mut Walk, object: &Obj, want: &str) {
    h.refused(walk, object, want);
    let point = walk.next();
    let accepted = h.accepted(&walk.state, &point, std::slice::from_ref(object));
    let next = h.fold(&walk.state, &point, &accepted).expect("the block that carried it folds");
    walk.advance(&point, next);
}

/// **(e1)** On testnet-12's attribution-off twin: below `palw_gdn_key_heads` the graph-v8
/// registration is refused by name and its block stands; past it the class is admitted and
/// registered. Graph-v7 over the same 16/32 weights passes the gate below the fence (the rule it
/// always had) and is refused past it by the hygiene rule — a class that could never capture its
/// recurrence.
#[tokio::test]
async fn p0a_e1_a_key_head_class_is_refused_below_the_fence_and_admitted_past_it() {
    let h = p0a_harness(false, Some(P0A_FENCE));
    let (v8, v7) = (p0a_class(&h, true), p0a_class(&h, false));
    let mut walk = h.genesis_walk();
    assert!(walk.daa + 1 < P0A_FENCE, "the walk starts below the fence");

    // Below the fence: refused by name, dropped, and the block folds without it.
    let registration = p0a_registration(&h, &walk.state, walk.daa + 1, &v8, REGISTRANT);
    p0a_refused_and_the_block_stands(
        &h,
        &mut walk,
        &registration,
        &not_admissible(&v8, PalwClassAdmissionError::GdnKeyHeadsNeedsItsFence),
    );
    assert!(walk.state.class(&v8.class_id()).is_none(), "nothing registered");
    // Graph-v7 at k != v still passes the gate below the fence — the pre-P0a rule, unchanged.
    let old = p0a_registration(&h, &walk.state, walk.daa + 1, &v7, REGISTRANT);
    assert_eq!(h.validate(&walk.state, &walk.next(), &old), Ok(()), "below the fence the gate is what it was");

    // Past the fence.
    p0a_empty_blocks_to(&h, &mut walk, P0A_FENCE);
    assert_eq!(walk.next().daa_score, P0A_FENCE, "the next block is the fence's");
    let old = p0a_registration(&h, &walk.state, walk.daa + 1, &v7, REGISTRANT);
    h.refused(&walk, &old, &not_admissible(&v7, PalwClassAdmissionError::GdnMapAssumesEqualHeads { key_heads: 16, value_heads: 32 }));
    let registration = p0a_registration(&h, &walk.state, walk.daa + 1, &v8, REGISTRANT);
    h.carry(&mut walk, vec![registration]);
    let class = walk.state.class(&v8.class_id()).expect("the graph-v8 class is registered");
    assert_eq!(class.artifact_root, v8.artifact_root, "under the fixture's inventory root");
    h.reloads(&walk.state);
}

/// **(e2) testnet-12 as shipped** (`palw_offence_attribution` armed at 0): the same registration is
/// refused below the fence by the fence, and past it by C5 — every held class with a recurrent layer
/// is `HeldClassUnanswerable` there. The finding P0a leaves open: on testnet-12 a Qwen3.6 held class
/// needs C5 answered (a windowed builder for the hybrid held site) before any graph can register it.
#[tokio::test]
async fn p0a_e2_on_testnet_12_c5_refuses_every_held_hybrid_past_the_fence() {
    let h = p0a_harness(true, Some(P0A_FENCE));
    let v8 = p0a_class(&h, true);
    let mut walk = h.genesis_walk();
    let registration = p0a_registration(&h, &walk.state, walk.daa + 1, &v8, REGISTRANT);
    p0a_refused_and_the_block_stands(
        &h,
        &mut walk,
        &registration,
        &not_admissible(&v8, PalwClassAdmissionError::GdnKeyHeadsNeedsItsFence),
    );
    p0a_empty_blocks_to(&h, &mut walk, P0A_FENCE);
    let registration = p0a_registration(&h, &walk.state, walk.daa + 1, &v8, REGISTRANT);
    let recurrent = PalwHeldUnanswerableV1::Recurrent { layers: 3 };
    p0a_refused_and_the_block_stands(
        &h,
        &mut walk,
        &registration,
        &not_admissible(&v8, PalwClassAdmissionError::HeldClassUnanswerable { why: recurrent }),
    );
    assert!(walk.state.class(&v8.class_id()).is_none(), "nothing registered on either side");
}

/// The GdnStep tile of value `head` in recurrence layer 1 at prompt position `position`.
fn gdn_step_leaf(binding: &PalwStepBindingV2, head: u32, position: u32) -> u64 {
    use kaspa_consensus_core::palw_step::{canonical_step_coordinates, kernel_semantics_id_v1};
    let (profile, job) = (&binding.shape_profile, &binding.job_context);
    let want = kernel_semantics_id_v1(kaspa_consensus_core::palw_step_refute::KDESC_Q36_GDN_STEP);
    (0..binding.step_leaf_count)
        .find(|i| {
            let c = canonical_step_coordinates(profile, job, *i).expect("a coordinate");
            c.call_index == 0
                && c.position == position
                && c.tile_index == head
                && profile.resolve_node_slot(c.node_slot).is_some_and(|(n, l)| n.kernel_semantics_id == want && l == Some(1))
        })
        .expect("the job has that GdnStep tile")
}

/// The prompt position of the disputed GdnStep tile (see the module doc: one carrier's evidence).
const DISPUTED_POSITION: u32 = 3;

/// **A model claim whose producer lied at value `head`'s GdnStep tile**: the anchor's own job run,
/// one lane of that tile moved (`execute_with_injected_fault`), the capture re-committed, and the
/// attempt folded as the block's own work under the lying roots — with the refutation the court
/// reads and the operands it opens.
fn open_gdn_step_fault(h: &H, walk: &mut Walk, m: &ModelClass, head: u32, nonce: u64) -> (Hash64, C) {
    use kaspa_consensus_core::hashing::header::pre_pow_hash_64;
    let header: Header = h.ctx.build_block_template_keeping_time(nonce).block.header.clone();
    let bond = h.cards[EXECUTOR];
    let mut facts = h.ctx.consensus.palw_producer_facts_v2(h.floor(), Some(bond.0)).expect("testnet-12 answers for card 0");
    facts.class_id = m.class_id();
    facts.artifact_root = m.artifact_root;
    let pre_pow = pre_pow_hash_64(&header);
    let anchor = execution_anchor_v3(h.domain, pre_pow, m.class_id(), &bond.0, header.nonce);
    let (canonical, prompt) = m.backend.job_for_anchor(anchor).expect("the class implies a job");
    let job = palw_attempt_job_v1(canonical, true);
    let honest = m.backend.execute(&job, &prompt).expect("the class runs its own job");
    let (binding, _) = decoded(&honest.material);
    let leaf = gdn_step_leaf(&binding, head, DISPUTED_POSITION);
    let lying = m.backend.execute_with_injected_fault(&job, &prompt, leaf).expect("the drill's fault commits");
    assert_ne!(lying.execution_root, honest.execution_root, "a different execution");
    let refutation = m.backend.refutation_for_index(&lying.material, leaf).expect("the lying capture opens");
    assert_eq!(refutation.output_opening.leaf_index, leaf, "the refutation opens the faulted leaf");
    let operand_openings = m.backend.operand_openings_for(&refutation).expect("the class opens the rows the court reads");
    let (claim_id, _envelope, _header) = h.fold_own_attempt(walk, header, anchor, pre_pow, &facts, roots_of(&lying));
    (claim_id, C::StepArithmetic { refutation, operand_openings })
}

/// **(e3) produced, disputed and closed**: the graph-v8 class on testnet-12 with the fence armed,
/// seeded through the carriage. Its honest claim's job crosses the recurrence's checkpoint (graph-v7
/// cannot capture the same job); a lie at value head 20's GdnStep tile — key head 4, the tiling the
/// kernel, the court and the v5 map share — is convicted by kind 4 before `Final` through the gate,
/// the walk and the fold; the honest claim's refutation at the same coordinate is refused and dropped.
#[tokio::test]
async fn p0a_e3_the_v3_class_is_produced_disputed_and_closed() {
    use kaspa_consensus_core::palw_context_ladder::palw_checkpoint_leaf_carries_recurrence_v1;
    use kaspa_consensus_core::palw_offence_attribution_v1::PalwForfeitScopeV1;
    let h = p0a_harness(true, Some(0));
    let m = p0a_class(&h, true);
    let mut walk = h.genesis_walk();
    seed(&h, &mut walk, &m);

    // Produced: the attempt's 31 prompt positions pass the checkpoint that carries the recurrence
    // (16 positions), and the claim folds.
    let honest = open_model_claim(&h, &mut walk, &m, ModelFault::Honest, bucket(1));
    let job = &honest.binding.job_context;
    assert_eq!((job.declared_prefill_tokens, job.exact_decode_tokens), (31, 1), "the formula's prompt under the prefill draw");
    assert_eq!(honest.binding.checkpoint_count, 31, "a checkpoint at every position of the job");
    assert!(palw_checkpoint_leaf_carries_recurrence_v1(&m.profile, 16), "the 16th carries the recurrence");
    assert_eq!(m.backend.verify_material(&honest.material, honest.roots()), PalwMaterialVerdictV1::Matches, "a seat licenses it");
    // Graph-v7 over the same weights cannot capture the same job: the defect P0a closes.
    let v7 = p0a_class(&h, false);
    let (canonical, prompt) = v7.backend.job_for_anchor(honest.anchor).expect("graph-v7 implies the same job shape");
    let refused = v7.backend.execute(&palw_attempt_job_v1(canonical, true), &prompt).err().expect("graph-v7 refuses the capture");
    assert!(refused.contains("convolution window"), "by the geometry's name: {refused}");

    // Disputed and closed: the lie at head 20 convicts before Final.
    let (liar, contradiction) = open_gdn_step_fault(&h, &mut walk, &m, 20, bucket(2));
    let finding = h.judge_refuted(&walk.state, liar, contradiction.clone()).expect("the GdnStep lie convicts");
    let root = walk.state.claim(&liar).expect("the claim is live").execution_root;
    let recorded = if finding.forfeit == PalwForfeitScopeV1::ByRoot { root } else { Hash64::default() };
    let (before, _) = h.carry(&mut walk, vec![h.refuted(liar, contradiction)]);
    assert_refuted_before_final(&h, &before, &walk.state, liar, walk.daa, recorded);

    // The honest claim at the same coordinate: no fault, refused by the gate and dropped by the walk.
    let leaf = gdn_step_leaf(&honest.binding, 20, DISPUTED_POSITION);
    let refutation = m.backend.refutation_for_index(&honest.material, leaf).expect("the honest capture opens");
    let operand_openings = m.backend.operand_openings_for(&refutation).expect("its operands open");
    let clean = C::StepArithmetic { refutation, operand_openings };
    let why = h.judge_refuted(&walk.state, honest.claim_id, clean.clone()).expect_err("an honest step convicts nobody");
    let object = h.refuted(honest.claim_id, clean);
    h.refused(&walk, &object, &why.to_string());
    assert!(matches!(walk.state.claim(&honest.claim_id).unwrap().phase, PalwClaimPhaseV2::Provisional), "the honest claim stands");
    h.reloads(&walk.state);
}
