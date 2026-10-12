//! **RFC-0001 §2.6 stage 2b, end to end on the floor: inherited prefix leaves** (`palw_fp_prefix_inherit`, FP job version 12).
//!
//! Two jobs over the same prompt prefix commit BYTE-IDENTICAL step leaves for the first `k` prefill positions (their leaves are
//! bound to a job-independent prefix context), each equal to the leaf recomputed under that prefix context; a seat replays the
//! claim through the same leaf rule; a lie in an inherited range is convicted by the court's own arithmetic close — and an honest
//! second claim's committed leaf at that index is the one the lie is refuted against; and a version-11 claim over the same prefix
//! (a version-2 context) commits job-bound leaves, unchanged.

mod common;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_backend::{PalwExecutionBackendV1, PalwFpIntervalVerdictV1, PalwFpRunV1};
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_commitment_v3};
use kaspa_consensus_core::palw_fp_prefix_v1::{PALW_FP_PREFIX_INHERIT_VERSION, PALW_FP_PREFIX_VERSION, palw_fp_prefix_state_root_v1};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFpJobTailV1, PalwFpPrefixStateV1, PalwFreePromptJobV3,
    fp_job_id_v3,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step::canonical_step_coordinates;
use kaspa_consensus_core::palw_step_leg::{PalwStepTileLeafV1, step_tile_leaf_hash_ctx_v1, step_tile_leaf_hash_v1};
use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PALW_TRACE_COMMITMENT_VERSION_V3_INHERITED};
use misaka_palw_base0::backend::Base0Backend;

const K: u32 = 3;

struct Claim {
    job: PalwFreePromptJobV3,
    ids: Vec<u32>,
    prompt: Vec<usize>,
    run: PalwFpRunV1,
}

fn job_of(backend: &Base0Backend, version: u16, ids: &[u32], nonce: u8, state: PalwFpPrefixStateV1) -> PalwFreePromptJobV3 {
    let floor = state.class_id;
    PalwFreePromptJobV3 {
        version,
        network_domain: Hash64::from_u64_word(0x4E),
        class_id: floor,
        executor_bond: kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(7), 0),
        executor_pubkey: vec![7; 32],
        operator_id: Hash64::from_u64_word(0xE0),
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 100,
        job_nonce: [nonce; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, ids)
            .expect("the ids commit"),
        prompt_tokens: ids.len() as u32,
        decode_token_limit: 4,
        max_context_tokens: backend.profile().n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: [0x3C; 32],
        temperature_q: 1 << 24,
        decode: Some(DecodeConfigV4::NOOP),
        tail: Some(PalwFpJobTailV1::Prefix(state)),
    }
}

fn claim(backend: &Base0Backend, version: u16, ids: &[u32], nonce: u8, state: PalwFpPrefixStateV1) -> Claim {
    let job = job_of(backend, version, ids, nonce, state);
    let prompt: Vec<usize> = ids.iter().map(|t| *t as usize).collect();
    let run = backend.execute_free_prompt(&job, &prompt).expect("the floor runs the job");
    Claim { job, ids: ids.to_vec(), prompt, run }
}

/// A seat's whole-capture check under the CLAIM's decode rule (what the panel does for a V4-rule job).
fn seat_material(
    backend: &Base0Backend,
    c: &Claim,
    bytes: &[u8],
    roots: kaspa_consensus_core::palw_backend::PalwClaimRootsV1,
) -> kaspa_consensus_core::palw_backend::PalwMaterialVerdictV1 {
    use kaspa_consensus_core::palw_decode_pipeline_v4::{PalwFpReplayRuleV1, palw_fp_with_replay_rule_v1};
    palw_fp_with_replay_rule_v1(PalwFpReplayRuleV1::of_job(&c.job, &c.run.output_token_ids), || backend.verify_material(bytes, roots))
}

/// The floor retains a dense capture: its binding is the first element of the retained tuple.
struct Bound {
    binding: kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
}

fn material(c: &Claim) -> Bound {
    let (binding, ..) = misaka_palw_base0::produce::base0_material_decode_v1(&c.run.outcome.material).expect("our own material decodes");
    Bound { binding }
}

/// The step leaves of call 0 below position `K`, as `(index, committed hash, preimage)`: the dense material's tiles, each hashed by
/// the leaf rule the claim commits under (and the material is checked against the claim's roots by the callers, so the hashes ARE
/// the committed ones).
fn prefix_leaves(c: &Claim) -> Vec<(u64, Hash64, PalwStepTileLeafV1)> {
    let (binding, tiles, ..) = misaka_palw_base0::produce::base0_material_decode_v1(&c.run.outcome.material).expect("decodes");
    let profile_hash = binding.shape_profile.shape_profile_id();
    let mut out: Vec<_> = tiles
        .into_iter()
        .filter(|(_, t)| t.coord.call_index == 0 && t.coord.position < K)
        .map(|(i, t)| (i, step_tile_leaf_hash_ctx_v1(&binding.job_context, &profile_hash, &t), t))
        .collect();
    out.sort_by_key(|(i, _, _)| *i);
    out
}

#[test]
fn inherited_prefix_leaves_are_job_independent_equal_the_recomputed_ones_and_leave_version_2_alone() {
    let backend = common::floor_backend(PalwPromptIdsFormV1::Flat);
    let floor = backend.profile().shape_profile_id();
    let prefix: Vec<u32> = vec![17, 3, 91];
    let kv = Hash64::from_u64_word(0xCAFE);
    let state = PalwFpPrefixStateV1 { state_root: palw_fp_prefix_state_root_v1(&floor, K, &kv), prefix_tokens: K, class_id: floor };
    let (mut ids_e, mut ids_l) = (prefix.clone(), prefix.clone());
    ids_e.extend([4, 5]);
    ids_l.extend([8, 2, 6]);

    // Two claims, one prefix, two jobs: version 12 (inherited).
    let e = claim(&backend, PALW_FP_PREFIX_INHERIT_VERSION, &ids_e, 0x11, state);
    let l = claim(&backend, PALW_FP_PREFIX_INHERIT_VERSION, &ids_l, 0x22, state);
    assert_ne!(fp_job_id_v3(&e.job), fp_job_id_v3(&l.job));
    let (me, ml) = (material(&e), material(&l));
    assert_eq!(me.binding.job_context.version, PALW_TRACE_COMMITMENT_VERSION_V3_INHERITED);
    assert_eq!(ml.binding.job_context.inherited_prefix_v1(), Some((K, state.state_root)));
    assert_ne!(me.binding.job_context.context_hash(), ml.binding.job_context.context_hash());
    assert_eq!(me.binding.job_context.prefix_context_hash_v1(), ml.binding.job_context.prefix_context_hash_v1(), "the prefix context is the class's and the prefix's");

    // The retained captures answer for the claims' own roots (so the leaf hashes below are the committed ones).
    for c in [&e, &l] {
        let roots = kaspa_consensus_core::palw_backend::PalwClaimRootsV1 {
            execution_root: c.run.outcome.execution_root,
            trace_root: c.run.outcome.trace_root,
            anchor: fp_job_id_v3(&c.job),
            attempt_draw: None,
            output_root: None,
            job_pin: None,
        };
        assert_eq!(seat_material(&backend, c, &c.run.outcome.material, roots), kaspa_consensus_core::palw_backend::PalwMaterialVerdictV1::Matches);
    }
    let (pe, pl) = (prefix_leaves(&e), prefix_leaves(&l));
    assert!(!pe.is_empty() && pe.len() == pl.len(), "the same leaves in the same places");
    for ((ie, he, te), (il, hl, tl)) in pe.iter().zip(&pl) {
        assert_eq!(ie, il);
        // INHERITED: the two claims committed the SAME leaf hash, and it is the leaf recomputed under the prefix context.
        assert_eq!(he, hl, "leaf {ie}: identical across jobs");
        assert_eq!(te, tl, "and over identical values");
        assert_eq!(*he, step_tile_leaf_hash_ctx_v1(&me.binding.job_context, &floor, te), "leaf {ie}: equals the recomputed one");
        let prefix_ctx = me.binding.job_context.prefix_context_hash_v1().unwrap();
        assert_eq!(*he, step_tile_leaf_hash_v1(&prefix_ctx, &floor, te));
        assert_ne!(*he, step_tile_leaf_hash_v1(&me.binding.job_context.context_hash(), &floor, te), "not bound to the job context");
    }
    // Everything past the prefix is job-bound as before: the first position past it differs between the two claims' contexts.
    let past = |m: &Bound| {
        (0..m.binding.step_leaf_count)
            .find(|i| canonical_step_coordinates(&m.binding.shape_profile, &m.binding.job_context, *i).is_some_and(|c| c.call_index == 0 && c.position == K))
            .expect("a leaf at position k")
    };
    let (ie, il) = (past(&me), past(&ml));
    let (re, rl) = (
        backend.refutation_for_free_prompt_index(&e.run.outcome.material, ie, &e.ids).unwrap(),
        backend.refutation_for_free_prompt_index(&l.run.outcome.material, il, &l.ids).unwrap(),
    );
    assert_eq!(
        re.output_opening.leaf_hash,
        step_tile_leaf_hash_v1(&me.binding.job_context.context_hash(), &floor, &re.output_preimage),
        "past the prefix the leaf is bound to the job context"
    );
    assert_ne!(re.output_opening.leaf_hash, rl.output_opening.leaf_hash);

    // VERSION 2 UNCHANGED: the same prefix as a version-11 claim is a version-2 context whose prefix leaves are job-bound.
    let v11e = claim(&backend, PALW_FP_PREFIX_VERSION, &ids_e, 0x11, state);
    let v11l = claim(&backend, PALW_FP_PREFIX_VERSION, &ids_l, 0x22, state);
    let (m11e, m11l) = (material(&v11e), material(&v11l));
    assert_eq!(m11e.binding.job_context.version, PALW_TRACE_COMMITMENT_VERSION_V2);
    assert_eq!(m11e.binding.job_context.inherited_prefix_v1(), None);
    let (qe, ql) = (prefix_leaves(&v11e), prefix_leaves(&v11l));
    assert!(qe.iter().zip(&ql).all(|((_, he, _), (_, hl, _))| he != hl), "a version-2 claim's prefix leaves are bound to its own job");
    for (_, h, t) in &qe {
        assert_eq!(*h, step_tile_leaf_hash_v1(&m11e.binding.job_context.context_hash(), &floor, t), "exactly the shipped rule");
    }

    // THE SEAT replays the inherited claim through the same leaf rule — every interval, Valid.
    let class = PalwFpClassFactsV3 {
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: floor,
        cu_ruleset_id: Hash64::default(),
    };
    let _ = (&class, palw_fp_commitment_v3 as fn(_, _, _, _, _) -> _);
    let roots = kaspa_consensus_core::palw_backend::PalwClaimRootsV1 {
        execution_root: l.run.outcome.execution_root,
        trace_root: l.run.outcome.trace_root,
        anchor: fp_job_id_v3(&l.job),
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let count = backend.fp_interval_count_for(l.job.prompt_tokens, l.run.facts.decode_tokens_executed).expect("intervals");
    for index in 0..count {
        let opening = backend.open_fp_interval(&l.run.outcome.material, index, &l.ids).expect("the executor opens it");
        backend.fp_forget_seat_state_v1();
        if let Some((_, covered, _)) = misaka_palw_base0::fp_interval::base0_fp_interval_opening_anchor_v1(&opening) {
            let c = misaka_palw_base0::fp_interval::Base0FpIntervalOpeningV4::decode_v1(&opening).unwrap().binding.job_context;
            backend.checkpoint_root_for_context_v1(&c, &l.ids, &l.run.output_token_ids, covered).expect("the seat recomputes");
        }
        let v = backend.verify_fp_interval_opening_under_job_v1(&opening, roots, index, &l.ids, l.run.facts.step_leaf_count, &l.job, &l.run.output_token_ids);
        assert_eq!(v, PalwFpIntervalVerdictV1::Valid, "interval {index} of the inherited claim");
    }

    // A LIE IN AN INHERITED RANGE: the executor retains a moved tile at a prefix leaf. Its committed leaf is not the leaf the honest claim
    // over this prefix (E) committed at that index, so anyone holding E's retained leaf refutes it; the seat's whole-capture check
    // (the same leaf rule) refuses it; and the honest claim's own leaf at that index clears the court's arithmetic close.
    let (target, honest_hash, honest_tile) = pe[pe.len() / 2].clone();
    let mut lying_tile = honest_tile.clone();
    lying_tile.values_le[0] = lying_tile.values_le[0].wrapping_add(1);
    let lying_hash = step_tile_leaf_hash_ctx_v1(&ml.binding.job_context, &floor, &lying_tile);
    assert_ne!(lying_hash, honest_hash, "a moved tile is a different committed leaf, and E committed the honest one at this index");
    assert_eq!(honest_hash, pl.iter().find(|(i, _, _)| *i == target).unwrap().1, "L's honest leaf is E's leaf");
    let roots = kaspa_consensus_core::palw_backend::PalwClaimRootsV1 {
        execution_root: l.run.outcome.execution_root,
        trace_root: l.run.outcome.trace_root,
        anchor: fp_job_id_v3(&l.job),
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let honest_material = l.run.outcome.material.clone();
    assert_eq!(seat_material(&backend, &l, &honest_material, roots), kaspa_consensus_core::palw_backend::PalwMaterialVerdictV1::Matches);
    let mut retained = misaka_palw_base0::produce::base0_material_decode_v1(&honest_material).expect("decodes");
    retained.1.iter_mut().find(|(i, _)| *i == target).expect("the tile is held").1.values_le[0] =
        lying_tile.values_le[0];
    let tampered = borsh::to_vec(&(&retained.0, &retained.1, &retained.2, &retained.3, &retained.4)).expect("re-encodes");
    assert_eq!(
        seat_material(&backend, &l, &tampered, roots),
        kaspa_consensus_core::palw_backend::PalwMaterialVerdictV1::Mismatch,
        "a lie in an inherited range is refused by the same leaf rule"
    );
    let (class_root, _) = backend.artifact_root_and_leaf_count().expect("the floor's registered root");
    let honest_ref = backend.refutation_for_free_prompt_index(&honest_material, target, &l.ids).unwrap();
    let honest_ops = backend.operand_openings_for(&honest_ref).unwrap();
    let honest_operands =
        kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&honest_ops, class_root).unwrap();
    let verdict = kaspa_consensus_core::palw_step_refute::check_execution_step_refutation_v1(&honest_ref, &honest_operands);
    assert!(
        matches!(verdict, Err(kaspa_consensus_core::palw_step_refute::PalwStepRefuteError::NoFaultFound)),
        "an honest inherited leaf clears the court's arithmetic close: {verdict:?}"
    );
}
